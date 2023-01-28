// This file is part of Substrate.

// Copyright (C) 2018-2022 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Substrate chain configurations.

use fuso_runtime::{
	constants::currency::*, wasm_binary_unwrap, BabeConfig, BalancesConfig, Block, GrandpaConfig,
	ImOnlineConfig, SessionConfig, SudoConfig, SystemConfig, opaque::SessionKeys, FoundationConfig,
};
use grandpa_primitives::AuthorityId as GrandpaId;
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use sc_chain_spec::ChainSpecExtension;
use sc_service::ChainType;
use serde::{Deserialize, Serialize};
use sp_consensus_babe::AuthorityId as BabeId;
use sp_core::{sr25519, Pair, Public};
use sp_runtime::traits::{IdentifyAccount, Verify};

pub use fuso_runtime::GenesisConfig;
pub use fuso_primitives::{AccountId, Balance, Signature};

// + beefy
use beefy_primitives::crypto::AuthorityId as BeefyId;

// + octopus pallets
use fuso_runtime::{
	constants::currency::OCT, OctopusAppchainConfig, OctopusBridgeConfig,
	OctopusLposConfig, OctopusUpwardMessagesConfig,
};
use pallet_octopus_appchain::sr25519::AuthorityId as OctopusId;

type AccountPublic = <Signature as Verify>::Signer;

/// Node `ChainSpec` extensions.
///
/// Additional parameters for some Substrate core modules,
/// customizable from the chain spec.
#[derive(Default, Clone, Serialize, Deserialize, ChainSpecExtension)]
#[serde(rename_all = "camelCase")]
pub struct Extensions {
	/// Block numbers with known hashes.
	pub fork_blocks: sc_client_api::ForkBlocks<Block>,
	/// Known bad block hashes.
	pub bad_blocks: sc_client_api::BadBlocks<Block>,
	/// The light sync state extension used by the sync-state rpc.
	pub light_sync_state: sc_sync_state_rpc::LightSyncStateExtension,
}

/// Specialized `ChainSpec`.
pub type ChainSpec = sc_service::GenericChainSpec<GenesisConfig, Extensions>;

/// Octopus testnet generator
pub fn octopus_testnet_config() -> Result<ChainSpec, String> {
	ChainSpec::from_json_bytes(&include_bytes!("../res/octopus-testnet.json")[..])
}

/// Octopus mainnet generator
pub fn octopus_mainnet_config() -> Result<ChainSpec, String> {
	ChainSpec::from_json_bytes(&include_bytes!("../res/octopus-mainnet.json")[..])
}

fn session_keys(
	babe: BabeId,
	grandpa: GrandpaId,
	im_online: ImOnlineId,
	beefy: BeefyId,
	octopus: OctopusId,
) -> SessionKeys {
	SessionKeys { babe, grandpa, im_online, beefy, octopus }
}

/// Helper function to generate a crypto pair from seed
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{}", seed), None)
		.expect("static values are valid; qed")
		.public()
}

/// Helper function to generate an account ID from seed
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
	AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
	AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

/// Helper function to generate controller and session key from seed
pub fn authority_keys_from_seed(
	seed: &str,
) -> (AccountId, BabeId, GrandpaId, ImOnlineId, BeefyId, OctopusId) {
	(
		get_account_id_from_seed::<sr25519::Public>(seed),
		get_from_seed::<BabeId>(seed),
		get_from_seed::<GrandpaId>(seed),
		get_from_seed::<ImOnlineId>(seed),
		get_from_seed::<BeefyId>(seed),
		get_from_seed::<OctopusId>(seed),
	)
}

fn barnacle_chain_spec_properties() -> serde_json::map::Map<String, serde_json::Value> {
	serde_json::json!({
		"ss58Format": 42,
		"tokenDecimals": 18,
		"tokenSymbol": "BAR",
	})
	.as_object()
	.expect("Map given; qed")
	.clone()
}

