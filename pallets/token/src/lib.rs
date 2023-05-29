// Copyright 2021-2023 UINB Technologies Pte. Ltd.
//
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
pub mod weights;

#[cfg(test)]
pub mod mock;
#[cfg(test)]
pub mod tests;

#[frame_support::pallet]
pub mod pallet {
    use crate::weights::WeightInfo;
    use ascii::AsciiStr;
    use codec::{Codec, Decode, Encode};
    use frame_support::traits::fungible::Mutate;
    use frame_support::traits::tokens::AssetId;
    use frame_support::{
        pallet_prelude::*,
        traits::{
            tokens::{
                fungibles, BalanceConversion, DepositConsequence, ExistenceRequirement,
                WithdrawConsequence,
            },
            BalanceStatus, Currency, ReservableCurrency,
        },
        transactional,
    };
    use frame_system::pallet_prelude::*;
    use fuso_support::traits::{ChainIdOf, DecimalsTransformer};
    use fuso_support::{
        constants::*,
        traits::{ReservableToken, Token},
        ChainId, XToken,
    };
    use pallet_octopus_support::traits::TokenIdAndAssetIdProvider;
    use scale_info::TypeInfo;
    use sp_runtime::traits::{
        AtLeast32BitUnsigned, CheckedAdd, CheckedSub, MaybeSerializeDeserialize, Member, One,
        StaticLookup, Zero,
    };
    use sp_runtime::DispatchResult;
    use sp_std::vec::Vec;

    #[derive(Encode, Decode, Clone, PartialEq, Eq, Default, TypeInfo, Debug)]
    pub struct TokenAccountData<Balance> {
        pub free: Balance,
        pub reserved: Balance,
    }

    pub type BalanceOf<T> = <T as pallet_balances::Config>::Balance;

    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_balances::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type TokenId: Member
            + Parameter
            + AtLeast32BitUnsigned
            + Default
            + PartialEq
            + Copy
            + Codec
            + MaybeSerializeDeserialize
            + MaxEncodedLen
            + AssetId;

        // TODO
        #[pallet::constant]
        type NearChainId: Get<ChainId>;

        #[pallet::constant]
        type EthChainId: Get<ChainId>;

        #[pallet::constant]
        type BnbChainId: Get<ChainId>;

        #[pallet::constant]
        type NativeChainId: Get<ChainId>;

        #[pallet::constant]
        type NativeTokenId: Get<Self::TokenId>;

        #[pallet::constant]
        type PolygonChainId: Get<ChainId>;

        type Weight: WeightInfo;

