#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use mint_sage_ini::living_world::load_files;
use mint_sage_ini::MemorySources;
use mint_sage_livingworld::rules::{Catalog, UnitInfo};
use mint_sage_livingworld::{
    apply, ArmyId, Decision, PlayerId, PlayerKind, PlayerSetup, RegionId, Setup,
};

use super::*;

const DATA: &str = "
LivingWorldPlayerTemplate Elves
 Faction = Elves
 StartingWorldCP = 4
 StartingTacticalCP = 5
 MaxWorldCP = 9
 MaxTacticalCP = 8
 MaxHeroCP = 6
 ScenarioStartResources = 100
End
LivingWorldPlayerTemplate Orcs
 Faction = Orcs
 StartingWorldCP = 4
 StartingTacticalCP = 5
 MaxWorldCP = 9
 MaxTacticalCP = 8
 MaxHeroCP = 6
 ScenarioStartResources = 100
End
LivingWorldBuilding Armoury
 Type = Armory
 BuildingNugget UpgradeTroops Up
  NumUpgradesPerTurn = 2
  UpgradeableUnits = elf
 End
End
LivingWorldRegionCampaign World
 Region A
  CPLimit = 3
  AllyCPLimit = 2
  ConnectsTo = B
  BuildingSpot = X:1 Y:1
 End
 Region B
  CPLimit = 3
  AllyCPLimit = 2
  ConnectsTo = A
 End
End
LivingWorldCampaign Camp
 LivingWorldVictoryType
  PlayerDefeatCondition
   Teams = 1 2
   NumControlledRegionsLessOrEqualTo = 0
  End
 End
 Scenario
  RegionCampaign = World
  MinPlayers = 2
  MaxPlayers = 2
  UseMpRulesVictoryCondition = Yes
 End
End
";

fn session() -> LivingWorldState {
    let mut sources = MemorySources::new();
    sources.insert("data/ini/t.ini", DATA.as_bytes().to_vec());
    let load = load_files(&sources, &["data/ini/t.ini".to_owned()]);
    assert!(load.errors.is_empty(), "{:?}", load.errors);
    let mut catalog = Catalog {
        resource_multiplier_limit: 2.0,
        ..Catalog::default()
    };
    catalog.units.insert(
        "elf".to_owned(),
        UnitInfo {
            command_points: 1,
            build_cost: 10,
            ..UnitInfo::default()
        },
    );
    let seat = |template: &str, kind, team, start: &str| PlayerSetup {
        template: template.to_owned(),
        kind,
        team,
        start_region: Some(start.to_owned()),
        skill: mint_sage_livingworld::Skill::Medium,
    };
    let setup = Setup {
        campaign: "Camp".to_owned(),
        scenario: 0,
        victory_type: 0,
        battle_type: mint_sage_livingworld::screen_queries::BattleType::default(),
        seed: 1,
        players: vec![
            seat("Elves", PlayerKind::Human, 0, "A"),
            seat("Orcs", PlayerKind::Computer, 1, "B"),
        ],
    };
    LivingWorldState::start(&load.data, &catalog, &setup).unwrap()
}

fn event(path: &str, suffix: &str) -> ClipEvent {
    ClipEvent::parse(&format!("{path}{suffix}"), "").unwrap()
}

fn selection_with_battle() -> Selection {
    Selection {
        battle: Some(7),
        ..Selection::default()
    }
}

fn cmds(state: &LivingWorldState, sel: &Selection, ev: &ClipEvent) -> Vec<Command> {
    commands(state, sel, &intent_of(ev).unwrap()).unwrap()
}

#[test]
fn a_click_on_the_details_toggle_opens_or_closes_the_tray_and_posts_no_command() {
    let ev = event("_level0.ToggleSelectionDetailsButton", "_OnClicked");
    assert_eq!(intent_of(&ev), Some(Intent::ToggleDetails));
    let state = session();
    assert_eq!(
        commands(&state, &Selection::default(), &Intent::ToggleDetails),
        Ok(Vec::new())
    );
}

