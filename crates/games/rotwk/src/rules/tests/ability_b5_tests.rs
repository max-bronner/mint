use super::*;
use crate::rules::debug::DebugCommand;

pub(super) const AT: Vec2 = Vec2::new(300.0, 300.0);

pub(super) fn at(dx: f32, dy: f32) -> Vec2 {
    Vec2::new(AT.x + dx, AT.y + dy)
}

pub(super) fn scene(
    text: &str,
    spawns: &[(&str, Vec2, PlayerId)],
    extra: &[&str],
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>, Arc<KindTable>) {
    scene_with(text, spawns, extra, false)
}

pub(super) fn scene_with(
    text: &str,
    spawns: &[(&str, Vec2, PlayerId)],
    extra: &[&str],
    allied: bool,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>, Arc<KindTable>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let mut names: Vec<String> = spawns.iter().map(|(name, ..)| (*name).to_owned()).collect();
    names.extend(extra.iter().map(|name| (*name).to_owned()));
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
    let rules = RotwkRules::new(passability, spawned)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_creation_lists(Arc::clone(&data.ocls))
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_experience(Arc::clone(&data.experience), data.armor_bonus_cap)
        .with_economy(production.buildables.clone(), 1_000)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_players(if allied { 3 } else { 2 })
        .with_teams(if allied {
            vec![Some(1), Some(1), None]
        } else {
            vec![None, None]
        });
    let simulation = Simulation::new(23, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let loose = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    (simulation, ids, loose, kinds)
}

pub(super) fn run(simulation: &mut Simulation<RotwkRules>, frames: u32) {
    for _ in 0..frames {
        simulation.tick().unwrap();
    }
}

pub(super) fn select(simulation: &mut Simulation<RotwkRules>, player: PlayerId, who: &[Entity]) {
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

pub(super) fn grant_player_upgrade(simulation: &mut Simulation<RotwkRules>, name: &str) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Debug(DebugCommand::Upgrade {
                target: crate::rules::DebugTarget::All,
                upgrade: name.to_owned(),
            }),
        )
        .unwrap();
    simulation.tick().unwrap();
}

pub(super) fn player_holds(
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    player: PlayerId,
    upgrade: &str,
) -> bool {
    let bit = simulation.rules().upgrade_bits.bit(upgrade).unwrap();
    crate::economy::player_entity(simulation.state(), RotwkRules::economy_ids(ids), player)
        .and_then(|entity| simulation.state().get(ids.player_upgrades, entity))
        .is_some_and(|held| held.0.contains(bit))
}

pub(super) fn kind_count(
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    kinds: &KindTable,
    name: &str,
) -> usize {
    let kind = kinds.kind(name).unwrap();
    simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(entity, k)| **k == kind && simulation.state().is_live(*entity))
        .count()
}

pub(super) fn position_of(
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    kinds: &KindTable,
    name: &str,
) -> Option<Vec2> {
    let kind = kinds.kind(name)?;
    simulation
        .state()
        .store(ids.kind)?
        .iter()
        .find(|(_, k)| **k == kind)
        .and_then(|(entity, _)| simulation.state().get(ids.body, entity))
        .map(|body| body.0.position)
}

const SUMMON_DATA: &str = "\
Upgrade Upgrade_Allies\n  Type = PLAYER\n  BuildCost = 0\n  BuildTime = 1\nEnd\n\
Upgrade Upgrade_Boost\n  Type = PLAYER\n  BuildCost = 0\n  BuildTime = 1\nEnd\n\
SpecialPower SummonPower\n  ReloadTime = 0\nEnd\n\
ObjectCreationList OCL_Plain\n  CreateObject\n    ObjectNames = Plain\n  End\nEnd\n\
ObjectCreationList OCL_Boosted\n  CreateObject\n    ObjectNames = Boosted\n  End\nEnd\n\
CommandSet SummonerSet\n  1 = Cast_Point\n  2 = Cast_Self\nEnd\n\
CommandButton Cast_Point\n  Command = SPECIAL_POWER\n  SpecialPower = SummonPower\n  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Cast_Self\n  Command = SPECIAL_POWER\n  SpecialPower = SummonPower\nEnd\n\
Object Summoner\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = SummonerSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = OCLSpecialPower Tag_Summon\n    SpecialPowerTemplate = SummonPower\n    \
    OCL = OCL_Plain\n    UpgradeOCL = Upgrade_Boost OCL_Boosted\n    \
    UpgradeName = Upgrade_Allies\n    CreateLocation = CREATE_AT_EDGE_NEAR_TARGET\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Plain\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Boosted\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn cast(
    simulation: &mut Simulation<RotwkRules>,
    caster: Entity,
    button: u8,
    at: Option<(f32, f32)>,
) {
    select(simulation, PLAYER, &[caster]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button,
                target: None,
                at,
            },
        )
        .unwrap();
    run(simulation, 12);
}

