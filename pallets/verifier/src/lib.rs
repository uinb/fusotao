// Copyright 2021-2023 UINB Technologies Pte. Ltd.

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
pub use pallet::*;
#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
#[cfg(test)]
pub mod mock;
#[cfg(test)]
pub mod tests;
pub mod weights;

#[frame_support::pallet]
pub mod pallet {
    use crate::weights::WeightInfo;
    use ascii::AsciiStr;
    use codec::{Compact, Decode, Encode, EncodeLike};
    use frame_support::{
        dispatch::{Dispatchable, GetDispatchInfo, PostDispatchInfo},
        weights::constants::RocksDbWeight,
        {pallet_prelude::*, transactional},
    };
    use frame_system::pallet_prelude::*;
    use fuso_support::{
        constants::*,
        traits::{FeeBeneficiary, MarketManager, PriceOracle, ReservableToken, Rewarding, Token},
    };
    use scale_info::TypeInfo;
    use sp_core::sr25519::{Public as Sr25519Public, Signature as Sr25519Signature};
    use sp_io::hashing::blake2_256 as hashing;
    use sp_runtime::{
        traits::{
            AccountIdConversion, CheckedAdd, CheckedSub, StaticLookup, TrailingZeroInput, Zero,
        },
        Permill, Perquintill, RuntimeDebug,
    };
    use sp_std::{
        collections::btree_map::BTreeMap, convert::*, prelude::*, result::Result, vec::Vec,
    };

    pub type TokenId<T> =
        <<T as Config>::Asset as Token<<T as frame_system::Config>::AccountId>>::TokenId;
    pub type Balance<T> =
        <<T as Config>::Asset as Token<<T as frame_system::Config>::AccountId>>::Balance;
    pub type Symbol<T> = (TokenId<T>, TokenId<T>);
    pub type Season = u32;
    pub type Amount = u128;
    pub type MerkleHash = [u8; 32];

    pub const PALLET_ID: frame_support::PalletId = frame_support::PalletId(*b"fuso/vrf");
    const UNSTAKE_DELAY_BLOCKS: u32 = 14400 * 4u32;
    const MAX_PROOF_SIZE: usize = 10 * 1024 * 1024usize;

    #[derive(Clone, Eq, PartialEq, RuntimeDebug)]
    pub struct Trade<TokenId, Balance> {
        pub token_id: TokenId,
        pub root: MerkleHash,
        pub amount: Balance,
        pub vol: Balance,
    }

    #[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo)]
    pub struct MerkleLeaf {
        pub key: Vec<u8>,
        pub old_v: MerkleHash,
        pub new_v: MerkleHash,
    }

    impl MerkleLeaf {
        const ACCOUNT_KEY: u8 = 0x00;
        const BESTPRICE_KEY: u8 = 0x02;
        const ORDERBOOK_KEY: u8 = 0x01;
        const ORDERPAGE_KEY: u8 = 0x03;

        fn try_get_account<T: Config>(&self) -> Result<(u32, T::AccountId), Error<T>> {
            if self.key.len() != 37 {
                return Err(Error::<T>::ProofsUnsatisfied);
            }
            match self.key[0] {
                Self::ACCOUNT_KEY => Ok((
                    u32::from_le_bytes(
                        self.key[33..]
                            .try_into()
                            .map_err(|_| Error::<T>::ProofsUnsatisfied)?,
                    ),
                    T::AccountId::decode(&mut &self.key[1..33])
                        .map_err(|_| Error::<T>::ProofsUnsatisfied)?,
                )),
                _ => Err(Error::<T>::ProofsUnsatisfied),
            }
        }

        fn try_get_symbol<T: Config>(&self) -> Result<(u32, u32), Error<T>> {
            if self.key.len() != 9 {
                return Err(Error::<T>::ProofsUnsatisfied);
            }
            match self.key[0] {
                Self::ORDERBOOK_KEY | Self::BESTPRICE_KEY => Ok((
                    u32::from_le_bytes(
                        self.key[1..5]
                            .try_into()
                            .map_err(|_| Error::<T>::ProofsUnsatisfied)?,
                    ),
                    u32::from_le_bytes(
                        self.key[5..]
                            .try_into()
                            .map_err(|_| Error::<T>::ProofsUnsatisfied)?,
                    ),
                )),
                _ => Err(Error::<T>::ProofsUnsatisfied),
            }
        }

        fn try_get_orderpage<T: Config>(&self) -> Result<(u32, u32, u128), Error<T>> {
            if self.key.len() != 25 {
                return Err(Error::<T>::ProofsUnsatisfied);
            }
            match self.key[0] {
                Self::ORDERPAGE_KEY => Ok((
                    u32::from_le_bytes(
                        self.key[1..5]
                            .try_into()
                            .map_err(|_| Error::<T>::ProofsUnsatisfied)?,
                    ),
                    u32::from_le_bytes(
                        self.key[5..9]
                            .try_into()
                            .map_err(|_| Error::<T>::ProofsUnsatisfied)?,
                    ),
                    u128::from_le_bytes(
                        self.key[9..]
                            .try_into()
                            .map_err(|_| Error::<T>::ProofsUnsatisfied)?,
                    ),
                )),
                _ => Err(Error::<T>::ProofsUnsatisfied),
            }
        }

        fn split_value(v: &[u8; 32]) -> ([u8; 16], [u8; 16]) {
            (v[..16].try_into().unwrap(), v[16..].try_into().unwrap())
        }

        fn split_old_to_u128(&self) -> (u128, u128) {
            let (l, r) = Self::split_value(&self.old_v);
            (u128::from_le_bytes(l), u128::from_le_bytes(r))
        }

        fn split_old_to_sum(&self) -> u128 {
            let (l, r) = self.split_old_to_u128();
            l + r
        }

        fn split_new_to_u128(&self) -> (u128, u128) {
            let (l, r) = Self::split_value(&self.new_v);
            (u128::from_le_bytes(l), u128::from_le_bytes(r))
        }

        fn split_new_to_sum(&self) -> u128 {
            let (l, r) = self.split_new_to_u128();
            l + r
        }
    }

    #[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo)]
    pub enum Command {
        // price, amount, maker_fee, taker_fee, base, quote
        AskLimit(
            Compact<u128>,
            Compact<u128>,
            Compact<u32>,
            Compact<u32>,
            Compact<u32>,
            Compact<u32>,
        ),
        BidLimit(
            Compact<u128>,
            Compact<u128>,
            Compact<u32>,
            Compact<u32>,
            Compact<u32>,
            Compact<u32>,
        ),
        Cancel(Compact<u32>, Compact<u32>),
        TransferOut(Compact<u32>, Compact<u128>),
        TransferIn(Compact<u32>, Compact<u128>),
        RejectTransferOut(Compact<u32>, Compact<u128>),
        RejectTransferIn,
    }

    #[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo)]
    pub enum CommandV2<AccountId> {
        AskLimit {
            price: Compact<u128>,
            amount: Compact<u128>,
            maker_fee: Compact<u32>,
            taker_fee: Compact<u32>,
            base: Compact<u32>,
            quote: Compact<u32>,
            broker: Option<AccountId>,
        },
        BidLimit {
            price: Compact<u128>,
            amount: Compact<u128>,
            maker_fee: Compact<u32>,
            taker_fee: Compact<u32>,
            base: Compact<u32>,
            quote: Compact<u32>,
            broker: Option<AccountId>,
        },
        Cancel {
            base: Compact<u32>,
            quote: Compact<u32>,
        },
        TransferOut {
            currency: Compact<u32>,
            amount: Compact<u128>,
        },
        TransferIn {
            currency: Compact<u32>,
            amount: Compact<u128>,
        },
        RejectTransferOut {
            currency: Compact<u32>,
            amount: Compact<u128>,
        },
        RejectTransferIn,
    }

    impl<AccountId> From<Command> for CommandV2<AccountId> {
        fn from(cmd: Command) -> Self {
            match cmd {
                Command::AskLimit(price, amount, maker_fee, taker_fee, base, quote) => {
                    CommandV2::AskLimit {
                        price,
                        amount,
                        maker_fee,
                        taker_fee,
                        base,
                        quote,
                        broker: None,
                    }
                }
                Command::BidLimit(price, amount, maker_fee, taker_fee, base, quote) => {
                    CommandV2::BidLimit {
                        price,
                        amount,
                        maker_fee,
                        taker_fee,
                        base,
                        quote,
                        broker: None,
                    }
                }
                Command::Cancel(base, quote) => CommandV2::Cancel { base, quote },
                Command::TransferOut(currency, amount) => {
                    CommandV2::TransferOut { currency, amount }
                }
                Command::TransferIn(currency, amount) => CommandV2::TransferIn { currency, amount },
                Command::RejectTransferOut(currency, amount) => {
                    CommandV2::RejectTransferOut { currency, amount }
                }
                Command::RejectTransferIn => CommandV2::RejectTransferIn,
            }
        }
    }

    #[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo)]
    pub struct Proof<AccountId> {
        pub event_id: u64,
        pub user_id: AccountId,
        pub cmd: Command,
        pub leaves: Vec<MerkleLeaf>,
        pub maker_page_delta: u8,
        pub maker_account_delta: u8,
        pub merkle_proof: Vec<u8>,
        pub root: MerkleHash,
    }

    #[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo)]
    pub struct ProofV2<AccountId> {
        pub event_id: u64,
        pub user_id: AccountId,
        pub cmd: CommandV2<AccountId>,
        pub leaves: Vec<MerkleLeaf>,
        pub maker_page_delta: u8,
        pub maker_account_delta: u8,
        pub merkle_proof: Vec<u8>,
        pub root: MerkleHash,
    }

    impl<AccountId> From<Proof<AccountId>> for ProofV2<AccountId> {
        fn from(proof: Proof<AccountId>) -> Self {
            Self {
                event_id: proof.event_id,
                user_id: proof.user_id,
                cmd: proof.cmd.into(),
                leaves: proof.leaves,
                maker_page_delta: proof.maker_page_delta,
                maker_account_delta: proof.maker_account_delta,
                merkle_proof: proof.merkle_proof,
                root: proof.root,
            }
        }
    }

    #[derive(Clone, Encode, Decode, RuntimeDebug, Eq, PartialEq, TypeInfo)]
    pub enum Receipt<TokenId, Balance, BlockNumber, Callback> {
        Authorize(TokenId, Balance, BlockNumber),
        Revoke(TokenId, Balance, BlockNumber),
        RevokeWithCallback(TokenId, Balance, BlockNumber, Callback),
    }

    #[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
    pub struct Dominator<Balance, BlockNumber> {
        pub name: Vec<u8>,
        pub staked: Balance,
        pub merkle_root: MerkleHash,
        pub start_from: BlockNumber,
        pub sequence: (u64, BlockNumber),
        pub status: u8,
    }

    #[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, Default)]
    pub struct Staking<Balance> {
        from_season: Season,
        amount: Balance,
    }

    #[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, Default)]
    pub struct Bonus<TokenId, Balance> {
        pub staked: Balance,
        pub profit: BTreeMap<TokenId, Balance>,
    }

