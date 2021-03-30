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

#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "256"]
use ascii::AsciiStr;
use codec::{Codec, Decode, Encode};
use frame_support::traits::BalanceStatus;
use frame_support::{decl_error, decl_event, decl_module, decl_storage, ensure, Parameter};
use frame_system::ensure_signed;
use fuso_support::traits::ReservableToken;
use sp_runtime::traits::{
    AtLeast32BitUnsigned, CheckedAdd, CheckedSub, MaybeSerializeDeserialize, Member, One,
    StaticLookup, Zero,
};
use sp_runtime::DispatchResult;
use sp_std::{fmt::Debug, vec::Vec};

#[derive(Encode, Decode, Clone, PartialEq, Eq, Default, Debug)]
pub struct TokenAccountData<Balance> {
    pub free: Balance,
    pub reserved: Balance,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Default, Debug)]
pub struct TokenInfo<Balance> {
    pub total: Balance,
    pub symbol: Vec<u8>,
}

pub trait Trait: frame_system::Trait {
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;

    type Balance: Member
        + Parameter
        + AtLeast32BitUnsigned
        + Default
        + Copy
        + Codec
        + Debug
        + MaybeSerializeDeserialize;

    type TokenId: Member
        + Parameter
        + AtLeast32BitUnsigned
        + Default
        + Copy
        + Codec
        + Debug
        + MaybeSerializeDeserialize;
}

decl_event! {
    pub enum Event<T>
    where
        AccountId = <T as frame_system::Trait>::AccountId,
        TokenId = <T as Trait>::TokenId,
        Balance = <T as Trait>::Balance,
    {
        TokenIssued(TokenId, AccountId, Balance),
        TokenTransfered(TokenId, AccountId, AccountId, Balance),
        TokenReserved(TokenId, AccountId, Balance),
        TokenUnreserved(TokenId, AccountId, Balance),
        TokenBurned(TokenId, AccountId, Balance),
        TokenRepatriated(TokenId, AccountId, AccountId, Balance),
    }
}

decl_error! {
    pub enum Error for Module<T: Trait> {
        AmountZero,
        BalanceLow,
        BalanceZero,
        InvalidTokenName,
        InvalidToken,
        InsufficientBalance,
        Overflow,
    }
}

decl_storage! {
    trait Store for Module<T: Trait> as Tokens {
        Balances get(fn get_token_balance): map hasher(blake2_128_concat)
            (T::TokenId, T::AccountId) => TokenAccountData<T::Balance>;

        Tokens get(fn get_token_info): map hasher(twox_64_concat)
            T::TokenId => TokenInfo<T::Balance>;

        NextTokenId get(fn next_token_id): T::TokenId = Zero::zero();
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        type Error = Error<T>;

        fn deposit_event() = default;

        #[weight = 10_000]
        fn issue(origin, #[compact] total: T::Balance, symbol: Vec<u8>) {
            let origin = ensure_signed(origin)?;
            ensure!(!total.is_zero(), Error::<T>::AmountZero);
            let name = AsciiStr::from_ascii(&symbol);
            ensure!(name.is_ok(), Error::<T>::InvalidTokenName);
            let name = name.unwrap();
            ensure!(name.len() >= 2 && name.len() <= 5, Error::<T>::InvalidTokenName);
            let id = Self::next_token_id();
            NextTokenId::<T>::mutate(|id| *id += One::one());
            // let token_address = <T as Trait>::Hashing::hash(&id.to_ne_bytes());
            Balances::<T>::insert((id, &origin), TokenAccountData {
                free: total,
                reserved: Zero::zero(),
            });
            Tokens::<T>::insert(id, TokenInfo {
                total: total,
                symbol: symbol,
            });
            Self::deposit_event(RawEvent::TokenIssued(id, origin, total));
        }

        #[weight = 0]
        fn transfer(origin,
            token: T::TokenId,
            target: <T::Lookup as StaticLookup>::Source,
            #[compact] amount: T::Balance,
        ) {
            let origin = ensure_signed(origin)?;
            ensure!(!amount.is_zero(), Error::<T>::AmountZero);
            let target = T::Lookup::lookup(target)?;
            <Balances<T>>::try_mutate_exists((&token, &origin), |from| -> DispatchResult {
                ensure!(from.is_some(), Error::<T>::BalanceZero);
                let mut account = from.take().unwrap();
                account.free = account
                    .free
                    .checked_sub(&amount)
                    .ok_or(Error::<T>::InsufficientBalance)?;
                match account.free == Zero::zero() && account.reserved == Zero::zero() {
                    true => {}
                    false => {
                        from.replace(account);
                    }
                }
                <Balances<T>>::try_mutate_exists((&token, &target), |to| -> DispatchResult {
                    let mut account = to.take().unwrap_or(TokenAccountData {
                        free: Zero::zero(),
                        reserved: Zero::zero(),
                    });
                    account.free = account
                        .free
                        .checked_add(&amount)
                        .ok_or(Error::<T>::Overflow)?;
                    to.replace(account);
                    Ok(())
                })?;
                Ok(())
            })?;
            Self::deposit_event(RawEvent::TokenTransfered(token, origin, target, amount));
        }
    }
}

