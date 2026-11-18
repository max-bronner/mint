mod ability_approach_tests;
mod ability_weapon_reload_tests;
mod ai_powers_tests;
mod amphibious_tests;
mod animal_ai_tests;
mod animal_flee_tests;
mod aod_crush_tests;
mod aod_oath_tests;
mod approach_wait_tests;
mod attach_horde_tests;
mod attach_tests;
mod attack_ground_tests;
mod attack_move_cadence_tests;
mod attack_voice_tests;
mod auto_cast_tests;
mod auto_repair_tests;
mod broadcast_tests;
mod build_order_tests;
mod build_variation_tests;
mod build_worker_tests;
mod burning_death_tests;
mod busy_ability_tests;
mod busy_tests;
mod cah_tests;
mod capture_tests;
mod castle_tests;
mod cheer_tests;
mod cinematic_tests;
mod claim_tests;
mod closest_path_tests;
mod cohere_speed_tests;
mod command_point_stall_tests;
mod contact_tests;
mod contain_tests;
mod corpse_fade_tests;
mod corpse_sink_tests;
mod cost_modifier_tests;
mod critter_emitter_tests;
mod crush_blocked_attack_tests;
mod crush_notice_tests;
mod crush_weapon_tests;
mod damage_creation_tests;
mod damage_fx_tests;
mod damage_spell_tests;
mod death_phase_tests;
mod death_rung_tests;
mod debug_tests;
mod difficulty_tests;
mod disguise_tests;
mod dodge_tests;
mod dominate_tests;
mod drag_formation_tests;
mod emotion_horde_entry_tests;
mod emotion_tests;
mod engaged_tests;
mod experience_tests;
mod face_target_tests;
mod fear_pulse_tests;
mod fire_tests;
mod flank_tests;
mod fling_tests;
mod force_attack_tests;
mod forfeit_score_tests;
mod forfeit_tests;
mod formation_preview_tests;
mod fx_frame_tests;
mod fxlist_die_tests;
mod gate_cycle_tests;
mod giant_bird_tests;
mod grab_tests;
mod group_formation_tests;
mod group_tests;
mod guard_tests;
mod harvest_tests;
mod held_exit_tests;
mod hero_tests;
mod hit_passenger_tests;
mod horde_approach_tests;
mod horde_behaviour_tests;
mod horde_formation_tests;
mod horde_knockback_tests;
mod horde_motion_tests;
mod horde_power_tests;
mod horde_teleport_tests;
mod horde_trail_tests;
mod idle_acquire_tests;
mod invisibility_special_power_tests;
mod knockback_tests;
mod launch_height_tests;
mod level_grant_tests;
mod location_cast_tests;
mod loose_approach_tests;
mod loose_melee_tests;
mod lose_stealth_trigger_tests;
mod map_clamp_tests;
mod map_keys_tests;
mod map_scripts_tests;
mod match_stats_tests;
mod modifier_tests;
mod module_tests;
mod move_allies_tests;
mod move_destination_tests;
mod movement_speed_tests;
mod neutral_building_tests;
mod nugget_dispatch_tests;
mod ocl_angle_tests;
mod ocl_callers_tests;
mod ocl_forest_tests;
mod ocl_nesting_tests;
mod ocl_upgrade_gate_tests;
mod off_map_entry_tests;
mod order_queue_tests;
mod pick_up_tests;
mod placed_horde_tests;
mod placed_keys_tests;
mod porcupine_banner_tests;
mod porcupine_filter_tests;
mod power_condition_tests;
mod price_display_tests;
mod production_exit_tests;
mod production_stream_tests;
mod rally_tests;
mod ramp_ordering_tests;
mod rampage_tests;
mod rank_display_tests;
mod rebuild_hole_tests;
mod recharge_timing_tests;
mod refund_die_tests;
mod replace_footprint_tests;
mod replenish_tests;
mod resimulation_tests;
mod reveal_to_all_tests;
mod rock_reload_tests;
mod selection_tests;
mod sell_tests;
mod side_change_tests;
mod siege_tests;
mod slaved_tests;
mod spawn_tests;
mod spell_tests;
mod squad_victim_tests;
mod stance_engagement_tests;
mod stealth_tests;
mod stop_tests;
mod summon_replacement_tests;
mod swarm_flinch_tests;
mod taunt_filter_tests;
mod taunt_guard_tests;
mod team_tests;
mod teleport_tests;
mod terrain_height_tests;
mod throw_tests;
mod unit_contact_tests;
mod unit_index_tests;
mod wall_climb_tests;
mod wall_conversion_tests;
mod wall_link_tests;
mod wall_member_drop_tests;
mod wall_span_tests;
mod wall_top_route_tests;
mod weapon_behaviour_tests;
mod weapon_toggle_horde_tests;
mod weapon_toggle_sync_tests;
mod wotr_battle_tests;

use super::*;
use crate::combat::acquire_target;
use crate::combat::GameData;
use mint_movement::{Facing, SearchMode, SmoothingMode};
use mint_sage_asset::KindOf;
use mint_sage_map::HeightMapData;
use mint_sim::Simulation;

const PLAYER: PlayerId = PlayerId::new(0);
const OTHER: PlayerId = PlayerId::new(1);

fn flat_map(width: u32, height: u32) -> HeightMapData {
    HeightMapData {
        width,
        height,
        border_size: 0,
        boundaries: Vec::new(),
        samples: vec![0; (width * height) as usize],
    }
}

#[test]
fn starting_cash_for_a_slot_prefers_its_own_override() {
    let map = flat_map(8, 8);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_economy(Vec::new(), 1_000)
        .with_starting_cash_overrides(vec![Some(4_000), None]);
    assert_eq!(rules.starting_cash_for(0), 4_000, "its own override wins");
    assert_eq!(
        rules.starting_cash_for(1),
        1_000,
        "no override for this slot: the RULES tab value stands"
    );
    assert_eq!(
        rules.starting_cash_for(9),
        1_000,
        "past the override table entirely: the RULES tab value stands"
    );
}

fn wall_map(width: u32, height: u32, wall_x: u32, gap: std::ops::Range<u32>) -> HeightMapData {
    let mut samples = vec![0u16; (width * height) as usize];
    for y in 0..height {
        if y >= gap.start && y <= gap.end {
            continue;
        }
        if let Some(sample) = samples.get_mut((y * width + wall_x) as usize) {
            *sample = 400;
        }
    }
    HeightMapData {
        width,
        height,
        border_size: 0,
        boundaries: Vec::new(),
        samples,
    }
}

fn shape(kind_of: KindOf) -> Shape {
    Shape {
        kind_of,
        radius: 5.0,
        height: 18.0,
        geometry_is_small: false,
    }
}

fn spawn_at(x: f32, y: f32, kind_of: KindOf, owner: PlayerId) -> Spawn {
    Spawn::inert(Vec2::new(x, y), Facing::EAST, shape(kind_of), owner)
}

fn simulation() -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let map = flat_map(64, 64);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let spawns = vec![
        spawn_at(55.0, 55.0, KindOf::SELECTABLE, PLAYER),
        spawn_at(75.0, 55.0, KindOf::SELECTABLE, PLAYER),
        spawn_at(95.0, 55.0, KindOf::SELECTABLE, OTHER),
        spawn_at(115.0, 55.0, KindOf::SHRUBBERY, PLAYER),
    ];
    let simulation = Simulation::new(7, RotwkRules::new(passability, spawns)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    (simulation, ids, units)
}

fn selection(simulation: &Simulation<RotwkRules>, ids: Ids, player: PlayerId) -> Vec<Entity> {
    simulation
        .state()
        .store(ids.selected)
        .map(|store| {
            store
                .iter()
                .filter(|(_, mask)| mask.holds(player))
                .map(|(entity, _)| entity)
                .collect()
        })
        .unwrap_or_default()
}

fn position(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> Vec2 {
    simulation
        .state()
        .get(ids.body, entity)
        .map_or(Vec2::ZERO, |body| body.0.position)
}

#[test]
fn a_selection_command_selects_and_a_second_one_replaces_it() {
    let (mut simulation, ids, units) = simulation();
    let (first, second) = (units[0], units[1]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![first],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(selection(&simulation, ids, PLAYER), vec![first]);

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![second],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(selection(&simulation, ids, PLAYER), vec![second]);

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: false,
                units: vec![first],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(selection(&simulation, ids, PLAYER), vec![first, second]);

    simulation
        .submit(PLAYER, RotwkCommand::ClearSelection)
        .unwrap();
    simulation.tick().unwrap();
    assert!(selection(&simulation, ids, PLAYER).is_empty());
}

#[test]
fn a_unit_the_kindof_predicate_rejects_cannot_be_selected_by_a_command() {
    let (mut simulation, ids, units) = simulation();
    let shrub = units[3];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![shrub],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(selection(&simulation, ids, PLAYER).is_empty());
}

#[test]
fn a_selectable_click_through_object_is_selected_by_a_command() {
    let passability = Passability::build(&flat_map(64, 64), 5.0 / 128.0).unwrap();
    let spawns = vec![spawn_at(
        55.0,
        55.0,
        KindOf::SELECTABLE.union(KindOf::CLICK_THROUGH),
        PLAYER,
    )];
    let mut simulation = Simulation::new(7, RotwkRules::new(passability, spawns)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let gust = simulation.state().store(ids.body).unwrap().entities()[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![gust],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(selection(&simulation, ids, PLAYER), vec![gust]);
}

#[test]
fn one_players_selection_does_not_touch_anothers() {
    let (mut simulation, ids, units) = simulation();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![units[0]],
            },
        )
        .unwrap();
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![units[0], units[2]],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(selection(&simulation, ids, PLAYER), vec![units[0]]);
    assert_eq!(selection(&simulation, ids, OTHER), vec![units[0], units[2]]);

    simulation
        .submit(PLAYER, RotwkCommand::ClearSelection)
        .unwrap();
    simulation.tick().unwrap();
    assert!(selection(&simulation, ids, PLAYER).is_empty());
    assert_eq!(selection(&simulation, ids, OTHER), vec![units[0], units[2]]);
}

#[test]
fn a_selection_list_past_the_cap_is_refused_rather_than_truncated() {
    let (mut simulation, ids, units) = simulation();
    let fat = vec![units[0]; MAX_SELECTION_UNITS + 1];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: fat,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(selection(&simulation, ids, PLAYER).is_empty());
    assert!(!simulation.is_poisoned());
}

#[test]
fn a_move_order_applies_to_the_selection_and_the_units_actually_travel() {
    let (mut simulation, ids, units) = simulation();
    let start = position(&simulation, ids, units[0]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![units[0], units[1]],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 405.0,
                y: 55.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..300 {
        simulation.tick().unwrap();
    }
    let moved = position(&simulation, ids, units[0]);
    assert!(
        (moved.x - 385.0).abs() < 15.0,
        "unit 0 went from {start:?} to {moved:?}"
    );
    let unselected = position(&simulation, ids, units[2]);
    assert!((unselected.x - 95.0).abs() < 1e-3, "{unselected:?}");
}

#[test]
fn an_explicit_move_order_beats_an_acquired_target() {
    let (mut simulation, ids, units) = battle(1, 60.0);
    let mine = units[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![mine],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 50.0,
                y: 200.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..200 {
        simulation.tick().unwrap();
    }
    let moved = position(&simulation, ids, mine);
    assert!(
        (moved.x - 50.0).abs() < 15.0,
        "an acquired target must not cancel an explicit move order; ended at {moved:?}"
    );
}

#[test]
fn a_move_order_breaks_off_a_unit_that_was_already_fighting() {
    let (mut simulation, ids, units) = battle(1, 60.0);
    let (mine, theirs) = (units[0], units[1]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![mine],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: theirs })
        .unwrap();
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    assert!(
        simulation.state().contains(ids.attack_order, mine),
        "the standing order should have taken hold before the move order arrives"
    );

    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 50.0,
                y: 200.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..200 {
        simulation.tick().unwrap();
    }
    assert!(
        !simulation.state().contains(ids.attack_order, mine),
        "an explicit move order must clear a standing attack order"
    );
    let moved = position(&simulation, ids, mine);
    assert!(
            (moved.x - 50.0).abs() < 15.0,
            "a unit already fighting must still break off for an explicit move order; ended at {moved:?}"
        );
}

#[test]
fn an_order_does_not_move_another_players_unit_that_is_merely_selected() {
    let (mut simulation, ids, units) = simulation();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![units[2]],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 405.0,
                y: 55.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..90 {
        simulation.tick().unwrap();
    }
    let held = position(&simulation, ids, units[2]);
    assert!((held.x - 95.0).abs() < 1e-3, "{held:?}");
}

#[test]
fn the_same_commands_produce_the_same_state_hash_on_a_second_run() {
    let run = || {
        let (mut simulation, _, units) = simulation();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![units[0], units[1]],
                },
            )
            .unwrap();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::MoveTo {
                    x: 305.0,
                    y: 155.0,
                    queue: false,
                },
            )
            .unwrap();
        for _ in 0..120 {
            simulation.tick().unwrap();
        }
        simulation.state_hash()
    };
    assert_eq!(run(), run());
}

#[test]
fn changing_the_search_stage_changes_the_path_and_travels_as_a_command() {
    let mut map = flat_map(64, 64);
    for y in 0..40u32 {
        if let Some(sample) = map.samples.get_mut((y * 64 + 20) as usize) {
            *sample = 60_000;
        }
    }
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let spawns = vec![spawn_at(55.0, 55.0, KindOf::SELECTABLE, PLAYER)];
    let mut simulation = Simulation::new(7, RotwkRules::new(passability, spawns)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let unit = simulation
        .state()
        .store(ids.body)
        .and_then(|store| store.entities().first().copied())
        .unwrap();

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetMovementModes(Modes {
                search: SearchMode::StraightLine,
                smoothing: SmoothingMode::Off,
                avoidance: Default::default(),
                formation: Default::default(),
            }),
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 405.0,
                y: 55.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..300 {
        simulation.tick().unwrap();
    }
    let blocked = position(&simulation, ids, unit);
    assert!(
        blocked.x < 200.0,
        "the null search should have stopped at the wall, reached {blocked:?}"
    );

    let (mut simulation, ids, unit) = {
        let mut map = flat_map(64, 64);
        for y in 0..40u32 {
            if let Some(sample) = map.samples.get_mut((y * 64 + 20) as usize) {
                *sample = 60_000;
            }
        }
        let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
        let spawns = vec![spawn_at(55.0, 55.0, KindOf::SELECTABLE, PLAYER)];
        let simulation = Simulation::new(7, RotwkRules::new(passability, spawns)).unwrap();
        let ids = simulation.rules().ids().unwrap();
        let unit = simulation
            .state()
            .store(ids.body)
            .and_then(|store| store.entities().first().copied())
            .unwrap();
        (simulation, ids, unit)
    };
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 405.0,
                y: 55.0,
                queue: false,
            },
        )
        .unwrap();
    let mut arrived = None;
    for tick in 0..900u32 {
        simulation.tick().unwrap();
        if !simulation.state().contains(ids.path, unit) {
            arrived = Some(tick);
            break;
        }
    }
    let round = position(&simulation, ids, unit);
    assert!(
        arrived.is_some(),
        "a* never finished the order, reached {round:?}"
    );
    assert!(
        round.x > 380.0,
        "a* should have gone round the wall, reached {round:?} after {arrived:?} ticks"
    );
}

fn combat_data() -> (
    CombatData,
    mint_sage_combat::WeaponId,
    mint_sage_combat::ArmorId,
) {
    use mint_sage_combat::{DamageNugget, Nugget, ProjectileNugget, WeaponTemplate};

    let mut combat = CombatData::new();
    let pierce = combat.damage_types.intern("PIERCE").unwrap();
    combat
        .weapons
        .insert(
            "TestWarhead",
            WeaponTemplate {
                nuggets: vec![Nugget::Damage(DamageNugget {
                    damage: 40.0,
                    damage_type: pierce,
                    death_type: None,
                    sub_type: None,
                    radius: 0.0,
                    damage_scalar: 1.0,
                    delay: mint_sim::LogicFrames::ZERO,
                    arc: None,
                    arc_inverted: false,
                    flanking_bonus: 0.0,
                    requires_upgrade: false,
                    accepts_add: true,
                })],
                hit_stored_target: true,
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    let bow = combat
        .weapons
        .insert(
            "TestBow",
            WeaponTemplate {
                attack_range: 200.0,
                delay_between_shots: mint_sage_combat::FrameRange::fixed(
                    mint_sim::LogicFrames::from_frames(60),
                ),
                clip_size: 0,
                weapon_speed: 300.0,
                nuggets: vec![Nugget::Projectile(ProjectileNugget {
                    projectile_template: Some("TestArrow".to_owned()),
                    warhead: "TestWarhead".to_owned(),
                    requires_upgrade: false,
                    launch_slot: None,
                })],
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    combat
        .weapons
        .insert(
            "TestSword",
            WeaponTemplate {
                attack_range: 60.0,
                delay_between_shots: mint_sage_combat::FrameRange::fixed(
                    mint_sim::LogicFrames::from_frames(30),
                ),
                clip_size: 0,
                melee: true,
                nuggets: vec![Nugget::Damage(DamageNugget {
                    damage: 40.0,
                    damage_type: pierce,
                    death_type: None,
                    sub_type: None,
                    radius: 0.0,
                    damage_scalar: 1.0,
                    delay: mint_sim::LogicFrames::ZERO,
                    arc: None,
                    arc_inverted: false,
                    flanking_bonus: 0.0,
                    requires_upgrade: false,
                    accepts_add: true,
                })],
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    for (name, leech, follow_through) in [
        ("TestLeechSpear", true, 10),
        ("TestLeechSpearNoFollowThrough", true, 0),
        ("TestPlainSpear", false, 10),
    ] {
        combat
            .weapons
            .insert(
                name,
                WeaponTemplate {
                    attack_range: 60.0,
                    pre_attack_delay: mint_sim::LogicFrames::from_frames(30),
                    delay_between_shots: mint_sage_combat::FrameRange::fixed(
                        mint_sim::LogicFrames::from_frames(30),
                    ),
                    clip_size: 0,
                    leech_range: leech,
                    firing_duration: mint_sim::LogicFrames::from_frames(follow_through),
                    nuggets: vec![Nugget::Damage(DamageNugget {
                        damage: 40.0,
                        damage_type: pierce,
                        death_type: None,
                        sub_type: None,
                        radius: 0.0,
                        damage_scalar: 1.0,
                        delay: mint_sim::LogicFrames::ZERO,
                        arc: None,
                        arc_inverted: false,
                        flanking_bonus: 0.0,
                        requires_upgrade: false,
                        accepts_add: true,
                    })],
                    ..WeaponTemplate::default()
                },
            )
            .unwrap();
    }
    let armour = combat
        .armors
        .insert(
            "TestArmor",
            mint_sage_combat::ArmorTemplate::from_rows(
                &[("DEFAULT".to_owned(), 1.0), ("PIERCE".to_owned(), 0.5)],
                &mut combat.damage_types,
            ),
        )
        .unwrap();
    (combat, bow, armour)
}

fn armed_spawn(
    x: f32,
    y: f32,
    owner: PlayerId,
    weapon: Option<mint_sage_combat::WeaponId>,
    armour: Option<mint_sage_combat::ArmorId>,
    health: f32,
) -> Spawn {
    Spawn {
        health: Some(health),
        weapon,
        armour,
        vision: Some(400.0),
        ..spawn_at(x, y, KindOf::SELECTABLE, owner)
    }
}

fn armed_immobile_spawn(
    x: f32,
    y: f32,
    owner: PlayerId,
    weapon: Option<mint_sage_combat::WeaponId>,
    armour: Option<mint_sage_combat::ArmorId>,
    health: f32,
) -> Spawn {
    let (kind_of, _) = KindOf::parse("SELECTABLE STRUCTURE IMMOBILE");
    Spawn {
        health: Some(health),
        weapon,
        armour,
        vision: Some(400.0),
        ..Spawn::inert(
            Vec2::new(x, y),
            Facing::EAST,
            Shape {
                kind_of,
                radius: 30.0,
                height: 40.0,
                geometry_is_small: false,
            },
            owner,
        )
    }
}

fn structure_and_target_scenario(seed: u64) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let (combat, _, armour) = combat_data();
    let sword = combat.weapons.id("TestSword").unwrap();
    let map = flat_map(64, 64);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let structure = armed_immobile_spawn(100.0, 100.0, PLAYER, Some(sword), Some(armour), 200.0);
    let enemy = armed_spawn(600.0, 100.0, OTHER, Some(sword), Some(armour), 100.0);
    let simulation = Simulation::new(
        seed,
        RotwkRules::new(passability, vec![structure, enemy]).with_combat(combat),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    (simulation, ids, units[0], units[1])
}

#[test]
fn a_move_order_on_an_immobile_structure_is_refused() {
    let (mut simulation, ids, structure, _enemy) = structure_and_target_scenario(1);
    let before = position(&simulation, ids, structure);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![structure],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 500.0,
                y: 500.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..90 {
        simulation.tick().unwrap();
    }
    let after = position(&simulation, ids, structure);
    assert_eq!(before, after, "an immobile structure must never move");
    assert!(
        !simulation.state().contains(ids.path, structure),
        "no route should ever be installed for it"
    );
}

#[test]
fn a_refused_move_order_changes_nothing_including_the_state_hash() {
    let (mut with_order, _ids, structure, _enemy) = structure_and_target_scenario(6);
    let (mut without_order, _, _, _) = structure_and_target_scenario(6);
    with_order
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![structure],
            },
        )
        .unwrap();
    with_order
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 500.0,
                y: 500.0,
                queue: false,
            },
        )
        .unwrap();
    without_order
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![structure],
            },
        )
        .unwrap();
    for _ in 0..90 {
        with_order.tick().unwrap();
        without_order.tick().unwrap();
    }
    assert_eq!(
        with_order.state_hash(),
        without_order.state_hash(),
        "a refused MoveTo must be indistinguishable from no command at all"
    );
}

#[test]
fn the_identical_move_order_issued_by_a_different_player_id_is_refused_the_same_way() {
    let (combat, _, armour) = combat_data();
    let sword = combat.weapons.id("TestSword").unwrap();
    let map = flat_map(64, 64);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let structure = armed_immobile_spawn(100.0, 100.0, OTHER, Some(sword), Some(armour), 200.0);
    let simulation_seed = 5;
    let mut simulation = Simulation::new(
        simulation_seed,
        RotwkRules::new(passability, vec![structure]).with_combat(combat),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let structure = simulation
        .state()
        .store(ids.body)
        .and_then(|store| store.entities().first().copied())
        .unwrap();
    let before = position(&simulation, ids, structure);
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![structure],
            },
        )
        .unwrap();
    simulation
        .submit(
            OTHER,
            RotwkCommand::MoveTo {
                x: 500.0,
                y: 500.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..90 {
        simulation.tick().unwrap();
    }
    let after = position(&simulation, ids, structure);
    assert_eq!(
        before, after,
        "owning the structure does not grant a capability it lacks"
    );
    assert!(!simulation.state().contains(ids.path, structure));
}

#[test]
fn an_attack_order_on_an_immobile_structure_never_moves_it_toward_an_out_of_range_target() {
    let (mut simulation, ids, structure, enemy) = structure_and_target_scenario(3);
    let before = position(&simulation, ids, structure);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![structure],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: enemy })
        .unwrap();
    for _ in 0..300 {
        simulation.tick().unwrap();
        assert!(
            !simulation.state().contains(ids.path, structure),
            "close_on_target must never install a route for an immobile entity"
        );
    }
    let after = position(&simulation, ids, structure);
    assert_eq!(before, after, "the structure must not have moved at all");
}

#[test]
fn an_armed_immobile_structure_is_never_an_eligible_idle_force() {
    assert!(
        !RotwkRules::is_eligible_idle_force(false, true, true, false),
        "immobile must refuse regardless of every other condition passing"
    );
    assert!(
        RotwkRules::is_eligible_idle_force(false, true, false, false),
        "a mobile, armed, unordered loose unit is exactly the case this exists for"
    );
    assert!(!RotwkRules::is_eligible_idle_force(
        true, true, false, false
    ));
    assert!(!RotwkRules::is_eligible_idle_force(
        false, false, false, false
    ));
    assert!(!RotwkRules::is_eligible_idle_force(
        false, true, false, true
    ));
}

fn entity_handles(count: usize) -> Vec<Entity> {
    let map = flat_map(32, 32);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let spawns: Vec<Spawn> = (0..count)
        .map(|_| spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER))
        .collect();
    let simulation = Simulation::new(1, RotwkRules::new(passability, spawns)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default()
}

