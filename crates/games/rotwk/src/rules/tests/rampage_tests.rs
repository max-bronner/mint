use super::*;

const EVENTS: &str = "<SageLuaScriptSection>\n\
                      <Events><InternalEvent Name=\"ReallyDamaged\" /></Events>\n\
                      <EventList Name=\"TestRampageFunctions\">\n\
                      <EventHandler EventName=\"ReallyDamaged\" ScriptFunctionName=\"GoIntoRampage\"/>\n\
                      </EventList>\n\
                      </SageLuaScriptSection>";

const HANDLERS: &str = "function GoIntoRampage(self)\n  ObjectEnterRampageState(self)\nend\n";

const DATA: &str = "\
Locomotor ZTrollLoco\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 100\nEnd\n\
Weapon ZArrow\n  AttackRange = 300\n  DelayBetweenShots = 600000\n  WeaponSpeed = 999999\n  \
  DamageNugget\n    Damage = 900\n    DamageType = PIERCE\n    DeathType = NORMAL\n  End\nEnd\n\
Object ZArcher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZArrow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  VisionRange = 400\n  ShroudClearingRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZTroll\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n    MaxHealthReallyDamaged = 500\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AILuaEventsList = TestRampageFunctions\n    \
    RampageTime = 4000\n  End\n  \
  LocomotorSet\n    Locomotor = ZTrollLoco\n    Condition = SET_NORMAL\n    Speed = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn scene(ticks: u32) -> (Simulation<RotwkRules>, Ids, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZArcher", "ZTroll"];
    let (kinds, missing) = KindTable::closure(
        &data,
        &roots
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    );
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let placed = [
        ("ZArcher", 400.0, 400.0, PLAYER),
        ("ZTroll", 550.0, 400.0, OTHER),
    ];
    let spawns: Vec<_> = placed
        .iter()
        .map(|(name, x, y, owner)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
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
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_object_scripts(Some(
            mint_sage_modules::object_scripts::ObjectScripts::load(EVENTS, HANDLERS).unwrap(),
        ));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let troll = {
        let kind = kinds.kind("ZTroll").unwrap();
        let world = simulation.state();
        world
            .store(ids.kind)
            .unwrap()
            .iter()
            .find(|(_, k)| **k == kind)
            .map(|(entity, _)| entity)
            .unwrap()
    };
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    (simulation, ids, troll)
}

#[test]
fn a_really_damaging_blow_enters_rampage() {
    let (simulation, ids, troll) = scene(40);
    let world = simulation.state();
    assert!(
        world.contains(ids.rampaging, troll),
        "the troll should be rampaging after the arrow lands"
    );
    assert!(
        !world.contains(ids.selected, troll),
        "a rampaging body is deselected"
    );
    let health = world.get(ids.health, troll).unwrap();
    assert!(
        health.0.current() > 0.0,
        "the arrow alone must not have killed it"
    );
}

const SELF_FIRE_DATA: &str = "\
Weapon ZArrow\n  AttackRange = 300\n  DelayBetweenShots = 600000\n  WeaponSpeed = 999999\n  \
  DamageNugget\n    Damage = 900\n    DamageType = PIERCE\n    DeathType = NORMAL\n  End\nEnd\n\
Weapon ZSelfSlam\n  AttackRange = 50\n  DelayBetweenShots = 200\n  \
  RadiusDamageAffects = SELF ALLIES\n  \
  DamageNugget\n    Damage = 20\n    Radius = 10\n    DamageType = CRUSH\n    \
    DeathType = NORMAL\n  End\nEnd\n\
Object ZArcher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZArrow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  VisionRange = 400\n  ShroudClearingRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZSlammer\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n    MaxHealthReallyDamaged = 500\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZSelfSlam\n  End\n  \
  WeaponSet\n    Conditions = RAMPAGE\n    Weapon = PRIMARY ZSelfSlam\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AILuaEventsList = TestRampageFunctions\n    \
    RampageTime = 20000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn self_fire_scene(ticks: u32) -> (Simulation<RotwkRules>, Ids, Entity) {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        SELF_FIRE_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZArcher", "ZSlammer"];
    let (kinds, missing) = KindTable::closure(
        &data,
        &roots
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    );
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let placed = [
        ("ZArcher", 400.0, 400.0, PLAYER),
        ("ZSlammer", 550.0, 400.0, OTHER),
    ];
    let spawns: Vec<_> = placed
        .iter()
        .map(|(name, x, y, owner)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
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
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_object_scripts(Some(
            mint_sage_modules::object_scripts::ObjectScripts::load(EVENTS, HANDLERS).unwrap(),
        ));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let slammer = {
        let kind = kinds.kind("ZSlammer").unwrap();
        let world = simulation.state();
        world
            .store(ids.kind)
            .unwrap()
            .iter()
            .find(|(_, k)| **k == kind)
            .map(|(entity, _)| entity)
            .unwrap()
    };
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    (simulation, ids, slammer)
}

