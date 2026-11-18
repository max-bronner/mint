use super::*;

fn dict(pairs: &[(&str, Value)]) -> Dictionary {
    Dictionary::from_pairs(
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect(),
    )
}

fn looping(_: &str, overrides: &Overrides) -> Option<EventFacts> {
    Some(EventFacts {
        permanent: overrides.looping.unwrap_or(true),
        stream: false,
    })
}

#[test]
fn a_damaged_rung_falls_back_to_the_pristine_field_and_rubble_does_not() {
    assert_eq!(
        fields(Rung::Damaged, false),
        &[SOUND_AMBIENT_DAMAGED, SOUND_AMBIENT]
    );
    assert_eq!(
        fields(Rung::ReallyDamaged, false),
        &[SOUND_AMBIENT_REALLY_DAMAGED, SOUND_AMBIENT]
    );
    assert_eq!(fields(Rung::Rubble, false), &[SOUND_AMBIENT_RUBBLE]);
}

#[test]
fn a_structure_going_up_plays_its_pristine_ambient_whatever_its_health() {
    assert_eq!(fields(Rung::ReallyDamaged, true), &[SOUND_AMBIENT]);
    assert_eq!(fields(Rung::Rubble, true), &[SOUND_AMBIENT_RUBBLE]);
    assert!(under_construction(Conditions::ACTIVELY_BEING_CONSTRUCTED));
    assert!(!under_construction(Conditions::PARTIALLY_CONSTRUCTED));
}

#[test]
fn only_engaged_starts_the_battle_ambient() {
    assert!(battle(Conditions::ENGAGED));
    assert!(!battle(Conditions::ACTIVELY_BEING_CONSTRUCTED));
}

#[test]
fn the_same_event_keeps_playing_and_a_new_one_replaces_it() {
    let mut emitters = Emitters::default();
    let none = Overrides::default();
    assert_eq!(
        emitters.set(7, Some(("Hum".into(), none)), [0.0; 3], true, |_, _| false),
        None
    );
    emitters.map.get_mut(&7).unwrap().phase = Phase::Playing;
    assert_eq!(
        emitters.set(7, Some(("Hum".into(), none)), [1.0; 3], true, |_, _| false),
        None
    );
    assert_eq!(emitters.map[&7].phase, Phase::Playing);
    assert_eq!(
        emitters.set(7, Some(("Crackle".into(), none)), [1.0; 3], true, |_, _| {
            false
        }),
        Some("Hum".into())
    );
    assert_eq!(emitters.map[&7].phase, Phase::Waiting);
    emitters.map.get_mut(&7).unwrap().phase = Phase::Playing;
    assert_eq!(
        emitters.set(7, None, [1.0; 3], true, |_, _| false),
        Some("Crackle".into())
    );
    assert!(emitters.map.is_empty());
}

#[test]
fn a_vanished_unit_stops_and_a_placement_stays() {
    let mut emitters = Emitters::default();
    let none = Overrides::default();
    emitters.set(3, Some(("Hum".into(), none)), [0.0; 3], true, |_, _| false);
    emitters.set(
        PLACED | 1,
        Some(("Brook".into(), none)),
        [0.0; 3],
        true,
        |_, _| false,
    );
    emitters.map.get_mut(&3).unwrap().phase = Phase::Playing;
    assert_eq!(emitters.retain_units(&[]), vec![(3, "Hum".to_owned())]);
    assert!(emitters.map.contains_key(&(PLACED | 1)));
}

#[test]
fn a_loop_holds_a_voice_only_in_reach_and_a_one_shot_plays_once() {
    assert_eq!(
        step(Phase::Waiting, true, Reach::Audible, true),
        Action::Play
    );
    assert_eq!(
        step(Phase::Playing, true, Reach::OutOfRange, true),
        Action::Stop
    );
    assert_eq!(
        step(Phase::Waiting, true, Reach::OutOfRange, true),
        Action::None
    );
    assert_eq!(
        step(Phase::Waiting, false, Reach::OutOfRange, true),
        Action::Play
    );
    assert_eq!(after_play(false, Outcome::Queued, false), Phase::Done);
    assert_eq!(after_play(true, Outcome::Queued, false), Phase::Playing);
    assert_eq!(after_play(true, Outcome::UnknownEvent, false), Phase::Done);
}

