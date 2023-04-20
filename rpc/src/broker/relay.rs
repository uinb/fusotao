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
use dashmap::DashMap;
use futures::{stream::StreamExt, SinkExt};
use http::{Request, Uri};
use jsonrpsee::core::{error::Error as RpcError, server::rpc_module::SubscriptionSink};
use sc_client_api::{Backend, StorageProvider};
use serde_json::{json, Value};
use sp_application_crypto::Ss58Codec;
use sp_blockchain::HeaderBackend;
use sp_keystore::CryptoStore;
use sp_runtime::traits::Block;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio_tungstenite::{self as tokio_ws, MaybeTlsStream, WebSocketStream};
use tungstenite::protocol::Message;

type WsConnection = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Clone)]
pub struct BackendSession {
    reqs: Arc<DashMap<u64, Sender<Result<Value, RpcError>>>>,
    subs: Arc<DashMap<String, SubscriptionSink>>,
    to_back: Sender<Value>,
    id: Arc<AtomicU64>,
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
        // TODO config
        let (to_back, from_front) = mpsc::channel(10000);
        let reqs = Arc::new(DashMap::new());
        let subs = Arc::new(DashMap::new());
        let id = Arc::new(AtomicU64::new(1));
        Self::start_inner(
            executor,
            prover,
            storage,
            keystore,
            reqs.clone(),
            subs.clone(),
            from_front,
            id.clone(),
        );
        Self {
            reqs,
            subs,
            to_back,
            id,
        }
    }

    pub async fn relay(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let id = self.id.fetch_add(1, Ordering::Relaxed);
        let (tx, mut rx) = mpsc::channel(1);
        self.reqs.insert(id, tx);
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        tokio::select! {
            _ = self.to_back.send(payload) => {
                tokio::select! {
                    rsp = rx.recv() => {
                        rsp.ok_or(rpc_error!("internal error"))?
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {
                        Err(rpc_error!("request timeout"))
                    }
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {
                self.reqs.remove(&id);
                Err(rpc_error!("request timeout"))
            }
        }
    }

    pub async fn multiplex(
        &self,
        user: String,
        signature: String,
        nonce: String,
        relayer: String,
        sink: SubscriptionSink,
    ) -> anyhow::Result<()> {
        let id = self.id.fetch_add(1, Ordering::Relaxed);
        let key = super::try_into_ss58(user.clone())?;
        let (tx, mut rx) = mpsc::channel(1);
        self.reqs.insert(id, tx);
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "append_user",
            "params": [user, signature, nonce, relayer],
        });
        tokio::select! {
            _ = self.to_back.send(payload) => {
                tokio::select! {
                    rsp = rx.recv() => {
                        log::debug!("received response of sub request: {:?}", rsp);
                        match rsp {
                            Some(Ok(_)) => {
                                self.subs.insert(key, sink);
                            }
                            Some(Err(e)) => {
                                sink.close(e);
                            }
                            None => {
                                sink.close(rpc_error!("internal error"));
                            }
                        }
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                        sink.close(rpc_error!("prover timeout"));
                    }
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {
                sink.close(rpc_error!("prover timeout"));
            }
        }
        Ok(())
    }

    fn start_inner<B, S, BS>(
        executor: TaskExecutor,
        prover: AccountId,
        storage: Arc<BS>,
        keystore: Arc<dyn CryptoStore>,
        reqs: Arc<DashMap<u64, Sender<Result<Value, RpcError>>>>,
        subs: Arc<DashMap<String, SubscriptionSink>>,
        rx: Receiver<Value>,
        id: Arc<AtomicU64>,
    ) where
        B: Block,
        S: Backend<B>,
        BS: StorageProvider<B, S> + HeaderBackend<B> + 'static,
    {
        executor.clone().spawn(
            "broker-prover-connector",
            Some("fusotao-rpc"),
            async move {
                log::info!("starting broker-prover-connector.");
                let mut from_front = rx;
                loop {
                    let ready = Self::init_connection::<B, S, BS>(&prover, &storage, &keystore, &subs, &id).await;
                    if let Err(e) = ready {
                        log::error!("{:?}", e);
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        continue;
                    }
                    let mut conn = ready.unwrap();
                    loop {
                        tokio::select! {
                            outgoing = from_front.recv() => {
                                log::debug!("==> outgoing msg: {:?}", outgoing);
                                match outgoing {
                                    Some(req) => {
                                        let payload = req.to_string();
                                        log::debug!("prepare sending request: {}", payload);
                                        let req = Message::Text(payload);
                                        if let Err(e) = conn.send(req).await {
                                            log::error!("sending request failed, {:?}", e);
                                            break;
                                        }
                                    }
                                    None => return,
                                }
                            }
                            _ = tokio::time::sleep(std::time::Duration::from_secs(10)) =>  {
                                let ping = Message::Ping(rand::random::<[u8; 8]>().to_vec());
                                if let Err(e) = conn.send(ping).await {
                                    log::error!("sending ping failed, {:?}", e);
                                    break;
                                }
                            }
                            incoming = conn.next() => {
                                log::debug!("<== incoming msg: {:?}", incoming);
                                match incoming {
                                    Some(Ok(msg)) => {
                                        match msg {
                                            Message::Text(msg) => {
                                                log::debug!("received response: {}", msg);
                                                let mut rsp: Value = match serde_json::from_str(&msg) {
                                                    Ok(rsp) => rsp,
                                                    Err(e) => {
                                                        log::error!("deserializing response failed, {:?}", e);
                                                        continue;
                                                    }
                                                };
                                                match (rsp["id"].as_u64(), rsp["params"]["subscription"].as_str()) {
                                                    // request with id
                                                    (Some(id), None) => {
                                                        if let Some((_, tx)) = reqs.remove(&id) {
                                                            let r = if rsp["error"].is_null() {
                                                                Ok(rsp["result"].take())
                                                            } else {
                                                                Err(rpc_error!(
                                                                    rsp["error"]["code"].as_i64().unwrap_or(-32099) as i32,
                                                                    rsp["error"]["message"].as_str().unwrap_or("the prover didn't reply").to_string()
                                                                ))
                                                            };
                                                            let _ = tx.send(r).await;
                                                        }
                                                    },
                                                    // subscribe on multiplex
                                                    (None, Some(_)) => {
                                                        let r = rsp["params"]["result"].take();
                                                        match r["user_id"].as_str() {
                                                            Some(user) => {
                                                                // TODO `send` is not async, so we can't use `await` here
                                                                subs.remove_if_mut(user, |_, sink| {
                                                                    !sink.send(&r).unwrap_or(true)
                                                                });
                                                            }
                                                            None => {}
                                                        }
                                                    },
                                                    _ => log::error!("invalid response: {:?}", rsp),
                                                }
                                            }
                                            Message::Ping(h) => {
                                                let _ = conn.send(Message::Pong(h)).await;
                                            }
                                            Message::Close(_) => {
                                                log::error!("connection closed by server");
                                                break;
                                            }
                                            _ => (),
                                        }
                                    }
                                    Some(Err(e)) => {
                                        log::error!("connection error, {:?}", e);
                                        break;
                                    }
                                    None => {
                                        log::error!("connection closed");
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    log::error!("connection interrupted, retrying..");
                }
            }
            .boxed());
    }

    async fn init_connection<B, S, BS>(
        prover: &AccountId,
        storage: &Arc<BS>,
        keystore: &Arc<dyn CryptoStore>,
        subs: &Arc<DashMap<String, SubscriptionSink>>,
        id: &Arc<AtomicU64>,
    ) -> anyhow::Result<WsConnection>
    where
        B: Block,
        S: Backend<B>,
        BS: StorageProvider<B, S> + HeaderBackend<B> + 'static,
    {
        let block_number = super::get_best_block_number(&storage);
        let to_be_signed = block_number.encode();
        let (account, sig) = super::sign_using_keystore(keystore.clone(), to_be_signed.as_slice())
            .await
            .map_err(|_| anyhow::anyhow!("broker key not configured correctly"))?;
        let rpc = super::get_prover_rpc(storage.clone(), prover)
            .ok_or(anyhow::anyhow!("prover rpc endpoint {} not found.", prover))?;
        let uri = rpc
            .parse::<Uri>()
            .map_err(|_| anyhow::anyhow!("invalid rpc uri"))?;
        let host = uri
            .host()
            .map(|h| h.to_string())
            .ok_or(anyhow::anyhow!("invalid rpc uri"))?;
        let request = Request::builder()
            .uri(uri)
            .method("GET")
            .header("X-Broker-Account", account.to_ss58check())
            .header("X-Broker-Nonce", format!("{}", block_number))
            .header("X-Broker-Signature", format!("0x{}", hex::encode(sig)))
            .header("Host", host)
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header("Sec-WebSocket-Key", generate_key())
            .body(())
            .map_err(|_| anyhow::anyhow!("invalid request"))?;
        tokio::select! {
            conn = tokio_ws::connect_async(request) => {
                let mut ready = conn.map(|r| r.0).map_err(|e| anyhow::anyhow!("prover not available, {:?}", e))?;
                // recover subscriptions
                let payload = json!({
                    "jsonrpc": "2.0",
                    "id": id.fetch_add(1, Ordering::Relaxed),
                    "method": "sub_trading",
                    "params": json!([super::get_broker_public(keystore.clone()).await?.to_ss58check()]),
                });
                ready.send(Message::Text(payload.to_string())).await?;
                // TODO we want to auto-resub for all users but we don't have the signatures
                subs.clear();
                Ok(ready)
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                Err(anyhow::anyhow!("connection timeout"))
            }
        }
    }
}

fn generate_key() -> String {
    // a base64-encoded (see Section 4 of [RFC4648]) value that,
    // when decoded, is 16 bytes in length (RFC 6455)
    let r: [u8; 16] = rand::random();
    base64::encode(&r)
}
