use crate::{crate_dirs, dir_tier, label};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

const SINGLE_FILE_LINE_BUDGET: usize = 1500;

struct Item {
    line: usize,
    kind: &'static str,
    name: String,
    doc: Option<String>,
}

struct Module {
    rel_path: String,
    doc: Option<String>,
    items: Vec<Item>,
}

struct CrateMap {
    label: String,
    pkg: Option<String>,
    tier: Option<&'static str>,
    layer: Option<String>,
    kind: Option<String>,
    modules: Vec<Module>,
}

enum Generated {
    Single(String),
    Split {
        would_be_lines: usize,
        files: Vec<(String, String)>,
    },
}

fn build_crates(root: &Path) -> Option<Vec<CrateMap>> {
    let mut crates: Vec<CrateMap> = Vec::new();

    for dir in crate_dirs(root) {
        let text = fs::read_to_string(dir.join("Cargo.toml")).unwrap_or_default();
        let tier = dir_tier(root, &dir).map(|t| t.name());
        let modules = scan_crate_src(&dir.join("src"));
        if modules.is_empty() {
            continue;
        }
        crates.push(CrateMap {
            label: label(root, &dir),
            pkg: package_name_of(&text),
            tier,
            layer: mint_meta(&text, "layer"),
            kind: mint_meta(&text, "kind"),
            modules,
        });
    }
    crates.sort_by(|a, b| a.label.cmp(&b.label));

    if crates.is_empty() {
        None
    } else {
        Some(crates)
    }
}

fn render(crates: &[CrateMap]) -> Generated {
    let mut single = String::new();
    write_header(&mut single, false);
    for c in crates {
        write_crate(&mut single, c);
    }
    let line_count = single.lines().count();

    if line_count <= SINGLE_FILE_LINE_BUDGET {
        return Generated::Single(single);
    }

    let mut index = String::new();
    write_header(&mut index, true);
    let mut files = Vec::new();
    for c in crates {
        let file_name = format!("{}.md", c.label.replace('/', "-"));
        let _ = writeln!(
            index,
            "- [{}]({file_name}){}",
            c.label,
            crate_summary_suffix(c)
        );
        let mut per_crate = String::new();
        write_crate(&mut per_crate, c);
        files.push((file_name, per_crate));
    }
    files.push(("README.md".to_string(), index));

    Generated::Split {
        would_be_lines: line_count,
        files,
    }
}

pub(crate) fn run(root: &Path) -> bool {
    println!("\n=== codemap ===");
    let Some(crates) = build_crates(root) else {
        eprintln!("FAILED: no crate sources found under crates/ - codemap would be empty");
        return false;
    };
    let crate_count = crates.len();
    let generated = render(&crates);
    write_generated(root, &generated, crate_count)
}

fn write_generated(root: &Path, generated: &Generated, crate_count: usize) -> bool {
    let codemap_dir = root.join("docs").join("codemap");
    let single_path = root.join("docs").join("CODEMAP.md");

    match generated {
        Generated::Single(content) => {
            if codemap_dir.is_dir() {
                if let Err(e) = fs::remove_dir_all(&codemap_dir) {
                    eprintln!("FAILED: could not remove stale docs/codemap/: {e}");
                    return false;
                }
            }
            let line_count = content.lines().count();
            if let Err(e) = fs::write(&single_path, content) {
                eprintln!("FAILED: could not write docs/CODEMAP.md: {e}");
                return false;
            }
            println!("wrote docs/CODEMAP.md: {crate_count} crates, {line_count} lines");
            true
        }
        Generated::Split {
            would_be_lines,
            files,
        } => {
            if single_path.is_file() {
                if let Err(e) = fs::remove_file(&single_path) {
                    eprintln!("FAILED: could not remove stale docs/CODEMAP.md: {e}");
                    return false;
                }
            }
            if let Err(e) = fs::create_dir_all(&codemap_dir) {
                eprintln!("FAILED: could not create docs/codemap/: {e}");
                return false;
            }
            let existing = fs::read_dir(&codemap_dir)
                .map(|rd| rd.flatten().map(|e| e.path()).collect::<Vec<_>>())
                .unwrap_or_default();
            for p in existing {
                let _ = fs::remove_file(p);
            }
            for (name, content) in files {
                if let Err(e) = fs::write(codemap_dir.join(name), content) {
                    eprintln!("FAILED: could not write docs/codemap/{name}: {e}");
                    return false;
                }
            }
            println!(
                "wrote docs/codemap/ ({crate_count} crates, index + per-crate files, \
                 {would_be_lines} lines total would have exceeded the \
                 {SINGLE_FILE_LINE_BUDGET}-line single-file budget)",
            );
            true
        }
    }
}

