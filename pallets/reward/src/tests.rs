use frame_support::{assert_noop, assert_ok};
use fuso_support::traits::Rewarding;
use sp_keyring::AccountKeyring;

use crate::mock::*;
use crate::Rewards;
use crate::*;
type RewardModule = Pallet<Test>;
type Balances = pallet_balances::Pallet<Test>;

#[test]
fn test_legacy_reward_should_work() {
    new_test_ext().execute_with(|| {
        frame_system::Pallet::<Test>::set_block_number(100);
        let alice: AccountId = AccountKeyring::Alice.into();
        let ferdie: AccountId = AccountKeyring::Ferdie.into();

        Volumes::<Test>::insert(100, 10000);
        Volumes::<Test>::insert(200, 10000);
        let vol = RewardModule::volumes(200);
        assert_eq!(vol, 10000);
        Rewards::<Test>::insert(
            &alice,
            Reward {
                confirmed: 0,
                pending_vol: 5000,
                last_modify: 100,
            },
        );
        let alice_reward = RewardModule::rewards(&alice);
        assert_eq!(
            alice_reward,
            Reward {
                confirmed: 0,
                pending_vol: 5000,
                last_modify: 100
            }
        );
        assert_ok!(RewardModule::save_trading(&alice, 5000, 200));
        let alice_reward = RewardModule::rewards(&alice);
        assert_eq!(
            alice_reward,
            Reward {
                confirmed: 500000000000000000000000,
                pending_vol: 5000,
                last_modify: 200
            }
        );

        assert_ok!(RewardModule::save_trading(&ferdie, 5000, 200));
        let ferdie_reward = RewardModule::rewards(&ferdie);
        assert_eq!(
            ferdie_reward,
            Reward {
                confirmed: 0,
                pending_vol: 5000,
                last_modify: 200
            }
        );
        let vol = RewardModule::volumes(200);
        assert_eq!(vol, 20000);
    });
}

#[test]
fn test_marketing_reward_should_work() {
    new_test_ext().execute_with(|| {
        frame_system::Pallet::<Test>::set_block_number(1);
        let alice: AccountId = AccountKeyring::Alice.into();
        let ferdie: AccountId = AccountKeyring::Ferdie.into();
        // get 5...
        assert_ok!(RewardModule::save_trading(&alice, 5000, 1));
        assert_ok!(RewardModule::save_trading(&ferdie, 5000, 1));

        assert_eq!(0, Balances::free_balance(&alice));
        // upgrade happens here
        frame_system::Pallet::<Test>::set_block_number(101);
        // get 10...
        assert_ok!(RewardModule::save_trading(&alice, 5000, 101));
        let alice_reward = RewardModule::rewards(&alice);
        assert_eq!(
            alice_reward,
            Reward {
                confirmed: 500000000000000000000000,
                pending_vol: 5000,
                last_modify: 100
            }
        );
        // alice didn't trade after upgrade
        // get 0
        RewardModule::consume_liquidity(&alice, (0, 1), 10, 101).unwrap();
        assert_eq!(
            RewardModule::marketing_rewards(&alice),
            Reward {
                confirmed: 0,
                pending_vol: 0,
                last_modify: 0,
            }
        );
        // alice placed a new order
        RewardModule::put_liquidity(&alice, (0, 1), 1000, 101);
        // get 10...
        RewardModule::consume_liquidity(&alice, (0, 1), 10, 101).unwrap();
        assert_eq!(
            RewardModule::marketing_rewards(&alice),
            Reward {
                confirmed: 0,
                pending_vol: 10,
                last_modify: 100,
            }
        );
        frame_system::Pallet::<Test>::set_block_number(301);
        RewardModule::consume_liquidity(&alice, (0, 1), 10, 301).unwrap();
        assert_eq!(
            RewardModule::marketing_rewards(&alice),
            Reward {
                confirmed: 1000000000000000000000000,
                pending_vol: 200,
                last_modify: 300,
            }
        );
        assert_noop!(
            RewardModule::remove_liquidity(&alice, (0, 1), 1000),
            Error::<Test>::InsufficientLiquidity
        );
        assert_ok!(RewardModule::remove_liquidity(&alice, (0, 1), 980));
        RewardModule::take_reward(frame_system::RawOrigin::Signed(alice.clone()).into()).unwrap();
        assert_eq!(2500000000000000000000000, Balances::free_balance(&alice));
        frame_system::Pallet::<Test>::set_block_number(501);
        RewardModule::take_reward(frame_system::RawOrigin::Signed(alice.clone()).into()).unwrap();
        assert_eq!(3500000000000000000000000, Balances::free_balance(&alice));
        RewardModule::put_liquidity(&alice, (0, 1), 1000, 301);
        assert_noop!(
            RewardModule::remove_liquidity(&alice, (0, 1), 1100),
            Error::<Test>::InsufficientLiquidity
        );
    });
}
