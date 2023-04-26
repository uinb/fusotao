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

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use fuso_support::traits::{PriceOracle, Token};
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
    pub struct Price<Balance, BlockNumber> {
        pub price: Balance,
        pub recent_matched_amount: Balance,
        pub recent_matched_vol: Balance,
        pub updated_at: BlockNumber,
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

    impl<T: Config> PriceOracle<TokenId<T>, Balance<T>, BlockNumberFor<T>> for Pallet<T>
    where
        Balance<T>: From<u128> + From<u64> + Into<u128>,
    {
        fn get_price(token_id: &TokenId<T>) -> Balance<T> {
            if T::Asset::is_stable(token_id) {
                QUINTILL.into()
            } else {
                Self::prices(token_id).price
            }
        }

        fn set_price(
            token_id: TokenId<T>,
            amount: Balance<T>,
            volume: Balance<T>,
            at: BlockNumberFor<T>,
        ) {
            OnChainPrices::<T>::mutate(&token_id, |p| {
                if !volume.is_zero() && !amount.is_zero() {
                    p.updated_at = at;
                    p.recent_matched_amount = amount;
                    p.recent_matched_vol = volume;
                    p.price = volume / amount * QUINTILL.into()
                        + (Perquintill::from_rational(volume % amount, amount).deconstruct()
                            as u128)
                            .into();
                }
            });
        }
    }
}