#[test]
fn a_summon_power_chooses_its_list_by_upgrade_and_grants_after_a_located_cast() {
    let (mut simulation, ids, found, kinds) = scene(
        SUMMON_DATA,
        &[("Summoner", AT, PLAYER)],
        &["Plain", "Boosted"],
    );
    let caster = found[0];
    cast(&mut simulation, caster, 1, None);
    assert_eq!(kind_count(&simulation, ids, &kinds, "Plain"), 1);
    let made = position_of(&simulation, ids, &kinds, "Plain").unwrap();
    assert!(
        (made.x - AT.x).abs() < 40.0 && (made.y - AT.y).abs() < 40.0,
        "a target-less cast creates at the caster, not at the edge: {made:?}"
    );
    assert!(
        !player_holds(&simulation, ids, PLAYER, "Upgrade_Allies"),
        "no grant without a target"
    );
    cast(
        &mut simulation,
        caster,
        0,
        Some((at(100.0, 0.0).x, at(100.0, 0.0).y)),
    );
    assert_eq!(kind_count(&simulation, ids, &kinds, "Plain"), 2);
    assert!(player_holds(&simulation, ids, PLAYER, "Upgrade_Allies"));
    grant_player_upgrade(&mut simulation, "Upgrade_Boost");
    cast(
        &mut simulation,
        caster,
        0,
        Some((at(100.0, 0.0).x, at(100.0, 0.0).y)),
    );
    assert_eq!(kind_count(&simulation, ids, &kinds, "Boosted"), 1);
    assert_eq!(kind_count(&simulation, ids, &kinds, "Plain"), 2);
}

const PILLAGE_DATA: &str = "\
Weapon Poke\n  AttackRange = 400\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 1\n    DamageType = SLASH\n  End\nEnd\n\
Object Pillager\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Poke\n  End\n  \
  Behavior = PillageModule Tag_Pillage\n    PillageAmount = 7\n    NumDamageEventsPerPillage = 2\n    \
    PillageFilter = NONE +INFANTRY\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Robber\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Poke\n  End\n  \
  Behavior = PillageModule Tag_Pillage\n    PillageAmount = 5000000\n    NumDamageEventsPerPillage = 1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Soldier\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn purse(simulation: &Simulation<RotwkRules>, ids: Ids, player: PlayerId) -> i64 {
    crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), player).amount()
}

fn health_of(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.health, entity)
        .map_or(0.0, |pool| pool.0.current())
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn pillage_run(attacker_name: &str) -> (u32, i64, i64) {
    let (mut simulation, ids, found, _) = scene(
        PILLAGE_DATA,
        &[
            (attacker_name, AT, PLAYER),
            ("Soldier", at(100.0, 0.0), OTHER),
        ],
        &[],
    );
    let (attacker, soldier) = (found[0], found[1]);
    let (mine, theirs) = (
        purse(&simulation, ids, PLAYER),
        purse(&simulation, ids, OTHER),
    );
    let before = health_of(&simulation, ids, soldier);
    select(&mut simulation, PLAYER, &[attacker]);
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: soldier })
        .unwrap();
    run(&mut simulation, 90);
    let hits = (before - health_of(&simulation, ids, soldier)).round() as u32;
    assert!(hits >= 4, "the premise: {hits} hits");
    let gained = purse(&simulation, ids, PLAYER) - mine;
    let lost = theirs - purse(&simulation, ids, OTHER);
    assert_eq!(gained, lost, "money moves, it is not made");
    (hits, gained, theirs)
}

