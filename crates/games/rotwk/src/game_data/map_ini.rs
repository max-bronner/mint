use std::collections::BTreeMap;
use std::sync::Arc;

use mint_sage_ini::{Block, BlockTable, Document};
use mint_vfs::Vfs;

use super::{horde_formation, GameData, LoadRefusal};

const COMPOSED: [&str; 9] = [
    "Locomotor",
    "Science",
    "SpecialPower",
    "PlayerTemplate",
    "CommandSet",
    "CommandButton",
    "Upgrade",
    "Weapon",
    "Rank",
];

const SINGLETONS: [&str; 3] = ["GameData", "AIData", "SkirmishAIData"];

const PARTICLES: [&str; 2] = ["FXParticleSystem", "ParticleSystem"];

pub(crate) fn compose_map_ini(
    data: &mut GameData,
    vfs: &Vfs,
    map_path: &str,
) -> Result<Option<String>, LoadRefusal> {
    let folder = map_path
        .rsplit_once(['/', '\\'])
        .map_or("", |(folder, _)| folder);
    let path = format!("{folder}/map.ini").to_ascii_lowercase();
    if let Some(done) = &data.map_ini {
        if *done == path {
            return Ok(None);
        }
        return Err(refusal(format!(
            "{path}: this copy of the game data already carries {done}"
        )));
    }
    let Ok(bytes) = vfs.read(&path) else {
        return Ok(None);
    };
    let text = String::from_utf8_lossy(&bytes);
    let document = Document::parse(&path, &text, &BlockTable::corpus_path())
        .map_err(|error| refusal(format!("{path}: {error}")))?;
    compose_document(data, document).map(Some)
}

pub(crate) fn compose_document(
    data: &mut GameData,
    mut document: Document,
) -> Result<String, LoadRefusal> {
    let started = std::time::Instant::now();
    let path = document.name().to_owned();
    if !document.defines().is_empty() {
        return Err(refusals(document.defines().iter().map(|define| {
            format!(
                "{}:{}: #define {}: MACROs not allowed in map.ini",
                define.file, define.line, define.name
            )
        })));
    }
    let macros = (*data.macros).clone();
    document.expand_token_list_macros(&macros);
    let report = mint_sage_ini::strict::check(
        &document,
        &macros,
        mint_sage_ini::tables::field_table::Titles::BOTH,
    );
    if !report.errors.is_empty() {
        return Err(refusals(report.errors.iter().map(ToString::to_string)));
    }
    let globals = Arc::clone(&data.ini_documents);
    let mut lines = compose_stores(data, &document, &globals, &macros)?;
    lines.extend(compose_singletons_and_particles(
        data, &document, &globals, &macros,
    )?);
    let weapons_changed = document
        .roots()
        .iter()
        .any(|block| block.keyword == "Weapon");

    let has_objects = document
        .roots()
        .iter()
        .any(|block| mint_sage_ini::object::OBJECT_KEYWORDS.contains(&block.keyword.as_str()));
    if has_objects || weapons_changed {
        lines.extend(compose_templates(
            data,
            &document,
            &globals,
            &macros,
            weapons_changed,
        )?);
    }
    let mut elsewhere: BTreeMap<&str, usize> = BTreeMap::new();
    for block in document.roots() {
        let keyword = block.keyword.as_str();
        if !COMPOSED.contains(&keyword)
            && !SINGLETONS.contains(&keyword)
            && !PARTICLES.contains(&keyword)
            && !mint_sage_ini::object::OBJECT_KEYWORDS.contains(&keyword)
        {
            let count = elsewhere.entry(keyword).or_default();
            *count = count.saturating_add(1);
        }
    }
    if !elsewhere.is_empty() {
        let kinds: Vec<String> = elsewhere
            .iter()
            .map(|(keyword, count)| format!("{count} {keyword}"))
            .collect();
        lines.push(format!(
            "not composed into the simulation: {}",
            kinds.join(", ")
        ));
    }
    data.map_ini = Some(path.to_ascii_lowercase());
    Ok(format!(
        "map.ini: {path}: {}; composed in {:.0} ms",
        lines.join("; "),
        started.elapsed().as_secs_f64() * 1000.0
    ))
}

