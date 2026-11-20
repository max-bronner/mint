use super::ability_b1_tests::{run, scene};
use super::*;
use crate::rules::debug::DebugCommand;

const AT: Vec2 = Vec2::new(300.0, 300.0);

const THIRD: PlayerId = PlayerId::new(2);

fn at(dx: f32, dy: f32) -> Vec2 {
    Vec2::new(AT.x + dx, AT.y + dy)
}

fn healer(name: &str, extra: &str) -> String {
    healer_with(name, extra, "")
}

fn healer_with(name: &str, extra: &str, tail: &str) -> String {
    format!(
        "Object {name}\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
         Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
         Behavior = AutoHealBehavior Tag_Heal\n    StartsActive = Yes\n    HealingAmount = 10\n    \
         HealingDelay = 1000\n    Radius = 100\n    KindOf = INFANTRY\n    {extra}\n  End\n  \
         {tail}\n  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n"
    )
}

const HEAL_DATA: &str = "\
Armor Plain\n  Armor = DEFAULT 100%\nEnd\n\
Weapon Nibble\n  AttackRange = 200\n  DelayBetweenShots = 300\n  \
  DamageNugget\n    Damage = 1\n    DamageType = SLASH\n  End\nEnd\n\
Upgrade Upgrade_Tonic\n  Type = OBJECT\n  BuildCost = 0\n  BuildTime = 1\nEnd\n\
ModifierList Tonic\n  Category = SPELL\n  Modifier = AUTO_HEAL 5.5\n  Duration = 0\nEnd\n\
ModifierList Draught\n  Category = SPELL\n  Modifier = AUTO_HEAL 50\n  Duration = 0\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Veteran\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = AutoHealBehavior Tag_Self\n    StartsActive = Yes\n    HealingAmount = 1\n    \
    HealingDelay = 400\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Dummy\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Pricker\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Nibble\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Rider\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n    InitialHealth = 50\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn data(extra: &[String]) -> String {
    let mut text = String::from(HEAL_DATA);
    for part in extra {
        text.push_str(part);
    }
    text
}

fn fighter(name: &str, extra: &str) -> String {
    healer_with(
        name,
        extra,
        "WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Nibble\n  End\n  \
         Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
         MoodAttackCheckRate = 100\n  End",
    )
}

fn container(name: &str, status: &str) -> String {
    format!(
        "Object {name}\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
         Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
         Behavior = TransportContain Tag_Contain\n    ObjectStatusOfContained = {status}\n    \
         Slots = 2\n    PassengerFilter = NONE +Rider\n    InitialPayload = Rider 2\n  End\n  \
         Behavior = AutoHealBehavior Tag_Heal\n    StartsActive = Yes\n    HealingAmount = 10\n    \
         HealingDelay = 1000\n    AffectsContained = Yes\n    UnitHealPulseFX = FX_Heal\n  End\n  \
         Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 40\nEnd\n"
    )
}

fn health(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.health, entity)
        .map_or(0.0, |pool| pool.0.current())
}

fn select_and(
    simulation: &mut Simulation<RotwkRules>,
    player: PlayerId,
    who: &[Entity],
    c: DebugCommand,
) {
    simulation
        .submit(
            player,
            RotwkCommand::SetSelection {
                replace: true,
                units: who.to_vec(),
            },
        )
        .unwrap();
    simulation.submit(player, RotwkCommand::Debug(c)).unwrap();
    simulation.tick().unwrap();
}

fn wound(simulation: &mut Simulation<RotwkRules>, who: &[Entity], percent: u8) {
    select_and(simulation, PLAYER, who, DebugCommand::Damage { percent });
}

fn kill(simulation: &mut Simulation<RotwkRules>, player: PlayerId, who: &[Entity]) {
    simulation
        .submit(
            player,
            RotwkCommand::SetSelection {
                replace: true,
                units: who.to_vec(),
            },
        )
        .unwrap();
    simulation.submit(player, RotwkCommand::DebugKill).unwrap();
    run(simulation, 2);
}

