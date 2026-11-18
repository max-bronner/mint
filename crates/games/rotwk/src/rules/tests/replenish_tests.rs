use super::*;

const DATA: &str = "\
Locomotor HordeFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 100\n  MaxTurnWithoutReform = 45\nEnd\n\
Armor Plain\n  Armor = DEFAULT 100%\nEnd\n\
Weapon Claw\n  AttackRange = 200\n  DelayBetweenShots = 2000\n  \
  DamageNugget\n    Damage = 200000\n    DamageType = SLASH\n  End\nEnd\n\
Weapon Prick\n  AttackRange = 200\n  DelayBetweenShots = 300\n  \
  DamageNugget\n    Damage = 1\n    DamageType = SLASH\n  End\nEnd\n\
Object Fighter\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Bannerman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE BANNER\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Behavior = BannerCarrierUpdate Tag_Banner\n    IdleSpawnRate = 1000\n    \
    MeleeFreeUnitSpawnTime = 2000\n    DiedRespawnTime = 1000\n    \
    MeleeFreeBannerReSpawnTime = 2000\n    UnitSpawnFX = FX_Spawn\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object FighterHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  \
  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 5\n  \
  LocomotorSet\n    Locomotor = HordeFeet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 3\n    \
    BannerCarriersAllowed = Bannerman\n    BannerCarrierMinLevel = 0\n    \
    BannerCarrierPosition = UnitType:Fighter Pos:X:-30.0 Y:0.0\n    \
    RankInfo = RankNumber:1 UnitType:Fighter Position:X:10 Y:-10 Position:X:10 Y:10 \
Position:X:10 Y:0\n  End\nEnd\n\
ExperienceLevel HordeLevel1\n  TargetNames = Fighter FighterHorde Bannerman\n  \
  RequiredExperience = 1\n  Rank = 1\nEnd\n\
ExperienceLevel HordeLevel2\n  TargetNames = Fighter FighterHorde Bannerman\n  \
  RequiredExperience = 10\n  Rank = 2\nEnd\n\
Object Killer\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Claw\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Pricker\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Prick\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Barracks\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = BarracksSet\n  BuildCost = 100\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 30\nEnd\n\
CommandSet BarracksSet\n  1 = Build_FighterHorde\nEnd\n\
CommandButton Build_FighterHorde\n  Command = UNIT_BUILD\n  Object = FighterHorde\nEnd\n";

const RANK_DATA: &str = r"Object ThreeRankHorde
  Side = Men
  KindOf = SELECTABLE HORDE
  BuildCost = 100
  BuildTime = 1
  CommandPoints = 5
  LocomotorSet
    Locomotor = HordeFeet
    Condition = SET_NORMAL
    Speed = 40
  End
  Behavior = HordeContain Tag_Contain
    Slots = 9
    BannerCarriersAllowed = Bannerman
    BannerCarrierMinLevel = 0
    BannerCarrierPosition = UnitType:Fighter Pos:X:-30.0 Y:0.0
    RankInfo = RankNumber:1 UnitType:Fighter Position:X:30 Y:-10 Position:X:30 Y:0 Position:X:30 Y:10
    RankInfo = RankNumber:2 UnitType:Fighter Position:X:20 Y:-10 Position:X:20 Y:0 Position:X:20 Y:10
    RankInfo = RankNumber:3 UnitType:Fighter Position:X:10 Y:-10 Position:X:10 Y:0 Position:X:10 Y:10
  End
End
Object StoppedHorde
  Side = Men
  KindOf = SELECTABLE HORDE
  BuildCost = 100
  BuildTime = 1
  CommandPoints = 5
  LocomotorSet
    Locomotor = HordeFeet
    Condition = SET_NORMAL
    Speed = 40
  End
  Behavior = HordeContain Tag_Contain
    Slots = 9
    RanksThatStopAdvance = 2
    RankInfo = RankNumber:1 UnitType:Fighter Position:X:30 Y:-10 Position:X:30 Y:0 Position:X:30 Y:10
    RankInfo = RankNumber:2 UnitType:Fighter Position:X:20 Y:-10 Position:X:20 Y:0 Position:X:20 Y:10
    RankInfo = RankNumber:3 UnitType:Fighter Position:X:10 Y:-10 Position:X:10 Y:0 Position:X:10 Y:10
  End
