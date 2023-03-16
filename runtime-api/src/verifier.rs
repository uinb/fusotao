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

use codec::Codec;
use sp_runtime::traits::MaybeDisplay;

sp_api::decl_runtime_apis! {
    /// API to interact with pallet-fuso-verifier
    pub trait FusoVerifierRuntimeApi<AccountId, Balance>
    where
        AccountId: Codec + MaybeDisplay,
        Balance: Codec + MaybeDisplay,
    {
        fn current_season_of_dominator(dominator: AccountId) -> u32;

        fn pending_shares_of_dominator(dominator: AccountId, who: AccountId) -> Balance;
    }
}