#[test]
fn the_grid_query_picks_the_same_target_as_a_full_scan_over_a_randomised_battle() {
    let entities = entity_handles(80);
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut rand = move || {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        seed
    };
    #[allow(clippy::cast_precision_loss)]
    for _ in 0..500u32 {
        let mut living: Vec<(Foe, PlayerId)> = Vec::new();
        for &entity in &entities {
            let x = (rand() % 2_000) as f32 / 10.0;
            let y = (rand() % 2_000) as f32 / 10.0;
            let radius = (rand() % 15) as f32;
            let height = (rand() % 20) as f32;
            let player = PlayerId::new(u8::try_from(rand() % 3).unwrap_or(0));
            living.push((
                Foe {
                    entity,
                    position: Vec2::new(x, y),
                    z: 0.0,
                    radius,
                    height,
                    is_structure: false,
                    airborne: None,
                    unattackable: false,
                    not_autoacquirable: false,
                    ai_kind_of: None,
                },
                player,
            ));
        }
        living.sort_by_key(|(foe, _)| foe.entity.index());

        let grid = TargetGrid::build(&living);
        let global_max_radius = living
            .iter()
            .map(|(foe, _)| foe.radius)
            .fold(0.0f32, f32::max);

        #[allow(clippy::cast_possible_truncation)]
        let shooter_at = (rand() as usize) % living.len();
        let (shooter, shooter_player) = living[shooter_at];
        let min_range = 0.0f32;
        let max_range = (rand() % 400) as f32;

        let enemies: Vec<Foe> = living
            .iter()
            .filter(|(_, player)| *player != shooter_player)
            .map(|(foe, _)| *foe)
            .collect();
        let expected = acquire_target(
            shooter.position,
            shooter.radius,
            &enemies,
            min_range,
            max_range,
        );

        let search_radius = TargetGrid::search_radius(max_range, shooter.radius, global_max_radius);
        let mut indices = Vec::new();
        grid.query_into(
            grid_cell(shooter.position.x),
            grid_cell(shooter.position.y),
            grid_radius_cells(search_radius),
            &mut indices,
        );
        let candidates: Vec<Foe> = indices
            .iter()
            .filter_map(|&at| living.get(at as usize))
            .filter(|(_, player)| *player != shooter_player)
            .map(|(foe, _)| *foe)
            .collect();
        let actual = acquire_target(
            shooter.position,
            shooter.radius,
            &candidates,
            min_range,
            max_range,
        );

        assert_eq!(actual, expected, "grid and full scan disagreed");
    }
}

#[test]
fn the_expanding_scan_picks_the_same_target_as_the_full_scan_it_replaced() {
    use super::auto_cast::{nearest_by_expanding_scan, nearest_by_full_scan};

    let entities = entity_handles(150);
    let mut seed = 0x243F_6A88_85A3_08D3u64;
    let mut rand = move || {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        seed
    };
    #[allow(clippy::cast_precision_loss)]
    for round in 0..400u32 {
        let spread = if round % 2 == 0 { 200.0 } else { 6_000.0 };
        let mut living: Vec<(Foe, PlayerId)> = Vec::new();
        for &entity in &entities {
            let x = (rand() % 10_000) as f32 / 10_000.0 * spread;
            let y = (rand() % 10_000) as f32 / 10_000.0 * spread;
            living.push((
                Foe {
                    entity,
                    position: Vec2::new(x, y),
                    z: 0.0,
                    radius: 0.0,
                    height: 0.0,
                    is_structure: false,
                    airborne: None,
                    unattackable: false,
                    not_autoacquirable: false,
                    ai_kind_of: None,
                },
                PLAYER,
            ));
        }
        living.sort_by_key(|(foe, _)| foe.entity.index());
        let grid = TargetGrid::build(&living);
        let position = Vec2::new(
            (rand() % 10_000) as f32 / 10_000.0 * spread,
            (rand() % 10_000) as f32 / 10_000.0 * spread,
        );
        let min_range = if round % 3 == 0 {
            (rand() % 200) as f32
        } else {
            0.0
        };
        let max_range = if round % 5 == 0 {
            min_range + (rand() % 900) as f32
        } else {
            1.0e7
        };
        let salt = rand();
        let passing = |index: u32| -> Option<(f32, Entity)> {
            let (foe, _) = living.get(usize::try_from(index).ok()?)?;
            if (u64::from(index)
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .wrapping_add(salt))
                % 4
                == 0
            {
                return None;
            }
            let (dx, dy) = (foe.position.x - position.x, foe.position.y - position.y);
            let reach = dx * dx + dy * dy;
            if reach > max_range * max_range || reach < min_range * min_range {
                return None;
            }
            Some((reach, foe.entity))
        };
        for min_matches in [0u32, 1, 2, 5, 40] {
            let expected = nearest_by_full_scan(position, &grid, max_range, min_matches, passing);
            let actual = nearest_by_expanding_scan(
                position,
                &grid,
                min_range,
                max_range,
                min_matches,
                passing,
            );
            assert_eq!(
                actual, expected,
                "round {round}, min_matches {min_matches}, spread {spread}"
            );
        }
    }
}

fn battle(count: u32, apart: f32) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let (combat, bow, armour) = combat_data();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let mut spawns = Vec::new();
    for index in 0..count {
        let y = 200.0 + index as f32 * 30.0;
        spawns.push(armed_spawn(
            200.0,
            y,
            PLAYER,
            Some(bow),
            Some(armour),
            100.0,
        ));
        spawns.push(armed_spawn(
            200.0 + apart,
            y,
            OTHER,
            Some(bow),
            Some(armour),
            100.0,
        ));
    }
    let simulation =
        Simulation::new(11, RotwkRules::new(passability, spawns).with_combat(combat)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    (simulation, ids, units)
}

fn health_of(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> Option<f32> {
    simulation
        .state()
        .get(ids.health, entity)
        .map(|health| health.0.current())
}

fn living(simulation: &Simulation<RotwkRules>, ids: Ids, player: PlayerId) -> usize {
    simulation.state().store(ids.health).map_or(0, |store| {
        store
            .iter()
            .filter(|(entity, pool)| {
                !pool.0.is_dead()
                    && simulation
                        .state()
                        .get(ids.owner, *entity)
                        .is_some_and(|owner| owner.0 == player)
            })
            .count()
    })
}

#[test]
fn health_falls_by_the_hand_computed_amount_and_death_lands_on_the_expected_frame() {
    let (combat, bow, armour) = combat_data();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let spawns = vec![
        armed_spawn(200.0, 200.0, PLAYER, Some(bow), Some(armour), 100.0),
        armed_spawn(260.0, 200.0, OTHER, None, Some(armour), 100.0),
    ];
    let mut simulation =
        Simulation::new(11, RotwkRules::new(passability, spawns).with_combat(combat)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    let (shooter, victim) = (units[0], units[1]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![shooter],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: victim })
        .unwrap();

    let mut steps: Vec<(u64, f32)> = Vec::new();
    let mut last = 100.0f32;
    for frame in 1..=260u64 {
        simulation.tick().unwrap();
        let now = health_of(&simulation, ids, victim).unwrap_or(0.0);
        if (now - last).abs() > 1e-3 {
            steps.push((frame, now));
            last = now;
        }
    }
    assert_eq!(
        steps,
        vec![(7, 80.0), (67, 60.0), (127, 40.0), (187, 20.0), (247, 0.0),],
        "health sequence"
    );
    assert!(simulation.state().is_live(victim));
    for _ in 0..DEFAULT_DESTRUCTION_DELAY.get() {
        simulation.tick().unwrap();
    }
    assert!(!simulation.state().is_live(victim));
}

#[test]
fn a_weapons_anti_mask_gates_whether_it_may_damage_a_structure() {
    let (mut combat, sword, armour) = combat_data();
    let mut ram = combat.weapons.get(sword).unwrap().clone();
    ram.anti_mask = ram
        .anti_mask
        .without(mint_sage_combat::AntiMask::GROUND)
        .union(mint_sage_combat::AntiMask::STRUCTURE);
    let ram = combat.weapons.insert("TestRam", ram).unwrap();

    for (weapon, target_is_structure, should_damage) in [
        (sword, false, true),
        (sword, true, true),
        (ram, false, false),
        (ram, true, true),
    ] {
        let map = flat_map(64, 64);
        let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
        let target = if target_is_structure {
            armed_immobile_spawn(110.0, 100.0, OTHER, None, Some(armour), 200.0)
        } else {
            armed_spawn(110.0, 100.0, OTHER, None, Some(armour), 200.0)
        };
        let spawns = vec![
            armed_spawn(100.0, 100.0, PLAYER, Some(weapon), Some(armour), 100.0),
            target,
        ];
        let mut simulation = Simulation::new(
            7,
            RotwkRules::new(passability, spawns).with_combat(combat.clone()),
        )
        .unwrap();
        let ids = simulation.rules().ids().unwrap();
        let units: Vec<Entity> = simulation
            .state()
            .store(ids.body)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        let (shooter, victim) = (units[0], units[1]);
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![shooter],
                },
            )
            .unwrap();
        simulation
            .submit(PLAYER, RotwkCommand::AttackObject { target: victim })
            .unwrap();
        for _ in 0..90 {
            simulation.tick().unwrap();
        }
        let damaged = health_of(&simulation, ids, victim).unwrap_or(200.0) < 200.0;
        assert_eq!(
            damaged, should_damage,
            "weapon {weapon:?} vs. structure={target_is_structure}: expected \
                 damaged={should_damage}"
        );
    }
}

#[test]
fn two_armies_ordered_at_each_other_fight_until_one_side_is_dead() {
    let (mut simulation, ids, units) = battle(4, 150.0);
    let mine: Vec<Entity> = units
        .iter()
        .copied()
        .filter(|e| {
            simulation
                .state()
                .get(ids.owner, *e)
                .is_some_and(|owner| owner.0 == PLAYER)
        })
        .collect();
    let theirs: Vec<Entity> = units
        .iter()
        .copied()
        .filter(|e| !mine.contains(e))
        .collect();
    assert_eq!(mine.len(), 4);
    assert_eq!(theirs.len(), 4);

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: mine.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: theirs[0] })
        .unwrap();
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: theirs.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(OTHER, RotwkCommand::AttackObject { target: mine[0] })
        .unwrap();

    for _ in 0..2000 {
        simulation.tick().unwrap();
        if living(&simulation, ids, PLAYER) == 0 || living(&simulation, ids, OTHER) == 0 {
            break;
        }
    }
    let (ours, theirs_left) = (
        living(&simulation, ids, PLAYER),
        living(&simulation, ids, OTHER),
    );
    assert!(
        ours == 0 || theirs_left == 0,
        "neither side was wiped out: {ours} vs {theirs_left}"
    );
    assert!(ours < 4, "player 0 lost nothing: {ours} of 4 left");
    assert!(
        theirs_left < 4,
        "player 1 lost nothing: {theirs_left} of 4 left"
    );
    assert!(!simulation.is_poisoned());
}

fn construction_scenario(
    seed: u64,
) -> (Simulation<RotwkRules>, Ids, Entity, Entity, Entity, Entity) {
    let (combat, _, armour) = combat_data();
    let sword = combat.weapons.id("TestSword").unwrap();
    let map = flat_map(160, 60);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();

    let (structure_kind, _) = KindOf::parse("SELECTABLE STRUCTURE IMMOBILE");
    let structure = Spawn {
        health: Some(20.0),
        armour: Some(armour),
        ..Spawn::inert(
            Vec2::new(500.0, 300.0),
            Facing::EAST,
            Shape {
                kind_of: structure_kind,
                radius: 60.0,
                height: 30.0,
                geometry_is_small: false,
            },
            OTHER,
        )
    };
    let attacker = armed_spawn(600.0, 300.0, PLAYER, Some(sword), Some(armour), 100.0);
    let mover = spawn_at(100.0, 300.0, KindOf::SELECTABLE, PLAYER);
    let walker = spawn_at(100.0, 300.0, KindOf::SELECTABLE, PLAYER);

    let footprint = vec![mint_sage_passability::FootprintRecord {
        offset: Vec2::new(0.0, 0.0),
        footprint: mint_sage_passability::Footprint::Box {
            major: 60.0,
            minor: 60.0,
        },
    }];
    let spawns = vec![structure, attacker, mover, walker];
    let simulation = Simulation::new(
        seed,
        RotwkRules::new(passability, spawns)
            .with_structure_footprints(vec![(0, footprint)])
            .with_combat(combat),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    (simulation, ids, units[0], units[1], units[2], units[3])
}

#[test]
fn a_structure_blocks_pathing_while_it_stands_and_stops_blocking_once_it_dies() {
    let (mut simulation, ids, structure, attacker, mover, walker) = construction_scenario(21);

    let clock = simulation
        .state()
        .store(ids.obstacles)
        .and_then(|store| store.entities().first().copied())
        .expect("the clock entity carries an obstacle layer");
    let layer = simulation.state().get(ids.obstacles, clock).unwrap();
    assert!(
        layer.counts().iter().any(|count| *count > 0),
        "the structure's footprint is not stamped on the dynamic layer"
    );

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![mover],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 900.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![attacker],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: structure })
        .unwrap();
    simulation.tick().unwrap();

    let route_len = |simulation: &Simulation<RotwkRules>, entity: Entity| -> Option<f32> {
        let path = simulation.state().get(ids.path, entity)?;
        let mut points = vec![simulation.state().get(ids.body, entity)?.0.position];
        points.extend(path.route.points.iter().copied());
        let mut length = 0.0f32;
        for pair in points.windows(2) {
            length += pair[0].minus(pair[1]).length();
        }
        Some(length)
    };
    let direct = 800.0;
    let detouring = route_len(&simulation, mover).expect("a route while the structure stands");
    assert!(
        detouring > direct + 10.0,
        "the route ({detouring}) does not detour around a structure in its way"
    );

    for _ in 0..200 {
        simulation.tick().unwrap();
        if !simulation.state().is_live(structure) {
            break;
        }
    }
    assert!(
        !simulation.state().is_live(structure),
        "the structure was not destroyed"
    );

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![walker],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 900.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let clear = route_len(&simulation, walker).expect("a fresh route once the structure is gone");
    assert!(
        clear < direct + 10.0,
        "the route ({clear}) still detours after the structure died"
    );
    assert!(!simulation.is_poisoned());
}

#[test]
fn construction_and_destruction_replay_to_the_same_hash_sequence() {
    let run = || {
        let (mut simulation, _ids, structure, attacker, mover, _walker) = construction_scenario(21);
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![mover],
                },
            )
            .unwrap();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::MoveTo {
                    x: 900.0,
                    y: 300.0,
                    queue: false,
                },
            )
            .unwrap();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![attacker],
                },
            )
            .unwrap();
        simulation
            .submit(PLAYER, RotwkCommand::AttackObject { target: structure })
            .unwrap();
        let mut hashes = Vec::new();
        for tick in 0..220 {
            simulation.tick().unwrap();
            hashes.push(simulation.state_hash());
            if tick == 150 {
                simulation
                    .submit(
                        PLAYER,
                        RotwkCommand::SetSelection {
                            replace: true,
                            units: vec![mover],
                        },
                    )
                    .unwrap();
                simulation
                    .submit(
                        PLAYER,
                        RotwkCommand::MoveTo {
                            x: 900.0,
                            y: 300.0,
                            queue: false,
                        },
                    )
                    .unwrap();
            }
        }
        hashes
    };
    let first = run();
    let second = run();
    assert_eq!(first, second, "the same commands must replay identically");
    assert!(
        first.iter().any(|hash| *hash != first[0]),
        "the world must actually change"
    );
}

#[test]
fn the_same_fight_produces_the_same_state_hash_on_a_second_run() {
    let run = || {
        let (mut simulation, ids, units) = battle(3, 120.0);
        let mine: Vec<Entity> = units
            .iter()
            .copied()
            .filter(|e| {
                simulation
                    .state()
                    .get(ids.owner, *e)
                    .is_some_and(|owner| owner.0 == PLAYER)
            })
            .collect();
        let theirs: Vec<Entity> = units
            .iter()
            .copied()
            .filter(|e| !mine.contains(e))
            .collect();
        for (player, ours, target) in [
            (PLAYER, mine.clone(), theirs[0]),
            (OTHER, theirs.clone(), mine[0]),
        ] {
            simulation
                .submit(
                    player,
                    RotwkCommand::SetSelection {
                        replace: true,
                        units: ours,
                    },
                )
                .unwrap();
            simulation
                .submit(player, RotwkCommand::AttackObject { target })
                .unwrap();
        }
        for _ in 0..600 {
            simulation.tick().unwrap();
        }
        simulation.state_hash()
    };
    assert_eq!(run(), run());
}

#[test]
fn simultaneous_shots_kill_the_same_units_on_every_run() {
    let survivors = || {
        let (mut simulation, ids, units) = battle(4, 120.0);
        let mine: Vec<Entity> = units
            .iter()
            .copied()
            .filter(|e| {
                simulation
                    .state()
                    .get(ids.owner, *e)
                    .is_some_and(|owner| owner.0 == PLAYER)
            })
            .collect();
        let theirs: Vec<Entity> = units
            .iter()
            .copied()
            .filter(|e| !mine.contains(e))
            .collect();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: mine,
                },
            )
            .unwrap();
        simulation
            .submit(PLAYER, RotwkCommand::AttackObject { target: theirs[0] })
            .unwrap();
        for _ in 0..400 {
            simulation.tick().unwrap();
        }
        let mut alive: Vec<u32> = simulation
            .state()
            .store(ids.health)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, pool)| !pool.0.is_dead())
                    .map(|(entity, _)| entity.index())
                    .collect()
            })
            .unwrap_or_default();
        alive.sort_unstable();
        alive
    };
    assert_eq!(survivors(), survivors());
}

#[test]
fn an_attack_order_naming_something_unattackable_is_refused_quietly() {
    let (combat, bow, armour) = combat_data();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let spawns = vec![
        armed_spawn(200.0, 200.0, PLAYER, Some(bow), Some(armour), 100.0),
        spawn_at(260.0, 200.0, KindOf::SELECTABLE, OTHER),
        armed_spawn(200.0, 240.0, PLAYER, Some(bow), Some(armour), 100.0),
    ];
    let mut simulation =
        Simulation::new(5, RotwkRules::new(passability, spawns).with_combat(combat)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    let (shooter, prop, ally) = (units[0], units[1], units[2]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![shooter],
            },
        )
        .unwrap();
    for target in [prop, ally, shooter] {
        simulation
            .submit(PLAYER, RotwkCommand::AttackObject { target })
            .unwrap();
        for _ in 0..40 {
            simulation.tick().unwrap();
        }
        assert!(!simulation.is_poisoned(), "target {}", target.index());
        assert_eq!(health_of(&simulation, ids, ally), Some(100.0));
        assert_eq!(health_of(&simulation, ids, shooter), Some(100.0));
    }
    let stale = prop;
    simulation.tick().unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: stale })
        .unwrap();
    simulation.tick().unwrap();
    assert!(!simulation.is_poisoned());
}

#[test]
fn a_horde_ordered_at_an_unattackable_object_never_holds_the_order() {
    let (mut simulation, ids, pad) = horde_and_structure_sim(
        Vec2::new(200.0, 200.0),
        Vec2::new(230.0, 200.0),
        "UNATTACKABLE",
    );
    let members = members_of(&simulation, ids);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members,
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: pad })
        .unwrap();
    simulation.tick().unwrap();
    let horde = horde_entity(&simulation, ids).unwrap();
    assert!(
        !simulation.state().contains(ids.attack_order, horde),
        "an UNATTACKABLE object must refuse the order outright"
    );
}

#[test]
fn a_horde_ordered_at_a_not_autoacquirable_object_still_holds_the_order() {
    let (mut simulation, ids, inn) = horde_and_structure_sim(
        Vec2::new(200.0, 200.0),
        Vec2::new(230.0, 200.0),
        "NOT_AUTOACQUIRABLE",
    );
    let members = members_of(&simulation, ids);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members,
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: inn })
        .unwrap();
    simulation.tick().unwrap();
    let horde = horde_entity(&simulation, ids).unwrap();
    assert_eq!(
        simulation
            .state()
            .get(ids.attack_order, horde)
            .map(|order| order.target),
        Some(inn),
        "NOT_AUTOACQUIRABLE is still a legal order target"
    );
}

#[test]
fn a_hostile_weapon_or_body_template_still_ticks_without_panicking() {
    use mint_sage_combat::{DamageNugget, Nugget, WeaponTemplate};
    for (range, damage, coefficient, health) in [
        (f32::NAN, 10.0f32, 1.0f32, 100.0f32),
        (1e30, 10.0, 1.0, 100.0),
        (200.0, f32::NAN, 1.0, 100.0),
        (200.0, f32::INFINITY, 1.0, 100.0),
        (200.0, 10.0, f32::NAN, 100.0),
        (200.0, 10.0, -5.0, 100.0),
        (200.0, -1e30, 1.0, 100.0),
        (200.0, 10.0, 1.0, f32::NAN),
        (200.0, 10.0, 1.0, -50.0),
    ] {
        let mut combat = CombatData::new();
        let pierce = combat.damage_types.intern("PIERCE").unwrap();
        let weapon = combat
            .weapons
            .insert(
                "Hostile",
                WeaponTemplate {
                    attack_range: range,
                    nuggets: vec![Nugget::Damage(DamageNugget {
                        damage,
                        damage_type: pierce,
                        death_type: None,
                        sub_type: None,
                        radius: 0.0,
                        damage_scalar: 1.0,
                        delay: mint_sim::LogicFrames::ZERO,
                        arc: None,
                        arc_inverted: false,
                        flanking_bonus: 0.0,
                        requires_upgrade: false,
                        accepts_add: true,
                    })],
                    ..WeaponTemplate::default()
                },
            )
            .unwrap();
        let armour = combat
            .armors
            .insert(
                "Hostile",
                mint_sage_combat::ArmorTemplate::from_rows(
                    &[("DEFAULT".to_owned(), coefficient)],
                    &mut combat.damage_types,
                ),
            )
            .unwrap();
        let map = flat_map(64, 64);
        let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
        let spawns = vec![
            armed_spawn(200.0, 200.0, PLAYER, Some(weapon), Some(armour), health),
            armed_spawn(260.0, 200.0, OTHER, Some(weapon), Some(armour), health),
        ];
        let mut simulation =
            Simulation::new(3, RotwkRules::new(passability, spawns).with_combat(combat)).unwrap();
        let ids = simulation.rules().ids().unwrap();
        let units: Vec<Entity> = simulation
            .state()
            .store(ids.body)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        for (index, unit) in units.iter().enumerate() {
            let player = if index == 0 { PLAYER } else { OTHER };
            let target = units.get(1 - index).copied();
            simulation
                .submit(
                    player,
                    RotwkCommand::SetSelection {
                        replace: true,
                        units: vec![*unit],
                    },
                )
                .unwrap();
            if let Some(target) = target {
                simulation
                    .submit(player, RotwkCommand::AttackObject { target })
                    .unwrap();
            }
        }
        let label = format!("range {range} damage {damage} coeff {coefficient} hp {health}");
        for _ in 0..120 {
            simulation.tick().unwrap();
        }
        assert!(!simulation.is_poisoned(), "{label}");
        for unit in &units {
            if let Some(current) = health_of(&simulation, ids, *unit) {
                assert!(current.is_finite(), "{label}: health {current}");
                assert!(current >= 0.0, "{label}: health {current}");
            }
        }
        if !health.is_finite() || health <= 0.0 {
            assert!(
                health_of(&simulation, ids, units[0]).is_none(),
                "{label}: a refused MaxHealth still produced a health pool"
            );
        }
    }
}

#[test]
fn a_projectile_in_flight_is_drawn_between_its_last_two_positions() {
    use crate::render_state::{PoseTrack, RenderState};
    let (mut simulation, ids, units) = battle(2, 150.0);
    let mine: Vec<Entity> = units
        .iter()
        .copied()
        .filter(|e| {
            simulation
                .state()
                .get(ids.owner, *e)
                .is_some_and(|owner| owner.0 == PLAYER)
        })
        .collect();
    let target = units
        .iter()
        .copied()
        .find(|e| !mine.contains(e))
        .expect("an enemy");
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: mine,
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target })
        .unwrap();

    let flying = |simulation: &Simulation<RotwkRules>| -> Vec<Entity> {
        simulation
            .state()
            .store(ids.flight)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default()
    };

    let mut track = PoseTrack::new();
    let mut checked = 0usize;
    for _ in 0..120 {
        track.capture(simulation.state(), ids);
        let entities_before = flying(&simulation);
        let before = RenderState::new(simulation.state(), 0.0).projectiles(ids);
        simulation.tick().unwrap();
        if flying(&simulation) != entities_before {
            continue;
        }
        let after = RenderState::new(simulation.state(), 0.0).projectiles(ids);
        let half = RenderState::interpolated(simulation.state(), &track, 0.5).projectiles(ids);
        for index in 0..before.len() {
            let (was, now, mid) = (before[index], after[index], half[index]);
            if (was.x - now.x).abs() < 0.5 {
                continue;
            }
            assert!(
                    (mid.x - (was.x + now.x) * 0.5).abs() < 1e-3
                        && (mid.y - (was.y + now.y) * 0.5).abs() < 1e-3
                        && (mid.z - (was.z + now.z) * 0.5).abs() < 1e-3,
                    "half a tick into a flight drew {mid:?}, not the midpoint of {was:?}                      and {now:?}"
                );
            checked = checked.saturating_add(1);
        }
    }
    assert!(
            checked >= 3,
            "only {checked} frames had exactly one arrow in flight that moved;              the test never exercised what it claims to"
        );
}

#[test]
fn a_projectile_whose_target_dies_in_flight_is_removed() {
    let (mut simulation, ids, units) = battle(2, 150.0);
    let mine: Vec<Entity> = units
        .iter()
        .copied()
        .filter(|e| {
            simulation
                .state()
                .get(ids.owner, *e)
                .is_some_and(|owner| owner.0 == PLAYER)
        })
        .collect();
    let theirs: Vec<Entity> = units
        .iter()
        .copied()
        .filter(|e| !mine.contains(e))
        .collect();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: mine,
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: theirs[0] })
        .unwrap();
    let mut peak = 0usize;
    for _ in 0..1200 {
        simulation.tick().unwrap();
        let flights = simulation
            .state()
            .store(ids.flight)
            .map_or(0, mint_sim::ComponentStore::len);
        peak = peak.max(flights);
    }
    assert!(peak > 0, "no projectile was ever launched");
    assert_eq!(
        simulation
            .state()
            .store(ids.flight)
            .map_or(0, mint_sim::ComponentStore::len),
        0,
        "projectiles were left in flight after the fight ended"
    );
}

