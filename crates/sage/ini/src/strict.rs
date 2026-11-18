use std::sync::Arc;

use crate::diagnostic::ValueError;
use crate::tables::field_table::{self, Context, Titles};
use crate::tree::{Block, Diagnostic, Document, Node};
use crate::value::MacroDiagnostic;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StrictError {
    #[error("{file}:{line}: unknown field `{token}` in `{context}`")]
    UnknownField {
        file: Arc<str>,
        line: u32,
        context: String,
        token: String,
    },
    #[error("{file}:{line}: unknown file-scope block `{keyword}`")]
    UnknownFileScopeBlock {
        file: Arc<str>,
        line: u32,
        keyword: String,
    },
    #[error("{0}")]
    Structural(Diagnostic),
    #[error("{0}")]
    Macro(MacroDiagnostic),
    #[error("{0}")]
    Value(ValueError),
}

pub const TYPED_READER_BLOCKS: &[&str] = &[
    "StrategicHUD",
    "LivingWorldRegionEffects",
    "AutoResolveReinforcementSchedule",
];

pub const UNBOUND_LEGEND_BLOCKS: &[&str] = &[
    "AerialPathfindNoFlyZone",
    "AudioLowMHz",
    "BenchProfile",
    "EvaEventForwardReference",
    "LODPreset",
    "MissionObjectiveList",
    "ParticleSystem",
    "ReallyLowMHz",
    "StanceTemplate",
];

pub const UNIMPLEMENTED_SUBSYSTEM_FILES: &[&str] = &["data/ini/credits.ini"];

#[derive(Debug, Clone, Default)]
pub struct StrictReport {
    pub errors: Vec<StrictError>,
    pub unchecked: u64,
}

impl StrictReport {
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty()
    }
}

#[must_use]
pub fn check(
    document: &Document,
    macros: &crate::value::MacroTable,
    title: Titles,
) -> StrictReport {
    let mut report = StrictReport::default();
    let unimplemented_subsystem = UNIMPLEMENTED_SUBSYSTEM_FILES.contains(&document.name());
    for diagnostic in document.diagnostics() {
        match diagnostic {
            Diagnostic::IncludeMissing { .. } => {}
            _ if unimplemented_subsystem => {
                report.unchecked = report.unchecked.saturating_add(1);
            }
            Diagnostic::FieldAtFileScope { keyword, .. }
                if UNBOUND_LEGEND_BLOCKS.contains(&keyword.as_str()) =>
            {
                report.unchecked = report.unchecked.saturating_add(1);
            }
            _ => report
                .errors
                .push(StrictError::Structural(diagnostic.clone())),
        }
    }
    for diagnostic in macros.diagnostics() {
        report.errors.push(StrictError::Macro(diagnostic.clone()));
    }
    for root in document.roots() {
        if TYPED_READER_BLOCKS.contains(&root.keyword.as_str()) {
            report.errors.extend(
                crate::living_world::check_block(root, macros)
                    .into_iter()
                    .map(StrictError::Value),
            );
            continue;
        }
        match field_table::file_scope(&root.keyword) {
            Some(context) => walk(root, context, &root.keyword, title, &mut report),
            None if UNBOUND_LEGEND_BLOCKS.contains(&root.keyword.as_str()) => {
                report.unchecked = report.unchecked.saturating_add(count_statements(root));
            }
            None => report.errors.push(StrictError::UnknownFileScopeBlock {
                file: Arc::clone(&root.file),
                line: root.line,
                keyword: root.keyword.clone(),
            }),
        }
    }
    report
}

const UNKNOWN_NOT_INVALID: &[(&str, &str)] = &[("ButtonSet", "Mode")];

