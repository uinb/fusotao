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

pub use pallet::*;

#[cfg(test)]
pub mod mock;
#[cfg(test)]
pub mod tests;

#[frame_support::pallet]
pub mod pallet {
    use codec::{Decode, Encode};
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use fuso_support::traits::{FeeBeneficiary, MarketManager, ReservableToken, Token};
    use scale_info::TypeInfo;
    use sp_runtime::{traits::AccountIdConversion, RuntimeDebug, Saturating};
    use sp_std::{convert::*, prelude::*, vec::Vec};

    pub const PALLET_ID: frame_support::PalletId = frame_support::PalletId(*b"fuso/mrk");

    pub type TokenId<T> =
        <<T as Config>::Assets as Token<<T as frame_system::Config>::AccountId>>::TokenId;

    pub type Balance<T> =
        <<T as Config>::Assets as Token<<T as frame_system::Config>::AccountId>>::Balance;

    #[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo)]
    pub enum MarketStatus {
        Registered,
        Open,
        Closed,
    }

    #[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo)]
    pub struct Broker<AccountId, Balance, BlockNumber> {
        pub beneficiary: AccountId,
        pub staked: Balance,
        pub register_at: BlockNumber,
        pub rpc_endpoint: Vec<u8>,
        pub name: Vec<u8>,
    }

    #[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo)]
    pub struct Market<Balance, BlockNumber> {
        pub min_base: Balance,
        pub base_scale: u8,
        pub quote_scale: u8,
        pub status: MarketStatus,
        pub trading_rewards: bool,
        pub liquidity_rewards: bool,
        pub unavailable_after: Option<BlockNumber>,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Assets: ReservableToken<Self::AccountId>;

        #[pallet::constant]
        type BrokerStakingThreshold: Get<Balance<Self>>;

