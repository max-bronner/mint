#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_math::rng::Pcg32;
use mint_sage_autoresolve::chain::Chain;
use mint_sage_autoresolve::roster::SkillTracker;
use mint_sage_autoresolve::tables::{
    Armor, Body, ExperienceLevel, Gated, Leadership, LeadershipBonus, Schedule, Template, UnitType,
    Weapon,
};
use mint_sage_autoresolve::{resolve, Army, Member, Options, Participant, Tables};
use mint_sage_big::BigArchive;
use mint_sage_ini::living_world::auto_resolve::{
    AutoResolveArmorDef, AutoResolveBodyDef, AutoResolveCombatChainDef, AutoResolveLeadershipDef,
    AutoResolveWeaponDef, ReinforcementScheduleDef, ScheduleSide, UnitType as IniUnitType,
};
use mint_sage_ini::living_world::{load_living_world, LivingWorldData};
use mint_sage_ini::value::{self, MacroTable};
use mint_sage_ini::{expand_macros, Block, BlockTable, Document, MemorySources, Node, Reader};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(root) = args.first() else {
        eprintln!("usage: matchup <install dir> [seed] [defender] [attacker] [quantity]");
        return ExitCode::FAILURE;
    };
    let seed: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let defender = args.get(2).map_or("GondorFighterHorde", String::as_str);
    let attacker = args.get(3).map_or("AngmarOrcWarriors", String::as_str);
    let quantity: i32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(3);

    let sources = read_sources(Path::new(root));
    let load = load_living_world(&sources);
    let table = BlockTable::corpus_path();
    let reader = Reader::new(&sources);
    let mut documents: Vec<Document> = sources
        .paths()
        .filter(|p| {
            p.ends_with(".ini")
                && (p.starts_with("data/ini/object/")
                    || p.starts_with("data/ini/default/object")
                    || p.ends_with("gamedata.ini")
                    || *p == "data/ini/experiencelevels.ini")
        })
        .filter_map(|p| reader.read(p, &table).ok())
        .collect();
    let macros = expand_macros(&mut documents);
    let objects = ObjectBlocks::new(&documents);

    let mut tables = tables_from(&load.data);
    let mut wanted: BTreeSet<String> = BTreeSet::new();
    for name in [defender, attacker] {
        wanted.insert(name.to_ascii_lowercase());
        if let Some(block) = objects.compose(name) {
            for (payload, _) in payloads(&block, &macros) {
                wanted.insert(payload.to_ascii_lowercase());
            }
        }
    }
    for name in &wanted {
        if let Some(block) = objects.compose(name) {
            tables
                .templates
                .insert(name.clone(), template_of(name, &block, &objects, &macros));
        }
    }
    for name in [defender, attacker] {
        if !tables.templates.contains_key(&name.to_ascii_lowercase()) {
            eprintln!("no object named {name}");
            return ExitCode::FAILURE;
        }
    }

    let side = |side: usize, name: &str| Participant {
        side,
        weapon_multiplier: 1.0,
        armor_multiplier: 1.0,
        experience_multiplier: 1.0,
        legendary_bonus: 0,
        skill: Some(SkillTracker {
            points: 0.0,
            purchase_points: 0,
            next_rank: 0,
            ranks: Vec::new(),
            gain_scale: 1.0,
            base_rank: 0,
            scaling: Vec::new(),
        }),
        science_bonus: Vec::new(),
        initial_units: Vec::new(),
        armies: vec![Army {
            id: 1,
            named: true,
            members: vec![Member::new(name, quantity)],
        }],
    };
    let participants = [side(0, defender), side(1, attacker)];
    let mut rng = Pcg32::new(seed);
    let out = resolve(&participants, &tables, Options::default(), &mut rng);
    println!("seed {seed}: {defender} x{quantity} (side 0) vs {attacker} x{quantity} (side 1)");
    for entry in &out.log {
        println!(
            "round {:>3}: units {:?} hp [{:.3}, {:.3}]",
            entry.round, entry.units, entry.hit_points[0], entry.hit_points[1]
        );
    }
    println!(
        "winner side {} after {} rounds, {} kills, {} draws",
        out.winner,
        out.rounds,
        out.kills.len(),
        out.draws
    );
    for (p, result) in out.participants.iter().enumerate() {
        for army in &result.armies {
            let xp: Vec<(f32, i32)> = army
                .survivors
                .iter()
                .map(|s| (s.member.experience, s.member.level))
                .collect();
            println!("participant {p} army {}: survivors {xp:?}", army.id);
        }
    }
    ExitCode::SUCCESS
}

