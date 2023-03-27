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
use jsonrpsee::types::params::ParamsSer;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use sc_client_api::{Backend, StorageProvider};
use serde_json::Value as JsonValue;
use sp_application_crypto::Ss58Codec;
use sp_blockchain::HeaderBackend;
use sp_keystore::CryptoStore;
use sp_runtime::traits::Block;
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
    params: Option<Vec<JsonValue>>,
}

pub struct Topic {
    sink: ToFront,
    method: String,
    params: Option<Vec<JsonValue>>,
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

    pub fn subscribe_until_fail_n_times(
        &self,
        sink: ToFront,
        method: impl ToString,
        params: Option<Vec<JsonValue>>,
        unsub: impl ToString,
        retry_left: u32,
    ) {
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
    }

    pub async fn request(
        &self,
        method: impl ToString,
        params: Option<Vec<JsonValue>>,
    ) -> Result<JsonValue, RpcError> {
        let (back_tx, mut front_rx) = mpsc::channel(1);
        let method = method.to_string();
        let _ = self
            .tx
            .send(ToBack::Req(Cmd {
                back_tx,
                method,
                params,
            }))
            .await;
        front_rx
            .recv()
            .await
            .unwrap_or(Err(rpc_error!(req => "Something went wrong.")))
    }

    async fn try_connect<B, S, BS>(
        established: &mut Option<WsClient>,
        prover: &AccountId,
        storage: Arc<BS>,
        keystore: &Arc<dyn CryptoStore>,
    ) where
        B: Block,
        S: Backend<B>,
        BS: StorageProvider<B, S> + HeaderBackend<B>,
    {
        match established {
            Some(conn) if conn.is_connected() => {}
            _ => {
                let mut headers = HeaderMap::new();
                let block_number = super::get_best_block_number(&storage);
                headers.insert(
                    "X-Broker-Nonce",
                    HeaderValue::from_str(&format!("{}", block_number)).expect("local read;qed"),
                );
                let to_be_signed = block_number.encode();
                let (account, sig) =
                    match super::sign_using_keystore(keystore.clone(), to_be_signed.as_slice())
                        .await
                    {
                        Ok((account, sig)) => (account, sig),
                        Err(e) => {
                            log::error!("Relayer key not found: {:?}.", e);
                            return;
                        }
                    };
                headers.insert(
                    "X-Broker-Account",
                    HeaderValue::from_str(&account.to_ss58check()).expect("local read;qed"),
                );
                headers.insert(
                    "X-Broker-Signature",
                    HeaderValue::from_str(&format!("0x{}", hex::encode(sig)))
                        .expect("local read;qed"),
                );
                let rpc = super::get_prover_rpc(storage.clone(), prover);
                if rpc.is_none() {
                    return;
                }
                let rpc = rpc.expect("qed;");
                match WsClientBuilder::default()
                    .set_headers(headers)
                    .build(rpc)
                    .await
                    .ok()
                {
                    Some(new) => {
                        established.replace(new);
                    }
                    None => {
                        established.take();
                    }
                }
            }
        }
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
                let mut client = None;
                let storage = storage;
                let keystore = keystore;
                let prover = prover;
                loop {
                    let signal = rx.recv().await;
                    if signal.is_none() {
                        break;
                    }
                    let signal = signal.unwrap();
                    Self::try_connect(&mut client, &prover, storage.clone(), &keystore).await;
                    match client {
                        None => Self::retry_or_close(executor.clone(), tx.clone(), signal),
                        Some(ref ready) => match signal {
                            ToBack::Sub(mut topic) => {
                                let param = topic.params.clone().map(|r| ParamsSer::Array(r));
                                match ready.subscribe(&topic.method, param, &topic.unsub).await {
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
                                let param = params.map(|p| ParamsSer::Array(p));
                                let _ = back_tx.send(ready.request(&method, param).await).await;
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