End
Object RankBarracks
  Side = Men
  KindOf = STRUCTURE SELECTABLE IMMOBILE
  CommandSet RankSet
  BuildCost = 100
  BuildTime = 1
  Body = StructureBody Tag_Body
    MaxHealth = 3000
  End
  Behavior = ProductionUpdate Tag_Production
    MaxQueueEntries = 3
  End
  Geometry = BOX
  GeometryMajorRadius = 20
  GeometryMinorRadius = 20
  GeometryHeight = 30
End
CommandSet RankSet
  1 = Build_ThreeRankHorde
  2 = Build_StoppedHorde
End
CommandButton Build_ThreeRankHorde
  Command = UNIT_BUILD
  Object = ThreeRankHorde
End
CommandButton Build_StoppedHorde
  Command = UNIT_BUILD
  Object = StoppedHorde
End
";

const MIXED_DATA: &str = "\
Object FighterB\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object MixedHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  \
  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 5\n  \
  LocomotorSet\n    Locomotor = HordeFeet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 2\n    \
    BannerCarriersAllowed = Bannerman\n    BannerCarrierMinLevel = 0\n    \
    BannerCarrierPosition = UnitType:Fighter Pos:X:-30.0 Y:0.0\n    \
    RankInfo = RankNumber:1 UnitType:Fighter Position:X:10 Y:-10\n    \
    RankInfo = RankNumber:2 UnitType:FighterB Position:X:10 Y:10\n  End\nEnd\n\
CommandSet MixedSet\n  1 = Build_MixedHorde\nEnd\n\
CommandButton Build_MixedHorde\n  Command = UNIT_BUILD\n  Object = MixedHorde\nEnd\n\
Object MixedBarracks\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = MixedSet\n  BuildCost = 100\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 30\nEnd\n";

struct Base {
    data: GameData,
    kinds: Arc<KindTable>,
}

