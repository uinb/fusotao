#![cfg(test)]
use crate::mock::{
    assert_events, expect_event, new_test_ext, AccountId, Assets, Balance, Balances, Bridge,
    ChainBridgeTransfer, Indicator, NativeResourceId, ProposalLifetime, RuntimeCall, RuntimeEvent,
    RuntimeOrigin, Test, Verifier, DOLLARS, ENDOWED_BALANCE, RELAYER_A, RELAYER_B, RELAYER_C,
    TREASURY,
};
use codec::Encode;
use frame_support::{assert_noop, assert_ok, dispatch::DispatchError, traits::fungibles::Inspect};
use frame_system::RawOrigin;
use fuso_support::chainbridge::*;
use fuso_support::{
    traits::{PriceOracle, Token},
    XToken,
};
use pallet_chainbridge as bridge;
use pallet_fuso_token as assets;
use sp_core::{blake2_256, crypto::AccountId32, H256};
use sp_keyring::AccountKeyring;
use sp_runtime::traits::Zero;
use sp_runtime::MultiAddress;

const TEST_THRESHOLD: u32 = 2;

fn make_remark_proposal(call: Vec<u8>) -> RuntimeCall {
    let depositer = [0u8; 20];
    RuntimeCall::ChainBridgeTransfer(crate::Call::remark {
        message: call,
        depositer,
        r_id: Default::default(),
    })
}

fn make_transfer_proposal(resource_id: ResourceId, to: AccountId32, amount: u64) -> RuntimeCall {
    RuntimeCall::ChainBridgeTransfer(crate::Call::transfer_in {
        to,
        amount: amount.into(),
        r_id: resource_id,
    })
}

#[test]
fn transfer_native() {
    new_test_ext().execute_with(|| {
        let dest_chain = 0;
        let resource_id = NativeResourceId::get();
        let amount: Balance = 1 * DOLLARS;
        let recipient = b"davirain.xyz".to_vec(); // recipient account

        assert_ok!(Bridge::whitelist_chain(
            RuntimeOrigin::signed(TREASURY),
            dest_chain.clone()
        ));
        assert_ok!(ChainBridgeTransfer::transfer_out(
            RuntimeOrigin::signed(RELAYER_A),
            amount.clone(),
            resource_id.clone(),
            recipient.clone(),
            dest_chain,
        ));

        expect_event(bridge::Event::FungibleTransfer(
            dest_chain,
            1,
            resource_id,
            amount.into(),
            recipient,
        ));
    })
}

#[test]
fn transfer_out_non_native() {
    new_test_ext().execute_with(|| {
        let dest_chain = 1;
        let ferdie: AccountId = AccountKeyring::Ferdie.into();
        let recipient = vec![99];
        let contract_address = "b20f54288947a89a4891d181b10fe04560b55c5e";
        let denom = XToken::ERC20(
            br#"DENOM"#.to_vec(),
            hex::decode(contract_address).unwrap(),
            Zero::zero(),
            false,
            18,
        );
        let resource_id = derive_resource_id(
            dest_chain,
            0,
            hex::decode(contract_address).unwrap().as_slice(),
        )
        .unwrap();
        let resource = b"ChainBridgeHandler.transfer_in".to_vec();
        let (chainid, _, contract) = decode_resource_id(resource_id).unwrap();
        assert_ok!(Assets::associate_token(
            RuntimeOrigin::signed(TREASURY),
            chainid,
            contract,
            1u32
        ));
        assert_ok!(Bridge::set_resource(
            RuntimeOrigin::signed(TREASURY),
            resource_id,
            resource
        ));
        assert_ok!(Assets::issue(RuntimeOrigin::signed(TREASURY), denom));
        let amount: Balance = 1 * DOLLARS;
        assert_ok!(Assets::do_mint(1, &ferdie, amount, None));

        // make sure have some  amount after mint
        assert_eq!(Assets::free_balance(&1, &ferdie), amount);
        assert_eq!(
            Assets::try_get_asset_id(dest_chain, hex::decode(contract_address).unwrap()),
            Ok(1)
        );
        assert_ok!(Bridge::whitelist_chain(
            RuntimeOrigin::signed(TREASURY),
            dest_chain.clone()
        ));
        assert_ok!(ChainBridgeTransfer::transfer_out(
            RuntimeOrigin::signed(ferdie.clone()),
            amount,
            resource_id,
            recipient.clone(),
            dest_chain,
        ));

        // make sure transfer have 0 amount
        assert_eq!(Assets::balance(0, &ferdie), 0);

        assert_events(vec![RuntimeEvent::Bridge(bridge::Event::FungibleTransfer(
            dest_chain,
            1,
            resource_id,
            sp_core::U256::from(amount),
            recipient,
        ))]);
    })
}

