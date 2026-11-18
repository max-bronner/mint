use crate::tree::{Document, Field};

pub const COMMAND_MAP_KEYWORD: &str = "CommandMap";

#[must_use]
pub fn command_map_keys(documents: &[Document]) -> std::collections::BTreeMap<String, String> {
    let mut keys = std::collections::BTreeMap::new();
    for document in documents {
        for block in document.roots() {
            if !block.keyword.eq_ignore_ascii_case(COMMAND_MAP_KEYWORD) {
                continue;
            }
            let Some(meta) = block.name() else { continue };
            let key = block.field("Key").and_then(Field::value);
            let token = match key {
                Some(key) if !key.eq_ignore_ascii_case("KEY_NONE") => Some(key),
                _ => block.field("Modifiers").and_then(Field::value),
            };
            let Some(token) = token else { continue };
            keys.insert(meta.to_ascii_uppercase(), token.to_ascii_uppercase());
        }
    }
    keys
}

#[must_use]
pub fn command_map_transitions(
    documents: &[Document],
) -> std::collections::BTreeMap<String, String> {
    let mut transitions = std::collections::BTreeMap::new();
    for document in documents {
        for block in document.roots() {
            if !block.keyword.eq_ignore_ascii_case(COMMAND_MAP_KEYWORD) {
                continue;
            }
            let Some(meta) = block.name() else { continue };
            if let Some(transition) = block.field("Transition").and_then(Field::value) {
                transitions.insert(meta.to_ascii_uppercase(), transition.to_ascii_uppercase());
            }
        }
    }
    transitions
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::table::BlockTable;

    fn parse(text: &str) -> Document {
        let document = Document::parse("commandmap.ini", text, &BlockTable::corpus_path()).unwrap();
        assert!(document.is_balanced(), "{:?}", document.diagnostics());
        document
    }

    #[test]
    fn a_meta_s_key_is_read_upper_cased() {
        let document = parse(
            "CommandMap ORDERMODE_WAYPOINT\n  Key = Alt\n  Transition = DOWN\nEnd\n\
             CommandMap ORDERMODE_IMMEDIATE\n  Key = Alt\n  Transition = UP\nEnd\n",
        );
        let keys = command_map_keys(&[document]);
        assert_eq!(
            keys.get("ORDERMODE_WAYPOINT").map(String::as_str),
            Some("ALT")
        );
        assert_eq!(
            keys.get("ORDERMODE_IMMEDIATE").map(String::as_str),
            Some("ALT")
        );
    }

    #[test]
    fn a_metas_transition_is_the_edge_it_fires_on() {
        let document = parse(
            "CommandMap DIPLOMACY
  Key = KEY_TAB
  Transition = UP
End
             CommandMap OTHER
  Key = KEY_A
End
",
        );
        let transitions = command_map_transitions(&[document]);
        assert_eq!(transitions.get("DIPLOMACY").map(String::as_str), Some("UP"));
        assert!(!transitions.contains_key("OTHER"));
    }

    #[test]
    fn a_later_block_for_the_same_meta_replaces_the_earlier_key() {
        let document = parse("CommandMap FOO\n  Key = A\nEnd\nCommandMap FOO\n  Key = B\nEnd\n");
        let keys = command_map_keys(&[document]);
        assert_eq!(keys.get("FOO").map(String::as_str), Some("B"));
    }

    #[test]
    fn key_none_falls_back_to_modifiers() {
        let document = parse(
            "CommandMap ORDERMODE_WAYPOINT\n  Key = KEY_NONE\n  Transition = DOWN\n  \
             Modifiers = ALT\nEnd\n",
        );
        let keys = command_map_keys(&[document]);
        assert_eq!(
            keys.get("ORDERMODE_WAYPOINT").map(String::as_str),
            Some("ALT")
        );
    }
}
