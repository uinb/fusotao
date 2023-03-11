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
use jsonrpsee::core::{
    client::Subscription, error::SubscriptionClosed, server::rpc_module::SubscriptionSink,
};
use jsonrpsee::types::params::ParamsSer;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use serde_json::Value as JsonValue;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time;

pub type FromBackend = Subscription<String>;
pub type ToFront = SubscriptionSink;

pub enum FrontToBack {
    Sub(Topic),
    Req(Cmd),
}

pub struct Cmd {
    // TODO
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

pub type Tx = Sender<FrontToBack>;
pub type Rx = Receiver<FrontToBack>;

pub struct BackendSession {
    remote: Vec<u8>,
    tx: Tx,
    executor: TaskExecutor,
}

impl BackendSession {
    pub fn init(executor: TaskExecutor, remote: Vec<u8>) -> Self {
        let (tx, rx): (Tx, Rx) = mpsc::channel(10000);
        Self::start_inner_task(executor.clone(), remote.clone(), rx, tx.clone());
        Self {
            remote,
            tx,
            executor,
        }
    }

    pub fn subscribe_until_fail_n_times(
        &self,
        sink: ToFront,
        method: String,
        params: Option<Vec<JsonValue>>,
        unsub: String,
        n: u32,
    ) {
        let tx = self.tx.clone();
        self.executor.spawn(
            "broker-subscription-pipeline",
            Some("fusotao-rpc"),
            async move {
                let _ = tx
                    .send(FrontToBack::Sub(Topic {
                        sink,
                        method,
                        params,
                        unsub,
                        retry_left: n,
                    }))
                    .await;
            }
            .boxed(),
        );
    }

    async fn try_connect(established: &mut Option<WsClient>, url: impl AsRef<str>) {
        match established {
            Some(conn) if conn.is_connected() => {}
            _ => match WsClientBuilder::default().build(url).await.ok() {
                Some(new) => {
                    established.replace(new);
                }
                None => {
                    established.take();
                }
            },
        }
    }

    fn start_inner_task(executor: TaskExecutor, remote: Vec<u8>, mut rx: Rx, tx: Tx) {
        executor.clone().spawn(
            "broker-prover-connector",
            Some("fusotao-rpc"),
            async move {
                let mut client = None;
                loop {
                    let signal = rx.recv().await;
                    if signal.is_none() {
                        break;
                    }
                    let signal = signal.unwrap();
                    // TODO
                    Self::try_connect(&mut client, "ws://127.0.0.1:10086").await;
                    match client {
                        None => match signal {
                            FrontToBack::Sub(topic) => {
                                Self::retry_or_close(executor.clone(), tx.clone(), topic)
                            }
                            FrontToBack::Req(cmd) => {}
                        },
                        Some(ref ready) => match signal {
                            FrontToBack::Sub(topic) => {
                                let Topic {
                                    sink,
                                    method,
                                    params,
                                    unsub,
                                    retry_left,
                                } = topic;
                                let param = params.clone().map(|r| ParamsSer::Array(r));
                                match ready.subscribe::<String>(&method, param, &unsub).await {
                                    Ok(stream) => {
                                        Self::start_pipe(
                                            executor.clone(),
                                            tx.clone(),
                                            Topic {
                                                sink,
                                                method,
                                                params,
                                                unsub,
                                                retry_left,
                                            },
                                            stream,
                                        );
                                    }
                                    Err(e) => {
                                        Self::retry_or_close(
                                            executor.clone(),
                                            tx.clone(),
                                            Topic {
                                                sink,
                                                method,
                                                params,
                                                unsub,
                                                retry_left: retry_left - 1,
                                            },
                                        );
                                    }
                                }
                            }
                            FrontToBack::Req(cmd) => {}
                        },
                    }
                }
            }
            .boxed(),
        );
    }

    fn start_pipe(executor: TaskExecutor, tx: Tx, topic: Topic, stream: Subscription<String>) {
        executor.clone().spawn(
            "enduser-broker-pipeline",
            Some("fusotao-rpc"),
            async move {
                let Topic {
                    mut sink,
                    method,
                    unsub,
                    params,
                    retry_left,
                } = topic;
                match sink.pipe_from_try_stream(stream).await {
                    SubscriptionClosed::RemotePeerAborted => {
                        println!("user aborted -------");
                    }
                    SubscriptionClosed::Failed(e) => {}
                    SubscriptionClosed::Success => {
                        Self::retry_or_close(
                            executor,
                            tx,
                            Topic {
                                sink,
                                method,
                                params,
                                unsub,
                                retry_left,
                            },
                        );
                    }
                }
            }
            .boxed(),
        );
    }

    fn retry_or_close(executor: TaskExecutor, tx: Tx, topic: Topic) {
        let Topic {
            sink,
            method,
            unsub,
            params,
            retry_left,
        } = topic;
        if retry_left > 0 && !sink.is_closed() {
            executor.spawn(
                "broker-prover-retry",
                Some("fusotao-rpc"),
                async move {
                    time::sleep(time::Duration::from_millis(5000)).await;
                    let v = FrontToBack::Sub(Topic {
                        sink,
                        method,
                        unsub,
                        params,
                        retry_left: retry_left - 1,
                    });
                    let _ = tx.send(v).await;
                }
                .boxed(),
            )
        } else {
            let err = ErrorObject::owned(
                ErrorCode::ServerError(93101i32).code(),
                "The prover is not available.",
                None::<String>,
            );
            sink.close(err);
        }
    }
}
