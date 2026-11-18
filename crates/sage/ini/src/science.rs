use std::collections::BTreeMap;

use crate::tree::{Block, Document, Field};
use crate::value::{field_expression, is_none, parse_bool, MacroTable};

pub const SCIENCE_KEYWORD: &str = "Science";

pub const OR_TOKEN: &str = "OR";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScienceDef {
    pub name: String,
    pub prerequisites: Vec<Vec<String>>,
    pub empty_alternatives: usize,
    pub cost: Option<i64>,
    pub cost_mp: Option<i64>,
    pub unresolved_costs: usize,
    pub grantable: Option<bool>,
}

#[must_use]
pub fn prerequisite_alternatives(tokens: &[String]) -> (Vec<Vec<String>>, usize) {
    if tokens.len() == 1 && tokens.first().is_some_and(|token| is_none(token)) {
        return (Vec::new(), 0);
    }
    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut empty = 0usize;
    let mut close = |current: &mut Vec<String>, groups: &mut Vec<Vec<String>>| {
        if current.is_empty() {
            empty = empty.saturating_add(1);
        } else {
            groups.push(core::mem::take(current));
        }
    };
    for token in tokens {
        if token.eq_ignore_ascii_case(OR_TOKEN) {
            close(&mut current, &mut groups);
        } else if !is_none(token) {
            current.push(token.clone());
        }
    }
    if !tokens.is_empty() {
        close(&mut current, &mut groups);
    }
    (groups, empty)
}

fn cost(block: &Block, keyword: &str, macros: &MacroTable, bad: &mut usize) -> Option<i64> {
    let field = block.field(keyword)?;
    let whole = field_expression(field, macros)
        .filter(|value| value.fract() == 0.0 && value.abs() <= 1_000_000.0)
        .map(|value| {
            #[allow(clippy::cast_possible_truncation)]
            let whole = value as i64;
            whole
        });
    if whole.is_none() {
        *bad = bad.saturating_add(1);
    }
    whole
}