#[test]
fn a_rampaging_body_fires_its_own_weapon_at_itself() {
    let (simulation, ids, slammer) = self_fire_scene(60);
    let health = simulation.state().get(ids.health, slammer).unwrap();
    assert!(
        health.0.current() < 80.0,
        "self-fire should have landed at least once more on top of the arrow's 900: {}",
        health.0.current()
    );
}

const DUAL_DATA: &str = "\
Locomotor ZMoverLoco\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 100\nEnd\n\
Weapon Harpoon\n  AttackRange = 300\n  DelayBetweenShots = 600000\n  WeaponSpeed = 999999\n  \
  DamageNugget\n    Damage = 900\n    DamageType = PIERCE\n    DeathType = NORMAL\n  End\nEnd\n\
Object ZSentry\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n    MaxHealthReallyDamaged = 500\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Harpoon\n  End\n  \
  WeaponSet\n    Conditions = CLOSE_RANGE\n    Weapon = PRIMARY Harpoon\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n    \
    MoodAttackCheckRate = 100\n    AILuaEventsList = TestRampageFunctions\n    \
    RampageTime = 20000\n  End\n  \
  Behavior = DualWeaponBehavior Tag_Dual\n    SwitchWeaponOnCloseRangeDistance = 150\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZAttacker\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Harpoon\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  VisionRange = 400\n  ShroudClearingRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZMover\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  LocomotorSet\n    Locomotor = ZMoverLoco\n    Condition = SET_NORMAL\n    Speed = 200\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn dual_scene(ticks: u32) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        DUAL_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZSentry", "ZAttacker", "ZMover"];
    let (kinds, missing) = KindTable::closure(
        &data,
        &roots
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    );
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let placed = [
        ("ZSentry", 400.0, 400.0, PLAYER),
        ("ZAttacker", 600.0, 400.0, OTHER),
        ("ZMover", 430.0, 400.0, OTHER),
    ];
    let spawns: Vec<_> = placed
        .iter()
        .map(|(name, x, y, owner)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
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
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_object_scripts(Some(
            mint_sage_modules::object_scripts::ObjectScripts::load(EVENTS, HANDLERS).unwrap(),
        ));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let find = |name: &str| {
        let kind = kinds.kind(name).unwrap();
        let world = simulation.state();
        world
            .store(ids.kind)
            .unwrap()
            .iter()
            .find(|(_, k)| **k == kind)
            .map(|(entity, _)| entity)
            .unwrap()
    };
    let (sentry, mover) = (find("ZSentry"), find("ZMover"));
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    (simulation, ids, sentry, mover)
}

