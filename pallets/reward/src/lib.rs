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

pub mod curve;

#[frame_support::pallet]
pub mod pallet {


	use frame_support::{ pallet_prelude::*};
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::{MaybeSerializeDeserialize, Member,  MaybeDisplay};
	use sp_std::{fmt::Debug};

	use frame_support::traits::{ReservableCurrency, Get, Currency};
	use fuso_support::traits::ProofOfSecurity;

	use crate::curve;
	use sp_std::convert::TryInto;



	pub type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

	pub type PositiveImbalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::PositiveImbalance;



	#[pallet::type_value]
	pub(super) fn DefaultBonus<T: Config>() -> Weight { curve::CURVE[0].try_into().ok().unwrap() }

	#[pallet::storage]
	#[pallet::getter(fn bonus)]
	pub type Bonus<T: Config> = StorageValue<_, Weight, ValueQuery>; //TODO default value



	#[pallet::type_value]
	pub(super) fn DefaultVitality<T: Config>() -> Weight { 0 }

	#[pallet::storage]
	#[pallet::getter(fn vitality)]
	pub type Vitality<T: Config> = StorageValue<_, Weight, ValueQuery>; //TODO default value



	#[pallet::storage]
	#[pallet::getter(fn locked_reward)]
	pub type LockedReward<T: Config> =
	StorageMap<_, Blake2_128Concat, T::AccountId, BalanceOf<T>>;


	#[pallet::config]
	pub trait Config: frame_system::Config {

		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		// TODO
		type Currency: ReservableCurrency<Self::AccountId>;

		type Era: Get<u32>;

		type MaximumBlockWeight: Get<Weight>;

		type ExternalChainAddress: Parameter
		+ Member
		+ MaybeSerializeDeserialize
		+ Debug
		+ MaybeDisplay
		+ Ord
		+ Default;

		type ProofOfSecurity: ProofOfSecurity<Self::AccountId>;
	}


	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {

		RewardIssued(<T as frame_system::Config>::BlockNumber, BalanceOf<T>),
	}


	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);


	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {

		fn on_finalize(now: T::BlockNumber) {
			let block_weight = <frame_system::Module<T>>::block_weight();
			let weights = block_weight.get(DispatchClass::Normal);
			Vitality::<T>::put(Self::vitality() + weights);
		}

		fn on_initialize(now: T::BlockNumber) -> Weight {
			if TryInto::<u32>::try_into(now).ok().unwrap() % T::Era::get() == 0 {
				if T::ProofOfSecurity::pos_enabled() {
					Self::reward_to_pos();
					Self::reward_to_council();
				} else {
					Self::reward_to_council();
				}
				Self::release();
				Vitality::<T>::put(0);
				T::MaximumBlockWeight::get()
			} else {
				0
			}
		}


	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {


	}


	impl<T: Config> Module<T> {
		fn release() {}

		fn reward_to_pos() {}

		fn reward_to_council() {}
	}


}
