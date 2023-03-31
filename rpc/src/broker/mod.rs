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

mod backend;

use crate::{error_msg, rpc_error};
use async_trait::async_trait;
use backend::BackendSession;
use codec::{Compact, Decode, Encode};
use dashmap::DashMap;
use futures::future::{BoxFuture, FutureExt};
use jsonrpsee::{
    core::{client::SubscriptionClientT, RpcResult},
    proc_macros::rpc,
    types::error::SubscriptionResult,
    ws_server::SubscriptionSink,
};
use sc_client_api::{Backend, StorageProvider};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_core::{
    crypto::{AccountId32, CryptoTypePublicPair, KeyTypeId},
    Bytes,
};
use sp_keystore::CryptoStore;
use sp_runtime::{
    generic::BlockId,
    traits::{Block as BlockT, Header as HeaderT},
};
use std::sync::Arc;

type TaskExecutor = Arc<dyn sp_core::traits::SpawnNamed>;
type Sr25519Public = sp_core::sr25519::Public;
type AccountId = AccountId32;

pub const RELAYER_KEY_TYPE: KeyTypeId = KeyTypeId(*b"rely");

#[derive(Clone, Encode, Decode, Eq, PartialEq)]
pub struct DominatorSetting {
    pub beneficiary: Option<AccountId>,
    pub x25519_pubkey: Vec<u8>,
    pub rpc_endpoint: Vec<u8>,
}

#[derive(Eq, PartialEq, Clone, TypeInfo, Encode, Decode, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TradingCommand {
    Ask {
        order_id: String,
        account_id: AccountId,
        base: u32,
        quote: u32,
        amount: Compact<u128>,
        price: Compact<u128>,
    },
    Bid {
        order_id: String,
        account_id: AccountId,
        base: u32,
        quote: u32,
        amount: Compact<u128>,
        price: Compact<u128>,
    },
    Cancel {
        order_id: String,
        account_id: AccountId,
    },
}

#[derive(Eq, PartialEq, Clone, Encode, Decode, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderEvent {
    order_id: String,
    account_id: AccountId,
    base: u32,
    quote: u32,
    state: u8,
    filled: Compact<u128>,
    price: Compact<u128>,
    update_at: u64,
}

#[derive(Eq, PartialEq, Clone, Encode, Decode, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderState {
    order_id: String,
    direction: u8,
    base: u32,
    quote: u32,
    state: u8,
    unfilled: Compact<u128>,
    total: Compact<u128>,
    price: Compact<u128>,
    filled_quote: Compact<u128>,
    update_at: u64,
}

/// relayer + runtime_api
#[rpc(client, server)]
pub trait FusoBrokerApi {
    #[method(name = "broker_trade")]
    async fn trade(
        &self,
        prover: AccountId,
        cmd: String,
        signature: String,
        nonce: String,
    ) -> RpcResult<String>;

    #[method(name = "broker_queryPendingOrders")]
    async fn query_pending_orders(
        &self,
        prover: AccountId,
        account_id: String,
        symbol: String,
        signature: String,
        nonce: String,
    ) -> RpcResult<Vec<String>>;

    #[method(name = "broker_queryAccount")]
    async fn query_account(
        &self,
        prover: AccountId,
        account_id: String,
        signature: String,
        nonce: String,
    ) -> RpcResult<Vec<String>>;

    #[method(name = "broker_registerTradingKey")]
    async fn register_trading_key(
        &self,
        prover: AccountId,
        account_id: String,
        x25519: String,
        sr25519: String,
    ) -> RpcResult<String>;

    #[method(name = "broker_getNonce")]
    async fn get_nonce(&self, prover: AccountId, account_id: String) -> RpcResult<String>;

    #[subscription(
        name = "broker_subscribeTrading",
        unsubscribe = "broker_unsubscribeTrading",
        item = Bytes,
    )]
    fn subscribe_trading(
        &self,
        prover: AccountId,
        account_id: String,
        signature: String,
        nonce: String,
    );
}

use sp_application_crypto::sr25519::CRYPTO_ID as Sr25519Id;
pub struct FusoBroker<C, B, S> {
    client: Arc<C>,
    executor: TaskExecutor,
    keystore: Arc<dyn CryptoStore>,
    backend_sessions: DashMap<AccountId, Arc<BackendSession>>,
    _marker: std::marker::PhantomData<(B, S)>,
}

