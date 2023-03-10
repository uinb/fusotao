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

use jsonrpsee::{core::RpcResult, proc_macros::rpc};

#[rpc(client, server)]
pub trait TokenApi<BlockHash, Account, Balance> {
    #[method(name = "token_freeBalance")]
    fn free_balance(&self, who: Account, at: Option<BlockHash>) -> RpcResult<Balance>;

    #[method(name = "token_totalBalance")]
    fn total_balance(&self, who: Account, at: Option<BlockHash>) -> RpcResult<Balance>;
}
