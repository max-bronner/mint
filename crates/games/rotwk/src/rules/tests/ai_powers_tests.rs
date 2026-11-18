use super::*;
use crate::combat::GameData;
use crate::unit::Kind;
use mint_sage_ini::Stance;

const DATA: &str = "\
StanceTemplate PlainStances\n  Stance Aggressive\n    AttributeModifier StanceMod\n  End\nEnd\n\
ModifierList StanceMod\n  Category = SPELL\n  Modifier = VISION 100%\nEnd\n\
CommandButton Command_SetStanceAggressive\n  Command = SET_STANCE\n  Stances = Aggressive\nEnd\n\
Object Warrior\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = StancesBehavior Tag_S\n    StanceTemplate = PlainStances\n  End\n  \
  Behavior = AISpecialPowerUpdate Tag_AI\n    CommandButtonName = Command_SetStanceAggressive\n    \
    SpecialPowerAIType = AI_SPECIAL_POWER_STANCEAGGRESSIVE\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

struct Match {
    data: GameData,
    kinds: std::sync::Arc<KindTable>,
}

fn load() -> Match {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Warrior"].into_iter().map(str::to_owned).collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    Match {
        data,
        kinds: std::sync::Arc::new(kinds),
    }
}

fn simulation(m: &Match, owner: PlayerId, computer: bool) -> Simulation<RotwkRules> {
    let kind = m.kinds.kind("Warrior").unwrap();
    let spawn = m
        .kinds
        .spawn(
            &m.data.combat,
            kind,
            Vec2::new(100.0, 100.0),
            Facing::EAST,
            owner,
        )
        .unwrap();
    let map = flat_map(32, 32);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let production = m.kinds.production(&m.data);
    let mut rules = RotwkRules::new(passability, vec![spawn])
        .with_combat(m.data.combat.clone())
        .with_construction(std::sync::Arc::clone(&m.kinds), production)
        .with_ai_power_buttons(std::sync::Arc::clone(&m.data.command_buttons));
    if computer {
        rules = rules.with_skirmish_ai(
            vec![crate::skirmish_ai::AiSeed {
                player: owner,
                candidates: Vec::new(),
            }],
            crate::combat::AiTiming::default(),
            Vec::new(),
        );
    }
    Simulation::new(9, rules).unwrap()
}

fn only_warrior(simulation: &Simulation<RotwkRules>, ids: Ids, kind: Kind) -> Entity {
    let mut found: Vec<Entity> = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(_, k)| **k == kind)
        .map(|(entity, _)| entity)
        .collect();
    found.sort_unstable_by_key(|entity| entity.index());
    found[0]
}

#[test]
fn a_computer_owned_unit_eventually_takes_the_aggressive_stance() {
    let m = load();
    let kind = m.kinds.kind("Warrior").unwrap();
    let mut simulation = simulation(&m, PLAYER, true);
    let ids = simulation.rules().ids().unwrap();
    let entity = only_warrior(&simulation, ids, kind);
    assert_ne!(
        simulation
            .rules()
            .stance_of(simulation.state(), ids, entity),
        Some(Stance::Aggressive),
        "starts in the default stance"
    );
    let mut switched = false;
    for _ in 0..240 {
        simulation.tick().unwrap();
        if simulation
            .rules()
            .stance_of(simulation.state(), ids, entity)
            == Some(Stance::Aggressive)
        {
            switched = true;
            break;
        }
    }
    assert!(
        switched,
        "a computer-owned unit must switch within 240 ticks"
    );
}

#[test]
fn a_human_owned_unit_never_takes_the_aggressive_stance_on_its_own() {
    let m = load();
    let kind = m.kinds.kind("Warrior").unwrap();
    let mut simulation = simulation(&m, PLAYER, false);
    let ids = simulation.rules().ids().unwrap();
    let entity = only_warrior(&simulation, ids, kind);
    for _ in 0..240 {
        simulation.tick().unwrap();
    }
    assert_ne!(
        simulation
            .rules()
            .stance_of(simulation.state(), ids, entity),
        Some(Stance::Aggressive),
        "a human-owned unit's stance is the player's own business"
    );
}

