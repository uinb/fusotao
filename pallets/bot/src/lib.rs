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

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
pub mod mock;
#[cfg(test)]
pub mod tests;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::{pallet_prelude::*, transactional};
    use frame_system::pallet_prelude::*;
    use fuso_support::traits::{Custody, ReservableToken, Token};
    use sp_runtime::traits::{AccountIdConversion, Zero};

    pub const PALLET_ID: frame_support::PalletId = frame_support::PalletId(*b"fuso/bot");

    pub type TokenId<T> =
        <<T as Config>::Assets as Token<<T as frame_system::Config>::AccountId>>::TokenId;

    pub type Balance<T> =
        <<T as Config>::Assets as Token<<T as frame_system::Config>::AccountId>>::Balance;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Assets: ReservableToken<Self::AccountId>;

        type Custody: Custody<Self::AccountId, TokenId<Self>, Balance<Self>>;

        #[pallet::constant]
        type BotStakingThreshold: Get<Balance<Self>>;
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub (super) fn deposit_event)]
    pub enum Event<T: Config> {
        BotCreated(T::AccountId, TokenId<T>, TokenId<T>),
    }

    #[pallet::error]
    pub enum Error<T> {
        BotAlreadyExist,
        BotNotFound,
        InvalidCurrency,
        BeyondMaxInstance,
        MinimalAmountRequired,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    #[derive(Encode, Decode, Clone, PartialEq, Eq, Default, TypeInfo, Debug)]
    pub struct Bot<Balance, TokenId> {
        pub staked: Balance,
        pub symbol: (TokenId, TokenId),
        pub name: Vec<u8>,
        pub max_instance: u32,
        pub current_instance: u32,
        pub min_base: Balance,
        pub min_quote: Balance,
        pub desc: Vec<u8>,
    }

    #[pallet::storage]
    #[pallet::getter(fn bots)]
    pub type Bots<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, Bot<Balance<T>, TokenId<T>>, OptionQuery>;

    #[pallet::pallet]
    #[pallet::without_storage_info]
    #[pallet::generate_store(pub (super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(3750_000_000)]
        pub fn register(
            origin: OriginFor<T>,
            symbol: (TokenId<T>, TokenId<T>),
            name: Vec<u8>,
            max_instance: u32,
            min_base: Balance<T>,
            min_quote: Balance<T>,
            desc: Vec<u8>,
        ) -> DispatchResultWithPostInfo {
            let creator = ensure_signed(origin)?;
            Self::register_bot(
                creator,
                symbol,
                name,
                max_instance,
                min_base,
                min_quote,
                desc,
            )?;
            Ok(().into())
        }

        #[pallet::weight(3750_000_000)]
        pub fn deposit(
            origin: OriginFor<T>,
            bot_id: T::AccountId,
            dominator: T::AccountId,
            base_amount: Balance<T>,
            quote_amount: Balance<T>,
        ) -> DispatchResultWithPostInfo {
            let from = ensure_signed(origin)?;
            Self::deposit_into_sub(from, bot_id, dominator, base_amount, quote_amount)?;
            Ok(().into())
        }

        /// revoke assets in sub-accounts from a dominator
        #[pallet::weight(8390_000_000)]
        pub fn revoke(
            origin: OriginFor<T>,
            bot_id: T::AccountId,
            dominator: T::AccountId,
            currency: TokenId<T>,
        ) -> DispatchResultWithPostInfo {
            let from = ensure_signed(origin)?;
            Self::revoke_to_sub(from, bot_id, dominator, currency)?;
            Ok(().into())
        }

        /// withdraw assets from sub-accounts
        #[pallet::weight(8390_000_000)]
        pub fn withdraw(
            origin: OriginFor<T>,
            bot_id: T::AccountId,
            currency: TokenId<T>,
        ) -> DispatchResultWithPostInfo {
            let from = ensure_signed(origin)?;
            Self::withdraw_from_sub(from, bot_id, currency)?;
            Ok(().into())
        }
    }

    impl<T: Config> Pallet<T> {
        #[transactional]
        pub(crate) fn register_bot(
            creator: T::AccountId,
            symbol: (TokenId<T>, TokenId<T>),
            name: Vec<u8>,
            max_instance: u32,
            min_base: Balance<T>,
            min_quote: Balance<T>,
            desc: Vec<u8>,
        ) -> DispatchResult {
            ensure!(
                !Bots::<T>::contains_key(&creator),
                Error::<T>::BotAlreadyExist
            );
            let requires = T::BotStakingThreshold::get();
            T::Assets::transfer_token(
                &creator,
                T::Assets::native_token_id(),
                requires,
                &Self::system_account(),
            )?;
            let bot = Bot {
                staked: requires,
                symbol: symbol.clone(),
                name,
                max_instance,
                current_instance: 0,
                min_base,
                min_quote,
                desc,
            };
            Bots::<T>::insert(creator.clone(), bot);
            Self::deposit_event(Event::BotCreated(creator, symbol.0, symbol.1));
            Ok(())
        }

        #[transactional]
        pub(crate) fn deposit_into_sub(
            from: T::AccountId,
            bot_id: T::AccountId,
            dominator: T::AccountId,
            base_amount: Balance<T>,
            quote_amount: Balance<T>,
        ) -> DispatchResult {
            let bot = Bots::<T>::get(&bot_id).ok_or(Error::<T>::BotNotFound)?;
            ensure!(
                bot.current_instance + 1 <= bot.max_instance,
                Error::<T>::BeyondMaxInstance
            );
            ensure!(
                bot.min_base <= base_amount,
                Error::<T>::MinimalAmountRequired
            );
            ensure!(
                bot.min_quote <= quote_amount,
                Error::<T>::MinimalAmountRequired
            );
            let sub0 = Self::derive_sub_account(from.clone(), bot_id.clone(), bot.symbol.0);
            let sub1 = Self::derive_sub_account(from.clone(), bot_id.clone(), bot.symbol.1);
            T::Assets::transfer_token(&from, bot.symbol.0, base_amount, &sub0)?;
            T::Assets::transfer_token(&from, bot.symbol.1, quote_amount, &sub0)?;
            T::Custody::authorize_to(sub0.clone(), dominator.clone(), bot.symbol.0, base_amount)?;
            T::Custody::authorize_to(sub1.clone(), dominator, bot.symbol.1, quote_amount)?;
            Bots::<T>::mutate(&bot_id, |b| b.as_mut().unwrap().current_instance += 1);
            Ok(())
        }

        #[transactional]
        pub(crate) fn withdraw_from_sub(
            from: T::AccountId,
            bot_id: T::AccountId,
            currency: TokenId<T>,
        ) -> DispatchResult {
            let bot = Bots::<T>::get(&bot_id).ok_or(Error::<T>::BotNotFound)?;
            let sub0 = Self::derive_sub_account(from.clone(), bot_id.clone(), bot.symbol.0);
            let sub1 = Self::derive_sub_account(from.clone(), bot_id.clone(), bot.symbol.1);
            T::Assets::transfer_token(
                &sub0,
                currency,
                T::Assets::free_balance(&currency, &sub0),
                &from,
            )?;
            T::Assets::transfer_token(
                &sub1,
                currency,
                T::Assets::free_balance(&currency, &sub1),
                &from,
            )?;
            Ok(())
        }

        #[transactional]
        pub(crate) fn revoke_to_sub(
            from: T::AccountId,
            bot_id: T::AccountId,
            dominator: T::AccountId,
            currency: TokenId<T>,
        ) -> DispatchResult {
            let bot = Bots::<T>::get(&bot_id).ok_or(Error::<T>::BotNotFound)?;
            let sub0 = Self::derive_sub_account(from.clone(), bot_id.clone(), bot.symbol.0);
            let sub1 = Self::derive_sub_account(from.clone(), bot_id.clone(), bot.symbol.1);
            let reserved0 = T::Assets::reserved_balance(&currency, &sub0);
            let reserved1 = T::Assets::reserved_balance(&currency, &sub1);
            if reserved0 != Zero::zero() {
                T::Custody::revoke_from(
                    sub0.clone(),
                    dominator.clone(),
                    currency,
                    reserved0,
                    None,
                )?;
            }
            if reserved1 != Zero::zero() {
                T::Custody::revoke_from(
                    sub1.clone(),
                    dominator.clone(),
                    currency,
                    reserved1,
                    None,
                )?;
            }
            Ok(())
        }

        pub(crate) fn derive_sub_account(
            from: T::AccountId,
            bot: T::AccountId,
            currency: TokenId<T>,
        ) -> T::AccountId {
            let h = (b"-*-#fuso-proxy#-*-", currency, from, bot)
                .using_encoded(sp_core::hashing::blake2_256);
            Decode::decode(&mut h.as_ref()).expect("32 bytes; qed")
        }

        pub(crate) fn system_account() -> T::AccountId {
            PALLET_ID.try_into_account().unwrap()
        }
    }
}