fn load() -> Base {
    let text = format!("{DATA}{RANK_DATA}");
    let doc =
        mint_sage_ini::Document::parse("t.ini", &text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = [
        "Barracks",
        "RankBarracks",
        "FighterHorde",
        "ThreeRankHorde",
        "StoppedHorde",
        "Killer",
        "Pricker",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    Base {
        data,
        kinds: Arc::new(kinds),
    }
}

fn mixed_base() -> Base {
    let text = format!("{DATA}{MIXED_DATA}");
    let doc =
        mint_sage_ini::Document::parse("t.ini", &text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["MixedBarracks", "MixedHorde", "FighterB", "Killer"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    Base {
        data,
        kinds: Arc::new(kinds),
    }
}

fn destroy_on_death_base() -> Base {
    let text = DATA.replace(
        "BannerCarrierMinLevel = 0\n",
        "BannerCarrierMinLevel = 0\n    BannerCarrierDestroyHordeOnDeath = Yes\n",
    );
    let doc =
        mint_sage_ini::Document::parse("t.ini", &text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Barracks", "FighterHorde", "Killer", "Pricker"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    Base {
        data,
        kinds: Arc::new(kinds),
    }
}

fn start(b: &Base, placed: &[(&str, f32, f32, PlayerId)]) -> (Simulation<RotwkRules>, Ids) {
    let production = b.kinds.production(&b.data);
    let power_names = crate::powers::PowerNames::build(b.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&b.data.special_powers);
    let spawns: Vec<Spawn> = placed
        .iter()
        .map(|(name, x, y, owner)| {
            let kind = b.kinds.kind(name).unwrap();
            b.kinds
                .spawn(
                    &b.data.combat,
                    kind,
                    Vec2::new(*x, *y),
                    Facing::EAST,
                    *owner,
                )
                .unwrap()
        })
        .collect();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_hordes(b.data.formations.tables().to_vec(), Vec::new())
        .with_banners(crate::horde::banner_plans(&b.data.formations, &b.kinds))
        .with_combat(b.data.combat.clone())
        .with_construction(Arc::clone(&b.kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_upgrades(b.kinds.upgrades(&b.data), Arc::clone(&b.data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_experience(Arc::clone(&b.data.experience), b.data.armor_bonus_cap);
    let simulation = Simulation::new(23, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

fn spawn_horde(simulation: &mut Simulation<RotwkRules>, ids: Ids, x: f32, y: f32) -> Entity {
    spawn_horde_of(simulation, ids, "FighterHorde", x, y)
}

fn spawn_horde_of(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    label: &str,
    x: f32,
    y: f32,
) -> Entity {
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
                x,
                y,
                facing: 0.0,
            },
        )
        .unwrap();
    run(simulation, 2);
    simulation
        .state()
        .store(ids.horde)
        .unwrap()
        .entities()
        .last()
        .copied()
        .unwrap()
}

fn members(simulation: &Simulation<RotwkRules>, ids: Ids, horde: Entity) -> usize {
    simulation
        .state()
        .get(ids.horde, horde)
        .map_or(0, |horde| horde.members.len())
}

fn holds_a(
    b: &Base,
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    horde: Entity,
    name: &str,
) -> bool {
    let wanted = b.kinds.kind(name).unwrap();
    simulation
        .state()
        .get(ids.horde, horde)
        .is_some_and(|horde| {
            horde
                .members
                .iter()
                .any(|member| simulation.state().get(ids.kind, *member) == Some(&wanted))
        })
}

fn remove(b: &Base, simulation: &mut Simulation<RotwkRules>, ids: Ids, name: &str) {
    let kind = b.kinds.kind(name).unwrap();
    let entity = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(entity, held)| **held == kind && simulation.state().is_live(*entity))
        .map(|(entity, _)| entity)
        .unwrap();
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![entity],
            },
        )
        .unwrap();
    simulation.submit(OTHER, RotwkCommand::DebugKill).unwrap();
    run(simulation, 2);
}

#[test]
fn a_horde_out_of_combat_regrows_the_members_it_lost() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("Barracks", 100.0, 100.0, PLAYER),
            ("Killer", 400.0, 300.0, OTHER),
        ],
    );
    let horde = spawn_horde(&mut simulation, ids, 300.0, 300.0);
    assert_eq!(members(&simulation, ids, horde), 4, "three and the carrier");
    run(&mut simulation, 60);
    assert!(members(&simulation, ids, horde) < 4, "a member was killed");
    remove(&b, &mut simulation, ids, "Killer");
    run(&mut simulation, 300);
    assert_eq!(members(&simulation, ids, horde), 4);
}

#[test]
fn a_mixed_horde_regrows_whichever_type_it_lost() {
    let b = mixed_base();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("MixedBarracks", 100.0, 100.0, PLAYER),
            ("Killer", 400.0, 300.0, OTHER),
        ],
    );
    let horde = spawn_horde_of(&mut simulation, ids, "MixedHorde", 300.0, 300.0);
    assert_eq!(
        members(&simulation, ids, horde),
        3,
        "banner, Fighter, FighterB"
    );
    run(&mut simulation, 60);
    assert_eq!(
        members(&simulation, ids, horde),
        2,
        "exactly one of the two should have died"
    );
    remove(&b, &mut simulation, ids, "Killer");
    run(&mut simulation, 300);
    assert_eq!(
        members(&simulation, ids, horde),
        3,
        "the lost slot's own type should have regrown"
    );
    assert!(holds_a(&b, &simulation, ids, horde, "Fighter"));
    assert!(holds_a(&b, &simulation, ids, horde, "FighterB"));
}

