use std::sync::Arc;

use mint_sage_ini::diagnostic::ValueErrors;
use mint_sage_ini::value::{field_expression, resolve_percent};
use mint_sage_ini::{Block, MacroTable};
use mint_sim::module::UpgradeMask;
use mint_sim::ModuleFieldSource;

use crate::template::ModuleDeclaration;
use crate::upgrade_bits::UpgradeBits;

#[derive(Debug, Clone, PartialEq)]
pub struct CapturedField {
    pub keyword: String,
    pub args: Vec<String>,
    pub numeric: Option<f32>,
    pub percent: Option<f32>,
    pub file: Arc<str>,
    pub line: u32,
}

const MAX_CAPTURED_DEPTH: u32 = 4;

#[derive(Debug, Clone, PartialEq)]
pub struct CapturedModule {
    pub name: String,
    pub tag: Option<String>,
    pub args: Vec<String>,
    pub fields: Vec<CapturedField>,
    pub blocks: Vec<CapturedModule>,
}

impl CapturedModule {
    #[must_use]
    pub fn capture(declaration: &ModuleDeclaration<'_>, macros: &MacroTable) -> Self {
        Self {
            name: declaration.name.to_owned(),
            tag: declaration.block.args.get(1).cloned(),
            args: declaration.block.args.clone(),
            fields: capture_fields(declaration.block, macros),
            blocks: capture_blocks(declaration.block, macros, 0),
        }
    }

    #[must_use]
    pub fn field(&self, keyword: &str) -> Option<&CapturedField> {
        self.fields.iter().find(|field| field.keyword == keyword)
    }

    #[must_use]
    pub const fn source<'a>(&'a self, bits: &'a UpgradeBits) -> CapturedFields<'a> {
        CapturedFields {
            module: self,
            bits,
            refusals: None,
        }
    }

    #[must_use]
    pub const fn recording_source<'a>(
        &'a self,
        bits: &'a UpgradeBits,
        errors: &'a ValueErrors,
        context: &'a str,
    ) -> CapturedFields<'a> {
        CapturedFields {
            module: self,
            bits,
            refusals: Some((errors, context)),
        }
    }
}

fn capture_fields(block: &Block, macros: &MacroTable) -> Vec<CapturedField> {
    block
        .fields()
        .map(|field| {
            let args = match field.args.as_slice() {
                [only] => macros
                    .tokens(only)
                    .map_or_else(|| field.args.clone(), <[String]>::to_vec),
                _ => field.args.clone(),
            };
            CapturedField {
                keyword: field.keyword.clone(),
                args,
                numeric: field_expression(field, macros).filter(|value| value.is_finite()),
                percent: resolve_percent(&field.args, macros).filter(|value| value.is_finite()),
                file: Arc::clone(&field.file),
                line: field.line,
            }
        })
        .collect()
}