const TARGETS_DATA: &str = "\
Object Caster\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object InfantryFoe\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object MonsterFoe\n  Side = Mordor\n  KindOf = MONSTER SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn load_targets() -> Match {
    let doc = mint_sage_ini::Document::parse(
        "targets.ini",
        TARGETS_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Caster", "InfantryFoe", "MonsterFoe"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    Match {
        data,
        kinds: std::sync::Arc::new(kinds),
    }
}

fn only_of_kind(simulation: &Simulation<RotwkRules>, ids: Ids, kind: Kind) -> Entity {
    let mut found: Vec<Entity> = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(_, k)| **k == kind)
        .map(|(entity, _)| entity)
        .collect();
    found.sort_unstable_by_key(|entity| entity.index());
    found[0]
}

struct Fields(Vec<(&'static str, &'static str)>);

impl mint_sim::ModuleFieldSource for Fields {
    fn field(&self, keyword: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
            .map(|(_, v)| *v)
    }
}

fn foe(entity: Entity, position: Vec2) -> Foe {
    Foe {
        entity,
        position,
        z: 0.0,
        radius: 5.0,
        height: 16.0,
        is_structure: false,
        airborne: None,
        unattackable: false,
        not_autoacquirable: false,
        ai_kind_of: None,
    }
}