impl<Client, Block, Storage> FusoBroker<Client, Block, Storage>
where
    Client: Send
        + Sync
        + ProvideRuntimeApi<Block>
        + HeaderBackend<Block>
        + StorageProvider<Block, Storage>
        + 'static,
    Storage: Backend<Block> + 'static,
    Block: BlockT + 'static,
{
    pub fn new(
        client: Arc<Client>,
        executor: TaskExecutor,
        keystore: Arc<dyn CryptoStore>,
    ) -> Self {
        Self {
            client,
            executor,
            keystore,
            backend_sessions: Default::default(),
            _marker: Default::default(),
        }
    }

    fn try_use_sync_session<T, E, F>(&self, prover: AccountId, f: F) -> Result<T, E>
    where
        F: FnOnce(Arc<backend::BackendSession>) -> Result<T, E>,
        E: From<jsonrpsee::types::error::SubscriptionEmptyError>,
    {
        let session = match self.backend_sessions.get(&prover) {
            Some(v) => v.value().clone(),
            None => {
                let new = Arc::new(BackendSession::init(
                    self.executor.clone(),
                    prover.clone(),
                    self.client.clone(),
                    self.keystore.clone(),
                ));
                self.backend_sessions.insert(prover.clone(), new.clone());
                new
                // TODO re-register from asscoc, otherwise `drop` all the subscriptions
            }
        };
        if session.is_initialized() {
            f(session)
        } else {
            self.backend_sessions.remove(&prover);
            Err(rpc_error!(sub => "session not initialized").into())
        }
    }

    async fn try_use_async_session<T, E, F>(&self, prover: AccountId, f: F) -> Result<T, E>
    where
        F: FnOnce(Arc<backend::BackendSession>) -> BoxFuture<'static, Result<T, E>>,
        T: Send + 'static,
        E: From<jsonrpsee::core::Error>,
    {
        let session = match self.backend_sessions.get(&prover) {
            Some(v) => v.value().clone(),
            None => {
                let new = Arc::new(BackendSession::init(
                    self.executor.clone(),
                    prover.clone(),
                    self.client.clone(),
                    self.keystore.clone(),
                ));
                self.backend_sessions.insert(prover.clone(), new.clone());
                new
                // TODO re-register from asscoc, otherwise `drop` all the subscriptions
            }
        };
        if session.is_initialized() {
            f(session).await
        } else {
            self.backend_sessions.remove(&prover);
            Err(rpc_error!(req => "session not initialized").into())
        }
    }
}

/// the keystore is very unconvenient to use, be careful
pub(crate) async fn sign_using_keystore(
    keystore: Arc<dyn CryptoStore>,
    payload: &[u8],
) -> Result<(Sr25519Public, Vec<u8>), sp_keystore::Error> {
    let key = CryptoStore::sr25519_public_keys(&*keystore, RELAYER_KEY_TYPE)
        .await
        .iter()
        .map(|k| CryptoTypePublicPair(Sr25519Id, k.0.to_vec()))
        .last()
        .ok_or(sp_keystore::Error::Unavailable)?;
    Ok((
        *CryptoStore::sr25519_public_keys(&*keystore, RELAYER_KEY_TYPE)
            .await
            .iter()
            .last()
            .expect("just checked; qed"),
        CryptoStore::sign_with(&*keystore, RELAYER_KEY_TYPE, &key, payload)
            .await
            .transpose()
            .ok_or(sp_keystore::Error::Unavailable)??,
    ))
}

pub(crate) fn get_prover_rpc<B, S, BS>(store: Arc<BS>, prover: &AccountId) -> Option<String>
where
    B: BlockT,
    S: Backend<B>,
    BS: StorageProvider<B, S> + HeaderBackend<B>,
{
    let key = super::blake2_128concat_storage_key(b"Verifier", b"DominatorSettings", prover);
    store
        .storage(&BlockId::Hash(store.info().best_hash), &key)
        .ok()
        .flatten()
        .map(|v| DominatorSetting::decode(&mut v.0.as_slice()).ok())
        .flatten()
        .map(|s| {
            std::str::from_utf8(&s.rpc_endpoint)
                .ok()
                .map(|s| s.to_owned())
        })
        .flatten()
}

type BlockNumber<B> = <<B as BlockT>::Header as HeaderT>::Number;