/// Helper function to create GenesisConfig for testing
pub fn testnet_genesis(
	initial_authorities: Vec<(AccountId, BabeId, GrandpaId, ImOnlineId, BeefyId, OctopusId)>,
	root_key: AccountId,
	endowed_accounts: Option<Vec<AccountId>>,
) -> GenesisConfig {
	let endowed_accounts: Vec<AccountId> = endowed_accounts.unwrap_or_else(|| {
		vec![
			get_account_id_from_seed::<sr25519::Public>("Alice"),
			get_account_id_from_seed::<sr25519::Public>("Bob"),
			get_account_id_from_seed::<sr25519::Public>("Charlie"),
			get_account_id_from_seed::<sr25519::Public>("Dave"),
			get_account_id_from_seed::<sr25519::Public>("Eve"),
			get_account_id_from_seed::<sr25519::Public>("Ferdie"),
		]
	});

	const ENDOWMENT: Balance = 10_000_000 * DOLLARS;
	const STASH: Balance = 100 * OCT;
	let validators = initial_authorities.iter().map(|x| (x.0.clone(), STASH)).collect::<Vec<_>>();

	GenesisConfig {
		system: SystemConfig { code: wasm_binary_unwrap().to_vec() },
		balances: BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|x| (x, ENDOWMENT)).collect(),
		},
		beefy: Default::default(),
		session: SessionConfig {
			keys: initial_authorities
				.iter()
				.map(|x| {
					(
						x.0.clone(),
						x.0.clone(),
						session_keys(
							x.1.clone(),
							x.2.clone(),
							x.3.clone(),
							x.4.clone(),
							x.5.clone(),
						),
					)
				})
				.collect::<Vec<_>>(),
		},
		sudo: SudoConfig { key: Some(root_key) },
		babe: BabeConfig {
			authorities: vec![],
			epoch_config: Some(fuso_runtime::BABE_GENESIS_EPOCH_CONFIG),
		},
		im_online: ImOnlineConfig { keys: vec![] },
		grandpa: GrandpaConfig { authorities: vec![] },
		transaction_payment: Default::default(),
		octopus_appchain: OctopusAppchainConfig {
			anchor_contract: "octopus-anchor.testnet".to_string(),
			validators,
		},
		octopus_bridge: OctopusBridgeConfig {
			premined_amount: 1024 * DOLLARS,
			asset_id_by_token_id: vec![("usdn.testnet".to_string(), 0)],
		},
		octopus_lpos: OctopusLposConfig { era_payout: 2 * DOLLARS, ..Default::default() },
		octopus_upward_messages: OctopusUpwardMessagesConfig { interval: 1 },
		foundation: FoundationConfig {
			fund: vec![
				(
					get_account_id_from_seed::<sr25519::Public>("Charile"),
					1,
					1,
					100,
					1000 * DOLLARS,
					2000 * DOLLARS,
				),
				(
					get_account_id_from_seed::<sr25519::Public>("Eva"),
					1,
					2,
					99,
					100 * DOLLARS,
					2000 * DOLLARS,
				),
			],
		},
	}
}

fn development_config_genesis() -> GenesisConfig {
	testnet_genesis(
		vec![authority_keys_from_seed("Alice")],
		get_account_id_from_seed::<sr25519::Public>("Alice"),
		None,
	)
}

/// Development config (single validator Alice)
pub fn development_config() -> ChainSpec {
	ChainSpec::from_genesis(
		"Development",
		"dev",
		ChainType::Development,
		development_config_genesis,
		vec![],
		None,
		// Protocol ID
		Some("bar"),
		None,
		// Properties
		Some(barnacle_chain_spec_properties()),
		// Extensions
		Default::default(),
	)
}

fn local_testnet_genesis() -> GenesisConfig {
	testnet_genesis(
		vec![authority_keys_from_seed("Alice"), authority_keys_from_seed("Bob")],
		get_account_id_from_seed::<sr25519::Public>("Alice"),
		None,
	)
}

/// Local testnet config (multivalidator Alice + Bob)
pub fn local_testnet_config() -> ChainSpec {
	ChainSpec::from_genesis(
		"Local Testnet",
		"local_testnet",
		ChainType::Local,
		local_testnet_genesis,
		vec![],
		None,
		// Protocol ID
		Some("bar"),
		None,
		// Properties
		Some(barnacle_chain_spec_properties()),
		// Extensions
		Default::default(),
	)
}

#[cfg(test)]
pub(crate) mod tests {
	use super::*;
	use crate::service::{new_full_base, NewFullBase};
	use sc_service_test;
	use sp_runtime::BuildStorage;

	fn local_testnet_genesis_instant_single() -> GenesisConfig {
		testnet_genesis(
			vec![authority_keys_from_seed("Alice")],
			get_account_id_from_seed::<sr25519::Public>("Alice"),
			None,
		)
	}

	/// Local testnet config (single validator - Alice)
	pub fn integration_test_config_with_single_authority() -> ChainSpec {
		ChainSpec::from_genesis(
			"Integration Test",
			"test",
			ChainType::Development,
			local_testnet_genesis_instant_single,
			vec![],
			None,
			// Protocol ID
			Some("bar"),
			None,
			// Properties
			Some(barnacle_chain_spec_properties()),
			// Extensions
			Default::default(),
		)
	}

	/// Local testnet config (multivalidator Alice + Bob)
	pub fn integration_test_config_with_two_authorities() -> ChainSpec {
		ChainSpec::from_genesis(
			"Integration Test",
			"test",
			ChainType::Development,
			local_testnet_genesis,
			vec![],
			None,
			// Protocol ID
			Some("bar"),
			None,
			// Properties
			Some(barnacle_chain_spec_properties()),
			// Extensions
			Default::default(),
		)
	}

	#[test]
	#[ignore]
	fn test_connectivity() {
		sp_tracing::try_init_simple();

		sc_service_test::connectivity(integration_test_config_with_two_authorities(), |config| {
			let NewFullBase { task_manager, client, network, transaction_pool, .. } =
				new_full_base(config, false, |_, _| ())?;
			Ok(sc_service_test::TestNetComponents::new(
				task_manager,
				client,
				network,
				transaction_pool,
			))
		});
	}

	#[test]
	fn test_create_development_chain_spec() {
		development_config().build_storage().unwrap();
	}

	#[test]
	fn test_create_local_testnet_chain_spec() {
		local_testnet_config().build_storage().unwrap();
	}
}
