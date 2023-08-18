#![cfg(test)]

use crate::mock::*;
use crate::{
    Battle, BattleStatus, BattleType, Error, Pallet, Season, SeasonStatus, VoteSelect,
    VoteSelectInfo, NPC,
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
        test_no_ticket_betting();
        test_buy_ticket();
        test_initial_vote();
        test_append_votes();
        test_set_result();
        test_settle();
        test_close_season();
        test_claim();
    });
}

pub fn test_claim() {
    let alice: AccountId = AccountKeyring::Alice.into();
    let season = Tournament::get_season_info(&1).unwrap();
    assert_eq!(
        pallet_fuso_token::Pallet::<Test>::get_token_balance((&1u32, &alice)),
        TokenAccountData {
            free: 0,
            reserved: Zero::zero(),
        }
    );

    assert_ok!(Tournament::claim(RuntimeOrigin::signed(alice.clone()), 1));

    assert_eq!(
        pallet_fuso_token::Pallet::<Test>::get_token_balance((&1u32, &alice)),
        TokenAccountData {
            free: 8330000000000000000000,
            reserved: Zero::zero(),
        }
    );

    assert_eq!(
        pallet_fuso_token::Pallet::<Test>::get_token_balance((&1u32, &season.treasury)),
        TokenAccountData {
            free: 0,
            reserved: Zero::zero(),
        }
    );
    assert_noop!(
        Tournament::claim(RuntimeOrigin::signed(alice.clone()), 1),
        Error::<Test>::HaveNoBonus
    );
}

pub fn test_settle() {
    let alice: AccountId = AccountKeyring::Alice.into();
    assert_ok!(Tournament::finals_settle(
        RuntimeOrigin::signed(TREASURY),
        1,
    ),);
    assert_noop!(
        Tournament::finals_settle(RuntimeOrigin::signed(TREASURY), 1,),
        Error::<Test>::BattleStatusError
    );
    assert_ok!(Tournament::finals_settle(
        RuntimeOrigin::signed(TREASURY),
        2,
    ),);
    assert_ok!(Tournament::finals_settle(
        RuntimeOrigin::signed(TREASURY),
        3,
    ),);
    /*    assert_eq!(Tournament::get_npc_point(1, 1), (2, 2, 6, 6));
    assert_eq!(Tournament::get_npc_point(1, 2), (1, 0, 0, -3));
    assert_eq!(Tournament::get_npc_point(1, 3), (2, 1, 3, 0));
    assert_eq!(Tournament::get_npc_point(1, 4), (1, 0, 0, -3));*/
    assert_eq!(
        Tournament::get_participant_point(&1, (&alice, 0)),
        (3, 2, 6)
    );
    assert_eq!(
        Tournament::get_participant_point(&1, (&alice, 1)),
        (3, 1, 3)
    );
    assert_eq!(
        Tournament::get_participant_point(&1, (&alice, 2)),
        (3, 2, 6)
    );
    assert_eq!(
        Tournament::get_participant_point(&1, (&alice, 3)),
        (3, 0, 0)
    );
    assert_eq!(
        Tournament::get_participant_point(&1, (&alice, 4)),
        (3, 3, 9)
    );
    assert_eq!(
        Tournament::get_participant_point(&1, (&alice, 5)),
        (3, 2, 6)
    );
    assert_eq!(
        Tournament::get_participant_point(&1, (&alice, 6)),
        (3, 2, 6)
    );
}

