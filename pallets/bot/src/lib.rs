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
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use fuso_support::traits::Token;
    use sp_runtime::{traits::Zero, Perquintill};

    pub type TokenId<T> =
        <<T as Config>::Asset as Token<<T as frame_system::Config>::AccountId>>::TokenId;

    pub type Balance<T> =
        <<T as Config>::Asset as Token<<T as frame_system::Config>::AccountId>>::Balance;

    const QUINTILL: u128 = 1_000_000_000_000_000_000;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Asset: Token<Self::AccountId>;
    }

    #[pallet::event]
    pub enum Event<T: Config> {
        PriceUpdated(TokenId<T>, Balance<T>),
    }

    #[pallet::error]
    pub enum Error<T> {}

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    #[derive(Encode, Decode, Clone, PartialEq, Eq, Default, TypeInfo, Debug)]
    pub struct Bot<AccountId, Balance, TokenId> {
        pub proxy: AccountId,
        pub staked: Balance,
        pub symbol: (TokenId, TokenId),
        pub desc: Vec<u8>,
    }

    #[pallet::storage]
    #[pallet::getter(fn prices)]
    pub type OnChainPrices<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        TokenId<T>,
        Price<Balance<T>, BlockNumberFor<T>>,
        ValueQuery,
    >;

    #[pallet::pallet]
    #[pallet::without_storage_info]
    #[pallet::generate_store(pub (super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::call]
    impl<T: Config> Pallet<T> {}
}
