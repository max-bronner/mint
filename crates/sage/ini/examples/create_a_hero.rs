use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_big::BigArchive;
use mint_sage_ini::{
    read_create_a_hero_system, BlockTable, CommandButtonIndex, CommandSetIndex,
    CreateAHeroSystemDef, MacroTable, MappedImageIndex, MemorySources, Reader, Sources,
    UpgradeIndex, ValueErrors, CREATE_A_HERO_SYSTEM_KEYWORD,
};

const INI_ROOT: &str = "data/ini/";
const MAX_LISTED: usize = 10;

struct Tally {
    name: &'static str,
    total: usize,
    missing: Vec<String>,
}

impl Tally {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            total: 0,
            missing: Vec::new(),
        }
    }

    fn check(&mut self, what: &str, resolves: bool) {
        self.total = self.total.saturating_add(1);
        if !resolves {
            self.missing.push(what.to_owned());
        }
    }

    fn report(&self) {
        let ok = self.total.saturating_sub(self.missing.len());
        println!("{}: {ok} of {} resolve", self.name, self.total);
        for what in self.missing.iter().take(MAX_LISTED) {
            println!("  unresolved: {what}");
        }
    }
}

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: create_a_hero <install dir> [<later install dir> ...]");
        return ExitCode::FAILURE;
    }
    let archives: Vec<PathBuf> = roots.iter().flat_map(|root| big_archives(root)).collect();
    let sources = ini_sources(&archives);
    let tops = top_level_paths(&sources);
    let table = BlockTable::corpus_path();
    let reader = Reader::new(&sources);
    let documents: Vec<_> = tops
        .iter()
        .filter_map(|path| reader.read(path, &table).ok())
        .collect();
    println!(
        "{} of {} top-level documents read",
        documents.len(),
        tops.len()
    );

    let mut macros = MacroTable::new();
    for document in &documents {
        macros.insert_defines(document.defines());
    }
    let mut upgrades = UpgradeIndex::new();
    let mut images = MappedImageIndex::new();
    let mut buttons = CommandButtonIndex::new();
    let mut sets = CommandSetIndex::new();
    let errors = ValueErrors::new();
    let mut systems: Vec<CreateAHeroSystemDef> = Vec::new();
    for document in &documents {
        upgrades.insert_document(document);
        images.insert_document(document);
        buttons.insert_document(document, &macros);
        sets.insert_document(document);
        for block in document.roots_named(CREATE_A_HERO_SYSTEM_KEYWORD) {
            systems.push(read_create_a_hero_system(block, &errors, &macros));
        }
    }
    println!(
        "{} CreateAHeroSystem block(s); {} value error(s) there, {} in CommandButton",
        systems.len(),
        errors.len(),
        buttons.errors().len()
    );
    let Some(system) = systems.first() else {
        return ExitCode::FAILURE;
    };
    println!(
        "{} class(es), {} subclass(es), {} bling(s), {} binder(s)",
        system.classes.len(),
        system
            .classes
            .iter()
            .map(|c| c.subclasses.len())
            .sum::<usize>(),
        system.blings.len(),
        system.binders.len()
    );

    let mut upgrade = Tally::new("system, class and subclass upgrades");
    let mut bling_up = Tally::new("bling upgrades (blings, subclass lists, attributes)");
    let mut groups = Tally::new("bling and attribute groups with a binder");
    let mut image = Tally::new("class and subclass images");
    let mut command_set = Tally::new("CommandSetTemplate");
    let binder_groups: BTreeSet<String> = system
        .binders
        .iter()
        .filter_map(|b| b.group.as_deref().map(str::to_ascii_lowercase))
        .collect();
    let has_binder = |group: &Option<String>| {
        group
            .as_deref()
            .is_some_and(|g| binder_groups.contains(&g.to_ascii_lowercase()))
    };
    let up = |name: &str| upgrades.get(name).is_some();
    for name in [
        &system.map_mode_upgrade,
        &system.game_mode_upgrade,
        &system.can_build_upgrade,
    ]
    .into_iter()
    .flatten()
    {
        upgrade.check(name, up(name));
    }
    if let Some(name) = &system.command_set_template {
        command_set.check(name, sets.get(name).is_some());
    }
    for bling in &system.blings {
        groups.check(
            &format!("bling {:?}", bling.upgrade),
            has_binder(&bling.group),
        );
        if let Some(name) = &bling.upgrade {
            bling_up.check(name, up(name));
        }
    }
    for class in &system.classes {
        if let Some(name) = &class.upgrade {
            upgrade.check(name, up(name));
        }
        if let Some(name) = &class.icon_image {
            image.check(name, images.get(name).is_some());
        }
        for sub in &class.subclasses {
            if let Some(name) = &sub.upgrade {
                upgrade.check(name, up(name));
            }
            for name in sub.icon_image.iter().chain(sub.button_image.iter()) {
                image.check(name, images.get(name).is_some());
            }
            for name in sub.bling_upgrades.iter().flat_map(|c| &c.upgrades) {
                bling_up.check(name, up(name));
            }
            for attribute in &sub.attributes {
                groups.check(
                    &format!("attribute {:?}", attribute.group),
                    has_binder(&attribute.group),
                );
                for name in [
                    &attribute.min_upgrade,
                    &attribute.max_upgrade,
                    &attribute.default_upgrade,
                ]
                .into_iter()
                .flatten()
                {
                    bling_up.check(name, up(name));
                }
            }
        }
    }

    let mut allowable = Tally::new("CreateAHeroUIAllowableUpgrades names");
    let mut prerequisite = Tally::new("CreateAHeroUIPrerequisiteButtonName");
    let mut icon = Tally::new("CreateAHeroUIIconImageName");
    let mut cost = Tally::new("CreateAHeroUICostIfSelected values");
    let mut powers = 0usize;
    for (name, def) in buttons.iter() {
        if def.create_a_hero_ui_minimum_level.is_none() {
            continue;
        }
        powers = powers.saturating_add(1);
        for upgrade_name in &def.create_a_hero_ui_allowable_upgrades {
            allowable.check(&format!("{name}: {upgrade_name}"), up(upgrade_name));
        }
        if let Some(button) = &def.create_a_hero_ui_prerequisite_button {
            prerequisite.check(&format!("{name}: {button}"), buttons.get(button).is_some());
        }
        if let Some(image_name) = &def.create_a_hero_ui_icon_image {
            icon.check(
                &format!("{name}: {image_name}"),
                images.get(image_name).is_some(),
            );
        }
        cost.check(name, def.create_a_hero_ui_cost_if_selected.is_some());
    }
    println!("{powers} button(s) carry CreateAHeroUIMinimumLevel");
    for tally in [
        &upgrade,
        &bling_up,
        &groups,
        &image,
        &command_set,
        &allowable,
        &prerequisite,
        &icon,
        &cost,
    ] {
        tally.report();
    }
    ExitCode::SUCCESS
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

fn ini_sources(archives: &[PathBuf]) -> MemorySources {
    let mut sources = MemorySources::new();
    for path in archives {
        let Ok(archive) = BigArchive::open(path) else {
            continue;
        };
        for entry in archive.entries() {
            if !entry.path.as_str().starts_with(INI_ROOT) {
                continue;
            }
            let Ok(bytes) = archive.read_entry(entry) else {
                continue;
            };
            sources.insert(entry.path.as_str(), bytes);
        }
    }
    sources
}

fn top_level_paths(sources: &MemorySources) -> Vec<String> {
    let paths: Vec<String> = sources.paths().map(str::to_owned).collect();
    let mut included: BTreeSet<String> = BTreeSet::new();
    for path in &paths {
        let Some(bytes) = Sources::read(sources, path) else {
            continue;
        };
        for item in mint_sage_ini::lex::lex(&mint_sage_ini::lex::decode_latin1(&bytes)) {
            if let mint_sage_ini::lex::Lexed::Directive {
                directive: mint_sage_ini::lex::Directive::Include(target),
                ..
            } = item
            {
                included.insert(mint_sage_ini::resolve_include(path, target));
            }
        }
    }
    paths
        .into_iter()
        .filter(|path| !included.contains(path))
        .collect()
}
