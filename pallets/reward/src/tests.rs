use frame_support::assert_ok;
use fuso_support::traits::Rewarding;
use sp_keyring::AccountKeyring;

use crate::mock::*;
use crate::Rewards;
use crate::*;
type RewardModule = Pallet<Test>;

#[test]
fn test_reward_should_work() {
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