impl<T: Trait> Module<T> {}

impl<T: Trait> ReservableToken<T::TokenId, T::AccountId> for Module<T> {
    type Balance = T::Balance;

    fn free_balance(token: &T::TokenId, who: &T::AccountId) -> Self::Balance {
        Self::get_token_balance((token, who)).free
    }

    fn reserved_balance(token: &T::TokenId, who: &T::AccountId) -> Self::Balance {
        Self::get_token_balance((token, who)).reserved
    }

    fn total_issuance(token: &T::TokenId) -> Self::Balance {
        Self::get_token_info(token).total
    }

    fn can_reserve(token: &T::TokenId, who: &T::AccountId, value: Self::Balance) -> bool {
        if value.is_zero() {
            return true;
        }
        if !<Balances<T>>::contains_key((token, who)) {
            return false;
        }
        <Balances<T>>::get((token, who))
            .free
            .checked_sub(&value)
            .is_some()
    }

    fn reserve(token: &T::TokenId, who: &T::AccountId, value: Self::Balance) -> DispatchResult {
        if value.is_zero() {
            return Ok(());
        }
        <Balances<T>>::try_mutate_exists((token, who), |account| -> DispatchResult {
            ensure!(account.is_some(), Error::<T>::BalanceZero);
            let account = account.as_mut().unwrap();
            account.free = account
                .free
                .checked_sub(&value)
                .ok_or(Error::<T>::InsufficientBalance)?;
            account.reserved = account
                .reserved
                .checked_add(&value)
                .ok_or(Error::<T>::Overflow)?;
            Ok(())
        })?;
        Self::deposit_event(RawEvent::TokenReserved(token.clone(), who.clone(), value));
        Ok(())
    }

    fn unreserve(token: &T::TokenId, who: &T::AccountId, value: Self::Balance) -> DispatchResult {
        if value.is_zero() {
            return Ok(());
        }
        <Balances<T>>::try_mutate_exists((token, who), |account| -> DispatchResult {
            ensure!(account.is_some(), Error::<T>::BalanceZero);
            let account = account.as_mut().unwrap();
            account.reserved = account
                .reserved
                .checked_sub(&value)
                .ok_or(Error::<T>::InsufficientBalance)?;
            account.free = account
                .free
                .checked_add(&value)
                .ok_or(Error::<T>::Overflow)?;
            Ok(())
        })?;
        Self::deposit_event(RawEvent::TokenUnreserved(token.clone(), who.clone(), value));
        Ok(())
    }