#[test]
fn a_rampaging_sentry_keeps_its_close_range_flag_after_the_target_leaves() {
    let (mut simulation, ids, sentry, mover) = dual_scene(20);
    assert!(
        simulation.state().contains(ids.rampaging, sentry),
        "the fixture's premise: ZAttacker's blow should have started rampage by tick 20"
    );
    let flagged = |sim: &Simulation<RotwkRules>| {
        sim.rules()
            .upgrade_state_id()
            .and_then(|id| sim.state().get(id, sentry).cloned())
            .is_some_and(|state| state.has_weapon_set_flag("CLOSE_RANGE"))
    };
    assert!(
        flagged(&simulation),
        "ZMover started within SwitchWeaponOnCloseRangeDistance"
    );
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![mover],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    simulation
        .submit(
            OTHER,
            RotwkCommand::MoveTo {
                x: 850.0,
                y: 400.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..200 {
        simulation.tick().unwrap();
    }
    let moved_away = simulation
        .state()
        .get(ids.body, mover)
        .is_some_and(|body| body.0.position.x > 700.0);
    assert!(moved_away, "ZMover should have walked well clear by now");
    assert!(
        flagged(&simulation),
        "RAMPAGING freezes the dual-weapon update, so CLOSE_RANGE must stay raised"
    );
}

#[test]
fn rampage_always_ends_in_death_after_rampage_time() {
    let (simulation, ids, troll) = scene(60);
    assert!(
        !simulation.state().contains(ids.doomed, troll),
        "RampageTime (4000 ms, 120 ticks) has not elapsed yet at tick 60"
    );
    let (simulation, ids, troll) = scene(140);
    assert!(
        simulation.state().contains(ids.doomed, troll),
        "RampageTime has elapsed by tick 140"
    );
}

const FEAR_DATA: &str = "\
Locomotor ZTrollLoco\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 100\nEnd\n\
Weapon ZArrow\n  AttackRange = 300\n  DelayBetweenShots = 600000\n  WeaponSpeed = 999999\n  \
  DamageNugget\n    Damage = 900\n    DamageType = PIERCE\n    DeathType = NORMAL\n  End\nEnd\n\
EmotionNugget ZFearOfArcher\n  Type = FEAR\n  AIState = BACK_AWAY\n  \
  ModelConditions = EMOTION_AFRAID\n  InactiveDurationSameObject = 10000\nEnd\n\
Object ZArcher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE SCARY\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZArrow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  VisionRange = 400\n  ShroudClearingRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZTroll\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n    MaxHealthReallyDamaged = 500\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AILuaEventsList = TestRampageFunctions\n    \
    RampageTime = 20000\n  End\n  \
  Behavior = EmotionTrackerUpdate Tag_Emotion\n    TauntAndPointUpdateDelay = 100\n    \
    FearScanDistance = 400\n    AfraidOf = NONE +ZArcher\n    AddEmotion = ZFearOfArcher\n  End\n  \
  LocomotorSet\n    Locomotor = ZTrollLoco\n    Condition = SET_NORMAL\n    Speed = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn fear_scene(ticks: u32) -> (Simulation<RotwkRules>, Ids, Entity) {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        FEAR_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let errors = mint_sage_ini::ValueErrors::new();
    let mut emotions = mint_sage_ini::EmotionIndex::new();
    emotions.insert_document(&doc, &errors);
    assert!(errors.is_empty());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZArcher", "ZTroll"];
    let (kinds, missing) = KindTable::closure(
        &data,
        &roots
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    );
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let placed = [
        ("ZArcher", 400.0, 400.0, PLAYER),
        ("ZTroll", 550.0, 400.0, OTHER),
    ];
    let spawns: Vec<_> = placed
        .iter()
        .map(|(name, x, y, owner)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
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
        .with_combat(data.combat.clone())
        .with_emotions(Arc::new(emotions))
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_object_scripts(Some(
            mint_sage_modules::object_scripts::ObjectScripts::load(EVENTS, HANDLERS).unwrap(),
        ));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let troll = {
        let kind = kinds.kind("ZTroll").unwrap();
        let world = simulation.state();
        world
            .store(ids.kind)
            .unwrap()
            .iter()
            .find(|(_, k)| **k == kind)
            .map(|(entity, _)| entity)
            .unwrap()
    };
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    (simulation, ids, troll)
}

#[test]
fn a_rampaging_troll_never_starts_a_fear_nugget() {
    let (simulation, ids, troll) = fear_scene(40);
    assert!(
        simulation.state().contains(ids.rampaging, troll),
        "the fixture's premise: the troll must be rampaging by tick 40"
    );
    assert!(
        simulation.state().get(ids.emotion, troll).is_none(),
        "a rampaging troll must not start the FEAR nugget"
    );
}