fn read_sources(root: &Path) -> MemorySources {
    let mut archives = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(listing) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in listing.flatten() {
            let path: PathBuf = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(kind) if kind.is_file() => {
                    if path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("big"))
                    {
                        archives.push(path);
                    }
                }
                _ => {}
            }
        }
    }
    archives.sort();
    let mut sources = MemorySources::new();
    for path in &archives {
        let Ok(archive) = BigArchive::open(path) else {
            continue;
        };
        for entry in archive.entries() {
            if entry.path.as_str().starts_with("data/ini/") {
                if let Ok(bytes) = archive.read_entry(entry) {
                    sources.insert(entry.path.as_str(), bytes);
                }
            }
        }
    }
    sources
}

fn unit_type(t: IniUnitType) -> UnitType {
    UnitType::ALL[t as usize]
}

fn weapon_of(def: &AutoResolveWeaponDef, base: &Weapon) -> Weapon {
    let mut w = base.clone();
    if let Some(v) = def.reduce_attack_when_hurt {
        w.reduce_attack_when_hurt = v;
    }
    if let Some(v) = def.miss_percent_chance {
        w.miss_percent = v;
    }
    for d in &def.damage_per_round {
        w.damage[unit_type(d.against).index()] = d.damage;
    }
    for b in &def.level_bonus {
        w.level_bonus.insert(b.level, b.bonus);
    }
    w
}

fn armor_of(def: &AutoResolveArmorDef, base: &Armor) -> Armor {
    let mut a = base.clone();
    for e in &def.armor {
        a.taken[unit_type(e.against).index()] = e.taken;
    }
    a
}

fn body_of(def: &AutoResolveBodyDef, base: &Body) -> Body {
    let mut b = base.clone();
    if let Some(v) = def.can_be_attacked {
        b.can_be_attacked = v;
    }
    if let Some(v) = def.leave_in_army_summary {
        b.leave_in_army_summary = v;
    }
    for e in &def.hitpoints_at_level {
        b.hitpoints.insert(e.level, e.amount);
    }
    for e in &def.regenerate_at_level {
        b.regenerate.insert(e.level, e.amount);
    }
    b
}

fn chain_of(def: &AutoResolveCombatChainDef, base: &Chain) -> Chain {
    let mut c = base.clone();
    for t in &def.targets {
        if c.name(unit_type(t.target), t.priority).is_err() {
            eprintln!("chain {}: {:?} named twice", def.name, t.target);
        }
    }
    c
}

fn leadership_of(def: &AutoResolveLeadershipDef) -> Leadership {
    Leadership {
        affects: def.affects.0.iter().map(|t| unit_type(*t)).collect(),
        affects_higher_level_first: def.affects_higher_level_first.unwrap_or(false),
        bonuses: def
            .bonuses
            .iter()
            .map(|b| {
                (
                    b.min_level.unwrap_or(0),
                    LeadershipBonus {
                        maximum_units_affected: b.maximum_units_affected.unwrap_or(0),
                        priority: b.priority.unwrap_or(0),
                        weapon: b.weapon_multiplier.map_or(1.0, |p| p.0),
                        armor: b.armor_multiplier.map_or(1.0, |p| p.0),
                        experience: b.experience_multiplier.map_or(1.0, |p| p.0),
                    },
                )
            })
            .collect(),
    }
}

fn tables_from(data: &LivingWorldData) -> Tables {
    let mut t = Tables::default();
    if let Some(d) = data.auto_resolve_weapons.get("AutoResolve_DefaultWeapon") {
        t.default_weapon = weapon_of(d, &Weapon::default());
    }
    if let Some(d) = data.auto_resolve_armors.get("AutoResolve_DefaultArmor") {
        t.default_armor = armor_of(d, &Armor::default());
    }
    if let Some(d) = data.auto_resolve_bodies.get("AutoResolve_DefaultBody") {
        t.default_body = body_of(d, &Body::default());
    }
    if let Some(d) = data
        .auto_resolve_combat_chains
        .get("AutoResolve_DefaultCombatChain")
    {
        t.default_chain = chain_of(d, &Chain::default());
    }
    for d in data.auto_resolve_weapons.iter() {
        t.weapons
            .insert(d.name.to_ascii_lowercase(), weapon_of(d, &t.default_weapon));
    }
    for d in data.auto_resolve_armors.iter() {
        t.armors
            .insert(d.name.to_ascii_lowercase(), armor_of(d, &t.default_armor));
    }
    for d in data.auto_resolve_bodies.iter() {
        t.bodies
            .insert(d.name.to_ascii_lowercase(), body_of(d, &t.default_body));
    }
    for d in data.auto_resolve_combat_chains.iter() {
        t.chains
            .insert(d.name.to_ascii_lowercase(), chain_of(d, &t.default_chain));
    }
    for d in data.auto_resolve_leaderships.iter() {
        t.leaderships
            .insert(d.name.to_ascii_lowercase(), leadership_of(d));
    }
    for s in &data.reinforcement_schedules {
        let slot = match s.side {
            ScheduleSide::Defender => 0,
            ScheduleSide::Attacker => 1,
        };
        t.schedules[slot] = schedule_of(s);
    }
    t
}