fn grant(simulation: &mut Simulation<RotwkRules>, who: &[Entity], upgrade: &str) {
    select_and(
        simulation,
        PLAYER,
        who,
        DebugCommand::GrantUpgrade {
            upgrade: upgrade.to_owned(),
        },
    );
}

fn rises(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    entity: Entity,
    ticks: u32,
) -> Vec<f32> {
    let mut found = Vec::new();
    let mut last = health(simulation, ids, entity);
    for _ in 0..ticks {
        simulation.tick().unwrap();
        let now = health(simulation, ids, entity);
        if now > last {
            found.push(now - last);
        }
        last = now;
    }
    found
}

fn played(simulation: &mut Simulation<RotwkRules>, name: &str, ticks: u32) -> usize {
    let mut count = 0;
    for _ in 0..ticks {
        let frame = simulation.tick().unwrap().frame;
        count += simulation
            .rules()
            .fx_events(simulation.state(), frame)
            .iter()
            .filter(|event| simulation.rules().fx_name(event.list) == Some(name))
            .count();
    }
    count
}

#[test]
fn a_healer_that_is_attacking_heals_nobody() {
    let text = data(&[fighter(
        "Warder",
        "HealOnlyIfNotInCombat = Yes\n    StartHealingDelay = 2000",
    )]);
    let (mut simulation, ids, found) = scene(
        &text,
        &[
            ("Warder", AT, PLAYER),
            ("Soldier", at(0.0, -80.0), PLAYER),
            ("Dummy", at(0.0, 180.0), OTHER),
        ],
    );
    let (soldier, dummy) = (found[1], found[2]);
    wound(&mut simulation, &[soldier], 50);
    run(&mut simulation, 200);
    assert_eq!(
        health(&simulation, ids, soldier),
        50.0,
        "fighting heals nobody"
    );
    kill(&mut simulation, OTHER, &[dummy]);
    run(&mut simulation, 150);
    assert!(health(&simulation, ids, soldier) > 50.0, "healing resumes");
}

#[test]
fn a_healer_that_is_being_hit_heals_nobody() {
    let text = data(&[healer("Mender", "HealOnlyIfNotUnderAttack = Yes")]);
    let (mut simulation, ids, found) = scene(
        &text,
        &[
            ("Mender", AT, PLAYER),
            ("Soldier", at(0.0, -80.0), PLAYER),
            ("Pricker", at(0.0, 180.0), OTHER),
        ],
    );
    let (mender, soldier, pricker) = (found[0], found[1], found[2]);
    run(&mut simulation, 60);
    wound(&mut simulation, &[soldier], 50);
    run(&mut simulation, 150);
    assert!(
        health(&simulation, ids, mender) < 100_000.0,
        "the healer is hit"
    );
    assert_eq!(health(&simulation, ids, soldier), 50.0);
    kill(&mut simulation, OTHER, &[pricker]);
    run(&mut simulation, 90);
    assert!(health(&simulation, ids, soldier) > 50.0, "healing resumes");
}

#[test]
fn a_nonstackable_healer_skips_a_target_healed_by_another_source() {
    let text = data(&[healer("Steady", "NonStackable = Yes")]);
    let (mut simulation, ids, found) = scene(
        &text,
        &[("Steady", AT, PLAYER), ("Veteran", at(0.0, 60.0), PLAYER)],
    );
    let veteran = found[1];
    wound(&mut simulation, &[veteran], 50);
    run(&mut simulation, 40);
    let found = rises(&mut simulation, ids, veteran, 120);
    assert!(found.len() >= 5, "its own pulses land: {found:?}");
    assert!(
        found.iter().all(|rise| *rise <= 1.0),
        "the healer's 10 never lands: {found:?}"
    );
}

#[test]
fn auto_heal_adds_to_the_healing_amount_truncated() {
    let text = data(&[healer(
        "Brewer",
        "\n  End\n  Behavior = AttributeModifierUpgrade Tag_Tonic\n    \
             TriggeredBy = Upgrade_Tonic\n    AttributeModifier = Tonic",
    )]);
    let (mut simulation, ids, found) = scene(
        &text,
        &[("Brewer", AT, PLAYER), ("Soldier", at(0.0, 80.0), PLAYER)],
    );
    let (brewer, soldier) = (found[0], found[1]);
    grant(&mut simulation, &[brewer], "Upgrade_Tonic");
    wound(&mut simulation, &[soldier], 80);
    let found = rises(&mut simulation, ids, soldier, 100);
    assert!(found.len() >= 3, "{found:?}");
    assert!(found.iter().take(3).all(|rise| *rise == 15.0), "{found:?}");
}

