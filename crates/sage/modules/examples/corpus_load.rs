use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_sage_ini::{BlockTable, MemorySources, Reader};
use mint_sage_modules::{
    register_every_family, register_live_families, spawn_template, TemplateIndex, UpgradeBits,
};
use mint_sim::module::{ModuleRegistry, StubHit};
use mint_sim::{Frame, World};

const INI_ROOT: &str = "data/ini/";

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: corpus_load <install dir> [<install dir> ...]");
        eprintln!("       mount both installs, ROTWK last, so its archives shadow BFME II's");
        return ExitCode::FAILURE;
    }

    let started = Instant::now();

    let mut sources = MemorySources::new();
    let mut entries = 0usize;
    for root in &roots {
        for path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&path) else {
                continue;
            };
            for entry in archive.entries() {
                if !entry.path.as_str().starts_with(INI_ROOT) {
                    continue;
                }
                let Ok(bytes) = archive.read_entry(entry) else {
                    continue;
                };
                entries += 1;
                sources.insert(entry.path.as_str(), bytes);
            }
        }
    }
    println!(
        "{entries} entries under {INI_ROOT} ({} distinct)",
        sources.len()
    );
    if sources.is_empty() {
        eprintln!("no data/ini entries found under any given install - wrong path?");
        return ExitCode::FAILURE;
    }

    let table = BlockTable::corpus_path();
    let reader = Reader::new(&sources);
    let paths: Vec<String> = sources.paths().map(str::to_owned).collect();
    let mut documents = Vec::new();
    let mut parsed = 0u64;
    let mut errors = 0u64;
    for path in &paths {
        match reader.read(path, &table) {
            Ok(doc) => {
                parsed += 1;
                documents.push(doc);
            }
            Err(_) => errors += 1,
        }
    }
    println!("{parsed} documents read, {errors} errored");

    mint_sage_ini::expand_macros(&mut documents);

    let mut index = TemplateIndex::new();
    for doc in &documents {
        index.insert_document(doc);
    }
    let names: Vec<String> = index.names().map(str::to_owned).collect();
    println!("{} distinct template names indexed", names.len());

    let upgrade_bits = UpgradeBits::from_index(index.upgrades());
    println!(
        "{} distinct Upgrade definitions assigned a bit ({} Upgrade blocks seen)",
        upgrade_bits.len(),
        index.upgrades().definitions()
    );

    println!("\n-- live: what a real match registers (`ModuleWiring::setup`) --");
    let live = run_pass(&index, &names, &upgrade_bits, |registry, world| {
        register_live_families(registry, world);
    });
    live.report(&names);

    println!("\n-- available: every module type this crate can build --");
    let available = run_pass(&index, &names, &upgrade_bits, |registry, world| {
        register_every_family(registry, world);
    });
    available.report(&names);

    println!(
        "\nlive vs available: {} fewer templates load clean under what a match actually \
         registers ({:.1} vs {:.1}%) - the families `register_every_family` adds to \
         `register_live_families`.",
        available.clean.saturating_sub(live.clean),
        percent(live.clean, names.len() as u64),
        percent(available.clean, names.len() as u64)
    );

    println!("\n-- greedy schedule (against \"available\"): which modules, added together, buy the most clean loads --");
    println!(
        "(myopic greedy, not exact optimal set cover - see the doc comment on \
         `greedy_schedule` in this file. Each row's module is the next one worth \
         building, in order.)"
    );
    greedy_schedule(&available.blocking, available.clean, names.len() as u64);

    let elapsed = started.elapsed().as_secs_f64();
    println!("\n{elapsed:.2} s total");
    if !live.deterministic || !available.deterministic {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

struct PassReport {
    clean: u64,
    needs_stub: u64,
    spawn_failures: u64,
    total_declarations: u64,
    tally: Vec<StubHit>,
    blocking: Vec<Vec<String>>,
    deterministic: bool,
}

impl PassReport {
    fn report(&self, names: &[String]) {
        let total = names.len() as u64;
        println!(
            "{} of {total} templates load with NO stub ({:.1}%)",
            self.clean,
            percent(self.clean, total)
        );
        println!(
            "{} of {total} templates need at least one stub ({:.1}%)",
            self.needs_stub,
            percent(self.needs_stub, total)
        );
        if self.spawn_failures > 0 {
            println!(
                "{} templates failed to spawn at all (unexpected in non-strict mode)",
                self.spawn_failures
            );
        }
        println!(
            "{} module declarations resolved in total across every template",
            self.total_declarations
        );
        println!(
            "{} distinct unregistered/unbuildable module names",
            self.tally.len()
        );
        for hit in self.tally.iter().take(20) {
            println!("  {:>6}  {}", hit.uses, hit.name);
        }
        if self.tally.len() > 20 {
            println!("  ... and {} more", self.tally.len() - 20);
        }
        println!(
            "determinism: two independent passes over the whole corpus {}",
            if self.deterministic {
                "MATCH"
            } else {
                "MISMATCH - a nondeterminism exists somewhere in template loading"
            }
        );
    }
}

fn run_pass(
    index: &TemplateIndex<'_>,
    names: &[String],
    upgrade_bits: &UpgradeBits,
    register: impl Fn(&mut ModuleRegistry, &mut World),
) -> PassReport {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    register(&mut registry, &mut world);

    let mut clean = 0u64;
    let mut needs_stub = 0u64;
    let mut total_declarations = 0u64;
    let mut spawn_failures = 0u64;
    let tally_map = |registry: &ModuleRegistry| -> BTreeMap<String, u64> {
        registry
            .tally()
            .into_iter()
            .map(|hit| (hit.name, hit.uses))
            .collect()
    };

    let mut blocking: Vec<Vec<String>> = Vec::new();

    for name in names {
        let declarations = index.resolve(name, &registry).len() as u64;
        total_declarations = total_declarations.saturating_add(declarations);
        let before = tally_map(&registry);
        match spawn_template(
            &mut world,
            &registry,
            Frame::ZERO,
            index,
            upgrade_bits,
            name,
        ) {
            Ok(_entity) => {
                let after = tally_map(&registry);
                let mut newly_blocked: Vec<String> = after
                    .iter()
                    .filter(|(k, v)| before.get(*k).copied().unwrap_or(0) < **v)
                    .map(|(k, _)| k.clone())
                    .collect();
                if newly_blocked.is_empty() {
                    clean = clean.saturating_add(1);
                } else {
                    needs_stub = needs_stub.saturating_add(1);
                    newly_blocked.sort();
                    blocking.push(newly_blocked);
                }
            }
            Err(_) => spawn_failures = spawn_failures.saturating_add(1),
        }
    }

    let mut tally = registry.tally();
    tally.sort_by(|a, b| b.uses.cmp(&a.uses).then_with(|| a.name.cmp(&b.name)));

    let mut world_b = World::new();
    let mut registry_b = ModuleRegistry::new(&mut world_b);
    register(&mut registry_b, &mut world_b);
    for name in names {
        let _ = spawn_template(
            &mut world_b,
            &registry_b,
            Frame::ZERO,
            index,
            upgrade_bits,
            name,
        );
    }

    PassReport {
        clean,
        needs_stub,
        spawn_failures,
        total_declarations,
        tally,
        blocking,
        deterministic: world.state_hash() == world_b.state_hash(),
    }
}

fn greedy_schedule(blocking: &[Vec<String>], already_clean: u64, total_templates: u64) {
    let mut remaining: Vec<Vec<&str>> = blocking
        .iter()
        .map(|set| set.iter().map(String::as_str).collect())
        .collect();
    let mut cumulative_clean = already_clean;
    let mut picked = 0usize;
    let mut printed = 0usize;

    loop {
        let mut completes: BTreeMap<&str, u64> = BTreeMap::new();
        let mut touches: BTreeMap<&str, u64> = BTreeMap::new();
        for set in &remaining {
            if set.is_empty() {
                continue;
            }
            for &m in set {
                *touches.entry(m).or_insert(0) += 1;
            }
            if set.len() == 1 {
                if let Some(&only) = set.first() {
                    *completes.entry(only).or_insert(0) += 1;
                }
            }
        }
        if touches.is_empty() {
            break;
        }

        let choice = if completes.is_empty() {
            touches
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
        } else {
            completes
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
        };
        let Some((&module, _)) = choice else { break };

        let mut newly_clean = 0u64;
        for set in &mut remaining {
            if set.is_empty() {
                continue;
            }
            set.retain(|&m| m != module);
            if set.is_empty() {
                newly_clean = newly_clean.saturating_add(1);
            }
        }
        cumulative_clean = cumulative_clean.saturating_add(newly_clean);
        picked = picked.saturating_add(1);
        if printed < 50 {
            println!(
                "  {picked:>3}. +{module:<38} completes {newly_clean:>4} more -> {cumulative_clean:>5}/{total_templates} clean ({:.1}%)",
                percent(cumulative_clean, total_templates)
            );
            printed = printed.saturating_add(1);
        }
    }

    println!(
        "{picked} more module types would take every template that declares at least one \
         module to 100% clean"
    );
}

fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 * 100.0 / whole as f64
    }
}

fn big_archives(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(listing) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in listing.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(kind) if kind.is_file() => {
                    if path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("big"))
                    {
                        found.push(path);
                    }
                }
                _ => {}
            }
        }
    }
    found.sort();
    found
}
