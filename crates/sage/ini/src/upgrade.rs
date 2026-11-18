use std::collections::BTreeMap;

use crate::tree::{Block, Document, Field};
use crate::value::Duration;

pub const UPGRADE_KEYWORD: &str = "Upgrade";

pub const UPGRADE_FIELDS: [&str; 15] = [
    "Type",
    "GroupName",
    "DisplayName",
    "GroupOrder",
    "BuildCost",
    "BuildTime",
    "ResearchSound",
    "SkirmishAIHeuristic",
    "UpgradeFX",
    "Tooltip",
    "RequiredObjectFilter",
    "StrategicIcon",
    "SubUpgradeTemplateNames",
    "NoUpgradeDiscount",
    "UseObjectTemplateForCostDiscount",
];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UpgradeDef {
    pub kind: Option<String>,
    pub group_name: Option<String>,
    pub display_name: Option<String>,
    pub group_order: Option<i64>,
    pub build_cost_raw: Option<String>,
    pub build_time_raw: Option<String>,
    pub build_time: Option<Duration>,
    pub research_sound: Option<String>,
    pub skirmish_ai_heuristic: Option<String>,
    pub upgrade_fx: Option<String>,
    pub tooltip: Option<String>,
    pub required_object_filter: Vec<String>,
    pub strategic_icon: Option<String>,
    pub sub_upgrade_template_names: Vec<String>,
    pub no_upgrade_discount: bool,
    pub use_object_template_for_cost_discount: Option<String>,
    pub persists_in_campaign: bool,
}

fn value(block: &Block, keyword: &str) -> Option<String> {
    block
        .field(keyword)
        .and_then(Field::value)
        .map(str::to_owned)
}

#[must_use]
pub fn read_upgrade(block: &Block) -> UpgradeDef {
    let build_time_raw = value(block, "BuildTime");
    let build_time = build_time_raw
        .as_deref()
        .and_then(crate::value::parse_float)
        .map(Duration::Seconds);
    UpgradeDef {
        kind: value(block, "Type"),
        group_name: value(block, "GroupName"),
        display_name: value(block, "DisplayName"),
        group_order: value(block, "GroupOrder")
            .as_deref()
            .and_then(crate::value::parse_int),
        build_cost_raw: value(block, "BuildCost"),
        build_time_raw,
        build_time,
        research_sound: value(block, "ResearchSound"),
        skirmish_ai_heuristic: value(block, "SkirmishAIHeuristic"),
        upgrade_fx: value(block, "UpgradeFX"),
        tooltip: value(block, "Tooltip"),
        required_object_filter: block
            .field("RequiredObjectFilter")
            .map(|f| f.args.clone())
            .unwrap_or_default(),
        strategic_icon: value(block, "StrategicIcon"),
        sub_upgrade_template_names: block
            .field("SubUpgradeTemplateNames")
            .map(|f| f.args.clone())
            .unwrap_or_default(),
        no_upgrade_discount: value(block, "NoUpgradeDiscount")
            .as_deref()
            .and_then(crate::value::parse_bool)
            .unwrap_or(false),
        use_object_template_for_cost_discount: value(block, "UseObjectTemplateForCostDiscount"),
        persists_in_campaign: value(block, "PersistsInCampaign")
            .as_deref()
            .and_then(crate::value::parse_bool)
            .unwrap_or(false),
    }
}

#[derive(Debug, Clone, Default)]
pub struct UpgradeIndex {
    defs: BTreeMap<String, UpgradeDef>,
    definitions: usize,
}

impl UpgradeIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_document(&mut self, document: &Document) {
        for block in document.roots() {
            self.insert_block(block);
        }
    }

    pub fn insert_block(&mut self, block: &Block) {
        if !block.keyword.eq(UPGRADE_KEYWORD) {
            return;
        }
        let Some(name) = block.name() else {
            return;
        };
        self.definitions = self.definitions.saturating_add(1);
        self.defs
            .insert(name.to_ascii_lowercase(), read_upgrade(block));
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&UpgradeDef> {
        self.defs.get(&name.to_ascii_lowercase())
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.defs.keys().map(String::as_str)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
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
    fn an_upgrade_reads_a_numeric_build_time_as_seconds() {
        let doc = crate::tree::Document::parse(
            "upgrade.ini",
            "Upgrade Upgrade_ElfStealth\n  \
                Type = PLAYER\n  \
                DisplayName = UPGRADE:ElfStealthUpgrade\n  \
                BuildCost = 500\n  \
                BuildTime = 15.0\n\
            End\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let def = read_upgrade(&doc.roots()[0]);
        assert_eq!(def.kind.as_deref(), Some("PLAYER"));
        assert_eq!(def.build_cost_raw.as_deref(), Some("500"));
        assert_eq!(def.build_time, Some(Duration::Seconds(15.0)));
    }

    #[test]
    fn chain_and_discount_fields_read_off_their_own_upgrade() {
        let doc = crate::tree::Document::parse(
            "upgrade.ini",
            "Upgrade Upgrade_RohanForgedBlades\n  \
                Type = OBJECT\n  \
                SubUpgradeTemplateNames = Upgrade_RohanForgedBladesForPeasants \
                Upgrade_RohanForgedBladesForElves\n\
            End\n\
            Upgrade Upgrade_GondorCastleUpgrade\n  \
                NoUpgradeDiscount = Yes\n  \
                UseObjectTemplateForCostDiscount = GondorCastleUpgrade\n\
            End\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        let chained = read_upgrade(&doc.roots()[0]);
        assert_eq!(
            chained.sub_upgrade_template_names,
            vec![
                "Upgrade_RohanForgedBladesForPeasants",
                "Upgrade_RohanForgedBladesForElves"
            ]
        );
        assert!(!chained.no_upgrade_discount, "not declared on this block");

        let discounted = read_upgrade(&doc.roots()[1]);
        assert!(discounted.sub_upgrade_template_names.is_empty());
        assert!(discounted.no_upgrade_discount);
        assert_eq!(
            discounted.use_object_template_for_cost_discount.as_deref(),
            Some("GondorCastleUpgrade")
        );
    }

    #[test]
    fn a_macro_valued_build_time_keeps_the_raw_token_and_no_number() {
        let doc = crate::tree::Document::parse(
            "upgrade.ini",
            "Upgrade Upgrade_BattleTower\n  \
                BuildTime = GONDOR_BATTLETOWER_BUILDTIME\n\
            End\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        let def = read_upgrade(&doc.roots()[0]);
        assert_eq!(
            def.build_time_raw.as_deref(),
            Some("GONDOR_BATTLETOWER_BUILDTIME")
        );
        assert_eq!(def.build_time, None);
    }
}