pub(crate) fn get_best_block_number<B: BlockT, H: HeaderBackend<B>>(
    store: &Arc<H>,
) -> BlockNumber<B> {
    store.info().best_number
}

#[async_trait]
impl<Client, Block, Storage> FusoBrokerApiServer for FusoBroker<Client, Block, Storage>
where
    Client: Send
        + Sync
        + ProvideRuntimeApi<Block>
        + HeaderBackend<Block>
        + StorageProvider<Block, Storage>
        + 'static,
    Storage: Backend<Block> + 'static,
    Block: BlockT + 'static,
{
    async fn trade(
        &self,
        prover: AccountId,
        cmd: String,
        signature: String,
        nonce: String,
    ) -> RpcResult<String> {
        self.try_use_async_session(prover.clone(), |session| {
            async move {
                let r = session
                    .request("trade", vec![json!(cmd), json!(signature), json!(nonce)])
                    .await?;
                r.as_str()
                    .ok_or(rpc_error!(req => "The prover didn't reply correctly."))
                    .map(|s| s.to_owned())
            }
            .boxed()
        })
        .await
    }

    async fn query_pending_orders(
        &self,
        prover: AccountId,
        account_id: String,
        symbol: String,
        signature: String,
        nonce: String,
    ) -> RpcResult<Vec<String>> {
        self.try_use_async_session(prover, |session| {
            async move {
                let r = session
                    .request(
                        "query_pending_orders",
                        vec![
                            json!(account_id),
                            json!(symbol),
                            json!(signature),
                            json!(nonce),
                        ],
                    )
                    .await?;
                let v = r
                    .as_array()
                    .ok_or(rpc_error!(req => "The prover didn't reply correctly."))?;
                let mut orders = Vec::new();
                for order in v {
                    let order = order
                        .as_str()
                        .ok_or(rpc_error!(req => "The prover didn't reply correctly."))?
                        .to_string();
                    orders.push(order.to_string());
                }
                Ok(orders)
            }
            .boxed()
        })
        .await
    }

    async fn query_account(
        &self,
        prover: AccountId,
        account_id: String,
        signature: String,
        nonce: String,
    ) -> RpcResult<Vec<String>> {
        self.try_use_async_session(prover, |session| {
            async move {
                let r = session
                    .request(
                        "query_account",
                        vec![json!(account_id), json!(signature), json!(nonce)],
                    )
                    .await?;
                let v = r
                    .as_array()
                    .ok_or(rpc_error!(req => "The prover didn't reply correctly."))?;
                let mut balances = Vec::new();
                for balance in v {
                    let balance = balance
                        .as_str()
                        .ok_or(rpc_error!(req => "The prover didn't reply correctly."))?
                        .to_string();
                    balances.push(balance.to_string());
                }
                Ok(balances)
            }
            .boxed()
        })
        .await
    }

    async fn register_trading_key(
        &self,
        prover: AccountId,
        account_id: String,
        x25519: String,
        sr25519: String,
    ) -> RpcResult<String> {
        self.try_use_async_session(prover, |session| {
            {
                async move {
                    let r = session
                        .request(
                            "register_trading_key",
                            vec![json!(account_id), json!(x25519), json!(sr25519)],
                        )
                        .await?;
                    r.as_str()
                        .ok_or(rpc_error!(req => "The prover didn't reply correctly."))
                        .map(|s| s.to_string())
                }
            }
            .boxed()
        })
        .await
    }

    async fn get_nonce(&self, prover: AccountId, account_id: String) -> RpcResult<String> {
        self.try_use_async_session(prover, |session| {
            async move {
                let r = session
                    .request("get_nonce", vec![json!(account_id)])
                    .await?;
                r.as_str()
                    .ok_or(rpc_error!(req => "The prover didn't reply correctly."))
                    .map(|s| s.to_string())
            }
            .boxed()
        })
        .await
    }

    fn subscribe_trading(
        &self,
        sink: SubscriptionSink,
        prover: AccountId,
        account_id: String,
        signature: String,
        nonce: String,
    ) -> SubscriptionResult {
        self.try_use_sync_session(prover.clone(), |session| {
            session.subscribe_until_fail_n_times(
                sink,
                "sub_trading".to_string(),
                vec![json!(account_id), json!(signature), json!(nonce)],
                "unsub_trading".to_string(),
                10,
            )
        })
    }
}
