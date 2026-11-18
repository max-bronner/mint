#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use mint_sage_livingworld::model::bonus;
use mint_sage_livingworld::{Bonus, Phase, PlayerId};

use super::award::{bonus_line, select};
use super::cache::StateCache;
use super::command_ui::{frames_for, Army, Button, Frames, Plot};
use super::config::{faction_icon, Banners};
use super::end_turn::{enabled, state_label, Action, EndTurn, CLIP};
use super::panels::{
    armies, building_image, retinue_layout, slot_state, structures, visibility, ArmySlot, Bind,
    PlotSlot, Text,
};
use super::panels::{tray_after, Tray, TrayView};
use super::prompt::{button_states, pages_of, swap_button, Side, TabCall, Tabs};
use crate::wotr_movies::Kind;
use mint_sage_apt_player::Value;
use mint_sage_ini::living_world::players::BuildingType;
use mint_sage_ini::living_world::{load_files, LivingWorldData};
use mint_sage_ini::MemorySources;
use mint_sage_livingworld::BuildingId;

fn p(n: u8) -> PlayerId {
    PlayerId(n)
}

fn allied(a: PlayerId, b: PlayerId) -> bool {
    a.0 <= 1 && b.0 <= 1
}

#[test]
fn the_local_player_is_ally_page_zero_then_the_others_in_team_and_slot_order_counted_per_side() {
    let teams = vec![vec![p(2), p(3)], vec![p(1), p(0)], vec![p(4)]];
    let pages = pages_of(&teams, p(0), allied);
    let shape: Vec<(Side, u32, u8, &str)> = pages
        .iter()
        .map(|g| (g.side, g.id, g.player.0, g.state()))
        .collect();
    assert_eq!(
        shape,
        vec![
            (Side::Ally, 0, 0, "_player"),
            (Side::Enemy, 0, 2, "_enemy"),
            (Side::Enemy, 1, 3, "_enemy"),
            (Side::Ally, 1, 1, "_ally"),
            (Side::Enemy, 2, 4, "_enemy"),
        ]
    );
    assert_eq!(pages[1].path(), "main.enemies.0.page");
}

#[test]
fn swap_button_n_is_shown_and_up_when_there_are_n_plus_two_heroes_else_disabled() {
    let state = |heroes| [0, 1, 2].map(|n| swap_button(n, heroes));
    assert_eq!(
        state(1),
        [
            (false, "_disabled"),
            (false, "_disabled"),
            (false, "_disabled")
        ]
    );
    assert_eq!(
        state(2),
        [(true, "_up"), (false, "_disabled"), (false, "_disabled")]
    );
    assert_eq!(state(4), [(true, "_up"), (true, "_up"), (true, "_up")]);
}

#[test]
fn selecting_a_tab_deselects_the_old_one_and_names_its_player() {
    let names = || vec!["Local".to_owned(), "Ally".to_owned()];
    let mut ally = Tabs::new(Side::Ally, names());
    assert_eq!(
        ally.select(0),
        vec![
            TabCall::State(0, "_selected"),
            TabCall::PlayerName("Local".to_owned()),
            TabCall::NameVisibility(false),
            TabCall::LocalPlayerName("Local".to_owned()),
        ],
        "tab 0 of the allies is the local player"
    );
    assert_eq!(
        ally.select(1),
        vec![
            TabCall::State(0, "_deselect"),
            TabCall::State(1, "_selected"),
            TabCall::PlayerName("Ally".to_owned()),
            TabCall::NameVisibility(true),
        ]
    );
    let mut enemy = Tabs::new(Side::Enemy, names());
    assert!(!enemy
        .select(0)
        .iter()
        .any(|c| matches!(c, TabCall::LocalPlayerName(_))));
    assert!(enemy.select(7).is_empty(), "no such tab");
}

#[test]
fn the_award_selects_a_row_after_deselecting_the_old_one() {
    assert_eq!(select(None, 0), vec![(0, "_selected")]);
    assert_eq!(select(Some(0), 2), vec![(0, "_deselect"), (2, "_selected")]);
    assert_eq!(select(Some(2), 2), vec![(2, "_selected")]);
}

