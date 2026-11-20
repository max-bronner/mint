use crate::diagnostic::ValueErrors;
use crate::tree::{Block, Field};
use crate::value::{is_none, parse_int};

pub const AWARD_SYSTEM_KEYWORD: &str = "AwardSystem";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AwardTrigger {
    pub stats: Vec<String>,
    pub threshold: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AwardDef {
    pub name: Option<String>,
    pub image: Option<String>,
    pub name_tag: Option<String>,
    pub description_tag: Option<String>,
    pub upgrade_awards: Vec<String>,
    pub triggers: Vec<AwardTrigger>,
}

impl AwardDef {
    #[must_use]
    pub fn earned(&self, stats: &std::collections::BTreeMap<String, i32>) -> bool {
        !self.triggers.is_empty()
            && self.triggers.iter().all(|trigger| {
                let sum: i64 = trigger
                    .stats
                    .iter()
                    .map(|name| i64::from(stats.get(name).copied().unwrap_or(0)))
                    .sum();
                sum >= trigger.threshold.unwrap_or(0)
            })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThingStatDef {
    pub stat_name: Option<String>,
    pub name_tag: Option<String>,
    pub description_tag: Option<String>,
    pub thing_template_names: Vec<String>,
    pub excluded_things: Vec<String>,
    pub kind_of: Vec<String>,
    pub excluded_kind_of: Vec<String>,
    pub primary_key: i64,
    pub secondary_key: i64,
}

fn text(block: &Block, keyword: &str) -> Option<String> {
    let value = block.field(keyword).and_then(Field::value)?;
    (!is_none(value)).then(|| value.to_owned())
}

fn trigger(block: &Block, errors: &ValueErrors) -> AwardTrigger {
    let threshold = block.field("Threshold").and_then(|field| {
        let read = field.value().and_then(parse_int);
        if read.is_none() {
            errors.field("Trigger", field, "an integer");
        }
        read
    });
    AwardTrigger {
        stats: block
            .fields_named("Stat")
            .flat_map(|field| field.args.iter())
            .filter(|name| !is_none(name))
            .cloned()
            .collect(),
        threshold,
    }
}

#[must_use]
pub fn read_award_system(block: &Block, errors: &ValueErrors) -> Vec<AwardDef> {
    block
        .blocks_named("ObjectAward")
        .map(|award| AwardDef {
            name: text(award, "AwardName").or_else(|| award.name().map(str::to_owned)),
            image: text(award, "ImageName"),
            name_tag: text(award, "NameTag"),
            description_tag: text(award, "DescriptionTag"),
            upgrade_awards: award
                .fields_named("UpgradeAwards")
                .flat_map(|field| field.args.iter())
                .filter(|name| !is_none(name))
                .cloned()
                .collect(),
            triggers: award
                .blocks_named("Trigger")
                .map(|t| trigger(t, errors))
                .collect(),
        })
        .collect()
}

fn names(block: &Block, keyword: &str) -> Vec<String> {
    block
        .fields_named(keyword)
        .flat_map(|field| field.args.iter())
        .filter(|name| !is_none(name))
        .cloned()
        .collect()
}

fn key(block: &Block, keyword: &str, errors: &ValueErrors) -> i64 {
    block.field(keyword).map_or(0, |field| {
        field.value().and_then(parse_int).unwrap_or_else(|| {
            errors.field("ThingStat", field, "an integer");
            0
        })
    })
}

#[must_use]
pub fn read_thing_stats(block: &Block, errors: &ValueErrors) -> Vec<ThingStatDef> {
    block
        .blocks_named("ThingStat")
        .map(|stat| ThingStatDef {
            stat_name: text(stat, "StatName"),
            name_tag: text(stat, "NameTag"),
            description_tag: text(stat, "DescriptionTag"),
            thing_template_names: names(stat, "ThingTemplateNames"),
            excluded_things: names(stat, "ExcludedThings"),
            kind_of: names(stat, "KindOf"),
            excluded_kind_of: names(stat, "ExcludedKindOf"),
            primary_key: key(stat, "PrimaryKey", errors),
            secondary_key: key(stat, "SecondaryKey", errors),
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::table::BlockTable;
    use crate::tree::Document;

    #[test]
    fn triggers_keep_every_stat_line_and_the_threshold() {
        let text = "AwardSystem\n\
            ObjectAward AwardTestOne\n  NameTag = TestTag\n\
              UpgradeAwards = UpA UpB\n  UpgradeAwards = UpC\n\
              Trigger\n    Stat = StatA StatB\n    Stat = StatC\n    Threshold = 7\n  End\n\
              Trigger\n    Stat = StatD\n    Threshold = lots\n  End\n\
            End\nEnd\n";
        let doc = Document::parse("a.ini", text, &BlockTable::corpus_path()).unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let errors = ValueErrors::new();
        let block = doc.roots_named(AWARD_SYSTEM_KEYWORD).next().unwrap();
        let awards = read_award_system(block, &errors);
        assert_eq!(awards[0].name.as_deref(), Some("AwardTestOne"));
        assert_eq!(awards[0].upgrade_awards, ["UpA", "UpB", "UpC"]);
        assert_eq!(awards[0].triggers[0].stats, ["StatA", "StatB", "StatC"]);
        assert_eq!(awards[0].triggers[0].threshold, Some(7));
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn thing_stats_keep_their_name_and_tags() {
        let text = "AwardSystem
            ThingStat
  StatName = TEST_COUNT
  NameTag = Test:CountName
              DescriptionTag = Test:CountDesc
  MultiPlayerOnly = Yes
  ThingTemplateNames = TestA TestB
  KindOf = HERO
  ExcludedKindOf = SUMMONED
  ExcludedThings = TestC
  PrimaryKey = 2
End
            ObjectAward AwardTestOne
End
End
";
        let doc = Document::parse("a.ini", text, &BlockTable::corpus_path()).unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let block = doc.roots_named(AWARD_SYSTEM_KEYWORD).next().unwrap();
        let stats = read_thing_stats(block, &ValueErrors::new());
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].stat_name.as_deref(), Some("TEST_COUNT"));
        assert_eq!(stats[0].name_tag.as_deref(), Some("Test:CountName"));
        assert_eq!(stats[0].description_tag.as_deref(), Some("Test:CountDesc"));
        assert_eq!(stats[0].thing_template_names, ["TestA", "TestB"]);
        assert_eq!(stats[0].kind_of, ["HERO"]);
        assert_eq!(stats[0].excluded_kind_of, ["SUMMONED"]);
        assert_eq!(stats[0].excluded_things, ["TestC"]);
        assert_eq!((stats[0].primary_key, stats[0].secondary_key), (2, 0));
    }
}
