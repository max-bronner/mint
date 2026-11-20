use mint_sage_ini::{BlockTable, Document, MemorySources, Reader};
use mint_sage_str::{StringRecord, StringTable};

use super::{
    holdings_of, spell_book_view, spell_state, SpellBookData, SpellBookHoldings, SpellState,
    SpellStoreTemplate,
};

fn parse(text: &str) -> Document {
    let mut sources = MemorySources::new();
    sources.insert("data/ini/science.ini", text.as_bytes().to_vec());
    Reader::new(&sources)
        .read("data/ini/science.ini", &BlockTable::corpus_path())
        .expect("synthetic INI parses")
}

fn owned(names: &[&str]) -> Vec<String> {
    names.iter().map(|&n| n.to_owned()).collect()
}

#[test]
fn a_spells_state_is_the_simulations() {
    let holdings = SpellBookHoldings {
        points: 100,
        owned: owned(&["SCIENCE_MEN", "SCIENCE_Old"]),
        states: vec![
            ("SCIENCE_Heal".to_owned(), SpellState::CantAfford),
            ("SCIENCE_Rally".to_owned(), SpellState::Available),
        ],
    };
    assert_eq!(
        spell_state("science_heal", &holdings),
        SpellState::CantAfford,
        "not the points' own answer, and names match any case"
    );
    assert_eq!(
        spell_state("SCIENCE_Rally", &holdings),
        SpellState::Available
    );
    assert_eq!(spell_state("SCIENCE_Old", &holdings), SpellState::Owned);
    assert_eq!(spell_state("SCIENCE_New", &holdings), SpellState::Locked);
}

#[test]
fn holdings_are_read_off_the_simulations_book() {
    let book = crate::sciences::SpellBook {
        points: 7,
        rank: 3,
        skill_points: 130,
        next_rank_at: Some(180),
        entries: vec![crate::sciences::SpellEntry {
            science: "SCIENCE_Heal".to_owned(),
            id: 2,
            button: "Command_PurchaseSpellHeal".to_owned(),
            cost: 5,
            state: SpellState::Available,
        }],
        held: owned(&["SCIENCE_MEN"]),
    };
    assert_eq!(
        holdings_of(Some(&book)),
        SpellBookHoldings {
            points: 7,
            owned: owned(&["SCIENCE_MEN"]),
            states: vec![("SCIENCE_Heal".to_owned(), SpellState::Available)],
        }
    );
    assert_eq!(holdings_of(None), SpellBookHoldings::default());
}

const INI: &str = "\
Science SCIENCE_MEN
  PrerequisiteSciences = None
  SciencePurchasePointCost = 0
  IsGrantable = No
End
Science SCIENCE_Heal
  PrerequisiteSciences = SCIENCE_GOOD OR SCIENCE_MEN
  SciencePurchasePointCost = 7
  SciencePurchasePointCostMP = 5
  IsGrantable = Yes
End
Science SCIENCE_Old
  PrerequisiteSciences = SCIENCE_Heal
  SciencePurchasePointCost = 3
End
PlayerTemplate FactionMen
  Side = Men
  IntrinsicSciencesMP = SCIENCE_MEN
  PurchaseScienceCommandSetMP = MenSpellStoreCommandSet
  PurchaseScienceCommandSet = CampaignSpellStoreCommandSet
End
CommandButton Command_PurchaseSpellHeal
  Command = PURCHASE_SCIENCE
  ButtonImage = SBGood_Heal
  Science = SCIENCE_Heal
  TextLabel = CONTROLBAR:Heal
  DescriptLabel = CONTROLBAR:TooltipHeal
End
CommandButton Command_PurchaseSpellOld
  Command = PURCHASE_SCIENCE
  Science = SCIENCE_Old
  TextLabel = CONTROLBAR:Old
End
CommandSet MenSpellStoreCommandSet
  1 = Command_PurchaseSpellHeal
  3 = Command_PurchaseSpellOld
End
CommandSet CampaignSpellStoreCommandSet
  2 = Command_PurchaseSpellOld
End
";

#[test]
fn the_ini_reads_costs_templates_and_button_sciences() {
    let data = SpellBookData::read(&[parse(INI)]);
    let heal = data.science("science_heal").unwrap();
    assert_eq!((heal.cost_mp, heal.cost), (Some(5), Some(7)));
    let old = data.science("SCIENCE_Old").unwrap();
    assert_eq!((old.cost_mp, old.cost), (None, Some(3)));
    assert_eq!(
        data.template("factionmen"),
        Some(&SpellStoreTemplate {
            command_set: Some("MenSpellStoreCommandSet".to_owned()),
            command_set_sp: Some("CampaignSpellStoreCommandSet".to_owned()),
        })
    );
    assert_eq!(
        data.button_sciences
            .get("command_purchasespellheal")
            .map(String::as_str),
        Some("SCIENCE_Heal")
    );
}

#[test]
fn the_view_follows_the_command_sets_slots() {
    let document = parse(INI);
    let data = SpellBookData::read(std::slice::from_ref(&document));
    let mut buttons = mint_sage_ini::CommandButtonIndex::new();
    buttons.insert_document(&document, &mint_sage_ini::MacroTable::new());
    let mut strings = StringTable::new();
    strings.insert(
        [
            ("CONTROLBAR:Heal", "&Heal"),
            ("CONTROLBAR:TooltipHeal", "Heals units"),
        ]
        .map(|(label, text)| StringRecord {
            label: label.to_owned(),
            text: text.to_owned(),
            speech: None,
        }),
    );
    let holdings = SpellBookHoldings {
        points: 4,
        owned: owned(&["SCIENCE_MEN"]),
        states: vec![("SCIENCE_Heal".to_owned(), SpellState::CantAfford)],
    };
    let view = spell_book_view(
        &data,
        &buttons,
        &strings,
        Some("FactionMen"),
        true,
        &holdings,
    );
    assert_eq!(view.points, 4);
    assert_eq!(view.slots.len(), 3);
    let heal = view.slots[0].as_ref().expect("slot 1");
    assert_eq!(heal.science, "SCIENCE_Heal");
    assert_eq!(heal.name, "Heal");
    assert_eq!(heal.description, "Heals units");
    assert_eq!(heal.image.as_deref(), Some("SBGood_Heal"));
    assert_eq!(heal.cost, 5);
    assert_eq!(heal.state, SpellState::CantAfford);
    assert_eq!(view.slots[1], None, "slot 2 is empty");
    let old = view.slots[2].as_ref().expect("slot 3");
    assert_eq!(old.state, SpellState::Locked);
    assert_eq!(
        spell_book_view(&data, &buttons, &strings, None, true, &holdings).slots,
        Vec::new(),
        "no template, no book"
    );
    let campaign = spell_book_view(
        &data,
        &buttons,
        &strings,
        Some("FactionMen"),
        false,
        &holdings,
    );
    assert_eq!(campaign.slots.len(), 2, "the campaign reads its own set");
    assert!(campaign.slots[0].is_none() && campaign.slots[1].is_some());
}
