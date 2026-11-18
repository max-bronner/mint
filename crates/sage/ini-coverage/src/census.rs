use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use mint_sage_ini::coverage::Access;
use mint_sage_ini::{table, Document};

use crate::applied;

const OBJECT_KEYWORDS: [&str; 3] = ["object", "childobject", "objectreskin"];

struct KnownType {
    title: &'static str,
    rel_path: &'static str,
    struct_name: &'static str,
    fn_name: &'static str,
}

const KNOWN_APPLIED_TYPES: &[KnownType] = &[
    KnownType {
        title: "Locomotor",
        rel_path: "crates/sage/ini/src/locomotor.rs",
        struct_name: "LocomotorDef",
        fn_name: "read_locomotor",
    },
    KnownType {
        title: "Weapon",
        rel_path: "crates/sage/ini/src/weapon.rs",
        struct_name: "WeaponDef",
        fn_name: "read_weapon",
    },
    KnownType {
        title: "SpecialPower",
        rel_path: "crates/sage/ini/src/special_power.rs",
        struct_name: "SpecialPowerDef",
        fn_name: "read_special_power",
    },
    KnownType {
        title: "Object",
        rel_path: "crates/sage/ini/src/object.rs",
        struct_name: "ObjectFields",
        fn_name: "read_object_fields",
    },
];

#[derive(Clone)]
struct Tally {
    display: String,
    uses: usize,
}

fn bump(map: &mut BTreeMap<String, Tally>, keyword: &str) {
    let key = keyword.to_ascii_lowercase();
    let entry = map.entry(key).or_insert_with(|| Tally {
        display: keyword.to_owned(),
        uses: 0,
    });
    entry.uses = entry.uses.saturating_add(1);
}

fn object_module_slot_keywords() -> Vec<&'static str> {
    table::OBJECT_MODEL_RULES
        .iter()
        .filter(|(context, _, opens)| *context == "object" && *opens)
        .map(|(_, keyword, _)| *keyword)
        .collect()
}

pub struct Census {
    root_in_data: BTreeMap<String, (String, BTreeMap<String, Tally>)>,
    module_in_data: BTreeMap<(String, String), (String, String, usize, BTreeMap<String, Tally>)>,
    root_parsed: BTreeMap<String, BTreeSet<String>>,
    module_parsed: BTreeMap<(String, String), BTreeSet<String>>,
}

impl Census {
    #[must_use]
    pub fn build(documents: &[Document], accesses: &[Access]) -> Self {
        let mut root_in_data: BTreeMap<String, (String, BTreeMap<String, Tally>)> = BTreeMap::new();
        let mut module_in_data: BTreeMap<
            (String, String),
            (String, String, usize, BTreeMap<String, Tally>),
        > = BTreeMap::new();
        let slot_keywords = object_module_slot_keywords();

        for doc in documents {
            for root in doc.roots() {
                let key = root.keyword.to_ascii_lowercase();
                let (_, fields) = root_in_data
                    .entry(key.clone())
                    .or_insert_with(|| (root.keyword.clone(), BTreeMap::new()));
                for field in root.fields() {
                    bump(fields, &field.keyword);
                }

                if !OBJECT_KEYWORDS.contains(&key.as_str()) {
                    continue;
                }
                for slot_kw in &slot_keywords {
                    for slot in root.blocks_named(slot_kw) {
                        let selector = slot.selector().unwrap_or("(none)");
                        let mkey = (slot_kw.to_ascii_lowercase(), selector.to_ascii_lowercase());
                        let entry = module_in_data.entry(mkey).or_insert_with(|| {
                            (
                                (*slot_kw).to_owned(),
                                selector.to_owned(),
                                0,
                                BTreeMap::new(),
                            )
                        });
                        entry.2 = entry.2.saturating_add(1);
                        for field in slot.fields() {
                            bump(&mut entry.3, &field.keyword);
                        }
                    }
                }
            }
        }

        let slot_keyword_set: BTreeSet<String> = slot_keywords
            .iter()
            .map(|k| k.to_ascii_lowercase())
            .collect();
        let mut root_parsed: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut module_parsed: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
        for access in accesses {
            let kw = access.block_keyword.to_ascii_lowercase();
            let field = access.field.to_ascii_lowercase();
            root_parsed
                .entry(kw.clone())
                .or_default()
                .insert(field.clone());
            if slot_keyword_set.contains(&kw) {
                let selector = access
                    .selector
                    .as_deref()
                    .unwrap_or("(none)")
                    .to_ascii_lowercase();
                module_parsed
                    .entry((kw, selector))
                    .or_default()
                    .insert(field);
            }
        }

        Self {
            root_in_data,
            module_in_data,
            root_parsed,
            module_parsed,
        }
    }

