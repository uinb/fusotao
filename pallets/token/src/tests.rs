use frame_support::traits::{BalanceStatus, Currency};
use frame_support::{assert_noop, assert_ok};
use frame_system::RawOrigin;
use fuso_support::traits::{ReservableToken, Token as TokenTrait};
use fuso_support::XToken;
use sp_keyring::AccountKeyring;
use sp_runtime::traits::Zero;
use sp_runtime::MultiAddress;

use crate::mock::*;
use crate::Error;
use crate::Pallet;
use crate::TokenAccountData;

type Token = Pallet<Test>;

#[test]
fn issuing_token_and_transfer_should_work() {
    let ferdie: AccountId = AccountKeyring::Ferdie.into();
    let alice: AccountId = AccountKeyring::Alice.into();
    let usdt = XToken::NEP141(
        br#"USDT"#.to_vec(),
        br#"usdt.testnet"#.to_vec(),
        Zero::zero(),
        true,
        6,
    );
    new_test_ext().execute_with(|| {
        assert_ok!(Token::issue(RuntimeOrigin::signed(TREASURY), usdt,));
        let id = 1u32;
        assert_eq!(
            Token::get_token_info(&id),
            Some(XToken::NEP141(
                br#"USDT"#.to_vec(),
                br#"usdt.testnet"#.to_vec(),
                0,
                true,
                6
            ))
        );
        // mint 1 usdt
        let _ = Token::do_mint(id, &ferdie, 1000000, None);
        assert_eq!(
            Token::get_token_balance((&id, &ferdie)),
            TokenAccountData {
                free: 1000000000000000000,
                reserved: Zero::zero(),
            }
        );

        assert_ok!(Token::transfer(
            RuntimeOrigin::signed(ferdie.clone()),
            id.clone(),
            MultiAddress::Id(alice.clone()),
            1000000000000000000
        ));
        assert_eq!(
            Token::get_token_balance((&id, &ferdie)),
            TokenAccountData {
                free: Zero::zero(),
                reserved: Zero::zero(),
            }
        );
        assert_eq!(
            Token::get_token_balance((&id, &alice)),
            TokenAccountData {
                free: 1000000000000000000,
                reserved: Zero::zero(),
            }
        );
    });
}

const ONE: u128 = 1000000000000000000;

#[test]
fn reservable_token_should_work() {
    let ferdie: AccountId = AccountKeyring::Ferdie.into();
    let alice: AccountId = AccountKeyring::Alice.into();
    let usdt = XToken::NEP141(
        br#"USDT"#.to_vec(),
        br#"usdt.testnet"#.to_vec(),
        Zero::zero(),
        true,
        6,
    );
    new_test_ext().execute_with(|| {
        assert_ok!(Token::issue(RawOrigin::Signed(TREASURY).into(), usdt,));
        let id = 1u32;
        assert_ok!(Token::do_mint(id, &ferdie, 1000000, None));
        assert_eq!(Token::can_reserve(&id, &ferdie, ONE), true);
        assert_eq!(Token::can_reserve(&id, &ferdie, ONE * 2), false);
        assert_ok!(Token::reserve(&id, &ferdie, ONE / 2));
        assert_eq!(Token::can_reserve(&id, &ferdie, ONE), false);
        assert_eq!(
            Token::get_token_balance((&id, &ferdie)),
            TokenAccountData {
                free: ONE / 2,
                reserved: ONE / 2,
            }
        );
        assert_noop!(
            Token::transfer(
                RuntimeOrigin::signed(ferdie.clone()),
                id,
                MultiAddress::Id(alice.clone()),
                ONE,
            ),
            Error::<Test>::InsufficientBalance
        );
        assert_eq!(
            Token::get_token_balance((&id, &ferdie)),
            TokenAccountData {
                free: ONE / 2,
                reserved: ONE / 2,
            }
        );
        assert_ok!(Token::reserve(&id, &ferdie, ONE / 2));
        assert_eq!(
            Token::get_token_balance((&id, &ferdie)),
            TokenAccountData {
                free: 0,
                reserved: ONE,
            }
        );
        assert_ok!(Token::unreserve(&id, &ferdie, ONE / 2));
        assert_eq!(
            Token::get_token_balance((&id, &ferdie)),
            TokenAccountData {
                free: ONE / 2,
                reserved: ONE / 2,
            }
        );
        assert_ok!(Token::transfer(
            RuntimeOrigin::signed(ferdie.clone()),
            id.clone(),
            MultiAddress::Id(alice.clone()),
            1
        ));
        assert_ok!(Token::repatriate_reserved(
            &id,
            &ferdie,
            &alice,
            1,
            BalanceStatus::Free
        ));
        assert_eq!(
            Token::get_token_balance((&id, &ferdie)),
            TokenAccountData {
                free: ONE / 2 - 1,
                reserved: ONE / 2 - 1,
            }
        );
        assert_eq!(
            Token::get_token_balance((&id, &alice)),
            TokenAccountData {
                free: 2,
                reserved: Zero::zero(),
            }
        );
        assert_noop!(
            Token::repatriate_reserved(&id, &alice, &ferdie, 1, BalanceStatus::Free),
            Error::<Test>::InsufficientBalance
        );
    });
}

