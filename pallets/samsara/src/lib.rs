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

#[frame_support:: pallet]
pub mod pallet {

	use frame_support::{ pallet_prelude::*};
	use frame_system::pallet_prelude::*;
	use codec::{Decode, Encode};
	use frame_support::{
		dispatch::DispatchResult,
		ensure,
		traits::{Get, LockIdentifier, LockableCurrency, WithdrawReasons},
		weights::{DispatchClass, Weight},
	};
	use frame_system::ensure_signed;
	use sp_runtime::{
		traits::{CheckedAdd, Saturating, Zero},
		Perbill,
	};
	use sp_std::vec::Vec;


	pub type BalanceOf<T> = <<T as Config>::Locks as pallet_balances::Config>::Balance;
	pub type AccountIdOf<T> = <<T as Config>::Locks as frame_system::Config>::AccountId;

	pub type ConfigBlockNumber<T> = <T as frame_system::Config>::BlockNumber;
	pub type ConfigAccountId<T> = <T as frame_system::Config>::AccountId;

	pub const SAMSARA_ID: LockIdentifier = *b"samsaras";

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		type VitalityBlock: Get<u32>;

		type TermDuration: Get<Self::BlockNumber>;

		type VoteDuration: Get<Self::BlockNumber>;

		type MinimumVitalityWeight: Get<Weight>;

		type VoteBalancePerbill: Get<Perbill>;

		type Currency: LockableCurrency<Self::AccountId>;

		type Locks: pallet_balances::Config;

	}


	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		ClearVote(ConfigBlockNumber<T>),
		Voted(ConfigAccountId<T>, BalanceOf<T>),
	}

	#[pallet::error]
	pub enum Error<T> {

		AmountZero,
		VoteNotStarted,
		InsufficientBalance,
		Overflow,

	}





	#[pallet::storage]
	#[pallet::getter(fn voter_members)]
	pub(crate) type VoterMembers<T> = StorageValue<_,Vec<(ConfigAccountId<T>, BalanceOf<T>)>,ValueQuery>;


	#[pallet::storage]
	#[pallet::getter(fn vitality)]
	pub(crate) type Vitality<T> = StorageValue<_,Vec<(ConfigBlockNumber<T>, Weight)>, ValueQuery>;


	#[pallet::type_value]
	pub(crate) fn DefaultVitalityTotal<T: Config>() -> Weight { 0 }


	#[pallet::storage]
	#[pallet::getter(fn vitality_total)]
	pub(crate) type VitalityTotal<T> = StorageValue<_, Weight,ValueQuery>;  //TODO defaultValue



	#[pallet::storage]
	#[pallet::getter(fn start_vote)]
	pub type StartVote<T> = StorageValue<_, Option<ConfigBlockNumber<T>>,ValueQuery>;




	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);



	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_finalize(now: T::BlockNumber) {
			Self::finalize(now);
		}
	}





	#[pallet::call]
	impl<T: Config> Pallet<T> {



		// vote
		#[pallet::weight(1_000_000)]
		fn vote(origin: OriginFor<T>,
				/*#[compact]*/ amount: BalanceOf<T>) -> DispatchResultWithPostInfo {
			let sender = ensure_signed(origin)?;

			// check input amount
			ensure!(!amount.is_zero(), Error::<T>::AmountZero);

			// sender encode
			let from: Vec<u8> = sender.encode();
			let account =
				<<T as Config>::Locks as frame_system::Config>::AccountId::decode(&mut from.as_ref())
					.unwrap_or_default();

			let free_balance = <pallet_balances::Module<T::Locks>>::usable_balance(&account);
			ensure!(free_balance > amount, Error::<T>::InsufficientBalance);

			ensure!(Self::start_vote().is_some(), Error::<T>::VoteNotStarted);

			// push member
			VoterMembers::<T>::try_mutate(|member| -> DispatchResult {
				member.push((sender.clone(), amount));
				Ok(())
			})?;

			// lock currency
			Self::lock_currency(&account, amount)?;

			Self::deposit_event(Event::Voted(sender, amount));

			Ok(().into())
		}



	}


	impl<T: Config> Module<T> {
		fn finalize(now: T::BlockNumber) {
			let vitality_len = Self::vitality().len() as u32;
			let vitality_block = T::VitalityBlock::get();

			if now > T::TermDuration::get() {
				let mut vitality = Self::vitality();
				let block_weight = <frame_system::Module<T>>::block_weight();
				let current_weight = block_weight.get(DispatchClass::Normal);

				if vitality_len >= vitality_block {
					let (_, first_weight) = vitality.remove(0);
					VitalityTotal::<T>::put(Self::vitality_total() - first_weight + current_weight);
				} else {
					VitalityTotal::<T>::put(Self::vitality_total() + current_weight);
				}

				vitality.push((now, *current_weight));

				Vitality::<T>::put(vitality);
			}

			Self::check_vote(&vitality_len, &vitality_block);
		}

		fn check_vote(vitality_len: &u32, vitality_block: &u32) {
			if vitality_len == vitality_block
				&& Self::vitality_total() < T::MinimumVitalityWeight::get()
			{
				if let Some(start_vote_block) = Self::start_vote() {
					let vote_block =
						frame_system::Module::<T>::block_number().saturating_sub(start_vote_block);
					// vote over
					if vote_block > T::VoteDuration::get() {
						Self::check_amount();
					}
				} else {
				 	StartVote::<T>::put(Option::from(frame_system::Module::<T>::block_number()));
				}
			}
		}

		// lock currency
		fn lock_currency(account: &AccountIdOf<T>, amount: BalanceOf<T>) -> DispatchResult {
			let lock_balance = pallet_balances::Locks::<T::Locks>::get(account);
			let vote_lock = lock_balance.iter().find(|b| b.id == SAMSARA_ID);
			if let Some(lock) = vote_lock {
				let total_lock_balance = amount
					.checked_add(&lock.amount)
					.ok_or(Error::<T>::Overflow)?;
				<pallet_balances::Module<T::Locks>>::extend_lock(
					SAMSARA_ID,
					account,
					total_lock_balance,
					WithdrawReasons::all(),
				);
			} else {
				<pallet_balances::Module<T::Locks>>::set_lock(
					SAMSARA_ID,
					account,
					amount,
					WithdrawReasons::all(),
				);
			}
			Ok(())
		}

		fn check_amount() {
			let total_amount: BalanceOf<T> = Self::voter_members()
				.iter()
				.fold(Zero::zero(), |acc: BalanceOf<T>, x| acc.saturating_add(x.1));
			let total_issuance = <pallet_balances::Module<T::Locks>>::total_issuance();
			let vote_balance_perbill = T::VoteBalancePerbill::get();
			let can_samsara_balance = vote_balance_perbill.mul_ceil(total_issuance);

			if total_amount > can_samsara_balance {
				// TODO: start samsara
			}
			// clear vote
			Self::clear_vote();
		}

		fn clear_vote() {
			// unlock balance
			for i in Self::voter_members() {
				T::Currency::remove_lock(SAMSARA_ID, &i.0);
			}

			if Self::voter_members().len() > 0 {
				VoterMembers::<T>::kill();
			}
			if Self::start_vote().is_some() {
				StartVote::<T>::kill();
			}

			Self::deposit_event(Event::ClearVote(
				frame_system::Module::<T>::block_number(),
			));
		}
	}

}
