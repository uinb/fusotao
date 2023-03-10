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
use jsonrpsee::rpc_params;
use jsonrpsee::types::params::ParamsSer;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time;

pub type FromBackend = Subscription<String>;
pub type ToFrontend = SubscriptionSink;

pub enum FrontMessage {
    Sub(Topic),
    Req(String, Option<String>),
}

struct Topic {
    sink: ToFrontend,
    method: String,
    unsub: String,
    params: String,
    retry_left: u32,
}

pub type Tx = Sender<FrontMessage>;
pub type Rx = Receiver<FrontMessage>;

pub struct BackendSession {
    remote: Vec<u8>,
    sender: Tx,
    executor: TaskExecutor,
}

impl BackendSession {
    pub fn new_session(executor: TaskExecutor, remote: Vec<u8>) -> Self {
        let (tx, rx): (Tx, Rx) = mpsc::channel(10000);
        Self::start_inner_task(executor.clone(), remote.clone(), rx, tx.clone());
        Self {
            remote,
            sender: tx,
            executor,
        }
    }

    pub fn subscribe_until_failed_n(
        &self,
        sink: ToFrontend,
        method: String,
        params: Option<String>,
        unsub: String,
        n: u32,
    ) {
        let tx = self.sender.clone();
        self.executor.spawn(
            "",
            Some(""),
            async move {
                let _ = tx
                    .send(FrontMessage::Sub(Topic {
                        sink,
                        method,
                        params: "".to_string(),
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
            "",
            Some(""),
            async move {
                let mut client = None;
                loop {
                    let signal = rx.recv().await;
                    if signal.is_none() {
                        break;
                    }
                    let signal = signal.unwrap();
                    Self::try_connect(&mut client, "ws://127.0.0.1:10086").await;
                    match client {
                        None => match signal {
                            FrontMessage::Sub(topic) => {
                                Self::retry_or_close(executor.clone(), tx.clone(), topic)
                            }
                            FrontMessage::Req(method, params) => {}
                        },
                        Some(ref ready) => {
                            match signal {
                                FrontMessage::Sub(topic) => {
                                    let Topic {
                                        mut sink,
                                        method,
                                        unsub,
                                        params,
                                        retry_left,
                                    } = topic;
                                    match ready
                                        .subscribe::<String>(&method, rpc_params![1], &unsub)
                                        .await
                                    {
                                        Ok(stream) => {
                                            let executor = executor.clone();
                                            let tx = tx.clone();
                                            // TODO check first item
                                            executor.clone().spawn(
                                                "",
                                                Some(""),
                                                async move {
                                                    match sink.pipe_from_try_stream(stream).await {
                                                        SubscriptionClosed::RemotePeerAborted => {}
                                                        SubscriptionClosed::Failed(e) => {}
                                                        SubscriptionClosed::Success => {
                                                            Self::retry_or_close(
                                                                executor,
                                                                tx,
                                                                Topic {
                                                                    sink,
                                                                    method,
                                                                    unsub,
                                                                    params,
                                                                    retry_left,
                                                                },
                                                            );
                                                        }
                                                    }
                                                }
                                                .boxed(),
                                            );
                                        }
                                        Err(e) => {
                                            // should
                                        }
                                    }
                                }
                                FrontMessage::Req(method, params) => {}
                            }
                        }
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
                "",
                Some(""),
                async move {
                    time::sleep(time::Duration::from_millis(5000)).await;
                    let v = FrontMessage::Sub(Topic {
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
            // sink.close()
        }
    }
}

// pub fn init(executor: TaskExecutor) -> Tx {
//     // TODO
//     let (tx, mut rx): (Tx, Rx) = mpsc::channel(10000);
//     let atx = tx.clone();
//     executor.clone().spawn(
//         "broker-prover-connector",
//         Some("fusotao-rpc"),
//         async move {
//             let mut connection = None;
//             loop {
//                 let sig = rx.recv().await;
//                 if sig.is_none() {
//                     break;
//                 }
//                 let (id, mut sink, n) = sig.unwrap();
//                 try_connect(&mut connection, "ws://127.0.0.1:10086").await;
//                 match connection {
//                     None => {
//                         let atx = atx.clone();
//                         executor.spawn(
//                             "broker-prover-connector",
//                             Some("fusotao-rpc"),
//                             async move {
//                                 if !sink.is_closed() {
//                                     tokio::time::sleep(tokio::time::Duration::from_millis(
//                                         (1000 * n).into(),
//                                     ))
//                                     .await;
//                                     let _ = atx.send((id, sink, n + 1)).await;
//                                 }
//                             }
//                             .boxed(),
//                         );
//                     }
//                     Some(ref conn) => {
//                         let r = conn
//                             .subscribe::<String>("sub_one_param", rpc_params![1], "unsub_one_param")
//                             .await;
//                         let back_sig = atx.clone();
//                         if let Ok(mut from_backend) = r {
//                             executor.spawn(
//                                 "broker-prover-stream",
//                                 Some("fusotao-rpc"),
//                                 async move {
//                                     let r = from_backend.next().await;
//                                     match r {
//                                         None => {
//                                             let err = ErrorObject::owned(
//                                                 ErrorCode::ServerError(93101i32).code(),
//                                                 "The prover is not available.",
//                                                 None::<String>,
//                                             );
//                                             sink.close(err);
//                                             return;
//                                         }
//                                         Some(Err(_)) => {
//                                             let err = ErrorObject::owned(
//                                                 ErrorCode::ServerError(93101i32).code(),
//                                                 "Unauthorized key",
//                                                 Some(format!("{:?}", id)),
//                                             );
//                                             sink.close(err);
//                                             return;
//                                         }
//                                         Some(Ok(head)) => {
//                                             let s = futures::stream::once(future::ok(head))
//                                                 .chain(from_backend);
//                                             match sink.pipe_from_try_stream(s).await {
//                                                 SubscriptionClosed::RemotePeerAborted => {}
//                                                 SubscriptionClosed::Failed(e) => {}
//                                                 SubscriptionClosed::Success => {
//                                                     // prover terminated the stream, re-sub
//                                                     let _ = back_sig.send((id, sink, 0)).await;
//                                                 }
//                                             }
//                                         }
//                                     }
//                                 }
//                                 .boxed(),
//                             );
//                         } else {
//                             let err = ErrorObject::owned(
//                                 ErrorCode::ServerError(93101i32).code(),
//                                 "The prover is not available.",
//                                 None::<String>,
//                             );
//                             sink.close(err.clone());
//                         }
//                     }
//                 }
//             }
//         }
//         .boxed(),
//     );
//     tx
// }
