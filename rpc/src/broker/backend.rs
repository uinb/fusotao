// Copyright 2021-2023 UINB Technologies Pte. Ltd.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::*;
use crate::{error_msg, rpc_error};
use futures::{
    future,
    stream::{self, StreamExt},
};
use hyper::header::{HeaderMap, HeaderValue};
use jsonrpsee::core::{
    client::{ClientT, Subscription},
    error::{Error as RpcError, SubscriptionClosed},
    server::rpc_module::SubscriptionSink,
};
use jsonrpsee::types::{error::SubscriptionEmptyError, params::ParamsSer};
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use sc_client_api::{Backend, StorageProvider};
use serde_json::Value as JsonValue;
use sp_application_crypto::Ss58Codec;
use sp_blockchain::HeaderBackend;
use sp_keystore::CryptoStore;
use sp_runtime::traits::Block;
use std::marker::PhantomData;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time;

pub type ToFront = SubscriptionSink;

pub enum ToBack {
    Sub(Topic),
    Req(Cmd),
}

pub struct Cmd {
    back_tx: Sender<Result<JsonValue, RpcError>>,
    method: String,
    params: Vec<JsonValue>,
}

pub struct Topic {
    sink: ToFront,
    method: String,
    params: Vec<JsonValue>,
    unsub: String,
    retry_left: u32,
}

impl Topic {
    fn decr_retry(&mut self) {
        if self.retry_left > 0 {
            self.retry_left -= 1;
        }
    }
}

pub type Tx = Sender<ToBack>;
pub type Rx = Receiver<ToBack>;

pub struct ClientPool<B, S, BS> {
    sender: Sender<WsClient>,
    receiver: Receiver<WsClient>,
    capacity: usize,
    prover: AccountId,
    storage: Arc<BS>,
    keystore: Arc<dyn CryptoStore>,
    _phantom: PhantomData<(B, S)>,
}

const MAX_CONNECTION_SIZE: usize = 24;
const INIT_CONNECTION_SIZE: usize = 12;

