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
use codec::{Decode, Encode};
use frame_support::{
    decl_error, decl_event, decl_module, decl_storage, dispatch::DispatchResult, ensure, Parameter,
};
use frame_support::{
    traits::{Get, LockIdentifier, LockableCurrency, WithdrawReasons},
    Hashable,
};
use frame_system::ensure_signed;
use fuso_support::{collections::binary_heap::BinaryHeap, traits::Referendum};
use sp_runtime::traits::{AtLeast32Bit, Bounded, CheckedAdd, Member, Zero};
use sp_runtime::RuntimeDebug;
use sp_std::cmp::{Eq, Ord, Ordering, PartialEq, PartialOrd};
use sp_std::{convert::TryInto, vec::Vec};

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

// pledger struct
#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Encode, Decode, Default, RuntimeDebug)]
pub struct Pledger<AccountId, BlockNumber, Balance> {
    account: AccountId,
    block_number: BlockNumber,
    amount: Balance,
}

// voter struct
#[derive(Eq, Clone, Encode, Decode, Default, RuntimeDebug)]
pub struct Voter<VoteIndex: Eq, AccountId: Eq, BlockNumber: Eq, Balance: Ord> {
    round: VoteIndex,
    account: AccountId,
    amount: Balance,
    pledger: Vec<Pledger<AccountId, BlockNumber, Balance>>,
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

pub type BalanceOf<T> = <<T as Trait>::Locks as pallet_balances::Trait>::Balance;

pub type AccountIdOf<T> = <<T as Trait>::Locks as frame_system::Trait>::AccountId;

pub trait Trait: frame_system::Trait {
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;

    type CandidatePeriod: Get<Self::BlockNumber>;

    type MinimumVotingLock: Get<BalanceOf<Self>>;

    type VoteIndex: Parameter + Member + AtLeast32Bit + Bounded + Default + Copy;

