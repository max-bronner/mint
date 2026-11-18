use std::collections::{BTreeMap, BTreeSet};

use mint_math::rng::Pcg32;
use mint_sage_ini::living_world::auto_resolve::UnitType;
use mint_sage_ini::living_world::LivingWorldData;
use mint_sage_ini::value::{parse_float, parse_float_or_expression, parse_float_or_macro};
use mint_sage_ini::{expand_macros, BlockTable, Document, Field, MacroTable};

use crate::ai::{Prediction, Predictor, Scale};
use crate::battle::{AutoResolve, BattleSpec};
use crate::model::BattleResult;
use crate::rules::{Catalog, PriceClass, UnitInfo};

#[derive(Debug, Default, Clone, Copy)]
pub struct DefenderWins;

impl AutoResolve for DefenderWins {
    fn resolve(&mut self, spec: &BattleSpec, _rng: &mut Pcg32) -> BattleResult {
        BattleResult {
            winner: u8::try_from(spec.sides.len().saturating_sub(1)).ok(),
            ..BattleResult::default()
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct HeadCount;

impl Predictor for HeadCount {
    fn predict(
        &mut self,
        spec: &BattleSpec,
        ai_side: u8,
        scale: Option<Scale>,
        _rng: &mut Pcg32,
    ) -> Prediction {
        let other = 1u8.saturating_sub(ai_side.min(1));
        let strength = |index: u8| -> f32 {
            let own = spec.sides.get(usize::from(index)).map_or(0, |s| {
                s.armies
                    .iter()
                    .flat_map(|a| a.roster.iter())
                    .fold(0u32, |sum, m| sum.saturating_add(m.count))
            });
            let factor = scale
                .filter(|_| index == ai_side)
                .map_or(1.0, |s| s.weapon * s.armor);
            f32::from(u16::try_from(own).unwrap_or(u16::MAX)) * factor
        };
        Prediction {
            winner: Some(if strength(ai_side) >= strength(other) {
                ai_side
            } else {
                other
            }),
            rounds: 3,
        }
    }
}

#[must_use]
pub fn catalog_from_object_ini<'a>(
    texts: impl IntoIterator<Item = &'a str>,
    data: &LivingWorldData,
) -> Catalog {
    let table = BlockTable::corpus_path();
    let mut documents: Vec<Document> = texts
        .into_iter()
        .filter_map(|text| Document::parse("object.ini", text, &table).ok())
        .collect();
    let macros = expand_macros(&mut documents);
    let mut scan = Scan::default();
    scan_objects(&documents, &macros, &mut scan);
    let units: BTreeMap<String, UnitInfo> = scan
        .own
        .keys()
        .map(|name| (name.clone(), resolve(&scan, name, data)))
        .collect();
    let portraits = scan
        .own
        .keys()
        .filter_map(|name| Some((name.clone(), inherited(&scan, name)?)))
        .collect();
    Catalog {
        portraits,
        units,
        resource_multiplier_limit: scan.multiplier_limit.unwrap_or(1.0),
        playable_factions: BTreeSet::new(),
        ranks: scan.ranks.into_values().collect(),
    }
}

#[derive(Default)]
struct Own {
    command_points: Option<i32>,
    build_cost: Option<i32>,
    resolve_type: Option<String>,
    chain: Option<String>,
    kind_of: Vec<Vec<String>>,
}

#[derive(Default)]
struct Scan {
    own: BTreeMap<String, Own>,
    parents: BTreeMap<String, String>,
    portraits: BTreeMap<String, String>,
    multiplier_limit: Option<f32>,
    ranks: BTreeMap<u32, crate::rules::RankGrant>,
}

fn lineage<'a>(scan: &'a Scan, name: &'a str) -> impl Iterator<Item = &'a str> {
    std::iter::successors(Some(name), |at| scan.parents.get(*at).map(String::as_str))
        .take(MAX_LINEAGE)
}

fn resolve(scan: &Scan, name: &str, data: &LivingWorldData) -> UnitInfo {
    let chain: Vec<&Own> = lineage(scan, name)
        .filter_map(|n| scan.own.get(n))
        .collect();
    let mut hero = false;
    for own in chain.iter().rev() {
        for line in &own.kind_of {
            if line.iter().any(|k| !k.starts_with(['+', '-'])) {
                hero = false;
            }
            for kind in line {
                if kind
                    .trim_start_matches(['+', '-'])
                    .eq_ignore_ascii_case("HERO")
                {
                    hero = !kind.starts_with('-');
                }
            }
        }
    }
    let mut info = UnitInfo {
        command_points: chain.iter().find_map(|o| o.command_points).unwrap_or(0),
        build_cost: chain.iter().find_map(|o| o.build_cost).unwrap_or(0),
        hero,
        ..UnitInfo::default()
    };
    if hero {
        info.price_class = PriceClass::Hero;
    }
    if let Some(kind) = chain.iter().find_map(|o| o.resolve_type.as_deref()) {
        info.set_resolve_type(kind);
    }
    info.counter_target = chain
        .iter()
        .find_map(|o| o.chain.as_deref())
        .and_then(|chain| data.auto_resolve_combat_chains.get(chain))
        .and_then(|c| c.targets.first())
        .and_then(|t| UnitType::NAMES.iter().position(|n| *n == t.target.name()))
        .and_then(|i| u8::try_from(i).ok());
    info
}

fn inherited(scan: &Scan, name: &str) -> Option<String> {
    lineage(scan, name).find_map(|n| scan.portraits.get(n).cloned())
}

const MAX_LINEAGE: usize = 32;

#[allow(clippy::cast_possible_truncation)]
fn scan_objects(documents: &[Document], macros: &MacroTable, scan: &mut Scan) {
    let number = |field: &Field| {
        field
            .value()
            .and_then(|v| parse_float_or_macro(v, macros))
            .map_or(0, |v| v as i32)
    };
    for block in documents.iter().flat_map(Document::roots) {
        match block.keyword.as_str() {
            "Object" | "ChildObject" => {
                let Some(name) = block.name().map(str::to_ascii_lowercase) else {
                    continue;
                };
                let mut own = Own::default();
                for field in block.fields() {
                    let value = field.value().map(str::to_owned);
                    match field.keyword.as_str() {
                        "CommandPoints" => own.command_points = Some(number(field)),
                        "BuildCost" => own.build_cost = Some(number(field)),
                        "KindOf" => own.kind_of.push(field.args.clone()),
                        "AutoResolveUnitType" => own.resolve_type = value,
                        "AutoResolveCombatChain" => own.chain = value,
                        "SelectPortrait" => {
                            if let Some(image) = value {
                                scan.portraits.insert(name.clone(), image);
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(parent) = block.args.get(1) {
                    scan.parents
                        .insert(name.clone(), parent.to_ascii_lowercase());
                }
                scan.own.insert(name, own);
            }
            "GameData" => {
                if let Some(limit) = block.field("ResourceMultiplierLimit") {
                    scan.multiplier_limit = limit.value().and_then(parse_float);
                }
            }
            "Rank" => {
                let Some(number) = block.name().and_then(|n| n.parse::<u32>().ok()) else {
                    continue;
                };
                let value = |name: &str| {
                    block
                        .field(name)
                        .and_then(Field::value)
                        .and_then(|v| parse_float_or_expression(v, macros))
                };
                #[allow(clippy::cast_possible_truncation)]
                let grant = crate::rules::RankGrant {
                    threshold: value("SkillPointsNeededDefault").unwrap_or(0.0),
                    granted: value("SciencePurchasePointsGranted").map_or(0, |v| v as i32),
                };
                scan.ranks.insert(number, grant);
            }
            _ => {}
        }
    }
}