    #[must_use]
    pub fn render(
        &self,
        repo_root: &Path,
        is_registered: &dyn Fn(&str) -> bool,
        meta: &Meta,
    ) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "# INI usage census");
        let _ = writeln!(out);
        let _ = writeln!(out, "- date: {}", meta.date);
        let _ = writeln!(out, "- commit: {}", meta.commit);
        let _ = writeln!(out, "- command: `{}`", meta.command);
        let _ = writeln!(out, "- wall time: {}", meta.wall_time);
        let _ = writeln!(out, "- documents: {}", meta.documents);
        let _ = writeln!(out);

        let named = source_literal_index(repo_root);

        let _ = writeln!(out, "## Root blocks");
        let _ = writeln!(out);
        let _ = writeln!(out, "| block | fields in data | parsed | applied |");
        let _ = writeln!(out, "|---|---:|---:|---|");
        for (key, (display, fields)) in &self.root_in_data {
            let empty = BTreeSet::new();
            let parsed_set = self.root_parsed.get(key).unwrap_or(&empty);
            let parsed = fields.keys().filter(|f| parsed_set.contains(*f)).count();
            let applied_word =
                applied_summary(KNOWN_APPLIED_TYPES, display, repo_root, fields, parsed_set);
            let _ = writeln!(
                out,
                "| {display} | {} | {parsed} | {applied_word} |",
                fields.len()
            );
        }
        let _ = writeln!(out);

        let _ = writeln!(out, "## Object module slots");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "| slot | type | declarations | registered in mint | fields in data | parsed |"
        );
        let _ = writeln!(out, "|---|---|---:|---|---:|---:|");
        for ((slot_key, selector_key), (slot, selector, declarations, fields)) in
            &self.module_in_data
        {
            let empty = BTreeSet::new();
            let parsed_set = self
                .module_parsed
                .get(&(slot_key.clone(), selector_key.clone()))
                .unwrap_or(&empty);
            let parsed = fields.keys().filter(|f| parsed_set.contains(*f)).count();
            let registered_word = if slot_key == "behavior" || slot_key == "body" {
                if is_registered(selector) || named.contains(selector.as_str()) {
                    "yes"
                } else {
                    "no"
                }
            } else {
                "n/a (presentation slot)"
            };
            let _ = writeln!(
                out,
                "| {slot} | {selector} | {declarations} | {registered_word} | {} | {parsed} |",
                fields.len()
            );
        }
        let _ = writeln!(out);

        self.render_unparsed(&mut out, &named);
        self.render_parsed_not_applied(&mut out, repo_root);
        out
    }

    fn render_unparsed(&self, out: &mut String, named: &BTreeSet<String>) {
        let mut certainly_unread: Vec<(usize, String, String)> = Vec::new();
        let mut named_not_parsed: Vec<(usize, String, String)> = Vec::new();
        let mut visit = |uses: usize, context: String, display: String| {
            if named.contains(&display) {
                named_not_parsed.push((uses, context, display));
            } else {
                certainly_unread.push((uses, context, display));
            }
        };
        for (key, (display, fields)) in &self.root_in_data {
            let empty = BTreeSet::new();
            let parsed_set = self.root_parsed.get(key).unwrap_or(&empty);
            for (field_key, tally) in fields {
                if !parsed_set.contains(field_key) {
                    visit(tally.uses, display.clone(), tally.display.clone());
                }
            }
        }
        for ((slot_key, selector_key), (slot, selector, _, fields)) in &self.module_in_data {
            let empty = BTreeSet::new();
            let parsed_set = self
                .module_parsed
                .get(&(slot_key.clone(), selector_key.clone()))
                .unwrap_or(&empty);
            for (field_key, tally) in fields {
                if !parsed_set.contains(field_key) {
                    visit(
                        tally.uses,
                        format!("{slot} = {selector}"),
                        tally.display.clone(),
                    );
                }
            }
        }
        let rank = |rows: &mut Vec<(usize, String, String)>| {
            rows.sort_by(|a, b| {
                b.0.cmp(&a.0)
                    .then_with(|| a.1.cmp(&b.1))
                    .then_with(|| a.2.cmp(&b.2))
            });
        };
        rank(&mut certainly_unread);
        rank(&mut named_not_parsed);

        let _ = writeln!(
            out,
            "## Certainly unread: neither asked for nor named anywhere, ranked by occurrences"
        );
        let _ = writeln!(out);
        let _ = writeln!(out, "| occurrences | block/slot | field |");
        let _ = writeln!(out, "|---:|---|---|");
        for (uses, context, field) in certainly_unread.iter().take(200) {
            let _ = writeln!(out, "| {uses} | {context} | {field} |");
        }
        let _ = writeln!(out);

        let _ = writeln!(
            out,
            "## Named but never asked for, ranked by occurrences (an iterating reader or a dead literal)"
        );
        let _ = writeln!(out);
        let _ = writeln!(out, "| occurrences | block/slot | field |");
        let _ = writeln!(out, "|---:|---|---|");
        for (uses, context, field) in named_not_parsed.iter().take(200) {
            let _ = writeln!(out, "| {uses} | {context} | {field} |");
        }
        let _ = writeln!(out);
    }

    fn render_parsed_not_applied(&self, out: &mut String, repo_root: &Path) {
        let mut rows: Vec<(usize, String, String)> = Vec::new();
        for info in KNOWN_APPLIED_TYPES {
            let key = info.title.to_ascii_lowercase();
            let Some((display, fields)) = self.root_in_data.get(&key) else {
                continue;
            };
            let empty = BTreeSet::new();
            let parsed_set = self.root_parsed.get(&key).unwrap_or(&empty);
            let field_map =
                applied::field_map(repo_root, info.rel_path, info.struct_name, info.fn_name);
            let reverse: BTreeMap<String, String> = field_map
                .iter()
                .map(|(struct_field, ini_field)| {
                    (ini_field.to_ascii_lowercase(), struct_field.clone())
                })
                .collect();
            for (field_key, tally) in fields {
                if !parsed_set.contains(field_key) {
                    continue;
                }
                let Some(struct_field) = reverse.get(field_key) else {
                    continue;
                };
                if !applied::is_applied(repo_root, struct_field, info.rel_path) {
                    rows.push((tally.uses, display.clone(), tally.display.clone()));
                }
            }
        }
        rows.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        let _ = writeln!(
            out,
            "## Parsed but unapplied fields, ranked by occurrences (four known types only)"
        );
        let _ = writeln!(out);
        let _ = writeln!(out, "| occurrences | block | field |");
        let _ = writeln!(out, "|---:|---|---|");
        for (uses, context, field) in rows.iter().take(200) {
            let _ = writeln!(out, "| {uses} | {context} | {field} |");
        }
    }
}

