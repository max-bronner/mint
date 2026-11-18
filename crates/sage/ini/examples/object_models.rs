use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_sage_ini::lex::{decode_latin1, lex, Directive, Lexed};
use mint_sage_ini::object::{
    first_model_state, model_naming, ModelNaming, ANIMATION_KEYWORD, IDLE_ANIMATION_STATE_KEYWORD,
};
use mint_sage_ini::{
    resolve_include, AnimationMode, AnimationSource, Block, BlockTable, Diagnostic, Document,
    MemorySources, NoModelReason, ObjectIndex, ParticleIndex, ParticleKind, ParticleSystemDef,
    Reader, Template, PARTICLE_DOCUMENTS,
};
use mint_sage_map::MapFile;

const INI_ROOT: &str = "data/ini/";

const W3D_DIR: &str = "art/w3d/";
const MOLDS_DIR: &str = "data/editor/molds/";
const PREFIX_LEN: usize = 2;

#[derive(Default)]
struct BoneTally {
    templates: usize,
    default_state: usize,
    idle_state: usize,
    conditioned: usize,
    follow_bone: usize,
    bone_none: usize,
    named: BTreeSet<String>,
    named_default: BTreeSet<String>,
    dangling: BTreeMap<String, usize>,
    seen: usize,
    seen_by_scope: BTreeMap<String, usize>,
}

impl BoneTally {
    fn walk(&mut self, root: &Block, scope: &str) {
        for field in root.fields() {
            if field
                .keyword
                .eq_ignore_ascii_case(mint_sage_ini::object::PARTICLE_SYS_BONE_KEYWORD)
            {
                self.seen = self.seen.saturating_add(1);
                *self.seen_by_scope.entry(scope.to_owned()).or_default() += 1;
            }
        }
        for child in root.blocks() {
            self.walk(child, &child.keyword);
        }
    }

    fn tally(&mut self, index: &ObjectIndex) {
        for (_, bones) in index.particle_bone_declarations() {
            self.templates = self.templates.saturating_add(1);
            for bone in &bones.default {
                self.default_state = self.default_state.saturating_add(1);
                self.follow_bone = self
                    .follow_bone
                    .saturating_add(usize::from(bone.follow_bone));
                self.bone_none = self
                    .bone_none
                    .saturating_add(usize::from(bone.bone.is_none()));
                self.named.insert(bone.system.to_ascii_lowercase());
                self.named_default.insert(bone.system.to_ascii_lowercase());
            }
            for bone in &bones.idle {
                self.idle_state = self.idle_state.saturating_add(1);
                self.follow_bone = self
                    .follow_bone
                    .saturating_add(usize::from(bone.follow_bone));
                self.bone_none = self
                    .bone_none
                    .saturating_add(usize::from(bone.bone.is_none()));
                self.named.insert(bone.system.to_ascii_lowercase());
                self.named_default.insert(bone.system.to_ascii_lowercase());
            }
            for bone in &bones.conditioned {
                self.conditioned = self.conditioned.saturating_add(1);
                self.named.insert(bone.system.to_ascii_lowercase());
            }
        }
    }

    fn resolve(&mut self, index: &ObjectIndex, systems: &ParticleIndex) -> (usize, usize) {
        let mut total = 0usize;
        let mut resolved = 0usize;
        for (template, bones) in index.particle_bone_declarations() {
            for bone in bones.all() {
                total = total.saturating_add(1);
                if systems.get(&bone.system).is_some() {
                    resolved = resolved.saturating_add(1);
                } else {
                    *self
                        .dangling
                        .entry(format!("{} named by {template}", bone.system))
                        .or_default() += 1;
                }
            }
        }
        (resolved, total)
    }
}

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: object_models <install dir> [<install dir> ...]");
        eprintln!("       mount both installs, ROTWK last - see the module docs");
        return ExitCode::FAILURE;
    }

    let started = Instant::now();

    let mut installs: Vec<Install> = Vec::new();
    let mut every_entry: BTreeSet<String> = BTreeSet::new();
    for root in &roots {
        let install = Install::open(root);
        for archive in &install.archives {
            for entry in archive.entries() {
                every_entry.insert(entry.path.as_str().to_owned());
            }
        }
        installs.push(install);
    }

    let maps = scan_maps(&installs);
    println!("== maps ==");
    println!(
        "{} .map/.bse files read, {} failed",
        maps.files, maps.failures
    );
    println!(
        "{} placements, {} distinct template_name values",
        maps.placements,
        maps.templates.len()
    );
    compare("map files", maps.files, 440);
    compare("placements", maps.placements, 357_185);
    compare(
        "distinct template names",
        maps.templates.len() as u64,
        1_991,
    );

    let mut failed = false;
    for install in &installs {
        let report = run_install(install, &maps, &every_entry);
        failed |= report;
    }

    let elapsed = started.elapsed().as_secs_f64();
    println!("\n== throughput ==");
    println!("{elapsed:.2} s total");

    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

