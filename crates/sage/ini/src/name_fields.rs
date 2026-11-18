use std::collections::BTreeMap;

use crate::tree::{Block, Document, Node};

pub const MAX_NAME_FIELDS: usize = 4096;

pub const MAX_NAME_FIELD_DEPTH: u32 = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameField {
    pub owner: String,
    pub keyword: String,
    pub value: String,
    pub siblings: Vec<(String, String)>,
}

impl NameField {
    #[must_use]
    pub fn sibling(&self, keyword: &str) -> Option<&str> {
        self.siblings
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(keyword))
            .map(|(_, value)| value.as_str())
    }
}

impl NameField {
    #[must_use]
    pub fn label(&self) -> String {
        if self.owner.is_empty() {
            self.keyword.clone()
        } else {
            format!("{}.{}", self.owner, self.keyword)
        }
    }
}

#[must_use]
pub fn is_name_shaped(value: &str) -> bool {
    if value.is_empty() || value.eq_ignore_ascii_case("NONE") {
        return false;
    }
    let first = value.as_bytes().first().copied().unwrap_or(0);
    if !first.is_ascii_alphabetic() && first != b'_' {
        return false;
    }
    value.parse::<f64>().is_err()
}

#[must_use]
pub fn read_name_fields(block: &Block) -> Vec<NameField> {
    read_name_fields_with(block, &[])
}

#[must_use]
pub fn read_name_fields_with(block: &Block, siblings: &[String]) -> Vec<NameField> {
    let mut rows = Vec::new();
    walk(block, String::new(), 0, siblings, &mut rows);
    rows
}

fn siblings_of(block: &Block, wanted: &[String]) -> Vec<(String, String)> {
    if wanted.is_empty() {
        return Vec::new();
    }
    block
        .children
        .iter()
        .filter_map(|node| match node {
            Node::Field(field) if field.args.len() == 1 => Some(field),
            _ => None,
        })
        .filter(|field| {
            wanted
                .iter()
                .any(|keyword| keyword.eq_ignore_ascii_case(&field.keyword))
        })
        .filter_map(|field| Some((field.keyword.clone(), field.value()?.to_owned())))
        .collect()
}

fn owner_label(block: &Block) -> String {
    block
        .args
        .get(1)
        .or_else(|| block.args.first())
        .cloned()
        .unwrap_or_else(|| block.keyword.clone())
}

fn walk(block: &Block, owner: String, depth: u32, wanted: &[String], rows: &mut Vec<NameField>) {
    let siblings = siblings_of(block, wanted);
    for node in &block.children {
        if rows.len() >= MAX_NAME_FIELDS {
            return;
        }
        match node {
            Node::Field(field) => {
                if field.args.len() != 1 {
                    continue;
                }
                let Some(value) = field.value() else { continue };
                if !is_name_shaped(value) {
                    continue;
                }
                rows.push(NameField {
                    owner: owner.clone(),
                    keyword: field.keyword.clone(),
                    value: value.to_owned(),
                    siblings: siblings.clone(),
                });
            }
            Node::Block(child) => {
                if depth >= MAX_NAME_FIELD_DEPTH {
                    continue;
                }
                let label = if owner.is_empty() {
                    owner_label(child)
                } else {
                    owner.clone()
                };
                walk(child, label, depth.saturating_add(1), wanted, rows);
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct NameFieldIndex {
    rows: BTreeMap<(String, String), Vec<NameField>>,
    siblings: Vec<String>,
}

impl NameFieldIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_siblings(keywords: &[&str]) -> Self {
        Self {
            rows: BTreeMap::new(),
            siblings: keywords
                .iter()
                .map(|keyword| (*keyword).to_owned())
                .collect(),
        }
    }

    pub fn insert_document(&mut self, document: &Document) {
        for block in document.roots() {
            self.insert_block(block);
        }
    }

    pub fn insert_block(&mut self, block: &Block) {
        let Some(name) = block.name() else {
            return;
        };
        let rows = read_name_fields_with(block, &self.siblings);
        if rows.is_empty() {
            return;
        }
        self.rows.insert(
            (
                block.keyword.to_ascii_lowercase(),
                name.to_ascii_lowercase(),
            ),
            rows,
        );
    }

    #[must_use]
    pub fn get(&self, keyword: &str, name: &str) -> &[NameField] {
        self.rows
            .get(&(keyword.to_ascii_lowercase(), name.to_ascii_lowercase()))
            .map_or(&[], Vec::as_slice)
    }

    #[must_use]
    pub fn keywords(&self) -> Vec<String> {
        let mut out: Vec<String> = self.rows.keys().map(|(block, _)| block.clone()).collect();
        out.dedup();
        out
    }

    pub fn names_of<'a>(&'a self, keyword: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        let keyword = keyword.to_ascii_lowercase();
        self.rows
            .keys()
            .filter(move |(block, _)| *block == keyword)
            .map(|(_, name)| name.as_str())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::table::BlockTable;
    use crate::{MemorySources, Reader};

    fn document(text: &str) -> Document {
        let mut sources = MemorySources::new();
        sources.insert("t.ini", text.as_bytes().to_vec());
        Reader::new(&sources)
            .read("t.ini", &BlockTable::corpus_path())
            .unwrap()
    }

    #[test]
    fn a_field_is_recorded_by_the_shape_of_its_value_not_by_its_keyword() {
        let document = document(
            "Object TestThing\n\
             Behavior = MadeUpModule ModuleTag_09\n\
             InventedField = FX_MadeUp\n\
             Radius = 12.5\n\
             Enabled = Yes\n\
             Pair = A B\n\
             Cleared = NONE\n\
             End\n\
             End\n",
        );
        let root = document.roots().first().unwrap();
        let rows = read_name_fields(root);
        let labels: Vec<String> = rows.iter().map(NameField::label).collect();
        assert_eq!(
            labels,
            vec![
                "ModuleTag_09.InventedField".to_owned(),
                "ModuleTag_09.Enabled".to_owned()
            ]
        );
        assert_eq!(
            rows.first().map(|row| row.value.as_str()),
            Some("FX_MadeUp")
        );
    }
}