#[test]
fn auto_heal_without_the_module_heals_nothing() {
    let plain = "Object Cask\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
        Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
        Behavior = AttributeModifierUpgrade Tag_Draught\n    TriggeredBy = Upgrade_Tonic\n    \
        AttributeModifier = Draught\n  End\n  \
        Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";
    let text = data(&[plain.to_owned()]);
    let (mut simulation, ids, found) = scene(&text, &[("Cask", AT, PLAYER)]);
    let cask = found[0];
    grant(&mut simulation, &[cask], "Upgrade_Tonic");
    wound(&mut simulation, &[cask], 50);
    run(&mut simulation, 60);
    assert_eq!(health(&simulation, ids, cask), 50.0);
}

#[test]
fn a_garrison_heals_its_occupants_and_hides_the_fx_when_they_are_enclosed() {
    let text = data(&[
        container("Open", "UNSELECTABLE"),
        container("Sealed", "UNSELECTABLE ENCLOSED"),
    ]);
    let count = |name: &str| {
        let (mut simulation, ids, found) = scene(
            &text,
            &[(name, AT, PLAYER), ("Rider", at(600.0, 0.0), OTHER)],
        );
        run(&mut simulation, 5);
        let held: Vec<Entity> = simulation
            .state()
            .store(ids.passengers)
            .and_then(|store| store.get(found[0]))
            .map(|list| list.0.clone())
            .unwrap_or_default();
        assert_eq!(held.len(), 2, "the payload filled");
        let fx = played(&mut simulation, "FX_Heal", 70);
        let healed = held
            .iter()
            .all(|entity| health(&simulation, ids, *entity) > 50.0);
        (healed, fx)
    };
    let (open_healed, open_fx) = count("Open");
    assert!(open_healed, "the occupants gain health");
    assert!(open_fx >= 2, "an FX per healed occupant: {open_fx}");
    let (sealed_healed, sealed_fx) = count("Sealed");
    assert!(sealed_healed, "enclosed occupants heal too");
    assert_eq!(sealed_fx, 0, "but no FX shows");
}

