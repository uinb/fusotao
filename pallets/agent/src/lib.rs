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
#[cfg(test)]
pub mod mock;
#[cfg(test)]
pub mod tests;

extern crate alloc;

use codec::{Codec, Encode};
use frame_support::dispatch::Dispatchable;
use fuso_support::ExternalSignWrapper;
use sp_std::{boxed::Box, vec::Vec};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct EthInstance;

pub struct EthPersonalSignWrapper;

impl<T: frame_system::Config> ExternalSignWrapper<T> for EthPersonalSignWrapper {
    fn extend_payload<W: Dispatchable<RuntimeOrigin = T::RuntimeOrigin> + Codec>(
        nonce: T::Index,
        tx: Box<W>,
    ) -> Vec<u8> {
        let encoded_payload = (nonce, tx).using_encoded(|v| v.to_vec());
        [
            &[0x19u8][..],
            &alloc::format!(
                "Ethereum Signed Message:\n{}{}",
                encoded_payload.len() * 2,
                hex::encode(encoded_payload)
            )
            .as_bytes()[..],
        ]
        .concat()
    }
}

#[frame_support::pallet]
pub mod pallet {
    use frame_support::{
        dispatch::{DispatchInfo, Dispatchable, GetDispatchInfo},
        pallet_prelude::*,
        traits::{Currency, ExistenceRequirement, Get, WithdrawReasons},
        weights::{Weight, WeightToFee},
    };
    use frame_system::pallet_prelude::*;
    pub use fuso_support::external_chain::{ChainId, ExternalSignWrapper};
    use sp_runtime::traits::{Saturating, TrailingZeroInput, Zero};
    use sp_std::boxed::Box;

    pub type BalanceOf<T, I = ()> =
        <<T as Config<I>>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    #[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
    pub enum ExternalVerifiable<Index, Call> {
        Ed25519 {
            public: [u8; 32],
            tx: Box<Call>,
            nonce: Index,
            signature: [u8; 64],
        },
        Ecdsa {
            tx: Box<Call>,
            nonce: Index,
            signature: [u8; 65],
        },
    }

