use super::*;
use crate::mock::*;
use crate::mock::{new_tester, AccountId};
use crate::Error;
use crate::Pallet;
use frame_support::traits::{OnFinalize, OnInitialize};
use frame_support::{assert_noop, assert_ok};
use frame_system::RawOrigin;
use fuso_support::traits::PriceOracle;
use fuso_support::{constants::*, XToken};
use sp_keyring::AccountKeyring;
use sp_runtime::{traits::Zero, MultiAddress};

type Token = pallet_fuso_token::Pallet<Test>;
type Indicator = pallet_fuso_indicator::Pallet<Test>;
type Verifier = Pallet<Test>;
type Balance = pallet_balances::Pallet<Test>;
type System = frame_system::Pallet<Test>;

#[test]
pub fn register_should_work() {
    new_tester().execute_with(|| {
        let alice: AccountId = AccountKeyring::Alice.into();
        let charlie: AccountId = AccountKeyring::Charlie.into();
        frame_system::Pallet::<Test>::set_block_number(15);
        assert_ok!(Verifier::register(
            RuntimeOrigin::signed(alice.clone()),
            b"cool".to_vec()
        ));
        let alice_dominator = Verifier::dominators(&alice);
        assert!(alice_dominator.is_some());
        assert_noop!(
            Verifier::register(RuntimeOrigin::signed(charlie.clone()), b"cool".to_vec()),
            Error::<Test>::InvalidName
        );
        assert_noop!(
            Verifier::register(RuntimeOrigin::signed(alice.clone()), b"cooq".to_vec()),
            Error::<Test>::DominatorAlreadyExists
        );
    });
}