const HORDE_DATA: &str = "\
Locomotor HordeFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 100\n  MaxTurnWithoutReform = 45\nEnd\n\
Object Barracks\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = BarracksSet\n  BuildCost = 100\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  GeometryHeight = 30\nEnd\n\
CommandSet BarracksSet\n  1 = Build_PlainHorde\n  2 = Build_EliteHorde\n  \
  3 = Build_ArmedHorde\n  4 = Build_HealBannerHorde\n  5 = Build_SacrificerHorde\n  \
  6 = Build_VictimHorde\nEnd\n\
CommandButton Build_SacrificerHorde\n  Command = UNIT_BUILD\n  Object = SacrificerHorde\nEnd\n\
CommandButton Build_VictimHorde\n  Command = UNIT_BUILD\n  Object = VictimHorde\nEnd\n\
Weapon Cleaver\n  AttackRange = 200\n  DelayBetweenShots = 3000\n  \
  DamageNugget\n    Damage = 1\n    DamageType = SLASH\n  End\nEnd\n\
Object Cutter\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Cleaver\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Meat\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object SacrificerHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 100\n  \
  BuildTime = 1\n  CommandPoints = 5\n  VisionRange = 400\n  \
  LocomotorSet\n    Locomotor = HordeFeet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = HordeAIUpdate Tag_AI\n    MoodAttackCheckRate = 100\n  End\n  \
  Behavior = BloodthirstyUpdate Tag_Thirst\n    SacrificeFilter = ALL\n    \
    ExperienceModifier = 1.5\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 1\n    RanksToReleaseWhenAttacking = 1\n    \
    RankInfo = RankNumber:1 UnitType:Cutter Position:X:10 Y:0\n  End\nEnd\n\
Object VictimHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 100\n  \
  BuildTime = 1\n  CommandPoints = 5\n  \
  LocomotorSet\n    Locomotor = HordeFeet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = BloodthirstyUpdate Tag_Thirst\n    SacrificeFilter = ALL\n    \
    ExperienceModifier = 1.5\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 3\n    \
    RankInfo = RankNumber:1 UnitType:Meat Position:X:10 Y:-10 Position:X:10 Y:10 \
Position:X:10 Y:0\n  End\nEnd\n\
CommandButton Build_PlainHorde\n  Command = UNIT_BUILD\n  Object = PlainHorde\nEnd\n\
CommandButton Build_EliteHorde\n  Command = UNIT_BUILD\n  Object = EliteHorde\nEnd\n\
CommandButton Build_ArmedHorde\n  Command = UNIT_BUILD\n  Object = ArmedHorde\nEnd\n\
CommandButton Build_HealBannerHorde\n  Command = UNIT_BUILD\n  Object = HealBannerHorde\nEnd\n\
Weapon Claw\n  AttackRange = 200\n  DelayBetweenShots = 1000\n  \
  DamageNugget\n    Damage = 200000\n    DamageType = SLASH\n  End\nEnd\n\
Object Killer\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Claw\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Fighter\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n    InitialHealth = 50\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ArmedFighter\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Nibble\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ArmedBannerman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE BANNER\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Nibble\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  Behavior = BannerCarrierUpdate Tag_Banner\n    IdleSpawnRate = 1000\n    \
    MeleeFreeUnitSpawnTime = 0\n    DiedRespawnTime = 1000\n    MeleeFreeBannerReSpawnTime = 0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object HealBannerman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE BANNER\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n    InitialHealth = 50\n  End\n  \
  Behavior = AutoHealBehavior Tag_Heal\n    StartsActive = Yes\n    HealingAmount = 10\n    \
    HealingDelay = 1000\n    AffectsContained = Yes\n    KindOf = CAVALRY\n    \
    UnitHealPulseFX = FX_BannerHeal\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object PlainHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 100\n  BuildTime = 1\n  \
  CommandPoints = 5\n  \
  LocomotorSet\n    Locomotor = HordeFeet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 3\n    \
    RankInfo = RankNumber:1 UnitType:Fighter Position:X:10 Y:-10 Position:X:10 Y:10 \
Position:X:10 Y:0\n  End\nEnd\n\
Object EliteHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 100\n  BuildTime = 1\n  \
  CommandPoints = 5\n  \
  LocomotorSet\n    Locomotor = HordeFeet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = AutoHealBehavior Tag_Heal\n    StartsActive = Yes\n    HealingAmount = 1\n    \
    HealingDelay = 500\n    Radius = 100\n    RespawnNearbyHordeMembers = Yes\n    \
    RespawnMinimumDelay = 10\n    RespawnFXList = FX_Regrow\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 3\n    \
    RankInfo = RankNumber:1 UnitType:Fighter Position:X:10 Y:-10 Position:X:10 Y:10 \
Position:X:10 Y:0\n  End\nEnd\n\
Object ArmedHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 100\n  BuildTime = 1\n  \
  CommandPoints = 5\n  VisionRange = 400\n  \
  LocomotorSet\n    Locomotor = HordeFeet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = HordeAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    MoodAttackCheckRate = 100\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 3\n    \
    RanksToReleaseWhenAttacking = 1\n    \
    BannerCarriersAllowed = ArmedBannerman\n    BannerCarrierMinLevel = 0\n    \
    BannerCarrierPosition = UnitType:ArmedFighter Pos:X:-30.0 Y:0.0\n    \
    RankInfo = RankNumber:1 UnitType:ArmedFighter Position:X:10 Y:-10 Position:X:10 Y:10 \
Position:X:10 Y:0\n  End\nEnd\n\
Object HealBannerHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 100\n  BuildTime = 1\n  \
  CommandPoints = 5\n  \
  LocomotorSet\n    Locomotor = HordeFeet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 3\n    \
    BannerCarriersAllowed = HealBannerman\n    BannerCarrierMinLevel = 0\n    \
    BannerCarrierPosition = UnitType:Fighter Pos:X:-30.0 Y:0.0\n    \
    RankInfo = RankNumber:1 UnitType:Fighter Position:X:10 Y:-10 Position:X:10 Y:10 \
Position:X:10 Y:0\n  End\nEnd\n\
Object Fountain\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = BannerCarrierUpdate Tag_Refill\n    IdleSpawnRate = 1000\n    \
    ReplenishNearbyHorde = Yes\n    ScanHordeDistance = 200\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 20\nEnd\n";