pub(crate) fn source_literal_index(repo_root: &Path) -> BTreeSet<String> {
    const EXCLUDED_REL_PATH: &str = "crates/sage/ini/src/tables/generated.rs";
    let mut literals = BTreeSet::new();
    let mut stack = vec![repo_root.join("crates")];
    while let Some(dir) = stack.pop() {
        let Ok(listing) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in listing.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if path.file_name().is_some_and(|n| n == "tests") {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if !kind.is_file() || path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            if path.file_name().is_some_and(|n| n == "tests.rs") {
                continue;
            }
            let Some(rel) = path.strip_prefix(repo_root).ok() else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if !rel.contains("/src/") || rel == EXCLUDED_REL_PATH {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            collect_quoted_identifiers(&text, &mut literals);
        }
    }
    literals
}

fn collect_quoted_identifiers(text: &str, out: &mut BTreeSet<String>) {
    for line in text.lines() {
        let mut rest = line;
        while let Some(start) = rest.find('"') {
            let Some(after) = rest.get(start.saturating_add(1)..) else {
                break;
            };
            let Some(end) = after.find('"') else {
                break;
            };
            if let Some(candidate) = after.get(..end) {
                let valid = !candidate.is_empty()
                    && candidate
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphabetic() || c == '_')
                    && candidate.chars().all(|c| c.is_alphanumeric() || c == '_');
                if valid {
                    out.insert(candidate.to_owned());
                }
            }
            let Some(next) = after.get(end.saturating_add(1)..) else {
                break;
            };
            rest = next;
        }
    }
}

fn applied_summary(
    types: &[KnownType],
    display: &str,
    repo_root: &Path,
    fields: &BTreeMap<String, Tally>,
    parsed_set: &BTreeSet<String>,
) -> &'static str {
    let Some(info) = types.iter().find(|t| t.title.eq_ignore_ascii_case(display)) else {
        return "-";
    };
    let field_map = applied::field_map(repo_root, info.rel_path, info.struct_name, info.fn_name);
    let reverse: BTreeMap<String, String> = field_map
        .iter()
        .map(|(struct_field, ini_field)| (ini_field.to_ascii_lowercase(), struct_field.clone()))
        .collect();
    let any_applied = fields
        .keys()
        .filter(|f| parsed_set.contains(*f))
        .any(|field_key| {
            reverse.get(field_key).is_some_and(|struct_field| {
                applied::is_applied(repo_root, struct_field, info.rel_path)
            })
        });
    if any_applied {
        "some fields (see ranked lists)"
    } else {
        "-"
    }
}

