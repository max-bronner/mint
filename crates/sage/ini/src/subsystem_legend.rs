use std::collections::BTreeSet;

use crate::source::normalise_path;
use crate::tree::{Block, Document};

pub const LOAD_SUBSYSTEM_KEYWORD: &str = "LoadSubsystem";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LegendEntry {
    pub name: String,
    pub init_files: Vec<String>,
    pub init_path: Option<String>,
    pub exclude_paths: Vec<String>,
}

#[must_use]
pub fn read_legend(document: &Document) -> Vec<LegendEntry> {
    document
        .roots_named(LOAD_SUBSYSTEM_KEYWORD)
        .map(read_entry)
        .collect()
}

fn read_entry(block: &Block) -> LegendEntry {
    let mut entry = LegendEntry {
        name: block.name().unwrap_or_default().to_owned(),
        ..LegendEntry::default()
    };
    for field in block.fields() {
        let Some(value) = field.value() else {
            continue;
        };
        if field.keyword.eq("InitFile") {
            entry.init_files.push(normalise_path(value));
        } else if field.keyword.eq("InitPath") {
            entry.init_path = Some(normalise_path(value));
        } else if field.keyword.eq("ExcludePath") {
            entry.exclude_paths.push(normalise_path(value));
        }
    }
    entry
}

#[must_use]
pub fn retail_load_order(legend: &[LegendEntry], available: &BTreeSet<String>) -> Vec<String> {
    let mut order = Vec::new();
    let mut seen = BTreeSet::new();
    for entry in legend {
        for file in &entry.init_files {
            if available.contains(file) && seen.insert(file.clone()) {
                order.push(file.clone());
            }
        }
        if let Some(root) = &entry.init_path {
            for file in directory_ini_files(root, &entry.exclude_paths, available) {
                if seen.insert(file.clone()) {
                    order.push(file);
                }
            }
        }
    }
    order
}

fn directory_ini_files(
    root: &str,
    excludes: &[String],
    available: &BTreeSet<String>,
) -> Vec<String> {
    let prefix = format!("{}/", root.trim_end_matches('/'));
    let mut direct = Vec::new();
    let mut nested = Vec::new();
    'paths: for path in available {
        let Some(rest) = path.strip_prefix(&prefix) else {
            continue;
        };
        if !path.ends_with(".ini") {
            continue;
        }
        for exclude in excludes {
            let exclude = exclude.trim_end_matches('/');
            if path == exclude || path.starts_with(&format!("{exclude}/")) {
                continue 'paths;
            }
        }
        if rest.contains('/') {
            nested.push(path.clone());
        } else {
            direct.push(path.clone());
        }
    }
    direct.sort();
    nested.sort();
    direct.into_iter().chain(nested).collect()
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::table::BlockTable;

    fn legend(text: &str) -> Vec<LegendEntry> {
        let table = BlockTable::corpus_path();
        let document = Document::parse("subsystemlegend.ini", text, &table).expect("parses");
        read_legend(&document)
    }

    fn paths(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|p| (*p).to_owned()).collect()
    }

    #[test]
    fn init_files_are_read_in_source_order() {
        let entries = legend(
            "LoadSubsystem TheWeaponStore\n  Loader = INI\n  \
             InitFile = Data\\INI\\Weapon.ini\n  \
             InitFile = Data\\INI\\Object\\Cinematic\\CinematicWeapon.ini\nEnd\n",
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].init_files,
            vec![
                "data/ini/weapon.ini",
                "data/ini/object/cinematic/cinematicweapon.ini"
            ]
        );
    }

    #[test]
    fn retail_order_follows_legend_order_not_alphabetical() {
        let entries = legend(
            "LoadSubsystem TheScienceStore\n  InitFile = Data\\INI\\Science.ini\nEnd\n\
             LoadSubsystem TheArmorStore\n  InitFile = Data\\INI\\Armor.ini\nEnd\n",
        );
        let available = paths(&["data/ini/armor.ini", "data/ini/science.ini"]);
        assert_eq!(
            retail_load_order(&entries, &available),
            vec!["data/ini/science.ini", "data/ini/armor.ini"],
            "Science is declared first in the legend, even though Armor sorts first"
        );
    }

    #[test]
    fn init_path_recurses_ini_only_and_honours_exclude_path() {
        let entries = legend(
            "LoadSubsystem TheThingFactory\n  \
             InitFile = Data\\INI\\Default\\Object.ini\n  \
             InitPath = Data\\INI\\Object\n  \
             ExcludePath = Data\\INI\\Object\\Cinematic\\CinematicWeapon.ini\nEnd\n",
        );
        let available = paths(&[
            "data/ini/default/object.ini",
            "data/ini/object/rock.ini",
            "data/ini/object/goodfaction/gondor.ini",
            "data/ini/object/cinematic/cinematicweapon.ini",
            "data/ini/object/includes/stub.inc",
        ]);
        let order = retail_load_order(&entries, &available);
        assert_eq!(
            order,
            vec![
                "data/ini/default/object.ini",
                "data/ini/object/rock.ini",
                "data/ini/object/goodfaction/gondor.ini",
            ],
            "the excluded file and the .inc fragment are both absent, and the \
             direct child precedes the nested one"
        );
    }

    #[test]
    fn a_path_no_entry_reaches_is_not_returned() {
        let entries =
            legend("LoadSubsystem TheArmorStore\n  InitFile = Data\\INI\\Armor.ini\nEnd\n");
        let available = paths(&["data/ini/armor.ini", "data/ini/commandbutton.ini"]);
        assert_eq!(
            retail_load_order(&entries, &available),
            vec!["data/ini/armor.ini"]
        );
    }

    #[test]
    fn a_path_repeated_across_entries_is_emitted_once() {
        let entries = legend(
            "LoadSubsystem A\n  InitFile = Data\\INI\\Shared.ini\nEnd\n\
             LoadSubsystem B\n  InitFile = Data\\INI\\Shared.ini\nEnd\n",
        );
        let available = paths(&["data/ini/shared.ini"]);
        assert_eq!(
            retail_load_order(&entries, &available),
            vec!["data/ini/shared.ini"]
        );
    }
}
