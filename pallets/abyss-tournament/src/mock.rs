use crate as pallet_abyss_tournament;
use fuso_support::{chainbridge::*, ChainId};
use pallet_chainbridge as bridge;
use sp_keyring::AccountKeyring;
use sp_runtime::{
    generic,
    traits::{AccountIdLookup, BlakeTwo256, IdentifyAccount, Verify},
    MultiSignature,
};

use frame_support::traits::SortedMembers;
pub use frame_support::{
    construct_runtime,
    pallet_prelude::GenesisBuild,
    parameter_types,
    traits::{
        ConstU128, ConstU32, Hooks, KeyOwnerProofSystem, OnFinalize, OnInitialize, Randomness,
        StorageInfo,
    },
    weights::{IdentityFee, Weight},
    PalletId, StorageValue,
};
use sp_runtime::{traits::AccountIdConversion, AccountId32};

pub(crate) type BlockNumber = u32;
pub type Signature = MultiSignature;
pub type Balance = u128;
pub type Index = u64;
pub type Hash = sp_core::H256;
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

pub const MILLICENTS: Balance = 10_000_000_000_000;
pub const CENTS: Balance = 1_000 * MILLICENTS;
pub const DOLLARS: Balance = 100 * CENTS;

parameter_types! {
    pub const BlockHashCount: BlockNumber = 2400;
    pub const SS58Prefix: u16 = 42;
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

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
    pub enum Test where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic
    {
        System: frame_system,
        Timestamp: pallet_timestamp,
        Assets: pallet_fuso_token,
        Balances: pallet_balances,
        Bridge: pallet_chainbridge,
        Verifier: pallet_fuso_verifier,
        Indicator: pallet_fuso_indicator,
        Tournament: pallet_abyss_tournament,
    }
);

parameter_types! {
    pub const TestChainId: u8 = 42;
    pub const ProposalLifetime: u32 = 50;
    pub const TreasuryAccount: AccountId = AccountId::new([5u8;32]);
}

impl bridge::Config for Test {
    type AdminOrigin = frame_system::EnsureSignedBy<TreasuryMembers, Self::AccountId>;
    type AssetIdByName = Assets;
    type ChainId = TestChainId;
    type Fungibles = Assets;
    type NativeResourceId = NativeResourceId;
    type Proposal = RuntimeCall;
    type ProposalLifetime = ProposalLifetime;
    type RuntimeEvent = RuntimeEvent;
    type TreasuryAccount = TreasuryAccount;
}

parameter_types! {
    pub const ExistentialDeposit: Balance = 1 * DOLLARS;
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
    pub const MinimumPeriod: Moment = 3000;
}

pub type Moment = u64;

impl pallet_timestamp::Config for Test {
    type MinimumPeriod = MinimumPeriod;
    type Moment = Moment;
    type OnTimestampSet = ();
    type WeightInfo = ();
}

parameter_types! {
    pub const NativeTokenId: u32 = 0;
    pub const NearChainId: ChainId = 255;
    pub const EthChainId: ChainId = 1;
    pub const BnbChainId: ChainId = 2;
    pub const PolygonChainId: ChainId = 3;
    pub const NativeChainId: ChainId = 42;
    pub const BurnTAOwhenIssue: Balance =10_000_000_000_000_000_000;
}

pub struct TreasuryMembers;
impl SortedMembers<AccountId> for TreasuryMembers {
    fn sorted_members() -> Vec<AccountId> {
        vec![TREASURY]
    }
}
impl pallet_fuso_token::Config for Test {
    type AdminOrigin = frame_system::EnsureSignedBy<TreasuryMembers, Self::AccountId>;
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
    pub NativeResourceId: ResourceId = derive_resource_id(42, 0, b"TAO").unwrap(); // native token id
    pub NativeTokenMaxValue : Balance = 1000_000_000_000_000_0000u128; // need to set correct value
    pub DonorAccount: AccountId32 = AccountId32::new([0u8; 32]);
    pub DonationForAgent : Balance = 100_000_000_000_000_000u128; // need to set correct value
}

pub struct PhantomData;

impl fuso_support::traits::Rewarding<AccountId, Balance, (u32, u32), BlockNumber> for PhantomData {
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

    fn put_liquidity(_maker: &AccountId, _symbol: (u32, u32), _vol: Balance, _at: BlockNumber) {}

    /// when liquidity is took out, the liquidity provider will get the reward.
    /// the rewards are calculated in the formula below:
    /// contribution ƒi = vol * min(current - from, era_duration) / 720
    /// rewards of contribution ∂ = ƒi / ∑ƒi * era_rewards
    /// NOTE: `vol` should be volume rather than amount
    fn consume_liquidity(
        _maker: &AccountId,
        _symbol: (u32, u32),
        _vol: Balance,
        _current: BlockNumber,
    ) -> frame_support::pallet_prelude::DispatchResult {
        Ok(())
    }

    /// remove liquidity
    fn remove_liquidity(
        _maker: &AccountId,
        _symbol: (u32, u32),
        _vol: Balance,
    ) -> Result<BlockNumber, frame_support::pallet_prelude::DispatchError> {
        Ok(1)
    }
}

