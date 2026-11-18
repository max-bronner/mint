use super::*;
use crate::weapon::WeaponStore;

fn weapon(store: &mut WeaponStore, name: &str) -> WeaponId {
    store
        .insert(name, crate::weapon::WeaponTemplate::default())
        .unwrap()
}

struct Fixture {
    slots: [Option<SlotCandidate>; SLOT_COUNT],
}

impl Fixture {
    fn new(store: &mut WeaponStore) -> Self {
        let mut slots: [Option<SlotCandidate>; SLOT_COUNT] = [None; SLOT_COUNT];
        for (index, slot) in slots.iter_mut().enumerate().take(3) {
            let name = format!("W{index}");
            *slot = Some(SlotCandidate {
                arm: SlotArm {
                    weapon: weapon(store, &name),
                    sources: SourceMask::ANY,
                    preferred_against: ConditionMask::NONE,
                    only_against: ConditionMask::NONE,
                    only_in_condition: ConditionMask::NONE,
                },
                fitness: SlotFitness {
                    class_allowed: true,
                    damage: 10.0,
                    unresistable: false,
                    ready: true,
                    inside_minimum: false,
                    out_of_ammo: false,
                },
            });
        }
        Self { slots }
    }
}

#[test]
fn equal_damage_across_slots_fires_the_primary() {
    let mut store = WeaponStore::new();
    let fixture = Fixture::new(&mut store);
    assert_eq!(
        choose_slot(
            &fixture.slots,
            ConditionMask::NONE,
            CommandSource::Ai,
            ConditionMask::NONE
        ),
        Some(WeaponSlot::Primary)
    );
}

#[test]
fn a_preferred_slot_beats_a_higher_damage_one() {
    let mut store = WeaponStore::new();
    let mut fixture = Fixture::new(&mut store);
    let structure = ConditionMask::bit(0);
    fixture.slots[0].as_mut().unwrap().fitness.damage = 500.0;
    fixture.slots[2].as_mut().unwrap().arm.preferred_against = structure;
    assert_eq!(
        choose_slot(
            &fixture.slots,
            structure,
            CommandSource::Ai,
            ConditionMask::NONE
        ),
        Some(WeaponSlot::Tertiary)
    );
}

#[test]
fn only_against_excludes_a_slot_the_victim_does_not_match() {
    let mut store = WeaponStore::new();
    let mut fixture = Fixture::new(&mut store);
    let structure = ConditionMask::bit(0);
    let infantry = ConditionMask::bit(1);
    fixture.slots[0].as_mut().unwrap().arm.only_against = structure;
    fixture.slots[1].as_mut().unwrap().fitness.damage = 1.0;
    fixture.slots[2] = None;
    assert_eq!(
        choose_slot(
            &fixture.slots,
            infantry,
            CommandSource::Ai,
            ConditionMask::NONE
        ),
        Some(WeaponSlot::Secondary)
    );
}

#[test]
fn a_slot_the_ai_may_not_choose_is_still_available_to_a_player_order() {
    let mut store = WeaponStore::new();
    let mut fixture = Fixture::new(&mut store);
    let tertiary = fixture.slots[2].as_mut().unwrap();
    tertiary.arm.sources = SourceMask::parse(&["FROM_PLAYER", "FROM_SCRIPT"]).unwrap();
    fixture.slots[0].as_mut().unwrap().fitness.ready = false;
    fixture.slots[1].as_mut().unwrap().fitness.ready = false;
    assert_eq!(
        choose_slot(
            &fixture.slots,
            ConditionMask::NONE,
            CommandSource::Ai,
            ConditionMask::NONE
        ),
        Some(WeaponSlot::Primary)
    );
    assert_eq!(
        choose_slot(
            &fixture.slots,
            ConditionMask::NONE,
            CommandSource::Player,
            ConditionMask::NONE
        ),
        Some(WeaponSlot::Tertiary)
    );
}

#[test]
fn a_reloading_slot_loses_to_a_ready_one_of_less_damage() {
    let mut store = WeaponStore::new();
    let mut fixture = Fixture::new(&mut store);
    let primary = fixture.slots[0].as_mut().unwrap();
    primary.fitness.damage = 100.0;
    primary.fitness.ready = false;
    assert_eq!(
        choose_slot(
            &fixture.slots,
            ConditionMask::NONE,
            CommandSource::Ai,
            ConditionMask::NONE
        ),
        Some(WeaponSlot::Secondary)
    );
}

#[test]
fn a_ready_primary_keeps_the_shot_over_a_stronger_slot() {
    let mut store = WeaponStore::new();
    let mut fixture = Fixture::new(&mut store);
    fixture.slots[1].as_mut().unwrap().fitness.damage = 100.0;
    assert_eq!(
        choose_slot(
            &fixture.slots,
            ConditionMask::NONE,
            CommandSource::Ai,
            ConditionMask::NONE
        ),
        Some(WeaponSlot::Primary)
    );
}

#[test]
fn a_preferred_slot_is_unready_inside_its_minimum_range() {
    let mut store = WeaponStore::new();
    let mut fixture = Fixture::new(&mut store);
    let structure = ConditionMask::bit(0);
    let tertiary = fixture.slots[2].as_mut().unwrap();
    tertiary.arm.preferred_against = structure;
    tertiary.fitness.inside_minimum = true;
    assert_eq!(
        choose_slot(
            &fixture.slots,
            structure,
            CommandSource::Ai,
            ConditionMask::NONE
        ),
        Some(WeaponSlot::Primary)
    );
}

#[test]
fn an_unknown_auto_choose_source_is_refused() {
    assert!(SourceMask::parse(&["FROM_PLAYER", "FROM_NOWHERE"]).is_err());
    assert_eq!(SourceMask::parse(&["NONE"]), Ok(SourceMask::NONE));
}

#[test]
fn only_in_condition_gates_a_slot_on_the_shooters_own_conditions() {
    let mut store = WeaponStore::new();
    let mut fixture = Fixture::new(&mut store);
    let moving = ConditionMask::bit(5);
    fixture.slots[0].as_mut().unwrap().fitness.ready = false;
    fixture.slots[1] = None;
    let tertiary = fixture.slots[2].as_mut().unwrap();
    tertiary.arm.only_in_condition = moving;
    assert_eq!(
        choose_slot(
            &fixture.slots,
            ConditionMask::NONE,
            CommandSource::Ai,
            ConditionMask::NONE
        ),
        Some(WeaponSlot::Primary)
    );
    assert_eq!(
        choose_slot(
            &fixture.slots,
            ConditionMask::NONE,
            CommandSource::Ai,
            moving
        ),
        Some(WeaponSlot::Tertiary)
    );
}

#[test]
fn an_unrecognised_weapon_choice_criteria_is_refused() {
    assert_eq!(
        WeaponChoiceCriteria::parse("SELECT_AT_RANDOM"),
        Some(WeaponChoiceCriteria::SelectAtRandom)
    );
    assert_eq!(WeaponChoiceCriteria::parse("SELECT_AT_WHIM"), None);
}
