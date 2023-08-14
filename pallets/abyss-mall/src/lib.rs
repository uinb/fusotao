// Copyright 2021-2023 UINB Technologies Pte. Ltd.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![cfg_attr(not(feature = "std"), no_std)]
pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use ascii::AsciiStr;
    use chrono::NaiveDateTime;
    use frame_support::traits::fungibles::Mutate;
    use frame_support::traits::{tokens::BalanceConversion, Time};
    use frame_support::{pallet_prelude::*, transactional};
    use frame_system::pallet_prelude::*;
    use fuso_support::chainbridge::*;
    use fuso_support::traits::{DecimalsTransformer, PriceOracle, ReservableToken, Token};
    use pallet_chainbridge as bridge;
    use sp_core::bounded::BoundedBTreeMap;
    use sp_runtime::traits::{TrailingZeroInput, Zero};
    use sp_runtime::Perquintill;
    use sp_std::vec;
    use sp_std::vec::Vec;

    type BalanceOf<T> = <<T as bridge::Config>::Fungibles as Token<
        <T as frame_system::Config>::AccountId,
    >>::Balance;

    type AssetId<T> = <<T as bridge::Config>::Fungibles as Token<
        <T as frame_system::Config>::AccountId,
    >>::TokenId;

    #[pallet::config]
    pub trait Config: frame_system::Config + bridge::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Assets: ReservableToken<Self::AccountId>;

        type BalanceConversion: BalanceConversion<BalanceOf<Self>, AssetId<Self>, BalanceOf<Self>>;
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub (super) fn deposit_event)]
    pub enum Event<T: Config> {}

    #[pallet::error]
    pub enum Error<T> {}

    #[pallet::pallet]
    #[pallet::without_storage_info]
    #[pallet::generate_store(pub (super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    #[pallet::call]
    impl<T: Config> Pallet<T> {}
}
