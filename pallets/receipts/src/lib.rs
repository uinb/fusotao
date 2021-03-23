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
    traits::{BalanceStatus, Currency, ReservableCurrency},
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
    Foreign(ExternalChain, Balance, ReceiptStatus<BlockNumber>),
}

impl<X: Eq + PartialEq, Y> Receipt<X, Y> {
    fn is_active(&self) -> bool {
        match self {
            Self::Tao(_, status) => matches!(status, ReceiptStatus::Active),
            Self::Token(_, _, status) => matches!(status, ReceiptStatus::Active),
            Self::Foreign(_, _, status) => matches!(status, ReceiptStatus::Active),
        }
    }

    fn is_revoking(&self) -> bool {
        match self {
            Self::Tao(_, status) => matches!(status, ReceiptStatus::Revoking(_)),
            Self::Token(_, _, status) => matches!(status, ReceiptStatus::Revoking(_)),
            Self::Foreign(_, _, status) => matches!(status, ReceiptStatus::Revoking(_)),
        }
    }

    fn get_value(&self) -> Balance {
        match self {
            Self::Tao(v, _) => *v,
            Self::Token(_, v, _) => *v,
            Self::Foreign(_, v, _) => *v,
        }
    }

    fn is_same_assets(&self, other: &Self) -> bool {
        match self {
            Self::Tao(_, _) => matches!(other, Self::Tao(_, _)),
            Self::Token(id0, _, _) => {
                if let Self::Token(id1, _, _) = other {
                    id0 == id1
                } else {
                    false
                }
            }
            Self::Foreign(chain0, _, _) => {
                if let Self::Foreign(chain1, _, _) = other {
                    chain0 == chain1
                } else {
                    false
                }
            }
        }
    }

    fn belongs_to(&self, pot: &CashPot<X>) -> bool {
        match self {
            Self::Tao(_, _) => matches!(pot, CashPot::Tao(_)),
            Self::Token(id0, _, _) => {
                if let CashPot::Token(id1, _) = pot {
                    id0 == id1
                } else {
                    false
                }
            }
            Self::Foreign(chain0, _, _) => {
                if let CashPot::Foreign(chain1, _) = pot {
                    chain0 == chain1
                } else {
                    false
                }
            }
        }
    }

    fn to_cash(self) -> CashPot<X> {
        match self {
            Self::Tao(v, _) => CashPot::Tao(v),
            Self::Token(id, v, _) => CashPot::Token(id, v),
            Self::Foreign(chain, v, _) => CashPot::Foreign(chain, v),
        }
    }
}

impl<X: Eq + PartialEq, Y> Saturating for Receipt<X, Y> {
    fn saturating_add(self, other: Self) -> Self {
        if self.is_revoking() || !self.is_same_assets(&other) {
            return self;
        }
        let new = self.get_value().saturating_add(other.get_value());
        match self {
            Self::Tao(_, status) => Self::Tao(new, status),
            Self::Token(id, _, status) => Self::Token(id, new, status),
            Self::Foreign(chain, _, status) => Self::Foreign(chain, new, status),
        }
    }

    fn saturating_sub(self, other: Self) -> Self {
        if self.is_revoking() || !self.is_same_assets(&other) {
            return self;
        }
        let new = self.get_value().saturating_sub(other.get_value());
        match self {
            Self::Tao(_, status) => Self::Tao(new, status),
            Self::Token(id, _, status) => Self::Token(id, new, status),
            Self::Foreign(chain, _, status) => Self::Foreign(chain, new, status),
        }
    }

    fn saturating_mul(self, _: Self) -> Self {
        unimplemented!();
    }

    fn saturating_pow(self, _: usize) -> Self {
        unimplemented!();
    }
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum CashPot<TokenId> {
    Tao(Balance),
    Token(TokenId, Balance),
    Foreign(ExternalChain, Balance),
}

impl<X: Eq + PartialEq> CashPot<X> {
    fn is_same_assets(&self, other: &Self) -> bool {
        match self {
            Self::Tao(_) => matches!(other, Self::Tao(_)),
            Self::Token(id0, _) => {
                if let Self::Token(id1, _) = other {
                    id0 == id1
                } else {
                    false
                }
            }
            Self::Foreign(chain0, _) => {
                if let Self::Foreign(chain1, _) = other {
                    chain0 == chain1
                } else {
                    false
                }
            }
        }
    }