pub fn test_close_season() {
    let season = Tournament::get_season_info(&1).unwrap();
    assert_eq!(
        pallet_fuso_token::Pallet::<Test>::get_token_balance((&1u32, &season.treasury)),
        TokenAccountData {
            free: 8330000000000000000000,
            reserved: Zero::zero(),
        }
    );
    assert_ok!(Tournament::close_season(RuntimeOrigin::signed(TREASURY), 1));
    assert_eq!(
        Tournament::get_season_info(&1),
        Some(Season {
            id: 1,
            name: b"sdsd".to_vec(),
            status: SeasonStatus::Finalized,
            treasury: AccountId::from_ss58check("5DxLSk3X52VhU76aZvPFt6Y6eTS7UW3pRT6y5WEZQrw5NYGp")
                .unwrap(),
            start_time: 1690243200,
            total_battles: 3,
            bonus_strategy: vec![
                (3, 10, 7288750000000000000000),
                (2, 50, 1041250000000000000000)
            ],
            ticket_price: 100000000000000000000,
            first_finals_battle_type: BattleType::SemiFinals,
            current_finals_battle_type: BattleType::SemiFinals,
            champion: Some(1),
            total_tickets: 98u32,
        }),
    );
}

pub fn test_set_result() {
    assert_ok!(Tournament::set_result(
        RuntimeOrigin::signed(TREASURY),
        1,
        4u8,
        1u8,
        "http://www.google.com".into()
    ));
    assert_noop!(
        Tournament::set_result(
            RuntimeOrigin::signed(TREASURY),
            7,
            2u8,
            1u8,
            "http://www.google.com".into()
        ),
        Error::<Test>::BattleNotFound
    );
    assert_ok!(Tournament::set_result(
        RuntimeOrigin::signed(TREASURY),
        2,
        4u8,
        1u8,
        "http://www.google.com".into()
    ));
    assert_ok!(Tournament::set_result(
        RuntimeOrigin::signed(TREASURY),
        3,
        4u8,
        1u8,
        "http://www.google.com".into()
    ));
    assert_eq!(
        Tournament::get_battle_info(&1),
        Some(Battle {
            season: 1,
            season_name: "sdsd".into(),
            battle_type: BattleType::SemiFinals,
            home: 1,
            visiting: 2,
            status: BattleStatus::Completed,
            start_time: 1690675200,
            position: 1,
            home_score: Some(4),
            visiting_score: Some(1),
            video_url: "http://www.google.com".into()
        })
    );

    assert_eq!(
        Tournament::get_battle_info(&2),
        Some(Battle {
            season: 1,
            season_name: "sdsd".into(),
            battle_type: BattleType::SemiFinals,
            home: 3,
            visiting: 4,
            status: BattleStatus::Completed,
            start_time: 1690675200,
            position: 2,
            home_score: Some(4),
            visiting_score: Some(1),
            video_url: "http://www.google.com".into()
        })
    );

    assert_eq!(
        Tournament::get_battle_info(&3),
        Some(Battle {
            season: 1,
            season_name: "sdsd".into(),
            battle_type: BattleType::Finals,
            home: 1,
            visiting: 3,
            status: BattleStatus::Completed,
            start_time: 1690675200,
            position: 1,
            home_score: Some(4),
            visiting_score: Some(1),
            video_url: "http://www.google.com".into()
        })
    );
    let mp = Tournament::get_season_winners(&1);
    assert_eq!(mp[&1], 1);
    assert_eq!(mp[&2], 3);
    assert_eq!(mp[&3], 1);
}