pub(crate) fn check(root: &Path) -> bool {
    println!("\n=== codemap freshness ===");
    let Some(crates) = build_crates(root) else {
        eprintln!("FAILED: no crate sources found under crates/ - codemap would be empty");
        return false;
    };
    let generated = render(&crates);
    compare_generated(root, &generated)
}

fn compare_generated(root: &Path, generated: &Generated) -> bool {
    let codemap_dir = root.join("docs").join("codemap");
    let single_path = root.join("docs").join("CODEMAP.md");
    let mut stale: Vec<String> = Vec::new();

    match generated {
        Generated::Single(content) => {
            if codemap_dir.is_dir() {
                stale.push(
                    "docs/codemap/ still exists; the codemap now fits docs/CODEMAP.md alone"
                        .to_string(),
                );
            }
            match fs::read_to_string(&single_path) {
                Ok(on_disk) if on_disk == *content => {}
                Ok(_) => stale.push("docs/CODEMAP.md".to_string()),
                Err(_) => stale.push("docs/CODEMAP.md (missing)".to_string()),
            }
        }
        Generated::Split { files, .. } => {
            if single_path.is_file() {
                stale.push(
                    "docs/CODEMAP.md still exists; the codemap now needs docs/codemap/".to_string(),
                );
            }
            for (name, content) in files {
                match fs::read_to_string(codemap_dir.join(name)) {
                    Ok(on_disk) if on_disk == *content => {}
                    Ok(_) => stale.push(format!("docs/codemap/{name}")),
                    Err(_) => stale.push(format!("docs/codemap/{name} (missing)")),
                }
            }
            let expected: std::collections::BTreeSet<&str> =
                files.iter().map(|(n, _)| n.as_str()).collect();
            let mut extra: Vec<String> = fs::read_dir(&codemap_dir)
                .map(|rd| {
                    rd.flatten()
                        .filter_map(|e| e.file_name().into_string().ok())
                        .filter(|n| !expected.contains(n.as_str()))
                        .map(|n| format!("docs/codemap/{n} (should not exist)"))
                        .collect()
                })
                .unwrap_or_default();
            extra.sort();
            stale.append(&mut extra);
        }
    }

    if stale.is_empty() {
        println!("codemap is up to date");
        true
    } else {
        eprintln!("FAILED: docs/codemap is stale - differs from `cargo xtask codemap`:");
        for s in &stale {
            eprintln!("  {s}");
        }
        eprintln!("run `cargo xtask codemap`");
        false
    }
}

fn write_header(out: &mut String, is_index: bool) {
    let _ = writeln!(
        out,
        "<!-- generated by `cargo xtask codemap`; do not edit by hand -->"
    );
    if is_index {
        let _ = writeln!(out, "# Codemap\n");
        let _ = writeln!(
            out,
            "One file per crate, listing each module's first `//!` line and \
             every `pub`/`pub(crate)` fn, struct, enum and trait with its \
             first doc line and `file:line`. Regenerate with `cargo xtask codemap`.\n"
        );
    }
}

fn crate_summary_suffix(c: &CrateMap) -> String {
    match (&c.tier, &c.layer) {
        (Some(t), Some(l)) => format!(" - {t} tier, layer {l}"),
        (Some(t), None) => format!(" - {t} tier"),
        _ => String::new(),
    }
}

