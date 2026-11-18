use std::collections::BTreeMap;

use crate::tree::{Block, Document, Field};
use crate::value::{field_expression, parse_int, MacroTable};

pub const RANK_KEYWORD: &str = "Rank";

const SKILL_POINTS_NEEDED: &str = "SkillPointsNeeded";

const MAX_POINTS: f32 = 100_000_000.0;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RankDef {
    pub level: u32,
    pub skill_points_needed: Vec<(String, u32)>,
    pub purchase_points_granted: u32,
    pub sciences_granted: Vec<String>,
    pub unresolved: usize,
}

impl RankDef {
    #[must_use]
    pub fn skill_points(&self, suffix: &str) -> Option<u32> {
        self.skill_points_needed
            .iter()
            .find(|(held, _)| held.eq_ignore_ascii_case(suffix))
            .map(|(_, points)| *points)
    }
}

fn points(field: &Field, macros: &MacroTable) -> Option<u32> {
    let value = field_expression(field, macros)?;
    if !(0.0..=MAX_POINTS).contains(&value) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = value as u32;
    Some(whole)
}

#[must_use]
pub fn read_rank(block: &Block, macros: &MacroTable) -> Option<RankDef> {
    let level = block
        .name()
        .and_then(parse_int)
        .and_then(|level| u32::try_from(level).ok())
        .filter(|level| *level >= 1)?;
    let mut def = RankDef {
        level,
        ..RankDef::default()
    };
    for field in block.fields() {
        let keyword = field.keyword.as_str();
        if let Some(suffix) = keyword
            .get(..SKILL_POINTS_NEEDED.len())
            .filter(|head| *head == SKILL_POINTS_NEEDED)
            .and_then(|_| keyword.get(SKILL_POINTS_NEEDED.len()..))
        {
            match points(field, macros) {
                Some(points) => def.skill_points_needed.push((suffix.to_owned(), points)),
                None => def.unresolved = def.unresolved.saturating_add(1),
            }
        } else if keyword.eq("SciencePurchasePointsGranted") {
            match points(field, macros) {
                Some(points) => def.purchase_points_granted = points,
                None => def.unresolved = def.unresolved.saturating_add(1),
            }
        } else if keyword.eq("SciencesGranted") {
            def.sciences_granted = field
                .args
                .iter()
                .filter(|name| !crate::value::is_none(name))
                .cloned()
                .collect();
        }
    }
    Some(def)
}

#[derive(Debug, Clone, Default)]
pub struct RankTable {
    ranks: Vec<RankDef>,
    pub dropped: usize,
    pub unresolved: usize,
}

impl RankTable {
    #[must_use]
    pub fn build(documents: &[Document], macros: &MacroTable) -> Self {
        Self::build_from_blocks(documents.iter().flat_map(Document::roots), macros)
    }

