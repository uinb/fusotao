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

#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "256"]
pub use pallet::*;

// #[cfg(feature = "runtime-benchmarks")]
// pub mod benchmarking;

// #[cfg(test)]
// pub mod mock;
// #[cfg(test)]
// pub mod tests;

#[frame_support::pallet]
pub mod pallet {
    use codec::{Decode, Encode};
    use frame_support::{
        dispatch::{Dispatchable, GetDispatchInfo, PostDispatchInfo},
        weights::constants::RocksDbWeight,
        {pallet_prelude::*, transactional},
    };
    use frame_system::pallet_prelude::*;
    use fuso_support::{
        constants::*,
        traits::{MarketManager, PriceOracle, ReservableToken, Rewarding, Token},
    };
    use scale_info::TypeInfo;
    use sp_runtime::{
        traits::{
            AccountIdConversion, CheckedAdd, CheckedSub, StaticLookup, TrailingZeroInput, Zero,
        },
        Permill, Perquintill, RuntimeDebug,
    };
    use sp_std::{
        collections::btree_map::BTreeMap, convert::*, prelude::*, result::Result, vec::Vec,
    };

    pub const PALLET_ID: frame_support::PalletId = frame_support::PalletId(*b"fuso/mrk");
    pub type TokenId<T> =
        <<T as Config>::Assets as Token<<T as frame_system::Config>::AccountId>>::TokenId;
    pub type Balance<T> =
        <<T as Config>::Assets as Token<<T as frame_system::Config>::AccountId>>::Balance;

    #[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, Default)]
    pub struct Broker<AccountId, Balance, BlockNumber> {
        pub beneficiary: AccountId,
        pub staked: Balance,
        pub register_at: BlockNumber,
        pub rpc_endpoint: Vec<u8>,
    }

    #[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, Default)]
    pub struct Market<Balance> {
        pub min_base: Balance,
        pub base_scale: u8,
        pub quote_scale: u8,
        pub status: u8,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Assets: Token<Self::AccountId>;

        #[pallet::constant]
        type BrokerStakingThreshold: Get<Balance<Self>>;
    }

    #[pallet::storage]
    #[pallet::getter(fn brokers)]
    pub type Brokers<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Broker<T::AccountId, Balance<T>, T::BlockNumber>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn markets)]
    pub type Markets<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        (u32, u32),
        Market<Balance<T>>,
        OptionQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub (super) fn deposit_event)]
    pub enum Event<T: Config> {
        BrokerRegistered(T::AccountId, T::AccountId),
        MarketOpen(TokenId<T>, TokenId<T>, Balance<T>),
        MarketClose(TokenId<T>, TokenId<T>),
    }

    #[pallet::error]
    pub enum Error<T> {
        BrokerNotFound,
        BrokerAlreadyRegistered,
    }

    #[pallet::pallet]
    #[pallet::without_storage_info]
    #[pallet::generate_store(pub (super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(1000_000)]
        pub fn register_broker(
            origin: OriginFor<T>,
            rpc_endpoint: Vec<u8>,
            beneficiary: T::AccountId,
        ) -> DispatchResultWithPostInfo {
            let broker = ensure_signed(origin)?;
            let requires = T::BrokerStakingThreshold::get();
            T::Assets::transfer_token(
                &broker,
                T::Assets::native_token_id(),
                requires,
                // TODO
                &Self::system_account(),
            )?;
            Brokers::<T>::try_mutate(&broker, |b| -> DispatchResult {
                ensure!(b.is_none(), Error::<T>::BrokerAlreadyRegistered);
                let broker = Broker {
                    beneficiary,
                    staked: requires,
                    register_at: frame_system::Pallet::<T>::block_number(),
                    rpc_endpoint,
                };
                b.replace(broker);
                Ok(())
            })?;
            Ok(().into())
        }

        #[pallet::weight(1000_000)]
        pub fn broker_set_rpc_endpoint(
            origin: OriginFor<T>,
            rpc_endpoint: Vec<u8>,
        ) -> DispatchResultWithPostInfo {
            let broker = ensure_signed(origin)?;
            Brokers::<T>::try_mutate_exists(&broker, |b| -> DispatchResult {
                if let Some(broker) = b {
                    broker.rpc_endpoint = rpc_endpoint;
                    Ok(())
                } else {
                    Err(Error::<T>::BrokerNotFound.into())
                }
            })?;
            Ok(().into())
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn system_account() -> T::AccountId {
            PALLET_ID.try_into_account().unwrap()
        }
    }

    impl<T: Config> MarketManager<T::AccountId, TokenId<T>, Balance<T>> for Pallet<T> {
        fn is_pair_open(dominator: T::AccountId, base: &TokenId<T>, quote: &TokenId<T>) -> bool {
            true
        }

        fn open(
            dominator: T::AccountId,
            base: &TokenId<T>,
            quote: &TokenId<T>,
            base_scale: u8,
            quote_scale: u8,
            min_base: Balance<T>,
        ) -> DispatchResult {
            Ok(())
        }

        fn close(dominator: T::AccountId, base: &TokenId<T>, quote: &TokenId<T>) -> DispatchResult {
            Ok(())
        }
    }
}
