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
    decl_error, decl_event, decl_module, decl_storage, ensure,
    traits::{BalanceStatus, Currency, ReservableCurrency},
    Parameter,
};
use frame_system::ensure_signed;
use fuso_support::{external_chain::*, traits::ReservableToken};
use sp_runtime::{
    traits::{
        CheckEqual, CheckedAdd, CheckedSub, MaybeDisplay, MaybeMallocSizeOf,
        MaybeSerializeDeserialize, Member, Saturating, SimpleBitOps, StaticLookup, Zero,
    },
    RuntimeDebug,
};
use sp_std::prelude::*;

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

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct Receipt<TokenId, Balance> {
    pub value: Balance,
    pub assets_type: Option<TokenId>,
    pub status: ReceiptStatus,
}

#[derive(PartialEq, Eq, Copy, Clone, Encode, Decode, RuntimeDebug)]
pub enum ReceiptStatus {
    Active,
    Revoking,
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct Dominator<TokenId, Balance> {
    pub assets_type: Option<TokenId>,
    pub total_pledged: Balance,
    pub total_hosted: Balance,
    pub support_chains: Vec<ExternalChain>,
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

pub type UID = u128;

decl_storage! {
    trait Store for Module<T: Trait> as Receipts {
        Receipts get(fn receipts): map
            hasher(blake2_128_concat) HostingPair<T>
        => Option<Receipt<(), BalanceOf<T>>>;

        TokenReceipts get(fn token_receipts): double_map
            hasher(blake2_128_concat) T::TokenId,
            hasher(blake2_128_concat) HostingPair<T>
        => Option<Receipt<T::TokenId, TokenBalanceOf<T>>>;

        Dominators get(fn dominators): map
            hasher(blake2_128_concat) T::AccountId
        => Option<Dominator<(), BalanceOf<T>>>;

        TokenDominators get(fn token_dominators): double_map
            hasher(blake2_128_concat) T::TokenId,
            hasher(blake2_128_concat) T::AccountId
        => Option<Dominator<T::TokenId, TokenBalanceOf<T>>>;
    }
}

decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as frame_system::Trait>::AccountId,
        TokenId = <T as Trait>::TokenId,
        Balance = BalanceOf<T>,
        TokenBalance = TokenBalanceOf<T>,
    {
        DominatorClaimed(AccountId, Balance, Vec<ExternalChain>),
        TokenDominatorClaimed(TokenId, AccountId, TokenBalance),
        AssetsHosted(AccountId, AccountId, Balance),
        TokenHosted(TokenId, AccountId, AccountId, TokenBalance),
        HostAssetsChanged(AccountId, AccountId, Balance),
        HostTokenChanged(TokenId, AccountId, AccountId, TokenBalance),
        AssetsRevokeSubmitted(AccountId, AccountId),
        TokenRevokeSubmitted(TokenId, AccountId, AccountId),
        AssetsRevokeConfirmed(AccountId, AccountId),
        TokenRevokeConfirmed(TokenId, AccountId, AccountId),
    }
);

decl_error! {
    pub enum Error for Module<T: Trait> {
        DominatorNotFound,
        ReceiptNotExists,
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
        pub fn claim_dominator(origin, #[compact] amount: BalanceOf<T>, support_chains: Vec<ExternalChain>) {
            let dominator = ensure_signed(origin)?;
            ensure!(!<Dominators<T>>::contains_key(&dominator), Error::<T>::DominatorAlreadyExists);
            T::Currency::reserve(&dominator, amount)?;
            <Dominators<T>>::insert(&dominator, Dominator {
                total_pledged: amount,
                total_hosted: Zero::zero(),
                status: DominatorStatus::Active,
                support_chains: support_chains.clone(),
                assets_type: None,
            });
            Self::deposit_event(RawEvent::DominatorClaimed(dominator, amount, support_chains));
        }

        #[weight = 100_000]
        pub fn grant(origin,
                     dominator: <T::Lookup as StaticLookup>::Source,
                     #[compact] amount: BalanceOf<T>,
                     declared_addresses: Vec<(ExternalChain, Vec<u8>)>,
                     memo: UID) {
            let fund_owner = ensure_signed(origin)?;
            let dominator = T::Lookup::lookup(dominator)?;
            let claimed = Dominators::<T>::get(&dominator).ok_or(Error::<T>::DominatorNotFound)?;
            ensure!(claimed.status == DominatorStatus::Active, Error::<T>::InvalidStatus);
            ensure!(!Receipts::<T>::contains_key((&fund_owner, &dominator)), Error::<T>::ReceiptAlreadyExists);
            ensure!(claimed.total_hosted.saturating_add(amount) <= claimed.total_pledged, Error::<T>::PledgeUnsatisfied);
            declared_addresses.iter().all(|(chain, _)|claimed.support_chains.contains(chain));
            ensure!(T::Currency::can_reserve(&fund_owner, amount), Error::<T>::InsufficientBalance);
            T::Currency::reserve(&fund_owner, amount)?;
            Receipts::<T>::insert((&fund_owner, &dominator), Receipt {
                value: amount,
                status: ReceiptStatus::Active,
                assets_type: None,
            });
            Dominators::<T>::insert(&dominator, Dominator {
                total_pledged: claimed.total_pledged,
                total_hosted: claimed.total_hosted.saturating_add(amount),
                status: DominatorStatus::Active,
                support_chains: claimed.support_chains,
                assets_type: None,
            });
            Self::deposit_event(RawEvent::AssetsHosted(fund_owner, dominator, amount));
        }

        // #[weight = 100]
        // pub fn grant_token(origin,
        //              token: T::TokenId,
        //              dominator: <T::Lookup as StaticLookup>::Source,
        //              #[compact] amount: TokenBalanceOf<T>,
        //              memo: UID) {
        //     let fund_owner = ensure_signed(origin)?;
        //     let dominator = T::Lookup::lookup(dominator)?;
        //     let claimed = TokenDominators::<T>::get(&token, &dominator).ok_or(Error::<T>::DominatorNotFound)?;
        //     ensure!(claimed.status == DominatorStatus::Active, Error::<T>::InvalidStatus);
        //     ensure!(!TokenReceipts::<T>::contains_key(&token, (&fund_owner, &dominator)), Error::<T>::ReceiptAlreadyExists);
        //     ensure!(claimed.total_hosted.saturating_add(amount) <= claimed.total_pledged, Error::<T>::PledgeUnsatisfied);
        //     ensure!(T::Token::can_reserve(&token, &fund_owner, amount), Error::<T>::InsufficientBalance);
        //     T::Token::reserve(&token, &fund_owner, amount)?;
        //     TokenReceipts::<T>::insert(&token, (&fund_owner, &dominator), Receipt {
        //         value: amount,
        //         status: ReceiptStatus::Active,
        //         assets_type: Some(token),
        //     });
        //     TokenDominators::<T>::insert(&token, &dominator, Dominator {
        //         total_pledged: claimed.total_pledged,
        //         total_hosted: claimed.total_hosted.saturating_add(amount),
        //         status: DominatorStatus::Active,
        //         assets_type: Some(token),
        //     });
        //     Self::deposit_event(RawEvent::TokenHosted(token, fund_owner, dominator, amount));
        // }

        #[weight = 100]
        pub fn revoke(origin, dominator: <T::Lookup as StaticLookup>::Source) {
            let fund_owner = ensure_signed(origin)?;
            let dominator = T::Lookup::lookup(dominator)?;
            let receipt = Receipts::<T>::get((&fund_owner, &dominator)).ok_or(Error::<T>::ReceiptNotExists)?;
            ensure!(receipt.status == ReceiptStatus::Active, Error::<T>::InvalidStatus);
            Receipts::<T>::insert((&fund_owner, &dominator), Receipt {
                value: receipt.value,
                status: ReceiptStatus::Revoking,
                assets_type: None,
            });
            Self::deposit_event(RawEvent::AssetsRevokeSubmitted(fund_owner, dominator));
        }

        #[weight = 100]
        pub fn revoke_token(origin, token: T::TokenId, dominator: <T::Lookup as StaticLookup>::Source) {
            let fund_owner = ensure_signed(origin)?;
            let dominator = T::Lookup::lookup(dominator)?;
            let receipt = TokenReceipts::<T>::get(&token, (&fund_owner, &dominator)).ok_or(Error::<T>::ReceiptNotExists)?;
            ensure!(receipt.status == ReceiptStatus::Active, Error::<T>::InvalidStatus);
            TokenReceipts::<T>::insert(token, (&fund_owner, &dominator), Receipt {
                value: receipt.value,
                status: ReceiptStatus::Revoking,
                assets_type: Some(token),
            });
            Self::deposit_event(RawEvent::TokenRevokeSubmitted(token, fund_owner, dominator));
        }

        /// signed by the dominator, not fund owner
        #[weight = 100]
        pub fn merge_to_deduct_stash(origin,
                                     fund_owner: T::AccountId,
                                     #[compact] amount: BalanceOf<T>,
                                     digest: Vec<u8>) {
            let dominator = ensure_signed(origin)?;
            let claimed = Dominators::<T>::get(&dominator).ok_or(Error::<T>::DominatorNotFound)?;
            ensure!(claimed.total_hosted >= amount, Error::<T>::InsufficientStashAccount);
            let receipt = Receipts::<T>::get((&fund_owner, &dominator)).ok_or(Error::<T>::ReceiptNotExists)?;
            ensure!(receipt.value >= amount, Error::<T>::InsufficientStashAccount);
            ensure!(T::Currency::reserved_balance(&fund_owner) >= amount, Error::<T>::InsufficientStashAccount);
            T::Currency::repatriate_reserved(&fund_owner, &dominator, amount, BalanceStatus::Free)?;
            Dominators::<T>::insert(&dominator, Dominator {
                total_hosted: claimed.total_hosted.saturating_sub(amount),
                total_pledged: claimed.total_pledged,
                status: claimed.status,
                support_chains: claimed.support_chains,
                assets_type: None,
            });
            if receipt.value == amount {
                Receipts::<T>::remove((&fund_owner, &dominator));
            } else {
                Receipts::<T>::insert((&fund_owner, &dominator), Receipt {
                    value: receipt.value.saturating_sub(amount),
                    status: receipt.status,
                    assets_type: None,
                });
            }
        }

        // #[weight = 100]
        // pub fn merge_to_deduct_stash_token(origin,
        //                                    token: T::TokenId,
        //                                    fund_owner: T::AccountId,
        //                                    #[compact] amount: TokenBalanceOf<T>,
        //                                    digest: Vec<u8>) {
        //     let dominator = ensure_signed(origin)?;
        //     let claimed = TokenDominators::<T>::get(&token, &dominator).ok_or(Error::<T>::DominatorNotFound)?;
        //     ensure!(claimed.total_hosted >= amount, Error::<T>::InsufficientStashAccount);
        //     let receipt = TokenReceipts::<T>::get(&token, (&fund_owner, &dominator)).ok_or(Error::<T>::ReceiptNotExists)?;
        //     ensure!(receipt.value >= amount, Error::<T>::InsufficientStashAccount);
        //     ensure!(T::Token::reserved_balance(&token, &fund_owner) >= amount, Error::<T>::InsufficientStashAccount);
        //     T::Token::repatriate_reserved(&token, &fund_owner, &dominator, amount, BalanceStatus::Free)?;
        //     TokenDominators::<T>::insert(&token, &dominator, Dominator {
        //         total_hosted: claimed.total_hosted.saturating_sub(amount),
        //         total_pledged: claimed.total_pledged,
        //         status: claimed.status,
        //         assets_type: Some(token),
        //     });
        //     if receipt.value == amount {
        //         TokenReceipts::<T>::remove(&token, (&fund_owner, &dominator));
        //     } else {
        //         TokenReceipts::<T>::insert(&token, (&fund_owner, &dominator), Receipt {
        //             value: receipt.value.saturating_sub(amount),
        //             status: receipt.status,
        //             assets_type: Some(token),
        //         });
        //     }
        // }

        /// signed by the dominator, not fund owner
        #[weight = 100]
        pub fn merge_to_add_stash(origin, fund_owner: T::AccountId, #[compact] amount: BalanceOf<T>, digest: Vec<u8>) {
            let dominator = ensure_signed(origin)?;
            let claimed = Dominators::<T>::get(&dominator).ok_or(Error::<T>::DominatorNotFound)?;
            ensure!(claimed.total_pledged >= amount, Error::<T>::InsufficientStashAccount);
            ensure!(T::Currency::reserved_balance(&dominator) >= amount, Error::<T>::InsufficientStashAccount);
            T::Currency::repatriate_reserved(&dominator, &fund_owner, amount, BalanceStatus::Free)?;
            Dominators::<T>::insert(&dominator, Dominator {
                total_hosted: claimed.total_hosted,
                total_pledged: claimed.total_pledged.saturating_sub(amount),
                status: claimed.status,
                support_chains: claimed.support_chains,
                assets_type: None,
            });
        }

        // #[weight = 100]
        // pub fn merge_to_add_stash_token(origin, token: T::TokenId, fund_owner: T::AccountId, #[compact] amount: TokenBalanceOf<T>, digest: Vec<u8>) {
        //     let dominator = ensure_signed(origin)?;
        //     let claimed = TokenDominators::<T>::get(&token, &dominator).ok_or(Error::<T>::DominatorNotFound)?;
        //     ensure!(claimed.total_pledged >= amount, Error::<T>::InsufficientStashAccount);
        //     ensure!(T::Token::reserved_balance(&token, &dominator) >= amount, Error::<T>::InsufficientStashAccount);
        //     T::Token::repatriate_reserved(&token, &dominator, &fund_owner, amount, BalanceStatus::Free)?;
        //     TokenDominators::<T>::insert(&token, &dominator, Dominator {
        //         total_hosted: claimed.total_hosted,
        //         total_pledged: claimed.total_pledged.saturating_sub(amount),
        //         status: claimed.status,
        //         assets_type: Some(token),
        //     });
        // }

        #[weight = 100]
        pub fn confirm_withdraw(origin, fund_owner: T::AccountId) {
            let dominator = ensure_signed(origin)?;
            let claimed = Dominators::<T>::get(&dominator).ok_or(Error::<T>::DominatorNotFound)?;
            let receipt = Receipts::<T>::get((&fund_owner, &dominator)).ok_or(Error::<T>::ReceiptNotExists)?;
            ensure!(receipt.status == ReceiptStatus::Revoking, Error::<T>::InvalidStatus);
            ensure!(T::Currency::reserved_balance(&fund_owner) >= receipt.value, Error::<T>::InvalidStatus);
            T::Currency::unreserve(&fund_owner, receipt.value);
            Dominators::<T>::insert(&dominator, Dominator {
                total_pledged: claimed.total_pledged,
                total_hosted: claimed.total_hosted.saturating_sub(receipt.value),
                status: claimed.status,
                support_chains: claimed.support_chains,
                assets_type: None,
            });
            Receipts::<T>::remove((&fund_owner, &dominator));
            Self::deposit_event(RawEvent::AssetsRevokeConfirmed(fund_owner, dominator));
        }

        // #[weight = 100]
        // pub fn confirm_token_withdraw(origin, token: T::TokenId, fund_owner: T::AccountId) {
        //     let dominator = ensure_signed(origin)?;
        //     let claimed = TokenDominators::<T>::get(&token, &dominator).ok_or(Error::<T>::DominatorNotFound)?;
        //     let receipt = TokenReceipts::<T>::get(&token, (&fund_owner, &dominator)).ok_or(Error::<T>::ReceiptNotExists)?;
        //     ensure!(receipt.status == ReceiptStatus::Revoking, Error::<T>::InvalidStatus);
        //     ensure!(T::Token::reserved_balance(&token, &fund_owner) >= receipt.value, Error::<T>::InvalidStatus);
        //     T::Token::unreserve(&token, &fund_owner, receipt.value)?;
        //     TokenDominators::<T>::insert(&token, &dominator, Dominator {
        //         total_pledged: claimed.total_pledged,
        //         total_hosted: claimed.total_hosted.saturating_sub(receipt.value),
        //         status: claimed.status,
        //         assets_type: Some(token),
        //     });
        //     TokenReceipts::<T>::remove(&token, (&fund_owner, &dominator));
        //     Self::deposit_event(RawEvent::TokenRevokeConfirmed(token, fund_owner, dominator));
        // }


        // #[weight = 100]
        // pub fn claim_token_dominator(origin, token: T::TokenId, #[compact] amount: TokenBalanceOf<T>) {
        //     let dominator = ensure_signed(origin)?;
        //     ensure!(!<TokenDominators<T>>::contains_key(&token, &dominator), Error::<T>::DominatorAlreadyExists);
        //     T::Token::reserve(&token, &dominator, amount)?;
        //     <TokenDominators<T>>::insert(&token, &dominator, Dominator {
        //         total_pledged: amount,
        //         total_hosted: Zero::zero(),
        //         status: DominatorStatus::Active,
        //         assets_type: Some(token),
        //     });
        //     Self::deposit_event(RawEvent::TokenDominatorClaimed(token, dominator, amount));
        // }

        // pub fn incr_pledge(origin) {

        // }

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
