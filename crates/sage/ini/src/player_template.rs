use std::collections::BTreeMap;

use crate::tree::{Block, Document, Field};
use crate::value::{is_none, parse_bool, parse_float, parse_int, parse_percent, tagged_value};

pub const PLAYER_TEMPLATE_KEYWORD: &str = "PlayerTemplate";

pub const MAX_STARTING_UNITS: u8 = 10;

pub const DEFAULT_MAX_LEVEL_MP: &str = "60";

pub const DEFAULT_MAX_LEVEL_SP: &str = "100";

#[derive(Debug, Clone, PartialEq)]
pub struct StartingUnit {
    pub index: u8,
    pub template: String,
    pub offset: Option<[f32; 3]>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerTemplateDef {
    pub name: String,
    pub side: Option<String>,
    pub playable: Option<bool>,
    pub evil: Option<bool>,
    pub starting_building: Option<String>,
    pub starting_units: Vec<StartingUnit>,
    pub starting_units_refused: usize,
    pub multi_selection_portrait: Option<String>,
    pub ring_heroes: Vec<String>,
    pub heroes: Vec<String>,
    pub intrinsic_sciences_mp: Vec<String>,
    pub intrinsic_sciences: Vec<String>,
    pub spell_book_mp: Option<String>,
    pub spell_book: Option<String>,
    pub purchase_science_command_set_mp: Option<String>,
    pub purchase_science_command_set: Option<String>,
    pub max_level_mp: Option<String>,
    pub max_level_sp: Option<String>,
    pub intrinsic_science_purchase_points: Option<String>,
    pub start_money: Option<i64>,
    pub initial_upgrades: Vec<String>,
    pub starting_units_wotr: Vec<String>,
    pub production_cost_change: Option<f32>,
    pub load_screen_music: Option<String>,
    pub objective_added_sound: Option<String>,
    pub objective_completed_sound: Option<String>,
}

fn numbered(keyword: &str, prefix: &str) -> Option<u32> {
    let head = keyword.get(..prefix.len())?;
    if !head.eq(prefix) {
        return None;
    }
    let digits = keyword.get(prefix.len()..)?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(digits.parse().unwrap_or(u32::MAX))
}

fn slot(index: u32) -> Option<u8> {
    u8::try_from(index)
        .ok()
        .filter(|index| *index < MAX_STARTING_UNITS)
}

fn offset(field: &Field) -> Option<[f32; 3]> {
    let axis = |tag: &str| tagged_value(&field.args, tag).and_then(parse_float);
    Some([axis("X")?, axis("Y")?, axis("Z")?])
}

#[must_use]
pub fn read_player_template(block: &Block) -> PlayerTemplateDef {
    let text = |keyword: &str| {
        block
            .field(keyword)
            .and_then(Field::value)
            .map(str::to_owned)
    };
    let flag = |keyword: &str| {
        block
            .field(keyword)
            .and_then(Field::value)
            .and_then(parse_bool)
    };
    let names = |keyword: &str| {
        block
            .field(keyword)
            .map(|f| f.args.iter().filter(|a| !is_none(a)).cloned().collect())
            .unwrap_or_default()
    };
    let mut units: BTreeMap<u8, String> = BTreeMap::new();
    let mut offsets: BTreeMap<u8, [f32; 3]> = BTreeMap::new();
    let mut refused = 0usize;
    let mut wotr = Vec::new();
    for field in block.fields() {
        if field
            .keyword
            .eq_ignore_ascii_case("StartingUnitTacticalWOTR")
        {
            wotr.extend(field.args.iter().filter(|a| !is_none(a)).cloned());
            continue;
        }
        if let Some(index) = numbered(&field.keyword, "StartingUnitOffset") {
            if let (Some(index), Some(xyz)) = (slot(index), offset(field)) {
                offsets.insert(index, xyz);
            }
            continue;
        }
        let Some(index) = numbered(&field.keyword, "StartingUnit") else {
            continue;
        };
        match (slot(index), field.value()) {
            (Some(index), Some(template)) => {
                units.insert(index, template.to_owned());
            }
            _ => refused = refused.saturating_add(1),
        }
    }
    PlayerTemplateDef {
        name: block.name().unwrap_or_default().to_owned(),
        side: text("Side"),
        playable: flag("PlayableSide"),
        evil: flag("Evil"),
        starting_building: text("StartingBuilding"),
        starting_units: units
            .into_iter()
            .map(|(index, template)| StartingUnit {
                index,
                template,
                offset: offsets.get(&index).copied(),
            })
            .collect(),
        starting_units_refused: refused,
        multi_selection_portrait: text("MultiSelectionPortrait"),
        ring_heroes: names("BuildableRingHeroesMP"),
        heroes: names("BuildableHeroesMP"),
        intrinsic_sciences_mp: names("IntrinsicSciencesMP"),
        intrinsic_sciences: names("IntrinsicSciences"),
        spell_book_mp: text("SpellBookMp").filter(|name| !is_none(name)),
        spell_book: text("SpellBook").filter(|name| !is_none(name)),
        purchase_science_command_set_mp: text("PurchaseScienceCommandSetMP")
            .filter(|name| !is_none(name)),
        purchase_science_command_set: text("PurchaseScienceCommandSet")
            .filter(|name| !is_none(name)),
        max_level_mp: block.field("MaxLevelMP").map(|field| field.args.join(" ")),
        max_level_sp: block.field("MaxLevelSP").map(|field| field.args.join(" ")),
        intrinsic_science_purchase_points: block
            .field("IntrinsicSciencePurchasePoints")
            .map(|field| field.args.join(" ")),
        start_money: block
            .field("StartMoney")
            .and_then(Field::value)
            .and_then(parse_int),
        initial_upgrades: names("InitialUpgrades"),
        starting_units_wotr: wotr,
        production_cost_change: block
            .field("ProductionCostChange")
            .and_then(Field::value)
            .and_then(parse_percent),
        load_screen_music: text("LoadScreenMusic").filter(|name| !is_none(name)),
        objective_added_sound: text("ObjectiveAddedSound").filter(|name| !is_none(name)),
        objective_completed_sound: text("ObjectiveCompletedSound").filter(|name| !is_none(name)),
    }
}

#[derive(Debug, Clone, Default)]
pub struct PlayerTemplateIndex {
    defs: BTreeMap<String, PlayerTemplateDef>,
    order: Vec<String>,
}

impl PlayerTemplateDef {
    #[must_use]
    pub fn intrinsic_sciences_for(&self, multiplayer: bool) -> &[String] {
        if multiplayer {
            &self.intrinsic_sciences_mp
        } else {
            &self.intrinsic_sciences
        }
    }

