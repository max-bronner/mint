use std::collections::BTreeMap;
use std::sync::Arc;

use crate::tree::{Block, Document, Field, Node};

pub const GAME_DATA_KEYWORD: &str = "GameData";
pub const AI_DATA_KEYWORD: &str = "AIData";
pub const SIDE_INFO_KEYWORD: &str = "SideInfo";
pub const SKIRMISH_AI_DATA_KEYWORD: &str = "SkirmishAIData";
pub const DIFFICULTY_TUNING_KEYWORD: &str = "DifficultyTuning";
pub const BRUTAL_DIFFICULTY_CHEATS_KEYWORD: &str = "BrutalDifficultyCheats";
pub const COMBAT_CHAIN_DEFINITION_KEYWORD: &str = "CombatChainDefinition";

const DIFFICULTY_NAMES: [&str; 4] = ["EASY", "NORMAL", "HARD", "BRUTAL"];

#[must_use]
pub fn skirmish_special_power_activation_tuning(documents: &[Document]) -> [(i64, i64); 4] {
    let mut rows = [(1i64, 1i64); 4];
    for block in root_blocks(documents, SKIRMISH_AI_DATA_KEYWORD) {
        for tuning in block.blocks_named(DIFFICULTY_TUNING_KEYWORD) {
            let Some(index) = difficulty_index(tuning) else {
                continue;
            };
            let Some(ratio) = probability_ratio(tuning, "SpecialPowerActivationProbability") else {
                continue;
            };
            if let Some(slot) = rows.get_mut(index) {
                *slot = ratio;
            }
        }
    }
    rows
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrutalCheats {
    pub build_cost_reduction: f32,
    pub build_time_reduction: f32,
}

impl BrutalCheats {
    pub const NONE: Self = Self {
        build_cost_reduction: 0.0,
        build_time_reduction: 0.0,
    };
}

impl Default for BrutalCheats {
    fn default() -> Self {
        Self {
            build_cost_reduction: 0.0,
            build_time_reduction: 0.1,
        }
    }
}

#[must_use]
pub fn brutal_difficulty_cheats(documents: &[Document]) -> BrutalCheats {
    let mut cheats = BrutalCheats::default();
    for ai_data in root_blocks(documents, SKIRMISH_AI_DATA_KEYWORD) {
        for block in ai_data.blocks_named(BRUTAL_DIFFICULTY_CHEATS_KEYWORD) {
            let percent = |keyword: &str, default: f32| {
                block
                    .field(keyword)
                    .and_then(Field::value)
                    .and_then(crate::value::parse_float)
                    .map_or(default, |value| value * 0.01)
            };
            let defaults = BrutalCheats::default();
            cheats = BrutalCheats {
                build_cost_reduction: percent("BuildCostReduction", defaults.build_cost_reduction),
                build_time_reduction: percent("BuildTimeReduction", defaults.build_time_reduction),
            };
        }
    }
    cheats
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RetreatTuning {
    pub farming_threshold: i32,
    pub army_quality_bias: i32,
    pub army_quantity_bias: i32,
    pub hero_quality_bias: i32,
    pub map_control_bias: i32,
    pub base_strength_bias: i32,
    pub ring_ownership_bias: i32,
    pub frames_till_checks_start: i32,
    pub frames_between_checks: i32,
    pub frames_till_self_destruct: i32,
    pub disable_tactical_ai: bool,
}

const RETREAT_INT_FIELDS: [&str; 10] = [
    "FarmingThreshold",
    "ArmyQualityBias",
    "ArmyQuantityBias",
    "HeroQualityBias",
    "MapControlBias",
    "BaseStrengthBias",
    "RingOwnershipBias",
    "LogicFramesTillRetreatChecksStart",
    "LogicFrameBetweenRetreatChecks",
    "LogicFramesTillAISelfDestructs",
];

pub fn wotr_retreat_tuning(documents: &[Document]) -> Result<Option<RetreatTuning>, String> {
    let mut values = [0i32; 10];
    let mut disable = false;
    let mut seen = false;
    for block in root_blocks(documents, SKIRMISH_AI_DATA_KEYWORD) {
        for (slot, name) in values.iter_mut().zip(RETREAT_INT_FIELDS) {
            let Some(field) = block.field(name) else {
                continue;
            };
            *slot = field
                .value()
                .and_then(crate::value::parse_int)
                .and_then(|value| i32::try_from(value).ok())
                .ok_or_else(|| format!("SkirmishAIData {name}: not an Int"))?;
            seen = true;
        }
        if let Some(field) = block.field("DisableTacticalAI") {
            disable = field
                .value()
                .and_then(crate::value::parse_bool)
                .ok_or_else(|| "SkirmishAIData DisableTacticalAI: not a Bool".to_owned())?;
            seen = true;
        }
    }
    let [farming_threshold, army_quality_bias, army_quantity_bias, hero_quality_bias, map_control_bias, base_strength_bias, ring_ownership_bias, frames_till_checks_start, frames_between_checks, frames_till_self_destruct] =
        values;
    Ok(seen.then_some(RetreatTuning {
        farming_threshold,
        army_quality_bias,
        army_quantity_bias,
        hero_quality_bias,
        map_control_bias,
        base_strength_bias,
        ring_ownership_bias,
        frames_till_checks_start,
        frames_between_checks,
        frames_till_self_destruct,
        disable_tactical_ai: disable,
    }))
}

#[must_use]
pub fn combat_chain_targets(block: &Block) -> Vec<(String, f32)> {
    let Some(target_types) = block.field("TargetTypes") else {
        return Vec::new();
    };
    let Some(modifiers) = block.field("TargetPriorityModifiers") else {
        return Vec::new();
    };
    target_types
        .args
        .iter()
        .zip(modifiers.args.iter())
        .filter_map(|(target, modifier)| {
            Some((target.as_str().to_owned(), modifier.as_str().parse().ok()?))
        })
        .collect()
}

#[must_use]
pub fn merged_combat_chain_definitions(
    documents: &[Document],
) -> BTreeMap<String, Vec<(String, f32)>> {
    let mut rows: BTreeMap<String, Vec<(String, f32)>> = BTreeMap::new();
    for ai_data in root_blocks(documents, SKIRMISH_AI_DATA_KEYWORD) {
        for chain in ai_data.blocks_named(COMBAT_CHAIN_DEFINITION_KEYWORD) {
            let Some(unit) = chain.field("Unit").and_then(Field::value) else {
                continue;
            };
            rows.insert(unit.to_ascii_uppercase(), combat_chain_targets(chain));
        }
    }
    rows
}

fn difficulty_index(block: &Block) -> Option<usize> {
    let value = block.field("Difficulty")?.value()?;
    DIFFICULTY_NAMES
        .iter()
        .position(|name| name.eq_ignore_ascii_case(value))
}

fn probability_ratio(block: &Block, keyword: &str) -> Option<(i64, i64)> {
    let field = block.field(keyword)?;
    let mut tokens = field.args.iter().filter(|token| token.as_str() != ":");
    let a = tokens.next()?.parse::<i64>().ok()?;
    let b = tokens.next()?.parse::<i64>().ok()?;
    if b < 1 {
        return None;
    }
    Some((a, b))
}

#[must_use]
pub fn merged_game_data(documents: &[Document]) -> Block {
    merge(GAME_DATA_KEYWORD, root_blocks(documents, GAME_DATA_KEYWORD))
}

#[must_use]
pub fn merged_ai_data(documents: &[Document]) -> Block {
    merge(AI_DATA_KEYWORD, root_blocks(documents, AI_DATA_KEYWORD))
}

#[must_use]
pub fn merged_side_info(documents: &[Document]) -> BTreeMap<String, Block> {
    let mut by_side: BTreeMap<String, Vec<&Block>> = BTreeMap::new();
    for ai_data in root_blocks(documents, AI_DATA_KEYWORD) {
        for side in ai_data.blocks_named(SIDE_INFO_KEYWORD) {
            let Some(name) = side.name() else { continue };
            by_side
                .entry(name.to_ascii_lowercase())
                .or_default()
                .push(side);
        }
    }
    by_side
        .into_iter()
        .map(|(name, blocks)| (name, merge(SIDE_INFO_KEYWORD, blocks.into_iter())))
        .collect()
}

#[must_use]
pub fn attack_priority_blocks(documents: &[Document]) -> BTreeMap<String, &Block> {
    let mut by_name: BTreeMap<String, &Block> = BTreeMap::new();
    for ai_data in root_blocks(documents, AI_DATA_KEYWORD) {
        for set in ai_data.blocks_named("AttackPriority") {
            let Some(name) = set.name() else { continue };
            by_name.insert(name.to_ascii_lowercase(), set);
        }
    }
    by_name
}

fn root_blocks<'a>(
    documents: &'a [Document],
    keyword: &'static str,
) -> impl Iterator<Item = &'a Block> {
    documents
        .iter()
        .flat_map(Document::roots)
        .filter(move |block| block.keyword == keyword)
}

