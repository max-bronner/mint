use std::collections::BTreeMap;
use std::sync::Arc;

use crate::tree::{Block, Document, Field, Node};

pub const GAME_DATA_KEYWORD: &str = "GameData";
pub const AI_DATA_KEYWORD: &str = "AIData";
pub const SIDE_INFO_KEYWORD: &str = "SideInfo";
pub const SKIRMISH_AI_DATA_KEYWORD: &str = "SkirmishAIData";
pub const DIFFICULTY_TUNING_KEYWORD: &str = "DifficultyTuning";
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
}
