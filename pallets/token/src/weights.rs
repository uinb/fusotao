#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use sp_std::marker::PhantomData;

pub trait WeightInfo {
	fn transfer() -> Weight;
}

pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	// r:1, w2;
	fn transfer() -> Weight {
		Weight::from_ref_time(71_780_000u64)
			.saturating_add(RocksDbWeight::get().reads(2u64))
			.saturating_add(RocksDbWeight::get().writes(2u64))
	}
}

impl WeightInfo for () {
	// r:1, w2;
	fn transfer() -> Weight {
		Weight::from_ref_time(71_780_000u64)
			.saturating_add(RocksDbWeight::get().reads(2u64))
			.saturating_add(RocksDbWeight::get().writes(2u64))
	}

}
