use std::collections::BTreeSet;

use crate::diagnostic::{ValueError, ValueErrors};
use crate::source::MemorySources;
use crate::subsystem_legend::{read_legend, retail_load_order};
use crate::table::BlockTable;
use crate::tree::{Diagnostic, Document};
use crate::value::MacroTable;
use crate::{Error, Reader};

use super::data::{LivingWorldData, ROOT_KEYWORDS};
use super::grammar::Cx;

pub const SUBSYSTEM_LEGEND_PATHS: [&str; 2] = [
    "data/ini/default/subsystemlegendexpansion1.ini",
    "data/ini/default/subsystemlegend.ini",
];

const OBJECT_KEYWORDS: [&str; 2] = ["Object", "ChildObject"];
const PLAYER_TEMPLATE_KEYWORD: &str = "PlayerTemplate";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoadError {
    #[error("{0}")]
    Value(ValueError),
    #[error("{0}")]
    Structural(Diagnostic),
}

#[derive(Debug, Clone)]
pub struct LivingWorldLoad {
    pub data: LivingWorldData,
    pub errors: Vec<LoadError>,
    pub unreadable: Vec<(String, Error)>,
    pub documents: usize,
    pub living_world_documents: usize,
    pub object_names: BTreeSet<String>,
    pub player_template_names: BTreeSet<String>,
}

fn is_living_world(block: &crate::tree::Block) -> bool {
    ROOT_KEYWORDS.contains(&block.keyword.as_str())
}

#[must_use]
pub fn load_living_world(sources: &MemorySources) -> LivingWorldLoad {
    let table = BlockTable::corpus_path();
    let reader = Reader::new(sources);
    let legend = SUBSYSTEM_LEGEND_PATHS
        .iter()
        .find_map(|path| reader.read(path, &table).ok());
    let available: BTreeSet<String> = sources
        .paths()
        .filter(|path| path.ends_with(".ini"))
        .map(str::to_owned)
        .collect();
    let order = legend
        .as_ref()
        .map(|document| retail_load_order(&read_legend(document), &available))
        .unwrap_or_default();
    load_files(sources, &order)
}

#[must_use]
pub fn load_files(sources: &MemorySources, files: &[String]) -> LivingWorldLoad {
    let table = BlockTable::corpus_path();
    let reader = Reader::new(sources);
    let mut macros = MacroTable::new();
    let mut retained: Vec<Document> = Vec::new();
    let mut errors: Vec<LoadError> = Vec::new();
    let mut unreadable = Vec::new();
    let mut object_names = BTreeSet::new();
    let mut player_template_names = BTreeSet::new();
    let mut documents = 0usize;
    for path in files {
        let document = match reader.read(path, &table) {
            Ok(document) => document,
            Err(error) => {
                unreadable.push((path.clone(), error));
                continue;
            }
        };
        documents = documents.saturating_add(1);
        macros.insert_defines(document.defines());
        for root in document.roots() {
            let Some(name) = root.name() else { continue };
            if OBJECT_KEYWORDS.contains(&root.keyword.as_str()) {
                object_names.insert(name.to_ascii_lowercase());
            } else if root.keyword == PLAYER_TEMPLATE_KEYWORD {
                player_template_names.insert(name.to_ascii_lowercase());
            }
        }
        if document.roots().iter().any(is_living_world) {
            errors.extend(
                document
                    .diagnostics()
                    .iter()
                    .filter(|d| !matches!(d, Diagnostic::IncludeMissing { .. }))
                    .cloned()
                    .map(LoadError::Structural),
            );
            retained.push(document.retaining_roots(is_living_world));
        }
    }
    let living_world_documents = retained.len();
    let value_errors = ValueErrors::new();
    let cx = Cx {
        macros: &macros,
        errors: &value_errors,
    };
    let mut data = LivingWorldData::default();
    for document in &mut retained {
        document.expand_token_list_macros(&macros);
        for root in document.roots() {
            data.read_root(root, &cx);
        }
    }
    errors.extend(value_errors.take().into_iter().map(LoadError::Value));
    LivingWorldLoad {
        data,
        errors,
        unreadable,
        documents,
        living_world_documents,
        object_names,
        player_template_names,
    }
}

#[must_use]
pub fn check_block(block: &crate::tree::Block, macros: &MacroTable) -> Vec<ValueError> {
    let value_errors = ValueErrors::new();
    let cx = Cx {
        macros,
        errors: &value_errors,
    };
    let mut scratch = LivingWorldData::default();
    scratch.read_root(block, &cx);
    value_errors.take()
}

#[must_use]
pub fn read_documents(
    documents: &[Document],
    macros: &MacroTable,
) -> (LivingWorldData, Vec<LoadError>) {
    let value_errors = ValueErrors::new();
    let cx = Cx {
        macros,
        errors: &value_errors,
    };
    let mut data = LivingWorldData::default();
    let mut errors = Vec::new();
    for document in documents {
        for root in document.roots() {
            data.read_root(root, &cx);
        }
        errors.extend(
            document
                .diagnostics()
                .iter()
                .filter(|d| !matches!(d, Diagnostic::IncludeMissing { .. }))
                .cloned()
                .map(LoadError::Structural),
        );
    }
    errors.extend(value_errors.take().into_iter().map(LoadError::Value));
    (data, errors)
}
