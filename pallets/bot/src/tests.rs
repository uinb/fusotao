#![cfg(test)]

use crate::mock::*;
use crate::{Error, Pallet};
use frame_support::dispatch::RawOrigin;
use frame_support::{assert_noop, assert_ok};
use fuso_support::traits::Token;
use fuso_support::XToken;
use pallet_fuso_token::TokenAccountData;
use pallet_fuso_verifier::Receipt;
use sp_keyring::AccountKeyring;
use sp_runtime::traits::Zero;
use sp_runtime::MultiAddress;
use std::str::FromStr;

type Bot = Pallet<Test>;
#[test]
fn test_register_bot() {
    let ferdie: AccountId = AccountKeyring::Ferdie.into();
    let bob: AccountId = AccountKeyring::Bob.into();
    let alice: AccountId = AccountKeyring::Alice.into();
    new_tester().execute_with(|| {
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
            RuntimeOrigin::signed(TREASURY),
            usdt_id
        ));
        assert_noop!(
            Bot::register(
                RuntimeOrigin::signed(bob),
                (0, 1),
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
            (0, 1),
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
                (0, 1),
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
            symbol: (0, 1),
            name: "03".as_bytes().to_vec(),
            max_instance: 1000,
            current_instance: 0,
            min_base: 10000000,
            min_quote: 10000000,
            users: vec![],
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
    let charlie: AccountId = AccountKeyring::Charlie.into();
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
            RuntimeOrigin::signed(TREASURY),
            usdt_id
        ));

        assert_ok!(Bot::register(
            RuntimeOrigin::signed(bot_account.clone()),
            (0, 1),
            "01".as_bytes().to_vec(),
            1u32,
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

        // mint 1 usdt
        let _ = pallet_fuso_token::Pallet::<Test>::do_mint(usdt_id, &alice, 1000000, None);
        // mint 1 usdt
        let _ = pallet_fuso_token::Pallet::<Test>::do_mint(usdt_id, &charlie, 1000000, None);

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
        assert_noop!(
            Bot::deposit(
                RuntimeOrigin::signed(charlie.clone()),
                bot_account.clone(),
                dominator.clone(),
                1000u128,
                1000u128
            ),
            Error::<Test>::BeyondMaxInstance
        );

        let sub0 = Bot::derive_sub_account(alice.clone(), bot_account.clone(), 0);
        let sub1 = Bot::derive_sub_account(alice.clone(), bot_account.clone(), 1);
        assert_eq!(Balances::free_balance(&sub0), 0);
        assert_eq!(Balances::reserved_balance(&sub0), 1000);
        assert_eq!(
            TokenModule::get_token_balance((&1u32, &sub1)),
            TokenAccountData {
                free: 0,
                reserved: 1000,
            }
        );
        assert_eq!(
            Verifier::receipts(dominator.clone(), sub0.clone()).unwrap(),
            Receipt::Authorize(0, 1000, 15)
        );
        assert_eq!(
            Verifier::receipts(dominator.clone(), sub1.clone()).unwrap(),
            Receipt::Authorize(1, 1000, 15)
        );
    });
}

#[test]
fn test_derive_sub_account() {
    use sp_core::ByteArray;
    let alice: AccountId = AccountKeyring::Alice.into();
    let bot: AccountId = AccountKeyring::Ferdie.into();
    let r = Bot::derive_sub_account(alice.clone(), bot.clone(), 1u32);
    assert_eq!(
        r,
        AccountId::from_str("0x768cff70bf523090fa1d09494cda1d4686361d1bc99129db3d67fe8b57649b7f")
            .unwrap()
    );
    println!(
        "参数:user_addr: {}, bot_addr: {}, tokenid: {}",
        format!("0x{}", hex::encode(alice.to_raw_vec())),
        format!("0x{}", hex::encode(bot.to_raw_vec())),
        1
    );
    println!("子账户: 0x768cff70bf523090fa1d09494cda1d4686361d1bc99129db3d67fe8b57649b7f",);
}

#[test]
fn test_withdraw() {
    let bot_account: AccountId = AccountKeyring::Ferdie.into();
    let alice: AccountId = AccountKeyring::Alice.into();
    let sub0 = Bot::derive_sub_account(alice.clone(), bot_account.clone(), 0);
    let sub1 = Bot::derive_sub_account(alice.clone(), bot_account.clone(), 1);
    new_tester().execute_with(|| {
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
            RuntimeOrigin::signed(TREASURY),
            usdt_id
        ));

        let _ = pallet_fuso_token::Pallet::<Test>::transfer_token(&alice, 0, 1000000, &sub0);
        // mint 1 usdt
        let _ = pallet_fuso_token::Pallet::<Test>::do_mint(usdt_id, &sub1, 1000000, None);

        assert_noop!(
            Bot::withdraw(RuntimeOrigin::signed(alice.clone()), bot_account.clone(), 0),
            Error::<Test>::BotNotFound
        );

        assert_ok!(Bot::register(
            RuntimeOrigin::signed(bot_account.clone()),
            (0, 1),
            "03".as_bytes().to_vec(),
            1000u32,
            100,
            100,
            "aas".as_bytes().to_vec(),
        ));
        assert_eq!(
            99999999989999999999000000u128,
            Balances::free_balance(&alice)
        );
        assert_ok!(Bot::withdraw(
            RuntimeOrigin::signed(alice.clone()),
            bot_account.clone(),
            0
        ),);
        assert_eq!(
            99999999990000000000000000u128,
            Balances::free_balance(&alice)
        );

        assert_eq!(0u128, TokenModule::free_balance(&1u32, &alice));
        assert_ok!(Bot::withdraw(
            RuntimeOrigin::signed(alice.clone()),
            bot_account.clone(),
            1
        ));
        assert_eq!(
            1000000000000000000u128,
            TokenModule::free_balance(&1u32, &alice)
        );
    });
}