#[test]
fn a_pillager_takes_money_from_the_victims_player_on_every_nth_hit() {
    let (hits, gained, _) = pillage_run("Pillager");
    assert_eq!(gained, i64::from(hits / 2) * 7);
    let (_, gained, victims_money) = pillage_run("Robber");
    assert_eq!(
        gained, victims_money,
        "cannot take more than the victim holds"
    );
}

const SWARM_DATA: &str = "\
ModifierList SwarmBonus\n  Category = BUFF\n  Modifier = ARMOR 50%\n  Duration = 0\nEnd\n\
Object Orc\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object SwarmHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE INFANTRY\n  CommandSet = EmptySet\n  \
  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 5\n  \
  Behavior = LargeGroupBonusUpdate Tag_Swarm\n    UpdateRate = 500\n    HordeMemberFilter = NONE +Orc\n    \
    Count = 9\n    Radius = 100\n    RubOffRadius = 100\n    AlliesOnly = Yes\n    \
    AttributeModifier = SwarmBonus\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 4\n    \
    RankInfo = RankNumber:1 UnitType:Orc Position:X:10 Y:-10 Position:X:10 Y:10 \
Position:X:10 Y:0 Position:X:20 Y:0\n  End\nEnd\n\
CommandSet EmptySet\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_SwarmHorde\nEnd\n\
CommandButton Build_SwarmHorde\n  Command = UNIT_BUILD\n  Object = SwarmHorde\nEnd\n";

fn swarm_horde(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    owner: PlayerId,
    at: Vec2,
) -> Entity {
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "SwarmHorde")
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

fn swarming(simulation: &Simulation<RotwkRules>, ids: Ids, horde: Entity) -> bool {
    simulation.state().contains(ids.group_bonus, horde)
}

#[test]
fn a_swarm_counts_the_same_players_hordes_and_spreads_to_itself() {
    let (mut simulation, ids, _, kinds) = scene_with(
        SWARM_DATA,
        &[("Camp", at(-200.0, -200.0), PLAYER)],
        &["SwarmHorde"],
        true,
    );
    let first = swarm_horde(&mut simulation, ids, PLAYER, AT);
    run(&mut simulation, 90);
    assert!(!swarming(&simulation, ids, first), "4 + 1 is short of 9");
    let ally = swarm_horde(&mut simulation, ids, OTHER, at(40.0, 0.0));
    run(&mut simulation, 90);
    assert!(
        !swarming(&simulation, ids, first),
        "an ally's horde does not count"
    );
    let second = swarm_horde(&mut simulation, ids, PLAYER, at(0.0, 40.0));
    run(&mut simulation, 90);
    assert!(swarming(&simulation, ids, first) && swarming(&simulation, ids, second));
    assert!(!swarming(&simulation, ids, ally));
    let at = simulation.state().get(ids.body, second).unwrap().0.position;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Debug(DebugCommand::Kill {
                target: crate::rules::DebugTarget::Near {
                    kind: kinds.kind("SwarmHorde").unwrap().0,
                    x: at.x,
                    y: at.y,
                },
            }),
        )
        .unwrap();
    run(&mut simulation, 120);
    assert!(
        swarming(&simulation, ids, first),
        "an active horde in reach keeps the bonus below the count"
    );
}

