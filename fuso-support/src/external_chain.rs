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

use codec::{Codec, Decode, Encode};
use sp_core::hexdisplay::HexDisplay;
use sp_runtime::RuntimeDebug;
use sp_std::{
    fmt::{Debug, Display, Formatter},
    vec::Vec,
};

#[derive(Encode, Decode, PartialEq, Eq, Clone, RuntimeDebug)]
pub enum ExternalChainAddress {
    BTC(Vec<u8>),
    ETH(Vec<u8>),
    ERC20(Vec<u8>, Vec<u8>),
    TRX(Vec<u8>),
    TRC20(Vec<u8>, Vec<u8>),
    FIL(Vec<u8>),
    DOT(Vec<u8>),
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum AddressError {
    BadSs58,
    BadKeccak256,
}

// TODO
impl ExternalChainAddress {
    pub fn from_btc_ss58_check(ss58_check: &str) -> Result<Self, AddressError> {
        Err(AddressError::BadSs58)
    }

    pub fn from_eth_keccak256_check(keccak256_check: &str) -> Result<Self, AddressError> {
        Err(AddressError::BadKeccak256)
    }
}

impl Display for ExternalChainAddress {
    fn fmt(&self, f: &mut Formatter) -> sp_std::fmt::Result {
        match self {
            ExternalChainAddress::BTC(addr) => write!(f, "BTC:{}", HexDisplay::from(addr)),
            ExternalChainAddress::ETH(addr) => write!(f, "ETH:{}", HexDisplay::from(addr)),
            ExternalChainAddress::ERC20(contract, addr) => write!(
                f,
                "ERC20({}):{}",
                HexDisplay::from(contract),
                HexDisplay::from(addr)
            ),
            ExternalChainAddress::TRX(addr) => write!(f, "TRX:{}", HexDisplay::from(addr)),
            ExternalChainAddress::TRC20(contract, addr) => write!(
                f,
                "TRC20({}):{}",
                HexDisplay::from(contract),
                HexDisplay::from(addr)
            ),
            ExternalChainAddress::FIL(addr) => write!(f, "FIL:{}", HexDisplay::from(addr)),
            ExternalChainAddress::DOT(addr) => write!(f, "DOT:{}", HexDisplay::from(addr)),
        }
    }
}
