use super::*;

const DATA: &str = "\
SpecialPower SpecialAbilityGrant\n  ReloadTime = 0\nEnd\n\
ExperienceLevel UnitLevel1\n  TargetNames = Raider RaiderHorde Archer MixedHorde Granter Stranger\n  \
  RequiredExperience = 1\n  Rank = 1\nEnd\n\
ExperienceLevel UnitLevel2\n  TargetNames = Raider RaiderHorde Archer MixedHorde Granter Stranger\n  \
  RequiredExperience = 100000\n  Rank = 2\nEnd\n\
Object Raider\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object RaiderHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = EmptySet\n  \
  BuildCost = 300\n  BuildTime = 2\n  CommandPoints = 10\n  \
  Behavior = HordeContain Tag_01\n    Slots = 3\n    \
    RankInfo = RankNumber:1 UnitType:Raider Position:X:20 Y:-20 Position:X:20 Y:20 Position:X:0 Y:0\n  \
  End\nEnd\n\
Object Archer\n  Side = Men\n  KindOf = INFANTRY SELECTABLE BANNER\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object MixedHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = EmptySet\n  \
  BuildCost = 300\n  BuildTime = 2\n  CommandPoints = 10\n  \
  Behavior = HordeContain Tag_01\n    Slots = 3\n    \
    BannerCarriersAllowed = Archer\n    BannerCarrierMinLevel = 0\n    \
    BannerCarrierPosition = UnitType:Raider Pos:X:0.0 Y:0.0\n    \
    RankInfo = RankNumber:1 UnitType:Raider Position:X:20 Y:-20 Position:X:20 Y:20\n  \
  End\nEnd\n\
CommandSet EmptySet\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_RaiderHorde\n  2 = Build_MixedHorde\nEnd\n\
CommandButton Build_MixedHorde\n  Command = UNIT_BUILD\n  Object = MixedHorde\nEnd\n\
CommandButton Build_RaiderHorde\n  Command = UNIT_BUILD\n  Object = RaiderHorde\nEnd\n\
Object Granter\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  CommandSet = GrantSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = LevelGrantSpecialPower Tag_Grant\n    SpecialPowerTemplate = SpecialAbilityGrant\n    \
    StartAbilityRange = 200\n    UnpackTime = 0\n    PreparationTime = 0\n    \
    Experience = 50\n    RadiusEffect = 100\n    AcceptanceFilter = NONE +INFANTRY\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet GrantSet\n  1 = Cast_Grant\nEnd\n\
CommandButton Cast_Grant\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityGrant\n  \
  Options = NEED_TARGET_POS\nEnd\n\
Object Stranger\n  Side = Other\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

struct Match {
    data: GameData,
    kinds: Arc<KindTable>,
}

