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
                    valid: true,
                    unresistable: false,
                    ready_status: true,
                    out_of_ammo: false,
                    range: 100.0,
                    distance: 50.0,
                    minimum: 0.0,
                    melee: false,
                    grab: false,
                },
            });
        }
        Self { slots }
    }

    fn slot(&mut self, index: usize) -> &mut SlotCandidate {
        self.slots[index].as_mut().unwrap()
    }

    fn choose(
        &self,
        victim: ConditionMask,
        source: CommandSource,
        active: ConditionMask,
        criteria: WeaponChoiceCriteria,
    ) -> Option<WeaponSlot> {
        choose_slot(&self.slots, victim, source, active, criteria, &mut || 0.5)
    }

    fn most_damage(&self, victim: ConditionMask) -> Option<WeaponSlot> {
        self.choose(
            victim,
            CommandSource::Ai,
            ConditionMask::NONE,
            WeaponChoiceCriteria::PreferMostDamage,
        )
    }

    fn under(&self, criteria: WeaponChoiceCriteria) -> Option<WeaponSlot> {
        self.choose(
            ConditionMask::NONE,
            CommandSource::Ai,
            ConditionMask::NONE,
            criteria,
        )
    }
}

#[test]
fn equal_slots_fire_the_primary() {
    let mut store = WeaponStore::new();
    let fixture = Fixture::new(&mut store);
    assert_eq!(
        fixture.most_damage(ConditionMask::NONE),
        Some(WeaponSlot::Primary)
    );
}

#[test]
fn a_preferred_slot_beats_an_ordinary_one() {
    let mut store = WeaponStore::new();
    let mut fixture = Fixture::new(&mut store);
    let structure = ConditionMask::bit(0);
    fixture.slot(2).arm.preferred_against = structure;
    assert_eq!(fixture.most_damage(structure), Some(WeaponSlot::Tertiary));
}

#[test]
fn a_matched_preferred_slot_beats_a_matched_only_against_slot_below_it() {
    let mut store = WeaponStore::new();
    let mut fixture = Fixture::new(&mut store);
    let structure = ConditionMask::bit(0);
    fixture.slot(0).arm.only_against = structure;
    fixture.slot(2).arm.preferred_against = structure;
    assert_eq!(fixture.most_damage(structure), Some(WeaponSlot::Tertiary));
}

#[test]
fn a_preferred_match_overrides_a_missed_only_against() {
    let mut store = WeaponStore::new();
    let mut fixture = Fixture::new(&mut store);
    let structure = ConditionMask::bit(0);
    let infantry = ConditionMask::bit(1);
    fixture.slot(2).arm.only_against = structure;
    fixture.slot(2).arm.preferred_against = infantry;
    assert_eq!(fixture.most_damage(infantry), Some(WeaponSlot::Tertiary));
}

#[test]
fn a_missed_only_against_slot_scores_below_every_other() {
    let mut store = WeaponStore::new();
    let mut fixture = Fixture::new(&mut store);
    let structure = ConditionMask::bit(0);
    let infantry = ConditionMask::bit(1);
    fixture.slot(0).arm.only_against = structure;
    fixture.slots[2] = None;
    assert_eq!(fixture.most_damage(infantry), Some(WeaponSlot::Secondary));
    fixture.slots[1] = None;
    assert_eq!(fixture.most_damage(infantry), None);
    assert_eq!(
        fixture.choose(
            infantry,
            CommandSource::Ai,
            ConditionMask::NONE,
            WeaponChoiceCriteria::PreferLeastMovement
        ),
        Some(WeaponSlot::Primary)
    );
}

#[test]
fn a_slot_the_ai_may_not_choose_is_still_available_to_a_player_order() {
    let mut store = WeaponStore::new();
    let mut fixture = Fixture::new(&mut store);
    fixture.slot(2).arm.sources = SourceMask::parse(&["FROM_PLAYER", "FROM_SCRIPT"]).unwrap();
    fixture.slot(0).fitness.ready_status = false;
    fixture.slot(1).fitness.ready_status = false;
    assert_eq!(
        fixture.most_damage(ConditionMask::NONE),
        Some(WeaponSlot::Primary)
    );
    assert_eq!(
        fixture.choose(
            ConditionMask::NONE,
            CommandSource::Player,
            ConditionMask::NONE,
            WeaponChoiceCriteria::PreferMostDamage
        ),
        Some(WeaponSlot::Tertiary)
    );
}

#[test]
fn a_reloading_slot_loses_to_a_ready_one() {
    let mut store = WeaponStore::new();
    let mut fixture = Fixture::new(&mut store);
    fixture.slot(0).fitness.ready_status = false;
    assert_eq!(
        fixture.most_damage(ConditionMask::NONE),
        Some(WeaponSlot::Secondary)
    );
}

#[test]
fn a_preferred_slot_is_unready_inside_its_minimum_range() {
    let mut store = WeaponStore::new();
    let mut fixture = Fixture::new(&mut store);
    let structure = ConditionMask::bit(0);
    fixture.slot(2).arm.preferred_against = structure;
    fixture.slot(2).fitness.minimum = 60.0;
    assert_eq!(fixture.most_damage(structure), Some(WeaponSlot::Primary));
}

#[test]
fn a_slot_no_nugget_accepts_is_dropped_unless_unresistable() {
    let mut store = WeaponStore::new();
    let mut fixture = Fixture::new(&mut store);
    fixture.slot(0).fitness.valid = false;
    assert_eq!(
        fixture.most_damage(ConditionMask::NONE),
        Some(WeaponSlot::Secondary)
    );
    fixture.slot(0).fitness.unresistable = true;
    fixture.slots[1] = None;
    fixture.slots[2] = None;
    assert_eq!(
        fixture.most_damage(ConditionMask::NONE),
        Some(WeaponSlot::Primary)
    );
}

