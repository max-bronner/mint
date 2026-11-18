use std::collections::BTreeMap;

use crate::tree::{Block, Document};
use crate::value::{keyed_pairs, resolve_percent, MacroTable};

pub const ARMOR_KEYWORD: &str = "Armor";

pub const ARMOR_FIELDS: [&str; 3] = ["Armor", "FlankedPenalty", "DamageScalar"];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ArmorSet {
    pub by_damage_type: BTreeMap<String, f32>,
    pub flanked_penalty: Option<f32>,
    pub damage_scalar: Option<f32>,
}

impl ArmorSet {
    #[must_use]
    pub fn percent_for(&self, damage_type: &str) -> Option<f32> {
        self.by_damage_type
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(damage_type))
            .map(|(_, percent)| *percent)
    }
}

#[must_use]
pub fn read_armor(block: &Block, macros: &MacroTable) -> ArmorSet {
    let mut by_damage_type = BTreeMap::new();
    for (damage_type, rest) in keyed_pairs(block.fields(), ARMOR_KEYWORD) {
        if let Some(percent) = resolve_percent(&rest, macros) {
            by_damage_type.insert(damage_type.to_ascii_uppercase(), percent);
        }
    }
    ArmorSet {
        by_damage_type,
        flanked_penalty: block
            .field("FlankedPenalty")
            .and_then(|field| resolve_percent(&field.args, macros)),
        damage_scalar: block
            .field("DamageScalar")
            .and_then(|field| resolve_percent(&field.args, macros)),
    }
}

#[derive(Debug, Clone, Default)]
pub struct ArmorIndex {
    sets: BTreeMap<String, ArmorSet>,
    definitions: usize,
}

impl ArmorIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_document(&mut self, document: &Document, macros: &MacroTable) {
        for block in document.roots() {
            self.insert_block(block, macros);
        }
    }

    pub fn insert_block(&mut self, block: &Block, macros: &MacroTable) {
        if !block.keyword.eq(ARMOR_KEYWORD) {
            return;
        }
        let Some(name) = block.name() else {
            return;
        };
        self.definitions = self.definitions.saturating_add(1);
        self.sets
            .insert(name.to_ascii_lowercase(), read_armor(block, macros));
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ArmorSet> {
        self.sets.get(&name.to_ascii_lowercase())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.sets.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }

    #[must_use]
    pub fn definitions(&self) -> usize {
        self.definitions
    }
}

#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]
mod tests {
    use super::*;
    use crate::table::BlockTable;

    #[test]
    fn an_armor_set_reads_its_per_damage_type_percents_and_two_scalars() {
        let doc = crate::tree::Document::parse(
            "armor.ini",
            "Armor GondorSoldierArmor\n  \
                Armor = DEFAULT 100%\n  \
                Armor = SIEGE 50%\n  \
                FlankedPenalty = 50%\n  \
                DamageScalar = 96%\n\
            End\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let block = &doc.roots()[0];
        let armor = read_armor(block, &MacroTable::new());
        assert_eq!(armor.percent_for("default"), Some(1.0));
        assert_eq!(armor.percent_for("SIEGE"), Some(0.5));
        assert_eq!(armor.percent_for("PIERCE"), None);
        assert_eq!(armor.flanked_penalty, Some(0.5));
        assert_eq!(armor.damage_scalar, Some(0.96));
    }

    #[test]
    fn a_definition_with_no_scalars_is_the_damage_table_alone() {
        let doc = crate::tree::Document::parse(
            "armor.ini",
            "Armor Bare\n  Armor = DEFAULT 100%\nEnd\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        let armor = read_armor(&doc.roots()[0], &MacroTable::new());
        assert_eq!(armor.flanked_penalty, None);
        assert_eq!(armor.damage_scalar, None);
        assert_eq!(armor.by_damage_type.len(), 1);
    }

    #[test]
    fn the_index_holds_every_definition_by_name() {
        let doc = crate::tree::Document::parse(
            "armor.ini",
            "Armor A\n  Armor = DEFAULT 100%\nEnd\nArmor B\n  Armor = DEFAULT 50%\nEnd\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        let mut index = ArmorIndex::new();
        index.insert_document(&doc, &MacroTable::new());
        assert_eq!(index.len(), 2);
        assert_eq!(index.definitions(), 2);
        assert_eq!(index.get("a").unwrap().percent_for("DEFAULT"), Some(1.0));
        assert!(index.get("missing").is_none());
    }

    #[test]
    fn an_armor_row_naming_a_macro_resolves_to_a_number() {
        let doc = crate::tree::Document::parse(
            "armor.ini",
            "#define BERSERKER_RAGE_ARMOR_L1 50%\n\
             #define FLANK_PENALTY_MACRO 25\n\
             Armor BerserkerArmor\n  \
                Armor = DEFAULT BERSERKER_RAGE_ARMOR_L1\n  \
                FlankedPenalty = FLANK_PENALTY_MACRO\n\
             End\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        let mut macros = MacroTable::new();
        macros.insert_defines(doc.defines());
        let armor = read_armor(&doc.roots()[0], &macros);
        assert_eq!(armor.percent_for("DEFAULT"), Some(0.5));
        assert_eq!(armor.flanked_penalty, Some(0.25));
    }
}
