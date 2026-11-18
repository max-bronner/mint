use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

mod codemap;
mod ini_tables;
mod matrix;
mod quick;

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_default();
    let root = match repo_root() {
        Some(r) => r,
        None => {
            eprintln!("error: could not locate workspace root");
            return ExitCode::FAILURE;
        }
    };

    let ok = match task.as_str() {
        "check" => check(&root),
        "codemap" => codemap::run(&root),
        "quick" => quick::run(&root, &std::env::args().skip(2).collect::<Vec<_>>()),
        "deps" => deps(&root),
        "lints" => lints(&root),
        "tiers" => tiers(&root),
        "text-boundary" => text_boundary(&root),
        "stats" => stats(&root),
        "matrix" => matrix::run(&root),
        "ini-coverage" => ini_coverage(&root),
        "ini-tables" => ini_tables::run(&root, &std::env::args().skip(2).collect::<Vec<_>>()),
        _ => {
            eprintln!(
                "usage: cargo xtask <command>\n\
                 \n\
                 check         fmt, clippy, tests, advisories and licence policy; the full gate\n\
                 codemap       regenerate docs/CODEMAP.md (or docs/codemap/*.md): crates, modules, pub items\n\
                 quick         fast per-agent check: fmt+clippy+test on crates changed since a base ref\n\
                 deps          verify every dependency in every manifest is justified\n\
                 lints         verify every crate opts into the workspace lints\n\
                 tiers         verify core <- sage <- games dependency direction\n\
                 text-boundary verify skrifa/read_fonts stay confined inside mint-ui (R6)\n\
                 stats         lines of code per crate, generated files separated out\n\
                 matrix        run the screenshot regression matrix; see `matrix --help`\n\
                 ini-coverage  per-field INI parsed/applied report; see `ini-coverage --help`\n\
                 ini-tables    recover retail INI parse tables from a Worldbuilder.exe; see `ini-tables --help`\n"
            );
            return ExitCode::FAILURE;
        }
    };

    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn repo_root() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join("Cargo.toml").is_file() && dir.join("deny.toml").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn step(root: &Path, label: &str, program: &str, args: &[&str]) -> bool {
    println!("\n=== {label} ===");
    match Command::new(program).args(args).current_dir(root).status() {
        Ok(s) if s.success() => true,
        Ok(s) => {
            eprintln!("FAILED: {label} (exit {:?})", s.code());
            if s.code() == Some(101) {
                if let Some(sub) = args.first() {
                    if sub.starts_with("audit") || sub.starts_with("deny") {
                        eprintln!(
                            "  if the command is missing: cargo install cargo-{sub} --locked"
                        );
                    }
                }
            }
            false
        }
        Err(e) => {
            eprintln!("FAILED: {label} could not run ({e})");
            eprintln!("  install it, or the gate is not actually being enforced");
            false
        }
    }
}

