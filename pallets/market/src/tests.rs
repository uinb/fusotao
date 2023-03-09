use crate::mock::*;
use crate::Error;
use frame_support::traits::{OnFinalize, OnInitialize};
use frame_support::{assert_noop, assert_ok};
use frame_system::RawOrigin;
use fuso_support::traits::MarketManager;
use fuso_support::{traits::FeeBeneficiary, XToken};
use sp_keyring::AccountKeyring;
use sp_runtime::traits::Zero;

type Token = pallet_fuso_token::Pallet<Test>;
type System = frame_system::Pallet<Test>;
type Market = crate::Pallet<Test>;

#[test]
pub fn register_broker_should_work() {
    new_tester().execute_with(|| {
        let charlie: AccountId = AccountKeyring::Charlie.into();
        System::set_block_number(15);
        let ferdie: AccountId = AccountKeyring::Ferdie.into();
        assert_ok!(Market::register_broker(
            RuntimeOrigin::signed(ferdie.clone()),
            b"127.0.0.1".to_vec(),
            charlie.clone().into(),
        ));
        assert_eq!(Balances::free_balance(&ferdie), 90000 * DOLLARS);
        assert_eq!(Market::beneficiary(ferdie.clone()), charlie.clone().into());
        assert_ok!(Market::broker_set_rpc_endpoint(
            RuntimeOrigin::signed(ferdie.clone()),
            b"192.168.1.1".to_vec(),
        ));
        assert_eq!(
            crate::Brokers::<Test>::get(ferdie).unwrap().rpc_endpoint,
            b"192.168.1.1"
        );
    });
}

#[test]
pub fn register_market_should_work() {
    new_tester().execute_with(|| {
        let ferdie: AccountId = AccountKeyring::Ferdie.into();
        let contract_address = "b20f54288947a89a4891d181b10fe04560b55c5e";
        let denom = XToken::ERC20(
            br#"DENOM"#.to_vec(),
            hex::decode(contract_address).unwrap(),
            Zero::zero(),
            false,
            18,
        );
        assert_ok!(Token::issue(RuntimeOrigin::signed(ferdie.clone()), denom));
        let contract_address = "dac09188947a89a4891d181b10fe04560b512e41";
        let usdt = XToken::ERC20(
            br#"USDT"#.to_vec(),
            hex::decode(contract_address).unwrap(),
            Zero::zero(),
            false,
            18,
        );
        assert_ok!(Token::issue(RuntimeOrigin::signed(ferdie.clone()), usdt));

        let alice: AccountId = AccountKeyring::Alice.into();
        assert_noop!(
            Market::apply_for_token_listing(
                RuntimeOrigin::signed(ferdie.clone()),
                alice.clone(),
                1,
                2,
                5,
                3,
                100,
            ),
            Error::<Test>::UnsupportedQuoteCurrency
        );
        assert_ok!(Token::mark_stable(RawOrigin::Root.into(), 2));
        assert_noop!(
            Market::apply_for_token_listing(
                RuntimeOrigin::signed(ferdie.clone()),
                alice.clone(),
                3,
                2,
                5,
                3,
                100,
            ),
            Error::<Test>::TokenNotFound,
        );
        assert_ok!(Market::apply_for_token_listing(
            RuntimeOrigin::signed(ferdie.clone()),
            alice.clone(),
            1,
            2,
            5,
            3,
            100,
        ));
        assert_events(vec![RuntimeEvent::Market(crate::Event::MarketRegistered(
            alice.clone().into(),
            1,
            2,
            5,
            3,
            100,
        ))]);
        assert_ok!(Market::open_market(alice.clone(), 1, 2, 5, 3, 100));
        assert_events(vec![RuntimeEvent::Market(crate::Event::MarketOpened(
            alice.clone().into(),
            1,
            2,
            5,
            3,
            100,
        ))]);
        assert!(Market::is_market_open(alice.clone(), 1, 2, 1));
        assert_ok!(Market::close_market(alice.clone(), 1, 2, 1));
        assert!(Market::is_market_open(alice.clone(), 1, 2, 1));
        run_to_block(10);
        assert!(Market::is_market_open(alice.clone(), 1, 2, 10));
        run_to_block(12);
        assert!(!Market::is_market_open(alice.clone(), 1, 2, 12));
    });
}

pub fn assert_events(mut expected: Vec<RuntimeEvent>) {
    let mut actual: Vec<RuntimeEvent> = frame_system::Pallet::<Test>::events()
        .iter()
        .map(|e| e.event.clone())
        .collect();
    expected.reverse();
    for evt in expected {
        let next = actual.pop().expect("event expected");
        assert_eq!(next, evt.into(), "Events don't match");
    }
}

fn run_to_block(n: u32) {
    while System::block_number() < n {
        if System::block_number() > 1 {
            System::on_finalize(System::block_number());
        }
        System::set_block_number(System::block_number() + 1);
        System::on_initialize(System::block_number());
    }
}