pub fn test_initial_vote() {
    let alice: AccountId = AccountKeyring::Alice.into();
    assert_ok!(Tournament::initial_vote(
        RuntimeOrigin::signed(alice.clone()),
        1,
        20,
        vec![
            VoteSelect {
                battle_id: 1,
                npc_id: 1
            },
            VoteSelect {
                battle_id: 2,
                npc_id: 3
            }
        ]
    ));

    assert_ok!(Tournament::initial_vote(
        RuntimeOrigin::signed(alice.clone()),
        1,
        20,
        vec![
            VoteSelect {
                battle_id: 1,
                npc_id: 1
            },
            VoteSelect {
                battle_id: 2,
                npc_id: 4
            }
        ]
    ));

    assert_ok!(Tournament::initial_vote(
        RuntimeOrigin::signed(alice.clone()),
        1,
        30,
        vec![
            VoteSelect {
                battle_id: 1,
                npc_id: 2
            },
            VoteSelect {
                battle_id: 2,
                npc_id: 3
            }
        ]
    ));

    assert_ok!(Tournament::initial_vote(
        RuntimeOrigin::signed(alice.clone()),
        1,
        20,
        vec![
            VoteSelect {
                battle_id: 1,
                npc_id: 2
            },
            VoteSelect {
                battle_id: 2,
                npc_id: 4
            }
        ]
    ));

    assert_noop!(
        Tournament::initial_vote(
            RuntimeOrigin::signed(alice.clone()),
            1,
            40,
            vec![
                VoteSelect {
                    battle_id: 1,
                    npc_id: 2
                },
                VoteSelect {
                    battle_id: 2,
                    npc_id: 4
                }
            ]
        ),
        Error::<Test>::TicketAmountError
    );

    assert_noop!(
        Tournament::initial_vote(
            RuntimeOrigin::signed(alice.clone()),
            1,
            5,
            vec![
                VoteSelect {
                    battle_id: 5,
                    npc_id: 1
                },
                VoteSelect {
                    battle_id: 1,
                    npc_id: 1
                }
            ]
        ),
        Error::<Test>::BattleNotFound
    );

    assert_noop!(
        Tournament::initial_vote(
            RuntimeOrigin::signed(alice.clone()),
            1,
            5,
            vec![
                VoteSelect {
                    battle_id: 1,
                    npc_id: 1
                },
                VoteSelect {
                    battle_id: 1,
                    npc_id: 2
                }
            ]
        ),
        Error::<Test>::DuplicateBetting
    );

    assert_noop!(
        Tournament::initial_vote(
            RuntimeOrigin::signed(alice.clone()),
            1,
            5,
            vec![
                VoteSelect {
                    battle_id: 1,
                    npc_id: 1
                },
                VoteSelect {
                    battle_id: 1,
                    npc_id: 2,
                }
            ]
        ),
        Error::<Test>::DuplicateBetting
    );

    assert_noop!(
        Tournament::initial_vote(
            RuntimeOrigin::signed(alice.clone()),
            1,
            0,
            vec![
                VoteSelect {
                    battle_id: 5,
                    npc_id: 1
                },
                VoteSelect {
                    battle_id: 1,
                    npc_id: 1
                }
            ]
        ),
        Error::<Test>::TicketAmountError
    );
    assert_noop!(
        Tournament::initial_vote(
            RuntimeOrigin::signed(alice.clone()),
            1,
            5,
            vec![
                VoteSelect {
                    battle_id: 1,
                    npc_id: 1
                },
                VoteSelect {
                    battle_id: 1,
                    npc_id: 1
                }
            ]
        ),
        Error::<Test>::DuplicateBetting
    );
    assert_eq!(Tournament::get_ticket(1, &alice), (99, 9));
    assert_noop!(
        Tournament::initial_vote(
            RuntimeOrigin::signed(alice.clone()),
            0,
            10,
            vec![
                VoteSelect {
                    battle_id: 5,
                    npc_id: 1
                },
                VoteSelect {
                    battle_id: 1,
                    npc_id: 1
                }
            ]
        ),
        Error::<Test>::SeasonNotFound
    );

    assert_noop!(
        Tournament::initial_vote(
            RuntimeOrigin::signed(alice.clone()),
            1,
            5,
            vec![
                VoteSelect {
                    battle_id: 1,
                    npc_id: 2
                },
                VoteSelect {
                    battle_id: 2,
                    npc_id: 2
                }
            ]
        ),
        Error::<Test>::NpcNotInBattle
    );
    assert_eq!(
        Tournament::get_vote_infos(alice.clone(), 1),
        (
            vec![
                VoteSelectInfo {
                    ticket_amount: 20,
                    selects: vec![
                        VoteSelect {
                            battle_id: 1,
                            npc_id: 1
                        },
                        VoteSelect {
                            battle_id: 2,
                            npc_id: 3
                        }
                    ]
                },
                VoteSelectInfo {
                    ticket_amount: 20,
                    selects: vec![
                        VoteSelect {
                            battle_id: 1,
                            npc_id: 1
                        },
                        VoteSelect {
                            battle_id: 2,
                            npc_id: 4,
                        }
                    ]
                },
                VoteSelectInfo {
                    ticket_amount: 30,
                    selects: vec![
                        VoteSelect {
                            battle_id: 1,
                            npc_id: 2
                        },
                        VoteSelect {
                            battle_id: 2,
                            npc_id: 3,
                        }
                    ]
                },
                VoteSelectInfo {
                    ticket_amount: 20,
                    selects: vec![
                        VoteSelect {
                            battle_id: 1,
                            npc_id: 2
                        },
                        VoteSelect {
                            battle_id: 2,
                            npc_id: 4,
                        }
                    ]
                }
            ],
            false
        )
    );
    assert_eq!(Tournament::get_ticket(1, &alice), (99, 9));
}

