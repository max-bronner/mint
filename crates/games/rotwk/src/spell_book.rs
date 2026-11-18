use std::collections::BTreeMap;

use mint_sage_ini::{BlockTable, Document, MemorySources, Reader};
use mint_sage_str::StringTable;
use mint_vfs::Vfs;

use crate::rules::{RotwkCommand, RotwkRules};
pub(crate) use crate::sciences::SpellState;

const SCIENCE_INI: &str = "data/ini/science.ini";
const PLAYER_TEMPLATE_INI: &str = "data/ini/playertemplate.ini";
const COMMAND_BUTTON_INI: &str = "data/ini/commandbutton.ini";
const COMMAND_SET_INI: &str = "data/ini/commandset.ini";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ScienceDef {
    pub(crate) cost_mp: Option<u32>,
    pub(crate) cost: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SpellStoreTemplate {
    pub(crate) command_set: Option<String>,
    pub(crate) command_set_sp: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SpellBookData {
    pub(crate) sciences: BTreeMap<String, ScienceDef>,
    pub(crate) templates: BTreeMap<String, SpellStoreTemplate>,
    pub(crate) button_sciences: BTreeMap<String, String>,
    pub(crate) command_sets: mint_sage_ini::CommandSetIndex,
}

fn named_blocks<'a>(
    documents: &'a [Document],
    keyword: &'a str,
) -> impl Iterator<Item = (&'a str, &'a mint_sage_ini::Block)> + 'a {
    documents
        .iter()
        .flat_map(Document::roots)
        .filter(move |block| block.keyword.eq_ignore_ascii_case(keyword))
        .filter_map(|block| Some((block.name()?, block)))
}

fn whole(token: Option<&str>) -> Option<u32> {
    let value = mint_sage_ini::value::parse_float(token?)?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = value.max(0.0).round() as u32;
    value.is_finite().then_some(whole)
}

impl SpellBookData {
    #[must_use]
    pub(crate) fn read(documents: &[Document]) -> Self {
        let mut data = Self::default();
        for document in documents {
            data.command_sets.insert_document(document);
        }
        for (name, block) in named_blocks(documents, "Science") {
            let read =
                |field: &str| whole(block.field(field).and_then(mint_sage_ini::Field::value));
            data.sciences.insert(
                name.to_ascii_lowercase(),
                ScienceDef {
                    cost_mp: read("SciencePurchasePointCostMP"),
                    cost: read("SciencePurchasePointCost"),
                },
            );
        }
        for (name, block) in named_blocks(documents, "PlayerTemplate") {
            let command_set_field = |field: &str| {
                block
                    .field(field)
                    .and_then(mint_sage_ini::Field::value)
                    .filter(|v| !mint_sage_ini::value::is_none(v))
                    .map(str::to_owned)
            };
            data.templates.insert(
                name.to_ascii_lowercase(),
                SpellStoreTemplate {
                    command_set: command_set_field("PurchaseScienceCommandSetMP"),
                    command_set_sp: command_set_field("PurchaseScienceCommandSet"),
                },
            );
        }
        for (name, block) in named_blocks(documents, "CommandButton") {
            if let Some(science) = block
                .field("Science")
                .and_then(mint_sage_ini::Field::value)
                .filter(|v| !mint_sage_ini::value::is_none(v))
            {
                data.button_sciences
                    .insert(name.to_ascii_lowercase(), science.to_owned());
            }
        }
        data
    }

    pub(crate) fn load(vfs: &Vfs) -> Result<Self, String> {
        let table = BlockTable::corpus_path();
        let mut documents = Vec::new();
        for path in [
            SCIENCE_INI,
            PLAYER_TEMPLATE_INI,
            COMMAND_BUTTON_INI,
            COMMAND_SET_INI,
        ] {
            if !vfs.contains(path) {
                println!("spell book: {path} is not in the install - its part is empty");
                continue;
            }
            let bytes = vfs.read(path).map_err(|error| format!("{path}: {error}"))?;
            let mut sources = MemorySources::new();
            sources.insert(path, bytes);
            let document = Reader::new(&sources)
                .read(path, &table)
                .map_err(|error| format!("{path}: {error}"))?;
            documents.push(document);
        }
        Ok(Self::read(&documents))
    }

