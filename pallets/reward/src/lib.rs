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
    use frame_support::pallet_prelude::DispatchResultWithPostInfo;
    use frame_support::{pallet_prelude::*, traits::Get, transactional};
    use frame_system::ensure_signed;
    use frame_system::pallet_prelude::*;
    use fuso_support::traits::{Rewarding, Token};
    use sp_runtime::{
        traits::{CheckedAdd, CheckedSub, Zero},
        DispatchError, DispatchResult, Perquintill,
    };
    use sp_std::result::Result;

    pub type Volume<T> =
        <<T as Config>::Asset as Token<<T as frame_system::Config>::AccountId>>::Balance;

    pub type Balance<T> =
        <<T as Config>::Asset as Token<<T as frame_system::Config>::AccountId>>::Balance;

    pub type TokenId<T> =
        <<T as Config>::Asset as Token<<T as frame_system::Config>::AccountId>>::TokenId;

    pub type Symbol<T> = (TokenId<T>, TokenId<T>);

    pub type Era<T> = <T as frame_system::Config>::BlockNumber;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Asset: Token<Self::AccountId>;

        #[pallet::constant]
        type EraDuration: Get<Self::BlockNumber>;

        // DEPRECATED
        #[pallet::constant]
        type RewardsPerEra: Get<Balance<Self>>;

        #[pallet::constant]
        type RewardTerminateAt: Get<Self::BlockNumber>;
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub (super) fn deposit_event)]
    pub enum Event<T: Config> {
        RewardClaimed(T::AccountId, Balance<T>),
        EraRewardsUpdated(Balance<T>),
    }

    #[pallet::error]
    pub enum Error<T> {
        Overflow,
        DivideByZero,
        RewardNotFound,
        InsufficientLiquidity,
        LiquidityNotFound,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    #[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, Default)]
    pub struct Reward<Balance, Volume, Era> {
        pub confirmed: Balance,
        pub pending_vol: Volume,
        pub last_modify: Era,
    }

    #[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, Default)]
    pub struct LiquidityProvider<Volume, BlockNumber> {
        pub volume: Volume,
        pub start_from: BlockNumber,
    }

    #[pallet::storage]
    #[pallet::getter(fn liquidity_pool)]
    pub type LiquidityPool<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        Symbol<T>,
        LiquidityProvider<Volume<T>, T::BlockNumber>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn rewards)]
    pub type Rewards<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Reward<Balance<T>, Volume<T>, Era<T>>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn era_rewards)]
    pub type EraRewards<T: Config> = StorageValue<_, Balance<T>, ValueQuery, T::RewardsPerEra>;

    #[pallet::storage]
    #[pallet::getter(fn volumes)]
    pub type Volumes<T: Config> = StorageMap<_, Blake2_128Concat, Era<T>, Volume<T>, ValueQuery>;

    #[pallet::pallet]
    #[pallet::without_storage_info]
    #[pallet::generate_store(pub (super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::call]
    impl<T: Config> Pallet<T>
    where
        Volume<T>: Into<u128>,
        Balance<T>: From<u128>,
    {
        #[pallet::weight(1_000_000_000_000_000)]
        pub fn take_reward(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            let at = frame_system::Pallet::<T>::block_number();
            let reward = Self::claim_reward(&who, at)?;
            Self::deposit_event(Event::RewardClaimed(who, reward));
            Ok(().into())
        }

        #[pallet::weight(0)]
        pub fn set_era_rewards(
            origin: OriginFor<T>,
            era_rewards: Balance<T>,
        ) -> DispatchResultWithPostInfo {
            let _ = ensure_root(origin)?;
            EraRewards::<T>::put(era_rewards);
            Self::deposit_event(Event::EraRewardsUpdated(era_rewards));
            Ok(().into())
        }
    }

    impl<T: Config> Pallet<T>
    where
        Volume<T>: Into<u128>,
        Balance<T>: From<u128>,
    {
        #[transactional]
        fn claim_reward(
            who: &T::AccountId,
            at: T::BlockNumber,
        ) -> Result<Balance<T>, DispatchError> {
            let at = at - at % Self::era_duration();
            let confirmed = Self::rotate_reward(at, Zero::zero(), &who)?;
            if confirmed == Zero::zero() {
                return Ok(Zero::zero());
            }
            Rewards::<T>::try_mutate_exists(who, |r| -> Result<Balance<T>, DispatchError> {
                ensure!(r.is_some(), Error::<T>::RewardNotFound);
                let mut reward: Reward<Balance<T>, Volume<T>, Era<T>> = r.take().unwrap();
                let confirmed = reward.confirmed;
                reward.confirmed = Zero::zero();
                if reward.pending_vol > Zero::zero() {
                    r.replace(reward);
                }
                if confirmed > Zero::zero() {
                    T::Asset::try_mutate_account(&T::Asset::native_token_id(), &who, |b| {
                        T::Asset::try_mutate_issuance(&T::Asset::native_token_id(), |v| {
                            *v = v.checked_add(&confirmed).ok_or(Error::<T>::Overflow)?;
                            Ok(())
                        })?;
                        Ok(b.0 += confirmed)
                    })?;
                }
                Ok(confirmed)
            })
        }

        #[transactional]
        fn rotate_reward(
            at: T::BlockNumber,
            vol: Volume<T>,
            account: &T::AccountId,
        ) -> Result<Balance<T>, DispatchError> {
            Rewards::<T>::try_mutate(account, |r| -> Result<Balance<T>, DispatchError> {
                if at == r.last_modify {
                    r.pending_vol = r
                        .pending_vol
                        .checked_add(&vol)
                        .ok_or(Error::<T>::Overflow)?;
                    Ok(r.confirmed)
                } else {
                    if r.pending_vol == Zero::zero() {
                        r.pending_vol = vol;
                        r.last_modify = at;
                    } else {
                        let pending_vol: u128 = r.pending_vol.into();
                        let total_vol: u128 = Volumes::<T>::get(r.last_modify).into();
                        ensure!(total_vol > 0, Error::<T>::DivideByZero);
                        let p: Perquintill = Perquintill::from_rational(pending_vol, total_vol);
                        let mut era_reward: u128 = EraRewards::<T>::get().into();
                        let now = frame_system::Pallet::<T>::block_number();
                        if now > T::RewardTerminateAt::get() {
                            era_reward = 0u128;
                        }
                        let a = p * era_reward;
                        r.confirmed = r
                            .confirmed
                            .checked_add(&a.into())
                            .ok_or(Error::<T>::Overflow)?;
                        r.pending_vol = vol;
                        r.last_modify = at;
                    }
                    Ok(r.confirmed)
                }
            })
        }
    }

    impl<T: Config> Rewarding<T::AccountId, Volume<T>, Symbol<T>, T::BlockNumber> for Pallet<T>
    where
        Volume<T>: Into<u128>,
        Balance<T>: From<u128>,
    {
        type Balance = Balance<T>;

        fn era_duration() -> T::BlockNumber {
            T::EraDuration::get()
        }

        fn total_volume(at: T::BlockNumber) -> Volume<T> {
            Self::volumes(at - at % Self::era_duration())
        }

        fn acked_reward(who: &T::AccountId) -> Self::Balance {
            Self::rewards(who).confirmed
        }

        #[transactional]
        fn save_trading(
            trader: &T::AccountId,
            vol: Volume<T>,
            at: T::BlockNumber,
        ) -> DispatchResult {
            if vol == Zero::zero() {
                return Ok(());
            }
            let at = at - at % Self::era_duration();
            Volumes::<T>::try_mutate(&at, |v| -> DispatchResult {
                Ok(*v = v.checked_add(&vol).ok_or(Error::<T>::Overflow)?)
            })?;
            Self::rotate_reward(at, vol, trader)?;
            Ok(())
        }

        /// put liquidity `vol` into `symbol`(override the previous value) `at` block number.
        /// NOTE: if the `maker` has already added liquidity at the same `symbol`, then the block number will be updated to `at`.
        fn put_liquidity(
            maker: &T::AccountId,
            symbol: Symbol<T>,
            vol: Volume<T>,
            at: T::BlockNumber,
        ) {
            if vol.is_zero() {
                return;
            }
            LiquidityPool::<T>::insert(
                maker,
                &symbol,
                LiquidityProvider {
                    volume: vol,
                    start_from: at,
                },
            );
        }

        /// when liquidity is took out, the liquidity provider will get the reward.
        /// the rewards are calculated in the formula below:
        /// contribution ƒi = vol * min(current - from, era_duration) / 720
        /// rewards of contribution ∂ = ƒi / ∑ƒi * era_rewards
        /// NOTE: `vol` should be volume rather than amount
        fn consume_liquidity(
            maker: &T::AccountId,
            symbol: Symbol<T>,
            vol: Volume<T>,
            current: T::BlockNumber,
        ) -> DispatchResult {
            if Self::remove_liquidity(maker, symbol, vol).is_err() {
                return Ok(());
            }
            // TODO
            Self::save_trading(maker, vol, current)
        }

        /// remove liquidity
        fn remove_liquidity(
            maker: &T::AccountId,
            symbol: Symbol<T>,
            vol: Volume<T>,
        ) -> DispatchResult {
            if vol.is_zero() {
                return Ok(());
            }
            LiquidityPool::<T>::try_mutate_exists(maker, &symbol, |l| -> DispatchResult {
                let liquidity = l.take();
                ensure!(liquidity.is_some(), Error::<T>::LiquidityNotFound);
                let mut liquidity = liquidity.unwrap();
                liquidity.volume = liquidity
                    .volume
                    .checked_sub(&vol)
                    .ok_or(Error::<T>::InsufficientLiquidity)?;
                if liquidity.volume > Zero::zero() {
                    l.replace(liquidity);
                }
                Ok(())
            })?;
            Ok(())
        }
    }
}
