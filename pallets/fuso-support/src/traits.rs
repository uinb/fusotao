// Copyright 2021-2023 UINB Technologies Pte. Ltd.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{ChainId, XToken};
use codec::{Codec, EncodeLike, MaxEncodedLen};
use frame_support::{traits::BalanceStatus, Parameter};
use sp_runtime::{
    traits::{AtLeast32BitUnsigned, MaybeSerializeDeserialize, Member},
    DispatchError, DispatchResult,
};

pub trait Token<AccountId> {
    type Balance: Member
        + Parameter
        + AtLeast32BitUnsigned
        + Default
        + Copy
        + Codec
        + MaybeSerializeDeserialize
        + MaxEncodedLen;

    type TokenId: Member
        + Parameter
        + AtLeast32BitUnsigned
        + Default
        + Copy
        + Codec
        + MaybeSerializeDeserialize;

    fn create(token: XToken<Self::Balance>) -> Result<Self::TokenId, DispatchError>;

    fn native_token_id() -> Self::TokenId;

    fn is_stable(token_id: &Self::TokenId) -> bool;

    fn free_balance(token: &Self::TokenId, who: &AccountId) -> Self::Balance;

    fn total_issuance(token: &Self::TokenId) -> Self::Balance;

    fn transfer_token(
        who: &AccountId,
        token: Self::TokenId,
        amount: Self::Balance,
        receiver: &AccountId,
    ) -> Result<Self::Balance, DispatchError>;

    fn try_mutate_account<R>(
        token: &Self::TokenId,
        who: &AccountId,
        f: impl FnOnce(&mut (Self::Balance, Self::Balance)) -> Result<R, DispatchError>,
    ) -> Result<R, DispatchError>;

    fn try_mutate_issuance(
        token: &Self::TokenId,
        f: impl FnOnce(&mut Self::Balance) -> Result<(), DispatchError>,
    ) -> Result<(), DispatchError>;

    fn token_external_decimals(token: &Self::TokenId) -> Result<u8, DispatchError>;
}

pub trait DecimalsUnifier<Balance> {
    fn transform_decimals_to_standard(amount: Balance, external_decimals: u8) -> Balance;
    fn transform_decimals_to_external(amount: Balance, external_decimals: u8) -> Balance;
}

pub trait ReservableToken<AccountId>: Token<AccountId> {
    /// Same result as `reserve(who, value)` (but without the side-effects) assuming there
    /// are no balance changes in the meantime.
    fn can_reserve(token: &Self::TokenId, who: &AccountId, value: Self::Balance) -> bool;

    /// The amount of the balance of a given account that is externally reserved; this can still get
    /// slashed, but gets slashed last of all.
    ///
    /// This balance is a 'reserve' balance that other subsystems use in order to set aside tokens
    /// that are still 'owned' by the account holder, but which are suspendable.
    ///
    /// When this balance falls below the value of `ExistentialDeposit`, then this 'reserve account'
    /// is deleted: specifically, `ReservedBalance`.
    ///
    /// `system::AccountNonce` is also deleted if `FreeBalance` is also zero (it also gets
    /// collapsed to zero if it ever becomes less than `ExistentialDeposit`.
    fn reserved_balance(token: &Self::TokenId, who: &AccountId) -> Self::Balance;

    /// Moves `value` from balance to reserved balance.
    ///
    /// If the free balance is lower than `value`, then no funds will be moved and an `Err` will
    /// be returned to notify of this.
    fn reserve(token: &Self::TokenId, who: &AccountId, value: Self::Balance) -> DispatchResult;

    /// Moves up to `value` from reserved balance to free balance.
    fn unreserve(token: &Self::TokenId, who: &AccountId, value: Self::Balance) -> DispatchResult;

    /// Moves up to `value` from reserved balance of account `slashed` to balance of account
    /// `beneficiary`. `beneficiary` must exist for this to succeed. If it does not, `Err` will be
    /// returned. Funds will be placed in either the `free` balance or the `reserved` balance,
    /// depending on the `status`.
    ///
    /// As much funds up to `value` will be deducted as possible. If this is less than `value`,
    /// then `Ok(non_zero)` will be returned.
    fn repatriate_reserved(
        token: &Self::TokenId,
        slashed: &AccountId,
        beneficiary: &AccountId,
        value: Self::Balance,
        status: BalanceStatus,
    ) -> DispatchResult;
}

pub trait NamedReservableToken<AccountId>: Token<AccountId> {
    type ReserveIdentifier;

    /// Same result as `reserve(who, value)` (but without the side-effects) assuming there
    /// are no balance changes in the meantime.
    fn can_reserve_named(
        id: &Self::ReserveIdentifier,
        token: &Self::TokenId,
        who: &AccountId,
        value: Self::Balance,
    ) -> bool;

