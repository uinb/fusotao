#![recursion_limit = "128"]
#![cfg_attr(not(feature = "std"), no_std)]
#![allow(dead_code)]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use codec::EncodeLike;
    use frame_support::{
        dispatch::GetDispatchInfo,
        pallet_prelude::*,
        traits::{fungibles::Mutate, tokens::BalanceConversion, EnsureOrigin, Get, StorageVersion},
        transactional,
    };
    use frame_system::{ensure_signed, pallet_prelude::*};
    use fuso_support::traits::DecimalsTransformer;
    use fuso_support::{
        chainbridge::*,
        traits::{Custody, PriceOracle, Token},
        ChainId,
    };
    use pallet_chainbridge as bridge;
    use pallet_fuso_verifier as verifier;
    use sp_core::U256;
    use sp_runtime::{
        traits::{Dispatchable, SaturatedConversion, Zero},
        Perquintill,
    };
    use sp_std::{convert::From, prelude::*};

    type Depositer = EthereumCompatibleAddress;
    type ContractAddress = EthereumCompatibleAddress;

    type AssetId<T> = <<T as bridge::Config>::Fungibles as Token<
        <T as frame_system::Config>::AccountId,
    >>::TokenId;

    type BalanceOf<T> = <<T as bridge::Config>::Fungibles as Token<
        <T as frame_system::Config>::AccountId,
    >>::Balance;

    type BalanceOfExternal<T> = <<T as bridge::Config>::Fungibles as Token<
        <T as frame_system::Config>::AccountId,
    >>::Balance;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    const QUINTILL: u128 = 1_000_000_000_000_000_000;

    #[pallet::pallet]
    #[pallet::generate_store(pub (super) trait Store)]
    #[pallet::without_storage_info]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config + bridge::Config + verifier::Config {
        /// The overarching event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Specifies the origin check provided by the bridge for calls that can only be called by
        /// the bridge pallet
        type BridgeOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Self::AccountId>;

        type BridgeAdminOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Self::AccountId>;

        type BalanceConversion: BalanceConversion<BalanceOf<Self>, AssetId<Self>, BalanceOf<Self>>;

        /// dispatchable call
        type Redirect: Parameter
            + Dispatchable<RuntimeOrigin = Self::RuntimeOrigin>
            + EncodeLike
            + GetDispatchInfo;

        type Oracle: PriceOracle<AssetId<Self>, BalanceOf<Self>, Self::BlockNumber>;

        /// Max native token value
        type NativeTokenMaxValue: Get<BalanceOf<Self>>;

        type DonorAccount: Get<Self::AccountId>;

        type DonationForAgent: Get<BalanceOf<Self>>;
    }

    #[pallet::storage]
    #[pallet::getter(fn native_check)]
    pub type NativeCheck<T> = StorageValue<_, bool, ValueQuery>;

    #[pallet::type_value]
    pub fn DefaultBridgingFee<T: Config>() -> u128 {
        QUINTILL * 2
    }

    #[pallet::storage]
    #[pallet::getter(fn bridging_fee)]
    pub type BridgingFeeInUSD<T> =
        StorageMap<_, Blake2_128Concat, ChainId, u128, ValueQuery, DefaultBridgingFee<T>>;

    #[pallet::storage]
    #[pallet::getter(fn associated_dominator)]
    pub type AssociatedDominator<T: Config> =
        StorageMap<_, Blake2_128Concat, u8, T::AccountId, OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub (super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// deposit assets
        Deposit {
            sender: T::AccountId,
            recipient: T::AccountId,
            resource_id: ResourceId,
            amount: BalanceOf<T>,
        },
        /// Withdraw assets
        Withdraw {
            sender: T::AccountId,
            recipient: Vec<u8>,
            resource_id: ResourceId,
            amount: BalanceOf<T>,
        },
        Remark(T::Hash),
    }

    #[pallet::error]
    pub enum Error<T> {
        InvalidTransfer,
        InvalidTokenId,
        InvalidResourceId,
        WrongAssetId,
        InvalidTokenName,
        OverTransferLimit,
        AssetAlreadyExists,
        InvalidCallMessage,
        RegisterAgentFailed,
        DepositerNotFound,
        PriceOverflow,
        LessThanBridgingThreshold,
        ResourceIdInvalid,
        FeeZero,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    #[pallet::call]
    impl<T: Config> Pallet<T>
    where
        <<T as verifier::Config>::Asset as Token<<T as frame_system::Config>::AccountId>>::Balance:
            From<u128> + Into<u128>,
        <<T as verifier::Config>::Asset as Token<<T as frame_system::Config>::AccountId>>::TokenId:
            Into<u32>,
        <T::Fungibles as Token<<T as frame_system::Config>::AccountId>>::Balance:
            From<u128> + Into<u128>,
        <T::Fungibles as Token<<T as frame_system::Config>::AccountId>>::TokenId: Into<u32>,
        <T as frame_system::Config>::BlockNumber: Into<u32>,
    {
        #[pallet::weight(195_000_0000)]
        pub fn native_limit(origin: OriginFor<T>, value: bool) -> DispatchResult {
            ensure_root(origin)?;
            <NativeCheck<T>>::put(value);
            Ok(())
        }

        /// Transfers some amount of the native token to some recipient on a (whitelisted)
        /// destination chain.
        #[pallet::weight(195_000_0000)]
        pub fn transfer_out(
            origin: OriginFor<T>,
            amount: BalanceOfExternal<T>,
            r_id: ResourceId,
            recipient: Vec<u8>,
            dest_id: ChainId,
        ) -> DispatchResult {
            let source = ensure_signed(origin)?;
            ensure!(
                <bridge::Pallet<T>>::chain_whitelisted(dest_id),
                <Error<T>>::InvalidTransfer
            );
            match Self::is_native_resource(r_id) {
                true => Self::do_lock(source, amount, r_id, recipient, dest_id)?,
                false => Self::do_burn_assets(source, amount, r_id, recipient, dest_id)?,
            }
            Ok(())
        }

        /// Executes a simple currency transfer using the bridge account as the source
        /// Triggered by a initial transfer on source chain, executed by relayer when proposal was
        /// resolved. this function by bridge triggered transfer
        #[pallet::weight(195_000_0000)]
        pub fn transfer_in(
            origin: OriginFor<T>,
            to: T::AccountId,
            amount: BalanceOf<T>,
            r_id: ResourceId,
        ) -> DispatchResult {
            let source = T::BridgeOrigin::ensure_origin(origin)?;
            match Self::is_native_resource(r_id) {
                true => {
                    Self::do_unlock(source, to.clone(), amount)?;
                    let (_, associated, _) =
                        decode_resource_id(r_id).map_err(|_| Error::<T>::InvalidResourceId)?;
                    match Self::associated_dominator(associated) {
                        Some(dominator) => {
                            let b: u128 = T::BalanceConversion::to_asset_balance(
                                amount,
                                T::Fungibles::native_token_id(),
                            )
                            .map_err(|_| Error::<T>::InvalidResourceId)?
                            .into();
                            if let Err(e) = verifier::Pallet::<T>::authorize_to(
                                to.clone(),
                                dominator,
                                <T as verifier::Config>::Asset::native_token_id(),
                                b.into(),
                            ) {
                                log::error!("failed to invoke authorize_to from {:?}, {:?}", to, e);
                            }
                        }
                        None => {}
                    }
                }
                false => {
                    Self::do_mint_assets(to.clone(), amount, r_id)?;
                    let (chain_id, associated, maybe_contract) =
                        decode_resource_id(r_id).map_err(|_| Error::<T>::InvalidResourceId)?;
                    match Self::associated_dominator(associated) {
                        Some(dominator) => {
                            let token =
                                T::AssetIdByName::try_get_asset_id(chain_id, maybe_contract)
                                    .map_err(|_| Error::<T>::InvalidResourceId)?;
                            let b: u128 = T::BalanceConversion::to_asset_balance(amount, token)
                                .map_err(|_| Error::<T>::InvalidResourceId)?
                                .into();
                            let t: u32 = token.into();
                            if let Err(e) = verifier::Pallet::<T>::authorize_to(
                                to.clone(),
                                dominator,
                                t.into(),
                                b.into(),
                            ) {
                                log::error!("failed to invoke authorize_to from {:?}, {:?}", to, e);
                            }
                        }
                        None => {}
                    }
                }
            }
            if frame_system::Pallet::<T>::account_nonce(&to) == Zero::zero() {
                let _ = T::Fungibles::transfer_token(
                    &T::DonorAccount::get(),
                    T::Fungibles::native_token_id(),
                    T::DonationForAgent::get(),
                    &to,
                );
            }
            Ok(())
        }

        #[pallet::weight(195_000_0000)]
        pub fn associate_dominator(
            origin: OriginFor<T>,
            associate_id: u8,
            dominator_account: T::AccountId,
        ) -> DispatchResult {
            let _ = Self::ensure_admin(origin)?;
            AssociatedDominator::<T>::insert(associate_id, dominator_account);
            Ok(())
        }

        /// This can be called by the bridge to demonstrate an arbitrary call from a proposal.
        #[pallet::weight(195_000_0000)]
        pub fn remark(
            origin: OriginFor<T>,
            _message: Vec<u8>,
            _depositer: Depositer,
            _r_id: ResourceId,
        ) -> DispatchResult {
            T::BridgeOrigin::ensure_origin(origin)?;
            Ok(())
        }

        #[pallet::weight(195_000_0000)]
        pub fn update_bridge_fee(
            origin: OriginFor<T>,
            chain_id: ChainId,
            fee: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let _ = T::BridgeAdminOrigin::ensure_origin(origin)?;
            ensure!(fee > 0.into(), Error::<T>::FeeZero);
            BridgingFeeInUSD::<T>::insert(chain_id, fee.into());
            Ok(().into())
        }
    }

    impl<T: Config> Pallet<T>
    where
        BalanceOf<T>: Into<u128> + From<u128>,
    {
        fn is_native_resource(r_id: ResourceId) -> bool {
            if let Ok((origin, _, p)) = decode_resource_id(r_id) {
                if let Ok((native, _, p0)) = decode_resource_id(T::NativeResourceId::get()) {
                    return native == origin && p == p0;
                }
            }
            false
        }

        pub fn ensure_admin(o: T::RuntimeOrigin) -> DispatchResult {
            T::AdminOrigin::ensure_origin(o)?;
            Ok(().into())
        }

        pub(crate) fn set_associated_dominator(idx: u8, dominator: T::AccountId) {
            AssociatedDominator::<T>::insert(idx, dominator);
        }

        pub(crate) fn calculate_bridging_fee(
            dest_chain_id: ChainId,
            token_id: &AssetId<T>,
        ) -> BalanceOf<T> {
            let price: u128 = T::Oracle::get_price(&token_id).into();
            // the oracle is not working
            if price.is_zero() {
                Zero::zero()
            } else {
                let usd = Self::bridging_fee(dest_chain_id);
                (usd / price * QUINTILL
                    + Perquintill::from_rational::<u128>(usd % price, price).deconstruct() as u128)
                    .into()
            }
        }

        #[transactional]
        pub(crate) fn do_lock(
            sender: T::AccountId,
            amount: BalanceOf<T>,
            r_id: ResourceId,
            recipient: Vec<u8>,
            dest_id: ChainId,
        ) -> DispatchResult {
            log::info!("transfer native token");
            let bridge_id = bridge::Pallet::<T>::account_id();
            let native_token_id = T::Fungibles::native_token_id();
            if NativeCheck::<T>::get() {
                let free_balance = T::Fungibles::free_balance(&native_token_id, &bridge_id);
                let total_balance = free_balance + amount;
                let right_balance = T::NativeTokenMaxValue::get() / 3u8.into();
                if total_balance > right_balance {
                    return Err(Error::<T>::OverTransferLimit)?;
                }
            }
            let fee = Self::calculate_bridging_fee(dest_id, &native_token_id);
            ensure!(amount > fee + fee, Error::<T>::LessThanBridgingThreshold);
            T::Fungibles::transfer_token(
                &sender,
                native_token_id,
                fee,
                &T::TreasuryAccount::get(),
            )?;
            T::Fungibles::transfer_token(&sender, native_token_id, amount - fee, &bridge_id)?;
            log::info!("transfer native token successful");
            bridge::Pallet::<T>::transfer_fungible(
                dest_id,
                r_id,
                recipient.clone(),
                U256::from((amount - fee).saturated_into::<u128>()),
            )?;
            Ok(())
        }

        pub(crate) fn do_unlock(
            sender: T::AccountId,
            to: T::AccountId,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            let native_token_id = T::Fungibles::native_token_id();
            T::Fungibles::transfer_token(&sender, native_token_id, amount, &to)?;
            Ok(())
        }

        #[transactional]
        pub(crate) fn do_burn_assets(
            who: T::AccountId,
            amount: BalanceOfExternal<T>,
            r_id: ResourceId,
            recipient: Vec<u8>,
            dest_id: ChainId,
        ) -> DispatchResult {
            let (chain_id, _, maybe_contract) =
                decode_resource_id(r_id).map_err(|_| Error::<T>::InvalidResourceId)?;
            let token_id = T::AssetIdByName::try_get_asset_id(chain_id, maybe_contract)
                .map_err(|_| Error::<T>::InvalidResourceId)?;
            let external_decimals = T::Fungibles::token_external_decimals(&token_id)?;
            let standard_ammount =
                T::Fungibles::transform_decimals_to_standard(amount, external_decimals);
            let fee = Self::calculate_bridging_fee(dest_id, &token_id);
            ensure!(
                standard_ammount > fee + fee,
                Error::<T>::LessThanBridgingThreshold
            );

            let standard_bridge_amount = standard_ammount - fee;
            let external_bridge_amount = T::Fungibles::transform_decimals_to_external(
                standard_bridge_amount,
                external_decimals,
            );
            let standard_bridge_amount = T::Fungibles::transform_decimals_to_standard(
                external_bridge_amount,
                external_decimals,
            );
            let actual_fee = standard_ammount - standard_bridge_amount;
            T::Fungibles::transfer_token(&who, token_id, actual_fee, &T::TreasuryAccount::get())?;
            T::Fungibles::burn_from(token_id, &who, external_bridge_amount)?;
            bridge::Pallet::<T>::transfer_fungible(
                dest_id,
                r_id,
                recipient.clone(),
                U256::from(external_bridge_amount.into()),
            )?;
            Ok(())
        }

        pub(crate) fn do_mint_assets(
            who: T::AccountId,
            amount: BalanceOf<T>,
            r_id: ResourceId,
        ) -> DispatchResult {
            let (chain_id, _, maybe_contract) =
                decode_resource_id(r_id).map_err(|_| Error::<T>::InvalidResourceId)?;
            let token_id = T::AssetIdByName::try_get_asset_id(chain_id, maybe_contract)
                .map_err(|_| Error::<T>::InvalidResourceId)?;
            T::Fungibles::mint_into(token_id, &who, amount)?;
            Ok(())
        }
    }
}