#[test]
pub fn test_stake_unstake_should_work() {
    new_tester().execute_with(|| {
        let alice: AccountId = AccountKeyring::Alice.into();
        let ferdie: AccountId = AccountKeyring::Ferdie.into();
        let bob: AccountId = AccountKeyring::Bob.into();
        frame_system::Pallet::<Test>::set_block_number(15);
        assert_noop!(
            Verifier::stake(
                RuntimeOrigin::signed(ferdie.clone()),
                MultiAddress::Id(alice.clone()),
                10000
            ),
            Error::<Test>::DominatorNotFound
        );
        assert_ok!(Verifier::register(
            RuntimeOrigin::signed(alice.clone()),
            b"cool".to_vec()
        ));
        assert_ok!(Verifier::launch(
            RawOrigin::Root.into(),
            MultiAddress::Id(alice.clone())
        ));
        //bob doesn't have enough TAO
        assert_noop!(
            Verifier::stake(
                RuntimeOrigin::signed(bob.clone()),
                MultiAddress::Id(alice.clone()),
                10000
            ),
            pallet_balances::Error::<Test>::InsufficientBalance
        );
        assert_ok!(Verifier::stake(
            RuntimeOrigin::signed(ferdie.clone()),
            MultiAddress::Id(alice.clone()),
            1000
        ));
        let alice_dominator: Dominator<u128, u32> = Verifier::dominators(&alice).unwrap();
        assert_eq!(alice_dominator.staked, 1000);
        assert_eq!(alice_dominator.status, DOMINATOR_INACTIVE);
        let reserves = Verifier::reserves(&(RESERVE_FOR_STAKING, ferdie.clone(), 0u32), &alice);
        assert_eq!(reserves, 1000);
        assert_ok!(Verifier::stake(
            RuntimeOrigin::signed(ferdie.clone()),
            MultiAddress::Id(alice.clone()),
            9000
        ));
        let alice_dominator: Dominator<u128, u32> = Verifier::dominators(&alice).unwrap();
        assert_eq!(alice_dominator.staked, 10000);
        assert_eq!(alice_dominator.status, DOMINATOR_ACTIVE);
        let reserves = Verifier::reserves(&(RESERVE_FOR_STAKING, ferdie.clone(), 0u32), &alice);
        assert_eq!(reserves, 10000);
        assert_noop!(
            //50 < MinimalStakingAmount(100)
            Verifier::stake(
                RuntimeOrigin::signed(ferdie.clone()),
                MultiAddress::Id(alice.clone()),
                50
            ),
            Error::<Test>::LittleStakingAmount
        );
        let reserves = Verifier::reserves(&(RESERVE_FOR_STAKING, ferdie.clone(), 0u32), &alice);
        assert_eq!(reserves, 10000);
        assert_noop!(
            //10000-9990 < MinimalStakingAmount(100)
            Verifier::unstake(
                RuntimeOrigin::signed(ferdie.clone()),
                MultiAddress::Id(alice.clone()),
                9990
            ),
            Error::<Test>::LittleStakingAmount
        );
        let reserves = Verifier::reserves(&(RESERVE_FOR_STAKING, ferdie.clone(), 0u32), &alice);
        assert_eq!(reserves, 10000);
        //first unstake slot
        let current_block: BlockNumber = System::block_number();
        let unlock_at1 = current_block - current_block % 10;
        let unlock_at1 = unlock_at1 + 14400 * 4;
        assert_ok!(Verifier::unstake(
            RuntimeOrigin::signed(ferdie.clone()),
            MultiAddress::Id(alice.clone()),
            2000
        ));
        assert_eq!(unlock_at1, 57610);
        assert_eq!(Verifier::pending_unstakings(unlock_at1, &ferdie), 2000);
        run_to_block(16);
        assert_ok!(Verifier::unstake(
            RuntimeOrigin::signed(ferdie.clone()),
            MultiAddress::Id(alice.clone()),
            2000
        ));
        assert_eq!(Verifier::pending_unstakings(unlock_at1, &ferdie), 4000); //2000+2000
        let reserves = Verifier::reserves(&(RESERVE_FOR_STAKING, ferdie.clone(), 0u32), &alice);
        assert_eq!(reserves, 6000);
        assert_eq!(Balance::reserved_balance(&ferdie), 10000);
        assert_eq!(
            Balance::usable_balance(&ferdie),
            10000000000000000000 - 10000
        );
        let _: Dominator<u128, u32> = Verifier::dominators(&alice).unwrap();
        crate::pallet::PendingUnstakings::<Test>::iter().for_each(|s| println!("{:?}", s));
        assert_eq!(Verifier::pending_unstakings(unlock_at1, &ferdie), 4000);
        let reserves = Verifier::reserves(
            &(RESERVE_FOR_PENDING_UNSTAKE, ferdie.clone(), 0u32),
            &Verifier::system_account(),
        );
        assert_eq!(reserves, 4000);
        //second unstake slot
        run_to_block(25);
        let current_block: BlockNumber = System::block_number();
        let unlock_at2 = current_block - current_block % 10;
        let unlock_at2 = unlock_at2 + 14400 * 4;
        assert_eq!(unlock_at2, 57620);
        assert_ok!(Verifier::unstake(
            RuntimeOrigin::signed(ferdie.clone()),
            MultiAddress::Id(alice.clone()),
            2000
        ));
        assert_eq!(Verifier::pending_unstakings(unlock_at2, &ferdie), 2000);
        assert_ok!(Verifier::unstake(
            RuntimeOrigin::signed(ferdie.clone()),
            MultiAddress::Id(alice.clone()),
            3000
        ));
        assert_eq!(Verifier::pending_unstakings(unlock_at2, &ferdie), 5000);
        assert_eq!(Verifier::pending_unstakings(unlock_at1, &ferdie), 4000);
        assert_eq!(Balance::reserved_balance(&ferdie), 10000);
        assert_eq!(
            Balance::usable_balance(&ferdie),
            10000000000000000000 - 10000
        );
        let alice_dominator: Dominator<u128, u32> = Verifier::dominators(&alice).unwrap();
        assert_eq!(alice_dominator.staked, 1000);
        assert_eq!(alice_dominator.status, DOMINATOR_INACTIVE);
        //first unlock
        run_to_block(unlock_at1);
        assert_eq!(Balance::reserved_balance(&ferdie), 6000);
        assert_eq!(Verifier::pending_unstakings(unlock_at1, &ferdie), 0);
        assert_eq!(
            Balance::usable_balance(&ferdie),
            10000000000000000000 - 6000
        );
        assert_noop!(
            Verifier::unstake(
                RuntimeOrigin::signed(ferdie.clone()),
                MultiAddress::Id(alice.clone()),
                5000
            ),
            Error::<Test>::InsufficientBalance
        );
        //second unlock
        run_to_block(unlock_at2);
        assert_eq!(Balance::reserved_balance(&ferdie), 1000);
        assert_eq!(Verifier::pending_unstakings(unlock_at2, &ferdie), 0);
        assert_eq!(
            Balance::usable_balance(&ferdie),
            10000000000000000000 - 1000
        );
        assert_noop!(
            Verifier::unstake(
                RuntimeOrigin::signed(ferdie.clone()),
                MultiAddress::Id(alice.clone()),
                5000
            ),
            Error::<Test>::InsufficientBalance
        );
        let alice_dominator: Dominator<u128, u32> = Verifier::dominators(&alice).unwrap();
        assert_eq!(alice_dominator.staked, 1000);
        assert_eq!(alice_dominator.status, DOMINATOR_INACTIVE);
    });
}