    type Locks: pallet_balances::Trait;
}

decl_event! {
    pub enum Event<T>
    where
        AccountId = <T as frame_system::Trait>::AccountId,
        Balance = BalanceOf<T>,
        VoteIndex = <T as Trait>::VoteIndex,
        BlockNumber = <T as frame_system::Trait>::BlockNumber,
    {
        StartProposal(BlockNumber, BlockNumber, VoteIndex),
        AddCandidate(AccountId, AccountId),
        Voted(AccountId, AccountId, Balance),
        StopProposal(VoteIndex),
    }
}

decl_error! {
    pub enum Error for Module<T: Trait> {
        NoProposalStarted,
        ProposalOver,
        VoteIsOver,
        AmountZero,
        AmountTooLow,
        InsufficientBalance,
        AlreadyIsVoter,
        AlreadyIsCandidate,
        NotCandidate,
        Overflow,
        NotCurrentVoteRound,
        NotCandidatePeriod,
        CandidatePeriodExpired,
    }
}

decl_storage! {
    trait Store for Module<T: Trait> as Votes {
        /// The present candidate list.
        Candidates get(fn candidates): Vec<T::AccountId>;

        VoterMembers get(fn voter_members): Option<BinaryHeap<Voter<T::VoteIndex, T::AccountId, T::BlockNumber, BalanceOf<T>>>>;

        VoteRoundCount get(fn vote_round_count): T::VoteIndex;

        StartBlockNumber get(fn start_block_number): Option<T::BlockNumber>;

        EndBlockNumber get(fn end_block_number): Option<T::BlockNumber>;
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        type Error = Error<T>;

        fn deposit_event() = default;

        const CandidatePeriod: T::BlockNumber = T::CandidatePeriod::get();

        /// The minimum amount to be used as a deposit for a public referendum proposal.
        const MinimumVotingLock: BalanceOf<T> = T::MinimumVotingLock::get();

        #[weight = 1_000_000_000]
        fn init_proposal(origin, start: T::BlockNumber, end: T::BlockNumber) -> DispatchResult {
            ensure_signed(origin)?;
            Self::start_proposal(start, end);
            Ok(())
        }

        #[weight = 1_000_000_000]
        fn add_candidate(
            origin,
            who: T::AccountId,
        ) -> DispatchResult {
            let sender = ensure_signed(origin)?;

            // judging whether it's a candidate period
            Self::is_candidate()?;

            // set who is candidate
            Self::set_candidate(&who)?;

            Self::deposit_event(RawEvent::AddCandidate(sender, who));
            Ok(())
        }

        // vote
        #[weight = 1_000_000_000]
        fn vote(origin,
            voter: T::AccountId,
            #[compact] vote_round: T::VoteIndex,
            #[compact] amount: BalanceOf<T>) -> DispatchResult {
            let sender = ensure_signed(origin)?;

            // judging whether it's a vote period
            Self::is_proposal()?;

            // check input vote round
            ensure!(vote_round == Self::vote_round_count(), Error::<T>::NotCurrentVoteRound);

            if let Some(voter_members) = Self::voter_members() {
                let voter_option = voter_members.iter().find(|v| v.account == voter);

                // if voter not included in members
                if voter_option.is_none() {
                    // ensure voter include in candidates
                    ensure!(Self::candidates().contains(&voter), Error::<T>::NotCandidate);
                }
            } else {
                // if members is empty, ensure voter include in candidates
                ensure!(Self::candidates().contains(&voter), Error::<T>::NotCandidate);
            }

            // check input amount
            ensure!(!amount.is_zero(), Error::<T>::AmountZero);

            // ensure input amount greater than or equal to MinimumVotingLock
            ensure!(amount >= T::MinimumVotingLock::get(), Error::<T>::AmountTooLow);

            // sender encode
            let from: Vec<u8> = sender.encode();

            let account = <<T as Trait>::Locks as frame_system::Trait>::AccountId::decode(&mut from.as_ref()).unwrap_or_default();

            // try vote
            Self::try_vote(&sender, &account, &voter, amount)?;

            Self::deposit_event(RawEvent::Voted(sender, voter, amount));

            Ok(())
        }
    }
}

impl<T: Trait> Module<T> {
    fn is_candidate() -> DispatchResult {
        // ensure start proposal
        ensure!(
            Self::start_block_number().is_some(),
            Error::<T>::NoProposalStarted
        );

        let start_block_number = Self::start_block_number().unwrap();
        let current_block_number = <frame_system::Module<T>>::block_number();

        ensure!(
            current_block_number < start_block_number,
            Error::<T>::CandidatePeriodExpired
        );

        // if start block number less than candidate period, start candidate will be zero
        let mut start_candidate = Zero::zero();
        if start_block_number > T::CandidatePeriod::get() {
            start_candidate = start_block_number - T::CandidatePeriod::get();
        }

        ensure!(
            current_block_number >= start_candidate,
            Error::<T>::NotCandidatePeriod
        );
        Ok(())
    }

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
        if let Some(voter_members) = Self::voter_members() {
            let voter_option = voter_members.iter().find(|v| &v.account == who);

            ensure!(voter_option.is_none(), Error::<T>::AlreadyIsVoter);
        }

        ensure!(
            !Self::candidates().contains(&who),
            Error::<T>::AlreadyIsCandidate
        );

        <Candidates<T>>::append(who);

        Ok(())
    }

    fn try_vote(
        sender: &T::AccountId,
        account: &AccountIdOf<T>,
        voter: &T::AccountId,
        amount: BalanceOf<T>,
    ) -> DispatchResult {
        let usable_balance: BalanceOf<T> =
            <pallet_balances::Module<T::Locks>>::usable_balance(account);

        // ensure usable balance greater than input amount
        ensure!(usable_balance > amount, Error::<T>::InsufficientBalance);

        if let Some(voter_members) = Self::voter_members() {
            let voter_option = voter_members.iter().find(|v| &v.account == voter);

            if voter_option.is_some() {
                // edit vote
                Self::edit_vote(sender, account, voter, amount);
            } else {
                // insert vote
                Self::insert_vote(sender, account, voter, amount);
            }
        } else {
            // insert vote
            Self::insert_vote(sender, account, voter, amount);
        }

        Ok(())
    }