#[test]
fn a_unit_ordered_at_a_target_out_of_range_walks_to_it_and_does_not_fire_first() {
    let (combat, _, armour) = combat_data();
    let sword = combat.weapons.id("TestSword").unwrap();
    let map = flat_map(128, 128);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let spawns = vec![
        armed_spawn(200.0, 200.0, PLAYER, Some(sword), Some(armour), 100.0),
        armed_spawn(600.0, 200.0, OTHER, None, Some(armour), 400.0),
    ];
    let mut simulation =
        Simulation::new(9, RotwkRules::new(passability, spawns).with_combat(combat)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    let (shooter, victim) = (units[0], units[1]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![shooter],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: victim })
        .unwrap();

    for frame in 1..=100u64 {
        simulation.tick().unwrap();
        assert_eq!(
            health_of(&simulation, ids, victim),
            Some(400.0),
            "the shooter hit from out of range on frame {frame}"
        );
    }
    let travelled = position(&simulation, ids, shooter).x - 200.0;
    assert!(travelled > 50.0, "the shooter did not close: {travelled}");
    for _ in 0..600 {
        simulation.tick().unwrap();
    }
    let left = health_of(&simulation, ids, victim).unwrap_or(400.0);
    assert!(
        left < 400.0,
        "the shooter never got into range: {left} hp left"
    );
}

#[test]
fn a_leech_range_weapon_holds_its_target_through_one_swing_only() {
    let hurt_with = |weapon_name: &str, ticks: u32| {
        let (combat, _, armour) = combat_data();
        let spear = combat.weapons.id(weapon_name).unwrap();
        let map = flat_map(128, 128);
        let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
        let spawns = vec![
            armed_immobile_spawn(200.0, 200.0, PLAYER, Some(spear), Some(armour), 400.0),
            armed_spawn(290.0, 200.0, OTHER, None, Some(armour), 400.0),
        ];
        let mut simulation =
            Simulation::new(9, RotwkRules::new(passability, spawns).with_combat(combat)).unwrap();
        let ids = simulation.rules().ids().unwrap();
        let units: Vec<Entity> = simulation
            .state()
            .store(ids.body)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        let victim = units[1];
        simulation
            .submit(
                OTHER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![victim],
                },
            )
            .unwrap();
        simulation
            .submit(
                OTHER,
                RotwkCommand::MoveTo {
                    x: 600.0,
                    y: 200.0,
                    queue: false,
                },
            )
            .unwrap();
        for _ in 0..ticks {
            simulation.tick().unwrap();
        }
        400.0 - health_of(&simulation, ids, victim).unwrap_or(400.0)
    };
    let first_swing = hurt_with("TestLeechSpear", 45);
    assert!(first_swing > 0.0, "the swing was dropped");
    assert_eq!(
        hurt_with("TestLeechSpear", 150),
        first_swing,
        "the lock outlived its swing"
    );
    assert_eq!(hurt_with("TestLeechSpearNoFollowThrough", 150), 0.0);
    assert_eq!(hurt_with("TestPlainSpear", 150), 0.0);
}

#[test]
fn a_missed_lob_still_hurts_what_it_lands_on_whatever_the_collide_mask_names() {
    use mint_sage_combat::{CollideMask, DamageNugget, Nugget, ProjectileNugget, WeaponTemplate};
    let hurt_with = |mask: &[&str]| {
        let (mut combat, _, armour) = combat_data();
        let pierce = combat.damage_types.intern("PIERCE").unwrap();
        combat
            .weapons
            .insert(
                "TestLobWarhead",
                WeaponTemplate {
                    projectile_collides: CollideMask::parse(mask).unwrap(),
                    nuggets: vec![Nugget::Damage(DamageNugget {
                        damage: 40.0,
                        damage_type: pierce,
                        death_type: None,
                        sub_type: None,
                        radius: 0.0,
                        damage_scalar: 1.0,
                        delay: mint_sim::LogicFrames::ZERO,
                        arc: None,
                        arc_inverted: false,
                        flanking_bonus: 0.0,
                        requires_upgrade: false,
                        accepts_add: true,
                    })],
                    ..WeaponTemplate::default()
                },
            )
            .unwrap();
        let lob = combat
            .weapons
            .insert(
                "TestLob",
                WeaponTemplate {
                    attack_range: 200.0,
                    delay_between_shots: mint_sage_combat::FrameRange::fixed(
                        mint_sim::LogicFrames::from_frames(60),
                    ),
                    weapon_speed: 300.0,
                    hit_chance: 0.0,
                    nuggets: vec![Nugget::Projectile(ProjectileNugget {
                        projectile_template: None,
                        warhead: "TestLobWarhead".to_owned(),
                        requires_upgrade: false,
                        launch_slot: None,
                    })],
                    ..WeaponTemplate::default()
                },
            )
            .unwrap();
        let map = flat_map(96, 96);
        let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
        let spawns = vec![
            Spawn {
                ground_offset: Some(10.0),
                ..armed_spawn(200.0, 200.0, PLAYER, Some(lob), Some(armour), 100.0)
            },
            armed_spawn(260.0, 200.0, OTHER, None, Some(armour), 100.0),
        ];
        let mut simulation =
            Simulation::new(11, RotwkRules::new(passability, spawns).with_combat(combat)).unwrap();
        let ids = simulation.rules().ids().unwrap();
        let victim = simulation
            .state()
            .store(ids.body)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default()[1];
        for _ in 0..40 {
            simulation.tick().unwrap();
        }
        100.0 - health_of(&simulation, ids, victim).unwrap_or(100.0)
    };
    assert!(
        hurt_with(&["ENEMIES"]) > 0.0,
        "the lob hurt nothing it fell on"
    );
    assert!(
        hurt_with(&["STRUCTURES"]) > 0.0,
        "a position blast must reach a bystander the collide mask excludes"
    );
}

#[test]
fn a_lost_target_is_not_replaced_until_the_mood_check_cadence_allows_it() {
    let (combat, bow, armour) = combat_data();
    let map = flat_map(128, 128);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let spawns = vec![
        armed_spawn(200.0, 200.0, PLAYER, Some(bow), Some(armour), 100.0),
        armed_spawn(260.0, 200.0, OTHER, None, Some(armour), 15.0),
        armed_spawn(200.0, 340.0, OTHER, None, Some(armour), 300.0),
    ];
    let mut simulation =
        Simulation::new(9, RotwkRules::new(passability, spawns).with_combat(combat)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    let (shooter, near, far) = (units[0], units[1], units[2]);

    let target_of = |simulation: &Simulation<RotwkRules>| -> Option<Entity> {
        simulation
            .state()
            .get(ids.armed, shooter)
            .and_then(|armed| armed.target)
    };

    let mut died_at: Option<u64> = None;
    let mut retargeted_at: Option<u64> = None;
    for frame in 1..=300u64 {
        simulation.tick().unwrap();
        if died_at.is_none() && health_of(&simulation, ids, near).is_some_and(|hp| hp <= 0.0) {
            died_at = Some(frame);
        }
        if let Some(died) = died_at.filter(|&died| frame > died) {
            if retargeted_at.is_none() && target_of(&simulation) == Some(far) {
                retargeted_at = Some(frame);
            } else if retargeted_at.is_none() {
                assert_eq!(
                    target_of(&simulation),
                    None,
                    "frame {frame}: idle is the only correct state between {died}'s death \
                         and the next cadence check - an early rescan is exactly the bug this \
                         cadence exists to prevent"
                );
            }
        }
    }
    let died = died_at.expect("the near enemy never died - the scenario needs adjusting");
    let retargeted = retargeted_at.expect("the shooter never picked the far enemy up at all");
    let cadence = u64::from(MOOD_ATTACK_CHECK.get());
    let stagger = u64::from(shooter.index()) % cadence;
    let next_acquire = stagger.saturating_add(cadence).saturating_add(1);
    let expected = died.saturating_add(1).max(next_acquire);
    assert_eq!(
        retargeted, expected,
        "retargeted on frame {retargeted}, not the cadence-predicted frame {expected} \
             (died {died}, stagger {stagger}, next_acquire {next_acquire})"
    );
}

#[test]
fn a_still_valid_target_is_kept_even_when_a_much_closer_enemy_arrives() {
    let (combat, bow, armour) = combat_data();
    let map = flat_map(128, 128);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let spawns = vec![
        armed_spawn(200.0, 200.0, PLAYER, Some(bow), Some(armour), 100.0),
        armed_spawn(200.0, 350.0, OTHER, None, Some(armour), 300.0),
        armed_spawn(200.0, 550.0, OTHER, None, Some(armour), 300.0),
    ];
    let mut simulation =
        Simulation::new(9, RotwkRules::new(passability, spawns).with_combat(combat)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    let (shooter, far, near) = (units[0], units[1], units[2]);

    for _ in 0..u64::from(MOOD_ATTACK_CHECK.get()).saturating_add(5) {
        simulation.tick().unwrap();
    }
    assert_eq!(
        simulation
            .state()
            .get(ids.armed, shooter)
            .and_then(|armed| armed.target),
        Some(far),
        "the shooter did not acquire the only candidate in range"
    );

    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![near],
            },
        )
        .unwrap();
    simulation
        .submit(
            OTHER,
            RotwkCommand::MoveTo {
                x: 200.0,
                y: 210.0,
                queue: false,
            },
        )
        .unwrap();

    for frame in 1..=320u64 {
        simulation.tick().unwrap();
        assert_eq!(
            simulation
                .state()
                .get(ids.armed, shooter)
                .and_then(|armed| armed.target),
            Some(far),
            "frame {frame}: switched off a still-valid target - stickiness failed"
        );
    }
    let near_position = simulation.state().get(ids.body, near).unwrap().0.position;
    let (dx, dy) = (near_position.x - 200.0, near_position.y - 200.0);
    let distance = (dx * dx + dy * dy).sqrt();
    assert!(
        distance < 100.0,
        "the near enemy never actually closed the distance: {distance}"
    );
}

#[test]
fn a_search_bigger_than_the_shared_budget_is_still_served_once_it_becomes_the_queues_head() {
    let (combat, bow, armour) = combat_data();
    let map = flat_map(4_600, 20);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let spawns = vec![
        armed_spawn(0.0, 100.0, PLAYER, Some(bow), Some(armour), 100.0),
        armed_spawn(45_000.0, 100.0, OTHER, None, Some(armour), 100.0),
    ];
    let mut simulation =
        Simulation::new(9, RotwkRules::new(passability, spawns).with_combat(combat)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    let (shooter, victim) = (units[0], units[1]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![shooter],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: victim })
        .unwrap();

    simulation.tick().unwrap();
    assert!(
        !simulation.state().contains(ids.path, shooter),
        "the first attempt should exhaust the shared per-tick budget and produce no route yet"
    );

    simulation.tick().unwrap();
    assert!(
        simulation.state().contains(ids.path, shooter),
        "the request must be served once it becomes the queue's guaranteed head - starvation"
    );
}

fn many_shooters_far_from_one_target(
    count: u32,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>, Entity) {
    let (combat, bow, armour) = combat_data();
    let map = flat_map(1_100, 20 * count + 100);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let mut spawns = Vec::new();
    for i in 0..count {
        spawns.push(armed_spawn(
            5.0,
            f32::from(u16::try_from(i).unwrap_or(0)) * 30.0 + 5.0,
            PLAYER,
            Some(bow),
            Some(armour),
            100.0,
        ));
    }
    spawns.push(armed_spawn(10_000.0, 5.0, OTHER, None, Some(armour), 100.0));
    let mut simulation =
        Simulation::new(9, RotwkRules::new(passability, spawns).with_combat(combat)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    let shooters = units[..count as usize].to_vec();
    let victim = units[count as usize];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: shooters.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: victim })
        .unwrap();
    (simulation, ids, shooters, victim)
}

#[test]
fn the_shared_budget_bounds_a_tick_where_many_units_replan_at_once() {
    const SHOOTERS: u32 = 30;
    let (mut simulation, ids, shooters, _victim) = many_shooters_far_from_one_target(SHOOTERS);

    simulation.tick().unwrap();
    let after_one_tick = shooters
        .iter()
        .filter(|&&e| simulation.state().contains(ids.path, e))
        .count();
    assert!(
        after_one_tick <= 5,
        "the shared budget should afford about three searches this size, not {after_one_tick} \
             of {SHOOTERS} - the per-tick bound is not holding"
    );
    assert!(
        after_one_tick > 0,
        "nobody got a route at all, which proves nothing about a *bound* - the map or the \
             distance is wrong for this test"
    );
}

#[test]
fn a_replay_with_a_busy_path_queue_still_produces_the_same_hash_sequence() {
    let run = || {
        let (mut simulation, _ids, _shooters, _victim) = many_shooters_far_from_one_target(20);
        let mut hashes = Vec::new();
        for _ in 0..40 {
            simulation.tick().unwrap();
            hashes.push(simulation.state_hash());
        }
        hashes
    };
    assert_eq!(run(), run());
}

#[test]
fn the_slow_death_roll_picks_between_the_declared_options() {
    let (mut simulation, ids, units) = battle(8, 120.0);
    let mine: Vec<Entity> = units
        .iter()
        .copied()
        .filter(|e| {
            simulation
                .state()
                .get(ids.owner, *e)
                .is_some_and(|owner| owner.0 == PLAYER)
        })
        .collect();
    let theirs: Vec<Entity> = units
        .iter()
        .copied()
        .filter(|e| !mine.contains(e))
        .collect();
    for (player, ours, target) in [
        (PLAYER, mine.clone(), theirs[0]),
        (OTHER, theirs.clone(), mine[0]),
    ] {
        simulation
            .submit(
                player,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: ours,
                },
            )
            .unwrap();
        simulation
            .submit(player, RotwkCommand::AttackObject { target })
            .unwrap();
    }
    let mut seen: Vec<u32> = Vec::new();
    for _ in 0..2000 {
        simulation.tick().unwrap();
        if let Some(store) = simulation.state().store(ids.doomed) {
            for (_, doomed) in store.iter() {
                if !seen.contains(&doomed.death_index) {
                    seen.push(doomed.death_index);
                }
            }
        }
    }
    seen.sort_unstable();
    assert_eq!(
        seen,
        vec![0, 1],
        "the roll never chose both options over a whole battle"
    );
}

fn table_of(offsets: &[Vec2]) -> FormationTable {
    FormationTable::from_offsets(offsets).with_settings(mint_sage_formation::HordeSettings {
        melee_attack_leash_distance: 1.0,
        ranks_to_release: vec![0],
        ..mint_sage_formation::HordeSettings::default()
    })
}

fn square_formation() -> FormationTable {
    let mut offsets = Vec::new();
    for row in 0..3i32 {
        for column in 0..3i32 {
            offsets.push(Vec2::new(
                f32::from(i16::try_from(column - 1).unwrap()) * 20.0,
                f32::from(i16::try_from(row - 1).unwrap()) * 20.0,
            ));
        }
    }
    table_of(&offsets)
}

fn wide_line_formation(members: i32) -> FormationTable {
    let half = members / 2;
    let offsets: Vec<Vec2> = (0..members)
        .map(|slot| Vec2::new(0.0, f32::from(i16::try_from(slot - half).unwrap()) * 20.0))
        .collect();
    table_of(&offsets)
}

fn horde_sim(origin: Vec2, formation: FormationTable) -> (Simulation<RotwkRules>, Ids) {
    horde_sim_over(origin, formation, flat_map(96, 96))
}

fn horde_sim_over(
    origin: Vec2,
    formation: FormationTable,
    map: HeightMapData,
) -> (Simulation<RotwkRules>, Ids) {
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let count = u32::try_from(formation.len()).unwrap();
    let spawn = HordeSpawn {
        formation: 0,
        origin,
        facing: Facing::EAST,
        member: spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER),
        count,
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let rules = RotwkRules::new(passability, Vec::new()).with_hordes(vec![formation], vec![spawn]);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

fn many_hordes_sim_over(
    origins: &[Vec2],
    formation: FormationTable,
    map: HeightMapData,
) -> (Simulation<RotwkRules>, Ids) {
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let count = u32::try_from(formation.len()).unwrap();
    let spawns: Vec<HordeSpawn> = origins
        .iter()
        .map(|&origin| HordeSpawn {
            formation: 0,
            origin,
            facing: Facing::EAST,
            member: spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER),
            count,
            vision: crate::shroud::HORDE_SHROUD_RANGE,
        })
        .collect();
    let formations = vec![formation; origins.len()];
    let rules = RotwkRules::new(passability, Vec::new()).with_hordes(formations, spawns);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

fn members_of(simulation: &Simulation<RotwkRules>, ids: Ids) -> Vec<Entity> {
    simulation
        .state()
        .store(ids.horde)
        .and_then(|store| store.iter().next().map(|(_, horde)| horde.members.clone()))
        .unwrap_or_default()
}

fn horde_entity(simulation: &Simulation<RotwkRules>, ids: Ids) -> Option<Entity> {
    simulation
        .state()
        .store(ids.horde)
        .and_then(|store| store.entities().first().copied())
}

fn centre_of(simulation: &Simulation<RotwkRules>, ids: Ids, of: &[Entity]) -> Vec2 {
    let mut sum = Vec2::ZERO;
    let mut count = 0u16;
    for entity in of {
        if let Some(body) = simulation.state().get(ids.body, *entity) {
            sum = Vec2::new(sum.x + body.0.position.x, sum.y + body.0.position.y);
            count = count.saturating_add(1);
        }
    }
    if count == 0 {
        return Vec2::ZERO;
    }
    Vec2::new(sum.x / f32::from(count), sum.y / f32::from(count))
}

fn extent(simulation: &Simulation<RotwkRules>, ids: Ids, of: &[Entity]) -> (f32, f32) {
    let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
    let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
    for entity in of {
        if let Some(body) = simulation.state().get(ids.body, *entity) {
            min_x = min_x.min(body.0.position.x);
            min_y = min_y.min(body.0.position.y);
            max_x = max_x.max(body.0.position.x);
            max_y = max_y.max(body.0.position.y);
        }
    }
    ((max_x - min_x).max(0.0), (max_y - min_y).max(0.0))
}

fn order_horde(simulation: &mut Simulation<RotwkRules>, ids: Ids, modes: Modes, goal: Vec2) {
    let members = members_of(simulation, ids);
    simulation
        .submit(PLAYER, RotwkCommand::SetMovementModes(modes))
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members,
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: goal.x,
                y: goal.y,
                queue: false,
            },
        )
        .unwrap();
    let _ = ids;
}

fn armed_horde_sim(origin: Vec2, enemy: Vec2) -> (Simulation<RotwkRules>, Ids) {
    let (combat, bow, armour) = combat_data();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let formation = square_formation();
    let count = u32::try_from(formation.len()).unwrap();
    let spawn = HordeSpawn {
        formation: 0,
        origin,
        facing: Facing::EAST,
        member: Spawn {
            health: Some(200.0),
            weapon: Some(bow),
            armour: Some(armour),
            ..spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER)
        },
        count,
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let rules = RotwkRules::new(
        passability,
        vec![Spawn {
            facing: Facing::from_vector(origin.minus(enemy)).unwrap_or(Facing::EAST),
            ..armed_spawn(enemy.x, enemy.y, OTHER, Some(bow), Some(armour), 5000.0)
        }],
    )
    .with_combat(combat)
    .with_hordes(vec![formation], vec![spawn]);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

fn horde_and_structure_sim(
    origin: Vec2,
    structure: Vec2,
    extra_kind_of: &str,
) -> (Simulation<RotwkRules>, Ids, Entity) {
    let (combat, bow, armour) = combat_data();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let formation = square_formation();
    let count = u32::try_from(formation.len()).unwrap();
    let spawn = HordeSpawn {
        formation: 0,
        origin,
        facing: Facing::EAST,
        member: Spawn {
            health: Some(200.0),
            weapon: Some(bow),
            armour: Some(armour),
            ..spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER)
        },
        count,
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let (kind_of, _) = KindOf::parse(&format!("STRUCTURE SELECTABLE IMMOBILE {extra_kind_of}"));
    let rules = RotwkRules::new(
        passability,
        vec![Spawn {
            health: Some(500.0),
            ..spawn_at(structure.x, structure.y, kind_of, OTHER)
        }],
    )
    .with_combat(combat)
    .with_hordes(vec![formation], vec![spawn]);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let structure_entity = enemy_of(&simulation, ids);
    (simulation, ids, structure_entity)
}

fn engaged(simulation: &Simulation<RotwkRules>, ids: Ids, members: &[Entity]) -> usize {
    members
        .iter()
        .filter(|member| {
            simulation
                .state()
                .get(ids.armed, **member)
                .is_some_and(|armed| armed.target.is_some())
        })
        .count()
}

#[test]
fn selecting_one_member_selects_every_member_of_its_horde() {
    let (mut simulation, ids) = horde_sim(Vec2::new(150.0, 150.0), square_formation());
    let members = members_of(&simulation, ids);
    assert_eq!(members.len(), 9);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![members[4]],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let selected = selection(&simulation, ids, PLAYER);
    let mut expected = members.clone();
    expected.sort_by_key(|entity| entity.index());
    assert_eq!(
        selected, expected,
        "one member was picked; the whole horde must be selected"
    );
}

#[test]
fn selecting_a_loose_unit_selects_only_that_unit() {
    let (mut simulation, ids, units) = simulation();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![units[0]],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(selection(&simulation, ids, PLAYER), vec![units[0]]);
}

#[test]
fn a_move_order_on_a_horde_breaks_its_members_off_the_fight() {
    let (mut simulation, ids) = armed_horde_sim(Vec2::new(200.0, 200.0), Vec2::new(400.0, 200.0));
    let members = members_of(&simulation, ids);
    assert_eq!(members.len(), 9);

    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    assert!(
        engaged(&simulation, ids, &members) > 0,
        "the members never engaged, so the test proves nothing"
    );

    order_horde(
        &mut simulation,
        ids,
        Modes::default(),
        Vec2::new(60.0, 200.0),
    );
    for _ in 0..40 {
        simulation.tick().unwrap();
        assert_eq!(
            engaged(&simulation, ids, &members),
            0,
            "a horde under an explicit move order must not have members re-acquiring"
        );
    }
    let horde = horde_entity(&simulation, ids).expect("the horde is alive");
    let moved = simulation.state().get(ids.body, horde).unwrap().0.position;
    assert!(
        moved.x < 190.0,
        "the horde did not march; it is at {moved:?}"
    );
}

#[test]
fn members_acquire_again_once_the_horde_has_arrived() {
    let (mut simulation, ids) = armed_horde_sim(Vec2::new(200.0, 200.0), Vec2::new(400.0, 200.0));
    let members = members_of(&simulation, ids);
    order_horde(
        &mut simulation,
        ids,
        Modes::default(),
        Vec2::new(300.0, 200.0),
    );
    for _ in 0..400 {
        simulation.tick().unwrap();
    }
    let horde = horde_entity(&simulation, ids).expect("the horde is alive");
    assert!(
        !simulation.state().contains(ids.path, horde),
        "the horde is still walking, so the suppression has not been lifted yet"
    );
    assert!(
        engaged(&simulation, ids, &members) > 0,
        "no member re-acquired after arriving; the suppression is permanent"
    );
}

fn enemy_of(simulation: &Simulation<RotwkRules>, ids: Ids) -> Entity {
    simulation
        .state()
        .store(ids.owner)
        .and_then(|store| {
            store
                .iter()
                .find(|(_, owner)| owner.0 == OTHER)
                .map(|(entity, _)| entity)
        })
        .expect("the enemy is in the world")
}

#[test]
fn ordering_many_hordes_to_attack_gives_each_horde_the_order_and_not_every_member() {
    let (combat, bow, armour) = combat_data();
    let map = flat_map(200, 200);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let formation = square_formation();
    let count = u32::try_from(formation.len()).unwrap();
    let attacker = |origin: Vec2| HordeSpawn {
        formation: 0,
        origin,
        facing: Facing::EAST,
        member: Spawn {
            health: Some(200.0),
            weapon: Some(bow),
            armour: Some(armour),
            ..spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER)
        },
        count,
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let rules = RotwkRules::new(
        passability,
        vec![armed_spawn(
            1400.0,
            200.0,
            OTHER,
            Some(bow),
            Some(armour),
            5000.0,
        )],
    )
    .with_combat(combat)
    .with_hordes(
        vec![formation],
        vec![
            attacker(Vec2::new(200.0, 200.0)),
            attacker(Vec2::new(200.0, 400.0)),
        ],
    );
    let mut simulation = Simulation::new(13, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let hordes: Vec<Entity> = simulation
        .state()
        .store(ids.horde)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    assert_eq!(hordes.len(), 2, "the fixture's own setup");
    let members: Vec<Entity> = hordes
        .iter()
        .filter_map(|horde| simulation.state().get(ids.horde, *horde))
        .flat_map(|horde| horde.members.iter().copied())
        .collect();
    assert_eq!(members.len(), 18, "the fixture's own setup");
    let enemy = enemy_of(&simulation, ids);

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: enemy })
        .unwrap();
    simulation.tick().unwrap();

    let ordered_hordes = hordes
        .iter()
        .filter(|horde| simulation.state().contains(ids.attack_order, **horde))
        .count();
    let ordered_members = members
        .iter()
        .filter(|member| simulation.state().contains(ids.attack_order, **member))
        .count();
    assert_eq!(
        ordered_hordes, 2,
        "both far hordes should hold the standing order themselves"
    );
    assert_eq!(
        ordered_members, 0,
        "no member should be ordered individually yet - that is the bug this round fixes"
    );
}

#[test]
fn an_attack_order_on_a_horde_far_from_its_target_keeps_the_order_on_the_horde() {
    let (mut simulation, ids) = armed_horde_sim(Vec2::new(200.0, 200.0), Vec2::new(600.0, 200.0));
    let members = members_of(&simulation, ids);
    let enemy = enemy_of(&simulation, ids);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: enemy })
        .unwrap();
    simulation.tick().unwrap();

    let horde = horde_entity(&simulation, ids).unwrap();
    assert!(
        simulation.state().contains(ids.attack_order, horde),
        "the horde should hold the standing order while it is still closing"
    );
    assert!(
        members
            .iter()
            .all(|member| !simulation.state().contains(ids.attack_order, *member)),
        "no member should be released yet - the horde has not closed the distance"
    );
}

