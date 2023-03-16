#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

/// Constant values used within the runtime.
pub mod constants;
use constants::{currency::*, time::*};
use pallet_grandpa::{
    fg_primitives, AuthorityId as GrandpaId, AuthorityList as GrandpaAuthorityList,
};
use sp_api::impl_runtime_apis;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata};
use sp_runtime::{
    create_runtime_str, generic, impl_opaque_keys,
    traits::{AccountIdLookup, BlakeTwo256, Block as BlockT, NumberFor, Verify},
    transaction_validity::{TransactionSource, TransactionValidity},
    ApplyExtrinsicResult,
};
use sp_std::prelude::*;
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

// A few exports that help ease life for downstream crates.
use beefy_primitives::{crypto::AuthorityId as BeefyId, mmr::MmrLeafVersion};
use codec::Encode;
pub use frame_support::{
    construct_runtime, parameter_types,
    traits::{ConstU128, ConstU16, ConstU32, KeyOwnerProofSystem, Randomness, StorageInfo},
    weights::{
        constants::{BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight, WEIGHT_PER_SECOND},
        IdentityFee, Weight,
    },
    StorageValue,
};
use frame_support::{
    dispatch::DispatchClass,
    traits::{AsEnsureOriginWithArg, SortedMembers},
    weights::ConstantMultiplier,
    PalletId,
};
use frame_system::{
    limits::{BlockLength, BlockWeights},
    EnsureSigned, EnsureSignedBy,
};
pub use fuso_primitives::{AccountId, Balance, BlockNumber, Hash, Index, Moment, Signature};
use fuso_support::{chainbridge::derive_resource_id, ChainId};
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use pallet_session::historical as pallet_session_historical;
pub use pallet_timestamp::Call as TimestampCall;
use pallet_transaction_payment::{
    CurrencyAdapter, FeeDetails, Multiplier, RuntimeDispatchInfo, TargetedFeeAdjustment,
};

#[cfg(any(feature = "std", test))]
pub use frame_system::Call as SystemCall;
#[cfg(any(feature = "std", test))]
pub use pallet_balances::Call as BalancesCall;
#[cfg(any(feature = "std", test))]
pub use pallet_sudo::Call as SudoCall;
use sp_mmr_primitives as mmr;
#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;

use sp_runtime::{
    generic::Era,
    traits::{self, ConvertInto, Keccak256, OpaqueKeys, SaturatedConversion, StaticLookup},
    transaction_validity::TransactionPriority,
    FixedPointNumber, Perquintill,
};
pub use sp_runtime::{Perbill, Permill};
use static_assertions::const_assert;

/// Opaque types. These are used by the CLI to instantiate machinery that don't need to know
/// the specifics of the runtime. They can then be made to be agnostic over specific formats
/// of data like extrinsics, allowing for them to continue syncing the network through upgrades
/// to even the core data structures.
pub mod opaque {
    use super::*;

    pub use sp_runtime::OpaqueExtrinsic as UncheckedExtrinsic;

    /// Opaque block header type.
    pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
    /// Opaque block type.
    pub type Block = generic::Block<Header, UncheckedExtrinsic>;
    /// Opaque block identifier type.
    pub type BlockId = generic::BlockId<Block>;

    impl_opaque_keys! {
        pub struct SessionKeys {
            pub babe: Babe,
            pub grandpa: Grandpa,
            pub im_online: ImOnline,
            pub beefy: Beefy,
            pub octopus: OctopusAppchain,
        }
    }
}

/// Wasm binary unwrapped. If built with `SKIP_WASM_BUILD`, the function panics.
#[cfg(feature = "std")]
pub fn wasm_binary_unwrap() -> &'static [u8] {
    WASM_BINARY.expect(
        "Development wasm binary is not available. This means the client is built with \
		 `SKIP_WASM_BUILD` flag and it is only usable for production chains. Please rebuild with \
		 the flag disabled.",
    )
}

// To learn more about runtime versioning and what each of the following value means:
//   https://docs.substrate.io/v3/runtime/upgrades#runtime-versioning
#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
    spec_name: create_runtime_str!("fusotao"),
    impl_name: create_runtime_str!("fusotao"),
    authoring_version: 1,
    // The version of the runtime specification. A full node will not attempt to use its native
    //   runtime in substitute for the on-chain Wasm runtime unless all of `spec_name`,
    //   `spec_version`, and `authoring_version` are the same between Wasm and native.
    // This value is set to 100 to notify Polkadot-JS App (https://polkadot.js.org/apps) to use
    //   the compatible custom types.
    spec_version: 154,
    impl_version: 1,
    apis: RUNTIME_API_VERSIONS,
    transaction_version: 3,
    state_version: 1,
};

/// The BABE epoch configuration at genesis.
pub const BABE_GENESIS_EPOCH_CONFIG: sp_consensus_babe::BabeEpochConfiguration =
    sp_consensus_babe::BabeEpochConfiguration {
        c: PRIMARY_PROBABILITY,
        allowed_slots: sp_consensus_babe::AllowedSlots::PrimaryAndSecondaryPlainSlots,
    };

/// The version information used to identify this runtime when compiled natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
    NativeVersion {
        runtime_version: VERSION,
        can_author_with: Default::default(),
    }
}

/// We assume that ~10% of the block weight is consumed by `on_initialize` handlers.
/// This is used to limit the maximal weight of a single extrinsic.
const AVERAGE_ON_INITIALIZE_RATIO: Perbill = Perbill::from_percent(10);
/// We allow `Normal` extrinsics to fill up the block up to 75%, the rest can be used
/// by  Operational  extrinsics.
const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);
/// We allow for 2 seconds of compute with a 6 second average block time.
const MAXIMUM_BLOCK_WEIGHT: Weight = WEIGHT_PER_SECOND.saturating_mul(2);