#[test]
fn a_regrown_member_takes_the_rank_the_horde_reached() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("Barracks", 100.0, 100.0, PLAYER),
            ("Killer", 400.0, 300.0, OTHER),
        ],
    );
    let horde = spawn_horde(&mut simulation, ids, 300.0, 300.0);
    let before: Vec<Entity> = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: before.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::DebugExperience { points: 10 })
        .unwrap();
    run(&mut simulation, 60);
    let rank_of = |simulation: &Simulation<RotwkRules>, entity| {
        simulation.rules().rank_of(simulation.state(), entity)
    };
    assert_eq!(rank_of(&simulation, horde), Some(2));
    assert!(members(&simulation, ids, horde) < 4, "a member was killed");
    remove(&b, &mut simulation, ids, "Killer");
    run(&mut simulation, 300);
    assert_eq!(members(&simulation, ids, horde), 4);
    let regrown: Vec<Entity> = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .iter()
        .copied()
        .filter(|member| !before.contains(member))
        .collect();
    assert!(!regrown.is_empty(), "at least one member regrew");
    for member in regrown {
        assert_eq!(rank_of(&simulation, member), Some(2));
    }
}

#[test]
fn a_horde_still_being_hit_does_not_regrow() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("Barracks", 100.0, 100.0, PLAYER),
            ("Killer", 400.0, 300.0, OTHER),
            ("Pricker", 200.0, 300.0, OTHER),
        ],
    );
    let horde = spawn_horde(&mut simulation, ids, 300.0, 300.0);
    run(&mut simulation, 60);
    let short = members(&simulation, ids, horde);
    assert!(short < 4, "a member was killed");
    remove(&b, &mut simulation, ids, "Killer");
    run(&mut simulation, 300);
    assert_eq!(
        members(&simulation, ids, horde),
        short,
        "a horde under fire is never melee-free for 2 s"
    );
}

#[test]
fn a_lost_banner_carrier_comes_back() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("Barracks", 100.0, 100.0, PLAYER),
            ("Killer", 200.0, 300.0, OTHER),
        ],
    );
    let horde = spawn_horde(&mut simulation, ids, 300.0, 300.0);
    run(&mut simulation, 10);
    assert!(
        !holds_a(&b, &simulation, ids, horde, "Bannerman"),
        "the carrier was killed"
    );
    run(&mut simulation, 30);
    assert!(holds_a(&b, &simulation, ids, horde, "Bannerman"));
}

#[test]
fn a_destroy_on_death_banner_kills_its_whole_horde() {
    let b = destroy_on_death_base();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("Barracks", 100.0, 100.0, PLAYER),
            ("Killer", 200.0, 300.0, OTHER),
        ],
    );
    let horde = spawn_horde(&mut simulation, ids, 300.0, 300.0);
    run(&mut simulation, 60);
    assert!(
        !holds_a(&b, &simulation, ids, horde, "Bannerman"),
        "the carrier was killed"
    );
    assert_eq!(
        members(&simulation, ids, horde),
        0,
        "the whole horde should have died with its carrier"
    );
}

