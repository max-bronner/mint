use crate::{crate_dirs, label, manifest_deps, package_name};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

pub(crate) fn run(root: &Path, args: &[String]) -> bool {
    let start = Instant::now();
    let base = match base_ref(root, args) {
        Some(b) => b,
        None => {
            eprintln!(
                "FAILED: could not resolve a base ref; pass --base <ref> \
                 (default is the merge base with combat/horde-fixes)"
            );
            return false;
        }
    };
    println!("\n=== quick (base {base}) ===");

    let crates: Vec<PathBuf> = crate_dirs(root);
    let mut by_label: Vec<(String, String)> = Vec::new();
    let mut deps_of: Vec<(String, Vec<String>)> = Vec::new();
    for dir in &crates {
        let text = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap_or_default();
        let Some(pkg) = package_name(&text) else {
            continue;
        };
        by_label.push((label(root, dir), pkg.clone()));
        deps_of.push((pkg, manifest_deps(&text)));
    }

    let changed_files = changed_files(root, &base);
    let mut changed_pkgs: BTreeSet<String> = BTreeSet::new();
    for file in &changed_files {
        if let Some(pkg) = pkg_for_file(root, &crates, &by_label, file) {
            changed_pkgs.insert(pkg);
        }
    }

    if changed_pkgs.is_empty() {
        println!("no crate changes against {base}");
        println!("quick: PASS ({:.1}s)", start.elapsed().as_secs_f64());
        return true;
    }

    let mut test_pkgs = changed_pkgs.clone();
    for (pkg, deps) in &deps_of {
        if deps.iter().any(|d| changed_pkgs.contains(d)) {
            test_pkgs.insert(pkg.clone());
        }
    }

    println!(
        "changed: {}",
        changed_pkgs.iter().cloned().collect::<Vec<_>>().join(", ")
    );
    println!(
        "tested (changed + direct dependents): {}",
        test_pkgs.iter().cloned().collect::<Vec<_>>().join(", ")
    );

    let mut ok = true;
    ok &= cargo_for_pkgs(root, "rustfmt", "fmt", &["--check"], &changed_pkgs);
    ok &= cargo_for_pkgs(
        root,
        "clippy",
        "clippy",
        &["--all-targets", "--", "-D", "warnings"],
        &changed_pkgs,
    );
    ok &= cargo_for_pkgs(root, "tests", "test", &["--no-fail-fast"], &test_pkgs);

    let elapsed = start.elapsed().as_secs_f64();
    println!(
        "\nquick: {} ({elapsed:.1}s)",
        if ok { "PASS" } else { "FAIL" }
    );
    ok
}

fn cargo_for_pkgs(
    root: &Path,
    label: &str,
    subcmd: &str,
    trailing: &[&str],
    pkgs: &BTreeSet<String>,
) -> bool {
    if pkgs.is_empty() {
        return true;
    }
    println!("\n--- {label} ---");
    let mut args: Vec<String> = vec![subcmd.to_string()];
    for p in pkgs {
        args.push("-p".to_string());
        args.push(p.clone());
    }
    args.extend(trailing.iter().map(|s| s.to_string()));
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    match Command::new("cargo").args(&refs).current_dir(root).status() {
        Ok(s) if s.success() => true,
        Ok(s) => {
            eprintln!("FAILED: {label} (exit {:?})", s.code());
            false
        }
        Err(e) => {
            eprintln!("FAILED: {label} could not run ({e})");
            false
        }
    }
}

fn base_ref(root: &Path, args: &[String]) -> Option<String> {
    if let Some(explicit) = base_arg(args) {
        return Some(explicit);
    }
    let out = Command::new("git")
        .args(["merge-base", "HEAD", "combat/horde-fixes"])
        .current_dir(root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn base_arg(args: &[String]) -> Option<String> {
    for (i, a) in args.iter().enumerate() {
        if a == "--base" {
            return args.get(i.saturating_add(1)).cloned();
        }
    }
    None
}

fn changed_files(root: &Path, base: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(o) = Command::new("git")
        .args(["diff", "--name-only", base])
        .current_dir(root)
        .output()
    {
        if o.status.success() {
            out.extend(
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .map(str::to_string),
            );
        }
    }
    if let Ok(o) = Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(root)
        .output()
    {
        if o.status.success() {
            out.extend(
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .map(str::to_string),
            );
        }
    }
    out.sort();
    out.dedup();
    out
}

fn pkg_for_file(
    root: &Path,
    crates: &[PathBuf],
    by_label: &[(String, String)],
    file: &str,
) -> Option<String> {
    let file = file.replace('\\', "/");
    let mut best: Option<(usize, String)> = None;
    for dir in crates {
        let lbl = label(root, dir);
        let prefix = if lbl == "xtask" {
            "xtask/".to_string()
        } else {
            format!("crates/{lbl}/")
        };
        if file.starts_with(&prefix) && prefix.len() > best.as_ref().map_or(0, |(l, _)| *l) {
            best = Some((prefix.len(), lbl));
        }
    }
    let (_, lbl) = best?;
    by_label
        .iter()
        .find(|(l, _)| *l == lbl)
        .map(|(_, pkg)| pkg.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_arg_reads_the_flag_value() {
        let args = vec!["--base".to_string(), "abc123".to_string()];
        assert_eq!(base_arg(&args), Some("abc123".to_string()));
        assert_eq!(base_arg(&[]), None);
    }

    #[test]
    fn pkg_for_file_picks_the_longest_matching_crate_prefix() {
        let root = Path::new("/repo");
        let crates = vec![
            root.join("crates").join("sage").join("big"),
            root.join("crates").join("sage").join("big-tools"),
            root.join("xtask"),
        ];
        let by_label = vec![
            ("sage/big".to_string(), "mint-sage-big".to_string()),
            (
                "sage/big-tools".to_string(),
                "mint-sage-big-tools".to_string(),
            ),
            ("xtask".to_string(), "xtask".to_string()),
        ];
        assert_eq!(
            pkg_for_file(root, &crates, &by_label, "crates/sage/big/src/lib.rs"),
            Some("mint-sage-big".to_string())
        );
        assert_eq!(
            pkg_for_file(root, &crates, &by_label, "crates/sage/big-tools/src/lib.rs"),
            Some("mint-sage-big-tools".to_string())
        );
        assert_eq!(
            pkg_for_file(root, &crates, &by_label, "xtask/src/main.rs"),
            Some("xtask".to_string())
        );
        assert_eq!(
            pkg_for_file(root, &crates, &by_label, "docs/README.md"),
            None
        );
    }
}