fn write_crate(out: &mut String, c: &CrateMap) {
    let pkg = c.pkg.as_deref().unwrap_or("?");
    let _ = writeln!(out, "## {} ({pkg})", c.label);
    let mut meta = Vec::new();
    if let Some(t) = c.tier {
        meta.push(format!("tier: {t}"));
    }
    if let Some(l) = &c.layer {
        meta.push(format!("layer: {l}"));
    }
    if let Some(k) = &c.kind {
        meta.push(format!("kind: {k}"));
    }
    if !meta.is_empty() {
        let _ = writeln!(out, "{}", meta.join(", "));
    }
    out.push('\n');

    for m in &c.modules {
        let doc = m.doc.as_deref().unwrap_or("");
        let _ = writeln!(out, "### {} - {doc}", m.rel_path);
        for it in &m.items {
            let doc = it.doc.as_deref().unwrap_or("");
            let _ = writeln!(
                out,
                "- {} `{}` - {doc} ({}:{})",
                it.kind, it.name, m.rel_path, it.line
            );
        }
        out.push('\n');
    }
}

fn scan_crate_src(src_dir: &Path) -> Vec<Module> {
    let mut out = Vec::new();
    walk_rs(src_dir, src_dir, &mut out);
    out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    out
}

fn walk_rs(root: &Path, dir: &Path, out: &mut Vec<Module>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk_rs(root, &p, out);
        } else if p.extension().is_some_and(|x| x == "rs")
            && p.file_stem().and_then(|s| s.to_str()) != Some("tests")
        {
            let Ok(text) = fs::read_to_string(&p) else {
                continue;
            };
            let rel = p
                .strip_prefix(root.parent().unwrap_or(root))
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            out.push(Module {
                rel_path: rel,
                doc: module_doc(&text),
                items: pub_items(&text),
            });
        }
    }
}

fn module_doc(text: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let t = l.trim_start();
        t.strip_prefix("//!").map(|rest| rest.trim().to_string())
    })
}

fn pub_items(text: &str) -> Vec<Item> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if t == "#[cfg(test)]"
            && lines.get(i.saturating_add(1)).is_some_and(|n| {
                let n = n.trim();
                n.starts_with("mod tests") && n.ends_with('{')
            })
        {
            break;
        }
        let Some(after_vis) = strip_pub_vis(t) else {
            continue;
        };
        let mut rest = after_vis.trim_start();
        loop {
            if let Some(r) = rest.strip_prefix("async ") {
                rest = r.trim_start();
            } else if let Some(r) = rest.strip_prefix("unsafe ") {
                rest = r.trim_start();
            } else if let Some(r) = rest.strip_prefix("const ") {
                rest = r.trim_start();
            } else {
                break;
            }
        }
        let (kind, after_kw) = if let Some(r) = rest.strip_prefix("fn ") {
            ("fn", r)
        } else if let Some(r) = rest.strip_prefix("struct ") {
            ("struct", r)
        } else if let Some(r) = rest.strip_prefix("enum ") {
            ("enum", r)
        } else if let Some(r) = rest.strip_prefix("trait ") {
            ("trait", r)
        } else {
            continue;
        };
        let name: String = after_kw
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        out.push(Item {
            line: i.saturating_add(1),
            kind,
            name,
            doc: doc_above(&lines, i),
        });
    }
    out
}

fn strip_pub_vis(t: &str) -> Option<&str> {
    t.strip_prefix("pub(crate)")
        .or_else(|| t.strip_prefix("pub "))
}

fn doc_above(lines: &[&str], item_idx: usize) -> Option<String> {
    let mut j = item_idx;
    while j > 0 {
        let Some(prev) = lines.get(j.saturating_sub(1)) else {
            break;
        };
        if prev.trim_start().starts_with("#[") {
            j = j.saturating_sub(1);
        } else {
            break;
        }
    }
    let mut doc_lines: Vec<&str> = Vec::new();
    let mut k = j;
    while k > 0 {
        let Some(prev) = lines.get(k.saturating_sub(1)) else {
            break;
        };
        if let Some(rest) = prev.trim_start().strip_prefix("///") {
            doc_lines.push(rest.trim());
            k = k.saturating_sub(1);
        } else {
            break;
        }
    }
    doc_lines.reverse();
    doc_lines.first().map(|s| s.to_string())
}

