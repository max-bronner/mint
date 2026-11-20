use super::*;

const DATA: &str = "\
SpecialPower SpecialAbilityRevenge\n  ReloadTime = 60000\nEnd\n\
SpecialPower SpecialAbilityGrace\n  ReloadTime = 60000\nEnd\n\
SpecialPower SpecialAbilityCurseP\n  ReloadTime = 1000\n  RadiusCursorRadius = 180\nEnd\n\
SpecialPower SpecialAbilityRestore\n  ReloadTime = 1000\nEnd\n\
ModifierList Calm\n  Category = SPELL\n  Modifier = DAMAGE_MULT 150%\n  Duration = 3000\nEnd\n\
CommandSet PowerSet\n  1 = Fire_Revenge\n  2 = Fire_Grace\nEnd\n\
CommandButton Fire_Revenge\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityRevenge\nEnd\n\
CommandButton Fire_Grace\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityGrace\nEnd\n\
Object Fallen\n  Side = Men\n  KindOf = HERO INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = HeroDie Tag_Die\n    SpecialPowerTemplate = SpecialAbilityRevenge\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Holder\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = PowerSet\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rev\n    SpecialPowerTemplate = SpecialAbilityRevenge\n    \
    Duration = 3000\n    AttributeModifier = Calm\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Grace\n    SpecialPowerTemplate = SpecialAbilityGrace\n    \
    Duration = 3000\n    AttributeModifier = Calm\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object RivalHero\n  Side = Mordor\n  KindOf = HERO INFANTRY SELECTABLE\n  CommandSet = PowerSet\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rev\n    SpecialPowerTemplate = SpecialAbilityRevenge\n    \
    Duration = 3000\n    AttributeModifier = Calm\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object RivalGrunt\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = PowerSet\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rev\n    SpecialPowerTemplate = SpecialAbilityRevenge\n    \
    Duration = 3000\n    AttributeModifier = Calm\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet CurseSet\n  1 = Cast_Curse\nEnd\n\
CommandButton Cast_Curse\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityCurseP\n  \
  Options = NEED_TARGET_POS\nEnd\n\
Object Cursor\n  Side = Men\n  KindOf = HERO INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  CommandSet = CurseSet\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = CurseSpecialPower Tag_Curse\n    SpecialPowerTemplate = SpecialAbilityCurseP\n    \
    CursePercentage = 100%\n    IgnoreFacingCheck = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet MenderSet\n  1 = Fire_Revenge\n  2 = Fire_Grace\n  3 = Cast_Restore\nEnd\n\
CommandButton Cast_Restore\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityRestore\nEnd\n\
SpecialPower SpecialAbilityVolley\n  ReloadTime = 1000\nEnd\n\
Weapon StormArrow\n  AttackRange = 500\n  DelayBetweenShots = 100\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
CommandSet StormSet\n  1 = Cast_Storm\nEnd\n\
CommandButton Cast_Storm\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityVolley\n  \
  Options = NEED_TARGET_POS\nEnd\n\
Object Stormer\n  Side = Men\n  KindOf = HERO INFANTRY SELECTABLE\n  CommandSet = StormSet\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = ArrowStormUpdate Tag_Storm\n    SpecialPowerTemplate = SpecialAbilityVolley\n    \
    WeaponTemplate = StormArrow\n    TargetRadius = 100\n    ShotsPerTarget = 1\n    \
    ShotsPerBurst = 4\n    MaxShots = 4\n    PersistentPrepTime = 100\n    IgnoreFacingCheck = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Locomotor StormFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 100\nEnd\n\
