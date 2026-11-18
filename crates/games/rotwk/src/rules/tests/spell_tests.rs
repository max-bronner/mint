use super::*;
use crate::sciences::SpellState;

const DATA: &str = "\
#define DELTA 60\n\
#define GRANTED 1\n\
#define FILTER NONE +COMMANDCENTER\n\
#define BUFF_FILTER ANY +INFANTRY -HERO\n\
Science SCIENCE_MEN\n  PrerequisiteSciences = None\n  SciencePurchasePointCost = 0\n  IsGrantable = No\nEnd\n\
Science SCIENCE_ANGMAR\n  PrerequisiteSciences = None\n  SciencePurchasePointCost = 0\n  IsGrantable = No\nEnd\n\
Science SCIENCE_Heal\n  PrerequisiteSciences = SCIENCE_MEN OR SCIENCE_ANGMAR\n  \
  SciencePurchasePointCost = 3\n  SciencePurchasePointCostMP = 5\nEnd\n\
Science SCIENCE_Rally\n  PrerequisiteSciences = SCIENCE_MEN\n  SciencePurchasePointCostMP = 5\nEnd\n\
Science SCIENCE_Summon\n  PrerequisiteSciences = SCIENCE_ANGMAR SCIENCE_Heal OR SCIENCE_MEN SCIENCE_Rally\n  \
  SciencePurchasePointCostMP = 10\nEnd\n\
Science SCIENCE_Gifts\n  PrerequisiteSciences = SCIENCE_MEN\n  SciencePurchasePointCostMP = 1\nEnd\n\
Rank 1\n  SkillPointsNeededDefault = 0\n  SkillPointsNeededCampaign = 0\n  SciencePurchasePointsGranted = 5\nEnd\n\
Rank 2\n  SkillPointsNeededDefault = #MULTIPLY( DELTA 1 )\n  SciencePurchasePointsGranted = GRANTED\nEnd\n\
Rank 3\n  SkillPointsNeededDefault = #MULTIPLY( DELTA 2 )\n  SciencePurchasePointsGranted = GRANTED\nEnd\n\
Rank 4\n  SkillPointsNeededDefault = #MULTIPLY( DELTA 3 )\n  SciencePurchasePointsGranted = 10\nEnd\n\
Upgrade Upgrade_Gift\n  Type = PLAYER\nEnd\n\
SpecialPower SpellBookHeal\n  ReloadTime = 2000\n  RequiredSciences = SCIENCE_Heal\nEnd\n\
SpecialPower SpellBookRally\n  ReloadTime = 2000\n  RequiredSciences = SCIENCE_RallySP SCIENCE_Rally\nEnd\n\
SpecialPower SpellBookSummon\n  ReloadTime = 2000\n  RequiredSciences = SCIENCE_Summon\nEnd\n\
SpecialPower SpellBookGifts\n  ReloadTime = 0\n  RequiredSciences = SCIENCE_Gifts\nEnd\n\
ModifierList RallyMod\n  Category = SPELL\n  Modifier = DAMAGE_MULT 150%\n  Duration = 1000\nEnd\n\
ModifierList FastRecharge\n  Category = SPELL\n  Modifier = RECHARGE_TIME 50%\n  Duration = 0\nEnd\n\
ObjectCreationList OCL_SummonEgg\n  CreateObject\n    ObjectNames = SummonEgg\n    \
  Disposition = LIKE_EXISTING\n  End\nEnd\n\
ObjectCreationList OCL_Summon\n  CreateObject\n    ObjectNames = Summoned\n    Count = 2\n    \
  Disposition = SPAWN_AROUND\n    DispositionAngle = 180\n    DispositionIntensity = 250\n  End\nEnd\n\
ObjectCreationList OCL_HealPing\n  CreateObject\n    ObjectNames = HealPing\n  End\nEnd\n\
Object HealPing\n  KindOf = INERT IMMOBILE UNATTACKABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\nEnd\n\
Object SummonEgg\n  KindOf = INERT IMMOBILE UNATTACKABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Behavior = LifetimeUpdate Tag_Hatch\n    MinLifetime = 0.0\n    MaxLifetime = 0.0\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Death\n    DestructionDelay = 1000\n    OCL = FINAL OCL_Summon\n  End\nEnd\n\
Object Summoned\n  Side = Men\n  KindOf = INFANTRY SELECTABLE SUMMONED\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = LifetimeUpdate Tag_Life\n    MinLifetime = 3000\n    MaxLifetime = 3000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object TestSpellBook\n  KindOf = SPELL_BOOK IMMOBILE IGNORES_SELECT_ALL INERT\n  CommandSet = SpellSet\n  \
  Behavior = PlayerHealSpecialPower Tag_Heal\n    SpecialPowerTemplate = SpellBookHeal\n    \
    HealAffects = INFANTRY\n    HealAmount = 0.5\n    HealRadius = 100\n    \
    HealFX = FX_HealTest\n    HealOCL = OCL_HealPing\n    \
    RequirementsFilterMPSkirmish = FILTER\n  End\n  \
  Behavior = SpecialPowerModule Tag_Rally\n    SpecialPowerTemplate = SpellBookRally\n    \
    AttributeModifier = RallyMod\n    AttributeModifierRange = 100\n    \
    AttributeModifierAffects = BUFF_FILTER\n  End\n  \
  Behavior = OCLSpecialPower Tag_Summon\n    SpecialPowerTemplate = SpellBookSummon\n    \
    OCL = OCL_SummonEgg\n    CreateLocation = CREATE_AT_LOCATION\n  End\n  \
  Behavior = PlayerUpgradeSpecialPower Tag_Gifts\n    SpecialPowerTemplate = SpellBookGifts\n    \
    UpgradeName = Upgrade_Gift\n  End\n  \
  Behavior = AttributeModifierUpgrade Tag_FastRecharge\n    TriggeredBy = Upgrade_Gift\n    \
    AttributeModifier = FastRecharge\n  End\n  \
  Behavior = OCLSpecialPower Tag_Edge\n    SpecialPowerTemplate = SpellBookEdge\n    \
    OCL = OCL_Marker\n    CreateLocation = CREATE_AT_EDGE_NEAR_TARGET\n  End\n  \
  Behavior = OCLSpecialPower Tag_Secondary\n    SpecialPowerTemplate = SpellBookSecondary\n    \
    OCL = OCL_Marker\n    CreateLocation = USE_SECONDARY_OBJECT_LOCATION\n    \
    NearestSecondaryObjectFilter = NONE SAME_PLAYER +CASTLE_KEEP\n  End\n  \
  Behavior = OCLSpecialPower Tag_StartReady\n    SpecialPowerTemplate = SpellBookStartReady\n    \
    AvailableAtStart = No\n    OCL = OCL_Marker\n    CreateLocation = CREATE_AT_LOCATION\n  End\nEnd\n\
SpecialPower SpellBookEdge\n  ReloadTime = 0\nEnd\n\
SpecialPower SpellBookSecondary\n  ReloadTime = 0\nEnd\n\
SpecialPower SpellBookStartReady\n  ReloadTime = 60000\n  RequiredSciences = SCIENCE_MEN\nEnd\n\
ObjectCreationList OCL_Marker\n  CreateObject\n    ObjectNames = Marker\n    \
  Disposition = LIKE_EXISTING\n    OrientInSecondaryDirection = Yes\n  End\nEnd\n\
Object Marker\n  KindOf = INERT IMMOBILE UNATTACKABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 2\n  GeometryHeight = 2\nEnd\n\
Object Castle\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE CASTLE_KEEP\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 40\nEnd\n\
CommandSet SpellSet\n  1 = Cast_Heal\n  2 = Cast_Rally\n  3 = Cast_Summon\n  4 = Cast_Gifts\n  \
  5 = Cast_Edge\n  6 = Cast_Secondary\nEnd\n\
CommandButton Cast_Heal\n  Command = SPELL_BOOK\n  SpecialPower = SpellBookHeal\n  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Cast_Rally\n  Command = SPELL_BOOK\n  SpecialPower = SpellBookRally\n  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Cast_Summon\n  Command = SPELL_BOOK\n  SpecialPower = SpellBookSummon\n  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Cast_Gifts\n  Command = SPELL_BOOK\n  SpecialPower = SpellBookGifts\n  Options = NONPRESSABLE\nEnd\n\
CommandButton Cast_Edge\n  Command = SPELL_BOOK\n  SpecialPower = SpellBookEdge\n  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Cast_Secondary\n  Command = SPELL_BOOK\n  SpecialPower = SpellBookSecondary\n  Options = NEED_TARGET_POS\nEnd\n\
CommandSet StoreSet\n  1 = Buy_Heal\n  2 = Buy_Rally\n  3 = Buy_Summon\n  4 = Buy_Gifts\nEnd\n\
CommandButton Buy_Heal\n  Command = PURCHASE_SCIENCE\n  Science = SCIENCE_Heal\nEnd\n\
CommandButton Buy_Rally\n  Command = PURCHASE_SCIENCE\n  Science = SCIENCE_Rally\nEnd\n\
CommandButton Buy_Summon\n  Command = PURCHASE_SCIENCE\n  Science = SCIENCE_Summon\nEnd\n\
CommandButton Buy_Gifts\n  Command = PURCHASE_SCIENCE\n  Science = SCIENCE_Gifts\nEnd\n\
Armor Plain\n  Armor = DEFAULT 100%\nEnd\n\
Weapon Club\n  AttackRange = 40\n  DelayBetweenShots = 100\n  \
  DamageNugget\n    Damage = 1000\n    DamageType = CRUSH\n  End\nEnd\n\
Weapon Needle\n  AttackRange = 40\n  DelayBetweenShots = 1000000\n  \
  DamageNugget\n    Damage = 90\n    DamageType = PIERCE\n  End\nEnd\n\
Object Stinger\n  Side = Men\n  KindOf = CAVALRY SELECTABLE CANNOT_RETALIATE\n  BuildCost = 50\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Needle\n  End\n  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ExperienceLevel SoldierLevel1\n  TargetNames = Soldier\n  RequiredExperience = 1\n  \
  ExperienceAward = 30\n  Rank = 1\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 50\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Troll\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 50\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Club\n  End\n  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Fortress\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE COMMANDCENTER\n  \
  BuildCost = 1000\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 5000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 40\nEnd\n\
PlayerTemplate FactionTest\n  Side = Men\n  IntrinsicSciencesMP = SCIENCE_MEN\n  \
  SpellBookMp = TestSpellBook\n  PurchaseScienceCommandSetMP = StoreSet\n  \
  MaxLevelMP = #DIVIDE( 150 GRANTED )\nEnd\n";

