use std::collections::BTreeMap;

use mint_sage_ini::diagnostic::ValueErrors;
use mint_sage_ini::value::parse_float_or_macro;
use mint_sage_ini::{Block, Document, MacroTable};

pub const EXPERIENCE_SCALAR_TABLE_KEYWORD: &str = "ExperienceScalarTable";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExperienceScalarReport {
    pub blocks: usize,
    pub dropped: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ExperienceScalarTables {
    tables: BTreeMap<String, Vec<f32>>,
    pub report: ExperienceScalarReport,
}

impl ExperienceScalarTables {
    #[must_use]
    pub fn build(documents: &[Document], macros: &MacroTable, errors: &ValueErrors) -> Self {
        let mut out = Self::default();
        for document in documents {
            for block in document.roots() {
                if !block
                    .keyword
                    .eq_ignore_ascii_case(EXPERIENCE_SCALAR_TABLE_KEYWORD)
                {
                    continue;
                }
                out.report.blocks = out.report.blocks.saturating_add(1);
                let Some(name) = block.name() else {
                    out.report.dropped = out.report.dropped.saturating_add(1);
                    continue;
                };
                let Some(scalars) = read_scalars(block, macros, name, errors) else {
                    out.report.dropped = out.report.dropped.saturating_add(1);
                    continue;
                };
                out.tables.insert(name.to_ascii_lowercase(), scalars);
            }
        }
        out
    }

    #[must_use]
    pub fn table(&self, name: &str) -> Option<&[f32]> {
        self.tables
            .get(&name.to_ascii_lowercase())
            .map(Vec::as_slice)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.tables.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }
}

fn read_scalars(
    block: &Block,
    macros: &MacroTable,
    name: &str,
    errors: &ValueErrors,
) -> Option<Vec<f32>> {
    let mut scalars = Vec::new();
    for field in block.fields_named("Scalars") {
        for arg in &field.args {
            match parse_float_or_macro(arg, macros).filter(|v| v.is_finite() && *v >= 0.0) {
                Some(value) => scalars.push(value),
                None => {
                    errors.value(name, field, arg, "a non-negative finite number");
                    return None;
                }
            }
        }
    }
    Some(scalars)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use mint_sage_ini::BlockTable;

    fn tables(text: &str) -> ExperienceScalarTables {
        let doc = Document::parse("scalars.ini", text, &BlockTable::corpus_path()).unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let macros = MacroTable::new();
        ExperienceScalarTables::build(&[doc], &macros, &ValueErrors::new())
    }

    #[test]
    fn a_table_reads_its_scalars_in_order() {
        let t = tables("ExperienceScalarTable Hero\n  Scalars = 1.0 0.5 0.25\nEnd\n");
        assert_eq!(t.table("Hero").unwrap(), [1.0, 0.5, 0.25]);
        assert!(t.table("NONE").is_none());
    }

    #[test]
    fn a_hostile_scalar_drops_the_whole_table() {
        let t = tables("ExperienceScalarTable Bad\n  Scalars = 1.0 -1.0\nEnd\n");
        assert_eq!(t.report.dropped, 1);
        assert!(t.table("Bad").is_none());
    }

    #[test]
    fn a_malformed_scalar_is_recorded_as_a_value_error() {
        let doc = Document::parse(
            "scalars.ini",
            "ExperienceScalarTable Bad\n  Scalars = 1.0 -1.0\nEnd\n",
            &BlockTable::corpus_path(),
        )
        .unwrap();
        let macros = MacroTable::new();
        let errors = ValueErrors::new();
        let _ = ExperienceScalarTables::build(&[doc], &macros, &errors);
        assert_eq!(errors.take().len(), 1, "the negative Scalars entry errors");
    }
}
