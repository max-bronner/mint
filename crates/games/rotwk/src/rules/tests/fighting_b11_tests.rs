use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Bow\n  AttackRange = 100\n  DelayBetweenShots = 5000\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon Blade\n  AttackRange = 10\n  DelayBetweenShots = 5000\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 1\n    DamageType = SLASH\n  End\nEnd\n\
Object Archer\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 300\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Bow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    MoodAttackCheckRate = 200\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Swordsman\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 300\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Blade\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    MoodAttackCheckRate = 200\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Peasant\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object PeasantHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  \
  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 3\n  \
  Behavior = HordeContain Tag_01\n    Slots = 2\n    \
    RankInfo = RankNumber:1 UnitType:Peasant Position:X:0 Y:0 Position:X:20 Y:0\n  End\nEnd\n\
ModifierList LongReach\n  Category = LEADERSHIP\n  Modifier = RANGE 25%\n  Duration = 0\nEnd\n\
Object Banner\n  Side = Wild\n  KindOf = SELECTABLE IMMOBILE STRUCTURE\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 2000\n  End\n  \
  Behavior = AttributeModifierAuraUpdate Tag_Aura\n    StartsActive = Yes\n    \
    BonusName = LongReach\n    RefreshDelay = 100\n    Range = 60\n    \
    ObjectFilter = ANY +INFANTRY\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_PeasantHorde\nEnd\n\
CommandButton Build_PeasantHorde\n  Command = UNIT_BUILD\n  Object = PeasantHorde\nEnd\n";

fn scene(
    seed: u64,
    shooter: &str,
    archer_at: Option<Vec2>,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>, Option<Entity>) {
    scene_with_banner(seed, shooter, archer_at, None)
}

pub(super) fn scene_with_banner(
    seed: u64,
    shooter: &str,
    archer_at: Option<Vec2>,
    banner_at: Option<Vec2>,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>, Option<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds = ["PeasantHorde", "Archer", "Swordsman", "Camp", "Banner"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawned: Vec<_> = archer_at
        .into_iter()
        .map(|at| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(shooter).unwrap(),
                    at,
                    Facing::EAST,
                    OTHER,
                )
                .unwrap()
        })
        .chain(banner_at.map(|at| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind("Banner").unwrap(),
                    at,
                    Facing::EAST,
                    OTHER,
                )
                .unwrap()
        }))
        .collect();
    let passability = Passability::build(&flat_map(160, 160), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawned)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_melee_hordes(data.formations.melee().to_vec())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec!["Creeps".to_owned(), "Bystander".to_owned()],
            vec![
                crate::alliance::MapRelations {
                    universal_hostile: true,
                    ..crate::alliance::MapRelations::default()
                },
                crate::alliance::MapRelations::default(),
            ],
        );
    let mut simulation = Simulation::new(seed, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "PeasantHorde")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 400.0,
                y: 400.0,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let horde = simulation.state().store(ids.horde).unwrap().entities()[0];
    let members = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    let shooter = simulation
        .state()
        .store(ids.armed)
        .unwrap()
        .entities()
        .first()
        .copied();
    (simulation, ids, members, shooter)
}

pub(super) fn position(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> Vec2 {
    simulation.state().get(ids.body, entity).unwrap().0.position
}

pub(super) fn line_of_members() -> (Vec2, Vec2, Vec2) {
    let (simulation, ids, members, _) = scene(1, "Archer", None);
    let first = position(&simulation, ids, members[0]);
    let last = position(&simulation, ids, *members.last().unwrap());
    let along = last.minus(first);
    (first, last, along.scale(1.0 / along.length()))
}

fn picks(shooter: &str, at: Vec2, seeds: u64) -> Vec<usize> {
    (1..=seeds)
        .filter_map(|seed| {
            let (mut simulation, ids, members, who) = scene(seed, shooter, Some(at));
            let who = who.unwrap();
            for _ in 0..40 {
                simulation.tick().unwrap();
                if let Some(target) = simulation
                    .state()
                    .get(ids.armed, who)
                    .and_then(|a| a.target)
                {
                    return members.iter().position(|member| *member == target);
                }
            }
            None
        })
        .collect()
}

#[test]
fn a_ranged_unit_that_finds_a_horde_member_picks_among_the_members_with_a_jitter() {
    let (first, _, along) = line_of_members();
    let at = first.minus(along.scale(60.0));
    let chosen = picks("Archer", at, 24);
    assert!(chosen.len() >= 20, "the archer never acquired: {chosen:?}");
    assert!(chosen.contains(&0), "never the nearer member: {chosen:?}");
    assert!(chosen.contains(&1), "never the farther member: {chosen:?}");
}

#[test]
fn a_melee_unit_that_finds_a_horde_member_picks_the_nearest_member_every_time() {
    let (_, last, along) = line_of_members();
    let at = last.plus(along.scale(60.0));
    let chosen = picks("Swordsman", at, 8);
    assert!(
        chosen.len() >= 6,
        "the swordsman never acquired: {chosen:?}"
    );
    assert!(chosen.iter().all(|&pick| pick == 1), "{chosen:?}");
}

#[test]
fn the_pick_limits_a_member_by_its_centre_distance_not_its_surface() {
    let (first, last, along) = line_of_members();
    let middle = first.plus(last).scale(0.5);
    let side = Vec2::new(-along.y, along.x);
    let toward_last = (102.0_f32 * 102.0 - 90.0 * 90.0) / 40.0;
    let height = (90.0_f32 * 90.0 - (toward_last - 10.0) * (toward_last - 10.0)).sqrt();
    let at = middle
        .plus(along.scale(toward_last))
        .plus(side.scale(height));
    let chosen = picks("Archer", at, 24);
    assert!(chosen.len() >= 20, "the archer never acquired: {chosen:?}");
    assert!(chosen.iter().all(|&pick| pick == 1), "{chosen:?}");
}