parameter_types! {
    pub const BlockHashCount: BlockNumber = 2400;
    pub const Version: RuntimeVersion = VERSION;
    pub RuntimeBlockLength: BlockLength =
        BlockLength::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
    pub RuntimeBlockWeights: BlockWeights = BlockWeights::builder()
        .base_block(BlockExecutionWeight::get())
        .for_class(DispatchClass::all(), |weights| {
            weights.base_extrinsic = ExtrinsicBaseWeight::get();
        })
        .for_class(DispatchClass::Normal, |weights| {
            weights.max_total = Some(NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT);
        })
        .for_class(DispatchClass::Operational, |weights| {
            weights.max_total = Some(MAXIMUM_BLOCK_WEIGHT);
            // Operational transactions have some extra reserved space, so that they
            // are included even if block reached `MAXIMUM_BLOCK_WEIGHT`.
            weights.reserved = Some(
                MAXIMUM_BLOCK_WEIGHT - NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT
            );
        })
        .avg_block_initialization(AVERAGE_ON_INITIALIZE_RATIO)
        .build_or_panic();
}

const_assert!(NORMAL_DISPATCH_RATIO.deconstruct() >= AVERAGE_ON_INITIALIZE_RATIO.deconstruct());

// Configure FRAME pallets to include in runtime.
impl frame_system::Config for Runtime {
    /// The data to be stored in an account.
    type AccountData = pallet_balances::AccountData<Balance>;
    /// The identifier used to distinguish between accounts.
    type AccountId = AccountId;
    /// The basic call filter to use in dispatchable.
    type BaseCallFilter = frame_support::traits::Everything;
    /// Maximum number of block number to block hash mappings to keep (oldest pruned first).
    type BlockHashCount = BlockHashCount;
    /// The maximum length of a block (in bytes).
    type BlockLength = RuntimeBlockLength;
    /// The index type for blocks.
    type BlockNumber = BlockNumber;
    /// Block & extrinsics weights: base values and limits.
    type BlockWeights = RuntimeBlockWeights;
    /// The weight of database operations that the runtime can invoke.
    type DbWeight = RocksDbWeight;
    /// The type for hashing blocks and tries.
    type Hash = Hash;
    /// The hashing algorithm used.
    type Hashing = BlakeTwo256;
    /// The header type.
    type Header = generic::Header<BlockNumber, BlakeTwo256>;
    /// The index type for storing how many extrinsics an account has signed.
    type Index = Index;
    /// The lookup mechanism to get account ID from whatever is passed in dispatchers.
    type Lookup = AccountIdLookup<AccountId, ()>;
    type MaxConsumers = ConstU32<16>;
    /// What to do if an account is fully reaped from the system.
    type OnKilledAccount = ();
    /// What to do if a new account is created.
    type OnNewAccount = ();
    /// The set code logic, just the default since we're not a parachain.
    type OnSetCode = ();
    /// Converts a module to the index of the module in `construct_runtime!`.
    /// This type is being generated by `construct_runtime!`.
    type PalletInfo = PalletInfo;
    /// The aggregated dispatch type that is available for extrinsics.
    type RuntimeCall = RuntimeCall;
    /// The ubiquitous event type.
    type RuntimeEvent = RuntimeEvent;
    /// The ubiquitous origin type.
    type RuntimeOrigin = RuntimeOrigin;
    /// This is used as an identifier of the chain. 42 is the generic substrate prefix.
    type SS58Prefix = ConstU16<42>;
    /// Weight information for the extrinsics of this pallet.
    type SystemWeightInfo = frame_system::weights::SubstrateWeight<Runtime>;
    /// Version of the runtime.
    type Version = Version;
}

parameter_types! {
    // NOTE: Currently it is not possible to change the epoch duration after the chain has started.
    //       Attempting to do so will brick block production.
    pub const EpochDuration: u64 = EPOCH_DURATION_IN_SLOTS;
    pub const ExpectedBlockTime: Moment = MILLISECS_PER_BLOCK;
    pub const ReportLongevity: u64 =
        BondingDuration::get() as u64 * SessionsPerEra::get() as u64 * EpochDuration::get();
}

impl pallet_babe::Config for Runtime {
    type DisabledValidators = Session;
    type EpochChangeTrigger = pallet_babe::ExternalTrigger;
    type EpochDuration = EpochDuration;
    type ExpectedBlockTime = ExpectedBlockTime;
    type HandleEquivocation = pallet_babe::EquivocationHandler<
        Self::KeyOwnerIdentification,
        pallet_octopus_lpos::FilterHistoricalOffences<OctopusLpos, Offences>,
        ReportLongevity,
    >;
    type KeyOwnerIdentification = <Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(
        KeyTypeId,
        pallet_babe::AuthorityId,
    )>>::IdentificationTuple;
    type KeyOwnerProof = <Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(
        KeyTypeId,
        pallet_babe::AuthorityId,
    )>>::Proof;
    type KeyOwnerProofSystem = Historical;
    type MaxAuthorities = MaxAuthorities;
    type WeightInfo = ();
}

parameter_types! {
    pub const ExistentialDeposit: Balance = 1;
    // For weight estimation, we assume that the most locks on an individual account will be 50.
    // This number may need to be adjusted in the future if this assumption no longer holds true.
    pub const MaxLocks: u32 = 50;
    pub const MaxReserves: u32 = 50;
}

impl pallet_balances::Config for Runtime {
    type AccountStore = System;
    /// The type for recording an account's balance.
    type Balance = Balance;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type MaxLocks = MaxLocks;
    type MaxReserves = MaxReserves;
    type ReserveIdentifier = [u8; 8];
    /// The ubiquitous event type.
    type RuntimeEvent = RuntimeEvent;
    //TODO
    type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    pub const TransactionByteFee: Balance = 100_000_000; //TODO
    pub const OperationalFeeMultiplier: u8 = 5;
    pub const TargetBlockFullness: Perquintill = Perquintill::from_percent(25);
    pub AdjustmentVariable: Multiplier = Multiplier::saturating_from_rational(1, 100_000);
    pub MinimumMultiplier: Multiplier = Multiplier::saturating_from_rational(1, 1_000_000_000u128);
}

impl pallet_transaction_payment::Config for Runtime {
    type FeeMultiplierUpdate =
        TargetedFeeAdjustment<Self, TargetBlockFullness, AdjustmentVariable, MinimumMultiplier>;
    type LengthToFee = ConstantMultiplier<Balance, TransactionByteFee>;
    type OnChargeTransaction = CurrencyAdapter<Balances, ()>;
    type OperationalFeeMultiplier = OperationalFeeMultiplier;
    type RuntimeEvent = RuntimeEvent;
    type WeightToFee = IdentityFee<Balance>;
}