#[test]
fn a_turn_does_not_move_the_banner_out_of_its_own_slot() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("Barracks", 100.0, 100.0, PLAYER)]);
    let horde = spawn_horde(&mut simulation, ids, 300.0, 300.0);
    let banner_slot = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .banner_slot
        .expect("min level 0 grants a carrier at spawn");
    let carrier = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .iter()
        .copied()
        .find(|member| {
            simulation
                .state()
                .get(ids.member, *member)
                .and_then(|member| member.slot)
                == Some(banner_slot)
        })
        .expect("one member holds the banner's slot");
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: simulation
                    .state()
                    .get(ids.horde, horde)
                    .unwrap()
                    .members
                    .clone(),
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 300.0,
                y: 50.0,
                queue: false,
            },
        )
        .unwrap();
    run(&mut simulation, 90);
    let facing_after = simulation
        .state()
        .get(ids.body, horde)
        .unwrap()
        .0
        .facing
        .radians();
    assert!(
        facing_after.abs() > 45f32.to_radians(),
        "the horde never turned far enough to reform: {facing_after}"
    );
    assert_eq!(
        simulation
            .state()
            .get(ids.member, carrier)
            .and_then(|member| member.slot),
        Some(banner_slot),
        "the turn reassigned the carrier out of its own slot"
    );
    for member in &simulation.state().get(ids.horde, horde).unwrap().members {
        if *member == carrier {
            continue;
        }
        assert_ne!(
            simulation
                .state()
                .get(ids.member, *member)
                .and_then(|member| member.slot),
            Some(banner_slot),
            "a soldier was assigned the banner's own slot"
        );
    }
}

fn holder_of(
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    horde: Entity,
    slot: u16,
) -> Option<Entity> {
    simulation
        .state()
        .get(ids.horde, horde)?
        .members
        .iter()
        .copied()
        .find(|member| {
            simulation
                .state()
                .get(ids.member, *member)
                .and_then(|m| m.slot)
                == Some(slot)
        })
}

fn first_death(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    horde: Entity,
) -> (u16, Vec<Option<Entity>>) {
    let count = members(simulation, ids, horde);
    for _ in 0..600 {
        let before: Vec<Option<Entity>> = (0..9)
            .map(|slot| holder_of(simulation, ids, horde, slot))
            .collect();
        run(simulation, 1);
        if members(simulation, ids, horde) < count {
            let members = simulation
                .state()
                .get(ids.horde, horde)
                .unwrap()
                .members
                .clone();
            let victim = (0..9)
                .find(|slot| {
                    before
                        .get(usize::from(*slot))
                        .copied()
                        .flatten()
                        .is_some_and(|held| !members.contains(&held))
                })
                .unwrap();
            return (victim, before);
        }
    }
    panic!("nobody died");
}

#[test]
fn a_death_cascades_down_the_column_and_replenishment_fills_the_back() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("RankBarracks", 100.0, 100.0, PLAYER),
            ("Killer", 480.0, 300.0, OTHER),
        ],
    );
    let horde = spawn_horde_of(&mut simulation, ids, "ThreeRankHorde", 300.0, 300.0);
    let (victim, before) = first_death(&mut simulation, ids, horde);
    assert_eq!(victim, 1, "the front-centre member was the nearest target");
    assert_eq!(holder_of(&simulation, ids, horde, 1), before[4]);
    assert_eq!(holder_of(&simulation, ids, horde, 4), before[7]);
    assert_eq!(holder_of(&simulation, ids, horde, 7), None);
    assert_eq!(
        simulation.state().get(ids.horde, horde).unwrap().free,
        vec![7]
    );
    remove(&b, &mut simulation, ids, "Killer");
    run(&mut simulation, 300);
    assert_eq!(members(&simulation, ids, horde), 10, "nine and the carrier");
    assert!(holder_of(&simulation, ids, horde, 7).is_some());
}

#[test]
fn a_rank_that_stops_advance_keeps_the_rank_behind_it_in_place() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("RankBarracks", 100.0, 100.0, PLAYER),
            ("Killer", 480.0, 300.0, OTHER),
        ],
    );
    let horde = spawn_horde_of(&mut simulation, ids, "StoppedHorde", 300.0, 300.0);
    let (victim, before) = first_death(&mut simulation, ids, horde);
    assert_eq!(victim, 1);
    assert_eq!(holder_of(&simulation, ids, horde, 1), before[4]);
    assert_eq!(holder_of(&simulation, ids, horde, 4), None);
    assert_eq!(holder_of(&simulation, ids, horde, 7), before[7]);
}