#[test]
fn the_award_bonus_lines_read_their_fields_and_row_five_re_reads_the_attack_bonus() {
    let mut b = Bonus::default();
    for (field, value) in [
        (bonus::ARMY, 11),
        (bonus::RESOURCE, 22),
        (bonus::LEGENDARY, 33),
        (bonus::ATTACK, 44),
        (bonus::DEFENSE, 55),
        (bonus::EXPERIENCE, 66),
    ] {
        b.set(field, value);
    }
    let values: Vec<i32> = (0..6).map(|i| bonus_line(&b, i).unwrap().1).collect();
    assert_eq!(values, vec![11, 22, 33, 44, 55, 44]);
    assert_eq!(
        bonus_line(&b, 0).unwrap().0,
        "STRATEGICHUD:RegionAwardCommandPoints"
    );
    assert!(bonus_line(&b, 6).is_none());
}

#[test]
fn icon_slots_are_empty_up_or_selected_and_every_other_state_is_show_or_hide() {
    assert_eq!(slot_state(false, false), "_empty");
    assert_eq!(slot_state(false, true), "_empty");
    assert_eq!(slot_state(true, false), "_up");
    assert_eq!(slot_state(true, true), "_selected");
    assert_eq!((visibility(true), visibility(false)), ("_show", "_hide"));
    let binds = armies(
        &[ArmySlot {
            name: "Theoden".to_owned(),
            hero: true,
            command_points: 5,
        }],
        Some(0),
    );
    let state = binds.iter().find_map(|b| match b {
        Bind::Call {
            function: "SetIconSlotState",
            args,
            ..
        } => Some(args.clone()),
        _ => None,
    });
    assert_eq!(
        state,
        Some(vec![Value::Number(0.0), Value::str("_selected")])
    );
    let name_state = binds.iter().find_map(|b| match b {
        Bind::Call {
            function: "SetArmyNameState",
            args,
            ..
        } => Some(args.clone()),
        _ => None,
    });
    assert_eq!(name_state, Some(vec![Value::str("_show")]));
}

