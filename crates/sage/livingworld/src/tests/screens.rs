use super::*;
use crate::battle::battle_sides;
use crate::engine::apply_with;
use crate::model::{Battle, Phase, PlayerKind, Rejected};

fn upgrade_site() -> (LivingWorldState, ArmyId, Vec<u32>) {
    let mut s = session();
    place_done(&mut s, "A", 1, "Armoury");
    let a = rid(&s, "A");
    let army = s.spawn_army(P0, a, None, &[("elf".to_owned(), 1), ("elf".to_owned(), 1)]);
    let ids = s.armies[&army].roster.iter().map(|m| m.id).collect();
    (s, army, ids)
}

fn link(army: ArmyId, member: u32) -> Command {
    Command::LinkMemberForUpgrade { army, member }
}

fn unlink(army: ArmyId, member: u32) -> Command {
    Command::UnlinkMemberForUpgrade { army, member }
}

fn siege() -> (LivingWorldState, u32) {
    let mut s = session();
    let army = hero_army(&mut s, P0, "D", 2);
    let e = rid(&s, "E");
    command(&mut s, P0, Command::MoveArmy { army, dest: e });
    end_planning(&mut s);
    let id = s.battles[0].id;
    (s, id)
}

#[test]
fn linking_rechecks_quota_template_hero_and_prior_link() {
    let (mut s, army, ids) = upgrade_site();
    let hero = hero_army(&mut s, P0, "A", 1);
    let hero_member = s.armies[&hero].roster[0].id;
    assert_eq!(
        apply(&mut s, P0, link(hero, hero_member)),
        Err(Rejected::MemberIsHero)
    );
    command(&mut s, P0, link(army, ids[0]));
    assert!(s.armies[&army].roster[0].is_upgraded());
    assert_eq!(
        apply(&mut s, P0, link(army, ids[0])),
        Err(Rejected::AlreadyUpgraded)
    );
    assert_eq!(
        apply(&mut s, P0, link(army, ids[1])),
        Err(Rejected::NotUpgradeable)
    );
    command(&mut s, P0, unlink(army, ids[0]));
    assert_eq!(
        apply(&mut s, P0, unlink(army, ids[0])),
        Err(Rejected::NotUpgraded)
    );
    command(&mut s, P0, link(army, ids[1]));
    assert_eq!(
        apply(&mut s, P1, link(army, ids[0])),
        Err(Rejected::NotYourArmy)
    );
}

#[test]
fn a_template_outside_the_upgradeable_list_is_not_linked() {
    let (mut s, army, ids) = upgrade_site();
    let orc = s.rules.intern("orc");
    s.armies.get_mut(&army).unwrap().roster[0].unit = orc;
    assert_eq!(
        apply(&mut s, P0, link(army, ids[0])),
        Err(Rejected::NotUpgradeable)
    );
}

#[test]
fn an_upgrade_building_with_an_empty_list_qualifies_no_unit() {
    let (mut s, army, ids) = upgrade_site();
    let a = rid(&s, "A");
    s.regions[a.index()].plots[1] = None;
    place_done(&mut s, "A", 1, "Hall");
    assert_eq!(
        apply(&mut s, P0, link(army, ids[0])),
        Err(Rejected::NotUpgradeable)
    );
}

#[test]
fn choosing_a_battle_mode_accepts_one_to_three_only() {
    let (mut s, id) = siege();
    for method in [0, 4, 255] {
        let cmd = Command::ChooseBattleMode { battle: id, method };
        assert_eq!(apply(&mut s, P0, cmd), Err(Rejected::InvalidMethod));
    }
    let unknown = Command::ChooseBattleMode {
        battle: id + 9,
        method: 3,
    };
    assert_eq!(apply(&mut s, P0, unknown), Err(Rejected::NoSuchBattle));
}

fn choose(s: &mut LivingWorldState, player: PlayerId, battle: u32, method: u8) {
    let cmd = Command::ChooseBattleMode { battle, method };
    apply_with(s, Some(&mut AttackerLoses), player, cmd).unwrap();
}

#[test]
fn a_battle_mode_waits_only_for_undecided_humans() {
    let (mut s, id) = siege();
    choose(&mut s, P0, id, 3);
    assert_eq!(s.awaiting, Some(id));
}