        #[pallet::constant]
        type MarketCloseGracePeriod: Get<Self::BlockNumber>;
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
        (TokenId<T>, TokenId<T>),
        Market<Balance<T>, T::BlockNumber>,
        OptionQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub (super) fn deposit_event)]
    pub enum Event<T: Config> {
        BrokerRegistered(T::AccountId, T::AccountId),
        MarketRegistered(T::AccountId, TokenId<T>, TokenId<T>, u8, u8, Balance<T>),
        MarketOpened(T::AccountId, TokenId<T>, TokenId<T>, u8, u8, Balance<T>),
        MarketClosed(T::AccountId, TokenId<T>, TokenId<T>),
    }

    #[pallet::error]
    pub enum Error<T> {
        BrokerNotFound,
        BrokerAlreadyRegistered,
        UnsupportedQuoteCurrency,
        TokenNotFound,
        MarketNotRegistered,
        MarketCouldntUpdate,
        MarketNotExists,
        MarketScaleOverflow,
        BrokerNameTooLong,
    }

    #[pallet::pallet]
    #[pallet::without_storage_info]
    #[pallet::generate_store(pub (super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(8_790_000_000)]
        pub fn register_broker(
            origin: OriginFor<T>,
            beneficiary: T::AccountId,
            rpc_endpoint: Vec<u8>,
            name: Vec<u8>,
        ) -> DispatchResultWithPostInfo {
            let broker = ensure_signed(origin)?;
            ensure!(name.len() <= 20, Error::<T>::BrokerNameTooLong);
            let requires = T::BrokerStakingThreshold::get();
            T::Assets::transfer_token(
                &broker,
                T::Assets::native_token_id(),
                requires,
                &Self::system_account(),
            )?;
            Brokers::<T>::try_mutate(&broker, |b| -> DispatchResult {
                ensure!(b.is_none(), Error::<T>::BrokerAlreadyRegistered);
                let broker = Broker {
                    beneficiary,
                    staked: requires,
                    register_at: frame_system::Pallet::<T>::block_number(),
                    rpc_endpoint,
                    name,
                };
                b.replace(broker);
                Ok(())
            })?;
            Ok(().into())
        }

        #[pallet::weight(8_790_000_000)]
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

        #[pallet::weight(8_790_000_000)]
        pub fn apply_for_token_listing(
            origin: OriginFor<T>,
            dominator: T::AccountId,
            base: TokenId<T>,
            quote: TokenId<T>,
            base_scale: u8,
            quote_scale: u8,
            min_base: Balance<T>,
        ) -> DispatchResultWithPostInfo {
            let _ = ensure_signed(origin)?;
            Self::register_market(dominator, base, quote, base_scale, quote_scale, min_base)?;
            Ok(().into())
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn system_account() -> T::AccountId {
            PALLET_ID.try_into_account().unwrap()
        }
    }

    impl<T: Config> MarketManager<T::AccountId, TokenId<T>, Balance<T>, T::BlockNumber> for Pallet<T> {
        fn liquidity_rewards_enabled(
            dominator: T::AccountId,
            base: TokenId<T>,
            quote: TokenId<T>,
        ) -> bool {
            Markets::<T>::get(&dominator, &(base, quote))
                .map(|p| p.liquidity_rewards)
                .unwrap_or_default()
        }

        fn trading_rewards_enabled(
            dominator: T::AccountId,
            base: TokenId<T>,
            quote: TokenId<T>,
        ) -> bool {
            Markets::<T>::get(&dominator, &(base, quote))
                .map(|p| p.trading_rewards)
                .unwrap_or_default()
        }

        fn is_market_open(
            dominator: T::AccountId,
            base: TokenId<T>,
            quote: TokenId<T>,
            at: T::BlockNumber,
        ) -> bool {
            Markets::<T>::get(&dominator, &(base, quote))
                .map(|p| match (p.status, p.unavailable_after) {
                    (MarketStatus::Open, _) => true,
                    (MarketStatus::Closed, Some(deadline)) if at <= deadline => true,
                    _ => false,
                })
                .unwrap_or_default()
        }

        fn register_market(
            dominator: T::AccountId,
            base: TokenId<T>,
            quote: TokenId<T>,
            base_scale: u8,
            quote_scale: u8,
            min_base: Balance<T>,
        ) -> DispatchResult {
            ensure!(
                T::Assets::is_stable(&quote),
                Error::<T>::UnsupportedQuoteCurrency
            );
            ensure!(T::Assets::exists(&base), Error::<T>::TokenNotFound);
            ensure!(
                base_scale <= 7 && quote_scale <= 7,
                Error::<T>::MarketScaleOverflow
            );
            Markets::<T>::try_mutate(&dominator, &(base, quote), |p| -> DispatchResult {
                // we only permit to update markets either registered or not created yet
                match p {
                    None => {
                        p.replace(Market {
                            min_base,
                            base_scale,
                            quote_scale,
                            status: MarketStatus::Registered,
                            liquidity_rewards: true,
                            trading_rewards: true,
                            unavailable_after: None,
                        });
                        Ok(())
                    }
                    Some(pair) if pair.status == MarketStatus::Registered => {
                        p.replace(Market {
                            min_base,
                            base_scale,
                            quote_scale,
                            status: MarketStatus::Registered,
                            liquidity_rewards: true,
                            trading_rewards: true,
                            unavailable_after: None,
                        });
                        Ok(())
                    }
                    _ => Err(Error::<T>::MarketCouldntUpdate.into()),
                }
            })?;
            Self::deposit_event(Event::MarketRegistered(
                dominator,
                base,
                quote,
                base_scale,
                quote_scale,
                min_base,
            ));
            Ok(())
        }

        fn open_market(
            dominator: T::AccountId,
            base: TokenId<T>,
            quote: TokenId<T>,
            base_scale: u8,
            quote_scale: u8,
            min_base: Balance<T>,
        ) -> DispatchResult {
            Markets::<T>::try_mutate(&dominator, &(base, quote), |p| -> DispatchResult {
                // we need to confirm that the market info didn't change before open
                // TODO re-open?
                ensure!(p.is_some(), Error::<T>::MarketNotRegistered);
                ensure!(
                    base_scale <= 7 && quote_scale <= 7,
                    Error::<T>::MarketScaleOverflow
                );
                p.replace(Market {
                    min_base,
                    base_scale,
                    quote_scale,
                    status: MarketStatus::Open,
                    liquidity_rewards: true,
                    trading_rewards: true,
                    unavailable_after: None,
                });
                Ok(())
            })?;
            Self::deposit_event(Event::MarketOpened(
                dominator,
                base,
                quote,
                base_scale,
                quote_scale,
                min_base,
            ));
            Ok(())
        }

        fn close_market(
            dominator: T::AccountId,
            base: TokenId<T>,
            quote: TokenId<T>,
            now: T::BlockNumber,
        ) -> DispatchResult {
            Markets::<T>::try_mutate_exists(&dominator, &(base, quote), |p| -> DispatchResult {
                ensure!(p.is_some(), Error::<T>::MarketNotExists);
                let mut pair = p.take().unwrap();
                pair.status = MarketStatus::Closed;
                pair.unavailable_after = Some(now.saturating_add(T::MarketCloseGracePeriod::get()));
                p.replace(pair);
                Ok(())
            })?;
            Self::deposit_event(Event::MarketClosed(dominator, base, quote));
            Ok(())
        }
    }

    impl<T: Config> FeeBeneficiary<T::AccountId> for Pallet<T> {
        fn beneficiary(origin: T::AccountId) -> T::AccountId {
            Brokers::<T>::get(&origin)
                .map(|b| b.beneficiary)
                .unwrap_or(origin)
        }
    }
}