    #[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, Default)]
    pub struct DominatorSetting<AccountId> {
        pub beneficiary: Option<AccountId>,
        pub x25519_pubkey: Vec<u8>,
        pub rpc_endpoint: Vec<u8>,
    }

    #[derive(Clone, RuntimeDebug)]
    struct Distribution<T: Config> {
        from_season: Season,
        to_season: Season,
        staking: Balance<T>,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Asset: ReservableToken<Self::AccountId>;

        type Rewarding: Rewarding<Self::AccountId, Balance<Self>, Symbol<Self>, Self::BlockNumber>;

        type WeightInfo: WeightInfo;

        type Callback: Parameter
            + Dispatchable<RuntimeOrigin = Self::RuntimeOrigin, PostInfo = PostDispatchInfo>
            + EncodeLike
            + GetDispatchInfo;

        type Indicator: PriceOracle<TokenId<Self>, Balance<Self>, Self::BlockNumber>;

        type MarketManager: MarketManager<
            Self::AccountId,
            TokenId<Self>,
            Balance<Self>,
            Self::BlockNumber,
        >;

        type BrokerBeneficiary: FeeBeneficiary<Self::AccountId>;

        #[pallet::constant]
        type DominatorOnlineThreshold: Get<Balance<Self>>;

        #[pallet::constant]
        type SeasonDuration: Get<Self::BlockNumber>;

        /// the SeasonDuration must be 1 * period, 2 * period, 3 * period...
        #[pallet::constant]
        type DominatorCheckGracePeriod: Get<Self::BlockNumber>;

        #[pallet::constant]
        type MinimalStakingAmount: Get<Balance<Self>>;

        #[pallet::constant]
        type MaxMakerFee: Get<u32>;

        #[pallet::constant]
        type MaxTakerFee: Get<u32>;
    }