const TROLL_DATA: &str = "\
Locomotor TrollFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
SpecialPower ScufflePower\n  ReloadTime = 60000\nEnd\n\
Object Troll\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = TrollFeet\n    Condition = SET_NORMAL\n    Speed = 30\n  End\n  \
  Behavior = ClickReactionBehavior Tag_Click\n    ClickReactionTimer = 10\n    ReactionFrames1 = 100\n    \
    ReactionFrames2 = 100\n    ReactionFrames3 = 100\n    ReactionFrames4 = 100\n    ReactionFrames5 = 100\n  End\n  \
  Behavior = SpecialPowerModule Tag_Scuffle\n    SpecialPowerTemplate = ScufflePower\n  End\n  \
  Behavior = BoredUpdate Tag_Bored\n    ScanDelayTime = 1000\n    ScanDistance = 70\n    \
    BoredFilter = NONE +INFANTRY\n    SpecialPowerTemplate = ScufflePower\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject PalTroll Troll\nEnd\n";

fn holds_react(simulation: &Simulation<RotwkRules>, entity: Entity) -> Vec<u32> {
    let wiring = simulation.rules().modules.as_ref().unwrap();
    let state = simulation.state().get(wiring.state, entity).unwrap();
    (1..=6)
        .filter(|rung| state.has_model_condition(&format!("REACT_{rung}")))
        .collect()
}

#[test]
fn clicks_on_an_idle_troll_climb_rungs_and_an_order_ends_the_reaction() {
    let (mut simulation, ids, found, _) = scene(TROLL_DATA, &[("Troll", AT, PLAYER)], &[]);
    let troll = found[0];
    for _ in 0..7 {
        select(&mut simulation, PLAYER, &[troll]);
    }
    run(&mut simulation, 2);
    assert_eq!(holds_react(&simulation, troll).len(), 1, "a reaction plays");
    select(&mut simulation, PLAYER, &[troll]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: AT.x + 300.0,
                y: AT.y,
                queue: false,
            },
        )
        .unwrap();
    run(&mut simulation, 12);
    assert!(
        holds_react(&simulation, troll).is_empty(),
        "an order ends it"
    );
    simulation.submit(PLAYER, RotwkCommand::Stop).unwrap();
    run(&mut simulation, 60);
    select(&mut simulation, PLAYER, &[troll]);
    run(&mut simulation, 1);
    let rung = holds_react(&simulation, troll);
    assert!(rung == vec![4] || rung == vec![5], "{rung:?}");

    let (mut simulation, ids2, found, _) = scene(TROLL_DATA, &[("Troll", AT, PLAYER)], &[]);
    select(&mut simulation, OTHER, &[found[0]]);
    run(&mut simulation, 2);
    assert!(
        !simulation.state().contains(ids2.click_reaction, found[0]),
        "another player's selection is no click"
    );
    let _ = ids;
}

fn scuffle_ready(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> bool {
    simulation.rules().power_is_ready(
        simulation.state(),
        ids,
        simulation.frame(),
        entity,
        "ScufflePower",
    )
}

#[test]
fn an_idle_troll_starts_its_scuffle_and_a_walking_one_does_not() {
    let (mut simulation, ids, found, _) = scene(
        TROLL_DATA,
        &[("Troll", AT, PLAYER), ("PalTroll", at(50.0, 0.0), PLAYER)],
        &[],
    );
    assert!(scuffle_ready(&simulation, ids, found[0]));
    run(&mut simulation, 50);
    assert!(!scuffle_ready(&simulation, ids, found[0]), "idle: cast");
    let (mut simulation, ids, found, _) = scene(
        TROLL_DATA,
        &[("Troll", AT, PLAYER), ("PalTroll", at(50.0, 0.0), PLAYER)],
        &[],
    );
    select(&mut simulation, PLAYER, &[found[0]]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: AT.x,
                y: AT.y + 300.0,
                queue: false,
            },
        )
        .unwrap();
    run(&mut simulation, 50);
    assert!(
        scuffle_ready(&simulation, ids, found[0]),
        "walking: no cast"
    );
}

const UNDEAD_DATA: &str = "\
Weapon Reaper\n  AttackRange = 400\n  DelayBetweenShots = 600\n  \
  DamageNugget\n    Damage = 5000\n    DamageType = CRUSH\n    DamageSubType = BECOME_UNDEAD_ONCE\n  End\nEnd\n\
ObjectCreationList OCL_Raise\n  CreateObject\n    ObjectNames = Mark\n  End\nEnd\n\
Object ReaperShooter\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Reaper\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Sleeper\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = DamageFilteredCreateObjectDie Tag_Raise\n    CreationList = OCL_Raise\n    \
    DamageTypeTriggersInstantly = BECOME_UNDEAD_ONCE\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Mark\n  KindOf = INERT IMMOBILE UNATTACKABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\nEnd\n";

