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


/*#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;
*/

use frame_support:: traits:: LockIdentifier;

pub const ELECTIONS_ID: LockIdentifier = *b"election";

pub use pallet::{
	Pledger, BalanceOf, VoterOf, Config,Event,Module
};

#[frame_support::pallet]
pub mod pallet {
	use codec::{Decode, Encode};
	use frame_support::traits::{Currency, Get, LockableCurrency, WithdrawReasons};
	use frame_support::{
		dispatch::DispatchResult, ensure, Parameter,
	};
	use frame_system::ensure_signed;
	use fuso_support::{collections::binary_heap::BinaryHeap, traits::Referendum};
	use sp_runtime::traits::{
		AtLeast32Bit, Bounded, CheckedAdd, CheckedSub, Member, One, Saturating, Zero,
	};
	use sp_runtime::RuntimeDebug;
	use sp_std::cmp::{Eq, Ord, Ordering, PartialEq, PartialOrd};
	use sp_std::vec::Vec;
	use frame_support::{ pallet_prelude::*};
	use frame_system::pallet_prelude::*;

	// pledger struct
	#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Encode, Decode, Default, RuntimeDebug)]
	pub struct Pledger<AccountId, BlockNumber, Balance> {
		pub account: AccountId,
		block_number: BlockNumber,
		pub amount: Balance,
	}

	// voter struct
	#[derive(Eq, Clone, Encode, Decode, Default, RuntimeDebug)]
	pub struct Voter<VoteIndex: Eq, AccountId: Eq, BlockNumber: Eq, Balance: Ord> {
		round: VoteIndex,
		pub account: AccountId,
		amount: Balance,
		pub pledger: Vec<Pledger<AccountId, BlockNumber, Balance>>,
	}