parameter_types! {
    pub const MinimumPeriod: Moment = SLOT_DURATION / 2;
}

impl pallet_timestamp::Config for Runtime {
    type MinimumPeriod = MinimumPeriod;
    /// A timestamp: milliseconds since the unix epoch.
    type Moment = Moment;
    type OnTimestampSet = Babe;
    type WeightInfo = pallet_timestamp::weights::SubstrateWeight<Runtime>;
}

impl pallet_randomness_collective_flip::Config for Runtime {}

parameter_types! {
    pub const UncleGenerations: BlockNumber = 0;
}

impl pallet_authorship::Config for Runtime {
    type EventHandler = (OctopusLpos, ImOnline);
    type FilterUncle = ();
    type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Babe>;
    type UncleGenerations = UncleGenerations;
}

impl pallet_session::Config for Runtime {
    type Keys = opaque::SessionKeys;
    type NextSessionRotation = Babe;
    type RuntimeEvent = RuntimeEvent;
    type SessionHandler = <opaque::SessionKeys as OpaqueKeys>::KeyTypeIdProviders;
    type SessionManager = pallet_session::historical::NoteHistoricalRoot<Self, OctopusLpos>;
    type ShouldEndSession = Babe;
    type ValidatorId = <Self as frame_system::Config>::AccountId;
    type ValidatorIdOf = ConvertInto;
    type WeightInfo = pallet_session::weights::SubstrateWeight<Runtime>;
}

impl pallet_session::historical::Config for Runtime {
    type FullIdentification = u128;
    type FullIdentificationOf = pallet_octopus_lpos::ExposureOf<Runtime>;
}

impl pallet_sudo::Config for Runtime {
    type RuntimeCall = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
}

parameter_types! {
    pub const ImOnlineUnsignedPriority: TransactionPriority = TransactionPriority::max_value();
    /// We prioritize im-online heartbeats over election solution submission.
    pub const StakingUnsignedPriority: TransactionPriority = TransactionPriority::max_value() / 2;
    pub const MaxAuthorities: u32 = 100;
    pub const MaxKeys: u32 = 10_000;
    pub const MaxPeerInHeartbeats: u32 = 10_000;
    pub const MaxPeerDataEncodingSize: u32 = 1_000;
}

impl<LocalCall> frame_system::offchain::CreateSignedTransaction<LocalCall> for Runtime
where
    RuntimeCall: From<LocalCall>,
{
    fn create_transaction<C: frame_system::offchain::AppCrypto<Self::Public, Self::Signature>>(
        call: RuntimeCall,
        public: <Signature as traits::Verify>::Signer,
        account: AccountId,
        nonce: Index,
    ) -> Option<(
        RuntimeCall,
        <UncheckedExtrinsic as traits::Extrinsic>::SignaturePayload,
    )> {
        let tip = 0;
        // take the biggest period possible.
        let period = BlockHashCount::get()
            .checked_next_power_of_two()
            .map(|c| c / 2)
            .unwrap_or(2) as u64;
        let current_block = System::block_number()
            .saturated_into::<u64>()
            // The `System::block_number` is initialized with `n+1`,
            // so the actual block number is `n`.
            .saturating_sub(1);
        let era = Era::mortal(period, current_block);
        let extra = (
            frame_system::CheckNonZeroSender::<Runtime>::new(),
            frame_system::CheckSpecVersion::<Runtime>::new(),
            frame_system::CheckTxVersion::<Runtime>::new(),
            frame_system::CheckGenesis::<Runtime>::new(),
            frame_system::CheckEra::<Runtime>::from(era),
            frame_system::CheckNonce::<Runtime>::from(nonce),
            frame_system::CheckWeight::<Runtime>::new(),
            pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(tip),
        );
        let raw_payload = SignedPayload::new(call, extra)
            .map_err(|e| {
                log::warn!("Unable to create signed payload: {:?}", e);
            })
            .ok()?;
        let signature = raw_payload.using_encoded(|payload| C::sign(payload, public))?;
        let address = <Runtime as frame_system::Config>::Lookup::unlookup(account); //TODO Self to Runtime
        let (call, extra, _) = raw_payload.deconstruct();
        Some((call, (address, signature.into(), extra)))
    }
}

impl frame_system::offchain::SigningTypes for Runtime {
    type Public = <Signature as traits::Verify>::Signer;
    type Signature = Signature;
}

impl<C> frame_system::offchain::SendTransactionTypes<C> for Runtime
where
    RuntimeCall: From<C>,
{
    type Extrinsic = UncheckedExtrinsic;
    type OverarchingCall = RuntimeCall;
}

impl pallet_im_online::Config for Runtime {
    type AuthorityId = ImOnlineId;
    type MaxKeys = MaxKeys;
    type MaxPeerDataEncodingSize = MaxPeerDataEncodingSize;
    type MaxPeerInHeartbeats = MaxPeerInHeartbeats;
    type NextSessionRotation = Babe;
    type ReportUnresponsiveness =
        pallet_octopus_lpos::FilterHistoricalOffences<OctopusLpos, Offences>;
    type RuntimeEvent = RuntimeEvent;
    type UnsignedPriority = ImOnlineUnsignedPriority;
    type ValidatorSet = Historical;
    type WeightInfo = pallet_im_online::weights::SubstrateWeight<Runtime>;
}

impl pallet_offences::Config for Runtime {
    type IdentificationTuple = pallet_session::historical::IdentificationTuple<Self>;
    type OnOffenceHandler = ();
    type RuntimeEvent = RuntimeEvent;
}

impl pallet_grandpa::Config for Runtime {
    type HandleEquivocation = pallet_grandpa::EquivocationHandler<
        Self::KeyOwnerIdentification,
        pallet_octopus_lpos::FilterHistoricalOffences<OctopusLpos, Offences>,
        ReportLongevity,
    >;
    type KeyOwnerIdentification = <Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(
        KeyTypeId,
        GrandpaId,
    )>>::IdentificationTuple;
    type KeyOwnerProof =
        <Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(KeyTypeId, GrandpaId)>>::Proof;
    type KeyOwnerProofSystem = Historical;
    type MaxAuthorities = MaxAuthorities;
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
}

parameter_types! {
    pub const CollectionDeposit: Balance = 100 * DOLLARS;
    pub const ItemDeposit: Balance = 1 * DOLLARS;
    pub const KeyLimit: u32 = 32;
    pub const ValueLimit: u32 = 256;
}

