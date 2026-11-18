use super::group_tests::melee_pair_of;
use super::*;
use mint_sage_formation::{
    FormationTable, HordeContainDef, HordeSettings, LeaderRef, RankLine, SlotPosition,
};
use mint_sage_ini::{MeleeBehaviorDef, MeleeBehaviorKind};

fn leader_and_follower_table() -> FormationTable {
    let def = HordeContainDef {
        slots: Some(2),
        random_offset: (0.0, 0.0),
        ranks: vec![
            RankLine {
                rank: 1,
                unit_type: "Spearman".to_owned(),
                positions: vec![SlotPosition {
                    x: 20.0,
                    y: 0.0,
                    leader: None,
                }],
            },
            RankLine {
                rank: 2,
                unit_type: "Spearman".to_owned(),
                positions: vec![SlotPosition {
                    x: -300.0,
                    y: 0.0,
                    leader: Some(LeaderRef { rank: 1, index: 0 }),
                }],
            },
        ],
        ..HordeContainDef::default()
    };
    let settings = HordeSettings {
        melee_behavior: Some(MeleeBehaviorDef {
            kind: MeleeBehaviorKind::WaitForLeader,
            amoeba: None,
        }),
        ..FormationTable::from_contain(&def).settings().clone()
    };
    FormationTable::from_contain(&def).with_settings(settings)
}

#[test]
fn a_follower_walks_its_leaders_breadcrumbs() {
    let (mut simulation, ids, sides) = melee_pair_of(
        AvoidanceMode::Off,
        [10_000.0; 2],
        [true, false],
        leader_and_follower_table(),
    );
    let [leader, follower] = sides[0][..] else {
        panic!("one leader, one follower");
    };
    let follower_spawn = simulation
        .state()
        .get(ids.body, follower)
        .map(|b| b.0.position)
        .expect("the follower spawned with a body");
    let mut checked = 0u32;
    for _ in 0..600 {
        simulation.tick().unwrap();
        let Some(trail) = simulation.state().get(ids.leader_trail, leader) else {
            continue;
        };
        if trail.0.is_empty() {
            continue;
        }
        let Some(record) = simulation.state().get(ids.member_melee, follower) else {
            continue;
        };
        assert!(
            trail
                .0
                .iter()
                .any(|crumb| crumb.minus(record.goal).length() < 0.5),
            "follower goal {:?} is not one of the leader's breadcrumbs {trail:?}",
            record.goal
        );
        assert!(
            record.goal.minus(follower_spawn).length() > 1.0,
            "follower goal {:?} is still its own spawn point {follower_spawn:?}",
            record.goal
        );
        checked += 1;
    }
    assert!(
        checked > 0,
        "never sampled the follower with a non-empty leader trail"
    );
}
