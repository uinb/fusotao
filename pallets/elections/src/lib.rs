#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "256"]
use codec::{Codec, Encode, Decode};
use frame_support::{Hashable, traits::{Get, LockableCurrency, LockIdentifier, WithdrawReasons}};
use frame_support::{decl_error, decl_event, decl_module, decl_storage, ensure, Parameter, dispatch::DispatchResult, debug};
use frame_support::weights::{Weight};
use frame_system::{ensure_signed};
use fuso_support::traits::Referendum;
use sp_runtime::{RuntimeDebug, print};
use sp_runtime::traits::{
    AtLeast32Bit, AtLeast32BitUnsigned, Bounded, CheckedAdd,
    MaybeSerializeDeserialize, Member, Zero, One
};
use sp_std::prelude::*;
use sp_std::{fmt::Debug, vec::Vec, convert::TryInto};

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[derive(PartialEq, Eq, Copy, Clone, Encode, Decode, Default, RuntimeDebug)]
pub struct Pledger<AccountId, BlockNumber, Balance> {
	account_id: AccountId,
	block_number: BlockNumber,
	amount: Balance
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, Default, RuntimeDebug)]
pub struct VoterData<VoteIndex, AccountId, BlockNumber, Balance> {
	round: VoteIndex,
	amount: Balance,
	pledger: Vec<Pledger<AccountId, BlockNumber, Balance>>
}

pub type BalanceOf<T> = <<T as Trait>::Locks as pallet_balances::Trait>::Balance;

pub type AccountIdOf<T> = <<T as Trait>::Locks as frame_system::Trait>::AccountId;

pub trait Trait: frame_system::Trait {
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;

	/// Minimum about that can be used as the locked value for voting.
	type MinimumVotingLock: Get<BalanceOf<Self>>;

	type VotingPeriod: Get<Self::BlockNumber>;

	type VoteIndex: Parameter + Member + AtLeast32Bit + Bounded + Default + Copy;

	type MaxMembers: Get<u32>;

    type Balance: Member
		+ Parameter
		+ AtLeast32BitUnsigned
		+ Default
		+ Copy
		+ Codec
		+ Debug
		+ MaybeSerializeDeserialize;

	type Locks: pallet_balances::Trait;
}

decl_event! {
    pub enum Event<T>
    where
        AccountId = <T as frame_system::Trait>::AccountId,
		Balance = BalanceOf<T>,
		VoteIndex = <T as Trait>::VoteIndex
    {
		StartVote(AccountId, VoteIndex),
		Voted(AccountId, AccountId, Balance),
		VoteFinalized(VoteIndex),
    }
}

decl_error! {
    pub enum Error for Module<T: Trait> {
		NoVotingStarted,
		VoteIsOver,
		AmountZero,
		AmountTooLow,
		InsufficientBalance,
		AlreadyIsVoter,
		AlreadyIsCandidate,
		NotCandidate,
		Overflow,
		NotCurrentVoteRound,
    }
}