	impl<V: Eq, A: Eq, B: Eq, T: Ord> PartialOrd for Voter<V, A, B, T> {
		fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
			Some(self.amount.cmp(&other.amount))
		}
	}

	impl<V: Eq, A: Eq, B: Eq, T: Ord> Ord for Voter<V, A, B, T> {
		fn cmp(&self, other: &Self) -> Ordering {
			self.amount.cmp(&other.amount)
		}
	}

	impl<V: Eq, A: Eq, B: Eq, T: Ord> PartialEq for Voter<V, A, B, T> {
		fn eq(&self, other: &Self) -> bool {
			self.amount == other.amount
		}
	}

	pub type BalanceOf<T> = <T as pallet_balances::Config>::Balance;


	pub type VoterOf<T> = Voter<
		<T as Config>::VoteIndex,
		<T as frame_system::Config>::AccountId,
		<T as frame_system::Config>::BlockNumber,
		BalanceOf<T>,
	>;

	pub type MemberOf<T> = BinaryHeap<VoterOf<T>>;

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_balances::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		type Currency: LockableCurrency<Self::AccountId>;

		type VotePeriod: Get<Self::BlockNumber>;

		type MinimumVotingLock: Get<BalanceOf<Self>>;

		type VoteIndex: Parameter + Member + AtLeast32Bit + Bounded + Default + Copy;
	}


	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config>
	{

		StartProposal(<T as frame_system::Config>::BlockNumber, <T as frame_system::Config>::BlockNumber, T::VoteIndex),
		AddCandidate(<T as frame_system::Config>::AccountId, <T as frame_system::Config>::AccountId),
		Voted(<T as frame_system::Config>::AccountId, <T as frame_system::Config>::AccountId, BalanceOf<T>),
		StopProposal(T::VoteIndex),
	}

	#[pallet::error]
	pub enum Error<T>{
		NoProposalStarted,
		ProposalOver,
		AmountZero,
		AmountTooLow,
		InsufficientBalance,
		AlreadyIsVoter,
		AlreadyIsCandidate,
		NotCandidate,
		Overflow,
		NotCurrentVoteRound,
		InvalidCandidate,
	}



	#[pallet::storage]
	#[pallet::getter(fn candidates)]
	pub type Candidates<T: Config> = StorageValue<_, Vec<T::AccountId>,ValueQuery>;


	#[pallet::storage]
	#[pallet::getter(fn voter_members)]
	pub type VoterMembers<T: Config> = StorageValue<_, MemberOf<T>,ValueQuery>;


	#[pallet::storage]
	#[pallet::getter(fn vote_round_count)]
	pub type VoteRoundCount<T: Config> = StorageValue<_, T::VoteIndex,ValueQuery>;


	#[pallet::storage]
	#[pallet::getter(fn start_block_number)]
	pub type StartBlockNumber<T: Config> = StorageValue<_, Option<T::BlockNumber>,ValueQuery>;


	#[pallet::storage]
	#[pallet::getter(fn end_block_number)]
	pub type EndBlockNumber<T: Config> = StorageValue<_, Option<T::BlockNumber>,ValueQuery>;








	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {

	}



	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);


	#[pallet::call]
	impl<T:Config> Pallet<T> {


		#[pallet::weight( 1_000)]
		pub fn add_candidate(
			origin: OriginFor<T>,
			who: T::AccountId,
		) -> DispatchResultWithPostInfo {
			let sender = ensure_signed(origin)?;

			Self::is_proposal()?;

			// set who is candidate
			Self::set_candidate(&who)?;

			Self::deposit_event(Event::AddCandidate(sender, who));
			Ok(().into())
		}

		// vote
		#[pallet::weight( 1_000)]
		pub fn vote(origin: OriginFor<T>,
					voter: T::AccountId,
					vote_round: T::VoteIndex,
					/*#[compact]*/ amount: BalanceOf<T>) -> DispatchResultWithPostInfo {
			let sender = ensure_signed(origin)?;

			// judging whether it's a vote period
			Self::is_proposal()?;

			// check input vote round
			ensure!(vote_round == Self::vote_round_count(), Error::<T>::NotCurrentVoteRound);

			let voter_members = Self::voter_members();
			let voter_option = voter_members.iter().find(|v| v.account == voter);

			// if voter not included in members
			if voter_option.is_none() {
				// ensure voter include in candidates
				ensure!(Self::candidates().contains(&voter), Error::<T>::NotCandidate);
			}

			// check input amount
			ensure!(!amount.is_zero(), Error::<T>::AmountZero);

			// ensure input amount greater than or equal to MinimumVotingLock
			ensure!(amount >= T::MinimumVotingLock::get(), Error::<T>::AmountTooLow);

			// try vote
			Self::try_vote(&sender, &voter, amount)?;

			Self::deposit_event(Event::Voted(sender, voter, amount));

			Ok(().into())
		}
	}


	impl<T: Config> Module<T> {
		fn is_proposal() -> DispatchResult {
			ensure!(
            Self::start_block_number().is_some(),
            Error::<T>::NoProposalStarted
        	);

			let current_block_number = <frame_system::Module<T>>::block_number();
			let end_block_number = Self::end_block_number().unwrap();

			ensure!(
            current_block_number <= end_block_number,
            Error::<T>::ProposalOver
        	);
			Ok(())
		}

		// set candidate
		fn set_candidate(who: &T::AccountId) -> DispatchResult {
			// if is voter, notice already
			let voter_members = Self::voter_members();
			let voter_option = voter_members.iter().find(|v| &v.account == who);

			ensure!(voter_option.is_none(), Error::<T>::AlreadyIsVoter);

			ensure!(
            !Self::candidates().contains(&who),
            Error::<T>::AlreadyIsCandidate
        	);

			<Candidates<T>>::append(who);

			Ok(())
		}

		fn try_vote(
			sender: &T::AccountId,
			voter: &T::AccountId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let total_balance = pallet_balances::Module::<T>::total_balance(sender);
			// account all lock balance
			let lock_balance = pallet_balances::Module::<T>::locks(sender);
			let election_lock = lock_balance.iter().find(|b| b.id == crate::ELECTIONS_ID);

			if let Some(lock) = election_lock {
				// not lockable balance
				let balance = lock.amount;
				let free_balance = total_balance
					.checked_sub(&balance)
					.ok_or(Error::<T>::Overflow)?;

				// ensure free balance greater than input amount
				ensure!(free_balance > amount, Error::<T>::InsufficientBalance);
			}


			let voter_members = Self::voter_members();
			let voter_option = voter_members.iter().find(|v| &v.account == voter);

			if voter_option.is_some() {
				// update vote
				Self::update_vote(sender, voter, amount)?;
			} else {
				// insert vote
				Self::insert_vote(sender, voter, amount)?;
			}

			Self::lock_currency(sender, amount)?;

			Ok(())
		}

		fn insert_vote(
			sender: &T::AccountId,
			voter: &T::AccountId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			// remove candidate
			Candidates::<T>::try_mutate(|candidates| -> DispatchResult {
				let index = candidates
					.iter()
					.position(|a| a == voter)
					.ok_or(Error::<T>::InvalidCandidate)?;
				candidates.remove(index);
				Ok(())
			})?;

			// current pledger
			let pledger = Pledger {
				account: sender.clone(),
				block_number: <frame_system::Module<T>>::block_number(),
				amount: amount.clone(),
			};

			// new vec
			let mut pledger_vec: Vec<Pledger<T::AccountId, T::BlockNumber, BalanceOf<T>>> = Vec::new();
			// push pledger
			pledger_vec.push(pledger);

			let voter_member = Voter {
				account: voter.clone(),
				round: Self::vote_round_count(),
				amount: amount.clone(),
				pledger: pledger_vec,
			};

			VoterMembers::<T>::try_mutate(|voters| -> DispatchResult {
				voters.push(voter_member);
				Ok(())
			})?;

			Ok(())
		}

		fn update_vote(
			sender: &T::AccountId,
			voter: &T::AccountId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			// update voter members
			let voter_members = Self::voter_members();
			let mut vec = voter_members.into_vec();
			// iter voter members, and removed elements
			for i in vec.iter_mut() {
				let mut data = i;
				if &data.account == voter {
					// current voter add amount
					let total_amount = data
						.amount
						.checked_add(&amount)
						.ok_or(Error::<T>::Overflow)?;
					data.amount = total_amount;

					// current sender has not pledge
					let mut has_pledger = false;

					for j in data.pledger.iter_mut() {
						if &j.account == sender {
							// current sender has pledger
							has_pledger = true;

							// current sender add to total amount
							let pledger_total_amount =
								j.amount.checked_add(&amount).ok_or(Error::<T>::Overflow)?;

							// update the latest block
							j.block_number = <frame_system::Module<T>>::block_number();

							// set current pledger total amount
							j.amount = pledger_total_amount;
						}
					}

					if !has_pledger {
						data.pledger.push(Pledger {
							account: sender.clone(),
							block_number: <frame_system::Module<T>>::block_number(),
							amount: amount,
						});
					}
				}
			}

			// update storage from voter members
			<VoterMembers<T>>::put(BinaryHeap::from(vec));

			Ok(())
		}

		// lock currency
		fn lock_currency(account: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
			let lock_balance = pallet_balances::Module::<T>::locks(account);
			let vote_lock = lock_balance.iter().find(|b| b.id == crate::ELECTIONS_ID);
			if let Some(lock) = vote_lock {
				let balance = lock.amount;
				let total_lock_balance = amount.checked_add(&balance).ok_or(Error::<T>::Overflow)?;
				pallet_balances::Module::<T>::extend_lock(
					crate::ELECTIONS_ID,
					account,
					total_lock_balance,
					WithdrawReasons::all(),
				);
			} else {
				pallet_balances::Module::<T>::set_lock(
					crate::ELECTIONS_ID,
					account,
					amount,
					WithdrawReasons::all(),
				);
			}
			Ok(())
		}

		pub fn start_proposal(start: T::BlockNumber) -> T::VoteIndex {
			let candidate_period = T::VotePeriod::get();
			let end_block = start.saturating_add(candidate_period);

			// initialize
			<Candidates<T>>::kill();
			<VoterMembers<T>>::kill();



			// set start block number
			<StartBlockNumber<T>>::put(Option::from(start));

			// set end block number
			<EndBlockNumber<T>>::put(Option::from(end_block));

			<VoteRoundCount<T>>::put(Self::vote_round_count() + One::one());

			let count = Self::vote_round_count();
			Self::deposit_event(Event::StartProposal(start, end_block, count));

			count
		}

	}

	impl<T: Config> Referendum<T::BlockNumber, T::VoteIndex, VoterOf<T>> for Module<T> {
		fn proposal(start: T::BlockNumber) -> T::VoteIndex {
			Self::start_proposal(start)
		}

		fn get_round() -> T::VoteIndex {
			Self::vote_round_count()
		}

		fn get_result(index: T::VoteIndex) -> Option<Vec<VoterOf<T>>> {
			if Self::vote_round_count() == index {
				let heap = Self::voter_members();
				let mut vec = heap.into_sorted_vec();
				vec.reverse();
				return Some(vec);
			} else {
				None
			}
		}
	}


}