#[test]
fn a_clip_event_splits_at_its_last_on_and_names_its_slot() {
    let ev = ClipEvent::parse("_level0.queue.iconSlot3_OnQueuedIconSlotClicked", "x").unwrap();
    assert_eq!(ev.clip, "_level0.queue.iconSlot3");
    assert_eq!(ev.suffix, "_OnQueuedIconSlotClicked");
    assert_eq!(ev.name(), "iconslot3");
    assert_eq!(ev.index(), Some(3));
    assert_eq!(ClipEvent::parse("nothing here", ""), None);
}

#[test]
fn the_prompt_buttons_post_the_wire_methods_of_message_0x6a7() {
    let state = session();
    let sel = selection_with_battle();
    let method = |suffix: &str| {
        let list = cmds(&state, &sel, &event("_level0.battlePrompt", suffix));
        match list.as_slice() {
            [Command::ChooseBattleMode { battle: 7, method }] => *method,
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(method("_OnAutoResolveButtonClicked"), 1);
    assert_eq!(method("_OnRetreatButtonClicked"), 2);
    assert_eq!(method("_OnRealTimeButtonClicked"), 3);
}

#[test]
fn the_end_turn_button_ends_the_phase_and_an_unknown_clip_means_nothing() {
    let state = session();
    let sel = Selection::default();
    assert_eq!(
        cmds(&state, &sel, &event("_level0.endTurnButton", "_OnClicked")),
        vec![Command::EndPhase]
    );
    assert_eq!(intent_of(&event("_level0.mystery", "_OnClicked")), None);
}

#[test]
fn the_swap_button_exchanges_the_pair_its_argument_names_in_the_pages_slot() {
    let state = session();
    let sel = Selection {
        battle: Some(7),
        page: (1, 2),
        ..Selection::default()
    };
    let mut clicked = event("_level0.main.allies.0.page", "_OnSwapButtonClicked");
    clicked.args = "1".to_owned();
    let list = cmds(&state, &sel, &clicked);
    assert_eq!(
        list,
        vec![Command::SwapArmiesInBattle {
            battle: 7,
            side: 1,
            slot: 2,
            a: 1,
            b: 2
        }]
    );
}

#[test]
fn a_queued_slot_cancels_one_past_its_number_and_the_one_in_progress_cancels_the_head() {
    let state = session();
    let sel = Selection {
        region: Some(RegionId(0)),
        plot: Some(0),
        ..Selection::default()
    };
    let index = |ev: ClipEvent| match cmds(&state, &sel, &ev).as_slice() {
        [Command::CancelQueuedArmy { index, .. }] => *index,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        index(event("q.queuedIconSlot2", "_OnQueuedIconSlotClicked")),
        3
    );
    assert_eq!(
        index(event("q.inProgress", "_OnInProgressIconSlotClicked")),
        0
    );
}

#[test]
fn the_award_confirm_posts_the_player_of_the_clicked_row() {
    let mut state = session();
    let region = RegionId(1);
    state.decisions.insert(
        region,
        Decision {
            region,
            players: vec![PlayerId(1), PlayerId(0)],
            flags: 0,
            resolved: false,
        },
    );
    let mut sel = Selection {
        region: Some(region),
        ..Selection::default()
    };
    assert_eq!(
        commands(&state, &sel, &Intent::AwardRegion),
        Err(Unavailable::AwardRow)
    );
    sel.award_row = Some(1);
    assert_eq!(
        commands(&state, &sel, &Intent::AwardRegion),
        Ok(vec![Command::ChooseSharedCapture {
            region,
            player: PlayerId(0)
        }])
    );
}

fn upgrade_site() -> (LivingWorldState, ArmyId, Vec<u32>) {
    let mut state = session();
    let id = state.rules.building_id("Armoury").unwrap();
    state.regions[0].plots[0] = Some(mint_sage_livingworld::PlotBuilding::new(id, 0, false));
    let army = state.spawn_army(
        PlayerId(0),
        RegionId(0),
        None,
        &[("elf".to_owned(), 1), ("elf".to_owned(), 1)],
    );
    let members = state.armies[&army].roster.iter().map(|m| m.id).collect();
    (state, army, members)
}

#[test]
fn upgrade_links_the_unmarked_members_and_releases_only_when_every_picked_member_is_marked() {
    let (mut state, army, members) = upgrade_site();
    let mut sel = Selection {
        army: Some(army),
        members: members.clone(),
        ..Selection::default()
    };
    let link = commands(&state, &sel, &Intent::UpgradeMembers).unwrap();
    assert_eq!(
        link,
        members
            .iter()
            .map(|&member| Command::LinkMemberForUpgrade { army, member })
            .collect::<Vec<_>>()
    );
    apply(&mut state, PlayerId(0), link[0].clone()).unwrap();
    assert_eq!(
        commands(&state, &sel, &Intent::UpgradeMembers).unwrap(),
        vec![Command::LinkMemberForUpgrade {
            army,
            member: members[1]
        }]
    );
    sel.members = vec![members[0]];
    assert_eq!(
        commands(&state, &sel, &Intent::UpgradeMembers).unwrap(),
        vec![Command::UnlinkMemberForUpgrade {
            army,
            member: members[0]
        }]
    );
}

#[test]
fn a_control_that_needs_a_pick_says_which_pick_is_missing() {
    let state = session();
    let sel = Selection::default();
    assert_eq!(
        commands(&state, &sel, &Intent::CancelArmyMove),
        Err(Unavailable::Army)
    );
    assert_eq!(
        commands(&state, &sel, &Intent::DestroyBuilding),
        Err(Unavailable::Region)
    );
    assert_eq!(
        commands(&state, &sel, &Intent::BattleMode(1)),
        Err(Unavailable::Battle)
    );
}

#[test]
fn an_army_picked_on_the_map_opens_details_and_one_chosen_in_the_tray_does_not() {
    let (a, b) = (ArmyId(3), ArmyId(4));
    let mut sel = Selection::default();
    assert!(sel.pick_army_on_map(Some(a)));
    assert!(sel.details);
    sel.members = vec![9];
    assert!(!sel.pick_army_on_map(Some(a)), "the same army again");
    assert!(sel.details);
    assert_eq!(sel.members, [9], "nothing was rebuilt");

    sel.pick_army_in_tray(Some(a));
    assert!(!sel.details, "the army on the map");
    assert!(sel.members.is_empty());
    assert!(!sel.pick_army_on_map(Some(a)), "still the same army");
    assert!(
        !sel.details,
        "a second pick keeps the army-on-the-map state"
    );

    assert!(sel.pick_army_on_map(Some(b)));
    assert!(sel.details);
    sel.pick_plot(Some(1));
    assert!(
        !sel.details && sel.army.is_none(),
        "a plot replaces the army"
    );
}

#[test]
fn a_unit_icon_click_picks_alone_and_ctrl_toggles() {
    let mut sel = Selection {
        members: vec![1, 2, 3],
        ..Selection::default()
    };
    sel.member_clicked(5, false);
    assert_eq!(sel.members, vec![5]);
    sel.member_clicked(5, false);
    assert!(sel.members.is_empty());
    sel.member_clicked(5, false);
    sel.member_clicked(7, true);
    assert_eq!(sel.members, vec![5, 7]);
    sel.member_clicked(5, true);
    assert_eq!(sel.members, vec![7]);
    sel.background_clicked();
    assert!(sel.members.is_empty());
}

#[test]
fn a_region_click_merges_into_the_players_first_garrison_there_unless_it_is_the_details_army() {
    let mut state = session();
    let elves = [("elf".to_owned(), 1)];
    let first = state.spawn_army(PlayerId(0), RegionId(1), None, &elves);
    let second = state.spawn_army(PlayerId(0), RegionId(1), None, &elves);
    let hero = state.spawn_army(PlayerId(0), RegionId(0), Some("elf"), &elves);
    assert_eq!(garrison_target(&state, hero, RegionId(1)), Some(first));
    assert_eq!(garrison_target(&state, first, RegionId(1)), None);
    assert_eq!(garrison_target(&state, second, RegionId(1)), Some(first));
}
