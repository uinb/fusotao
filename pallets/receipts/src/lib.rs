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
use codec::{Codec, Decode, Encode};
use frame_support::{
    decl_error, decl_event, decl_module, decl_storage, ensure,
    traits::{BalanceStatus, Currency, LockableCurrency, ReservableCurrency},
    Parameter,
};
use frame_system::ensure_signed;
use fuso_support::{external_chain::*, traits::ReservableToken};
use sp_runtime::{
    traits::{
        AtLeast32BitUnsigned, CheckEqual, CheckedAdd, CheckedSub, MaybeDisplay, MaybeMallocSizeOf,
        MaybeSerializeDeserialize, Member, Saturating, SimpleBitOps, StaticLookup, Zero,
    },
    DispatchResult, RuntimeDebug,
};
use sp_std::{convert::TryInto, fmt::Debug, prelude::*, vec::Vec};

pub type BalanceOf<T> =
    <<T as Trait>::Currency as Currency<<T as frame_system::Trait>::AccountId>>::Balance;

pub type TokenBalanceOf<T> = <<T as Trait>::Token as ReservableToken<
    <T as Trait>::TokenId,
    <T as frame_system::Trait>::AccountId,
>>::Balance;

pub type PositiveImbalanceOf<T> =
    <<T as Trait>::Currency as Currency<<T as frame_system::Trait>::AccountId>>::PositiveImbalance;

pub type NegativeImbalanceOf<T> =
    <<T as Trait>::Currency as Currency<<T as frame_system::Trait>::AccountId>>::NegativeImbalance;

pub type HostingPair<T> = (
    <T as frame_system::Trait>::AccountId,
    <T as frame_system::Trait>::AccountId,
);

pub type UID = u128;

pub type Balance = u128;

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum Receipt<TokenId, BlockNumber> {
    Tao(Balance, ReceiptStatus<BlockNumber>),
    Token(TokenId, Balance, ReceiptStatus<BlockNumber>),
}

impl<X: Eq + PartialEq, Y> Receipt<X, Y> {
    fn is_active(&self) -> bool {
        match self {
            Self::Tao(_, status) => matches!(status, ReceiptStatus::Active),
            Self::Token(_, _, status) => matches!(status, ReceiptStatus::Active),
        }
    }
    fn is_revoking(&self) -> bool {
        match self {
            Self::Tao(_, status) => matches!(status, ReceiptStatus::Revoking(_)),
            Self::Token(_, _, status) => matches!(status, ReceiptStatus::Revoking(_)),
        }
    }

    fn get_value(&self) -> Balance {
        match self {
            Self::Tao(v, _) => *v,
            Self::Token(_, v, _) => *v,
        }
    }

    fn is_token_of(&self, token_id: &X) -> bool {
        match self {
            Self::Tao(_, _) => false,
            Self::Token(id, _, _) => id == token_id,
        }
    }

    fn is_same_assets(&self, other: &Self) -> bool {
        match self {
            Self::Tao(_, _) => matches!(other, Self::Tao(_, _)),
            Self::Token(id, _, _) => other.is_token_of(id),
        }
    }
}

impl<X: Eq + PartialEq, Y> Saturating for Receipt<X, Y> {
    fn saturating_add(self, other: Self) -> Self {
        let new = self.get_value().saturating_add(other.get_value());
        match self {
            Self::Tao(_, status) => Self::Tao(new, status),
            Self::Token(id, _, status) => Self::Token(id, new, status),
        }
    }

    fn saturating_sub(self, other: Self) -> Self {
        let new = self.get_value().saturating_sub(other.get_value());
        match self {
            Self::Tao(_, status) => Self::Tao(new, status),
            Self::Token(id, _, status) => Self::Token(id, new, status),
        }
    }

    fn saturating_mul(self, _: Self) -> Self {
        unimplemented!();
    }

    fn saturating_pow(self, _: usize) -> Self {
        unimplemented!();
    }
}

