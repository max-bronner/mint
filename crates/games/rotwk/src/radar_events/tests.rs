use super::*;

#[test]
fn under_attack_label_follows_r2_5_s_kindof_order() {
    let cases: [(&[&str], &str); 5] = [
        (&["INFANTRY"], "RADAR:UnitUnderAttack"),
        (&["CAVALRY"], "RADAR:UnitUnderAttack"),
        (&["INFANTRY", "HARVESTER"], "RADAR:HarvesterUnderAttack"),
        (&["STRUCTURE"], "RADAR:StructureUnderAttack"),
        (&["MP_COUNT_FOR_VICTORY"], "RADAR:StructureUnderAttack"),
    ];
    for (kinds, want) in cases {
        assert_eq!(
            under_attack_label(|token| kinds.contains(&token)),
            want,
            "{kinds:?}"
        );
    }
    assert_eq!(under_attack_label(|_| false), "RADAR:UnderAttack");
}

#[test]
fn ping_clip_matches_r2_1s_table() {
    let cases = [
        (RadarEventType::Information, Some("PingInformation")),
        (RadarEventType::Construction, Some("PingGeneric")),
        (RadarEventType::Upgrade, Some("PingUpgrade")),
        (RadarEventType::UnderAttack, Some("PingAttack")),
        (RadarEventType::Infiltration, None),
        (RadarEventType::Banner, None),
        (RadarEventType::BeaconPulse, Some("PingBeacon")),
        (RadarEventType::BattlePlan, None),
        (RadarEventType::StealthDiscovered, None),
        (RadarEventType::StealthNeutralized, None),
        (RadarEventType::Fake, None),
    ];
    for (kind, want) in cases {
        assert_eq!(kind.ping_clip(), want);
    }
}

#[test]
fn try_event_suppresses_within_600_units_and_10_seconds() {
    let mut ring = RadarEventRing::default();
    assert!(ring.try_event(RadarEventType::UnderAttack, (0.0, 0.0), 0.0));

    assert!(
        !ring.try_event(RadarEventType::UnderAttack, (599.0, 0.0), 1.0),
        "599 units, 1 s later - suppressed"
    );
    assert!(
        ring.try_event(RadarEventType::UnderAttack, (601.0, 0.0), 1.0),
        "601 units - past the distance bound"
    );
}

#[test]
fn try_event_suppresses_at_exactly_600_units() {
    let mut ring = RadarEventRing::default();
    assert!(ring.try_event(RadarEventType::UnderAttack, (0.0, 0.0), 0.0));
    assert!(!ring.try_event(RadarEventType::UnderAttack, (600.0, 0.0), 1.0));
}

#[test]
fn try_event_suppresses_within_10_seconds() {
    let mut ring = RadarEventRing::default();
    assert!(ring.try_event(RadarEventType::UnderAttack, (0.0, 0.0), 0.0));
    assert!(
        !ring.try_event(RadarEventType::UnderAttack, (0.0, 0.0), 9.9),
        "9.9 s later - suppressed"
    );

    let mut ring = RadarEventRing::default();
    assert!(ring.try_event(RadarEventType::UnderAttack, (0.0, 0.0), 0.0));
    assert!(
        ring.try_event(RadarEventType::UnderAttack, (0.0, 0.0), 10.1),
        "10.1 s later - past the age bound"
    );
}

#[test]
fn try_event_suppresses_from_a_slot_already_dead() {
    let mut ring = RadarEventRing::default();
    assert!(ring.try_event(RadarEventType::UnderAttack, (0.0, 0.0), 0.0));
    assert!(
        !ring.slots[0].expect("just created").alive(5.0),
        "4 s life, dead by 5 s"
    );
    assert!(
        !ring.try_event(RadarEventType::UnderAttack, (0.0, 0.0), 5.0),
        "dead but still within the 10 s suppression window"
    );
}

#[test]
fn the_ring_wraps_at_65_events() {
    let mut ring = RadarEventRing::default();
    for i in 0..65 {
        ring.create(RadarEventType::Information, (i as f32, 0.0), 0.0);
    }
    assert_eq!(ring.slots[0].expect("wrapped").at, (64.0, 0.0));
}

#[test]
fn fake_events_are_never_drawable() {
    let mut ring = RadarEventRing::default();
    ring.create(RadarEventType::Fake, (0.0, 0.0), 0.0);
    ring.create(RadarEventType::Information, (1.0, 0.0), 0.0);
    let kinds: Vec<_> = ring.drawable(0.5).map(|e| e.kind).collect();
    assert_eq!(kinds, vec![RadarEventType::Information]);
}

#[test]
fn an_event_fades_half_a_second_before_it_dies() {
    let mut ring = RadarEventRing::default();
    ring.create(RadarEventType::Information, (0.0, 0.0), 0.0);
    let event = ring.slots[0].expect("just created");
    assert!(!event.fading(3.4));
    assert!(event.fading(3.5));
    assert!(event.alive(3.9));
    assert!(!event.alive(4.0));
}

#[test]
fn under_attack_observes_only_the_local_players_own_objects() {
    use crate::combat_facts::Struck;
    use mint_movement::Vec2;
    use mint_sage_asset::{KindOf, PickType};
    use mint_sage_combat::DamageTypes;
    use mint_sage_scene::Conditions;
    use mint_sage_shroud::ObjectVisibility;
    use mint_sim::{Entity, PlayerId};

    let mut damage_types = DamageTypes::new();
    let pierce = damage_types.intern("PIERCE").expect("fresh table");
    let local = PlayerId::new(0);
    let enemy = PlayerId::new(1);
    let make_unit = |entity: Entity, owner: PlayerId, x: f32| -> UnitView {
        UnitView {
            entity,
            position: Vec2 { x, y: 0.0 },
            ground: 0.0,
            facing: 0.0,
            radius: 1.0,
            height: 1.0,
            kind_of: KindOf::NONE,
            targetable: false,
            pick_type: PickType::None,
            owner,
            selected: false,
            moving: false,
            health: Some(1.0),
            dying: false,
            created: 0,
            speed: 0.0,
            weapon_cycle: 0.0,
            attack_cycle: 0,
            attack_slot: None,
            visibility: ObjectVisibility::Clear,
            kind: None,
            art_kind: None,
            told_kind: None,
            hover_kind: None,
            conditions: Conditions::default(),
            build_elapsed: None,
            build_total: None,
            under_repair: false,
            held: false,
        }
    };
    let mine = Entity::from_parts(1, 1);
    let theirs = Entity::from_parts(2, 1);
    let units = vec![
        make_unit(mine, local, 100.0),
        make_unit(theirs, enemy, 900.0),
    ];
    let struck = vec![
        Struck {
            victim: mine,
            attacker: theirs,
            damage_type: pierce,
            dealt: 10.0,
        },
        Struck {
            victim: theirs,
            attacker: mine,
            damage_type: pierce,
            dealt: 10.0,
        },
    ];
    let mut ring = RadarEventRing::default();
    let kinds = crate::templates::KindTable::default();
    observe_under_attack(&mut ring, &units, &kinds, &struck, local, 0.0);
    let events: Vec<_> = ring.drawable(0.0).map(|e| (e.kind, e.at)).collect();
    assert_eq!(events, vec![(RadarEventType::UnderAttack, (100.0, 0.0))]);
}
