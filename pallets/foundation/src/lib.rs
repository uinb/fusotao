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


#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {

	use frame_support::{
		traits::{Currency, Get, ReservableCurrency},
		weights::Weight,
	};

	use sp_runtime::traits::{One, Saturating, Zero};
	use sp_runtime::Perbill;
	use frame_support::{ pallet_prelude::*};
	use frame_system::pallet_prelude::*;


	pub type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;


	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		type UnlockDelay: Get<Self::BlockNumber>;

		type UnlockPeriod: Get<Self::BlockNumber>;

		type UnlockRatioEachPeriod: Get<Perbill>;

		type Currency: ReservableCurrency<Self::AccountId>;

		type MaximumBlockWeight: Get<Weight>;
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {

		pub fund: Vec<(T::AccountId, BalanceOf<T>)>,//TODO
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				fund: Vec::new(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			for (account, balance) in &self.fund {
				Foundation::<T>::insert(account, balance);
			}
		}
	}


	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		PreLockedFundUnlocked(T::AccountId, BalanceOf<T>),
		UnlockedFundAllBalance(T::AccountId),
	}




	#[pallet::error]
	pub enum Error<T> {}




	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(now: T::BlockNumber) -> Weight {
			if now < T::UnlockDelay::get() {
				0
			} else {
				Self::initialize(now)
			}
		}
	}




	#[pallet::storage]
	#[pallet::getter(fn foundation)]
	pub type Foundation<T: Config> =
	StorageMap<_, Blake2_128Concat, T::AccountId,BalanceOf<T>>;



	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::call]
	impl<T:Config> Pallet<T> {

	}




	impl<T: Config> Module<T> {
		fn initialize(now: T::BlockNumber) -> Weight {
			let unlock_delay: T::BlockNumber = T::UnlockDelay::get();
			let unlock_period: T::BlockNumber = T::UnlockPeriod::get();
			if (now.saturating_sub(unlock_delay) % unlock_period) == Zero::zero() {
				let unlock_ratio_each_period: Perbill = T::UnlockRatioEachPeriod::get();
				let mut unlock_total_times: T::BlockNumber =
					unlock_ratio_each_period.saturating_reciprocal_mul_ceil(One::one());
				if unlock_total_times >= One::one() {
					unlock_total_times = unlock_total_times.saturating_sub(One::one());
				}
				let last_cycle_block = unlock_period.saturating_mul(unlock_total_times);
				let over_block = unlock_delay.saturating_add(last_cycle_block);
				if now <= over_block {
					Self::unlock_fund(now, over_block);
					return T::MaximumBlockWeight::get();
				}
			}
			0
		}

		fn unlock_fund(now: T::BlockNumber, over_block: T::BlockNumber) {
			for item in Foundation::<T>::iter() {
				// (account, balance)
				let account = item.0;
				let all_reserve_balance = Self::foundation(&account).unwrap();
				let unlock_ratio_each_period = T::UnlockRatioEachPeriod::get();

				// to be free balance
				let to_free_balance = unlock_ratio_each_period.mul_floor(all_reserve_balance);

				// if is over block, free all reserved balance
				if now == over_block {
					let mut unlock_total_times: BalanceOf<T> =
						unlock_ratio_each_period.saturating_reciprocal_mul_ceil(One::one());
					if unlock_total_times >= One::one() {
						unlock_total_times = unlock_total_times.saturating_sub(One::one());
					}
					let already_free_balance = to_free_balance.saturating_mul(unlock_total_times);
					let last_to_free_balance = all_reserve_balance.saturating_sub(already_free_balance);
					// unreserve
					T::Currency::unreserve(&account, last_to_free_balance);
					Self::deposit_event(Event::UnlockedFundAllBalance(account));
				} else {
					// unreserve
					T::Currency::unreserve(&account, to_free_balance);
					Self::deposit_event(Event::PreLockedFundUnlocked(account, to_free_balance));
				}
			}
		}
	}


}