#[test]
fn transfer() {
    new_test_ext().execute_with(|| {
        // Check inital state
        let bridge_id: AccountId32 = Bridge::account_id();
        let resource_id = NativeResourceId::get();
        let resource = b"ChainBridgeHandler.transfer_in".to_vec();
        assert_ok!(Bridge::set_resource(
            RuntimeOrigin::signed(TREASURY),
            resource_id,
            resource
        ));
        assert_eq!(Balances::free_balance(&bridge_id), ENDOWED_BALANCE);
        // Transfer and check result
        assert_ok!(ChainBridgeTransfer::transfer_in(
            RuntimeOrigin::signed(Bridge::account_id()),
            RELAYER_A,
            10,
            resource_id,
        ));
        assert_eq!(Balances::free_balance(&bridge_id), ENDOWED_BALANCE - 10);
        assert_eq!(Balances::free_balance(RELAYER_A), ENDOWED_BALANCE + 10);

        assert_events(vec![RuntimeEvent::Balances(
            pallet_balances::Event::Transfer {
                from: Bridge::account_id(),
                to: RELAYER_A,
                amount: 10,
            },
        )]);
    })
}

#[test]
fn execute_remark() {
    new_test_ext().execute_with(|| {
        let call = frame_system::Call::remark::<Test> {
            remark: vec![0xff; 32],
        };
        let proposal = make_remark_proposal(call.encode());
        let prop_id = 1;
        let src_id = 1;
        let r_id = derive_resource_id(src_id, 0, b"hash").unwrap();
        let resource = b"Example.remark".to_vec();

        assert_ok!(Bridge::set_threshold(
            RuntimeOrigin::signed(TREASURY),
            TEST_THRESHOLD,
        ));
        assert_ok!(Bridge::add_relayer(
            RuntimeOrigin::signed(TREASURY),
            RELAYER_A
        ));
        assert_ok!(Bridge::add_relayer(
            RuntimeOrigin::signed(TREASURY),
            RELAYER_B
        ));
        assert_ok!(Bridge::whitelist_chain(
            RuntimeOrigin::signed(TREASURY),
            src_id
        ));
        let (chainid, _, contract) = decode_resource_id(r_id).unwrap();
        assert_ok!(Assets::associate_token(
            RuntimeOrigin::signed(TREASURY),
            chainid,
            contract,
            1u32
        ));
        assert_ok!(Bridge::set_resource(
            RuntimeOrigin::signed(TREASURY),
            r_id,
            resource
        ));

        assert_ok!(Bridge::acknowledge_proposal(
            RuntimeOrigin::signed(RELAYER_A),
            prop_id,
            src_id,
            r_id,
            [0u8; 32],
            Box::new(proposal.clone())
        ));
        /* assert_ok!(Bridge::acknowledge_proposal(
            RuntimeOrigin::signed(RELAYER_B),
            prop_id,
            src_id,
            r_id,
            Box::new(proposal.clone())
        ));*/

        // event_exists(ChainBridgeTransferEvent::Remark(hash));
    })
}

#[test]
fn execute_remark_bad_origin() {
    new_test_ext().execute_with(|| {
        let depositer = [0u8; 20];
        let hash: H256 = "ABC".using_encoded(blake2_256).into();
        // Don't allow any signed origin except from bridge addr
        assert_noop!(
            ChainBridgeTransfer::remark(
                RuntimeOrigin::signed(RELAYER_A),
                hash.as_bytes().to_vec(),
                depositer,
                Default::default(),
            ),
            DispatchError::BadOrigin
        );
        // Don't allow root calls
        assert_noop!(
            ChainBridgeTransfer::remark(
                RuntimeOrigin::signed(TREASURY),
                hash.as_bytes().to_vec(),
                depositer,
                Default::default(),
            ),
            DispatchError::BadOrigin
        );
    })
}