#[test]
fn an_unknown_auto_choose_source_is_refused() {
    assert!(SourceMask::parse(&["FROM_PLAYER", "FROM_NOWHERE"]).is_err());
    assert_eq!(SourceMask::parse(&["NONE"]), Ok(SourceMask::NONE));
}

#[test]
fn a_held_only_in_condition_slot_wins_even_while_reloading() {
    let mut store = WeaponStore::new();
    let mut fixture = Fixture::new(&mut store);
    let moving = ConditionMask::bit(5);
    fixture.slots[1] = None;
    fixture.slot(2).arm.only_in_condition = moving;
    let choose = |fixture: &Fixture, active| {
        fixture.choose(
            ConditionMask::NONE,
            CommandSource::Ai,
            active,
            WeaponChoiceCriteria::PreferMostDamage,
        )
    };
    assert_eq!(
        choose(&fixture, ConditionMask::NONE),
        Some(WeaponSlot::Primary)
    );
    assert_eq!(choose(&fixture, moving), Some(WeaponSlot::Tertiary));
    fixture.slot(2).fitness.ready_status = false;
    assert_eq!(choose(&fixture, moving), Some(WeaponSlot::Tertiary));
}

#[test]
fn prefer_longest_range_takes_the_longer_slot_and_ties_keep_the_higher() {
    let mut store = WeaponStore::new();
    let mut fixture = Fixture::new(&mut store);
    fixture.slot(0).fitness.range = 100.0;
    fixture.slot(1).fitness.range = 300.0;
    fixture.slot(2).fitness.range = 50.0;
    assert_eq!(
        fixture.under(WeaponChoiceCriteria::PreferLongestRange),
        Some(WeaponSlot::Secondary)
    );
    fixture.slot(0).fitness.range = 300.0;
    assert_eq!(
        fixture.under(WeaponChoiceCriteria::PreferLongestRange),
        Some(WeaponSlot::Secondary)
    );
}

#[test]
fn prefer_least_movement_takes_the_slot_needing_the_least_walking() {
    let mut store = WeaponStore::new();
    let mut fixture = Fixture::new(&mut store);
    for index in 0..3 {
        fixture.slot(index).fitness.distance = 250.0;
    }
    fixture.slot(0).fitness.range = 100.0;
    fixture.slot(1).fitness.range = 300.0;
    fixture.slot(2).fitness.range = 150.0;
    assert_eq!(
        fixture.under(WeaponChoiceCriteria::PreferLeastMovement),
        Some(WeaponSlot::Secondary)
    );
    fixture.slot(1).fitness.melee = true;
    assert_eq!(
        fixture.under(WeaponChoiceCriteria::PreferLeastMovement),
        Some(WeaponSlot::Tertiary)
    );
}

#[test]
fn select_at_random_takes_the_slot_with_the_largest_draw() {
    let mut store = WeaponStore::new();
    let mut fixture = Fixture::new(&mut store);
    fixture.slots[2] = None;
    let mut draws = [0.7, 0.2].into_iter();
    let chosen = choose_slot(
        &fixture.slots,
        ConditionMask::NONE,
        CommandSource::Ai,
        ConditionMask::NONE,
        WeaponChoiceCriteria::SelectAtRandom,
        &mut || draws.next().unwrap(),
    );
    assert_eq!(chosen, Some(WeaponSlot::Secondary));
}

#[test]
fn prefer_grab_over_damage_takes_a_reloading_grab_slot_at_once() {
    let mut store = WeaponStore::new();
    let mut fixture = Fixture::new(&mut store);
    fixture.slot(1).fitness.grab = true;
    fixture.slot(1).fitness.ready_status = false;
    assert_eq!(
        fixture.under(WeaponChoiceCriteria::PreferGrabOverDamage),
        Some(WeaponSlot::Secondary)
    );
}

#[test]
fn a_set_whose_own_default_is_the_default_marker_takes_no_slot() {
    let mut store = WeaponStore::new();
    let fixture = Fixture::new(&mut store);
    assert_eq!(
        fixture.under(WeaponChoiceCriteria::UseWeaponSetDefaultCriteria),
        None
    );
}

#[test]
fn an_attack_with_no_victim_takes_the_first_non_melee_no_victim_slot() {
    let slot = |out_of_ammo, melee, no_victim_needed| {
        Some(NoVictimSlot {
            out_of_ammo,
            melee,
            no_victim_needed,
        })
    };
    let mut slots = [None; SLOT_COUNT];
    slots[0] = slot(false, false, false);
    slots[1] = slot(false, true, true);
    slots[2] = slot(true, false, true);
    assert_eq!(choose_slot_without_victim(&slots), None);
    slots[3] = slot(false, false, true);
    assert_eq!(
        choose_slot_without_victim(&slots),
        Some(WeaponSlot::Quaternary)
    );
}

#[test]
fn the_criteria_names_are_the_retail_spellings() {
    assert_eq!(
        WeaponChoiceCriteria::parse("PREFER_LONGEST_RANGE"),
        Some(WeaponChoiceCriteria::PreferLongestRange)
    );
    assert_eq!(
        WeaponChoiceCriteria::parse("USE_WEAPONSET_DEFAULT_CRITERIA"),
        Some(WeaponChoiceCriteria::UseWeaponSetDefaultCriteria)
    );
    assert_eq!(WeaponChoiceCriteria::parse("PREFER_LONGEST_DAMAGE"), None);
    assert_eq!(
        WeaponChoiceCriteria::parse("USE_WEAPON_SET_DEFAULT_CRITERIA"),
        None
    );
}