struct Install {
    label: String,
    archives: Vec<BigArchive>,
}

impl Install {
    fn open(root: &Path) -> Self {
        let label = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string());
        let mut archives = Vec::new();
        for path in big_archives(root) {
            if let Ok(archive) = BigArchive::open(&path) {
                archives.push(archive);
            }
        }
        Self { label, archives }
    }

    fn ini_sources(&self) -> (MemorySources, usize) {
        let mut sources = MemorySources::new();
        let mut entries = 0usize;
        for archive in &self.archives {
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
        (sources, entries)
    }
}

#[derive(Default)]
struct Maps {
    files: u64,
    failures: u64,
    placements: u64,
    templates: BTreeMap<String, u64>,
    per_map: BTreeMap<String, BTreeMap<String, u64>>,
}

const STEP_LABELS: [&str; 10] = [
    "GPU (step 12)",
    "DRAWABLE (step 13)",
    "STREAK (step 13)",
    "VOLUME_PARTICLE (13)",
    "SMUDGE (step 13)",
    "SlaveSystem (step 14)",
    "PerParticle (step 14)",
    "TerrainCollision (14)",
    "Wind (step 11)",
    "SortLevel > 1 (8)",
];

fn placed_kinds(definition: &ParticleSystemDef) -> Vec<&'static str> {
    let mut out = Vec::new();
    let module = definition.draw_module.as_deref().unwrap_or_default();
    if module.eq_ignore_ascii_case("GpuDraw") {
        out.push("GPU (step 12)");
    }
    if module.eq_ignore_ascii_case("RenderObjectDraw") {
        out.push("DRAWABLE (step 13)");
    }
    if module.eq_ignore_ascii_case("StreakDraw") || module.eq_ignore_ascii_case("LightningDraw") {
        out.push("STREAK (step 13)");
    }
    if definition.kind == ParticleKind::VolumeParticle {
        out.push("VOLUME_PARTICLE (13)");
    }
    if definition.kind == ParticleKind::Smudge {
        out.push("SMUDGE (step 13)");
    }
    if definition.slave_system.is_some() {
        out.push("SlaveSystem (step 14)");
    }
    if definition.per_particle_system.is_some() {
        out.push("PerParticle (step 14)");
    }
    if definition.event.is_some() {
        out.push("TerrainCollision (14)");
    }
    if definition.wind.is_some_and(|wind| wind.motion.moves()) {
        out.push("Wind (step 11)");
    }
    if definition.sort_level.is_some_and(|level| level > 1) {
        out.push("SortLevel > 1 (8)");
    }
    out
}

fn scan_maps(installs: &[Install]) -> Maps {
    let mut maps = Maps::default();
    for install in installs {
        for archive in &install.archives {
            for entry in archive.entries() {
                let name = entry.path.as_str();
                if !(name.ends_with(".map") || name.ends_with(".bse")) {
                    continue;
                }
                maps.files += 1;
                let Ok(raw) = archive.read_entry(entry) else {
                    maps.failures += 1;
                    continue;
                };
                match MapFile::parse(&raw) {
                    Ok(map) => {
                        let per = maps.per_map.entry(name.to_owned()).or_default();
                        for object in &map.objects {
                            maps.placements += 1;
                            *per.entry(object.template_name.to_ascii_lowercase())
                                .or_default() += 1;
                        }
                    }
                    Err(_) => maps.failures += 1,
                }
            }
        }
    }
    for templates in maps.per_map.values() {
        for (template, count) in templates {
            *maps.templates.entry(template.clone()).or_default() += count;
        }
    }
    maps
}

