// Copyright 2021 UINB Technologies Pte. Ltd.

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

use crate::external_chain::ExternalChainAddress;
use codec::FullCodec;
use frame_support::{traits::BalanceStatus, Parameter};
use sp_runtime::traits::{AtLeast32BitUnsigned, MaybeDisplay, MaybeSerializeDeserialize, Member};
use sp_runtime::DispatchResult;
use sp_std::fmt::Debug;
use sp_std::vec::Vec;

pub trait ReservableToken<TokenId, AccountId> {
    type Balance: AtLeast32BitUnsigned
        + FullCodec
        + Copy
        + MaybeSerializeDeserialize
        + Default
        + Debug;

    fn free_balance(token: &TokenId, who: &AccountId) -> Self::Balance;

    fn reserved_balance(token: &TokenId, who: &AccountId) -> Self::Balance;

    fn total_issuance(token: &TokenId) -> Self::Balance;

    fn can_reserve(token: &TokenId, who: &AccountId, value: Self::Balance) -> bool;

    fn reserve(token: &TokenId, who: &AccountId, value: Self::Balance) -> DispatchResult;

    fn unreserve(token: &TokenId, who: &AccountId, value: Self::Balance) -> DispatchResult;

    fn repatriate_reserved(
        token: &TokenId,
        slashed: &AccountId,
        beneficiary: &AccountId,
        value: Self::Balance,
        status: BalanceStatus,
    ) -> DispatchResult;
}

pub trait ProofOfSecurity<AccountId> {
    type ExternalChainAddress: Parameter
        + Member
        + MaybeSerializeDeserialize
        + Debug
        + MaybeDisplay
        + Ord
        + Default;

    fn pos_enabled() -> bool;

    // TODO
}

pub trait Referendum<BlockNumber, Index, Members> {
    fn proposal(start_include: BlockNumber) -> Index;

    fn get_round() -> Index;

    fn get_result(index: Index) -> Option<Vec<Members>>;
}

pub type ExternalTransactionId = u64;

pub trait Inspector<T: frame_system::Config> {
    type ExternalChainBalance: AtLeast32BitUnsigned
        + FullCodec
        + Parameter
        + Member
        + Copy
        + MaybeDisplay
        + MaybeSerializeDeserialize
        + Default
        + Debug;

    type ExternalChainTxHash: Clone
        + Parameter
        + Member
        + MaybeSerializeDeserialize
        + Debug
        + MaybeDisplay
        + Ord
        + Default;

    fn expect_transaction(
        to: ExternalChainAddress,
        memo: Vec<u8>,
        amount: Self::ExternalChainBalance,
    );

    fn approve(
        from: ExternalChainAddress,
        to: ExternalChainAddress,
        memo: Vec<u8>,
        amount: Self::ExternalChainBalance,
        external_transaction_hash: Self::ExternalChainTxHash,
    );
}