Object SlowStormer\n  Side = Men\n  KindOf = HERO INFANTRY SELECTABLE\n  CommandSet = StormSet\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = ArrowStormUpdate Tag_Storm\n    SpecialPowerTemplate = SpecialAbilityVolley\n    \
    WeaponTemplate = StormArrow\n    TargetRadius = 100\n    ShotsPerTarget = 1\n    \
    ShotsPerBurst = 4\n    MaxShots = 4\n    PreparationTime = 1500\n    PersistentPrepTime = 100\n    \
    IgnoreFacingCheck = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ShortStormer\n  Side = Men\n  KindOf = HERO INFANTRY SELECTABLE\n  CommandSet = StormSet\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = ArrowStormUpdate Tag_Storm\n    SpecialPowerTemplate = SpecialAbilityVolley\n    \
    WeaponTemplate = StormArrow\n    TargetRadius = 100\n    ShotsPerTarget = 1\n    \
    ShotsPerBurst = 1\n    MaxShots = 6\n    PreparationTime = 500\n    PersistentPrepTime = 1000\n    \
    IgnoreFacingCheck = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object StormRunner\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 400\n  End\n  \
  LocomotorSet\n    Locomotor = StormFeet\n    Condition = SET_NORMAL\n    Speed = 120\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object StormEnemy\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 400\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object StormAlly\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 400\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Mender\n  Side = Men\n  KindOf = HERO INFANTRY SELECTABLE\n  CommandSet = MenderSet\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rev\n    SpecialPowerTemplate = SpecialAbilityRevenge\n    \
    Duration = 3000\n    AttributeModifier = Calm\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Grace\n    SpecialPowerTemplate = SpecialAbilityGrace\n    \
    Duration = 3000\n    AttributeModifier = Calm\n  End\n  \
  Behavior = SpecialPowerTimerRefreshSpecialPower Tag_Restore\n    \
    SpecialPowerTemplate = SpecialAbilityRestore\n    AttributeModifierRange = 150\n    \
    AttributeModifierAffects = ALL -Mender ALLIES\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const ROOTS: [&str; 13] = [
    "SlowStormer",
    "ShortStormer",
    "StormRunner",
    "Stormer",
    "StormEnemy",
    "StormAlly",
    "Fallen",
    "Holder",
    "RivalHero",
    "RivalGrunt",
    "Cursor",
    "Mender",
    "Holder",
];

