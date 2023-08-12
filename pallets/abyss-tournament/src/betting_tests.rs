#![cfg(test)]

use crate::mock::*;
use crate::{
    Battle, BattleStatus, BattleType, Betting, BettingType, Error, OddsItem, Pallet, Season,
    SeasonStatus, VoteSelect, VoteSelectInfo, NPC,
};
use codec::Encode;

use frame_support::{assert_noop, assert_ok};
use fuso_support::XToken;
use pallet_fuso_token::TokenAccountData;
use sp_core::crypto::Ss58Codec;
use sp_core::ByteArray;
use sp_keyring::AccountKeyring;
use sp_runtime::traits::Zero;

type Tournament = Pallet<Test>;

#[test]
fn test_all() {
    new_test_ext().execute_with(|| {
        init();
        create_betting();
        do_bet();
        set_result();
        claim();
    });
}

pub fn claim() {
    let alice: AccountId = AccountKeyring::Alice.into();

    assert_eq!(
        pallet_fuso_token::Pallet::<Test>::get_token_balance((&1u32, &alice)),
        TokenAccountData {
            free: 9940_000_000_000_000_000_000,
            reserved: Zero::zero(),
        }
    );

    assert_eq!(
        Tournament::get_betting_records_info((&alice, 1)),
        (vec![(1, 200, 20_000_000_000_000_000_000)], false)
    );

    assert_eq!(
        Tournament::get_betting_records_info((&alice, 2)),
        (vec![(0, 600, 40_000_000_000_000_000_000)], false)
    );
    assert_ok!(Tournament::betting_claim(
        RuntimeOrigin::signed(alice.clone()),
        1
    ));

    assert_noop!(
        Tournament::betting_claim(RuntimeOrigin::signed(alice.clone()), 1),
        Error::<Test>::HaveNoBonus
    );

    assert_ok!(Tournament::betting_claim(
        RuntimeOrigin::signed(alice.clone()),
        2
    ));

    assert_noop!(
        Tournament::betting_claim(RuntimeOrigin::signed(alice.clone()), 2),
        Error::<Test>::HaveNoBonus
    );

    assert_eq!(
        pallet_fuso_token::Pallet::<Test>::get_token_balance((&1u32, &alice)),
        TokenAccountData {
            free: 10180_000_000_000_000_000_000,
            reserved: Zero::zero(),
        }
    );

    assert_eq!(
        Tournament::get_betting_records_info((&alice, 1)),
        (vec![(1, 200, 20_000_000_000_000_000_000)], true)
    );

    assert_eq!(
        Tournament::get_betting_records_info((&alice, 2)),
        (vec![(0, 600, 40_000_000_000_000_000_000)], true)
    );

    assert_eq!(
        pallet_fuso_token::Pallet::<Test>::get_token_balance((
            &1u32,
            &Tournament::get_betting_treasury(1)
        )),
        TokenAccountData {
            free: 100_000_000_000_000_000_000,
            reserved: Zero::zero(),
        }
    );

    assert_eq!(
        pallet_fuso_token::Pallet::<Test>::get_token_balance((
            &1u32,
            &Tournament::get_betting_treasury(2)
        )),
        TokenAccountData {
            free: 60_000_000_000_000_000_000,
            reserved: Zero::zero(),
        }
    );
}

pub fn set_result() {
    assert_ok!(Tournament::set_result(
        RuntimeOrigin::signed(TREASURY),
        1u32,
        3u8,
        0u8,
        "11".to_string().into_bytes()
    ));
    assert_ok!(Tournament::league_settle(
        RuntimeOrigin::signed(TREASURY),
        1u32,
    ));
}