    fn get_value(&self) -> Balance {
        match self {
            Self::Tao(v) => *v,
            Self::Token(_, v) => *v,
            Self::Foreign(_, v) => *v,
        }
    }

    fn append<Y>(self, receipt: &Receipt<X, Y>) -> Option<Self> {
        if !receipt.belongs_to(&self) || !receipt.is_active() {
            return None;
        }
        match self {
            Self::Tao(v) => Some(Self::Tao(v.saturating_add(receipt.get_value()))),
            Self::Token(id, v) => Some(Self::Token(id, v.saturating_add(receipt.get_value()))),
            Self::Foreign(chain, v) => {
                Some(Self::Foreign(chain, v.saturating_add(receipt.get_value())))
            }
        }
    }
}

impl<X: Eq + PartialEq> Saturating for CashPot<X> {
    fn saturating_add(self, other: Self) -> Self {
        if !self.is_same_assets(&other) {
            return self;
        }
        let new = self.get_value().saturating_add(other.get_value());
        match self {
            Self::Tao(_) => Self::Tao(new),
            Self::Token(id, _) => Self::Token(id, new),
            Self::Foreign(chain, _) => Self::Foreign(chain, new),
        }
    }

    fn saturating_sub(self, other: Self) -> Self {
        if !self.is_same_assets(&other) {
            return self;
        }
        let new = self.get_value().saturating_sub(other.get_value());
        match self {
            Self::Tao(_) => Self::Tao(new),
            Self::Token(id, _) => Self::Token(id, new),
            Self::Foreign(chain, _) => Self::Foreign(chain, new),
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
pub struct Dominator<TokenId> {
    pub pledged: Balance,
    pub pot: Vec<CashPot<TokenId>>,
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

    type Currency: ReservableCurrency<Self::AccountId>;

    type TokenId: Parameter
        + Member
        + MaybeSerializeDeserialize
        + MaybeDisplay
        + SimpleBitOps
        + Ord
        + Default
        + Copy
        + CheckEqual
        + MaybeMallocSizeOf;

    type Token: ReservableToken<Self::TokenId, Self::AccountId>;
}

decl_storage! {
    trait Store for Module<T: Trait> as Receipts {
        Receipts get(fn receipts): map
            hasher(blake2_128_concat) HostingPair<T>
        => Vec<Receipt<T::TokenId, T::BlockNumber>>;

        Dominators get(fn dominators): map
            hasher(blake2_128_concat) T::AccountId
        => Option<Dominator<T::TokenId>>;
    }
}

decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as frame_system::Trait>::AccountId,
        // TokenId = <T as Trait>::TokenId,
        Balance = Balance,
    {
        DominatorClaimed(AccountId, Balance),
        // TokenDominatorClaimed(TokenId, AccountId, TokenBalance),
        AssetsHosted(AccountId, AccountId, Balance),
        // TokenHosted(TokenId, AccountId, AccountId, TokenBalance),
        HostAssetsChanged(AccountId, AccountId, Balance),
        // HostTokenChanged(TokenId, AccountId, AccountId, TokenBalance),
        AssetsRevokeSubmitted(AccountId, AccountId),
        // TokenRevokeSubmitted(TokenId, AccountId, AccountId),
        AssetsRevokeConfirmed(AccountId, AccountId),
        // TokenRevokeConfirmed(TokenId, AccountId, AccountId),
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
            T::Currency::reserve(&dominator, v)?;
            <Dominators<T>>::insert(&dominator, Dominator {
                pledged: pledge,
                pot: vec![],
                status: DominatorStatus::Active,
            });
            Self::deposit_event(RawEvent::DominatorClaimed(dominator, pledge));
        }

        #[weight = 100_000]
        pub fn grant_tao(origin,
                         dominator: <T::Lookup as StaticLookup>::Source,
                         #[compact] amount: Balance,
                         memo: UID) {
            let fund_owner = ensure_signed(origin)?;
            let dominator = T::Lookup::lookup(dominator)?;
            let claimed = Dominators::<T>::get(&dominator).ok_or(Error::<T>::DominatorNotFound)?;
            ensure!(claimed.status == DominatorStatus::Active, Error::<T>::InvalidStatus);
            let value: BalanceOf<T> = amount.try_into().or(Err(Error::<T>::IllegalParameters))?;
            ensure!(T::Currency::can_reserve(&fund_owner, value), Error::<T>::InsufficientBalance);
            T::Currency::reserve(&fund_owner, value)?;
            Self::mutate_receipt(&fund_owner, &dominator, Receipt::Tao(amount, ReceiptStatus::Active))?;
            Self::deposit_event(RawEvent::AssetsHosted(fund_owner, dominator, amount));
        }

        #[weight = 100_000]
        pub fn grant_token(origin,
                           token: T::TokenId,
                           dominator: <T::Lookup as StaticLookup>::Source,
                           #[compact] amount: Balance,
                           memo: UID) {
            let fund_owner = ensure_signed(origin)?;
            let dominator = T::Lookup::lookup(dominator)?;
            let claimed = Dominators::<T>::get(&dominator).ok_or(Error::<T>::DominatorNotFound)?;
            ensure!(claimed.status == DominatorStatus::Active, Error::<T>::InvalidStatus);
            let value: TokenBalanceOf<T> = amount.try_into().or(Err(Error::<T>::IllegalParameters))?;
            ensure!(T::Token::can_reserve(&token, &fund_owner, value), Error::<T>::InsufficientBalance);
            T::Token::reserve(&token, &fund_owner, value)?;
            // Receipts::<T>::insert((&fund_owner, &dominator), Receipt::Token(token, amount, ReceiptStatus::Active));
            // Self::deposit_event(RawEvent::TokenHosted(token, fund_owner, dominator, amount));
        }

        #[weight = 100_000]
        pub fn revoke(origin, dominator: <T::Lookup as StaticLookup>::Source) {
            let fund_owner = ensure_signed(origin)?;
            let dominator = T::Lookup::lookup(dominator)?;
        }

        #[weight = 100]
        pub fn confirm_withdraw(origin, fund_owner: T::AccountId) {
            let dominator = ensure_signed(origin)?;

        }

        #[weight = 100]
        pub fn quit_dominator(origin) {
            let dominator = ensure_signed(origin)?;
        }

        #[weight = 100]
        pub fn slash(origin, dominator: <T::Lookup as StaticLookup>::Source) {
            // ensure_root(origin)?;
        }
    }
}

impl<T: Trait> Module<T> {
    fn mutate_receipt(
        owner: &T::AccountId,
        dominator: &T::AccountId,
        receipt: Receipt<T::TokenId, T::BlockNumber>,
    ) -> Result<(), Error<T>> {
        Receipts::<T>::try_mutate_exists((&owner, &dominator), |old| -> Result<(), Error<T>> {
            let mut vec = old.take().unwrap_or(vec![]);
            match vec.iter().position(|r| r.is_same_assets(&receipt)) {
                Some(i) => match vec[i].is_active() {
                    true => {
                        vec[i] = vec[i].clone().saturating_add(receipt.clone());
                        Dominators::<T>::mutate(&dominator, |d| {
                            let dominator = d.as_mut().unwrap();
                            match dominator.pot.iter().position(|p| receipt.belongs_to(p)) {
                                Some(i) => {
                                    dominator.pot[i] =
                                        dominator.pot[i].clone().append(&receipt).unwrap()
                                }
                                None => dominator.pot.push(receipt.to_cash()),
                            }
                        });
                        old.replace(vec);
                        Ok(())
                    }
                    false => Err(Error::<T>::InvalidStatus),
                },
                None => {
                    vec.push(receipt);
                    old.replace(vec);
                    Ok(())
                }
            }
        })
    }
}