fn load() -> Match {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = [
        "Camp",
        "Granter",
        "Raider",
        "RaiderHorde",
        "Archer",
        "MixedHorde",
        "Stranger",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    Match {
        data,
        kinds: Arc::new(kinds),
    }
}

fn simulation(m: &Match, placed: &[(&str, f32, f32, PlayerId)]) -> Simulation<RotwkRules> {
    let production = m.kinds.production(&m.data);
    let power_names = crate::powers::PowerNames::build(m.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&m.data.special_powers);
    let spawns: Vec<Spawn> = placed
        .iter()
        .map(|(name, x, y, owner)| {
            let kind = m.kinds.kind(name).unwrap();
            m.kinds
                .spawn(
                    &m.data.combat,
                    kind,
                    Vec2::new(*x, *y),
                    Facing::EAST,
                    *owner,
                )
                .unwrap()
        })
        .collect();
    let map = flat_map(160, 160);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_hordes(m.data.formations.tables().to_vec(), Vec::new())
        .with_banners(crate::horde::banner_plans(&m.data.formations, &m.kinds))
        .with_combat(m.data.combat.clone())
        .with_construction(Arc::clone(&m.kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_powers(power_names, power_reload)
        .with_experience(Arc::clone(&m.data.experience), m.data.armor_bonus_cap)
        .with_experience_scalars(Arc::clone(&m.data.experience_scalars));
    Simulation::new(5, rules).unwrap()
}

fn points(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> u32 {
    simulation
        .rules()
        .experience_of(simulation.state(), ids, entity)
        .unwrap()
        .1
}

fn of_kind(m: &Match, simulation: &Simulation<RotwkRules>, ids: Ids, name: &str) -> Entity {
    let kind = m.kinds.kind(name).unwrap();
    simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(_, k)| **k == kind)
        .map(|(entity, _)| entity)
        .unwrap()
}

#[test]
fn a_level_grant_reaches_allied_hordes_once_and_neither_enemy_nor_caster() {
    let m = load();
    let mut simulation = simulation(
        &m,
        &[
            ("Granter", 200.0, 200.0, PLAYER),
            ("Stranger", 235.0, 205.0, OTHER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "RaiderHorde")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 230.0,
                y: 200.0,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let store = simulation.state().store(ids.horde).unwrap();
    let horde = store.entities().first().copied().unwrap();
    let members = store.get(horde).unwrap().members.clone();
    assert_eq!(members.len(), 3, "the fixture's premise");

    let caster = of_kind(&m, &simulation, ids, "Granter");
    let stranger = of_kind(&m, &simulation, ids, "Stranger");
    let watched: Vec<Entity> = [horde, caster, stranger]
        .into_iter()
        .chain(members.iter().copied())
        .collect();
    let before: Vec<u32> = watched
        .iter()
        .map(|e| points(&simulation, ids, *e))
        .collect();

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![caster],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: None,
                at: Some((230.0, 200.0)),
            },
        )
        .unwrap();
    for _ in 0..40 {
        simulation.tick().unwrap();
    }

    let gained: Vec<u32> = watched
        .iter()
        .zip(&before)
        .map(|(e, b)| points(&simulation, ids, *e) - b)
        .collect();
    assert_eq!(gained, vec![50, 0, 0, 50, 50, 50]);
}

fn spawn_horde_of(simulation: &mut Simulation<RotwkRules>, ids: Ids, label: &str) -> Entity {
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == label)
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 230.0,
                y: 200.0,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    simulation
        .state()
        .store(ids.horde)
        .unwrap()
        .entities()
        .first()
        .copied()
        .unwrap()
}

fn cast_grant(m: &Match, simulation: &mut Simulation<RotwkRules>, ids: Ids) {
    let caster = of_kind(m, simulation, ids, "Granter");
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![caster],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: None,
                at: Some((230.0, 200.0)),
            },
        )
        .unwrap();
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
}

#[test]
fn a_mixed_horde_splits_a_grant_over_template_pools_and_keeps_higher_kill_points() {
    let m = load();
    let mut simulation = simulation(&m, &[("Granter", 200.0, 200.0, PLAYER)]);
    let ids = simulation.rules().ids().unwrap();
    let horde = spawn_horde_of(&mut simulation, ids, "MixedHorde");
    let members = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    let raider = m.kinds.kind("Raider").unwrap();
    let kind_of = |simulation: &Simulation<RotwkRules>, e: Entity| {
        *simulation.state().get(ids.kind, e).unwrap()
    };
    assert_eq!(members.len(), 3, "the fixture's premise");
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![horde],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::DebugExperience { points: 1000 })
        .unwrap();
    simulation.tick().unwrap();
    let veteran = points(&simulation, ids, horde);
    assert!(veteran > 34, "precondition: {veteran}");
    cast_grant(&m, &mut simulation, ids);
    assert_eq!(points(&simulation, ids, horde), veteran);
    for member in members {
        let want = if kind_of(&simulation, member) == raider {
            34
        } else {
            17
        };
        assert_eq!(points(&simulation, ids, member), want);
    }
}
