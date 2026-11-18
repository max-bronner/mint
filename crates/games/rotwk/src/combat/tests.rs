use super::*;
use mint_sage_combat::{DamageNugget, DotNugget, ModifierNugget, WeaponTemplate};
use mint_sim::World;
mod area_tests;

fn entities(count: u32) -> Vec<Entity> {
    let mut world = World::new();
    (0..count).filter_map(|_| world.spawn().ok()).collect()
}

fn foe_at(entity: Entity, x: f32, y: f32) -> Foe {
    Foe {
        entity,
        position: Vec2::new(x, y),
        z: 0.0,
        radius: 5.0,
        height: 18.0,
        is_structure: false,
        airborne: None,
        unattackable: false,
        not_autoacquirable: false,
        ai_kind_of: None,
    }
}

fn no_allies(_a: Entity, _b: Entity) -> bool {
    false
}

fn crusher(level: u8, velocity: Vec2) -> CrushState {
    CrushState {
        crusher_level: level,
        crushable_level: 2,
        crushes_allies: false,
        velocity,
        facing: mint_movement::Facing::EAST,
        volume: Volume::Circle(5.0),
        too_slow: false,
    }
}

fn crushable(level: i16) -> CrushState {
    CrushState {
        crusher_level: 0,
        crushable_level: level,
        crushes_allies: false,
        velocity: Vec2::ZERO,
        facing: mint_movement::Facing::EAST,
        volume: Volume::Circle(5.0),
        too_slow: false,
    }
}

#[test]
fn a_moving_crusher_kills_an_overlapping_lower_level_victim_it_moves_toward() {
    let [cavalry, infantry] = <[Entity; 2]>::try_from(entities(2)).unwrap();
    let living = vec![
        (foe_at(cavalry, 0.0, 0.0), PlayerId::new(0)),
        (foe_at(infantry, 3.0, 0.0), PlayerId::new(1)),
    ];
    let crush = vec![crusher(1, Vec2::new(1.0, 0.0)), crushable(0)];
    let grid = TargetGrid::build(&living);
    let hits = run_crush(&living, &crush, &grid, &no_allies);
    assert_eq!(hits, vec![(0, 1)]);

    let mut types = mint_sage_combat::DamageTypes::new();
    let crush_id = types.intern("CRUSH").unwrap();
    let events = crush_damage_events(&living, &hits, crush_id);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].victim, infantry);
    assert_eq!(events[0].attacker, cavalry);
    assert_eq!(events[0].amount, CRUSH_KILL_DAMAGE);
    assert_eq!(events[0].damage_type, crush_id);
}

#[test]
fn a_box_crusher_tramples_what_its_box_covers() {
    let [mumak, infantry] = <[Entity; 2]>::try_from(entities(2)).unwrap();
    let living = vec![
        (foe_at(mumak, 0.0, 0.0), PlayerId::new(0)),
        (foe_at(infantry, 0.0, 18.0), PlayerId::new(1)),
    ];
    let grid = TargetGrid::build(&living);
    let boxed = |facing: f32| CrushState {
        facing: mint_movement::Facing::from_radians(facing),
        volume: Volume::Box(24.0, 10.0),
        ..crusher(1, Vec2::new(0.0, 1.0))
    };
    let along_x = vec![boxed(0.0), crushable(0)];
    assert!(run_crush(&living, &along_x, &grid, &no_allies).is_empty());
    let along_y = vec![boxed(std::f32::consts::FRAC_PI_2), crushable(0)];
    assert_eq!(
        run_crush(&living, &along_y, &grid, &no_allies),
        vec![(0, 1)]
    );
}

#[test]
fn a_crusher_crushes_only_what_it_overlaps_in_height() {
    let [beast, infantry] = <[Entity; 2]>::try_from(entities(2)).unwrap();
    let at_height = |z: f32| {
        let flier = Foe {
            z,
            height: 40.0,
            ..foe_at(beast, 0.0, 0.0)
        };
        vec![
            (flier, PlayerId::new(0)),
            (foe_at(infantry, 3.0, 0.0), PlayerId::new(1)),
        ]
    };
    let crush = vec![crusher(3, Vec2::new(1.0, 0.0)), crushable(0)];
    let cruising = at_height(121.0);
    let grid = TargetGrid::build(&cruising);
    assert!(run_crush(&cruising, &crush, &grid, &no_allies).is_empty());
    let low = at_height(5.0);
    let grid = TargetGrid::build(&low);
    assert_eq!(run_crush(&low, &crush, &grid, &no_allies), vec![(0, 1)]);
}

#[test]
fn a_crusher_moving_away_does_not_crush() {
    let [cavalry, infantry] = <[Entity; 2]>::try_from(entities(2)).unwrap();
    let living = vec![
        (foe_at(cavalry, 0.0, 0.0), PlayerId::new(0)),
        (foe_at(infantry, 3.0, 0.0), PlayerId::new(1)),
    ];
    let crush = vec![crusher(1, Vec2::new(-1.0, 0.0)), crushable(0)];
    let grid = TargetGrid::build(&living);
    assert!(run_crush(&living, &crush, &grid, &no_allies).is_empty());
}

#[test]
fn an_allied_crusher_does_not_crush() {
    let [cavalry, infantry] = <[Entity; 2]>::try_from(entities(2)).unwrap();
    let living = vec![
        (foe_at(cavalry, 0.0, 0.0), PlayerId::new(0)),
        (foe_at(infantry, 3.0, 0.0), PlayerId::new(1)),
    ];
    let crush = vec![crusher(1, Vec2::new(1.0, 0.0)), crushable(0)];
    let grid = TargetGrid::build(&living);
    assert!(run_crush(&living, &crush, &grid, &|_, _| true).is_empty());

    let reckless = CrushState {
        crushes_allies: true,
        ..crusher(1, Vec2::new(1.0, 0.0))
    };
    let crush = vec![reckless, crushable(0)];
    assert_eq!(
        run_crush(&living, &crush, &grid, &|_, _| true),
        vec![(0, 1)]
    );
}

#[test]
fn a_victim_at_the_crushers_level_is_not_crushed() {
    let [cavalry, hero] = <[Entity; 2]>::try_from(entities(2)).unwrap();
    let living = vec![
        (foe_at(cavalry, 0.0, 0.0), PlayerId::new(0)),
        (foe_at(hero, 3.0, 0.0), PlayerId::new(1)),
    ];
    let crush = vec![crusher(1, Vec2::new(1.0, 0.0)), crushable(1)];
    let grid = TargetGrid::build(&living);
    assert!(run_crush(&living, &crush, &grid, &no_allies).is_empty());
}