#[test]
fn create_sucessful_transfer_proposal_non_native_token() {
    new_test_ext().execute_with(|| {
        let prop_id = 1;
        let src_id = 1;
        let contract_address = "b20f54288947a89a4891d181b10fe04560b55c5e";
        let r_id = derive_resource_id(src_id, 0, hex::decode(contract_address).unwrap().as_slice())
            .unwrap();
        let (chain, associated, contract) = decode_resource_id(r_id).unwrap();
        assert_eq!(src_id, chain);
        assert_eq!(ChainBridgeTransfer::associated_dominator(associated), None);
        assert_eq!(contract, hex::decode(contract_address).unwrap());
        let resource = b"ChainBridgeTransfer.transfer_in".to_vec();
        let proposal = make_transfer_proposal(r_id, RELAYER_A, 10);

        let denom = XToken::ERC20(
            br#"DENOM"#.to_vec(),
            hex::decode(contract_address).unwrap(),
            Zero::zero(),
            true,
            18,
        );
        assert_ok!(Assets::issue(RuntimeOrigin::signed(TREASURY), denom,));

        assert_ok!(Bridge::set_threshold(
            RuntimeOrigin::signed(TREASURY),
            TEST_THRESHOLD,
        ));
        assert_ok!(Bridge::add_relayer(
            RuntimeOrigin::signed(TREASURY),
            RELAYER_A
        ));
        assert_ok!(Bridge::add_relayer(
            RuntimeOrigin::signed(TREASURY),
            RELAYER_B
        ));
        assert_ok!(Bridge::add_relayer(
            RuntimeOrigin::signed(TREASURY),
            RELAYER_C
        ));
        assert_ok!(Bridge::whitelist_chain(
            RuntimeOrigin::signed(TREASURY),
            src_id
        ));
        let (chainid, _, contract) = decode_resource_id(r_id).unwrap();
        assert_ok!(Assets::associate_token(
            RuntimeOrigin::signed(TREASURY),
            chainid,
            contract,
            1u32
        ));
        assert_ok!(Bridge::set_resource(
            RuntimeOrigin::signed(TREASURY),
            r_id,
            resource
        ));

        // Create proposal (& vote)
        assert_ok!(Bridge::acknowledge_proposal(
            RuntimeOrigin::signed(RELAYER_A),
            prop_id,
            src_id,
            r_id,
            [0u8; 32],
            Box::new(proposal.clone())
        ));
        let prop = Bridge::votes(src_id, (prop_id.clone(), proposal.clone())).unwrap();
        let expected = bridge::ProposalVotes {
            votes_for: vec![RELAYER_A],
            votes_against: vec![],
            status: bridge::ProposalStatus::Initiated,
            expiry: ProposalLifetime::get() + 1,
        };
        assert_eq!(prop, expected);

        // Second relayer votes against
        assert_ok!(Bridge::reject_proposal(
            RuntimeOrigin::signed(RELAYER_B),
            prop_id,
            src_id,
            r_id,
            Box::new(proposal.clone())
        ));
        let prop = Bridge::votes(src_id, (prop_id.clone(), proposal.clone())).unwrap();
        let expected = bridge::ProposalVotes {
            votes_for: vec![RELAYER_A],
            votes_against: vec![RELAYER_B],
            status: bridge::ProposalStatus::Initiated,
            expiry: ProposalLifetime::get() + 1,
        };
        assert_eq!(prop, expected);

        // Third relayer votes in favour
        assert_ok!(Bridge::acknowledge_proposal(
            RuntimeOrigin::signed(RELAYER_C),
            prop_id,
            src_id,
            r_id,
            [0u8; 32],
            Box::new(proposal.clone())
        ));
        let prop = Bridge::votes(src_id, (prop_id.clone(), proposal.clone())).unwrap();
        let expected = bridge::ProposalVotes {
            votes_for: vec![RELAYER_A, RELAYER_C],
            votes_against: vec![RELAYER_B],
            status: bridge::ProposalStatus::Approved,
            expiry: ProposalLifetime::get() + 1,
        };
        assert_eq!(prop, expected);

        // mint 10 resource_id to RELAYER_A
        assert_eq!(Assets::free_balance(&1, &RELAYER_A), 10);

        assert_events(vec![
            RuntimeEvent::Bridge(bridge::Event::VoteFor(src_id, prop_id, RELAYER_A)),
            RuntimeEvent::Bridge(bridge::Event::VoteAgainst(src_id, prop_id, RELAYER_B)),
            RuntimeEvent::Bridge(bridge::Event::ProposalVote(src_id, [0u8; 32], prop_id)),
            RuntimeEvent::Bridge(bridge::Event::VoteFor(src_id, prop_id, RELAYER_C)),
            RuntimeEvent::Bridge(bridge::Event::ProposalApproved(src_id, prop_id)),
            RuntimeEvent::Assets(assets::Event::TokenMinted(1, RELAYER_A, 10)),
            RuntimeEvent::Bridge(bridge::Event::ProposalSucceeded(src_id, prop_id)),
        ]);
    })
}

