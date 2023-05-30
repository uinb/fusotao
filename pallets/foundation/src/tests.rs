use crate::mock::*;
use crate::FoundationData;
use crate::Pallet;
use anyhow::anyhow;
use beefy_primitives::crypto::Public;
use beefy_primitives::mmr::{BeefyAuthoritySet, MmrLeaf, MmrLeafVersion};
use beefy_primitives::{ConsensusLog, VersionedFinalityProof};

use ckb_merkle_mountain_range::helper::pos_height_in_tree;
use codec::{Decode, Encode};
use frame_support::crypto::ecdsa::ECDSAExt;
use frame_support::dispatch::Weight;
use frame_support::inherent::BlockT;
use frame_support::sp_core_hashing_proc_macro::keccak_256;
use frame_support::traits::{OnFinalize, OnInitialize};
use frame_system::AccountInfo;
use hex::ToHex;
use sc_finality_grandpa::GrandpaJustification;
use secp256k1::PublicKey;
use sp_core::crypto::Ss58Codec;
use sp_core::ecdsa::Signature;
use sp_core::{keccak_256, ByteArray, Hasher, H256};
use sp_keyring::AccountKeyring;
use sp_runtime::generic::{Block, Header};
use sp_runtime::traits::{Convert, Keccak256, TrailingZeroInput};
type Foundation = Pallet<Test>;

#[test]
pub fn test_decode_beefy_justifications() -> anyhow::Result<()> {
    let j = "01046d68804b709d64520c8f7f21b79e6db198ed9e150d6a4dce6ee46fee166a59740fdc9d49e43a00ca01000000000000108bff1780190000004425b5a0f2710aa89bed949bd443c7e3cc63cc3b29a38571eb0e1181ea7e196bdb023fec6f36c94eae7ee35b9849cda20e8b44910a5fa99389a71b45d56c594cd500b4608f32dd68f09024a9c08892b8344ae5a1389ce367f8f579d0a1c4a95f9b9709547a9cc7f539188f361fb56ff251756c07916afd42e942e2500a1cd1b46b200121c37f90a6001f79a48462712cc6b7a4707fb281d2841293532efbc2fad1c4b93033d82f969b1661eb7df208b4cacd1c87de641f91c365ce29c4d7f35415541b0133bee15b7ce36589674a2d2009e56f45c0cfb9f129e7494083896df5cc1e4ba609a514097bf549cceddfd4f8029ebc84e60d923cdce185e54e424018fa23a211013500892143fc20314e90143e18edc069cc10c1f9972e3ed9990e7c3e207368d833e37065e65ba5103bbe07a54f8bf11d7c0972b7db0684e8030df007fcc702bb000ec04560b3e414bfa558a457a5bfcf70de8cd637ace9341c40c63f174aa652cb6bdedc928b10916b29656047418c99f57709930c75748b53d83b1ba73e6c3dd701711ca4f451fcd52285b32a8ac567f78a327b8078314aa80668b1c4e2f85fb5c478a745e1052232105d837490fbb0eec63c1d32ce50bb86baf7b5c6ccb464b98b016dd086ba5cd13fc548dbe2c3b49b4709b40a4dfe1a9fe95364b5cf8aecd58440177c835bdf05c3fb60d8e1cc2af71d41db866a37d282ec770626337334a8987701b840f0f32f7720ddf01981b034599eadbd09a845ad861edef5b2dc889e743d8b6aa8c9380347bb74ab3f59269ef88895fe632b86c7360ebdf4380a600129e69801b1149312448c0f8ba1f8064174a6a3ca1d810540f4254fe0aea0da19c188c6df3f49fccad931a97a4045b58550034d96cbd15da56f5c235b4af2e7c0d25f350e00f7a3b111697f0f42450822e9eeaf2c5a3e8a9273aeb334378fc635ff4c1fcfd60ad9ea9d37e3c80d5dfc23fca151ab3379f3f0aa44ef135d8993b4f78a4422f1007049d5e0deb36da3c39c6332f84c36726abdb667ae00471f8f7b8cdaa65d14c35b2621f00a833f41b6dacc1616ab263b835f10c28a4e47742a23e4e58d703848004f6599f7ab814467a81684090ae9a81c7cf26f4277e3ae9903fc13912fc1c7653f294cdb1f5e3c7637c3c1faad1c0a45f849ad6eee359f40bd2e736e8c5200dc00942f886add4d21357e4e6f8f7f6b3ae150dabf4e78018e5c544c3c7162d9f5db68bd393cb588bc637bf310712ab2c6a38993402e1affe5d2a5c9aabc81b4b6c201e8ec717b5b58f5aaa55fca89eb1b096e01fd8ddb80e3dac594dec3609ae2b9e31e91707cc7c6f341bf06543e29c4e4888e597d11e882e976a515afae9e41759d01d623bd72dbbae2dc0eb8dcdf86205bafbf2ed750909ad580fad335bc8939259275ef9814bbeaecf84671c7b1db54bc04c18e964712c89796395154e8ff8f88fc015e2ee973e9f37815806d8bc3098016cb2117e36c877f6efa5da006eba7cb530b04d663d986b2eb341d13072b8e3f458d3de165ebdcda2f476451954aa857ca9e01";
    let hex = hex::decode(j).unwrap();
    let r: VersionedFinalityProof<u32, Signature> =
        Decode::decode(&mut TrailingZeroInput::new(hex.as_ref())).unwrap();
    if let VersionedFinalityProof::V1(ref c) = r {
        /*       let s = ;
        let s = hex::decode(s).unwrap();
        let sig = Signature::decode(&mut &s[..]).unwrap();*/
        let sig = c.signatures[4].clone().unwrap();
        for s in c.signatures.clone() {
            if s.is_some() {
                let sig = s.unwrap();
                let n = c.commitment.encode();
                let h = sp_core::hashing::keccak_256(n.as_slice());
                // println!("hex message: {}", hex::encode(h));
                //  println!("sig: {}", hex::encode(sig.0));
                let pubkey = sig.recover_prehashed(&h).unwrap();
                //  println!("{:?}", hex::encode(&pubkey));
                let t = Public::from_slice(pubkey.as_ref());
                let r = pallet_beefy_mmr::BeefyEcdsaToEthereum::convert(t.unwrap());
                //   println!("0x{},", hex::encode(r));
            }
        }
        println!("------------  {}", hex::encode(c.commitment.encode()));
    }
    println!("{:?}", r);

    Ok(())
}