#[must_use]
pub fn read_science(block: &Block, macros: &MacroTable) -> ScienceDef {
    let tokens = block
        .field("PrerequisiteSciences")
        .map(|field| field.args.clone())
        .unwrap_or_default();
    let (prerequisites, empty_alternatives) = prerequisite_alternatives(&tokens);
    let mut unresolved_costs = 0usize;
    ScienceDef {
        name: block.name().unwrap_or_default().to_owned(),
        prerequisites,
        empty_alternatives,
        cost: cost(
            block,
            "SciencePurchasePointCost",
            macros,
            &mut unresolved_costs,
        ),
        cost_mp: cost(
            block,
            "SciencePurchasePointCostMP",
            macros,
            &mut unresolved_costs,
        ),
        unresolved_costs,
        grantable: block
            .field("IsGrantable")
            .and_then(Field::value)
            .and_then(parse_bool),
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScienceIndex {
    defs: BTreeMap<String, ScienceDef>,
    definitions: usize,
}

impl ScienceIndex {
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
        if !block.keyword.eq(SCIENCE_KEYWORD) {
            return;
        }
        if block.name().is_none() {
            return;
        }
        let def = read_science(block, macros);
        self.definitions = self.definitions.saturating_add(1);
        self.defs.insert(def.name.to_ascii_lowercase(), def);
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ScienceDef> {
        self.defs.get(&name.to_ascii_lowercase())
    }

    pub fn iter(&self) -> impl Iterator<Item = &ScienceDef> {
        self.defs.values()
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
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::table::BlockTable;

    fn tokens(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    fn parse(text: &str) -> Document {
        let doc = Document::parse("science.ini", text, &BlockTable::corpus_path()).unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        doc
    }

    #[test]
    fn or_separates_alternatives_and_each_alternative_is_an_and() {
        let (groups, empty) = prerequisite_alternatives(&tokens("A B OR C OR D E F"));
        assert_eq!(
            groups,
            vec![tokens("A B"), tokens("C"), tokens("D E F")],
            "three alternatives"
        );
        assert_eq!(empty, 0);
    }

    #[test]
    fn a_single_list_without_or_is_one_alternative() {
        let (groups, _) = prerequisite_alternatives(&tokens("SCIENCE_ANGMAR SCIENCE_WarChant"));
        assert_eq!(groups, vec![tokens("SCIENCE_ANGMAR SCIENCE_WarChant")]);
    }

    #[test]
    fn none_alone_clears_the_list() {
        assert_eq!(prerequisite_alternatives(&tokens("None")), (Vec::new(), 0));
        assert_eq!(prerequisite_alternatives(&[]), (Vec::new(), 0));
    }

    #[test]
    fn an_empty_alternative_is_dropped_and_counted_never_free() {
        let (groups, empty) = prerequisite_alternatives(&tokens("A OR OR B OR"));
        assert_eq!(groups, vec![tokens("A"), tokens("B")]);
        assert_eq!(empty, 2);
        let (groups, empty) = prerequisite_alternatives(&tokens("OR"));
        assert!(groups.is_empty());
        assert_eq!(empty, 2);
    }

    #[test]
    fn or_is_matched_case_insensitively() {
        let (groups, _) = prerequisite_alternatives(&tokens("A or B"));
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn a_science_block_reads_both_costs_through_macros() {
        let doc = parse(
            "#define GOOD_RANK_2_COST 10\n\
             Science SCIENCE_TomBombadil\n  \
               PrerequisiteSciences = SCIENCE_GOOD SCIENCE_Mist OR SCIENCE_MEN SCIENCE_Rebuild\n  \
               SciencePurchasePointCost = GOOD_RANK_2_COST\n  \
               SciencePurchasePointCostMP = 15\n  \
               IsGrantable = Yes\n\
             End\n",
        );
        let mut macros = MacroTable::new();
        macros.insert_defines(doc.defines());
        let mut index = ScienceIndex::new();
        index.insert_document(&doc, &macros);
        let def = index.get("science_tombombadil").unwrap();
        assert_eq!(def.name, "SCIENCE_TomBombadil");
        assert_eq!(def.cost, Some(10));
        assert_eq!(def.cost_mp, Some(15));
        assert_eq!(def.grantable, Some(true));
        assert_eq!(def.prerequisites.len(), 2);
        assert_eq!(def.unresolved_costs, 0);
        assert_eq!(index.definitions(), 1);
    }

    #[test]
    fn a_cost_that_does_not_resolve_is_counted_not_zero() {
        let doc = parse(
            "Science SCIENCE_X\n  SciencePurchasePointCost = MISSING_MACRO\n  \
             SciencePurchasePointCostMP = 2.5\nEnd\n",
        );
        let mut index = ScienceIndex::new();
        index.insert_document(&doc, &MacroTable::new());
        let def = index.get("SCIENCE_X").unwrap();
        assert_eq!(def.cost, None);
        assert_eq!(def.cost_mp, None, "not a whole number");
        assert_eq!(def.unresolved_costs, 2);
        assert!(def.prerequisites.is_empty());
    }

    #[test]
    fn a_truncated_science_file_never_panics() {
        let text = "#define C 5\nScience SCIENCE_A\n  PrerequisiteSciences = X Y OR Z\n  \
                    SciencePurchasePointCost = C\n  SciencePurchasePointCostMP = #MULTIPLY( C 2 )\n  \
                    IsGrantable = Yes\nEnd\n";
        for end in 0..=text.len() {
            let Some(prefix) = text.get(..end) else {
                continue;
            };
            let Ok(doc) = Document::parse("science.ini", prefix, &BlockTable::corpus_path()) else {
                continue;
            };
            let mut macros = MacroTable::new();
            macros.insert_defines(doc.defines());
            let mut index = ScienceIndex::new();
            index.insert_document(&doc, &macros);
        }
    }

    #[test]
    fn a_corrupted_science_file_never_panics() {
        let text = b"Science SCIENCE_A\n  PrerequisiteSciences = X Y OR Z\n  \
                     SciencePurchasePointCostMP = 10\nEnd\n";
        for at in 0..text.len() {
            for byte in [0u8, b'O', b'R', b'#', b'(', 0xFF] {
                let mut bytes = text.to_vec();
                bytes[at] = byte;
                let text = String::from_utf8_lossy(&bytes);
                if let Ok(doc) = Document::parse("science.ini", &text, &BlockTable::corpus_path()) {
                    let mut index = ScienceIndex::new();
                    index.insert_document(&doc, &MacroTable::new());
                }
            }
        }
    }
}