fn compose_templates(
    data: &mut GameData,
    document: &Document,
    globals: &[Document],
    macros: &mint_sage_ini::MacroTable,
    weapons_changed: bool,
) -> Result<Option<String>, LoadRefusal> {
    let mut index = mint_sage_modules::TemplateIndex::new();
    for global in globals.iter() {
        index.insert_document(global);
    }
    let load = index.insert_override_document(document);
    if !load.errors.is_empty() {
        return Err(refusals(load.errors));
    }
    let mut templates = (*data.templates).clone();
    let value_errors = templates.reread(&index, &load.touched);
    if !value_errors.is_empty() {
        return Err(refusals(value_errors.iter().map(ToString::to_string)));
    }
    crate::validate::declared_modules_of(
        data,
        load.touched.iter().filter_map(|name| templates.get(name)),
    )
    .map_err(refusals)?;
    let contain_errors = mint_sage_ini::ValueErrors::new();
    let mut formations = data.formations.clone();
    let mut refresh = load.touched.clone();
    if weapons_changed {
        refresh.extend(formations.names.iter().cloned());
    }
    for name in &refresh {
        if let Some((_, Some(entry))) = horde_formation(
            name,
            &index,
            &templates,
            &data.combat,
            macros,
            &contain_errors,
        ) {
            formations.insert(name, entry);
        }
    }
    let contain_errors = contain_errors.take();
    if !contain_errors.is_empty() {
        return Err(refusals(contain_errors.iter().map(ToString::to_string)));
    }
    let mut priorities = (*data.attack_priorities).clone();
    priorities
        .defined_templates
        .extend(load.touched.iter().cloned());
    data.attack_priorities = Arc::new(priorities);
    data.formations = formations;
    data.footprints = Arc::new(mint_sage_passability::TemplateIndex::from_records(
        &templates,
    ));
    let described: Vec<String> = load
        .touched
        .iter()
        .filter_map(|name| templates.get(name))
        .map(|record| {
            format!(
                "{} (CommandSet {}, BuildTime {})",
                record.name,
                record.command_set.as_deref().unwrap_or("-"),
                record
                    .build_time
                    .map_or_else(|| "-".to_owned(), |time| format!("{time}")),
            )
        })
        .collect();
    let line = (!described.is_empty()).then(|| {
        format!(
            "{} object template(s): {}",
            described.len(),
            described.join(", ")
        )
    });
    data.templates = Arc::new(templates);
    Ok(line)
}

fn compose_singletons_and_particles(
    data: &mut GameData,
    document: &Document,
    globals: &[Document],
    macros: &mint_sage_ini::MacroTable,
) -> Result<Vec<String>, LoadRefusal> {
    let mut lines = Vec::new();
    let singletons = document
        .roots()
        .iter()
        .filter(|block| SINGLETONS.contains(&block.keyword.as_str()))
        .count();
    if singletons > 0 {
        super::recompose_singletons(data, globals, document, macros).map_err(refusals)?;
        lines.push(format!(
            "{singletons} GameData/AIData/SkirmishAIData block(s) merged"
        ));
    }
    let mut systems = mint_sage_ini::ParticleIndex::new();
    systems.insert_override_document(globals, document);
    let errors = systems.value_errors().take();
    if !errors.is_empty() {
        return Err(refusals(errors.iter().map(ToString::to_string)));
    }
    if !systems.is_empty() {
        lines.push(format!(
            "{} particle system(s) for the scene",
            systems.len()
        ));
    }
    Ok(lines)
}