/*#[test]
pub fn test_compress_pubkey() {
    let p = PublicKey::from_slice(hex::decode("04f39f806b877709aba755c9cd8477f8d5cd4182be6354c931828d2c363f19d598613a5ad01c2901dc02d09626b7b5b5eb6d756dfbc57097f219360380c5d4b206").unwrap().as_slice()).unwrap();
    let r = p.serialize();
    println!("==={}", hex::encode(r));
}*/
/*#[test]
pub fn test_encode_mmr() {
    let parent_hash =
        hex::decode("4afc7fb8743d7c2c130c538097a645199cc6ee46d70a1d09536ef364b0188ed4").unwrap();
    let mut h = [0u8; 32];
    h.copy_from_slice(parent_hash.as_slice());
    let lastet_beefy_root =
        hex::decode("bbe9f37e6342bdea28f5d3aa11ecd8e2b154b9e5b5bce14b8e660bc3775a39a3").unwrap();
    let mut b = [0u8; 32];
    b.copy_from_slice(lastet_beefy_root.as_slice());
    let r: MmrLeaf<u32, H256, H256, Vec<u8>> = MmrLeaf {
        version: MmrLeafVersion::new(0, 0),
        leaf_extra: vec![],
        parent_number_and_hash: (3472262u32, sp_core::H256(h)),
        beefy_next_authority_set: BeefyAuthoritySet {
            id: 297u64,
            len: 25u32,
            root: H256(b),
        },
    };
    let r = r.using_encoded(Keccak256::hash);
    println!("------ {}", hex::encode(r.0));
}*/

