use mint_sage_ini::value::{parse_float_or_macro, MacroTable};
use mint_sage_ini::UpgradeIndex;

#[derive(Debug, Clone, PartialEq)]
pub struct UpgradeRecord {
    pub name: String,
    pub is_player: bool,
    pub cost: f32,
    pub build_time_seconds: f32,
    pub display_name: Option<String>,
    pub sub_upgrade_template_names: Vec<String>,
    pub no_upgrade_discount: bool,
    pub use_object_template_for_cost_discount: Option<String>,
}

#[must_use]
pub fn build_upgrade_records(upgrades: &UpgradeIndex, macros: &MacroTable) -> Vec<UpgradeRecord> {
    upgrades
        .names()
        .filter_map(|name| {
            let def = upgrades.get(name)?;
            let cost = def
                .build_cost_raw
                .as_deref()
                .and_then(|raw| parse_float_or_macro(raw, macros))
                .filter(|v| v.is_finite() && *v >= 0.0)?;
            let build_time_seconds = def
                .build_time_raw
                .as_deref()
                .and_then(|raw| parse_float_or_macro(raw, macros))
                .filter(|v| v.is_finite() && *v >= 0.0)?;
            Some(UpgradeRecord {
                name: name.to_owned(),
                is_player: def
                    .kind
                    .as_deref()
                    .is_some_and(|k| k.eq_ignore_ascii_case("PLAYER")),
                cost,
                build_time_seconds,
                display_name: def.display_name.clone(),
                sub_upgrade_template_names: def.sub_upgrade_template_names.clone(),
                no_upgrade_discount: def.no_upgrade_discount,
                use_object_template_for_cost_discount: def
                    .use_object_template_for_cost_discount
                    .clone(),
            })
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use mint_sage_ini::{BlockTable, Document};

    fn index(text: &str) -> UpgradeIndex {
        let doc =
            Document::parse("upgrade.ini", text, &BlockTable::priority_blocks_path()).unwrap();
        let mut index = UpgradeIndex::new();
        index.insert_document(&doc);
        index
    }

    #[test]
    fn a_player_upgrade_resolves_its_cost_and_time() {
        let upgrades = index(
            "Upgrade Upgrade_Oil\n  Type = PLAYER\n  BuildCost = 1500\n  BuildTime = 30\nEnd\n",
        );
        let records = build_upgrade_records(&upgrades, &MacroTable::new());
        assert_eq!(records.len(), 1);
        let oil = &records[0];
        assert!(oil.is_player);
        assert_eq!(oil.cost, 1500.0);
        assert_eq!(oil.build_time_seconds, 30.0);
    }

    #[test]
    fn object_type_and_unset_type_both_default_to_object_scope() {
        let upgrades = index(
            "Upgrade Upgrade_A\n  Type = OBJECT\n  BuildCost = 1\n  BuildTime = 1\nEnd\n\
             Upgrade Upgrade_B\n  BuildCost = 1\n  BuildTime = 1\nEnd\n",
        );
        let records = build_upgrade_records(&upgrades, &MacroTable::new());
        assert!(records.iter().all(|r| !r.is_player));
    }

    #[test]
    fn a_macro_valued_cost_and_time_resolve() {
        let text = "#define GONDOR_BARRACKS_LEVEL2_UPGRADE_COST 600\n\
             #define GONDOR_BARRACKS_LEVEL2_UPGRADE_BUILDTIME 45\n\
             Upgrade Upgrade_Level2\n  Type = OBJECT\n  \
                BuildCost = GONDOR_BARRACKS_LEVEL2_UPGRADE_COST\n  \
                BuildTime = GONDOR_BARRACKS_LEVEL2_UPGRADE_BUILDTIME\nEnd\n";
        let doc =
            Document::parse("upgrade.ini", text, &BlockTable::priority_blocks_path()).unwrap();
        let mut upgrades = UpgradeIndex::new();
        upgrades.insert_document(&doc);
        let mut macros = MacroTable::new();
        macros.insert_defines(doc.defines());
        let records = build_upgrade_records(&upgrades, &macros);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].cost, 600.0);
        assert_eq!(records[0].build_time_seconds, 45.0);
    }

    #[test]
    fn an_unresolvable_cost_drops_the_definition() {
        let upgrades = index(
            "Upgrade Upgrade_Broken\n  BuildCost = SOME_UNDEFINED_MACRO\n  BuildTime = 10\nEnd\n",
        );
        let records = build_upgrade_records(&upgrades, &MacroTable::new());
        assert!(records.is_empty());
    }
}