fn three_humans() -> (LivingWorldState, u32) {
    let (mut s, id) = siege();
    s.players[1].kind = PlayerKind::Human;
    let mut third = s.players[1].clone();
    third.id = PlayerId(2);
    s.players.push(third);
    s.battles[0].players.push(PlayerId(2));
    (s, id)
}

#[test]
fn a_retreat_that_leaves_two_sides_resolves_on_the_choices_already_made() {
    let (mut s, id) = three_humans();
    choose(&mut s, P0, id, 1);
    choose(&mut s, P1, id, 3);
    assert_eq!(s.awaiting, None);
    choose(&mut s, PlayerId(2), id, 2);
    assert_eq!(s.awaiting, Some(id));
}

#[test]
fn a_retreat_that_leaves_no_human_auto_resolves_even_against_a_real_time_vote() {
    let (mut s, id) = three_humans();
    s.players[1].kind = PlayerKind::Computer;
    s.players[2].kind = PlayerKind::Computer;
    s.players[2].team = 7;
    s.battles[0].choices.push((P1, 3));
    choose(&mut s, P0, id, 2);
    assert_eq!(s.awaiting, None);
    assert!(s.battles.is_empty());
}

#[test]
fn a_battle_with_no_human_auto_resolves_on_entering_resolve_battles() {
    for kind in [PlayerKind::Computer, PlayerKind::Inert] {
        let mut s = session();
        s.players[0].kind = kind;
        let army = hero_army(&mut s, P0, "D", 2);
        let e = rid(&s, "E");
        command(&mut s, P0, Command::MoveArmy { army, dest: e });
        let mut resolver = AttackerLoses;
        apply_with(&mut s, Some(&mut resolver), P0, Command::EndPhase).unwrap();
        apply_with(&mut s, Some(&mut resolver), P1, Command::EndPhase).unwrap();
        assert_eq!((s.turn, s.phase), (2, Phase::Planning));
        assert_eq!(s.owner(e), Some(P1));
    }
}

#[test]
fn a_retreat_leaves_the_stayer_the_battle_and_the_region_without_a_fight() {
    let (mut s, id) = siege();
    let e = rid(&s, "E");
    choose(&mut s, P1, id, 2);
    assert_eq!(s.battles.len(), 1);
    choose(&mut s, P0, id, 1);
    assert!(s.battles.is_empty());
    assert!(s.dialog.is_none());
    assert_eq!(s.owner(e), Some(P0));
}

#[test]
fn a_retreat_by_everyone_ends_the_battle_with_no_winner() {
    let (mut s, id) = siege();
    s.players[1].kind = PlayerKind::Human;
    choose(&mut s, P0, id, 2);
    choose(&mut s, P1, id, 2);
    assert!(s.battles.is_empty());
    assert_eq!(s.owner(rid(&s, "E")), Some(P1));
}

#[test]
fn an_auto_resolve_choice_opens_the_dialog_and_its_steps_change_no_strategic_state() {
    let (mut s, id) = siege();
    choose(&mut s, P0, id, 1);
    let before = s.state_hash();
    let step = Command::AutoResolveStep {
        battle: id,
        round: 2,
    };
    command(&mut s, P0, step);
    command(&mut s, P0, Command::AutoResolveSkip { battle: id });
    assert_eq!(s.state_hash(), before);
    let dialog = s.dialog.clone().unwrap();
    assert_eq!(dialog.rounds, vec![(P0, 2)]);
    assert_eq!(dialog.skipped, vec![P0]);
    assert_eq!(
        apply(&mut s, P0, Command::AutoResolveSkip { battle: id + 1 }),
        Err(Rejected::NoDialog)
    );
}