/*#[test]
pub fn test_decode_grandpa_justification() {
    let j = "41e5000000000000e62f3ba46c315ddbc1f5743ee8b06a0e0c4de1332c0b40f71ca1e6ab47e265032a18330044e62f3ba46c315ddbc1f5743ee8b06a0e0c4de1332c0b40f71ca1e6ab47e265032a18330053299e405311e4c88ca89a47ce85289a5f5cf756455d91f862be350ca7ed05698da6aaa8643a9cf70c96dcdda8537fead6df23f4b9ff852c3103f12620d9d00d033d14c1afb16cff49a3a0ca4e302ef21959883785ca588705f333dfd8bf79c0e62f3ba46c315ddbc1f5743ee8b06a0e0c4de1332c0b40f71ca1e6ab47e265032a183300c19ec6efb5a36d6233aa705d76d528d8655d6e109c7076c48a9cfa0e5b925862b6d6bbaaedbb57f84792f66ed41b2989c924adf6bb4f231964ffb836839adf0609a788ae3db4318420f8785d23d4e624c0c0735b8a21dc459b115b075dc43e19e62f3ba46c315ddbc1f5743ee8b06a0e0c4de1332c0b40f71ca1e6ab47e265032a18330003daa1a6688f3b4a2a4885f1c2bf6cffe68eaa616b1dbe8c1bc24fa1235409bda1a5128b0b3163ac97b8642961cc69f430c2ea2d289f8e715e47d97420c59807123375278a6f8f7316565cac82cfb8a25c6a45e4b5af9f58f0045fea7b1abb33e62f3ba46c315ddbc1f5743ee8b06a0e0c4de1332c0b40f71ca1e6ab47e265032a1833009debfbf13eb3338fdecf2f99645e05dfedf6b0e70187bdba75e0488a8b2555447e4efeab4cc77dcb3a75b3a1d957efc21e77b72853451c903dfca7c391b42b0516513fc688e4b169815a51746b40abd14c1f92b4d6b01cc6aafb0fca0a7b9d12e62f3ba46c315ddbc1f5743ee8b06a0e0c4de1332c0b40f71ca1e6ab47e265032a183300dc8c4f7a04abf7e3d6d956f846b99826284dffada983f4d6584690742a7c5677c8473016d830051c0742891775c8fe5f481486ca5b85d73f7df99ee00700a60921c34d7c6bf0349f5fc970dcaa5d7aefb14976784afc72205d4b1cb423d23a9ae62f3ba46c315ddbc1f5743ee8b06a0e0c4de1332c0b40f71ca1e6ab47e265032a183300c402738bc9a6bcdb5f730592937ce2114a93e1d10af7c737d05f7956f92e56979f4b683396d17b2cfbb51ea80dbda14465660e51c98a002fbcb4b4679d3969013d659ed256231f234d9693899ea76394db4ed089172c5693e272d292393cf888e62f3ba46c315ddbc1f5743ee8b06a0e0c4de1332c0b40f71ca1e6ab47e265032a1833002e33e11530f78bfad1d31f1e09164bccb4b43d50b847fe0eacad293f62634810c5021dd7b7f4ff135f2d3e732fd26f06fd661b4a2d4486d1de77a23f39d2be0b53a3b2645aecbbb72245464d871d8832bf5fc98b1ae1c838d1113ce291f9dfc2e62f3ba46c315ddbc1f5743ee8b06a0e0c4de1332c0b40f71ca1e6ab47e265032a183300c51a99eb506dc6eafc15ee59c9ac6bea1c4dfe2a0fa137e28f406ea035b40ae4cd99ff8e5864cc8c197dc0bdf6c67da316f5f0446807d4b995937a4b038d8e015c340255b17f94ba8d61a68d55992f1faeaf94abe2d531d9a2c28e868b46bc91e62f3ba46c315ddbc1f5743ee8b06a0e0c4de1332c0b40f71ca1e6ab47e265032a1833009f9a9ad3b86e220027e819a816047f428315bf62199622e5975dda6f3b8d51ffcebdcfe7adb2e1726cc8acd7b89055f45304d528ce64dacad0edc362e18622035d9d7a23e55efaaab7c1312e1fb125d12fa654c121e65600cd89123ec3c89edae62f3ba46c315ddbc1f5743ee8b06a0e0c4de1332c0b40f71ca1e6ab47e265032a18330030b2f00ad63a4f816592c1e31d31c692f7de103940f595ec3463b6149c576e44a9662e906cf1c73a529ebdd811bd4cf46b47ac49e1f7ab21ce3509c97b7a1c0a9b58308c084466adc7a4c7da619a7d34fc6c84c6e836d15f21a15d0b73414408e62f3ba46c315ddbc1f5743ee8b06a0e0c4de1332c0b40f71ca1e6ab47e265032a1833009b6a7aca8bbd9f5ac7d20119bf5f6c1faf8d0511b8b164bc7c7be5080581842caa874c1961dc03bc54993dbf37449db5c5a21afe48961ffa79335c0976c94603ad88a6a5fd6d9545a0e9674f890af78ba90277f318d7f5c529f2450c52503638e62f3ba46c315ddbc1f5743ee8b06a0e0c4de1332c0b40f71ca1e6ab47e265032a183300e39bb7da4b421d83789f1264a5402d500f268d29e9d104cd5162ccc3d8914b24108e672d03f4d9ce2c1972f2f6fc06c947195950efa2f04f1fd9c3953eab3a06b02f9bd6ea82683f2a9519dd4a0654bb2d73d0cda9fc4f2f5c685ef4457b4259e62f3ba46c315ddbc1f5743ee8b06a0e0c4de1332c0b40f71ca1e6ab47e265032a1833005f8eda10367221265a558bd37537ee3118129ed97a049dc55c63d6292450053b3f71fb08df449c1aab8163e902aa19d2114c6564d7ade79877f41b1594006303b67700ec57f4d90e3a65d9816d9de20023debd9c930a319fa8d882a87eae45a5e62f3ba46c315ddbc1f5743ee8b06a0e0c4de1332c0b40f71ca1e6ab47e265032a1833004c2a48712897a980eaaf573a36fbfe4e703007e493caf8b09ac59598c52ba90a3eb7db8bcf0e265b71093d70d5443090da0b3a4268a557edf00def383ef9f305bd3f2525c285f4be0a9ee4bcab6d7622b4fa5bca931b8ce5a2f74e3b3eb9e969e62f3ba46c315ddbc1f5743ee8b06a0e0c4de1332c0b40f71ca1e6ab47e265032a183300fd4fbaf5bbdbda3b427f77e3f638b074dd7bdea0957c2e20d2542f57033c5015ec6ff81f04dd743fd4f957f3c38e52909be60955b5bce16264107b28364ad704d500ec8d3c55ee51bd5ce88c6b2045f3103c120280f2b398c0000d10263aa354e62f3ba46c315ddbc1f5743ee8b06a0e0c4de1332c0b40f71ca1e6ab47e265032a1833006f422cbf6a0d78ce0ff8a58b5624eda577d313e33a36c4a2f1d84f3f247741cfc88f6946e5b436cc61a0b1d5ed6995e8f0f6c44ba57eb000ea3826a38e893e08e9b3b084fc44999c871131caead8f7db9151b8781f39b6875da6e95504a5bb27e62f3ba46c315ddbc1f5743ee8b06a0e0c4de1332c0b40f71ca1e6ab47e265032a183300bccc9a3531244040bf2bafa7df306f3262d49dab5b9e118ec050b70f0a461d587ac59d0b645623190c1eda037d7fb0e1b36a4c4386ed1680b43afc93b275fd04ec6913a175db41639ef3daff742bdf785c156800269b3d078ebf9f8b8e4edc4300";
    let hex = hex::decode(j).unwrap();
    let r: GrandpaJustification<fuso_primitives::Block> =
        Decode::decode(&mut TrailingZeroInput::new(hex.as_ref())).unwrap();
    println!("{:?}", r);
}*/