pub fn do_bet() {
    let alice: AccountId = AccountKeyring::Alice.into();
    assert_noop!(
        Tournament::go_bet(
            RuntimeOrigin::signed(alice.clone()),
            1u32,
            1,
            10_000_000_000_000_000_000
        ),
        Error::<Test>::BettingAmountTooSmall
    );
    assert_noop!(
        Tournament::go_bet(
            RuntimeOrigin::signed(alice.clone()),
            1u32,
            1,
            100_000_000_000_000_000_000
        ),
        Error::<Test>::BettingAmountOverflow
    );
    assert_noop!(
        Tournament::go_bet(
            RuntimeOrigin::signed(alice.clone()),
            1u32,
            3,
            20_000_000_000_000_000_000
        ),
        Error::<Test>::SelectIndexOverflow
    );

    assert_ok!(Tournament::go_bet(
        RuntimeOrigin::signed(alice.clone()),
        1u32,
        1,
        20_000_000_000_000_000_000
    ));

    assert_eq!(
        Tournament::get_betting_info(&1),
        Some(Betting {
            creator: TREASURY,
            pledge_account: Tournament::get_betting_treasury(1),
            betting_type: BettingType::WinLose,
            battles: vec![1],
            odds: vec![
                OddsItem {
                    win_lose: vec![1],
                    score: vec![],
                    o: 200,
                    total_compensate_amount: 0,
                },
                OddsItem {
                    win_lose: vec![2],
                    score: vec![],
                    o: 200,
                    total_compensate_amount: 40_000_000_000_000_000_000,
                }
            ],
            token_id: 1u32,
            min_betting_amount: 20000000000000000000,
            season: 1u32
        })
    );

    assert_noop!(
        Tournament::go_bet(
            RuntimeOrigin::signed(alice.clone()),
            2u32,
            0,
            10_000_000_000_000_000_000
        ),
        Error::<Test>::BettingAmountTooSmall
    );
    assert_noop!(
        Tournament::go_bet(
            RuntimeOrigin::signed(alice.clone()),
            2u32,
            0,
            100_000_000_000_000_000_000
        ),
        Error::<Test>::BettingAmountOverflow
    );
    assert_noop!(
        Tournament::go_bet(
            RuntimeOrigin::signed(alice.clone()),
            2u32,
            6,
            20_000_000_000_000_000_000
        ),
        Error::<Test>::SelectIndexOverflow
    );
    assert_ok!(Tournament::go_bet(
        RuntimeOrigin::signed(alice.clone()),
        2u32,
        0,
        20_000_000_000_000_000_000
    ));
    assert_ok!(Tournament::go_bet(
        RuntimeOrigin::signed(alice.clone()),
        2u32,
        0,
        20_000_000_000_000_000_000
    ));
    assert_noop!(
        Tournament::go_bet(
            RuntimeOrigin::signed(alice.clone()),
            2u32,
            0,
            20_000_000_000_000_000_000
        ),
        Error::<Test>::BettingAmountOverflow
    );
    assert_noop!(
        Tournament::go_bet(
            RuntimeOrigin::signed(alice.clone()),
            5u32,
            0,
            20_000_000_000_000_000_000
        ),
        Error::<Test>::BettingNotFound
    );

    assert_eq!(
        Tournament::get_betting_info(&2),
        Some(Betting {
            creator: TREASURY,
            pledge_account: Tournament::get_betting_treasury(2),
            betting_type: BettingType::Score,
            battles: vec![1],
            odds: vec![
                OddsItem {
                    win_lose: vec![],
                    score: vec![(3, 0)],
                    o: 600,
                    total_compensate_amount: 240_000_000_000_000_000_000,
                },
                OddsItem {
                    win_lose: vec![],
                    score: vec![(3, 1)],
                    o: 600,
                    total_compensate_amount: 0,
                },
                OddsItem {
                    win_lose: vec![],
                    score: vec![(3, 2)],
                    o: 600,
                    total_compensate_amount: 0,
                },
                OddsItem {
                    win_lose: vec![],
                    score: vec![(0, 3)],
                    o: 600,
                    total_compensate_amount: 0,
                },
                OddsItem {
                    win_lose: vec![],
                    score: vec![(1, 3)],
                    o: 600,
                    total_compensate_amount: 0,
                },
                OddsItem {
                    win_lose: vec![],
                    score: vec![(2, 3)],
                    o: 600,
                    total_compensate_amount: 0,
                },
            ],
            token_id: 1u32,
            min_betting_amount: 20000000000000000000,
            season: 1u32
        })
    );
}