    fn dispatch_info_of<T: Config<I>, I: 'static>(
        verifiable: &ExternalVerifiable<T::Index, T::Transaction>,
    ) -> DispatchInfo {
        match verifiable {
            ExternalVerifiable::Ed25519 {
                public: _p, ref tx, ..
            } => tx.get_dispatch_info(),
            ExternalVerifiable::Ecdsa { ref tx, .. } => tx.get_dispatch_info(),
        }
    }

    #[pallet::config]
    pub trait Config<I: 'static = ()>: frame_system::Config {
        type RuntimeEvent: From<Event<Self, I>>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Transaction: Parameter
            + Dispatchable<RuntimeOrigin = Self::RuntimeOrigin>
            + GetDispatchInfo;

        type WeightToFee: WeightToFee<Balance = BalanceOf<Self, I>>;

        type TransactionByteFee: Get<BalanceOf<Self, I>>;

        type Currency: Currency<Self::AccountId>;

        type ExternalSignWrapper: ExternalSignWrapper<Self>;

        type MainOrTestnet: Get<u16>;
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub (super) fn deposit_event)]
    pub enum Event<T: Config<I>, I: 'static = ()> {
        ExternalTransactionExecuted(T::AccountId, DispatchResult),
    }

    #[pallet::error]
    pub enum Error<T, I = ()> {
        InvalidSignature,
    }

    #[pallet::pallet]
    #[pallet::without_storage_info]
    #[pallet::generate_store(pub (super) trait Store)]
    pub struct Pallet<T, I = ()>(_);

    #[pallet::call]
    impl<T: Config<I>, I: 'static> Pallet<T, I> {
        // no need to set weight here, we would charge gas fee in `pre_dispatch`
        #[pallet::weight((0, dispatch_info_of::<T, I>(tx).class, Pays::No))]
        pub fn submit_external_tx(
            origin: OriginFor<T>,
            tx: ExternalVerifiable<T::Index, T::Transaction>,
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;
            let account = Pallet::<T, I>::extract(&tx)?;
            match tx {
                ExternalVerifiable::Ed25519 { .. } => Err(Error::<T, I>::InvalidSignature.into()),
                ExternalVerifiable::Ecdsa { tx, .. } => {
                    let r = tx
                        .dispatch(frame_system::RawOrigin::Signed(account.clone()).into())
                        .map(|_| ().into());
                    Self::deposit_event(Event::<T, I>::ExternalTransactionExecuted(
                        account,
                        r.map_err(|e| e.error),
                    ));
                    Ok(().into())
                }
            }
        }
    }

    impl<T: Config<I>, I: 'static> Pallet<T, I> {
        pub fn extract(
            sig: &ExternalVerifiable<T::Index, T::Transaction>,
        ) -> Result<T::AccountId, DispatchError> {
            match sig {
                ExternalVerifiable::Ed25519 { .. } => Err(Error::<T, I>::InvalidSignature.into()),
                ExternalVerifiable::Ecdsa {
                    ref tx,
                    ref nonce,
                    signature,
                } => {
                    let msg = T::ExternalSignWrapper::extend_payload(*nonce, tx.clone());
                    let digest = sp_io::hashing::keccak_256(&msg);
                    let pubkey = sp_io::crypto::secp256k1_ecdsa_recover(signature, &digest)
                        .map_err(|_| Error::<T, I>::InvalidSignature)?;
                    let address = sp_io::hashing::keccak_256(&pubkey)[12..].to_vec();
                    let h = (b"-*-#fusotao#-*-", T::MainOrTestnet::get(), address)
                        .using_encoded(sp_io::hashing::blake2_256);
                    Decode::decode(&mut TrailingZeroInput::new(h.as_ref()))
                        .map_err(|_| Error::<T, I>::InvalidSignature.into())
                }
            }
        }

        fn withdraw_fee(
            who: &T::AccountId,
            fee: BalanceOf<T, I>,
        ) -> Result<(), TransactionValidityError> {
            if fee.is_zero() {
                return Ok(());
            }
            match T::Currency::withdraw(
                who,
                fee,
                WithdrawReasons::TRANSACTION_PAYMENT,
                ExistenceRequirement::KeepAlive,
            ) {
                Ok(_) => Ok(()),
                Err(e) => {
                    log::error!("withdraw original error: {:?}", e);
                    Err(InvalidTransaction::Payment.into())
                }
            }
        }

        fn compute_fee(len: u32, weight: Weight, class: DispatchClass) -> BalanceOf<T, I> {
            let len = <BalanceOf<T, I>>::from(len);
            let per_byte = T::TransactionByteFee::get();
            let len_fee = per_byte.saturating_mul(len);
            let weight_fee = Self::weight_to_fee(weight);
            let base_fee = Self::weight_to_fee(T::BlockWeights::get().get(class).base_extrinsic);
            base_fee.saturating_add(len_fee).saturating_add(weight_fee)
        }

        fn weight_to_fee(weight: Weight) -> BalanceOf<T, I> {
            let capped_weight = weight.min(T::BlockWeights::get().max_block);
            T::WeightToFee::weight_to_fee(&capped_weight)
            //  <BalanceOf<T, I>>::from(8u8)
        }
    }

    #[pallet::validate_unsigned]
    impl<T: Config<I>, I: 'static> ValidateUnsigned for Pallet<T, I> {
        type Call = Call<T, I>;

        /// Validate unsigned call to this module.
        /// TODO make it compatiable with Ed25519 signature
        fn validate_unsigned(_: TransactionSource, call: &Self::Call) -> TransactionValidity {
            if let Call::submit_external_tx { ref tx } = call {
                let account =
                    Pallet::<T, I>::extract(tx).map_err(|_| InvalidTransaction::BadProof)?;
                let index = frame_system::Pallet::<T>::account_nonce(&account);
                let (nonce, call) = match tx {
                    ExternalVerifiable::Ed25519 {
                        public: _,
                        tx,
                        nonce,
                        signature: _,
                    } => (*nonce, tx),
                    ExternalVerifiable::Ecdsa {
                        tx,
                        nonce,
                        signature: _,
                    } => (*nonce, tx),
                };
                ensure!(index == nonce, InvalidTransaction::BadProof);
                frame_system::Pallet::<T>::inc_account_nonce(account.clone());
                let info = call.get_dispatch_info();
                let len = tx
                    .encoded_size()
                    .try_into()
                    .map_err(|_| InvalidTransaction::ExhaustsResources)?;
                ensure!(
                    len < *T::BlockLength::get().max.get(DispatchClass::Normal),
                    InvalidTransaction::ExhaustsResources
                );
                ensure!(
                    info.weight
                        .any_lt(T::BlockWeights::get().max_block.saturating_div(5)),
                    InvalidTransaction::ExhaustsResources
                );
                let fee = Pallet::<T, I>::compute_fee(len, info.weight, info.class);
                log::info!("withdraw fee who: {:?}", &account);
                log::info!("withdraw fee amount: {:?}", fee);
                let _ = Pallet::<T, I>::withdraw_fee(&account, fee)?;
                ValidTransaction::with_tag_prefix("FusoAgent")
                    .priority(info.weight.ref_time())
                    .and_provides(account)
                    .longevity(5)
                    .propagate(true)
                    .build()
            } else {
                InvalidTransaction::Call.into()
            }
        }
    }
}