#[test]
pub fn test_decode_beefy_new_authories() {
    let j = "0164020122633f657eb1eac81da03782908e0baabd905236cef3fcd2daf81e929b6f77023a919502c5050fe4c60df4fdff562f5b0c1b1f42ae56b7b4a88f34efea37528803c35f266187532ca8a9f4fb100da6236a7bc3a530de0c3c19ee9cdd8a5c5cc38503723c248c206ae89cc0ac9bdeaaf9aead09fd24517d117a6875170166b2f87e3a038a71b7825c60fd708ac5e7f72a74ac305a72a8ff49ae1b997f7cfdc1505593a2028f5ff02bcc17e81287c3b943ff7e6dd91fc60b396fa01e5080ab6b4efadb8a0d03578885dae5394714b36b803808017e1b2ed2f5207768d5eb25055075a35aca240360eb470fd233903188d97b47cd8691021d8da90bcf5d2e576d760ab79387bb7802bb762a5697981dae79509fedb24ba25b6e5cb6e91a3c4b99ff2b896f0c61966b02e2d7f78f818114de1f58d3cb226081dad716125e128251cb73ca4a752e40a6bb0337ad47cb09bd0ece4eb972f32def4335b746f21bc7d7e1ce89db8b5316c0e52203431f27612c5c2565ea3ada3a28e322b967bd9fcaca7703d48dcb980659dd7e37027aff8c422bced8ff9f27d8861f305b6cb22902d3c180b5b8164923efcddaa68a02c73841bdec71918192bcab42ad3f39e3a3d636377a88bff5e3c2ca7bf6176e04032d6130079f19a3716d348ec9781395a88946ee2f2b9d10a5e78d9b142ac51b8502f39f806b877709aba755c9cd8477f8d5cd4182be6354c931828d2c363f19d598028e9b93500522bde7210064cea587ccbdce3aa702139fdaa1bd3503eeac472c2a0312ecdeda02292b424ea665c0b2d726bc2d16b496829390ce7eafd3b9ad718eb302d61eb859039d1d81ecec359a8c8c4b14adc81b33722d90f7125b38edbfb2a8bc029e49d8f6560984a1c053ddfcd4f58c49de5b82ff19a74864fc76cb1358823e05028fefcd513cedee6d6ca2972eb4557f74f68447abb00fd043b0d7ea85c5ed9ce8034528cc52e9e5728c2276ba536b620882df80eb4afe3ffb79c5a9c44a7e9d07ea039f720016ab4f706777deef01c1969b9ac545db300db25a2b7a516e3346c31fae03e585f37a0e1b9f56808f15180a3c606e9d72be4aa607c87d32d2ba3b97c2a4e502219f8149326de426455e1f8c9922a25427bf10f794f694d4609580ec92af29f1a601000000000000";
    let hex = hex::decode(j).unwrap();
    let r: ConsensusLog<Public> =
        Decode::decode(&mut TrailingZeroInput::new(hex.as_ref())).unwrap();
    match r {
        ConsensusLog::AuthoritiesChange(s) => {
            println!("v id = {}", s.id());
            for v in s.validators() {
                let t = Public::from_slice(v.as_ref());
                let r = pallet_beefy_mmr::BeefyEcdsaToEthereum::convert(t.unwrap());
                println!("0x{},", hex::encode(r));
            }
        }
        ConsensusLog::OnDisabled(_) => {}
        ConsensusLog::MmrRoot(_) => {}
    }
}

