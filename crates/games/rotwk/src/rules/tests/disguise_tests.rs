use super::*;

const TOGGLE_DATA: &str = "\
SpecialPower DisguisePower\n  ReloadTime = 10\nEnd\n\
SpecialPower MountPower\n  ReloadTime = 10\nEnd\n\
CommandSet HeroSet\n  1 = Fire_Disguise\n  2 = Fire_Mount\nEnd\n\
CommandButton Fire_Disguise\n  Command = SPECIAL_POWER\n  SpecialPower = DisguisePower\nEnd\n\
CommandButton Fire_Mount\n  Command = SPECIAL_POWER\n  SpecialPower = MountPower\nEnd\n\
Object Hero\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = HeroSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = SpecialDisguiseUpdate Tag_Disguise\n    SpecialPowerTemplate = DisguisePower\n    \
    UnpackTime = 0\n    PreparationTime = 0\n    PackTime = 0\n    DisguiseFX = FX_Disguise\n  End\n  \
  Behavior = ToggleMountedSpecialAbilityUpdate Tag_Mount\n    SpecialPowerTemplate = MountPower\n    \
    UnpackTime = 0\n    PreparationTime = 0\n    PackTime = 0\n    \
    CancelDisguiseWhenDismounting = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Weapon Sword\n  AttackRange = 20\n  DelayBetweenShots = 1000\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 10\n    DamageType = SLASH\n  End\nEnd\n\
Object Other\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Sword\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    \
    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

struct Match {
    data: GameData,
    kinds: Arc<KindTable>,
}

fn load(data: &str) -> Match {
    load_seeded(data, &["Hero", "Other"])
}

fn load_seeded(data: &str, seeds: &[&str]) -> Match {
    let doc =
        mint_sage_ini::Document::parse("t.ini", data, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = seeds.iter().map(|name| (*name).to_owned()).collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    Match {
        data,
        kinds: Arc::new(kinds),
    }
}

fn simulation(m: &Match) -> Simulation<RotwkRules> {
    let production = m.kinds.production(&m.data);
    let power_names = crate::powers::PowerNames::build(m.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&m.data.special_powers);
    let spawns: Vec<Spawn> = [
        ("Hero", 100.0, 100.0, PLAYER),
        ("Other", 104.0, 100.0, OTHER),
    ]
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
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(m.data.combat.clone())
        .with_construction(Arc::clone(&m.kinds), production.clone())
        .with_upgrades(m.kinds.upgrades(&m.data), Arc::clone(&m.data.upgrade_bits))
        .with_powers(power_names, power_reload);
    Simulation::new(5, rules).unwrap()
}

fn only_owned_by(simulation: &Simulation<RotwkRules>, ids: Ids, owner: PlayerId) -> Entity {
    let mut found: Vec<Entity> = simulation
        .state()
        .store(ids.owner)
        .unwrap()
        .iter()
        .filter(|(_, o)| o.0 == owner)
        .map(|(entity, _)| entity)
        .collect();
    found.sort_unstable_by_key(|entity| entity.index());
    found[0]
}

fn holds(simulation: &Simulation<RotwkRules>, entity: Entity, flag: &str) -> bool {
    simulation
        .rules()
        .modules
        .as_ref()
        .and_then(|wiring| simulation.state().get(wiring.state, entity))
        .is_some_and(|state| {
            state
                .model_condition_names()
                .iter()
                .any(|held| held.eq_ignore_ascii_case(flag))
        })
}

fn press(simulation: &mut Simulation<RotwkRules>, caster: Entity, button: u8) {
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
                button,
                target: None,
                at: None,
            },
        )
        .unwrap();
}

fn fx_names_played(simulation: &mut Simulation<RotwkRules>, ticks: u32) -> Vec<String> {
    let mut seen = Vec::new();
    for _ in 0..ticks {
        let frame = simulation.tick().unwrap().frame;
        for event in simulation.rules().fx_events(simulation.state(), frame) {
            if let Some(name) = simulation.rules().fx_name(event.list) {
                seen.push(name.to_owned());
            }
        }
    }
    seen
}

#[test]
fn the_power_sets_then_clears_disguised_with_no_fx() {
    let m = load(TOGGLE_DATA);
    let mut simulation = simulation(&m);
    simulation.tick().unwrap();
    let ids = simulation.rules().ids().unwrap();
    let hero = only_owned_by(&simulation, ids, PLAYER);
    press(&mut simulation, hero, 0);
    let fx = fx_names_played(&mut simulation, 5);
    assert!(holds(&simulation, hero, "DISGUISED"), "first press sets it");
    assert!(fx.is_empty(), "no FX on disguising: {fx:?}");
    press(&mut simulation, hero, 0);
    let fx = fx_names_played(&mut simulation, 5);
    assert!(
        !holds(&simulation, hero, "DISGUISED"),
        "second press clears it"
    );
    assert!(fx.is_empty(), "no FX on a re-trigger: {fx:?}");
}

