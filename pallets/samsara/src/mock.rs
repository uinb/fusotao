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

use crate::{Module, Config};
use frame_support::traits::{OnFinalize, OnInitialize};
use frame_support::{
    impl_outer_origin, parameter_types,
    weights::{constants::WEIGHT_PER_SECOND, Weight},
};
use frame_system as system;
use pallet_balances as balances;
use sp_core::H256;
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentityLookup},
    Perbill,
};

pub const ALICE: <Test as system::Config>::AccountId = 1;
pub const BOB: <Test as system::Config>::AccountId = 2;

impl_outer_origin! {
    pub enum Origin for Test {}
}

// Configure a mock runtime to test the pallet.

#[derive(Clone, Eq, PartialEq)]
pub struct Test;
parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const MaximumBlockWeight: Weight = 1024;
    pub const MaximumBlockLength: u32 = 2 * 1024;
    pub const AvailableBlockRatio: Perbill = Perbill::from_percent(75);

    pub const ExistentialDeposit: u64 = 1;
    pub const TransferFee: u128 = 0;
    pub const CreationFee: u128 = 0;

    pub const VitalityBlock: u32 = 21;
    pub const TermDuration: u32 = 100;
    pub const VoteDuration: u32 = 60;
    pub const MinimumVitalityWeight: Weight = 7 * WEIGHT_PER_SECOND; // 7_000_000_000_000 Weight
    pub VoteBalancePerbill: Perbill = Perbill::from_rational_approximation(2u32, 3u32); // 2/3
}

impl system::Config for Test {
    type BaseCallFilter = ();
    type Origin = Origin;
    type Call = ();
    type Index = u64;
    type BlockNumber = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type Event = ();
    type BlockHashCount = BlockHashCount;
    type MaximumBlockWeight = MaximumBlockWeight;
    type DbWeight = ();
    type BlockExecutionWeight = ();
    type ExtrinsicBaseWeight = ();
    type MaximumExtrinsicWeight = MaximumBlockWeight;
    type MaximumBlockLength = MaximumBlockLength;
    type AvailableBlockRatio = AvailableBlockRatio;
    type Version = ();
    type PalletInfo = ();
    type AccountData = pallet_balances::AccountData<u64>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
}

impl balances::Config for Test {
    type Balance = u64;
    type MaxLocks = ();
    type Event = ();
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = ();
}

impl Config for Test {
    type Event = ();
    type VitalityBlock = VitalityBlock;
    type TermDuration = TermDuration;
    type VoteDuration = VoteDuration;
    type MinimumVitalityWeight = MinimumVitalityWeight;
    type VoteBalancePerbill = VoteBalancePerbill;
    type Currency = balances::Module<Self>;
    type Locks = Test;
}

pub type SamsaraModule = Module<Test>;
pub type System = frame_system::Module<Test>;
pub type Balances = pallet_balances::Module<Test>;

pub fn run_to_block(block: u64) {
    while System::block_number() < block {
        SamsaraModule::on_finalize(System::block_number());
        System::on_finalize(System::block_number());
        System::set_block_number(System::block_number() + 1);
        System::on_initialize(System::block_number());
        System::set_block_limits(5_290_048_000, 1);
        SamsaraModule::on_initialize(System::block_number());
    }
}

// Build genesis storage according to the mock runtime.
pub fn samsara_test_ext() -> sp_io::TestExternalities {
    let mut t = system::GenesisConfig::default()
        .build_storage::<Test>()
        .unwrap()
        .into();

    balances::GenesisConfig::<Test> {
        balances: vec![(ALICE, 60000000000), (BOB, 610000000000)],
    }
    .assimilate_storage(&mut t)
    .unwrap();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}