#[test]
fn an_attack_order_on_a_horde_drops_its_route_when_the_target_dies_before_it_closes() {
    let (combat, bow, armour) = combat_data();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let formation = square_formation();
    let count = u32::try_from(formation.len()).unwrap();
    let spawn = HordeSpawn {
        formation: 0,
        origin: Vec2::new(200.0, 200.0),
        facing: Facing::EAST,
        member: Spawn {
            health: Some(200.0),
            weapon: Some(bow),
            armour: Some(armour),
            ..spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER)
        },
        count,
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let executioner = PlayerId::new(2);
    let rules = RotwkRules::new(
        passability,
        vec![
            armed_spawn(600.0, 200.0, OTHER, None, Some(armour), 30.0),
            armed_spawn(605.0, 200.0, executioner, Some(bow), Some(armour), 200.0),
        ],
    )
    .with_combat(combat)
    .with_hordes(vec![formation], vec![spawn]);
    let mut simulation = Simulation::new(17, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let members = members_of(&simulation, ids);
    let enemy = enemy_of(&simulation, ids);

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members,
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: enemy })
        .unwrap();
    simulation.tick().unwrap();
    let horde = horde_entity(&simulation, ids).unwrap();
    assert!(
        simulation.state().contains(ids.attack_order, horde),
        "the horde should be closing before the target dies"
    );
    assert!(
        simulation.state().contains(ids.path, horde),
        "and it should have a route towards it"
    );

    for _ in 0..200 {
        simulation.tick().unwrap();
        if !simulation.state().is_live(enemy) {
            break;
        }
    }
    assert!(
        !simulation.state().is_live(enemy),
        "the fixture's own setup - the executioner should have killed it by now"
    );
    assert!(
        !simulation.state().contains(ids.attack_order, horde),
        "a dead target must end the horde's own standing order"
    );
    assert!(
        !simulation.state().contains(ids.path, horde),
        "and its route towards that target - marching the rest of the way to a corpse \
             is pointless"
    );
}

#[test]
fn an_attack_order_releases_its_members_once_the_horde_is_within_reach() {
    let (mut simulation, ids) = armed_horde_sim(Vec2::new(200.0, 200.0), Vec2::new(350.0, 200.0));
    let members = members_of(&simulation, ids);
    let enemy = enemy_of(&simulation, ids);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: enemy })
        .unwrap();
    simulation.tick().unwrap();

    let horde = horde_entity(&simulation, ids).unwrap();
    assert!(
        simulation.state().contains(ids.attack_order, horde),
        "the horde's own standing order should persist - it is still fighting the same \
             target, just no longer travelling to it"
    );
    assert!(
        !simulation.state().contains(ids.path, horde),
        "close enough that the horde itself should have stopped travelling"
    );
    let ordered = members
        .iter()
        .filter(|member| simulation.state().contains(ids.attack_order, **member))
        .count();
    assert_eq!(
        ordered, 0,
        "a member must never hold its own AttackOrder under the horde-target model"
    );
    let engaged = members
        .iter()
        .filter(|member| {
            simulation
                .state()
                .get(ids.armed, **member)
                .is_some_and(|armed| armed.target == Some(enemy))
        })
        .count();
    assert_eq!(
        engaged,
        members.len(),
        "every member should be engaging the enemy directly - already in range at this \
             distance"
    );
}

#[test]
fn with_formations_off_an_attack_order_still_orders_every_member_individually() {
    let (mut simulation, ids) = armed_horde_sim(Vec2::new(200.0, 200.0), Vec2::new(900.0, 200.0));
    let members = members_of(&simulation, ids);
    let enemy = enemy_of(&simulation, ids);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetMovementModes(Modes {
                formation: FormationMode::Off,
                ..Modes::default()
            }),
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: enemy })
        .unwrap();
    simulation.tick().unwrap();

    let horde = horde_entity(&simulation, ids).unwrap();
    assert!(
        !simulation.state().contains(ids.attack_order, horde),
        "formations off must never install a group order on the horde"
    );
    let ordered = members
        .iter()
        .filter(|member| simulation.state().contains(ids.attack_order, **member))
        .count();
    assert_eq!(
        ordered,
        members.len(),
        "with formations off every member is still ordered individually, same as before this round"
    );
}

#[test]
fn toggling_formations_off_mid_order_releases_a_grouped_horde_immediately() {
    let (mut simulation, ids) = armed_horde_sim(Vec2::new(200.0, 200.0), Vec2::new(900.0, 200.0));
    let members = members_of(&simulation, ids);
    let enemy = enemy_of(&simulation, ids);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: enemy })
        .unwrap();
    simulation.tick().unwrap();
    let horde = horde_entity(&simulation, ids).unwrap();
    assert!(
        simulation.state().contains(ids.attack_order, horde),
        "still grouped and closing before the mode switch"
    );

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetMovementModes(Modes {
                formation: FormationMode::Off,
                ..Modes::default()
            }),
        )
        .unwrap();
    simulation.tick().unwrap();

    assert!(
        !simulation.state().contains(ids.attack_order, horde),
        "formations off must clear a standing group order rather than leave an \
             invisible horde marching alone"
    );
    assert!(
        !simulation.state().contains(ids.path, horde),
        "and drop its own group-closing route with it"
    );
    let ordered = members
        .iter()
        .filter(|member| simulation.state().contains(ids.attack_order, **member))
        .count();
    assert_eq!(
        ordered, 0,
        "a member must never hold its own AttackOrder, on or off"
    );
}

#[test]
fn a_grouped_attack_order_replays_to_the_same_hash_sequence() {
    let run = || {
        let (mut simulation, ids) =
            armed_horde_sim(Vec2::new(200.0, 200.0), Vec2::new(700.0, 200.0));
        let members = members_of(&simulation, ids);
        let enemy = enemy_of(&simulation, ids);
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: members.clone(),
                },
            )
            .unwrap();
        simulation
            .submit(PLAYER, RotwkCommand::AttackObject { target: enemy })
            .unwrap();
        let horde = horde_entity(&simulation, ids).unwrap();
        let mut hashes = Vec::new();
        let mut saw_group_phase = false;
        let mut saw_engagement = false;
        for _ in 0..400 {
            simulation.tick().unwrap();
            hashes.push(simulation.state_hash());
            saw_group_phase |= simulation.state().contains(ids.attack_order, horde);
            saw_engagement |= members.iter().any(|member| {
                simulation
                    .state()
                    .get(ids.armed, *member)
                    .is_some_and(|armed| armed.target == Some(enemy))
            });
        }
        assert!(saw_group_phase, "the horde never held the group order");
        assert!(saw_engagement, "no member ever engaged the enemy");
        hashes
    };
    assert_eq!(run(), run());
}

#[test]
fn an_idle_horde_acquires_its_attacker_immediately_when_hit() {
    let (mut simulation, ids) = armed_horde_sim(Vec2::new(200.0, 200.0), Vec2::new(350.0, 200.0));
    let enemy = enemy_of(&simulation, ids);
    let horde = horde_entity(&simulation, ids).unwrap();
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![enemy],
            },
        )
        .unwrap();
    let members = members_of(&simulation, ids);
    simulation
        .submit(OTHER, RotwkCommand::AttackObject { target: members[0] })
        .unwrap();
    assert!(
        !simulation.state().contains(ids.horde_target, horde),
        "the fixture's own setup - the horde must start with nothing"
    );

    let mut acquired_at = None;
    for tick in 0..20 {
        simulation.tick().unwrap();
        if simulation
            .state()
            .get(ids.horde_target, horde)
            .is_some_and(|hordetarget| hordetarget.target == Some(enemy))
        {
            acquired_at = Some(tick);
            break;
        }
    }
    assert_eq!(
        acquired_at,
        Some(0),
        "the horde should have acquired its attacker on the very first tick it was hit"
    );
}

#[test]
fn a_horde_under_a_plain_move_order_does_not_fight_back_when_attacked() {
    let (mut simulation, ids) = armed_horde_sim(Vec2::new(200.0, 200.0), Vec2::new(350.0, 200.0));
    let enemy = enemy_of(&simulation, ids);
    let horde = horde_entity(&simulation, ids).unwrap();
    let members = members_of(&simulation, ids);
    let health_before: Vec<f32> = members
        .iter()
        .map(|member| health_of(&simulation, ids, *member).unwrap_or(0.0))
        .collect();
    order_horde(
        &mut simulation,
        ids,
        Modes::default(),
        Vec2::new(200.0, 700.0),
    );
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![enemy],
            },
        )
        .unwrap();
    simulation
        .submit(OTHER, RotwkCommand::AttackObject { target: members[0] })
        .unwrap();
    for _ in 0..20 {
        simulation.tick().unwrap();
    }
    let took_damage = members
        .iter()
        .zip(health_before)
        .any(|(member, before)| health_of(&simulation, ids, *member).unwrap_or(before) < before);
    assert!(
        took_damage,
        "the fixture's own setup - the enemy should have landed at least one hit \
             (TestBow's own comment: a shot lands on frame 0)"
    );
    assert!(
        !simulation.state().contains(ids.horde_target, horde),
        "a horde under a plain move order must not fight back when attacked"
    );
}

#[test]
fn melee_members_engage_only_what_is_locally_adjacent() {
    let (combat, _bow, armour) = combat_data();
    let sword = combat.weapons.id("TestSword").unwrap();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let formation = square_formation();
    let count = u32::try_from(formation.len()).unwrap();
    let spawn = HordeSpawn {
        formation: 0,
        origin: Vec2::new(200.0, 200.0),
        facing: Facing::EAST,
        member: Spawn {
            health: Some(200.0),
            weapon: Some(sword),
            armour: Some(armour),
            ..spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER)
        },
        count,
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let enemy_pos = Vec2::new(260.0, 200.0);
    let rules = RotwkRules::new(
        passability,
        vec![armed_spawn(
            enemy_pos.x,
            enemy_pos.y,
            OTHER,
            Some(sword),
            Some(armour),
            200.0,
        )],
    )
    .with_combat(combat)
    .with_hordes(vec![formation], vec![spawn]);
    let mut simulation = Simulation::new(23, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let members = members_of(&simulation, ids);
    let enemy = enemy_of(&simulation, ids);
    let horde = horde_entity(&simulation, ids).unwrap();

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: enemy })
        .unwrap();
    simulation.tick().unwrap();

    assert!(
        !simulation.state().contains(ids.path, horde),
        "the fixture's own setup - already within reach, no closing needed"
    );
    let front = members
        .iter()
        .find(|member| {
            simulation
                .state()
                .get(ids.body, **member)
                .is_some_and(|body| body.0.position.x > 210.0)
        })
        .copied()
        .expect("the fixture's own square formation has a front rank");
    let rear = members
        .iter()
        .find(|member| {
            simulation
                .state()
                .get(ids.body, **member)
                .is_some_and(|body| body.0.position.x < 190.0)
        })
        .copied()
        .expect("the fixture's own square formation has a rear rank");
    assert_eq!(
        simulation
            .state()
            .get(ids.armed, front)
            .and_then(|armed| armed.target),
        Some(enemy),
        "the front rank should be engaging - it is within its own local reach"
    );
    assert_eq!(
        simulation
            .state()
            .get(ids.armed, rear)
            .and_then(|armed| armed.target),
        None,
        "the rear rank should not be engaging - nothing of the enemy is locally \
             adjacent to it yet"
    );

    let rear_start = position(&simulation, ids, rear);
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    let rear_end = position(&simulation, ids, rear);
    let dx = rear_end.x - rear_start.x;
    let dy = rear_end.y - rear_start.y;
    let drift = (dx * dx + dy * dy).sqrt();
    assert!(
        drift < 5.0,
        "the rear rank should hold its slot, not walk toward the enemy: drifted \
             {drift:.1} world units"
    );
}

#[test]
fn attack_move_travels_to_an_empty_destination_and_finishes() {
    let (mut simulation, ids) = horde_sim(Vec2::new(150.0, 150.0), square_formation());
    let members = members_of(&simulation, ids);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members,
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::AttackMove {
                x: 450.0,
                y: 150.0,
                queue: false,
            },
        )
        .unwrap();
    let horde = horde_entity(&simulation, ids).unwrap();
    for _ in 0..300 {
        simulation.tick().unwrap();
        if !simulation.state().contains(ids.attack_move_order, horde) {
            break;
        }
    }
    assert!(
        !simulation.state().contains(ids.attack_move_order, horde),
        "the fixture's own setup - a 300-unit march should finish well within 300 ticks"
    );
    let arrived = position(&simulation, ids, horde);
    assert!(
        (arrived.x - 450.0).abs() < 40.0,
        "the horde should have actually arrived near the destination: ended at {arrived:?}"
    );
}

#[test]
fn a_plain_move_during_an_attack_move_ends_it_instead_of_resuming() {
    let (mut simulation, ids) = horde_sim(Vec2::new(150.0, 150.0), square_formation());
    let members = members_of(&simulation, ids);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members,
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::AttackMove {
                x: 450.0,
                y: 150.0,
                queue: false,
            },
        )
        .unwrap();
    let horde = horde_entity(&simulation, ids).unwrap();
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    assert!(
        simulation.state().contains(ids.attack_move_order, horde),
        "the fixture's own setup - 10 ticks of a 300-unit march must not have finished yet"
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 150.0,
                y: 400.0,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        !simulation.state().contains(ids.attack_move_order, horde),
        "a plain move order must clear a standing attack-move, not sit alongside it"
    );
    for _ in 0..300 {
        simulation.tick().unwrap();
    }
    let arrived = position(&simulation, ids, horde);
    assert!(
        (arrived.x - 150.0).abs() < 40.0 && (arrived.y - 400.0).abs() < 40.0,
        "the horde should have stayed at its new destination, not drifted back toward the old \
             attack-move goal (450, 150): ended at {arrived:?}"
    );
    assert!(
        !simulation.state().contains(ids.attack_move_order, horde),
        "no attack-move should have resumed after the plain move finished"
    );
}

#[test]
fn a_loose_unit_attack_moves_fighting_on_the_way() {
    let (combat, bow, armour) = combat_data();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(
        passability,
        vec![
            armed_spawn(200.0, 200.0, PLAYER, Some(bow), Some(armour), 200.0),
            armed_spawn(220.0, 260.0, OTHER, None, Some(armour), 5.0),
        ],
    )
    .with_combat(combat);
    let mut simulation = Simulation::new(29, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let enemy = enemy_of(&simulation, ids);
    let unit = simulation
        .state()
        .store(ids.owner)
        .and_then(|store| {
            store
                .iter()
                .find(|(_, owner)| owner.0 == PLAYER)
                .map(|(entity, _)| entity)
        })
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::AttackMove {
                x: 200.0,
                y: 700.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..1200 {
        simulation.tick().unwrap();
    }
    assert!(
        !simulation.state().is_live(enemy),
        "the enemy beside the route was fought"
    );
    let at = position(&simulation, ids, unit);
    assert!(
        at.minus(Vec2::new(200.0, 700.0)).length() < 20.0,
        "walked on to the destination: at {at:?}"
    );
    assert!(
        !simulation.state().contains(ids.attack_move_order, unit),
        "the order ends at the destination"
    );
}

#[test]
fn attack_move_fights_something_on_the_way_then_resumes_toward_the_destination() {
    let (combat, bow, armour) = combat_data();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let formation = square_formation();
    let count = u32::try_from(formation.len()).unwrap();
    let spawn = HordeSpawn {
        formation: 0,
        origin: Vec2::new(200.0, 200.0),
        facing: Facing::EAST,
        member: Spawn {
            health: Some(200.0),
            weapon: Some(bow),
            armour: Some(armour),
            ..spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER)
        },
        count,
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let rules = RotwkRules::new(
        passability,
        vec![armed_spawn(220.0, 200.0, OTHER, None, Some(armour), 5.0)],
    )
    .with_combat(combat)
    .with_hordes(vec![formation], vec![spawn]);
    let mut simulation = Simulation::new(29, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let members = members_of(&simulation, ids);
    let enemy = enemy_of(&simulation, ids);
    let horde = horde_entity(&simulation, ids).unwrap();

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members,
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::AttackMove {
                x: 200.0,
                y: 700.0,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(
        simulation
            .state()
            .get(ids.horde_target, horde)
            .and_then(|hordetarget| hordetarget.target),
        Some(enemy),
        "attack-move should have acquired the enemy immediately - already in reach"
    );
    assert!(
        simulation.state().contains(ids.attack_move_order, horde),
        "the attack-move order itself should survive the fight, not be replaced by it"
    );

    for _ in 0..100 {
        simulation.tick().unwrap();
        if !simulation.state().is_live(enemy) {
            break;
        }
    }
    assert!(
        !simulation.state().is_live(enemy),
        "the fixture's own setup - 5 hit points should not survive 100 ticks of fire"
    );

    let mut resumed = false;
    for _ in 0..20 {
        simulation.tick().unwrap();
        if simulation.state().contains(ids.path, horde) {
            resumed = true;
            break;
        }
    }
    assert!(
        resumed,
        "the horde should resume travelling toward its own destination once the fight ends"
    );
    assert!(
        simulation.state().contains(ids.attack_move_order, horde),
        "the order is not finished - the horde has not reached (200, 700) yet"
    );
}

#[test]
fn a_move_order_on_a_horde_drops_its_members_orders_and_routes() {
    let (mut simulation, ids) = armed_horde_sim(Vec2::new(200.0, 200.0), Vec2::new(400.0, 200.0));
    let members = members_of(&simulation, ids);
    let enemy = simulation
        .state()
        .store(ids.owner)
        .and_then(|store| {
            store
                .iter()
                .find(|(_, owner)| owner.0 == OTHER)
                .map(|(entity, _)| entity)
        })
        .expect("the enemy is in the world");

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: enemy })
        .unwrap();
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    let horde = horde_entity(&simulation, ids).unwrap();
    assert!(
        simulation.state().contains(ids.attack_order, horde),
        "the horde's own standing order should have taken hold"
    );
    let engaged = members
        .iter()
        .filter(|member| {
            simulation
                .state()
                .get(ids.armed, **member)
                .is_some_and(|armed| armed.target == Some(enemy))
        })
        .count();
    assert!(engaged > 0, "no member engaged the enemy");

    order_horde(
        &mut simulation,
        ids,
        Modes::default(),
        Vec2::new(60.0, 200.0),
    );
    simulation.tick().unwrap();
    assert!(
        !simulation.state().contains(ids.attack_order, horde),
        "a horde move order must clear its own standing attack order"
    );
    for member in &members {
        assert!(
            !simulation.state().contains(ids.attack_order, *member),
            "a member must never hold its own attack order under the horde-target model"
        );
        assert!(
            !simulation.state().contains(ids.path, *member),
            "a member must never hold its own route under the horde-target model"
        );
        assert!(
            simulation
                .state()
                .get(ids.armed, *member)
                .is_none_or(|armed| armed.target != Some(enemy)),
            "a member kept engaging the enemy through a horde move order"
        );
    }
}

#[test]
fn a_horde_ordered_somewhere_arrives_in_formation_rather_than_in_a_pile() {
    let (mut simulation, ids) = horde_sim(Vec2::new(150.0, 150.0), square_formation());
    let members = members_of(&simulation, ids);
    assert_eq!(members.len(), 9);
    let spawned = extent(&simulation, ids, &members);
    assert!(
        (spawned.0 - 40.0).abs() < 1e-3 && (spawned.1 - 40.0).abs() < 1e-3,
        "the horde must spawn already formed, got {spawned:?}"
    );

    order_horde(
        &mut simulation,
        ids,
        Modes::default(),
        Vec2::new(450.0, 150.0),
    );
    for _ in 0..400 {
        simulation.tick().unwrap();
    }

    let horde = horde_entity(&simulation, ids).expect("the horde is still alive");
    let body = simulation.state().get(ids.body, horde).unwrap().0;
    assert!(
        body.position.minus(Vec2::new(450.0, 150.0)).length() < 15.0,
        "the horde did not arrive, it is at {:?}",
        body.position
    );
    let arrived = extent(&simulation, ids, &members);
    assert!(
        (arrived.0 - 40.0).abs() < 8.0 && (arrived.1 - 40.0).abs() < 8.0,
        "the formation collapsed on the way: spawned {spawned:?}, arrived {arrived:?}"
    );
    let mut occupied: Vec<u16> = Vec::new();
    for member in &members {
        let slot = simulation.state().get(ids.member, *member).unwrap().slot;
        occupied.push(slot.expect("a member without a slot"));
    }
    occupied.sort_unstable();
    occupied.dedup();
    assert_eq!(occupied.len(), 9, "two members share a slot");
}

#[test]
fn a_straggler_behind_a_chokepoint_is_rescued_back_to_the_horde() {
    let map = wall_map(96, 96, 48, 46..49);
    let origin = Vec2::new(300.0, 475.0);
    let goal = Vec2::new(700.0, 475.0);
    let (mut simulation, ids) = horde_sim_over(origin, square_formation(), map);
    let members = members_of(&simulation, ids);
    assert_eq!(members.len(), 9);

    order_horde(&mut simulation, ids, Modes::default(), goal);
    for _ in 0..3000 {
        simulation.tick().unwrap();
    }

    let horde = horde_entity(&simulation, ids).expect("the horde is still alive");
    let body = simulation.state().get(ids.body, horde).unwrap().0;
    assert!(
        body.position.minus(goal).length() < 60.0,
        "the horde did not even approach the goal, it is at {:?}",
        body.position
    );
    for member in &members {
        let position = simulation
            .state()
            .get(ids.body, *member)
            .unwrap()
            .0
            .position;
        assert!(
            position.x > 480.0,
            "member at {position:?} never crossed the wall at x=480"
        );
    }
    let arrived = extent(&simulation, ids, &members);
    assert!(
        arrived.0 < 60.0 && arrived.1 < 60.0,
        "the formation is still spread out after the rescue: {arrived:?}"
    );
}

#[test]
fn a_rescued_straggler_keeps_its_marching_speed() {
    let map = wall_map(96, 96, 48, 5..8);
    let (mut simulation, ids) = horde_sim_over(Vec2::new(560.0, 475.0), square_formation(), map);
    let members = members_of(&simulation, ids);
    order_horde(
        &mut simulation,
        ids,
        Modes::default(),
        Vec2::new(300.0, 475.0),
    );
    let mut rescued_while_moving = 0;
    for _ in 0..3000 {
        let before: Vec<(f32, bool)> = members
            .iter()
            .map(|member| {
                let speed = simulation.state().get(ids.body, *member).unwrap().0.speed;
                (speed, simulation.state().contains(ids.path, *member))
            })
            .collect();
        simulation.tick().unwrap();
        for (member, (speed, had_route)) in members.iter().zip(before) {
            if had_route || speed <= 0.0 || !simulation.state().contains(ids.path, *member) {
                continue;
            }
            rescued_while_moving += 1;
            let now = simulation.state().get(ids.body, *member).unwrap().0.speed;
            assert_eq!(
                now, speed,
                "a rescued member restarted its route at {now}, not {speed}"
            );
        }
    }
    assert!(
        rescued_while_moving > 0,
        "no member was rescued while moving"
    );
}

#[test]
fn many_simultaneous_stragglers_stay_within_the_tick_wide_rescue_budget() {
    let map = wall_map(96, 96, 48, 46..49);
    let origins: Vec<Vec2> = (0..14i16)
        .map(|index| {
            Vec2::new(
                300.0 + f32::from(index) * 6.0,
                470.0 + f32::from(index) * 2.0,
            )
        })
        .collect();
    let (mut simulation, ids) = many_hordes_sim_over(&origins, wide_line_formation(41), map);
    let hordes: Vec<Entity> = simulation
        .state()
        .store(ids.horde)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    assert_eq!(
        hordes.len(),
        origins.len(),
        "the fixture must spawn one horde per origin"
    );
    simulation
        .submit(PLAYER, RotwkCommand::SetMovementModes(Modes::default()))
        .unwrap();
    for horde in &hordes {
        let members = simulation
            .state()
            .get(ids.horde, *horde)
            .map(|h| h.members.clone())
            .unwrap_or_default();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: members,
                },
            )
            .unwrap();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::MoveTo {
                    x: 700.0,
                    y: 475.0,
                    queue: false,
                },
            )
            .unwrap();
    }
    let mut peak_spent = 0u32;
    for _ in 0..3000 {
        simulation.tick().unwrap();
        peak_spent = peak_spent.max(last_straggler_rescue_spent());
    }
    let cap = STRAGGLER_RESCUE_BUDGET_PER_TICK + PATH_NODE_BUDGET;
    assert!(
        peak_spent <= cap,
        "one tick's straggler rescues spent {peak_spent}, over the {cap}-node tick-wide cap"
    );
}