#[test]
fn a_stationary_crusher_crushes_nothing() {
    let [cavalry, infantry] = <[Entity; 2]>::try_from(entities(2)).unwrap();
    let living = vec![
        (foe_at(cavalry, 0.0, 0.0), PlayerId::new(0)),
        (foe_at(infantry, 3.0, 0.0), PlayerId::new(1)),
    ];
    let crush = vec![crusher(1, Vec2::ZERO), crushable(0)];
    let grid = TargetGrid::build(&living);
    assert!(run_crush(&living, &crush, &grid, &no_allies).is_empty());
}

#[test]
fn target_allowed_checks_the_airborne_category_instead_of_ground() {
    let flier = Foe {
        airborne: Some(crate::templates::AirborneCategory::Monster),
        ..foe_at(entities(1)[0], 0.0, 0.0)
    };
    let ground_only = mint_sage_combat::AntiMask::DEFAULT;
    assert!(
        !target_allowed(ground_only, &flier),
        "a plain ground weapon cannot hit a flier"
    );
    let anti_monster = ground_only.union(mint_sage_combat::AntiMask::AIRBORNE_MONSTER);
    assert!(target_allowed(anti_monster, &flier));
    let anti_vehicle = ground_only.union(mint_sage_combat::AntiMask::AIRBORNE_VEHICLE);
    assert!(
        !target_allowed(anti_vehicle, &flier),
        "the wrong AntiAirborne* category still refuses"
    );
}

#[test]
fn range_is_measured_surface_to_surface_rather_than_centre_to_centre() {
    let here = Vec2::new(0.0, 0.0);
    assert!(in_range(here, 5.0, Vec2::new(110.0, 0.0), 5.0, 0.0, 100.0));
    assert!(!in_range(here, 5.0, Vec2::new(111.0, 0.0), 5.0, 0.0, 100.0));
}

#[test]
fn two_equidistant_enemies_are_broken_apart_by_entity_index() {
    let e = entities(10);
    let forwards = [foe_at(e[9], 100.0, 0.0), foe_at(e[4], -100.0, 0.0)];
    let backwards = [foe_at(e[4], -100.0, 0.0), foe_at(e[9], 100.0, 0.0)];
    let a = acquire_target(Vec2::new(0.0, 0.0), 5.0, &forwards, 0.0, 300.0);
    let b = acquire_target(Vec2::new(0.0, 0.0), 5.0, &backwards, 0.0, 300.0);
    assert_eq!(a, Some(e[4]));
    assert_eq!(a, b);
}

#[test]
fn target_allowed_reads_ground_and_structure_off_the_mask() {
    let ground_unit = Foe {
        is_structure: false,
        ..foe_at(entities(1)[0], 0.0, 0.0)
    };
    let building = Foe {
        is_structure: true,
        ..foe_at(entities(1)[0], 0.0, 0.0)
    };
    let ground_only = mint_sage_combat::AntiMask::DEFAULT;
    assert!(target_allowed(ground_only, &ground_unit));
    assert!(
        target_allowed(ground_only, &building),
        "a structure counts as ground too"
    );
    let ram = mint_sage_combat::AntiMask::DEFAULT
        .without(mint_sage_combat::AntiMask::GROUND)
        .union(mint_sage_combat::AntiMask::STRUCTURE);
    assert!(!target_allowed(ram, &ground_unit), "ram: structures only");
    assert!(target_allowed(ram, &building));
}

#[test]
fn nothing_in_range_acquires_nothing() {
    let e = entities(2);
    let foes = [foe_at(e[1], 5000.0, 0.0)];
    assert_eq!(
        acquire_target(Vec2::new(0.0, 0.0), 5.0, &foes, 0.0, 100.0),
        None
    );
    assert_eq!(
        acquire_target(Vec2::new(0.0, 0.0), 5.0, &[], 0.0, 100.0),
        None
    );
}

#[test]
fn a_non_finite_position_neither_targets_nor_is_targeted() {
    let e = entities(2);
    let broken = [foe_at(e[1], f32::NAN, 0.0)];
    assert_eq!(
        acquire_target(Vec2::new(0.0, 0.0), 5.0, &broken, 0.0, 1e9),
        None
    );
    let good = [foe_at(e[1], 10.0, 0.0)];
    assert_eq!(
        acquire_target(Vec2::new(f32::NAN, 0.0), 5.0, &good, 0.0, 1e9),
        None
    );
    assert!(!in_range(
        Vec2::new(f32::NAN, 0.0),
        5.0,
        Vec2::new(0.0, 0.0),
        5.0,
        0.0,
        1e9
    ));
}

#[test]
fn damage_events_are_ordered_by_victim_then_attacker_then_nugget_then_amount() {
    let mut types = mint_sage_combat::DamageTypes::new();
    let pierce = types.intern("PIERCE").unwrap();
    let e = entities(10);
    let event = |victim: usize, attacker: usize, nugget: u16, amount: f32| DamageEvent {
        victim: e[victim],
        attacker: e[attacker],
        amount,
        damage_type: pierce,
        scalar: 1.0,
        fx_type: None,
        dealt: 0.0,
        death: DeathNaming::Normal,
        sub_type: None,
        flanking_bonus: 0.0,
        weapon: None,
        launched: None,
        nugget,
        accepts_add: true,
        bonuses_applied: false,
        force_kill: false,
        drain_life: 0.0,
    };
    let mut events = [
        event(5, 2, 0, 10.0),
        event(1, 9, 1, 5.0),
        event(5, 1, 0, 30.0),
        event(1, 9, 0, 10.0),
        event(1, 9, 0, 5.0),
    ];
    sort_damage(&mut events);
    let order: Vec<(u32, u32, u16, f32)> = events
        .iter()
        .map(|e| (e.victim.index(), e.attacker.index(), e.nugget, e.amount))
        .collect();
    assert_eq!(
        order,
        vec![
            (1, 9, 0, 5.0),
            (1, 9, 0, 10.0),
            (1, 9, 1, 5.0),
            (5, 1, 0, 30.0),
            (5, 2, 0, 10.0)
        ]
    );
}

