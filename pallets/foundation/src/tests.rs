use crate::mock::*;
use crate::FoundationData;
use crate::Pallet;
use frame_support::dispatch::Weight;
use frame_support::traits::{OnFinalize, OnInitialize};
use frame_system::AccountInfo;
use sp_keyring::AccountKeyring;

type Foundation = Pallet<Test>;

#[test]
fn test_foundation() {
    new_test_ext().execute_with(|| {
        let alice: AccountId = AccountKeyring::Alice.into();
        let alice_balance: AccountInfo<u64, pallet_balances::AccountData<u128>> =
            frame_system::Pallet::<Test>::account(&alice);
        assert_eq!(alice_balance.data.reserved, 2000000000000000000000);
        assert_eq!(alice_balance.data.free, 0);

        let alice_foundation = Foundation::foundation(&alice);
        assert!(alice_foundation.is_some());
        assert_eq!(
            alice_foundation.unwrap(),
            FoundationData {
                delay_durations: 2,
                interval_durations: 1,
                times: 5,
                amount: 300000000000000000000,
                first_amount: 500000000000000000000
            }
        );

        let weight = run_to_block(5);
        assert!(weight.eq(&Weight::from_ref_time(0u64)));
        let weight = run_to_block(15);
        assert!(weight.eq(&Weight::from_ref_time(0u64)));
        run_to_block(20);
        let alice_foundation = Foundation::foundation(&alice);
        assert_eq!(
            alice_foundation.unwrap(),
            FoundationData {
                delay_durations: 2,
                interval_durations: 1,
                times: 5,
                amount: 300_000000000_000000000,
                first_amount: 500_000000000_000000000
            }
        );
        let alice_balance: AccountInfo<u64, pallet_balances::AccountData<u128>> =
            frame_system::Pallet::<Test>::account(&alice);
        assert_eq!(alice_balance.data.reserved, 1500000000000000000000);
        assert_eq!(alice_balance.data.free, 500000000000000000000);

        let bob: AccountId = AccountKeyring::Bob.into();
        let ferdie: AccountId = AccountKeyring::Ferdie.into();
        assert!(Foundation::put_into_vault(
            RuntimeOrigin::signed(ferdie.clone()),
            bob.clone(),
            FoundationData {
                delay_durations: 3,
                interval_durations: 1,
                times: 5,
                amount: 83333_333333333_333333333,
                first_amount: 83333_333333333_333333333,
            },
        )
        .is_ok());
        assert_eq!(0, Balances::free_balance(&bob));
        assert_eq!(499999_999999999_999999998, Balances::reserved_balance(&bob));
        run_to_block(21);
        assert_eq!(0, Balances::free_balance(&bob));
        assert_eq!(499999_999999999_999999998, Balances::reserved_balance(&bob));
        run_to_block(30);
        assert_eq!(83333_333333333_333333333, Balances::free_balance(&bob));
        assert_eq!(416666_666666666_666666665, Balances::reserved_balance(&bob));
        run_to_block(40);
        assert_eq!(166666_666666666_666666666, Balances::free_balance(&bob));
        assert_eq!(333333_333333333_333333332, Balances::reserved_balance(&bob));
        run_to_block(50);
        run_to_block(60);
        let alice_foundation_data = Foundation::foundation(&alice);
        assert!(alice_foundation_data.is_some());
        let alice_foundation = Foundation::foundation(&alice);
        assert_eq!(
            alice_foundation.unwrap(),
            FoundationData {
                delay_durations: 2,
                interval_durations: 1,
                times: 1,
                amount: 300000000000000000000,
                first_amount: 500000000000000000000
            }
        );

        run_to_block(70);
        let alice_foundation_data = Foundation::foundation(&alice);
        assert!(alice_foundation_data.is_none());
        let alice_balance: AccountInfo<u64, pallet_balances::AccountData<u128>> =
            frame_system::Pallet::<Test>::account(&alice);
        assert_eq!(alice_balance.data.free, 2000000000000000000000);
        assert_eq!(alice_balance.data.reserved, 0);

        let alice_foundation_data = Foundation::foundation(&alice);
        assert!(alice_foundation_data.is_none());
        let alice_balance: AccountInfo<u64, pallet_balances::AccountData<u128>> =
            frame_system::Pallet::<Test>::account(&alice);
        assert_eq!(alice_balance.data.free, 2000000000000000000000);
        assert_eq!(alice_balance.data.reserved, 0);
    });
}

fn run_to_block(n: u32) -> Weight {
    while System::block_number() < n {
        if System::block_number() > 1 {
            System::on_finalize(System::block_number());
        }
        System::set_block_number(System::block_number() + 1);
        System::on_initialize(System::block_number());
        Foundation::on_initialize(System::block_number());
    }
    System::on_finalize(System::block_number());
    System::set_block_number(System::block_number() + 1);
    System::on_initialize(System::block_number());
    Foundation::on_initialize(System::block_number())
}