#[test]
fn dismounting_with_the_cancel_flag_breaks_the_disguise_with_no_fx() {
    let m = load(TOGGLE_DATA);
    let mut simulation = simulation(&m);
    simulation.tick().unwrap();
    let ids = simulation.rules().ids().unwrap();
    let hero = only_owned_by(&simulation, ids, PLAYER);
    press(&mut simulation, hero, 0);
    fx_names_played(&mut simulation, 5);
    assert!(holds(&simulation, hero, "DISGUISED"));
    press(&mut simulation, hero, 1);
    fx_names_played(&mut simulation, 5);
    assert!(
        holds(&simulation, hero, "MOUNTED"),
        "fixture premise: mounted"
    );
    press(&mut simulation, hero, 1);
    let fx = fx_names_played(&mut simulation, 5);
    assert!(
        !holds(&simulation, hero, "DISGUISED"),
        "the dismount must have broken the disguise"
    );
    assert!(fx.is_empty(), "no FX on a dismount break: {fx:?}");
}

#[test]
fn dismounting_without_the_cancel_flag_leaves_the_disguise_alone() {
    let data = TOGGLE_DATA.replace("    CancelDisguiseWhenDismounting = Yes\n", "");
    let m = load(&data);
    let mut simulation = simulation(&m);
    simulation.tick().unwrap();
    let ids = simulation.rules().ids().unwrap();
    let hero = only_owned_by(&simulation, ids, PLAYER);
    press(&mut simulation, hero, 0);
    fx_names_played(&mut simulation, 5);
    press(&mut simulation, hero, 1);
    fx_names_played(&mut simulation, 5);
    press(&mut simulation, hero, 1);
    fx_names_played(&mut simulation, 5);
    assert!(
        holds(&simulation, hero, "DISGUISED"),
        "no CancelDisguiseWhenDismounting - the dismount must not break it"
    );
}

const DETECT_DATA: &str = "\
SpecialPower DisguisePower\n  ReloadTime = 10\nEnd\n\
CommandSet HeroSet\n  1 = Fire_Disguise\nEnd\n\
CommandButton Fire_Disguise\n  Command = SPECIAL_POWER\n  SpecialPower = DisguisePower\nEnd\n\
Object Hero\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = HeroSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = SpecialDisguiseUpdate Tag_Disguise\n    SpecialPowerTemplate = DisguisePower\n    \
    UnpackTime = 0\n    PreparationTime = 0\n    PackTime = 0\n    DisguiseFX = FX_Disguise\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Detector\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = StealthDetectorUpdate Tag_Detect\n    DetectionRate = 100\n    \
    DetectionRange = 500\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn detect_simulation(same_team: bool) -> (Simulation<RotwkRules>, Ids, Entity) {
    let m = load_seeded(DETECT_DATA, &["Hero", "Detector"]);
    let production = m.kinds.production(&m.data);
    let power_names = crate::powers::PowerNames::build(m.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&m.data.special_powers);
    let spawns: Vec<Spawn> = [
        ("Hero", 100.0, 100.0, PLAYER),
        ("Detector", 110.0, 100.0, OTHER),
    ]
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
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let mut rules = RotwkRules::new(passability, spawns)
        .with_combat(m.data.combat.clone())
        .with_construction(Arc::clone(&m.kinds), production.clone())
        .with_upgrades(m.kinds.upgrades(&m.data), Arc::clone(&m.data.upgrade_bits))
        .with_powers(power_names, power_reload);
    if same_team {
        rules = rules.with_teams(vec![Some(0), Some(0)]);
    }
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    simulation.tick().unwrap();
    let hero = only_owned_by(&simulation, ids, PLAYER);
    press(&mut simulation, hero, 0);
    (simulation, ids, hero)
}

#[test]
fn an_enemy_detector_breaks_the_disguise() {
    let (mut simulation, _ids, hero) = detect_simulation(false);
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert!(
        !holds(&simulation, hero, "DISGUISED"),
        "an enemy detector in range must have revealed it"
    );
}

#[test]
fn an_allied_detector_does_not_break_the_disguise() {
    let (mut simulation, _ids, hero) = detect_simulation(true);
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert!(
        holds(&simulation, hero, "DISGUISED"),
        "an allied detector must not reveal it"
    );
}