    fn insert_vote(
        sender: &T::AccountId,
        account: &AccountIdOf<T>,
        voter: &T::AccountId,
        amount: BalanceOf<T>,
    ) {
        // remove candidate
        let candidates = Self::candidates();
        let new_candidates: Vec<_> = candidates
            .iter()
            .filter(|a| a != &voter)
            .map(|b| b)
            .collect();
        <Candidates<T>>::put(new_candidates);

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

        if let Some(voter_members) = Self::voter_members() {
            let mut new_voter_members = voter_members;
            new_voter_members.push(voter_member);

            // push voter
            <VoterMembers<T>>::put(new_voter_members);
        } else {
            let mut new_voter_members: BinaryHeap<
                Voter<T::VoteIndex, T::AccountId, T::BlockNumber, BalanceOf<T>>,
            > = BinaryHeap::new();
            new_voter_members.push(voter_member);

            // insert voter
            <VoterMembers<T>>::put(new_voter_members);
        }

        Self::lock_currency(sender, account, amount, false);
    }

    fn edit_vote(
        sender: &T::AccountId,
        account: &AccountIdOf<T>,
        voter: &T::AccountId,
        amount: BalanceOf<T>,
    ) {
        // voter info
        let mut voter_members = Self::voter_members().unwrap();
        let mut new_voter_members: BinaryHeap<
            Voter<T::VoteIndex, T::AccountId, T::BlockNumber, BalanceOf<T>>,
        > = BinaryHeap::new();

        // default total amount is zero
        let mut total_amount = Zero::zero();

        // iter voter members, and removed elements
        for i in voter_members.drain() {
            let mut data = i;
            if &data.account == voter {
                // current voter add amount
                total_amount = data
                    .amount
                    .checked_add(&amount)
                    .ok_or(Error::<T>::Overflow)
                    .unwrap();
                data.amount = total_amount.clone();

                // current sender has not pledge
                let mut has_pledger = false;

                for j in data.pledger.iter_mut() {
                    if &j.account == sender {
                        // current sender has pledger
                        has_pledger = true;

                        // current sender add to total amount
                        let pledger_total_amount =
                            j.amount.checked_add(&amount).ok_or(Error::<T>::Overflow);

                        // update the latest block
                        j.block_number = <frame_system::Module<T>>::block_number();

                        // set current pledger total amount
                        j.amount = pledger_total_amount.unwrap();
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
            // push new voter members
            new_voter_members.push(data);
        }

        // update storage from voter members
        <VoterMembers<T>>::put(new_voter_members);

        Self::lock_currency(sender, account, total_amount, true);
    }

    // lock currency
    fn lock_currency(
        sender: &T::AccountId,
        account: &AccountIdOf<T>,
        amount: BalanceOf<T>,
        is_edit: bool,
    ) {
        let identity = sender.identity();

        // LockId
        let id: LockIdentifier = identity.try_into().unwrap_or_default();

        if is_edit {
            <pallet_balances::Module<T::Locks>>::extend_lock(
                id,
                account,
                amount,
                WithdrawReasons::all(),
            );
        } else {
            <pallet_balances::Module<T::Locks>>::set_lock(
                id,
                account,
                amount,
                WithdrawReasons::all(),
            );
        }
    }

    pub fn start_proposal(start: T::BlockNumber, end: T::BlockNumber) -> T::VoteIndex {
        // initialize
        <Candidates<T>>::kill();
        <VoterMembers<T>>::kill();

        // set start block number
        <StartBlockNumber<T>>::put(start);

        // set end block number
        <EndBlockNumber<T>>::put(end);

        <VoteRoundCount<T>>::put(Self::vote_round_count() + (1 as u32).into());

        let count = Self::vote_round_count();
        Self::deposit_event(RawEvent::StartProposal(start, end, count));

        count
    }
}

impl<T: Trait> Referendum<T::BlockNumber, T::VoteIndex> for Module<T> {
    type Result =
        Option<BinaryHeap<Voter<T::VoteIndex, T::AccountId, T::BlockNumber, BalanceOf<T>>>>;

    fn proposal(start: T::BlockNumber, end: T::BlockNumber) -> T::VoteIndex {
        Self::start_proposal(start, end)
    }

    fn is_end(index: T::VoteIndex) -> bool {
        Self::vote_round_count() > index
    }

    fn get_result(index: T::VoteIndex) -> Self::Result {
        if Self::vote_round_count() == index {
            return Self::voter_members();
        } else {
            None
        }
    }
}
