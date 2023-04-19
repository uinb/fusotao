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

// mod backend;
mod relay;

use crate::{error_msg, rpc_error};
use async_trait::async_trait;
use codec::{Decode, Encode};
use dashmap::DashMap;
use futures::future::{BoxFuture, FutureExt};
use jsonrpsee::{
    core::RpcResult, proc_macros::rpc, types::error::SubscriptionResult,
    ws_server::SubscriptionSink,
};
use relay::BackendSession;
use sc_client_api::{Backend, StorageProvider};
use serde_json::json;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_core::{
    crypto::{AccountId32, CryptoTypePublicPair, KeyTypeId, Ss58Codec},
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

/// relayer + runtime_api
#[rpc(client, server)]
pub trait FusoBrokerApi {
    #[method(name = "broker_trade")]
    async fn trade(
        &self,
        prover: AccountId,
        account_id: String,
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

    fn try_use_sync_session<F>(&self, prover: AccountId, f: F)
    where
        F: FnOnce(Arc<BackendSession>),
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
            }
        };
        f(session)
    }

    async fn try_use_async_session<T, E, F>(&self, prover: AccountId, f: F) -> Result<T, E>
    where
        F: FnOnce(Arc<BackendSession>) -> BoxFuture<'static, Result<T, E>>,
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
            }
        };
        f(session).await
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

async fn get_broker_public(keystore: Arc<dyn CryptoStore>) -> anyhow::Result<Sr25519Public> {
    CryptoStore::sr25519_public_keys(&*keystore, RELAYER_KEY_TYPE)
        .await
        .iter()
        .last()
        .map(|v| *v)
        .ok_or(anyhow::anyhow!("no broker key found"))
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
        account_id: String,
        cmd: String,
        signature: String,
        nonce: String,
    ) -> RpcResult<String> {
        let keystore = self.keystore.clone();
        self.try_use_async_session(prover.clone(), |session| {
            async move {
                let key = get_broker_public(keystore)
                    .await
                    .inspect_err(|e| log::error!("{:?}", e))
                    .map_err(|_| rpc_error!(-32102, "The broker key is not registered."))?;
                let r = session
                    .relay(
                        "trade",
                        json!([account_id, cmd, signature, nonce, key.to_ss58check()]),
                    )
                    .await?;
                r.as_str()
                    .ok_or(rpc_error!("The prover didn't reply correctly."))
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
                    .relay(
                        "query_pending_orders",
                        json!([account_id, symbol, signature, nonce]),
                    )
                    .await?;
                let v = r
                    .as_array()
                    .ok_or(rpc_error!("The prover didn't reply correctly."))?;
                let mut orders = Vec::new();
                for order in v {
                    let order = order
                        .as_str()
                        .ok_or(rpc_error!("The prover didn't reply correctly."))?
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
                    .relay("query_account", json!([account_id, signature, nonce]))
                    .await?;
                let v = r
                    .as_array()
                    .ok_or(rpc_error!("The prover didn't reply correctly."))?;
                let mut balances = Vec::new();
                for balance in v {
                    let balance = balance
                        .as_str()
                        .ok_or(rpc_error!("The prover didn't reply correctly."))?
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
                        .relay("register_trading_key", json!([account_id, x25519, sr25519]))
                        .await?;
                    r.as_str()
                        .ok_or(rpc_error!("The prover didn't reply correctly."))
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
                let r = session.relay("get_nonce", json!([account_id])).await?;
                r.as_str()
                    .ok_or(rpc_error!("The prover didn't reply correctly."))
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
            let keystore = self.keystore.clone();
            self.executor.spawn(
                "broker-prover-connector",
                Some("fusotao-rpc"),
                async move {
                    if let Ok(relayer) = get_broker_public(keystore)
                        .await
                        .inspect_err(|e| log::error!("{:?}", e))
                    {
                        let _ = session.multiplex(
                            account_id,
                            signature,
                            nonce,
                            relayer.to_ss58check(),
                            sink,
                        );
                    }
                }
                .boxed(),
            );
        });
        Ok(())
    }
}

#[cfg(feature = "testenv")]
const LEGACY_MAPPING_CODE: u16 = 5;
#[cfg(not(feature = "testenv"))]
const LEGACY_MAPPING_CODE: u16 = 1;

pub fn try_into_ss58(addr: String) -> anyhow::Result<String> {
    if addr.starts_with("0x") {
        let addr = hexstr_to_vec(&addr)?;
        match addr.len() {
            32 => {
                let addr = AccountId32::decode(&mut &addr[..])
                    .map_err(|_| anyhow::anyhow!("Invalid address"))?;
                Ok(addr.to_ss58check())
            }
            20 => {
                let addr = to_mapping_address(addr);
                Ok(addr.to_ss58check())
            }
            _ => Err(anyhow::anyhow!("Invalid address")),
        }
    } else {
        Ok(addr)
    }
}

pub fn to_mapping_address(address: Vec<u8>) -> AccountId32 {
    let h = (b"-*-#fusotao#-*-", LEGACY_MAPPING_CODE, address)
        .using_encoded(sp_core::hashing::blake2_256);
    Decode::decode(&mut h.as_ref()).expect("32 bytes; qed")
}

pub fn hexstr_to_vec(h: impl AsRef<str>) -> anyhow::Result<Vec<u8>> {
    hex::decode(h.as_ref().trim_start_matches("0x")).map_err(|_| anyhow::anyhow!("invalid hex str"))
}
