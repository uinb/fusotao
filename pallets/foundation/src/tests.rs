use crate::mock::*;
use crate::FoundationData;
use crate::Pallet;
use frame_support::dispatch::Weight;
use frame_support::traits::OnInitialize;
use frame_system::AccountInfo;
use sp_keyring::AccountKeyring;

type Foundation = Pallet<Test>;

#[test]
fn test_foundation() {
    new_test_ext().execute_with(|| {
        let alice: AccountId = AccountKeyring::Alice.into();
        frame_system::Pallet::<Test>::set_block_number(30);
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

        let weight = Foundation::on_initialize(5);
        assert!(weight.eq(&Weight::from_ref_time(0u64)));
        let weight = Foundation::on_initialize(15);
        assert!(weight.eq(&Weight::from_ref_time(0u64)));
        Foundation::on_initialize(20);
        let alice_foundation = Foundation::foundation(&alice);
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
        let alice_balance: AccountInfo<u64, pallet_balances::AccountData<u128>> =
            frame_system::Pallet::<Test>::account(&alice);
        assert_eq!(alice_balance.data.reserved, 1500000000000000000000);
        assert_eq!(alice_balance.data.free, 500000000000000000000);

        Foundation::on_initialize(30);
        Foundation::on_initialize(40);
        Foundation::on_initialize(50);
        Foundation::on_initialize(60);
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

        Foundation::on_initialize(70);
        let alice_foundation_data = Foundation::foundation(&alice);
        assert!(alice_foundation_data.is_none());
        let alice_balance: AccountInfo<u64, pallet_balances::AccountData<u128>> =
            frame_system::Pallet::<Test>::account(&alice);
        assert_eq!(alice_balance.data.free, 2000000000000000000000);
        assert_eq!(alice_balance.data.reserved, 0);

        let weight = Foundation::on_initialize(101);
        assert!(weight.eq(&Weight::from_ref_time(0u64)));
        let alice_foundation_data = Foundation::foundation(&alice);
        assert!(alice_foundation_data.is_none());
        let alice_balance: AccountInfo<u64, pallet_balances::AccountData<u128>> =
            frame_system::Pallet::<Test>::account(&alice);
        assert_eq!(alice_balance.data.free, 2000000000000000000000);
        assert_eq!(alice_balance.data.reserved, 0);
    });
}
