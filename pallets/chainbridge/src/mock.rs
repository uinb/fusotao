#![cfg(test)]

use super::*;

use frame_support::traits::SortedMembers;
use frame_support::{assert_ok, parameter_types};
pub use frame_support::{
    construct_runtime,
    pallet_prelude::GenesisBuild,
    traits::{
        ConstU128, ConstU32, Hooks, KeyOwnerProofSystem, OnFinalize, OnInitialize, Randomness,
        StorageInfo,
    },
    weights::IdentityFee,
    PalletId, StorageValue,
};
use frame_system::{self as system};
use sp_core::H256;
use sp_runtime::{
    generic,
    traits::{AccountIdConversion, AccountIdLookup, BlakeTwo256, IdentifyAccount, Verify},
    AccountId32, MultiSignature,
};

use crate::{self as pallet_chainbridge, Config};
pub use pallet_balances;

pub type BlockNumber = u32;
pub type Balance = u128;
pub type Index = u64;
pub type Hash = H256;
pub type Signature = MultiSignature;
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

pub const MILLICENTS: Balance = 10_000_000_000;
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
    pub const TestChainId: ChainId = 5;
    pub const ProposalLifetime: u32 = 50;
    pub const NativeTokenId: u32 = 0;
    pub const NearChainId: ChainId = 255;
    pub const EthChainId: ChainId = 1;
    pub const BnbChainId: ChainId = 2;
    pub const NativeChainId: ChainId = 42;
    pub NativeResourceId: ResourceId = derive_resource_id(42, 0, b"TAO").unwrap(); // native token id
    pub const TreasuryAccount: AccountId = AccountId::new([5u8; 32]);
    pub const BurnTAOwhenIssue: Balance = 10_000_000_000_000_000_000;
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
    type RuntimeEvent = RuntimeEvent;
    type TokenId = u32;
    type Weight = ();
}

impl Config for Test {
    type AdminOrigin = frame_system::EnsureSignedBy<TreasuryMembers, Self::AccountId>;
    type AssetIdByName = Token;
    type ChainId = TestChainId;
    type Fungibles = Token;
    type NativeResourceId = NativeResourceId;
    type Proposal = RuntimeCall;
    type ProposalLifetime = ProposalLifetime;
    type RuntimeEvent = RuntimeEvent;
    type TreasuryAccount = TreasuryAccount;
}

type Block = frame_system::mocking::MockBlock<Test>;
type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;

construct_runtime!(
    pub enum Test where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic
    {
        System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
        Balances: pallet_balances::{Pallet, Call, Config<T>, Storage, Event<T>},
        Bridge: pallet_chainbridge::{Pallet, Call, Storage, Event<T>},
        Token: pallet_fuso_token::{Pallet, Call, Storage, Event<T>},
    }
);

pub const RELAYER_A: AccountId32 = AccountId32::new([2u8; 32]);
pub const RELAYER_B: AccountId32 = AccountId32::new([3u8; 32]);
pub const RELAYER_C: AccountId32 = AccountId32::new([4u8; 32]);
pub const TREASURY: AccountId32 = AccountId32::new([5u8; 32]);
pub const ENDOWED_BALANCE: Balance = 100 * DOLLARS;
pub const TEST_THRESHOLD: u32 = 2;

pub fn new_test_ext() -> sp_io::TestExternalities {
    let bridge_id = PalletId(*b"oc/bridg").try_into_account().unwrap();
    let mut t = frame_system::GenesisConfig::default()
        .build_storage::<Test>()
        .unwrap();
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(bridge_id, ENDOWED_BALANCE)],
    }
    .assimilate_storage(&mut t)
    .unwrap();
    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

pub fn new_test_ext_initialized(
    src_id: ChainId,
    r_id: ResourceId,
    resource: Vec<u8>,
) -> sp_io::TestExternalities {
    let mut t = new_test_ext();
    t.execute_with(|| {
        let treasury: AccountId = AccountId::new([5u8; 32]);
        // Set and check threshold
        assert_ok!(Bridge::set_threshold(
            RuntimeOrigin::signed(treasury.clone()),
            TEST_THRESHOLD
        ));
        assert_eq!(Bridge::relayer_threshold(), TEST_THRESHOLD);
        // Add relayers
        assert_ok!(Bridge::add_relayer(
            RuntimeOrigin::signed(treasury.clone()),
            RELAYER_A
        ));
        assert_ok!(Bridge::add_relayer(
            RuntimeOrigin::signed(treasury.clone()),
            RELAYER_B
        ));
        assert_ok!(Bridge::add_relayer(
            RuntimeOrigin::signed(treasury.clone()),
            RELAYER_C
        ));
        // Whitelist chain
        assert_ok!(Bridge::whitelist_chain(
            RuntimeOrigin::signed(treasury.clone()),
            src_id
        ));
        let (chainid, _, contract) = decode_resource_id(r_id);
        assert_ok!(Token::associate_token(
            RuntimeOrigin::signed(treasury.clone()),
            chainid,
            contract,
            1u32
        ));
        // Set and check resource ID mapped to some junk data
        assert_ok!(Bridge::set_resource(
            RuntimeOrigin::signed(treasury.clone()),
            r_id,
            resource
        ));
        assert_eq!(Bridge::resource_exists(r_id), true);
    });
    t
}

// Checks events against the latest. A contiguous set of events must be provided. They must
// include the most recent event, but do not have to include every past event.
pub fn assert_events(mut expected: Vec<RuntimeEvent>) {
    let mut actual: Vec<RuntimeEvent> = system::Pallet::<Test>::events()
        .iter()
        .map(|e| e.event.clone())
        .collect();

    expected.reverse();

    for evt in expected {
        let next = actual.pop().expect("event expected");
        assert_eq!(next, evt.into(), "Events don't match (actual,expected)");
    }
}