pub type CollectionId = u128;
pub type ItemId = u128;
impl pallet_uniques::Config<pallet_uniques::Instance1> for Runtime {
    type AttributeDepositBase = MetadataDepositBase;
    type CollectionDeposit = CollectionDeposit;
    type CollectionId = CollectionId;
    type CreateOrigin = AsEnsureOriginWithArg<EnsureSigned<AccountId>>;
    type Currency = Balances;
    type DepositPerByte = MetadataDepositPerByte;
    type ForceOrigin = frame_system::EnsureRoot<AccountId>;
    #[cfg(feature = "runtime-benchmarks")]
    type Helper = ();
    type ItemDeposit = ItemDeposit;
    type ItemId = ItemId;
    type KeyLimit = KeyLimit;
    type Locker = ();
    type MetadataDepositBase = MetadataDepositBase;
    type RuntimeEvent = RuntimeEvent;
    type StringLimit = StringLimit;
    type ValueLimit = ValueLimit;
    type WeightInfo = pallet_uniques::weights::SubstrateWeight<Runtime>;
}

impl pallet_beefy::Config for Runtime {
    type BeefyId = BeefyId;
    type MaxAuthorities = MaxAuthorities;
    type OnNewValidatorSet = MmrLeaf;
}

impl pallet_mmr::Config for Runtime {
    type Hash = <Keccak256 as traits::Hash>::Output;
    type Hashing = Keccak256;
    type LeafData = pallet_beefy_mmr::Pallet<Runtime>;
    type OnNewRoot = pallet_beefy_mmr::DepositBeefyDigest<Runtime>;
    type WeightInfo = ();

    const INDEXING_PREFIX: &'static [u8] = b"mmr";
}

parameter_types! {
    /// Version of the produced MMR leaf.
    ///
    /// The version consists of two parts;
    /// - `major` (3 bits)
    /// - `minor` (5 bits)
    ///
    /// `major` should be updated only if decoding the previous MMR Leaf format from the payload
    /// is not possible (i.e. backward incompatible change).
    /// `minor` should be updated if fields are added to the previous MMR Leaf, which given SCALE
    /// encoding does not prevent old leafs from being decoded.
    ///
    /// Hence we expect `major` to be changed really rarely (think never).
    /// See [`MmrLeafVersion`] type documentation for more details.
    pub LeafVersion: MmrLeafVersion = MmrLeafVersion::new(0, 0);
}

impl pallet_beefy_mmr::Config for Runtime {
    type BeefyAuthorityToMerkleLeaf = pallet_beefy_mmr::BeefyEcdsaToEthereum;
    type BeefyDataProvider = ();
    type LeafExtra = Vec<u8>;
    type LeafVersion = LeafVersion;
}

pub struct OctopusAppCrypto;

impl frame_system::offchain::AppCrypto<<Signature as Verify>::Signer, Signature>
    for OctopusAppCrypto
{
    type GenericPublic = sp_core::sr25519::Public;
    type GenericSignature = sp_core::sr25519::Signature;
    type RuntimeAppPublic = pallet_octopus_appchain::sr25519::AuthorityId;
}

parameter_types! {
    pub const OctopusAppchainPalletId: PalletId = PalletId(*b"py/octps");
    pub const GracePeriod: u32 = 10;
    pub const UnsignedPriority: u64 = 1 << 21;
    pub const RequestEventLimit: u32 = 10;
    pub const UpwardMessagesLimit: u32 = 10;
}

impl pallet_octopus_appchain::Config for Runtime {
    type AppCrypto = OctopusAppCrypto;
    type AuthorityId = pallet_octopus_appchain::sr25519::AuthorityId;
    type BridgeInterface = OctopusBridge;
    type GracePeriod = GracePeriod;
    type LposInterface = OctopusLpos;
    type MaxValidators = MaxAuthorities;
    type RequestEventLimit = RequestEventLimit;
    type RuntimeCall = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type UnsignedPriority = UnsignedPriority;
    type UpwardMessagesInterface = OctopusUpwardMessages;
    type WeightInfo = pallet_octopus_appchain::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    pub const NativeTokenDecimals: u128 = TAO;
    pub const FeeTh: u64 = 2;
}

impl pallet_octopus_bridge::Config for Runtime {
    type AppchainInterface = OctopusAppchain;
    type AssetBalance = Balance;
    type AssetId = TokenId;
    type AssetIdByTokenId = Token;
    type CollectionId = CollectionId;
    type Convertor = ();
    type Currency = Balances;
    type Fungibles = Token;
    type ItemId = ItemId;
    type NativeTokenDecimals = NativeTokenDecimals;
    type Nonfungibles = OctopusUniques;
    type PalletId = OctopusAppchainPalletId;
    type RuntimeEvent = RuntimeEvent;
    type Threshold = FeeTh;
    type UpwardMessagesInterface = OctopusUpwardMessages;
    type WeightInfo = pallet_octopus_bridge::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    pub const SessionsPerEra: sp_staking::SessionIndex = 6;
    pub const BondingDuration: pallet_octopus_lpos::EraIndex = 24 * 21;
    pub const BlocksPerEra: u32 = EPOCH_DURATION_IN_BLOCKS * 6;
}

impl pallet_octopus_lpos::Config for Runtime {
    type AppchainInterface = OctopusAppchain;
    type BondingDuration = BondingDuration;
    type Currency = Balances;
    type PalletId = OctopusAppchainPalletId;
    type RuntimeEvent = RuntimeEvent;
    type SessionInterface = Self;
    type SessionsPerEra = SessionsPerEra;
    type UnixTime = Timestamp;
    type UpwardMessagesInterface = OctopusUpwardMessages;
    type WeightInfo = pallet_octopus_lpos::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    pub const MaxMessagePayloadSize: u32 = 256;
    pub const MaxMessagesPerCommit: u32 = 20;
}

impl pallet_octopus_upward_messages::Config for Runtime {
    type Hashing = Keccak256;
    type MaxMessagePayloadSize = MaxMessagePayloadSize;
    type MaxMessagesPerCommit = MaxMessagesPerCommit;
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = pallet_octopus_upward_messages::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    pub const MultisigDepositBase: Balance = DOLLARS;
    pub const MultisigDepositFactor: Balance = MILLICENTS;
    pub const MaxSignatories: u16 = 5;
}