#[test]
fn separation_keeps_two_crossing_hordes_off_the_same_ground() {
    let west = Vec2::new(150.0, 150.0);
    let east = Vec2::new(450.0, 150.0);
    let build = || {
        let map = flat_map(96, 96);
        let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
        let formation = square_formation();
        let spawn_of = |origin: Vec2| HordeSpawn {
            formation: 0,
            origin,
            facing: Facing::EAST,
            member: spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER),
            count: u32::try_from(formation.len()).unwrap(),
            vision: crate::shroud::HORDE_SHROUD_RANGE,
        };
        let spawns = vec![spawn_of(west), spawn_of(east)];
        let rules = RotwkRules::new(passability, Vec::new())
            .with_hordes(vec![formation.clone(), formation], spawns);
        let simulation = Simulation::new(5, rules).unwrap();
        let ids = simulation.rules().ids().unwrap();
        (simulation, ids)
    };

    let closest_approach = |avoidance: AvoidanceMode| {
        let (mut simulation, ids) = build();
        let hordes: Vec<Entity> = simulation
            .state()
            .store(ids.horde)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        assert_eq!(hordes.len(), 2, "the fixture must spawn exactly two hordes");
        let modes = Modes {
            avoidance,
            ..Modes::default()
        };
        simulation
            .submit(PLAYER, RotwkCommand::SetMovementModes(modes))
            .unwrap();
        let past = Vec2::new(80.0, 0.0);
        for (horde, goal) in [(hordes[0], east.plus(past)), (hordes[1], west.minus(past))] {
            let members = simulation
                .state()
                .get(ids.horde, horde)
                .map(|h| h.members.clone())
                .unwrap_or_default();
            simulation
                .submit(
                    PLAYER,
                    RotwkCommand::SetSelection {
                        replace: true,
                        units: members,
                    },
                )
                .unwrap();
            simulation
                .submit(
                    PLAYER,
                    RotwkCommand::MoveTo {
                        x: goal.x,
                        y: goal.y,
                        queue: false,
                    },
                )
                .unwrap();
        }
        let mut closest = f32::MAX;
        for _ in 0..300 {
            simulation.tick().unwrap();
            let bodies: Vec<Vec2> = hordes
                .iter()
                .filter_map(|h| simulation.state().get(ids.body, *h))
                .map(|body| body.0.position)
                .collect();
            if let [a, b] = bodies.as_slice() {
                closest = closest.min(a.minus(*b).length());
            }
        }
        closest
    };

    let off = closest_approach(AvoidanceMode::Off);
    let separated = closest_approach(AvoidanceMode::Separation);
    assert!(
        off < 20.0,
        "the fixture's own premise is that the hordes cross without avoidance - \
             closest approach was {off}, they never actually met"
    );
    assert!(
        separated > off,
        "separation did not increase the closest approach: off={off}, separation={separated}"
    );
}

#[test]
fn a_replay_with_separation_avoidance_active_produces_the_same_hash_sequence() {
    let run = || {
        let map = flat_map(96, 96);
        let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
        let formation = square_formation();
        let west = Vec2::new(150.0, 150.0);
        let east = Vec2::new(450.0, 150.0);
        let spawn_of = |origin: Vec2| HordeSpawn {
            formation: 0,
            origin,
            facing: Facing::EAST,
            member: spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER),
            count: u32::try_from(formation.len()).unwrap(),
            vision: crate::shroud::HORDE_SHROUD_RANGE,
        };
        let spawns = vec![spawn_of(west), spawn_of(east)];
        let rules = RotwkRules::new(passability, Vec::new())
            .with_hordes(vec![formation.clone(), formation], spawns);
        let mut simulation = Simulation::new(5, rules).unwrap();
        let ids = simulation.rules().ids().unwrap();
        let hordes: Vec<Entity> = simulation
            .state()
            .store(ids.horde)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetMovementModes(Modes {
                    avoidance: AvoidanceMode::Separation,
                    ..Modes::default()
                }),
            )
            .unwrap();
        for (horde, goal) in [(hordes[0], east), (hordes[1], west)] {
            let members = simulation
                .state()
                .get(ids.horde, horde)
                .map(|h| h.members.clone())
                .unwrap_or_default();
            simulation
                .submit(
                    PLAYER,
                    RotwkCommand::SetSelection {
                        replace: true,
                        units: members,
                    },
                )
                .unwrap();
            simulation
                .submit(
                    PLAYER,
                    RotwkCommand::MoveTo {
                        x: goal.x,
                        y: goal.y,
                        queue: false,
                    },
                )
                .unwrap();
        }
        let mut hashes = Vec::new();
        for _ in 0..300 {
            simulation.tick().unwrap();
            hashes.push(simulation.state_hash());
        }
        hashes
    };
    let first = run();
    assert_eq!(first, run());
    let mut distinct = first.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert!(
        distinct.len() > 100,
        "only {} distinct hashes over 300 ticks with avoidance active",
        distinct.len()
    );
}

#[test]
fn a_replay_of_a_marching_horde_produces_the_same_hash_sequence() {
    let run = || {
        let (mut simulation, ids) = horde_sim(Vec2::new(150.0, 150.0), square_formation());
        order_horde(
            &mut simulation,
            ids,
            Modes::default(),
            Vec2::new(400.0, 260.0),
        );
        let mut hashes = Vec::new();
        for _ in 0..300 {
            simulation.tick().unwrap();
            hashes.push(simulation.state_hash());
        }
        hashes
    };
    let first = run();
    assert_eq!(first, run());
    let mut distinct = first.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert!(
        distinct.len() > 100,
        "only {} distinct hashes over 300 ticks of a march",
        distinct.len()
    );
}

#[test]
fn opposing_hordes_still_meet_in_melee_with_separation_active() {
    let (combat, _bow, armour) = combat_data();
    let sword = combat
        .weapons
        .id("TestSword")
        .expect("combat_data always registers TestSword");
    let engaged_after_meeting = |avoidance: AvoidanceMode| {
        let map = flat_map(96, 96);
        let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
        let formation = square_formation();
        let west = Vec2::new(150.0, 150.0);
        let east = Vec2::new(350.0, 150.0);
        let spawn_of = |origin: Vec2, facing: Facing, owner: PlayerId| HordeSpawn {
            formation: 0,
            origin,
            facing,
            member: Spawn {
                health: Some(200.0),
                weapon: Some(sword),
                armour: Some(armour),
                ..spawn_at(0.0, 0.0, KindOf::SELECTABLE, owner)
            },
            count: u32::try_from(formation.len()).unwrap(),
            vision: crate::shroud::HORDE_SHROUD_RANGE,
        };
        let west_facing = Facing::from_vector(Vec2::new(-1.0, 0.0)).unwrap();
        let spawns = vec![
            spawn_of(west, Facing::EAST, PLAYER),
            spawn_of(east, west_facing, OTHER),
        ];
        let rules = RotwkRules::new(passability, Vec::new())
            .with_combat(combat.clone())
            .with_hordes(vec![formation.clone(), formation], spawns);
        let mut simulation = Simulation::new(5, rules).unwrap();
        let ids = simulation.rules().ids().unwrap();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetMovementModes(Modes {
                    avoidance,
                    ..Modes::default()
                }),
            )
            .unwrap();
        let hordes: Vec<Entity> = simulation
            .state()
            .store(ids.horde)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        assert_eq!(hordes.len(), 2, "the fixture must spawn exactly two hordes");
        let mut all_members = Vec::new();
        for (horde, goal) in [(hordes[0], east), (hordes[1], west)] {
            let members = simulation
                .state()
                .get(ids.horde, horde)
                .map(|h| h.members.clone())
                .unwrap_or_default();
            simulation
                .submit(
                    PLAYER,
                    RotwkCommand::SetSelection {
                        replace: true,
                        units: members.clone(),
                    },
                )
                .unwrap();
            simulation
                .submit(
                    PLAYER,
                    RotwkCommand::MoveTo {
                        x: goal.x,
                        y: goal.y,
                        queue: false,
                    },
                )
                .unwrap();
            all_members.extend(members);
        }
        let mut peak_engaged = 0;
        for _ in 0..400 {
            simulation.tick().unwrap();
            peak_engaged = peak_engaged.max(engaged(&simulation, ids, &all_members));
        }
        peak_engaged
    };
    let off = engaged_after_meeting(AvoidanceMode::Off);
    let separated = engaged_after_meeting(AvoidanceMode::Separation);
    assert!(
        off > 0,
        "the fixture's own premise: they must meet with avoidance off"
    );
    assert!(
        separated > 0,
        "separation stopped the hordes from ever reaching melee range - \
             it pushed their bodies apart faster than their front ranks could close"
    );
}

#[test]
fn melee_contact_loosens_a_hordes_block_without_spreading_it_wide() {
    let (combat, _bow, armour) = combat_data();
    let sword = combat
        .weapons
        .id("TestSword")
        .expect("combat_data always registers TestSword");
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let formation = square_formation();
    let west = Vec2::new(150.0, 150.0);
    let east = Vec2::new(350.0, 160.0);
    let spawn_of = |origin: Vec2, facing: Facing, owner: PlayerId| HordeSpawn {
        formation: 0,
        origin,
        facing,
        member: Spawn {
            health: Some(200.0),
            weapon: Some(sword),
            armour: Some(armour),
            ..spawn_at(0.0, 0.0, KindOf::SELECTABLE, owner)
        },
        count: u32::try_from(formation.len()).unwrap(),
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let west_facing = Facing::from_vector(Vec2::new(-1.0, 0.0)).unwrap();
    let spawns = vec![
        spawn_of(west, Facing::EAST, PLAYER),
        spawn_of(east, west_facing, OTHER),
    ];
    let rules = RotwkRules::new(passability, Vec::new())
        .with_combat(combat)
        .with_hordes(vec![formation.clone(), formation], spawns);
    let mut simulation = Simulation::new(7, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let horde_ids = RotwkRules::horde_ids(ids);
    let hordes: Vec<Entity> = simulation
        .state()
        .store(ids.horde)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    assert_eq!(hordes.len(), 2, "the fixture must spawn exactly two hordes");
    let watched = hordes[0];
    let (before_x, before_y) = crate::horde::footprint(simulation.state(), horde_ids, watched);
    let before_span = before_x.max(before_y);
    assert!(
        before_span > 0.0,
        "the fixture's own premise: a placed square formation has a nonzero span"
    );

    let mut all_members = Vec::new();
    for (horde, goal) in [(hordes[0], east), (hordes[1], west)] {
        let members = simulation
            .state()
            .get(ids.horde, horde)
            .map(|h| h.members.clone())
            .unwrap_or_default();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: members.clone(),
                },
            )
            .unwrap();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::MoveTo {
                    x: goal.x,
                    y: goal.y,
                    queue: false,
                },
            )
            .unwrap();
        all_members.extend(members);
    }

    let mut peak_engaged = 0;
    let mut peak_ratio = 0.0f32;
    for _ in 0..400 {
        simulation.tick().unwrap();
        let now_engaged = engaged(&simulation, ids, &all_members);
        peak_engaged = peak_engaged.max(now_engaged);
        if now_engaged > 0 {
            let (x, y) = crate::horde::footprint(simulation.state(), horde_ids, watched);
            peak_ratio = peak_ratio.max(x.max(y) / before_span);
        }
    }
    assert!(
        peak_engaged > 0,
        "the fixture's own premise: the two hordes must actually meet and fight"
    );
    println!(
        "melee span ratio: before {before_span:.1}, peak during melee {:.4}x",
        peak_ratio
    );
    assert!(
        peak_ratio <= 1.6,
        "the block should loosen, not spread wide: span grew to {peak_ratio:.2}x its \
             pre-contact size, past the owner's own ~1.5x reading of the reference pictures \
             plus margin"
    );
    assert!(
        peak_ratio >= 1.0,
        "the block should never end up tighter than its own pre-contact formation: \
             ratio was {peak_ratio:.2}"
    );
}

#[test]
fn a_standing_horde_does_not_creep_while_nothing_orders_it() {
    let spawn = Vec2::new(150.0, 150.0);
    let (mut simulation, ids) = horde_sim(spawn, square_formation());
    let members = members_of(&simulation, ids);
    let start = centre_of(&simulation, ids, &members);
    for _ in 0..3000 {
        simulation.tick().unwrap();
    }
    let horde = horde_entity(&simulation, ids).expect("the horde is still alive");
    let anchor = simulation.state().get(ids.body, horde).unwrap().0.position;
    let end = centre_of(&simulation, ids, &members);
    assert!(
        anchor.minus(spawn).length() < 0.01,
        "the anchor crept to {anchor:?} over 3000 ticks from {spawn:?}"
    );
    assert!(
        end.minus(start).length() < 0.01,
        "the members crept from {start:?} to {end:?} over 3000 ticks"
    );
}

#[test]
fn ordering_a_horde_plans_one_path_and_not_one_per_member() {
    let (mut simulation, ids) = horde_sim(Vec2::new(150.0, 150.0), square_formation());
    order_horde(
        &mut simulation,
        ids,
        Modes::default(),
        Vec2::new(450.0, 150.0),
    );
    simulation.tick().unwrap();
    let paths = simulation
        .state()
        .store(ids.path)
        .map_or(0, mint_sim::ComponentStore::len);
    assert_eq!(paths, 1, "{paths} path(s) for a nine-member horde");
    let horde = horde_entity(&simulation, ids).unwrap();
    assert!(
        simulation.state().contains(ids.path, horde),
        "and it is the horde's"
    );
}

#[test]
fn with_formations_off_every_member_keeps_its_own_relative_position() {
    let (mut simulation, ids) = horde_sim(Vec2::new(150.0, 150.0), square_formation());
    let members = members_of(&simulation, ids);
    let modes = Modes {
        formation: FormationMode::Off,
        ..Modes::default()
    };
    let before = extent(&simulation, ids, &members);
    let starts: Vec<(Entity, Vec2)> = members
        .iter()
        .filter_map(|member| {
            Some((
                *member,
                simulation.state().get(ids.body, *member)?.0.position,
            ))
        })
        .collect();
    let goal = Vec2::new(450.0, 150.0);
    let reference = starts
        .iter()
        .map(|(_, at)| *at)
        .min_by(|a, b| {
            a.minus(goal)
                .length_squared()
                .total_cmp(&b.minus(goal).length_squared())
        })
        .unwrap();
    order_horde(&mut simulation, ids, modes, goal);
    simulation.tick().unwrap();
    let paths = simulation
        .state()
        .store(ids.path)
        .map_or(0, mint_sim::ComponentStore::len);
    assert_eq!(paths, 9, "one path per member, not {paths}");
    let horde = horde_entity(&simulation, ids).unwrap();
    assert!(
        !simulation.state().contains(ids.path, horde),
        "the horde itself must not be ordered when formations are off"
    );

    for _ in 0..400 {
        simulation.tick().unwrap();
    }
    let after = extent(&simulation, ids, &members);
    assert!(
        (after.0 - before.0).abs() < 10.0 && (after.1 - before.1).abs() < 10.0,
        "with formations off the nine keep their own relative spread \
             (movement.md §17.1's loose-group rule), started {before:?} ended {after:?}"
    );
    for (member, start) in &starts {
        let position = simulation
            .state()
            .get(ids.body, *member)
            .unwrap()
            .0
            .position;
        let expected = goal.plus(start.minus(reference));
        assert!(
            position.minus(expected).length() < 15.0,
            "member started at {start:?}, expected near {expected:?} (the order plus its \
                 own relative offset), got {position:?}"
        );
    }
    let horde_body = simulation.state().get(ids.body, horde).unwrap().0;
    assert_eq!(
        horde_body.position,
        Vec2::new(150.0, 150.0),
        "the horde entity must not have moved"
    );
}

#[test]
fn switching_formations_back_on_reassigns_slots_and_the_horde_re_forms() {
    let (mut simulation, ids) = horde_sim(Vec2::new(150.0, 150.0), square_formation());
    let members = members_of(&simulation, ids);
    let off = Modes {
        formation: FormationMode::Off,
        ..Modes::default()
    };
    order_horde(&mut simulation, ids, off, Vec2::new(450.0, 150.0));
    for _ in 0..400 {
        simulation.tick().unwrap();
    }
    let before: Vec<Option<u16>> = members
        .iter()
        .map(|m| simulation.state().get(ids.member, *m).unwrap().slot)
        .collect();

    simulation
        .submit(PLAYER, RotwkCommand::SetMovementModes(Modes::default()))
        .unwrap();
    simulation.tick().unwrap();
    let after: Vec<Option<u16>> = members
        .iter()
        .map(|m| simulation.state().get(ids.member, *m).unwrap().slot)
        .collect();
    assert_ne!(before, after, "nothing was re-assigned");
    let mut distinct = after.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), 9, "two members were given the same slot");

    for _ in 0..400 {
        simulation.tick().unwrap();
    }
    let reformed = extent(&simulation, ids, &members);
    assert!(
        (reformed.0 - 40.0).abs() < 8.0 && (reformed.1 - 40.0).abs() < 8.0,
        "the horde did not re-form: {reformed:?}"
    );
}

#[test]
fn an_order_does_not_move_another_players_horde_that_is_merely_selected() {
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let mine = HordeSpawn {
        formation: 0,
        origin: Vec2::new(150.0, 150.0),
        facing: Facing::EAST,
        member: spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER),
        count: 9,
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let theirs = HordeSpawn {
        formation: 0,
        origin: Vec2::new(150.0, 400.0),
        facing: Facing::EAST,
        member: spawn_at(0.0, 0.0, KindOf::SELECTABLE, OTHER),
        count: 9,
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let rules = RotwkRules::new(passability, Vec::new())
        .with_hordes(vec![square_formation()], vec![mine, theirs]);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let hordes: Vec<Entity> = simulation
        .state()
        .store(ids.horde)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    assert_eq!(hordes.len(), 2);
    let everyone: Vec<Entity> = simulation
        .state()
        .store(ids.member)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    assert_eq!(everyone.len(), 18);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: everyone,
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 450.0,
                y: 150.0,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(simulation.state().contains(ids.path, hordes[0]));
    assert!(
        !simulation.state().contains(ids.path, hordes[1]),
        "player 0 ordered player 1's horde"
    );
    for _ in 0..200 {
        simulation.tick().unwrap();
    }
    let theirs_body = simulation.state().get(ids.body, hordes[1]).unwrap().0;
    assert_eq!(theirs_body.position, Vec2::new(150.0, 400.0));
}

#[test]
fn a_dead_member_leaves_its_horde_and_an_empty_horde_is_despawned() {
    let (mut simulation, ids) = horde_sim(Vec2::new(150.0, 150.0), square_formation());
    let members = members_of(&simulation, ids);
    let slots_before: Vec<Option<u16>> = members
        .iter()
        .map(|m| simulation.state().get(ids.member, *m).unwrap().slot)
        .collect();
    assert!(simulation.state().get(ids.health, members[0]).is_none());
    assert_eq!(members_of(&simulation, ids).len(), 9);
    assert_eq!(slots_before.iter().filter(|slot| slot.is_some()).count(), 9);

    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    assert!(horde_entity(&simulation, ids).is_some());
    assert_eq!(members_of(&simulation, ids).len(), 9);
}

#[test]
fn a_horde_asked_for_more_members_than_it_has_slots_gets_the_slots() {
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let spawn = HordeSpawn {
        formation: 0,
        origin: Vec2::new(150.0, 150.0),
        facing: Facing::EAST,
        member: spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER),
        count: u32::MAX,
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let rules =
        RotwkRules::new(passability, Vec::new()).with_hordes(vec![square_formation()], vec![spawn]);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    assert_eq!(members_of(&simulation, ids).len(), 9);
    assert_eq!(
        simulation
            .state()
            .store(ids.member)
            .map_or(0, mint_sim::ComponentStore::len),
        9
    );
}

#[test]
fn a_member_that_fights_leaves_its_slot_and_a_member_that_dies_leaves_its_horde() {
    let (combat, bow, armour) = combat_data();
    let map = flat_map(128, 128);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let lone = FormationTable::from_offsets(&[Vec2::ZERO]);
    let attackers = HordeSpawn {
        formation: 0,
        origin: Vec2::new(200.0, 200.0),
        facing: Facing::EAST,
        member: armed_spawn(0.0, 0.0, PLAYER, Some(bow), Some(armour), 200.0),
        count: 9,
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let victim = HordeSpawn {
        formation: 1,
        origin: Vec2::new(900.0, 200.0),
        facing: Facing::EAST,
        member: armed_spawn(0.0, 0.0, OTHER, None, Some(armour), 40.0),
        count: 1,
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let rules = RotwkRules::new(passability, Vec::new())
        .with_combat(combat)
        .with_hordes(vec![square_formation(), lone], vec![attackers, victim]);
    let mut simulation = Simulation::new(9, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let hordes: Vec<Entity> = simulation
        .state()
        .store(ids.horde)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    assert_eq!(hordes.len(), 2);
    let mine = members_of(&simulation, ids);
    assert_eq!(mine.len(), 9);
    let theirs = simulation
        .state()
        .get(ids.horde, hordes[1])
        .unwrap()
        .members
        .clone();
    assert_eq!(theirs.len(), 1);

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: mine.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: theirs[0] })
        .unwrap();

    let mut left_its_slot = false;
    let mut horde_alive_when_corpse_appeared = None;
    for _ in 0..900 {
        simulation.tick().unwrap();
        if horde_alive_when_corpse_appeared.is_none()
            && simulation.state().contains(ids.doomed, theirs[0])
        {
            horde_alive_when_corpse_appeared = Some(simulation.state().is_live(hordes[1]));
        }
        if !left_its_slot {
            left_its_slot = mine.iter().any(|member| {
                simulation
                    .state()
                    .get(ids.body, *member)
                    .is_some_and(|body| {
                        body.0.position.minus(Vec2::new(200.0, 200.0)).length() > 120.0
                    })
            });
        }
        if !simulation.state().is_live(hordes[1]) {
            break;
        }
    }
    assert!(
        left_its_slot,
        "no member ever left its slot to close on the enemy - the slot pass \
             is overriding the path combat gave it"
    );
    assert!(
        !simulation.state().is_live(hordes[1]),
        "the one-member horde was not despawned when its member died"
    );
    assert_eq!(
            horde_alive_when_corpse_appeared,
            Some(false),
            "the horde outlived its only member's death - a corpse is still              being counted as a member"
        );
    assert!(
        !simulation.state().is_live(theirs[0])
            || simulation.state().contains(ids.doomed, theirs[0]),
        "the victim is neither dead nor a corpse"
    );
}

#[test]
fn the_same_horde_order_produces_the_same_state_hash_on_a_second_run() {
    let run = || {
        let (mut simulation, ids) = horde_sim(Vec2::new(150.0, 150.0), square_formation());
        order_horde(
            &mut simulation,
            ids,
            Modes::default(),
            Vec2::new(450.0, 250.0),
        );
        let mut hashes = Vec::new();
        for _ in 0..200 {
            simulation.tick().unwrap();
            hashes.push(simulation.state_hash());
        }
        hashes
    };
    let first = run();
    assert_eq!(first, run());
    let mut distinct = first.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert!(
        distinct.len() > 100,
        "only {} distinct hashes over 200 ticks; the world barely moved",
        distinct.len()
    );
}

#[test]
fn the_match_clock_advances_by_exactly_one_per_tick_and_nowhere_else() {
    let (mut simulation, ids, _) = simulation();
    let read = |sim: &Simulation<RotwkRules>| -> u64 {
        sim.state()
            .store(ids.clock)
            .and_then(|store| store.iter().next().map(|(_, clock)| clock.0))
            .unwrap_or(u64::MAX)
    };
    assert_eq!(read(&simulation), 0);
    simulation.tick().unwrap();
    assert_eq!(read(&simulation), 1);
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    assert_eq!(read(&simulation), 3);
}

fn economy_sim(use_shroud: bool) -> (Simulation<RotwkRules>, Ids) {
    economy_sim_income(use_shroud, 10)
}

fn quiet_economy_sim(use_shroud: bool) -> (Simulation<RotwkRules>, Ids) {
    economy_sim_income(use_shroud, 100_000)
}

fn economy_sim_income(use_shroud: bool, income_frames: u32) -> (Simulation<RotwkRules>, Ids) {
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let (structure_kind, _) = KindOf::parse("SELECTABLE STRUCTURE IMMOBILE");

    let built = Spawn {
        vision: Some(mint_sage_shroud::SHROUD_CLEAR_STANDARD),
        ..spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER)
    };
    let buildables = vec![
        crate::economy::Buildable {
            output: crate::economy::Output::Unit(built),
            kind: crate::unit::Kind(0),
            cost: 200,
            build_seconds: 1.0,
            command_points: 0,
            label: String::from("test unit"),
            builder: false,
        },
        crate::economy::Buildable {
            output: crate::economy::Output::Unit(Spawn {
                owner: OTHER,
                ..built
            }),
            kind: crate::unit::Kind(1),
            cost: 200,
            build_seconds: 1.0,
            command_points: 0,
            label: String::from("test unit"),
            builder: false,
        },
    ];

    let barracks = |x: f32, owner: PlayerId, buildable: u16, rally: Vec2| Spawn {
        health: Some(1_000.0),
        vision: Some(300.0),
        producer: Some(crate::unit::ProducerSpawn {
            menu: buildable,
            rally,
            depth: DEFAULT_MAX_QUEUE_ENTRIES,
        }),
        income: Some((mint_sim::LogicFrames::from_frames(income_frames), 50)),
        ..Spawn::inert(
            Vec2::new(x, 400.0),
            Facing::EAST,
            Shape {
                kind_of: structure_kind,
                radius: 26.0,
                height: 40.0,
                geometry_is_small: false,
            },
            owner,
        )
    };
    let scout = |x: f32, owner: PlayerId| Spawn {
        vision: Some(300.0),
        ..spawn_at(x, 200.0, KindOf::SELECTABLE, owner)
    };
    let spawns = vec![
        barracks(150.0, PLAYER, 0, Vec2::new(210.0, 400.0)),
        barracks(750.0, OTHER, 1, Vec2::new(690.0, 400.0)),
        scout(150.0, PLAYER),
        scout(750.0, OTHER),
    ];
    let simulation = Simulation::new(
        11,
        RotwkRules::new(passability, spawns)
            .with_economy(buildables, 500)
            .with_shroud(use_shroud),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

fn producers(simulation: &Simulation<RotwkRules>, ids: Ids, player: PlayerId) -> Vec<Entity> {
    let world = simulation.state();
    world
        .store(ids.producer)
        .map(|store| {
            store
                .entities()
                .iter()
                .copied()
                .filter(|e| world.get(ids.owner, *e).is_some_and(|o| o.0 == player))
                .collect()
        })
        .unwrap_or_default()
}

fn purse(simulation: &Simulation<RotwkRules>, ids: Ids, player: PlayerId) -> i64 {
    crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), player).amount()
}

fn queued(simulation: &Simulation<RotwkRules>, ids: Ids, structure: Entity) -> usize {
    simulation
        .state()
        .get(ids.producer, structure)
        .map_or(0, |producer| producer.queue.entries().len())
}

fn bodies(simulation: &Simulation<RotwkRules>, ids: Ids) -> usize {
    simulation.state().store(ids.body).map_or(0, |s| s.len())
}

fn order_build(simulation: &mut Simulation<RotwkRules>, player: PlayerId, structure: Entity) {
    simulation
        .submit(
            player,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![structure],
            },
        )
        .unwrap();
    simulation
        .submit(player, RotwkCommand::QueueUnit { button: 0 })
        .unwrap();
}

