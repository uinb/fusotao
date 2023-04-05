use crate::mock::*;
use crate::FoundationData;
use crate::Pallet;
use beefy_primitives::crypto::Public;
use beefy_primitives::{ConsensusLog, VersionedFinalityProof};
use codec::Decode;
use frame_support::dispatch::Weight;
use frame_support::traits::{OnFinalize, OnInitialize};
use frame_system::AccountInfo;
use sp_core::H256;
use sp_keyring::AccountKeyring;
use sp_runtime::traits::TrailingZeroInput;

type Foundation = Pallet<Test>;

#[test]
pub fn test_decode_beefy_justifications() {
    let j = "01046d688007f327c9556a617cbdcf4c65bd1a997c690753465cf93e633d58af8ac79a489ce8c03300070100000000000010ccbdb780190000004467dc149f56f4f7f3c231301c2c7b9adba701a0a3be3fd0043d494e4a6d51c5001bf6a6763f18c109d8b6dace4c136c49cf780985d5bf9ab69588c410d2d3135d01616a0e66d11cc51b64547c6edf2962d6bc8100f349fabd024eecfa0b9274e927654069337792a8d820b7e98448e83a1be5fe99c47fc970a9af05b7923a1aad61015efa32dd4ce9f21314f90d8ff91a79b96666a1d590d290f6d1e9db3b61f04b6c7f7ebc0dc0b1e627c125acca31814d19b3545012915fe31d3464e1db3651ce3c01e0be99cc7fbcc85e1efd6ad8a0a7c25db458bf4c8fa492781fc0353ee54caaa05f9a8c4332a687f81a131b0b9644b6fd5fc2ac42a4921cfe464b477e2d8790d500b22a2299452c19a487081a0bc8a4827555bf2fa1c964f40e2a3c70a43e02368408069f1c735b90c2f3dd53e15801930e05549d50531a7d7ff07bdb2e2a902b100033b79fb7be44c54b58d8d7cc269f18fdd1b74763985957034beec484c88ab4ce4ce6472686f4949bce536464f6cdd1e29203532bc7b96838034dfe1e03b90b8101545e5e269a23fde982a0f469774b24c7b0a251434cde91d4d15e7c3dd67c291c092fafee59c250287172c491ee27fa5245eb2ddf2c24a11a6c71df6467dcbcaf01f8208d5d443c21a9cc9d9b4d16ae6c04ea152d41732930cd97f3baa1de55e9a7575b974a129c8476fd0f3a3e4feee6d4ffa81d8e9049528ee0230b18400d626b003b46b781f005fc6de33a5f84f24224a78964a67a2d36223bcb675e9c205e486f74ec830a4c28722ee3145b0fa738418c0e79f21b8b8bc0a6bdfce84d8bb6eb64019446a0c6345cc554773aa56c63ef69f249abd8c5b77852f0adf7678f28ca695616c27eee7fb2e2ae0b5c25a75db834ddb523cd2dc48fd45fee6d84d8c235dcf5016f106dddf366c50cb10b57d8835a1356ceb1eafb6b88d56510d57b4a2ebbe4b50b438b7364c2dab771b4327aaae463fd8d6a0e29feef816c2db998dc66d73c9001318b16769c56f3b3b60c7f226eecaba1e46ee9f9adfd263215a852e57f17080374409336ab28bcb652db6ce10187e53fd64963c3ef5773fc82cf409f8e6a555f00ed178bb1c25769d78d488c450b24039ab5bd73d24ac93b2cf52bf9d79e9e1d88195aeecafe922c8fd6b39a74947777841604b5549d9fdf8a3c778f9059df0a2600289ee7d488bc55b6f764036c0f679f15e39008357755e3fb48bcd89a48e4dc752f492091938e79f97b673a90663b5eddaaeda9fc8c7560de7be12c6b4fcacfce0190a8e2da396a5e2c24ff4738b488f7c4db5ef03a5d311c5aad45198eaa7375d253ccfaf65999b6612f81e307f9f7cd59dade0f662e1747b8dda885b80820cb140045ece1a9cc55dea7b6eb737687c6167dd2f898d6b980fc568e771412f60a09cb776fdfa7e47cfc88da8a759b2481c2669c94ba6c2bead779c8f2d674bb003de201f3c3ad2c18aa57c92e1e8eaab72ea477f1de622a53d3c985d394b11215ce8a4631f36f78bbb89ccb9bf6277b94a474eaa5e1273c76da8d755a12e48b58bdb2d700";
    let hex = hex::decode(j).unwrap();
    let r: VersionedFinalityProof<u32, H256> =
        Decode::decode(&mut TrailingZeroInput::new(hex.as_ref())).unwrap();
    println!("{:?}", r);
}

