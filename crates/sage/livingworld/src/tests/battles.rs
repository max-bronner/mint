use super::*;
use crate::harness::{play, replay};
use crate::model::{BattleMode, Phase, Rejected};

fn siege() -> (LivingWorldState, ArmyId) {
    let mut s = session();
    let army = hero_army(&mut s, P0, "D", 2);
    let e = rid(&s, "E");
    command(&mut s, P0, Command::MoveArmy { army, dest: e });
    end_planning(&mut s);
    (s, army)
}

#[test]
fn an_unopposed_move_into_an_unowned_region_captures_it_after_the_battles() {
    let mut s = session();
    let d = rid(&s, "D");
    s.regions[d.index()].owner = None;
    let army = hero_army(&mut s, P0, "C", 1);
    command(&mut s, P0, Command::MoveArmy { army, dest: d });
    let events = end_planning(&mut s);
    assert!(events.contains(&Event::RegionCaptured {
        region: d,
        player: P0
    }));
    assert_eq!(s.owner(d), Some(P0));
}

#[test]
fn a_defended_region_holds_the_phase_until_its_battle_is_resolved() {
    let (mut s, army) = siege();
    assert_eq!(s.phase, Phase::ResolveBattles);
    assert_eq!(s.battles.len(), 1);
    let region = rid(&s, "E");
    let direct = apply(
        &mut s,
        P0,
        Command::ResolveBattle {
            region,
            mode: BattleMode::Auto,
        },
    );
    assert_eq!(direct, Err(Rejected::NoResolver));
    let mut resolver = AttackerLoses;
    crate::apply_with(
        &mut s,
        Some(&mut resolver),
        P0,
        Command::ResolveBattle {
            region,
            mode: BattleMode::Auto,
        },
    )
    .unwrap();
    assert_eq!((s.turn, s.phase), (2, Phase::Planning));
    assert_eq!(s.armies[&army].region, rid(&s, "D"));
    assert_eq!(s.owner(region), Some(P1));
}

#[test]
fn a_real_time_battle_blocks_everything_but_its_result() {
    let (mut s, _) = siege();
    let region = rid(&s, "E");
    let events = command(
        &mut s,
        P0,
        Command::ResolveBattle {
            region,
            mode: BattleMode::RealTime,
        },
    );
    assert!(matches!(events.as_slice(), [Event::BattleSpec(_)]));
    assert_eq!(
        apply(&mut s, P1, Command::EndPhase),
        Err(Rejected::AwaitingBattle)
    );
    command(
        &mut s,
        P0,
        Command::ApplyBattleResult {
            region,
            result: crate::model::BattleResult {
                winner: Some(0),
                rosters: Vec::new(),
                fallen_heroes: Vec::new(),
                fallen_records: Vec::new(),
                players: Vec::new(),
            },
        },
    );
    assert_eq!(s.owner(region), Some(P0));
    assert_eq!(s.ended, Some(Some(0)));
}

#[test]
fn a_retreat_with_two_options_waits_for_the_choice() {
    let mut s = session();
    let army = hero_army(&mut s, P0, "C", 1);
    s.phase = Phase::PlanRetreats;
    s.retreat_entries.push(army);
    s.retreat_triage(&mut Vec::new());
    assert_eq!(s.retreat_entries, vec![army]);
    let far = rid(&s, "E");
    assert_eq!(
        apply(&mut s, P0, Command::ChooseRetreat { army, dest: far }),
        Err(Rejected::NotFriendly)
    );
    let d = rid(&s, "D");
    command(&mut s, P0, Command::ChooseRetreat { army, dest: d });
    assert_eq!((s.turn, s.phase), (2, Phase::Planning));
    assert_eq!(s.armies[&army].region, d);
}

#[test]
fn a_hero_army_with_nowhere_to_go_is_destroyed() {
    let mut s = session();
    let army = hero_army(&mut s, P1, "E", 1);
    let d = rid(&s, "D");
    s.regions[d.index()].owner = None;
    s.armies.get_mut(&army).unwrap().prev_region = Some(rid(&s, "E"));
    s.retreat_entries.push(army);
    s.retreat_triage(&mut Vec::new());
    assert_eq!(s.flush_destroy(), vec![army]);
}

#[test]
fn a_winning_team_is_decided_before_any_defeat() {
    let victory = "
 LivingWorldVictoryType
  TeamVictoryCondition
   Teams = 2
   NumControlledRegionsGreaterOrEqualTo = 1
  End
  TeamDefeatCondition
   Teams = 1 2
   NumControlledRegionsLessOrEqualTo = 9
  End
 End
";
    let mut s = session_with(victory);
    for r in &mut s.regions {
        r.owner = None;
    }
    s.regions[0].owner = Some(P0);
    s.regions[4].owner = Some(P1);
    s.check_victory(&mut Vec::new());
    assert_eq!(s.ended, Some(Some(1)));
    assert!(s.player(P0).unwrap().defeated);
    assert!(!s.player(P1).unwrap().defeated);
}

#[test]
fn a_defeated_players_region_goes_to_the_strongest_other_army_there() {
    let mut s = session();
    let e = rid(&s, "E");
    let own = hero_army(&mut s, P1, "E", 1);
    let heir = hero_army(&mut s, P0, "E", 1);
    s.register_armies();
    s.defeat_player(P1, &mut Vec::new());
    assert_eq!(s.owner(e), Some(P0));
    assert!(!s.armies.contains_key(&own));
    assert!(s.armies.contains_key(&heir));
}

#[test]
fn losing_the_capital_defeats_a_player_under_that_condition() {
    let victory = "
 LivingWorldVictoryType
  PlayerDefeatCondition
   Teams = 1 2
   LoseIfCapitalLost = Yes
  End
 End
";
    let mut s = session_with(victory);
    let a = rid(&s, "A");
    s.regions[a.index()].owner = Some(P1);
    s.check_victory(&mut Vec::new());
    assert!(s.player(P0).unwrap().defeated);
    assert_eq!(s.ended, Some(Some(1)));
}

#[test]
fn a_command_log_replays_to_the_same_hash_at_every_boundary() {
    let (data, setup) = setup("", 7);
    let fresh = || LivingWorldState::start(&data, &catalog(), &setup).unwrap();
    let mut first = fresh();
    let mut log = Vec::new();
    let played = play(&mut first, &mut AttackerLoses, 12, &mut log);
    assert!(played.len() >= 70, "{}", played.len());
    let mut again = fresh();
    assert_eq!(replay(&mut again, &mut AttackerLoses, &log), played);
    assert_eq!(again.state_hash(), first.state_hash());
}
