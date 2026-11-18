use mint_sage_ini::value::{parse_bool, parse_percent};
use mint_sage_ini::{Block, Field, MacroTable};

use crate::object_filter::ObjectFilter;
use crate::template::ModuleDeclaration;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CostModifierRecord {
    pub triggered_by: Vec<String>,
    pub conflicts_with: Vec<String>,
    pub requires_all_triggers: bool,
    pub starts_active: bool,
    pub percentages: Vec<f32>,
    pub object_filter: ObjectFilter,
    pub upgrade_discount: bool,
    pub apply_to_these_upgrades: Vec<String>,
    pub label: Option<String>,
}

impl CostModifierRecord {
    fn read(block: &Block, macros: &MacroTable) -> Self {
        let flag = |keyword: &str| {
            block
                .field(keyword)
                .and_then(Field::value)
                .and_then(parse_bool)
                .unwrap_or(false)
        };
        let names = |keyword: &str| field_tokens(block, keyword, macros);
        Self {
            triggered_by: names("TriggeredBy"),
            conflicts_with: names("ConflictsWith"),
            requires_all_triggers: flag("RequiresAllTriggers"),
            starts_active: flag("StartsActive"),
            percentages: block
                .fields_named("Percentage")
                .filter_map(Field::value)
                .filter_map(parse_percent)
                .collect(),
            object_filter: ObjectFilter::parse(&names("ObjectFilter")),
            upgrade_discount: flag("UpgradeDiscount"),
            apply_to_these_upgrades: names("ApplyToTheseUpgrades"),
            label: block
                .field("LabelForPalantirString")
                .and_then(Field::value)
                .map(str::to_owned),
        }
    }
}

fn field_tokens(block: &Block, keyword: &str, macros: &MacroTable) -> Vec<String> {
    block
        .field(keyword)
        .map(|field| match field.args.as_slice() {
            [only] => macros
                .tokens(only)
                .map_or_else(|| vec![only.clone()], <[String]>::to_vec),
            args => args.to_vec(),
        })
        .unwrap_or_default()
}

pub(crate) fn cost_modifiers(
    modules: &[ModuleDeclaration<'_>],
    macros: &MacroTable,
) -> Vec<CostModifierRecord> {
    modules
        .iter()
        .filter(|decl| decl.name.eq_ignore_ascii_case("CostModifierUpgrade"))
        .map(|decl| CostModifierRecord::read(decl.block, macros))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sage_ini::{BlockTable, Document};

    fn modules<'a>(doc: &'a Document) -> Vec<ModuleDeclaration<'a>> {
        doc.roots()[0]
            .blocks_named("Behavior")
            .map(|block| ModuleDeclaration {
                name: block.args.first().map_or("", String::as_str),
                block,
            })
            .collect()
    }

    #[test]
    fn percentages_object_filter_and_starts_active_are_read_in_order() {
        let table = BlockTable::corpus_path();
        let doc = Document::parse(
            "t.ini",
            "Object TestStatue\n  Behavior = CostModifierUpgrade ModuleTag_01\n    \
             StartsActive = Yes\n    ObjectFilter = ANY +TEST_DISCOUNT_KIND\n    \
             Percentage = -4%\n    Percentage = -8%\n    Percentage = -12%\n    \
             LabelForPalantirString = TEST:Discount\n  End\nEnd\n",
            &table,
        )
        .unwrap();
        let macros = MacroTable::new();
        let records = cost_modifiers(&modules(&doc), &macros);
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert!(record.starts_active);
        assert_eq!(record.percentages, [-0.04, -0.08, -0.12]);
        assert!(record
            .object_filter
            .passes(&[String::from("TEST_DISCOUNT_KIND")], "Anything"));
        assert_eq!(record.label.as_deref(), Some("TEST:Discount"));
        assert!(!record.upgrade_discount);
    }

    #[test]
    fn a_triggered_upgrade_discount_reads_its_own_fields() {
        let table = BlockTable::corpus_path();
        let doc = Document::parse(
            "t.ini",
            "Object TestFortress\n  Behavior = CostModifierUpgrade ModuleTag_02\n    \
             TriggeredBy = Upgrade_TestEconomy\n    UpgradeDiscount = Yes\n    \
             ApplyToTheseUpgrades = Upgrade_TestSpell\n    Percentage = -50%\n  End\nEnd\n",
            &table,
        )
        .unwrap();
        let macros = MacroTable::new();
        let records = cost_modifiers(&modules(&doc), &macros);
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert!(!record.starts_active);
        assert_eq!(record.triggered_by, ["Upgrade_TestEconomy"]);
        assert!(record.upgrade_discount);
        assert_eq!(record.apply_to_these_upgrades, ["Upgrade_TestSpell"]);
    }

    #[test]
    fn a_template_with_no_cost_modifier_upgrade_reads_an_empty_list() {
        let table = BlockTable::corpus_path();
        let doc = Document::parse("t.ini", "Object Plain\nEnd\n", &table).unwrap();
        let macros = MacroTable::new();
        assert!(cost_modifiers(&modules(&doc), &macros).is_empty());
    }
}