fn compose_stores(
    data: &mut GameData,
    document: &Document,
    globals: &[Document],
    macros: &mint_sage_ini::MacroTable,
) -> Result<Vec<String>, LoadRefusal> {
    let wanted: Vec<(&str, String)> = document
        .roots()
        .iter()
        .filter(|block| COMPOSED.contains(&block.keyword.as_str()))
        .filter_map(|block| Some((block.keyword.as_str(), block.name()?.to_ascii_lowercase())))
        .collect();
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    let mut bases: BTreeMap<(&str, String), &Block> = BTreeMap::new();
    for block in globals.iter().flat_map(Document::roots) {
        let Some(name) = block.name() else { continue };
        let key = (block.keyword.as_str(), name.to_ascii_lowercase());
        if wanted.contains(&key) {
            bases.insert(key, block);
        }
    }
    let mut order: Vec<(&str, String)> = Vec::new();
    let mut laid: BTreeMap<(&str, String), Block> = BTreeMap::new();
    let mut overrides = 0usize;
    for block in document.roots() {
        if !COMPOSED.contains(&block.keyword.as_str()) {
            continue;
        }
        let key = (
            block.keyword.as_str(),
            block.name().unwrap_or_default().to_ascii_lowercase(),
        );
        let next = match (laid.get(&key), bases.get(&key)) {
            (Some(earlier), _) => earlier.overlaid(block),
            (None, Some(base)) => {
                overrides = overrides.saturating_add(1);
                base.overlaid(block)
            }
            (None, None) => block.clone(),
        };
        if laid.insert(key.clone(), next).is_none() {
            order.push(key);
        }
    }
    let merged: Vec<Block> = order
        .iter()
        .filter_map(|key| laid.get(key).cloned())
        .collect();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for block in &merged {
        let count = counts.entry(block.keyword.as_str()).or_default();
        *count = count.saturating_add(1);
    }
    let has = |keyword: &str| counts.contains_key(keyword);

    let mut value_errors = Vec::new();
    if has("Science") {
        let mut sciences = (*data.sciences).clone();
        for block in &merged {
            sciences.insert_block(block, macros);
        }
        data.sciences = Arc::new(sciences);
    }
    if has("SpecialPower") {
        let mut powers = (*data.special_powers).clone();
        for block in &merged {
            powers.insert_block(block, macros);
        }
        data.special_powers = Arc::new(powers);
    }
    if has("PlayerTemplate") {
        let mut players = (*data.players).clone();
        for block in &merged {
            players.insert_block(block);
        }
        data.players = Arc::new(players);
    }
    if has("CommandSet") {
        let mut sets = (*data.command_sets).clone();
        for block in &merged {
            sets.insert_block(block);
        }
        data.command_sets = Arc::new(sets);
    }
    if has("CommandButton") {
        let mut buttons = (*data.command_buttons).clone();
        let before = buttons.errors().len();
        for block in &merged {
            buttons.insert_block(block, macros);
        }
        value_errors.extend(buttons.errors().iter().skip(before).cloned());
        data.command_buttons = Arc::new(buttons);
    }
    if has("Upgrade") {
        let mut upgrades = mint_sage_ini::UpgradeIndex::new();
        for global in globals {
            upgrades.insert_document(global);
        }
        for block in &merged {
            upgrades.insert_block(block);
        }
        data.upgrades = Arc::new(mint_sage_modules::build_upgrade_records(&upgrades, macros));
        data.upgrade_bits = Arc::new(mint_sage_modules::UpgradeBits::from_index(&upgrades));
    }
    if has("Locomotor") {
        let mut locomotors = (*data.locomotors).clone();
        let before = locomotors.errors().len();
        for block in &merged {
            locomotors.insert_block(block);
        }
        value_errors.extend(locomotors.errors().iter().skip(before).cloned());
        data.locomotors = Arc::new(locomotors);
    }
    if has("Weapon") {
        for block in merged.iter().filter(|block| block.keyword == "Weapon") {
            data.combat.load_weapon(block, macros);
        }
        value_errors.extend(data.combat.value_errors.take());
    }
    if has("Rank") {
        let blocks = globals
            .iter()
            .flat_map(Document::roots)
            .chain(merged.iter());
        data.ranks = Arc::new(mint_sage_ini::RankTable::build_from_blocks(blocks, macros));
    }
    if !value_errors.is_empty() {
        return Err(refusals(value_errors.iter().map(ToString::to_string)));
    }
    let kinds: Vec<String> = counts
        .iter()
        .map(|(keyword, count)| format!("{count} {keyword}"))
        .collect();
    Ok(vec![format!(
        "{} ({overrides} over a global definition)",
        kinds.join(", ")
    )])
}

fn refusal(error: String) -> LoadRefusal {
    refusals(std::iter::once(error))
}

fn refusals(errors: impl IntoIterator<Item = String>) -> LoadRefusal {
    LoadRefusal {
        errors: errors.into_iter().collect(),
        documents: 1,
    }
}

#[cfg(test)]
mod tests;