#[test]
fn a_per_shot_weapon_winds_up_once_and_then_fires() {
    let mut combat = CombatData::new();
    let pierce = combat.damage_types.intern("PIERCE").unwrap();
    let id = combat
        .weapons
        .insert(
            "Test",
            WeaponTemplate {
                attack_range: 100.0,
                pre_attack_delay: LogicFrames::from_frames(3),
                delay_between_shots: mint_sage_combat::FrameRange::fixed(LogicFrames::from_frames(
                    10,
                )),
                nuggets: vec![Nugget::Damage(DamageNugget {
                    damage: 5.0,
                    damage_type: pierce,
                    death_type: None,
                    sub_type: None,
                    radius: 0.0,
                    damage_scalar: 1.0,
                    delay: LogicFrames::ZERO,
                    arc: None,
                    arc_inverted: false,
                    flanking_bonus: 0.0,
                    requires_upgrade: false,
                    accepts_add: true,
                    shape: Default::default(),
                })],
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    let template = combat.weapons.get(id).unwrap();
    let mut armed = Armed {
        weapon: id,
        state: WeaponState::new(template),
        target: None,
        winding: false,
        shots_at_target: 0,
        last_shot_at: None,
        next_acquire: Frame::ZERO,
        reacquire_ready: None,
        squish_reacquire: false,
    };
    let mut fired_on = Vec::new();
    for frame in 0..24u64 {
        if tick_weapon(
            &combat,
            &mut armed,
            Frame::new(frame),
            false,
            Vec2::new(0.0, 0.0),
            1.0,
            mint_sage_combat::BonusConditions::default(),
            &mut mint_sim::Pcg32::new(1),
        )
        .fired()
        {
            fired_on.push(frame);
        }
    }
    assert_eq!(fired_on, vec![3, 16]);
}

#[test]
fn a_per_attack_weapon_winds_up_for_its_first_shot_at_a_target_it_walked_to() {
    let mut combat = CombatData::new();
    let pierce = combat.damage_types.intern("PIERCE").unwrap();
    let id = combat
        .weapons
        .insert(
            "Test",
            WeaponTemplate {
                attack_range: 100.0,
                pre_attack_delay: LogicFrames::from_frames(3),
                pre_attack_type: mint_sage_combat::PreAttackType::PerAttack,
                delay_between_shots: mint_sage_combat::FrameRange::fixed(LogicFrames::from_frames(
                    10,
                )),
                nuggets: vec![Nugget::Damage(DamageNugget {
                    damage: 5.0,
                    damage_type: pierce,
                    death_type: None,
                    sub_type: None,
                    radius: 0.0,
                    damage_scalar: 1.0,
                    delay: LogicFrames::ZERO,
                    arc: None,
                    arc_inverted: false,
                    flanking_bonus: 0.0,
                    requires_upgrade: false,
                    accepts_add: true,
                    shape: Default::default(),
                })],
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    let template = combat.weapons.get(id).unwrap();
    let target = entities(1).first().copied();
    let mut armed = Armed {
        weapon: id,
        state: WeaponState::new(template),
        target,
        winding: false,
        shots_at_target: 0,
        last_shot_at: None,
        next_acquire: Frame::ZERO,
        reacquire_ready: None,
        squish_reacquire: false,
    };
    let mut rng = mint_sim::Pcg32::new(1);
    let mut fired_on = Vec::new();
    for frame in 0..8u64 {
        if tick_weapon(
            &combat,
            &mut armed,
            Frame::new(frame),
            false,
            Vec2::new(0.0, 0.0),
            1.0,
            mint_sage_combat::BonusConditions::default(),
            &mut rng,
        )
        .fired()
        {
            fired_on.push(frame);
        }
    }
    assert_eq!(fired_on, vec![3]);
}

#[test]
fn a_projectile_weapon_launches_rather_than_damaging_immediately() {
    let mut combat = CombatData::new();
    let pierce = combat.damage_types.intern("PIERCE").unwrap();
    combat
        .weapons
        .insert(
            "Warhead",
            WeaponTemplate {
                nuggets: vec![Nugget::Damage(DamageNugget {
                    damage: 40.0,
                    damage_type: pierce,
                    death_type: None,
                    sub_type: None,
                    radius: 0.0,
                    damage_scalar: 1.0,
                    delay: LogicFrames::ZERO,
                    arc: None,
                    arc_inverted: false,
                    flanking_bonus: 0.0,
                    requires_upgrade: false,
                    accepts_add: true,
                    shape: Default::default(),
                })],
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    let bow = combat
        .weapons
        .insert(
            "Bow",
            WeaponTemplate {
                attack_range: 300.0,
                weapon_speed: 321.0,
                nuggets: vec![Nugget::Projectile(mint_sage_combat::ProjectileNugget {
                    projectile_template: Some("Arrow".to_owned()),
                    warhead: "Warhead".to_owned(),
                    requires_upgrade: false,
                    launch_slot: None,
                    always_attack_offset: None,
                })],
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    let e = entities(2);
    let (shooter, target) = (e[0], e[1]);
    let result = run_nuggets(
        &combat,
        bow,
        shooter,
        launch_point(Vec2::new(0.0, 0.0), 0.0),
        target,
        Vec2::new(200.0, 0.0),
        18.0,
        &[],
    );
    assert!(result.damage.is_empty());
    assert_eq!(result.projectiles.len(), 1);
    assert!((result.projectiles[0].speed - 321.0).abs() < 1e-3);
    assert_eq!(result.projectiles[0].victim, Some(target));

    let hit = run_nuggets(
        &combat,
        result.projectiles[0].warhead,
        shooter,
        result.projectiles[0].to,
        target,
        Vec2::new(200.0, 0.0),
        18.0,
        &[],
    );
    assert_eq!(hit.damage.len(), 1);
    assert!((hit.damage[0].amount - 40.0).abs() < 1e-3);
    assert_eq!(hit.damage[0].attacker, shooter);
}

#[test]
fn a_missed_shot_strikes_no_one_and_its_projectile_flies_to_the_landing() {
    let mut combat = CombatData::new();
    let pierce = combat.damage_types.intern("PIERCE").unwrap();
    let blade = combat
        .weapons
        .insert(
            "Blade",
            WeaponTemplate {
                nuggets: vec![Nugget::Damage(DamageNugget {
                    damage: 40.0,
                    damage_type: pierce,
                    death_type: None,
                    sub_type: None,
                    radius: 0.0,
                    damage_scalar: 1.0,
                    delay: LogicFrames::ZERO,
                    arc: None,
                    arc_inverted: false,
                    flanking_bonus: 0.0,
                    requires_upgrade: false,
                    accepts_add: true,
                    shape: Default::default(),
                })],
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    let bow = combat
        .weapons
        .insert(
            "Bow",
            WeaponTemplate {
                nuggets: vec![Nugget::Projectile(mint_sage_combat::ProjectileNugget {
                    projectile_template: Some("Arrow".to_owned()),
                    warhead: "Blade".to_owned(),
                    requires_upgrade: false,
                    launch_slot: None,
                    always_attack_offset: None,
                })],
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    let e = entities(2);
    let (shooter, target) = (e[0], e[1]);
    let others = [foe_at(target, 200.0, 0.0)];
    let landing = Vec2::new(230.0, 10.0);
    let run = |weapon| {
        let shot = Shot::missing(
            weapon,
            shooter,
            Point3::new(0.0, 0.0, 0.0),
            target,
            landing,
            0.0,
        );
        run_nuggets_for(
            &combat,
            &shot,
            others.iter().copied(),
            &mint_sim::UpgradeMask::EMPTY,
            &Everyone,
        )
    };
    assert!(run(blade).damage.is_empty());
    let flying = run(bow).projectiles;
    assert_eq!(flying.len(), 1);
    assert!(!flying[0].struck);
    assert_eq!(flying[0].victim, Some(target));
    assert!((flying[0].to.x - 230.0).abs() < 1e-3 && (flying[0].to.y - 10.0).abs() < 1e-3);
}

#[test]
fn two_nuggets_sharing_a_warhead_are_still_told_apart() {
    let mut combat = CombatData::new();
    combat
        .weapons
        .insert("Head", WeaponTemplate::default())
        .unwrap();
    let bow = combat
        .weapons
        .insert(
            "Bow",
            WeaponTemplate {
                attack_range: 300.0,
                weapon_speed: 300.0,
                nuggets: vec![
                    Nugget::Projectile(mint_sage_combat::ProjectileNugget {
                        projectile_template: Some("GoodFactionArrow".to_owned()),
                        warhead: "Head".to_owned(),
                        requires_upgrade: false,
                        launch_slot: None,
                        always_attack_offset: None,
                    }),
                    Nugget::Projectile(mint_sage_combat::ProjectileNugget {
                        projectile_template: Some("EvilFactionArrow".to_owned()),
                        warhead: "Head".to_owned(),
                        requires_upgrade: false,
                        launch_slot: None,
                        always_attack_offset: None,
                    }),
                ],
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    let e = entities(2);
    let result = run_nuggets(
        &combat,
        bow,
        e[0],
        launch_point(Vec2::new(0.0, 0.0), 0.0),
        e[1],
        Vec2::new(100.0, 0.0),
        18.0,
        &[],
    );
    assert_eq!(result.projectiles.len(), 2);
    assert_eq!(result.projectiles[0].warhead, result.projectiles[1].warhead);
    assert_eq!(result.projectiles[0].source, bow);
    assert_eq!(result.projectiles[1].source, bow);
    assert_eq!(result.projectiles[0].nugget, 0);
    assert_eq!(result.projectiles[1].nugget, 1);
}

#[test]
fn a_nugget_requiring_an_upgrade_does_not_fire() {
    let mut combat = CombatData::new();
    combat
        .weapons
        .insert("Head", WeaponTemplate::default())
        .unwrap();
    let bow = combat
        .weapons
        .insert(
            "Bow",
            WeaponTemplate {
                attack_range: 300.0,
                weapon_speed: 300.0,
                nuggets: vec![
                    Nugget::Projectile(mint_sage_combat::ProjectileNugget {
                        projectile_template: Some("Arrow".to_owned()),
                        warhead: "Head".to_owned(),
                        requires_upgrade: false,
                        launch_slot: None,
                        always_attack_offset: None,
                    }),
                    Nugget::Projectile(mint_sage_combat::ProjectileNugget {
                        projectile_template: Some("FireArrow".to_owned()),
                        warhead: "Head".to_owned(),
                        requires_upgrade: true,
                        launch_slot: None,
                        always_attack_offset: None,
                    }),
                ],
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    let e = entities(2);
    let result = run_nuggets(
        &combat,
        bow,
        e[0],
        launch_point(Vec2::new(0.0, 0.0), 0.0),
        e[1],
        Vec2::new(100.0, 0.0),
        18.0,
        &[],
    );
    assert_eq!(
        result.projectiles.len(),
        1,
        "the upgrade-gated arrow was fired as well"
    );
}

#[test]
fn an_area_nugget_hits_every_body_in_the_radius_once_in_index_order() {
    let mut combat = CombatData::new();
    let flame = combat.damage_types.intern("FLAME").unwrap();
    let id = combat
        .weapons
        .insert(
            "Blast",
            WeaponTemplate {
                nuggets: vec![Nugget::Damage(DamageNugget {
                    damage: 20.0,
                    damage_type: flame,
                    death_type: None,
                    sub_type: None,
                    radius: 50.0,
                    damage_scalar: 1.0,
                    delay: LogicFrames::ZERO,
                    arc: None,
                    arc_inverted: false,
                    flanking_bonus: 0.0,
                    requires_upgrade: false,
                    accepts_add: true,
                    shape: Default::default(),
                })],
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    let e = entities(10);
    let victim = e[3];
    let others = [
        foe_at(victim, 5.0, 0.0),
        foe_at(e[7], 10.0, 0.0),
        foe_at(e[2], -20.0, 0.0),
        foe_at(e[9], 900.0, 0.0),
    ];
    let result = run_nuggets(
        &combat,
        id,
        e[0],
        Point3::new(0.0, 0.0, 0.0),
        victim,
        Vec2::new(5.0, 0.0),
        18.0,
        &others,
    );
    let hit: Vec<u32> = result.damage.iter().map(|e| e.victim.index()).collect();
    assert_eq!(hit, vec![2, 3, 7]);
}

#[test]
fn an_area_nugget_misses_a_primary_victim_outside_its_radius() {
    let mut combat = CombatData::new();
    let flame = combat.damage_types.intern("FLAME").unwrap();
    let id = combat
        .weapons
        .insert(
            "Blast",
            WeaponTemplate {
                nuggets: vec![Nugget::Damage(DamageNugget {
                    damage: 20.0,
                    damage_type: flame,
                    death_type: None,
                    sub_type: None,
                    radius: 50.0,
                    damage_scalar: 1.0,
                    delay: LogicFrames::ZERO,
                    arc: None,
                    arc_inverted: false,
                    flanking_bonus: 0.0,
                    requires_upgrade: false,
                    accepts_add: true,
                    shape: Default::default(),
                })],
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    let e = entities(4);
    let victim = e[3];
    let others = [foe_at(victim, 900.0, 0.0)];
    let shot = Shot {
        weapon: id,
        attacker: e[0],
        origin: Point3::new(0.0, 0.0, 0.0),
        victim: Some((victim, Vec2::new(900.0, 0.0), 9.0)),
        missed: None,
        centre: Vec2::new(0.0, 0.0),
        centre_height: 0.0,
        owner_if_gone: None,
    };
    let result = run_nuggets_for(
        &combat,
        &shot,
        others.iter().copied(),
        &mint_sim::UpgradeMask::EMPTY,
        &Everyone,
    );
    assert!(result.damage.is_empty());
}

#[test]
fn a_shot_with_no_victim_still_hits_by_area_with_its_radius_floored_at_one() {
    let mut combat = CombatData::new();
    let pierce = combat.damage_types.intern("PIERCE").unwrap();
    let id = combat
        .weapons
        .insert(
            "Warhead",
            WeaponTemplate {
                nuggets: vec![Nugget::Damage(DamageNugget {
                    damage: 40.0,
                    damage_type: pierce,
                    death_type: None,
                    sub_type: None,
                    radius: 0.0,
                    damage_scalar: 1.0,
                    delay: LogicFrames::ZERO,
                    arc: None,
                    arc_inverted: false,
                    flanking_bonus: 0.0,
                    requires_upgrade: false,
                    accepts_add: true,
                    shape: Default::default(),
                })],
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    let e = entities(3);
    let on_the_spot = foe_at(e[1], 0.0, 0.0);
    let far_away = foe_at(e[2], 500.0, 0.0);
    let shot = Shot {
        weapon: id,
        attacker: e[0],
        origin: Point3::new(0.0, 0.0, 0.0),
        victim: None,
        missed: Some(Aim::Ground),
        centre: Vec2::new(0.0, 0.0),
        centre_height: 0.0,
        owner_if_gone: None,
    };
    let result = run_nuggets_for(
        &combat,
        &shot,
        [on_the_spot, far_away].into_iter(),
        &mint_sim::UpgradeMask::EMPTY,
        &Everyone,
    );
    let hit: Vec<u32> = result.damage.iter().map(|e| e.victim.index()).collect();
    assert_eq!(hit, vec![e[1].index()]);
}

#[test]
fn a_shot_with_no_victim_gives_its_dot_and_modifier_nuggets_the_target_on_the_spot() {
    let mut combat = CombatData::new();
    let poison = combat.damage_types.intern("POISON").unwrap();
    let id = combat
        .weapons
        .insert(
            "Sting",
            WeaponTemplate {
                nuggets: vec![
                    Nugget::Dot(DotNugget {
                        damage: 5.0,
                        damage_type: poison,
                        radius: 0.0,
                        interval: LogicFrames::from_engine_milliseconds(1000),
                        duration: LogicFrames::from_engine_milliseconds(10_000),
                        death_type: None,
                        sub_type: None,
                        requires_upgrade: false,
                        accepts_add: true,
                    }),
                    Nugget::Modifier(ModifierNugget {
                        list: String::from("Root"),
                        radius: 0.0,
                    }),
                ],
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    let e = entities(3);
    let shot = Shot {
        weapon: id,
        attacker: e[0],
        origin: Point3::new(0.0, 0.0, 0.0),
        victim: None,
        missed: None,
        centre: Vec2::new(0.0, 0.0),
        centre_height: 0.0,
        owner_if_gone: None,
    };
    let result = run_nuggets_for(
        &combat,
        &shot,
        [foe_at(e[1], 0.0, 0.0), foe_at(e[2], 500.0, 0.0)].into_iter(),
        &mint_sim::UpgradeMask::EMPTY,
        &Everyone,
    );
    let dotted: Vec<u32> = result.dots.iter().map(|d| d.victim.index()).collect();
    assert_eq!(dotted, vec![e[1].index()]);
    assert_eq!(result.modifiers, vec![(e[1], 1)]);
    assert_eq!(
        result.damage.len(),
        1,
        "the DOT also lands one full hit now"
    );
    assert_eq!(result.damage[0].victim, e[1]);
    assert_eq!(result.damage[0].amount, 5.0);
    assert_eq!(result.damage[0].damage_type, poison);
}

#[test]
fn an_area_nugget_does_not_double_hit_a_victim_already_present_in_others() {
    let mut combat = CombatData::new();
    let flame = combat.damage_types.intern("FLAME").unwrap();
    let id = combat
        .weapons
        .insert(
            "Blast",
            WeaponTemplate {
                nuggets: vec![Nugget::Damage(DamageNugget {
                    damage: 20.0,
                    damage_type: flame,
                    death_type: None,
                    sub_type: None,
                    radius: 50.0,
                    damage_scalar: 1.0,
                    delay: LogicFrames::ZERO,
                    arc: None,
                    arc_inverted: false,
                    flanking_bonus: 0.0,
                    requires_upgrade: false,
                    accepts_add: true,
                    shape: Default::default(),
                })],
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    let e = entities(3);
    let victim = e[1];
    let others = [foe_at(e[0], -10.0, 0.0), foe_at(victim, 5.0, 0.0)];
    let result = run_nuggets(
        &combat,
        id,
        e[2],
        Point3::new(0.0, 0.0, 0.0),
        victim,
        Vec2::new(5.0, 0.0),
        18.0,
        &others,
    );
    let hit: Vec<u32> = result.damage.iter().map(|e| e.victim.index()).collect();
    assert_eq!(
        hit,
        vec![0, 1],
        "the victim must be hit once, not once from the scan and once from the fallback"
    );
}

#[test]
fn a_delayed_nugget_lands_later_rather_than_on_the_firing_frame() {
    let mut combat = CombatData::new();
    let pierce = combat.damage_types.intern("PIERCE").unwrap();
    let id = combat
        .weapons
        .insert(
            "Slow",
            WeaponTemplate {
                nuggets: vec![Nugget::Damage(DamageNugget {
                    damage: 5.0,
                    damage_type: pierce,
                    death_type: None,
                    sub_type: None,
                    radius: 0.0,
                    damage_scalar: 1.0,
                    delay: LogicFrames::from_frames(30),
                    arc: None,
                    arc_inverted: false,
                    flanking_bonus: 0.0,
                    requires_upgrade: false,
                    accepts_add: true,
                    shape: Default::default(),
                })],
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    let e = entities(2);
    let result = run_nuggets(
        &combat,
        id,
        e[0],
        Point3::new(0.0, 0.0, 0.0),
        e[1],
        Vec2::new(10.0, 0.0),
        18.0,
        &[],
    );
    assert!(result.damage.is_empty());
    assert_eq!(result.delayed, vec![0]);
}

#[test]
fn an_idle_scan_reaches_plain_vision_for_either_owner() {
    let vision = 150.0;
    for computer in [true, false] {
        let scan = idle_scan(crate::combat::ScanInput {
            during_idle: true,
            flags: mint_sage_modules::AutoAcquire::IDLE,
            vision: Some(vision),
            vision_bonus: 0.0,
            height_bonus: 0.0,
            mood_check_ms: None,
            computer,
            attitude: 0,
            ranges: priority::AttitudeRanges::default(),
        })
        .expect("a unit with vision scans");
        assert!(
            (scan.reach - vision).abs() < 0.01,
            "computer {computer}: {scan:?}"
        );
    }
}

#[test]
fn an_attitude_changes_what_a_computer_units_idle_scan_reaches() {
    use crate::rules::map_scripts::{
        ATTITUDE_AGGRESSIVE, ATTITUDE_ALERT, ATTITUDE_NORMAL, ATTITUDE_PASSIVE, ATTITUDE_PEACEFUL,
        ATTITUDE_SLEEP,
    };
    let scan = |attitude| {
        idle_scan(crate::combat::ScanInput {
            during_idle: true,
            flags: mint_sage_modules::AutoAcquire::IDLE,
            vision: Some(100.0),
            vision_bonus: 0.0,
            height_bonus: 0.0,
            mood_check_ms: None,
            computer: true,
            attitude,
            ranges: priority::AttitudeRanges::default(),
        })
    };
    assert!(scan(ATTITUDE_SLEEP).is_none() && scan(ATTITUDE_PEACEFUL).is_none());
    let reach = |attitude| scan(attitude).map(|s| s.reach);
    assert_eq!(reach(ATTITUDE_NORMAL), Some(100.0));
    assert_eq!(reach(ATTITUDE_ALERT), Some(110.0));
    assert_eq!(reach(ATTITUDE_AGGRESSIVE), Some(150.0));
    assert!(scan(ATTITUDE_PASSIVE).is_some_and(|s| s.retaliate_only));
}

#[test]
fn only_the_idle_scan_needs_auto_acquire_yes() {
    let input = |during_idle| crate::combat::ScanInput {
        during_idle,
        flags: mint_sage_modules::AutoAcquire::NO,
        vision: Some(150.0),
        vision_bonus: 0.0,
        height_bonus: 0.0,
        mood_check_ms: None,
        computer: false,
        attitude: 0,
        ranges: priority::AttitudeRanges::default(),
    };
    assert!(idle_scan(input(true)).is_none());
    assert!(idle_scan(input(false)).is_some());
}

#[test]
fn standing_above_the_surrounding_ground_lengthens_sight_up_to_the_cap() {
    let bonus = mint_sage_modules::VisionBonus {
        per_foot: 0.01,
        max: 3.0,
        min: -0.5,
        test_radius: 200.0,
        test_segments: 8,
    };
    let reach = |height: f32| {
        let mesa = move |x: f32, y: f32| {
            Some(if x * x + y * y < 10_000.0 {
                height
            } else {
                0.0
            })
        };
        let rise = crate::combat::ground_rise(&bonus, &mesa, 0.0, 0.0).unwrap();
        crate::combat::vision_reach(150.0, crate::combat::height_bonus(&bonus, rise))
    };
    assert!((reach(0.0) - 150.0).abs() < 0.01);
    assert!((reach(60.0) - 150.0 * 1.6).abs() < 0.01, "{}", reach(60.0));
    assert!(
        (reach(500.0) - 150.0 * 4.0).abs() < 0.01,
        "{}",
        reach(500.0)
    );
}

#[test]
fn a_hit_carries_the_nuggets_own_death_and_sub_type() {
    let mut combat = CombatData::new();
    let poison = combat.damage_types.intern("POISON").unwrap();
    let undead = combat.damage_types.intern("BECOME_UNDEAD").unwrap();
    let burned = combat.death_types.intern("BURNED").unwrap();
    let id = combat
        .weapons
        .insert(
            "MorgulBlade",
            WeaponTemplate {
                nuggets: vec![Nugget::Damage(DamageNugget {
                    damage: 20.0,
                    damage_type: poison,
                    death_type: Some(burned),
                    sub_type: Some(undead),
                    radius: 0.0,
                    damage_scalar: 1.0,
                    delay: LogicFrames::ZERO,
                    arc: None,
                    arc_inverted: false,
                    flanking_bonus: 0.0,
                    requires_upgrade: false,
                    accepts_add: true,
                    shape: Default::default(),
                })],
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    let e = entities(2);
    let result = run_nuggets(
        &combat,
        id,
        e[0],
        Point3::new(0.0, 0.0, 0.0),
        e[1],
        Vec2::new(10.0, 0.0),
        18.0,
        &[],
    );
    assert_eq!(result.damage.len(), 1);
    assert_eq!(result.damage[0].death, DeathNaming::Named(burned));
    assert_eq!(result.damage[0].sub_type, Some(undead));
}

#[test]
fn an_area_gather_on_a_slope_leaves_out_a_victim_high_above_the_blast() {
    let [below, above] = <[Entity; 2]>::try_from(entities(2)).unwrap();
    let foes = [
        foe_at(below, 3.0, 0.0),
        Foe {
            z: 30.0,
            ..foe_at(above, 8.0, 0.0)
        },
    ];
    let reached = in_radius(foes.into_iter(), Vec2::ZERO, 0.0, 10.0, Reach::Sphere);
    assert_eq!(reached, vec![(below.index(), below)]);
}

#[test]
fn a_foe_behind_the_scanner_is_found_at_the_same_reach_as_one_in_front() {
    let scan = crate::combat::IdleScan {
        reach: 300.0,
        pursues: true,
        structures: false,
        cadence: LogicFrames::from_engine_milliseconds(2000),
        retaliate_only: false,
    };
    let here = Vec2::new(0.0, 0.0);
    assert!(scan.finds(here, 0.0, Vec2::new(180.0, 0.0), 0.0));
    assert!(scan.finds(here, 0.0, Vec2::new(-270.0, 0.0), 0.0));
    assert!(!scan.finds(here, 0.0, Vec2::new(-330.0, 0.0), 0.0));
}

fn ranked(
    foes: &[Foe],
    set: &priority::PrioritySet,
    chain: Option<(mint_sage_modules::AiKindOf, &CombatChainTable)>,
    template_of: impl Fn(Entity) -> Option<&'static str>,
) -> Option<Entity> {
    ranked_for(foes, set, chain, None, template_of, |_| None, |_| true)
}

fn ranked_for(
    foes: &[Foe],
    set: &priority::PrioritySet,
    chain: Option<(mint_sage_modules::AiKindOf, &CombatChainTable)>,
    target_player: Option<PlayerId>,
    template_of: impl Fn(Entity) -> Option<&'static str>,
    owner_of: impl Fn(Entity) -> Option<PlayerId>,
    reachable: impl Fn(Entity) -> bool,
) -> Option<Entity> {
    let ranking = priority::PriorityRanking {
        set,
        distance_modifier: 80.0,
        chain,
        target_player,
        computer: false,
    };
    priority::acquire_ranked(
        Vec2::ZERO,
        0.0,
        foes,
        0.0,
        9000.0,
        &ranking,
        |entity| priority::TargetPriority {
            template: template_of(entity),
            owner: owner_of(entity),
            ..priority::TargetPriority::default()
        },
        |foe| reachable(foe.entity),
        |_| false,
    )
}

#[test]
fn a_template_priority_outweighs_distance_and_the_default_alone_does_not() {
    let [farm, orc] = <[Entity; 2]>::try_from(entities(2)).unwrap();
    let foes = [foe_at(farm, 500.0, 0.0), foe_at(orc, 50.0, 0.0)];
    let name = |e: Entity| (e == farm).then_some("Farm").or(Some("Orc"));
    let mut set = priority::PrioritySet::default();
    set.templates.insert("farm".into(), 1000);
    assert_eq!(ranked(&foes, &set, None, name), Some(farm));
    assert_eq!(
        ranked(&foes, &priority::PrioritySet::default(), None, name),
        Some(orc)
    );
}

#[test]
fn a_chain_modifier_adds_to_the_set_priority_and_minus_one_excludes() {
    use mint_sage_modules::AiKindOf;

    let [structure, unit] = <[Entity; 2]>::try_from(entities(2)).unwrap();
    let table = CombatChainTable::new(vec![(
        AiKindOf::Infantry,
        vec![(AiKindOf::Structure, 4000.0), (AiKindOf::Wall, -1.0)],
    )]);
    let foes = [
        Foe {
            ai_kind_of: Some(AiKindOf::Structure),
            ..foe_at(structure, 300.0, 0.0)
        },
        Foe {
            ai_kind_of: Some(AiKindOf::Creep),
            ..foe_at(unit, 30.0, 0.0)
        },
    ];
    let set = priority::PrioritySet::default();
    let chain = Some((AiKindOf::Infantry, &table));
    assert_eq!(ranked(&foes, &set, chain, |_| None), Some(structure));
    assert_eq!(ranked(&foes, &set, None, |_| None), Some(unit));

    let walls = [Foe {
        ai_kind_of: Some(AiKindOf::Wall),
        ..foe_at(structure, 5.0, 0.0)
    }];
    let loud = priority::PrioritySet {
        default: 10_000,
        ..priority::PrioritySet::default()
    };
    assert_eq!(ranked(&walls, &loud, chain, |_| None), None);
}

#[test]
fn a_chain_ranked_scanner_prefers_its_ai_chosen_enemy_player() {
    use mint_sage_modules::AiKindOf;

    let [first, second] = <[Entity; 2]>::try_from(entities(2)).unwrap();
    let (a, b) = (PlayerId::new(1), PlayerId::new(2));
    let foes = [foe_at(first, 100.0, 0.0), foe_at(second, 0.0, 100.0)];
    let owner = |e: Entity| Some(if e == first { a } else { b });
    let table = CombatChainTable::default();
    let set = priority::PrioritySet::default();
    let chain = Some((AiKindOf::Infantry, &table));
    let pick = |chain, target| ranked_for(&foes, &set, chain, target, |_| None, owner, |_| true);
    assert_eq!(pick(chain, Some(b)), Some(second));
    assert_eq!(pick(chain, None), Some(first));
    assert_eq!(pick(None, Some(b)), Some(first));
}

#[test]
fn a_computer_scanner_ranks_creeps_last() {
    let [creep, orc] = <[Entity; 2]>::try_from(entities(2)).unwrap();
    let foes = [foe_at(creep, 10.0, 0.0), foe_at(orc, 600.0, 0.0)];
    let set = priority::PrioritySet::default();
    let pick = |computer: bool| {
        let ranking = priority::PriorityRanking {
            set: &set,
            distance_modifier: 80.0,
            chain: None,
            target_player: None,
            computer,
        };
        priority::acquire_ranked(
            Vec2::ZERO,
            0.0,
            &foes,
            0.0,
            9000.0,
            &ranking,
            |entity| priority::TargetPriority {
                creep: entity == creep,
                ..priority::TargetPriority::default()
            },
            |_| true,
            |_| false,
        )
    };
    assert_eq!(pick(true), Some(orc));
    assert_eq!(pick(false), Some(creep));
}

#[test]
fn an_unreachable_candidate_never_beats_a_reachable_one() {
    let [near, far] = <[Entity; 2]>::try_from(entities(2)).unwrap();
    let foes = [foe_at(near, 10.0, 0.0), foe_at(far, 400.0, 0.0)];
    let set = priority::PrioritySet::default();
    let pick = |far_only: bool| {
        ranked_for(
            &foes,
            &set,
            None,
            None,
            |_| None,
            |_| None,
            |e| !far_only || e == far,
        )
    };
    assert_eq!(pick(true), Some(far));
    assert_eq!(pick(false), Some(near));
    let none_reachable = ranked_for(&foes, &set, None, None, |_| None, |_| None, |_| false);
    assert_eq!(none_reachable, Some(near));
}

fn ranked_with<'a>(
    foes: &[Foe],
    info: impl Fn(Entity) -> priority::TargetPriority<'a>,
    reachable: impl Fn(Entity) -> bool,
    friendly_fire: impl Fn(Entity) -> bool,
) -> Option<Entity> {
    let set = priority::PrioritySet::default();
    let ranking = priority::PriorityRanking {
        set: &set,
        distance_modifier: 80.0,
        chain: None,
        target_player: None,
        computer: false,
    };
    priority::acquire_ranked(
        Vec2::ZERO,
        0.0,
        foes,
        0.0,
        20_000.0,
        &ranking,
        info,
        |foe| reachable(foe.entity),
        |foe| friendly_fire(foe.entity),
    )
}

#[test]
fn a_container_borrows_the_priority_of_what_it_holds() {
    let [building, orc] = <[Entity; 2]>::try_from(entities(2)).unwrap();
    let foes = [foe_at(building, 500.0, 0.0), foe_at(orc, 50.0, 0.0)];
    let mut set = priority::PrioritySet::default();
    set.templates.insert("hero".into(), 1000);
    let ranking = priority::PriorityRanking {
        set: &set,
        distance_modifier: 80.0,
        chain: None,
        target_player: None,
        computer: false,
    };
    let pick = |held: bool| {
        priority::acquire_ranked(
            Vec2::ZERO,
            0.0,
            &foes,
            0.0,
            9000.0,
            &ranking,
            |entity| priority::TargetPriority {
                contents: if held && entity == building {
                    vec![(Some("Hero"), None, None)]
                } else {
                    Vec::new()
                },
                ..priority::TargetPriority::default()
            },
            |_| true,
            |_| false,
        )
    };
    assert_eq!(pick(true), Some(building));
    assert_eq!(pick(false), Some(orc));
}

#[test]
fn a_member_of_a_contained_horde_is_never_picked() {
    let [member, orc] = <[Entity; 2]>::try_from(entities(2)).unwrap();
    let foes = [foe_at(member, 10.0, 0.0), foe_at(orc, 300.0, 0.0)];
    let pick = |contained: bool| {
        ranked_with(
            &foes,
            |entity| priority::TargetPriority {
                horde: (entity == member).then_some((None, None)),
                horde_contained: contained,
                ..priority::TargetPriority::default()
            },
            |_| true,
            |_| false,
        )
    };
    assert_eq!(pick(true), Some(orc));
    assert_eq!(pick(false), Some(member));
}

#[test]
fn a_reachable_candidate_is_taken_whatever_its_score() {
    let [far] = <[Entity; 1]>::try_from(entities(1)).unwrap();
    let foes = [foe_at(far, 9_000.0, 0.0)];
    let info = |_| priority::TargetPriority::default();
    assert_eq!(ranked_with(&foes, info, |_| true, |_| false), Some(far));
    assert_eq!(ranked_with(&foes, info, |_| false, |_| false), None);
}

#[test]
fn a_splash_target_among_allies_is_passed_over() {
    let [near, far] = <[Entity; 2]>::try_from(entities(2)).unwrap();
    let foes = [foe_at(near, 10.0, 0.0), foe_at(far, 300.0, 0.0)];
    let info = |_| priority::TargetPriority::default();
    assert_eq!(ranked_with(&foes, info, |_| true, |e| e == near), Some(far));
}

#[test]
fn friendly_fire_is_the_allied_share_of_the_health_in_reach() {
    assert!(priority::friendly_fire_skips(300.0, 100.0, 0.5));
    assert!(priority::friendly_fire_skips(100.0, 100.0, 0.5));
    assert!(!priority::friendly_fire_skips(100.0, 300.0, 0.5));
    assert!(priority::friendly_fire_skips(0.0, 0.5, 1.0));
    assert!(!priority::friendly_fire_skips(0.0, 200.0, 1.0));
}

#[test]
fn a_raised_weapon_bonus_shortens_the_wind_up_and_the_shot_delay() {
    let mut combat = CombatData::new();
    for (field, percent) in [("PRE_ATTACK", "50%"), ("RATE_OF_FIRE", "200%")] {
        combat
            .weapon_bonus
            .read_line(
                &mint_sage_ini::Field {
                    keyword: "WeaponBonus".into(),
                    args: vec!["VETERAN".into(), field.into(), percent.into()],
                    file: "test.ini".into(),
                    line: 1,
                },
                &mint_sage_ini::MacroTable::default(),
            )
            .unwrap();
    }
    let id = combat
        .weapons
        .insert(
            "Test",
            WeaponTemplate {
                attack_range: 100.0,
                pre_attack_delay: LogicFrames::from_frames(10),
                delay_between_shots: mint_sage_combat::FrameRange::fixed(LogicFrames::from_frames(
                    10,
                )),
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    let shots = |conditions: u32| {
        let template = combat.weapons.get(id).unwrap();
        let mut armed = Armed {
            weapon: id,
            state: WeaponState::new(template),
            target: entities(1).first().copied(),
            winding: false,
            shots_at_target: 0,
            last_shot_at: None,
            next_acquire: Frame::ZERO,
            reacquire_ready: None,
            squish_reacquire: false,
        };
        let mut rng = mint_sim::Pcg32::new(1);
        (0..32u64)
            .filter(|&frame| {
                tick_weapon(
                    &combat,
                    &mut armed,
                    Frame::new(frame),
                    false,
                    Vec2::new(0.0, 0.0),
                    1.0,
                    mint_sage_combat::BonusConditions(conditions),
                    &mut rng,
                )
                .fired()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        (shots(0), shots(mint_sage_combat::BonusConditions::VETERAN)),
        (vec![10, 30], vec![5, 15, 25])
    );
}

#[test]
fn an_absent_damage_rung_takes_the_global_ratio_only_under_use_default_damage_settings() {
    let record = |use_default: Option<bool>, really: Option<f32>| mint_sage_modules::ObjectRecord {
        max_health: Some(150.0),
        max_health_damaged: Some(75.0),
        max_health_really_damaged: really,
        use_default_damage_settings: use_default,
        ..Default::default()
    };
    let global = DamageThresholds {
        damaged: 0.65,
        really: 0.4,
    };
    let rung = |record: &mint_sage_modules::ObjectRecord| {
        DamageThresholds::of(Some(record), global).rung(30.0 / 150.0)
    };
    assert_eq!(rung(&record(Some(false), None)), DamageRung::Damaged);
    assert_eq!(rung(&record(None, None)), DamageRung::ReallyDamaged);
    assert_eq!(rung(&record(None, Some(0.0))), DamageRung::ReallyDamaged);
}