#[test]
fn test_xtoken_should_work() {
    new_test_ext().execute_with(|| {
        let alice: AccountId = AccountKeyring::Alice.into();
        let ferdie: AccountId = AccountKeyring::Ferdie.into();
        //token new
        // let token_id = Token::try_get_asset_id("USDT").unwrap();
        // assert_eq!(token_id, 1);
        // let token_id = Token::try_get_asset_id("USDC").unwrap();
        // assert_eq!(token_id, 2);
        // let token_id = Token::try_get_asset_id("USDT").unwrap();
        // assert_eq!(token_id, 1);
        // let token_name = Token::try_get_asset_name(1).unwrap();
        // assert_eq!(String::from_utf8(token_name).unwrap(), "USDT".to_string());
        let usdt = XToken::NEP141(
            br#"USDT"#.to_vec(),
            br#"usdt.testnet"#.to_vec(),
            Zero::zero(),
            true,
            6,
        );
        assert_ok!(Token::issue(
            RawOrigin::Signed(TREASURY).into(),
            usdt.clone(),
        ));
        assert_noop!(
            Token::issue(RawOrigin::Signed(TREASURY).into(), usdt.clone()),
            Error::<Test>::ContractError
        );
        let usdc = XToken::NEP141(
            br#"USDC"#.to_vec(),
            br#"usdc.testnet"#.to_vec(),
            Zero::zero(),
            true,
            6,
        );
        assert_ok!(Token::issue(RawOrigin::Signed(TREASURY).into(), usdc,));
        let token_info: XToken<u128> = Token::get_token_info(1).unwrap();
        assert_eq!(
            token_info,
            XToken::NEP141(br#"USDT"#.to_vec(), br#"usdt.testnet"#.to_vec(), 0, true, 6)
        );
        assert_noop!(
            Token::do_mint(3, &alice, 100000000000, Option::None),
            Error::<Test>::TokenNotFound
        );

        assert_ok!(Token::do_mint(1, &alice, 1000000000, Option::None));
        let b: TokenAccountData<u128> = Token::get_token_balance((&1, &alice));
        assert_eq!(
            b,
            TokenAccountData {
                free: ONE * 1000,
                reserved: 0
            }
        );
        let token_info: XToken<u128> = Token::get_token_info(1).unwrap();
        assert_eq!(
            token_info,
            XToken::NEP141(
                br#"USDT"#.to_vec(),
                br#"usdt.testnet"#.to_vec(),
                ONE * 1000,
                true,
                6,
            )
        );

        assert_ok!(Token::do_mint(1, &ferdie, 1000000000, Option::None));
        let b: TokenAccountData<u128> = Token::get_token_balance((&1, &ferdie));
        assert_eq!(
            b,
            TokenAccountData {
                free: ONE * 1000,
                reserved: 0
            }
        );
        let token_info: XToken<u128> = Token::get_token_info(1).unwrap();
        assert_eq!(
            token_info,
            XToken::NEP141(
                br#"USDT"#.to_vec(),
                br#"usdt.testnet"#.to_vec(),
                ONE * 2000,
                true,
                6
            )
        );

        assert_noop!(
            Token::do_burn(1, &alice, 1000000001, Option::None),
            Error::<Test>::InsufficientBalance
        );
        assert_ok!(Token::do_burn(1, &alice, 1000000000, Option::None));
        let b: TokenAccountData<u128> = Token::get_token_balance((&1, &alice));
        assert_eq!(
            b,
            TokenAccountData {
                free: 0,
                reserved: 0
            }
        );
        let token_info: XToken<u128> = Token::get_token_info(1).unwrap();
        assert_eq!(
            token_info,
            XToken::NEP141(
                br#"USDT"#.to_vec(),
                br#"usdt.testnet"#.to_vec(),
                ONE * 1000,
                true,
                6
            )
        );
    });
}

#[test]
fn alter_total_issuance_should_work() {
    new_test_ext().execute_with(|| {
        Token::try_mutate_issuance(&Token::native_token_id(), |v| {
            *v = v.checked_add(1000000000).unwrap();
            Ok(())
        })
        .unwrap();
        assert_eq!(
            100000000001000000000,
            Token::total_issuance(&Token::native_token_id())
        );
        let bob: AccountId = AccountKeyring::Bob.into();
        pallet_balances::Pallet::<Test>::make_free_balance_be(&bob, 1000000000);
        assert_eq!(
            100000000002000000000,
            Token::total_issuance(&Token::native_token_id())
        );
    });
}