#[derive(PartialEq, Eq, Copy, Clone, Encode, Decode, RuntimeDebug)]
pub enum ReceiptStatus<BlockNumber> {
    Active,
    Revoking(BlockNumber),
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct Dominator {
    pub pledged: Balance,
    pub status: DominatorStatus,
}

#[derive(PartialEq, Eq, Copy, Clone, Encode, Decode, RuntimeDebug)]
pub enum DominatorStatus {
    Active,
    Closing,
    Banned,
}

pub trait Trait: frame_system::Trait {
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;

    type Currency: ReservableCurrency<Self::AccountId> + LockableCurrency<Self::AccountId>;

    type TokenId: Parameter
        + Member
        + MaybeSerializeDeserialize
        + MaybeDisplay
        + SimpleBitOps
        + Codec
        + Eq
        + Ord
        + Default
        + Copy;

    type Token: ReservableToken<Self::TokenId, Self::AccountId>;
}

decl_storage! {
    trait Store for Module<T: Trait> as Receipts {
        Receipts get(fn receipts): map
            hasher(blake2_128_concat) HostingPair<T>
        => Vec<Receipt<T::TokenId, T::BlockNumber>>;

        Dominators get(fn dominators): map
            hasher(blake2_128_concat) T::AccountId
        => Option<Dominator>;
    }
}

decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as frame_system::Trait>::AccountId,
        TokenId = <T as Trait>::TokenId,
        Balance = Balance,
    {
        DominatorClaimed(AccountId, Balance),
        TaoHosted(AccountId, AccountId, Balance, UID),
        TokenHosted(AccountId, AccountId, TokenId, Balance, UID),
        AssetsClear(
            AccountId,
            AccountId,
            Option<Balance>,
            Vec<(TokenId, Balance)>,
        ),
    }
);