#[test]
pub fn test_authorize_should_work() {
    new_tester().execute_with(|| {
        let alice: AccountId = AccountKeyring::Alice.into();
        let ferdie: AccountId = AccountKeyring::Ferdie.into();
        frame_system::Pallet::<Test>::set_block_number(15);
        let usdt = XToken::NEP141(
            br#"USDT"#.to_vec(),
            br#"usdt.testnet"#.to_vec(),
            Zero::zero(),
            true,
            6,
        );

        assert_ok!(Token::issue(RawOrigin::Signed(TREASURY).into(), usdt,));
        assert_ok!(Token::do_mint(1, &ferdie, 10000000, None));
        // assert_ok!(Token::issue(
        //     RuntimeOrigin::signed(ferdie.clone()),
        //     10000000000000000000,
        //     br#"USDT"#.to_vec()
        // ));
        let token_info = Token::get_token_info(1);
        assert!(token_info.is_some());
        let token_info: XToken<u128> = token_info.unwrap();
        match token_info {
            XToken::NEP141(_, _, total, _, _) => {
                assert_eq!(total, 10000000000000000000);
            }
            _ => {}
        }
        assert_ok!(Verifier::register(
            RuntimeOrigin::signed(alice.clone()),
            b"cool".to_vec()
        ));
        assert_noop!(
            Verifier::authorize(
                RuntimeOrigin::signed(ferdie.clone()),
                MultiAddress::Id(alice.clone()),
                1,
                500000000000
            ),
            Error::<Test>::DominatorInactive
        );
        assert_noop!(
            Verifier::stake(
                RuntimeOrigin::signed(ferdie.clone()),
                MultiAddress::Id(alice.clone()),
                10000
            ),
            Error::<Test>::DominatorStatusInvalid
        );
        assert_noop!(
            Verifier::authorize(
                RuntimeOrigin::signed(ferdie.clone()),
                MultiAddress::Id(alice.clone()),
                1,
                5000000000000000000000000
            ),
            Error::<Test>::DominatorInactive
        );

        assert_ok!(Verifier::launch(
            RawOrigin::Root.into(),
            MultiAddress::Id(alice.clone())
        ));

        assert_noop!(
            Verifier::authorize(
                RuntimeOrigin::signed(ferdie.clone()),
                MultiAddress::Id(alice.clone()),
                1,
                500000000000
            ),
            Error::<Test>::DominatorInactive
        );
        assert_ok!(Verifier::stake(
            RuntimeOrigin::signed(ferdie.clone()),
            MultiAddress::Id(alice.clone()),
            10000
        ));
        assert_noop!(
            Verifier::authorize(
                RuntimeOrigin::signed(ferdie.clone()),
                MultiAddress::Id(alice.clone()),
                1,
                5000000000000000000000000
            ),
            Error::<Test>::InsufficientBalance
        );
        let reserves = Verifier::reserves(&(RESERVE_FOR_AUTHORIZING, ferdie.clone(), 1u32), &alice);
        assert_eq!(reserves, 0);

        assert_noop!(
            Verifier::authorize(
                RuntimeOrigin::signed(ferdie.clone()),
                MultiAddress::Id(alice.clone()),
                0,
                5000000000000000000000000
            ),
            Error::<Test>::InsufficientBalance
        );
        assert_ok!(Verifier::authorize(
            RuntimeOrigin::signed(ferdie.clone()),
            MultiAddress::Id(alice.clone()),
            1,
            100000
        ));
        let reserves = Verifier::reserves(
            &(RESERVE_FOR_AUTHORIZING_STASH, ferdie.clone(), 1u32),
            &alice,
        );
        assert_eq!(reserves, 100000);
        let t = Verifier::reserves(
            (RESERVE_FOR_AUTHORIZING_STASH, ferdie.clone(), 1),
            alice.clone(),
        );
        assert_eq!(t, 100000);
        assert_noop!(
            Verifier::authorize(
                RuntimeOrigin::signed(ferdie.clone()),
                MultiAddress::Id(alice.clone()),
                1,
                1000000
            ),
            Error::<Test>::ReceiptAlreadyExists
        );
        let t = Verifier::reserves(
            (RESERVE_FOR_AUTHORIZING_STASH, ferdie.clone(), 1),
            alice.clone(),
        );
        assert_eq!(t, 100000);
    });
}