    #[must_use]
    pub fn spell_book_for(&self, multiplayer: bool) -> Option<&str> {
        if multiplayer {
            self.spell_book_mp.as_deref()
        } else {
            self.spell_book.as_deref()
        }
    }

    #[must_use]
    pub fn max_level_for(&self, multiplayer: bool) -> &str {
        if multiplayer {
            self.max_level_mp.as_deref().unwrap_or(DEFAULT_MAX_LEVEL_MP)
        } else {
            self.max_level_sp.as_deref().unwrap_or(DEFAULT_MAX_LEVEL_SP)
        }
    }

    #[must_use]
    pub fn purchase_science_command_set_for(&self, multiplayer: bool) -> Option<&str> {
        if multiplayer {
            self.purchase_science_command_set_mp.as_deref()
        } else {
            self.purchase_science_command_set.as_deref()
        }
    }
}

impl PlayerTemplateIndex {
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
        if !block.keyword.eq(PLAYER_TEMPLATE_KEYWORD) {
            return;
        }
        let Some(name) = block.name() else {
            return;
        };
        let key = name.to_ascii_lowercase();
        if !self.defs.contains_key(&key) {
            self.order.push(key.clone());
        }
        self.defs.insert(key, read_player_template(block));
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&PlayerTemplateDef> {
        self.defs.get(&name.to_ascii_lowercase())
    }