#[test]
fn create_sucessful_transfer_proposal_native_token() {
    new_test_ext().execute_with(|| {
        let prop_id = 1;
        let src_id = 1;

        // let r_id = derive_resource_id(src_id, 0, b"transfer").unwrap();
        let resource = b"ChainBridgeTransfer.transfer_in".to_vec();
        let r_id = NativeResourceId::get();
        let proposal = make_transfer_proposal(r_id, RELAYER_A, 10);

        assert_ok!(Bridge::set_threshold(
            RuntimeOrigin::signed(TREASURY),
            TEST_THRESHOLD
        ));
        assert_ok!(Bridge::add_relayer(
            RuntimeOrigin::signed(TREASURY),
            RELAYER_A
        ));
        assert_ok!(Bridge::add_relayer(
            RuntimeOrigin::signed(TREASURY),
            RELAYER_B
        ));
        assert_ok!(Bridge::add_relayer(
            RuntimeOrigin::signed(TREASURY),
            RELAYER_C
        ));
        assert_ok!(Bridge::whitelist_chain(
            RuntimeOrigin::signed(TREASURY),
            src_id
        ));
        assert_ok!(Bridge::set_resource(
            RuntimeOrigin::signed(TREASURY),
            r_id,
            resource
        ));

        // Create proposal (& vote)
        assert_ok!(Bridge::acknowledge_proposal(
            RuntimeOrigin::signed(RELAYER_A),
            prop_id,
            src_id,
            r_id,
            [0u8; 32],
            Box::new(proposal.clone())
        ));
        let prop = Bridge::votes(src_id, (prop_id.clone(), proposal.clone())).unwrap();
        let expected = bridge::ProposalVotes {
            votes_for: vec![RELAYER_A],
            votes_against: vec![],
            status: bridge::ProposalStatus::Initiated,
            expiry: ProposalLifetime::get() + 1,
        };
        assert_eq!(prop, expected);

        // Second relayer votes against
        assert_ok!(Bridge::reject_proposal(
            RuntimeOrigin::signed(RELAYER_B),
            prop_id,
            src_id,
            r_id,
            Box::new(proposal.clone())
        ));
        let prop = Bridge::votes(src_id, (prop_id.clone(), proposal.clone())).unwrap();
        let expected = bridge::ProposalVotes {
            votes_for: vec![RELAYER_A],
            votes_against: vec![RELAYER_B],
            status: bridge::ProposalStatus::Initiated,
            expiry: ProposalLifetime::get() + 1,
        };
        assert_eq!(prop, expected);

        // Third relayer votes in favour
        assert_ok!(Bridge::acknowledge_proposal(
            RuntimeOrigin::signed(RELAYER_C),
            prop_id,
            src_id,
            r_id,
            [0u8; 32],
            Box::new(proposal.clone())
        ));
        let prop = Bridge::votes(src_id, (prop_id.clone(), proposal.clone())).unwrap();
        let expected = bridge::ProposalVotes {
            votes_for: vec![RELAYER_A, RELAYER_C],
            votes_against: vec![RELAYER_B],
            status: bridge::ProposalStatus::Approved,
            expiry: ProposalLifetime::get() + 1,
        };
        assert_eq!(prop, expected);

        assert_eq!(Balances::free_balance(RELAYER_A), ENDOWED_BALANCE + 10);
        assert_eq!(
            Balances::free_balance(Bridge::account_id()),
            ENDOWED_BALANCE - 10
        );

        assert_events(vec![
            RuntimeEvent::Bridge(bridge::Event::VoteFor(src_id, prop_id, RELAYER_A)),
            RuntimeEvent::Bridge(bridge::Event::VoteAgainst(src_id, prop_id, RELAYER_B)),
            RuntimeEvent::Bridge(bridge::Event::ProposalVote(src_id, [0u8; 32], prop_id)),
            RuntimeEvent::Bridge(bridge::Event::VoteFor(src_id, prop_id, RELAYER_C)),
            RuntimeEvent::Bridge(bridge::Event::ProposalApproved(src_id, prop_id)),
            RuntimeEvent::Balances(pallet_balances::Event::Transfer {
                from: Bridge::account_id(),
                to: RELAYER_A,
                amount: 10,
            }),
            RuntimeEvent::Bridge(bridge::Event::ProposalSucceeded(src_id, prop_id)),
        ]);
    })
}