#[test]
pub fn test_set_price_should_work() {
    new_tester().execute_with(|| {
        let ferdie: AccountId = AccountKeyring::Ferdie.into();
        frame_system::Pallet::<Test>::set_block_number(15);
        let usdt = XToken::NEP141(
            br#"USDT"#.to_vec(),
            br#"usdt.testnet"#.to_vec(),
            Zero::zero(),
            false,
            6,
        );
        assert_ok!(Token::issue(RawOrigin::Signed(TREASURY).into(), usdt,));
        assert_ok!(Token::do_mint(1, &ferdie, 10000000, None));
        Indicator::set_price(1, 9_000_000, 3_000_000, 3333);
        let r = Indicator::get_price(&1u32.into());
        assert_eq!(r, 333333333333333333);
    });
}

#[test]
pub fn test_revoke_should_work() {
    new_tester().execute_with(|| {
        run_to_block(100);
        let alice: AccountId = AccountKeyring::Alice.into();
        let bob: AccountId = AccountKeyring::Bob.into();
        let ferdie: AccountId = AccountKeyring::Ferdie.into();
        frame_system::Pallet::<Test>::set_block_number(15);
        let usdt = XToken::NEP141(
            br#"USDT"#.to_vec(),
            br#"usdt.testnet"#.to_vec(),
            Zero::zero(),
            true,
            6,
        );

        assert_ok!(Token::issue(RawOrigin::Signed(TREASURY).into(), usdt,));
        assert_ok!(Token::do_mint(1, &ferdie, 10000000, None));
        // assert_ok!(Token::issue(
        //     RuntimeOrigin::signed(ferdie.clone()),
        //     10000000000000000000,
        //     br#"USDT"#.to_vec()
        // ));
        let token_info = Token::get_token_info(1);
        assert!(token_info.is_some());
        let token_info: XToken<u128> = token_info.unwrap();
        match token_info {
            XToken::NEP141(_, _, total, _, _) => {
                assert_eq!(total, 10000000000000000000);
            }
            _ => {}
        }
        assert_ok!(Verifier::register(
            RuntimeOrigin::signed(alice.clone()),
            b"cool".to_vec()
        ));
        assert_ok!(Verifier::launch(
            RawOrigin::Root.into(),
            MultiAddress::Id(alice.clone())
        ));
        assert_ok!(Verifier::stake(
            RuntimeOrigin::signed(ferdie.clone()),
            MultiAddress::Id(alice.clone()),
            800000000000
        ));
        run_to_block(1000);
        assert_ok!(Verifier::authorize(
            RuntimeOrigin::signed(ferdie.clone()),
            MultiAddress::Id(alice.clone()),
            1,
            500000000000
        ));
        assert_eq!(
            Verifier::receipts(alice.clone(), ferdie.clone()),
            Some(Receipt::Authorize(1, 500000000000u128.into(), 1000))
        );
        use codec::Encode;
        let mut states = GlobalStates::default();
        let key = [&[0x00][..], &ferdie.encode()[..], &u32::to_le_bytes(1)[..]].concat();
        let leaves = vec![MerkleLeaf {
            key: key.clone(),
            old_v: [0u8; 32],
            new_v: u128le_to_h256(500000000000, 0),
        }];
        let proof = gen_proofs(&mut states, &leaves);
        assert_ok!(Verifier::verify(
            RuntimeOrigin::signed(alice.clone()),
            vec![Proof {
                event_id: 1,
                user_id: ferdie.clone(),
                cmd: Command::TransferIn(1.into(), 500000000000.into()),
                leaves,
                maker_page_delta: 0,
                maker_account_delta: 0,
                merkle_proof: proof,
                root: states.root().clone().into(),
            }]
        ));
        assert_eq!(Verifier::receipts(alice.clone(), ferdie.clone()), None);

        assert_ok!(Verifier::revoke_with_callback(
            RuntimeOrigin::signed(ferdie.clone()),
            MultiAddress::Id(alice.clone()),
            1,
            500000000000,
            Box::new(crate::mock::RuntimeCall::Balances(pallet_balances::Call::<
                Test,
            >::transfer {
                dest: MultiAddress::Id(bob.clone()),
                value: 500000000000u128.into(),
            })),
        ));
        assert_eq!(
            Verifier::receipts(alice.clone(), ferdie.clone()),
            Some(Receipt::RevokeWithCallback(
                1,
                500000000000u128.into(),
                1000,
                crate::mock::RuntimeCall::Balances(pallet_balances::Call::<Test>::transfer {
                    dest: MultiAddress::Id(bob.clone()),
                    value: 500000000000u128.into(),
                })
            ))
        );
        // let call = crate::mock::Call::Balances(pallet_balances::Call::<Test>::transfer {
        //     dest: MultiAddress::Id(bob.clone()),
        //     value: 500000000000u128.into(),
        // });

        // frame_support::dispatch::Dispatchable::dispatch(
        //     call,
        //     RawOrigin::Signed(ferdie.clone()).into(),
        // )
        // .unwrap();
        let leaves = vec![MerkleLeaf {
            key: key.clone(),
            new_v: [0u8; 32],
            old_v: u128le_to_h256(500000000000, 0),
        }];
        let proof = gen_proofs(&mut states, &leaves);
        assert_ok!(Verifier::verify(
            RuntimeOrigin::signed(alice.clone()),
            vec![Proof {
                event_id: 2,
                user_id: ferdie.clone(),
                cmd: Command::TransferOut(1.into(), 500000000000.into()),
                leaves,
                maker_page_delta: 0,
                maker_account_delta: 0,
                merkle_proof: proof,
                root: states.root().clone().into(),
            }]
        ));
        assert_eq!(Verifier::receipts(alice.clone(), ferdie.clone()), None);
        run_to_block(1002);
        assert_eq!(Balances::free_balance(bob.clone()), 500000000000);
    });
}

