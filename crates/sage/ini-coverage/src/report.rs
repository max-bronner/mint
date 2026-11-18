use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use mint_sage_ini::{
    coverage, read_locomotor, read_object_fields, read_special_power, read_weapon, Document,
    MacroTable,
};
use mint_sage_modules::{register_every_family, spawn_template, TemplateIndex, UpgradeBits};
use mint_sim::module::ModuleRegistry;
use mint_sim::{Frame, World};

use crate::applied;

const OBJECT_KEYWORDS: [&str; 3] = ["Object", "ChildObject", "ObjectReskin"];
const MODULE_SLOT_KEYWORDS: [&str; 2] = ["Behavior", "Body"];

#[derive(Clone)]
pub struct FieldStat {
    pub display: String,
    pub uses: usize,
    pub sample_value: String,
    pub sample_file: String,
}

pub(crate) struct TypeInfo {
    pub(crate) title: &'static str,
    pub(crate) rel_path: &'static str,
    pub(crate) struct_name: &'static str,
    pub(crate) fn_name: &'static str,
}

pub(crate) const TYPES: &[TypeInfo] = &[
    TypeInfo {
        title: "Locomotor",
        rel_path: "crates/sage/ini/src/locomotor.rs",
        struct_name: "LocomotorDef",
        fn_name: "read_locomotor",
    },
    TypeInfo {
        title: "Weapon",
        rel_path: "crates/sage/ini/src/weapon.rs",
        struct_name: "WeaponDef",
        fn_name: "read_weapon",
    },
    TypeInfo {
        title: "SpecialPower",
        rel_path: "crates/sage/ini/src/special_power.rs",
        struct_name: "SpecialPowerDef",
        fn_name: "read_special_power",
    },
    TypeInfo {
        title: "Object",
        rel_path: "crates/sage/ini/src/object.rs",
        struct_name: "ObjectFields",
        fn_name: "read_object_fields",
    },
];

pub struct Run {
    in_data: BTreeMap<String, BTreeMap<String, FieldStat>>,
    module_in_data: BTreeMap<(String, String), BTreeMap<String, FieldStat>>,
    looked_up: BTreeMap<String, BTreeSet<String>>,
    module_looked_up: BTreeMap<(String, String), BTreeSet<String>>,
}

impl Run {
    #[must_use]
    pub fn build(documents: &[Document]) -> Self {
        let mut in_data: BTreeMap<String, BTreeMap<String, FieldStat>> = BTreeMap::new();
        let mut module_in_data: BTreeMap<(String, String), BTreeMap<String, FieldStat>> =
            BTreeMap::new();

        for doc in documents {
            for root in doc.roots() {
                merge_fields(
                    in_data
                        .entry(root.keyword.to_ascii_lowercase())
                        .or_default(),
                    root,
                );
                let is_object = OBJECT_KEYWORDS
                    .iter()
                    .any(|k| root.keyword.eq_ignore_ascii_case(k));
                if !is_object {
                    continue;
                }
                for slot_kw in MODULE_SLOT_KEYWORDS {
                    for slot in root.blocks_named(slot_kw) {
                        let Some(selector) = slot.selector() else {
                            continue;
                        };
                        let key = (slot_kw.to_ascii_lowercase(), selector.to_ascii_lowercase());
                        merge_fields(module_in_data.entry(key).or_default(), slot);
                    }
                }
            }
        }

        let mut macros = MacroTable::new();
        for doc in documents {
            macros.insert_defines(doc.defines());
        }
        for doc in documents {
            for root in doc.roots() {
                if root.keyword.eq_ignore_ascii_case("Locomotor") {
                    let _def = read_locomotor(root, &mint_sage_ini::ValueErrors::new());
                } else if root.keyword.eq_ignore_ascii_case("Weapon") {
                    let _def = read_weapon(root);
                } else if root.keyword.eq_ignore_ascii_case("SpecialPower") {
                    let _def = read_special_power(root, &macros);
                } else if OBJECT_KEYWORDS
                    .iter()
                    .any(|k| root.keyword.eq_ignore_ascii_case(k))
                {
                    let _fields = read_object_fields(root);
                }
            }
        }

        run_module_pipeline(documents);

        let mut looked_up: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut module_looked_up: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
        for access in coverage::take() {
            let kw_lower = access.block_keyword.to_ascii_lowercase();
            let field_lower = access.field.to_ascii_lowercase();
            if MODULE_SLOT_KEYWORDS
                .iter()
                .any(|k| k.eq_ignore_ascii_case(&access.block_keyword))
            {
                if let Some(selector) = &access.selector {
                    module_looked_up
                        .entry((kw_lower.clone(), selector.to_ascii_lowercase()))
                        .or_default()
                        .insert(field_lower.clone());
                }
            }
            looked_up.entry(kw_lower).or_default().insert(field_lower);
        }

        Self {
            in_data,
            module_in_data,
            looked_up,
            module_looked_up,
        }
    }