#[test]
pub fn test_decode_beefy_new_authories() {
    let j = "0164020122633f657eb1eac81da03782908e0baabd905236cef3fcd2daf81e929b6f77023a919502c5050fe4c60df4fdff562f5b0c1b1f42ae56b7b4a88f34efea37528803c35f266187532ca8a9f4fb100da6236a7bc3a530de0c3c19ee9cdd8a5c5cc38503723c248c206ae89cc0ac9bdeaaf9aead09fd24517d117a6875170166b2f87e3a038a71b7825c60fd708ac5e7f72a74ac305a72a8ff49ae1b997f7cfdc1505593a2028f5ff02bcc17e81287c3b943ff7e6dd91fc60b396fa01e5080ab6b4efadb8a0d03578885dae5394714b36b803808017e1b2ed2f5207768d5eb25055075a35aca240360eb470fd233903188d97b47cd8691021d8da90bcf5d2e576d760ab79387bb7802bb762a5697981dae79509fedb24ba25b6e5cb6e91a3c4b99ff2b896f0c61966b02e2d7f78f818114de1f58d3cb226081dad716125e128251cb73ca4a752e40a6bb0337ad47cb09bd0ece4eb972f32def4335b746f21bc7d7e1ce89db8b5316c0e52203431f27612c5c2565ea3ada3a28e322b967bd9fcaca7703d48dcb980659dd7e37027aff8c422bced8ff9f27d8861f305b6cb22902d3c180b5b8164923efcddaa68a02c73841bdec71918192bcab42ad3f39e3a3d636377a88bff5e3c2ca7bf6176e04032d6130079f19a3716d348ec9781395a88946ee2f2b9d10a5e78d9b142ac51b8502f39f806b877709aba755c9cd8477f8d5cd4182be6354c931828d2c363f19d598028e9b93500522bde7210064cea587ccbdce3aa702139fdaa1bd3503eeac472c2a0312ecdeda02292b424ea665c0b2d726bc2d16b496829390ce7eafd3b9ad718eb302d61eb859039d1d81ecec359a8c8c4b14adc81b33722d90f7125b38edbfb2a8bc029e49d8f6560984a1c053ddfcd4f58c49de5b82ff19a74864fc76cb1358823e05028fefcd513cedee6d6ca2972eb4557f74f68447abb00fd043b0d7ea85c5ed9ce8034528cc52e9e5728c2276ba536b620882df80eb4afe3ffb79c5a9c44a7e9d07ea039f720016ab4f706777deef01c1969b9ac545db300db25a2b7a516e3346c31fae03e585f37a0e1b9f56808f15180a3c606e9d72be4aa607c87d32d2ba3b97c2a4e502219f8149326de426455e1f8c9922a25427bf10f794f694d4609580ec92af29f10d01000000000000";
    let hex = hex::decode(j).unwrap();
    let r: ConsensusLog<Public> =
        Decode::decode(&mut TrailingZeroInput::new(hex.as_ref())).unwrap();
    match r {
        ConsensusLog::AuthoritiesChange(s) => {
            println!("{:?}", s);
        }
        ConsensusLog::OnDisabled(_) => {}
        ConsensusLog::MmrRoot(_) => {}
    }
}

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
