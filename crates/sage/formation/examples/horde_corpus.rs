use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_sage_formation::{horde_contains, read_horde_contain, FormationTable, HordeContainDef};
use mint_sage_ini::{BlockTable, Document, MacroTable, MemorySources, Reader};

const INI_ROOT: &str = "data/ini/";

const OBJECT_KEYWORDS: [&str; 3] = ["Object", "ChildObject", "ObjectReskin"];

fn main() -> ExitCode {
    let mut list = false;
    let roots: Vec<PathBuf> = std::env::args()
        .skip(1)
        .filter(|arg| {
            if arg == "--list" {
                list = true;
                return false;
            }
            true
        })
        .map(PathBuf::from)
        .collect();
    if roots.is_empty() {
        eprintln!("usage: horde_corpus <install dir> [<install dir> ...]");
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
                entries = entries.saturating_add(1);
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
    let mut documents: Vec<Document> = Vec::new();
    let mut unreadable = 0usize;
    for path in &paths {
        match reader.read(path, &table) {
            Ok(document) => documents.push(document),
            Err(_) => unreadable = unreadable.saturating_add(1),
        }
    }
    let mut macros = MacroTable::new();
    for document in &documents {
        macros.insert_defines(document.defines());
    }
    let parsed = started.elapsed();

    let mut found: Vec<(String, String, String, u32, HordeContainDef)> = Vec::new();
    let mut nested = 0usize;
    for document in &documents {
        for object in document.roots() {
            if !OBJECT_KEYWORDS
                .iter()
                .any(|k| object.keyword.eq_ignore_ascii_case(k))
            {
                continue;
            }
            for module in horde_contains(object) {
                if module.line > 0 && !object.blocks().any(|direct| std::ptr::eq(direct, module)) {
                    nested = nested.saturating_add(1);
                }
                found.push((
                    module.selector().unwrap_or("<unnamed>").to_owned(),
                    object.name().unwrap_or("<unnamed>").to_owned(),
                    module.file.to_string(),
                    module.line,
                    read_horde_contain(module, &macros, &mint_sage_ini::ValueErrors::new()),
                ));
            }
        }
    }

    println!(
        "{} INI document(s) read ({unreadable} unreadable, {} macro(s)) in {:.2} s; \
         {} horde-contain module(s), {nested} of them recovered from below depth 1",
        documents.len(),
        macros.len(),
        parsed.as_secs_f64(),
        found.len(),
    );
    if found.is_empty() {
        eprintln!("no HordeContain module found - the block table is not opening `Behavior`");
        return ExitCode::FAILURE;
    }

    if list {
        for (module, name, file, line, def) in &found {
            println!(
                "  LIST {file}:{line} {module} {name} slots={:?} lines={} positions={}",
                def.slots,
                def.ranks.len(),
                def.position_count()
            );
        }
    }
    report_by_module(&found);
    report_slot_invariant(&found);
    report_leader_graph(&found);
    report_geometry(&found);
    report_losses(&found);

    println!("total {:.2} s", started.elapsed().as_secs_f64());
    ExitCode::SUCCESS
}

fn report_by_module(found: &[(String, String, String, u32, HordeContainDef)]) {
    let mut rows: Vec<(String, usize, usize, usize, usize, usize)> = Vec::new();
    for (module, _, _, _, def) in found {
        let leaders = def
            .ranks
            .iter()
            .flat_map(|line| line.positions.iter())
            .filter(|slot| slot.leader.is_some())
            .count();
        let entry = match rows
            .iter_mut()
            .find(|(name, ..)| name.eq_ignore_ascii_case(module))
        {
            Some(entry) => entry,
            None => {
                rows.push((module.clone(), 0, 0, 0, 0, 0));
                match rows.last_mut() {
                    Some(entry) => entry,
                    None => continue,
                }
            }
        };
        entry.1 = entry.1.saturating_add(1);
        entry.2 = entry.2.saturating_add(def.ranks.len());
        entry.3 = entry.3.saturating_add(def.position_count());
        entry.4 = entry.4.saturating_add(leaders);
        entry.5 = entry.5.saturating_add(usize::from(def.slots.is_none()));
    }
    rows.sort_by(|a, b| b.1.cmp(&a.1));
    println!("by module: blocks / RankInfo lines / positions / Leader refs / no Slots");
    for (name, blocks, lines, positions, leaders, no_slots) in rows {
        println!("  {name:<24} {blocks:>4} {lines:>6} {positions:>7} {leaders:>6} {no_slots:>4}");
    }
}

fn report_slot_invariant(found: &[(String, String, String, u32, HordeContainDef)]) {
    let (mut equal, mut more, mut fewer, mut no_slots) = (0usize, 0usize, 0usize, 0usize);
    for (_, name, file, line, def) in found {
        let positions = def.position_count();
        let Some(slots) = def.slots.and_then(|s| usize::try_from(s).ok()) else {
            no_slots = no_slots.saturating_add(1);
            continue;
        };
        if positions == slots {
            equal = equal.saturating_add(1);
        } else if positions > slots {
            more = more.saturating_add(1);
        } else {
            fewer = fewer.saturating_add(1);
            println!("  FEWER positions than Slots: {name} ({file}:{line}) {positions} < {slots}");
        }
    }
    println!(
        "slot invariant: positions == Slots in {equal}, > Slots in {more}, \
         < Slots in {fewer} (must be 0), no Slots declared in {no_slots}"
    );
}

fn report_leader_graph(found: &[(String, String, String, u32, HordeContainDef)]) {
    let (mut total, mut forest, mut own_rank, mut past_end) = (0usize, 0usize, 0usize, 0usize);
    let mut offenders: Vec<String> = Vec::new();
    for (_, name, _, _, def) in found {
        let mut bad_here = false;
        for line in &def.ranks {
            for slot in &line.positions {
                let Some(leader) = slot.leader else { continue };
                total = total.saturating_add(1);
                let target = def.ranks.iter().find(|other| other.rank == leader.rank);
                let within = target.is_some_and(|other| {
                    usize::try_from(leader.index).is_ok_and(|i| i < other.positions.len())
                });
                if leader.rank >= line.rank {
                    own_rank = own_rank.saturating_add(1);
                    bad_here = true;
                } else if !within {
                    past_end = past_end.saturating_add(1);
                    bad_here = true;
                } else {
                    forest = forest.saturating_add(1);
                }
            }
        }
        if bad_here {
            offenders.push(name.clone());
        }
    }
    println!(
        "leader graph: {total} reference(s); {forest} point at an existing slot in a strictly \
         lower rank, {own_rank} name their own rank or a later one, {past_end} name an index \
         past the end of the rank they reference"
    );
    offenders.sort_unstable();
    offenders.dedup();
    println!(
        "  {} object(s) carry at least one malformed reference: {}",
        offenders.len(),
        offenders.join(", ")
    );
}

fn report_geometry(found: &[(String, String, String, u32, HordeContainDef)]) {
    let mut centroids_x: Vec<f32> = Vec::new();
    let mut centroids_y: Vec<f32> = Vec::new();
    let mut lines = 0usize;
    let mut positions = 0usize;
    let mut all_x_forward = 0usize;
    let mut jittered = 0usize;
    for (_, _, _, _, def) in found {
        lines = lines.saturating_add(def.ranks.len());
        positions = positions.saturating_add(def.position_count());
        if def.random_offset != (0.0, 0.0) {
            jittered = jittered.saturating_add(1);
        }
        let table = FormationTable::from_contain(def);
        if table.is_empty() {
            continue;
        }
        let centroid = table.centroid();
        centroids_x.push(centroid.x);
        centroids_y.push(centroid.y);
        if table.slots().iter().all(|slot| slot.offset.x >= 0.0) {
            all_x_forward = all_x_forward.saturating_add(1);
        }
    }
    println!("geometry: {lines} RankInfo line(s), {positions} position(s) in all");
    println!(
        "  {jittered} block(s) declare a non-zero RandomOffset; \
         {all_x_forward} of {} have every slot at or ahead of the origin",
        centroids_x.len()
    );
    for (axis, mut values) in [("x", centroids_x), ("y", centroids_y)] {
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = values.get(values.len() / 2).copied().unwrap_or(0.0);
        let low = values.first().copied().unwrap_or(0.0);
        let high = values.last().copied().unwrap_or(0.0);
        println!("  per-block centroid {axis}: min {low:.1} median {median:.1} max {high:.1}");
    }
}

fn report_losses(found: &[(String, String, String, u32, HordeContainDef)]) {
    let (mut malformed, mut dropped, mut unresolved) = (0usize, 0usize, 0usize);
    for (_, name, file, line, def) in found {
        malformed = malformed.saturating_add(def.report.positions_malformed);
        dropped = dropped.saturating_add(def.report.positions_dropped);
        if def.report.slots_unresolved {
            unresolved = unresolved.saturating_add(1);
        }
        if def.report.positions_malformed > 0
            || def.report.positions_dropped > 0
            || def.report.slots_unresolved
        {
            println!(
                "  LOSS in {name} ({file}:{line}): {} malformed, {} dropped, slots unresolved {}",
                def.report.positions_malformed,
                def.report.positions_dropped,
                def.report.slots_unresolved
            );
        }
    }
    println!(
        "losses: {malformed} malformed position(s), {dropped} dropped past the cap, \
         {unresolved} block(s) whose Slots did not resolve"
    );
}

fn big_archives(root: &Path) -> Vec<PathBuf> {
    let mut paths = mint_sage_big::archives_under(root);
    paths.sort();
    paths
}
