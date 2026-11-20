use super::*;
use crate::model::{Phase, Rejected};

#[test]
fn a_turn_runs_seven_phases_once_every_player_ends_planning() {
    let mut s = session();
    command(&mut s, P0, Command::EndPhase);
    assert_eq!(s.phase, Phase::Planning);
    let events = command(&mut s, P1, Command::EndPhase);
    assert_eq!(boundaries(&events), 7);
    assert_eq!((s.turn, s.phase), (2, Phase::Planning));
}

#[test]
fn a_command_outside_its_phase_changes_nothing() {
    let mut s = session();
    let before = s.state_hash();
    let region = rid(&s, "A");
    let result = apply(
        &mut s,
        P0,
        Command::ResolveBattle {
            region,
            mode: crate::model::BattleMode::Auto,
        },
    );
    assert_eq!(result, Err(Rejected::WrongPhase));
    assert_eq!(s.state_hash(), before);
}

fn order(s: &mut LivingWorldState, army: ArmyId, to: &str) -> Result<Vec<Event>, Rejected> {
    let dest = rid(s, to);
    let command = Command::MoveArmy { army, dest };
    apply(s, P0, command)
}

#[test]
fn move_range_is_path_cost_one_inside_a_block_two_across_a_border() {
    let mut s = session();
    let hero = hero_army(&mut s, P0, "A", 1);
    let heroless = garrison(&mut s, P0, "A", 1);
    assert!(order(&mut s, hero, "C").is_ok());
    assert_eq!(order(&mut s, heroless, "C"), Err(Rejected::OutOfRange));
    assert!(order(&mut s, heroless, "B").is_ok());
    let at_b = hero_army(&mut s, P0, "B", 1);
    assert!(order(&mut s, at_b, "D").is_ok());
    let d_id = rid(&s, "D");
    s.regions[d_id.index()].owner = None;
    assert_eq!(order(&mut s, at_b, "D"), Err(Rejected::OutOfRange));
    let at_c = hero_army(&mut s, P0, "C", 1);
    assert!(order(&mut s, at_c, "D").is_ok());
    let heroless_c = garrison(&mut s, P0, "C", 1);
    assert_eq!(order(&mut s, heroless_c, "D"), Err(Rejected::NotFriendly));
}

#[test]
fn the_stronger_troop_total_wins_a_head_on_swap() {
    let mut s = session();
    let c = rid(&s, "D");
    s.regions[c.index()].owner = Some(P1);
    let strong = hero_army(&mut s, P0, "C", 3);
    let weak = hero_army(&mut s, P1, "D", 1);
    let (d, c2) = (rid(&s, "D"), rid(&s, "C"));
    command(
        &mut s,
        P0,
        Command::MoveArmy {
            army: strong,
            dest: d,
        },
    );
    apply(
        &mut s,
        P1,
        Command::MoveArmy {
            army: weak,
            dest: c2,
        },
    )
    .unwrap();
    let events = end_planning(&mut s);
    assert!(events.contains(&Event::HeadOnLost { army: weak }));
    assert!(!events.contains(&Event::HeadOnLost { army: strong }));
}

#[test]
fn equal_troops_fall_back_to_the_hero_level() {
    let mut s = session();
    let d = rid(&s, "D");
    s.regions[d.index()].owner = Some(P1);
    let a = hero_army(&mut s, P0, "C", 2);
    let b = hero_army(&mut s, P1, "D", 2);
    s.armies.get_mut(&b).unwrap().roster[0].level = 5;
    let (d2, c3) = (rid(&s, "D"), rid(&s, "C"));
    command(&mut s, P0, Command::MoveArmy { army: a, dest: d2 });
    apply(&mut s, P1, Command::MoveArmy { army: b, dest: c3 }).unwrap();
    let events = end_planning(&mut s);
    assert!(events.contains(&Event::HeadOnLost { army: a }));
}

#[test]
fn disband_refunds_the_price_once_per_member_record_and_refuses_a_hero() {
    let mut s = session();
    let army = hero_army(&mut s, P0, "A", 2);
    let (hero, troop) = {
        let a = &s.armies[&army];
        (a.roster[0].id, a.roster[1].id)
    };
    assert_eq!(
        apply(&mut s, P0, Command::DisbandMember { army, member: hero }),
        Err(Rejected::MemberIsHero)
    );
    command(
        &mut s,
        P0,
        Command::DisbandMember {
            army,
            member: troop,
        },
    );
    assert_eq!(s.player(P0).unwrap().resources, 110);
    assert_eq!(s.armies[&army].roster.len(), 1);
}

#[test]
fn an_army_is_queued_for_destruction_once_and_destroyed_in_order() {
    let mut s = session();
    let a = garrison(&mut s, P0, "A", 1);
    let b = garrison(&mut s, P0, "A", 1);
    s.queue_destroy(b);
    s.queue_destroy(a);
    s.queue_destroy(b);
    assert_eq!(s.destroy_pending, vec![b, a]);
    assert_eq!(s.flush_destroy(), vec![b, a]);
    assert!(s.armies.is_empty());
}