        #[pallet::constant]
        type BurnTAOwhenIssue: Get<BalanceOf<Self>>;

        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;
    }

    #[pallet::pallet]
    #[pallet::without_storage_info]
    #[pallet::generate_store(pub (super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::error]
    pub enum Error<T> {
        AmountZero,
        BalanceLow,
        BalanceZero,
        InvalidTokenName,
        TokenNotFound,
        InsufficientBalance,
        Overflow,
        TooManyReserves,
        InvalidDecimals,
        ContractError,
    }

    #[pallet::storage]
    #[pallet::getter(fn get_token_balance)]
    pub type Balances<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        (T::TokenId, T::AccountId),
        TokenAccountData<BalanceOf<T>>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn get_token_info)]
    pub type Tokens<T: Config> =
        StorageMap<_, Twox64Concat, T::TokenId, XToken<BalanceOf<T>>, OptionQuery>;

    #[pallet::type_value]
    pub fn DefaultNextTokenId<T: Config>() -> T::TokenId {
        One::one()
    }

    #[pallet::storage]
    #[pallet::getter(fn next_token_id)]
    pub type NextTokenId<T: Config> =
        StorageValue<_, T::TokenId, ValueQuery, DefaultNextTokenId<T>>;

    /// used by the octbridge. the chainid is omited. avoid to use the storage directly in case mess everything
    #[pallet::storage]
    #[pallet::getter(fn get_token_from_octopus)]
    pub type TokenByName<T: Config> = StorageMap<_, Twox64Concat, Vec<u8>, T::TokenId, OptionQuery>;

    /// used by the chainbridge. avoid to use the storage directly in case mess everything
    #[pallet::storage]
    #[pallet::getter(fn get_token_from_chainbridge)]
    pub type TokenByContract<T: Config> =
        StorageMap<_, Blake2_128Concat, (ChainId, Vec<u8>), T::TokenId, OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub (super) fn deposit_event)]
    pub enum Event<T: Config> {
        TokenIssued(T::TokenId, Vec<u8>),
        TokenTransfered(T::TokenId, T::AccountId, T::AccountId, BalanceOf<T>),
        TokenReserved(T::TokenId, T::AccountId, BalanceOf<T>),
        TokenUnreserved(T::TokenId, T::AccountId, BalanceOf<T>),
        TokenMinted(T::TokenId, T::AccountId, BalanceOf<T>),
        TokenBurned(T::TokenId, T::AccountId, BalanceOf<T>),
        TokenRepatriated(T::TokenId, T::AccountId, T::AccountId, BalanceOf<T>),
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[transactional]
        #[pallet::weight(10_000)]
        pub fn issue(
            origin: OriginFor<T>,
            token_info: XToken<BalanceOf<T>>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            pallet_balances::Pallet::<T>::burn_from(&who, T::BurnTAOwhenIssue::get())?;
            let id = Self::create(token_info.clone())?;
            Self::deposit_event(Event::<T>::TokenIssued(id, token_info.symbol()));
            Ok(().into())
        }

        #[pallet::weight(0)]
        pub fn mark_stable(origin: OriginFor<T>, id: T::TokenId) -> DispatchResultWithPostInfo {
            let _ = ensure_root(origin)?;
            Tokens::<T>::try_mutate_exists(id, |info| -> DispatchResult {
                ensure!(info.is_some(), Error::<T>::TokenNotFound);
                let mut token_info = info.take().unwrap();
                match token_info {
                    XToken::NEP141(_, _, _, ref mut stable, _) => *stable = true,
                    XToken::ERC20(_, _, _, ref mut stable, _) => *stable = true,
                    XToken::POLYGON(_, _, _, ref mut stable, _) => *stable = true,
                    XToken::BEP20(_, _, _, ref mut stable, _) => *stable = true,
                    XToken::FND10(_, _) => return Err(Error::<T>::TokenNotFound.into()),
                }
                info.replace(token_info);
                Ok(())
            })?;
            Ok(().into())
        }

        #[pallet::weight(T::Weight::transfer())]
        pub fn transfer(
            origin: OriginFor<T>,
            token: T::TokenId,
            target: <T::Lookup as StaticLookup>::Source,
            amount: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            ensure!(!amount.is_zero(), Error::<T>::BalanceZero);
            let target = T::Lookup::lookup(target)?;
            Self::transfer_token(&who, token, amount, &target)?;
            Ok(().into())
        }

        #[pallet::weight(195_000_0000)]
        pub fn associate_token(
            origin: OriginFor<T>,
            chain_id: ChainId,
            contract_id: Vec<u8>,
            token_id: T::TokenId,
        ) -> DispatchResult {
            let _ = T::AdminOrigin::ensure_origin(origin)?;
            TokenByContract::<T>::insert((chain_id, contract_id), token_id);
            Ok(())
        }
    }

    impl<T: Config> Pallet<T>
    where
        BalanceOf<T>: From<u128> + Into<u128>,
    {
        /// the verifier requests all amount should be 10^18, the `do_mint` is called by oct-pallets,
        /// the parameter `amount` is 10^decimals_of_metadata, in anthor word, the real storage of token amount unified
        #[transactional]
        pub fn do_mint(
            token: T::TokenId,
            beneficiary: &T::AccountId,
            amount: BalanceOf<T>,
            _maybe_check_issuer: Option<T::AccountId>,
        ) -> DispatchResult {
            if amount == Zero::zero() {
                return Ok(());
            }
            Tokens::<T>::try_mutate_exists(&token, |token_info| -> DispatchResult {
                ensure!(token_info.is_some(), Error::<T>::TokenNotFound);
                let mut info = token_info.take().unwrap();
                let unified_amount = match info {
                    XToken::NEP141(_, _, ref mut total, _, decimals)
                    | XToken::ERC20(_, _, ref mut total, _, decimals)
                    | XToken::POLYGON(_, _, ref mut total, _, decimals)
                    | XToken::BEP20(_, _, ref mut total, _, decimals) => {
                        let unified_amount = Self::transform_decimals_to_standard(amount, decimals);
                        *total = total
                            .checked_add(&unified_amount)
                            .ok_or(Error::<T>::InsufficientBalance)?;
                        unified_amount
                    }
                    XToken::FND10(_, total) => total,
                };
                ensure!(!unified_amount.is_zero(), Error::<T>::AmountZero);
                Balances::<T>::try_mutate_exists((&token, beneficiary), |to| -> DispatchResult {
                    let mut account = to.take().unwrap_or_default();
                    account.free = account
                        .free
                        .checked_add(&unified_amount)
                        .ok_or(Error::<T>::Overflow)?;
                    to.replace(account);
                    Ok(())
                })?;
                token_info.replace(info);
                Self::deposit_event(Event::TokenMinted(
                    token,
                    beneficiary.clone(),
                    unified_amount,
                ));
                Ok(())
            })?;
            Ok(())
        }

        /// the verifier requests all amount should be 10^18, the `do_burn` is called by oct-pallets,
        /// the parameter `amount` is 10^decimals_of_metadata, in anthor word, the real storage of token amount is unified
        #[transactional]
        pub fn do_burn(
            token: T::TokenId,
            target: &T::AccountId,
            amount: BalanceOf<T>,
            _maybe_check_admin: Option<T::AccountId>,
        ) -> Result<BalanceOf<T>, DispatchError> {
            ensure!(!amount.is_zero(), Error::<T>::AmountZero);
            Tokens::<T>::try_mutate_exists(&token, |token_info| -> DispatchResult {
                ensure!(token_info.is_some(), Error::<T>::BalanceZero);
                let mut info = token_info.take().unwrap();
                let unified_amount = match info {
                    XToken::NEP141(_, _, ref mut total, _, decimals)
                    | XToken::ERC20(_, _, ref mut total, _, decimals)
                    | XToken::POLYGON(_, _, ref mut total, _, decimals)
                    | XToken::BEP20(_, _, ref mut total, _, decimals) => {
                        let unified_amount = Self::transform_decimals_to_standard(amount, decimals);
                        *total = total
                            .checked_sub(&unified_amount)
                            .ok_or(Error::<T>::InsufficientBalance)?;
                        unified_amount
                    }
                    XToken::FND10(_, total) => total,
                };
                ensure!(!unified_amount.is_zero(), Error::<T>::AmountZero);
                Balances::<T>::try_mutate_exists((&token, target), |from| -> DispatchResult {
                    ensure!(from.is_some(), Error::<T>::BalanceZero);
                    let mut account = from.take().unwrap();
                    account.free = account
                        .free
                        .checked_sub(&unified_amount)
                        .ok_or(Error::<T>::InsufficientBalance)?;
                    match account.free == Zero::zero() && account.reserved == Zero::zero() {
                        true => {}
                        false => {
                            from.replace(account);
                        }
                    }
                    Ok(())
                })?;
                token_info.replace(info);
                Self::deposit_event(Event::TokenBurned(token, target.clone(), unified_amount));
                Ok(())
            })?;
            Ok(amount)
        }
    }

    impl<T: Config> fungibles::Inspect<T::AccountId> for Pallet<T> {
        type AssetId = T::TokenId;
        type Balance = BalanceOf<T>;

        fn total_issuance(asset: Self::AssetId) -> Self::Balance {
            <Self as Token<T::AccountId>>::total_issuance(&asset)
        }

        fn minimum_balance(_asset: Self::AssetId) -> Self::Balance {
            // TODO sybil attack
            One::one()
        }

        fn balance(asset: Self::AssetId, who: &T::AccountId) -> Self::Balance {
            let balance = Balances::<T>::get((asset, who));
            balance.free + balance.reserved
        }

        fn reducible_balance(
            asset: Self::AssetId,
            who: &T::AccountId,
            _keep_alive: bool,
        ) -> Self::Balance {
            <Self as Token<T::AccountId>>::free_balance(&asset, &who)
        }

        fn can_deposit(
            asset: Self::AssetId,
            _who: &T::AccountId,
            _amount: Self::Balance,
            _mint: bool,
        ) -> DepositConsequence {
            match Self::get_token_info(asset) {
                Some(_) => DepositConsequence::Success,
                None => DepositConsequence::UnknownAsset,
            }
        }

        fn can_withdraw(
            asset: Self::AssetId,
            who: &T::AccountId,
            amount: Self::Balance,
        ) -> WithdrawConsequence<Self::Balance> {
            let free = <Self as Token<T::AccountId>>::free_balance(&asset, who);
            match free >= amount {
                true => WithdrawConsequence::Success,
                false => WithdrawConsequence::NoFunds,
            }
        }
    }

    impl<T: Config> fungibles::Mutate<T::AccountId> for Pallet<T>
    where
        Self::Balance: From<u128> + Into<u128>,
    {
        fn mint_into(
            asset: Self::AssetId,
            who: &T::AccountId,
            amount: Self::Balance,
        ) -> DispatchResult {
            Self::do_mint(asset, who, amount, None)
        }

        fn burn_from(
            asset: Self::AssetId,
            who: &T::AccountId,
            amount: Self::Balance,
        ) -> Result<Self::Balance, DispatchError> {
            Self::do_burn(asset, who, amount, None)
        }

        fn slash(
            asset: Self::AssetId,
            who: &T::AccountId,
            amount: Self::Balance,
        ) -> Result<Self::Balance, DispatchError> {
            Self::do_burn(asset, who, amount, None)
        }
    }

    impl<T: Config> Token<T::AccountId> for Pallet<T> {
        type Balance = BalanceOf<T>;
        type TokenId = T::TokenId;

        #[transactional]
        fn create(mut token_info: XToken<BalanceOf<T>>) -> Result<Self::TokenId, DispatchError> {
            let id = Self::next_token_id();
            match token_info {
                XToken::NEP141(
                    ref symbol,
                    ref contract,
                    ref mut total,
                    ref mut stable,
                    decimals,
                ) => {
                    ensure!(decimals <= MAX_DECIMALS, Error::<T>::InvalidDecimals);
                    let name = AsciiStr::from_ascii(&symbol);
                    ensure!(name.is_ok(), Error::<T>::InvalidTokenName);
                    let name = name.unwrap();
                    ensure!(
                        name.len() >= 2 && name.len() <= 8,
                        Error::<T>::InvalidTokenName
                    );
                    ensure!(
                        contract.len() < 120 && contract.len() > 2,
                        Error::<T>::ContractError
                    );
                    ensure!(
                        !TokenByName::<T>::contains_key(&contract),
                        Error::<T>::ContractError
                    );
                    *total = Zero::zero();
                    *stable = false;
                    TokenByName::<T>::insert(contract.clone(), id);
                }
                XToken::ERC20(
                    ref symbol,
                    ref contract,
                    ref mut total,
                    ref mut stable,
                    decimals,
                )
                | XToken::POLYGON(
                    ref symbol,
                    ref contract,
                    ref mut total,
                    ref mut stable,
                    decimals,
                )
                | XToken::BEP20(
                    ref symbol,
                    ref contract,
                    ref mut total,
                    ref mut stable,
                    decimals,
                ) => {
                    ensure!(decimals <= MAX_DECIMALS, Error::<T>::InvalidDecimals);
                    let name = AsciiStr::from_ascii(&symbol);
                    ensure!(name.is_ok(), Error::<T>::InvalidTokenName);
                    let name = name.unwrap();
                    ensure!(
                        name.len() >= 2 && name.len() <= 8,
                        Error::<T>::InvalidTokenName
                    );
                    ensure!(contract.len() == 20, Error::<T>::ContractError);
                    *total = Zero::zero();
                    *stable = false;
                }
                XToken::FND10(ref symbol, ref mut total) => {
                    let name = AsciiStr::from_ascii(&symbol);
                    ensure!(name.is_ok(), Error::<T>::InvalidTokenName);
                    let name = name.unwrap();
                    ensure!(
                        name.len() >= 2 && name.len() <= 8,
                        Error::<T>::InvalidTokenName
                    );
                    *total = Zero::zero();
                }
            }
            NextTokenId::<T>::mutate(|id| *id += One::one());
            Tokens::<T>::insert(id, token_info);
            Ok(id)
        }

        #[transactional]
        fn transfer_token(
            origin: &T::AccountId,
            token: Self::TokenId,
            amount: Self::Balance,
            target: &T::AccountId,
        ) -> Result<Self::Balance, DispatchError> {
            if amount.is_zero() {
                return Ok(amount);
            }
            if origin == target {
                return Ok(amount);
            }
            if token == Self::native_token_id() {
                return <pallet_balances::Pallet<T> as Currency<T::AccountId>>::transfer(
                    origin,
                    target,
                    amount,
                    ExistenceRequirement::KeepAlive,
                )
                .map(|_| amount);
            }
            Balances::<T>::try_mutate_exists((&token, &origin), |from| -> DispatchResult {
                ensure!(from.is_some(), Error::<T>::BalanceZero);
                let mut account = from.take().unwrap();
                account.free = account
                    .free
                    .checked_sub(&amount)
                    .ok_or(Error::<T>::InsufficientBalance)?;
                match account.free == Zero::zero() && account.reserved == Zero::zero() {
                    true => {}
                    false => {
                        from.replace(account);
                    }
                }
                Balances::<T>::try_mutate_exists((&token, &target), |to| -> DispatchResult {
                    let mut account = to.take().unwrap_or(TokenAccountData {
                        free: Zero::zero(),
                        reserved: Zero::zero(),
                    });
                    account.free = account
                        .free
                        .checked_add(&amount)
                        .ok_or(Error::<T>::Overflow)?;
                    to.replace(account);
                    Ok(())
                })?;
                Ok(())
            })?;
            Self::deposit_event(Event::TokenTransfered(
                token,
                origin.clone(),
                target.clone(),
                amount,
            ));
            Ok(amount)
        }

        fn try_mutate_account<R>(
            token: &Self::TokenId,
            who: &T::AccountId,
            f: impl FnOnce(&mut (Self::Balance, Self::Balance)) -> Result<R, DispatchError>,
        ) -> Result<R, DispatchError> {
            if *token == Self::native_token_id() {
                pallet_balances::Pallet::<T>::mutate_account(
                    who,
                    |b| -> Result<R, DispatchError> {
                        let mut v = (b.free, b.reserved);
                        let r = f(&mut v)?;
                        b.free = v.0;
                        b.reserved = v.1;
                        Ok(r)
                    },
                )?
            } else {
                Balances::<T>::try_mutate_exists((token, who), |t| -> Result<R, DispatchError> {
                    let mut b = t.take().unwrap_or_default();
                    let mut v = (b.free, b.reserved);
                    let r = f(&mut v)?;
                    b.free = v.0;
                    b.reserved = v.1;
                    match b.free == Zero::zero() && b.reserved == Zero::zero() {
                        true => {}
                        false => {
                            t.replace(b);
                        }
                    }
                    Ok(r)
                })
            }
        }

        fn try_mutate_issuance(
            token: &Self::TokenId,
            f: impl FnOnce(&mut Self::Balance) -> Result<(), DispatchError>,
        ) -> Result<(), DispatchError> {
            if *token == Self::native_token_id() {
                <pallet_balances::TotalIssuance<T>>::try_mutate(|total| f(total))
            } else {
                Err(DispatchError::Other("can't update the token issuance"))
            }
        }

        fn exists(token: &T::TokenId) -> bool {
            *token == Self::native_token_id() || Tokens::<T>::contains_key(token)
        }

        fn native_token_id() -> Self::TokenId {
            T::NativeTokenId::get()
        }

        fn is_stable(token: &T::TokenId) -> bool {
            if *token == Self::native_token_id() {
                false
            } else {
                Self::get_token_info(token)
                    .map(|t| t.is_stable())
                    .unwrap_or(false)
            }
        }

        fn free_balance(token: &T::TokenId, who: &T::AccountId) -> Self::Balance {
            if *token == Self::native_token_id() {
                return pallet_balances::Pallet::<T>::free_balance(who);
            }
            Self::get_token_balance((token, who)).free
        }

        fn total_issuance(token: &T::TokenId) -> Self::Balance {
            if *token == Self::native_token_id() {
                return pallet_balances::Pallet::<T>::total_issuance();
            }
            let token_info = Self::get_token_info(token);
            if token_info.is_some() {
                let token = token_info.unwrap();
                match token {
                    XToken::NEP141(_, _, total, _, _)
                    | XToken::ERC20(_, _, total, _, _)
                    | XToken::POLYGON(_, _, total, _, _)
                    | XToken::BEP20(_, _, total, _, _) => total,
                    XToken::FND10(_, total) => total,
                }
            } else {
                Zero::zero()
            }
        }

        fn token_external_decimals(token: &T::TokenId) -> Result<u8, DispatchError> {
            if *token == Self::native_token_id() {
                return Ok(STANDARD_DECIMALS);
            }
            let token_info = Self::get_token_info(token);
            if token_info.is_some() {
                let token = token_info.unwrap();
                match token {
                    XToken::NEP141(_, _, _, _, decimals)
                    | XToken::ERC20(_, _, _, _, decimals)
                    | XToken::POLYGON(_, _, _, _, decimals)
                    | XToken::BEP20(_, _, _, _, decimals) => Ok(decimals),
                    XToken::FND10(_, _) => Err(Error::<T>::TokenNotFound.into()),
                }
            } else {
                Err(Error::<T>::TokenNotFound.into())
            }
        }
    }

    impl<T: Config> DecimalsTransformer<BalanceOf<T>> for Pallet<T>
    where
        BalanceOf<T>: From<u128> + Into<u128>,
    {
        fn transform_decimals_to_standard(
            amount: BalanceOf<T>,
            external_decimals: u8,
        ) -> BalanceOf<T>
        where
            BalanceOf<T>: From<u128> + Into<u128>,
        {
            let mut amount: u128 = amount.into();
            if external_decimals > STANDARD_DECIMALS {
                let diff = external_decimals - STANDARD_DECIMALS;
                for _i in 0..diff {
                    amount /= 10
                }
            } else {
                let diff = STANDARD_DECIMALS - external_decimals;
                for _i in 0..diff {
                    amount *= 10
                }
            }
            amount.into()
        }

        fn transform_decimals_to_external(
            amount: BalanceOf<T>,
            external_decimals: u8,
        ) -> BalanceOf<T> {
            let mut amount: u128 = amount.into();
            if external_decimals > STANDARD_DECIMALS {
                let diff = external_decimals - STANDARD_DECIMALS;
                for _i in 0..diff {
                    amount *= 10
                }
            } else {
                let diff = STANDARD_DECIMALS - external_decimals;
                for _i in 0..diff {
                    amount /= 10
                }
            }
            amount.into()
        }
    }

    impl<T: Config> ReservableToken<T::AccountId> for Pallet<T> {
        fn can_reserve(token: &T::TokenId, who: &T::AccountId, value: BalanceOf<T>) -> bool {
            if value.is_zero() {
                return true;
            }
            if *token == Self::native_token_id() {
                return pallet_balances::Pallet::<T>::can_reserve(who, value);
            }
            Self::free_balance(token, who) >= value
        }

        fn reserve(
            token: &T::TokenId,
            who: &T::AccountId,
            value: BalanceOf<T>,
        ) -> sp_std::result::Result<(), DispatchError> {
            if value.is_zero() {
                return Ok(());
            }
            if *token == Self::native_token_id() {
                return pallet_balances::Pallet::<T>::reserve(who, value);
            }
            Balances::<T>::try_mutate_exists(
                (token, who),
                |account| -> sp_std::result::Result<(), DispatchError> {
                    ensure!(account.is_some(), Error::<T>::BalanceZero);
                    let account = account.as_mut().ok_or(Error::<T>::BalanceZero)?;
                    account.free = account
                        .free
                        .checked_sub(&value)
                        .ok_or(Error::<T>::InsufficientBalance)?;
                    account.reserved = account
                        .reserved
                        .checked_add(&value)
                        .ok_or(Error::<T>::Overflow)?;
                    Self::deposit_event(Event::TokenReserved(token.clone(), who.clone(), value));
                    Ok(())
                },
            )
        }

        fn unreserve(
            token: &T::TokenId,
            who: &T::AccountId,
            value: BalanceOf<T>,
        ) -> DispatchResult {
            if value.is_zero() {
                return Ok(());
            }
            if *token == Self::native_token_id() {
                ensure!(
                    pallet_balances::Pallet::<T>::reserved_balance(who) >= value,
                    Error::<T>::InsufficientBalance
                );
                pallet_balances::Pallet::<T>::unreserve(who, value);
                return Ok(());
            }
            Balances::<T>::try_mutate_exists((token, who), |account| -> DispatchResult {
                ensure!(account.is_some(), Error::<T>::BalanceZero);
                let account = account.as_mut().ok_or(Error::<T>::BalanceZero)?;
                account.reserved = account
                    .reserved
                    .checked_sub(&value)
                    .ok_or(Error::<T>::InsufficientBalance)?;
                account.free = account
                    .free
                    .checked_add(&value)
                    .ok_or(Error::<T>::Overflow)?;
                Self::deposit_event(Event::TokenUnreserved(token.clone(), who.clone(), value));
                Ok(())
            })
        }

        fn reserved_balance(token: &Self::TokenId, who: &T::AccountId) -> Self::Balance {
            if *token == Self::native_token_id() {
                return pallet_balances::Pallet::<T>::reserved_balance(who);
            }
            Balances::<T>::get((token, who)).reserved
        }

        fn repatriate_reserved(
            token: &T::TokenId,
            slashed: &T::AccountId,
            beneficiary: &T::AccountId,
            value: Self::Balance,
            status: BalanceStatus,
        ) -> DispatchResult {
            if *token == Self::native_token_id() {
                ensure!(
                    pallet_balances::Pallet::<T>::reserved_balance(slashed) >= value,
                    Error::<T>::InsufficientBalance
                );
                return pallet_balances::Pallet::<T>::repatriate_reserved(
                    slashed,
                    beneficiary,
                    value,
                    status,
                )
                .map(|_| ());
            }
            if slashed == beneficiary {
                return match status {
                    BalanceStatus::Free => Self::unreserve(token, slashed, value),
                    BalanceStatus::Reserved => Self::reserve(token, slashed, value),
                };
            }
            Balances::<T>::try_mutate_exists((token, slashed), |from| -> DispatchResult {
                ensure!(from.is_some(), Error::<T>::BalanceZero);
                let mut account = from.take().unwrap();
                account.reserved = account
                    .reserved
                    .checked_sub(&value)
                    .ok_or(Error::<T>::InsufficientBalance)?;
                // drop the `from` if dead
                match account.reserved == Zero::zero() && account.free == Zero::zero() {
                    true => {}
                    false => {
                        from.replace(account);
                    }
                }
                Balances::<T>::try_mutate_exists((token, beneficiary), |to| -> DispatchResult {
                    let mut account = to.take().unwrap_or_default();
                    match status {
                        BalanceStatus::Free => {
                            account.free = account
                                .free
                                .checked_add(&value)
                                .ok_or(Error::<T>::Overflow)?;
                        }
                        BalanceStatus::Reserved => {
                            account.reserved = account
                                .reserved
                                .checked_add(&value)
                                .ok_or(Error::<T>::Overflow)?;
                        }
                    }
                    to.replace(account);
                    Ok(())
                })?;
                Ok(())
            })?;
            Self::deposit_event(Event::TokenRepatriated(
                token.clone(),
                slashed.clone(),
                beneficiary.clone(),
                value,
            ));
            Ok(())
        }
    }

    /// used by the octopus bridge, asset_id means NEP-141 contract
    impl<T: Config> TokenIdAndAssetIdProvider<T::TokenId> for Pallet<T> {
        type Err = ();

        fn try_get_asset_id(token_id: impl AsRef<[u8]>) -> Result<T::TokenId, Self::Err> {
            Self::get_token_from_octopus(token_id.as_ref().to_vec()).ok_or(())
        }

        fn try_get_token_id(asset_id: T::TokenId) -> Result<Vec<u8>, Self::Err> {
            let token_result = Self::get_token_info(asset_id);
            match token_result {
                Some(XToken::NEP141(_, name, _, _, _)) => Ok(name),
                _ => Err(()),
            }
        }
    }

    /// used by the chainbridge
    impl<T: Config> fuso_support::chainbridge::AssetIdResourceIdProvider<T::TokenId> for Pallet<T> {
        type Err = ();

        fn try_get_asset_id(
            chain_id: ChainId,
            contract_id: impl AsRef<[u8]>,
        ) -> Result<T::TokenId, Self::Err> {
            Self::get_token_from_chainbridge((chain_id, contract_id.as_ref().to_vec())).ok_or(())
        }
    }

    impl<T: Config> BalanceConversion<BalanceOf<T>, T::TokenId, BalanceOf<T>> for Pallet<T>
    where
        BalanceOf<T>: From<u128> + Into<u128>,
    {
        type Error = DispatchError;

        fn to_asset_balance(
            balance: BalanceOf<T>,
            token_id: T::TokenId,
        ) -> Result<BalanceOf<T>, Self::Error> {
            if token_id == Self::native_token_id() {
                return Ok(balance);
            }
            Tokens::<T>::try_get(&token_id)
                .map(|info| match info {
                    XToken::NEP141(_, _, _, _, decimals)
                    | XToken::ERC20(_, _, _, _, decimals)
                    | XToken::POLYGON(_, _, _, _, decimals)
                    | XToken::BEP20(_, _, _, _, decimals) => {
                        Self::transform_decimals_to_standard(balance, decimals)
                    }
                    XToken::FND10(..) => balance,
                })
                .map_err(|_| Error::<T>::TokenNotFound.into())
        }
    }

    // TODO
    impl<T: Config> ChainIdOf<BalanceOf<T>> for Pallet<T> {
        fn chain_id_of(token_info: &XToken<BalanceOf<T>>) -> ChainId {
            match token_info {
                XToken::NEP141(..) => T::NearChainId::get(),
                XToken::ERC20(..) => T::EthChainId::get(),
                XToken::POLYGON(..) => T::PolygonChainId::get(),
                XToken::BEP20(..) => T::BnbChainId::get(),
                XToken::FND10(..) => T::NativeChainId::get(),
            }
        }
    }
}