decl_error! {
    pub enum Error for Module<T: Trait> {
        DominatorNotFound,
        IllegalParameters,
        ReceiptNotExists,
        ChainNotSupport,
        ReceiptAlreadyExists,
        DominatorAlreadyExists,
        DominatorBanned,
        PledgeUnsatisfied,
        DominatorClosing,
        InsufficientBalance,
        InsufficientStashAccount,
        InvalidStatus,
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        type Error = Error<T>;

        fn deposit_event() = default;

        #[weight = 1_000_000]
        pub fn claim_dominator(origin, #[compact] pledge: Balance) {
            let dominator = ensure_signed(origin)?;
            ensure!(!<Dominators<T>>::contains_key(&dominator), Error::<T>::DominatorAlreadyExists);
            let v: BalanceOf<T> = pledge.try_into().or(Err(Error::<T>::IllegalParameters))?;
            // TODO lock
            T::Currency::reserve(&dominator, v)?;
            <Dominators<T>>::insert(&dominator, Dominator {
                pledged: pledge,
                status: DominatorStatus::Active,
            });
            Self::deposit_event(RawEvent::DominatorClaimed(dominator, pledge));
        }

        #[weight = 100_000]
        pub fn grant_tao(origin,
                         dominator: <T::Lookup as StaticLookup>::Source,
                         amount: Balance,
                         memo: UID) {
            let fund_owner = ensure_signed(origin)?;
            let dominator = T::Lookup::lookup(dominator)?;
            let claimed = Dominators::<T>::get(&dominator).ok_or(Error::<T>::DominatorNotFound)?;
            ensure!(claimed.status == DominatorStatus::Active, Error::<T>::InvalidStatus);
            let value: BalanceOf<T> = amount.try_into().or(Err(Error::<T>::IllegalParameters))?;
            ensure!(T::Currency::can_reserve(&fund_owner, value), Error::<T>::InsufficientBalance);
            Self::try_mutate_receipt(&fund_owner,
                                     &dominator,
                                     |r| matches!(r, Receipt::Tao(_, _)),
                                     |exists| {
                                         let taken = exists.take().unwrap_or(Receipt::Tao(Zero::zero(), ReceiptStatus::Active));
                                         if !taken.is_active() {
                                             return Err(Error::<T>::InvalidStatus);
                                         }
                                         exists.replace(Receipt::Tao(taken.get_value().saturating_add(amount), ReceiptStatus::Active));
                                         Ok(())
                                     })?;
            T::Currency::reserve(&fund_owner, value)?;
            T::Currency::repatriate_reserved(&fund_owner, &dominator, value, BalanceStatus::Reserved)?;
            Self::deposit_event(RawEvent::TaoHosted(fund_owner, dominator, amount, memo));
        }

        #[weight = 100_000]
        pub fn grant_token(origin,
                           dominator: <T::Lookup as StaticLookup>::Source,
                           token: T::TokenId,
                           amount: Balance,
                           memo: UID) {
            let fund_owner = ensure_signed(origin)?;
            let dominator = T::Lookup::lookup(dominator)?;
            let claimed = Dominators::<T>::get(&dominator).ok_or(Error::<T>::DominatorNotFound)?;
            ensure!(claimed.status == DominatorStatus::Active, Error::<T>::InvalidStatus);
            let value: TokenBalanceOf<T> = amount.try_into().or(Err(Error::<T>::IllegalParameters))?;
            ensure!(T::Token::can_reserve(&token, &fund_owner, value), Error::<T>::InsufficientBalance);
            Self::try_mutate_receipt(&fund_owner,
                                     &dominator,
                                     |r| r.is_token_of(&token),
                                     |exists| {
                                         let taken = exists.take().unwrap_or(Receipt::Token(token, Zero::zero(), ReceiptStatus::Active));
                                         if !taken.is_active() {
                                             return Err(Error::<T>::InvalidStatus);
                                         }
                                         exists.replace(Receipt::Tao(taken.get_value().saturating_add(amount), ReceiptStatus::Active));
                                         Ok(())
                                     })?;
            T::Token::reserve(&token, &fund_owner, value)?;
            T::Token::repatriate_reserved(&token, &fund_owner, &dominator, value, BalanceStatus::Reserved)?;
            Self::deposit_event(RawEvent::TokenHosted(fund_owner, dominator, token, amount, memo));
        }

        #[weight = 100_000]
        pub fn sync(origin,
                    fund_owner: <T::Lookup as StaticLookup>::Source,
                    tao: Balance,
                    tokens: Vec<(T::TokenId, Balance)>,
                    digest: Vec<u8>) {
            let dominator = ensure_signed(origin)?;
            let fund_owner = T::Lookup::lookup(fund_owner)?;
            ensure!(Dominators::<T>::contains_key(&dominator), Error::<T>::DominatorNotFound);
            Self::try_mutate_receipt(&fund_owner,
                                     &dominator,
                                     |r| matches!(r, Receipt::Tao(_, _)),
                                     |exists_tao| {
                                         match exists_tao.take() {
                                             None => {},
                                             Some(Receipt::Tao(0, _)) => {},
                                             Some(Receipt::Token(_,_,_)) => {}
                                             Some(Receipt::Tao(_, s)) => {
                                                 exists_tao.replace(Receipt::Tao(tao, s));
                                             }
                                         }
                                         Ok(())
                                     })?;
            for (token, value) in tokens {
                Self::try_mutate_receipt(&fund_owner,
                                         &dominator,
                                         |r| r.is_token_of(&token),
                                         |exists_token| {
                                             match exists_token.take() {
                                                 None => {}
                                                 Some(Receipt::Token(_, 0, _)) => {}
                                                 Some(Receipt::Tao(_, _)) => {}
                                                 Some(Receipt::Token(t, _, s)) => {
                                                     exists_token.replace(Receipt::Token(t, value, s));
                                                 }
                                             }
                                             Ok(())
                                         })?;
            }
        }

        #[weight = 100_000]
        pub fn withdraw(origin, dominator: <T::Lookup as StaticLookup>::Source) {
            let fund_owner = ensure_signed(origin)?;
            let dominator = T::Lookup::lookup(dominator)?;
            ensure!(Dominators::<T>::contains_key(&dominator), Error::<T>::DominatorNotFound);
            let t = frame_system::Module::<T>::block_number();
            Self::try_mutate_receipt_exists(&fund_owner,
                                            &dominator,
                                            |r| matches!(r, Receipt::Tao(_, ReceiptStatus::Active)),
                                            |exists| {
                                                match exists.take() {
                                                    None => {}
                                                    Some(Receipt::Tao(0, _)) => {}
                                                    Some(Receipt::Token(_, _, _)) => {}
                                                    Some(Receipt::Tao(v, _)) => {
                                                        exists.replace(Receipt::Tao(v, ReceiptStatus::Revoking(t)));
                                                    }
                                                }
                                                Ok(())
                                            })?;
        }

        #[weight = 100_000]
        pub fn withdraw_token(origin, dominator: <T::Lookup as StaticLookup>::Source, token_id: T::TokenId) {
            let fund_owner = ensure_signed(origin)?;
            let dominator = T::Lookup::lookup(dominator)?;
            ensure!(Dominators::<T>::contains_key(&dominator), Error::<T>::DominatorNotFound);
            let t = frame_system::Module::<T>::block_number();
            Self::try_mutate_receipt_exists(&fund_owner,
                                            &dominator,
                                            |r| r.is_active() && r.is_token_of(&token_id),
                                            |exists| {
                                                match exists.take() {
                                                    None => {}
                                                    Some(Receipt::Token(_, 0, _)) => {}
                                                    Some(Receipt::Tao(_, _)) => {}
                                                    Some(Receipt::Token(id, v, _)) => {
                                                        exists.replace(Receipt::Token(id, v, ReceiptStatus::Revoking(t)));
                                                    }
                                                }
                                                Ok(())
                                            })?;
        }

        #[weight = 1000]
        pub fn confirm(origin, fund_owner: <T::Lookup as StaticLookup>::Source) {
            let dominator = ensure_signed(origin)?;

        }

        #[weight = 1000]
        pub fn quit_dominator(origin) {
            let dominator = ensure_signed(origin)?;
        }

        #[weight = 1000]
        pub fn slash(origin, dominator: <T::Lookup as StaticLookup>::Source) {
            // ensure_root(origin)?;
        }
    }
}