    /// The amount of the balance of a given account that is externally reserved; this can still get
    /// slashed, but gets slashed last of all.
    ///
    /// This balance is a 'reserve' balance that other subsystems use in order to set aside tokens
    /// that are still 'owned' by the account holder, but which are suspendable.
    ///
    /// When this balance falls below the value of `ExistentialDeposit`, then this 'reserve account'
    /// is deleted: specifically, `ReservedBalance`.
    ///
    /// `system::AccountNonce` is also deleted if `FreeBalance` is also zero (it also gets
    /// collapsed to zero if it ever becomes less than `ExistentialDeposit`.
    fn reserved_balance_named(
        id: &Self::ReserveIdentifier,
        token: &Self::TokenId,
        who: &AccountId,
    ) -> Self::Balance;

    /// Moves `value` from balance to reserved balance.
    ///
    /// If the free balance is lower than `value`, then no funds will be moved and an `Err` will
    /// be returned to notify of this.
    fn reserve_named(
        id: &Self::ReserveIdentifier,
        token: &Self::TokenId,
        who: &AccountId,
        value: Self::Balance,
    ) -> sp_std::result::Result<Self::Balance, DispatchError>;

    /// Moves up to `value` from reserved balance to free balance.
    fn unreserve_named(
        id: &Self::ReserveIdentifier,
        token: &Self::TokenId,
        who: &AccountId,
        value: Self::Balance,
    ) -> sp_std::result::Result<Self::Balance, DispatchError>;

    /// Moves up to `value` from reserved balance of account `slashed` to balance of account
    /// `beneficiary`. `beneficiary` must exist for this to succeed. If it does not, `Err` will be
    /// returned. Funds will be placed in either the `free` balance or the `reserved` balance,
    /// depending on the `status`.
    ///
    /// As much funds up to `value` will be deducted as possible. If this is less than `value`,
    /// then `Ok(non_zero)` will be returned.
    fn repatriate_reserved_named(
        id: &Self::ReserveIdentifier,
        token: &Self::TokenId,
        slashed: &AccountId,
        beneficiary: &AccountId,
        value: Self::Balance,
        status: BalanceStatus,
    ) -> sp_std::result::Result<Self::Balance, DispatchError>;
}

pub trait Rewarding<AccountId, Volume: Copy, BlockNumber> {
    /// $TAO
    type Balance: Member
        + Parameter
        + AtLeast32BitUnsigned
        + Default
        + Copy
        + Codec
        + MaybeSerializeDeserialize;

    fn era_duration() -> BlockNumber;

    fn total_volume(at: BlockNumber) -> Volume;

    fn acked_reward(who: &AccountId) -> Self::Balance;

    fn save_trading(trader: &AccountId, amount: Volume, at: BlockNumber) -> DispatchResult;
}

pub trait Agent<AccountId> {
    type Origin: Codec;
    type Message: EncodeLike + Codec;

    /// bind the origin to an appchain account without private key
    /// function RegisterInterchainAccount(counterpartyPortId: Identifier, connectionID: Identifier) returns (nil)
    fn register_agent(origin: Self::Origin) -> Result<AccountId, DispatchError>;

    /// function AuthenticateTx(msgs []Any, connectionId string, portId string) returns (error)
    fn authenticate_tx(origin: Self::Origin, msg: Self::Message) -> Result<(), DispatchError>;

    /// function ExecuteTx(sourcePort: Identifier, channel Channel, msgs []Any) returns (resultString, error)
    fn execute_tx(origin: Self::Origin, msg: Self::Message) -> DispatchResult;
}

pub trait Smuggler<AccountId> {
    fn is_wanted(t: &AccountId) -> bool;

    fn repatriate_if_wanted(t: &AccountId) -> bool;
}

impl<T> Smuggler<T> for () {
    fn is_wanted(_: &T) -> bool {
        false
    }

    fn repatriate_if_wanted(_: &T) -> bool {
        false
    }
}

pub trait PriceOracle<TokenId, Balance: Default, BlockNumber> {
    fn get_price(token_id: &TokenId) -> Balance;

    fn set_price(token_id: TokenId, amount: Balance, volume: Balance, at: BlockNumber);
}

impl<TokenId, Balance: Default, BlockNumber> PriceOracle<TokenId, Balance, BlockNumber> for () {
    fn get_price(_token_id: &TokenId) -> Balance {
        Default::default()
    }

    fn set_price(_token_id: TokenId, _amount: Balance, _volume: Balance, _at: BlockNumber) {}
}

pub trait ChainIdOf<Balance> {
    fn chain_id_of(token_info: &XToken<Balance>) -> ChainId;
}