#[test]
fn authorize_and_revoke_in_remote() {
    new_test_ext().execute_with(|| {
        let prop_id = 1;
        let src_id = 1;
        let resource = b"ChainBridgeTransfer.transfer".to_vec();
        let contract_address = "b20f54288947a89a4891d181b10fe04560b55c5e";
        let r_id = derive_resource_id(src_id, 0, hex::decode(contract_address).unwrap().as_slice())
            .unwrap();
        let ferdie: AccountId = AccountKeyring::Ferdie.into();
        let alice: AccountId = AccountKeyring::Alice.into();
        let bob: AccountId = AccountKeyring::Bob.into();

        let denom = XToken::ERC20(
            br#"DENOM"#.to_vec(),
            hex::decode(contract_address).unwrap(),
            Zero::zero(),
            true,
            18,
        );
        assert_ok!(Assets::issue(RuntimeOrigin::signed(TREASURY), denom,));
        assert_ok!(Verifier::register(
            RuntimeOrigin::signed(bob.clone()),
            b"cool".to_vec()
        ));
        assert_ok!(Verifier::launch(
            RawOrigin::Root.into(),
            MultiAddress::Id(bob.clone())
        ));
        assert_ok!(Verifier::stake(
            RuntimeOrigin::signed(alice.clone()),
            MultiAddress::Id(bob.clone()),
            800000000000
        ));
        ChainBridgeTransfer::set_associated_dominator(1, bob.clone());
        let dominator_specified =
            derive_resource_id(src_id, 1, hex::decode(contract_address).unwrap().as_slice())
                .unwrap();

        let amount: Balance = 1 * DOLLARS;
        assert_ok!(Bridge::set_threshold(
            RuntimeOrigin::signed(TREASURY),
            TEST_THRESHOLD,
        ));
        assert_ok!(Bridge::add_relayer(
            RuntimeOrigin::signed(TREASURY),
            RELAYER_A
        ));
        assert_ok!(Bridge::add_relayer(
            RuntimeOrigin::signed(TREASURY),
            RELAYER_B
        ));
        assert_ok!(Bridge::add_relayer(
            RuntimeOrigin::signed(TREASURY),
            RELAYER_C
        ));
        assert_ok!(Bridge::whitelist_chain(
            RuntimeOrigin::signed(TREASURY),
            src_id
        ));
        let (chainid, _, contract) = decode_resource_id(r_id).unwrap();
        assert_ok!(Assets::associate_token(
            RuntimeOrigin::signed(TREASURY),
            chainid,
            contract,
            1u32
        ));
        assert_ok!(Bridge::set_resource(
            RuntimeOrigin::signed(TREASURY),
            r_id,
            resource
        ));

        let proposal = RuntimeCall::ChainBridgeTransfer(crate::Call::transfer_in {
            to: alice.clone(),
            amount: amount.into(),
            r_id: dominator_specified,
        });

        // Create proposal (& vote)
        assert_ok!(Bridge::acknowledge_proposal(
            RuntimeOrigin::signed(RELAYER_A),
            prop_id,
            src_id,
            r_id,
            [0u8; 32],
            Box::new(proposal.clone())
        ));
        assert_ok!(Bridge::acknowledge_proposal(
            RuntimeOrigin::signed(RELAYER_B),
            prop_id,
            src_id,
            r_id,
            [0u8; 32],
            Box::new(proposal.clone())
        ));
        assert_eq!(
            Verifier::receipts(bob.clone(), alice.clone()),
            Some(pallet_fuso_verifier::Receipt::Authorize(1, 1 * DOLLARS, 1))
        );

        // create a transfer_in call without dominator specified
        let proposal = RuntimeCall::ChainBridgeTransfer(crate::Call::transfer_in {
            to: ferdie.clone(),
            amount: amount.into(),
            r_id,
        });

        // Create proposal (& vote)
        assert_ok!(Bridge::acknowledge_proposal(
            RuntimeOrigin::signed(RELAYER_A),
            prop_id,
            src_id,
            r_id,
            [0u8; 32],
            Box::new(proposal.clone())
        ));
        assert_ok!(Bridge::acknowledge_proposal(
            RuntimeOrigin::signed(RELAYER_B),
            prop_id,
            src_id,
            r_id,
            [0u8; 32],
            Box::new(proposal.clone())
        ));
        assert_eq!(Verifier::receipts(ferdie.clone(), alice.clone()), None);
    })
}

