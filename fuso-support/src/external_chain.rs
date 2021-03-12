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
    fmt::{Debug, Display, Formatter, Result},
    vec::Vec,
};

#[derive(Encode, Decode, PartialEq, Eq, Clone, RuntimeDebug)]
pub enum ExternalChainAddress {
    BTC(Vec<u8>),
    ETH(Vec<u8>),
    ERC20(Vec<u8>, Vec<u8>),
    EOS(Vec<u8>),
}

impl Display for ExternalChainAddress {
    fn fmt(&self, f: &mut Formatter) -> Result {
        match self {
            ExternalChainAddress::BTC(addr) => write!(f, "BTC:{}", HexDisplay::from(addr)),
            ExternalChainAddress::ETH(addr) => write!(f, "ETH:{}", HexDisplay::from(addr)),
            ExternalChainAddress::ERC20(contract, addr) => write!(
                f,
                "ERC20({}):{}",
                HexDisplay::from(contract),
                HexDisplay::from(addr)
            ),
            ExternalChainAddress::EOS(addr) => write!(f, "EOS:{}", HexDisplay::from(addr)),
        }
    }
}
