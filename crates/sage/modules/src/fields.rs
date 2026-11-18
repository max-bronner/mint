use mint_sage_ini::value::{field_expression, resolve_percent};
use mint_sage_ini::{Block, MacroTable};
use mint_sim::module::UpgradeMask;
use mint_sim::ModuleFieldSource;

use crate::upgrade_bits::UpgradeBits;

pub struct BlockFields<'a>(pub &'a Block, pub &'a MacroTable, pub &'a UpgradeBits);

impl ModuleFieldSource for BlockFields<'_> {
    fn field(&self, keyword: &str) -> Option<&str> {
        self.field_args(keyword).first().copied()
    }

    fn field_numeric(&self, keyword: &str) -> Option<f32> {
        field_expression(self.0.field(keyword)?, self.1)
    }

    fn field_percent(&self, keyword: &str) -> Option<f32> {
        resolve_percent(&self.0.field(keyword)?.args, self.1)
    }

    fn field_args(&self, keyword: &str) -> Vec<&str> {
        self.0
            .field(keyword)
            .map(|f| match f.args.as_slice() {
                [only] => self.1.tokens(only).map_or_else(
                    || vec![only.as_str()],
                    |list| list.iter().map(String::as_str).collect(),
                ),
                args => args.iter().map(String::as_str).collect(),
            })
            .unwrap_or_default()
    }

    fn field_args_each(&self, keyword: &str) -> Vec<Vec<&str>> {
        self.0
            .fields()
            .filter(|f| f.keyword == keyword)
            .map(|f| f.args.iter().map(String::as_str).collect())
            .collect()
    }

    fn field_upgrade_mask(&self, keyword: &str) -> UpgradeMask {
        self.2.mask(self.field_args(keyword))
    }

    fn field_upgrade_group_mask(&self, keyword: &str) -> UpgradeMask {
        self.2.group_mask(self.field_args(keyword))
    }

    fn nested(&self, keyword: &str, visit: &mut dyn FnMut(&dyn ModuleFieldSource)) {
        for block in self.0.blocks_named(keyword) {
            visit(&BlockFields(block, self.1, self.2));
        }
    }

    fn header_args(&self) -> Vec<&str> {
        self.0.args.iter().map(String::as_str).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sage_ini::{BlockTable, Document};

    #[test]
    fn nested_visits_each_sub_block_in_order() {
        let table = BlockTable::corpus_path();
        let doc = Document::parse(
            "t.ini",
            "Object T\n  Behavior = FireWeaponUpdate Tag_01\n    FireWeaponNugget\n      \
             WeaponName = A\n    End\n    FireWeaponNugget\n      WeaponName = B\n    End\n  \
             End\nEnd\n",
            &table,
        )
        .unwrap();
        let object = &doc.roots()[0];
        let module = object.blocks_named("Behavior").next().unwrap();
        let macros = MacroTable::new();
        let bits = UpgradeBits::default();
        let fields = BlockFields(module, &macros, &bits);
        let mut seen = Vec::new();
        fields.nested("FireWeaponNugget", &mut |nugget| {
            seen.push(nugget.field("WeaponName").map(str::to_owned));
        });
        assert_eq!(seen, vec![Some("A".to_owned()), Some("B".to_owned())]);
    }

    #[test]
    fn reads_a_direct_child_field_strcmp_exact() {
        let table = BlockTable::object_model_path();
        let doc = Document::parse(
            "t.ini",
            "Object T\n  Body = ActiveBody Tag_01\n    MaxHealth = 99999.0\n  End\nEnd\n",
            &table,
        )
        .unwrap();
        let object = &doc.roots()[0];
        let body = object.blocks_named("Body").next().unwrap();
        let macros = MacroTable::new();
        let bits = UpgradeBits::default();
        let fields = BlockFields(body, &macros, &bits);
        assert_eq!(fields.field("MaxHealth"), Some("99999.0"));
        assert_eq!(
            fields.field("maxhealth"),
            None,
            "retail's strcmp does not fold case (B334)"
        );
        assert_eq!(fields.field("NotThere"), None);
    }

    #[test]
    fn field_numeric_falls_back_to_the_macro_table() {
        let table = BlockTable::object_model_path();
        let doc = Document::parse(
            "t.ini",
            "Object T\n  Body = ActiveBody Tag_01\n    MaxHealth = TREE_HEALTH\n  End\nEnd\n",
            &table,
        )
        .unwrap();
        let object = &doc.roots()[0];
        let body = object.blocks_named("Body").next().unwrap();
        let mut macros = MacroTable::new();
        macros.insert_defines(&[mint_sage_ini::Define {
            name: "TREE_HEALTH".to_owned(),
            body: "2000".to_owned(),
            file: std::sync::Arc::from("gamedata.ini"),
            line: 1,
        }]);
        let bits = UpgradeBits::default();
        let fields = BlockFields(body, &macros, &bits);
        assert_eq!(fields.field("MaxHealth"), Some("TREE_HEALTH"));
        assert_eq!(fields.field_numeric("MaxHealth"), Some(2000.0));
    }

    #[test]
    fn field_args_reads_every_whitespace_token() {
        let table = BlockTable::object_model_path();
        let doc = Document::parse(
            "t.ini",
            "Object T\n  Behavior = SubObjectsUpgrade Tag_01\n    ShowSubObjects = Sub_A Sub_B\n  End\nEnd\n",
            &table,
        )
        .unwrap();
        let object = &doc.roots()[0];
        let module = object.blocks_named("Behavior").next().unwrap();
        let macros = MacroTable::new();
        let bits = UpgradeBits::default();
        let fields = BlockFields(module, &macros, &bits);
        assert_eq!(fields.field_args("ShowSubObjects"), vec!["Sub_A", "Sub_B"]);
        assert!(fields.field_args("NotThere").is_empty());
    }

    #[test]
    fn field_upgrade_mask_resolves_names_through_the_upgrade_bit_table() {
        let table = BlockTable::object_model_path();
        let doc = Document::parse(
            "t.ini",
            "Object T\n  Behavior = ArmorUpgrade Tag_01\n    TriggeredBy = Upgrade_A Upgrade_Unknown\n  End\nEnd\n",
            &table,
        )
        .unwrap();
        let object = &doc.roots()[0];
        let module = object.blocks_named("Behavior").next().unwrap();
        let macros = MacroTable::new();

        let upgrade_doc = Document::parse(
            "upgrade.ini",
            "Upgrade Upgrade_A\nEnd\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        let mut upgrade_index = mint_sage_ini::UpgradeIndex::new();
        upgrade_index.insert_document(&upgrade_doc);
        let bits = UpgradeBits::from_index(&upgrade_index);

        let fields = BlockFields(module, &macros, &bits);
        let mask = fields.field_upgrade_mask("TriggeredBy");
        assert!(mask.contains(bits.bit("Upgrade_A").unwrap()));
        assert!(!mask.is_empty());
    }
}
