use super::*;

fn lance(combat: &mut CombatData) -> mint_sage_combat::WeaponId {
    use mint_sage_combat::{DamageNugget, Nugget, WeaponTemplate};
    let crush = combat.damage_types.intern("CRUSH").unwrap();
    combat
        .weapons
        .insert(
            "TestLance",
            WeaponTemplate {
                attack_range: 1000.0,
                delay_between_shots: mint_sage_combat::FrameRange::fixed(
                    mint_sim::LogicFrames::from_frames(3),
                ),
                clip_size: 0,
                nuggets: vec![Nugget::Damage(DamageNugget {
                    damage: 10.0,
                    damage_type: crush,
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
        .unwrap()
}

fn fixture() -> (Simulation<RotwkRules>, Ids, Entity, Entity, Entity, Entity) {
    let mut combat = CombatData::new();
    let lance = lance(&mut combat);
    let passability = Passability::build(&flat_map(100, 100), 5.0 / 128.0).unwrap();
    let member = |x: f32, y: f32, health: f32| Spawn {
        health: Some(health),
        weapon: None,
        armour: None,
        ..Spawn::inert(
            Vec2::new(x, y),
            Facing::EAST,
            Shape {
                kind_of: KindOf::SELECTABLE,
                radius: 5.0,
                height: 16.0,
                geometry_is_small: false,
            },
            OTHER,
        )
    };
    let formation = FormationTable::from_offsets(&[
        Vec2::new(150.0, 500.0),
        Vec2::new(400.0, 500.0),
        Vec2::new(400.0, 700.0),
    ])
    .with_settings(mint_sage_formation::HordeSettings {
        ranks_to_release: vec![0],
        ..mint_sage_formation::HordeSettings::default()
    });
    let horde_spawn = HordeSpawn {
        formation: 0,
        origin: Vec2::new((150.0 + 400.0 + 400.0) / 3.0, (500.0 + 500.0 + 700.0) / 3.0),
        facing: Facing::EAST,
        member: member(0.0, 0.0, 100.0),
        count: 3,
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let attacker = Spawn {
        health: Some(1000.0),
        weapon: Some(lance),
        armour: None,
        ..Spawn::inert(
            Vec2::new(100.0, 500.0),
            Facing::EAST,
            Shape {
                kind_of: KindOf::SELECTABLE,
                radius: 5.0,
                height: 16.0,
                geometry_is_small: false,
            },
            PLAYER,
        )
    };
    let block = Spawn::inert(
        Vec2::new(250.0, 500.0),
        Facing::EAST,
        Shape {
            kind_of: KindOf::SELECTABLE,
            radius: 5.0,
            height: 16.0,
            geometry_is_small: false,
        },
        OTHER,
    );
    let footprint = mint_sage_passability::FootprintRecord {
        offset: Vec2::ZERO,
        footprint: mint_sage_passability::Footprint::Box {
            major: 5.0,
            minor: 30.0,
        },
    };
    let rules = RotwkRules::new(passability, vec![attacker, block])
        .with_combat(combat)
        .with_structure_footprints(vec![(1, vec![footprint])])
        .with_hordes(vec![formation], vec![horde_spawn]);
    let simulation = Simulation::new(3, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let attacker = simulation
        .state()
        .store(ids.armed)
        .unwrap()
        .entities()
        .to_vec()[0];
    let horde = simulation
        .state()
        .store(ids.horde)
        .unwrap()
        .entities()
        .to_vec()[0];
    let members = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    let at = |e: Entity| simulation.state().get(ids.body, e).unwrap().0.position;
    let decoy = *members
        .iter()
        .find(|m| (at(**m).x - 150.0).abs() < 1.0)
        .unwrap();
    let blocked = *members
        .iter()
        .find(|m| (at(**m).y - 500.0).abs() < 1.0 && (at(**m).x - 400.0).abs() < 1.0)
        .unwrap();
    let clear = *members
        .iter()
        .find(|m| (at(**m).y - 700.0).abs() < 1.0)
        .unwrap();
    (simulation, ids, attacker, decoy, blocked, clear)
}

fn ordered_target(
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    attacker: Entity,
) -> Option<Entity> {
    simulation
        .state()
        .get(ids.attack_order, attacker)
        .map(|order| order.target)
}

#[test]
fn the_view_blocked_exclusion_keeps_a_nearer_member_from_winning_the_discount() {
    let (mut simulation, ids, attacker, decoy, blocked, clear) = fixture();
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
        .submit(PLAYER, RotwkCommand::AttackObject { target: decoy })
        .unwrap();
    for _ in 0..30 {
        simulation.tick().unwrap();
        if ordered_target(&simulation, ids, attacker) != Some(decoy) {
            break;
        }
    }
    let next = ordered_target(&simulation, ids, attacker);
    assert_eq!(
        next,
        Some(clear),
        "{next:?}, expected `clear` over `blocked` ({blocked:?})"
    );
}