parameter_types! {
    pub const DominatorOnlineThreshold: Balance = 1_000_000;
    pub const SeasonDuration: BlockNumber = 1440;
    pub const MinimalStakingAmount: Balance = 100;
    pub const DominatorCheckGracePeriod: BlockNumber = 1;
    pub const MaxMakerFee: u32 = 10000;
    pub const MaxTakerFee: u32 = 10000;
}

impl pallet_fuso_verifier::Config for Test {
    type Asset = Assets;
    type BrokerBeneficiary = ();
    type Callback = RuntimeCall;
    type DominatorCheckGracePeriod = DominatorCheckGracePeriod;
    type DominatorOnlineThreshold = DominatorOnlineThreshold;
    type Indicator = ();
    type MarketManager = ();
    type MaxMakerFee = MaxMakerFee;
    type MaxTakerFee = MaxTakerFee;
    type MinimalStakingAmount = MinimalStakingAmount;
    type Rewarding = PhantomData;
    type RuntimeEvent = RuntimeEvent;
    type SeasonDuration = SeasonDuration;
    type WeightInfo = ();
}

pub type AssetBalance = u128;
pub type AssetId = u32;

impl pallet_fuso_indicator::Config for Test {
    type Asset = Assets;
    type RuntimeEvent = RuntimeEvent;
}

parameter_types! {
    pub const AwtTokenId: u32 = 1;
    pub const MaxTicketAmount: u32 = 100;
    pub const MaxPariticipantPerBattle:u32 = 10000000;
    pub const BvbTreasury: AccountId32 = AccountId32::new([6u8; 32]);
    pub const DefaultMinBetingAmount: Balance = 20* TAO;
}

impl pallet_abyss_tournament::Config for Test {
    type Assets = Assets;
    type AwtTokenId = AwtTokenId;
    type BalanceConversion = Assets;
    type BridgeOrigin = bridge::EnsureBridge<Test>;
    type BvbOrganizer = TreasuryAccount;
    type BvbTreasury = BvbTreasury;
    type DefaultMinBetingAmount = DefaultMinBetingAmount;
    type DonationForAgent = DonationForAgent;
    type DonorAccount = DonorAccount;
    type MaxParticipantPerBattle = MaxPariticipantPerBattle;
    type MaxTicketAmount = MaxTicketAmount;
    type Oracle = ();
    type OrganizerOrigin = frame_system::EnsureSignedBy<TreasuryMembers, Self::AccountId>;
    type RuntimeEvent = RuntimeEvent;
    type SwapPoolAccount = TreasuryAccount;
    type TimeProvider = Timestamp;
}

pub const RELAYER_A: AccountId32 = AccountId32::new([2u8; 32]);
pub const RELAYER_B: AccountId32 = AccountId32::new([3u8; 32]);
pub const RELAYER_C: AccountId32 = AccountId32::new([4u8; 32]);
pub const TREASURY: AccountId32 = AccountId32::new([5u8; 32]);
pub const BVBTREASURY: AccountId32 = AccountId32::new([6u8; 32]);
pub const ENDOWED_BALANCE: Balance = 100 * DOLLARS;

pub fn new_test_ext() -> sp_io::TestExternalities {
    let bridge_id = PalletId(*b"oc/bridg").try_into_account().unwrap();
    let mut storage = frame_system::GenesisConfig::default()
        .build_storage::<Test>()
        .unwrap();
    let alice: AccountId32 = AccountKeyring::Alice.into();
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![
            (bridge_id, ENDOWED_BALANCE),
            (RELAYER_A, ENDOWED_BALANCE),
            (alice, ENDOWED_BALANCE),
            (TREASURY, ENDOWED_BALANCE),
        ],
    }
    .assimilate_storage(&mut storage)
    .unwrap();

    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

fn last_event() -> RuntimeEvent {
    frame_system::Pallet::<Test>::events()
        .pop()
        .map(|e| e.event)
        .expect("Event expected")
}

pub fn expect_event<E: Into<RuntimeEvent>>(e: E) {
    assert_eq!(last_event(), e.into());
}

// Asserts that the event was emitted at some point.
pub fn event_exists<E: Into<RuntimeEvent>>(e: E) {
    let actual: Vec<RuntimeEvent> = frame_system::Pallet::<Test>::events()
        .iter()
        .map(|e| e.event.clone())
        .collect();
    let e: RuntimeEvent = e.into();
    let mut exists = false;
    for evt in actual {
        if evt == e {
            exists = true;
            break;
        }
    }
    assert!(exists);
}

// Checks events against the latest. A contiguous set of events must be provided. They must
// include the most recent event, but do not have to include every past event.
pub fn assert_events(mut expected: Vec<RuntimeEvent>) {
    let mut actual: Vec<RuntimeEvent> = frame_system::Pallet::<Test>::events()
        .iter()
        .map(|e| e.event.clone())
        .collect();

    expected.reverse();

    for evt in expected {
        let next = actual.pop().expect("event expected");
        assert_eq!(next, evt.into(), "Events don't match");
    }
}
