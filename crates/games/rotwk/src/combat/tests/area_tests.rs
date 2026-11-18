use super::*;
use mint_sage_combat::DamageShape;

fn burst(radius: f32, shape: DamageShape, others: &[Foe]) -> FireResult {
    let mut combat = CombatData::new();
    let slash = combat.damage_types.intern("SLASH").unwrap();
    let id = combat
        .weapons
        .insert(
            "Burst",
            WeaponTemplate {
                nuggets: vec![Nugget::Damage(DamageNugget {
                    damage: 100.0,
                    damage_type: slash,
                    death_type: None,
                    sub_type: None,
                    radius,
                    damage_scalar: 1.0,
                    delay: LogicFrames::ZERO,
                    arc: None,
                    arc_inverted: false,
                    flanking_bonus: 0.0,
                    requires_upgrade: false,
                    accepts_add: true,
                    shape,
                })],
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    let attacker = entities(1)[0];
    run_nuggets(
        &combat,
        id,
        attacker,
        Point3::new(0.0, 0.0, 0.0),
        others[0].entity,
        Vec2::ZERO,
        18.0,
        others,
    )
}

fn struck(result: &FireResult) -> Vec<u32> {
    result.damage.iter().map(|hit| hit.victim.index()).collect()
}

#[test]
fn a_tapered_nugget_deals_the_midpoint_value_halfway_to_the_rim() {
    let e = entities(2);
    let shape = DamageShape {
        taper_off: Some(50.0),
        ..Default::default()
    };
    let result = burst(
        40.0,
        shape,
        &[foe_at(e[0], 0.0, 0.0), foe_at(e[1], 20.0, 0.0)],
    );
    let amount = |entity: Entity| {
        result
            .damage
            .iter()
            .find(|hit| hit.victim == entity)
            .unwrap()
            .amount
    };
    assert_eq!(amount(e[0]), 100.0);
    assert_eq!(amount(e[1]), 75.0);
}

#[test]
fn a_shock_wave_reaches_a_distant_victim_later() {
    let e = entities(2);
    let shape = DamageShape {
        speed: 80.0,
        ..Default::default()
    };
    let result = burst(
        200.0,
        shape,
        &[foe_at(e[0], 0.0, 0.0), foe_at(e[1], 160.0, 0.0)],
    );
    assert_eq!(struck(&result), vec![e[0].index()]);
    assert_eq!(result.travelling.len(), 1);
    assert_eq!(result.travelling[0].0.victim, e[1]);
    assert_eq!(result.travelling[0].1, 2);
}

#[test]
fn the_area_distance_mode_follows_the_radius_and_cylinder_aoe() {
    let e = entities(2);
    let at = |x: f32, z: f32| {
        [
            foe_at(e[0], 0.0, 0.0),
            Foe {
                z,
                ..foe_at(e[1], x, 0.0)
            },
        ]
    };
    let plain = DamageShape::default();
    assert_eq!(
        struck(&burst(13.0, plain, &at(20.0, 0.0))),
        vec![e[0].index()]
    );
    assert_eq!(struck(&burst(8.0, plain, &at(14.0, 0.0))).len(), 2);
    let cylinder = DamageShape {
        cylinder_aoe: true,
        ..plain
    };
    assert_eq!(struck(&burst(13.0, cylinder, &at(10.0, 60.0))).len(), 2);
    assert_eq!(
        struck(&burst(13.0, plain, &at(10.0, 60.0))),
        vec![e[0].index()]
    );
}