fn horde_scene(
    spawns: &[(&str, Vec2, PlayerId)],
    allied: bool,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    full_scene(&data(&[HORDE_DATA.to_owned()]), spawns, allied)
}

fn full_scene(
    text: &str,
    spawns: &[(&str, Vec2, PlayerId)],
    allied: bool,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let mut names: Vec<String> = [
        "Barracks",
        "PlainHorde",
        "EliteHorde",
        "ArmedHorde",
        "HealBannerHorde",
        "SacrificerHorde",
        "VictimHorde",
    ]
    .into_iter()
    .filter(|name| {
        text.contains(&format!(
            "Object {name}
"
        ))
    })
    .map(str::to_owned)
    .collect();
    names.extend(spawns.iter().map(|(name, ..)| (*name).to_owned()));
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let spawned = spawns
        .iter()
        .map(|(name, at, owner)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    *at,
                    Facing::EAST,
                    *owner,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let teams = if allied {
        vec![Some(1), Some(1), None]
    } else {
        vec![None, None]
    };
    let rules = RotwkRules::new(passability, spawned)
        .with_players(if allied { 3 } else { 2 })
        .with_teams(teams)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_banners(crate::horde::banner_plans(&data.formations, &kinds))
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_experience(Arc::clone(&data.experience), data.armor_bonus_cap);
    let simulation = Simulation::new(23, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let loose = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    (simulation, ids, loose)
}

fn cull_to(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    horde: Entity,
    keep: usize,
    killers: &[Entity],
    owner: PlayerId,
) {
    for _ in 0..300 {
        if members_of(simulation, ids, horde).len() <= keep {
            break;
        }
        simulation.tick().unwrap();
    }
    kill(simulation, owner, killers);
    assert_eq!(members_of(simulation, ids, horde).len(), keep, "culled");
}

fn spawn_horde(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    owner: PlayerId,
    label: &str,
    at: Vec2,
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
            owner,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: at.x,
                y: at.y,
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

fn members_of(simulation: &Simulation<RotwkRules>, ids: Ids, horde: Entity) -> Vec<Entity> {
    simulation
        .state()
        .get(ids.horde, horde)
        .map(|horde| horde.members.clone())
        .unwrap_or_default()
}

#[test]
fn an_elite_horde_regrows_one_member_per_minimum_delay() {
    let (mut simulation, ids, loose) = horde_scene(
        &[
            ("Barracks", at(-100.0, 0.0), PLAYER),
            ("Killer", at(150.0, 0.0), OTHER),
        ],
        false,
    );
    let horde = spawn_horde(&mut simulation, ids, PLAYER, "EliteHorde", AT);
    assert_eq!(members_of(&simulation, ids, horde).len(), 3);
    cull_to(&mut simulation, ids, horde, 1, &loose[1..2], OTHER);
    let mut last = members_of(&simulation, ids, horde).len();
    let mut regrew = Vec::new();
    for _ in 0..400 {
        let frame = simulation.tick().unwrap().frame.get();
        let now = members_of(&simulation, ids, horde).len();
        if now > last {
            regrew.push((frame, now - last));
        }
        last = now;
    }
    assert_eq!(last, 3, "whole again: {regrew:?}");
    assert_eq!(regrew.len(), 2, "{regrew:?}");
    assert!(regrew.iter().all(|(_, grown)| *grown == 1), "{regrew:?}");
    assert!(regrew[1].0 - regrew[0].0 >= 60, "{regrew:?}");
}

#[test]
fn a_refill_reaches_an_allied_players_horde() {
    let (mut simulation, ids, loose) = horde_scene(
        &[
            ("Barracks", at(-200.0, 0.0), PLAYER),
            ("Fountain", at(0.0, 80.0), PLAYER),
            ("Killer", at(150.0, 0.0), THIRD),
        ],
        true,
    );
    let horde = spawn_horde(&mut simulation, ids, OTHER, "PlainHorde", AT);
    cull_to(&mut simulation, ids, horde, 2, &loose[2..3], THIRD);
    run(&mut simulation, 150);
    assert_eq!(members_of(&simulation, ids, horde).len(), 3);
}

#[test]
fn a_horde_fighting_on_order_regrows_through_a_carrier_that_is_not_attacking() {
    let (mut simulation, ids, loose) = horde_scene(
        &[
            ("Barracks", at(-200.0, 0.0), PLAYER),
            ("Dummy", at(0.0, 150.0), OTHER),
            ("Killer", at(150.0, 0.0), OTHER),
        ],
        false,
    );
    let horde = spawn_horde(&mut simulation, ids, PLAYER, "ArmedHorde", AT);
    let members = members_of(&simulation, ids, horde);
    assert_eq!(members.len(), 4, "three and the carrier");
    cull_to(&mut simulation, ids, horde, 3, &loose[2..3], OTHER);
    select(&mut simulation, PLAYER, &members);
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: loose[1] })
        .unwrap();
    run(&mut simulation, 300);
    assert!(
        simulation.state().contains(ids.attack_order, horde),
        "the horde holds its order"
    );
    assert_eq!(members_of(&simulation, ids, horde).len(), 4, "regrown");
}

#[test]
fn a_banner_carrier_heals_its_hordes_members_whatever_the_kind() {
    let (mut simulation, ids, _) = horde_scene(&[("Barracks", at(-200.0, 0.0), PLAYER)], false);
    let horde = spawn_horde(&mut simulation, ids, PLAYER, "HealBannerHorde", AT);
    let members = members_of(&simulation, ids, horde);
    assert_eq!(members.len(), 4, "three and the carrier");
    let fx = played(&mut simulation, "FX_BannerHeal", 100);
    for member in members {
        assert!(health(&simulation, ids, member) > 50.0, "{member:?} healed");
    }
    assert!(fx >= 3, "the FX plays on healed members: {fx}");
}

const ORDER_DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
SpecialPower SpecialAbilityZap\n  ReloadTime = 30000\nEnd\n\
Weapon ZapWeapon\n  AttackRange = 500\n  DelayBetweenShots = 3000\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
CommandSet ZapSet\n  1 = Fire_Zap\nEnd\n\
CommandButton Fire_Zap\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityZap\n  \
  Options = NEED_TARGET_ENEMY_OBJECT\nEnd\n\
Upgrade Upgrade_Dissent\n  Type = OBJECT\n  BuildCost = 0\n  BuildTime = 1\nEnd\n\
ModifierList Dissent\n  Category = SPELL\n  Modifier = DAMAGE_MULT 110%\n  Duration = 600000\n  \
  ModelCondition = EMOTION_DISSIDENT\nEnd\n\
Object Zapper\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = ZapSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Behavior = WeaponFireSpecialAbilityUpdate Tag_Zap\n    SpecialPowerTemplate = SpecialAbilityZap\n    \
    SpecialWeapon = ZapWeapon\n    WhichSpecialWeapon = 1\n    UnpackTime = 1\n    \
    PreparationTime = 1\n    BusyForDuration = 3000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Rebel\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Behavior = ModelConditionUpgrade Tag_Dissent\n    TriggeredBy = Upgrade_Dissent\n    \
    AddConditionFlags = EMOTION_DISSIDENT\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Footman\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn position(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> Vec2 {
    simulation
        .state()
        .get(ids.body, entity)
        .map_or(Vec2::ZERO, |body| body.0.position)
}

fn order_move(simulation: &mut Simulation<RotwkRules>, entity: Entity, to: Vec2) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![entity],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: to.x,
                y: to.y,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
}

#[test]
fn an_order_given_in_the_busy_window_runs_when_it_ends() {
    let (mut simulation, ids, found) = full_scene(
        ORDER_DATA,
        &[("Zapper", AT, PLAYER), ("Footman", at(100.0, 0.0), OTHER)],
        false,
    );
    let (zapper, footman) = (found[0], found[1]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![zapper],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: Some(footman),
                at: None,
            },
        )
        .unwrap();
    for _ in 0..60 {
        if simulation.state().contains(ids.ignore_ai_command, zapper) {
            break;
        }
        simulation.tick().unwrap();
    }
    assert!(
        simulation.state().contains(ids.ignore_ai_command, zapper),
        "the window opened"
    );
    let start = position(&simulation, ids, zapper);
    order_move(&mut simulation, zapper, at(0.0, 100.0));
    order_move(&mut simulation, zapper, at(0.0, 200.0));
    run(&mut simulation, 30);
    let held = position(&simulation, ids, zapper);
    assert!(
        (held.y - start.y).abs() < 1.0,
        "no walking inside the window: {start:?} -> {held:?}"
    );
    run(&mut simulation, 400);
    let end = position(&simulation, ids, zapper);
    assert!(
        (end.y - (AT.y + 200.0)).abs() < 12.0,
        "walked to the newest order: {end:?}"
    );
}