#[test]
fn a_become_undead_once_killer_creates_at_most_once_per_six_retail_frames() {
    let (mut simulation, ids, found, kinds) = scene(
        UNDEAD_DATA,
        &[
            ("ReaperShooter", AT, PLAYER),
            ("Sleeper", at(100.0, 0.0), OTHER),
            ("Sleeper", at(100.0, 20.0), OTHER),
            ("Sleeper", at(100.0, 40.0), OTHER),
            ("Sleeper", at(100.0, 60.0), OTHER),
        ],
        &["Mark"],
    );
    select(&mut simulation, PLAYER, &[found[0]]);
    for target in &found[1..5] {
        simulation
            .submit(PLAYER, RotwkCommand::AttackObject { target: *target })
            .unwrap();
        run(&mut simulation, 24);
    }
    run(&mut simulation, 60);
    let dead = (1..5)
        .filter(|i| !simulation.state().is_live(found[*i]))
        .count();
    let marks = kind_count(&simulation, ids, &kinds, "Mark");
    assert!(dead >= 3, "the premise: {dead} kills");
    assert!(
        marks >= 1 && marks < dead,
        "{marks} creations for {dead} kills"
    );
}

const LOCO_DATA: &str = "\
Locomotor Slow\n  Surfaces = GROUND\n  Acceleration = 50\n  Braking = 50\n  TurnTime = 50\nEnd\n\
Locomotor Fast\n  Surfaces = GROUND\n  Acceleration = 50\n  Braking = 50\n  TurnTime = 50\nEnd\n\
Upgrade Upgrade_Climb\n  Type = OBJECT\n  BuildCost = 0\n  BuildTime = 1\nEnd\n\
Object Climber\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = Slow\n    Condition = SET_NORMAL\n    Speed = 30\n  End\n  \
  LocomotorSet\n    Locomotor = Fast\n    Condition = SET_NORMAL_UPGRADED\n    Speed = 120\n  End\n  \
  Behavior = LocomotorSetUpgrade Tag_Loco\n    TriggeredBy = Upgrade_Climb\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Plodder\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = Slow\n    Condition = SET_NORMAL\n    Speed = 30\n  End\n  \
  LocomotorSet\n    Locomotor = Fast\n    Condition = SET_NORMAL_UPGRADED\n    Speed = 120\n  End\n  \
  Behavior = LocomotorSetUpgrade Tag_Loco\n    TriggeredBy = Upgrade_Climb\n    KillLocomotorUpgrade = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn walked(name: &str, upgrade: bool) -> f32 {
    let (mut simulation, ids, found, _) = scene(LOCO_DATA, &[(name, AT, PLAYER)], &[]);
    let unit = found[0];
    if upgrade {
        select(&mut simulation, PLAYER, &[unit]);
        simulation
            .submit(
                PLAYER,
                RotwkCommand::Debug(DebugCommand::GrantUpgrade {
                    upgrade: "Upgrade_Climb".to_owned(),
                }),
            )
            .unwrap();
        run(&mut simulation, 2);
    }
    select(&mut simulation, PLAYER, &[unit]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: AT.x + 400.0,
                y: AT.y,
                queue: false,
            },
        )
        .unwrap();
    run(&mut simulation, 60);
    simulation.state().get(ids.body, unit).unwrap().0.position.x - AT.x
}

#[test]
fn a_locomotor_upgrade_swaps_the_normal_set_for_the_upgraded_one() {
    let plain = walked("Climber", false);
    let upgraded = walked("Climber", true);
    assert!(upgraded > plain * 2.0, "{plain} -> {upgraded}");
    let killed = walked("Plodder", true);
    assert!((killed - plain).abs() < 1.0, "{killed} vs {plain}");
}