pub fn init() {
    assert_ok!(Tournament::create_npc(
        RuntimeOrigin::signed(TREASURY),
        b"npc1".to_vec(),
        b"fsgrethges".to_vec(),
        b"sgseirgeiwrgwerhw".to_vec(),
        b"csghert".to_vec(),
    ));
    assert_eq!(
        Tournament::get_npc_info(&1),
        Some(NPC {
            name: b"npc1".to_vec(),
            img_url: b"fsgrethges".to_vec(),
            story: b"sgseirgeiwrgwerhw".to_vec(),
            features: b"csghert".to_vec(),
        })
    );

    assert_ok!(Tournament::create_npc(
        RuntimeOrigin::signed(TREASURY),
        b"npc2".to_vec(),
        b"fsgrethges".to_vec(),
        b"sgseirgeiwrgwerhw".to_vec(),
        b"csghert".to_vec(),
    ));

    assert_ok!(Tournament::create_season(
        RuntimeOrigin::signed(TREASURY),
        b"sdsd".to_vec(),
        "2023-07-25 00:00:00".into(),
        7,
        BattleType::QuarterFinals,
        100000000000000000000
    ));
    assert_ok!(Tournament::set_default_season(
        RuntimeOrigin::signed(TREASURY),
        1,
    ));

    assert_ok!(Tournament::update_season_current_round(
        RuntimeOrigin::signed(TREASURY),
        1,
        BattleType::League
    ));
    assert_ok!(Tournament::create_battle(
        RuntimeOrigin::signed(TREASURY),
        1,
        BattleType::League,
        1,
        2,
        "2023-07-30 00:00:00".into(),
        1
    ));

    let alice: AccountId = AccountKeyring::Alice.into();
    let awt_id = 1u32;
    let awt = XToken::NEP141(
        br#"AWT"#.to_vec(),
        br#"AWT"#.to_vec(),
        Zero::zero(),
        false,
        2,
    );
    assert_ok!(pallet_fuso_token::Pallet::<Test>::issue(
        RuntimeOrigin::signed(alice.clone()),
        awt,
    ));
    let _ = pallet_fuso_token::Pallet::<Test>::do_mint(awt_id, &alice, 1000000, None);
    let _ = pallet_fuso_token::Pallet::<Test>::do_mint(awt_id, &TREASURY, 1000000, None);
}

pub fn create_betting() {
    assert_ok!(Tournament::create_betting(
        RuntimeOrigin::signed(TREASURY),
        BettingType::WinLose,
        vec![1],
        vec![],
        1,
        100_000_000_000_000_000_000
    ));

    assert_eq!(
        Tournament::get_betting_info(&1),
        Some(Betting {
            creator: TREASURY,
            pledge_account: Tournament::get_betting_treasury(1),
            betting_type: BettingType::WinLose,
            battles: vec![1],
            odds: vec![
                OddsItem {
                    win_lose: vec![1],
                    score: vec![],
                    o: 200,
                    total_compensate_amount: 0,
                },
                OddsItem {
                    win_lose: vec![2],
                    score: vec![],
                    o: 200,
                    total_compensate_amount: 0,
                }
            ],
            token_id: 1u32,
            min_betting_amount: 20000000000000000000,
            season: 1u32
        })
    );
    assert_noop!(
        Tournament::create_betting(
            RuntimeOrigin::signed(TREASURY),
            BettingType::Score,
            vec![1],
            vec![],
            1,
            10000_000_000_000_000_000_000
        ),
        pallet_fuso_token::Error::<Test>::InsufficientBalance
    );
    assert_eq!(
        pallet_fuso_token::Pallet::<Test>::get_token_balance((&1u32, &TREASURY)),
        TokenAccountData {
            free: 9900_000_000_000_000_000_000,
            reserved: Zero::zero(),
        }
    );
    assert_ok!(Tournament::create_betting(
        RuntimeOrigin::signed(TREASURY),
        BettingType::Score,
        vec![1],
        vec![],
        1,
        300_000_000_000_000_000_000
    ));
    assert_eq!(
        Tournament::get_betting_info(&2),
        Some(Betting {
            creator: TREASURY,
            pledge_account: Tournament::get_betting_treasury(2),
            betting_type: BettingType::Score,
            battles: vec![1],
            odds: vec![
                OddsItem {
                    win_lose: vec![],
                    score: vec![(3, 0)],
                    o: 600,
                    total_compensate_amount: 0,
                },
                OddsItem {
                    win_lose: vec![],
                    score: vec![(3, 1)],
                    o: 600,
                    total_compensate_amount: 0,
                },
                OddsItem {
                    win_lose: vec![],
                    score: vec![(3, 2)],
                    o: 600,
                    total_compensate_amount: 0,
                },
                OddsItem {
                    win_lose: vec![],
                    score: vec![(0, 3)],
                    o: 600,
                    total_compensate_amount: 0,
                },
                OddsItem {
                    win_lose: vec![],
                    score: vec![(1, 3)],
                    o: 600,
                    total_compensate_amount: 0,
                },
                OddsItem {
                    win_lose: vec![],
                    score: vec![(2, 3)],
                    o: 600,
                    total_compensate_amount: 0,
                },
            ],
            token_id: 1u32,
            min_betting_amount: 20000000000000000000,
            season: 1u32
        })
    );
    assert_eq!(
        pallet_fuso_token::Pallet::<Test>::get_token_balance((&1u32, &TREASURY)),
        TokenAccountData {
            free: 9600_000_000_000_000_000_000,
            reserved: Zero::zero(),
        }
    );
    assert_eq!(Tournament::get_bettings_by_battle(1), vec![1, 2]);
}