#[test]
fn enemy_type_killer_excludes_a_hostile_outside_its_own_kindof_mask() {
    let m = load_targets();
    let caster_kind = m.kinds.kind("Caster").unwrap();
    let infantry_kind = m.kinds.kind("InfantryFoe").unwrap();
    let monster_kind = m.kinds.kind("MonsterFoe").unwrap();
    let spawn_at = |kind, x, owner| {
        m.kinds
            .spawn(&m.data.combat, kind, Vec2::new(x, 0.0), Facing::EAST, owner)
            .unwrap()
    };
    let map = flat_map(32, 32);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let production = m.kinds.production(&m.data);
    let rules = RotwkRules::new(
        passability,
        vec![
            spawn_at(caster_kind, 0.0, PLAYER),
            spawn_at(infantry_kind, 50.0, OTHER),
            spawn_at(monster_kind, 100.0, OTHER),
        ],
    )
    .with_combat(m.data.combat.clone())
    .with_construction(std::sync::Arc::clone(&m.kinds), production);
    let simulation = Simulation::new(9, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let caster = only_of_kind(&simulation, ids, caster_kind);
    let infantry = only_of_kind(&simulation, ids, infantry_kind);
    let monster = only_of_kind(&simulation, ids, monster_kind);
    let living = vec![
        (foe(infantry, Vec2::new(50.0, 0.0)), OTHER),
        (foe(monster, Vec2::new(100.0, 0.0)), OTHER),
    ];
    let grid = TargetGrid::build(&living);
    let module = mint_sage_modules::ai_special_power_update_from_fields(
        &Fields(vec![
            ("CommandButtonName", "Command_EnemyTypeKiller"),
            ("SpecialPowerAIType", "AI_SPECIAL_POWER_ENEMY_TYPE_KILLER"),
            ("SpecialPowerRange", "300"),
        ]),
        Frame::ZERO,
    )
    .unwrap();
    let candidates = simulation.rules().ai_power_candidates(
        simulation.state(),
        ids,
        caster,
        Vec2::new(0.0, 0.0),
        "AI_SPECIAL_POWER_ENEMY_TYPE_KILLER",
        &module,
        &living,
        &grid,
    );
    let entities: Vec<Entity> = candidates.iter().map(|c| c.entity).collect();
    assert!(
        entities.contains(&monster),
        "the MONSTER-kind foe must be scanned"
    );
    assert!(
        !entities.contains(&infantry),
        "the INFANTRY-kind foe is outside the mask and must not be scanned"
    );
}

#[test]
fn ranged_aoe_attack_refuses_rather_than_falling_back_to_a_farther_enemy() {
    let m = load_targets();
    let caster_kind = m.kinds.kind("Caster").unwrap();
    let infantry_kind = m.kinds.kind("InfantryFoe").unwrap();
    let monster_kind = m.kinds.kind("MonsterFoe").unwrap();
    let spawn_at = |kind, x, owner| {
        m.kinds
            .spawn(&m.data.combat, kind, Vec2::new(x, 0.0), Facing::EAST, owner)
            .unwrap()
    };
    let map = flat_map(32, 32);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let production = m.kinds.production(&m.data);
    let rules = RotwkRules::new(
        passability,
        vec![
            spawn_at(caster_kind, 0.0, PLAYER),
            spawn_at(infantry_kind, 100.0, OTHER),
            spawn_at(monster_kind, 250.0, OTHER),
        ],
    )
    .with_combat(m.data.combat.clone())
    .with_construction(std::sync::Arc::clone(&m.kinds), production);
    let simulation = Simulation::new(9, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let caster = only_of_kind(&simulation, ids, caster_kind);
    let infantry = only_of_kind(&simulation, ids, infantry_kind);
    let monster = only_of_kind(&simulation, ids, monster_kind);
    let living = vec![
        (foe(infantry, Vec2::new(100.0, 0.0)), OTHER),
        (foe(monster, Vec2::new(250.0, 0.0)), OTHER),
    ];
    let grid = TargetGrid::build(&living);
    let module = mint_sage_modules::ai_special_power_update_from_fields(
        &Fields(vec![
            ("CommandButtonName", "Command_RangedAoeAttack"),
            ("SpecialPowerAIType", "AI_SPECIAL_POWER_RANGED_AOE_ATTACK"),
            ("SpecialPowerRange", "300"),
        ]),
        Frame::ZERO,
    )
    .unwrap();
    let candidates = simulation.rules().ai_power_candidates(
        simulation.state(),
        ids,
        caster,
        Vec2::new(0.0, 0.0),
        "AI_SPECIAL_POWER_RANGED_AOE_ATTACK",
        &module,
        &living,
        &grid,
    );
    assert!(
        candidates.is_empty(),
        "the nearest enemy fails the far-enough test alone, so the type \
         must refuse rather than aim at the farther, passing one: got {candidates:?}"
    );
    let _ = monster;
}

#[test]
fn a_distance_tie_is_broken_on_entity_id_not_on_living_order() {
    let m = load_targets();
    let caster_kind = m.kinds.kind("Caster").unwrap();
    let infantry_kind = m.kinds.kind("InfantryFoe").unwrap();
    let monster_kind = m.kinds.kind("MonsterFoe").unwrap();
    let spawn_at = |kind, x, y, owner| {
        m.kinds
            .spawn(&m.data.combat, kind, Vec2::new(x, y), Facing::EAST, owner)
            .unwrap()
    };
    let map = flat_map(32, 32);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let production = m.kinds.production(&m.data);
    let rules = RotwkRules::new(
        passability,
        vec![
            spawn_at(caster_kind, 0.0, 0.0, PLAYER),
            spawn_at(infantry_kind, 50.0, 0.0, OTHER),
            spawn_at(monster_kind, 0.0, 50.0, OTHER),
        ],
    )
    .with_combat(m.data.combat.clone())
    .with_construction(std::sync::Arc::clone(&m.kinds), production);
    let simulation = Simulation::new(9, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let caster = only_of_kind(&simulation, ids, caster_kind);
    let infantry = only_of_kind(&simulation, ids, infantry_kind);
    let monster = only_of_kind(&simulation, ids, monster_kind);
    let module = mint_sage_modules::ai_special_power_update_from_fields(
        &Fields(vec![
            ("CommandButtonName", "Command_Charge"),
            ("SpecialPowerAIType", "AI_SPECIAL_POWER_CHARGE"),
            ("SpecialPowerRange", "300"),
        ]),
        Frame::ZERO,
    )
    .unwrap();
    let forward = vec![
        (foe(infantry, Vec2::new(50.0, 0.0)), OTHER),
        (foe(monster, Vec2::new(0.0, 50.0)), OTHER),
    ];
    let forward_grid = TargetGrid::build(&forward);
    let forward_first = simulation
        .rules()
        .ai_power_candidates(
            simulation.state(),
            ids,
            caster,
            Vec2::new(0.0, 0.0),
            "AI_SPECIAL_POWER_CHARGE",
            &module,
            &forward,
            &forward_grid,
        )
        .first()
        .map(|c| c.entity);
    let reversed = vec![
        (foe(monster, Vec2::new(0.0, 50.0)), OTHER),
        (foe(infantry, Vec2::new(50.0, 0.0)), OTHER),
    ];
    let reversed_grid = TargetGrid::build(&reversed);
    let reversed_first = simulation
        .rules()
        .ai_power_candidates(
            simulation.state(),
            ids,
            caster,
            Vec2::new(0.0, 0.0),
            "AI_SPECIAL_POWER_CHARGE",
            &module,
            &reversed,
            &reversed_grid,
        )
        .first()
        .map(|c| c.entity);
    assert_eq!(
        forward_first, reversed_first,
        "the nearest-of-a-tie must not depend on living's own order"
    );
    let expected = infantry.min(monster);
    assert_eq!(
        forward_first,
        Some(expected),
        "the tie is broken by entity id"
    );
}

const HORDE_DATA: &str = "\
StanceTemplate PlainStances\n  Stance Aggressive\n    AttributeModifier StanceMod\n  End\nEnd\n\
ModifierList StanceMod\n  Category = SPELL\n  Modifier = VISION 100%\nEnd\n\
CommandButton Command_SetStanceAggressive\n  Command = SET_STANCE\n  Stances = Aggressive\nEnd\n\
Object Spearman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object PikeHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE MELEE_HORDE\n  VisionRange = 100\n  \
  Behavior = StancesBehavior Tag_S\n    StanceTemplate = PlainStances\n  End\n  \
  Behavior = AISpecialPowerUpdate Tag_AI\n    CommandButtonName = Command_SetStanceAggressive\n    \
    SpecialPowerAIType = AI_SPECIAL_POWER_STANCEAGGRESSIVE\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 1\n    \
    RankInfo = RankNumber:1 UnitType:Spearman Position:X:0 Y:0\n  \
  End\nEnd\n";

#[test]
fn a_horde_container_attaches_a_kept_family_alongside_an_excluded_one() {
    let doc = mint_sage_ini::Document::parse(
        "horde.ini",
        HORDE_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["PikeHorde", "Spearman"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = std::sync::Arc::new(kinds);
    let horde_kind = kinds.kind("PikeHorde").unwrap();
    let member_kind = kinds.kind("Spearman").unwrap();
    let entry = kinds.get(horde_kind).unwrap();
    let (formation, member) = (
        entry.formation.expect("PikeHorde declares a HordeContain"),
        entry
            .member
            .expect("PikeHorde's own RankInfo names Spearman"),
    );
    let member_spawn = kinds
        .spawn(&data.combat, member, Vec2::ZERO, Facing::EAST, PLAYER)
        .unwrap();
    let map = flat_map(32, 32);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let production = kinds.production(&data);
    let rules = RotwkRules::new(passability, Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(std::sync::Arc::clone(&kinds), production)
        .with_ai_power_buttons(std::sync::Arc::clone(&data.command_buttons))
        .with_hordes(
            data.formations.tables().to_vec(),
            vec![HordeSpawn {
                formation,
                origin: Vec2::new(100.0, 100.0),
                facing: Facing::EAST,
                member: member_spawn,
                count: data
                    .formations
                    .tables()
                    .get(usize::from(formation))
                    .map_or(0, |table| u32::try_from(table.len()).unwrap_or(0)),
                vision: crate::shroud::HORDE_SHROUD_RANGE,
            }],
        )
        .with_skirmish_ai(
            vec![crate::skirmish_ai::AiSeed {
                player: PLAYER,
                candidates: Vec::new(),
            }],
            crate::combat::AiTiming::default(),
            Vec::new(),
        );
    let mut simulation = Simulation::new(11, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let horde = simulation
        .state()
        .store(ids.horde)
        .and_then(|store| store.entities().first().copied())
        .expect("one horde container spawned");
    assert_eq!(
        simulation.rules().stance_of(simulation.state(), ids, horde),
        Some(Stance::Battle),
        "starts in the default stance"
    );
    let mut switched = false;
    for _ in 0..240 {
        simulation.tick().unwrap();
        if simulation.rules().stance_of(simulation.state(), ids, horde) == Some(Stance::Aggressive)
        {
            switched = true;
            break;
        }
    }
    assert!(
        switched,
        "the horde container's own AISpecialPowerUpdate must fire even \
         though StancesBehavior, an excluded family, is declared right \
         alongside it"
    );
    let _ = member_kind;
}
