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

use codec::{Codec, FullCodec};
use frame_support::{traits::BalanceStatus, Parameter};
use sp_runtime::traits::{
    self, AtLeast32Bit, AtLeast32BitUnsigned, Bounded, CheckEqual, CheckedAdd, CheckedSub,
    Dispatchable, Hash, MaybeDisplay, MaybeMallocSizeOf, MaybeSerialize, MaybeSerializeDeserialize,
    Member, One, SimpleBitOps, StaticLookup, Zero,
};
use sp_runtime::DispatchResult;
use sp_std::{collections::btree_set::BTreeSet, fmt::Debug};

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

pub trait Participants<AccountId> {
    fn get_participants() -> BTreeSet<AccountId>;
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

pub trait Referendum<BlockNumber, Index> {
    type Result: Clone;

    fn proposal(start_include: BlockNumber, end_include: BlockNumber) -> Index;

    fn is_end(index: Index) -> bool;

    fn get_result(index: Index) -> Self::Result;
}

pub type ExternalChainId = u32;

pub type ExternalTransactionId = u64;

pub trait Inspector {
    type ExternalChainAddress: Clone
        + Parameter
        + Member
        + MaybeSerializeDeserialize
        + Debug
        + MaybeDisplay
        + Ord
        + Default;

    type ExternalChainBalance: AtLeast32BitUnsigned
        + FullCodec
        + Parameter
        + Member
        + Copy
        + MaybeDisplay
        + MaybeSerializeDeserialize
        + Default
        + Debug;

    type ExternalChainTransHash: Clone
        + Parameter
        + Member
        + MaybeSerializeDeserialize
        + Debug
        + MaybeDisplay
        + Ord
        + Default;

    fn expect_transaction(
        external_chain_id: ExternalChainId,
        to: Self::ExternalChainAddress,
        memo: sp_std::vec::Vec<u8>,
        amount: Self::ExternalChainBalance,
    );

    fn decl_secure_address(external_chain_id: ExternalChainId, addr: Self::ExternalChainAddress);

    fn approve(
        external_chain_id: ExternalChainId,
        from: Self::ExternalChainAddress,
        to: Self::ExternalChainAddress,
        memo: sp_std::vec::Vec<u8>,
        amount: Self::ExternalChainBalance,
        external_transaction_hash: Self::ExternalChainTransHash,
    );
}