impl pallet_multisig::Config for Runtime {
    type Currency = Balances;
    type DepositBase = MultisigDepositBase;
    type DepositFactor = MultisigDepositFactor;
    type MaxSignatories = MaxSignatories;
    type RuntimeCall = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
}

parameter_types! {
    pub const NativeTokenId: u32 = 0;
    pub const NearChainId: ChainId = 255;
    pub const EthChainId: ChainId = 1;
    pub const BnbChainId: ChainId = 56;
    pub const NativeChainId: ChainId = 42;
    pub const BurnTAOwhenIssue: Balance = 10 * TAO;
}

pub type TokenId = u32;

impl pallet_fuso_token::Config for Runtime {
    type AdminOrigin = EnsureSignedBy<BridgeAdminMembers, Self::AccountId>;
    type BnbChainId = BnbChainId;
    type BurnTAOwhenIssue = BurnTAOwhenIssue;
    type EthChainId = EthChainId;
    type NativeChainId = NativeChainId;
    type NativeTokenId = NativeTokenId;
    type NearChainId = NearChainId;
    type RuntimeEvent = RuntimeEvent;
    type TokenId = TokenId;
    type Weight = pallet_fuso_token::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    pub const FusotaoChainId: u16 = 42;
    pub const ProposalLifetime: BlockNumber = 48 * HOURS;
}

impl pallet_chainbridge::Config for Runtime {
    type AdminOrigin = frame_system::EnsureSignedBy<BridgeAdminMembers, Self::AccountId>;
    type AssetIdByName = Token;
    type ChainId = FusotaoChainId;
    type Fungibles = Token;
    type NativeResourceId = NativeResourceId;
    type Proposal = RuntimeCall;
    type ProposalLifetime = ProposalLifetime;
    type RuntimeEvent = RuntimeEvent;
    type TreasuryAccount = TreasuryAccount;
}

impl pallet_fuso_indicator::Config for Runtime {
    type Asset = Token;
    type RuntimeEvent = RuntimeEvent;
}
pub const TREASURY: AccountId = AccountId::new(hex_literal::hex!(
    "36e5fc3abd178f8823ec53a94fb03873779fa85d61f03a95901a4bde1eca1626"
));

pub const BRIDGE_ADMIN: AccountId = AccountId::new(hex_literal::hex!(
    "9653992f8241ee51bb578076a1af026b74e08c1c0d53164fc880a4ae5442334c"
));

pub const BRIDGE_ADMIN1: AccountId = AccountId::new(hex_literal::hex!(
    "5e6197279237e048a53e59b25c50e7e48de9d1c76c7077d5f33b8fcde4af873f"
));

parameter_types! {
    pub NativeResourceId: fuso_support::chainbridge::ResourceId = derive_resource_id(FusotaoChainId::get(), 0, b"TAO".as_ref()).unwrap();
    pub NativeTokenMaxValue: Balance = 30_000_000 * TAO;
    pub DonorAccount: AccountId = AccountId::new([0u8; 32]);
    pub TreasuryAccount: AccountId = TREASURY;
    pub DonationForAgent: Balance = 1 * TAO;
}

pub struct BridgeAdminMembers;
impl SortedMembers<AccountId> for BridgeAdminMembers {
    fn sorted_members() -> Vec<AccountId> {
        vec![BRIDGE_ADMIN, BRIDGE_ADMIN1]
    }
}

impl pallet_chainbridge_handler::Config for Runtime {
    type BalanceConversion = Token;
    type BridgeOrigin = pallet_chainbridge::EnsureBridge<Runtime>;
    type DonationForAgent = DonationForAgent;
    type DonorAccount = DonorAccount;
    type NativeTokenMaxValue = NativeTokenMaxValue;
    type Oracle = Indicator;
    type Redirect = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
}

parameter_types! {
    pub const MainOrTestnet: u16 = 1;
}

impl pallet_fuso_agent::Config<pallet_fuso_agent::EthInstance> for Runtime {
    type Currency = Balances;
    type ExternalSignWrapper = pallet_fuso_agent::EthPersonalSignWrapper;
    type MainOrTestnet = MainOrTestnet;
    type RuntimeEvent = RuntimeEvent;
    type Transaction = RuntimeCall;
    type TransactionByteFee = TransactionByteFee;
    type WeightToFee = IdentityFee<Balance>;
}

parameter_types! {
    pub const AssetDeposit: Balance = 100 * DOLLARS;
    pub const ApprovalDeposit: Balance = 1 * DOLLARS;
    pub const StringLimit: u32 = 50;
    pub const MetadataDepositBase: Balance = 10 * DOLLARS;
    pub const MetadataDepositPerByte: Balance = 1 * DOLLARS;
}

parameter_types! {
    pub const Duration: BlockNumber = MONTHS;
}

impl pallet_fuso_foundation::Config for Runtime {
    type Duration = Duration;
    type RuntimeEvent = RuntimeEvent;
}

parameter_types! {
    pub const EraDuration: BlockNumber = DAYS;
    // TODO move this to storage and automatically calculate by the $tao price
    pub const RewardsPerEra: Balance = 6575 * DOLLARS;
    pub const RewardTerminateAt: BlockNumber = 1825 * DAYS;
}

impl pallet_fuso_reward::Config for Runtime {
    type Asset = Token;
    type EraDuration = EraDuration;
    type RewardTerminateAt = RewardTerminateAt;
    type RewardsPerEra = RewardsPerEra;
    type RuntimeEvent = RuntimeEvent;
}

parameter_types! {
    pub const DominatorOnlineThreshold: Balance = 80_000 * DOLLARS;
    pub const SeasonDuration: BlockNumber = 3 * DAYS;
    pub const MinimalStakingAmount: Balance = 100 * DOLLARS;
    pub const DominatorCheckGracePeriod: BlockNumber = 20;
    pub const MaxTakerFee: u32 = 10000;
    pub const MaxMakerFee: u32 = 10000;
}

const_assert!(DAYS % 20 == 0);