#[test]
fn two_viewers_see_different_things_and_the_state_hash_does_not_notice() {
    let run = |viewer: PlayerId| -> (Vec<u64>, Vec<(u32, u32, u32)>) {
        let (mut simulation, ids) = economy_sim(true);
        let scouts: Vec<Entity> = simulation
            .state()
            .store(ids.vision)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        for (player, goal) in [(PLAYER, 600.0f32), (OTHER, 300.0f32)] {
            simulation
                .submit(
                    player,
                    RotwkCommand::SetSelection {
                        replace: true,
                        units: scouts.clone(),
                    },
                )
                .unwrap();
            simulation
                .submit(
                    player,
                    RotwkCommand::MoveTo {
                        x: goal,
                        y: 200.0,
                        queue: false,
                    },
                )
                .unwrap();
        }
        let mut hashes = Vec::new();
        let mut seen = Vec::new();
        for _ in 0..120 {
            simulation.tick().unwrap();
            seen.push(crate::shroud::cell_counts(
                simulation.state(),
                RotwkRules::shroud_ids(ids),
                viewer,
            ));
            hashes.push(simulation.state_hash());
        }
        (hashes, seen)
    };

    let (hash_a, seen_a) = run(PLAYER);
    let (hash_b, seen_b) = run(OTHER);
    assert_eq!(
        hash_a, hash_b,
        "reading one player's visibility instead of another's changed the simulation"
    );
    assert_ne!(
        seen_a, seen_b,
        "the two players saw exactly the same thing, so this test proves nothing"
    );
    assert!(
        seen_a.last().is_some_and(|(_, _, visible)| *visible > 0),
        "nobody could see anything, so this test proves nothing"
    );
}

#[test]
fn every_player_slot_is_computed_even_though_only_one_is_ever_drawn() {
    let (mut simulation, ids) = economy_sim(true);
    for _ in 0..5 {
        simulation.tick().unwrap();
    }
    let shroud = RotwkRules::shroud_ids(ids);
    let zero = crate::shroud::cell_counts(simulation.state(), shroud, PLAYER);
    let one = crate::shroud::cell_counts(simulation.state(), shroud, OTHER);
    assert!(zero.2 > 0, "player 0 sees nothing");
    assert!(one.2 > 0, "player 1 sees nothing");
    let empty = crate::shroud::cell_counts(simulation.state(), shroud, PlayerId::new(3));
    assert_eq!(empty.1, 0);
    assert_eq!(empty.2, 0);
    assert!(empty.0 > 0);
}

#[test]
fn a_shipped_match_starts_explored_because_rotwk_ships_use_shroud_no() {
    let (mut simulation, ids) = economy_sim(false);
    simulation.tick().unwrap();
    let (unexplored, explored, visible) =
        crate::shroud::cell_counts(simulation.state(), RotwkRules::shroud_ids(ids), PLAYER);
    assert_eq!(unexplored, 0, "the black state should never appear");
    assert!(explored > 0 && visible > 0);
}

#[test]
fn a_map_placed_neutral_structure_is_fogged_from_the_start_and_a_neutral_unit_stays_hidden() {
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    const NEUTRAL: PlayerId = PlayerId::new(2);
    let (structure_kind, _) = KindOf::parse("SELECTABLE STRUCTURE IMMOBILE");
    let spawns = vec![
        spawn_at(150.0, 400.0, KindOf::SELECTABLE, PLAYER),
        Spawn {
            ..Spawn::inert(
                Vec2::new(750.0, 400.0),
                Facing::EAST,
                Shape {
                    kind_of: structure_kind,
                    radius: 26.0,
                    height: 40.0,
                    geometry_is_small: false,
                },
                NEUTRAL,
            )
        },
        spawn_at(750.0, 460.0, KindOf::SELECTABLE, NEUTRAL),
    ];
    let rules = RotwkRules::new(passability, spawns)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::new()],
            vec![crate::alliance::MapRelations::default()],
        );
    let mut simulation = Simulation::new(3, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    simulation.tick().unwrap();

    let shroud = RotwkRules::shroud_ids(ids);
    let entities: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    let (structure, unit) = (entities[1], entities[2]);
    let structure_seen = simulation.state().get(shroud.seen, structure).unwrap();
    let unit_seen = simulation.state().get(shroud.seen, unit).unwrap();
    assert_eq!(
        structure_seen.visibility(PLAYER),
        mint_sage_shroud::ObjectVisibility::Fogged,
        "a map-placed neutral structure should read as seen once, not shrouded"
    );
    assert_eq!(
        unit_seen.visibility(PLAYER),
        mint_sage_shroud::ObjectVisibility::Shrouded,
        "a neutral unit is never remembered"
    );
}

#[test]
fn an_unexplored_match_reveals_only_what_a_unit_has_walked_past() {
    let (mut simulation, ids) = economy_sim(true);
    let shroud = RotwkRules::shroud_ids(ids);
    simulation.tick().unwrap();
    let (unexplored_before, explored_before, _) =
        crate::shroud::cell_counts(simulation.state(), shroud, PLAYER);
    assert!(
        unexplored_before > 0,
        "an unexplored match should have black"
    );

    let scouts: Vec<Entity> = simulation
        .state()
        .store(ids.vision)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: scouts,
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 700.0,
                y: 200.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..200 {
        simulation.tick().unwrap();
    }
    let (unexplored_after, explored_after, _) =
        crate::shroud::cell_counts(simulation.state(), shroud, PLAYER);
    assert!(
        explored_after > explored_before,
        "the march explored nothing: {explored_before} then {explored_after}"
    );
    assert!(
        unexplored_after < unexplored_before,
        "the black area did not shrink"
    );
    assert!(
        unexplored_after > 0,
        "one march revealed the whole map, so nothing was learned"
    );
}

#[test]
fn the_shroud_conserves_every_counter_over_a_long_march() {
    let (mut simulation, ids) = economy_sim(true);
    let shroud = RotwkRules::shroud_ids(ids);
    let scouts: Vec<Entity> = simulation
        .state()
        .store(ids.vision)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: scouts,
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 700.0,
                y: 600.0,
                queue: false,
            },
        )
        .unwrap();
    for frame in 0..300 {
        simulation.tick().unwrap();
        let (unbalanced, _) = crate::shroud::conservation(simulation.state(), shroud);
        assert_eq!(unbalanced, 0, "a counter left a state it cannot reach");
        let pending = crate::shroud::pending_undos(simulation.state(), shroud);
        assert!(
            pending <= 4,
            "frame {frame} left {pending} unlooks queued - the drain has stopped"
        );
    }
    let (_, watched) = crate::shroud::conservation(simulation.state(), shroud);
    assert!(watched > 0, "every unit stopped looking");
}

#[test]
fn a_structure_is_remembered_in_fog_and_a_unit_is_not() {
    let (mut simulation, ids) = economy_sim(true);
    let mine: Vec<Entity> = producers(&simulation, ids, PLAYER);
    let scout = simulation
        .state()
        .store(ids.vision)
        .and_then(|store| {
            store.entities().iter().copied().find(|e| {
                simulation
                    .state()
                    .get(ids.owner, *e)
                    .is_some_and(|o| o.0 == PLAYER)
                    && !mine.contains(e)
            })
        })
        .unwrap();
    let theirs = producers(&simulation, ids, OTHER);
    let their_barracks = theirs.first().copied().unwrap();
    let their_scout = simulation
        .state()
        .store(ids.vision)
        .and_then(|store| {
            store.entities().iter().copied().find(|e| {
                simulation
                    .state()
                    .get(ids.owner, *e)
                    .is_some_and(|o| o.0 == OTHER)
                    && *e != their_barracks
            })
        })
        .unwrap();

    let visibility = |simulation: &Simulation<RotwkRules>, entity: Entity| {
        simulation
            .state()
            .get(ids.seen, entity)
            .map(|seen| seen.visibility(PLAYER))
    };

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![scout],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 730.0,
                y: 380.0,
                queue: false,
            },
        )
        .unwrap();
    let mut saw_barracks = false;
    for _ in 0..400 {
        simulation.tick().unwrap();
        if visibility(&simulation, their_barracks)
            .is_some_and(|v| v >= mint_sage_shroud::ObjectVisibility::PartialClear)
        {
            saw_barracks = true;
            break;
        }
    }
    assert!(saw_barracks, "the scout never reached the enemy base");

    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 100.0,
                y: 100.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..600 {
        simulation.tick().unwrap();
        if visibility(&simulation, their_barracks)
            == Some(mint_sage_shroud::ObjectVisibility::Fogged)
        {
            break;
        }
    }
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        visibility(&simulation, their_barracks),
        Some(mint_sage_shroud::ObjectVisibility::Fogged),
        "the enemy structure was not remembered in fog"
    );
    assert_eq!(
        visibility(&simulation, their_scout),
        Some(mint_sage_shroud::ObjectVisibility::Shrouded),
        "an enemy unit was remembered, and units are never remembered"
    );
}

#[test]
fn a_structure_earns_on_its_timer_and_a_queued_unit_is_charged_at_once() {
    let (mut simulation, ids) = economy_sim(false);
    let structure = producers(&simulation, ids, PLAYER)
        .first()
        .copied()
        .unwrap();
    assert_eq!(purse(&simulation, ids, PLAYER), 500);

    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    assert_eq!(purse(&simulation, ids, PLAYER), 550);
    for _ in 0..20 {
        simulation.tick().unwrap();
    }
    assert_eq!(purse(&simulation, ids, PLAYER), 650);

    let before = bodies(&simulation, ids);
    order_build(&mut simulation, PLAYER, structure);
    simulation.tick().unwrap();
    assert_eq!(purse(&simulation, ids, PLAYER), 450);
    assert_eq!(queued(&simulation, ids, structure), 1);
    assert_eq!(bodies(&simulation, ids), before, "the unit appeared early");

    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    assert_eq!(queued(&simulation, ids, structure), 0);
    assert_eq!(
        bodies(&simulation, ids),
        before.saturating_add(1),
        "the finished unit never appeared"
    );
}

#[test]
fn a_player_who_cannot_pay_is_refused_and_charged_nothing() {
    let (mut simulation, ids) = quiet_economy_sim(false);
    let structure = producers(&simulation, ids, PLAYER)
        .first()
        .copied()
        .unwrap();
    order_build(&mut simulation, PLAYER, structure);
    order_build(&mut simulation, PLAYER, structure);
    order_build(&mut simulation, PLAYER, structure);
    simulation.tick().unwrap();
    assert_eq!(
        queued(&simulation, ids, structure),
        2,
        "the third order should have been refused"
    );
    assert_eq!(
        purse(&simulation, ids, PLAYER),
        100,
        "the clamping withdraw part-paid for a refused order"
    );
}

#[test]
fn cancelling_refunds_in_full_and_removes_the_entry() {
    let (mut simulation, ids) = quiet_economy_sim(false);
    let structure = producers(&simulation, ids, PLAYER)
        .first()
        .copied()
        .unwrap();
    order_build(&mut simulation, PLAYER, structure);
    simulation.tick().unwrap();
    assert_eq!(queued(&simulation, ids, structure), 1);
    let after_queue = purse(&simulation, ids, PLAYER);

    simulation
        .submit(PLAYER, RotwkCommand::CancelProduction)
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(queued(&simulation, ids, structure), 0);
    assert_eq!(
        purse(&simulation, ids, PLAYER),
        after_queue.saturating_add(200),
        "the refund was not the full cost"
    );
}

#[test]
fn one_players_build_order_does_not_touch_anothers_structure() {
    let (mut simulation, ids) = quiet_economy_sim(false);
    let theirs = producers(&simulation, ids, OTHER).first().copied().unwrap();
    order_build(&mut simulation, PLAYER, theirs);
    simulation.tick().unwrap();
    assert_eq!(queued(&simulation, ids, theirs), 0);
    assert_eq!(purse(&simulation, ids, PLAYER), 500);
    assert_eq!(purse(&simulation, ids, OTHER), 500);
}

#[test]
fn command_points_fill_a_factions_limit_and_gate_the_queue() {
    let (mut simulation, ids) = economy_sim(false);
    simulation.tick().unwrap();
    let points =
        crate::economy::points_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER);
    assert_eq!(points.limit(), 300, "player 0 is Good: gamedata.ini:8732");
    assert_eq!(
        crate::economy::points_of(simulation.state(), RotwkRules::economy_ids(ids), OTHER).limit(),
        600,
        "player 1 is Evil: gamedata.ini:8733"
    );
    assert_eq!(points.used(), 0);
}

#[test]
fn a_hordes_upkeep_falls_as_its_members_die() {
    let (combat, _bow, armour) = combat_data();
    let sword = combat.weapons.id("TestSword").unwrap();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let formation = table_of(&[Vec2::new(-10.0, 0.0), Vec2::new(10.0, 0.0)]);
    let spawn = HordeSpawn {
        formation: 0,
        origin: Vec2::new(200.0, 200.0),
        facing: Facing::EAST,
        member: Spawn {
            health: Some(10.0),
            upkeep: Some(50),
            ..spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER)
        },
        count: 2,
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let rules = RotwkRules::new(
        passability,
        vec![armed_spawn(
            250.0,
            200.0,
            OTHER,
            Some(sword),
            Some(armour),
            200.0,
        )],
    )
    .with_combat(combat)
    .with_hordes(vec![formation], vec![spawn]);
    let mut simulation = Simulation::new(29, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let economy_ids = RotwkRules::economy_ids(ids);
    let members = members_of(&simulation, ids);
    assert_eq!(members.len(), 2);
    let enemy = enemy_of(&simulation, ids);
    let target = *members
        .iter()
        .find(|member| {
            simulation
                .state()
                .get(ids.body, **member)
                .is_some_and(|body| body.0.position.x > 200.0)
        })
        .expect("the near member is in the world");

    simulation.tick().unwrap();
    assert_eq!(
        crate::economy::points_of(simulation.state(), economy_ids, PLAYER).used(),
        100,
        "both members' upkeep should be charged before either dies"
    );

    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![enemy],
            },
        )
        .unwrap();
    simulation
        .submit(OTHER, RotwkCommand::AttackObject { target })
        .unwrap();
    for _ in 0..60 {
        simulation.tick().unwrap();
        if !simulation.state().is_live(target) {
            break;
        }
    }
    assert!(
        !simulation.state().is_live(target),
        "the fixture's own setup - the member should have died"
    );
    assert_eq!(
        crate::economy::points_of(simulation.state(), economy_ids, PLAYER).used(),
        50,
        "the dead member's upkeep must drop off the recount, not linger"
    );
}

#[test]
fn resetting_the_shroud_forgets_every_structure_the_player_had_seen() {
    let (mut simulation, ids) = economy_sim(false);
    let shroud = RotwkRules::shroud_ids(ids);
    for _ in 0..3 {
        simulation.tick().unwrap();
    }
    let (unexplored, explored, _) = crate::shroud::cell_counts(simulation.state(), shroud, PLAYER);
    assert_eq!(unexplored, 0);
    assert!(explored > 0);

    simulation
        .submit(PLAYER, RotwkCommand::ResetShroud { use_shroud: true })
        .unwrap();
    simulation.tick().unwrap();
    let (unexplored, _, visible) = crate::shroud::cell_counts(simulation.state(), shroud, PLAYER);
    assert!(unexplored > 0, "the reset did not re-shroud anything");
    assert!(
        visible > 0,
        "the reset lost the discs of everything still standing"
    );
    let (unbalanced, _) = crate::shroud::conservation(simulation.state(), shroud);
    assert_eq!(unbalanced, 0);
}

#[test]
fn the_same_economy_and_shroud_commands_produce_the_same_state_hash_twice() {
    let run = || {
        let (mut simulation, ids) = economy_sim(true);
        let structure = producers(&simulation, ids, PLAYER)
            .first()
            .copied()
            .unwrap();
        let scouts: Vec<Entity> = simulation
            .state()
            .store(ids.vision)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: scouts,
                },
            )
            .unwrap();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::MoveTo {
                    x: 600.0,
                    y: 500.0,
                    queue: false,
                },
            )
            .unwrap();
        for round in 0..6 {
            for _ in 0..40 {
                simulation.tick().unwrap();
            }
            order_build(&mut simulation, PLAYER, structure);
            if round % 3 == 2 {
                simulation
                    .submit(PLAYER, RotwkCommand::CancelProduction)
                    .unwrap();
            }
        }
        for _ in 0..40 {
            simulation.tick().unwrap();
        }
        simulation.state_hash()
    };
    assert_eq!(run(), run());
}

const CONSTRUCTION_INI: &str = "\
Object Porter
  KindOf = SELECTABLE INFANTRY DOZER
  CommandSet = PorterSet
  ShroudClearingRange = 25
  Body = ActiveBody Tag_01
    MaxHealth = 200
  End
  Geometry = CYLINDER
  GeometryMajorRadius = 8
  GeometryHeight = 16
End
CommandSet PorterSet
  1 = Build_Shed
End
CommandButton Build_Shed
  Command = DOZER_CONSTRUCT
  Object = Shed
End
Object Shed
  KindOf = STRUCTURE SELECTABLE IMMOBILE
  BuildCost = 100
  BuildTime = 10
  Body = StructureBody Tag_01
    MaxHealth = 200
  End
  Geometry = BOX
  GeometryMajorRadius = 40
  GeometryMinorRadius = 40
  GeometryHeight = 30
End
Object Attacker
  KindOf = SELECTABLE INFANTRY
  WeaponSet
    Conditions = None
    Weapon = PRIMARY TestSword
  End
  Body = ActiveBody Tag_01
    MaxHealth = 100
  End
  Geometry = CYLINDER
  GeometryMajorRadius = 6
  GeometryHeight = 16
End
Weapon TestSword
  AttackRange = 60
  MeleeWeapon = Yes
  DelayBetweenShots = 1000
  DamageNugget
    Damage = 500
    DamageType = PIERCE
  End
End
";

fn tick_until_site(simulation: &mut Simulation<RotwkRules>, ids: Ids) -> Entity {
    for _ in 0..600 {
        simulation.tick().unwrap();
        if let Some(site) = simulation
            .state()
            .store(ids.construction)
            .and_then(|store| store.entities().first().copied())
        {
            return site;
        }
    }
    panic!("no site appeared - the builder never arrived");
}

fn construction_kinds() -> (GameData, std::sync::Arc<KindTable>) {
    let data = crate::game::tests::data_from(CONSTRUCTION_INI);
    let (kinds, missing) = KindTable::closure(
        &data,
        &[
            "Porter".to_owned(),
            "Shed".to_owned(),
            "Attacker".to_owned(),
        ],
    );
    assert!(missing.is_empty(), "template(s) not resolved: {missing:?}");
    (data, std::sync::Arc::new(kinds))
}

#[test]
fn a_sites_health_fraction_starts_low_and_rises_to_the_templates_real_max() {
    let (data, kinds) = construction_kinds();
    let production = kinds.production(&data);
    let porter_kind = kinds.kind("Porter").unwrap();
    let porter = kinds
        .spawn(
            &data.combat,
            porter_kind,
            Vec2::new(140.0, 200.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let map = flat_map(160, 60);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let mut simulation = Simulation::new(
        52,
        RotwkRules::new(passability, vec![porter])
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production)
            .with_economy(Vec::new(), 1_000),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let porter_entity = simulation
        .state()
        .store(ids.body)
        .and_then(|store| store.entities().first().copied())
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![porter_entity],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Construct {
                kind: kinds.kind("Shed").unwrap().0,
                x: 150.0,
                y: 200.0,
                angle: 0.0,
            },
        )
        .unwrap();
    let site = tick_until_site(&mut simulation, ids);
    let fraction_of = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.health, site)
            .map(|health| health.0.fraction())
    };
    let started = fraction_of(&simulation).unwrap();
    assert!(
        started > 0.0 && started < 0.01,
        "current must start at 1 against the template's real maximum, not a synthetic one: \
         {started}"
    );
    for _ in 0..400 {
        simulation.tick().unwrap();
        if !simulation.state().contains(ids.construction, site) {
            break;
        }
    }
    assert!(
        !simulation.state().contains(ids.construction, site),
        "the build never completed"
    );
    assert_eq!(
        fraction_of(&simulation),
        Some(1.0),
        "a completed structure must be at its real full health, not 1 out of a 1.0 maximum"
    );
    assert!(!simulation.is_poisoned());
}

#[test]
fn damage_taken_while_building_is_still_missing_when_the_site_completes() {
    let ini = format!(
        "{CONSTRUCTION_INI}
Object Pricker
  KindOf = SELECTABLE INFANTRY
  WeaponSet
    Conditions = None
    Weapon = PRIMARY TestNeedle
  End
  Body = ActiveBody Tag_01
    MaxHealth = 100
  End
  Geometry = CYLINDER
  GeometryMajorRadius = 6
  GeometryHeight = 16
End
Weapon TestNeedle
  AttackRange = 60
  MeleeWeapon = Yes
  DelayBetweenShots = 100000
  DamageNugget
    Damage = 20
    DamageType = PIERCE
  End
End
"
    );
    let data = crate::game::tests::data_from(&ini);
    let (kinds, missing) = KindTable::closure(
        &data,
        &["Porter".to_owned(), "Shed".to_owned(), "Pricker".to_owned()],
    );
    assert!(missing.is_empty(), "template(s) not resolved: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let porter = kinds
        .spawn(
            &data.combat,
            kinds.kind("Porter").unwrap(),
            Vec2::new(140.0, 200.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let pricker = kinds
        .spawn(
            &data.combat,
            kinds.kind("Pricker").unwrap(),
            Vec2::new(150.0, 285.0),
            Facing::EAST,
            OTHER,
        )
        .unwrap();
    let map = flat_map(160, 60);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let mut simulation = Simulation::new(
        71,
        RotwkRules::new(passability, vec![porter, pricker])
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production)
            .with_economy(Vec::new(), 1_000),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    let (porter_entity, pricker_entity) = (units[0], units[1]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![porter_entity],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Construct {
                kind: kinds.kind("Shed").unwrap().0,
                x: 150.0,
                y: 200.0,
                angle: 0.0,
            },
        )
        .unwrap();
    let site = tick_until_site(&mut simulation, ids);
    let health_of = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.health, site)
            .map_or(0.0, |health| health.0.current())
    };
    for _ in 0..220 {
        simulation.tick().unwrap();
    }
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![pricker_entity],
            },
        )
        .unwrap();
    simulation
        .submit(OTHER, RotwkCommand::AttackObject { target: site })
        .unwrap();
    let mut before = health_of(&simulation);
    let mut hit = false;
    for _ in 0..90 {
        simulation.tick().unwrap();
        if health_of(&simulation) < before {
            hit = true;
            break;
        }
        before = health_of(&simulation);
    }
    assert!(hit, "the site was never damaged while building");
    simulation
        .submit(
            OTHER,
            RotwkCommand::MoveTo {
                x: 700.0,
                y: 40.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..400 {
        simulation.tick().unwrap();
        if !simulation.state().contains(ids.construction, site) {
            break;
        }
    }
    assert!(
        !simulation.state().contains(ids.construction, site),
        "the build never completed"
    );
    let finished = health_of(&simulation);
    assert!(
        (finished - 181.0).abs() < 2.0,
        "a 200-health Shed hit for 20 while building must finish near 181, not {finished}"
    );
    assert!(!simulation.is_poisoned());
}

#[test]
fn a_moving_unit_replans_around_a_site_placed_across_its_path() {
    let (data, kinds) = construction_kinds();
    let production = kinds.production(&data);
    let porter_kind = kinds.kind("Porter").unwrap();
    let porter = kinds
        .spawn(
            &data.combat,
            porter_kind,
            Vec2::new(400.0, 120.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let mover = spawn_at(10.0, 200.0, KindOf::SELECTABLE, PLAYER);
    let map = flat_map(160, 60);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let mut simulation = Simulation::new(
        51,
        RotwkRules::new(passability, vec![porter, mover])
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production)
            .with_economy(Vec::new(), 1_000),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    let (porter_entity, mover_entity) = (units[0], units[1]);

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![mover_entity],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 600.0,
                y: 200.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..20 {
        simulation.tick().unwrap();
    }
    let mid_route = simulation
        .state()
        .get(ids.body, mover_entity)
        .unwrap()
        .0
        .position;
    assert!(
        mid_route.x > 10.0 && mid_route.x < 300.0,
        "the mover has not actually started: {mid_route:?}"
    );

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![porter_entity],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Construct {
                kind: kinds.kind("Shed").unwrap().0,
                x: 400.0,
                y: 200.0,
                angle: 0.0,
            },
        )
        .unwrap();
    let clock = simulation
        .state()
        .store(ids.obstacles)
        .and_then(|store| store.entities().first().copied())
        .expect("the clock entity carries an obstacle layer");
    let stamped = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.obstacles, clock)
            .is_some_and(|layer| layer.counts().iter().any(|count| *count > 0))
    };
    assert!(!stamped(&simulation), "the order alone must stamp nothing");
    for _ in 0..200 {
        simulation.tick().unwrap();
        if stamped(&simulation) {
            break;
        }
    }
    assert!(stamped(&simulation), "the arriving porter stamps the site");
    let when_stamped = simulation
        .state()
        .get(ids.body, mover_entity)
        .unwrap()
        .0
        .position;
    assert!(
        when_stamped.x < 360.0,
        "the mover had already passed the site: {when_stamped:?}"
    );

    for _ in 0..600 {
        simulation.tick().unwrap();
        if simulation.state().get(ids.path, mover_entity).is_none() {
            break;
        }
    }
    let final_pos = simulation
        .state()
        .get(ids.body, mover_entity)
        .unwrap()
        .0
        .position;
    assert!(
        (final_pos.x - 600.0).abs() < 10.0 && (final_pos.y - 200.0).abs() < 10.0,
        "the mover did not reach its goal around the site: {final_pos:?}"
    );
    assert!(
        simulation.state().contains(ids.replanned_at, mover_entity),
        "replan_if_due never ran - the mover reaching its goal must have been luck, not a re-plan"
    );
    assert!(!simulation.is_poisoned());
}

