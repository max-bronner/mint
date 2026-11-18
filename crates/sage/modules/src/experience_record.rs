use std::collections::BTreeMap;

use mint_sage_ini::multiplayer_color::rgb_field;
use mint_sage_ini::value::{parse_bool, parse_float_or_macro, parse_percent_shaped};
use mint_sage_ini::{Block, Document, Field, MacroTable};

pub const EXPERIENCE_LEVEL_KEYWORD: &str = "ExperienceLevel";

const MAX_EXPERIENCE: f32 = 1_000_000_000.0;

#[derive(Debug, Clone, PartialEq)]
pub struct SelectionDecal {
    pub texture: String,
    pub texture2: Option<String>,
    pub style: Option<String>,
    pub opacity: (f32, f32),
    pub throb_ms: u32,
    pub min_radius: Option<f32>,
    pub max_radius: Option<f32>,
    pub max_selected: Option<f32>,
}

impl SelectionDecal {
    #[must_use]
    pub fn merges(&self) -> bool {
        self.style.as_deref().is_some_and(|style| {
            style
                .split_whitespace()
                .any(|bit| bit.eq_ignore_ascii_case("SHADOW_MERGE_DECAL"))
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LevelRecord {
    pub block: String,
    pub rank: u16,
    pub required: u32,
    pub award: u32,
    pub own_guys_award: u32,
    pub attribute_modifiers: Vec<String>,
    pub upgrades: Vec<String>,
    pub decal: Option<SelectionDecal>,
    pub level_up_fx: Option<String>,
    pub emotion_type: Option<String>,
    pub level_up_tint: Option<LevelUpTint>,
    pub inform_update_module: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelUpTint {
    pub color: [f32; 3],
    pub attack: f32,
    pub sustain: f32,
    pub decay: f32,
}

fn read_level_up_tint(block: &Block, macros: &MacroTable) -> Option<LevelUpTint> {
    let shown = block
        .field("ShowLevelUpTint")
        .and_then(Field::value)
        .and_then(parse_bool)?;
    let color = rgb_field(block, "LevelUpTintColor")?;
    let seconds = |keyword: &str| {
        block
            .field(keyword)
            .and_then(Field::value)
            .and_then(|raw| parse_float_or_macro(raw, macros))
            .filter(|ms| ms.is_finite() && *ms >= 0.0)
            .map_or(0.0, |ms| ms / 1000.0)
    };
    shown.then(|| LevelUpTint {
        color: color.to_unit(),
        attack: seconds("LevelUpTintPreColorTime"),
        sustain: seconds("LevelUpTintSustainColorTime"),
        decay: seconds("LevelUpTintPostColorTime"),
    })
}

fn read_decal(block: &Block, macros: &MacroTable) -> Option<SelectionDecal> {
    let text = |keyword: &str| {
        block
            .field(keyword)
            .and_then(Field::value)
            .filter(|value| !value.eq_ignore_ascii_case("NONE"))
            .map(str::to_owned)
    };
    let number = |keyword: &str| {
        let raw = block.field(keyword).and_then(Field::value)?;
        parse_percent_shaped(raw, macros).filter(|v| v.is_finite() && *v >= 0.0)
    };
    Some(SelectionDecal {
        texture: text("Texture")?,
        texture2: text("Texture2"),
        style: text("Style"),
        opacity: (
            number("OpacityMin").unwrap_or(1.0),
            number("OpacityMax").unwrap_or(1.0),
        ),
        throb_ms: block
            .field("OpacityThrobTime")
            .and_then(Field::value)
            .and_then(|raw| parse_float_or_macro(raw, macros))
            .filter(|ms| ms.is_finite() && *ms >= 0.0)
            .map_or(1000, |ms| {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let ms = ms.min(u32::MAX as f32) as u32;
                ms
            }),
        min_radius: number("MinRadius"),
        max_radius: number("MaxRadius"),
        max_selected: number("MaxSelectedUnits"),
    })
}

fn fx_list_name(value: &str) -> &str {
    match value.split_once(':') {
        Some((prefix, name)) if prefix.eq_ignore_ascii_case("FX") => name,
        _ => value,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExperienceReport {
    pub blocks: usize,
    pub dropped: usize,
    pub unresolved_awards: usize,
    pub targets: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ExperienceTables {
    tables: BTreeMap<String, Vec<LevelRecord>>,
    pub report: ExperienceReport,
}

impl ExperienceTables {
    #[must_use]
    pub fn build(documents: &[Document], macros: &MacroTable) -> Self {
        let mut lists: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for document in documents {
            for define in document.defines() {
                let tokens: Vec<&str> = define.body.split_whitespace().collect();
                if tokens.len() > 1 {
                    lists.insert(define.name.as_str(), tokens);
                }
            }
        }
        let mut out = Self::default();
        for document in documents {
            for block in document.roots() {
                if !block.keyword.eq_ignore_ascii_case(EXPERIENCE_LEVEL_KEYWORD) {
                    continue;
                }
                out.report.blocks = out.report.blocks.saturating_add(1);
                let Some(level) = out.read_level(block, macros) else {
                    out.report.dropped = out.report.dropped.saturating_add(1);
                    continue;
                };
                let targets = block.field("TargetNames").map_or(&[][..], |f| &f.args[..]);
                for token in targets {
                    match lists.get(token.as_str()) {
                        Some(names) => {
                            for name in names {
                                out.push(name, &level);
                            }
                        }
                        None => out.push(token, &level),
                    }
                }
            }
        }
        for table in out.tables.values_mut() {
            table.sort_by(|a, b| a.required.cmp(&b.required).then(a.rank.cmp(&b.rank)));
        }
        out.report.targets = out.tables.len();
        out
    }

    fn push(&mut self, target: &str, level: &LevelRecord) {
        self.tables
            .entry(target.to_ascii_lowercase())
            .or_default()
            .push(level.clone());
    }

    fn read_level(&mut self, block: &Block, macros: &MacroTable) -> Option<LevelRecord> {
        let number = |keyword: &str| -> Option<f32> {
            parse_float_or_macro(block.field(keyword).and_then(Field::value)?, macros)
                .filter(|v| v.is_finite() && (0.0..=MAX_EXPERIENCE).contains(v))
        };
        let required = number("RequiredExperience")?;
        let rank = number("Rank").filter(|rank| *rank <= f32::from(u16::MAX))?;
        let mut award_of = |keyword: &str| match block.field(keyword) {
            None => 0.0,
            Some(_) => number(keyword).unwrap_or_else(|| {
                self.report.unresolved_awards = self.report.unresolved_awards.saturating_add(1);
                0.0
            }),
        };
        let award = award_of("ExperienceAward");
        let own_guys_award = award_of("ExperienceAwardOwnGuysDie");
        let names = |keyword: &str| -> Vec<String> {
            block
                .fields_named(keyword)
                .flat_map(|field| field.args.iter().cloned())
                .filter(|name| !name.eq_ignore_ascii_case("None"))
                .collect()
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(LevelRecord {
            block: block.name().unwrap_or_default().to_owned(),
            rank: rank as u16,
            required: required as u32,
            award: award as u32,
            own_guys_award: own_guys_award as u32,
            attribute_modifiers: names("AttributeModifiers"),
            upgrades: names("Upgrades"),
            decal: block
                .blocks_named("SelectionDecal")
                .next()
                .and_then(|decal| read_decal(decal, macros)),
            level_up_fx: block
                .field("LevelUpFx")
                .and_then(Field::value)
                .map(fx_list_name)
                .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case("NONE"))
                .map(str::to_owned),
            level_up_tint: read_level_up_tint(block, macros),
            emotion_type: block
                .field("EmotionType")
                .and_then(Field::value)
                .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case("NONE"))
                .map(str::to_owned),
            inform_update_module: block
                .field("InformUpdateModule")
                .and_then(Field::value)
                .and_then(parse_bool)
                .unwrap_or(false),
        })
    }

    #[must_use]
    pub fn table(&self, name: &str) -> Option<&[LevelRecord]> {
        self.tables
            .get(&name.to_ascii_lowercase())
            .map(Vec::as_slice)
    }

    pub fn targets(&self) -> impl Iterator<Item = &str> {
        self.tables.keys().map(String::as_str)
    }

    #[must_use]
    pub fn out_of_order(&self) -> Vec<&str> {
        self.tables
            .iter()
            .filter(|(_, table)| {
                table.windows(2).any(
                    |pair| matches!(pair, [a, b] if a.rank >= b.rank || a.required >= b.required),
                )
            })
            .map(|(name, _)| name.as_str())
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use mint_sage_ini::BlockTable;

    fn tables(text: &str) -> ExperienceTables {
        let doc =
            Document::parse("experiencelevels.ini", text, &BlockTable::corpus_path()).unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let mut macros = MacroTable::new();
        macros.insert_defines(doc.defines());
        ExperienceTables::build(&[doc], &macros)
    }

    const TROOPS: &str = "\
#define TROOPS Soldier SoldierHorde\n\
#define REQ_2 50\n#define AWARD_1 3\n#define AWARD_2 4\n#define MIN_R 40\n\
ExperienceLevel TroopLevel2\n  TargetNames = TROOPS\n  RequiredExperience = REQ_2\n  \
  ExperienceAward = AWARD_2\n  ExperienceAwardOwnGuysDie = 5\n  AttributeModifiers = TroopBonusRank2\n  Rank = 2\n  \
  Upgrades = Upgrade_ObjectLevel2\n  LevelUpFx = FX:LevelUp2\n  SelectionDecal\n    Texture = decal\n    Texture2 = decal_co\n    \
  Style = SHADOW_MERGE_DECAL\n    OpacityMin = 80%\n    OpacityMax = 100%\n    MinRadius = MIN_R\n    \
  MaxRadius = 200\n  End\nEnd\n\
ExperienceLevel TroopLevel1\n  TargetNames = TROOPS\n  RequiredExperience = 1\n  \
  ExperienceAward = AWARD_1\n  Rank = 1\n  Upgrades = Upgrade_ObjectLevel1\nEnd\n";

    #[test]
    fn a_list_define_expands_and_a_table_sorts_by_required_experience() {
        let t = tables(TROOPS);
        assert_eq!(t.report.blocks, 2);
        assert_eq!(t.report.targets, 2);
        let soldier = t.table("SOLDIER").unwrap();
        assert_eq!(soldier.len(), 2);
        assert_eq!(
            (soldier[0].rank, soldier[0].required, soldier[0].award),
            (1, 1, 3)
        );
        assert_eq!(
            (soldier[1].rank, soldier[1].required, soldier[1].award),
            (2, 50, 4)
        );
        assert_eq!(
            (soldier[0].own_guys_award, soldier[1].own_guys_award),
            (0, 5)
        );
        assert!(soldier[0].decal.is_none());
        let decal = soldier[1].decal.as_ref().unwrap();
        assert_eq!(decal.texture, "decal");
        assert_eq!(decal.texture2.as_deref(), Some("decal_co"));
        assert_eq!(decal.style.as_deref(), Some("SHADOW_MERGE_DECAL"));
        assert_eq!(decal.opacity, (0.8, 1.0));
        assert_eq!(
            (decal.min_radius, decal.max_radius),
            (Some(40.0), Some(200.0))
        );
        assert_eq!(soldier[1].attribute_modifiers, ["TroopBonusRank2"]);
        assert_eq!(soldier[1].upgrades, ["Upgrade_ObjectLevel2"]);
        assert_eq!(
            (
                soldier[0].level_up_fx.as_deref(),
                soldier[1].level_up_fx.as_deref()
            ),
            (None, Some("LevelUp2"))
        );
        assert_eq!(t.table("SoldierHorde").unwrap().len(), 2);
        assert!(t.out_of_order().is_empty());
    }

    #[test]
    fn a_block_name_declared_twice_keeps_both_targets() {
        let t = tables(
            "ExperienceLevel FortLevel1\n  TargetNames = DwarfFort\n  RequiredExperience = 1\n  Rank = 1\nEnd\n\
             ExperienceLevel FortLevel1\n  TargetNames = WildFort\n  RequiredExperience = 1\n  Rank = 1\nEnd\n",
        );
        assert!(t.table("DwarfFort").is_some());
        assert!(t.table("WildFort").is_some());
    }

    #[test]
    fn an_unresolved_requirement_drops_the_level_and_an_unresolved_award_reads_zero() {
        let t = tables(
            "ExperienceLevel HeroLevel3\n  TargetNames = Hero\n  RequiredExperience = hero_lvl3_needed\n  Rank = 3\nEnd\n\
             ExperienceLevel HeroLevel2\n  TargetNames = Hero\n  RequiredExperience = 50\n  \
               ExperienceAward = NOPE\n  Rank = 2\nEnd\n",
        );
        assert_eq!(t.report.dropped, 1);
        assert_eq!(t.report.unresolved_awards, 1);
        let hero = t.table("Hero").unwrap();
        assert_eq!(hero.len(), 1);
        assert_eq!(hero[0].award, 0);
    }

    #[test]
    fn a_rank_that_falls_as_experience_rises_is_reported() {
        let t = tables(
            "ExperienceLevel A\n  TargetNames = X\n  RequiredExperience = 1\n  Rank = 2\nEnd\n\
             ExperienceLevel B\n  TargetNames = X\n  RequiredExperience = 5\n  Rank = 1\nEnd\n",
        );
        assert_eq!(t.out_of_order(), ["x"]);
    }

    #[test]
    fn a_hostile_number_is_refused() {
        let t = tables(
            "ExperienceLevel A\n  TargetNames = X\n  RequiredExperience = -5\n  Rank = 1\nEnd\n\
             ExperienceLevel B\n  TargetNames = X\n  RequiredExperience = 1e30\n  Rank = 1\nEnd\n\
             ExperienceLevel C\n  TargetNames = X\n  RequiredExperience = 1\n  Rank = 99999999\nEnd\n",
        );
        assert_eq!(t.report.dropped, 3);
        assert!(t.table("X").is_none());
    }
}
