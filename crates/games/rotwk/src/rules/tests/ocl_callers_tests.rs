use super::*;

const DATA: &str = "\
Weapon MarkGun\n  AttackRange = 400\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 1\n    DamageType = CRUSH\n  End\n  \
  WeaponOCLNugget\n    WeaponOCLName = OCL_Mark\n  End\nEnd\n\
Weapon Reaper\n  AttackRange = 400\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 5000\n    DamageType = CRUSH\n    DamageSubType = TEST_SUB\n  End\nEnd\n\
ObjectCreationList OCL_Mark\n  CreateObject\n    ObjectNames = Mark\n  End\nEnd\n\
ObjectCreationList OCL_Raise\n  CreateObject\n    ObjectNames = Mark\n  End\nEnd\n\
Object MarkShooter\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY MarkGun\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ReaperShooter\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Reaper\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Dummy\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Sleeper\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = DamageFilteredCreateObjectDie Tag_Raise\n    CreationList = OCL_Raise\n    \
    DamageTypeTriggersInstantly = TEST_SUB\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Mark\n  KindOf = INERT IMMOBILE UNATTACKABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\nEnd\n";

fn mark_after(shooter: &str, target: &str, ticks: u32) -> (Simulation<RotwkRules>, Ids, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = [shooter, target, "Mark"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [
        (shooter, Vec2::new(300.0, 300.0), PLAYER),
        (target, Vec2::new(500.0, 300.0), OTHER),
    ]
    .into_iter()
    .map(|(name, at, player)| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                at,
                Facing::EAST,
                player,
            )
            .unwrap()
    })
    .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_creation_lists(Arc::clone(&data.ocls))
        .with_players(2);
    let mut simulation = Simulation::new(11, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let (me, victim) = (entities[0], entities[1]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![me],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: victim })
        .unwrap();
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    let mark = kinds.kind("Mark").unwrap();
    let made = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(_, kind)| **kind == mark)
        .map(|(entity, _)| entity)
        .expect("a Mark was created");
    (simulation, ids, made)
}

#[test]
fn a_weapon_list_on_an_object_hit_starts_at_the_firer() {
    let (simulation, ids, made) = mark_after("MarkShooter", "Dummy", 40);
    let at = simulation.state().get(ids.body, made).unwrap().0.position;
    assert!((at.x - 300.0).abs() < 1.0, "made at {at:?}");
}

#[test]
fn a_damage_filtered_die_list_is_made_for_the_killer() {
    let (simulation, ids, made) = mark_after("ReaperShooter", "Sleeper", 80);
    assert_eq!(simulation.state().get(ids.owner, made).unwrap().0, PLAYER);
    let at = simulation.state().get(ids.body, made).unwrap().0.position;
    assert!((at.x - 500.0).abs() < 30.0, "made at {at:?}");
}