    #[must_use]
    pub(crate) fn root_in_data(&self, keyword: &str) -> Option<&BTreeMap<String, FieldStat>> {
        self.in_data.get(keyword)
    }

    #[must_use]
    pub(crate) fn root_looked_up(&self, keyword: &str) -> &BTreeSet<String> {
        static EMPTY: BTreeSet<String> = BTreeSet::new();
        self.looked_up.get(keyword).unwrap_or(&EMPTY)
    }

    #[must_use]
    pub(crate) fn module_looked_up(&self, slot: &str, selector: &str) -> &BTreeSet<String> {
        static EMPTY: BTreeSet<String> = BTreeSet::new();
        self.module_looked_up
            .get(&(slot.to_owned(), selector.to_owned()))
            .unwrap_or(&EMPTY)
    }
}

fn merge_fields(dest: &mut BTreeMap<String, FieldStat>, block: &mint_sage_ini::Block) {
    for field in block.fields() {
        let key = field.keyword.to_ascii_lowercase();
        let entry = dest.entry(key).or_insert_with(|| FieldStat {
            display: field.keyword.clone(),
            uses: 0,
            sample_value: field.args.join(" "),
            sample_file: format!("{}:{}", field.file, field.line),
        });
        entry.uses = entry.uses.saturating_add(1);
    }
}

fn run_module_pipeline(documents: &[Document]) {
    let mut index = TemplateIndex::new();
    for doc in documents {
        index.insert_document(doc);
    }
    let names: Vec<String> = index.names().map(str::to_owned).collect();
    let upgrade_bits = UpgradeBits::from_index(index.upgrades());

    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    register_every_family(&mut registry, &mut world);

    for name in &names {
        let _ = spawn_template(
            &mut world,
            &registry,
            Frame::ZERO,
            &index,
            &upgrade_bits,
            name,
        );
    }
}

pub fn summary(run: &Run, repo_root: &Path) {
    println!(
        "\n{:<14} {:>10} {:>8} {:>8}",
        "type", "in-data", "parsed", "applied"
    );
    for info in TYPES {
        let key = info.title.to_ascii_lowercase();
        let empty_map = BTreeMap::new();
        let empty_set = BTreeSet::new();
        let in_data = run.in_data.get(&key).unwrap_or(&empty_map);
        let looked_up = run.looked_up.get(&key).unwrap_or(&empty_set);
        let field_map =
            applied::field_map(repo_root, info.rel_path, info.struct_name, info.fn_name);
        let reverse = reverse_map(&field_map);
        let parsed = in_data.keys().filter(|f| looked_up.contains(*f)).count();
        let applied = in_data
            .keys()
            .filter(|f| looked_up.contains(*f))
            .filter(|f| {
                reverse
                    .get(*f)
                    .is_some_and(|sf| applied::is_applied(repo_root, sf, info.rel_path))
            })
            .count();
        println!(
            "{:<14} {:>10} {:>8} {:>8}",
            info.title,
            in_data.len(),
            parsed,
            applied
        );
    }

    let module_types: BTreeSet<&(String, String)> = run.module_in_data.keys().collect();
    println!(
        "\n{} distinct Object module types seen under Behavior/Body (parsed-only, see Object report for detail)",
        module_types.len()
    );
}

pub fn one(run: &Run, repo_root: &Path, kind: &str) {
    if kind.eq_ignore_ascii_case("Object") {
        object_report(run, repo_root);
        return;
    }
    let Some(info) = TYPES.iter().find(|t| t.title.eq_ignore_ascii_case(kind)) else {
        eprintln!(
            "unsupported block type {kind:?}; supported: Locomotor, Weapon, SpecialPower, Object"
        );
        return;
    };
    let key = info.title.to_ascii_lowercase();
    let empty_map = BTreeMap::new();
    let empty_set = BTreeSet::new();
    let in_data = run.in_data.get(&key).unwrap_or(&empty_map);
    let looked_up = run.looked_up.get(&key).unwrap_or(&empty_set);
    print_type(info.title, repo_root, info, in_data, looked_up);
}