fn scene(placed: &[(&str, f32, f32, PlayerId)]) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (game, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots: Vec<String> = ROOTS.iter().map(|name| (*name).to_owned()).collect();
    let (kinds, missing) = KindTable::closure(&game, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&game);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&game.special_powers);
    let power_rules = power_names.cast_rules(&game.special_powers);
    let spawns = placed
        .iter()
        .map(|(name, x, y, owner)| {
            kinds
                .spawn(
                    &game.combat,
                    kinds.kind(name).unwrap(),
                    Vec2::new(*x, *y),
                    Facing::EAST,
                    *owner,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_combat(game.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&game), Arc::clone(&game.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_power_cast_rules(power_rules);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    simulation.tick().unwrap();
    (simulation, ids, entities)
}

fn select(simulation: &mut Simulation<RotwkRules>, player: PlayerId, unit: Entity) {
    simulation
        .submit(
            player,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
}

fn cast(
    simulation: &mut Simulation<RotwkRules>,
    player: PlayerId,
    caster: Entity,
    button: u8,
    at: Option<(f32, f32)>,
) {
    select(simulation, player, caster);
    simulation
        .submit(
            player,
            RotwkCommand::FireSpecialPower {
                button,
                target: None,
                at,
            },
        )
        .unwrap();
    for _ in 0..3 {
        simulation.tick().unwrap();
    }
}

fn ready(simulation: &Simulation<RotwkRules>, ids: Ids, holder: Entity, power: &str) -> bool {
    simulation
        .rules()
        .power_is_ready(simulation.state(), ids, simulation.frame(), holder, power)
}

#[test]
fn a_heros_death_readies_the_power_on_its_players_holders_only() {
    let (mut simulation, ids, e) = scene(&[
        ("Fallen", 100.0, 100.0, PLAYER),
        ("Holder", 150.0, 100.0, PLAYER),
        ("Holder", 150.0, 200.0, OTHER),
    ]);
    let (fallen, mine, theirs) = (e[0], e[1], e[2]);
    cast(&mut simulation, PLAYER, mine, 0, None);
    cast(&mut simulation, OTHER, theirs, 0, None);
    assert!(!ready(&simulation, ids, mine, "SpecialAbilityRevenge"));
    assert!(!ready(&simulation, ids, theirs, "SpecialAbilityRevenge"));
    select(&mut simulation, PLAYER, fallen);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Debug(DebugCommand::Damage { percent: 100 }),
        )
        .unwrap();
    for _ in 0..5 {
        simulation.tick().unwrap();
    }
    assert!(ready(&simulation, ids, mine, "SpecialAbilityRevenge"));
    assert!(!ready(&simulation, ids, theirs, "SpecialAbilityRevenge"));
}

#[test]
fn a_point_curse_reaches_enemy_heroes_within_the_radius_only() {
    let (mut simulation, ids, e) = scene(&[
        ("Cursor", 300.0, 100.0, PLAYER),
        ("RivalHero", 450.0, 300.0, OTHER),
        ("RivalHero", 500.0, 300.0, OTHER),
        ("RivalGrunt", 400.0, 300.0, OTHER),
    ]);
    let (cursor, near, far, grunt) = (e[0], e[1], e[2], e[3]);
    assert!(ready(&simulation, ids, near, "SpecialAbilityRevenge"));
    cast(&mut simulation, PLAYER, cursor, 0, Some((300.0, 300.0)));
    for _ in 0..5 {
        simulation.tick().unwrap();
    }
    assert!(!ready(&simulation, ids, near, "SpecialAbilityRevenge"));
    assert!(ready(&simulation, ids, far, "SpecialAbilityRevenge"));
    assert!(ready(&simulation, ids, grunt, "SpecialAbilityRevenge"));
}

#[test]
fn a_timer_refresh_readies_allies_and_not_its_caster() {
    let (mut simulation, ids, e) = scene(&[
        ("Mender", 300.0, 300.0, PLAYER),
        ("Holder", 340.0, 300.0, PLAYER),
    ]);
    let (mender, ally) = (e[0], e[1]);
    cast(&mut simulation, PLAYER, mender, 0, None);
    cast(&mut simulation, PLAYER, ally, 0, None);
    assert!(!ready(&simulation, ids, mender, "SpecialAbilityRevenge"));
    assert!(!ready(&simulation, ids, ally, "SpecialAbilityRevenge"));
    cast(&mut simulation, PLAYER, mender, 2, None);
    assert!(ready(&simulation, ids, ally, "SpecialAbilityRevenge"));
    assert!(!ready(&simulation, ids, mender, "SpecialAbilityRevenge"));
}

#[test]
fn an_arrow_storm_shoots_enemies_and_spares_allies() {
    let (mut simulation, ids, e) = scene(&[
        ("Stormer", 100.0, 100.0, PLAYER),
        ("StormEnemy", 320.0, 300.0, OTHER),
        ("StormAlly", 300.0, 320.0, PLAYER),
    ]);
    let (stormer, enemy, ally) = (e[0], e[1], e[2]);
    cast(&mut simulation, PLAYER, stormer, 0, Some((300.0, 300.0)));
    for _ in 0..90 {
        simulation.tick().unwrap();
    }
    let health = |entity: Entity| {
        simulation
            .state()
            .get(ids.health, entity)
            .map(|health| (health.0.current(), health.0.max()))
            .unwrap()
    };
    let (enemy_now, enemy_max) = health(enemy);
    let (ally_now, ally_max) = health(ally);
    assert!(enemy_now < enemy_max, "the enemy is shot");
    assert_eq!(ally_now, ally_max, "the ally is not");
}

fn health_of(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> (f32, f32) {
    simulation
        .state()
        .get(ids.health, entity)
        .map(|health| (health.0.current(), health.0.max()))
        .unwrap()
}

#[test]
fn an_arrow_storm_lists_its_targets_when_preparation_starts() {
    let (mut simulation, ids, e) = scene(&[
        ("SlowStormer", 100.0, 100.0, PLAYER),
        ("StormRunner", 300.0, 300.0, OTHER),
        ("StormRunner", 460.0, 300.0, OTHER),
    ]);
    let (stormer, leaver, arriver) = (e[0], e[1], e[2]);
    cast(&mut simulation, PLAYER, stormer, 0, Some((300.0, 300.0)));
    for (runner, to) in [(leaver, (300.0, 900.0)), (arriver, (300.0, 300.0))] {
        select(&mut simulation, OTHER, runner);
        simulation
            .submit(
                OTHER,
                RotwkCommand::MoveTo {
                    x: to.0,
                    y: to.1,
                    queue: false,
                },
            )
            .unwrap();
    }
    for _ in 0..140 {
        simulation.tick().unwrap();
    }
    let (now, max) = health_of(&simulation, ids, leaver);
    assert!(
        now < max,
        "the one in the area at the preparation start is shot"
    );
    let (now, max) = health_of(&simulation, ids, arriver);
    assert_eq!(now, max, "the one that walked in later is not");
}

#[test]
fn an_arrow_storm_with_nothing_to_shoot_ends_at_its_first_trigger() {
    let (mut simulation, ids, e) = scene(&[("ShortStormer", 100.0, 100.0, PLAYER)]);
    let stormer = e[0];
    cast(&mut simulation, PLAYER, stormer, 0, Some((300.0, 300.0)));
    assert!(
        simulation.state().contains(ids.arrow_storm_hits, stormer),
        "the storm is being prepared"
    );
    for _ in 0..90 {
        simulation.tick().unwrap();
    }
    assert!(
        !simulation.state().contains(ids.arrow_storm_hits, stormer),
        "the cast is still running its bursts"
    );
}
