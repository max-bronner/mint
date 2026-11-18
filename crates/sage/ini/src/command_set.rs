use std::collections::BTreeMap;

use crate::tree::{Block, Document};

pub const COMMAND_SET_KEYWORD: &str = "CommandSet";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommandSetDef {
    pub slots: BTreeMap<u32, String>,
    pub initial_visible: Option<i64>,
}

impl CommandSetDef {
    #[must_use]
    pub fn slot(&self, index: u32) -> Option<&str> {
        self.slots.get(&index).map(String::as_str)
    }

    #[must_use]
    pub fn max_slot(&self) -> Option<u32> {
        self.slots.keys().next_back().copied()
    }
}

#[must_use]
pub fn read_command_set(block: &Block) -> CommandSetDef {
    let mut slots = BTreeMap::new();
    for field in block.fields() {
        if field.keyword.is_empty() || !field.keyword.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(index) = field.keyword.parse::<u32>() else {
            continue;
        };
        if let Some(name) = field.value() {
            slots.insert(index, name.to_owned());
        }
    }
    let initial_visible = block
        .field("InitialVisible")
        .and_then(crate::tree::Field::value)
        .and_then(crate::value::parse_int);
    CommandSetDef {
        slots,
        initial_visible,
    }
}

#[derive(Debug, Clone, Default)]
pub struct CommandSetIndex {
    defs: BTreeMap<String, CommandSetDef>,
    definitions: usize,
}

impl CommandSetIndex {
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
        if !block.keyword.eq(COMMAND_SET_KEYWORD) {
            return;
        }
        let Some(name) = block.name() else {
            return;
        };
        self.definitions = self.definitions.saturating_add(1);
        self.defs
            .insert(name.to_ascii_lowercase(), read_command_set(block));
    }

    pub fn insert(&mut self, name: &str, def: CommandSetDef) {
        self.defs.insert(name.to_ascii_lowercase(), def);
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&CommandSetDef> {
        self.defs.get(&name.to_ascii_lowercase())
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
#[allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]
mod tests {
    use super::*;
    use crate::table::BlockTable;

    #[test]
    fn numbered_fields_become_slots_in_order() {
        let doc = crate::tree::Document::parse(
            "commandset.ini",
            "CommandSet ShelobCommandSet\n  \
                1 = Command_ToggleStance\n  \
                2 = Command_SpecialAbilityShelobWebbing\n  \
                14 = Command_Stop\n\
            End\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let def = read_command_set(&doc.roots()[0]);
        assert_eq!(def.slot(1), Some("Command_ToggleStance"));
        assert_eq!(def.slot(2), Some("Command_SpecialAbilityShelobWebbing"));
        assert_eq!(def.slot(3), None);
        assert_eq!(def.max_slot(), Some(14));
        assert_eq!(def.initial_visible, None);
    }

    #[test]
    fn initial_visible_is_read_alongside_the_slots() {
        let doc = crate::tree::Document::parse(
            "commandset.ini",
            "CommandSet ArnorFortressCommandSet\n  \
                InitialVisible = 6\n  \
                1 = Command_ConstructArnorPorter\n\
            End\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let def = read_command_set(&doc.roots()[0]);
        assert_eq!(def.initial_visible, Some(6));
        assert_eq!(def.slot(1), Some("Command_ConstructArnorPorter"));
    }
}