fn schedule_of(s: &ReinforcementScheduleDef) -> Schedule {
    Schedule {
        rounds: s.rounds.clone(),
        each_remaining: s.each_remaining.unwrap_or(0),
    }
}

struct ObjectBlocks<'a> {
    by_name: BTreeMap<String, &'a Block>,
    levels: Vec<&'a Block>,
}

impl<'a> ObjectBlocks<'a> {
    fn new(documents: &'a [Document]) -> Self {
        let mut by_name = BTreeMap::new();
        let mut levels = Vec::new();
        for block in documents.iter().flat_map(Document::roots) {
            match (block.keyword.as_str(), block.name()) {
                ("Object" | "ChildObject", Some(name)) => {
                    by_name.insert(name.to_ascii_lowercase(), block);
                }
                ("ExperienceLevel", _) => levels.push(block),
                _ => {}
            }
        }
        Self { by_name, levels }
    }

    fn compose(&self, name: &str) -> Option<Block> {
        let block = *self.by_name.get(&name.to_ascii_lowercase())?;
        if block.keyword == "ChildObject" {
            let parent = self.compose(block.args.get(1)?)?;
            Some(parent.overlaid(block))
        } else {
            Some(block.clone())
        }
    }
}

fn tokens(block: &Block, keyword: &str) -> Vec<String> {
    block
        .fields_named(keyword)
        .flat_map(|f| f.args.iter().cloned())
        .collect()
}

fn payloads(block: &Block, macros: &MacroTable) -> Vec<(String, i32)> {
    fn walk(block: &Block, macros: &MacroTable, out: &mut Vec<(String, i32)>) {
        for node in &block.children {
            match node {
                Node::Field(f) if f.keyword == "InitialPayload" => {
                    if let [name, count] = f.args.as_slice() {
                        let n = value::parse_float_or_expression(count, macros).unwrap_or(0.0);
                        out.push((name.clone(), n as i32));
                    }
                }
                Node::Block(b) => walk(b, macros, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(block, macros, &mut out);
    out
}

fn gated(block: &Block, keyword: &str, field: &str) -> Vec<Gated> {
    block
        .blocks_named(keyword)
        .map(|b| Gated {
            required: tokens(b, "RequiredUpgrades"),
            excluded: tokens(b, "ExcludedUpgrades"),
            name: tokens(b, field).into_iter().next().unwrap_or_default(),
        })
        .collect()
}

fn template_of(
    name: &str,
    block: &Block,
    objects: &ObjectBlocks<'_>,
    macros: &MacroTable,
) -> Template {
    let first = |keyword: &str| tokens(block, keyword).into_iter().next();
    let kinds = tokens(block, "KindOf");
    let has = |kind: &str| kinds.iter().any(|k| k.eq_ignore_ascii_case(kind));
    let unit_type = first("AutoResolveUnitType")
        .and_then(|n| IniUnitType::from_name(&n))
        .map_or(UnitType::Invalid, unit_type_of);
    let levels = objects
        .levels
        .iter()
        .filter(|l| {
            tokens(l, "TargetNames")
                .iter()
                .any(|t| t.eq_ignore_ascii_case(name))
        })
        .map(|l| {
            let num = |k: &str| {
                first_of(l, k)
                    .and_then(|v| value::parse_float_or_expression(&v, macros))
                    .unwrap_or(0.0)
            };
            ExperienceLevel {
                required: num("RequiredExperience"),
                award: num("ExperienceAward") as i32,
                rank: num("Rank") as i32,
            }
        })
        .collect();
    Template {
        unit_type,
        weapons: gated(block, "AutoResolveWeapon", "Weapon"),
        armors: gated(block, "AutoResolveArmor", "Armor"),
        body: first("AutoResolveBody"),
        leadership: first("AutoResolveLeadership"),
        chain: first("AutoResolveCombatChain"),
        trainable: first("IsTrainable")
            .and_then(|v| value::parse_bool(&v))
            .unwrap_or(true),
        horde: has("HORDE"),
        structure: has("STRUCTURE"),
        hero: has("HERO"),
        initial_payload: payloads(block, macros),
        levels,
    }
}

fn first_of(block: &Block, keyword: &str) -> Option<String> {
    tokens(block, keyword).into_iter().next()
}

fn unit_type_of(t: IniUnitType) -> UnitType {
    unit_type(t)
}