fn object_report(run: &Run, repo_root: &Path) {
    let Some(info) = TYPES.iter().find(|t| t.title == "Object") else {
        return;
    };
    let key = "object".to_owned();
    let empty_map = BTreeMap::new();
    let empty_set = BTreeSet::new();
    let in_data = run.in_data.get(&key).unwrap_or(&empty_map);
    let looked_up = run.looked_up.get(&key).unwrap_or(&empty_set);
    print_type(
        "Object (top-level fields)",
        repo_root,
        info,
        in_data,
        looked_up,
    );

    println!(
        "\n-- Object module blocks (Behavior/Body), per module type - parsed only; \
         applied-detection is not wired up for module types this round, see report --"
    );
    let mut module_types: Vec<&(String, String)> = run.module_in_data.keys().collect();
    module_types.sort();
    for (slot, selector) in module_types {
        let empty_map = BTreeMap::new();
        let empty_set = BTreeSet::new();
        let in_data = run
            .module_in_data
            .get(&(slot.clone(), selector.clone()))
            .unwrap_or(&empty_map);
        let looked_up = run
            .module_looked_up
            .get(&(slot.clone(), selector.clone()))
            .unwrap_or(&empty_set);
        println!(
            "\n== {slot} = {selector} ({} fields in data, {} parsed) ==",
            in_data.len(),
            in_data.keys().filter(|f| looked_up.contains(*f)).count()
        );
        for (field, stat) in in_data {
            let parsed = if looked_up.contains(field) {
                "yes"
            } else {
                "no "
            };
            println!(
                "  {:<28} {:>5}  {}  sample={:?} @ {}",
                stat.display, stat.uses, parsed, stat.sample_value, stat.sample_file
            );
        }
    }
}

fn print_type(
    title: &str,
    repo_root: &Path,
    info: &TypeInfo,
    in_data: &BTreeMap<String, FieldStat>,
    looked_up: &BTreeSet<String>,
) {
    let collisions = applied::collisions(repo_root);
    let field_map = applied::field_map(repo_root, info.rel_path, info.struct_name, info.fn_name);
    let reverse = reverse_map(&field_map);

    println!("\n== {title} ({} fields in data) ==", in_data.len());
    println!(
        "{:<28} {:>6}  {:<30}  parsed  applied  consumers",
        "field", "uses", "sample (file:line)"
    );
    let mut unparsed: Vec<(&String, &FieldStat)> = Vec::new();
    let mut parsed_not_applied: Vec<String> = Vec::new();
    for (field, stat) in in_data {
        let parsed = looked_up.contains(field);
        let (applied_word, consumers) = if !parsed {
            unparsed.push((field, stat));
            ("-", Vec::new())
        } else if let Some(struct_field) = reverse.get(field) {
            let hits = applied::consumers(repo_root, struct_field, info.rel_path);
            if hits.is_empty() {
                parsed_not_applied.push(stat.display.clone());
                if applied::ambiguous(&collisions, struct_field) {
                    ("ambiguous", Vec::new())
                } else {
                    ("no", Vec::new())
                }
            } else if applied::ambiguous(&collisions, struct_field) {
                ("yes (ambig. owner)", hits)
            } else {
                ("yes", hits)
            }
        } else {
            ("unmapped", Vec::new())
        };
        println!(
            "{:<28} {:>6}  {:<30}  {:<6}  {:<8} {}",
            stat.display,
            stat.uses,
            format!("{:?} @ {}", stat.sample_value, stat.sample_file),
            if parsed { "yes" } else { "no" },
            applied_word,
            consumers.join("; "),
        );
    }

    if !unparsed.is_empty() {
        println!("\n-- unparsed: present in data, never looked up --");
        for (field, stat) in &unparsed {
            let _ = field;
            println!("  {:<28} {:>6} uses", stat.display, stat.uses);
        }
    }
    if !parsed_not_applied.is_empty() {
        println!("\n-- parsed but not applied: read by the reader, nothing else touches the struct field --");
        for name in &parsed_not_applied {
            println!("  {name}");
        }
    }

    let unknown: Vec<&String> = looked_up
        .iter()
        .filter(|f| !in_data.contains_key(f.as_str()))
        .collect();
    if !unknown.is_empty() {
        println!("\n-- looked up by the reader but never seen in the corpus (likely misspelling or BFME-absent Generals field) --");
        for field in unknown {
            println!("  {field}");
        }
    }
}

pub(crate) fn reverse_map(field_map: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    field_map
        .iter()
        .map(|(struct_field, ini_field)| (ini_field.to_ascii_lowercase(), struct_field.clone()))
        .collect()
}