#[allow(clippy::too_many_lines)]
fn run_install(install: &Install, maps: &Maps, every_entry: &BTreeSet<String>) -> bool {
    println!("\n== {} ==", install.label);
    let (sources, entries) = install.ini_sources();
    println!(
        "{entries} entries under {INI_ROOT} ({} distinct)",
        sources.len()
    );

    let mut directives = 0u64;
    let mut resolved = 0u64;
    let mut defines = 0u64;
    let mut unresolved: Vec<String> = Vec::new();
    let mut included: BTreeSet<String> = BTreeSet::new();
    let paths: Vec<String> = sources.paths().map(str::to_owned).collect();
    for path in &paths {
        let Some(bytes) = mint_sage_ini::Sources::read(&sources, path) else {
            continue;
        };
        for item in lex(&decode_latin1(&bytes)) {
            let Lexed::Directive { number, directive } = item else {
                continue;
            };
            match directive {
                Directive::Define { .. } => defines += 1,
                Directive::Include(target) => {
                    directives += 1;
                    let target = resolve_include(path, target);
                    if mint_sage_ini::Sources::read(&sources, &target).is_some() {
                        resolved += 1;
                        included.insert(target);
                    } else {
                        unresolved.push(format!("{path}:{number}: {target}"));
                    }
                }
                Directive::Other(_) => {}
            }
        }
    }
    println!(
        "#include: {resolved} of {directives} resolve; #define: {defines} recorded, 0 expanded"
    );
    for line in unresolved.iter().take(20) {
        println!("  unresolved: {line}");
    }

    let top_level: Vec<&String> = paths.iter().filter(|p| !included.contains(*p)).collect();
    println!(
        "{} pulled in by an #include, {} top-level documents",
        included.len(),
        top_level.len()
    );

    let table = BlockTable::corpus_path();
    let reader = Reader::new(&sources);
    let mut parsed = 0u64;
    let mut balanced = 0u64;
    let mut stray_ends = 0u64;
    let mut unclosed = 0u64;
    let mut statements = 0u64;
    let mut unbalanced_docs: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut index = ObjectIndex::new();
    let mut bones_seen = BoneTally::default();
    let mut draws = Draws::default();
    let mut anims = Anims::default();
    let mut kinds: BTreeMap<String, Kind> = BTreeMap::new();

    for path in &top_level {
        match reader.read(path, &table) {
            Ok(doc) => {
                parsed += 1;
                statements += doc.statements() as u64;
                let (stray, open) = imbalance(&doc);
                stray_ends += stray;
                unclosed += open;
                if stray == 0 && open == 0 {
                    balanced += 1;
                } else {
                    let first = doc
                        .diagnostics()
                        .iter()
                        .find(|d| {
                            matches!(
                                d,
                                Diagnostic::StrayEnd { .. } | Diagnostic::UnclosedBlock { .. }
                            )
                        })
                        .map_or_else(String::new, ToString::to_string);
                    unbalanced_docs.push(format!(
                        "{path} ({stray} stray End, {open} unclosed) {first}"
                    ));
                }
                for root in doc.roots() {
                    draws.tally(root);
                    anims.tally(root);
                    Kind::tally(root, &mut kinds);
                    bones_seen.walk(root, &root.keyword);
                }
                index.insert_document(&doc);
            }
            Err(e) => errors.push(format!("{path}: {e}")),
        }
    }

    println!("\n-- parse --");
    println!(
        "{parsed} of {} documents read, {} errored",
        top_level.len(),
        errors.len()
    );
    for line in &errors {
        println!("  ERROR {line}");
    }
    println!("{statements} statements after comment stripping and include resolution");
    println!(
        "{balanced} of {parsed} documents balance; {stray_ends} stray End, {unclosed} unclosed, \
         {} documents affected",
        unbalanced_docs.len()
    );
    for line in &unbalanced_docs {
        println!("  unbalanced: {line}");
    }

    println!("\n-- cross-checks --");
    println!(
        "{} draw modules: {} name a model directly, {} through a condition state, \
         {} name none, {} do BOTH",
        draws.total, draws.by_model_name, draws.by_state, draws.neither, draws.both
    );
    if draws.both > 0 {
        println!("  FINDING: the research measures 0 of 4,994 modules using both mechanisms");
    }
    println!(
        "{} of {} draw modules with a model condition state list an unconditional one first",
        draws.first_state_unconditional, draws.with_state
    );
    println!("draw modules by type, and how each names its model:");
    for ((kind, naming), count) in &draws.by_type {
        println!("  {count:>5}  {kind:<24} {naming:?}");
    }
    println!(
        "{} object definitions, {} roads, {} terrain types, {} redefinitions",
        index.objects(),
        index.roads(),
        index.terrains(),
        index.redefinitions()
    );

    let mut systems = ParticleIndex::new();
    let particle_table = BlockTable::particle_path();
    for path in PARTICLE_DOCUMENTS {
        if sources.paths().all(|held| held != path) {
            continue;
        }
        if let Ok(document) = reader.read(path, &particle_table) {
            systems.insert_document(&document);
        }
    }
    let mut bones = bones_seen;
    bones.tally(&index);
    let (bones_resolved, bones_total) = bones.resolve(&index, &systems);

    println!("\n-- ParticleSysBone (step 7) --");
    println!(
        "{} leaf(s) in the parsed tree, {bones_total} attributed to a template",
        bones.seen
    );
    if bones.seen != bones_total {
        for (scope, count) in &bones.seen_by_scope {
            println!("    {count:>6}  under {scope}");
        }
    }
    println!(
        "{bones_total} attachment(s) over {} template(s): {} in an unconditional model \
         condition state, {} in an IdleAnimationState, {} in a conditioned or transient one",
        bones.templates, bones.default_state, bones.idle_state, bones.conditioned
    );
    println!(
        "  {} distinct system(s) named, {} of them from an unconditional state",
        bones.named.len(),
        bones.named_default.len()
    );
    println!(
        "  of the unconditional ones, {} write FollowBone and {} write the bone NONE",
        bones.follow_bone, bones.bone_none
    );
    println!(
        "  {bones_resolved} of {bones_total} name a defined particle system; \
         {} distinct dangling name(s)",
        bones.dangling.len()
    );
    for (name, uses) in &bones.dangling {
        println!("    dangling {name} ({uses} line(s))");
    }
    let mut map_attachments = 0u64;
    let mut map_placements = 0u64;
    let mut map_templates = 0usize;
    for (name, placements) in &maps.templates {
        let Some(reachable) = index.particle_bones(name) else {
            continue;
        };
        let plays = reachable.default.len().saturating_add(reachable.idle.len());
        if plays == 0 {
            continue;
        }
        map_templates += 1;
        map_placements += *placements;
        map_attachments += (plays as u64).saturating_mul(*placements);
    }
    println!(
        "over every shipped map: {map_templates} placed template(s) carry an unconditional \
         attachment, {map_placements} placement(s), {map_attachments} live system(s) if every \
         one were played at once"
    );
    let mut per_map: Vec<(u64, u64, &str)> = maps
        .per_map
        .iter()
        .map(|(path, templates)| {
            let mut systems = 0u64;
            let mut placements = 0u64;
            for (name, count) in templates {
                let Some(bones) = index.particle_bones(name) else {
                    continue;
                };
                let plays = bones.default.len().saturating_add(bones.idle.len()) as u64;
                if plays == 0 {
                    continue;
                }
                placements += *count;
                systems += plays.saturating_mul(*count);
            }
            (systems, placements, path.as_str())
        })
        .filter(|(systems, _, _)| *systems > 0)
        .collect();
    per_map.sort_by(|a, b| b.0.cmp(&a.0));
    println!("  the ten densest maps, by systems the default rung would start:");
    for (systems, placements, path) in per_map.iter().take(10) {
        println!("    {systems:>5} system(s) over {placements:>4} placement(s)  {path}");
    }

    for rung in ["default", "all"] {
        let mut kind_counts: BTreeMap<&'static str, u64> = BTreeMap::new();
        let mut kind_maps: BTreeMap<&'static str, BTreeMap<&str, u64>> = BTreeMap::new();
        let mut kind_names: BTreeMap<&'static str, BTreeSet<String>> = BTreeMap::new();
        for (path, templates) in &maps.per_map {
            for (name, count) in templates {
                let Some(reachable) = index.particle_bones(name) else {
                    continue;
                };
                let standing = reachable.default.iter().chain(reachable.idle.iter());
                let every = standing.clone().chain(reachable.conditioned.iter());
                let bones: Box<dyn Iterator<Item = _>> = if rung == "all" {
                    Box::new(every)
                } else {
                    Box::new(standing)
                };
                for bone in bones {
                    let Some(definition) = systems.get(&bone.system) else {
                        continue;
                    };
                    for label in placed_kinds(definition) {
                        *kind_counts.entry(label).or_default() += count;
                        *kind_maps.entry(label).or_default().entry(path).or_default() += count;
                        kind_names
                            .entry(label)
                            .or_default()
                            .insert(definition.name.clone());
                    }
                }
            }
        }
        println!("  on the {rung} rung, the ones steps 12, 13 and 14 are about:");
        for label in STEP_LABELS {
            let Some(count) = kind_counts.get(label) else {
                println!("    {label:<24}     0 - nothing to judge but the debug spawn");
                continue;
            };
            let empty = BTreeMap::new();
            let per = kind_maps.get(label).unwrap_or(&empty);
            let mut best: Vec<(&&str, &u64)> = per.iter().collect();
            best.sort_by(|a, b| b.1.cmp(a.1));
            let names = kind_names.get(label).map_or(0, BTreeSet::len);
            println!(
                "    {label:<24} {count:>5} system(s), {names} name(s), {} map(s); densest {}",
                per.len(),
                match best.first() {
                    Some((path, count)) => format!("{count} on {path}"),
                    None => "none".to_owned(),
                }
            );
            for name in kind_names.get(label).into_iter().flatten().take(8) {
                println!("        {name}");
            }
        }
    }

    let mut answers = Answers::default();
    for (name, placements) in &maps.templates {
        answers.count(index.lookup(name), *placements);
        if index.lookup(name).model().is_some() && index.lookup_local(name).model().is_none() {
            answers.inherited += 1;
            answers.inherited_placements += placements;
        }
    }

    println!("\n-- template_name -> model --");
    println!(
        "{} of {} map template names have a definition, covering {} placements ({:.1}%)",
        answers.defined,
        maps.templates.len(),
        answers.defined_placements,
        percent(answers.defined_placements, maps.placements)
    );
    println!(
        "{} yield a model, covering {} placements ({:.1}%)",
        answers.with_model,
        answers.model_placements,
        percent(answers.model_placements, maps.placements)
    );
    println!(
        "{} draw nothing on purpose: {} Model = NONE / no state, {} non-drawing module, \
         {} no Draw at all",
        answers.no_model, answers.model_is_none, answers.non_drawing, answers.no_draw
    );
    println!(
        "  their placements: {} + {} + {}",
        answers.model_is_none_placements,
        answers.non_drawing_placements,
        answers.no_draw_placements
    );
    println!(
        "{} are Road ({} placements), {} are Terrain ({} placements), {} declared nowhere \
         ({} placements)",
        answers.road,
        answers.road_placements,
        answers.terrain,
        answers.terrain_placements,
        answers.undeclared,
        answers.undeclared_placements
    );
    println!(
        "{} names get their model only through a ChildObject parent ({} placements)",
        answers.inherited, answers.inherited_placements
    );
    let shadowed = maps
        .templates
        .keys()
        .filter(|name| {
            index.lookup(name).is_object()
                && (index.declared_as_road(name) || index.declared_as_terrain(name))
        })
        .count();
    println!("{shadowed} of them are also declared as a Road or a Terrain type; the object wins");

    let through_state: Vec<&str> = index
        .names()
        .filter(|name| {
            index.lookup(name).model().is_none()
                && index
                    .model_states(name)
                    .iter()
                    .any(|state| state.model.is_some())
        })
        .collect();
    println!(
        "{} templates draw their body only under a model state (e.g. {})",
        through_state.len(),
        through_state
            .iter()
            .filter(|name| name.contains("fortress"))
            .take(4)
            .copied()
            .collect::<Vec<_>>()
            .join(", ")
    );

    let mut shared: BTreeMap<String, u64> = BTreeMap::new();
    let mut as_written: BTreeSet<String> = BTreeSet::new();
    for name in maps.templates.keys() {
        if let Some(model) = index.lookup(name).model() {
            *shared.entry(model.to_ascii_lowercase()).or_default() += 1;
            as_written.insert(model.to_owned());
        }
    }
    let names: BTreeSet<String> = shared.keys().cloned().collect();
    let mut most: Vec<(&String, &u64)> = shared.iter().collect();
    most.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    println!(
        "\n{} distinct model names ({} counting case as written) for {} templates; \
         most-shared: {}",
        names.len(),
        as_written.len(),
        answers.with_model,
        most.iter()
            .take(5)
            .map(|(n, c)| format!("{n} x{c}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "
-- animation states --"
    );
    println!(
        "{} IdleAnimationState headers, {} of them carrying a condition",
        anims.idle_states, anims.idle_states_conditioned
    );
    if anims.idle_states_conditioned > 0 {
        println!(
            "  FINDING: an IdleAnimationState with a condition is not something              ROTWK's data/ini contains; the default-animation rule assumes there are none"
        );
    }
    println!(
        "{} AnimationState headers, {} of them unconditional; {} TransitionState",
        anims.anim_states, anims.anim_states_unconditional, anims.transition_states
    );
    println!(
        "Animation blocks: {} in model condition states, {} in idle states,          {} in other animation/transition states",
        anims.animation_in_model_state, anims.animation_in_idle_state, anims.animation_elsewhere
    );
    println!(
        "{} AnimationName leaves, {} of them empty; {} carry a HIERARCHY. prefix;          {} blocks carry an AnimationBlendTime",
        anims.animation_names,
        anims.animation_names_empty,
        anims.animation_names_qualified,
        anims.blend_times
    );
    println!("AnimationMode values, over every Animation block reached:");
    for (value, count) in &anims.modes {
        let known = match value.as_str() {
            "<ABSENT>" => "  (the block names no mode)",
            _ if AnimationMode::parse(value).is_some() => "",
            _ => "  <- not one of EA's six spellings",
        };
        println!("  {count:>6}  {value}{known}");
    }
    for (name, _) in AnimationMode::NAMES {
        if !anims.modes.contains_key(name) {
            println!("  {:>6}  {name}  (never used in a model draw module)", 0);
        }
    }

    println!(
        "
-- default animation --"
    );
    let declared: Vec<_> = index.animations().collect();
    let mut by_source: BTreeMap<AnimationSource, u64> = BTreeMap::new();
    let mut by_mode: BTreeMap<String, u64> = BTreeMap::new();
    for (_, animation) in &declared {
        *by_source.entry(animation.source).or_default() += 1;
        let mode = animation
            .mode
            .map_or_else(|| "<none>".to_owned(), |m| m.as_str().to_owned());
        *by_mode.entry(mode).or_default() += 1;
    }
    println!(
        "{} of {} object definitions declare a default animation in their own body",
        declared.len(),
        index.len()
    );
    for (source, count) in &by_source {
        println!("  {count:>5}  from {source:?}");
    }
    for (mode, count) in &by_mode {
        println!("  {count:>5}  AnimationMode = {mode}");
    }

    let mut with_animation = 0u64;
    let mut animated_placements = 0u64;
    let mut inherited_animation = 0u64;
    let mut assets: BTreeSet<String> = BTreeSet::new();
    let mut tree_or_shrub = 0u64;
    let mut tree_or_shrub_animated = 0u64;
    for (name, placements) in &maps.templates {
        if Kind::is_tree_or_shrub(&kinds, name) {
            tree_or_shrub += 1;
        }
        let Some(animation) = index.animation(name) else {
            continue;
        };
        with_animation += 1;
        animated_placements += placements;
        assets.insert(animation.asset().to_ascii_lowercase());
        if index.animation_local(name).is_none() {
            inherited_animation += 1;
        }
        if Kind::is_tree_or_shrub(&kinds, name) {
            tree_or_shrub_animated += 1;
        }
    }
    println!(
        "{with_animation} of {} map template names resolve to a default animation,          covering {animated_placements} placements ({:.1}%)",
        maps.templates.len(),
        percent(animated_placements, maps.placements)
    );
    println!("{inherited_animation} of those get it only through a ChildObject parent");
    println!(
        "{tree_or_shrub} map template names are a tree or a shrub by KindOf;          {tree_or_shrub_animated} of them declare a default animation"
    );
    let trees: Vec<&String> = kinds
        .keys()
        .filter(|name| Kind::is_tree_or_shrub(&kinds, name))
        .collect();
    let declared_trees = trees.len();
    let declared_trees_animated = trees
        .iter()
        .filter(|name| index.animation(name).is_some())
        .count();
    println!(
        "over every definition, not just the placed ones: {declared_trees} are a tree or          a shrub, {declared_trees_animated} of them with a default animation"
    );

    let mut anim_found = 0u64;
    let mut anim_missing: Vec<String> = Vec::new();
    let unexpanded = assets.iter().filter(|a| a.contains("#(")).count();
    for asset in &assets {
        let (primary, molds) = w3d_candidates(asset);
        if every_entry.contains(&primary) || molds.as_ref().is_some_and(|m| every_entry.contains(m))
        {
            anim_found += 1;
        } else {
            anim_missing.push(asset.clone());
        }
    }
    println!(
        "{anim_found} of {} distinct animation assets resolve to a shipped .w3d;          {unexpanded} of the rest are an unexpanded #(MACRO) rather than a missing file",
        assets.len()
    );
    for asset in anim_missing.iter().take(40) {
        println!("  no .w3d: {asset}");
    }
    if anim_missing.len() > 40 {
        println!("  ... and {} more", anim_missing.len() - 40);
    }

    let mut found = 0u64;
    let mut via_molds = 0u64;
    let mut missing: Vec<String> = Vec::new();
    for model in &names {
        let (primary, molds) = w3d_candidates(model);
        if every_entry.contains(&primary) {
            found += 1;
        } else if molds.as_ref().is_some_and(|m| every_entry.contains(m)) {
            found += 1;
            via_molds += 1;
        } else {
            missing.push(model.clone());
        }
    }
    println!("\n-- model name to file --");
    println!(
        "{found} of {} distinct default-model names resolve to a shipped .w3d ({via_molds} \
         through data/editor/molds)",
        names.len()
    );
    for model in &missing {
        println!("  no .w3d: {model}");
    }

    !errors.is_empty()
}

#[derive(Default)]
struct Draws {
    total: u64,
    by_model_name: u64,
    by_state: u64,
    neither: u64,
    both: u64,
    with_state: u64,
    first_state_unconditional: u64,
    by_type: BTreeMap<(String, ModelNaming), u64>,
}

impl Draws {
    fn tally(&mut self, root: &Block) {
        if !is_object(root) {
            return;
        }
        for draw in root.blocks_named("Draw") {
            self.total += 1;
            let naming = model_naming(draw);
            match naming {
                ModelNaming::ModelName => self.by_model_name += 1,
                ModelNaming::ConditionState => self.by_state += 1,
                ModelNaming::Neither => self.neither += 1,
                ModelNaming::Both => self.both += 1,
            }
            let kind = draw.selector().unwrap_or("<untyped>").to_owned();
            *self.by_type.entry((kind, naming)).or_default() += 1;

            if let Some(state) = first_model_state(draw) {
                self.with_state += 1;
                let unconditional = state
                    .keyword
                    .eq_ignore_ascii_case("DefaultModelConditionState")
                    || state.args.iter().all(|a| a.eq_ignore_ascii_case("NONE"))
                        && !state.args.is_empty();
                if unconditional {
                    self.first_state_unconditional += 1;
                }
            }
        }
    }
}

#[derive(Default)]
struct Anims {
    idle_states: u64,
    idle_states_conditioned: u64,
    anim_states: u64,
    anim_states_unconditional: u64,
    transition_states: u64,
    animation_in_model_state: u64,
    animation_in_idle_state: u64,
    animation_elsewhere: u64,
    animation_names: u64,
    animation_names_empty: u64,
    animation_names_qualified: u64,
    blend_times: u64,
    modes: BTreeMap<String, u64>,
}

impl Anims {
    fn tally(&mut self, root: &Block) {
        if !is_object(root) {
            return;
        }
        for draw in root.blocks_named("Draw") {
            for state in draw.blocks() {
                let keyword = state.keyword.as_str();
                let animations = state.blocks_named(ANIMATION_KEYWORD).count() as u64;
                if keyword.eq_ignore_ascii_case(IDLE_ANIMATION_STATE_KEYWORD) {
                    self.idle_states += 1;
                    if !state.args.is_empty() {
                        self.idle_states_conditioned += 1;
                    }
                    self.animation_in_idle_state += animations;
                } else if keyword.eq_ignore_ascii_case("AnimationState") {
                    self.anim_states += 1;
                    if state.args.is_empty() {
                        self.anim_states_unconditional += 1;
                    }
                    self.animation_elsewhere += animations;
                } else if keyword.eq_ignore_ascii_case("TransitionState") {
                    self.transition_states += 1;
                    self.animation_elsewhere += animations;
                } else if keyword.eq_ignore_ascii_case("DefaultModelConditionState")
                    || keyword.eq_ignore_ascii_case("ModelConditionState")
                {
                    self.animation_in_model_state += animations;
                }
                for animation in state.blocks_named(ANIMATION_KEYWORD) {
                    match animation.field("AnimationName").and_then(|f| f.value()) {
                        None => self.animation_names_empty += 1,
                        Some(name) => {
                            self.animation_names += 1;
                            if name.contains('.') {
                                self.animation_names_qualified += 1;
                            }
                        }
                    }
                    if animation.field("AnimationBlendTime").is_some() {
                        self.blend_times += 1;
                    }
                    let mode = animation
                        .field("AnimationMode")
                        .and_then(|f| f.value())
                        .unwrap_or("<absent>")
                        .to_ascii_uppercase();
                    *self.modes.entry(mode).or_default() += 1;
                }
            }
        }
    }
}

struct Kind {
    tree_or_shrub: bool,
    parent: Option<String>,
    stated: bool,
}

const TREE_FLAGS: [&str; 3] = ["TREE", "SHRUB", "SHRUBBERY"];

impl Kind {
    fn tally(root: &Block, into: &mut BTreeMap<String, Kind>) {
        if !is_object(root) {
            return;
        }
        let Some(name) = root.name() else {
            return;
        };
        let parent = if root.keyword.eq_ignore_ascii_case("Object") {
            None
        } else {
            root.args.get(1).map(|p| p.to_ascii_lowercase())
        };
        let mut stated = false;
        let mut tree_or_shrub = false;
        for field in root.fields_named("KindOf") {
            stated = true;
            for flag in &field.args {
                let flag = flag.trim_start_matches(['+', '-']);
                if TREE_FLAGS.iter().any(|t| flag.eq_ignore_ascii_case(t)) {
                    tree_or_shrub = true;
                }
            }
        }
        into.insert(
            name.to_ascii_lowercase(),
            Kind {
                tree_or_shrub,
                parent,
                stated,
            },
        );
    }

    fn is_tree_or_shrub(kinds: &BTreeMap<String, Kind>, name: &str) -> bool {
        let mut key = name.to_ascii_lowercase();
        for _ in 0..8 {
            let Some(kind) = kinds.get(&key) else {
                return false;
            };
            if kind.stated {
                return kind.tree_or_shrub;
            }
            let Some(parent) = &kind.parent else {
                return false;
            };
            key.clone_from(parent);
        }
        false
    }
}

fn is_object(root: &Block) -> bool {
    ["object", "childobject", "objectreskin"]
        .iter()
        .any(|k| root.keyword.eq_ignore_ascii_case(k))
}

#[derive(Default)]
struct Answers {
    defined: u64,
    defined_placements: u64,
    with_model: u64,
    model_placements: u64,
    no_model: u64,
    model_is_none: u64,
    model_is_none_placements: u64,
    non_drawing: u64,
    non_drawing_placements: u64,
    no_draw: u64,
    no_draw_placements: u64,
    road: u64,
    road_placements: u64,
    terrain: u64,
    terrain_placements: u64,
    undeclared: u64,
    undeclared_placements: u64,
    inherited: u64,
    inherited_placements: u64,
}

impl Answers {
    fn count(&mut self, template: Template<'_>, placements: u64) {
        if template.is_object() {
            self.defined += 1;
            self.defined_placements += placements;
        }
        match template {
            Template::Model(_) => {
                self.with_model += 1;
                self.model_placements += placements;
            }
            Template::NoModel(reason) => {
                self.no_model += 1;
                match reason {
                    NoModelReason::ModelIsNone => {
                        self.model_is_none += 1;
                        self.model_is_none_placements += placements;
                    }
                    NoModelReason::NonDrawingModule => {
                        self.non_drawing += 1;
                        self.non_drawing_placements += placements;
                    }
                    NoModelReason::NoDrawModule => {
                        self.no_draw += 1;
                        self.no_draw_placements += placements;
                    }
                }
            }
            Template::Road => {
                self.road += 1;
                self.road_placements += placements;
            }
            Template::Terrain => {
                self.terrain += 1;
                self.terrain_placements += placements;
            }
            Template::Undeclared => {
                self.undeclared += 1;
                self.undeclared_placements += placements;
            }
        }
    }
}

fn imbalance(doc: &Document) -> (u64, u64) {
    let mut stray = 0u64;
    let mut unclosed = 0u64;
    for diagnostic in doc.diagnostics() {
        match diagnostic {
            Diagnostic::StrayEnd { .. } => stray += 1,
            Diagnostic::UnclosedBlock { .. } => unclosed += 1,
            _ => {}
        }
    }
    (stray, unclosed)
}

fn w3d_candidates(stem: &str) -> (String, Option<String>) {
    let primary = match stem.get(..PREFIX_LEN) {
        Some(prefix) => format!("{W3D_DIR}{prefix}/{stem}.w3d"),
        None => format!("{W3D_DIR}{stem}.w3d"),
    };
    (primary, Some(format!("{MOLDS_DIR}{stem}.w3d")))
}

fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 * 100.0 / whole as f64
    }
}

fn compare(what: &str, measured: u64, research: u64) {
    if measured == research {
        println!("  {what}: {measured} - matches object-ini.md");
    } else {
        println!("  {what}: {measured} - object-ini.md states {research}: DISAGREES");
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