#[test]
fn a_dissident_unit_refuses_its_players_orders() {
    let (mut simulation, ids, found) = scene(ORDER_DATA, &[("Rebel", AT, PLAYER)]);
    let rebel = found[0];
    order_move(&mut simulation, rebel, at(0.0, 150.0));
    run(&mut simulation, 120);
    assert!(
        position(&simulation, ids, rebel).y > AT.y + 100.0,
        "obedient"
    );
    grant(&mut simulation, &[rebel], "Upgrade_Dissent");
    let here = position(&simulation, ids, rebel);
    order_move(&mut simulation, rebel, at(0.0, 0.0));
    run(&mut simulation, 120);
    let after = position(&simulation, ids, rebel);
    assert!(
        (after.y - here.y).abs() < 1.0,
        "the dissident stays put: {here:?} -> {after:?}"
    );
}

fn select(simulation: &mut Simulation<RotwkRules>, player: PlayerId, who: &[Entity]) {
    simulation
        .submit(
            player,
            RotwkCommand::SetSelection {
                replace: true,
                units: who.to_vec(),
            },
        )
        .unwrap();
}

fn sacrifice_scene() -> (
    Simulation<RotwkRules>,
    Ids,
    (Entity, Entity),
    (Entity, Vec<Entity>),
) {
    let (mut simulation, ids, _) = horde_scene(&[("Barracks", at(-200.0, 0.0), PLAYER)], false);
    let sacrificer = spawn_horde(&mut simulation, ids, PLAYER, "SacrificerHorde", AT);
    let victims = spawn_horde(&mut simulation, ids, PLAYER, "VictimHorde", at(100.0, 0.0));
    let cutter = members_of(&simulation, ids, sacrificer)[0];
    let meat = members_of(&simulation, ids, victims);
    (simulation, ids, (sacrificer, cutter), (victims, meat))
}