pub fn test_append_votes() {
    let alice: AccountId = AccountKeyring::Alice.into();
    assert_noop!(
        Tournament::append_vote(
            RuntimeOrigin::signed(alice.clone()),
            1,
            4,
            1,
            10,
            vec![VoteSelect {
                battle_id: 3,
                npc_id: 3,
            }]
        ),
        Error::<Test>::SelectIndexOverflow
    );
    assert_ok!(Tournament::append_vote(
        RuntimeOrigin::signed(alice.clone()),
        1,
        0,
        1,
        10,
        vec![VoteSelect {
            battle_id: 3,
            npc_id: 1,
        }]
    ));
    assert_ok!(Tournament::append_vote(
        RuntimeOrigin::signed(alice.clone()),
        1,
        0,
        1,
        10,
        vec![VoteSelect {
            battle_id: 3,
            npc_id: 3,
        }]
    ));

    assert_ok!(Tournament::append_vote(
        RuntimeOrigin::signed(alice.clone()),
        1,
        1,
        1,
        10,
        vec![VoteSelect {
            battle_id: 3,
            npc_id: 1,
        }]
    ));

    assert_ok!(Tournament::append_vote(
        RuntimeOrigin::signed(alice.clone()),
        1,
        1,
        1,
        10,
        vec![VoteSelect {
            battle_id: 3,
            npc_id: 3,
        }]
    ));
    assert_noop!(
        Tournament::append_vote(
            RuntimeOrigin::signed(alice.clone()),
            1,
            2,
            1,
            10,
            vec![VoteSelect {
                battle_id: 3,
                npc_id: 4,
            }]
        ),
        Error::<Test>::NpcNotInBattle
    );
    assert_ok!(Tournament::append_vote(
        RuntimeOrigin::signed(alice.clone()),
        1,
        2,
        1,
        20,
        vec![VoteSelect {
            battle_id: 3,
            npc_id: 1,
        }]
    ));
    assert_ok!(Tournament::append_vote(
        RuntimeOrigin::signed(alice.clone()),
        1,
        2,
        1,
        10,
        vec![VoteSelect {
            battle_id: 3,
            npc_id: 1,
        }]
    ));

    assert_ok!(Tournament::append_vote(
        RuntimeOrigin::signed(alice.clone()),
        1,
        3,
        1,
        20,
        vec![VoteSelect {
            battle_id: 3,
            npc_id: 3,
        }]
    ));

    assert_noop!(
        Tournament::append_vote(
            RuntimeOrigin::signed(alice.clone()),
            1,
            4,
            1,
            10,
            vec![VoteSelect {
                battle_id: 3,
                npc_id: 3,
            }]
        ),
        Error::<Test>::DuplicateBetting,
    );

    assert_noop!(
        Tournament::append_vote(
            RuntimeOrigin::signed(alice.clone()),
            1,
            2,
            1,
            10,
            vec![VoteSelect {
                battle_id: 3,
                npc_id: 4,
            }]
        ),
        Error::<Test>::DuplicateBetting
    );

    assert_noop!(
        Tournament::append_vote(
            RuntimeOrigin::signed(alice.clone()),
            1,
            2,
            1,
            10,
            vec![
                VoteSelect {
                    battle_id: 3,
                    npc_id: 1,
                },
                VoteSelect {
                    battle_id: 3,
                    npc_id: 1,
                }
            ]
        ),
        Error::<Test>::DuplicateBetting
    );
    assert_noop!(
        Tournament::append_vote(RuntimeOrigin::signed(alice.clone()), 1, 2, 1, 10, vec![]),
        Error::<Test>::VoteSelectZero
    );

    assert_eq!(
        Tournament::get_vote_infos(alice.clone(), 1),
        (
            vec![
                VoteSelectInfo {
                    ticket_amount: 10,
                    selects: vec![
                        VoteSelect {
                            battle_id: 1,
                            npc_id: 1
                        },
                        VoteSelect {
                            battle_id: 2,
                            npc_id: 3
                        },
                        VoteSelect {
                            battle_id: 3,
                            npc_id: 3,
                        }
                    ]
                },
                VoteSelectInfo {
                    ticket_amount: 10,
                    selects: vec![
                        VoteSelect {
                            battle_id: 1,
                            npc_id: 1
                        },
                        VoteSelect {
                            battle_id: 2,
                            npc_id: 4,
                        },
                        VoteSelect {
                            battle_id: 3,
                            npc_id: 3,
                        }
                    ]
                },
                VoteSelectInfo {
                    ticket_amount: 10,
                    selects: vec![
                        VoteSelect {
                            battle_id: 1,
                            npc_id: 2
                        },
                        VoteSelect {
                            battle_id: 2,
                            npc_id: 3,
                        },
                        VoteSelect {
                            battle_id: 3,
                            npc_id: 1,
                        }
                    ]
                },
                VoteSelectInfo {
                    ticket_amount: 20,
                    selects: vec![
                        VoteSelect {
                            battle_id: 1,
                            npc_id: 2
                        },
                        VoteSelect {
                            battle_id: 2,
                            npc_id: 4,
                        },
                        VoteSelect {
                            battle_id: 3,
                            npc_id: 3,
                        }
                    ]
                },
                VoteSelectInfo {
                    ticket_amount: 10,
                    selects: vec![
                        VoteSelect {
                            battle_id: 1,
                            npc_id: 1
                        },
                        VoteSelect {
                            battle_id: 2,
                            npc_id: 3,
                        },
                        VoteSelect {
                            battle_id: 3,
                            npc_id: 1,
                        }
                    ]
                },
                VoteSelectInfo {
                    ticket_amount: 10,
                    selects: vec![
                        VoteSelect {
                            battle_id: 1,
                            npc_id: 1
                        },
                        VoteSelect {
                            battle_id: 2,
                            npc_id: 4,
                        },
                        VoteSelect {
                            battle_id: 3,
                            npc_id: 1,
                        }
                    ]
                },
                VoteSelectInfo {
                    ticket_amount: 20,
                    selects: vec![
                        VoteSelect {
                            battle_id: 1,
                            npc_id: 2
                        },
                        VoteSelect {
                            battle_id: 2,
                            npc_id: 3,
                        },
                        VoteSelect {
                            battle_id: 3,
                            npc_id: 1,
                        }
                    ]
                }
            ],
            false
        )
    );
    assert_eq!(Tournament::get_ticket(1, &alice), (99, 9));
}