#[test]
fn swapping_armies_in_a_battle_reorders_one_slot_and_bounds_its_indices() {
    let (mut s, id) = siege();
    let e = rid(&s, "E");
    let extra = garrison(&mut s, P0, "E", 1);
    s.regions[e.index()].present.push(extra);
    let battle = s.battles[0].clone();
    let side = u8::try_from(
        battle_sides(&s, &battle)
            .iter()
            .position(|p| p.contains(&P0))
            .unwrap(),
    )
    .unwrap();
    let order = |s: &LivingWorldState| -> Vec<ArmyId> {
        s.regions[e.index()]
            .present
            .iter()
            .copied()
            .filter(|a| s.armies[a].player == P0)
            .collect()
    };
    let before = order(&s);
    let swap = |slot, a, b| Command::SwapArmiesInBattle {
        battle: id,
        side,
        slot,
        a,
        b,
    };
    command(&mut s, P0, swap(0, 0, 1));
    assert_eq!(order(&s), vec![before[1], before[0]]);
    assert_eq!(apply(&mut s, P0, swap(0, 0, 2)), Err(Rejected::InvalidSwap));
    assert_eq!(apply(&mut s, P0, swap(5, 0, 1)), Err(Rejected::InvalidSwap));
}

#[test]
fn forcing_real_time_holds_the_session_on_that_battle() {
    let (mut s, id) = siege();
    let events = command(&mut s, P0, Command::ForceRealTime { battle: id });
    assert_eq!(s.awaiting, Some(id));
    assert!(events.iter().any(|e| matches!(e, Event::BattleSpec(_))));
}

#[test]
fn forcing_real_time_needs_a_pending_battle() {
    let (mut s, id) = siege();
    assert_eq!(
        apply(&mut s, P0, Command::ForceRealTime { battle: id + 1 }),
        Err(Rejected::NoSuchBattle)
    );
}

#[test]
fn surrender_defeats_the_player_and_its_team_and_only_for_oneself() {
    let mut s = session();
    assert_eq!(
        apply(&mut s, P0, Command::Surrender { player: P1 }),
        Err(Rejected::NotYourself)
    );
    s.players[1].team = s.players[0].team;
    let events = command(&mut s, P0, Command::Surrender { player: P0 });
    assert!(s.players[0].defeated && s.players[1].defeated);
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::PlayerDefeated { .. }))
            .count(),
        2
    );
}

#[test]
fn surrender_in_a_single_player_real_time_battle_ends_the_session() {
    let (mut s, id) = siege();
    command(&mut s, P0, Command::ForceRealTime { battle: id });
    command(&mut s, P0, Command::Surrender { player: P0 });
    assert_eq!(s.ended, Some(None));
    assert!(!s.players[0].defeated);
}

#[test]
fn surrender_in_a_two_human_real_time_battle_only_defeats() {
    let (mut s, id) = siege();
    s.players[1].kind = PlayerKind::Human;
    command(&mut s, P0, Command::ForceRealTime { battle: id });
    command(&mut s, P0, Command::Surrender { player: P0 });
    assert!(s.players[0].defeated);
    assert_eq!(s.ended, None);
}

#[test]
fn leaving_the_session_tells_presentation_and_changes_nothing() {
    let mut s = session();
    let before = s.state_hash();
    let events = command(&mut s, P0, Command::LeaveSession);
    assert_eq!(events, vec![Event::PlayerLeft { player: P0 }]);
    assert_eq!(s.state_hash(), before);
}

#[test]
fn a_battle_spec_names_the_regions_owner_as_the_defender() {
    let (s, id) = siege();
    let battle = s.battles.iter().find(|b| b.id == id).unwrap().clone();
    let spec = s.spec_for(&battle);
    assert_eq!(spec.owner, Some(P1));
    assert!(spec.sides.iter().any(|side| side.players.contains(&P1)));
}

#[derive(Default)]
struct Recorder {
    specs: Vec<BattleSpec>,
    shrink: Option<ArmyId>,
}

impl AutoResolve for Recorder {
    fn resolve(&mut self, spec: &BattleSpec, _rng: &mut Pcg32) -> BattleResult {
        self.specs.push(spec.clone());
        let rosters = self
            .shrink
            .and_then(|id| {
                spec.sides
                    .iter()
                    .flat_map(|s| &s.armies)
                    .find(|a| a.id == id)
            })
            .map(|a| {
                let mut roster = a.roster.clone();
                roster.pop();
                (a.id, roster)
            })
            .into_iter()
            .collect();
        BattleResult {
            winner: u8::try_from(spec.sides.len().saturating_sub(1)).ok(),
            rosters,
            ..BattleResult::default()
        }
    }
}

