use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use mint_math::rng::Pcg32;
use mint_sage_autoresolve::chain::Chain;
use mint_sage_autoresolve::roster::Outcome;
use mint_sage_autoresolve::tables::{
    Armor, Body, ExperienceLevel, Gated, Leadership, LeadershipBonus, Schedule, Template, UnitType,
    Weapon,
};
use mint_sage_autoresolve::{resolve, Army, Member, Options, Participant, Tables};
use mint_sage_ini::living_world::auto_resolve::{
    AutoResolveArmorDef, AutoResolveBodyDef, AutoResolveCombatChainDef, AutoResolveLeadershipDef,
    AutoResolveWeaponDef, ReinforcementScheduleDef, ScheduleSide, UnitType as IniUnitType,
};
use mint_sage_ini::living_world::LivingWorldData;
use mint_sage_ini::value::{self, MacroTable};
use mint_sage_ini::{
    expand_macros, Block, BlockTable, Document, MemorySources, Node, Reader as IniReader,
};
use mint_sage_livingworld::ai::{Prediction, Scale};
use mint_sage_livingworld::{
    ArmyId, AutoResolve, BattleResult, BattleSpec, FallenHero, Member as StratMember, PlayerId,
    Predictor,
};

#[derive(Debug, Clone)]
pub(crate) struct ResolvedBattle {
    pub(crate) battle: u32,
    pub(crate) region: mint_sage_livingworld::RegionId,
    pub(crate) sides: [Vec<PlayerId>; 2],
    pub(crate) order: Vec<PlayerId>,
    pub(crate) outcome: Outcome,
    pub(crate) heroes: BTreeMap<u32, String>,
}

impl ResolvedBattle {
    #[cfg(test)]
    pub(crate) fn log(&self) -> &[mint_sage_autoresolve::roster::RoundLog] {
        &self.outcome.log
    }

    #[cfg(test)]
    pub(crate) fn survivors(&self) -> [usize; 2] {
        let mut out = [0usize; 2];
        for (index, participant) in self.outcome.participants.iter().enumerate() {
            let side = self.side_of(index);
            for army in &participant.armies {
                if army.touched {
                    let n = army.survivors.len();
                    if let Some(slot) = out.get_mut(side) {
                        *slot = slot.saturating_add(n);
                    }
                }
            }
        }
        out
    }

    pub(crate) fn side_of(&self, participant: usize) -> usize {
        let player = self.order.get(participant).copied();
        usize::from(!player.is_some_and(|p| self.sides.first().is_some_and(|s| s.contains(&p))))
    }
}

#[derive(Clone)]
pub(crate) struct Forecast {
    tables: Arc<Tables>,
    names: Arc<[String]>,
}

pub(crate) struct InstallResolver {
    forecast: Forecast,
    pub(crate) last: Option<ResolvedBattle>,
}

fn unit_type(t: IniUnitType) -> UnitType {
    UnitType::ALL
        .get(t as usize)
        .copied()
        .unwrap_or(UnitType::Invalid)
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
        if let Some(slot) = w.damage.get_mut(unit_type(d.against).index()) {
            *slot = d.damage;
        }
    }
    for b in &def.level_bonus {
        w.level_bonus.insert(b.level, b.bonus);
    }
    w
}