#[test]
fn the_armies_page_publishes_its_name_points_and_a_portrait_and_type_image_per_slot() {
    let slot = |hero| ArmySlot {
        name: "Army".to_owned(),
        hero,
        command_points: 7,
    };
    let binds = armies(&[slot(true), slot(false)], Some(1));
    let texts: Vec<(&str, &Text)> = binds
        .iter()
        .filter_map(|b| match b {
            Bind::Text { name, text } => Some((name.as_str(), text)),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts,
        vec![
            ("ArmyName", &Text::Plain("Army".to_owned())),
            ("CommandPoints", &Text::Plain("7".to_owned()))
        ]
    );
    let pictures: Vec<(&str, Option<&str>)> = binds
        .iter()
        .filter_map(|b| match b {
            Bind::Picture { name, element } => Some((name.as_str(), element.as_deref())),
            _ => None,
        })
        .collect();
    assert_eq!(
        pictures,
        vec![
            ("IconSlotPortrait0", None),
            ("IconSlotTypeImage0", Some("UnitHero")),
            ("IconSlotPortrait1", None),
            ("IconSlotTypeImage1", Some("UnitFortress")),
        ]
    );
}

#[test]
fn the_structures_page_counts_turns_remaining_only_for_a_building_under_construction() {
    let plot = |under_construction, turns_left| PlotSlot {
        building: Some("Barracks".to_owned()),
        kind: Some(BuildingType::Barracks),
        turns_left,
        under_construction,
    };
    let binds = structures(&[plot(true, 1), plot(true, 3), plot(false, 0)], None);
    let turns: Vec<&Text> = binds
        .iter()
        .filter_map(|b| match b {
            Bind::Text { name, text } if name.starts_with("IconSlotTurns") => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(
        turns,
        vec![
            &Text::Formatted("STRATEGICHUD:ConstructionOneTurnRemaining", 1),
            &Text::Formatted("STRATEGICHUD:ConstructionTurnsRemaining", 3),
            &Text::Plain(String::new()),
        ]
    );
    assert_eq!(
        building_image(Some(BuildingType::Armory)),
        Some("BuildingArmory")
    );
    assert_eq!(building_image(Some(BuildingType::None)), None);
}

#[test]
fn a_command_frame_reports_its_index_and_path_and_a_button_in_it_finds_its_frame() {
    let mut frames = Frames::default();
    assert_eq!(
        frames.loaded("index=2&name=_level0.main.globe.commandUI.2"),
        Some(2)
    );
    assert_eq!(frames.loaded("index=x&name=a"), None);
    assert_eq!(frames.path(2).as_deref(), Some("main.globe.commandUI.2"));
    assert_eq!(
        frames.index_of("_level0.main.globe.commandUI.2.button"),
        Some(2)
    );
    assert_eq!(
        frames.index_of("_level0.main.globe.commandUI.3.button"),
        None
    );
}

#[test]
fn a_state_setter_calls_the_movie_only_when_the_state_changes() {
    let mut cache = StateCache::default();
    cache.seed(Kind::Hud, CLIP, "SetState", "_up");
    assert!(
        !cache.changed(Kind::Hud, CLIP, "SetState", "_up"),
        "seeded wrapper"
    );
    assert!(cache.changed(Kind::Hud, CLIP, "SetState", "_disabled"));
    assert!(!cache.changed(Kind::Hud, CLIP, "SetState", "_disabled"));
    assert!(cache.changed(Kind::Hud, CLIP, "SetState", "_up"));
    assert!(
        cache.changed(Kind::Prompt, "main.AutoResolve", "SetState", "_up"),
        "another wrapper keeps its own state"
    );
}

#[test]
fn a_call_that_never_landed_and_a_movie_loaded_again_are_sent_afresh() {
    let mut cache = StateCache::default();
    assert!(cache.changed(Kind::Hud, CLIP, "SetState", "_disabled"));
    cache.forget(Kind::Hud, CLIP, "SetState");
    assert!(cache.changed(Kind::Hud, CLIP, "SetState", "_disabled"));
    cache.clear(Kind::Hud);
    assert!(cache.changed(Kind::Hud, CLIP, "SetState", "_disabled"));
}

#[test]
fn the_end_turn_button_is_up_in_three_phases_and_disabled_in_the_rest() {
    let up: Vec<Phase> = [
        Phase::Planning,
        Phase::MoveArmies,
        Phase::ResolveBattles,
        Phase::FakePhaseOne,
        Phase::PlanRetreats,
        Phase::RetreatArmies,
        Phase::Complete,
    ]
    .into_iter()
    .filter(|p| enabled(*p))
    .collect();
    assert_eq!(
        up,
        vec![Phase::Planning, Phase::ResolveBattles, Phase::PlanRetreats]
    );
    assert_eq!(
        (state_label(true), state_label(false)),
        ("_up", "_disabled")
    );
}

#[test]
fn leaving_an_enabled_phase_disables_the_button_and_entering_one_enables_it() {
    let mut end = EndTurn::default();
    assert_eq!(
        end.update(Phase::Planning, false, false),
        vec![Action::Enable(true)]
    );
    assert!(end.update(Phase::Planning, false, false).is_empty());
    assert_eq!(
        end.update(Phase::MoveArmies, true, false),
        vec![Action::Enable(false)]
    );
    assert_eq!(
        end.update(Phase::ResolveBattles, false, false),
        vec![Action::Enable(true)]
    );
    assert_eq!(
        end.update(Phase::FakePhaseOne, true, false),
        vec![Action::Enable(false)]
    );
}

#[test]
fn the_alert_flash_plays_once_when_the_phase_can_end_and_halts_when_the_phase_ends() {
    let mut end = EndTurn::default();
    end.update(Phase::ResolveBattles, false, false);
    assert!(end.update(Phase::ResolveBattles, false, false).is_empty());
    assert_eq!(
        end.update(Phase::ResolveBattles, true, false),
        vec![Action::PlayAlertFlash]
    );
    assert!(
        end.update(Phase::ResolveBattles, true, false).is_empty(),
        "once per phase"
    );
    assert_eq!(
        end.update(Phase::FakePhaseOne, true, false),
        vec![Action::HaltAlertFlash, Action::Enable(false)]
    );
    assert_eq!(
        end.update(Phase::PlanRetreats, true, false),
        vec![Action::Enable(true), Action::PlayAlertFlash],
        "a new phase flashes again"
    );
}

#[test]
fn planning_never_flashes_and_resolve_battles_waits_for_the_dialog_to_close() {
    let mut end = EndTurn::default();
    assert_eq!(
        end.update(Phase::Planning, true, false),
        vec![Action::Enable(true)]
    );
    end.update(Phase::MoveArmies, true, false);
    assert_eq!(
        end.update(Phase::ResolveBattles, true, true),
        vec![Action::Enable(true)],
        "a dialog is open"
    );
    assert_eq!(
        end.update(Phase::ResolveBattles, true, false),
        vec![Action::PlayAlertFlash]
    );
}

fn living_world(text: &str) -> LivingWorldData {
    let mut sources = MemorySources::new();
    sources.insert("data/ini/t.ini", text.as_bytes().to_vec());
    let load = load_files(&sources, &["data/ini/t.ini".to_owned()]);
    assert!(load.errors.is_empty(), "{:?}", load.errors);
    load.data
}

#[test]
fn the_choice_buttons_are_up_for_the_bits_of_the_mask_and_disabled_otherwise() {
    assert_eq!(
        button_states(14),
        [
            ("AutoResolve", "_up"),
            ("RealTime", "_up"),
            ("Retreat", "_up")
        ]
    );
    assert_eq!(
        button_states(10),
        [
            ("AutoResolve", "_up"),
            ("RealTime", "_up"),
            ("Retreat", "_disabled")
        ]
    );
    assert_eq!(
        button_states(4),
        [
            ("AutoResolve", "_disabled"),
            ("RealTime", "_disabled"),
            ("Retreat", "_up")
        ]
    );
    assert_eq!(button_states(8)[1], ("RealTime", "_up"));
}

#[test]
fn a_banner_is_the_icon_of_the_type_the_heros_spawn_army_names_else_banner_men() {
    let data = living_world(
        "BannerType BannerMen\n Icon = Banner_Men\nEnd\n\
         BannerType BannerElves\n Icon = Banner_Elf\nEnd\n\
         LivingWorldCampaign C\n Act One\n  SpawnArmy\n   HeroTemplateName = Thranduil\n   Banner = BannerElves\n  End\n  \
         SpawnArmy\n   HeroTemplateName = Stray\n   Banner = BannerGone\n  End\n End\nEnd\n",
    );
    let banners = Banners::read(&data);
    assert_eq!(banners.icon(Some("thranduil")), Some("Banner_Elf"));
    assert_eq!(banners.icon(None), Some("Banner_Men"), "a garrison");
    assert_eq!(banners.icon(Some("Nobody")), Some("Banner_Men"));
    assert_eq!(
        banners.icon(Some("Stray")),
        Some("Banner_Men"),
        "an unknown banner type falls back"
    );
    let none = Banners::read(&living_world("LivingWorldCampaign C\nEnd\n"));
    assert_eq!(none.icon(None), None, "no BannerMen: no picture");
}

#[test]
fn the_faction_icon_is_the_living_world_player_templates() {
    let data = living_world(
        "LivingWorldPlayerTemplate PlayerMen\n Faction = FactionMen\n FactionIcon = AptIconMen\nEnd\n\
         LivingWorldPlayerTemplate PlayerNone\n Faction = FactionNone\nEnd\n",
    );
    assert_eq!(
        faction_icon(&data, "playermen").as_deref(),
        Some("AptIconMen")
    );
    assert_eq!(faction_icon(&data, "PlayerNone"), None);
    assert_eq!(faction_icon(&data, "PlayerGone"), None);
}

fn frame_names(list: &[super::command_ui::Want]) -> Vec<(usize, String, bool)> {
    list.iter()
        .map(|w| {
            let name = match w.button {
                Button::Element(e) => e.to_owned(),
                Button::Build(id) => format!("build {}", id.0),
            };
            (w.frame, name, w.enabled)
        })
        .collect()
}

fn named(frame: usize, name: &str, enabled: bool) -> (usize, String, bool) {
    (frame, name.to_owned(), enabled)
}

#[test]
fn an_army_on_the_map_holds_cancel_move_in_frame_one_and_the_toggle_sits_in_frame_zero() {
    let army = |has_order| Army {
        has_order,
        in_details: false,
        members_picked: false,
    };
    assert_eq!(
        frame_names(&frames_for(Some(army(true)), None, true)),
        vec![
            named(0, "ToggleSelectionDetailsButton", true),
            named(1, "CancelArmyMoveButton", true)
        ]
    );
    assert_eq!(
        frame_names(&frames_for(Some(army(false)), None, true))[1],
        named(1, "CancelArmyMoveButton", false)
    );
    assert_eq!(
        frame_names(&frames_for(Some(army(true)), None, false)),
        vec![named(0, "ToggleSelectionDetailsButton", true)],
        "outside Planning only the toggle"
    );
}

#[test]
fn army_details_hold_member_cancel_upgrade_and_disband_in_frames_one_two_and_five() {
    let details = Army {
        has_order: false,
        in_details: true,
        members_picked: true,
    };
    assert_eq!(
        frame_names(&frames_for(Some(details), None, true)),
        vec![
            named(0, "ToggleSelectionDetailsButton", true),
            named(1, "CancelArmyMemberMoveButton", false),
            named(2, "UpgradeUnitButton", true),
            named(5, "DisbandArmyMemberButton", true),
        ],
        "frames 3 and 4 stay empty"
    );
}

#[test]
fn the_army_state_not_the_members_decides_between_details_and_the_army_on_the_map() {
    let army = |in_details, members_picked| Army {
        has_order: true,
        in_details,
        members_picked,
    };
    assert_eq!(
        frame_names(&frames_for(Some(army(true, false)), None, true)),
        vec![
            named(0, "ToggleSelectionDetailsButton", true),
            named(1, "CancelArmyMemberMoveButton", true),
            named(2, "UpgradeUnitButton", false),
            named(5, "DisbandArmyMemberButton", false),
        ],
        "details with no member picked"
    );
    assert_eq!(
        frame_names(&frames_for(Some(army(false, true)), None, true)),
        vec![
            named(0, "ToggleSelectionDetailsButton", true),
            named(1, "CancelArmyMoveButton", true),
        ],
        "members picked on an army that is only on the map"
    );
}

#[test]
fn a_tutorial_keeps_the_tray_shut_and_flashes_frame_zero_5_s_the_first_time_it_is_allowed() {
    use super::panels::{LockStep, TrayLock};
    let mut lock = TrayLock::default();
    assert_eq!(lock.update(None, true), vec![], "no tutorial: nothing sent");
    assert_eq!(
        lock.update(Some(false), false),
        vec![LockStep::ToggleButton("_disabled")]
    );
    assert_eq!(
        lock.update(Some(false), true),
        vec![LockStep::Close, LockStep::ToggleButton("_disabled")],
        "an open tray is closed"
    );
    assert_eq!(
        lock.update(Some(true), false),
        vec![
            LockStep::ToggleButton("_enabled"),
            LockStep::Flash {
                frame: 0,
                seconds: 5.0
            }
        ]
    );
    assert_eq!(
        lock.update(Some(true), true),
        vec![LockStep::ToggleButton("_enabled")],
        "the flash is once"
    );
}

#[test]
fn the_retinue_layout_is_the_hero_variant_for_a_hero_army_and_the_garrison_one_otherwise() {
    let layout = |hero| match retinue_layout(hero).as_slice() {
        [Bind::Call { function, args, .. }] => (*function, args.clone()),
        other => panic!("{other:?}"),
    };
    assert_eq!(layout(true), ("SetLayout", vec![Value::str("_hero")]));
    assert_eq!(layout(false), ("SetLayout", vec![Value::str("_garrison")]));
}

#[test]
fn a_built_plot_holds_destroy_or_cancel_construction_in_frame_five_and_beats_the_army() {
    let army = Army {
        has_order: true,
        in_details: false,
        members_picked: false,
    };
    let built = |under_construction| Plot::Built { under_construction };
    assert_eq!(
        frame_names(&frames_for(Some(army), Some(built(false)), true)),
        vec![
            named(0, "ToggleSelectionDetailsButton", true),
            named(5, "DestroyBuildingButton", true)
        ]
    );
    assert_eq!(
        frame_names(&frames_for(None, Some(built(true)), true))[1],
        named(5, "CancelBuildingConstructionButton", true)
    );
}

#[test]
fn an_empty_plot_fills_frames_from_one_with_its_offers_at_most_five() {
    let offers = |n: u16| Plot::Empty {
        offers: (0..n).map(|i| (BuildingId(i), i != 1)).collect(),
    };
    assert_eq!(
        frame_names(&frames_for(None, Some(offers(3)), true)),
        vec![
            named(0, "ToggleSelectionDetailsButton", true),
            named(1, "build 0", true),
            named(2, "build 1", false),
            named(3, "build 2", true),
        ],
        "an offer takes the next frame; a filtered one was never in the list"
    );
    let seven = frames_for(None, Some(offers(7)), true);
    let frames: Vec<usize> = seven.iter().map(|w| w.frame).collect();
    assert_eq!(frames, vec![0, 1, 2, 3, 4, 5]);
}

#[test]
fn the_tray_loads_by_selection_and_leaves_an_army_on_the_map_or_an_empty_plot_alone() {
    let view = |region, army_details, production_building| TrayView {
        region,
        army_details,
        production_building,
    };
    assert_eq!(
        tray_after(None, view(true, false, false)),
        Some(Tray::Region)
    );
    assert_eq!(
        tray_after(Some(Tray::Region), view(true, false, true)),
        Some(Tray::BuildQueue)
    );
    assert_eq!(
        tray_after(Some(Tray::Region), view(true, true, false)),
        Some(Tray::ArmyRetinue)
    );
    assert_eq!(
        tray_after(Some(Tray::BuildQueue), view(true, false, false)),
        Some(Tray::Region),
        "the building's state ended"
    );
    assert_eq!(
        tray_after(Some(Tray::Region), view(true, false, false)),
        Some(Tray::Region),
        "an army on the map or an empty plot keeps the region's panel"
    );
    assert_eq!(tray_after(None, view(false, false, false)), None);
    assert_eq!(Tray::BuildQueue.movie(), "StrategicDetailsBuildQueue.swf");
}

#[test]
fn the_prompt_reads_the_battle_type_the_setup_chose() {
    use super::config::HudConfig;
    use mint_sage_livingworld::screen_queries::BattleType;
    assert_eq!(HudConfig::default().battle_type(), BattleType::Both);
    let config = HudConfig::default().with_battle_type(BattleType::RealTimeOnly);
    assert_eq!(config.battle_type(), BattleType::RealTimeOnly);
}

#[test]
fn unit_icon_slots_fill_rows_of_whole_slots_and_rows_past_the_stage_are_hidden() {
    let places = super::retinue::grid(7, (100.0, 60.0), (30.0, 25.0), 0.0);
    let at: Vec<(f32, f32, bool)> = places.iter().map(|p| (p.x, p.y, p.visible)).collect();
    assert_eq!(
        at,
        vec![
            (0.0, 0.0, true),
            (30.0, 0.0, true),
            (60.0, 0.0, true),
            (0.0, 25.0, true),
            (30.0, 25.0, true),
            (60.0, 25.0, true),
            (0.0, 50.0, true),
        ]
    );
    let deep = super::retinue::grid(10, (100.0, 60.0), (30.0, 25.0), 0.0);
    assert!(!deep[9].visible);
}