#[test]
fn taking_damage_does_not_break_the_disguise() {
    let m = load(TOGGLE_DATA);
    let mut simulation = simulation(&m);
    simulation.tick().unwrap();
    let ids = simulation.rules().ids().unwrap();
    let hero = only_owned_by(&simulation, ids, PLAYER);
    press(&mut simulation, hero, 0);
    fx_names_played(&mut simulation, 5);
    assert!(holds(&simulation, hero, "DISGUISED"), "fixture premise");
    let before = simulation
        .state()
        .get(ids.health, hero)
        .map_or(1.0, |health| health.0.fraction());
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    let after = simulation
        .state()
        .get(ids.health, hero)
        .map_or(1.0, |health| health.0.fraction());
    assert!(
        after < before,
        "fixture premise: the attacker must have landed a hit"
    );
    assert!(
        holds(&simulation, hero, "DISGUISED"),
        "taking damage must not have broken the disguise"
    );
}

const KIND_DATA: &str = "\
SpecialPower DisguisePower\n  ReloadTime = 10\nEnd\n\
CommandSet HeroSet\n  1 = Fire_Disguise\nEnd\n\
CommandButton Fire_Disguise\n  Command = SPECIAL_POWER\n  SpecialPower = DisguisePower\nEnd\n\
Object Hero\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = HeroSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = SpecialDisguiseUpdate Tag_Disguise\n    SpecialPowerTemplate = DisguisePower\n    \
    UnpackTime = 0\n    PreparationTime = 0\n    PackTime = 0\n    DisguiseFX = FX_Disguise\n    \
    DisguiseAsTemplate = CavalryChild\n    \
    DisguisedAsTemplate_EnemyPerspective = CavalryHorde\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object CavalryChild\n  Side = Men\n  KindOf = SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object CavalryHorde\n  Side = Men\n  KindOf = SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Other\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const ALLY: PlayerId = PlayerId::new(2);

#[test]
fn disguise_kinds_differ_by_viewer() {
    let m = load_seeded(
        KIND_DATA,
        &["Hero", "Other", "CavalryChild", "CavalryHorde"],
    );
    let production = m.kinds.production(&m.data);
    let power_names = crate::powers::PowerNames::build(m.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&m.data.special_powers);
    let spawns: Vec<Spawn> = [
        ("Hero", 100.0, 100.0, PLAYER),
        ("Other", 104.0, 100.0, OTHER),
    ]
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
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(m.data.combat.clone())
        .with_construction(Arc::clone(&m.kinds), production.clone())
        .with_upgrades(m.kinds.upgrades(&m.data), Arc::clone(&m.data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_teams(vec![Some(0), Some(1), Some(0)]);
    let mut simulation = Simulation::new(5, rules).unwrap();
    simulation.tick().unwrap();
    let ids = simulation.rules().ids().unwrap();
    let hero = only_owned_by(&simulation, ids, PLAYER);
    press(&mut simulation, hero, 0);
    fx_names_played(&mut simulation, 5);
    assert!(holds(&simulation, hero, "DISGUISED"), "fixture premise");

    let true_kind = m.kinds.kind("Hero").unwrap();
    let art_kind = m.kinds.kind("CavalryChild").unwrap();
    let told_kind = m.kinds.kind("CavalryHorde").unwrap();

    let world = simulation.state();
    let rules = simulation.rules();
    assert_eq!(
        rules.art_kind(world, PLAYER, hero),
        Some(true_kind),
        "owner's own art"
    );
    assert_eq!(
        rules.art_kind(world, ALLY, hero),
        Some(art_kind),
        "ally sees the disguise"
    );
    assert_eq!(
        rules.art_kind(world, OTHER, hero),
        Some(art_kind),
        "enemy sees the disguise"
    );

    assert_eq!(
        rules.told_kind(world, PLAYER, hero),
        Some(true_kind),
        "owner's own portrait"
    );
    assert_eq!(
        rules.told_kind(world, ALLY, hero),
        Some(true_kind),
        "ally's portrait is the true one"
    );
    assert_eq!(
        rules.told_kind(world, OTHER, hero),
        Some(told_kind),
        "enemy's portrait is the swap"
    );

    assert_eq!(
        rules.hover_kind(world, PLAYER, hero),
        Some(true_kind),
        "owner's own hover name"
    );
    assert_eq!(
        rules.hover_kind(world, ALLY, hero),
        Some(true_kind),
        "ally's hover name is the true one"
    );
    assert_eq!(
        rules.hover_kind(world, OTHER, hero),
        Some(art_kind),
        "enemy's hover name is the swap"
    );
}