fn armor_of(def: &AutoResolveArmorDef, base: &Armor) -> Armor {
    let mut a = base.clone();
    for e in &def.armor {
        if let Some(slot) = a.taken.get_mut(unit_type(e.against).index()) {
            *slot = e.taken;
        }
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
        let _ = c.name(unit_type(t.target), t.priority);
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
        if let Some(entry) = t.schedules.get_mut(slot) {
            *entry = schedule_of(s);
        }
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

    fn compose(&self, name: &str, depth: u8) -> Option<Block> {
        let block = *self.by_name.get(&name.to_ascii_lowercase())?;
        if block.keyword == "ChildObject" {
            let parent = self.compose(
                block.args.get(1)?,
                depth.checked_add(1).filter(|d| *d < 32)?,
            )?;
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

fn first_of(block: &Block, keyword: &str) -> Option<String> {
    tokens(block, keyword).into_iter().next()
}

fn payloads(block: &Block, macros: &MacroTable) -> Vec<(String, i32)> {
    fn walk(block: &Block, macros: &MacroTable, out: &mut Vec<(String, i32)>) {
        for node in &block.children {
            match node {
                Node::Field(f) if f.keyword == "InitialPayload" => {
                    if let [name, count] = f.args.as_slice() {
                        let n = value::parse_float_or_expression(count, macros).unwrap_or(0.0);
                        #[allow(clippy::cast_possible_truncation)]
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

#[allow(clippy::cast_possible_truncation)]
fn template_of(
    name: &str,
    block: &Block,
    objects: &ObjectBlocks<'_>,
    macros: &MacroTable,
) -> Template {
    let kinds = tokens(block, "KindOf");
    let has = |kind: &str| kinds.iter().any(|k| k.eq_ignore_ascii_case(kind));
    let unit_type = first_of(block, "AutoResolveUnitType")
        .and_then(|n| IniUnitType::from_name(&n))
        .map_or(UnitType::Invalid, unit_type);
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
        body: first_of(block, "AutoResolveBody"),
        leadership: first_of(block, "AutoResolveLeadership"),
        chain: first_of(block, "AutoResolveCombatChain"),
        trainable: first_of(block, "IsTrainable")
            .and_then(|v| value::parse_bool(&v))
            .unwrap_or(true),
        horde: has("HORDE"),
        structure: has("STRUCTURE"),
        hero: has("HERO"),
        initial_payload: payloads(block, macros),
        levels,
    }
}

impl InstallResolver {
    pub(crate) fn new(
        data: &LivingWorldData,
        sources: &MemorySources,
        unit_names: &[String],
    ) -> Self {
        let table = BlockTable::corpus_path();
        let reader = IniReader::new(sources);
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
        let mut tables = tables_from(data);
        let mut pending: Vec<String> = unit_names.iter().map(|n| n.to_ascii_lowercase()).collect();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        while let Some(name) = pending.pop() {
            if !seen.insert(name.clone()) {
                continue;
            }
            let Some(block) = objects.compose(&name, 0) else {
                continue;
            };
            for (payload, _) in payloads(&block, &macros) {
                pending.push(payload.to_ascii_lowercase());
            }
            tables
                .templates
                .insert(name.clone(), template_of(&name, &block, &objects, &macros));
        }
        Self::with(tables, unit_names.to_vec())
    }

    fn with(tables: Tables, names: Vec<String>) -> Self {
        Self {
            forecast: Forecast {
                tables: Arc::new(tables),
                names: names.into(),
            },
            last: None,
        }
    }

    pub(crate) fn forecast(&self) -> Forecast {
        self.forecast.clone()
    }

    pub(crate) fn template_count(&self) -> usize {
        self.forecast.tables.templates.len()
    }
}

fn level_experience(tables: &Tables, template: &str, level: i32) -> f32 {
    tables
        .templates
        .get(&template.to_ascii_lowercase())
        .and_then(|t| t.levels.iter().find(|l| l.rank == level))
        .map_or(0.0, |l| l.required)
}

struct Field {
    participants: Vec<Participant>,
    order: Vec<PlayerId>,
    sides: [Vec<PlayerId>; 2],
    owner_side: usize,
}

impl Field {
    fn one_sided(&self) -> bool {
        self.participants.iter().all(|p| p.side == 0)
            || self.participants.iter().all(|p| p.side == 1)
    }

    fn spec_side(&self, spec: &BattleSpec, winner: usize) -> Option<u8> {
        let players = self.sides.get(winner)?;
        spec.sides
            .iter()
            .position(|s| s.players.iter().any(|p| players.contains(p)))
            .and_then(|i| u8::try_from(i).ok())
    }
}

impl Forecast {
    fn field(&self, spec: &BattleSpec, scaled: Option<(usize, Scale)>) -> Field {
        let owner_side = spec
            .sides
            .iter()
            .position(|s| spec.owner.is_some_and(|o| s.players.contains(&o)))
            .unwrap_or(0);
        let mut participants: Vec<Participant> = Vec::new();
        let mut order: Vec<PlayerId> = Vec::new();
        let mut sides: [Vec<PlayerId>; 2] = [Vec::new(), Vec::new()];
        for (index, side) in spec.sides.iter().enumerate() {
            let slot = usize::from(index != owner_side);
            let scale = scaled
                .filter(|(s, _)| *s == index)
                .map_or((1.0, 1.0), |(_, s)| (s.weapon, s.armor));
            for player in &side.players {
                let armies = side
                    .armies
                    .iter()
                    .filter(|a| a.player == *player)
                    .map(|a| Army {
                        id: a.id.0,
                        named: a.is_hero_army(),
                        members: a
                            .roster
                            .iter()
                            .map(|m| {
                                let template = self
                                    .names
                                    .get(usize::from(m.unit.0))
                                    .cloned()
                                    .unwrap_or_default();
                                let level = i32::try_from(m.level).unwrap_or(1).max(1);
                                let mut member = Member::new(
                                    &template,
                                    i32::try_from(m.count).unwrap_or(i32::MAX),
                                );
                                member.level = level;
                                member.experience =
                                    level_experience(&self.tables, &template, level);
                                member.object = Some(m.id);
                                member
                            })
                            .collect(),
                    })
                    .collect();
                participants.push(Participant {
                    side: slot,
                    weapon_multiplier: scale.0
                        * spec
                            .weapon_scale
                            .iter()
                            .find(|(p, _)| p == player)
                            .map_or(1.0, |(_, w)| *w),
                    armor_multiplier: scale.1,
                    experience_multiplier: 1.0,
                    skill: None,
                    legendary_bonus: 0,
                    science_bonus: Vec::new(),
                    initial_units: Vec::new(),
                    armies,
                });
                order.push(*player);
                if let Some(list) = sides.get_mut(slot) {
                    list.push(*player);
                }
            }
        }
        Field {
            participants,
            order,
            sides,
            owner_side,
        }
    }
}

impl Predictor for Forecast {
    fn predict(
        &mut self,
        spec: &BattleSpec,
        ai_side: u8,
        scale: Option<Scale>,
        rng: &mut Pcg32,
    ) -> Prediction {
        let field = self.field(spec, scale.map(|s| (usize::from(ai_side), s)));
        if field.one_sided() {
            return Prediction {
                winner: u8::try_from(field.owner_side).ok(),
                rounds: 0,
            };
        }
        let options = Options {
            round_cap: spec.round_cap,
            ..Options::default()
        };
        let outcome = resolve(&field.participants, &self.tables, options, rng);
        Prediction {
            winner: field.spec_side(spec, outcome.winner),
            rounds: outcome.rounds,
        }
    }
}

impl AutoResolve for InstallResolver {
    fn resolve(&mut self, spec: &BattleSpec, rng: &mut Pcg32) -> BattleResult {
        let field = self.forecast.field(spec, None);
        if field.one_sided() {
            return BattleResult {
                winner: u8::try_from(field.owner_side).ok(),
                rosters: Vec::new(),
                ..BattleResult::default()
            };
        }
        let options = Options {
            round_cap: spec.round_cap,
            ..Options::default()
        };
        let outcome = resolve(&field.participants, &self.forecast.tables, options, rng);
        let mut rosters: Vec<(ArmyId, Vec<StratMember>)> = Vec::new();
        for (participant, result) in field.participants.iter().zip(&outcome.participants) {
            for (input, army) in participant.armies.iter().zip(&result.armies) {
                if !army.touched {
                    continue;
                }
                let roster = army
                    .survivors
                    .iter()
                    .filter_map(|s| {
                        let id = s.member.object?;
                        let original = spec
                            .sides
                            .iter()
                            .flat_map(|side| side.armies.iter())
                            .find(|a| a.id.0 == input.id)?
                            .roster
                            .iter()
                            .find(|m| m.id == id)?;
                        Some(StratMember {
                            id,
                            unit: original.unit,
                            level: u32::try_from(s.member.level).unwrap_or(original.level),
                            count: u32::try_from(s.member.quantity.max(0)).unwrap_or(0),
                            experience: original.experience,
                            battles: 0,
                            kills: 0,
                            kills_battle: 0,
                            upgraded_at: original.upgraded_at,
                        })
                    })
                    .collect();
                rosters.push((ArmyId(input.id), roster));
            }
        }
        let winner = field.spec_side(spec, outcome.winner);
        let fallen_records = outcome
            .fallen_heroes
            .iter()
            .filter_map(|fallen| {
                let army = spec
                    .sides
                    .iter()
                    .flat_map(|side| side.armies.iter())
                    .find(|a| a.id.0 == fallen.army)?;
                let original = army
                    .roster
                    .iter()
                    .find(|m| Some(m.id) == fallen.member.object)?;
                Some((
                    army.player,
                    FallenHero {
                        unit: original.unit,
                        level: u32::try_from(fallen.member.level).unwrap_or(original.level),
                        experience: original.experience,
                    },
                ))
            })
            .collect();
        self.last = Some(ResolvedBattle {
            battle: spec.battle,
            region: spec.region,
            sides: field.sides,
            order: field.order,
            outcome,
            heroes: spec
                .sides
                .iter()
                .flat_map(|side| side.armies.iter())
                .filter_map(|a| {
                    let name = self.forecast.names.get(usize::from(a.hero?.0))?;
                    Some((a.id.0, name.clone()))
                })
                .collect(),
        });
        BattleResult {
            winner,
            rosters,
            fallen_records,
            ..BattleResult::default()
        }
    }
}

#[cfg(test)]
mod tests;