impl<B, S, BS> ClientPool<B, S, BS>
where
    B: Block,
    S: Backend<B>,
    BS: StorageProvider<B, S> + HeaderBackend<B> + 'static,
{
    pub async fn new(
        prover: AccountId,
        storage: Arc<BS>,
        keystore: Arc<dyn CryptoStore>,
    ) -> Option<Self> {
        let (sender, receiver) = mpsc::channel(MAX_CONNECTION_SIZE);
        let mut capacity = 0;
        for _i in 0..INIT_CONNECTION_SIZE {
            if let Some(conn) = Self::init_client(&prover, storage.clone(), &keystore).await {
                let _ = sender.send(conn).await;
                capacity += 1;
            }
        }
        if capacity == 0 {
            return None;
        }
        log::info!(
            "Initialized {} connections with prover {:?}.",
            capacity,
            prover
        );
        Some(Self {
            sender,
            receiver,
            capacity,
            prover,
            storage,
            keystore,
            _phantom: PhantomData,
        })
    }

    async fn init_client(
        prover: &AccountId,
        storage: Arc<BS>,
        keystore: &Arc<dyn CryptoStore>,
    ) -> Option<WsClient> {
        let mut headers = HeaderMap::new();
        let block_number = super::get_best_block_number(&storage);
        headers.insert(
            "X-Broker-Nonce",
            HeaderValue::from_str(&format!("{}", block_number)).expect("read from local;qed"),
        );
        let to_be_signed = block_number.encode();
        let (account, sig) =
            match super::sign_using_keystore(keystore.clone(), to_be_signed.as_slice()).await {
                Ok((account, sig)) => (account, sig),
                Err(e) => {
                    log::error!("Relayer key not found: {:?}.", e);
                    return None;
                }
            };
        headers.insert(
            "X-Broker-Account",
            HeaderValue::from_str(&account.to_ss58check()).expect("read from local;qed"),
        );
        headers.insert(
            "X-Broker-Signature",
            HeaderValue::from_str(&format!("0x{}", hex::encode(sig))).expect("read from local;qed"),
        );
        let rpc = super::get_prover_rpc(storage.clone(), prover);
        if rpc.is_none() {
            log::error!("Prover rpc endpoint {} not found.", prover);
            return None;
        }
        let rpc = rpc.expect("qed;");
        WsClientBuilder::default()
            .set_headers(headers)
            .build(rpc)
            .await
            .ok()
    }

    #[async_recursion::async_recursion]
    pub async fn take(&mut self) -> Option<WsClient> {
        tokio::select! {
            available = self.receiver.recv() => {
                match available {
                    Some(client) if client.is_connected() => Some(client),
                    _ => {
                        log::error!("A client is closed, try to init a new one.");
                        match Self::init_client(&self.prover, self.storage.clone(), &self.keystore).await {
                            Some(new) => {
                                log::info!("New client is initialized.");
                                let _ = self.sender.send(new).await;
                                self.take().await
                            }
                            None => {
                                self.capacity -= 1;
                                log::error!("Failed to init client, current pool size is {}.", self.capacity);
                                None
                            }
                        }
                    }
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {
                log::error!("Acquiring a client timed out, try to init a new one.");
                if self.capacity < MAX_CONNECTION_SIZE {
                    match Self::init_client(&self.prover, self.storage.clone(), &self.keystore).await {
                        Some(new) => {
                            log::info!("New client is initialized.");
                            self.capacity += 1;
                            let _ = self.sender.send(new).await;
                            self.take().await
                        }
                        None => {
                            log::error!("Failed to init client, current pool size is {}.", self.capacity);
                            None
                        }
                    }
                } else {
                    log::error!("No capacity to init new clients, current pool size is {}.", self.capacity);
                    None
                }
            }
        }
    }

    pub fn returning_entry(&self) -> Sender<WsClient> {
        self.sender.clone()
    }
}

#[derive(Clone)]
pub struct BackendSession {
    tx: Tx,
    executor: TaskExecutor,
}

impl BackendSession {
    pub fn init<B, S, BS>(
        executor: TaskExecutor,
        prover: AccountId,
        storage: Arc<BS>,
        keystore: Arc<dyn CryptoStore>,
    ) -> Self
    where
        B: Block,
        S: Backend<B>,
        BS: StorageProvider<B, S> + HeaderBackend<B> + 'static,
    {
        let (tx, rx): (Tx, Rx) = mpsc::channel(10000);
        Self::start_inner_task(executor.clone(), prover, storage, keystore, rx, tx.clone());
        Self { tx, executor }
    }

    pub fn is_initialized(&self) -> bool {
        !self.tx.is_closed()
    }

    pub fn subscribe_until_fail_n_times(
        &self,
        sink: ToFront,
        method: impl ToString,
        params: Vec<JsonValue>,
        unsub: impl ToString,
        retry_left: u32,
    ) -> Result<(), SubscriptionEmptyError> {
        if self.tx.is_closed() {
            return Err(rpc_error!(sub => "Invalid prover"));
        }
        let tx = self.tx.clone();
        let (method, unsub) = (method.to_string(), unsub.to_string());
        self.executor.spawn(
            "broker-subscription-pipeline",
            Some("fusotao-rpc"),
            async move {
                let _ = tx
                    .send(ToBack::Sub(Topic {
                        sink,
                        method,
                        params,
                        unsub,
                        retry_left,
                    }))
                    .await;
            }
            .boxed(),
        );
        Ok(())
    }

    pub async fn request(
        &self,
        method: impl ToString,
        params: Vec<JsonValue>,
    ) -> Result<JsonValue, RpcError> {
        let (back_tx, mut front_rx) = mpsc::channel(1);
        let method = method.to_string();
        self.tx
            .send(ToBack::Req(Cmd {
                back_tx,
                method,
                params,
            }))
            .await
            .map_err(|_| rpc_error!(req => "Invalid prover"))?;
        front_rx
            .recv()
            .await
            .unwrap_or(Err(rpc_error!(req => "Invalid prover")))
    }

    fn start_inner_task<B, S, BS>(
        executor: TaskExecutor,
        prover: AccountId,
        storage: Arc<BS>,
        keystore: Arc<dyn CryptoStore>,
        mut rx: Rx,
        tx: Tx,
    ) where
        B: Block,
        S: Backend<B>,
        BS: StorageProvider<B, S> + HeaderBackend<B> + 'static,
    {
        executor.clone().spawn(
            "broker-prover-connector",
            Some("fusotao-rpc"),
            async move {
                log::info!("Starting broker-prover-connector.");
                let client_pool = ClientPool::<B, S, BS>::new(prover, storage, keystore).await;
                if client_pool.is_none() {
                    log::error!("Failed to init client pool.");
                    return;
                }
                let mut client_pool = client_pool.unwrap();
                loop {
                    let signal = rx.recv().await;
                    if signal.is_none() {
                        break;
                    }
                    let signal = signal.unwrap();
                    let client = client_pool.take().await;
                    match client {
                        None => Self::retry_or_close(executor.clone(), tx.clone(), signal),
                        Some(ready) => match signal {
                            ToBack::Sub(mut topic) => {
                                let param = ParamsSer::Array(topic.params.clone());
                                match ready
                                    .subscribe(&topic.method, Some(param), &topic.unsub)
                                    .await
                                {
                                    Ok(stream) => {
                                        Self::start_pipe(
                                            executor.clone(),
                                            tx.clone(),
                                            topic,
                                            stream,
                                        );
                                    }
                                    Err(_) => {
                                        topic.decr_retry();
                                        Self::retry_or_close(
                                            executor.clone(),
                                            tx.clone(),
                                            ToBack::Sub(topic),
                                        );
                                    }
                                }
                            }
                            ToBack::Req(Cmd {
                                back_tx,
                                method,
                                params,
                            }) => {
                                let after_sent = client_pool.returning_entry();
                                executor.spawn(
                                    "broker-req",
                                    Some("fusotao-rpc"),
                                    async move {
                                        let rsp = ready
                                            .request(&method, Some(ParamsSer::Array(params)))
                                            .await;
                                        log::debug!("Got response from galois: {:?}", rsp);
                                        let _ = back_tx.send(rsp).await;
                                        let _ = after_sent.send(ready).await;
                                    }
                                    .boxed(),
                                );
                            }
                        },
                    }
                }
            }
            .boxed(),
        );
    }

    fn start_pipe(
        executor: TaskExecutor,
        tx: Tx,
        mut topic: Topic,
        mut channel: Subscription<String>,
    ) {
        executor.clone().spawn(
            "enduser-broker-pipeline",
            Some("fusotao-rpc"),
            async move {
                match channel.next().await {
                    Some(Ok(s)) => {
                        let s = stream::once(future::ok(s)).chain(channel);
                        match topic.sink.pipe_from_try_stream(s).await {
                            SubscriptionClosed::RemotePeerAborted => {}
                            SubscriptionClosed::Failed(_) => {}
                            SubscriptionClosed::Success => {
                                Self::retry_or_close(executor, tx, ToBack::Sub(topic));
                            }
                        }
                    }
                    _ => {
                        topic.sink.close(error_msg!("Unauthorized key"));
                    }
                }
            }
            .boxed(),
        );
    }

    fn retry_or_close(executor: TaskExecutor, tx: Tx, to_back: ToBack) {
        match to_back {
            ToBack::Sub(mut topic) => {
                log::debug!("Retry subscription, left: {}", topic.retry_left);
                if topic.retry_left > 0 && !topic.sink.is_closed() {
                    executor.spawn(
                        "broker-prover-retry",
                        Some("fusotao-rpc"),
                        async move {
                            time::sleep(time::Duration::from_millis(5000)).await;
                            topic.decr_retry();
                            let _ = tx.send(ToBack::Sub(topic)).await;
                        }
                        .boxed(),
                    )
                } else {
                    topic.sink.close(error_msg!("The prover is not available."));
                }
            }
            ToBack::Req(Cmd { back_tx, .. }) => executor.spawn(
                "broker-prover-retry",
                Some("fusotao-rpc"),
                async move {
                    let _ = back_tx
                        .send(Err(rpc_error!(req => "The prover is not available.")))
                        .await;
                }
                .boxed(),
            ),
        }
    }
}
