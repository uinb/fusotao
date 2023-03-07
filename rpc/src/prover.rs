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

pub use fuso_verifier_runtime_api::FusoVerifierRuntimeApi;

use codec::Codec;
use jsonrpsee::{
    core::{error::Error as RpcError, RpcResult},
    proc_macros::rpc,
    types::error::{CallError, ErrorCode, ErrorObject},
};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_rpc::number::NumberOrHex;
use sp_runtime::{
    generic::BlockId,
    traits::{Block as BlockT, MaybeDisplay},
};
use std::sync::Arc;

#[rpc(client, server)]
pub trait FusoVerifierApi<BlockHash, AccountId, Balance> {
    #[method(name = "verifier_currentSeasonOfDominator")]
    fn current_season_of_dominator(
        &self,
        dominator: AccountId,
        at: Option<BlockHash>,
    ) -> RpcResult<u32>;

    #[method(name = "verifier_pendingSharesOfDominator")]
    fn pending_shares_of_dominator(
        &self,
        dominator: AccountId,
        who: AccountId,
        at: Option<BlockHash>,
    ) -> RpcResult<NumberOrHex>;
}

pub struct FusoVerifier<C, B> {
    client: Arc<C>,
    _marker: std::marker::PhantomData<B>,
}

impl<C, B> FusoVerifier<C, B> {
    pub fn new(client: Arc<C>) -> Self {
        Self {
            client,
            _marker: Default::default(),
        }
    }
}

impl<C, Block, AccountId, Balance>
    FusoVerifierApiServer<<Block as BlockT>::Hash, AccountId, Balance>
    for FusoVerifier<C, (Block, AccountId, Balance)>
where
    C: Send + Sync + 'static + ProvideRuntimeApi<Block> + HeaderBackend<Block>,
    C::Api: FusoVerifierRuntimeApi<Block, AccountId, Balance>,
    Block: BlockT,
    AccountId: Codec + MaybeDisplay + Send + Sync + 'static,
    Balance: Codec + MaybeDisplay + TryInto<NumberOrHex> + Send + Sync + 'static,
{
    fn current_season_of_dominator(
        &self,
        dominator: AccountId,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<u32> {
        let api = self.client.runtime_api();
        let block_hash = BlockId::hash(at.unwrap_or_else(|| self.client.info().best_hash));
        api.current_season_of_dominator(&block_hash, dominator)
            .map_err(|e| {
                CallError::Custom(ErrorObject::owned(
                    ErrorCode::ServerError(101i32).code(),
                    "Unable to query current season",
                    Some(format!("{:?}", e)),
                ))
                .into()
            })
    }

    fn pending_shares_of_dominator(
        &self,
        dominator: AccountId,
        who: AccountId,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<NumberOrHex> {
        let api = self.client.runtime_api();
        let block_hash = BlockId::hash(at.unwrap_or_else(|| self.client.info().best_hash));
        let shares = api
            .pending_shares_of_dominator(&block_hash, dominator, who)
            .map_err(|e| {
                CallError::Custom(ErrorObject::owned(
                    ErrorCode::ServerError(100i32).code(),
                    "Unable to query pending shares",
                    Some(format!("{:?}", e)),
                ))
            })?;
        shares.try_into().map_err(|_| {
            CallError::Custom(ErrorObject::owned(
                ErrorCode::InvalidParams.code(),
                "doesn't fit in NumberOrHex representation",
                None::<()>,
            ))
            .into()
        })
    }
}