#[test]
fn transfer_out_charge_stable_non_native() {
    new_test_ext().execute_with(|| {
        let dest_chain = 1;
        let ferdie: AccountId = AccountKeyring::Ferdie.into();
        let recipient = vec![99];
        let contract_address = "b20f54288947a89a4891d181b10fe04560b55c5e";
        let denom = XToken::ERC20(
            br#"DENOM"#.to_vec(),
            hex::decode(contract_address).unwrap(),
            Zero::zero(),
            true,
            18,
        );

        let resource_id = derive_resource_id(
            dest_chain,
            0,
            hex::decode(contract_address).unwrap().as_slice(),
        )
        .unwrap();
        let resource = b"ChainBridgeHandler.transfer_in".to_vec();
        let (chainid, _, contract) = decode_resource_id(resource_id).unwrap();
        assert_ok!(Assets::associate_token(
            RuntimeOrigin::signed(TREASURY),
            chainid,
            contract,
            1u32
        ));
        assert_ok!(Bridge::set_resource(
            RuntimeOrigin::signed(TREASURY),
            resource_id,
            resource
        ));
        assert_ok!(Assets::issue(RuntimeOrigin::signed(TREASURY), denom));
        assert_ok!(Assets::mark_stable(RuntimeOrigin::signed(TREASURY), 1));
        let amount: Balance = 5 * DOLLARS;
        assert_ok!(Assets::do_mint(1, &ferdie, amount, None));
        assert_ok!(Bridge::whitelist_chain(
            RuntimeOrigin::signed(TREASURY),
            dest_chain.clone()
        ));
        assert_eq!(
            ChainBridgeTransfer::calculate_bridging_fee(1, &1),
            2 * DOLLARS
        );
        assert_ok!(ChainBridgeTransfer::transfer_out(
            RuntimeOrigin::signed(ferdie.clone()),
            amount,
            resource_id,
            recipient.clone(),
            dest_chain,
        ));

        // make sure transfer have 0 amount
        assert_eq!(Assets::free_balance(&1, &ferdie), 0);
        assert_eq!(Assets::free_balance(&1, &TREASURY), 2 * DOLLARS);
        assert_events(vec![RuntimeEvent::Bridge(bridge::Event::FungibleTransfer(
            dest_chain,
            1,
            resource_id,
            sp_core::U256::from(3 * DOLLARS),
            recipient,
        ))]);
    })
}