impl pallet_fuso_verifier::Config for Runtime {
    type Asset = Token;
    type Callback = RuntimeCall;
    type DominatorCheckGracePeriod = DominatorCheckGracePeriod;
    type DominatorOnlineThreshold = DominatorOnlineThreshold;
    type Indicator = Indicator;
    type MarketManager = Market;
    type MaxMakerFee = MaxMakerFee;
    type MaxTakerFee = MaxTakerFee;
    type MinimalStakingAmount = MinimalStakingAmount;
    type Rewarding = Reward;
    type RuntimeEvent = RuntimeEvent;
    type SeasonDuration = SeasonDuration;
    type WeightInfo = ();
}

parameter_types! {
    pub const BrokerStakingThreshold: Balance = 50000 * TAO;
    pub const MarketCloseGracePeriod: BlockNumber = 14400;
}

impl pallet_fuso_market::Config for Runtime {
    type Assets = Token;
    type BrokerStakingThreshold = BrokerStakingThreshold;
    type MarketCloseGracePeriod = MarketCloseGracePeriod;
    type RuntimeEvent = RuntimeEvent;
}

// Create the runtime by composing the FRAME pallets that were previously configured.
construct_runtime!(
    pub enum Runtime where
        Block = Block,
        NodeBlock = fuso_primitives::Block,
        UncheckedExtrinsic = UncheckedExtrinsic
    {
        System: frame_system,
        RandomnessCollectiveFlip: pallet_randomness_collective_flip,
        Timestamp: pallet_timestamp,
        Babe: pallet_babe,
        Authorship: pallet_authorship,
        Balances: pallet_balances,
        TransactionPayment: pallet_transaction_payment,
        Multisig: pallet_multisig,
        OctopusAppchain: pallet_octopus_appchain, // must before session
        OctopusBridge: pallet_octopus_bridge,
        OctopusLpos: pallet_octopus_lpos,
        OctopusUpwardMessages: pallet_octopus_upward_messages,
        OctopusUniques: pallet_uniques::<Instance1>,
        Session: pallet_session,
        Grandpa: pallet_grandpa,
        ImOnline: pallet_im_online,
        Offences: pallet_offences,
        Historical: pallet_session_historical::{Pallet},
        Mmr: pallet_mmr,
        Beefy: pallet_beefy,
        MmrLeaf: pallet_beefy_mmr,
        Sudo: pallet_sudo,
        Foundation: pallet_fuso_foundation,
        Token: pallet_fuso_token,
        ChainBridge: pallet_chainbridge,
        ChainBridgeHandler: pallet_chainbridge_handler,
        Reward: pallet_fuso_reward,
        Agent: pallet_fuso_agent::<EthInstance>,
        Indicator: pallet_fuso_indicator,
        Verifier: pallet_fuso_verifier,
        Market: pallet_fuso_market
    }
);

/// The address format for describing accounts.
pub type Address = sp_runtime::MultiAddress<AccountId, ()>;
/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
/// BlockId type as expected by this runtime.
pub type BlockId = generic::BlockId<Block>;
/// A Block signed with a Justification
pub type SignedBlock = generic::SignedBlock<Block>;

/// The SignedExtension to the basic transaction logic.
pub type SignedExtra = (
    frame_system::CheckNonZeroSender<Runtime>,
    frame_system::CheckSpecVersion<Runtime>,
    frame_system::CheckTxVersion<Runtime>,
    frame_system::CheckGenesis<Runtime>,
    frame_system::CheckEra<Runtime>,
    frame_system::CheckNonce<Runtime>,
    frame_system::CheckWeight<Runtime>,
    pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
);
/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic =
    generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;
/// The payload being signed in transactions.
pub type SignedPayload = generic::SignedPayload<RuntimeCall, SignedExtra>;
/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
    Runtime,
    Block,
    frame_system::ChainContext<Runtime>,
    Runtime,
    AllPalletsWithSystem,
    Migrations,
>;

type Migrations = (SetIntervalValueRuntimeUpgrade,);
/// Please set the value of interval according to your own needs.
const INTERVAL: u32 = 1;
pub struct SetIntervalValueRuntimeUpgrade;
impl frame_support::traits::OnRuntimeUpgrade for SetIntervalValueRuntimeUpgrade {
    fn on_runtime_upgrade() -> frame_support::weights::Weight {
        pallet_octopus_upward_messages::migrations::migration_to_v1::<Runtime>(INTERVAL)
    }
}

#[cfg(feature = "runtime-benchmarks")]
#[macro_use]
extern crate frame_benchmarking;

#[cfg(feature = "runtime-benchmarks")]
mod benches {
    define_benchmarks!(
        [frame_benchmarking, BaselineBench::<Runtime>]
        [frame_system, SystemBench::<Runtime>]
        [pallet_balances, Balances]
        [pallet_timestamp, Timestamp]
        [pallet_fuso_verifier, Verifier]
    );
}

pub type MmrHashing = <Runtime as pallet_mmr::Config>::Hashing;

