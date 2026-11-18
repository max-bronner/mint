use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_sage_ini::{
    ArmorIndex, BlockTable, CommandButtonIndex, CommandSetIndex, MemorySources,
    PlayerTemplateIndex, Reader, Stance, StanceTemplateIndex, ValueErrors, WeaponIndex,
};
use mint_sage_modules::{unit_builds, AutoAcquire, ObjectRecord, TemplateIndex, TemplateRecords};

const INI_ROOT: &str = "data/ini/";

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: object_records <install dir> [<install dir> ...]  (ROTWK last)");
        return ExitCode::FAILURE;
    }
    let mut sources = MemorySources::new();
    for root in &roots {
        for path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&path) else {
                continue;
            };
            for entry in archive.entries() {
                if !entry.path.as_str().starts_with(INI_ROOT) {
                    continue;
                }
                if let Ok(bytes) = archive.read_entry(entry) {
                    sources.insert(entry.path.as_str(), bytes);
                }
            }
        }
    }
    if sources.is_empty() {
        eprintln!("no data/ini entries under any given install");
        return ExitCode::FAILURE;
    }
    let table = BlockTable::corpus_path();
    let reader = Reader::new(&sources);
    let paths: Vec<String> = sources.paths().map(str::to_owned).collect();
    let mut documents: Vec<_> = paths
        .iter()
        .filter_map(|path| reader.read(path, &table).ok())
        .collect();
    println!("{} of {} documents read", documents.len(), paths.len());
    let macros = mint_sage_ini::expand_macros(&mut documents);

    let mut index = TemplateIndex::new();
    let mut weapons = WeaponIndex::new();
    let mut armors = ArmorIndex::new();
    let mut sets = CommandSetIndex::new();
    let mut buttons = CommandButtonIndex::new();
    let mut players = PlayerTemplateIndex::new();
    for document in &documents {
        index.insert_document(document);
        weapons.insert_document(document);
        armors.insert_document(document, &macros);
        sets.insert_document(document);
        buttons.insert_document(document, &macros);
        players.insert_document(document);
    }
    let started = Instant::now();
    let (records, refused) = TemplateRecords::build_collecting(&index);
    for error in &refused {
        println!("refused: {error}");
    }
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    let failed = index.len().saturating_sub(records.len());
    println!(
        "{} of {} template name(s) read into a record in {elapsed:.1} ms ({:.1} us each); {failed} failed",
        records.len(),
        index.len(),
        elapsed * 1000.0 / records.len().max(1) as f64
    );

    let mut by_depth = [0usize; 9];
    for record in records.iter() {
        if let Some(slot) = by_depth.get_mut(record.depth.min(8)) {
            *slot += 1;
        }
    }
    println!("lineage depth 0..8: {by_depth:?}");

    let count = |f: &dyn Fn(&ObjectRecord) -> bool| records.iter().filter(|r| f(r)).count();
    println!(
        "fields present: KindOf {}, MaxHealth {}, weapon {}, armour {}, CommandSet {}, \
         BuildCost {}, BuildTime {}, CommandPoints {}, ShroudClearingRange {}, VisionRange {}, \
         radius {}, income {}, ProductionUpdate {}, NaturalRallyPoint {}, dozer {}, castle {}",
        count(&|r| !r.kind_of.is_empty()),
        count(&|r| r.max_health.is_some()),
        count(&|r| r.weapon.is_some()),
        count(&|r| r.armor.is_some()),
        count(&|r| r.command_set.is_some()),
        count(&|r| r.build_cost.is_some()),
        count(&|r| r.build_time.is_some()),
        count(&|r| r.command_points.is_some()),
        count(&|r| r.shroud_clearing_range.is_some()),
        count(&|r| r.vision_range.is_some()),
        count(&|r| r.geometry.radius.is_some()),
        count(&|r| r.income.is_some()),
        count(&|r| r.produces),
        count(&|r| r.natural_rally_point.is_some()),
        count(&|r| r.dozer),
        count(&|r| r.castle.is_some()),
    );
    println!(
        "vision fields: VisionSide {}, VisionRear {}, a height bonus {}",
        count(&|r| r.vision_side.is_some()),
        count(&|r| r.vision_rear.is_some()),
        count(&|r| r.vision_bonus.is_some()),
    );

    let mut inherited = [0usize; 6];
    for name in index.names() {
        let lineage = index.lineage(name);
        let (Some(leaf), Some(record)) = (lineage.last(), records.get(name)) else {
            continue;
        };
        if record.depth == 0 {
            continue;
        }
        let checks = [
            leaf.field("CommandSet").is_none() && record.command_set.is_some(),
            leaf.field("BuildCost").is_none() && record.build_cost.is_some(),
            leaf.blocks_named("Body").next().is_none() && record.max_health.is_some(),
            leaf.blocks_named("WeaponSet").next().is_none() && record.weapon.is_some(),
            leaf.blocks_named("ArmorSet").next().is_none() && record.armor.is_some(),
            leaf.field("Geometry").is_none() && record.geometry.radius.is_some(),
        ];
        for (slot, hit) in inherited.iter_mut().zip(checks) {
            if hit {
                *slot += 1;
            }
        }
    }
    println!(
        "inherited, not restated: CommandSet {}, BuildCost {}, MaxHealth {}, weapon {}, armour {}, geometry {}",
        inherited[0], inherited[1], inherited[2], inherited[3], inherited[4], inherited[5]
    );

    let mut points: std::collections::BTreeMap<i64, usize> = std::collections::BTreeMap::new();
    for record in records.iter().filter(|r| r.has_kind("HORDE")) {
        #[allow(clippy::cast_possible_truncation)]
        let key = record.command_points.map_or(-1, |p| p as i64);
        *points.entry(key).or_insert(0) += 1;
    }
    println!(
        "CommandPoints over {} KindOf HORDE template(s) (-1 = absent): {points:?}",
        count(&|r| r.has_kind("HORDE"))
    );

    let cost_modifier_declarations: usize = records.iter().map(|r| r.cost_modifiers.len()).sum();
    let cost_modifier_templates = count(&|r| !r.cost_modifiers.is_empty());
    let starts_active = records
        .iter()
        .flat_map(|r| &r.cost_modifiers)
        .filter(|m| m.starts_active)
        .count();
    let triggered = records
        .iter()
        .flat_map(|r| &r.cost_modifiers)
        .filter(|m| !m.triggered_by.is_empty())
        .count();
    let upgrade_discount = records
        .iter()
        .flat_map(|r| &r.cost_modifiers)
        .filter(|m| m.upgrade_discount)
        .count();
    println!(
        "CostModifierUpgrade: {cost_modifier_declarations} declaration(s) over \
         {cost_modifier_templates} template(s) ({starts_active} StartsActive, \
         {triggered} TriggeredBy, {upgrade_discount} UpgradeDiscount)"
    );
    let production_cost_change = players
        .iter()
        .filter(|p| p.production_cost_change.is_some())
        .count();
    println!(
        "PlayerTemplate.ProductionCostChange: {production_cost_change} of {} declared \
         PlayerTemplate(s)",
        players.len()
    );

    auto_acquire_report(&records);

    let mut ok = failed == 0;
    ok &= stance_report(&records, &documents);
    let mut dangling_weapons: BTreeSet<String> = BTreeSet::new();
    let mut dangling_armors: BTreeSet<String> = BTreeSet::new();
    for record in records.iter() {
        if let Some(weapon) = &record.weapon {
            if weapons.get(weapon).is_none() {
                dangling_weapons.insert(format!("{} -> {weapon}", record.name));
            }
        }
        if let Some(armor) = &record.armor {
            if armors.get(armor).is_none() {
                dangling_armors.insert(format!("{} -> {armor}", record.name));
            }
        }
    }
    println!(
        "cross-check: {} of {} default PRIMARY weapon(s) name a Weapon block; {} of {} armour(s) an Armor block",
        count(&|r| r.weapon.is_some()) - dangling_weapons.len(),
        count(&|r| r.weapon.is_some()),
        count(&|r| r.armor.is_some()) - dangling_armors.len(),
        count(&|r| r.armor.is_some()),
    );
    for line in dangling_weapons
        .iter()
        .chain(dangling_armors.iter())
        .take(20)
    {
        println!("  dangling: {line}");
    }

    let mut named_sets: BTreeSet<String> = BTreeSet::new();
    for record in records.iter() {
        if let Some(set) = &record.command_set {
            named_sets.insert(set.to_ascii_lowercase());
        }
    }
    let mut builds = 0usize;
    let mut dangling_builds: BTreeSet<String> = BTreeSet::new();
    for set in &named_sets {
        for build in unit_builds(set, &sets, &buttons) {
            builds += 1;
            if records.get(&build.object).is_none() {
                dangling_builds.insert(format!("{} -> {}", build.button, build.object));
            }
        }
    }
    println!(
        "cross-check: {} of {builds} UNIT_BUILD button(s) across {} named command set(s) build a declared template",
        builds - dangling_builds.len(),
        named_sets.len()
    );
    for line in dangling_builds.iter().take(20) {
        println!("  dangling: {line}");
    }

    let mut factions = 0usize;
    for template in players.iter().filter(|t| t.playable == Some(true)) {
        factions += 1;
        let side = template.side.as_deref().unwrap_or_default();
        let building = template
            .starting_building
            .as_deref()
            .and_then(|b| records.get(b));
        let castle = building
            .and_then(|b| b.castle.as_ref())
            .and_then(|c| c.for_side(side));
        let units: Vec<String> = template
            .starting_units
            .iter()
            .map(|unit| {
                format!(
                    "{}{}",
                    unit.template,
                    if records.get(&unit.template).is_some() {
                        ""
                    } else {
                        " (MISSING)"
                    }
                )
            })
            .collect();
        let resolved = building.is_some()
            && castle.is_some()
            && template
                .starting_units
                .iter()
                .all(|u| records.get(&u.template).is_some());
        ok &= resolved;
        println!(
            "faction {} (side {side}, evil {:?}): building {} -> castle {}; units {}",
            template.name,
            template.evil,
            template.starting_building.as_deref().unwrap_or("-"),
            castle.unwrap_or("NONE"),
            units.join(", ")
        );
    }
    println!("cross-check: {factions} playable faction(s), all resolving: {ok}");
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn stance_report(records: &TemplateRecords, documents: &[mint_sage_ini::Document]) -> bool {
    let errors = ValueErrors::new();
    let mut stances = StanceTemplateIndex::new();
    let mut lists: BTreeSet<String> = BTreeSet::new();
    let mut button_tokens = 0usize;
    let mut buttons_with_stances = 0usize;
    let mut bad_tokens: Vec<String> = Vec::new();
    for document in documents {
        stances.insert_document(document, &errors);
        for block in document.roots() {
            if block.keyword.eq_ignore_ascii_case("ModifierList") {
                if let Some(name) = block.name() {
                    lists.insert(name.to_ascii_lowercase());
                }
            }
            if !block.keyword.eq_ignore_ascii_case("CommandButton") {
                continue;
            }
            let Some(field) = block.field("Stances") else {
                continue;
            };
            buttons_with_stances += 1;
            for token in &field.args {
                button_tokens += 1;
                if Stance::from_name(token).is_none() {
                    bad_tokens.push(token.clone());
                }
            }
        }
    }
    for error in errors.take() {
        println!("  refused: {error}");
    }
    let blocks: usize = stances.iter().map(|t| t.stances.len()).sum();
    let resolved_lists = stances
        .iter()
        .flat_map(|t| &t.stances)
        .filter(|s| {
            s.attribute_modifier
                .as_deref()
                .is_some_and(|name| lists.contains(&name.to_ascii_lowercase()))
        })
        .count();
    let holders: Vec<&ObjectRecord> = records
        .iter()
        .filter(|r| r.stance_template.is_some())
        .collect();
    let resolved_holders = holders
        .iter()
        .filter(|r| {
            r.stance_template
                .as_deref()
                .is_some_and(|name| stances.get(name).is_some())
        })
        .count();
    println!(
        "stances: {} StanceTemplate(s), {blocks} Stance block(s), {resolved_lists} of {blocks} \
         naming a declared ModifierList; {resolved_holders} of {} StancesBehavior holder(s) \
         resolve; {button_tokens} Stances token(s) over {buttons_with_stances} CommandButton(s), \
         {} unknown",
        stances.len(),
        holders.len(),
        bad_tokens.len(),
    );
    let vision_of = |name: Option<&str>, stance: Stance| -> Option<f32> {
        let list = stances
            .get(name?)?
            .stance(stance)?
            .attribute_modifier
            .as_deref()?;
        Some(vision_percent(documents, list))
    };
    let outer_human = guard_outer_human(documents);
    let ranges = weapon_ranges(documents);
    let mut armed = 0usize;
    let mut with_vision = 0usize;
    let mut beyond_weapon = 0usize;
    let mut compared = 0usize;
    for record in holders.iter().filter(|r| r.weapon.is_some()) {
        armed += 1;
        let Some(vision) = record.vision_range else {
            continue;
        };
        with_vision += 1;
        let reach = |stance: Stance| {
            vision
                * (1.0 + vision_of(record.stance_template.as_deref(), stance).unwrap_or(0.0))
                * outer_human
        };
        if let Some(weapon) = record
            .weapon
            .as_deref()
            .and_then(|name| ranges.get(&name.to_ascii_lowercase()))
        {
            compared += 1;
            if reach(Stance::Battle) > *weapon {
                beyond_weapon += 1;
            }
        }
        if armed <= 5 {
            println!(
                "  {} ({}): Battle {:.0}, Aggressive {:.0}, HoldGround {:.0}",
                record.name,
                record.stance_template.as_deref().unwrap_or("-"),
                reach(Stance::Battle),
                reach(Stance::Aggressive),
                reach(Stance::HoldGround),
            );
        }
    }
    println!(
        "  {armed} armed stance holder(s), {with_vision} with a VisionRange; first five reaches \
         above (GuardOuterModifierHuman {outer_human}); {beyond_weapon} of {compared} reach \
         further in Battle than their PRIMARY weapon does"
    );
    bad_tokens.is_empty() && resolved_holders == holders.len() && resolved_lists == blocks
}

