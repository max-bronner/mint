use std::collections::BTreeMap;

use crate::tree::{Block, Document, Field};
use crate::value::{parse_float, parse_int, parse_percent};

pub const ARMY_DEFINITION_KEYWORD: &str = "ArmyDefinition";
pub const ARMY_MEMBER_KEYWORD: &str = "ArmyMemberDefinition";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ArmyMember {
    pub unit: Option<String>,
    pub phase: [Option<f32>; 3],
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ArmyDefinitionDef {
    pub name: String,
    pub side: Option<String>,
    pub must_use_command_point_percentage: [Option<f32>; 3],
    pub percent_to_save: [Option<f32>; 3],
    pub phase_duration_rush: Option<f32>,
    pub phase_duration_mid_game: Option<f32>,
    pub economy_builder_min_farms_owned: Option<i64>,
    pub economy_builder_per_farm_value: Option<f32>,
    pub economy_template: Option<String>,
    pub members: Vec<ArmyMember>,
    pub hero_build_order: Vec<String>,
}

fn percent(block: &Block, keyword: &str) -> Option<f32> {
    block
        .field(keyword)
        .and_then(Field::value)
        .and_then(parse_percent)
}

fn number(block: &Block, keyword: &str) -> Option<f32> {
    block
        .field(keyword)
        .and_then(Field::value)
        .and_then(parse_float)
}

fn integer(block: &Block, keyword: &str) -> Option<i64> {
    block
        .field(keyword)
        .and_then(Field::value)
        .and_then(parse_int)
}

fn text(block: &Block, keyword: &str) -> Option<String> {
    block
        .field(keyword)
        .and_then(Field::value)
        .filter(|value| !value.eq_ignore_ascii_case("NONE"))
        .map(str::to_owned)
}

fn nested_template_name(block: &Block, keyword: &str) -> Option<String> {
    block
        .blocks_named(keyword)
        .next()
        .and_then(|inner| inner.field("TemplateName"))
        .and_then(Field::value)
        .filter(|value| !value.eq_ignore_ascii_case("NONE"))
        .map(str::to_owned)
}

fn read_member(block: &Block) -> ArmyMember {
    ArmyMember {
        unit: text(block, "Unit"),
        phase: [
            number(block, "PercentageOfArmyPhase1"),
            number(block, "PercentageOfArmyPhase2"),
            number(block, "PercentageOfArmyPhase3"),
        ],
    }
}

#[must_use]
pub fn read_army_definition(block: &Block) -> ArmyDefinitionDef {
    ArmyDefinitionDef {
        name: block.name().unwrap_or_default().to_owned(),
        side: text(block, "Side"),
        must_use_command_point_percentage: [
            percent(block, "MustUseCommandPointPercentage_Phase1"),
            percent(block, "MustUseCommandPointPercentage_Phase2"),
            percent(block, "MustUseCommandPointPercentage_Phase3"),
        ],
        percent_to_save: [
            percent(block, "PercentToSave_Rush"),
            percent(block, "PercentToSave_MidGame"),
            percent(block, "PercentToSave_EndGame"),
        ],
        phase_duration_rush: number(block, "PhaseDuration_Rush"),
        phase_duration_mid_game: number(block, "PhaseDuration_MidGame"),
        economy_builder_min_farms_owned: integer(block, "EconomyBuilderMinFarmsOwned"),
        economy_builder_per_farm_value: number(block, "EconomyBuilderPerFarmValue"),
        economy_template: nested_template_name(block, "AIEconomyAssigment"),
        members: block
            .blocks_named(ARMY_MEMBER_KEYWORD)
            .map(read_member)
            .collect(),
        hero_build_order: block
            .field("HeroBuildOrder")
            .map(|field| field.args.iter().map(String::clone).collect())
            .unwrap_or_default(),
    }
}

#[derive(Debug, Clone, Default)]
pub struct ArmyDefinitionIndex {
    defs: BTreeMap<String, ArmyDefinitionDef>,
    order: Vec<String>,
}

impl ArmyDefinitionIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_document(&mut self, document: &Document) {
        for block in document.roots() {
            self.insert_block(block);
        }
    }

    pub fn insert_block(&mut self, block: &Block) {
        if !block.keyword.eq(ARMY_DEFINITION_KEYWORD) {
            return;
        }
        let Some(name) = block.name() else {
            return;
        };
        let key = name.to_ascii_lowercase();
        if !self.defs.contains_key(&key) {
            self.order.push(key.clone());
        }
        self.defs.insert(key, read_army_definition(block));
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ArmyDefinitionDef> {
        self.defs.get(&name.to_ascii_lowercase())
    }

    #[must_use]
    pub fn by_side(&self, side: &str) -> Option<&ArmyDefinitionDef> {
        self.iter().find(|def| {
            def.side
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case(side))
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = &ArmyDefinitionDef> {
        self.order.iter().filter_map(|key| self.defs.get(key))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::table::BlockTable;

    fn index(text: &str) -> ArmyDefinitionIndex {
        let document =
            Document::parse("skirmishaidata.ini", text, &BlockTable::corpus_path()).unwrap();
        let mut index = ArmyDefinitionIndex::new();
        index.insert_document(&document);
        index
    }

    const MEN: &str = "\
ArmyDefinition MenOfTheWestArmy
  Side = Men
  MustUseCommandPointPercentage_Phase1 = 90%
  MustUseCommandPointPercentage_Phase2 = 80%
  MustUseCommandPointPercentage_Phase3 = 75%
  EconomyBuilderMinFarmsOwned = 4
  EconomyBuilderPerFarmValue = 70
  PercentToSave_Rush = 1.0%
  PercentToSave_MidGame = 3.0%
  PercentToSave_EndGame = 4.0%
  PhaseDuration_Rush = 270.0
  PhaseDuration_MidGame = 280.0
  AIEconomyAssigment MenFarm
    TemplateName = GondorFarm
  End
  ArmyMemberDefinition GondorFighterHorde_Member
    Unit = GondorFighterHorde
    PercentageOfArmyPhase1 = 40.0
    PercentageOfArmyPhase2 = 40.0
    PercentageOfArmyPhase3 = 30.0
  End
  ArmyMemberDefinition GondorRangerHorde_Member
    Unit = GondorRangerHorde
    PercentageOfArmyPhase1 = 0.0
    PercentageOfArmyPhase2 = 25.0
    PercentageOfArmyPhase3 = 30.0
  End
  HeroBuildOrder = ElvenGaladriel_RingHero RohanFrodo RohanEowyn
End
";

    #[test]
    fn every_field_this_module_reads_comes_off_the_shipped_shape() {
        let index = index(MEN);
        let men = index.get("MenOfTheWestArmy").unwrap();
        assert_eq!(men.side.as_deref(), Some("Men"));
        assert_eq!(
            men.must_use_command_point_percentage,
            [Some(0.9), Some(0.8), Some(0.75)]
        );
        assert_eq!(men.percent_to_save, [Some(0.01), Some(0.03), Some(0.04)]);
        assert_eq!(men.phase_duration_rush, Some(270.0));
        assert_eq!(men.phase_duration_mid_game, Some(280.0));
        assert_eq!(men.economy_builder_min_farms_owned, Some(4));
        assert_eq!(men.economy_builder_per_farm_value, Some(70.0));
        assert_eq!(
            men.economy_template.as_deref(),
            Some("GondorFarm"),
            "the nested TemplateName, not AIEconomyAssigment's own header label"
        );
        assert_eq!(men.members.len(), 2);
        assert_eq!(men.members[0].unit.as_deref(), Some("GondorFighterHorde"));
        assert_eq!(men.members[0].phase, [Some(40.0), Some(40.0), Some(30.0)]);
        assert_eq!(men.members[1].phase[0], Some(0.0), "0% is a real target");
        assert_eq!(
            men.hero_build_order,
            vec!["ElvenGaladriel_RingHero", "RohanFrodo", "RohanEowyn"]
        );
    }

    #[test]
    fn by_side_matches_case_insensitively_and_get_matches_the_block_name() {
        let index = index(MEN);
        assert!(index.by_side("men").is_some());
        assert!(index.by_side("MEN").is_some());
        assert!(index.by_side("Mordor").is_none());
        assert_eq!(
            index.get("menofthewestarmy").unwrap().name,
            "MenOfTheWestArmy"
        );
    }

    #[test]
    fn a_definition_with_no_members_or_hero_order_reads_as_empty_not_absent() {
        let index = index("ArmyDefinition Empty\n  Side = Test\nEnd\n");
        let empty = index.get("Empty").unwrap();
        assert!(empty.members.is_empty());
        assert!(empty.hero_build_order.is_empty());
        assert_eq!(empty.economy_builder_min_farms_owned, None);
    }

    #[test]
    fn every_truncation_reads_without_panicking() {
        for end in 0..=MEN.len() {
            let Some(prefix) = MEN.get(..end) else {
                continue;
            };
            if let Ok(document) =
                Document::parse("skirmishaidata.ini", prefix, &BlockTable::corpus_path())
            {
                let mut index = ArmyDefinitionIndex::new();
                index.insert_document(&document);
                let _ = index.get("MenOfTheWestArmy");
            }
        }
    }
}
