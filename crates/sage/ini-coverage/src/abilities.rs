use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use mint_sage_ini::Document;

use crate::applied;
use crate::census::source_literal_index;
use crate::report::{reverse_map, Run, TYPES};

const OBJECT_KEYWORDS: [&str; 3] = ["Object", "ChildObject", "ObjectReskin"];
const MODULE_SLOT_KEYWORDS: [&str; 2] = ["Behavior", "Body"];

struct ModuleRef {
    template: String,
    slot: &'static str,
    selector: String,
    power: Option<String>,
    fields: BTreeSet<String>,
}

pub struct AbilityIndex {
    powers: BTreeMap<String, String>,
    refs: Vec<ModuleRef>,
}

impl AbilityIndex {
    #[must_use]
    pub fn build(documents: &[Document]) -> Self {
        let mut powers = BTreeMap::new();
        for doc in documents {
            for root in doc.roots_named("SpecialPower") {
                if let Some(name) = root.name() {
                    powers.insert(name.to_ascii_lowercase(), name.to_owned());
                }
            }
        }

        let mut refs = Vec::new();
        for doc in documents {
            for root in doc.roots() {
                if !OBJECT_KEYWORDS
                    .iter()
                    .any(|k| root.keyword.eq_ignore_ascii_case(k))
                {
                    continue;
                }
                let Some(template) = root.name() else {
                    continue;
                };
                for slot_kw in MODULE_SLOT_KEYWORDS {
                    for slot in root.blocks_named(slot_kw) {
                        let Some(selector) = slot.selector() else {
                            continue;
                        };
                        let lower = selector.to_ascii_lowercase();
                        let is_family =
                            lower.contains("specialpower") || lower.contains("specialability");
                        let named_power = slot
                            .field("SpecialPowerTemplate")
                            .and_then(mint_sage_ini::Field::value)
                            .map(str::to_ascii_lowercase)
                            .filter(|v| powers.contains_key(v));
                        if !is_family && named_power.is_none() {
                            continue;
                        }
                        refs.push(ModuleRef {
                            template: template.to_owned(),
                            slot: slot_kw,
                            selector: selector.to_owned(),
                            power: named_power,
                            fields: slot
                                .fields()
                                .map(|f| f.keyword.to_ascii_lowercase())
                                .collect(),
                        });
                    }
                }
            }
        }

        Self { powers, refs }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Handled {
    Yes,
    No,
}

fn module_handled(
    is_registered: &dyn Fn(&str) -> bool,
    named: &BTreeSet<String>,
    selector: &str,
) -> Handled {
    if is_registered(selector) || named.contains(selector) {
        Handled::Yes
    } else {
        Handled::No
    }
}

#[must_use]
pub fn render(
    documents: &[Document],
    run: &Run,
    repo_root: &Path,
    is_registered: &dyn Fn(&str) -> bool,
) -> String {
    let index = AbilityIndex::build(documents);
    let named = source_literal_index(repo_root);

    let mut by_power: BTreeMap<String, Vec<&ModuleRef>> = BTreeMap::new();
    let mut generic: Vec<&ModuleRef> = Vec::new();
    for reference in &index.refs {
        match &reference.power {
            Some(power) => by_power.entry(power.clone()).or_default().push(reference),
            None => generic.push(reference),
        }
    }

    let mut out = String::new();
    let _ = writeln!(out, "# Ability coverage");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "- {} `SpecialPower` definitions, {} module declarations linked to one by name, \
         {} module declarations naming the special-power family with no `SpecialPowerTemplate`",
        index.powers.len(),
        by_power.values().map(Vec::len).sum::<usize>(),
        generic.len()
    );
    let _ = writeln!(out);

    let mut sp_missing: Vec<String> = Vec::new();
    if let Some(special_power_info) = TYPES.iter().find(|t| t.title == "SpecialPower") {
        let field_map = applied::field_map(
            repo_root,
            special_power_info.rel_path,
            special_power_info.struct_name,
            special_power_info.fn_name,
        );
        let reverse = reverse_map(&field_map);
        let sp_in_data = run
            .root_in_data("specialpower")
            .cloned()
            .unwrap_or_default();
        let sp_looked_up = run.root_looked_up("specialpower");
        for field in sp_in_data.keys() {
            if !sp_looked_up.contains(field) {
                continue;
            }
            let Some(struct_field) = reverse.get(field) else {
                continue;
            };
            if !applied::is_applied(repo_root, struct_field, special_power_info.rel_path) {
                sp_missing.push(format!("SpecialPower.{field}"));
            }
        }
    }
    sp_missing.sort();

    let mut fully: Vec<String> = Vec::new();
    let mut partial: Vec<(String, Vec<String>, Vec<String>)> = Vec::new();
    let mut unhandled: Vec<(String, Vec<String>)> = Vec::new();

    let mut power_names: Vec<&String> = index.powers.values().collect();
    power_names.sort();
    for power in &power_names {
        let key = power.to_ascii_lowercase();
        let modules: Vec<&ModuleRef> = by_power.get(&key).into_iter().flatten().copied().collect();

        if modules.is_empty() {
            unhandled.push(((*power).clone(), Vec::new()));
            continue;
        }

        let mut lines: Vec<String> = Vec::new();
        let mut missing_fields: Vec<String> = sp_missing.clone();
        let mut any_unhandled_module = false;
        for module in &modules {
            let handled = module_handled(is_registered, &named, &module.selector) == Handled::Yes;
            if !handled {
                any_unhandled_module = true;
            }
            lines.push(format!(
                "{} ({}.{}, handled: {})",
                module.template,
                module.slot,
                module.selector,
                if handled { "yes" } else { "no" }
            ));
            let parsed = run.module_looked_up(
                &module.slot.to_ascii_lowercase(),
                &module.selector.to_ascii_lowercase(),
            );
            for field in &module.fields {
                if !parsed.contains(field) {
                    missing_fields.push(format!("{}.{field}", module.selector));
                }
            }
        }
        missing_fields.sort();
        missing_fields.dedup();

        if any_unhandled_module {
            unhandled.push(((*power).clone(), lines));
        } else if missing_fields.is_empty() {
            fully.push((*power).clone());
        } else {
            partial.push(((*power).clone(), lines, missing_fields));
        }
    }

    let _ = writeln!(
        out,
        "## Fully handled ({}) / partially handled ({}) / not handled ({})",
        fully.len(),
        partial.len(),
        unhandled.len()
    );
    let _ = writeln!(out);

    let _ = writeln!(out, "### Fully handled");
    let _ = writeln!(out);
    for power in &fully {
        let _ = writeln!(out, "- {power}");
    }
    let _ = writeln!(out);

    let _ = writeln!(
        out,
        "### Partially handled (fields present but not applied)"
    );
    let _ = writeln!(out);
    for (power, lines, fields) in &partial {
        let _ = writeln!(out, "- **{power}**");
        for line in lines {
            let _ = writeln!(out, "  - {line}");
        }
        let _ = writeln!(out, "  - missing: {}", fields.join(", "));
    }
    let _ = writeln!(out);

    let _ = writeln!(
        out,
        "### Not handled (no module linked, or every linked module is unregistered)"
    );
    let _ = writeln!(out);
    for (power, lines) in &unhandled {
        if lines.is_empty() {
            let _ = writeln!(out, "- {power} (no module in the corpus names it)");
        } else {
            let _ = writeln!(out, "- **{power}**");
            for line in lines {
                let _ = writeln!(out, "  - {line}");
            }
        }
    }
    let _ = writeln!(out);

    let _ = writeln!(
        out,
        "## Modules naming the special-power family with no `SpecialPowerTemplate`"
    );
    let _ = writeln!(out);
    let mut generic_selectors: BTreeSet<(String, bool)> = BTreeSet::new();
    for module in &generic {
        let handled = module_handled(is_registered, &named, &module.selector) == Handled::Yes;
        generic_selectors.insert((module.selector.clone(), handled));
    }
    for (selector, handled) in &generic_selectors {
        let _ = writeln!(
            out,
            "- {selector} (handled: {})",
            if *handled { "yes" } else { "no" }
        );
    }

    out
}