decl_storage! {
    trait Store for Module<T: Trait> as Votes {
		/// The present candidate list.
		Candidates get(fn candidates): Vec<T::AccountId>; // has holes

		VoteMember get(fn vote_member): Vec<(T::AccountId, T::BlockNumber, BalanceOf<T>)>;

		VoterInfoData get(fn voter_info):
			map hasher(twox_64_concat) T::AccountId => Option<VoterData<T::VoteIndex, T::AccountId, T::BlockNumber, BalanceOf<T>>>;

		VoteRoundCount get(fn vote_round_count): T::VoteIndex;

		StartBlockNumber get(fn start_block_number): Option<T::BlockNumber>;

		Leaderboard get(fn leaderboard): Option<Vec<(T::AccountId, BalanceOf<T>)>>;
	}
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        type Error = Error<T>;

        fn deposit_event() = default;

		/// How often (in blocks) to check for new votes.
		const VotingPeriod: T::BlockNumber = T::VotingPeriod::get();

		/// The minimum amount to be used as a deposit for a public referendum proposal.
		const MinimumVotingLock: BalanceOf<T> = T::MinimumVotingLock::get();

		#[weight = 1_000_000_000]
		fn start_vote(origin) {
			let sender = ensure_signed(origin)?;

			Self::start_vote_tally(sender);
		}

		#[weight = 1_000_000_000]
		fn add_candidate(
			origin,
			who: T::AccountId,
		) -> DispatchResult {
			let _sender = ensure_signed(origin)?;
			Self::set_candidate(who)
		}

		// vote
        #[weight = 1_000_000_000]
		fn vote(origin,
			voter: T::AccountId,
			#[compact] vote_round: T::VoteIndex,
			#[compact] amount: BalanceOf<T>) -> DispatchResult {
			let sender = ensure_signed(origin)?;

			ensure!(Self::start_block_number().is_some(), Error::<T>::NoVotingStarted);

			ensure!(!Self::is_end_vote(), Error::<T>::VoteIsOver);

			ensure!(vote_round == Self::vote_round_count(), Error::<T>::NotCurrentVoteRound);

			ensure!(Self::candidates().contains(&voter), Error::<T>::NotCandidate);

			ensure!(!amount.is_zero(), Error::<T>::AmountZero);

			ensure!(amount >= T::MinimumVotingLock::get(), Error::<T>::AmountTooLow);

			let from: Vec<u8> = sender.encode();

			let account = <<T as Trait>::Locks as frame_system::Trait>::AccountId::decode(&mut from.as_ref()).unwrap_or_default();

			// let locked_balance = pallet_balances::Locks::<T::Locks>::get(&account);

			Self::try_vote(&sender, &account, &voter, amount)?;

			Self::deposit_event(RawEvent::Voted(sender, voter, amount));

			Ok(())
		}

		fn on_initialize(n: T::BlockNumber) -> Weight {
			if let Err(e) = Self::init_block(n) {
				debug::info!("Guru meditation");
				print(e);
			}
			0
		}
    }
}

impl<T: Trait> Module<T> {
	fn is_end_vote() -> bool {
		let start_block_number = Self::start_block_number().unwrap();
		let current_block_number = <frame_system::Module<T>>::block_number();
		let diff_block_number = current_block_number - start_block_number;
		if diff_block_number > T::VotingPeriod::get() {
			return true
		}
		false
	}

	// set candidate
	fn set_candidate(
		who: T::AccountId,
	) -> DispatchResult {
		ensure!(Self::start_block_number().is_some(), Error::<T>::NoVotingStarted);

		ensure!(!Self::is_end_vote(), Error::<T>::VoteIsOver);

		ensure!(!VoterInfoData::<T>::contains_key(&who), Error::<T>::AlreadyIsVoter);

		ensure!(!Self::candidates().contains(&who), Error::<T>::AlreadyIsCandidate);

		<Candidates<T>>::append(who);

		Ok(())
	}

	fn try_vote(sender: &T::AccountId, account: &AccountIdOf<T>, voter: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
		let usable_balance: BalanceOf<T> = <pallet_balances::Module<T::Locks>>::usable_balance(account);

		ensure!(usable_balance > amount, Error::<T>::InsufficientBalance);

		// check has voted
		if VoterInfoData::<T>::contains_key(voter) {
			// edit vote
			Self::edit_vote(sender, account, voter, amount);
		} else {
			// insert vote
			Self::insert_vote(sender, account, voter, amount);
		}

		Ok(())
	}

	fn insert_vote(sender: &T::AccountId, account: &AccountIdOf<T>, voter: &T::AccountId, amount: BalanceOf<T>) {
		// current pledger
		let pledger = Pledger {
			account_id: sender.clone(),
			block_number: <frame_system::Module<T>>::block_number(),
			amount: amount.clone()
		};
		// new vec
		let mut v: Vec<Pledger<T::AccountId, T::BlockNumber, BalanceOf<T>>> = Vec::new();
		// push pledger
		v.push(pledger);
		// insert voter
		VoterInfoData::<T>::insert(voter, VoterData {
			round: Self::vote_round_count(),
			amount: amount.clone(),
			pledger: v
		});

		let voter_data = Self::voter_info(&voter);

		let mut members = Self::vote_member();
		let acc = voter.clone();
		members.push((acc, <frame_system::Module<T>>::block_number(), amount.clone()));
		VoteMember::<T>::put(members);
		Self::lock_currency(sender, account, amount, false);
	}