    fn repatriate_reserved(
        token: &T::TokenId,
        slashed: &T::AccountId,
        beneficiary: &T::AccountId,
        value: Self::Balance,
        status: BalanceStatus,
    ) -> DispatchResult {
        if slashed == beneficiary {
            return match status {
                BalanceStatus::Free => Self::unreserve(token, slashed, value),
                BalanceStatus::Reserved => Self::reserve(token, slashed, value),
            };
        }
        <Balances<T>>::try_mutate_exists((token, slashed), |from| -> DispatchResult {
            ensure!(from.is_some(), Error::<T>::BalanceZero);
            let mut account = from.take().unwrap();
            account.reserved = account
                .reserved
                .checked_sub(&value)
                .ok_or(Error::<T>::InsufficientBalance)?;
            // drop the `from` if dead
            match account.reserved == Zero::zero() && account.free == Zero::zero() {
                true => {}
                false => {
                    from.replace(account);
                }
            }
            <Balances<T>>::try_mutate_exists((token, beneficiary), |to| -> DispatchResult {
                let mut account = to.take().unwrap_or(TokenAccountData {
                    free: Zero::zero(),
                    reserved: Zero::zero(),
                });
                match status {
                    BalanceStatus::Free => {
                        account.free = account
                            .free
                            .checked_add(&value)
                            .ok_or(Error::<T>::Overflow)?;
                    }
                    BalanceStatus::Reserved => {
                        account.reserved = account
                            .reserved
                            .checked_add(&value)
                            .ok_or(Error::<T>::Overflow)?;
                    }
                }
                to.replace(account);
                Ok(())
            })?;
            Ok(())
        })?;
        Self::deposit_event(RawEvent::TokenRepatriated(
            token.clone(),
            slashed.clone(),
            beneficiary.clone(),
            value,
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use frame_support::{
        assert_noop, assert_ok, impl_outer_origin, parameter_types, weights::Weight,
    };
    use sp_core::H256;
    use sp_runtime::{
        testing::Header,
        traits::{BlakeTwo256, IdentityLookup},
        Perbill,
    };

    impl_outer_origin! {
        pub enum Origin for Test where system = frame_system {}
    }

    #[derive(Clone, Eq, PartialEq)]
    pub struct Test;
    parameter_types! {
        pub const BlockHashCount: u64 = 250;
        pub const MaximumBlockWeight: Weight = 1024;
        pub const MaximumBlockLength: u32 = 2 * 1024;
        pub const AvailableBlockRatio: Perbill = Perbill::one();
    }
    impl frame_system::Trait for Test {
        type BaseCallFilter = ();
        type Origin = Origin;
        type Index = u64;
        type Call = ();
        type BlockNumber = u64;
        type Hash = H256;
        type Hashing = BlakeTwo256;
        type AccountId = u64;
        type Lookup = IdentityLookup<Self::AccountId>;
        type Header = Header;
        type Event = ();
        type BlockHashCount = BlockHashCount;
        type MaximumBlockWeight = MaximumBlockWeight;
        type DbWeight = ();
        type BlockExecutionWeight = ();
        type ExtrinsicBaseWeight = ();
        type MaximumExtrinsicWeight = MaximumBlockWeight;
        type AvailableBlockRatio = AvailableBlockRatio;
        type MaximumBlockLength = MaximumBlockLength;
        type Version = ();
        type PalletInfo = ();
        type AccountData = ();
        type OnNewAccount = ();
        type OnKilledAccount = ();
        type SystemWeightInfo = ();
    }
    impl Trait for Test {
        type Event = ();

        type Balance = u128;

        type TokenId = u32;
    }
    type Token = Module<Test>;

    fn new_test_ext() -> sp_io::TestExternalities {
        frame_system::GenesisConfig::default()
            .build_storage::<Test>()
            .unwrap()
            .into()
    }

    #[test]
    fn issuing_token_and_transfer_should_work() {
        new_test_ext().execute_with(|| {
            assert_ok!(Token::issue(
                Origin::signed(1),
                1000000,
                br#"USDT"#.to_vec()
            ));
            let id = 0u32;
            assert_eq!(
                Token::get_token_info(&id),
                TokenInfo {
                    total: 1000000,
                    symbol: br#"USDT"#.to_vec(),
                }
            );
            assert_eq!(
                Token::get_token_balance((&id, &1)),
                TokenAccountData {
                    free: 1000000,
                    reserved: Zero::zero(),
                }
            );
            assert_ok!(Token::transfer(Origin::signed(1), id.clone(), 2, 1000000));
            assert_eq!(
                Token::get_token_balance((&id, &1)),
                TokenAccountData {
                    free: Zero::zero(),
                    reserved: Zero::zero(),
                }
            );
            assert_eq!(
                Token::get_token_balance((&id, &2)),
                TokenAccountData {
                    free: 1000000,
                    reserved: Zero::zero(),
                }
            );
        });
    }

    #[test]
    fn reservable_token_should_work() {
        new_test_ext().execute_with(|| {
            assert_ok!(Token::issue(
                Origin::signed(1),
                1000000,
                br#"USDT"#.to_vec()
            ));
            // let id = <Test as Trait>::Hashing::hash(&0u32.to_ne_bytes());
            let id = 0u32;
            assert_eq!(Token::can_reserve(&id, &1, 1000000), true);
            assert_ok!(Token::reserve(&id, &1, 500000));
            assert_eq!(Token::can_reserve(&id, &1, 1000000), false);
            assert_eq!(
                Token::get_token_balance((&id, &1)),
                TokenAccountData {
                    free: 500000,
                    reserved: 500000,
                }
            );
            assert_noop!(
                Token::transfer(Origin::signed(1), id, 2, 1000000),
                Error::<Test>::InsufficientBalance
            );
            assert_eq!(
                Token::get_token_balance((&id, &1)),
                TokenAccountData {
                    free: 500000,
                    reserved: 500000,
                }
            );
            assert_ok!(Token::reserve(&id, &1, 500000));
            assert_eq!(
                Token::get_token_balance((&id, &1)),
                TokenAccountData {
                    free: Zero::zero(),
                    reserved: 1000000,
                }
            );
            assert_ok!(Token::unreserve(&id, &1, 500000));
            assert_eq!(
                Token::get_token_balance((&id, &1)),
                TokenAccountData {
                    free: 500000,
                    reserved: 500000,
                }
            );
            assert_ok!(Token::transfer(Origin::signed(1), id.clone(), 2, 1));
            assert_ok!(Token::repatriate_reserved(
                &id,
                &1,
                &2,
                1,
                BalanceStatus::Free
            ));
            assert_eq!(
                Token::get_token_balance((&id, &1)),
                TokenAccountData {
                    free: 499999,
                    reserved: 499999,
                }
            );
            assert_eq!(
                Token::get_token_balance((&id, &2)),
                TokenAccountData {
                    free: 2,
                    reserved: Zero::zero(),
                }
            );
            assert_noop!(
                Token::repatriate_reserved(&id, &2, &1, 1, BalanceStatus::Free),
                Error::<Test>::InsufficientBalance
            );
        });
    }
}