#[test]
fn transfer_out_charge_unstable_non_native() {
    new_test_ext().execute_with(|| {
        let dest_chain = 1;
        let ferdie: AccountId = AccountKeyring::Ferdie.into();
        let recipient = vec![99];
        let contract_address = "b20f54288947a89a4891d181b10fe04560b55c5e";
        let denom = XToken::ERC20(
            br#"DENOM"#.to_vec(),
            hex::decode(contract_address).unwrap(),
            Zero::zero(),
            false,
            18,
        );
        let resource_id = derive_resource_id(
            dest_chain,
            0,
            hex::decode(contract_address).unwrap().as_slice(),
        )
        .unwrap();
        let resource = b"ChainBridgeHandler.transfer_in".to_vec();
        let (chainid, _, contract) = decode_resource_id(resource_id).unwrap();
        assert_ok!(Assets::associate_token(
            RuntimeOrigin::signed(TREASURY),
            chainid,
            contract,
            1u32
        ));
        assert_ok!(Bridge::set_resource(
            RuntimeOrigin::signed(TREASURY),
            resource_id,
            resource
        ));
        assert_ok!(Assets::issue(RuntimeOrigin::signed(TREASURY), denom));
        let amount: Balance = 5 * DOLLARS;
        assert_ok!(Assets::do_mint(1, &ferdie, amount, None));
        assert_ok!(Bridge::whitelist_chain(
            RuntimeOrigin::signed(TREASURY),
            dest_chain.clone()
        ));
        assert_ok!(ChainBridgeTransfer::transfer_out(
            RuntimeOrigin::signed(ferdie.clone()),
            amount,
            resource_id,
            recipient.clone(),
            dest_chain,
        ));

        // make sure transfer have 0 amount
        assert_eq!(Assets::free_balance(&1, &ferdie), 0);
        assert_events(vec![RuntimeEvent::Bridge(bridge::Event::FungibleTransfer(
            dest_chain,
            1,
            resource_id,
            sp_core::U256::from(5 * DOLLARS),
            recipient.clone(),
        ))]);

        // price = 3 * DOLLARS
        Indicator::set_price(1, DOLLARS, 3 * DOLLARS, 1);
        assert_ok!(Assets::do_mint(1, &ferdie, 10 * DOLLARS, None));
        assert_eq!(
            ChainBridgeTransfer::calculate_bridging_fee(1, &1),
            666_666_666_666_666_666
        );
        assert_ok!(ChainBridgeTransfer::transfer_out(
            RuntimeOrigin::signed(ferdie.clone()),
            10 * DOLLARS,
            resource_id,
            recipient.clone(),
            dest_chain,
        ));
        assert_eq!(Assets::free_balance(&1, &ferdie), 0);
        // charge 2/3 = 0.666_666_666_666_666_666
        assert_eq!(Assets::free_balance(&1, &TREASURY), 666_666_666_666_666_666);
        assert_events(vec![RuntimeEvent::Bridge(bridge::Event::FungibleTransfer(
            dest_chain,
            2,
            resource_id,
            sp_core::U256::from(10 * DOLLARS - 666_666_666_666_666_666),
            recipient.clone(),
        ))]);

        // price = 0.333333333333333333 * DOLLARS
        Indicator::set_price(1, 3 * DOLLARS, DOLLARS, 1);
        assert_ok!(Assets::do_mint(1, &ferdie, 100 * DOLLARS, None));
        // charge = 2 / 0.333333333333333333 * DOLLARS
        assert_eq!(
            ChainBridgeTransfer::calculate_bridging_fee(1, &1),
            6 * DOLLARS + 6
        );
        assert_ok!(ChainBridgeTransfer::transfer_out(
            RuntimeOrigin::signed(ferdie.clone()),
            100 * DOLLARS,
            resource_id,
            recipient.clone(),
            dest_chain,
        ));
        assert_eq!(Assets::free_balance(&1, &ferdie), 0);
        assert_eq!(
            Assets::free_balance(&1, &TREASURY),
            666_666_666_666_666_666 + 6 * DOLLARS + 6
        );
        assert_events(vec![RuntimeEvent::Bridge(bridge::Event::FungibleTransfer(
            dest_chain,
            3,
            resource_id,
            sp_core::U256::from(100 * DOLLARS - 6 * DOLLARS - 6),
            recipient,
        ))]);
    })
}