	fn edit_vote(sender: &T::AccountId, account: &AccountIdOf<T>, voter: &T::AccountId, amount: BalanceOf<T>) {
		// voter info
		let info = Self::voter_info(voter).unwrap();
		// current voter add amount
		let new_amount = info.amount.checked_add(&amount).ok_or(Error::<T>::Overflow);
		let edit_amount = new_amount.unwrap();
		let edit_status = <VoterInfoData<T>>::try_mutate_exists(voter, |voter_data| -> DispatchResult {
			let mut acc = voter_data.as_mut().unwrap();
			acc.amount = edit_amount.clone();

			// current sender has not pledge
			let mut has_pledger = false;

			for i in acc.pledger.iter_mut() {
				if &i.account_id == sender {
					// current sender add to total amount
					let total_amount = i.amount.checked_add(&amount).ok_or(Error::<T>::Overflow);
					has_pledger = true;
					i.block_number = <frame_system::Module<T>>::block_number();
					i.amount = total_amount?;
				}
			}

			if !has_pledger {
				acc.pledger.push(Pledger {
					account_id: sender.clone(),
					block_number: <frame_system::Module<T>>::block_number(),
					amount: amount
				});
			}
			Ok(())
		});

		if edit_status.is_ok() {
			// edit current voter members block number
			let mut members = Self::vote_member();
			for i in members.iter_mut() {
				if &i.0 == voter {
					i.1 = <frame_system::Module<T>>::block_number();
					i.2 = edit_amount.clone();
				}
			}
			VoteMember::<T>::put(members);
			Self::lock_currency(sender, account,  edit_amount, true);
		}
	}

	// lock currency
	fn lock_currency(sender: &T::AccountId, account: &AccountIdOf<T>, amount: BalanceOf<T>, is_edit: bool) {
		let identity = sender.identity();

		// LockId
		let id: LockIdentifier = identity.try_into().unwrap_or_default();

		if is_edit {
			<pallet_balances::Module<T::Locks>>::extend_lock(id, account, amount, WithdrawReasons::all());
		} else {
			<pallet_balances::Module<T::Locks>>::set_lock(id, account, amount, WithdrawReasons::all());
		}
	}

	fn sort_leaderboard() {
		// sort by amount
		let members = Self::vote_member();
		if members.len() > 0 {
			let mut mem = members.clone();
			mem.sort_by(|a, b| {
				b.2.cmp(&a.2)
			});
			let data: Vec<(T::AccountId, BalanceOf<T>)> = mem.iter().map(|m| {
				(m.0.clone(), m.2)
			}).collect();

			// set leaderboard
			<Leaderboard<T>>::put(data);
		}

		Self::deposit_event(RawEvent::VoteFinalized(Self::vote_round_count()));
	}

	fn init_block(block_number: T::BlockNumber) -> DispatchResult {
		// vote over
		if let Some(number) = Self::start_block_number() {
			let stop_vote_number = Self::stop_vote_from(number);
			if block_number == stop_vote_number {
				Self::sort_leaderboard();
			}
		}
		Ok(())
	}

	// stop vote pre block number
	fn stop_vote_from(n: T::BlockNumber) -> T::BlockNumber {
		let voting_period = T::VotingPeriod::get();
		n + voting_period - One::one()
	}

	pub fn start_vote_tally(sender: T::AccountId) {
		// initialize
		<Candidates<T>>::kill();
		<VoteMember<T>>::kill();
		<VoterInfoData<T>>::remove_all();
		<Leaderboard<T>>::kill();

		// set start block number
		<StartBlockNumber<T>>::put(<frame_system::Module<T>>::block_number());

		<VoteRoundCount<T>>::put(Self::vote_round_count() + (1 as u32).into());

		let count = Self::vote_round_count();
		Self::deposit_event(RawEvent::StartVote(sender, count));
	}
}

impl<T: Trait> Referendum<T::BlockNumber, T::VoteIndex> for Module<T> {
	type Result = Option<Vec<(T::AccountId, BalanceOf<T>)>>;

	fn proposal(start: T::BlockNumber, end: T::BlockNumber) -> Option<T::VoteIndex> {
		let start_block_number = Self::start_block_number();
		let stop_vote_from = Self::stop_vote_from(start);
		if start_block_number.is_some() && start > start_block_number.unwrap() && stop_vote_from > end {
			return Some(Self::vote_round_count());
		}
		None
	}

	fn is_end(index: T::VoteIndex) -> bool {
		if Self::vote_round_count() > index {
			return true;
		}
		false
	}

	fn get_result(index: T::VoteIndex) -> Self::Result {
		if Self::vote_round_count() == index && Self::leaderboard().is_some() {
			return Self::leaderboard();
		} else {
			None
		}
	}
}