fn check(root: &Path) -> bool {
    let mut ok = true;
    ok &= step(root, "rustfmt", "cargo", &["fmt", "--all", "--check"]);
    ok &= step(
        root,
        "clippy",
        "cargo",
        &[
            "clippy",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
    );
    ok &= step(
        root,
        "tests",
        "cargo",
        &["test", "--all-features", "--no-fail-fast"],
    );
    ok &= deps(root);
    ok &= lints(root);
    ok &= tiers(root);
    ok &= text_boundary(root);
    ok &= codemap::check(root);
    ok &= step(root, "advisories (cargo-audit)", "cargo", &["audit"]);
    ok &= step(
        root,
        "licences and sources (cargo-deny)",
        "cargo",
        &["deny", "check"],
    );

    println!("\n{}", if ok { "gate: PASS" } else { "gate: FAIL" });
    ok
}

fn deps(root: &Path) -> bool {
    println!("\n=== dependency justifications ===");
    let mut manifests = vec![root.join("Cargo.toml")];
    for d in crate_dirs(root) {
        manifests.push(d.join("Cargo.toml"));
    }
    manifests.sort();

    let mut missing: Vec<String> = Vec::new();
    let mut count = 0usize;

    for manifest in &manifests {
        let Ok(text) = std::fs::read_to_string(manifest) else {
            continue;
        };
        let label = manifest
            .strip_prefix(root)
            .unwrap_or(manifest)
            .display()
            .to_string();
        let mut in_deps = false;
        let mut justified = false;
        let mut depth = 0i32;

        for line in text.lines() {
            let t = line.trim();

            if depth == 0 && t.starts_with('[') && !t.starts_with("[[") {
                let head: String = t
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                if let Some(rest) = head.split("dependencies.").nth(1) {
                    in_deps = false;
                    if !rest.is_empty() {
                        count = count.saturating_add(1);
                        if !justified {
                            missing.push(format!("{label}: {rest}"));
                        }
                    }
                } else {
                    in_deps = head.ends_with("dependencies");
                }
                justified = false;
                continue;
            }

            if in_deps && depth == 0 && !t.is_empty() {
                if let Some(rest) = t.strip_prefix('#') {
                    justified |= rest.trim_start().to_ascii_lowercase().starts_with("why:");
                } else if let Some((dep, _)) = t.split_once('=') {
                    let dep = dep.trim();
                    if !dep.is_empty() {
                        count = count.saturating_add(1);
                        if !justified {
                            missing.push(format!("{label}: {dep}"));
                        }
                        justified = false;
                    }
                }
            }

            for c in t.chars() {
                match c {
                    '#' => break,
                    '[' | '{' => depth = depth.saturating_add(1),
                    ']' | '}' => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
        }
    }

    if missing.is_empty() {
        println!(
            "{count} dependencies across {} manifests, all justified",
            manifests.len()
        );
        true
    } else {
        eprintln!(
            "FAILED: {} dependencies with no `# why:` line:",
            missing.len()
        );
        for m in &missing {
            eprintln!("  {m}");
        }
        eprintln!("\nAdd one line above each saying why it earns a permanent place in the tree.");
        false
    }
}

fn lints(root: &Path) -> bool {
    println!("\n=== lint opt-in ===");
    let mut bad: Vec<String> = Vec::new();
    let mut checked = 0usize;
    let mut layered = 0usize;

    for d in &crate_dirs(root) {
        let lbl = label(root, d);
        let Ok(text) = std::fs::read_to_string(d.join("Cargo.toml")) else {
            bad.push(format!("{lbl}: Cargo.toml could not be read"));
            continue;
        };
        checked = checked.saturating_add(1);
        let flat = squeeze(&text);

        if !flat.contains("[lints]workspace=true") {
            bad.push(format!(
                "{lbl}: no `[lints]` with `workspace = true` - the safety lints do nothing here"
            ));
        }

        if dir_tier(root, d).is_none() {
            continue;
        }

        let Some(layer) = declared_layer(&text) else {
            bad.push(format!(
                "{lbl}: no `[package.metadata.mint] layer = N` - the layer decides which \
                 extra lints this crate must carry, so it cannot be left unstated"
            ));
            continue;
        };
        if layer > 2 {
            continue;
        }
        layered = layered.saturating_add(1);

        let src = crate_root_source(d).unwrap_or_default();
        let squeezed_src = squeeze(&src);
        for (attr, manifest_form) in [
            ("#![forbid(unsafe_code)]", "unsafe_code=\"forbid\""),
            (
                "#![deny(clippy::arithmetic_side_effects)]",
                "arithmetic_side_effects=\"deny\"",
            ),
        ] {
            if !squeezed_src.contains(&squeeze(attr)) && !flat.contains(manifest_form) {
                bad.push(format!("{lbl}: layer {layer} crate is missing `{attr}`"));
            }
        }
    }

    if bad.is_empty() {
        println!(
            "{checked} crates opted into the workspace lints; \
             {layered} layer 0-2 crates carry the extra safety lints"
        );
        return true;
    }

    eprintln!("FAILED:");
    for b in &bad {
        eprintln!("  {b}");
    }
    eprintln!(
        "\nWithout the opt-in the safety lints do nothing at all in that crate.\n\
         Layers 0-2 parse untrusted input, so they additionally forbid unsafe and deny\n\
         silent arithmetic overflow. Cargo rejects those in `[lints]` beside\n\
         `workspace = true` (\"cannot override `workspace.lints` in `lints`\"), so they\n\
         belong at the crate root in src/lib.rs instead."
    );
    false
}

fn squeeze(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn declared_layer(text: &str) -> Option<u8> {
    mint_metadata(text, "layer")?.parse().ok()
}

fn declared_kind(text: &str) -> Option<String> {
    mint_metadata(text, "kind").map(|v| v.trim_matches('"').to_string())
}

fn layer_zero_edge_allowed(from: &str, to: &str) -> bool {
    match to {
        "infrastructure" => true,
        "codec" => from == "format",
        _ => false,
    }
}

fn mint_metadata(text: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_section = squeeze(t) == "[package.metadata.mint]";
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

fn crate_root_source(dir: &Path) -> Option<String> {
    ["src/lib.rs", "src/main.rs"]
        .iter()
        .find_map(|f| std::fs::read_to_string(dir.join(f)).ok())
}

fn ini_coverage(root: &Path) -> bool {
    let args: Vec<String> = std::env::args().skip(2).collect();
    if args.is_empty() {
        eprintln!(
            "usage: cargo xtask ini-coverage <BlockType|--summary|--abilities <out.md>> <install dir> [<install dir> ...]\n\
             \n\
             BlockType is one of: Locomotor, Weapon, SpecialPower, Object\n\
             Installs may also come from MINT_CORPUS.\n\
             Mount order matters: base game first, expansion second (`ini-schema.md` §9)."
        );
        return false;
    }
    let mut cmd_args = vec![
        "run".to_owned(),
        "--release".to_owned(),
        "-p".to_owned(),
        "mint-sage-ini-coverage".to_owned(),
        "--".to_owned(),
    ];
    cmd_args.extend(args);
    let refs: Vec<&str> = cmd_args.iter().map(String::as_str).collect();
    step(root, "ini-coverage", "cargo", &refs)
}

fn stats(root: &Path) -> bool {
    println!("\n=== size ===");
    let mut rows: Vec<(String, usize, usize)> = Vec::new();

    for d in crate_dirs(root) {
        let name = label(root, &d);
        let (mut hand, mut gen) = (0usize, 0usize);
        walk(&d.join("src"), &mut |p: &Path, lines: usize| {
            if is_generated(p) {
                gen += lines;
            } else {
                hand += lines;
            }
        });
        if hand + gen > 0 {
            rows.push((name, hand, gen));
        }
    }

    let (mut th, mut tg) = (0usize, 0usize);
    for (n, h, g) in &rows {
        th += h;
        tg += g;
        println!("  {n:<24} {h:>7} hand-written  {g:>7} generated");
    }
    println!("  {:<24} {th:>7} hand-written  {tg:>7} generated", "TOTAL");
    true
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tier {
    Core,
    Sage,
    Game,
}

impl Tier {
    fn name(self) -> &'static str {
        match self {
            Tier::Core => "core",
            Tier::Sage => "sage",
            Tier::Game => "games",
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            Tier::Core => "mint-",
            Tier::Sage => "mint-sage-",
            Tier::Game => "mint-game-",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Tier::Core => 0,
            Tier::Sage => 1,
            Tier::Game => 2,
        }
    }
}

fn tier_of_package(name: &str) -> Option<Tier> {
    if name.starts_with("mint-game-") {
        Some(Tier::Game)
    } else if name.starts_with("mint-sage-") {
        Some(Tier::Sage)
    } else if name.starts_with("mint-") {
        Some(Tier::Core)
    } else {
        None
    }
}

fn dir_tier(root: &Path, dir: &Path) -> Option<Tier> {
    match label(root, dir).split('/').next() {
        Some("core") => Some(Tier::Core),
        Some("sage") => Some(Tier::Sage),
        Some("games") => Some(Tier::Game),
        _ => None,
    }
}

fn tiers(root: &Path) -> bool {
    println!("\n=== tier boundaries ===");
    let mut problems: Vec<String> = Vec::new();
    let mut checked = 0usize;

    let mut placed: Vec<(String, u8, Option<String>)> = Vec::new();
    for dir in crate_dirs(root) {
        if dir_tier(root, &dir).is_none() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(dir.join("Cargo.toml")) else {
            continue;
        };
        if let (Some(pkg), Some(layer)) = (package_name(&text), declared_layer(&text)) {
            placed.push((pkg, layer, declared_kind(&text)));
        }
    }

    for dir in crate_dirs(root) {
        let Some(tier) = dir_tier(root, &dir) else {
            continue;
        };
        let lbl = label(root, &dir);
        let Ok(text) = std::fs::read_to_string(dir.join("Cargo.toml")) else {
            problems.push(format!("{lbl}: Cargo.toml could not be read"));
            continue;
        };
        checked = checked.saturating_add(1);

        match package_name(&text) {
            Some(pkg) => {
                if tier_of_package(&pkg) != Some(tier) {
                    problems.push(format!(
                        "{lbl}: package `{pkg}` is in the {} tier by directory, \
                         but its name does not say so (expected prefix `{}`)",
                        tier.name(),
                        tier.prefix()
                    ));
                }
            }
            None => problems.push(format!("{lbl}: no package name found in Cargo.toml")),
        }

        for dep in manifest_deps(&text) {
            let Some(dep_tier) = tier_of_package(&dep) else {
                continue;
            };
            if dep_tier.rank() > tier.rank() {
                problems.push(format!(
                    "{lbl}: {} crate depends on `{dep}`, which is {} tier",
                    tier.name(),
                    dep_tier.name()
                ));
            }
        }

        let Some(layer) = declared_layer(&text) else {
            continue;
        };
        let kind = declared_kind(&text);
        if layer == 0 && !matches!(kind.as_deref(), Some("infrastructure" | "codec" | "format")) {
            problems.push(format!(
                "{lbl}: layer 0 needs `kind = \"infrastructure\" | \"codec\" | \"format\"` \
                 under [package.metadata.mint] (ADR 0015)"
            ));
        }
        for dep in runtime_deps(&text) {
            let Some((_, dep_layer, dep_kind)) = placed.iter().find(|(name, _, _)| *name == dep)
            else {
                continue;
            };
            if *dep_layer > layer {
                problems.push(format!(
                    "{lbl}: layer {layer} depends on `{dep}`, which is layer {dep_layer}"
                ));
            }
            if layer == 0 && *dep_layer == 0 {
                let from = kind.as_deref().unwrap_or("");
                let to = dep_kind.as_deref().unwrap_or("");
                if !layer_zero_edge_allowed(from, to) {
                    problems.push(format!(
                        "{lbl}: a layer-0 {from} crate depends on `{dep}`, a layer-0 {to} crate \
                         (ADR 0015: formats never depend on formats)"
                    ));
                }
            }
        }
    }

    if !problems.is_empty() {
        eprintln!("FAILED: tier boundary violations:");
        for p in &problems {
            eprintln!("  {p}");
        }
        eprintln!("\nDependencies run core <- sage <- games, one way only. See ADR 0002.");
        return false;
    }

    if checked == 0 {
        eprintln!("FAILED: no tiered crates found under crates/ - the check inspected nothing");
        return false;
    }
    println!(
        "{checked} tiered crates: core <- sage <- games holds, no upward layer edge, \
         layer-0 kinds respected"
    );
    true
}

fn text_boundary(root: &Path) -> bool {
    println!("\n=== text boundary (R6) ===");
    let ui_src = root.join("crates").join("core").join("ui").join("src");
    if !ui_src.is_dir() {
        eprintln!("FAILED: crates/core/ui/src not found - the check inspected nothing");
        return false;
    }
    let allowed = ["text/font.rs", "text/outline.rs"];
    let mut problems: Vec<String> = Vec::new();
    let mut checked = 0usize;

    walk(&ui_src, &mut |path: &Path, _lines: usize| {
        checked += 1;
        let rel = path.strip_prefix(&ui_src).unwrap_or(path);
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if allowed.contains(&rel_str.as_str()) {
            return;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        for (n, line) in text.lines().enumerate() {
            if line_names_skrifa(line) {
                problems.push(format!(
                    "{rel_str}:{}: names skrifa/read_fonts outside the allow-list",
                    n.saturating_add(1)
                ));
            }
        }
    });

    if !problems.is_empty() {
        eprintln!("FAILED: R6 violated - skrifa/read_fonts named outside {allowed:?}:");
        for p in &problems {
            eprintln!("  {p}");
        }
        eprintln!(
            "\nNo skrifa or read-fonts type may appear in mint-ui's public API. \
             See docs/design/ui-substrate.md R6."
        );
        return false;
    }

    if checked == 0 {
        eprintln!("FAILED: no source files under crates/core/ui/src - the check inspected nothing");
        return false;
    }
    println!("{checked} files under crates/core/ui/src checked; skrifa/read_fonts confined to {allowed:?}");
    true
}

fn line_names_skrifa(line: &str) -> bool {
    if line.trim_start().starts_with("//") {
        return false;
    }
    line.contains("skrifa") || line.contains("read_fonts")
}

fn crate_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if root.join("xtask/Cargo.toml").is_file() {
        out.push(root.join("xtask"));
    }
    collect_crates(&root.join("crates"), &mut out);
    out.sort();
    out
}

fn collect_crates(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        if p.join("Cargo.toml").is_file() {
            out.push(p);
        } else {
            collect_crates(&p, out);
        }
    }
}

fn label(root: &Path, dir: &Path) -> String {
    let rel = dir.strip_prefix(root).unwrap_or(dir);
    let s = rel.to_string_lossy().replace('\\', "/");
    s.strip_prefix("crates/").unwrap_or(&s).to_string()
}

fn package_name(text: &str) -> Option<String> {
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

fn manifest_deps(text: &str) -> Vec<String> {
    deps_where(text, |_| true)
}

fn runtime_deps(text: &str) -> Vec<String> {
    deps_where(text, |head| !head.contains("dev-dependencies"))
}

fn deps_where(text: &str, wanted: impl Fn(&str) -> bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_deps = false;
    let mut depth = 0i32;

    for line in text.lines() {
        let t = line.trim();

        if depth == 0 && t.starts_with('[') && !t.starts_with("[[") {
            let head: String = t
                .trim_start_matches('[')
                .trim_end_matches(']')
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            if !wanted(&head) {
                in_deps = false;
            } else if let Some(rest) = head.split("dependencies.").nth(1) {
                in_deps = false;
                if !rest.is_empty() {
                    out.push(rest.to_string());
                }
            } else {
                in_deps = head.ends_with("dependencies");
            }
            continue;
        }

        if in_deps && depth == 0 && !t.is_empty() && !t.starts_with('#') {
            if let Some((dep, _)) = t.split_once('=') {
                let dep = dep.trim().trim_matches('"');
                let dep = dep.split('.').next().unwrap_or(dep).trim();
                if !dep.is_empty() {
                    out.push(dep.to_string());
                }
            }
        }

        for c in t.chars() {
            match c {
                '#' => break,
                '[' | '{' => depth = depth.saturating_add(1),
                ']' | '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    out
}

fn is_generated(p: &Path) -> bool {
    std::fs::read_to_string(p)
        .map(|s| {
            s.lines()
                .find(|l| !l.trim().is_empty())
                .map(|l| {
                    let t = l.trim_start_matches(['/', '!', '*', ' ']);
                    t.starts_with("@generated") || t.starts_with("GENERATED")
                })
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

fn walk(dir: &Path, f: &mut impl FnMut(&Path, usize)) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, f);
        } else if p.extension().is_some_and(|x| x == "rs") {
            let n = std::fs::read_to_string(&p)
                .map(|s| s.lines().count())
                .unwrap_or(0);
            f(&p, n);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_tier_prefers_the_longer_prefix() {
        assert!(matches!(tier_of_package("mint-vfs"), Some(Tier::Core)));
        assert!(matches!(tier_of_package("mint-sage-big"), Some(Tier::Sage)));
        assert!(matches!(
            tier_of_package("mint-game-rotwk"),
            Some(Tier::Game)
        ));
        assert!(tier_of_package("thiserror").is_none());
        assert!(tier_of_package("mint").is_none());
    }

    #[test]
    fn dependency_direction_is_one_way() {
        assert!(Tier::Sage.rank() > Tier::Core.rank());
        assert!(Tier::Game.rank() > Tier::Sage.rank());
    }

    #[test]
    fn package_name_is_read_from_the_package_section_only() {
        let toml = "\
[package]
name = \"mint-sage-big\"
edition = \"2021\"

[dependencies]
name = \"not-the-package\"
";
        assert_eq!(package_name(toml).as_deref(), Some("mint-sage-big"));
    }

    #[test]
    fn package_name_ignores_subsections_of_package() {
        let toml = "\
[package.metadata.docs]
name = \"wrong\"

[package]
name = \"mint-vfs\"
";
        assert_eq!(package_name(toml).as_deref(), Some("mint-vfs"));
    }

    #[test]
    fn deps_are_found_in_every_section_shape() {
        let toml = "\
[dependencies]
thiserror = \"1\"
mint-vfs.workspace = true

[dependencies.byteorder]
version = \"1\"

[dev-dependencies]
mint-sage-big = { path = \"../big\" }

[target.'cfg(windows)'.dependencies]
tracing = \"0.1\"
";
        let found = manifest_deps(toml);
        for expected in [
            "thiserror",
            "mint-vfs",
            "byteorder",
            "mint-sage-big",
            "tracing",
        ] {
            assert!(found.iter().any(|d| d == expected), "missing {expected}");
        }
    }

    #[test]
    fn keys_inside_a_multiline_spec_are_not_mistaken_for_dependencies() {
        let toml = "\
[dependencies]
wgpu = { version = \"0.20\", features = [
    \"webgl\",
], default-features = false }
mint-vfs = \"0.1\"
";
        let found = manifest_deps(toml);
        assert!(found.iter().any(|d| d == "wgpu"));
        assert!(found.iter().any(|d| d == "mint-vfs"));
        assert!(!found.iter().any(|d| d == "features"));
        assert!(!found.iter().any(|d| d == "default-features"));
    }

    #[test]
    fn label_strips_the_crates_prefix() {
        let root = Path::new("/repo");
        assert_eq!(
            label(root, &root.join("crates").join("sage").join("big")),
            "sage/big"
        );
        assert_eq!(label(root, &root.join("xtask")), "xtask");
    }

    #[test]
    fn dir_tier_reads_the_tier_directory_and_skips_tooling() {
        let root = Path::new("/repo");
        let core = root.join("crates").join("core").join("vfs");
        let sage = root.join("crates").join("sage").join("w3d");
        let game = root.join("crates").join("games").join("rotwk");
        assert!(matches!(dir_tier(root, &core), Some(Tier::Core)));
        assert!(matches!(dir_tier(root, &sage), Some(Tier::Sage)));
        assert!(matches!(dir_tier(root, &game), Some(Tier::Game)));
        assert!(dir_tier(root, &root.join("xtask")).is_none());
    }

    #[test]
    fn declared_layer_is_read_with_or_without_a_trailing_comment() {
        let plain = "[package.metadata.mint]\nlayer = 0\n";
        let commented = "[package.metadata.mint]\nlayer = 2  # simulation\n";
        assert_eq!(declared_layer(plain), Some(0));
        assert_eq!(declared_layer(commented), Some(2));
    }

    #[test]
    fn a_format_may_use_a_codec_but_never_another_format() {
        assert!(layer_zero_edge_allowed("format", "infrastructure"));
        assert!(layer_zero_edge_allowed("format", "codec"));
        assert!(layer_zero_edge_allowed("codec", "infrastructure"));
        assert!(!layer_zero_edge_allowed("format", "format"));
        assert!(!layer_zero_edge_allowed("codec", "codec"));
        assert!(!layer_zero_edge_allowed("infrastructure", "codec"));
        assert!(!layer_zero_edge_allowed("format", ""));
    }

    #[test]
    fn runtime_deps_leave_dev_dependencies_out() {
        let toml = "\
[dependencies]
mint-vfs.workspace = true

[dev-dependencies]
mint-sage-big.workspace = true

[dev-dependencies.mint-sage-map]
workspace = true
";
        assert_eq!(runtime_deps(toml), vec!["mint-vfs".to_string()]);
        assert_eq!(manifest_deps(toml).len(), 3);
    }

    #[test]
    fn declared_kind_is_read_unquoted() {
        let toml = "[package.metadata.mint]\nlayer = 0\nkind = \"codec\"  # ADR 0015\n";
        assert_eq!(declared_kind(toml).as_deref(), Some("codec"));
        assert_eq!(declared_kind("[package.metadata.mint]\nlayer = 0\n"), None);
    }

    #[test]
    fn declared_layer_is_absent_rather_than_defaulted() {
        assert_eq!(declared_layer("[package]\nname = \"mint-vfs\"\n"), None);
        assert_eq!(declared_layer(""), None);
    }

    #[test]
    fn declared_layer_ignores_a_layer_key_in_another_section() {
        let toml = "\
[package.metadata.other]
layer = 9

[package.metadata.mint]
layer = 1
";
        assert_eq!(declared_layer(toml), Some(1));
    }

    #[test]
    fn squeeze_makes_spacing_irrelevant() {
        assert_eq!(squeeze("a = \"b\""), squeeze("a=\"b\""));
        assert_eq!(
            squeeze("[lints]\nworkspace = true"),
            "[lints]workspace=true"
        );
    }

    #[test]
    fn line_names_skrifa_catches_real_code() {
        assert!(line_names_skrifa("use skrifa::FontRef;"));
        assert!(line_names_skrifa(
            "    let font = skrifa::FontRef::new(&bytes)?;"
        ));
        assert!(line_names_skrifa("use read_fonts::types::GlyphId;"));
    }

    #[test]
    fn line_names_skrifa_exempts_doc_comments() {
        assert!(!line_names_skrifa("/// mentions skrifa in prose only"));
        assert!(!line_names_skrifa("//! this crate wraps skrifa/read_fonts"));
        assert!(!line_names_skrifa(
            "    // skrifa is not imported on this line"
        ));
    }

    #[test]
    fn line_names_skrifa_ignores_unrelated_lines() {
        assert!(!line_names_skrifa("pub struct Font { bytes: Vec<u8> }"));
        assert!(!line_names_skrifa(""));
    }
}