fn choose_with(s: &mut LivingWorldState, r: &mut Recorder, player: PlayerId, battle: u32, m: u8) {
    let cmd = Command::ChooseBattleMode { battle, method: m };
    apply_with(s, Some(r), player, cmd).unwrap();
}

#[test]
fn a_withdrawer_is_pursued_then_loses_its_garrison_and_keeps_its_hero_army_for_retreat() {
    let (mut s, id) = three_humans();
    let e = rid(&s, "E");
    let third = PlayerId(2);
    let garrison = s.spawn_army(third, e, None, &[("orc".to_owned(), 1)]);
    let hero = hero_army(&mut s, third, "E", 2);
    s.region_mut(e).unwrap().present.extend([garrison, hero]);
    let members = s.armies[&hero].roster.len();
    s.battles[0].choices.push((P1, 3));
    let mut r = Recorder {
        shrink: Some(hero),
        ..Recorder::default()
    };
    s.battles[0].choices.push((third, 2));
    choose_with(&mut s, &mut r, P0, id, 3);
    let pursuit = r.specs.first().expect("a pursuit ran");
    assert_eq!(pursuit.weapon_scale, vec![(third, 0.0)]);
    assert_eq!(pursuit.round_cap, Some(s.rules.army_retreat_rounds));
    assert_eq!(s.retreat_entries, vec![hero]);
    assert_eq!(s.armies[&hero].roster.len(), members - 1);
    assert!(!s.armies.contains_key(&garrison));
    assert!(s.armies.contains_key(&hero));
    assert_eq!(s.battles[0].withdrawn, vec![third]);
    assert!(!s.battles[0].players.contains(&third));
}

#[test]
fn a_pursuit_capped_at_zero_rounds_runs_nothing() {
    let (mut s, id) = siege();
    s.rules.army_retreat_rounds = 0;
    let mut r = Recorder::default();
    s.battles[0].choices.push((P1, 2));
    choose_with(&mut s, &mut r, P0, id, 1);
    assert!(r.specs.is_empty());
    assert!(s.battles.is_empty());
}

#[test]
fn the_owner_withdrawing_sets_capture_flag_eight_and_an_allied_stayer_keeps_the_region() {
    let (mut s, id) = siege();
    let e = rid(&s, "E");
    let elsewhere = rid(&s, "A");
    s.battles.push(Battle {
        id: id + 1,
        region: elsewhere,
        players: vec![P0],
        choices: Vec::new(),
        withdrawn: Vec::new(),
        snapshot: Vec::new(),
    });
    let mut r = Recorder::default();
    s.battles[0].choices.push((P1, 2));
    choose_with(&mut s, &mut r, P0, id, 1);
    let capture = s.captures.iter().find(|c| c.region == e).unwrap();
    assert_eq!((capture.player, capture.flags & 0x18), (P0, 0x8));

    let (mut s, id) = siege();
    s.players[0].team = s.players[1].team;
    let mut r = Recorder::default();
    choose_with(&mut s, &mut r, P0, id, 2);
    assert!(s.captures.is_empty());
}

#[test]
fn the_walk_stops_after_a_human_withdrawer() {
    let (mut s, id) = three_humans();
    s.players[1].kind = PlayerKind::Computer;
    s.players[2].kind = PlayerKind::Computer;
    s.battles[0].choices.push((P1, 2));
    s.battles[0].choices.push((PlayerId(2), 2));
    let mut r = Recorder::default();
    choose_with(&mut s, &mut r, P0, id, 2);
    assert_eq!(r.specs.len(), 1);
    assert_eq!(r.specs[0].weapon_scale, vec![(P0, 0.0)]);
}

#[test]
fn withdrawers_leave_in_session_player_order() {
    let (mut s, id) = three_humans();
    s.players[1].kind = PlayerKind::Computer;
    s.players[2].kind = PlayerKind::Computer;
    s.battles[0].choices.push((PlayerId(2), 2));
    s.battles[0].choices.push((P1, 2));
    let mut r = Recorder::default();
    choose_with(&mut s, &mut r, P0, id, 1);
    assert!(s.battles.is_empty());
    assert_eq!(r.specs.len(), 2);
    assert_eq!(r.specs[0].weapon_scale, vec![(P1, 0.0)]);
    assert_eq!(r.specs[1].weapon_scale, vec![(PlayerId(2), 0.0)]);
}
