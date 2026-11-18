use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_sage_combat::load::{ARMOR_KEYWORD, WEAPON_KEYWORD};
use mint_sage_combat::{CombatData, Nugget};
use mint_sage_ini::{BlockTable, Document, MacroTable, MemorySources, Reader};

const INI_ROOT: &str = "data/ini/";

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: combat_corpus <install dir> [<install dir> ...]");
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
    let mut parse_errors: Vec<String> = Vec::new();
    for path in &paths {
        match reader.read(path, &table) {
            Ok(doc) => documents.push(doc),
            Err(error) => parse_errors.push(format!("{path}: {error}")),
        }
    }
    println!(
        "{} documents read, {} errored",
        documents.len(),
        parse_errors.len()
    );

    let mut macros = MacroTable::new();
    for doc in &documents {
        macros.insert_defines(doc.defines());
    }
    println!("{} numeric #define(s) resolved", macros.len());

    let mut data = CombatData::new();
    for doc in &documents {
        data.load_document(doc, &macros);
    }
    let elapsed = started.elapsed();

    let report = data.report.clone();
    println!();
    println!(
        "{} Weapon block(s) -> {} distinct template(s); {} Armor block(s) -> {} distinct",
        report.weapons,
        data.weapons.len(),
        report.armors,
        data.armors.len()
    );
    println!(
        "{} armour row(s); {} damage type(s) interned; {} death type(s)",
        report.armor_rows,
        data.damage_types.len(),
        data.death_types.len()
    );
    println!("  damage types: {}", data.damage_types.names().join(" "));
    println!("  death types:  {}", data.death_types.names().join(" "));
    println!(
        "{} nugget(s), {} of them modelled ({:.1}%)",
        report.nuggets,
        report.modelled_nuggets,
        percent(report.modelled_nuggets, report.nuggets)
    );
    println!(
        "{} field(s) held unresolvable inline arithmetic or an unknown macro",
        report.unresolved_fields
    );
    println!(
        "{} weapon(s) declare no modelled damaging effect; {} unnamed block(s)",
        report.weapons_without_effect, report.unnamed_blocks
    );
    println!("loaded in {:.2} s", elapsed.as_secs_f64());
    let refused = data.value_errors.take();
    println!("{} value(s) refused", refused.len());
    for error in &refused {
        println!("  {error}");
    }
    let count = |test: &dyn Fn(&mint_sage_combat::WeaponTemplate) -> bool| {
        data.weapons.iter().filter(|(_, w)| test(w)).count()
    };
    println!(
        "hit fields: {} miss sometimes, {} always; {} scatter; {} random wind-up; \
         {} HitStoredTarget; {} CanBeDodged; {} HitPassengerPercentage; \
         {} NoVictimNeeded; {} CanFireWhileMoving",
        count(&|w| w.hit_chance > 0.0 && w.hit_chance < 1.0),
        count(&|w| w.hit_chance <= 0.0),
        count(&|w| w.scatter_radius > 0.0),
        count(&|w| !w.pre_attack_random.is_zero()),
        count(&|w| w.hit_stored_target),
        count(&|w| w.can_be_dodged),
        count(&|w| w.hit_passenger_chance > 0.0),
        count(&|w| w.no_victim_needed),
        count(&|w| w.can_fire_while_moving),
    );
    println!(
        "geometry fields: {} ProjectileCollidesWith other than STRUCTURES; {} LeechRangeWeapon; {} ScatterRadiusVsInfantry; {} RangeBonusPerFoot",
        count(&|w| w.projectile_collides != mint_sage_combat::CollideMask::default()),
        count(&|w| w.leech_range),
        count(&|w| w.scatter_vs_infantry > 0.0),
        count(&|w| w.range_bonus.per_foot > 0.0),
    );

    let mut census: BTreeMap<String, usize> = BTreeMap::new();
    for (_, weapon) in data.weapons.iter() {
        for nugget in &weapon.nuggets {
            *census.entry(nugget.keyword().to_owned()).or_insert(0) += 1;
        }
    }
    println!("\nnugget census, by kind:");
    let mut rows: Vec<(&String, &usize)> = census.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    for (kind, count) in rows {
        println!("  {count:5}  {kind}");
    }

    let mut dangling: Vec<String> = Vec::new();
    let mut projectile_nuggets = 0usize;
    for (id, weapon) in data.weapons.iter() {
        for nugget in &weapon.nuggets {
            let Nugget::Projectile(projectile) = nugget else {
                continue;
            };
            projectile_nuggets = projectile_nuggets.saturating_add(1);
            if data.weapons.id(&projectile.warhead).is_none() {
                dangling.push(format!(
                    "weapon #{} names warhead `{}`, which is not a loaded Weapon",
                    id.index(),
                    projectile.warhead
                ));
            }
        }
    }
    println!(
        "\n{projectile_nuggets} ProjectileNugget(s); {} name a warhead that is not a loaded \
         Weapon",
        dangling.len()
    );

    let mut default_not_first: Vec<String> = Vec::new();
    let mut armor_blocks = 0usize;
    for doc in &documents {
        for block in doc.roots() {
            if !block.keyword.eq_ignore_ascii_case(ARMOR_KEYWORD) {
                continue;
            }
            armor_blocks = armor_blocks.saturating_add(1);
            let first = block
                .fields()
                .find(|f| f.keyword.eq_ignore_ascii_case(ARMOR_KEYWORD))
                .and_then(|f| f.args.first());
            if let Some(token) = first {
                if !token.eq_ignore_ascii_case("DEFAULT") {
                    default_not_first.push(format!(
                        "{} `{}` opens with `{token}`, not DEFAULT",
                        block.file,
                        block.name().unwrap_or("<unnamed>")
                    ));
                }
            }
        }
    }
    println!(
        "{armor_blocks} Armor block(s) walked; {} do not put DEFAULT first (§6.2 measured 0)",
        default_not_first.len()
    );

    println!(
        "{} damage type(s) against §6.3's measured 18 (plus the DEFAULT wildcard, not a type)",
        data.damage_types.len()
    );

    let mut failed = false;
    failed |= report_list("parse failure", &parse_errors);
    failed |= report_list("dangling warhead", &dangling);
    failed |= report_list("armour block not opening with DEFAULT", &default_not_first);

    if report.weapons == 0 {
        eprintln!("\nno Weapon blocks loaded at all - wrong install path, or a parse failure");
        failed = true;
    }
    if failed {
        ExitCode::FAILURE
    } else {
        println!("\nall invariants held");
        ExitCode::SUCCESS
    }
}

fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        0.0
    } else {
        100.0 * part as f64 / whole as f64
    }
}

fn report_list(label: &str, failures: &[String]) -> bool {
    if failures.is_empty() {
        println!("no {label}s");
        return false;
    }
    println!("\n{} {label}(s):", failures.len());
    for failure in failures.iter().take(40) {
        println!("  {failure}");
    }
    if failures.len() > 40 {
        println!("  ... and {} more", failures.len().saturating_sub(40));
    }
    true
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

#[allow(dead_code)]
const WEAPON: &str = WEAPON_KEYWORD;