impl_runtime_apis! {
    impl sp_api::Core<Block> for Runtime {
        fn version() -> RuntimeVersion {
            VERSION
        }

        fn execute_block(block: Block) {
            Executive::execute_block(block);
        }

        fn initialize_block(header: &<Block as BlockT>::Header) {
            Executive::initialize_block(header)
        }
    }

    impl sp_api::Metadata<Block> for Runtime {
        fn metadata() -> OpaqueMetadata {
            OpaqueMetadata::new(Runtime::metadata().into())
        }
    }

    impl sp_block_builder::BlockBuilder<Block> for Runtime {
        fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
            Executive::apply_extrinsic(extrinsic)
        }

        fn finalize_block() -> <Block as BlockT>::Header {
            Executive::finalize_block()
        }

        fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
            data.create_extrinsics()
        }

        fn check_inherents(
            block: Block,
            data: sp_inherents::InherentData,
        ) -> sp_inherents::CheckInherentsResult {
            data.check_extrinsics(&block)
        }
    }

    impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
        fn offchain_worker(header: &<Block as BlockT>::Header) {
            Executive::offchain_worker(header)
        }
    }

    impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
        fn validate_transaction(
            source: TransactionSource,
            tx: <Block as BlockT>::Extrinsic,
            block_hash: <Block as BlockT>::Hash,
        ) -> TransactionValidity {
            Executive::validate_transaction(source, tx, block_hash)
        }
    }

    impl fg_primitives::GrandpaApi<Block> for Runtime {
        fn grandpa_authorities() -> GrandpaAuthorityList {
            Grandpa::grandpa_authorities()
        }

        fn current_set_id() -> fg_primitives::SetId {
            Grandpa::current_set_id()
        }

        fn submit_report_equivocation_unsigned_extrinsic(
            equivocation_proof: fg_primitives::EquivocationProof<
                <Block as BlockT>::Hash,
                NumberFor<Block>,
            >,
            key_owner_proof: fg_primitives::OpaqueKeyOwnershipProof,
        ) -> Option<()> {
            let key_owner_proof = key_owner_proof.decode()?;
            Grandpa::submit_unsigned_equivocation_report(
                equivocation_proof,
                key_owner_proof,
            )
        }

        fn generate_key_ownership_proof(
            _set_id: fg_primitives::SetId,
            authority_id: GrandpaId,
        ) -> Option<fg_primitives::OpaqueKeyOwnershipProof> {
            use codec::Encode;

            Historical::prove((fg_primitives::KEY_TYPE, authority_id))
                .map(|p| p.encode())
                .map(fg_primitives::OpaqueKeyOwnershipProof::new)
        }
    }

    impl sp_consensus_babe::BabeApi<Block> for Runtime {
        fn configuration() -> sp_consensus_babe::BabeConfiguration {
            // The choice of `c` parameter (where `1 - c` represents the
            // probability of a slot being empty), is done in accordance to the
            // slot duration and expected target block time, for safely
            // resisting network delays of maximum two seconds.
            // <https://research.web3.foundation/en/latest/polkadot/BABE/Babe/#6-practical-results>
            let epoch_config = Babe::epoch_config().unwrap_or(BABE_GENESIS_EPOCH_CONFIG);
            sp_consensus_babe::BabeConfiguration {
                slot_duration: Babe::slot_duration(),
                epoch_length: EpochDuration::get(),
                c: epoch_config.c,
                authorities: Babe::authorities().to_vec(),
                randomness: Babe::randomness(),
                allowed_slots: epoch_config.allowed_slots,
            }
        }

        fn current_epoch_start() -> sp_consensus_babe::Slot {
            Babe::current_epoch_start()
        }

        fn current_epoch() -> sp_consensus_babe::Epoch {
            Babe::current_epoch()
        }

        fn next_epoch() -> sp_consensus_babe::Epoch {
            Babe::next_epoch()
        }

        fn generate_key_ownership_proof(
            _slot: sp_consensus_babe::Slot,
            authority_id: sp_consensus_babe::AuthorityId,
        ) -> Option<sp_consensus_babe::OpaqueKeyOwnershipProof> {
            use codec::Encode;

            Historical::prove((sp_consensus_babe::KEY_TYPE, authority_id))
                .map(|p| p.encode())
                .map(sp_consensus_babe::OpaqueKeyOwnershipProof::new)
        }

        fn submit_report_equivocation_unsigned_extrinsic(
            equivocation_proof: sp_consensus_babe::EquivocationProof<<Block as BlockT>::Header>,
            key_owner_proof: sp_consensus_babe::OpaqueKeyOwnershipProof,
        ) -> Option<()> {
            let key_owner_proof = key_owner_proof.decode()?;
            Babe::submit_unsigned_equivocation_report(
                equivocation_proof,
                key_owner_proof,
            )
        }
    }

    impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Index> for Runtime {
        fn account_nonce(account: AccountId) -> Index {
            System::account_nonce(account)
        }
    }

    impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, Balance> for Runtime {
        fn query_info(
            uxt: <Block as BlockT>::Extrinsic,
            len: u32,
        ) -> pallet_transaction_payment_rpc_runtime_api::RuntimeDispatchInfo<Balance> {
            TransactionPayment::query_info(uxt, len)
        }
        fn query_fee_details(
            uxt: <Block as BlockT>::Extrinsic,
            len: u32,
        ) -> pallet_transaction_payment::FeeDetails<Balance> {
            TransactionPayment::query_fee_details(uxt, len)
        }
    }

    impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentCallApi<Block, Balance, RuntimeCall>
        for Runtime
    {
        fn query_call_info(call: RuntimeCall, len: u32) -> RuntimeDispatchInfo<Balance> {
            TransactionPayment::query_call_info(call, len)
        }
        fn query_call_fee_details(call: RuntimeCall, len: u32) -> FeeDetails<Balance> {
            TransactionPayment::query_call_fee_details(call, len)
        }
    }

    impl beefy_primitives::BeefyApi<Block> for Runtime {
        fn validator_set() -> Option<beefy_primitives::ValidatorSet<BeefyId>> {
            Beefy::validator_set()
        }
    }

    impl beefy_merkle_tree::BeefyMmrApi<Block, Hash> for RuntimeApi {
        fn authority_set_proof() -> beefy_primitives::mmr::BeefyAuthoritySet<Hash> {
            MmrLeaf::authority_set_proof()
        }

        fn next_authority_set_proof() -> beefy_primitives::mmr::BeefyNextAuthoritySet<Hash> {
            MmrLeaf::next_authority_set_proof()
        }
    }


    impl mmr::MmrApi<Block, Hash> for Runtime {
        fn generate_proof(leaf_index: u64)
            -> Result<(mmr::EncodableOpaqueLeaf, mmr::Proof<Hash>), mmr::Error>
        {
            Mmr::generate_batch_proof(vec![leaf_index])
                .and_then(|(leaves, proof)| Ok((
                    mmr::EncodableOpaqueLeaf::from_leaf(&leaves[0]),
                    mmr::BatchProof::into_single_leaf_proof(proof)?
                )))
        }

        fn verify_proof(leaf: mmr::EncodableOpaqueLeaf, proof: mmr::Proof<Hash>)
            -> Result<(), mmr::Error>
        {
            pub type MmrLeaf = <<Runtime as pallet_mmr::Config>::LeafData as mmr::LeafDataProvider>::LeafData;
            let leaf: MmrLeaf = leaf
                .into_opaque_leaf()
                .try_decode()
                .ok_or(mmr::Error::Verify)?;
            Mmr::verify_leaves(vec![leaf], mmr::Proof::into_batch_proof(proof))
        }

        fn verify_proof_stateless(
            root: Hash,
            leaf: mmr::EncodableOpaqueLeaf,
            proof: mmr::Proof<Hash>
        ) -> Result<(), mmr::Error> {
            let node = mmr::DataOrHash::Data(leaf.into_opaque_leaf());
            pallet_mmr::verify_leaves_proof::<MmrHashing, _>(root, vec![node], mmr::Proof::into_batch_proof(proof))
        }

        fn mmr_root() -> Result<Hash, mmr::Error> {
            Ok(Mmr::mmr_root())
        }

        fn generate_batch_proof(leaf_indices: Vec<mmr::LeafIndex>)
            -> Result<(Vec<mmr::EncodableOpaqueLeaf>, mmr::BatchProof<Hash>), mmr::Error>
        {
            Mmr::generate_batch_proof(leaf_indices)
                .map(|(leaves, proof)| (leaves.into_iter().map(|leaf| mmr::EncodableOpaqueLeaf::from_leaf(&leaf)).collect(), proof))
        }

        fn verify_batch_proof(leaves: Vec<mmr::EncodableOpaqueLeaf>, proof: mmr::BatchProof<Hash>)
            -> Result<(), mmr::Error>
        {
            pub type MmrLeaf = <<Runtime as pallet_mmr::Config>::LeafData as mmr::LeafDataProvider>::LeafData;
            let leaves = leaves.into_iter().map(|leaf|
                leaf.into_opaque_leaf()
                .try_decode()
                .ok_or(mmr::Error::Verify)).collect::<Result<Vec<MmrLeaf>, mmr::Error>>()?;
            Mmr::verify_leaves(leaves, proof)
        }

        fn verify_batch_proof_stateless(
            root: Hash,
            leaves: Vec<mmr::EncodableOpaqueLeaf>,
            proof: mmr::BatchProof<Hash>
        ) -> Result<(), mmr::Error> {
            let nodes = leaves.into_iter().map(|leaf|mmr::DataOrHash::Data(leaf.into_opaque_leaf())).collect();
            pallet_mmr::verify_leaves_proof::<MmrHashing, _>(root, nodes, proof)
        }
    }

    impl sp_session::SessionKeys<Block> for Runtime {
        fn generate_session_keys(seed: Option<Vec<u8>>) -> Vec<u8> {
            opaque::SessionKeys::generate(seed)
        }

        fn decode_session_keys(
            encoded: Vec<u8>,
        ) -> Option<Vec<(Vec<u8>, KeyTypeId)>> {
            opaque::SessionKeys::decode_into_raw_public_keys(&encoded)
        }
    }

    impl fuso_verifier_runtime_api::FusoVerifierRuntimeApi<Block, AccountId, Balance> for Runtime {
        fn current_season_of_dominator(dominator: AccountId) -> u32 {
            Verifier::current_season_of_dominator(dominator)
        }

        fn pending_shares_of_dominator(dominator: AccountId, who: AccountId) -> Balance {
            Verifier::pending_shares_of_dominator(dominator, who)
        }
    }

    #[cfg(feature = "try-runtime")]
    impl frame_try_runtime::TryRuntime<Block> for Runtime {
        fn on_runtime_upgrade() -> (Weight, Weight) {
            // NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
            // have a backtrace here. If any of the pre/post migration checks fail, we shall stop
            // right here and right now.
            let weight = Executive::try_runtime_upgrade().unwrap();
            (weight, RuntimeBlockWeights::get().max_block)
        }

        fn execute_block_no_check(block: Block) -> Weight {
            Executive::execute_block_no_check(block)
        }
    }

    #[cfg(feature = "runtime-benchmarks")]
    impl frame_benchmarking::Benchmark<Block> for Runtime {
        fn benchmark_metadata(extra: bool) -> (
            Vec<frame_benchmarking::BenchmarkList>,
            Vec<frame_support::traits::StorageInfo>,
        ) {
            use frame_benchmarking::{baseline, Benchmarking, BenchmarkList};
            use frame_support::traits::StorageInfoTrait;
            use frame_system_benchmarking::Pallet as SystemBench;
            use baseline::Pallet as BaselineBench;
            let mut list = Vec::<BenchmarkList>::new();
            list_benchmark!(list, extra, frame_benchmarking, BaselineBench::<Runtime>);
            list_benchmark!(list, extra, frame_system, SystemBench::<Runtime>);
            list_benchmark!(list, extra, pallet_balances, Balances);
            list_benchmark!(list, extra, pallet_timestamp, Timestamp);
            list_benchmark!(list, extra, pallet_fuso_verifier, Verifier);
            list_benchmarks!(list, extra);
            let storage_info = AllPalletsWithSystem::storage_info();
            return (list, storage_info)
        }

        fn dispatch_benchmark(
            config: frame_benchmarking::BenchmarkConfig
        ) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, sp_runtime::RuntimeString> {
            use frame_benchmarking::{baseline, Benchmarking, BenchmarkBatch, TrackedStorageKey};
            use frame_system_benchmarking::Pallet as SystemBench;
            use baseline::Pallet as BaselineBench;
            impl frame_system_benchmarking::Config for Runtime {}
            impl baseline::Config for Runtime {}
            let whitelist: Vec<TrackedStorageKey> = vec![
                // Block Number
                hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac").to_vec().into(),
                // Total Issuance
                hex_literal::hex!("c2261276cc9d1f8598ea4b6a74b15c2f57c875e4cff74148e4628f264b974c80").to_vec().into(),
                // Execution Phase
                hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef7ff553b5a9862a516939d82b3d3d8661a").to_vec().into(),
                // Event Count
                hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef70a98fdbe9ce6c55837576c60c7af3850").to_vec().into(),
                // System Events
                hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef780d41e5e16056765bc8461851072c9d7").to_vec().into(),
            ];
            let mut batches = Vec::<BenchmarkBatch>::new();
            let params = (&config, &whitelist);
            add_benchmark!(params, batches, frame_benchmarking, BaselineBench::<Runtime>);
            add_benchmark!(params, batches, frame_system, SystemBench::<Runtime>);
            add_benchmark!(params, batches, pallet_balances, Balances);
            add_benchmark!(params, batches, pallet_timestamp, Timestamp);
            add_benchmark!(params, batches, pallet_fuso_verifier, Verifier);
            add_benchmarks!(params, batches);
            Ok(batches)
        }
    }
}
