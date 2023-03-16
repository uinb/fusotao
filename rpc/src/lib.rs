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

#![feature(result_flattening)]
pub mod broker;
pub mod token;
pub mod verifier;

pub use broker::{FusoBroker, FusoBrokerApiServer};
pub use verifier::{FusoVerifier, FusoVerifierApiServer};

pub fn blake2_128concat_storage_key<K: codec::Encode>(
    module: &[u8],
    storage: &[u8],
    key: K,
) -> sp_core::storage::StorageKey {
    let mut bytes = sp_core::twox_128(module).to_vec();
    bytes.extend(&sp_core::twox_128(storage)[..]);
    let encoded = key.encode();
    let x: &[u8] = encoded.as_slice();
    let v = sp_core::blake2_128(x)
        .iter()
        .chain(x.iter())
        .cloned()
        .collect::<Vec<_>>();
    bytes.extend(v);
    sp_core::storage::StorageKey(bytes)
}

#[macro_export]
macro_rules! error_msg {
    ($msg:expr) => {
        jsonrpsee::types::error::ErrorObject::owned(
            jsonrpsee::types::error::ErrorCode::InternalError.code(),
            stringify!($msg),
            None::<String>,
        )
    };
}

#[macro_export]
macro_rules! rpc_error {
    (req=>$msg:expr) => {
        jsonrpsee::core::Error::Call(jsonrpsee::types::error::CallError::Custom(error_msg!($msg)))
    };
    (sub=>$msg:expr) => {
        jsonrpsee::types::error::SubscriptionEmptyError::from(
            jsonrpsee::types::error::CallError::Custom(error_msg!($msg)),
        )
    };
}