    #[pallet::storage]
    #[pallet::getter(fn receipts)]
    pub type Receipts<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        T::AccountId,
        Receipt<TokenId<T>, Balance<T>, T::BlockNumber, T::Callback>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn dominators)]
    pub type Dominators<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Dominator<Balance<T>, T::BlockNumber>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn dominator_settings)]
    pub type DominatorSettings<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, DominatorSetting<T::AccountId>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn reserves)]
    pub type Reserves<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        (u8, T::AccountId, TokenId<T>),
        Blake2_128Concat,
        T::AccountId,
        Balance<T>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn bonuses)]
    pub type Bonuses<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        Season,
        Bonus<TokenId<T>, Balance<T>>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn stakings)]
    pub type Stakings<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        T::AccountId,
        Staking<Balance<T>>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn pending_unstakings)]
    pub type PendingUnstakings<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::BlockNumber,
        Blake2_128Concat,
        T::AccountId,
        Balance<T>,
        ValueQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub (super) fn deposit_event)]
    pub enum Event<T: Config> {
        DominatorClaimed(T::AccountId),
        // DEPREACATED
        CoinHosted(T::AccountId, T::AccountId, Balance<T>),
        TokenHosted(T::AccountId, T::AccountId, TokenId<T>, Balance<T>),
        // DEPREACATED
        CoinRevoked(T::AccountId, T::AccountId, Balance<T>),
        TokenRevoked(T::AccountId, T::AccountId, TokenId<T>, Balance<T>),
        ProofAccepted(T::AccountId, u32),
        ProofRejected(T::AccountId, u32),
        TaoStaked(T::AccountId, T::AccountId, Balance<T>),
        TaoUnstaked(T::AccountId, T::AccountId, Balance<T>),
        TaoUnstakeUnlock(T::AccountId, Balance<T>),
        DominatorOnline(T::AccountId),
        DominatorOffline(T::AccountId),
        DominatorSlashed(T::AccountId),
        DominatorEvicted(T::AccountId),
        DominatorInactive(T::AccountId),
        DominatorX25519KeyUpdated(T::AccountId, Vec<u8>),
        DominatorRpcEndpointUpdated(T::AccountId, Vec<u8>),
    }

    #[pallet::error]
    pub enum Error<T> {
        DominatorNotFound,
        ProofsUnsatisfied,
        IllegalParameters,
        ReceiptNotExists,
        ChainNotSupport,
        ReceiptAlreadyExists,
        DominatorAlreadyExists,
        TooEarlyToRegister,
        DominatorInactive,
        InsufficientBalance,
        Overflow,
        InsufficientStakingAmount,
        InvalidName,
        InvalidStatus,
        InvalidStaking,
        StakingNotExists,
        DistributionOngoing,
        LittleStakingAmount,
        UnsupportedQuoteCurrency,
        DominatorEvicted,
        DominatorStatusInvalid,
        FeesTooHigh,
        ProofDecompressError,
        ProofFormatError,
        ProofTooLarge,
        InvalidBeneficiaryProof,
    }

    #[pallet::pallet]
    #[pallet::without_storage_info]
    #[pallet::generate_store(pub (super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
    where
        TokenId<T>: Copy + From<u32> + Into<u32>,
        Balance<T>: Copy + From<u128> + Into<u128>,
        T::BlockNumber: Into<u32> + From<u32>,
    {
        /// save total staking of previous season
        fn on_initialize(now: T::BlockNumber) -> Weight {
            let mut weight: Weight = Weight::from_ref_time(0u64);
            if now % T::DominatorCheckGracePeriod::get() != Zero::zero() {
                return weight;
            }
            for (id, dominator) in Dominators::<T>::iter() {
                let start = dominator.start_from;
                if now == start {
                    continue;
                }
                weight = weight.saturating_add(RocksDbWeight::get().reads(1u64));
                if (now - start) % T::SeasonDuration::get() == Zero::zero() {
                    let prv_season = ((now - start) / T::SeasonDuration::get()).into() - 1;
                    Bonuses::<T>::mutate(id, prv_season, |b| b.staked = dominator.staked);
                    weight = weight.saturating_add(RocksDbWeight::get().writes(1u64))
                }
            }
            for (staker, amount) in PendingUnstakings::<T>::drain_prefix(&now) {
                let r = Self::unreserve(
                    RESERVE_FOR_PENDING_UNSTAKE,
                    staker.clone(),
                    T::Asset::native_token_id(),
                    amount,
                    &Self::system_account(),
                );
                weight = weight.saturating_add(RocksDbWeight::get().writes(2u64));
                if r.is_err() {
                    log::error!(
                        "No enough tokens of {:?} to unlock, check onchain storage.",
                        staker
                    );
                } else {
                    Self::deposit_event(Event::TaoUnstakeUnlock(staker.clone(), amount.clone()));
                }
            }
            weight.saturating_add(RocksDbWeight::get().writes(1u64))
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T>
    where
        TokenId<T>: Copy + From<u32> + Into<u32>,
        Balance<T>: Copy + From<u128> + Into<u128>,
        T::BlockNumber: Into<u32> + From<u32>,
    {
        /// Initialize an empty sparse merkle tree with sequence 0 for a new dominator.
        #[pallet::weight(<T as Config>::WeightInfo::register())]
        pub fn register(origin: OriginFor<T>, identifier: Vec<u8>) -> DispatchResultWithPostInfo {
            let dominator = ensure_signed(origin)?;
            let name = AsciiStr::from_ascii(&identifier);
            ensure!(name.is_ok(), Error::<T>::InvalidName);
            let name = name.unwrap();
            ensure!(name.len() >= 2 && name.len() <= 32, Error::<T>::InvalidName);
            let current_block = frame_system::Pallet::<T>::block_number();
            ensure!(
                current_block >= T::DominatorCheckGracePeriod::get(),
                Error::<T>::TooEarlyToRegister
            );
            ensure!(
                !Dominators::<T>::contains_key(&dominator),
                Error::<T>::DominatorAlreadyExists
            );
            ensure!(
                Dominators::<T>::iter()
                    .find(|d| &d.1.name == &identifier)
                    .is_none(),
                Error::<T>::InvalidName
            );
            let register_at = current_block - current_block % T::DominatorCheckGracePeriod::get();
            Dominators::<T>::insert(
                &dominator,
                Dominator {
                    name: identifier,
                    staked: Zero::zero(),
                    start_from: register_at,
                    sequence: (0, current_block),
                    merkle_root: Default::default(),
                    status: DOMINATOR_REGISTERED,
                },
            );
            Self::deposit_event(Event::DominatorClaimed(dominator));
            Ok(().into())
        }

        #[pallet::weight(<T as Config>::WeightInfo::evict())]
        pub fn evict(
            origin: OriginFor<T>,
            dominator_id: <T::Lookup as StaticLookup>::Source,
        ) -> DispatchResultWithPostInfo {
            let _ = ensure_root(origin)?;
            let dominator = T::Lookup::lookup(dominator_id)?;
            Dominators::<T>::try_mutate_exists(&dominator, |d| -> DispatchResult {
                ensure!(d.is_some(), Error::<T>::DominatorNotFound);
                let mut dominator = d.take().unwrap();
                dominator.status = DOMINATOR_EVICTED;
                d.replace(dominator);
                Ok(())
            })?;
            Self::deposit_event(Event::DominatorEvicted(dominator));
            Ok(().into())
        }

        #[pallet::weight(<T as Config>::WeightInfo::launch())]
        pub fn launch(
            origin: OriginFor<T>,
            dominator_id: <T::Lookup as StaticLookup>::Source,
        ) -> DispatchResultWithPostInfo {
            let _ = ensure_root(origin)?;
            let dominator = T::Lookup::lookup(dominator_id)?;
            Dominators::<T>::try_mutate_exists(&dominator, |d| -> DispatchResult {
                ensure!(d.is_some(), Error::<T>::DominatorNotFound);
                let mut dominator = d.take().unwrap();
                ensure!(
                    dominator.status == DOMINATOR_REGISTERED,
                    Error::<T>::DominatorStatusInvalid
                );
                dominator.status = DOMINATOR_INACTIVE;
                d.replace(dominator);
                Ok(())
            })?;
            Self::deposit_event(Event::DominatorInactive(dominator));
            Ok(().into())
        }

        #[pallet::weight(<T as Config>::WeightInfo::update_setting())]
        pub fn dominator_set_pubkey(
            origin: OriginFor<T>,
            key: Vec<u8>,
        ) -> DispatchResultWithPostInfo {
            let dominator = ensure_signed(origin)?;
            ensure!(
                Dominators::<T>::contains_key(&dominator),
                Error::<T>::DominatorNotFound
            );
            DominatorSettings::<T>::try_mutate(&dominator, |d| -> DispatchResult {
                match d {
                    Some(setting) => {
                        setting.x25519_pubkey = key.clone();
                    }
                    None => {
                        d.replace(DominatorSetting {
                            beneficiary: None,
                            x25519_pubkey: key.clone(),
                            rpc_endpoint: vec![],
                        });
                    }
                }
                Ok(())
            })?;
            Self::deposit_event(Event::DominatorX25519KeyUpdated(dominator, key));
            Ok(().into())
        }

        #[pallet::weight(<T as Config>::WeightInfo::update_setting())]
        pub fn dominator_set_rpc_endpoint(
            origin: OriginFor<T>,
            rpc_endpoint: Vec<u8>,
        ) -> DispatchResultWithPostInfo {
            let dominator = ensure_signed(origin)?;
            ensure!(
                Dominators::<T>::contains_key(&dominator),
                Error::<T>::DominatorNotFound
            );
            DominatorSettings::<T>::try_mutate(&dominator, |d| -> DispatchResult {
                match d {
                    Some(setting) => {
                        setting.rpc_endpoint = rpc_endpoint.clone();
                    }
                    None => {
                        d.replace(DominatorSetting {
                            beneficiary: None,
                            x25519_pubkey: vec![],
                            rpc_endpoint: rpc_endpoint.clone(),
                        });
                    }
                }
                Ok(())
            })?;
            Self::deposit_event(Event::DominatorRpcEndpointUpdated(dominator, rpc_endpoint));
            Ok(().into())
        }

        #[pallet::weight(<T as Config>::WeightInfo::set_beneficiary())]
        pub fn dominator_set_beneficiary(
            origin: OriginFor<T>,
            beneficiary: T::AccountId,
            proof: Sr25519Signature,
        ) -> DispatchResultWithPostInfo {
            let dominator = ensure_signed(origin)?;
            ensure!(
                Dominators::<T>::contains_key(&dominator),
                Error::<T>::DominatorNotFound
            );
            DominatorSettings::<T>::try_mutate(&dominator, |d| -> DispatchResult {
                match d {
                    Some(setting) => {
                        Self::validate_beneficiary(
                            setting.beneficiary.clone(),
                            proof,
                            beneficiary.clone(),
                        )?;
                        setting.beneficiary = Some(beneficiary);
                    }
                    None => {
                        d.replace(DominatorSetting {
                            beneficiary: Some(beneficiary.clone()),
                            x25519_pubkey: vec![],
                            rpc_endpoint: vec![],
                        });
                    }
                }
                Ok(())
            })?;
            Ok(().into())
        }

        #[pallet::weight((<T as Config>::WeightInfo::verify(), DispatchClass::Normal, Pays::No))]
        pub fn verify_compress(
            origin: OriginFor<T>,
            compressed_proofs: Vec<u8>,
        ) -> DispatchResultWithPostInfo {
            let dominator_id = ensure_signed(origin)?;
            let dominator = Dominators::<T>::try_get(&dominator_id)
                .map_err(|_| Error::<T>::DominatorNotFound)?;
            ensure!(
                dominator.status == DOMINATOR_ACTIVE,
                Error::<T>::DominatorInactive
            );
            let (uncompress_size, input) =
                lz4_flex::block::uncompressed_size(compressed_proofs.as_ref())
                    .map_err(|_| Error::<T>::ProofDecompressError)?;
            ensure!(uncompress_size < MAX_PROOF_SIZE, Error::<T>::ProofTooLarge);
            let uncompress_proofs = lz4_flex::decompress(input, uncompress_size)
                .map_err(|_| Error::<T>::ProofDecompressError)?;
            let proofs: Vec<Proof<T::AccountId>> =
                Decode::decode(&mut TrailingZeroInput::new(uncompress_proofs.as_ref()))
                    .map_err(|_| Error::<T>::ProofFormatError)?;
            let proofs: Vec<ProofV2<T::AccountId>> =
                proofs.into_iter().map(|p| p.into()).collect::<Vec<_>>();
            let beneficiary: T::AccountId = Self::dominator_settings(&dominator_id)
                .map(|setting| setting.beneficiary)
                .flatten()
                .unwrap_or(dominator_id.clone());
            Self::verify_batch(dominator_id, beneficiary, &dominator, proofs)
        }

        #[pallet::weight((<T as Config>::WeightInfo::verify(), DispatchClass::Normal, Pays::No))]
        pub fn verify(
            origin: OriginFor<T>,
            proofs: Vec<Proof<T::AccountId>>,
        ) -> DispatchResultWithPostInfo {
            let dominator_id = ensure_signed(origin)?;
            let dominator = Dominators::<T>::try_get(&dominator_id)
                .map_err(|_| Error::<T>::DominatorNotFound)?;
            ensure!(
                dominator.status == DOMINATOR_ACTIVE,
                Error::<T>::DominatorInactive
            );
            let proofs: Vec<ProofV2<T::AccountId>> =
                proofs.into_iter().map(|p| p.into()).collect::<Vec<_>>();
            let beneficiary: T::AccountId = Self::dominator_settings(&dominator_id)
                .map(|setting| setting.beneficiary)
                .flatten()
                .unwrap_or(dominator_id.clone());
            Self::verify_batch(dominator_id, beneficiary, &dominator, proofs)
        }

        #[pallet::weight((<T as Config>::WeightInfo::verify(), DispatchClass::Normal, Pays::No))]
        pub fn verify_compress_v2(
            origin: OriginFor<T>,
            compressed_proofs: Vec<u8>,
        ) -> DispatchResultWithPostInfo {
            let dominator_id = ensure_signed(origin)?;
            let dominator = Dominators::<T>::try_get(&dominator_id)
                .map_err(|_| Error::<T>::DominatorNotFound)?;
            ensure!(
                dominator.status == DOMINATOR_ACTIVE,
                Error::<T>::DominatorInactive
            );
            let (uncompress_size, input) =
                lz4_flex::block::uncompressed_size(compressed_proofs.as_ref())
                    .map_err(|_| Error::<T>::ProofDecompressError)?;
            ensure!(uncompress_size < MAX_PROOF_SIZE, Error::<T>::ProofTooLarge);
            let uncompress_proofs = lz4_flex::decompress(input, uncompress_size)
                .map_err(|_| Error::<T>::ProofDecompressError)?;
            let proofs: Vec<ProofV2<T::AccountId>> =
                Decode::decode(&mut TrailingZeroInput::new(uncompress_proofs.as_ref()))
                    .map_err(|_| Error::<T>::ProofFormatError)?;
            let beneficiary: T::AccountId = Self::dominator_settings(&dominator_id)
                .map(|setting| setting.beneficiary)
                .flatten()
                .unwrap_or(dominator_id.clone());
            Self::verify_batch(dominator_id, beneficiary, &dominator, proofs)
        }

        #[pallet::weight((<T as Config>::WeightInfo::verify(), DispatchClass::Normal, Pays::No))]
        pub fn verify_v2(
            origin: OriginFor<T>,
            proofs: Vec<ProofV2<T::AccountId>>,
        ) -> DispatchResultWithPostInfo {
            let dominator_id = ensure_signed(origin)?;
            let dominator = Dominators::<T>::try_get(&dominator_id)
                .map_err(|_| Error::<T>::DominatorNotFound)?;
            ensure!(
                dominator.status == DOMINATOR_ACTIVE,
                Error::<T>::DominatorInactive
            );
            let beneficiary: T::AccountId = Self::dominator_settings(&dominator_id)
                .map(|setting| setting.beneficiary)
                .flatten()
                .unwrap_or(dominator_id.clone());
            Self::verify_batch(dominator_id, beneficiary, &dominator, proofs)
        }

        #[transactional]
        #[pallet::weight(<T as Config>::WeightInfo::stake())]
        pub fn stake(
            origin: OriginFor<T>,
            dominator: <T::Lookup as StaticLookup>::Source,
            amount: Balance<T>,
        ) -> DispatchResultWithPostInfo {
            let staker = ensure_signed(origin)?;
            let dominator = T::Lookup::lookup(dominator)?;
            Self::stake_on(&staker, &dominator, amount)?;
            Ok(().into())
        }

        #[transactional]
        #[pallet::weight(<T as Config>::WeightInfo::unstake())]
        pub fn unstake(
            origin: OriginFor<T>,
            dominator: <T::Lookup as StaticLookup>::Source,
            amount: Balance<T>,
        ) -> DispatchResultWithPostInfo {
            let staker = ensure_signed(origin)?;
            let dominator = T::Lookup::lookup(dominator)?;
            Self::unstake_from(&staker, &dominator, amount)?;
            Ok(().into())
        }

        #[transactional]
        #[pallet::weight(<T as Config>::WeightInfo::claim_shares())]
        pub fn claim_shares(
            origin: OriginFor<T>,
            dominator: <T::Lookup as StaticLookup>::Source,
        ) -> DispatchResultWithPostInfo {
            let signer = ensure_signed(origin)?;
            let dex = T::Lookup::lookup(dominator)?;
            let dominator =
                Dominators::<T>::try_get(&dex).map_err(|_| Error::<T>::DominatorNotFound)?;
            ensure!(
                dominator.status != DOMINATOR_REGISTERED,
                Error::<T>::DominatorStatusInvalid
            );
            let staking =
                Stakings::<T>::try_get(&dex, &signer).map_err(|_| Error::<T>::InvalidStaking)?;
            let current_block = frame_system::Pallet::<T>::block_number();
            let current_season = Self::current_season(current_block, dominator.start_from);
            let distribution = Distribution {
                from_season: staking.from_season,
                to_season: current_season,
                staking: staking.amount,
            };
            Stakings::<T>::try_mutate(&dex, &signer, |s| -> DispatchResult {
                Ok(s.from_season = Self::take_shares(&signer, &dex, &distribution)?)
            })?;
            Ok(().into())
        }

        #[pallet::weight(<T as Config>::WeightInfo::authorize())]
        pub fn authorize(
            origin: OriginFor<T>,
            dominator: <T::Lookup as StaticLookup>::Source,
            token_id: TokenId<T>,
            amount: Balance<T>,
        ) -> DispatchResultWithPostInfo {
            let fund_owner = ensure_signed(origin)?;
            let dex = T::Lookup::lookup(dominator)?;
            Self::authorize_to(fund_owner, dex, token_id, amount)?;
            Ok(().into())
        }

        #[transactional]
        #[pallet::weight(<T as Config>::WeightInfo::revoke())]
        pub fn revoke(
            origin: OriginFor<T>,
            dominator: <T::Lookup as StaticLookup>::Source,
            token_id: TokenId<T>,
            amount: Balance<T>,
        ) -> DispatchResultWithPostInfo {
            let fund_owner = ensure_signed(origin)?;
            let dominator = T::Lookup::lookup(dominator)?;
            Self::revoke_from(fund_owner, dominator, token_id, amount, None)?;
            Ok(().into())
        }

        #[transactional]
        #[pallet::weight(<T as Config>::WeightInfo::revoke())]
        pub fn revoke_with_callback(
            origin: OriginFor<T>,
            dominator: <T::Lookup as StaticLookup>::Source,
            token_id: TokenId<T>,
            amount: Balance<T>,
            callback: Box<T::Callback>,
        ) -> DispatchResultWithPostInfo {
            let fund_owner = ensure_signed(origin)?;
            let dominator = T::Lookup::lookup(dominator)?;
            Self::revoke_from(fund_owner, dominator, token_id, amount, Some(*callback))?;
            Ok(().into())
        }

        #[transactional]
        #[pallet::weight(1_000_000)]
        pub fn open_market(
            origin: OriginFor<T>,
            base: TokenId<T>,
            quote: TokenId<T>,
            base_scale: u8,
            quote_scale: u8,
            min_base: Balance<T>,
        ) -> DispatchResultWithPostInfo {
            let dominator = ensure_signed(origin)?;
            ensure!(
                Dominators::<T>::contains_key(&dominator),
                Error::<T>::DominatorNotFound
            );
            T::MarketManager::open_market(
                dominator,
                base,
                quote,
                base_scale,
                quote_scale,
                min_base,
            )?;
            Ok(().into())
        }
    }

    #[derive(Clone)]
    struct ClearingResult<T: Config> {
        pub makers: Vec<MakerMutation<T::AccountId, Balance<T>>>,
        pub taker: TakerMutation<T::AccountId, Balance<T>>,
        pub base_fee: Balance<T>,
        pub quote_fee: Balance<T>,
    }

    #[derive(Clone)]
    struct TakerMutation<AccountId, Balance> {
        pub who: AccountId,
        pub unfilled_volume: Balance,
        pub filled_volume: Balance,
        pub filled_amount: Balance,
        pub base_balance: Balance,
        pub quote_balance: Balance,
    }

    #[derive(Clone)]
    struct MakerMutation<AccountId, Balance> {
        pub who: AccountId,
        pub filled_volume: Balance,
        pub base_balance: Balance,
        pub quote_balance: Balance,
    }

    impl<T: Config> Pallet<T>
    where
        Balance<T>: Copy + From<u128> + Into<u128>,
        TokenId<T>: Copy + From<u32> + Into<u32>,
        T::BlockNumber: From<u32> + Into<u32>,
    {
        pub fn system_account() -> T::AccountId {
            PALLET_ID.try_into_account().unwrap()
        }

        /// this is not associated with the runtime definitions
        pub fn validate_beneficiary(
            old_beneficiary: Option<T::AccountId>,
            signature: Sr25519Signature,
            new_beneficiary: T::AccountId,
        ) -> DispatchResult {
            if let Some(old_beneficiary) = old_beneficiary {
                let nonce = frame_system::Pallet::<T>::account_nonce(&old_beneficiary);
                let payload = (nonce, new_beneficiary).using_encoded(|v| v.to_vec());
                let raw_old_beneficiary: [u8; 32] = old_beneficiary
                    .encode()
                    .try_into()
                    .expect("AccountId32 <-> [u8; 32]; qed");
                if sp_io::crypto::sr25519_verify(
                    &signature,
                    &payload,
                    &Sr25519Public(raw_old_beneficiary),
                ) {
                    frame_system::Pallet::<T>::inc_account_nonce(old_beneficiary);
                    Ok(())
                } else {
                    Err(Error::<T>::InvalidBeneficiaryProof.into())
                }
            } else {
                Ok(())
            }
        }

        #[transactional]
        pub fn authorize_to(
            fund_owner: T::AccountId,
            dex: T::AccountId,
            token_id: TokenId<T>,
            amount: Balance<T>,
        ) -> DispatchResult {
            let dominator =
                Dominators::<T>::try_get(&dex).map_err(|_| Error::<T>::DominatorNotFound)?;
            ensure!(
                dominator.status == DOMINATOR_ACTIVE,
                Error::<T>::DominatorInactive
            );
            ensure!(
                !Receipts::<T>::contains_key(&dex, &fund_owner),
                Error::<T>::ReceiptAlreadyExists,
            );
            ensure!(
                T::Asset::can_reserve(&token_id, &fund_owner, amount),
                Error::<T>::InsufficientBalance
            );
            let block_number = frame_system::Pallet::<T>::block_number();
            Self::reserve(
                RESERVE_FOR_AUTHORIZING_STASH,
                fund_owner.clone(),
                token_id,
                amount,
                &dex,
            )?;
            Receipts::<T>::insert(
                dex.clone(),
                fund_owner.clone(),
                Receipt::Authorize(token_id, amount, block_number),
            );
            Self::deposit_event(Event::TokenHosted(fund_owner, dex, token_id, amount));
            Ok(())
        }

        #[transactional]
        pub fn revoke_from(
            fund_owner: T::AccountId,
            dominator_id: T::AccountId,
            token_id: TokenId<T>,
            amount: Balance<T>,
            callback: Option<T::Callback>,
        ) -> DispatchResult {
            ensure!(
                Self::has_authorized_morethan(fund_owner.clone(), token_id, amount, &dominator_id),
                Error::<T>::InsufficientBalance
            );
            let dominator = Dominators::<T>::try_get(&dominator_id)
                .map_err(|_| Error::<T>::DominatorNotFound)?;
            ensure!(
                dominator.status != DOMINATOR_REGISTERED,
                Error::<T>::DominatorStatusInvalid
            );
            if dominator.status == DOMINATOR_EVICTED {
                Reserves::<T>::try_mutate_exists(
                    &(RESERVE_FOR_AUTHORIZING_STASH, fund_owner.clone(), token_id),
                    &dominator_id,
                    |ov| -> DispatchResult {
                        let av: Balance<T> = ov.take().unwrap_or(0.into());
                        if av > 0.into() {
                            return Reserves::<T>::try_mutate(
                                &(RESERVE_FOR_AUTHORIZING, fund_owner.clone(), token_id),
                                &dominator_id,
                                |v| -> DispatchResult {
                                    Ok(*v = v.checked_add(&av).ok_or(Error::<T>::Overflow)?)
                                },
                            );
                        }
                        Ok(())
                    },
                )?;
                Self::unreserve(
                    RESERVE_FOR_AUTHORIZING,
                    fund_owner.clone(),
                    token_id,
                    amount,
                    &dominator_id,
                )?;
                Receipts::<T>::remove(dominator_id.clone(), &fund_owner);
            } else {
                ensure!(
                    !Receipts::<T>::contains_key(&dominator_id, &fund_owner),
                    Error::<T>::ReceiptAlreadyExists,
                );
                let block_number = frame_system::Pallet::<T>::block_number();
                let receipt = if let Some(callback) = callback {
                    Receipt::RevokeWithCallback(token_id, amount, block_number, callback)
                } else {
                    Receipt::Revoke(token_id, amount, block_number)
                };
                Receipts::<T>::insert(dominator_id.clone(), fund_owner.clone(), receipt);
            }
            Self::deposit_event(Event::TokenRevoked(
                fund_owner,
                dominator_id,
                token_id,
                amount,
            ));
            Ok(())
        }

        fn verify_batch(
            dominator_id: T::AccountId,
            beneficiary: T::AccountId,
            dominator: &Dominator<Balance<T>, BlockNumberFor<T>>,
            proofs: Vec<ProofV2<T::AccountId>>,
        ) -> DispatchResultWithPostInfo {
            let mut known_root = dominator.merkle_root;
            let mut incr: BTreeMap<TokenId<T>, (Balance<T>, Balance<T>)> = BTreeMap::new();
            for proof in proofs.into_iter() {
                let trade = Self::verify_and_update(
                    &dominator_id,
                    &beneficiary,
                    known_root,
                    dominator.start_from.clone(),
                    proof,
                )?;
                known_root = trade.root;
                if trade.amount != Zero::zero() && trade.vol != Zero::zero() {
                    incr.entry(trade.token_id)
                        .and_modify(|(b, q)| {
                            *b += trade.amount;
                            *q += trade.vol;
                        })
                        .or_insert((trade.amount, trade.vol));
                }
            }
            let current_block = frame_system::Pallet::<T>::block_number();
            for (token_id, trade) in incr.into_iter() {
                T::Indicator::set_price(token_id, trade.0, trade.1, current_block);
            }
            Ok(().into())
        }

        #[transactional]
        fn verify_and_update(
            dominator_id: &T::AccountId,
            beneficiary: &T::AccountId,
            known_root: MerkleHash,
            claim_at: T::BlockNumber,
            proof: ProofV2<T::AccountId>,
        ) -> Result<Trade<TokenId<T>, Balance<T>>, DispatchError> {
            let mp = smt::CompiledMerkleProof(proof.merkle_proof.clone());
            let (old, new): (Vec<_>, Vec<_>) = proof
                .leaves
                .iter()
                .map(|v| {
                    let key = hashing(&v.key).into();
                    ((key, v.old_v.into()), (key, v.new_v.into()))
                })
                .unzip();
            let r = mp
                .verify::<smt::blake2b::Blake2bHasher>(&known_root.into(), old)
                .map_err(|_| Error::<T>::ProofsUnsatisfied)?;
            ensure!(r, Error::<T>::ProofsUnsatisfied);
            let r = mp
                .verify::<smt::blake2b::Blake2bHasher>(&proof.root.into(), new)
                .map_err(|_| Error::<T>::ProofsUnsatisfied)?;
            ensure!(r, Error::<T>::ProofsUnsatisfied);
            let current_block = frame_system::Pallet::<T>::block_number();
            let current_season = Self::current_season(current_block, claim_at);
            let mut trade = Trade {
                token_id: Default::default(),
                root: known_root,
                amount: Zero::zero(),
                vol: Zero::zero(),
            };
            match proof.cmd {
                CommandV2::AskLimit {
                    price,
                    amount,
                    maker_fee,
                    taker_fee,
                    base,
                    quote,
                    broker,
                } => {
                    Self::check_fee(taker_fee.into(), maker_fee.into())?;
                    let (price, amount, maker_fee, taker_fee, base, quote): (
                        u128,
                        u128,
                        Permill,
                        Permill,
                        u32,
                        u32,
                    ) = (
                        price.into(),
                        amount.into(),
                        Permill::from_parts(maker_fee.into()),
                        Permill::from_parts(taker_fee.into()),
                        base.into(),
                        quote.into(),
                    );
                    ensure!(
                        T::Asset::is_stable(&quote.into()),
                        Error::<T>::UnsupportedQuoteCurrency
                    );
                    let cr = Self::verify_ask_limit(
                        price,
                        amount,
                        maker_fee,
                        taker_fee,
                        base,
                        quote,
                        proof.maker_account_delta,
                        proof.maker_page_delta,
                        dominator_id,
                        &proof.leaves,
                    )?;
                    for d in cr.makers.iter() {
                        Self::clear(&d.who, dominator_id, base.into(), d.base_balance)?;
                        Self::clear(&d.who, dominator_id, quote.into(), d.quote_balance)?;
                        T::Rewarding::consume_liquidity(
                            &d.who,
                            (base.into(), quote.into()),
                            d.filled_volume,
                            current_block,
                        )?;
                    }
                    Self::clear(
                        &cr.taker.who,
                        dominator_id,
                        base.into(),
                        cr.taker.base_balance,
                    )?;
                    Self::clear(
                        &cr.taker.who,
                        dominator_id,
                        quote.into(),
                        cr.taker.quote_balance,
                    )?;
                    T::Rewarding::put_liquidity(
                        &cr.taker.who,
                        (base.into(), quote.into()),
                        cr.taker.unfilled_volume,
                        current_block,
                    );

                    trade.token_id = base.into();
                    trade.amount += cr.taker.filled_amount;
                    trade.vol += cr.taker.filled_volume;

                    Self::put_profit(dominator_id, current_season, quote.into(), cr.quote_fee)?;
                    // 50% to broker if exists
                    if cr.base_fee != Zero::zero() {
                        let base_fee = if let Some(broker) = broker {
                            let near_half = Permill::from_percent(50).mul_ceil(cr.base_fee);
                            T::Asset::try_mutate_account(
                                &base.into(),
                                &T::BrokerBeneficiary::beneficiary(broker),
                                |b| Ok(b.0 += near_half),
                            )?;
                            cr.base_fee
                                .checked_sub(&near_half)
                                .ok_or(Error::<T>::Overflow)?
                        } else {
                            cr.base_fee
                        };
                        T::Asset::try_mutate_account(&base.into(), beneficiary, |b| {
                            Ok(b.0 += base_fee)
                        })?;
                    }
                }
                CommandV2::BidLimit {
                    price,
                    amount,
                    maker_fee,
                    taker_fee,
                    base,
                    quote,
                    broker,
                } => {
                    Self::check_fee(taker_fee.into(), maker_fee.into())?;
                    let (price, amount, maker_fee, taker_fee, base, quote): (
                        u128,
                        u128,
                        Permill,
                        Permill,
                        u32,
                        u32,
                    ) = (
                        price.into(),
                        amount.into(),
                        Permill::from_parts(maker_fee.into()),
                        Permill::from_parts(taker_fee.into()),
                        base.into(),
                        quote.into(),
                    );
                    ensure!(
                        T::Asset::is_stable(&quote.into()),
                        Error::<T>::UnsupportedQuoteCurrency
                    );
                    let cr = Self::verify_bid_limit(
                        price,
                        amount,
                        maker_fee,
                        taker_fee,
                        base,
                        quote,
                        proof.maker_account_delta,
                        proof.maker_page_delta,
                        dominator_id,
                        &proof.leaves,
                    )?;
                    for d in cr.makers.iter() {
                        Self::clear(&d.who, dominator_id, base.into(), d.base_balance)?;
                        Self::clear(&d.who, dominator_id, quote.into(), d.quote_balance)?;
                        T::Rewarding::consume_liquidity(
                            &d.who,
                            (base.into(), quote.into()),
                            d.filled_volume,
                            current_block,
                        )?;
                    }

                    Self::clear(
                        &cr.taker.who,
                        dominator_id,
                        base.into(),
                        cr.taker.base_balance,
                    )?;
                    Self::clear(
                        &cr.taker.who,
                        dominator_id,
                        quote.into(),
                        cr.taker.quote_balance,
                    )?;
                    T::Rewarding::put_liquidity(
                        &cr.taker.who,
                        (base.into(), quote.into()),
                        cr.taker.unfilled_volume,
                        current_block,
                    );

                    trade.token_id = base.into();
                    trade.amount += cr.taker.filled_amount;
                    trade.vol += cr.taker.filled_volume;

                    Self::put_profit(dominator_id, current_season, quote.into(), cr.quote_fee)?;
                    if cr.base_fee != Zero::zero() {
                        let base_fee = if let Some(broker) = broker {
                            let near_half = Permill::from_percent(50).mul_ceil(cr.base_fee);
                            T::Asset::try_mutate_account(
                                &base.into(),
                                &T::BrokerBeneficiary::beneficiary(broker),
                                |b| Ok(b.0 += near_half),
                            )?;
                            cr.base_fee
                                .checked_sub(&near_half)
                                .ok_or(Error::<T>::Overflow)?
                        } else {
                            cr.base_fee
                        };
                        T::Asset::try_mutate_account(&base.into(), beneficiary, |b| {
                            Ok(b.0 += base_fee)
                        })?;
                    }
                }
                CommandV2::Cancel { base, quote } => {
                    let (base, quote): (u32, u32) = (base.into(), quote.into());
                    let unfilled = Self::verify_cancel(base, quote, &proof.user_id, &proof.leaves)?;
                    let _ = T::Rewarding::remove_liquidity(
                        &proof.user_id,
                        (base.into(), quote.into()),
                        unfilled.into(),
                    );
                }
                CommandV2::TransferOut { currency, amount } => {
                    let (currency, amount) = (currency.into(), amount.into());
                    let r = Receipts::<T>::get(dominator_id, &proof.user_id)
                        .ok_or(Error::<T>::ReceiptNotExists)?;
                    let exists = match r {
                        Receipt::Revoke(id, value, _) => {
                            id.into() == currency && value.into() == amount
                        }
                        Receipt::RevokeWithCallback(id, value, ..) => {
                            id.into() == currency && value.into() == amount
                        }
                        Receipt::Authorize(..) => false,
                    };
                    ensure!(exists, Error::<T>::ReceiptNotExists);
                    Self::verify_transfer_out(currency, amount, &proof.user_id, &proof.leaves)?;
                    Self::unreserve(
                        RESERVE_FOR_AUTHORIZING,
                        proof.user_id.clone(),
                        currency.into(),
                        amount.into(),
                        &dominator_id,
                    )?;
                    Receipts::<T>::remove(dominator_id, &proof.user_id);
                    match r {
                        Receipt::RevokeWithCallback(_, _, _, cb) => {
                            if let Err(e) = cb
                                .dispatch(
                                    frame_system::RawOrigin::Signed(proof.user_id.clone()).into(),
                                )
                                .map_err(|e| e.error)
                            {
                                log::error!(
                                    "execute callback of {:?} failed: {:?}",
                                    proof.user_id,
                                    e
                                );
                            }
                        }
                        _ => {}
                    }
                }
                CommandV2::TransferIn { currency, amount } => {
                    let (currency, amount) = (currency.into(), amount.into());
                    let r = Receipts::<T>::get(dominator_id, &proof.user_id)
                        .ok_or(Error::<T>::ReceiptNotExists)?;
                    let exists = match r {
                        Receipt::Authorize(id, value, _) => {
                            id.into() == currency && value.into() == amount
                        }
                        _ => false,
                    };
                    ensure!(exists, Error::<T>::ReceiptNotExists);
                    Self::verify_transfer_in(currency, amount, &proof.user_id, &proof.leaves)?;
                    //stash->authorizing
                    Reserves::<T>::remove(
                        &(
                            RESERVE_FOR_AUTHORIZING_STASH,
                            proof.user_id.clone(),
                            currency.into(),
                        ),
                        dominator_id,
                    );
                    Reserves::<T>::try_mutate(
                        &(
                            RESERVE_FOR_AUTHORIZING,
                            proof.user_id.clone(),
                            currency.into(),
                        ),
                        dominator_id,
                        |ov| -> DispatchResult {
                            Ok(*ov = ov.checked_add(&amount.into()).ok_or(Error::<T>::Overflow)?)
                        },
                    )?;
                    Receipts::<T>::remove(dominator_id, &proof.user_id);
                }
                CommandV2::RejectTransferOut { currency, amount } => {
                    let (currency, amount): (u32, u128) = (currency.into(), amount.into());
                    let r = Receipts::<T>::get(&dominator_id, &proof.user_id)
                        .ok_or(Error::<T>::ReceiptNotExists)?;
                    let exists = match r {
                        Receipt::Revoke(id, value, _) => {
                            currency == id.into() && value.into() == amount
                        }
                        Receipt::RevokeWithCallback(id, value, ..) => {
                            currency == id.into() && value.into() == amount
                        }
                        Receipt::Authorize(..) => false,
                    };
                    ensure!(exists, Error::<T>::ReceiptNotExists);
                    Self::verify_reject_transfer_out(
                        currency,
                        amount,
                        &proof.user_id,
                        &proof.leaves,
                    )?;
                    Receipts::<T>::remove(&dominator_id, &proof.user_id);
                    // needn't step forward
                    return Ok(trade);
                }
                CommandV2::RejectTransferIn => {
                    let r = Receipts::<T>::get(&dominator_id, &proof.user_id);
                    if r.is_none() {
                        return Ok(trade);
                    }
                    let r = r.unwrap();
                    ensure!(
                        matches!(r, Receipt::Authorize(_, _, _)),
                        Error::<T>::ReceiptNotExists
                    );
                    Receipts::<T>::remove(&dominator_id, &proof.user_id);
                    return Ok(trade);
                }
            }
            Dominators::<T>::mutate(&dominator_id, |d| {
                let update = d.as_mut().unwrap();
                update.merkle_root = proof.root;
                update.sequence = (proof.event_id, current_block);
            });
            trade.root = proof.root;
            Ok(trade)
        }

        fn verify_ask_limit(
            price: u128,
            amount: u128,
            maker_fee: Permill,
            taker_fee: Permill,
            base: u32,
            quote: u32,
            maker_accounts: u8,
            pages: u8,
            dominator: &T::AccountId,
            leaves: &[MerkleLeaf],
        ) -> Result<ClearingResult<T>, DispatchError> {
            // v2: orderbook_size, maker_accounts, taker_account, best_price, orderpage
            let leaves_count = (4u8 + maker_accounts + pages) as usize;
            ensure!(leaves.len() == leaves_count, Error::<T>::ProofsUnsatisfied);
            ensure!(maker_accounts % 2 == 0, Error::<T>::ProofsUnsatisfied);
            let (b, q) = leaves[0].try_get_symbol::<T>()?;
            ensure!(b == base && q == quote, Error::<T>::ProofsUnsatisfied);
            let (ask0, bid0) = leaves[0].split_old_to_u128();
            let (ask1, bid1) = leaves[0].split_new_to_u128();
            // 0 or remain
            let ask_delta = ask1 - ask0;
            // equals to traded base
            let bid_delta = bid0 - bid1;

            let taker_base = &leaves[maker_accounts as usize + 1];
            let (bk, taker_b_id) = taker_base.try_get_account::<T>()?;
            let (tba0, tbf0) = taker_base.split_old_to_u128();
            ensure!(
                Self::has_authorized_exactly_on(
                    taker_b_id.clone(),
                    base.into(),
                    (tba0.checked_add(tbf0).ok_or(Error::<T>::Overflow)?).into(),
                    &dominator,
                ),
                Error::<T>::ProofsUnsatisfied
            );
            let (tba1, tbf1) = taker_base.split_new_to_u128();
            // equals to traded base
            let tb_delta = (tba0 + tbf0) - (tba1 + tbf1);

            let taker_quote = &leaves[maker_accounts as usize + 2];
            let (qk, taker_q_id) = taker_quote.try_get_account::<T>()?;
            let (tqa0, tqf0) = taker_quote.split_old_to_u128();
            ensure!(
                Self::has_authorized_exactly_on(
                    taker_q_id.clone(),
                    quote.into(),
                    (tqa0 + tqf0).into(),
                    &dominator,
                ),
                Error::<T>::ProofsUnsatisfied
            );
            let (tqa1, tqf1) = taker_quote.split_new_to_u128();
            let tq_delta = (tqa1.checked_add(tqf1).ok_or(Error::<T>::Overflow)?)
                .checked_sub(tqa0.checked_add(tqf0).ok_or(Error::<T>::Overflow)?)
                .ok_or(Error::<T>::Overflow)?;
            ensure!(bk == base && qk == quote, Error::<T>::ProofsUnsatisfied);
            ensure!(taker_b_id == taker_q_id, Error::<T>::ProofsUnsatisfied);
            // the delta of taker base available account(a.k.a base freezed of taker), equals to the amount of cmd
            if ask_delta != 0 {
                ensure!(amount == tba0 - tba1, Error::<T>::ProofsUnsatisfied);
            } else {
                ensure!(tbf0 == tbf1, Error::<T>::ProofsUnsatisfied);
            }
            ensure!(bid_delta == tb_delta, Error::<T>::ProofsUnsatisfied);
            let mut mb_delta = 0u128;
            let mut mq_delta = 0u128;
            let mut maker_mutation = Vec::new();
            for i in 0..maker_accounts as usize / 2 {
                // base first
                let maker_base = &leaves[i * 2 + 1];
                let (bk, maker_b_id) = maker_base.try_get_account::<T>()?;
                let mb0 = maker_base.split_old_to_sum();
                ensure!(
                    Self::has_authorized_exactly_on(
                        maker_b_id.clone(),
                        base.into(),
                        mb0.into(),
                        &dominator,
                    ),
                    Error::<T>::ProofsUnsatisfied
                );
                let mb1 = maker_base.split_new_to_sum();
                let base_incr = mb1 - mb0;
                mb_delta += base_incr;
                // then quote account
                let maker_quote = &leaves[i * 2 + 2];
                let (qk, maker_q_id) = maker_quote.try_get_account::<T>()?;
                ensure!(base == bk && quote == qk, Error::<T>::ProofsUnsatisfied);
                let mq0 = maker_quote.split_old_to_sum();
                ensure!(
                    Self::has_authorized_exactly_on(
                        maker_q_id.clone(),
                        quote.into(),
                        mq0.into(),
                        &dominator,
                    ),
                    Error::<T>::ProofsUnsatisfied
                );
                let mq1 = maker_quote.split_new_to_sum();
                let quote_decr = mq0 - mq1;
                mq_delta += quote_decr;
                // the accounts should be owned by same user
                ensure!(maker_b_id == maker_q_id, Error::<T>::ProofsUnsatisfied);
                maker_mutation.push(MakerMutation {
                    who: maker_q_id,
                    // this includes the maker fee
                    filled_volume: quote_decr.into(),
                    base_balance: mb1.into(),
                    quote_balance: mq1.into(),
                });
            }
            let base_charged = maker_fee.mul_ceil(tb_delta);
            ensure!(
                mb_delta + base_charged == tb_delta,
                Error::<T>::ProofsUnsatisfied
            );
            let quote_charged = taker_fee.mul_ceil(mq_delta);
            ensure!(
                tq_delta + quote_charged == mq_delta,
                Error::<T>::ProofsUnsatisfied
            );
            let taker_mutation = TakerMutation {
                who: taker_b_id,
                unfilled_volume: tqf1.into(),
                filled_volume: mq_delta.into(),
                filled_amount: tb_delta.into(),
                base_balance: (tba1 + tbf1).into(),
                quote_balance: (tqa1 + tqf1).into(),
            };
            let best_price = &leaves[maker_accounts as usize + 3];
            let (b, q) = best_price.try_get_symbol::<T>()?;
            ensure!(b == base && q == quote, Error::<T>::ProofsUnsatisfied);
            let (best_ask0, best_bid0) = best_price.split_old_to_u128();
            let (best_ask1, best_bid1) = best_price.split_new_to_u128();

            if bid_delta != 0 {
                // trading happened
                ensure!(
                    pages > 0 && price <= best_bid0,
                    Error::<T>::ProofsUnsatisfied
                );
                // best_bid0 >= page0 > page1 > .. > pagen >= best_bid1
                let mut pre_best = best_bid0 + 1;
                let mut taken_bids = 0u128;
                for i in 0..pages as usize - 1 {
                    let page = &leaves[maker_accounts as usize + 4 + i];
                    let (b, q, p) = page.try_get_orderpage::<T>()?;
                    ensure!(b == base && q == quote, Error::<T>::ProofsUnsatisfied);
                    ensure!(pre_best > p, Error::<T>::ProofsUnsatisfied);
                    pre_best = p;
                    ensure!(page.split_new_to_sum() == 0, Error::<T>::ProofsUnsatisfied);
                    taken_bids += page.split_old_to_sum();
                }

                if ask_delta != 0 {
                    // partial_filled
                    let taker_page = leaves.last().unwrap();
                    let (b, q, p) = taker_page.try_get_orderpage::<T>()?;
                    ensure!(b == base && q == quote, Error::<T>::ProofsUnsatisfied);
                    ensure!(
                        pre_best > p && p >= best_bid1,
                        Error::<T>::ProofsUnsatisfied
                    );
                    ensure!(
                        best_ask1 == price && p == price,
                        Error::<T>::ProofsUnsatisfied
                    );
                    let prv_is_maker = taker_page.split_old_to_sum();
                    let now_is_taker = taker_page.split_new_to_sum();
                    ensure!(
                        taken_bids + prv_is_maker + now_is_taker == amount,
                        Error::<T>::ProofsUnsatisfied
                    );
                } else {
                    // filled or conditionally_canceled
                    let vanity_maker = leaves.last().unwrap();
                    let (b, q, p) = vanity_maker.try_get_orderpage::<T>()?;
                    ensure!(b == base && q == quote, Error::<T>::ProofsUnsatisfied);
                    ensure!(
                        pre_best > p && p >= best_bid1,
                        Error::<T>::ProofsUnsatisfied
                    );
                    ensure!(best_ask1 == best_ask0, Error::<T>::ProofsUnsatisfied);
                    let prv_is_maker = vanity_maker.split_old_to_sum();
                    let now_is_maker = vanity_maker.split_new_to_sum();
                    ensure!(
                        tb_delta == taken_bids + prv_is_maker - now_is_maker,
                        Error::<T>::ProofsUnsatisfied
                    );
                }
            } else {
                // no trading
                ensure!(best_bid1 == best_bid0, Error::<T>::ProofsUnsatisfied);
                if ask_delta != 0 {
                    // placed
                    let vanity_maker = leaves.last().unwrap();
                    let (b, q, p) = vanity_maker.try_get_orderpage::<T>()?;
                    ensure!(
                        b == base && q == quote && p > best_bid1,
                        Error::<T>::ProofsUnsatisfied
                    );
                    let prv_is_maker = vanity_maker.split_old_to_sum();
                    let now_is_maker = vanity_maker.split_new_to_sum();
                    ensure!(
                        amount == now_is_maker - prv_is_maker,
                        Error::<T>::ProofsUnsatisfied
                    );
                }
            }
            Ok(ClearingResult {
                makers: maker_mutation,
                taker: taker_mutation,
                base_fee: base_charged.into(),
                quote_fee: quote_charged.into(),
            })
        }

        fn verify_bid_limit(
            price: u128,
            amount: u128,
            maker_fee: Permill,
            taker_fee: Permill,
            base: u32,
            quote: u32,
            maker_accounts: u8,
            pages: u8,
            dominator: &T::AccountId,
            leaves: &[MerkleLeaf],
        ) -> Result<ClearingResult<T>, DispatchError> {
            // orderbook_size, maker_accounts, taker_account, best_price, orderpage
            let leaves_count = (4u8 + maker_accounts + pages) as usize;
            ensure!(leaves.len() == leaves_count, Error::<T>::ProofsUnsatisfied);
            ensure!(maker_accounts % 2 == 0, Error::<T>::ProofsUnsatisfied);
            let (ask0, bid0) = leaves[0].split_old_to_u128();
            let (ask1, bid1) = leaves[0].split_new_to_u128();
            let ask_delta = ask0 - ask1;
            let bid_delta = bid1 - bid0;

            let taker_base = &leaves[maker_accounts as usize + 1];
            let (tba0, tbf0) = taker_base.split_old_to_u128();
            let (tba1, tbf1) = taker_base.split_new_to_u128();
            let tb_delta = (tba1 + tbf1) - (tba0 + tbf0);
            let (bk, taker_b_id) = taker_base.try_get_account::<T>()?;
            ensure!(
                Self::has_authorized_exactly_on(
                    taker_b_id.clone(),
                    base.into(),
                    (tba0 + tbf0).into(),
                    &dominator,
                ),
                Error::<T>::ProofsUnsatisfied
            );

            let taker_quote = &leaves[maker_accounts as usize + 2];
            let (tqa0, tqf0) = taker_quote.split_old_to_u128();
            let (tqa1, tqf1) = taker_quote.split_new_to_u128();
            let (qk, taker_q_id) = taker_quote.try_get_account::<T>()?;
            ensure!(
                Self::has_authorized_exactly_on(
                    taker_q_id.clone(),
                    quote.into(),
                    (tqa0 + tqf0).into(),
                    &dominator,
                ),
                Error::<T>::ProofsUnsatisfied
            );

            let tq_delta = (tqa0.checked_add(tqf0).ok_or(Error::<T>::Overflow)?)
                .checked_sub(tqa1.checked_add(tqf1).ok_or(Error::<T>::Overflow)?)
                .ok_or(Error::<T>::Overflow)?;
            ensure!(bk == base && qk == quote, Error::<T>::ProofsUnsatisfied);
            ensure!(taker_b_id == taker_q_id, Error::<T>::ProofsUnsatisfied);
            let mut mb_delta = 0u128;
            let mut mq_delta = 0u128;
            let mut maker_mutation = Vec::new();
            for i in 0..maker_accounts as usize / 2 {
                // base first
                let maker_base = &leaves[i * 2 + 1];
                let (bk, maker_b_id) = maker_base.try_get_account::<T>()?;
                let mb0 = maker_base.split_old_to_sum();
                ensure!(
                    Self::has_authorized_exactly_on(
                        maker_b_id.clone(),
                        base.into(),
                        mb0.into(),
                        &dominator,
                    ),
                    Error::<T>::ProofsUnsatisfied
                );
                let mb1 = maker_base.split_new_to_sum();
                let base_decr = mb0 - mb1;
                mb_delta += base_decr;
                // then quote
                let maker_quote = &leaves[i * 2 + 2];
                let (qk, maker_q_id) = maker_quote.try_get_account::<T>()?;
                ensure!(quote == qk && base == bk, Error::<T>::ProofsUnsatisfied);
                ensure!(maker_b_id == maker_q_id, Error::<T>::ProofsUnsatisfied);
                let mq0 = maker_quote.split_old_to_sum();
                ensure!(
                    Self::has_authorized_exactly_on(
                        maker_q_id.clone(),
                        quote.into(),
                        mq0.into(),
                        &dominator,
                    ),
                    Error::<T>::ProofsUnsatisfied
                );
                let mq1 = maker_quote.split_new_to_sum();
                let quote_incr = mq1.checked_sub(mq0).ok_or(Error::<T>::Overflow)?;
                mq_delta = mq_delta
                    .checked_add(quote_incr)
                    .ok_or(Error::<T>::Overflow)?;
                // let filled_vol = Permill::one()
                //     .checked_sub(&maker_fee)
                //     .ok_or(Error::<T>::Overflow)?
                //     .saturating_reciprocal_mul_ceil(quote_incr);
                // FIXME this is actually wrong, we need to include the transaction fee
                maker_mutation.push(MakerMutation {
                    who: maker_b_id,
                    filled_volume: quote_incr.into(),
                    base_balance: mb1.into(),
                    quote_balance: mq1.into(),
                });
            }
            let quote_charged = maker_fee.mul_ceil(tq_delta);
            ensure!(
                mq_delta
                    .checked_add(quote_charged)
                    .ok_or(Error::<T>::Overflow)?
                    == tq_delta,
                Error::<T>::ProofsUnsatisfied
            );
            let base_charged = taker_fee.mul_ceil(mb_delta);
            ensure!(
                tb_delta
                    .checked_add(base_charged)
                    .ok_or(Error::<T>::Overflow)?
                    == mb_delta,
                Error::<T>::ProofsUnsatisfied
            );
            ensure!(ask_delta == mb_delta, Error::<T>::ProofsUnsatisfied);
            if bid_delta != 0 {
                ensure!(
                    bid_delta == amount - mb_delta,
                    Error::<T>::ProofsUnsatisfied
                );
            }
            let taker_mutation = TakerMutation {
                who: taker_b_id,
                unfilled_volume: tqf1.into(),
                filled_volume: tq_delta.into(),
                filled_amount: mb_delta.into(),
                base_balance: (tba1.checked_add(tbf1).ok_or(Error::<T>::Overflow)?).into(),
                quote_balance: (tqa1.checked_add(tqf1).ok_or(Error::<T>::Overflow)?).into(),
            };
            let best_price = &leaves[maker_accounts as usize + 3];
            let (b, q) = best_price.try_get_symbol::<T>()?;
            ensure!(b == base && q == quote, Error::<T>::ProofsUnsatisfied);
            let (best_ask0, best_bid0) = best_price.split_old_to_u128();
            let (best_ask1, best_bid1) = best_price.split_new_to_u128();

            if ask_delta != 0 {
                // trading happened
                ensure!(
                    pages > 0 && price >= best_ask0,
                    Error::<T>::ProofsUnsatisfied
                );
                // best_ask0 <= page0 < page1 < .. < pagen <= best_ask1
                let mut pre_best = best_ask0;
                let mut taken_asks = 0u128;
                for i in 0..pages as usize - 1 {
                    let page = &leaves[maker_accounts as usize + 4 + i];
                    let (b, q, p) = page.try_get_orderpage::<T>()?;
                    ensure!(b == base && q == quote, Error::<T>::ProofsUnsatisfied);
                    ensure!(pre_best <= p, Error::<T>::ProofsUnsatisfied);
                    pre_best = p;
                    ensure!(page.split_new_to_sum() == 0, Error::<T>::ProofsUnsatisfied);
                    taken_asks += page.split_old_to_sum();
                }
                if bid_delta != 0 {
                    // partial_filled
                    let taker_price_page = leaves.last().unwrap();
                    let (b, q, p) = taker_price_page.try_get_orderpage::<T>()?;
                    ensure!(
                        b == base && q == quote && p == price,
                        Error::<T>::ProofsUnsatisfied
                    );
                    ensure!(best_bid1 == price, Error::<T>::ProofsUnsatisfied);
                    let prv_is_maker = taker_price_page.split_old_to_sum();
                    let now_is_taker = taker_price_page.split_new_to_sum();
                    ensure!(
                        taken_asks + prv_is_maker + now_is_taker == amount,
                        Error::<T>::ProofsUnsatisfied
                    );
                } else {
                    // filled or conditional_canceled
                    let vanity_maker = leaves.last().unwrap();
                    let (b, q, _) = vanity_maker.try_get_orderpage::<T>()?;
                    ensure!(b == base && q == quote, Error::<T>::ProofsUnsatisfied);
                    ensure!(best_bid1 == best_bid0, Error::<T>::ProofsUnsatisfied);
                    let prv_is_maker = vanity_maker.split_old_to_sum();
                    let now_is_maker = vanity_maker.split_new_to_sum();
                    ensure!(
                        tb_delta
                            .checked_add(base_charged)
                            .ok_or(Error::<T>::Overflow)?
                            == taken_asks
                                .checked_add(prv_is_maker)
                                .ok_or(Error::<T>::Overflow)?
                                .checked_sub(now_is_maker)
                                .ok_or(Error::<T>::Overflow)?,
                        Error::<T>::ProofsUnsatisfied
                    );
                }
            } else {
                // no trading
                ensure!(best_ask1 == best_ask0, Error::<T>::ProofsUnsatisfied);
                if bid_delta != 0 {
                    // placed
                    let taker_price_page = leaves.last().unwrap();
                    let (b, q, p) = taker_price_page.try_get_orderpage::<T>()?;
                    ensure!(
                        b == base && q == quote && p == price,
                        Error::<T>::ProofsUnsatisfied
                    );
                    let prv_is_maker = taker_price_page.split_old_to_sum();
                    let now_is_maker = taker_price_page.split_new_to_sum();
                    ensure!(
                        amount == now_is_maker - prv_is_maker,
                        Error::<T>::ProofsUnsatisfied
                    );
                }
            }
            Ok(ClearingResult {
                makers: maker_mutation,
                taker: taker_mutation,
                base_fee: base_charged.into(),
                quote_fee: quote_charged.into(),
            })
        }

        fn verify_transfer_in(
            currency: u32,
            amount: u128,
            account: &T::AccountId,
            leaves: &[MerkleLeaf],
        ) -> Result<(), DispatchError> {
            ensure!(leaves.len() == 1, Error::<T>::ProofsUnsatisfied);
            let (a0, f0) = leaves[0].split_old_to_u128();
            let (a1, f1) = leaves[0].split_new_to_u128();
            ensure!(a1 - a0 == amount, Error::<T>::ProofsUnsatisfied);
            ensure!(f1 == f0, Error::<T>::ProofsUnsatisfied);
            let (c, id) = leaves[0].try_get_account::<T>()?;
            ensure!(
                currency == c && account == &id,
                Error::<T>::ProofsUnsatisfied
            );
            Ok(())
        }

        fn check_fee(taker_fee: u32, maker_fee: u32) -> Result<(), DispatchError> {
            ensure!(
                maker_fee <= T::MaxMakerFee::get() && taker_fee <= T::MaxTakerFee::get(),
                Error::<T>::FeesTooHigh
            );
            Ok(())
        }

        fn verify_transfer_out(
            currency: u32,
            amount: u128,
            account: &T::AccountId,
            leaves: &[MerkleLeaf],
        ) -> Result<(), DispatchError> {
            ensure!(leaves.len() == 1, Error::<T>::ProofsUnsatisfied);
            let (a0, f0) = leaves[0].split_old_to_u128();
            let (a1, f1) = leaves[0].split_new_to_u128();
            ensure!(a0 - a1 == amount, Error::<T>::ProofsUnsatisfied);
            ensure!(f1 == f0, Error::<T>::ProofsUnsatisfied);
            let (c, id) = leaves[0].try_get_account::<T>()?;
            ensure!(
                currency == c && account == &id,
                Error::<T>::ProofsUnsatisfied
            );
            Ok(())
        }

        fn verify_reject_transfer_out(
            currency: u32,
            amount: u128,
            account: &T::AccountId,
            leaves: &[MerkleLeaf],
        ) -> Result<(), DispatchError> {
            ensure!(leaves.len() == 1, Error::<T>::ProofsUnsatisfied);
            let (a0, _) = leaves[0].split_old_to_u128();
            ensure!(a0 < amount, Error::<T>::ProofsUnsatisfied);
            let (c, id) = leaves[0].try_get_account::<T>()?;
            ensure!(
                currency == c && account == &id,
                Error::<T>::ProofsUnsatisfied
            );
            Ok(())
        }

        fn verify_cancel(
            base: u32,
            quote: u32,
            account: &T::AccountId,
            leaves: &[MerkleLeaf],
        ) -> Result<u128, DispatchError> {
            ensure!(leaves.len() == 5, Error::<T>::ProofsUnsatisfied);
            let (b, q) = leaves[0].try_get_symbol::<T>()?;
            ensure!(b == base && q == quote, Error::<T>::ProofsUnsatisfied);
            let (ask0, bid0) = leaves[0].split_old_to_u128();
            let (ask1, bid1) = leaves[0].split_new_to_u128();
            let ask_delta = ask0 - ask1;
            let bid_delta = bid0 - bid1;
            ensure!(ask_delta + bid_delta != 0, Error::<T>::ProofsUnsatisfied);
            ensure!(ask_delta & bid_delta == 0, Error::<T>::ProofsUnsatisfied);

            let (b, id) = leaves[1].try_get_account::<T>()?;
            ensure!(b == base, Error::<T>::ProofsUnsatisfied);
            ensure!(account == &id, Error::<T>::ProofsUnsatisfied);
            let (ba0, bf0) = leaves[1].split_old_to_u128();
            let (ba1, bf1) = leaves[1].split_new_to_u128();
            ensure!(ba0 + bf0 == ba1 + bf1, Error::<T>::ProofsUnsatisfied);

            let (q, id) = leaves[2].try_get_account::<T>()?;
            ensure!(q == quote, Error::<T>::ProofsUnsatisfied);
            ensure!(account == &id, Error::<T>::ProofsUnsatisfied);
            let (qa0, qf0) = leaves[2].split_old_to_u128();
            let (qa1, qf1) = leaves[2].split_new_to_u128();
            ensure!(qa0 + qf0 == qa1 + qf1, Error::<T>::ProofsUnsatisfied);

            let (best_ask0, best_bid0) = leaves[3].split_old_to_u128();
            let (b, q, cancel_at) = leaves[4].try_get_orderpage::<T>()?;
            ensure!(
                b == base && q == quote && (cancel_at >= best_ask0 || cancel_at <= best_bid0),
                Error::<T>::ProofsUnsatisfied,
            );
            let before_cancel = leaves[4].split_old_to_sum();
            let after_cancel = leaves[4].split_new_to_sum();
            let delta = before_cancel - after_cancel;
            if cancel_at >= best_ask0 && best_ask0 != 0 {
                ensure!(ask_delta == delta, Error::<T>::ProofsUnsatisfied);
            } else {
                ensure!(bid_delta == delta, Error::<T>::ProofsUnsatisfied);
            }
            Ok(delta)
        }

        fn has_authorized_morethan(
            who: T::AccountId,
            token_id: TokenId<T>,
            amount: Balance<T>,
            dominator: &T::AccountId,
        ) -> bool {
            Reserves::<T>::get(&(RESERVE_FOR_AUTHORIZING, who, token_id), dominator) >= amount
        }

        fn has_authorized_exactly_on(
            who: T::AccountId,
            token_id: TokenId<T>,
            amount: Balance<T>,
            dominator: &T::AccountId,
        ) -> bool {
            let confirmed =
                Reserves::<T>::get(&(RESERVE_FOR_AUTHORIZING, who, token_id), dominator);
            // FIXME the offchain matchers loose the precesions
            if confirmed >= amount {
                confirmed - amount <= 100000000000u128.into()
            } else {
                amount - confirmed <= 100000000000u128.into()
            }
        }

        #[transactional]
        fn clear(
            who: &T::AccountId,
            dominator: &T::AccountId,
            token_id: TokenId<T>,
            balance: Balance<T>,
        ) -> DispatchResult {
            Reserves::<T>::try_mutate(
                &(RESERVE_FOR_AUTHORIZING, who.clone(), token_id),
                dominator,
                |reserved| -> DispatchResult {
                    T::Asset::try_mutate_account(&token_id, who, |b| -> DispatchResult {
                        // FIXME the offchain matchers loose the precesions
                        b.1 =
                            b.1.checked_sub(reserved)
                                .unwrap_or_default()
                                .checked_add(&balance)
                                .ok_or(Error::<T>::Overflow)?;
                        Ok(())
                    })?;
                    *reserved = balance;
                    Ok(())
                },
            )
        }

        /// take shares from dominator, return season should update
        #[transactional]
        fn take_shares(
            staker: &T::AccountId,
            dominator: &T::AccountId,
            distributions: &Distribution<T>,
        ) -> Result<Season, DispatchError> {
            if distributions.to_season == distributions.from_season {
                return Ok(distributions.from_season);
            }
            let mut shares: BTreeMap<TokenId<T>, u128> = BTreeMap::new();
            for season in distributions.from_season..distributions.to_season {
                let bonus = Bonuses::<T>::get(dominator, season);
                if bonus.staked.is_zero() || bonus.profit.is_empty() {
                    continue;
                }
                // TODO associated type
                let staking: u128 = distributions.staking.into();
                let total_staking: u128 = bonus.staked.into();
                let r: Perquintill = Perquintill::from_rational(staking, total_staking);
                for (token_id, profit) in bonus.profit.into_iter() {
                    shares
                        .entry(token_id)
                        .and_modify(|share| *share += r * profit.into())
                        .or_insert(r * profit.into());
                }
            }
            for (token_id, profit) in shares {
                T::Asset::try_mutate_account(&token_id, staker, |b| Ok(b.0 += profit.into()))?;
            }
            Ok(distributions.to_season)
        }

        #[transactional]
        fn reserve(
            reserve_id: u8,
            fund_owner: T::AccountId,
            token: TokenId<T>,
            value: Balance<T>,
            to: &T::AccountId,
        ) -> DispatchResult {
            if value.is_zero() {
                return Ok(());
            }
            Reserves::<T>::try_mutate(
                &(reserve_id, fund_owner.clone(), token),
                to,
                |ov| -> DispatchResult {
                    T::Asset::reserve(&token, &fund_owner, value)?;
                    Ok(*ov = ov.checked_add(&value).ok_or(Error::<T>::Overflow)?)
                },
            )
        }

        #[transactional]
        fn unreserve(
            reserve_id: u8,
            fund_owner: T::AccountId,
            token: TokenId<T>,
            value: Balance<T>,
            from: &T::AccountId,
        ) -> DispatchResult {
            if value.is_zero() {
                return Ok(());
            }
            Reserves::<T>::try_mutate_exists(
                &(reserve_id, fund_owner.clone(), token),
                from,
                |ov| -> DispatchResult {
                    T::Asset::unreserve(&token, &fund_owner, value)?;
                    let mut reserve = ov.take().ok_or(Error::<T>::InsufficientBalance)?;
                    reserve = reserve
                        .checked_sub(&value)
                        .ok_or(Error::<T>::InsufficientBalance)?;
                    if reserve > Zero::zero() {
                        ov.replace(reserve);
                    }
                    Ok(())
                },
            )
        }

        #[transactional]
        fn stake_on(
            staker: &T::AccountId,
            dominator_id: &T::AccountId,
            amount: Balance<T>,
        ) -> DispatchResult {
            ensure!(
                amount >= T::MinimalStakingAmount::get(),
                Error::<T>::LittleStakingAmount
            );
            Dominators::<T>::try_mutate_exists(dominator_id, |exists| -> DispatchResult {
                ensure!(exists.is_some(), Error::<T>::DominatorNotFound);
                let mut dominator = exists.take().unwrap();
                ensure!(
                    dominator.status == DOMINATOR_ACTIVE || dominator.status == DOMINATOR_INACTIVE,
                    Error::<T>::DominatorStatusInvalid
                );
                Stakings::<T>::try_mutate(&dominator_id, &staker, |staking| -> DispatchResult {
                    Self::reserve(
                        RESERVE_FOR_STAKING,
                        staker.clone(),
                        T::Asset::native_token_id(),
                        amount,
                        &dominator_id,
                    )?;
                    let current_block = frame_system::Pallet::<T>::block_number();
                    let current_season = Self::current_season(current_block, dominator.start_from);
                    if !staking.amount.is_zero() {
                        //not first staking
                        let distribution = Distribution {
                            from_season: staking.from_season,
                            to_season: current_season,
                            staking: staking.amount,
                        };
                        Self::take_shares(staker, dominator_id, &distribution)?;
                    }
                    staking.amount += amount;
                    staking.from_season = current_season;
                    Ok(())
                })?;
                dominator.staked += amount;
                let dominator_old_status = dominator.status;
                dominator.status = if dominator.staked >= T::DominatorOnlineThreshold::get() {
                    DOMINATOR_ACTIVE
                } else {
                    DOMINATOR_INACTIVE
                };
                Self::deposit_event(Event::TaoStaked(
                    staker.clone(),
                    dominator_id.clone(),
                    amount,
                ));
                if dominator.status == DOMINATOR_ACTIVE
                    && dominator_old_status == DOMINATOR_INACTIVE
                {
                    Self::deposit_event(Event::DominatorOnline(dominator_id.clone()));
                }
                exists.replace(dominator);
                Ok(())
            })
        }

        #[transactional]
        fn unstake_from(
            staker: &T::AccountId,
            dominator_id: &T::AccountId,
            amount: Balance<T>,
        ) -> DispatchResult {
            Dominators::<T>::try_mutate_exists(dominator_id, |exists| -> DispatchResult {
                ensure!(exists.is_some(), Error::<T>::DominatorNotFound);
                let mut dominator = exists.take().unwrap();
                ensure!(
                    dominator.status != DOMINATOR_REGISTERED,
                    Error::<T>::DominatorStatusInvalid
                );
                let dominator_total_staking = dominator
                    .staked
                    .checked_sub(&amount)
                    .ok_or(Error::<T>::InsufficientBalance)?;
                Stakings::<T>::try_mutate_exists(&dominator_id, &staker, |s| -> DispatchResult {
                    let staking = s.take();
                    ensure!(staking.is_some(), Error::<T>::InvalidStaking);
                    let mut staking = staking.unwrap();
                    let remain = staking
                        .amount
                        .checked_sub(&amount)
                        .ok_or(Error::<T>::InsufficientBalance)?;
                    ensure!(
                        remain.is_zero() || remain >= T::MinimalStakingAmount::get(),
                        Error::<T>::LittleStakingAmount
                    );
                    Self::unreserve(
                        RESERVE_FOR_STAKING,
                        staker.clone(),
                        T::Asset::native_token_id(),
                        amount,
                        &dominator_id,
                    )?;
                    Self::reserve(
                        RESERVE_FOR_PENDING_UNSTAKE,
                        staker.clone(),
                        T::Asset::native_token_id(),
                        amount,
                        &Self::system_account(),
                    )?;
                    let current_block = frame_system::Pallet::<T>::block_number();
                    let current_season = Self::current_season(current_block, dominator.start_from);
                    let unlock_at =
                        current_block - current_block % T::DominatorCheckGracePeriod::get();
                    let unlock_at = unlock_at + UNSTAKE_DELAY_BLOCKS.into();
                    PendingUnstakings::<T>::try_mutate(
                        &unlock_at,
                        &staker,
                        |v| -> DispatchResult {
                            Ok(*v = v.checked_add(&amount).ok_or(Error::<T>::Overflow)?)
                        },
                    )?;
                    let distribution = Distribution {
                        from_season: staking.from_season,
                        to_season: current_season,
                        staking: staking.amount,
                    };
                    Self::take_shares(staker, dominator_id, &distribution)?;
                    staking.from_season = current_season;
                    staking.amount = remain;
                    if !remain.is_zero() {
                        s.replace(staking);
                    }
                    Ok(())
                })?;
                dominator.staked = dominator_total_staking;
                let dominator_old_status = dominator.status;
                if dominator.status != DOMINATOR_EVICTED {
                    dominator.status = if dominator.staked >= T::DominatorOnlineThreshold::get() {
                        DOMINATOR_ACTIVE
                    } else {
                        DOMINATOR_INACTIVE
                    };
                }
                Self::deposit_event(Event::TaoUnstaked(
                    staker.clone(),
                    dominator_id.clone(),
                    amount,
                ));
                if dominator.status == DOMINATOR_INACTIVE
                    && dominator_old_status == DOMINATOR_ACTIVE
                {
                    Self::deposit_event(Event::DominatorOffline(dominator_id.clone()));
                }
                exists.replace(dominator);
                Ok(())
            })
        }

        pub fn current_season(now: T::BlockNumber, claim_at: T::BlockNumber) -> Season {
            if now <= claim_at {
                return 0;
            }
            ((now - claim_at) / T::SeasonDuration::get()).into()
        }

        pub fn start_block_of_season(claim_at: T::BlockNumber, season: Season) -> T::BlockNumber {
            claim_at + (season * T::SeasonDuration::get().into()).into()
        }

        #[transactional]
        fn put_profit(
            dominator: &T::AccountId,
            season: Season,
            currency: TokenId<T>,
            balance: Balance<T>,
        ) -> DispatchResult {
            if balance == Zero::zero() {
                Ok(())
            } else {
                Bonuses::<T>::try_mutate(dominator, season, |b| {
                    b.profit
                        .entry(currency)
                        .and_modify(|p| *p += balance)
                        .or_insert(balance);
                    Ok(())
                })
            }
        }

        pub fn current_season_of_dominator(dominator: T::AccountId) -> Season {
            let now = frame_system::Pallet::<T>::block_number();
            let claim_at = Dominators::<T>::try_get(&dominator)
                .map(|d| d.start_from)
                .unwrap_or_default();
            Self::current_season(now, claim_at)
        }

        pub fn pending_shares_of_dominator(
            dominator: T::AccountId,
            who: T::AccountId,
        ) -> Balance<T> {
            let start_from = Dominators::<T>::try_get(&dominator)
                .map(|d| d.start_from)
                .unwrap_or_default();
            if start_from == Zero::zero() {
                return Zero::zero();
            }
            let current_block = frame_system::Pallet::<T>::block_number();
            let current_season = Self::current_season(current_block, start_from);
            let staking = Stakings::<T>::get(&dominator, &who);
            if staking.amount.is_zero() {
                return Zero::zero();
            }
            let user_staking: u128 = staking.amount.into();
            let mut shares = 0u128;
            for season in staking.from_season..current_season {
                let bonus = Bonuses::<T>::get(&dominator, season);
                if bonus.staked.is_zero() || bonus.profit.is_empty() {
                    continue;
                }
                let total_staking: u128 = bonus.staked.into();
                let r: Perquintill = Perquintill::from_rational(user_staking, total_staking);
                for (_, profit) in bonus.profit.into_iter() {
                    shares += r * profit.into();
                }
            }
            shares.into()
        }
    }
}