/*
#[test]
pub fn test_to_eth() {
    let p =
        hex::decode("9e49d8f6560984a1c053ddfcd4f58c49de5b82ff19a74864fc76cb1358823e059207d21bf136dd3ce0b43c26763d94c383c05523c05a53363cb93cb5a6064b80").unwrap();
    let mut v: [u8; 64] = [0u8; 64];
    v.copy_from_slice(p.as_slice());
    let r = sp_core::keccak_256(v.as_slice());
    println!("{}", hex::encode(r.as_slice()));

     let public = Public::from_slice(v.as_ref()).unwrap();
    let r = pallet_beefy_mmr::BeefyEcdsaToEthereum::convert(public);
    println!("rr {}", hex::encode(r));
}
*/
#[test]
pub fn gen_authories_root() {}

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
            true,
        )
        .is_ok());
        assert_eq!(0, Balances::free_balance(&bob));
        assert_eq!(499999_999999999_999999998, Balances::reserved_balance(&bob));
        run_to_block(21);
        assert_eq!(0, Balances::free_balance(&bob));
        assert_eq!(499999_999999999_999999998, Balances::reserved_balance(&bob));
        assert!(Foundation::approve(RuntimeOrigin::signed(ferdie.clone()), bob.clone(), 3).is_ok());
        assert!(Foundation::approvals(&bob, 3).is_none());
        run_to_block(30);
        assert_eq!(83333_333333333_333333333, Balances::free_balance(&bob));
        assert_eq!(416666_666666666_666666665, Balances::reserved_balance(&bob));
        assert!(Foundation::approve(RuntimeOrigin::signed(ferdie.clone()), bob.clone(), 4).is_ok());
        assert!(Foundation::approvals(&bob, 4).is_none());
        run_to_block(40);
        assert_eq!(166666_666666666_666666666, Balances::free_balance(&bob));
        assert_eq!(333333_333333333_333333332, Balances::reserved_balance(&bob));
        assert!(Foundation::approvals(&bob, 5).is_some());
        run_to_block(50);
        // the bob's free balance shouldn't change
        assert_eq!(166666_666666666_666666666, Balances::free_balance(&bob));
        // the bob's reserved balance should be decreased
        assert_eq!(249999_999999999_999999999, Balances::reserved_balance(&bob));
        // the ferdie's free balance should be increased by 83333_333333333_333333333
        assert_eq!(249999_999999999_999999999, Balances::reserved_balance(&bob));
        assert_eq!(583333_333333333_333333335, Balances::free_balance(&ferdie));
        assert!(Foundation::approvals(&bob, 5).is_none());
        run_to_block(60);
        assert!(Foundation::approvals(&bob, 6).is_none());
        assert!(Foundation::approvals(&bob, 7).is_some());
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