const HEAL: u8 = 0;
const RALLY: u8 = 1;
const SUMMON: u8 = 2;
const GIFTS: u8 = 3;
const EDGE: u8 = 4;
const SECONDARY: u8 = 5;

struct Spells {
    data: GameData,
    kinds: Arc<KindTable>,
}

fn load() -> Spells {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = [
        "TestSpellBook",
        "Soldier",
        "Troll",
        "Stinger",
        "Fortress",
        "Castle",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    assert!(
        kinds.kind("SummonEgg").is_some() && kinds.kind("Summoned").is_some(),
        "the closure follows the summon's creation lists"
    );
    Spells {
        data,
        kinds: Arc::new(kinds),
    }
}

fn simulation(s: &Spells, placed: &[(&str, f32, f32, PlayerId)]) -> (Simulation<RotwkRules>, Ids) {
    simulation_with(s, placed, false, None)
}

fn simulation_with(
    s: &Spells,
    placed: &[(&str, f32, f32, PlayerId)],
    ai: bool,
    teams: Option<Vec<Option<u8>>>,
) -> (Simulation<RotwkRules>, Ids) {
    let production = s.kinds.production(&s.data);
    let power_names = crate::powers::PowerNames::build(s.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&s.data.special_powers);
    let faction = s.data.players.get("FactionTest").unwrap().clone();
    let starts: Vec<crate::skirmish::PlayerStart> = [PLAYER, OTHER]
        .into_iter()
        .map(|player| crate::skirmish::PlayerStart {
            player,
            faction: faction.clone(),
            at: Vec2::ZERO,
            castle: None,
            ai_base_candidates: Vec::new(),
        })
        .collect();
    let (sciences, ranks, plans) = crate::skirmish::spell_setup(
        &s.data,
        &s.kinds,
        &starts,
        crate::game_mode::GameMode::Skirmish,
        false,
    );
    let power_sciences = power_names.required_sciences(&s.data.special_powers, &sciences);
    let spawns: Vec<Spawn> = placed
        .iter()
        .map(|(name, x, y, owner)| {
            let kind = s.kinds.kind(name).unwrap();
            s.kinds
                .spawn(
                    &s.data.combat,
                    kind,
                    Vec2::new(*x, *y),
                    Facing::from_radians(1.0),
                    *owner,
                )
                .unwrap()
        })
        .collect();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let mut rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_hordes(s.data.formations.tables().to_vec(), Vec::new())
        .with_combat(s.data.combat.clone())
        .with_construction(Arc::clone(&s.kinds), production.clone())
        .with_economy(production.buildables.clone(), 1_000)
        .with_upgrades(s.kinds.upgrades(&s.data), Arc::clone(&s.data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_experience(Arc::clone(&s.data.experience), s.data.armor_bonus_cap)
        .with_spell_books(sciences, ranks, plans, power_sciences)
        .with_creation_lists(Arc::clone(&s.data.ocls));
    if let Some(teams) = teams {
        let players = u8::try_from(teams.len()).unwrap();
        rules = rules.with_players(players).with_teams(teams);
    }
    if ai {
        rules = rules.with_skirmish_ai(
            vec![crate::skirmish_ai::AiSeed {
                player: PLAYER,
                candidates: Vec::new(),
            }],
            crate::combat::AiTiming::default(),
            Vec::new(),
        );
    }
    let simulation = Simulation::new(3, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

fn stung(soldiers: &[(f32, f32, PlayerId)]) -> Vec<(&'static str, f32, f32, PlayerId)> {
    let mut out = Vec::new();
    for &(x, y, owner) in soldiers {
        let other = if owner == PLAYER { OTHER } else { PLAYER };
        out.push(("Soldier", x, y, owner));
        out.push(("Stinger", x + 12.0, y, other));
    }
    out
}

const STING: u32 = 45;

fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

fn book(simulation: &Simulation<RotwkRules>, player: PlayerId) -> crate::sciences::SpellBook {
    simulation
        .rules()
        .spell_book(simulation.state(), player)
        .unwrap()
}

fn entry(book: &crate::sciences::SpellBook, science: &str) -> SpellState {
    book.entries
        .iter()
        .find(|entry| entry.science == science)
        .unwrap()
        .state
}

fn buy(simulation: &mut Simulation<RotwkRules>, player: PlayerId, science: &str) {
    let command = simulation.rules().purchase_command(science).unwrap();
    simulation.submit(player, command).unwrap();
    run(simulation, 1);
}

fn cast(simulation: &mut Simulation<RotwkRules>, player: PlayerId, button: u8, at: (f32, f32)) {
    simulation
        .submit(
            player,
            RotwkCommand::CastSpell {
                button,
                target: None,
                at: Some(at),
            },
        )
        .unwrap();
    run(simulation, 1);
}

fn placed(s: &Spells, simulation: &Simulation<RotwkRules>, ids: Ids, name: &str) -> Vec<Entity> {
    let kind = s.kinds.kind(name).unwrap();
    simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(entity, k)| {
            **k == kind
                && simulation.state().is_live(*entity)
                && !simulation.state().contains(ids.doomed, *entity)
        })
        .map(|(entity, _)| entity)
        .collect()
}

fn health(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.health, entity)
        .unwrap()
        .0
        .current()
}

#[test]
fn a_player_starts_with_rank_one_its_points_and_its_spell_book() {
    let s = load();
    let (simulation, ids) = simulation(&s, &[]);
    let book = book(&simulation, PLAYER);
    assert_eq!((book.rank, book.points, book.skill_points), (1, 5, 0));
    assert_eq!(book.next_rank_at, Some(60));
    assert_eq!(
        book.entries.len(),
        4,
        "the store's four buttons, page order"
    );
    assert_eq!(entry(&book, "SCIENCE_Heal"), SpellState::Available);
    assert_eq!(entry(&book, "SCIENCE_Summon"), SpellState::Locked);
    let state = simulation
        .rules()
        .science_of(simulation.state(), ids, PLAYER)
        .unwrap();
    assert_eq!(state.rank_cap, 4, "MaxLevelMP's 150 bounded by four ranks");
    let spell_book = state.book.unwrap();
    assert_eq!(
        simulation.state().get(ids.kind, spell_book).copied(),
        s.kinds.kind("TestSpellBook")
    );
    assert!(
        simulation.state().get(ids.body, spell_book).is_none(),
        "never drawn, selected or targeted"
    );
}

#[test]
fn kills_give_skill_points_and_a_rank_grants_power_points() {
    let s = load();
    let (mut simulation, _ids) = simulation(
        &s,
        &[
            ("Troll", 100.0, 100.0, PLAYER),
            ("Soldier", 120.0, 100.0, OTHER),
            ("Soldier", 100.0, 120.0, OTHER),
        ],
    );
    run(&mut simulation, 60);
    let book = book(&simulation, PLAYER);
    assert_eq!(book.skill_points, 60, "two kills worth 30 each");
    assert_eq!((book.rank, book.points), (2, 6));
    let loser = book_of(&simulation, OTHER);
    assert_eq!(loser.skill_points, 0, "losing units gives nothing");
}

#[test]
fn debug_spell_points_raises_purchase_points_only() {
    let s = load();
    let (mut simulation, _ids) = simulation(&s, &[]);
    let before = book(&simulation, PLAYER);
    simulation
        .submit(PLAYER, RotwkCommand::DebugSpellPoints { points: 5 })
        .unwrap();
    run(&mut simulation, 1);
    let after = book(&simulation, PLAYER);
    assert_eq!(after.points, before.points + 5);
    assert_eq!(after.skill_points, before.skill_points);
    assert_eq!(after.rank, before.rank);
}

fn book_of(simulation: &Simulation<RotwkRules>, player: PlayerId) -> crate::sciences::SpellBook {
    book(simulation, player)
}

#[test]
fn a_purchase_needs_points_and_one_prerequisite_branch_and_happens_once() {
    let s = load();
    let (mut simulation, _ids) = simulation(&s, &[]);
    buy(&mut simulation, PLAYER, "SCIENCE_Summon");
    assert_eq!(book(&simulation, PLAYER).points, 5, "locked: nothing paid");
    buy(&mut simulation, PLAYER, "SCIENCE_Rally");
    let after = book(&simulation, PLAYER);
    assert_eq!(after.points, 0);
    assert_eq!(entry(&after, "SCIENCE_Rally"), SpellState::Owned);
    assert_eq!(
        entry(&after, "SCIENCE_Summon"),
        SpellState::CantAfford,
        "MEN + Rally is one whole branch; ANGMAR + Heal need not be held"
    );
    buy(&mut simulation, PLAYER, "SCIENCE_Heal");
    assert_eq!(
        entry(&book(&simulation, PLAYER), "SCIENCE_Heal"),
        SpellState::CantAfford
    );
    simulation
        .submit(PLAYER, RotwkCommand::DebugSkillPoints { points: 180 })
        .unwrap();
    run(&mut simulation, 1);
    let ranked = book(&simulation, PLAYER);
    assert_eq!(
        (ranked.rank, ranked.points),
        (4, 12),
        "ranks 2-4: 1 + 1 + 10"
    );
    buy(&mut simulation, PLAYER, "SCIENCE_Summon");
    buy(&mut simulation, PLAYER, "SCIENCE_Summon");
    let bought = book(&simulation, PLAYER);
    assert_eq!(entry(&bought, "SCIENCE_Summon"), SpellState::Owned);
    assert_eq!(bought.points, 2, "paid once");
}

#[test]
fn refused_purchases_and_casts_leave_the_state_hash_unchanged() {
    let s = load();
    let (mut with, _) = simulation(&s, &[("Soldier", 100.0, 100.0, PLAYER)]);
    let (mut without, _) = simulation(&s, &[("Soldier", 100.0, 100.0, PLAYER)]);
    for command in [
        RotwkCommand::PurchaseScience { science: 999 },
        with.rules().purchase_command("SCIENCE_Summon").unwrap(),
        with.rules().purchase_command("SCIENCE_MEN").unwrap(),
        RotwkCommand::CastSpell {
            button: HEAL,
            target: None,
            at: Some((100.0, 100.0)),
        },
        RotwkCommand::CastSpell {
            button: 77,
            target: None,
            at: Some((100.0, 100.0)),
        },
        RotwkCommand::CastSpell {
            button: GIFTS,
            target: None,
            at: None,
        },
        RotwkCommand::Sell,
    ] {
        with.submit(PLAYER, command).unwrap();
    }
    run(&mut with, 5);
    run(&mut without, 5);
    assert_eq!(with.state_hash(), without.state_hash());
}

#[test]
fn the_heal_needs_its_science_heals_its_circle_and_recharges() {
    let s = load();
    let mut placed_units = stung(&[
        (100.0, 100.0, PLAYER),
        (400.0, 400.0, PLAYER),
        (100.0, 150.0, OTHER),
    ]);
    placed_units.push(("Fortress", 700.0, 700.0, PLAYER));
    let (mut simulation, ids) = simulation(&s, &placed_units);
    let soldiers = placed(&s, &simulation, ids, "Soldier");
    run(&mut simulation, STING);
    for &soldier in &soldiers {
        assert_eq!(health(&simulation, ids, soldier), 10.0, "stung once");
    }
    cast(&mut simulation, PLAYER, HEAL, (100.0, 100.0));
    assert_eq!(
        health(&simulation, ids, soldiers[0]),
        10.0,
        "no science: refused"
    );
    buy(&mut simulation, PLAYER, "SCIENCE_Heal");
    cast(&mut simulation, PLAYER, HEAL, (100.0, 100.0));
    assert_eq!(
        health(&simulation, ids, soldiers[0]),
        60.0,
        "half of 100 more"
    );
    assert_eq!(
        health(&simulation, ids, soldiers[1]),
        10.0,
        "outside the circle"
    );
    assert_eq!(
        health(&simulation, ids, soldiers[2]),
        10.0,
        "not the caster's"
    );
    cast(&mut simulation, PLAYER, HEAL, (100.0, 100.0));
    assert_eq!(
        health(&simulation, ids, soldiers[0]),
        60.0,
        "recharging: refused"
    );
    run(&mut simulation, 61);
    cast(&mut simulation, PLAYER, HEAL, (100.0, 100.0));
    assert_eq!(
        health(&simulation, ids, soldiers[0]),
        100.0,
        "ready again, capped at max"
    );
}

#[test]
fn the_heal_marks_every_admitted_object_and_runs_its_creation_list_once() {
    let s = load();
    let mut placed_units = stung(&[(100.0, 100.0, PLAYER)]);
    placed_units.push(("Soldier", 130.0, 100.0, PLAYER));
    placed_units.push(("Fortress", 700.0, 700.0, PLAYER));
    let (mut simulation, ids) = simulation(&s, &placed_units);
    let soldiers = placed(&s, &simulation, ids, "Soldier");
    run(&mut simulation, STING);
    assert_eq!(health(&simulation, ids, soldiers[0]), 10.0, "stung once");
    assert_eq!(health(&simulation, ids, soldiers[1]), 100.0, "whole");
    buy(&mut simulation, PLAYER, "SCIENCE_Heal");
    simulation
        .submit(
            PLAYER,
            RotwkCommand::CastSpell {
                button: HEAL,
                target: None,
                at: Some((100.0, 100.0)),
            },
        )
        .unwrap();
    let frame = simulation.tick().unwrap().frame;
    let marks = simulation
        .rules()
        .fx_events(simulation.state(), frame)
        .iter()
        .filter(|event| simulation.rules().fx_name(event.list) == Some("FX_HealTest"))
        .count();
    assert_eq!(health(&simulation, ids, soldiers[0]), 60.0, "healed");
    assert_eq!(
        marks, 2,
        "the whole soldier is marked as well as the hurt one"
    );
    assert_eq!(placed(&s, &simulation, ids, "HealPing").len(), 1);
}

#[test]
fn the_heal_heals_and_marks_an_allys_unit() {
    let s = load();
    let ally = PlayerId::new(2);
    let mut placed_units = stung(&[(100.0, 100.0, PLAYER)]);
    placed_units.push(("Soldier", 130.0, 100.0, ally));
    placed_units.push(("Stinger", 142.0, 100.0, OTHER));
    placed_units.push(("Fortress", 700.0, 700.0, PLAYER));
    let (mut simulation, ids) =
        simulation_with(&s, &placed_units, false, Some(vec![Some(0), None, Some(0)]));
    let soldiers = placed(&s, &simulation, ids, "Soldier");
    let allied = soldiers
        .iter()
        .copied()
        .find(|soldier| simulation.state().get(ids.owner, *soldier).map(|o| o.0) == Some(ally))
        .unwrap();
    run(&mut simulation, STING);
    assert_eq!(health(&simulation, ids, allied), 10.0, "stung once");
    buy(&mut simulation, PLAYER, "SCIENCE_Heal");
    simulation
        .submit(
            PLAYER,
            RotwkCommand::CastSpell {
                button: HEAL,
                target: None,
                at: Some((100.0, 100.0)),
            },
        )
        .unwrap();
    let frame = simulation.tick().unwrap().frame;
    let marks = simulation
        .rules()
        .fx_events(simulation.state(), frame)
        .iter()
        .filter(|event| simulation.rules().fx_name(event.list) == Some("FX_HealTest"))
        .count();
    assert_eq!(
        health(&simulation, ids, allied),
        60.0,
        "the ally's soldier is healed"
    );
    assert_eq!(
        marks, 2,
        "the ally's soldier is marked as well as the caster's"
    );
}

#[test]
fn the_spell_row_percent_follows_the_scaled_reload() {
    let s = load();
    let (mut simulation, _ids) = simulation(&s, &[("Fortress", 600.0, 600.0, PLAYER)]);
    simulation
        .submit(PLAYER, RotwkCommand::DebugSkillPoints { points: 60 })
        .unwrap();
    run(&mut simulation, 1);
    buy(&mut simulation, PLAYER, "SCIENCE_Heal");
    buy(&mut simulation, PLAYER, "SCIENCE_Gifts");
    cast(&mut simulation, PLAYER, HEAL, (600.0, 600.0));
    run(&mut simulation, 15);
    let now = simulation.frame();
    let (readiness, _) = simulation
        .rules()
        .spell_bar_state(simulation.state(), PLAYER, HEAL, now)
        .expect("still held");
    let SpellBarReadiness::OnCooldown { percent } = readiness else {
        panic!("halfway through a 30-frame reload: still on cooldown, {readiness:?}");
    };
    assert!((40..=60).contains(&percent), "{percent}");
}

#[test]
fn a_spell_book_spell_needs_what_its_requirement_filter_names() {
    let s = load();
    let (mut simulation, ids) = simulation(&s, &stung(&[(100.0, 100.0, PLAYER)]));
    let soldier = placed(&s, &simulation, ids, "Soldier")[0];
    run(&mut simulation, STING);
    buy(&mut simulation, PLAYER, "SCIENCE_Heal");
    assert!(!simulation
        .rules()
        .spell_ready(simulation.state(), PLAYER, HEAL, Frame::new(1_000)));
    cast(&mut simulation, PLAYER, HEAL, (100.0, 100.0));
    assert_eq!(health(&simulation, ids, soldier), 10.0, "no COMMANDCENTER");
}
#[test]
fn a_summon_lays_an_egg_that_hatches_its_summons_which_expire() {
    let s = load();
    let (mut simulation, ids) = simulation(&s, &[]);
    simulation
        .submit(PLAYER, RotwkCommand::DebugSkillPoints { points: 180 })
        .unwrap();
    buy(&mut simulation, PLAYER, "SCIENCE_Rally");
    buy(&mut simulation, PLAYER, "SCIENCE_Summon");
    cast(&mut simulation, PLAYER, SUMMON, (300.0, 300.0));
    let eggs = placed(&s, &simulation, ids, "SummonEgg");
    let dying_eggs = simulation.state().store(ids.doomed).map_or(0, |d| d.len());
    assert!(
        eggs.len() + dying_eggs >= 1,
        "the egg was laid ({} live, {} dying)",
        eggs.len(),
        dying_eggs
    );
    run(&mut simulation, 35);
    let summons = placed(&s, &simulation, ids, "Summoned");
    assert_eq!(summons.len(), 2, "the FINAL list ran once");
    let mut xs: Vec<f32> = summons
        .iter()
        .map(|entity| {
            simulation
                .state()
                .get(ids.body, *entity)
                .unwrap()
                .0
                .position
                .x
        })
        .collect();
    xs.sort_by(f32::total_cmp);
    assert!(
        (xs[0] - 250.0).abs() < 0.01 && (xs[1] - 350.0).abs() < 0.01,
        "{xs:?}"
    );
    for summon in &summons {
        assert_eq!(
            simulation.state().get(ids.owner, *summon).map(|o| o.0),
            Some(PLAYER)
        );
    }
    assert!(
        placed(&s, &simulation, ids, "SummonEgg").is_empty(),
        "the egg is gone"
    );
    run(&mut simulation, 95);
    assert!(
        placed(&s, &simulation, ids, "Summoned").is_empty(),
        "their LifetimeUpdate killed them"
    );
}

#[test]
fn a_rally_gives_its_timed_list_to_friends_in_range_and_it_ends() {
    let s = load();
    let (mut simulation, ids) = simulation(
        &s,
        &[
            ("Soldier", 100.0, 100.0, PLAYER),
            ("Soldier", 105.0, 100.0, OTHER),
            ("Soldier", 500.0, 500.0, PLAYER),
        ],
    );
    let soldiers = placed(&s, &simulation, ids, "Soldier");
    buy(&mut simulation, PLAYER, "SCIENCE_Rally");
    cast(&mut simulation, PLAYER, RALLY, (100.0, 100.0));
    let damage_mult = |simulation: &Simulation<RotwkRules>, entity: Entity| {
        simulation
            .state()
            .get(ids.bonuses, entity)
            .map_or(1.0, |bonuses| bonuses.0.damage_mult)
    };
    assert!((damage_mult(&simulation, soldiers[0]) - 1.5).abs() < 1e-6);
    assert_eq!(damage_mult(&simulation, soldiers[1]), 1.0, "an enemy");
    assert_eq!(damage_mult(&simulation, soldiers[2]), 1.0, "out of range");
    run(&mut simulation, 31);
    assert_eq!(
        damage_mult(&simulation, soldiers[0]),
        1.0,
        "Duration 1000 ended"
    );
    assert!(simulation
        .state()
        .get(ids.timed_modifiers, soldiers[0])
        .is_none());
}

#[test]
fn a_passive_spell_takes_effect_when_bought_and_is_never_cast() {
    let s = load();
    let (mut simulation, ids) = simulation(&s, &[]);
    let gift = s.data.upgrade_bits.bit("Upgrade_Gift").unwrap();
    let holds = |simulation: &Simulation<RotwkRules>| {
        let player =
            crate::economy::player_entity(simulation.state(), RotwkRules::economy_ids(ids), PLAYER)
                .unwrap();
        simulation
            .state()
            .get(ids.player_upgrades, player)
            .is_some_and(|mask| mask.0.contains(gift))
    };
    simulation
        .submit(
            PLAYER,
            RotwkCommand::CastSpell {
                button: GIFTS,
                target: None,
                at: None,
            },
        )
        .unwrap();
    run(&mut simulation, 1);
    assert!(!holds(&simulation));
    buy(&mut simulation, PLAYER, "SCIENCE_Gifts");
    assert!(holds(&simulation), "PlayerUpgradeSpecialPower granted it");
}

#[test]
fn an_intrinsic_science_held_at_start_makes_its_available_at_start_no_power_ready() {
    let s = load();
    let (simulation, ids) = simulation(&s, &[]);
    let science = simulation
        .rules()
        .science_of(simulation.state(), ids, PLAYER)
        .unwrap();
    let book = simulation
        .state()
        .get(ids.power_book, science.book.unwrap())
        .unwrap();
    let instance = book.0.last().unwrap();
    assert_eq!(
        instance.available_on,
        Frame::ZERO,
        "an intrinsic science's AvailableAtStart = No power must be ready at spawn"
    );
}

#[test]
fn the_ai_buys_its_first_spell_and_casts_it_where_the_enemy_is() {
    let s = load();
    let mut units = vec![("Soldier", 700.0, 100.0, PLAYER)];
    units.extend(stung(&[(100.0, 100.0, PLAYER)]));
    units.push(("Fortress", 700.0, 700.0, PLAYER));
    let (mut simulation, ids) = simulation_with(&s, &units, true, None);
    let soldier = placed(&s, &simulation, ids, "Soldier")[1];
    run(&mut simulation, 91);
    let after = book(&simulation, PLAYER);
    assert_eq!(
        entry(&after, "SCIENCE_Heal"),
        SpellState::Owned,
        "the first page entry it could buy"
    );
    assert_eq!(after.points, 0);
    assert_eq!(
        health(&simulation, ids, soldier),
        60.0,
        "stung to 10, then healed where the enemy stood"
    );
}
#[test]
fn a_match_with_purchases_casts_and_summons_replays_identically() {
    let run_once = || {
        let s = load();
        let (mut simulation, _ids) = simulation(
            &s,
            &[
                ("Soldier", 100.0, 100.0, PLAYER),
                ("Fortress", 600.0, 600.0, PLAYER),
            ],
        );
        simulation
            .submit(PLAYER, RotwkCommand::DebugSkillPoints { points: 180 })
            .unwrap();
        let mut hashes = Vec::new();
        for (science, button) in [("SCIENCE_Heal", HEAL), ("SCIENCE_Rally", RALLY)] {
            buy(&mut simulation, PLAYER, science);
            cast(&mut simulation, PLAYER, button, (100.0, 100.0));
            hashes.push(simulation.state_hash());
        }
        buy(&mut simulation, PLAYER, "SCIENCE_Summon");
        cast(&mut simulation, PLAYER, SUMMON, (200.0, 200.0));
        for _ in 0..60 {
            simulation.tick().unwrap();
            hashes.push(simulation.state_hash());
        }
        hashes
    };
    let first = run_once();
    let second = run_once();
    assert_eq!(first, second);
    assert!(first.iter().any(|hash| *hash != first[0]));
}

#[test]
fn the_screen_offers_exactly_what_a_purchase_accepts() {
    let s = load();
    let (mut simulation, _ids) = simulation(
        &s,
        &[
            ("Soldier", 100.0, 100.0, PLAYER),
            ("Fortress", 600.0, 600.0, PLAYER),
        ],
    );
    simulation
        .submit(PLAYER, RotwkCommand::DebugSkillPoints { points: 60 })
        .unwrap();
    run(&mut simulation, 1);
    let mut seen = Vec::new();
    let store = book(&simulation, PLAYER).entries.len();
    for at in (0..store).cycle().take(3 * store) {
        let holdings = crate::spell_book::holdings_of(
            simulation
                .rules()
                .spell_book(simulation.state(), PLAYER)
                .as_ref(),
        );
        if let Some((science, shown)) = holdings.states.get(at).cloned() {
            let before = book(&simulation, PLAYER).points;
            let command =
                crate::spell_book::purchase_clicked(simulation.rules(), &science).unwrap();
            simulation.submit(PLAYER, command).unwrap();
            run(&mut simulation, 1);
            let bought = book(&simulation, PLAYER).points < before;
            assert_eq!(
                bought,
                shown == SpellState::Available,
                "{science} {shown:?}"
            );
            seen.push(shown);
        }
    }
    assert!(seen.contains(&SpellState::Available));
    assert!(seen.contains(&SpellState::Locked) || seen.contains(&SpellState::CantAfford));
    assert!(crate::spell_book::purchase_clicked(simulation.rules(), "SCIENCE_Nowhere").is_none());
}

#[test]
fn a_provisional_purchase_is_shown_bought_and_changes_nothing() {
    let s = load();
    let (mut simulation, _ids) = simulation(&s, &[("Fortress", 600.0, 600.0, PLAYER)]);
    simulation
        .submit(PLAYER, RotwkCommand::DebugSkillPoints { points: 180 })
        .unwrap();
    run(&mut simulation, 1);
    let committed = book(&simulation, PLAYER);
    assert!(committed.points >= 15, "{}", committed.points);
    let hash = simulation.state_hash();
    let shown = |provisional: &[&str]| {
        let provisional: Vec<String> = provisional.iter().map(|s| (*s).to_owned()).collect();
        simulation
            .rules()
            .provisional_spell_book(simulation.state(), PLAYER, &provisional)
            .unwrap()
    };
    let state = |book: &crate::sciences::SpellBook, science: &str| {
        book.entries
            .iter()
            .find(|entry| entry.science == science)
            .unwrap()
            .state
    };
    assert_eq!(state(&committed, "SCIENCE_Summon"), SpellState::Locked);
    let both = shown(&["SCIENCE_Rally", "SCIENCE_Summon"]);
    assert_eq!(state(&both, "SCIENCE_Rally"), SpellState::Owned);
    assert_eq!(state(&both, "SCIENCE_Summon"), SpellState::Owned);
    assert_eq!(both.points, committed.points - 15);
    let reversed = shown(&["SCIENCE_Summon", "SCIENCE_Rally"]);
    assert_eq!(state(&reversed, "SCIENCE_Summon"), SpellState::Available);
    assert_eq!(reversed.points, committed.points - 5);
    assert_eq!(shown(&[]), committed);
    assert_eq!(simulation.state_hash(), hash);
}

#[test]
fn the_power_bar_shows_only_held_powers_and_agrees_with_a_press() {
    let s = load();
    let (mut simulation, _ids) = simulation(&s, &[("Fortress", 600.0, 600.0, PLAYER)]);
    simulation
        .submit(PLAYER, RotwkCommand::DebugSkillPoints { points: 60 })
        .unwrap();
    run(&mut simulation, 1);
    let now = simulation.frame();
    assert_eq!(
        simulation
            .rules()
            .spell_bar_state(simulation.state(), PLAYER, HEAL, now),
        None,
        "not bought yet: no slot at all"
    );
    buy(&mut simulation, PLAYER, "SCIENCE_Heal");
    let now = simulation.frame();
    assert_eq!(
        simulation
            .rules()
            .spell_bar_state(simulation.state(), PLAYER, HEAL, now),
        Some((SpellBarReadiness::Ready, "Cast_Heal".to_owned())),
        "bought, off cooldown, a live COMMANDCENTER: castable"
    );
    assert_eq!(
        simulation
            .rules()
            .spell_bar_state(simulation.state(), PLAYER, RALLY, now),
        None,
        "Rally was never bought"
    );
    cast(&mut simulation, PLAYER, HEAL, (600.0, 600.0));
    let now = simulation.frame();
    let (readiness, button) = simulation
        .rules()
        .spell_bar_state(simulation.state(), PLAYER, HEAL, now)
        .expect("still held");
    let SpellBarReadiness::OnCooldown { percent } = readiness else {
        panic!("just cast: on cooldown, {readiness:?}");
    };
    assert!(percent < 100, "{percent}");
    assert_eq!(button, "Cast_Heal");
    assert_eq!(
        simulation
            .rules()
            .spell_ready(simulation.state(), PLAYER, HEAL, now),
        matches!(readiness, SpellBarReadiness::Ready),
        "the row and a press read the same gate"
    );
}

#[test]
fn the_rank_progress_ring_resets_at_each_rank_and_climbs_to_the_next() {
    let s = load();
    let (mut simulation, _ids) = simulation(&s, &[]);
    assert_eq!(
        simulation.rules().rank_progress(simulation.state(), PLAYER),
        0.0,
        "rank 1, no points yet"
    );
    simulation
        .submit(PLAYER, RotwkCommand::DebugSkillPoints { points: 30 })
        .unwrap();
    run(&mut simulation, 1);
    assert!(
        (simulation.rules().rank_progress(simulation.state(), PLAYER) - 0.5).abs() < 1e-6,
        "halfway to rank 2's 60"
    );
    simulation
        .submit(PLAYER, RotwkCommand::DebugSkillPoints { points: 30 })
        .unwrap();
    run(&mut simulation, 1);
    assert_eq!(
        book(&simulation, PLAYER).rank,
        2,
        "60 points reaches rank 2"
    );
    assert_eq!(
        simulation.rules().rank_progress(simulation.state(), PLAYER),
        0.0,
        "just reached rank 2: reset, not the old fraction"
    );
}

#[test]
fn an_edge_create_location_lands_the_marker_on_the_nearest_map_edge() {
    let s = load();
    let (mut simulation, ids) = simulation(&s, &[]);
    let high = simulation.rules().passability.playable_high;
    cast(&mut simulation, PLAYER, EDGE, (high.x - 10.0, high.y * 0.5));
    let markers = placed(&s, &simulation, ids, "Marker");
    assert_eq!(markers.len(), 1, "one marker from CreateObject's one Count");
    let at = position(&simulation, ids, markers[0]);
    assert!(
        (at.x - high.x).abs() < 0.01 && (at.y - high.y * 0.5).abs() < 0.01,
        "closest edge is the +x boundary, held at the target's own y: {at:?}, high {high:?}"
    );
}

#[test]
fn a_secondary_object_create_location_faces_the_marker_toward_the_nearest_matching_object() {
    let s = load();
    let (mut simulation, ids) = simulation(
        &s,
        &[
            ("Castle", 100.0, 100.0, PLAYER),
            ("Castle", 500.0, 500.0, PLAYER),
            ("Castle", 530.0, 510.0, OTHER),
        ],
    );
    cast(&mut simulation, PLAYER, SECONDARY, (520.0, 520.0));
    let markers = placed(&s, &simulation, ids, "Marker");
    assert_eq!(markers.len(), 1);
    let body = simulation.state().get(ids.body, markers[0]).unwrap().0;
    assert!(
        (body.position.x - 520.0).abs() < 0.01 && (body.position.y - 520.0).abs() < 0.01,
        "made at the target: {:?}",
        body.position
    );
    let toward = body.facing.radians().to_degrees();
    assert!(
        (toward + 135.0).abs() < 0.5,
        "toward the keep at (500, 500), not the enemy's at (530, 510): {toward}"
    );
}

#[test]
fn a_power_creation_without_a_secondary_has_creation_angle_zero() {
    let s = load();
    let (mut simulation, ids) = simulation(&s, &[("Castle", 100.0, 100.0, OTHER)]);
    cast(&mut simulation, PLAYER, SECONDARY, (520.0, 520.0));
    let markers = placed(&s, &simulation, ids, "Marker");
    assert_eq!(markers.len(), 1);
    let body = simulation.state().get(ids.body, markers[0]).unwrap().0;
    assert!(
        (body.position.x - 520.0).abs() < 0.01,
        "made at the target: {:?}",
        body.position
    );
    assert_eq!(body.facing.radians(), 0.0);
}

#[test]
fn a_bought_passive_power_takes_a_row_as_static() {
    let s = load();
    let (mut simulation, _ids) = simulation(&s, &[("Castle", 100.0, 100.0, PLAYER)]);
    simulation
        .submit(PLAYER, RotwkCommand::DebugSkillPoints { points: 60 })
        .unwrap();
    run(&mut simulation, 1);
    let bar = |simulation: &Simulation<RotwkRules>| {
        simulation
            .rules()
            .spell_bar_state(simulation.state(), PLAYER, GIFTS, simulation.frame())
    };
    assert_eq!(bar(&simulation), None);
    buy(&mut simulation, PLAYER, "SCIENCE_Gifts");
    assert_eq!(
        bar(&simulation),
        Some((SpellBarReadiness::Static, "Cast_Gifts".to_owned()))
    );
}