pub struct Meta {
    pub date: String,
    pub commit: String,
    pub command: String,
    pub wall_time: String,
    pub documents: usize,
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use mint_sage_ini::{coverage, BlockTable, Document};

    use super::{Census, Meta};

    #[test]
    fn a_root_blocks_field_count_and_parsed_count_match_what_was_asked_for() {
        let text = "MultiplayerColor TestColor\n\
             MadeUpFieldAlpha = 1\n\
             MadeUpFieldBeta = 2\n\
             MadeUpFieldGamma = 3\n\
             End\n";
        let doc = Document::parse("test.ini", text, &BlockTable::corpus_path())
            .expect("synthetic document parses");

        let _ = coverage::take();
        let root = &doc.roots()[0];
        let _ = root.field("MadeUpFieldAlpha");
        let _ = root.field("MadeUpFieldBeta");
        let accesses = coverage::take();

        let census = Census::build(&[doc], &accesses);
        let report = census.render(
            Path::new("/no-such-workspace-root-for-this-test"),
            &|_name| false,
            &Meta {
                date: "test".to_owned(),
                commit: "test".to_owned(),
                command: "test".to_owned(),
                wall_time: "0s".to_owned(),
                documents: 1,
            },
        );

        assert!(
            report.contains("| MultiplayerColor | 3 | 2 | - |"),
            "expected a 3 fields / 2 parsed row for MultiplayerColor, got:\n{report}"
        );
    }

    #[test]
    fn source_literal_index_finds_a_field_name_only_under_src_outside_generated_rs() {
        let root =
            std::env::temp_dir().join(format!("mint-ini-census-test-{}", std::process::id()));
        let src_dir = root.join("crates/testcrate/src");
        let tests_dir = root.join("crates/testcrate/src/tests");
        let ini_tables_dir = root.join("crates/sage/ini/src/tables");
        std::fs::create_dir_all(&src_dir).expect("create src dir");
        std::fs::create_dir_all(&tests_dir).expect("create tests dir");
        std::fs::create_dir_all(&ini_tables_dir).expect("create tables dir");
        std::fs::write(
            src_dir.join("reader.rs"),
            "fn f() { let _ = keyword.eq_ignore_ascii_case(\"NamedUnderSrc\"); }\n",
        )
        .expect("write reader.rs");
        let examples_dir = root.join("crates/testcrate/examples");
        std::fs::create_dir_all(&examples_dir).expect("create examples dir");
        std::fs::write(examples_dir.join("not_src.rs"), "// \"NamedOutsideSrc\"\n")
            .expect("write not_src.rs");
        std::fs::write(tests_dir.join("tests.rs"), "// \"NamedOnlyInTests\"\n")
            .expect("write tests.rs");
        std::fs::write(
            ini_tables_dir.join("generated.rs"),
            "// \"NamedOnlyInGenerated\"\n",
        )
        .expect("write generated.rs");

        let found = super::source_literal_index(&root);

        std::fs::remove_dir_all(&root).ok();

        assert!(found.contains("NamedUnderSrc"));
        assert!(!found.contains("NamedOutsideSrc"));
        assert!(!found.contains("NamedOnlyInTests"));
        assert!(!found.contains("NamedOnlyInGenerated"));
    }

    #[test]
    fn a_module_named_as_a_literal_outside_the_registry_counts_as_registered() {
        let root =
            std::env::temp_dir().join(format!("mint-ini-census-test2-{}", std::process::id()));
        let src_dir = root.join("crates/testcrate/src");
        std::fs::create_dir_all(&src_dir).expect("create src dir");
        std::fs::write(
            src_dir.join("reader.rs"),
            "fn f() { module(&modules, \"ZzzNameOnlyModule\"); }\n",
        )
        .expect("write reader.rs");

        let table = BlockTable::object_model_path();
        let doc = Document::parse(
            "t.ini",
            "Object T\n  Behavior = ZzzNameOnlyModule Tag_01\n    Foo = 1\n  End\nEnd\n",
            &table,
        )
        .expect("synthetic document parses");
        let _ = coverage::take();
        let census = Census::build(&[doc], &[]);
        let report = census.render(
            &root,
            &|_name| false,
            &Meta {
                date: "test".to_owned(),
                commit: "test".to_owned(),
                command: "test".to_owned(),
                wall_time: "0s".to_owned(),
                documents: 1,
            },
        );

        std::fs::remove_dir_all(&root).ok();

        assert!(
            report.contains("| Behavior | ZzzNameOnlyModule | 1 | yes | 1 | 0 |"),
            "expected ZzzNameOnlyModule registered via the literal scan, got:\n{report}"
        );
    }
}
