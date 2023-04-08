use crate as pallet_fuso_verifier;
use frame_support::traits::{ConstU32, SortedMembers};
use frame_support::{construct_runtime, parameter_types};
use frame_system as system;
use frame_system::EnsureSignedBy;
use fuso_support::ChainId;
use sp_keyring::AccountKeyring;
use sp_runtime::traits::{IdentifyAccount, Verify};
use sp_runtime::{
    generic,
    traits::{AccountIdLookup, BlakeTwo256},
    MultiSignature,
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
pub(crate) type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

pub(crate) type BlockNumber = u32;
pub type Signature = MultiSignature;
pub type Balance = u128;
pub type Moment = u64;
pub type Index = u64;
pub type Hash = sp_core::H256;

pub const MILLICENTS: Balance = 10_000_000_000;
pub const CENTS: Balance = 1_000 * MILLICENTS;
pub const DOLLARS: Balance = 100 * CENTS;
pub const MILLISECS_PER_BLOCK: Moment = 3000;
pub const SECS_PER_BLOCK: Moment = MILLISECS_PER_BLOCK / 1000;
pub const SLOT_DURATION: Moment = MILLISECS_PER_BLOCK;
pub const EPOCH_DURATION_IN_BLOCKS: BlockNumber = 1 * MINUTES;
pub const MINUTES: BlockNumber = 60 / (SECS_PER_BLOCK as BlockNumber);

parameter_types! {
    pub const BlockHashCount: BlockNumber = 250;
    pub const SS58Prefix: u8 = 42;
}

impl frame_system::Config for Test {
    type AccountData = pallet_balances::AccountData<Balance>;
    type AccountId = AccountId;
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockHashCount = BlockHashCount;
    type BlockLength = ();
    type BlockNumber = BlockNumber;
    type BlockWeights = ();
    type DbWeight = ();
    type Hash = Hash;
    type Hashing = BlakeTwo256;
    type Header = generic::Header<BlockNumber, BlakeTwo256>;
    type Index = Index;
    type Lookup = AccountIdLookup<AccountId, ()>;
    type MaxConsumers = ConstU32<16>;
    type OnKilledAccount = ();
    type OnNewAccount = ();
    type OnSetCode = ();
    type PalletInfo = PalletInfo;
    type RuntimeCall = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type RuntimeOrigin = RuntimeOrigin;
    type SS58Prefix = SS58Prefix;
    type SystemWeightInfo = ();
    type Version = ();
}

parameter_types! {
    pub const ExistentialDeposit: Balance = 1;
    pub const MaxLocks: u32 = 50;
    pub const MaxReserves: u32 = 50;
}
impl pallet_balances::Config for Test {
    type AccountStore = System;
    type Balance = Balance;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type MaxLocks = MaxLocks;
    type MaxReserves = MaxReserves;
    type ReserveIdentifier = [u8; 8];
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
}

parameter_types! {
    pub const NativeTokenId: u32 = 0;
    pub const NearChainId: ChainId = 255;
    pub const EthChainId: ChainId = 1;
    pub const BnbChainId: ChainId = 2;
    pub const PolygonChainId: ChainId = 3;
    pub const NativeChainId: ChainId = 42;
    pub const BurnTAOwhenIssue: Balance = 10000000000000000;
}

pub const TREASURY: AccountId = AccountId::new([5u8; 32]);
pub struct TreasuryMembers;
impl SortedMembers<AccountId> for TreasuryMembers {
    fn sorted_members() -> Vec<AccountId> {
        vec![TREASURY.clone()]
    }
}
impl pallet_fuso_token::Config for Test {
    type AdminOrigin = EnsureSignedBy<TreasuryMembers, Self::AccountId>;
    type BnbChainId = BnbChainId;
    type BurnTAOwhenIssue = BurnTAOwhenIssue;
    type EthChainId = EthChainId;
    type NativeChainId = NativeChainId;
    type NativeTokenId = NativeTokenId;
    type NearChainId = NearChainId;
    type PolygonChainId = PolygonChainId;
    type RuntimeEvent = RuntimeEvent;
    type TokenId = u32;
    type Weight = ();
}

parameter_types! {
    pub const DominatorOnlineThreshold: Balance = 10_000;
    pub const SeasonDuration: BlockNumber = 1440;
    pub const MinimalStakingAmount: Balance = 100;
    pub const DominatorCheckGracePeriod: BlockNumber = 10;
    pub const MaxMakerFee: u32 = 10000;
    pub const MaxTakerFee: u32 = 10000;
}

pub struct PhantomData;

impl fuso_support::traits::Rewarding<AccountId, Balance, BlockNumber> for PhantomData {
    type Balance = Balance;

    fn era_duration() -> BlockNumber {
        1
    }

    fn total_volume(_at: BlockNumber) -> Balance {
        100 * DOLLARS
    }

    fn acked_reward(_who: &AccountId) -> Self::Balance {
        0
    }

    fn save_trading(
        _trader: &AccountId,
        _amount: Balance,
        _at: BlockNumber,
    ) -> frame_support::pallet_prelude::DispatchResult {
        Ok(())
    }
}

impl pallet_fuso_indicator::Config for Test {
    type Asset = TokenModule;
    type RuntimeEvent = RuntimeEvent;
}

impl pallet_fuso_verifier::Config for Test {
    type Asset = TokenModule;
    type BrokerBeneficiary = ();
    type Callback = RuntimeCall;
    type DominatorCheckGracePeriod = DominatorCheckGracePeriod;
    type DominatorOnlineThreshold = DominatorOnlineThreshold;
    type Indicator = Indicator;
    type MarketManager = ();
    type MaxMakerFee = MaxMakerFee;
    type MaxTakerFee = MaxTakerFee;
    type MinimalStakingAmount = MinimalStakingAmount;
    type Rewarding = PhantomData;
    type RuntimeEvent = RuntimeEvent;
    type SeasonDuration = SeasonDuration;
    type WeightInfo = ();
}

// Configure a mock runtime to test the pallet.
construct_runtime!(
    pub enum Test where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: frame_system,
        Balances: pallet_balances,
        TokenModule: pallet_fuso_token,
        Indicator: pallet_fuso_indicator,
        Verifier: pallet_fuso_verifier
    }
);

// Build genesis storage according to the mock runtime.
pub fn new_tester() -> sp_io::TestExternalities {
    let mut t = system::GenesisConfig::default()
        .build_storage::<Test>()
        .unwrap();

    let ferdie = AccountKeyring::Ferdie.into();
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(ferdie, 10000000000000000000), (TREASURY, 1000 * DOLLARS)],
    }
    .assimilate_storage(&mut t)
    .unwrap();

    sp_io::TestExternalities::new(t)
}