fn merge<'a>(keyword: &str, blocks: impl Iterator<Item = &'a Block>) -> Block {
    let mut fields: Vec<Field> = Vec::new();
    for block in blocks {
        for field in block.fields() {
            match fields.iter_mut().find(|f| f.keyword == field.keyword) {
                Some(slot) => *slot = field.clone(),
                None => fields.push(field.clone()),
            }
        }
    }
    Block {
        keyword: keyword.to_owned(),
        args: Vec::new(),
        children: fields.into_iter().map(Node::Field).collect(),
        file: Arc::from("<merged>"),
        line: 0,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::table::BlockTable;

    fn parse(name: &str, text: &str) -> Document {
        let document = Document::parse(name, text, &BlockTable::corpus_path()).unwrap();
        assert!(document.is_balanced(), "{:?}", document.diagnostics());
        document
    }

    #[test]
    fn two_game_data_blocks_combine_field_by_field_with_the_later_one_winning() {
        let first = parse(
            "a.ini",
            "GameData\n  SellPercentage = 50%\n  AttributeModifierArmorMaxBonus = 75%\nEnd\n",
        );
        let second = parse("b.ini", "GameData\n  SellPercentage = 40%\nEnd\n");
        let merged = merged_game_data(&[first, second]);
        assert_eq!(merged.field("SellPercentage").unwrap().value(), Some("40%"));
        assert_eq!(
            merged
                .field("AttributeModifierArmorMaxBonus")
                .unwrap()
                .value(),
            Some("75%")
        );
    }

    #[test]
    fn an_unconsumed_field_does_not_break_the_read() {
        let document = parse(
            "gamedata.ini",
            "GameData\n  SomeFieldNothingReads = 1\n  SellPercentage = 50%\nEnd\n",
        );
        let merged = merged_game_data(&[document]);
        assert_eq!(merged.field("SellPercentage").unwrap().value(), Some("50%"));
        assert_eq!(
            merged.field("SomeFieldNothingReads").unwrap().value(),
            Some("1")
        );
    }

    #[test]
    fn side_info_merges_per_side_with_the_later_block_winning() {
        let document = parse(
            "aidata.ini",
            "AIData\n  \
                SideInfo Good\n    ResourceGatherersNormal = 3\n  End\n  \
                SideInfo Good\n    ResourceGatherersNormal = 5\n  End\n  \
                SideInfo Evil\n    ResourceGatherersNormal = 2\n  End\n\
             End\n",
        );
        let merged = merged_side_info(&[document]);
        assert_eq!(merged.len(), 2);
        assert_eq!(
            merged["good"]
                .field("ResourceGatherersNormal")
                .unwrap()
                .value(),
            Some("5")
        );
        assert_eq!(
            merged["evil"]
                .field("ResourceGatherersNormal")
                .unwrap()
                .value(),
            Some("2")
        );
    }

    #[test]
    fn a_difficulty_with_no_tuning_block_defaults_to_one_to_one() {
        let document = parse("skirmishaidata.ini", "SkirmishAIData\nEnd\n");
        let rows = skirmish_special_power_activation_tuning(&[document]);
        assert_eq!(rows, [(1, 1); 4]);
    }

    #[test]
    fn a_later_tuning_block_for_the_same_difficulty_replaces_the_whole_row() {
        let document = parse(
            "skirmishaidata.ini",
            "SkirmishAIData\n  \
                DifficultyTuning\n    Difficulty = EASY\n    \
                    SpecialPowerActivationProbability = 1 : 4\n  End\n  \
                DifficultyTuning\n    Difficulty = EASY\n    \
                    SpecialPowerActivationProbability = 1 : 2\n  End\n\
             End\n",
        );
        let rows = skirmish_special_power_activation_tuning(&[document]);
        assert_eq!(rows[0], (1, 2));
    }

    #[test]
    fn a_zero_denominator_is_refused_and_the_row_keeps_its_default() {
        let document = parse(
            "skirmishaidata.ini",
            "SkirmishAIData\n  \
                DifficultyTuning\n    Difficulty = BRUTAL\n    \
                    SpecialPowerActivationProbability = 1 : 0\n  End\n\
             End\n",
        );
        let rows = skirmish_special_power_activation_tuning(&[document]);
        assert_eq!(rows[3], (1, 1));
    }

    #[test]
    fn a_block_of_a_different_keyword_is_not_folded_in() {
        let document = parse(
            "both.ini",
            "GameData\n  SellPercentage = 50%\nEnd\nAIData\n  Wealthy = 7000\nEnd\n",
        );
        let merged = merged_game_data(&[document]);
        assert_eq!(merged.field("SellPercentage").unwrap().value(), Some("50%"));
        assert!(merged.field("Wealthy").is_none());
    }

    #[test]
    fn retreat_tuning_merges_later_writes_and_refuses_a_wrong_type() {
        let first = parse(
            "a.ini",
            "SkirmishAIData\n  FarmingThreshold = -2\n  LogicFramesTillAISelfDestructs = 600\n  DisableTacticalAI = Yes\nEnd\n",
        );
        let second = parse(
            "b.ini",
            "SkirmishAIData\n  LogicFramesTillAISelfDestructs = 90\n  DisableTacticalAI = No\nEnd\n",
        );
        let tuning = wotr_retreat_tuning(&[first, second]).unwrap().unwrap();
        assert_eq!(tuning.farming_threshold, -2);
        assert_eq!(tuning.frames_till_self_destruct, 90);
        assert!(!tuning.disable_tactical_ai);
        let bad = parse("c.ini", "SkirmishAIData\n  FarmingThreshold = high\nEnd\n");
        assert!(wotr_retreat_tuning(&[bad])
            .unwrap_err()
            .contains("FarmingThreshold"));
        assert_eq!(wotr_retreat_tuning(&[]), Ok(None));
    }
}