pub fn test_buy_ticket() {
    let alice: AccountId = AccountKeyring::Alice.into();
    assert_eq!(
        pallet_fuso_token::Pallet::<Test>::get_token_balance((&1u32, &alice)),
        TokenAccountData {
            free: 9800000000000000000000,
            reserved: Zero::zero(),
        }
    );

    assert_eq!(Tournament::get_ticket(1, &alice), (0, 0));
    assert_ok!(Tournament::buy_ticket(
        RuntimeOrigin::signed(alice.clone()),
        5,
        vec![]
    ));
    assert_eq!(Tournament::get_ticket(1, &alice), (5, 5));

    assert_ok!(Tournament::buy_ticket(
        RuntimeOrigin::signed(alice.clone()),
        2,
        vec![]
    ));
    assert_eq!(Tournament::get_ticket(1, &alice), (7, 7));

    assert_eq!(
        pallet_fuso_token::Pallet::<Test>::get_token_balance((&1u32, &alice)),
        TokenAccountData {
            free: 9100000000000000000000,
            reserved: Zero::zero(),
        }
    );
    assert_noop!(
        Tournament::buy_ticket(RuntimeOrigin::signed(alice.clone()), 0, vec![]),
        Error::<Test>::TicketAmountError
    );

    assert_noop!(
        Tournament::buy_ticket(RuntimeOrigin::signed(alice.clone()), 100, vec![]),
        Error::<Test>::TicketAmountError
    );
    assert_noop!(
        Tournament::buy_ticket(RuntimeOrigin::signed(alice.clone()), 92, vec![]),
        Error::<Test>::InsufficientBalance
    );
    assert_ok!(Tournament::buy_ticket(
        RuntimeOrigin::signed(alice.clone()),
        91,
        "QcH88cpFX2sx8uTAaXpmFE8ab7bBOWwoR03Fz/7Ww7I="
            .to_string()
            .into_bytes(),
    ),);
    assert_eq!(
        pallet_fuso_token::Pallet::<Test>::get_token_balance((
            &1u32,
            &AccountId::from_ss58check("5G76ZGjY3xaR3XCxHTPTEyktJ6kajnKFPiSrEVSqoKvJSwrb").unwrap()
        )),
        TokenAccountData {
            free: 455000000000000000000,
            reserved: Zero::zero(),
        }
    );
    assert_eq!(Tournament::get_ticket(1, &alice), (98, 98));
    let n = alice.to_raw_vec();
    let v = vec![n].encode();
    assert_ok!(Tournament::give_away_tickets(
        RuntimeOrigin::signed(TREASURY),
        1,
        v
    ));
    assert_eq!(Tournament::get_ticket(1, &alice), (99, 99));
}