fn defeated_holding(held: &str, mode: crate::defeat::DefeatMode, filter: &[&str]) -> bool {
    let (data, kinds) = construction_kinds();
    let spawn = |name: &str, at: f32, player| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                Vec2::new(at, 100.0),
                Facing::EAST,
                player,
            )
            .unwrap()
    };
    let objects = vec![spawn(held, 100.0, PLAYER), spawn("Shed", 400.0, OTHER)];
    let passability = Passability::build(&flat_map(160, 60), 5.0 / 128.0).unwrap();
    let structure =
        crate::defeat::ObjectFilter::parse(&["NONE".to_owned(), "+STRUCTURE".to_owned()]);
    let unit = crate::defeat::ObjectFilter::parse(
        &filter.iter().map(|t| (*t).to_owned()).collect::<Vec<_>>(),
    );
    let mut simulation = Simulation::new(
        62,
        RotwkRules::new(passability, objects)
            .with_combat(data.combat.clone())
            .with_victory(Arc::clone(&kinds), structure)
            .with_victory_units(unit)
            .with_defeat_mode(mode),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    simulation.tick().unwrap();
    let clock = simulation
        .state()
        .store(ids.clock)
        .and_then(|store| store.entities().first().copied())
        .unwrap();
    simulation.state().get(ids.outcome, clock).is_some()
}

#[test]
fn a_players_units_keep_it_alive_by_the_defeat_mode() {
    use crate::defeat::DefeatMode::{Dozer, UnitFilter};
    let shipped = ["ANY", "-DOZER", "-NOT_AUTOACQUIRABLE"];
    let rows = [
        ("Porter", Dozer, &shipped[..], false),
        ("Attacker", Dozer, &shipped[..], true),
        ("Porter", UnitFilter, &shipped[..], true),
        ("Attacker", UnitFilter, &shipped[..], true),
        ("Attacker", UnitFilter, &["ALL", "-DOZER"][..], false),
        ("Porter", UnitFilter, &["ALL", "-DOZER"][..], true),
    ];
    for (held, mode, filter, defeated) in rows {
        assert_eq!(
            defeated_holding(held, mode, filter),
            defeated,
            "{held} under {mode:?} with {filter:?}"
        );
    }
}

#[test]
fn a_player_with_only_a_structure_and_porters_is_not_defeated_until_it_dies() {
    let (data, kinds) = construction_kinds();
    let fortress_kind = kinds.kind("Shed").unwrap();
    let porter_kind = kinds.kind("Porter").unwrap();
    let attacker_kind = kinds.kind("Attacker").unwrap();
    let fortress = kinds
        .spawn(
            &data.combat,
            fortress_kind,
            Vec2::new(100.0, 100.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let porter_1 = kinds
        .spawn(
            &data.combat,
            porter_kind,
            Vec2::new(0.0, 100.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let porter_2 = kinds
        .spawn(
            &data.combat,
            porter_kind,
            Vec2::new(0.0, 150.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let attacker = kinds
        .spawn(
            &data.combat,
            attacker_kind,
            Vec2::new(100.0, 250.0),
            Facing::EAST,
            OTHER,
        )
        .unwrap();
    let other_fortress = kinds
        .spawn(
            &data.combat,
            fortress_kind,
            Vec2::new(1400.0, 300.0),
            Facing::EAST,
            OTHER,
        )
        .unwrap();
    let map = flat_map(160, 60);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let filter = crate::defeat::ObjectFilter::parse(&["NONE".to_owned(), "+STRUCTURE".to_owned()]);
    let mut simulation = Simulation::new(
        61,
        RotwkRules::new(
            passability,
            vec![fortress, porter_1, porter_2, attacker, other_fortress],
        )
        .with_combat(data.combat.clone())
        .with_victory(Arc::clone(&kinds), filter),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    let (fortress_entity, attacker_entity) = (units[0], units[3]);

    simulation.tick().unwrap();
    let clock = simulation
        .state()
        .store(ids.clock)
        .and_then(|store| store.entities().first().copied())
        .expect("the clock entity exists");
    assert!(
        simulation.state().get(ids.outcome, clock).is_none(),
        "a fortress and two porters on frame 1 must not be defeated"
    );

    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![attacker_entity],
            },
        )
        .unwrap();
    simulation
        .submit(
            OTHER,
            RotwkCommand::AttackObject {
                target: fortress_entity,
            },
        )
        .unwrap();
    for _ in 0..300 {
        simulation.tick().unwrap();
        if simulation.state().get(ids.outcome, clock).is_some() {
            break;
        }
    }
    let outcome = simulation
        .state()
        .get(ids.outcome, clock)
        .copied()
        .expect("the match must end once the only structure dies");
    assert_eq!(outcome.winner, OTHER);
    assert!(!simulation.is_poisoned());
}

#[test]
fn construct_cancel_and_defeat_replay_to_the_same_hash_sequence() {
    let run = || {
        let (data, kinds) = construction_kinds();
        let production = kinds.production(&data);
        let porter_kind = kinds.kind("Porter").unwrap();
        let attacker_kind = kinds.kind("Attacker").unwrap();
        let shed_kind = kinds.kind("Shed").unwrap();
        let porter_a1 = kinds
            .spawn(
                &data.combat,
                porter_kind,
                Vec2::new(50.0, 300.0),
                Facing::EAST,
                PLAYER,
            )
            .unwrap();
        let porter_a2 = kinds
            .spawn(
                &data.combat,
                porter_kind,
                Vec2::new(250.0, 300.0),
                Facing::EAST,
                PLAYER,
            )
            .unwrap();
        let porter_b = kinds
            .spawn(
                &data.combat,
                porter_kind,
                Vec2::new(500.0, 40.0),
                Facing::EAST,
                OTHER,
            )
            .unwrap();
        let attacker_a = kinds
            .spawn(
                &data.combat,
                attacker_kind,
                Vec2::new(500.0, 250.0),
                Facing::EAST,
                PLAYER,
            )
            .unwrap();
        let hall_a = kinds
            .spawn(
                &data.combat,
                shed_kind,
                Vec2::new(50.0, 100.0),
                Facing::EAST,
                PLAYER,
            )
            .unwrap();
        let hall_b = kinds
            .spawn(
                &data.combat,
                shed_kind,
                Vec2::new(500.0, 180.0),
                Facing::EAST,
                OTHER,
            )
            .unwrap();
        let map = flat_map(700, 400);
        let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
        let filter =
            crate::defeat::ObjectFilter::parse(&["NONE".to_owned(), "+STRUCTURE".to_owned()]);
        let mut simulation = Simulation::new(
            71,
            RotwkRules::new(
                passability,
                vec![porter_a1, porter_a2, porter_b, attacker_a, hall_a, hall_b],
            )
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production)
            .with_victory(Arc::clone(&kinds), filter)
            .with_economy(Vec::new(), 1_000),
        )
        .unwrap();
        let ids = simulation.rules().ids().unwrap();
        let units: Vec<Entity> = simulation
            .state()
            .store(ids.body)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        let (porter_a1_e, porter_a2_e, porter_b_e, attacker_a_e, hall_b_e) =
            (units[0], units[1], units[2], units[3], units[5]);

        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![porter_a1_e],
                },
            )
            .unwrap();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::Construct {
                    kind: shed_kind.0,
                    x: 150.0,
                    y: 300.0,
                    angle: 0.0,
                },
            )
            .unwrap();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![porter_a2_e],
                },
            )
            .unwrap();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::Construct {
                    kind: shed_kind.0,
                    x: 350.0,
                    y: 300.0,
                    angle: 0.0,
                },
            )
            .unwrap();
        simulation
            .submit(
                OTHER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![porter_b_e],
                },
            )
            .unwrap();
        simulation
            .submit(
                OTHER,
                RotwkCommand::Construct {
                    kind: shed_kind.0,
                    x: 500.0,
                    y: 100.0,
                    angle: 0.0,
                },
            )
            .unwrap();
        simulation.tick().unwrap();

        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![porter_a2_e],
                },
            )
            .unwrap();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::MoveTo {
                    x: 250.0,
                    y: 40.0,
                    queue: false,
                },
            )
            .unwrap();

        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![attacker_a_e],
                },
            )
            .unwrap();
        simulation
            .submit(PLAYER, RotwkCommand::AttackObject { target: hall_b_e })
            .unwrap();

        let mut hashes = Vec::new();
        for _ in 0..500 {
            simulation.tick().unwrap();
            hashes.push(simulation.state_hash());
        }
        hashes
    };
    let first = run();
    let second = run();
    assert_eq!(first, second, "the same commands must replay identically");
    assert!(
        first.iter().any(|hash| *hash != first[0]),
        "the world must actually change"
    );
}

fn upgrade_power_scenario() -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    upgrade_power_scenario_with(false)
}

fn upgrade_power_scenario_with(with_ai: bool) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let text = "\
Upgrade Upgrade_Level2\n  Type = OBJECT\n  BuildCost = 100\n  BuildTime = 2\nEnd\n\
Upgrade Upgrade_Oil\n  Type = PLAYER\n  BuildCost = 300\n  BuildTime = 1\nEnd\n\
SpecialPower SpecialAbilityOil\n  Enum = SPECIAL_GENERAL_TARGETLESS\n  ReloadTime = 600\nEnd\n\
CommandSet BarracksSet\n  1 = Build_Soldier\n  2 = Upg_Level2\nEnd\n\
CommandSet BarracksSetLevel2\n  1 = Build_Soldier\n  2 = Build_Veteran\nEnd\n\
CommandButton Build_Soldier\n  Command = UNIT_BUILD\n  Object = Soldier\nEnd\n\
CommandButton Build_Veteran\n  Command = UNIT_BUILD\n  Object = Veteran\nEnd\n\
CommandButton Upg_Level2\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Level2\nEnd\n\
CommandSet FortressSet\n  1 = Upg_Oil\n  2 = Fire_Oil\nEnd\n\
CommandButton Upg_Oil\n  Command = PLAYER_UPGRADE\n  Upgrade = Upgrade_Oil\nEnd\n\
CommandButton Fire_Oil\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityOil\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  BuildCost = 50\n  BuildTime = 1\n  \
  Body = ActiveBody Tag\n    MaxHealth = 50\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\nEnd\n\
Object Veteran\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  BuildCost = 80\n  BuildTime = 1\n  MaxSimultaneousOfType = 1\n  \
  Body = ActiveBody Tag\n    MaxHealth = 80\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\nEnd\n\
Object Barracks\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = BarracksSet\n  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag\n    MaxHealth = 1000\n  End\n  \
  Behavior = ProductionUpdate Tag\n    MaxQueueEntries = 3\n  End\n  \
  Behavior = CommandSetUpgrade Tag2\n    TriggeredBy = Upgrade_Level2\n    \
    CommandSet = BarracksSetLevel2\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\nEnd\n\
Object Fortress\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = FortressSet\n  BuildCost = 2000\n  BuildTime = 20\n  \
  Body = StructureBody Tag\n    MaxHealth = 3000\n  End\n  \
  Behavior = UnpauseSpecialPowerUpgrade Tag1\n    \
    SpecialPowerTemplate = SpecialAbilityOil\n    TriggeredBy = Upgrade_Oil\n  End\n  \
  Behavior = SpecialPowerModule Tag2\n    \
    SpecialPowerTemplate = SpecialAbilityOil\n    StartsPaused = Yes\n    \
    AvailableAtStart = No\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 40\nEnd\n";
    let doc =
        mint_sage_ini::Document::parse("t.ini", text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _line) =
        crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds = vec![String::from("Barracks"), String::from("Fortress")];
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let upgrade_menus = kinds.upgrades(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);

    let barracks_kind = kinds.kind("Barracks").unwrap();
    let fortress_kind = kinds.kind("Fortress").unwrap();
    let mut barracks_spawn = kinds
        .spawn(
            &data.combat,
            barracks_kind,
            Vec2::new(0.0, 0.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    barracks_spawn.producer = kinds.producer(
        &production,
        barracks_kind,
        Vec2::new(0.0, 0.0),
        Facing::EAST,
    );
    let mut fortress_spawn = kinds
        .spawn(
            &data.combat,
            fortress_kind,
            Vec2::new(300.0, 0.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    fortress_spawn.producer = kinds.producer(
        &production,
        fortress_kind,
        Vec2::new(300.0, 0.0),
        Facing::EAST,
    );

    let map = flat_map(64, 64);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let mut spawns = vec![barracks_spawn, fortress_spawn];
    if with_ai {
        spawns.push(Spawn {
            health: Some(10.0),
            ..spawn_at(320.0, 0.0, KindOf::SELECTABLE, OTHER)
        });
    }
    let mut rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 10_000)
        .with_menus(production.menus.clone())
        .with_upgrades(upgrade_menus, Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload);
    if with_ai {
        rules = rules.with_skirmish_ai(
            vec![crate::skirmish_ai::AiSeed {
                player: PLAYER,
                candidates: Vec::new(),
            }],
            crate::combat::AiTiming::default(),
            Vec::new(),
        );
    }
    let simulation = Simulation::new(1, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let barracks = *simulation
        .state()
        .store(ids.producer)
        .unwrap()
        .entities()
        .first()
        .unwrap();
    let fortress = *simulation
        .state()
        .store(ids.power_book)
        .unwrap()
        .entities()
        .first()
        .unwrap();
    (simulation, ids, barracks, fortress)
}

fn select(simulation: &mut Simulation<RotwkRules>, player: PlayerId, unit: Entity) {
    simulation
        .submit(
            player,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
}

#[test]
fn a_producer_that_dies_refunds_its_queue_and_frees_the_players_upgrade() {
    let (mut simulation, ids, _barracks, fortress) = upgrade_power_scenario();
    let economy = RotwkRules::economy_ids(ids);
    let held = |simulation: &Simulation<RotwkRules>| {
        crate::economy::player_entity(simulation.state(), economy, PLAYER)
            .and_then(|player| {
                simulation
                    .state()
                    .get(ids.player_upgrades_in_progress, player)
            })
            .map_or(0, |queued| queued.0.len())
    };
    let money = |simulation: &Simulation<RotwkRules>| {
        crate::economy::purse_of(simulation.state(), economy, PLAYER).amount()
    };
    select(&mut simulation, PLAYER, fortress);
    let before = money(&simulation);
    simulation
        .submit(PLAYER, RotwkCommand::PlayerUpgrade { button: 0 })
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(before - money(&simulation), 300, "the upgrade was charged");
    assert_eq!(held(&simulation), 1, "it is in production for the player");
    simulation.submit(PLAYER, RotwkCommand::DebugKill).unwrap();
    simulation.tick().unwrap();
    assert_eq!(money(&simulation), before, "the death refunded nothing");
    assert_eq!(
        held(&simulation),
        0,
        "the upgrade is still locked for the player at every other structure"
    );
}

#[test]
fn a_dead_structure_earns_nothing_while_its_corpse_stands() {
    let (mut simulation, ids) = economy_sim(false);
    let barracks = producers(&simulation, ids, PLAYER)
        .first()
        .copied()
        .unwrap();
    for _ in 0..12 {
        simulation.tick().unwrap();
    }
    let earning = purse(&simulation, ids, PLAYER);
    select(&mut simulation, PLAYER, barracks);
    simulation.submit(PLAYER, RotwkCommand::DebugKill).unwrap();
    simulation.tick().unwrap();
    let dead = purse(&simulation, ids, PLAYER);
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert!(
        earning > 500,
        "the barracks never earned while it was alive - the test proves nothing"
    );
    assert_eq!(
        purse(&simulation, ids, PLAYER),
        dead,
        "the corpse went on paying its owner"
    );
}

#[test]
fn an_object_upgrade_swaps_the_researching_structures_own_command_set() {
    let (mut simulation, ids, barracks, _fortress) = upgrade_power_scenario();
    select(&mut simulation, PLAYER, barracks);
    let before =
        crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER).amount();
    simulation
        .submit(PLAYER, RotwkCommand::ObjectUpgrade { button: 0 })
        .unwrap();
    simulation.tick().unwrap();
    let after_queue =
        crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER).amount();
    assert_eq!(
        before - after_queue,
        100,
        "BuildCost charged in full at queue time"
    );
    assert!(
        simulation
            .state()
            .get(ids.object_upgrades, barracks)
            .is_none_or(|held| held.0.is_empty()),
        "not complete yet"
    );
    simulation
        .submit(PLAYER, RotwkCommand::ObjectUpgrade { button: 0 })
        .unwrap();
    simulation.tick().unwrap();
    let after_second =
        crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER).amount();
    assert_eq!(
        after_queue, after_second,
        "the duplicate must not charge again"
    );
    let original_menu = simulation.state().get(ids.producer, barracks).unwrap().menu;
    for _ in 0..70 {
        simulation.tick().unwrap();
    }
    assert!(
        simulation
            .state()
            .get(ids.object_upgrades, barracks)
            .is_some_and(|held| !held.0.is_empty()),
        "the bit must be set on completion"
    );
    let new_menu = simulation.state().get(ids.producer, barracks).unwrap().menu;
    assert_ne!(
        original_menu, new_menu,
        "Producer.menu must swap to the level-2 roster"
    );
    simulation
        .submit(PLAYER, RotwkCommand::QueueUnit { button: 1 })
        .unwrap();
    simulation.tick().unwrap();
    let queued_label = simulation
        .state()
        .get(ids.producer, barracks)
        .and_then(|producer| producer.queue.entries().first().copied())
        .is_some();
    assert!(queued_label, "the level-2 button must now queue something");
}

#[test]
fn a_template_limited_to_one_refuses_the_second_press() {
    let (mut simulation, ids, barracks, _fortress) = upgrade_power_scenario();
    select(&mut simulation, PLAYER, barracks);
    simulation
        .submit(PLAYER, RotwkCommand::ObjectUpgrade { button: 0 })
        .unwrap();
    for _ in 0..70 {
        simulation.tick().unwrap();
    }
    let queued = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.producer, barracks)
            .map_or(0, |producer| producer.queue.entries().len())
    };
    simulation
        .submit(PLAYER, RotwkCommand::QueueUnit { button: 1 })
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(queued(&simulation), 1, "the first Veteran is queued");
    simulation
        .submit(PLAYER, RotwkCommand::QueueUnit { button: 1 })
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(
        queued(&simulation),
        1,
        "MaxSimultaneousOfType = 1 must refuse the second while the first is queued"
    );
}

#[test]
fn a_debug_spawned_structure_can_queue_a_unit() {
    let (mut simulation, ids, barracks, _fortress) = upgrade_power_scenario();
    let barracks_kind = simulation
        .rules()
        .kinds
        .kind("Barracks")
        .expect("Barracks is in the closure");
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnStructure {
                kind: barracks_kind.0,
                x: 150.0,
                y: 300.0,
                angle: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let spawned = *simulation
        .state()
        .store(ids.producer)
        .unwrap()
        .entities()
        .iter()
        .find(|&&entity| entity != barracks && entity != _fortress)
        .expect("the debug spawn must carry a Producer, same as the fixture's own barracks");
    select(&mut simulation, PLAYER, spawned);
    let before =
        crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER).amount();
    simulation
        .submit(PLAYER, RotwkCommand::QueueUnit { button: 0 })
        .unwrap();
    simulation.tick().unwrap();
    let after =
        crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER).amount();
    assert_eq!(
        before - after,
        50,
        "Build_Soldier's BuildCost charged in full"
    );
    assert_eq!(
        simulation
            .state()
            .get(ids.producer, spawned)
            .unwrap()
            .queue
            .entries()
            .len(),
        1,
        "the button must queue"
    );
}

#[test]
fn a_command_set_upgrade_never_fires_before_its_trigger_is_held() {
    let (simulation, ids, barracks, _fortress) = upgrade_power_scenario();
    let menu = simulation.state().get(ids.producer, barracks).unwrap().menu;
    assert_eq!(menu, Some(0), "the level-1 menu is the first one built");
}

#[test]
fn a_player_upgrade_unpauses_a_special_power_which_then_fires_and_recharges() {
    let (mut simulation, ids, _barracks, fortress) = upgrade_power_scenario();
    select(&mut simulation, PLAYER, fortress);
    let before = simulation.state().get(ids.power_book, fortress).cloned();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: None,
                at: None,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let after = simulation.state().get(ids.power_book, fortress).cloned();
    assert_eq!(before, after, "a refused fire must change nothing");

    simulation
        .submit(PLAYER, RotwkCommand::PlayerUpgrade { button: 0 })
        .unwrap();
    let purse_before =
        crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER).amount();
    simulation
        .submit(PLAYER, RotwkCommand::PlayerUpgrade { button: 0 })
        .unwrap();
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    let purse_after =
        crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER).amount();
    assert_eq!(
        purse_before - purse_after,
        300,
        "only one queue of a PLAYER upgrade may ever be charged"
    );
    let player_entity =
        crate::economy::player_entity(simulation.state(), RotwkRules::economy_ids(ids), PLAYER)
            .unwrap();
    assert!(
        simulation
            .state()
            .get(ids.player_upgrades, player_entity)
            .is_some_and(|held| !held.0.is_empty()),
        "the PLAYER upgrade must have completed"
    );

    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: None,
                at: None,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let fired = simulation
        .state()
        .get(ids.power_book, fortress)
        .and_then(|book| book.0.first().copied())
        .unwrap();
    assert!(
        fired.available_on.get() > 0,
        "firing must start the recharge timer"
    );

    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: None,
                at: None,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let after_second_fire = simulation
        .state()
        .get(ids.power_book, fortress)
        .and_then(|book| book.0.first().copied())
        .unwrap();
    assert_eq!(
        fired.available_on, after_second_fire.available_on,
        "a power still on cooldown must refuse and change nothing"
    );
}

#[test]
fn a_targetless_power_refuses_a_command_that_names_a_target() {
    let (mut simulation, ids, _barracks, fortress) = upgrade_power_scenario();
    select(&mut simulation, PLAYER, fortress);
    simulation
        .submit(PLAYER, RotwkCommand::PlayerUpgrade { button: 0 })
        .unwrap();
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    let before = simulation.state().get(ids.power_book, fortress).cloned();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: None,
                at: Some((10.0, 10.0)),
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let after = simulation.state().get(ids.power_book, fortress).cloned();
    assert_eq!(
        before, after,
        "a targetless button given a position must refuse and change nothing"
    );
}

#[test]
fn refused_upgrade_and_power_commands_leave_the_state_hash_unchanged() {
    let (mut with_commands, _ids, barracks, _fortress) = upgrade_power_scenario();
    let (mut without_commands, _, _, _) = upgrade_power_scenario();
    select(&mut with_commands, PLAYER, barracks);
    select(&mut without_commands, PLAYER, barracks);
    with_commands
        .submit(PLAYER, RotwkCommand::ObjectUpgrade { button: 99 })
        .unwrap();
    with_commands
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 99,
                target: None,
                at: None,
            },
        )
        .unwrap();
    for _ in 0..10 {
        with_commands.tick().unwrap();
        without_commands.tick().unwrap();
    }
    assert_eq!(
        with_commands.state_hash(),
        without_commands.state_hash(),
        "an out-of-range upgrade/power button must be indistinguishable from no command"
    );
}

#[test]
fn a_released_power_is_ready_on_the_release_frame_despite_available_at_start_no() {
    let (mut simulation, ids, _barracks, fortress) = upgrade_power_scenario();
    select(&mut simulation, PLAYER, fortress);
    simulation
        .submit(PLAYER, RotwkCommand::PlayerUpgrade { button: 0 })
        .unwrap();
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    let now = simulation.frame();
    let instance = simulation
        .state()
        .get(ids.power_book, fortress)
        .and_then(|book| book.0.first().copied())
        .unwrap();
    assert!(
        now.get() >= instance.available_on.get(),
        "a No-Obey release must be ready at once: now {}, available_on {}",
        now.get(),
        instance.available_on.get()
    );
}

