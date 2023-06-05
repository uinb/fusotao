#![cfg(test)]

use crate::mock::*;
use crate::{pallet, Error, Pallet};
use frame_support::dispatch::RawOrigin;
use frame_support::{assert_noop, assert_ok};
use fuso_support::XToken;
use pallet_fuso_token::TokenAccountData;
use sp_keyring::AccountKeyring;
use sp_runtime::traits::Zero;
use sp_runtime::MultiAddress;

type Bot = Pallet<Test>;
#[test]
fn test_register_bot() {
    let ferdie: AccountId = AccountKeyring::Ferdie.into();
    let bob: AccountId = AccountKeyring::Bob.into();
    new_tester().execute_with(|| {
        assert_noop!(
            Bot::register(
                RuntimeOrigin::signed(bob),
                (0, 3),
                "03".as_bytes().to_vec(),
                1000u32,
                10000000,
                10000000,
                "aas".as_bytes().to_vec(),
            ),
            pallet_balances::Error::<Test>::InsufficientBalance
        );
        assert_ok!(Bot::register(
            RuntimeOrigin::signed(ferdie.clone()),
            (0, 3),
            "03".as_bytes().to_vec(),
            1000u32,
            10000000,
            10000000,
            "aas".as_bytes().to_vec(),
        ));
        assert_eq!(99999999999999990000u128, Balances::free_balance(&ferdie));
        assert_noop!(
            Bot::register(
                RuntimeOrigin::signed(ferdie.clone()),
                (0, 3),
                "03".as_bytes().to_vec(),
                1000u32,
                10000000,
                10000000,
                "aas".as_bytes().to_vec(),
            ),
            Error::<Test>::BotAlreadyExist
        );
        let b = crate::pallet::Bot {
            staked: 10000,
            symbol: (0, 3),
            name: "03".as_bytes().to_vec(),
            max_instance: 1000,
            current_instance: 0,
            min_base: 10000000,
            min_quote: 10000000,
            desc: "aas".as_bytes().to_vec(),
        };
        assert_eq!(b, Bot::bots(&ferdie).unwrap());
    });
}

#[test]
fn test_deposit() {
    let bot_account: AccountId = AccountKeyring::Ferdie.into();
    let alice: AccountId = AccountKeyring::Alice.into();
    let dominator: AccountId = AccountKeyring::Eve.into();
    new_tester().execute_with(|| {
        //error
        assert_noop!(
            Bot::deposit(
                RuntimeOrigin::signed(alice.clone()),
                bot_account.clone(),
                dominator.clone(),
                1000u128,
                1000u128
            ),
            Error::<Test>::BotNotFound
        );

        assert_ok!(Bot::register(
            RuntimeOrigin::signed(bot_account.clone()),
            (0, 1),
            "01".as_bytes().to_vec(),
            1000u32,
            1000,
            1000,
            "aas".as_bytes().to_vec(),
        ));

        assert_noop!(
            Bot::deposit(
                RuntimeOrigin::signed(alice.clone()),
                bot_account.clone(),
                dominator.clone(),
                100u128,
                100u128
            ),
            Error::<Test>::MinimalAmountRequired
        );

        frame_system::Pallet::<Test>::set_block_number(15);
        assert_ok!(Verifier::register(
            RuntimeOrigin::signed(dominator.clone()),
            b"cool".to_vec()
        ));
        assert_ok!(Verifier::launch(
            RawOrigin::Root.into(),
            sp_runtime::MultiAddress::Id(dominator.clone())
        ));
        assert_ok!(Verifier::stake(
            RuntimeOrigin::signed(alice.clone()),
            MultiAddress::Id(dominator.clone()),
            90000
        ));

        let usdt_id = 1u32;
        let usdt = XToken::NEP141(
            br#"USDT"#.to_vec(),
            br#"usdt.testnet"#.to_vec(),
            Zero::zero(),
            true,
            6,
        );

        assert_ok!(pallet_fuso_token::Pallet::<Test>::issue(
            RuntimeOrigin::signed(alice.clone()),
            usdt,
        ));
        assert_ok!(pallet_fuso_token::Pallet::<Test>::mark_stable(
            RawOrigin::Root.into(),
            usdt_id
        ));

        // mint 1 usdt
        let _ = pallet_fuso_token::Pallet::<Test>::do_mint(usdt_id, &alice, 1000000, None);
        assert_eq!(
            pallet_fuso_token::Pallet::<Test>::get_token_balance((&usdt_id, &alice)),
            TokenAccountData {
                free: 1000000000000000000,
                reserved: Zero::zero(),
            }
        );
        assert_ok!(Bot::deposit(
            RuntimeOrigin::signed(alice.clone()),
            bot_account.clone(),
            dominator.clone(),
            1000u128,
            1000u128
        ));
    });
}
