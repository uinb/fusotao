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
    convert::TryFrom,
    fmt::{Debug, Display, Formatter},
    vec::Vec,
};

#[derive(Encode, Decode, PartialEq, Eq, Clone, RuntimeDebug)]
pub enum ExternalChain {
    BTC,
    LTC,
    ETH,
    ERC20(Vec<u8>),
    TRX,
    TRC20(Vec<u8>),
    DOT,
    FIL,
}

impl TryFrom<(u16, Option<Vec<u8>>)> for ExternalChain {
    type Error = ();

    fn try_from((chain, contract): (u16, Option<Vec<u8>>)) -> Result<ExternalChain, ()> {
        match chain {
            0 => Ok(ExternalChain::BTC),
            1 => Ok(ExternalChain::LTC),
            2 => match contract {
                Some(addr) => Ok(ExternalChain::ERC20(addr)),
                None => Ok(ExternalChain::ETH),
            },
            3 => match contract {
                Some(addr) => Ok(ExternalChain::TRC20(addr)),
                None => Ok(ExternalChain::TRX),
            },
            4 => Ok(ExternalChain::DOT),
            5 => Ok(ExternalChain::FIL),
            _ => Err(()),
        }
    }
}

#[derive(Encode, Decode, PartialEq, Eq, Clone, RuntimeDebug)]
pub struct ExternalChainAddress {
    chain: ExternalChain,
    pubkey: Vec<u8>,
}

// TODO
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum AddressError {
    IllegalSs58,
    IllegalKeccak256,
}

impl TryFrom<(ExternalChain, Vec<u8>)> for ExternalChainAddress {
    type Error = AddressError;

    // TODO
    fn try_from((chain, encoded_addr): (ExternalChain, Vec<u8>)) -> Result<Self, AddressError> {
        match chain {
            ExternalChain::BTC => Err(AddressError::IllegalSs58),
            ExternalChain::ETH | ExternalChain::ERC20(_) => Err(AddressError::IllegalKeccak256),
            _ => Err(AddressError::IllegalKeccak256),
        }
    }
}

#[cfg(feature = "std")]
impl Display for ExternalChain {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            // TODO codec
            ExternalChain::ERC20(addr) => write!(f, "ERC20({})", HexDisplay::from(contract)),
            ExternalChain::TRC20(addr) => write!(f, "TRC20({})", HexDisplay::from(contract)),
            _ => write!(f, "{:?}", self),
        }
    }
}

#[cfg(feature = "std")]
impl Display for ExternalChainAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