type ReceiptOf<T> = Receipt<<T as Trait>::TokenId, <T as frame_system::Trait>::BlockNumber>;

impl<T: Trait> Module<T> {
    fn try_mutate_receipt(
        owner: &T::AccountId,
        dominator: &T::AccountId,
        filter: impl Fn(&ReceiptOf<T>) -> bool,
        mutator: impl FnOnce(&mut Option<ReceiptOf<T>>) -> Result<(), Error<T>>,
    ) -> Result<(), Error<T>> {
        Receipts::<T>::mutate_exists((&owner, &dominator), |old| -> Result<(), Error<T>> {
            let mut vec = old.take().unwrap_or(vec![]);
            let mut target = match vec.iter().position(|r| filter(r)) {
                Some(i) => Some(vec.swap_remove(i)),
                None => None,
            };
            mutator(&mut target)?;
            match target {
                Some(v) => vec.push(v),
                None => {}
            }
            old.replace(vec);
            Ok(())
        })
    }

    fn try_mutate_receipt_exists(
        owner: &T::AccountId,
        dominator: &T::AccountId,
        filter: impl Fn(&ReceiptOf<T>) -> bool,
        mutator: impl FnOnce(&mut Option<ReceiptOf<T>>) -> Result<(), Error<T>>,
    ) -> Result<(), Error<T>> {
        Receipts::<T>::mutate_exists((&owner, &dominator), |old| -> Result<(), Error<T>> {
            let mut vec = old.take().unwrap_or(vec![]);
            let mut target = match vec.iter().position(|r| filter(r)) {
                Some(i) => Some(vec.swap_remove(i)),
                None => None,
            };
            if target.is_none() {
                return Err(Error::<T>::ReceiptNotExists);
            }
            mutator(&mut target)?;
            match target {
                Some(v) => vec.push(v),
                None => {}
            }
            old.replace(vec);
            Ok(())
        })
    }
}
