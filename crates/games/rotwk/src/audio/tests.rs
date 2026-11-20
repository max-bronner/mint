#![allow(clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;

use super::{
    flags_for, pick_by_total, pick_damaged_or_plain, select_voice_field, transitions,
    weather_sound_change, Change, WeatherSoundChange, VOICE_SELECT, VOICE_SELECT_BATTLE,
    VOICE_SELECT_UNDER_CONSTRUCTION,
};
use crate::render_state::UnitView;
use mint_sage_ini::WeatherSound;
use mint_sage_modules::WeatherKind;

fn site(world: &mut mint_sim::World, conditions: mint_sage_scene::Conditions) -> UnitView {
    UnitView {
        entity: world.spawn().unwrap(),
        position: mint_movement::Vec2::new(0.0, 0.0),
        ground: 0.0,
        facing: 0.0,
        radius: 5.0,
        height: 5.0,
        kind_of: mint_sage_asset::KindOf::STRUCTURE,
        targetable: false,
        pick_type: mint_sage_asset::PickType::Selectable,
        owner: mint_sim::PlayerId::new(0),
        selected: false,
        moving: false,
        health: Some(1.0),
        dying: false,
        created: 0,
        visibility: mint_sage_shroud::ObjectVisibility::Clear,
        speed: 0.0,
        weapon_cycle: 0.0,
        attack_cycle: 0,
        attack_slot: None,
        kind: Some(0),
        art_kind: Some(0),
        told_kind: Some(0),
        hover_kind: Some(0),
        conditions,
        build_elapsed: Some(0),
        build_total: None,
        under_repair: false,
        held: false,
    }
}

#[test]
fn a_loop_stops_when_its_owner_vanishes() {
    let mut world = mint_sim::World::new();
    let unit = site(
        &mut world,
        mint_sage_scene::Conditions::ACTIVELY_BEING_CONSTRUCTED,
    );
    let previous = vec![(unit.entity.index(), flags_for(&unit))];
    let changes = transitions(&previous, &[]);
    assert_eq!(changes.len(), 1);
    assert!(matches!(changes[0], Change::Stop { .. }));
}

#[test]
fn the_loop_does_not_start_before_the_drawable_is_actively_building() {
    let mut world = mint_sim::World::new();
    let awaiting = site(
        &mut world,
        mint_sage_scene::Conditions::AWAITING_CONSTRUCTION,
    );
    let now = vec![(awaiting.entity.index(), flags_for(&awaiting))];
    assert!(transitions(&[], &now).is_empty());

    let mut world = mint_sim::World::new();
    let building = site(
        &mut world,
        mint_sage_scene::Conditions::ACTIVELY_BEING_CONSTRUCTED,
    );
    let now = vec![(building.entity.index(), flags_for(&building))];
    let changes = transitions(&[], &now);
    assert_eq!(changes.len(), 1);
    assert!(matches!(changes[0], Change::Start { .. }));
}

#[test]
fn a_site_under_construction_picks_its_own_select_voice() {
    use mint_sage_scene::Conditions;

    assert_eq!(
        select_voice_field(Conditions::ACTIVELY_BEING_CONSTRUCTED),
        VOICE_SELECT_UNDER_CONSTRUCTION
    );
    assert_eq!(
        select_voice_field(Conditions::AWAITING_CONSTRUCTION),
        VOICE_SELECT_UNDER_CONSTRUCTION
    );
    assert_eq!(
        select_voice_field(Conditions::PARTIALLY_CONSTRUCTED),
        VOICE_SELECT_UNDER_CONSTRUCTION
    );
    assert_eq!(select_voice_field(Conditions::NONE), VOICE_SELECT);
    assert_eq!(select_voice_field(Conditions::ENGAGED), VOICE_SELECT_BATTLE);
    assert_eq!(
        select_voice_field(Conditions::ENGAGED.union(Conditions::ACTIVELY_BEING_CONSTRUCTED)),
        VOICE_SELECT_BATTLE
    );
}

#[test]
fn a_recycled_slot_stops_the_old_owners_loop_and_starts_the_new_one() {
    let mut world = mint_sim::World::new();
    let building = mint_sage_scene::Conditions::ACTIVELY_BEING_CONSTRUCTED;
    let old = site(&mut world, building);
    world.despawn(old.entity);
    let new = site(&mut world, building);
    assert_eq!(old.entity.index(), new.entity.index());
    let previous = vec![(old.entity.index(), flags_for(&old))];
    let now = vec![(new.entity.index(), flags_for(&new))];
    let changes = transitions(&previous, &now);
    assert!(matches!(
        changes[..],
        [Change::Stop { .. }, Change::Start { .. }]
    ));
}

#[test]
fn the_vote_plays_the_record_most_units_chose() {
    let entries = [("A", 1), ("B", 1), ("B", 1)];
    assert_eq!(pick_by_total(entries), Some("B"));
}

#[test]
fn the_crowd_vote_is_by_weight_not_by_unit_count() {
    let entries = [("A", 1), ("A", 1), ("A", 1), ("B", 5)];
    assert_eq!(pick_by_total(entries), Some("B"));
}

#[test]
fn a_tie_keeps_the_first_record_seen() {
    let entries = [("A", 2), ("B", 2)];
    assert_eq!(pick_by_total(entries), Some("A"));
}

#[test]
fn the_damaged_field_is_tried_only_while_damaged_and_declared() {
    assert_eq!(
        pick_damaged_or_plain("Plain", "Damaged", true, true),
        "Damaged"
    );
    assert_eq!(
        pick_damaged_or_plain("Plain", "Damaged", true, false),
        "Plain"
    );
    assert_eq!(
        pick_damaged_or_plain("Plain", "Damaged", false, true),
        "Plain"
    );
}

#[test]
fn a_weather_kind_change_plays_or_stops_once_and_a_repeat_does_nothing() {
    let mut sounds = BTreeMap::new();
    sounds.insert(
        "RAINY".to_owned(),
        WeatherSound {
            sound: Some("TestRainLoop".to_owned()),
            has_lightning: false,
        },
    );

    let to_rainy = weather_sound_change(WeatherKind::None, WeatherKind::Rainy, false, &sounds);
    assert_eq!(to_rainy, WeatherSoundChange::Play("TestRainLoop"));

    let repeat = weather_sound_change(WeatherKind::Rainy, WeatherKind::Rainy, true, &sounds);
    assert_eq!(repeat, WeatherSoundChange::None);

    let to_sunny = weather_sound_change(WeatherKind::Rainy, WeatherKind::Sunny, true, &sounds);
    assert_eq!(to_sunny, WeatherSoundChange::Stop);

    let still_sunny = weather_sound_change(WeatherKind::None, WeatherKind::Sunny, false, &sounds);
    assert_eq!(still_sunny, WeatherSoundChange::None);
}