fn u128le_to_h256(a0: u128, a1: u128) -> [u8; 32] {
    let mut v: [u8; 32] = Default::default();
    v[..16].copy_from_slice(&a0.to_le_bytes());
    v[16..].copy_from_slice(&a1.to_le_bytes());
    v
}

type GlobalStates = smt::SparseMerkleTree<
    smt::blake2b::Blake2bHasher,
    smt::H256,
    smt::default_store::DefaultStore<smt::H256>,
>;

fn gen_proofs(merkle_tree: &mut GlobalStates, leaves: &Vec<MerkleLeaf>) -> Vec<u8> {
    let keys = leaves
        .iter()
        .map(|leaf| sp_io::hashing::blake2_256(&leaf.key).into())
        .collect::<Vec<_>>();
    leaves.iter().for_each(|leaf| {
        merkle_tree
            .update(
                sp_io::hashing::blake2_256(&leaf.key).into(),
                leaf.new_v.into(),
            )
            .unwrap();
    });
    merkle_tree
        .merkle_proof(keys.clone())
        .expect("generate merkle proof failed")
        .compile(keys)
        .expect("compile merkle proof failed")
        .into()
}

fn run_to_block(n: u32) {
    while System::block_number() < n {
        if System::block_number() > 1 {
            System::on_finalize(System::block_number());
        }
        System::set_block_number(System::block_number() + 1);
        System::on_initialize(System::block_number());
        Verifier::on_initialize(System::block_number());
    }
}