#[test]
fn available_at_start_no_recharges_from_creation() {
    let text = "\
SpecialPower SpecialAbilityBeacon\n  Enum = SPECIAL_GENERAL_TARGETLESS\n  ReloadTime = 60000\nEnd\n\
CommandSet BeaconSet\n  1 = Fire_Beacon\nEnd\n\
CommandButton Fire_Beacon\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityBeacon\nEnd\n\
Object Beacon\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = BeaconSet\n  BuildCost = 500\n  BuildTime = 1\n  \
  Body = StructureBody Tag\n    MaxHealth = 500\n  End\n  \
  Behavior = SpecialPowerModule Tag\n    \
    SpecialPowerTemplate = SpecialAbilityBeacon\n    AvailableAtStart = No\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\nEnd\n";
    let doc =
        mint_sage_ini::Document::parse("t.ini", text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _line) =
        crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds = vec![String::from("Beacon")];
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let kind = kinds.kind("Beacon").unwrap();
    let spawn = kinds
        .spawn(
            &data.combat,
            kind,
            Vec2::new(0.0, 0.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let map = flat_map(64, 64);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![spawn])
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 10_000)
        .with_powers(power_names, power_reload);
    let mut simulation = Simulation::new(1, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let beacon = *simulation
        .state()
        .store(ids.power_book)
        .unwrap()
        .entities()
        .first()
        .unwrap();
    let at_spawn = simulation
        .state()
        .get(ids.power_book, beacon)
        .and_then(|book| book.0.first().copied())
        .unwrap();
    assert!(
        at_spawn.available_on.get() > 0,
        "AvailableAtStart = No must not be ready at spawn"
    );
    assert_eq!(
        at_spawn.available_on,
        Frame::new(1800),
        "60,000 ms is 1,800 ticks at 30 Hz"
    );
    for _ in 0..1800 {
        simulation.tick().unwrap();
    }
    assert!(
        simulation.frame().get() >= at_spawn.available_on.get(),
        "ready once the recharge seeded at creation has elapsed"
    );
}

#[test]
fn an_ai_controlled_player_researches_upgrades_and_fires_a_ready_power() {
    let (mut simulation, ids, barracks, fortress) = upgrade_power_scenario_with(true);
    for _ in 0..400 {
        simulation.tick().unwrap();
    }
    assert!(
        simulation
            .state()
            .get(ids.object_upgrades, barracks)
            .is_some_and(|held| !held.0.is_empty()),
        "the AI must have researched the barracks's OBJECT_UPGRADE"
    );
    let player_entity =
        crate::economy::player_entity(simulation.state(), RotwkRules::economy_ids(ids), PLAYER)
            .unwrap();
    assert!(
        simulation
            .state()
            .get(ids.player_upgrades, player_entity)
            .is_some_and(|held| !held.0.is_empty()),
        "the AI must have researched the fortress's PLAYER_UPGRADE"
    );
    assert!(
        simulation
            .state()
            .get(ids.power_book, fortress)
            .and_then(|book| book.0.first().copied())
            .is_some_and(|instance| instance.available_on.get() > 0),
        "the AI must have fired the fortress's power once it was unpaused"
    );
}

#[test]
fn a_match_with_upgrades_and_powers_replays_identically() {
    let run = || {
        let (mut simulation, _ids, barracks, fortress) = upgrade_power_scenario();
        select(&mut simulation, PLAYER, barracks);
        simulation
            .submit(PLAYER, RotwkCommand::ObjectUpgrade { button: 0 })
            .unwrap();
        for _ in 0..70 {
            simulation.tick().unwrap();
        }
        select(&mut simulation, PLAYER, fortress);
        simulation
            .submit(PLAYER, RotwkCommand::PlayerUpgrade { button: 0 })
            .unwrap();
        for _ in 0..40 {
            simulation.tick().unwrap();
        }
        simulation
            .submit(
                PLAYER,
                RotwkCommand::FireSpecialPower {
                    button: 0,
                    target: None,
                    at: None,
                },
            )
            .unwrap();
        let mut hashes = Vec::new();
        for _ in 0..30 {
            simulation.tick().unwrap();
            hashes.push(simulation.state_hash());
        }
        hashes
    };
    let first = run();
    let second = run();
    assert_eq!(first, second, "the same commands must replay identically");
    assert!(
        first.iter().any(|hash| *hash != first[0]),
        "the world must actually change"
    );
}

fn grant_scenario() -> (Simulation<RotwkRules>, Ids, Entity, Entity, Entity, Entity) {
    let text = "\
Upgrade Upgrade_RebuyGrant\n  Type = OBJECT\n  BuildCost = 1\n  BuildTime = 1\nEnd\n\
Upgrade Upgrade_RemoveTrigger\n  Type = OBJECT\n  BuildCost = 1\n  BuildTime = 1\nEnd\n\
Upgrade Upgrade_ObjectGrant\n  Type = OBJECT\nEnd\n\
Upgrade Upgrade_PlayerGrant\n  Type = PLAYER\nEnd\n\
CommandSet GranterBaseSet\n  1 = Buy_RebuyGrant\n  2 = Buy_RemoveTrigger\nEnd\n\
CommandSet GranterRebuiltSet\n  1 = Buy_RebuyGrant\n  2 = Buy_RemoveTrigger\nEnd\n\
CommandButton Buy_RebuyGrant\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_RebuyGrant\nEnd\n\
CommandButton Buy_RemoveTrigger\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_RemoveTrigger\nEnd\n\
CommandSet WatcherBaseSet\n  1 = Buy_RebuyGrant\nEnd\n\
CommandSet WatcherGrantedSet\n  1 = Buy_RebuyGrant\nEnd\n\
Object Granter\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = GranterBaseSet\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag\n    MaxHealth = 100\n  End\n  \
  Behavior = ProductionUpdate Tag\n    MaxQueueEntries = 3\n  End\n  \
  Behavior = CommandSetUpgrade TagC\n    TriggeredBy = Upgrade_RebuyGrant\n    \
    CommandSet = GranterRebuiltSet\n  End\n  \
  Behavior = RemoveUpgradeUpgrade TagR\n    TriggeredBy = Upgrade_RemoveTrigger\n    \
    UpgradeToRemove = Upgrade_RebuyGrant\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\nEnd\n\
Object SelfGranter\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag\n    MaxHealth = 100\n  End\n  \
  Behavior = GrantUpgradeCreate TagG\n    UpgradeToGrant = Upgrade_ObjectGrant\n  End\n  \
  Behavior = CommandSetUpgrade TagS\n    TriggeredBy = Upgrade_ObjectGrant\n    \
    CommandSet = GranterRebuiltSet\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\nEnd\n\
Object PlayerGranter\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag\n    MaxHealth = 100\n  End\n  \
  Behavior = GrantUpgradeCreate TagP\n    UpgradeToGrant = Upgrade_PlayerGrant\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\nEnd\n\
Object Watcher\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = WatcherBaseSet\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag\n    MaxHealth = 100\n  End\n  \
  Behavior = CommandSetUpgrade TagW\n    TriggeredBy = Upgrade_PlayerGrant\n    \
    CommandSet = WatcherGrantedSet\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\nEnd\n\
Object Bystander\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag\n    MaxHealth = 100\n  End\n  \
  Behavior = CommandSetUpgrade TagB\n    TriggeredBy = Upgrade_ObjectGrant\n    \
    CommandSet = GranterRebuiltSet\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\nEnd\n";
    let doc =
        mint_sage_ini::Document::parse("t.ini", text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _line) =
        crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds = vec![
        String::from("Granter"),
        String::from("SelfGranter"),
        String::from("PlayerGranter"),
        String::from("Watcher"),
        String::from("Bystander"),
    ];
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let upgrade_menus = kinds.upgrades(&data);

    let spawn_of = |name: &str, x: f32| {
        let kind = kinds.kind(name).unwrap();
        let mut spawn = kinds
            .spawn(&data.combat, kind, Vec2::new(x, 0.0), Facing::EAST, PLAYER)
            .unwrap();
        spawn.producer = kinds.producer(&production, kind, Vec2::new(x, 0.0), Facing::EAST);
        spawn
    };
    let spawns = vec![
        spawn_of("Granter", 0.0),
        spawn_of("SelfGranter", 200.0),
        spawn_of("PlayerGranter", 400.0),
        spawn_of("Watcher", 600.0),
        spawn_of("Bystander", 800.0),
    ];

    let map = flat_map(64, 64);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 10_000)
        .with_menus(production.menus.clone())
        .with_upgrades(upgrade_menus, Arc::clone(&data.upgrade_bits));
    let simulation = Simulation::new(1, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entity_of = |name: &str| -> Entity {
        let want = kinds.kind(name).unwrap();
        simulation
            .state()
            .store(ids.kind)
            .unwrap()
            .iter()
            .find(|(_, kind)| **kind == want)
            .map(|(entity, _)| entity)
            .unwrap()
    };
    let granter = entity_of("Granter");
    let self_granter = entity_of("SelfGranter");
    let watcher = entity_of("Watcher");
    let bystander = entity_of("Bystander");
    (simulation, ids, granter, self_granter, watcher, bystander)
}

#[test]
fn initial_upgrades_reach_an_objects_module_at_creation() {
    let text = "\
Upgrade Upgrade_MadeUpFaction\n  Type = PLAYER\nEnd\n\
CommandSet WatcherBaseSet\n  1 = Dummy\nEnd\n\
CommandSet WatcherGrantedSet\n  1 = Dummy\nEnd\n\
CommandButton Dummy\n  Command = UNIT_BUILD\n  Object = Watcher\nEnd\n\
Object Watcher\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = WatcherBaseSet\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag\n    MaxHealth = 100\n  End\n  \
  Behavior = CommandSetUpgrade TagW\n    TriggeredBy = Upgrade_MadeUpFaction\n    \
    CommandSet = WatcherGrantedSet\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\nEnd\n";
    let doc =
        mint_sage_ini::Document::parse("t.ini", text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _line) =
        crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds = vec![String::from("Watcher")];
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let upgrade_menus = kinds.upgrades(&data);
    let bit = data.upgrade_bits.bit("Upgrade_MadeUpFaction").unwrap();
    let mut mask = mint_sim::UpgradeMask::EMPTY;
    mask.set(bit);

    let kind = kinds.kind("Watcher").unwrap();
    let mut spawn = kinds
        .spawn(
            &data.combat,
            kind,
            Vec2::new(0.0, 0.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    spawn.producer = kinds.producer(&production, kind, Vec2::new(0.0, 0.0), Facing::EAST);

    let map = flat_map(64, 64);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![spawn])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 10_000)
        .with_menus(production.menus.clone())
        .with_upgrades(upgrade_menus, Arc::clone(&data.upgrade_bits))
        .with_initial_upgrades(vec![mask]);
    let simulation = Simulation::new(1, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let watcher = *simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .entities()
        .first()
        .unwrap();
    let state = simulation
        .state()
        .get(simulation.rules().upgrade_state_id().unwrap(), watcher)
        .cloned();
    assert_eq!(
        state.and_then(|s| s.command_set).as_deref(),
        Some("WatcherGrantedSet"),
        "InitialUpgrades must be held before the first object is created"
    );
}

#[test]
fn grant_upgrade_create_grants_an_object_upgrade_that_fires_at_creation() {
    let (simulation, _ids, _granter, self_granter, _watcher, bystander) = grant_scenario();
    let state_id = simulation.rules().upgrade_state_id().unwrap();
    let self_state = simulation.state().get(state_id, self_granter).cloned();
    assert_eq!(
        self_state.and_then(|s| s.command_set).as_deref(),
        Some("GranterRebuiltSet"),
        "the self-granted OBJECT upgrade must have fired at creation"
    );
    let bystander_state = simulation.state().get(state_id, bystander).cloned();
    assert_eq!(
        bystander_state.and_then(|s| s.command_set),
        None,
        "an OBJECT-scoped grant must not reach a different object"
    );
}

#[test]
fn grant_upgrade_create_grants_a_player_upgrade_a_later_object_sees() {
    let (simulation, _ids, _granter, _self_granter, watcher, _bystander) = grant_scenario();
    let state = simulation
        .state()
        .get(simulation.rules().upgrade_state_id().unwrap(), watcher)
        .cloned();
    assert_eq!(
        state.and_then(|s| s.command_set).as_deref(),
        Some("WatcherGrantedSet"),
        "a later-created object must see the PLAYER-scoped grant"
    );
}

#[test]
fn removing_an_upgrade_re_arms_its_trigger_for_a_second_grant() {
    let (mut simulation, ids, granter, _self_granter, _watcher, _bystander) = grant_scenario();
    let bit = simulation
        .rules()
        .upgrade_bits
        .bit("Upgrade_RebuyGrant")
        .unwrap();

    assert!(
        simulation
            .rules()
            .affected_by_upgrade(simulation.state(), ids, granter, bit),
        "unfired, the trigger would react"
    );

    select(&mut simulation, PLAYER, granter);
    simulation
        .submit(PLAYER, RotwkCommand::ObjectUpgrade { button: 0 })
        .unwrap();
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        simulation
            .state()
            .get(simulation.rules().upgrade_state_id().unwrap(), granter)
            .and_then(|s| s.command_set.clone())
            .as_deref(),
        Some("GranterRebuiltSet"),
        "fired once"
    );
    assert!(
        !simulation
            .rules()
            .affected_by_upgrade(simulation.state(), ids, granter, bit),
        "latched: would not fire again"
    );

    simulation
        .submit(PLAYER, RotwkCommand::ObjectUpgrade { button: 1 })
        .unwrap();
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert!(
        simulation
            .rules()
            .affected_by_upgrade(simulation.state(), ids, granter, bit),
        "removed: C2 re-armed the trigger"
    );

    simulation
        .submit(PLAYER, RotwkCommand::ObjectUpgrade { button: 0 })
        .unwrap();
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert!(
        !simulation
            .rules()
            .affected_by_upgrade(simulation.state(), ids, granter, bit),
        "fired a second time, and latched again"
    );
}

#[test]
fn remove_upgrade_upgrade_with_remove_from_all_player_objects_clears_both_masks() {
    let text = "\
Upgrade Upgrade_SelfTrigger\n  Type = OBJECT\nEnd\n\
Upgrade Upgrade_PlayerBit\n  Type = PLAYER\nEnd\n\
Upgrade Upgrade_KeepBit\n  Type = OBJECT\nEnd\n\
Object Keep\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag\n    MaxHealth = 100\n  End\n  \
  Behavior = GrantUpgradeCreate TagK\n    UpgradeToGrant = Upgrade_KeepBit\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\nEnd\n\
Object Deliverer\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag\n    MaxHealth = 100\n  End\n  \
  Behavior = GrantUpgradeCreate TagP\n    UpgradeToGrant = Upgrade_PlayerBit\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\nEnd\n\
Object Champion\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag\n    MaxHealth = 100\n  End\n  \
  Behavior = GrantUpgradeCreate TagS\n    UpgradeToGrant = Upgrade_SelfTrigger\n  End\n  \
  Behavior = RemoveUpgradeUpgrade TagR\n    TriggeredBy = Upgrade_SelfTrigger\n    \
    UpgradeToRemove = Upgrade_PlayerBit Upgrade_KeepBit\n    \
    RemoveFromAllPlayerObjects = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\nEnd\n";
    let doc =
        mint_sage_ini::Document::parse("t.ini", text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _line) =
        crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds = vec![
        String::from("Keep"),
        String::from("Deliverer"),
        String::from("Champion"),
    ];
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let upgrade_menus = kinds.upgrades(&data);

    let spawn_of = |name: &str, x: f32| {
        let kind = kinds.kind(name).unwrap();
        let mut spawn = kinds
            .spawn(&data.combat, kind, Vec2::new(x, 0.0), Facing::EAST, PLAYER)
            .unwrap();
        spawn.producer = kinds.producer(&production, kind, Vec2::new(x, 0.0), Facing::EAST);
        spawn
    };
    let spawns = vec![
        spawn_of("Keep", 0.0),
        spawn_of("Deliverer", 200.0),
        spawn_of("Champion", 400.0),
    ];

    let map = flat_map(64, 64);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 10_000)
        .with_menus(production.menus.clone())
        .with_upgrades(upgrade_menus, Arc::clone(&data.upgrade_bits));
    let simulation = Simulation::new(1, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let keep = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(_, kind)| **kind == kinds.kind("Keep").unwrap())
        .map(|(entity, _)| entity)
        .unwrap();
    let player_bit = data.upgrade_bits.bit("Upgrade_PlayerBit").unwrap();
    let keep_bit = data.upgrade_bits.bit("Upgrade_KeepBit").unwrap();

    let player_entity =
        crate::economy::player_entity(simulation.state(), RotwkRules::economy_ids(ids), PLAYER)
            .unwrap();
    assert!(
        !simulation
            .state()
            .get(ids.player_upgrades, player_entity)
            .is_some_and(|held| held.0.contains(player_bit)),
        "the PLAYER bit must be cleared from the player by Champion's creation"
    );
    assert!(
        !simulation
            .state()
            .get(ids.object_upgrades, keep)
            .is_some_and(|held| held.0.contains(keep_bit)),
        "RemoveFromAllPlayerObjects must clear the OBJECT bit from Keep too, not just Champion"
    );
}

#[test]
fn a_mover_inside_its_path_back_off_is_handed_an_empty_search_budget() {
    let (_simulation, _ids, units) = simulation();
    let (deferred, other) = (units[0], units[1]);
    let mut budgets = PathBudgets::new(None, vec![deferred]);
    assert_eq!(
        budgets.budget_for(deferred).remaining(),
        0,
        "a mover still inside its back-off must be refused before a node is spent"
    );
    assert!(
        budgets.budget_for(other).remaining() > 0,
        "every other mover still draws on the tick's shared ceiling"
    );
}

#[test]
fn a_mod_units_own_stance_joins_its_own_modifier_list_when_it_is_commanded() {
    let text = "\
ModifierList ModGuardAggressive\n  Modifier = ARMOR -15%\nEnd\n\
StanceTemplate ModTemplate\n  Stance Aggressive\n    \
  AttributeModifier ModGuardAggressive\n  End\nEnd\n\
Object ModGuard\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag\n    MaxHealth = 100\n  End\n  \
  Behavior = StancesBehavior Tag2\n    StanceTemplate = ModTemplate\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\nEnd\n";
    let doc =
        mint_sage_ini::Document::parse("t.ini", text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _line) =
        crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &[String::from("ModGuard")]);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let kind = kinds.kind("ModGuard").unwrap();
    let spawn = kinds
        .spawn(
            &data.combat,
            kind,
            Vec2::new(60.0, 60.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let map = flat_map(64, 64);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![spawn])
        .with_combat(data.combat.clone())
        .with_stances(Arc::clone(&data.stances))
        .with_construction(Arc::clone(&kinds), production);
    let mut simulation = Simulation::new(3, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let unit = *simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .first()
        .unwrap();
    let armour = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.bonuses, unit)
            .map_or(0.0, |bonuses| bonuses.0.armor)
    };
    assert_eq!(
        simulation.rules().stance_of(simulation.state(), ids, unit),
        Some(mint_sage_ini::Stance::Battle),
        "every unit starts in Battle, which no template gives a modifier"
    );
    assert_eq!(armour(&simulation), 0.0);
    select(&mut simulation, PLAYER, unit);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetStance {
                stance: mint_sage_ini::Stance::Aggressive.index(),
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(
        simulation.rules().stance_of(simulation.state(), ids, unit),
        Some(mint_sage_ini::Stance::Aggressive)
    );
    assert!(
        (armour(&simulation) - -0.15).abs() < 1e-6,
        "the mod stance's own list joined: {}",
        armour(&simulation)
    );
}

#[test]
fn a_goal_move_is_judged_against_a_tenth_of_the_distance_to_it() {
    let planned = Vec2::new(0.0, 0.0);
    let goal = Vec2::new(20.0, 0.0);
    assert!(super::movement::goal_moved(
        Vec2::new(20.0, 100.0),
        planned,
        goal
    ));
    assert!(!super::movement::goal_moved(
        Vec2::new(20.0, 300.0),
        planned,
        goal
    ));
}

#[test]
fn a_flammable_structure_placed_on_a_settled_grid_overwrites_its_cell() {
    assert_eq!(hut_cell_fuel_after_placing(false), Some(77));
    assert_eq!(
        hut_cell_fuel_after_placing(true),
        Some(100),
        "a unit already standing there claims the cell first"
    );
}

fn hut_cell_fuel_after_placing(blocked: bool) -> Option<u16> {
    use crate::rules::terrain_fire::{singleton_of, FireGrid};
    use mint_sage_ini::fire_logic_system::{FireLogicSystem, FireMaterial};

    let text = "Object Hut
  Side = Men
  KindOf = STRUCTURE SELECTABLE IMMOBILE
    Body = StructureBody Tag
    MaxHealth = 100
  End
    Flammability
    Fuel = 77
    MaxBurnRate = 5
    Decay = 1
    Resistance = 2
  End
    Geometry = CYLINDER
  GeometryMajorRadius = 4
End
";
    let doc =
        mint_sage_ini::Document::parse("t.ini", text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _line) =
        crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &[String::from("Hut")]);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let hut = kinds.kind("Hut").unwrap();
    let production = kinds.production(&data);
    let system = FireLogicSystem {
        cell_types: [
            FireMaterial::default(),
            FireMaterial {
                fuel: 100,
                max_burn_rate: 20,
                decay: 0,
                resistance: 0,
            },
            FireMaterial::default(),
            FireMaterial::default(),
        ],
        ..FireLogicSystem::default()
    };
    let grid = FireGrid::build(20, 20, |_, _| Some(1), &system);
    let map = flat_map(64, 64);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let blocker = blocked.then(|| armed_spawn(100.0, 100.0, PLAYER, None, None, 100.0));
    let mut simulation = Simulation::new(
        1,
        RotwkRules::new(passability, blocker.into_iter().collect())
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production)
            .with_fire_grid(grid),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let fuel = |simulation: &Simulation<RotwkRules>| {
        let grid = singleton_of(simulation.state(), ids.fire_grid).unwrap();
        let grid = simulation.state().get(ids.fire_grid, grid).unwrap();
        (grid.is_settled(), grid.test_fuel(10, 10))
    };
    simulation.tick().unwrap();
    assert_eq!(fuel(&simulation), (true, Some(100)));
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnStructure {
                kind: hut.0,
                x: 100.0,
                y: 100.0,
                angle: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    fuel(&simulation).1
}

#[test]
fn a_lit_cell_damages_an_own_unit_standing_in_it_with_no_attacker() {
    use crate::rules::terrain_fire::{FireGrid, FireLighting, FireLogicKind, DEFAULT_DAMAGE_ARC};
    use mint_sage_ini::fire_logic_system::{FireLogicSystem, FireMaterial};

    let mut combat = CombatData::new();
    combat.damage_types.intern("LOGICAL_FIRE").unwrap();
    combat.death_types.intern("BURNED").unwrap();
    let armour = combat
        .armors
        .insert(
            "TestArmor",
            mint_sage_combat::ArmorTemplate::from_rows(&[], &mut combat.damage_types),
        )
        .unwrap();

    let map = flat_map(64, 64);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let victim = armed_spawn(55.0, 55.0, PLAYER, None, Some(armour), 100.0);

    let system = FireLogicSystem {
        cell_types: [
            FireMaterial::default(),
            FireMaterial {
                fuel: 100,
                max_burn_rate: 20,
                decay: 0,
                resistance: 0,
            },
            FireMaterial::default(),
            FireMaterial::default(),
        ],
        ..FireLogicSystem::default()
    };
    let mut grid = FireGrid::build(
        10,
        10,
        |x, y| if (x, y) == (6, 6) { Some(1) } else { None },
        &system,
    );
    grid.light(
        &FireLighting {
            logic_type: FireLogicKind::IncreaseBurnRate {
                existing_only: false,
                arc: false,
            },
            radius: 1.0,
            damage: 5.0,
            damage_arc: DEFAULT_DAMAGE_ARC,
            min_max_burn_rate: None,
            min_decay: None,
            max_resistance: 10_000,
        },
        (60.0, 60.0),
        None,
        &|_, _| false,
    );

    let mut simulation = Simulation::new(
        1,
        RotwkRules::new(passability, vec![victim])
            .with_combat(combat)
            .with_fire_grid(grid),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entity = simulation.state().store(ids.body).unwrap().entities()[0];
    let before = simulation
        .state()
        .get(ids.health, entity)
        .unwrap()
        .0
        .current();
    simulation.tick().unwrap();
    let after = simulation
        .state()
        .get(ids.health, entity)
        .unwrap()
        .0
        .current();
    assert_eq!(before - after, 5.0);
}

fn site_over_a_unit_of(enemy: bool) -> bool {
    let (data, kinds) = construction_kinds();
    let production = kinds.production(&data);
    let porter_kind = kinds.kind("Porter").unwrap();
    let spawn = |x: f32, owner: PlayerId| {
        kinds
            .spawn(
                &data.combat,
                porter_kind,
                Vec2::new(x, 200.0),
                Facing::EAST,
                owner,
            )
            .unwrap()
    };
    let units = vec![spawn(140.0, PLAYER), spawn(150.0, PlayerId::new(1))];
    let passability = Passability::build(&flat_map(160, 60), 5.0 / 128.0).unwrap();
    let relations = crate::alliance::MapRelations {
        enemies: if enemy { vec![PLAYER] } else { Vec::new() },
        ..Default::default()
    };
    let mut simulation = Simulation::new(
        52,
        RotwkRules::new(passability, units)
            .with_players(1)
            .with_teams(vec![None])
            .with_map_players(1, vec![String::from("Other")], vec![relations])
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production)
            .with_economy(Vec::new(), 1_000),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let builder = simulation.state().store(ids.body).unwrap().entities()[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![builder],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Construct {
                kind: kinds.kind("Shed").unwrap().0,
                x: 150.0,
                y: 200.0,
                angle: 0.0,
            },
        )
        .unwrap();
    (0..200).any(|_| {
        simulation.tick().unwrap();
        simulation
            .state()
            .store(ids.construction)
            .is_some_and(|store| !store.entities().is_empty())
    })
}

#[test]
fn a_neutral_unit_on_the_site_does_not_refuse_the_building_but_an_enemy_does() {
    assert!(site_over_a_unit_of(false), "neutral");
    assert!(!site_over_a_unit_of(true), "enemy");
}
