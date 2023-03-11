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

use async_trait::async_trait;
use codec::{Compact, Decode, Encode};
use dashmap::DashMap;
use futures::future::FutureExt;
use jsonrpsee::{
    core::{client::SubscriptionClientT, error::Error as RpcError, RpcResult},
    proc_macros::rpc,
    types::error::{CallError, ErrorCode, ErrorObject, SubscriptionEmptyError, SubscriptionResult},
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
    Bytes, H256,
};
use sp_keystore::CryptoStore;
use sp_runtime::{
    generic::BlockId,
    traits::{Block as BlockT, MaybeDisplay},
};
use std::sync::Arc;

type TaskExecutor = Arc<dyn sp_core::traits::SpawnNamed>;
// sha256
type Signature = H256;
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
        signature: Signature,
    },
    Bid {
        order_id: String,
        account_id: AccountId,
        base: u32,
        quote: u32,
        amount: Compact<u128>,
        price: Compact<u128>,
        signature: Signature,
    },
    Cancel {
        order_id: String,
        account_id: AccountId,
        signature: Signature,
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
    #[method(name = "broker_placeOrder")]
    async fn trade(&self, prover: AccountId, cmd: TradingCommand) -> RpcResult<String>;

    #[method(name = "broker_queryOrders")]
    async fn query_orders(
        &self,
        prover: AccountId,
        account_id: AccountId,
        orders: Vec<(u32, u32, String)>,
        signature: Signature,
    ) -> RpcResult<Vec<Bytes>>;

    #[subscription(
        name = "broker_subscribeOrderEvents",
        unsubscribe = "broker_unsubscribeOrderEvents",
        item = Bytes,
    )]
    fn subscribe_order_events(
        &self,
        prover: AccountId,
        account_id: AccountId,
        signature: Signature,
    );
}

use sp_application_crypto::sr25519::CRYPTO_ID as Sr25519Id;
pub struct FusoBroker<C, B, S> {
    client: Arc<C>,
    executor: TaskExecutor,
    keystore: Arc<dyn CryptoStore>,
    backend_sessions: DashMap<AccountId, backend::BackendSession>,
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
        F: FnOnce(Option<&backend::BackendSession>) -> Result<T, E>,
    {
        match self.backend_sessions.get(&prover) {
            Some(v) => f(Some(v.value())),
            None => {
                let rpc = self.get_prover_rpc(&prover);
                tracing::info!("lookup rpc endpoint of {}", prover);
                if rpc.is_none() {
                    return f(None);
                }
                let rpc_endpoint = rpc.unwrap();
                let new = backend::BackendSession::init(self.executor.clone(), rpc_endpoint);
                self.backend_sessions.insert(prover.clone(), new);
                self.backend_sessions
                    .get(&prover)
                    .map(|s| f(Some(s.value())))
                    .unwrap()
                // TODO re-register from asscoc, otherwise `drop` all the subscriptions
            }
        }
    }

    fn get_prover_rpc(&self, prover: &AccountId) -> Option<String> {
        let key = super::blake2_128concat_storage_key(b"Verifier", b"DominatorSettings", prover);
        self.client
            .storage(&BlockId::Hash(self.client.info().best_hash), &key)
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

    /// the keystore is very unconvenient to use, be careful
    async fn sign_request(&self, payload: &[u8]) -> Result<Vec<u8>, sp_keystore::Error> {
        let key = CryptoStore::sr25519_public_keys(&*self.keystore, RELAYER_KEY_TYPE)
            .await
            .iter()
            .map(|k| CryptoTypePublicPair(Sr25519Id, k.0.to_vec()))
            .last()
            .ok_or(sp_keystore::Error::Unavailable)?;
        CryptoStore::sign_with(&*self.keystore, RELAYER_KEY_TYPE, &key, payload)
            .await
            .transpose()
            .ok_or(sp_keystore::Error::Unavailable)?
    }
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
    async fn trade(&self, prover: AccountId, cmd: TradingCommand) -> RpcResult<String> {
        let payload = cmd.encode();
        let v = self.sign_request(&payload).await.map_err(|e| {
            RpcError::Call(CallError::Custom(ErrorObject::owned(
                ErrorCode::ServerError(93101i32).code(),
                "The broker hasn't register its signing key, please switch to another node.",
                Some(format!("{:?}", e)),
            )))
        })?;
        // TODO RELAY
        Ok("Ni4qf".to_string())
    }

    async fn query_orders(
        &self,
        prover: AccountId,
        account_id: AccountId,
        orders: Vec<(u32, u32, String)>,
        signature: Signature,
    ) -> RpcResult<Vec<Bytes>> {
        let endpoint = self.get_prover_rpc(&prover);
        // TODO
        Ok(vec![])
    }

    fn subscribe_order_events(
        &self,
        sink: SubscriptionSink,
        prover: AccountId,
        account_id: AccountId,
        signature: Signature,
    ) -> SubscriptionResult {
        self.try_use_sync_session(prover.clone(), |session| match session {
            Some(session) => {
                session.subscribe_until_fail_n_times(
                    sink,
                    "sub_one_param".to_string(),
                    Some(vec![json!(1)]),
                    "unsub_one_param".to_string(),
                    10,
                );
                Ok::<(), SubscriptionEmptyError>(())
            }
            None => {
                let err = ErrorObject::owned(
                    ErrorCode::ServerError(93102i32).code(),
                    "Illegal prover address.",
                    Some(format!("{:?}", prover)),
                );
                sink.close(err.clone());
                Err(CallError::Custom(err).into())
            }
        })
    }
}