fn guard_outer_human(documents: &[mint_sage_ini::Document]) -> f32 {
    for document in documents {
        for block in document.roots() {
            if !block.keyword.eq_ignore_ascii_case("AIData") {
                continue;
            }
            if let Some(value) = block
                .field("GuardOuterModifierHuman")
                .and_then(mint_sage_ini::Field::value)
                .and_then(|v| v.trim().parse::<f32>().ok())
            {
                return value;
            }
        }
    }
    2.2
}

fn weapon_ranges(documents: &[mint_sage_ini::Document]) -> BTreeMap<String, f32> {
    let mut out = BTreeMap::new();
    for document in documents {
        for block in document.roots() {
            if !block.keyword.eq_ignore_ascii_case("Weapon") {
                continue;
            }
            let (Some(name), Some(range)) = (
                block.name(),
                block
                    .field("AttackRange")
                    .and_then(mint_sage_ini::Field::value)
                    .and_then(|v| v.trim().parse::<f32>().ok()),
            ) else {
                continue;
            };
            out.insert(name.to_ascii_lowercase(), range);
        }
    }
    out
}

fn vision_percent(documents: &[mint_sage_ini::Document], list: &str) -> f32 {
    for document in documents {
        for block in document.roots() {
            if !block.keyword.eq_ignore_ascii_case("ModifierList")
                || !block
                    .name()
                    .is_some_and(|name| name.eq_ignore_ascii_case(list))
            {
                continue;
            }
            return block
                .fields_named("Modifier")
                .filter(|field| field.args.first().is_some_and(|k| k == "VISION"))
                .filter_map(|field| {
                    let raw = field.args.get(1)?;
                    raw.trim_end_matches('%')
                        .parse::<f32>()
                        .ok()
                        .map(|v| v / 100.0)
                })
                .sum();
        }
    }
    0.0
}