pub fn test_no_ticket_betting() {
    let alice: AccountId = AccountKeyring::Alice.into();
    assert_noop!(
        Tournament::initial_vote(
            RuntimeOrigin::signed(alice.clone()),
            1u32,
            20u32,
            vec![VoteSelect {
                battle_id: 1,
                npc_id: 1
            }]
        ),
        Error::<Test>::TicketAmountError
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

    assert_ok!(Tournament::create_npc(
        RuntimeOrigin::signed(TREASURY),
        b"npc3".to_vec(),
        b"fsgrethges".to_vec(),
        b"sgseirgeiwrgwerhw".to_vec(),
        b"csghert".to_vec(),
    ));

    assert_ok!(Tournament::create_npc(
        RuntimeOrigin::signed(TREASURY),
        b"npc4".to_vec(),
        b"fsgrethges".to_vec(),
        b"sgseirgeiwrgwerhw".to_vec(),
        b"csghert".to_vec(),
    ));

    assert_ok!(Tournament::create_season(
        RuntimeOrigin::signed(TREASURY),
        b"sdsd".to_vec(),
        "2023-07-25 00:00:00".into(),
        BattleType::SemiFinals,
        100000000000000000000
    ));
    assert_ok!(Tournament::set_default_season(
        RuntimeOrigin::signed(TREASURY),
        1,
    ));

    assert_ok!(Tournament::update_season_current_round(
        RuntimeOrigin::signed(TREASURY),
        1,
        BattleType::Finals
    ));
    assert_eq!(
        Tournament::get_season_info(&1),
        Some(Season {
            id: 1,
            name: b"sdsd".to_vec(),
            status: SeasonStatus::Initial,
            treasury: AccountId::from_ss58check("5DxLSk3X52VhU76aZvPFt6Y6eTS7UW3pRT6y5WEZQrw5NYGp")
                .unwrap(),
            start_time: 1690243200,
            total_battles: 3,
            bonus_strategy: vec![],
            ticket_price: 100000000000000000000,
            first_finals_battle_type: BattleType::SemiFinals,
            current_finals_battle_type: BattleType::Finals,
            champion: None,
            total_tickets: 0u32,
        }),
    );
    assert_ok!(Tournament::update_season_current_round(
        RuntimeOrigin::signed(TREASURY),
        1,
        BattleType::SemiFinals
    ));
    assert_ok!(Tournament::create_battle(
        RuntimeOrigin::signed(TREASURY),
        1,
        BattleType::SemiFinals,
        1,
        2,
        "2023-07-30 00:00:00".into(),
        1
    ));

    assert_ok!(Tournament::create_battle(
        RuntimeOrigin::signed(TREASURY),
        1,
        BattleType::SemiFinals,
        3,
        4,
        "2023-07-30 00:00:00".into(),
        2
    ));

    assert_ok!(Tournament::create_battle(
        RuntimeOrigin::signed(TREASURY),
        1,
        BattleType::Finals,
        1,
        3,
        "2023-07-30 00:00:00".into(),
        1
    ));

    assert_noop!(
        Tournament::create_battle(
            RuntimeOrigin::signed(TREASURY),
            1,
            BattleType::League,
            1,
            1,
            "2023-07-30 00:00:00".into(),
            1,
        ),
        Error::<Test>::BattleNpcCantSame
    );
    assert_noop!(
        Tournament::create_battle(
            RuntimeOrigin::signed(TREASURY),
            4,
            BattleType::League,
            1,
            2,
            "2023-07-30 00:00:00".into(),
            1
        ),
        Error::<Test>::SeasonNotFound
    );

    assert_noop!(
        Tournament::create_battle(
            RuntimeOrigin::signed(TREASURY),
            1,
            BattleType::League,
            5,
            2,
            "2023-07-30 00:00:00".into(),
            1
        ),
        Error::<Test>::NpcNotFound
    );
    let alice: AccountId = AccountKeyring::Alice.into();
    let awt_id = 1u32;
    let awt = XToken::NEP141(
        br#"AWT"#.to_vec(),
        br#"AWT"#.to_vec(),
        Zero::zero(),
        false,
        6,
    );
    assert_ok!(pallet_fuso_token::Pallet::<Test>::issue(
        RuntimeOrigin::signed(alice.clone()),
        awt,
    ));
    let _ = pallet_fuso_token::Pallet::<Test>::do_mint(awt_id, &alice, 9800000000, None);
}

#[test]
pub fn test_decode_invite() {
    use sp_core::crypto::Ss58Codec;
    let a: AccountId =
        AccountId::from_ss58check("5G76ZGjY3xaR3XCxHTPTEyktJ6kajnKFPiSrEVSqoKvJSwrb").unwrap();
    let t = Tournament::addr_to_invite_code(a.clone());
    println!("{}", String::from_utf8(t.clone()).unwrap());
    let addr = Tournament::invite_code_to_addr(t);
    assert_eq!(a, addr.unwrap().into());
}

#[test]
pub fn vec_remove() {
    let mut v = vec![1, 2, 3, 1, 2];
    for i in 0..v.len() {
        if v[i] == 3 {
            v.remove(i);
            break;
        }
    }

    println!("{:?}", v);
}