fn package_name_of(text: &str) -> Option<String> {
    let mut in_package = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t.starts_with("[package]");
            continue;
        }
        if in_package {
            if let Some((lhs, rhs)) = t.split_once('=') {
                if lhs.trim() == "name" {
                    return Some(rhs.trim().trim_matches('"').to_string());
                }
            }
        }
    }
    None
}

fn mint_meta(text: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_section = t.replace(' ', "") == "[package.metadata.mint]";
            continue;
        }
        if in_section {
            if let Some((lhs, rhs)) = t.split_once('=') {
                if lhs.trim() == key {
                    return Some(rhs.split('#').next().unwrap_or(rhs).trim().to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pub_items_finds_fn_struct_enum_trait_but_not_private() {
        let src = "\
fn private_helper() {}
pub fn visible() {}
pub(crate) struct Inner;
struct PrivateStruct;
pub enum Colour { Red }
pub trait Doer {}
";
        let items = pub_items(src);
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["visible", "Inner", "Colour", "Doer"]);
        assert_eq!(items[0].kind, "fn");
        assert_eq!(items[1].kind, "struct");
    }

    #[test]
    fn doc_above_takes_the_first_line_of_the_block_and_skips_attributes() {
        let src = "\
/// Summary sentence.
/// A second line that is not the summary.
#[derive(Debug)]
#[must_use]
pub fn thing() {}
";
        let lines: Vec<&str> = src.lines().collect();
        let item_idx = lines.iter().position(|l| l.contains("pub fn")).unwrap();
        assert_eq!(
            doc_above(&lines, item_idx),
            Some("Summary sentence.".to_string())
        );
    }

    #[test]
    fn doc_above_is_none_with_no_doc_comment() {
        let src = "let x = 1;\npub fn thing() {}\n";
        let lines: Vec<&str> = src.lines().collect();
        assert_eq!(doc_above(&lines, 1), None);
    }

    #[test]
    fn pub_items_stops_at_the_test_module_so_fixture_strings_are_not_mistaken_for_items() {
        let src = "\
pub fn real_item() {}

#[cfg(test)]
mod tests {
    #[test]
    fn a_test() {
        let src = \"pub fn not_real() {}\";
    }
}
";
        let items = pub_items(src);
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["real_item"]);
    }

    #[test]
    fn a_sibling_test_module_declaration_does_not_stop_the_scan() {
        let src = "\
#[cfg(test)]
mod tests;

pub fn real_item() {}
";
        let items = pub_items(src);
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["real_item"]);
    }

    #[test]
    fn a_cfg_test_on_anything_but_mod_tests_does_not_stop_the_scan() {
        let src = "\
#[cfg(test)]
mod testutil;

pub fn real_item() {}
";
        let items = pub_items(src);
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["real_item"]);
    }

    #[test]
    fn module_doc_reads_the_first_bang_comment_wherever_it_falls() {
        let src = "// ordinary comment\n//! The module summary.\n//! more detail\n";
        assert_eq!(module_doc(src), Some("The module summary.".to_string()));
        assert_eq!(module_doc("fn f() {}\n"), None);
    }

    #[test]
    fn compare_generated_reports_a_changed_item_as_stale() {
        let root = std::env::temp_dir().join(format!(
            "mint-codemap-check-test-{}-{}",
            std::process::id(),
            line!()
        ));
        let codemap_dir = root.join("docs").join("codemap");
        fs::create_dir_all(&codemap_dir).unwrap();
        fs::write(codemap_dir.join("foo.md"), "## foo\n- fn `bar` - old doc\n").unwrap();

        let matching = Generated::Split {
            would_be_lines: 0,
            files: vec![(
                "foo.md".to_string(),
                "## foo\n- fn `bar` - old doc\n".to_string(),
            )],
        };
        assert!(compare_generated(&root, &matching));

        let changed = Generated::Split {
            would_be_lines: 0,
            files: vec![(
                "foo.md".to_string(),
                "## foo\n- fn `bar` - new doc after an edit\n".to_string(),
            )],
        };
        assert!(!compare_generated(&root, &changed));

        let _ = fs::remove_dir_all(&root);
    }
}