fn capture_blocks(block: &Block, macros: &MacroTable, depth: u32) -> Vec<CapturedModule> {
    if depth >= MAX_CAPTURED_DEPTH {
        return Vec::new();
    }
    block
        .blocks()
        .map(|sub| CapturedModule {
            name: sub.keyword.clone(),
            tag: None,
            args: sub.args.clone(),
            fields: capture_fields(sub, macros),
            blocks: capture_blocks(sub, macros, depth.saturating_add(1)),
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub struct CapturedFields<'a> {
    module: &'a CapturedModule,
    bits: &'a UpgradeBits,
    refusals: Option<(&'a ValueErrors, &'a str)>,
}

impl CapturedFields<'_> {
    fn refuse(&self, field: &CapturedField, expected: &'static str) {
        if let Some((errors, context)) = self.refusals {
            errors.at(
                &field.file,
                field.line,
                context,
                &field.keyword,
                &field.args.join(" "),
                expected,
            );
        }
    }
}

impl ModuleFieldSource for CapturedFields<'_> {
    fn field(&self, keyword: &str) -> Option<&str> {
        self.module
            .field(keyword)
            .and_then(|field| field.args.first())
            .map(String::as_str)
    }

    fn field_numeric(&self, keyword: &str) -> Option<f32> {
        let field = self.module.field(keyword)?;
        if field.numeric.is_none() {
            self.refuse(field, "a number");
        }
        field.numeric
    }

    fn field_percent(&self, keyword: &str) -> Option<f32> {
        let field = self.module.field(keyword)?;
        if field.percent.is_none() {
            self.refuse(field, "a percentage");
        }
        field.percent
    }

    fn field_args(&self, keyword: &str) -> Vec<&str> {
        self.module
            .field(keyword)
            .map(|field| field.args.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }

    fn field_args_each(&self, keyword: &str) -> Vec<Vec<&str>> {
        self.module
            .fields
            .iter()
            .filter(|field| field.keyword == keyword)
            .map(|field| field.args.iter().map(String::as_str).collect())
            .collect()
    }

    fn field_upgrade_mask(&self, keyword: &str) -> UpgradeMask {
        self.bits.mask(self.field_args(keyword))
    }

    fn field_upgrade_group_mask(&self, keyword: &str) -> UpgradeMask {
        self.bits.group_mask(self.field_args(keyword))
    }

    fn nested(&self, keyword: &str, visit: &mut dyn FnMut(&dyn ModuleFieldSource)) {
        for block in self
            .module
            .blocks
            .iter()
            .filter(|block| block.name == keyword)
        {
            match self.refusals {
                Some((errors, context)) => {
                    visit(&block.recording_source(self.bits, errors, context));
                }
                None => visit(&block.source(self.bits)),
            }
        }
    }

    fn header_args(&self) -> Vec<&str> {
        self.module.args.iter().map(String::as_str).collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::fields::BlockFields;
    use crate::template::TemplateIndex;
    use mint_sage_ini::{BlockTable, Document};

    #[test]
    fn captured_fields_answer_like_block_fields() {
        let doc = Document::parse(
            "t.ini",
            "#define HP 250\n\
             Object T\n  \
               Behavior = SubObjectsUpgrade Tag_01\n    \
                 TriggeredBy = Upgrade_A Upgrade_Missing\n    \
                 ShowSubObjects = Sub_A Sub_B\n    \
                 FadeTimeInSeconds = HP\n  End\n\
             End\n",
            &BlockTable::object_model_path(),
        )
        .unwrap();
        let upgrades = Document::parse(
            "upgrade.ini",
            "Upgrade Upgrade_A\nEnd\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        let mut index = TemplateIndex::new();
        index.insert_document(&doc);
        index.insert_document(&upgrades);
        let bits = UpgradeBits::from_index(index.upgrades());
        let declarations = index.declared_modules("t");
        let declaration = declarations[0];
        let captured = CapturedModule::capture(&declaration, index.macros());
        assert_eq!(captured.name, "SubObjectsUpgrade");
        assert_eq!(captured.tag.as_deref(), Some("Tag_01"));

        let owned = captured.source(&bits);
        let borrowed = BlockFields(declaration.block, index.macros(), &bits);
        for keyword in [
            "TriggeredBy",
            "ShowSubObjects",
            "FadeTimeInSeconds",
            "Absent",
        ] {
            assert_eq!(owned.field(keyword), borrowed.field(keyword), "{keyword}");
            assert_eq!(
                owned.field_args(keyword),
                borrowed.field_args(keyword),
                "{keyword}"
            );
            assert_eq!(
                owned.field_numeric(keyword),
                borrowed.field_numeric(keyword),
                "{keyword}"
            );
            assert_eq!(
                owned.field_upgrade_mask(keyword),
                borrowed.field_upgrade_mask(keyword),
                "{keyword}"
            );
        }
        assert_eq!(owned.field_numeric("FadeTimeInSeconds"), Some(250.0));
    }

    #[test]
    fn a_present_value_that_is_not_a_number_is_refused_and_an_absent_one_is_not() {
        let doc = Document::parse(
            "t.ini",
            "Object T\n  \
               Behavior = SubObjectsUpgrade Tag_01\n    \
                 FadeTimeInSeconds = soon\n  End\n\
             End\n",
            &BlockTable::object_model_path(),
        )
        .unwrap();
        let mut index = TemplateIndex::new();
        index.insert_document(&doc);
        let bits = UpgradeBits::default();
        let declarations = index.declared_modules("t");
        let captured = CapturedModule::capture(&declarations[0], index.macros());
        let errors = ValueErrors::new();
        let fields = captured.recording_source(&bits, &errors, "T / SubObjectsUpgrade");

        assert_eq!(fields.field_numeric("Absent"), None);
        assert_eq!(fields.field_numeric("FadeTimeInSeconds"), None);
        let refused = errors.take();
        assert_eq!(refused.len(), 1);
        assert_eq!(refused[0].field, "FadeTimeInSeconds");
        assert_eq!(refused[0].line, 3);
    }

    #[test]
    fn a_percent_field_reads_through_a_macro_and_refuses_a_non_number() {
        let doc = Document::parse(
            "t.ini",
            "#define OPACITY 100.0%\n\
             Object T\n  \
               Behavior = StealthUpdate Tag_01\n    \
                 FriendlyOpacityMin = OPACITY\n    \
                 FriendlyOpacityMax = sometimes\n  End\n\
             End\n",
            &BlockTable::object_model_path(),
        )
        .unwrap();
        let mut index = TemplateIndex::new();
        index.insert_document(&doc);
        let bits = UpgradeBits::default();
        let declarations = index.declared_modules("t");
        let captured = CapturedModule::capture(&declarations[0], index.macros());
        let errors = ValueErrors::new();
        let fields = captured.recording_source(&bits, &errors, "T / StealthUpdate");

        assert_eq!(fields.field_percent("FriendlyOpacityMin"), Some(1.0));
        assert_eq!(fields.field_percent("FriendlyOpacityMax"), None);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn nested_sub_blocks_answer_the_same_owned_or_borrowed() {
        let doc = Document::parse(
            "t.ini",
            "#define DELAY 2500\n\
             Object T\n  \
               Behavior = FireWeaponUpdate Tag_01\n    \
                 FireWeaponNugget\n      WeaponName = A\n      FireDelay = 0\n    End\n    \
                 FireWeaponNugget\n      WeaponName = B\n      FireDelay = DELAY\n    End\n  \
               End\n\
             End\n",
            &BlockTable::corpus_path(),
        )
        .unwrap();
        let mut index = TemplateIndex::new();
        index.insert_document(&doc);
        let bits = UpgradeBits::default();
        let declarations = index.declared_modules("t");
        let declaration = declarations[0];
        let captured = CapturedModule::capture(&declaration, index.macros());
        assert_eq!(captured.blocks.len(), 2);
        let read = |source: &dyn ModuleFieldSource| {
            let mut out = Vec::new();
            source.nested("FireWeaponNugget", &mut |nugget| {
                out.push((
                    nugget.field("WeaponName").map(str::to_owned),
                    nugget.field_numeric("FireDelay"),
                ));
            });
            out
        };
        let owned = read(&captured.source(&bits));
        let borrowed = read(&BlockFields(declaration.block, index.macros(), &bits));
        assert_eq!(owned, borrowed);
        assert_eq!(owned[1], (Some("B".to_owned()), Some(2500.0)));
        assert!(read_named(&captured, &bits, "Other").is_empty());
    }

    fn read_named(module: &CapturedModule, bits: &UpgradeBits, keyword: &str) -> Vec<String> {
        let mut out = Vec::new();
        module.source(bits).nested(keyword, &mut |nugget| {
            out.push(nugget.field("WeaponName").unwrap_or("").to_owned());
        });
        out
    }

    #[test]
    fn a_list_macro_expands_the_same_owned_or_borrowed() {
        let doc = Document::parse(
            "t.ini",
            "#define FILTER ANY +INFANTRY -HERO\n\
             Object T\n  \
               Behavior = SpecialPowerModule Tag_01\n    \
                 AttributeModifierAffects = FILTER\n    \
                 AttributeModifierRange = 100\n  End\n\
             End\n",
            &BlockTable::object_model_path(),
        )
        .unwrap();
        let mut index = TemplateIndex::new();
        index.insert_document(&doc);
        let bits = UpgradeBits::default();
        let declarations = index.declared_modules("t");
        let declaration = declarations[0];
        let captured = CapturedModule::capture(&declaration, index.macros());
        let owned = captured.source(&bits);
        let borrowed = BlockFields(declaration.block, index.macros(), &bits);
        let expanded = vec!["ANY", "+INFANTRY", "-HERO"];
        assert_eq!(owned.field_args("AttributeModifierAffects"), expanded);
        assert_eq!(borrowed.field_args("AttributeModifierAffects"), expanded);
        assert_eq!(
            owned.field("AttributeModifierAffects"),
            borrowed.field("AttributeModifierAffects")
        );
        assert_eq!(owned.field_numeric("AttributeModifierRange"), Some(100.0));
    }
}