fn auto_acquire_report(records: &TemplateRecords) {
    let mut members: BTreeSet<String> = BTreeSet::new();
    for record in records.iter() {
        for module in &record.declared_modules {
            for field in module.fields.iter().filter(|f| f.keyword == "RankInfo") {
                for arg in &field.args {
                    if let Some(name) = arg.strip_prefix("UnitType:") {
                        members.insert(name.to_ascii_lowercase());
                    }
                }
            }
        }
    }
    let mut by_value: std::collections::BTreeMap<(bool, bool, &str), usize> =
        std::collections::BTreeMap::new();
    let mut lone_without: Vec<&str> = Vec::new();
    for record in records.iter().filter(|r| r.weapon.is_some()) {
        let member = members.contains(&record.name.to_ascii_lowercase());
        let flags = record.auto_acquire;
        let value = if flags.contains(AutoAcquire::IDLE) {
            if flags.contains(AutoAcquire::ATTACK_BUILDINGS) {
                "Yes ATTACK_BUILDINGS"
            } else {
                "Yes"
            }
        } else if flags == AutoAcquire::default() {
            "unset"
        } else {
            "No"
        };
        *by_value
            .entry((member, record.has_kind("STRUCTURE"), value))
            .or_insert(0) += 1;
        if !member && !flags.contains(AutoAcquire::IDLE) {
            lone_without.push(&record.name);
        }
    }
    println!(
        "AutoAcquireEnemiesWhenIdle over armed templates, (horde member, STRUCTURE, value): count"
    );
    for (key, count) in &by_value {
        println!("  {key:?}: {count}");
    }
    println!(
        "  lone armed without YES ({}): {}",
        lone_without.len(),
        lone_without.join(" ")
    );
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