    pub fn iter(&self) -> impl Iterator<Item = &PlayerTemplateDef> {
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

    fn index(text: &str) -> PlayerTemplateIndex {
        let document =
            Document::parse("playertemplate.ini", text, &BlockTable::corpus_path()).unwrap();
        let mut index = PlayerTemplateIndex::new();
        index.insert_document(&document);
        index
    }

    const MEN: &str = "\
PlayerTemplate FactionTest
  Side = Men
  PlayableSide = Yes
  Evil = No
  StartingBuilding = TestFortress
  ;StartingUnit0 = TestHorde
  StartingUnit1 = TestPorter
  StartingUnitOffset1 = X:30 Y:200 Z:0
  StartingUnit0 = TestPorter
  StartingUnitOffset0 = X:1 Y:130 Z:0
  StartingUnitTacticalWOTR = TestPorter
  MultiSelectionPortrait = UPTest_Army
  BuildableHeroesMP = CreateAHero TestHeroA TestHeroB
  BuildableRingHeroesMP = TestRingHero
End
";

    #[test]
    fn the_starting_units_are_read_in_index_order_with_their_offsets() {
        let index = index(MEN);
        let men = index.get("factiontest").unwrap();
        assert_eq!(men.side.as_deref(), Some("Men"));
        assert_eq!(men.playable, Some(true));
        assert_eq!(men.evil, Some(false));
        assert_eq!(men.starting_building.as_deref(), Some("TestFortress"));
        let indices: Vec<u8> = men.starting_units.iter().map(|u| u.index).collect();
        assert_eq!(indices, [0, 1], "sorted by N, not file order");
        assert_eq!(men.starting_units[0].offset, Some([1.0, 130.0, 0.0]));
        assert_eq!(men.starting_units[1].offset, Some([30.0, 200.0, 0.0]));
        assert_eq!(men.starting_units_refused, 0);
        assert_eq!(men.multi_selection_portrait.as_deref(), Some("UPTest_Army"));
        assert_eq!(men.heroes, ["CreateAHero", "TestHeroA", "TestHeroB"]);
        assert_eq!(men.ring_heroes, ["TestRingHero"]);
    }

    #[test]
    fn start_money_is_read_as_written() {
        let index = index(
            "PlayerTemplate Zero\n  StartMoney = 0\nEnd\n\
             PlayerTemplate NonZero\n  StartMoney = 5000\nEnd\n\
             PlayerTemplate Absent\nEnd\n",
        );
        assert_eq!(index.get("Zero").unwrap().start_money, Some(0));
        assert_eq!(index.get("NonZero").unwrap().start_money, Some(5000));
        assert_eq!(index.get("Absent").unwrap().start_money, None);
    }

    #[test]
    fn the_rank_cap_follows_the_game_mode_and_defaults_when_omitted() {
        let index = index(
            "PlayerTemplate Both
  MaxLevelMP = 5
  MaxLevelSP = 7
End
\n             PlayerTemplate Neither
End
",
        );
        let both = index.get("Both").unwrap();
        assert_eq!(both.max_level_for(true), "5");
        assert_eq!(both.max_level_for(false), "7");
        let neither = index.get("Neither").unwrap();
        assert_eq!(neither.max_level_for(true), "60");
        assert_eq!(neither.max_level_for(false), "100");
    }

    #[test]
    fn initial_upgrades_are_read_as_written() {
        let index = index(
            "PlayerTemplate F\n  InitialUpgrades = Upgrade_FactionA Upgrade_DualEconomy\nEnd\n\
             PlayerTemplate G\nEnd\n",
        );
        assert_eq!(
            index.get("F").unwrap().initial_upgrades,
            ["Upgrade_FactionA", "Upgrade_DualEconomy"]
        );
        assert!(index.get("G").unwrap().initial_upgrades.is_empty());
    }

    #[test]
    fn production_cost_change_reads_as_a_fraction() {
        let index =
            index("PlayerTemplate F\n  ProductionCostChange = -20\nEnd\nPlayerTemplate G\nEnd\n");
        assert_eq!(index.get("F").unwrap().production_cost_change, Some(-0.2));
        assert_eq!(index.get("G").unwrap().production_cost_change, None);
    }

    #[test]
    fn the_skirmish_spell_book_fields_are_read() {
        let index = index(
            "PlayerTemplate F\n  IntrinsicSciences = SCIENCE_GOOD\n  \
             IntrinsicSciencesMP = SCIENCE_MEN\n  SpellBook = GoodSpellBook\n  \
             SpellBookMp = MenSpellBook\n  PurchaseScienceCommandSetMP = MenSpellStoreCommandSet\n  \
             MaxLevelMP = #DIVIDE( A B )\n  MaxLevelSP = 5\nEnd\nPlayerTemplate G\n  SpellBookMp = None\nEnd\n",
        );
        let f = index.get("F").unwrap();
        assert_eq!(f.intrinsic_sciences_for(true), ["SCIENCE_MEN"]);
        assert_eq!(f.intrinsic_sciences_for(false), ["SCIENCE_GOOD"]);
        assert_eq!(f.spell_book_for(false), Some("GoodSpellBook"));
        assert_eq!(f.spell_book_for(true), Some("MenSpellBook"));
        assert_eq!(f.spell_book_mp.as_deref(), Some("MenSpellBook"));
        assert_eq!(
            f.purchase_science_command_set_mp.as_deref(),
            Some("MenSpellStoreCommandSet")
        );
        assert_eq!(f.max_level_mp.as_deref(), Some("#DIVIDE( A B )"));
        assert_eq!(f.max_level_for(false), "5");
        assert_eq!(f.max_level_for(true), "#DIVIDE( A B )");
        assert_eq!(index.get("G").unwrap().spell_book_mp, None, "None clears");
    }

    #[test]
    fn a_slot_past_nine_is_refused_and_a_broken_offset_reads_as_absent() {
        let index = index(
            "PlayerTemplate F\n  StartingUnit10 = A\n  StartingUnit2 = B\n  \
             StartingUnitOffset2 = X:1 Y:NaN Z:0\n  StartingUnit9 = C\n  \
             StartingUnitOffset9 = X:1 Z:0\n  StartingUnit99999999999 = D\nEnd\n",
        );
        let f = index.get("F").unwrap();
        let names: Vec<&str> = f
            .starting_units
            .iter()
            .map(|u| u.template.as_str())
            .collect();
        assert_eq!(names, ["B", "C"]);
        assert!(f.starting_units.iter().all(|u| u.offset.is_none()));
        assert_eq!(f.starting_units_refused, 2);
    }

    #[test]
    fn declaration_order_is_kept_and_a_redefinition_keeps_its_place() {
        let index = index(
            "PlayerTemplate Zeta\n  Side = Z\nEnd\nPlayerTemplate Alpha\nEnd\n\
             PlayerTemplate Zeta\n  Side = Z2\nEnd\n",
        );
        let names: Vec<&str> = index.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Zeta", "Alpha"]);
        assert_eq!(index.get("zeta").unwrap().side.as_deref(), Some("Z2"));
    }

    #[test]
    fn every_truncation_reads_without_panicking() {
        for end in 0..=MEN.len() {
            let Some(prefix) = MEN.get(..end) else {
                continue;
            };
            if let Ok(document) =
                Document::parse("playertemplate.ini", prefix, &BlockTable::corpus_path())
            {
                let mut index = PlayerTemplateIndex::new();
                index.insert_document(&document);
                let _ = index.get("FactionTest");
            }
        }
    }
}