    #[must_use]
    pub fn build_from_blocks<'b>(
        blocks: impl IntoIterator<Item = &'b Block>,
        macros: &MacroTable,
    ) -> Self {
        let mut by_level: BTreeMap<u32, RankDef> = BTreeMap::new();
        let mut unreadable = 0usize;
        for block in blocks {
            if !block.keyword.eq(RANK_KEYWORD) {
                continue;
            }
            match read_rank(block, macros) {
                Some(def) => {
                    by_level.insert(def.level, def);
                }
                None => unreadable = unreadable.saturating_add(1),
            }
        }
        Self::from_levels(by_level, unreadable)
    }

    fn from_levels(by_level: BTreeMap<u32, RankDef>, unreadable: usize) -> Self {
        let mut table = Self {
            dropped: unreadable,
            ..Self::default()
        };
        let mut expected = 1u32;
        for (level, def) in by_level {
            if level == expected && table.dropped == unreadable {
                table.unresolved = table.unresolved.saturating_add(def.unresolved);
                table.ranks.push(def);
                expected = expected.saturating_add(1);
            } else {
                table.dropped = table.dropped.saturating_add(1);
            }
        }
        table
    }

    #[must_use]
    pub fn get(&self, level: u32) -> Option<&RankDef> {
        let index = usize::try_from(level.checked_sub(1)?).ok()?;
        self.ranks.get(index)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.ranks.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ranks.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &RankDef> {
        self.ranks.iter()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::table::BlockTable;

    fn table(text: &str) -> RankTable {
        let doc = Document::parse("rank.ini", text, &BlockTable::corpus_path()).unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let mut macros = MacroTable::new();
        macros.insert_defines(doc.defines());
        RankTable::build(&[doc], &macros)
    }

    #[test]
    fn a_rank_reads_its_thresholds_by_suffix_and_its_grant() {
        let ranks = table(
            "#define DELTA 60\n#define GRANTED 1\n\
             Rank 1\n  SkillPointsNeededDefault = 0\n  SkillPointsNeededCampaign = 0\n  \
               SciencePurchasePointsGranted = 5\nEnd\n\
             Rank 2\n  SkillPointsNeededDefault = #MULTIPLY( DELTA 1 )\n  \
               SkillPointsNeededCampaign = 100\n  SciencePurchasePointsGranted = GRANTED\n  \
               SciencesGranted = SCIENCE_A SCIENCE_B\nEnd\n",
        );
        assert_eq!(ranks.len(), 2);
        let one = ranks.get(1).unwrap();
        assert_eq!(one.purchase_points_granted, 5);
        let two = ranks.get(2).unwrap();
        assert_eq!(two.skill_points("default"), Some(60));
        assert_eq!(two.skill_points("Campaign"), Some(100));
        assert_eq!(two.skill_points("Men"), None);
        assert_eq!(two.purchase_points_granted, 1);
        assert_eq!(two.sciences_granted, vec!["SCIENCE_A", "SCIENCE_B"]);
        assert!(ranks.get(0).is_none() && ranks.get(3).is_none());
        assert_eq!((ranks.dropped, ranks.unresolved), (0, 0));
    }

    #[test]
    fn the_zero_hour_keyword_is_the_empty_suffix() {
        let ranks = table("Rank 1\n  SkillPointsNeeded = 7\nEnd\n");
        assert_eq!(ranks.get(1).unwrap().skill_points(""), Some(7));
    }

    #[test]
    fn ranks_stop_at_the_first_gap_and_count_the_rest() {
        let ranks = table(
            "Rank 1\n  SkillPointsNeededDefault = 0\nEnd\n\
             Rank 3\n  SkillPointsNeededDefault = 120\nEnd\n\
             Rank 2\n  SkillPointsNeededDefault = 60\nEnd\n\
             Rank 5\n  SkillPointsNeededDefault = 240\nEnd\n\
             Rank X\nEnd\n",
        );
        assert_eq!(ranks.len(), 3, "1-3 are contiguous whatever the file order");
        assert_eq!(ranks.dropped, 2, "rank 5 past the gap, and rank X");
    }

    #[test]
    fn an_unresolved_threshold_is_counted() {
        let ranks = table("Rank 1\n  SkillPointsNeededDefault = #MULTIPLY( NOPE 2 )\nEnd\n");
        assert_eq!(ranks.get(1).unwrap().skill_points("Default"), None);
        assert_eq!(ranks.unresolved, 1);
    }

    #[test]
    fn a_truncated_or_corrupted_rank_file_never_panics() {
        let text = "#define D 60\nRank 1\n  SkillPointsNeededDefault = 0\nEnd\n\
                    Rank 2\n  SkillPointsNeededDefault = #MULTIPLY( D 1 )\n  \
                    SciencesGranted = A B\n  SciencePurchasePointsGranted = 1\nEnd\n";
        let run = |text: &str| {
            if let Ok(doc) = Document::parse("rank.ini", text, &BlockTable::corpus_path()) {
                let mut macros = MacroTable::new();
                macros.insert_defines(doc.defines());
                let _ = RankTable::build(&[doc], &macros);
            }
        };
        for end in 0..=text.len() {
            if let Some(prefix) = text.get(..end) {
                run(prefix);
            }
        }
        let bytes = text.as_bytes();
        for at in 0..bytes.len() {
            for byte in [0u8, b'9', b'#', b')', 0xFF] {
                let mut corrupt = bytes.to_vec();
                corrupt[at] = byte;
                run(&String::from_utf8_lossy(&corrupt));
            }
        }
    }
}