fn press_sacrifice(simulation: &mut Simulation<RotwkRules>, cutter: Entity, meat: Entity) {
    select(simulation, PLAYER, &[cutter]);
    simulation
        .submit(PLAYER, RotwkCommand::Sacrifice { target: meat })
        .unwrap();
    simulation.tick().unwrap();
}

#[test]
fn a_sacrificed_horde_holds_still_and_dies_a_member_a_blow() {
    let (mut simulation, ids, (sacrificer, cutter), (victims, meat)) = sacrifice_scene();
    assert_eq!(meat.len(), 3);
    press_sacrifice(&mut simulation, cutter, meat[0]);
    assert!(simulation.state().contains(ids.sacrificing, sacrificer));
    let before = position(&simulation, ids, meat[2]);
    select(&mut simulation, PLAYER, &meat);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 600.0,
                y: 600.0,
                queue: false,
            },
        )
        .unwrap();
    run(&mut simulation, 40);
    let after = position(&simulation, ids, meat[2]);
    assert!(
        (after.x - before.x).abs() + (after.y - before.y).abs() < 1.0,
        "the victims stand: {before:?} -> {after:?}"
    );
    run(&mut simulation, 400);
    assert!(
        members_of(&simulation, ids, victims).len() < 3,
        "the sacrifice kills"
    );
    assert_eq!(members_of(&simulation, ids, sacrificer).len(), 1);
}