fn walk(block: &Block, context: &Context, label: &str, title: Titles, report: &mut StrictReport) {
    for node in &block.children {
        let (token, file, line) = match node {
            Node::Field(f) => (f.keyword.as_str(), &f.file, f.line),
            Node::Block(b) => (b.keyword.as_str(), &b.file, b.line),
            Node::Script(s) => ("BeginScript", &s.file, s.line),
        };
        if !context.accepts(token, title) {
            if UNKNOWN_NOT_INVALID.contains(&(label, token)) {
                report.unchecked = report.unchecked.saturating_add(1);
            } else {
                report.errors.push(StrictError::UnknownField {
                    file: Arc::clone(file),
                    line,
                    context: label.to_owned(),
                    token: token.to_owned(),
                });
            }
            continue;
        }
        let Node::Block(child) = node else { continue };
        let arg = child.args.first().map(String::as_str);
        match context.child(&child.keyword, arg, title) {
            Some(next) => {
                let child_label = format!("{label}/{}", child.keyword);
                walk(child, next, &child_label, title, report);
            }
            None => {
                report.unchecked = report.unchecked.saturating_add(count_statements(child));
            }
        }
    }
}

fn count_statements(block: &Block) -> u64 {
    let mut total = 0u64;
    for node in &block.children {
        total = total.saturating_add(1);
        if let Node::Block(child) = node {
            total = total.saturating_add(count_statements(child));
        }
    }
    total
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::table::BlockTable;
    use crate::value::MacroTable;

    fn parse(text: &str) -> Document {
        Document::parse("t.ini", text, &BlockTable::corpus_path()).unwrap()
    }

    fn parse_named(name: &str, text: &str) -> Document {
        Document::parse(name, text, &BlockTable::corpus_path()).unwrap()
    }

    #[test]
    fn an_unknown_field_in_a_known_block_errors_with_file_and_line() {
        let doc = parse("Object T\n  NotARealField = 1\nEnd\n");
        let report = check(&doc, &MacroTable::new(), Titles::BOTH);
        assert_eq!(report.errors.len(), 1);
        assert!(matches!(
            &report.errors[0],
            StrictError::UnknownField { line: 2, token, .. } if token == "NotARealField"
        ));
    }

    #[test]
    fn a_new_object_with_only_known_fields_is_clean() {
        let doc = parse("Object MyNewThing\n  KindOf = INFANTRY\n  Side = MyFaction\nEnd\n");
        let report = check(&doc, &MacroTable::new(), Titles::BOTH);
        assert!(report.is_clean(), "{:?}", report.errors);
    }

    #[test]
    fn a_module_registered_by_a_new_name_is_unchecked_not_refused() {
        let doc = parse(
            "Object T\n  Behavior = SomeModNoTitleShipped Tag_01\n    Anything = 1\n  End\nEnd\n",
        );
        let report = check(&doc, &MacroTable::new(), Titles::BOTH);
        assert!(report.is_clean(), "{:?}", report.errors);
        assert!(report.unchecked > 0);
    }

    #[test]
    fn an_unknown_field_in_special_disguise_update_errors() {
        let doc = parse(
            "Object T\n  Behavior = SpecialDisguiseUpdate Tag_01\n    NotARealField = 1\n  End\nEnd\n",
        );
        let report = check(&doc, &MacroTable::new(), Titles::BOTH);
        assert_eq!(report.errors.len(), 1);
        assert!(matches!(
            &report.errors[0],
            StrictError::UnknownField { token, .. } if token == "NotARealField"
        ));
    }

    #[test]
    fn a_new_key_in_a_wildcard_block_is_clean() {
        let doc = parse(
            "Object T\n  UnitSpecificSounds\n    AnyModderInventedKey = SomeSound\n  End\nEnd\n",
        );
        let report = check(&doc, &MacroTable::new(), Titles::BOTH);
        assert!(report.is_clean(), "{:?}", report.errors);
    }

    #[test]
    fn a_block_with_a_load_time_field_table_is_checked_by_its_typed_reader() {
        for (text, bad) in [
            ("StrategicHUD\n  Whatever = 1\nEnd\n", 1),
            (
                "LivingWorldRegionEffects Foo\n  BordersEffect\n    Time = 1\n  End\nEnd\n",
                1,
            ),
            (
                "AutoResolveReinforcementSchedule Attacker\n  #1 = 0\nEnd\n",
                1,
            ),
            (
                "StrategicHUD\n  OptionsButton\n    Title = T\n  End\nEnd\n\
                 LivingWorldRegionEffects Foo\n  BordersEffect\n    Opacity = 1\n  End\nEnd\n\
                 AutoResolveReinforcementSchedule Attacker\n  1 = 0\n  EachRemaining = +1\nEnd\n",
                0,
            ),
        ] {
            let report = check(&parse(text), &MacroTable::new(), Titles::BOTH);
            assert_eq!(report.errors.len(), bad, "{text}: {:?}", report.errors);
        }
    }

    #[test]
    fn a_legend_registered_block_with_no_editor_table_is_unchecked() {
        let doc = parse("ParticleSystem Foo\n  VolumeType = SPHERE\nEnd\n");
        let report = check(&doc, &MacroTable::new(), Titles::BOTH);
        assert!(report.is_clean(), "{:?}", report.errors);
        assert_eq!(report.unchecked, 1);
    }

    #[test]
    fn a_leaf_registered_at_file_scope_is_unchecked_not_refused() {
        let doc = parse("LODPreset = Low\nLODPreset = Medium\n");
        let report = check(&doc, &MacroTable::new(), Titles::BOTH);
        assert!(report.is_clean(), "{:?}", report.errors);
        assert_eq!(report.unchecked, 2);
    }

    #[test]
    fn credits_ini_is_unchecked_by_its_own_known_path_not_refused() {
        let doc = parse_named("data/ini/credits.ini", "ScrollRate = 1\nText = Hello\n");
        assert!(doc.roots().is_empty());
        let report = check(&doc, &MacroTable::new(), Titles::BOTH);
        assert!(report.is_clean(), "{:?}", report.errors);
        assert_eq!(report.unchecked, 2);
    }

    #[test]
    fn a_garbage_only_file_with_no_recognised_block_is_refused() {
        let doc = parse_named("data/ini/garbage.ini", "ScrollRate = 1\nText = Hello\n");
        assert!(doc.roots().is_empty());
        let report = check(&doc, &MacroTable::new(), Titles::BOTH);
        assert!(!report.is_clean());
    }

    #[test]
    fn a_genuinely_unknown_file_scope_block_errors() {
        let table = BlockTable::from_rules(&[("", "MadeUpBlockNoTableHas", true)]);
        let doc =
            Document::parse("t.ini", "MadeUpBlockNoTableHas\n  X = 1\nEnd\n", &table).unwrap();
        let report = check(&doc, &MacroTable::new(), Titles::BOTH);
        assert!(matches!(
            report.errors.as_slice(),
            [StrictError::UnknownFileScopeBlock { line: 1, .. }]
        ));
    }

    #[test]
    fn an_unbalanced_document_errors() {
        let doc = parse("Object T\n  KindOf = INFANTRY\n");
        let report = check(&doc, &MacroTable::new(), Titles::BOTH);
        assert!(report
            .errors
            .iter()
            .any(|e| matches!(e, StrictError::Structural(Diagnostic::UnclosedBlock { .. }))));
    }

    #[test]
    fn a_missing_include_stays_non_fatal() {
        let doc = parse("#include \"gone.inc\"\nObject T\n  KindOf = INFANTRY\nEnd\n");
        let report = check(&doc, &MacroTable::new(), Titles::BOTH);
        assert!(report.is_clean(), "{:?}", report.errors);
    }

    #[test]
    fn a_macro_diagnostic_becomes_an_error() {
        let doc = parse("Object T\n  KindOf = INFANTRY\nEnd\n");
        let mut macros = MacroTable::new();
        macros.insert_defines(&[crate::tree::Define {
            name: "lowercase_name".to_owned(),
            body: "1".to_owned(),
            file: Arc::from("t.ini"),
            line: 1,
        }]);
        let report = check(&doc, &macros, Titles::BOTH);
        assert!(report
            .errors
            .iter()
            .any(|e| matches!(e, StrictError::Macro(MacroDiagnostic::NotUpperCase { .. }))));
    }
}