#[test]
fn a_looping_delay_does_not_replay_before_its_cooldown() {
    assert_eq!(after_play(true, Outcome::Queued, true), Phase::Waiting);
    assert_eq!(
        step(Phase::Waiting, true, Reach::Audible, false),
        Action::None
    );
    assert_eq!(
        step(Phase::Waiting, true, Reach::Audible, true),
        Action::Play
    );
}

#[test]
fn an_empty_placement_name_silences_and_a_named_one_replaces_the_template() {
    let silenced = dict(&[(KEY_AMBIENT, Value::Str(String::new()))]);
    assert_eq!(placed(&silenced, Some("Hum"), looping), None);
    let named = dict(&[(KEY_AMBIENT, Value::Str("Brook".into()))]);
    assert_eq!(placed(&named, Some("Hum"), looping).unwrap().event, "Brook");
    let unknown = dict(&[(KEY_AMBIENT, Value::Str("Nowhere".into()))]);
    let only_hum = |name: &str, o: &Overrides| (name == "Hum").then(|| looping(name, o)).flatten();
    assert_eq!(
        placed(&unknown, Some("Hum"), only_hum).unwrap().event,
        "Hum"
    );
}

#[test]
fn customised_fields_apply_only_when_the_flag_says_so() {
    let pairs = [
        (KEY_MAX_RANGE, Value::Float(900.0)),
        (KEY_PRIORITY, Value::Int(3)),
    ];
    let plain = placed(&dict(&pairs), Some("Hum"), looping).unwrap();
    assert_eq!(plain.overrides, Overrides::default());
    let mut with_flag = pairs.to_vec();
    with_flag.push((KEY_CUSTOMIZED, Value::Bool(true)));
    let custom = placed(&dict(&with_flag), Some("Hum"), looping).unwrap();
    assert_eq!(custom.overrides.max_range, Some(900.0));
    assert_eq!(custom.overrides.priority, Some(AudioPriority::High));
}

#[test]
fn with_no_enabled_key_only_a_permanent_loop_starts() {
    let looping_off = dict(&[
        (KEY_CUSTOMIZED, Value::Bool(true)),
        (KEY_LOOPING, Value::Bool(false)),
    ]);
    assert!(placed(&dict(&[]), Some("Hum"), looping).unwrap().enabled);
    assert!(!placed(&looping_off, Some("Hum"), looping).unwrap().enabled);
    let forced = dict(&[
        (KEY_CUSTOMIZED, Value::Bool(true)),
        (KEY_LOOPING, Value::Bool(false)),
        (KEY_ENABLED, Value::Bool(true)),
    ]);
    assert!(placed(&forced, Some("Hum"), looping).unwrap().enabled);
}

#[test]
fn a_placed_objects_own_sound_replaces_every_rung_but_rubble() {
    let named = dict(&[(KEY_AMBIENT, Value::Str("Brook".into()))]);
    let custom = customised(&named, Some("Hum"), looping).unwrap();
    let template = || Some("Fire".to_owned());
    assert_eq!(
        choose(Some(&custom), Rung::ReallyDamaged, template)
            .unwrap()
            .0,
        "Brook"
    );
    assert_eq!(
        choose(Some(&custom), Rung::Rubble, template).unwrap().0,
        "Fire"
    );
    assert_eq!(choose(None, Rung::Pristine, template).unwrap().0, "Fire");
}

#[test]
fn a_placed_object_is_silenced_held_or_left_to_its_template() {
    let silenced = dict(&[(KEY_AMBIENT, Value::Str(String::new()))]);
    let custom = customised(&silenced, Some("Hum"), looping).unwrap();
    assert!(custom.silenced);
    assert_eq!(
        choose(Some(&custom), Rung::Pristine, || Some("Hum".into())),
        None
    );
    let held = dict(&[(KEY_ENABLED, Value::Bool(false))]);
    let custom = customised(&held, Some("Hum"), looping).unwrap();
    assert_eq!(custom.sound, None);
    assert_eq!(
        choose(Some(&custom), Rung::Pristine, || Some("Hum".into())),
        None
    );
    let range_only = dict(&[
        (KEY_CUSTOMIZED, Value::Bool(true)),
        (KEY_MAX_RANGE, Value::Float(900.0)),
    ]);
    let custom = customised(&range_only, Some("Hum"), looping).unwrap();
    let (event, overrides) = custom.sound.unwrap();
    assert_eq!((event.as_str(), overrides.max_range), ("Hum", Some(900.0)));
    assert_eq!(customised(&dict(&[]), Some("Hum"), looping), None);
}
