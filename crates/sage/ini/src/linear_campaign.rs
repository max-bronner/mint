use std::collections::BTreeMap;

use crate::diagnostic::{ValueError, ValueErrors};
use crate::tree::{Block, Document, Field};
use crate::value::{is_none, parse_int};

pub const LINEAR_CAMPAIGN_KEYWORD: &str = "LinearCampaign";

pub const DEFAULT_FADE_UP_FRAMES: u32 = 7;

#[must_use]
pub fn ms_to_frames(ms: u32) -> u32 {
    let frames = u64::from(ms).saturating_add(199) / 200;
    u32::try_from(frames).unwrap_or(u32::MAX)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MissionDef {
    pub name: String,
    pub map: Option<String>,
    pub intro_movie: Option<String>,
    pub load_screen_image: Option<String>,
    pub load_screen_music_track: Option<String>,
    pub fade_up_frames: Option<u32>,
    pub delay_carryover_spawning_of: Vec<String>,
}

impl MissionDef {
    #[must_use]
    pub fn fade_up(&self) -> u32 {
        self.fade_up_frames.unwrap_or(DEFAULT_FADE_UP_FRAMES)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinearCampaignDef {
    pub name: String,
    pub display_name_label: Option<String>,
    pub overall_intro_movie: Option<String>,
    pub carryover_units: Vec<String>,
    pub missions: Vec<MissionDef>,
}

fn text(block: &Block, keyword: &str) -> Option<String> {
    block
        .field(keyword)
        .and_then(Field::value)
        .filter(|value| !is_none(value))
        .map(str::to_owned)
}

fn names(block: &Block, keyword: &str) -> Vec<String> {
    block
        .fields_named(keyword)
        .flat_map(|field| field.args.iter())
        .filter(|name| !is_none(name))
        .cloned()
        .collect()
}

fn read_mission(block: &Block, errors: &ValueErrors) -> MissionDef {
    let name = block.name().unwrap_or_default().to_owned();
    let fade_up_frames = block
        .field("MillisecondsAfterStartToStartFadeUp")
        .and_then(|field| {
            let read = field
                .value()
                .and_then(parse_int)
                .and_then(|ms| u32::try_from(ms).ok());
            if read.is_none() {
                errors.field(&name, field, "a non-negative integer");
            }
            read.map(ms_to_frames)
        });
    MissionDef {
        map: text(block, "Map"),
        intro_movie: text(block, "IntroMovie"),
        load_screen_image: text(block, "LoadScreenImage"),
        load_screen_music_track: text(block, "LoadScreenMusicTrack"),
        fade_up_frames,
        delay_carryover_spawning_of: names(block, "DelayCarryoverSpawningOf"),
        name,
    }
}

#[must_use]
pub fn read_linear_campaign(block: &Block, errors: &ValueErrors) -> LinearCampaignDef {
    LinearCampaignDef {
        name: block.name().unwrap_or_default().to_owned(),
        display_name_label: text(block, "CampaignDisplayNameLabel"),
        overall_intro_movie: text(block, "OverallCampaignIntroMovie"),
        carryover_units: names(block, "CarryoverUnit"),
        missions: block
            .blocks_named("Mission")
            .map(|mission| read_mission(mission, errors))
            .collect(),
    }
}

#[derive(Debug, Clone, Default)]
pub struct LinearCampaignIndex {
    campaigns: Vec<LinearCampaignDef>,
    by_name: BTreeMap<String, usize>,
    errors: Vec<ValueError>,
}

impl LinearCampaignIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_document(&mut self, document: &Document) {
        for block in document.roots_named(LINEAR_CAMPAIGN_KEYWORD) {
            let errors = ValueErrors::new();
            let def = read_linear_campaign(block, &errors);
            self.errors.append(&mut errors.take());
            let key = def.name.to_ascii_lowercase();
            match self.by_name.get(&key).copied() {
                Some(slot) => {
                    if let Some(held) = self.campaigns.get_mut(slot) {
                        *held = def;
                    }
                }
                None => {
                    self.by_name.insert(key, self.campaigns.len());
                    self.campaigns.push(def);
                }
            }
        }
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&LinearCampaignDef> {
        let slot = self.by_name.get(&name.to_ascii_lowercase())?;
        self.campaigns.get(*slot)
    }

    #[must_use]
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.by_name.get(&name.to_ascii_lowercase()).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = &LinearCampaignDef> {
        self.campaigns.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.campaigns.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.campaigns.is_empty()
    }

    #[must_use]
    pub fn errors(&self) -> &[ValueError] {
        &self.errors
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::strict::check;
    use crate::table::BlockTable;
    use crate::tables::field_table::Titles;
    use crate::value::MacroTable;

    const TEXT: &str = "LinearCampaign TEST_B
  CampaignDisplayNameLabel = LBL
  CarryoverUnit = UnitA UnitB
  Mission Zeta
    Map = maps/z
    MillisecondsAfterStartToStartFadeUp = 1001
  End
  Mission Alpha
    Map = maps/a
    IntroMovie = Intro
    MillisecondsAfterStartToStartFadeUp = 1400
    DelayCarryoverSpawningOf = UnitA
  End
  Mission Mid
    Map = maps/m
  End
End
LinearCampaign TEST_A
  Mission One
    Map = maps/one
  End
End
";

    fn parse(text: &str) -> Document {
        Document::parse("lc.ini", text, &BlockTable::corpus_path()).unwrap()
    }

    #[test]
    fn missions_and_campaigns_keep_file_order() {
        let mut index = LinearCampaignIndex::new();
        index.insert_document(&parse(TEXT));
        let names: Vec<_> = index.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["TEST_B", "TEST_A"]);
        let missions: Vec<_> = index
            .get("test_b")
            .unwrap()
            .missions
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(missions, ["Zeta", "Alpha", "Mid"]);
    }

    #[test]
    fn fade_up_rounds_milliseconds_up_to_frames_and_defaults_to_seven() {
        let mut index = LinearCampaignIndex::new();
        index.insert_document(&parse(TEXT));
        let missions = &index.get("TEST_B").unwrap().missions;
        assert_eq!(missions[0].fade_up(), 6);
        assert_eq!(missions[1].fade_up(), 7);
        assert_eq!(missions[2].fade_up_frames, None);
        assert_eq!(missions[2].fade_up(), DEFAULT_FADE_UP_FRAMES);
        assert_eq!(ms_to_frames(2000), 10);
        assert_eq!(ms_to_frames(1000), 5);
    }

    #[test]
    fn a_bad_fade_up_value_is_a_load_error() {
        let mut index = LinearCampaignIndex::new();
        index.insert_document(&parse(
            "LinearCampaign C\n  Mission M\n    MillisecondsAfterStartToStartFadeUp = soon\n  End\nEnd\n",
        ));
        assert_eq!(index.errors().len(), 1);
    }

    #[test]
    fn an_unknown_field_in_a_mission_or_campaign_is_refused() {
        let doc = parse("LinearCampaign C\n  Bogus = 1\n  Mission M\n    Wat = 2\n  End\nEnd\n");
        let report = check(&doc, &MacroTable::new(), Titles::BOTH);
        assert_eq!(report.errors.len(), 2, "{:?}", report.errors);
    }
}