    #[must_use]
    pub(crate) fn template(&self, template: &str) -> Option<&SpellStoreTemplate> {
        self.templates.get(&template.to_ascii_lowercase())
    }

    #[must_use]
    pub(crate) fn science(&self, name: &str) -> Option<&ScienceDef> {
        self.sciences.get(&name.to_ascii_lowercase())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SpellBookHoldings {
    pub(crate) points: u32,
    pub(crate) owned: Vec<String>,
    pub(crate) states: Vec<(String, SpellState)>,
}

#[must_use]
pub(crate) fn holdings_of(book: Option<&crate::sciences::SpellBook>) -> SpellBookHoldings {
    book.map_or_else(SpellBookHoldings::default, |book| SpellBookHoldings {
        points: book.points,
        owned: book.held.clone(),
        states: book
            .entries
            .iter()
            .map(|entry| (entry.science.clone(), entry.state))
            .collect(),
    })
}

#[must_use]
pub(crate) fn spell_state(science: &str, holdings: &SpellBookHoldings) -> SpellState {
    if let Some((_, state)) = holdings
        .states
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(science))
    {
        return *state;
    }
    if holdings
        .owned
        .iter()
        .any(|o| o.eq_ignore_ascii_case(science))
    {
        SpellState::Owned
    } else {
        SpellState::Locked
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpellSlotView {
    pub(crate) science: String,
    pub(crate) image: Option<String>,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) cost: u32,
    pub(crate) state: SpellState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SpellBookView {
    pub(crate) points: u32,
    pub(crate) slots: Vec<Option<SpellSlotView>>,
}

#[must_use]
pub(crate) fn spell_book_view(
    data: &SpellBookData,
    command_buttons: &mint_sage_ini::CommandButtonIndex,
    strings: &StringTable,
    template: Option<&str>,
    multiplayer: bool,
    holdings: &SpellBookHoldings,
) -> SpellBookView {
    let store = template.and_then(|t| data.template(t));
    let set = store
        .and_then(|t| {
            if multiplayer {
                t.command_set.as_deref()
            } else {
                t.command_set_sp.as_deref()
            }
        })
        .and_then(|name| data.command_sets.get(name));
    let last = set
        .and_then(mint_sage_ini::CommandSetDef::max_slot)
        .unwrap_or(0);
    let text = |key: Option<&str>| {
        key.and_then(|k| strings.get(k))
            .map(crate::help_box::strip_mnemonic)
            .unwrap_or_default()
    };
    let slots = (1..=last)
        .map(|n| {
            let button_name = set?.slot(n)?;
            let button = command_buttons.get(button_name)?;
            let science = data
                .button_sciences
                .get(&button_name.to_ascii_lowercase())?;
            let def = data.science(science);
            Some(SpellSlotView {
                science: science.clone(),
                image: button.button_image.clone(),
                name: text(button.text_label.as_deref()),
                description: text(button.descript_label.as_deref()),
                cost: def
                    .and_then(|d| if multiplayer { d.cost_mp } else { d.cost })
                    .unwrap_or(0),
                state: spell_state(science, holdings),
            })
        })
        .collect();
    SpellBookView {
        points: holdings.points,
        slots,
    }
}

#[must_use]
pub(crate) fn purchase_clicked(rules: &RotwkRules, science: &str) -> Option<RotwkCommand> {
    let command = rules.purchase_command(science);
    if command.is_none() {
        println!("spell book: {science} is not a science this match knows");
    }
    command
}

#[cfg(test)]
mod tests;