#[test]
fn any_other_order_ends_the_sacrifice() {
    let (mut simulation, ids, (sacrificer, cutter), (victims, meat)) = sacrifice_scene();
    press_sacrifice(&mut simulation, cutter, meat[0]);
    select(&mut simulation, PLAYER, &meat);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 100.0,
                y: 450.0,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let before = position(&simulation, ids, meat[2]);
    select(&mut simulation, PLAYER, &[cutter]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 150.0,
                y: 150.0,
                queue: false,
            },
        )
        .unwrap();
    run(&mut simulation, 60);
    assert!(!simulation.state().contains(ids.sacrificing, sacrificer));
    let after = position(&simulation, ids, meat[2]);
    assert!(
        (after.y - before.y).abs() > 3.0,
        "the held order runs once it ends: {before:?} -> {after:?}"
    );
    run(&mut simulation, 200);
    assert_eq!(
        members_of(&simulation, ids, victims).len(),
        3,
        "nobody died"
    );
}

const MONITOR_DATA: &str = "\
Weapon Barrage\n  AttackRange = 300\n  DelayBetweenShots = 500\n  NoVictimNeeded = Yes\n  \
  DamageNugget\n    Damage = 10\n    Radius = 20\n    DamageType = PIERCE\n  End\nEnd\n\
Upgrade Upgrade_Mount\n  Type = OBJECT\n  BuildCost = 0\n  BuildTime = 1\nEnd\n\
CommandButton Cmd_Stop\n  Command = STOP\nEnd\n\
CommandSet BaseSet\n  1 = Cmd_Stop\nEnd\n\
CommandSet StopSet\n  1 = Cmd_Stop\nEnd\n\
CommandSet ToggleSet\n  1 = Cmd_Stop\nEnd\n\
Object Mortarman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 200\n  \
  CommandSet = BaseSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Barrage\n    \
    Weapon = TERTIARY Barrage\n    AutoChooseSources = TERTIARY FROM_PLAYER\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  Behavior = MonitorConditionUpdate Tag_Monitor\n    ModelConditionFlags = ATTACKING_POSITION\n    \
    ModelConditionCommandSet = StopSet\n    WeaponSetFlags = MOUNTED\n    \
    WeaponToggleCommandSet = ToggleSet\n  End\n  \
  Behavior = ModelConditionUpgrade Tag_Mount\n    TriggeredBy = Upgrade_Mount\n    \
    AddConditionFlags = MOUNTED\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

#[test]
fn the_command_set_follows_the_condition_over_the_weapon_flag_and_comes_back() {
    let (mut simulation, ids, found) = scene(MONITOR_DATA, &[("Mortarman", AT, PLAYER)]);
    let who = found[0];
    let state = simulation.rules().upgrade_state_id().unwrap();
    let set = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(state, who)
            .and_then(|held| held.command_set.clone())
    };
    run(&mut simulation, 5);
    assert_eq!(set(&simulation), None);
    grant(&mut simulation, &[who], "Upgrade_Mount");
    run(&mut simulation, 5);
    assert_eq!(
        set(&simulation).as_deref(),
        Some("ToggleSet"),
        "the weapon flag"
    );
    select(&mut simulation, PLAYER, &[who]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::AttackGround {
                slot: 2,
                x: AT.x + 100.0,
                y: AT.y,
            },
        )
        .unwrap();
    run(&mut simulation, 10);
    assert_eq!(
        set(&simulation).as_deref(),
        Some("StopSet"),
        "the condition wins"
    );
    simulation.submit(PLAYER, RotwkCommand::Stop).unwrap();
    run(&mut simulation, 10);
    assert_eq!(
        set(&simulation).as_deref(),
        Some("ToggleSet"),
        "back to the flag's"
    );
    let _ = ids;
}
