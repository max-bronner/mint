use super::*;

const DATA: &str = "\
Object Ghost\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = GhostSet\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = AODCrushCollide Tag_Crush\n    DamageType = MAGIC_WOUND\n    \
    DeathType = SUPERNATURAL\n    Damage = 20\n    SmallObjectCreationList = OCL_CrushSpark\n    \
    MediumObjectCreationList = OCL_CrushSpark\n    LargeObjectCreationList = OCL_CrushSpark\n  End\n  \
  Behavior = ModelConditionUpgrade Tag_Oath\n    TriggeredBy = Upgrade_Oath\n    \
    AddConditionFlags = OATH_FULLFILLED\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 16\nEnd\n\
Object Victim\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = PhysicsBehavior Tag_Physics\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 16\nEnd\n\
Object Wisp\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Behavior = PhysicsBehavior Tag_Physics\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 16\nEnd\n\
Object Shed\n  Side = Wild\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 16\nEnd\n\
Object Spark\n  Side = Wild\n  KindOf = INERT IMMOBILE UNATTACKABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 2\n  GeometryHeight = 2\nEnd\n\
ObjectCreationList OCL_CrushSpark\n  CreateObject\n    ObjectNames = Spark\n    Disposition = LIKE_EXISTING\n  End\nEnd\n\
CommandSet GhostSet\n  1 = Buy_Oath\nEnd\n\
CommandButton Buy_Oath\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Oath\nEnd\n\
Upgrade Upgrade_Oath\n  Type = OBJECT\n  BuildCost = 0\n  BuildTime = 1\nEnd\n";

fn scene() -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    scene_with("Victim")
}

fn scene_with(victim_name: &str) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = [
        "Ghost".to_owned(),
        victim_name.to_owned(),
        "Spark".to_owned(),
    ];
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [
        ("Ghost", Vec2::new(300.0, 300.0), PLAYER),
        (victim_name, Vec2::new(305.0, 300.0), OTHER),
    ]
    .into_iter()
    .map(|(name, at, player)| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                at,
                if name == victim_name {
                    Facing::from_radians(std::f32::consts::FRAC_PI_2)
                } else {
                    Facing::EAST
                },
                player,
            )
            .unwrap()
    })
    .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_economy(Vec::new(), 0)
        .with_players(2)
        .with_creation_lists(Arc::clone(&data.ocls));
    let simulation = Simulation::new(7, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let ghost_kind = kinds.kind("Ghost").unwrap();
    let victim_kind = kinds.kind(victim_name).unwrap();
    let find = |kind: crate::unit::Kind| {
        simulation
            .state()
            .store(ids.kind)
            .unwrap()
            .iter()
            .find(|(_, k)| **k == kind)
            .unwrap()
            .0
    };
    let (ghost, victim) = (find(ghost_kind), find(victim_kind));
    (simulation, ids, ghost, victim)
}

#[test]
fn an_overlapping_non_ally_takes_the_crushers_own_damage() {
    let (mut simulation, ids, _ghost, victim) = scene();
    let before = simulation
        .state()
        .get(ids.health, victim)
        .unwrap()
        .0
        .current();
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    let after = simulation
        .state()
        .get(ids.health, victim)
        .unwrap()
        .0
        .current();
    assert!(
        after < before,
        "expected the victim's health to drop from {before}, found {after}"
    );
}

#[test]
fn an_object_without_physics_takes_no_crush_damage() {
    let (mut simulation, ids, _ghost, shed) = scene_with("Shed");
    let health = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.health, shed)
            .unwrap()
            .0
            .current()
    };
    let before = health(&simulation);
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    assert_eq!(
        health(&simulation),
        before,
        "a physics-less object took crush damage"
    );
}

#[test]
fn the_attack_look_ends_two_seconds_after_the_last_touch() {
    let (mut simulation, _ids, ghost, _wisp) = scene_with("Wisp");
    let firing = |simulation: &Simulation<RotwkRules>| {
        simulation.rules().modules.as_ref().is_some_and(|wiring| {
            simulation
                .state()
                .get(wiring.state, ghost)
                .is_some_and(|state| {
                    state
                        .model_condition_names()
                        .iter()
                        .any(|held| held.eq_ignore_ascii_case("FIRING_A"))
                })
        })
    };
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    assert!(
        firing(&simulation),
        "the first touch starts the attack look"
    );
    for _ in 0..LogicFrames::from_engine_milliseconds(3_000).get() {
        simulation.tick().unwrap();
    }
    assert!(
        !firing(&simulation),
        "the attack look outlived its 2 s hold"
    );
}

#[test]
fn a_crusher_holding_oath_fullfilled_deals_no_damage() {
    let (mut simulation, ids, ghost, victim) = scene();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![ghost],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::ObjectUpgrade { button: 0 })
        .unwrap();
    for _ in 0..35 {
        simulation.tick().unwrap();
    }
    assert!(
        simulation.rules().modules.as_ref().is_some_and(|wiring| {
            simulation
                .state()
                .get(wiring.state, ghost)
                .is_some_and(|state| {
                    state
                        .model_condition_names()
                        .iter()
                        .any(|held| held.eq_ignore_ascii_case("OATH_FULLFILLED"))
                })
        }),
        "fixture's premise: the ghost holds OATH_FULLFILLED after the upgrade"
    );
    let before = simulation
        .state()
        .get(ids.health, victim)
        .unwrap()
        .0
        .current();
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    let after = simulation
        .state()
        .get(ids.health, victim)
        .unwrap()
        .0
        .current();
    assert_eq!(
        after, before,
        "expected no damage from a crusher holding OATH_FULLFILLED"
    );
}

#[test]
fn a_crush_creation_list_is_made_for_the_touched_victim() {
    let (mut simulation, ids, ghost, victim) = scene();
    simulation.tick().unwrap();
    let world = simulation.state();
    let (entity, _) = world
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(entity, _)| *entity != ghost && *entity != victim)
        .expect("a spark was created");
    assert_eq!(simulation.state().get(ids.owner, entity).unwrap().0, OTHER);
    let facing = simulation.state().get(ids.body, entity).unwrap().0.facing;
    assert!((facing.radians() - std::f32::consts::FRAC_PI_2).abs() < 0.01);
}
