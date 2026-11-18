use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_sage_ini::particle::{
    DefinitionForm, EmissionVelocity, EmissionVolume, ParticleKind, ParticleShader, RandomRange,
};
use mint_sage_ini::{
    BlockTable, Diagnostic, Document, FxListIndex, MemorySources, ParticleIndex, Reader,
    PARTICLE_DOCUMENTS,
};

const COMPILED_TEXTURES_DIR: &str = "art/compiledtextures/";
const W3D_DIR: &str = "art/w3d/";
const PREFIX_LEN: usize = 2;
const TEXTURE_EXTENSIONS: [&str; 4] = ["dds", "tga", "jpg", "png"];

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: particle_systems <install dir> [<install dir> ...]");
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
    println!(
        "{} archive entr(ies) across {} install(s)",
        every_entry.len(),
        installs.len()
    );

    let mut failed = false;
    for install in &installs {
        failed |= run_install(install, &every_entry);
    }
    println!("\ntotal {:.2}s", started.elapsed().as_secs_f64());
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

    fn particle_sources(&self) -> MemorySources {
        let mut sources = MemorySources::new();
        for archive in &self.archives {
            for entry in archive.entries() {
                if !PARTICLE_DOCUMENTS.contains(&entry.path.as_str()) {
                    continue;
                }
                if let Ok(bytes) = archive.read_entry(entry) {
                    sources.insert(entry.path.as_str(), bytes);
                }
            }
        }
        sources
    }
}

#[allow(clippy::too_many_lines)]
fn run_install(install: &Install, every_entry: &BTreeSet<String>) -> bool {
    println!("\n===== {} =====", install.label);
    let started = Instant::now();
    let sources = install.particle_sources();
    let table = BlockTable::particle_path();
    let reader = Reader::new(&sources);

    let mut systems = ParticleIndex::new();
    let mut lists = FxListIndex::new();
    let mut statements = 0usize;
    let mut stray_ends = 0usize;
    let mut unclosed = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut per_document: Vec<(String, usize, usize)> = Vec::new();

    for path in PARTICLE_DOCUMENTS {
        if sources.paths().all(|held| held != path) {
            failures.push(format!("{path}: not present in this install"));
            continue;
        }
        let document: Document = match reader.read(path, &table) {
            Ok(document) => document,
            Err(error) => {
                failures.push(format!("{path}: {error}"));
                continue;
            }
        };
        statements = statements.saturating_add(document.statements());
        for diagnostic in document.diagnostics() {
            match diagnostic {
                Diagnostic::StrayEnd { .. } => stray_ends = stray_ends.saturating_add(1),
                Diagnostic::UnclosedBlock { .. } => unclosed = unclosed.saturating_add(1),
                _ => {}
            }
            println!("  diagnostic: {diagnostic}");
        }
        let before_systems = systems.len();
        let before_lists = lists.len();
        systems.insert_document(&document);
        lists.insert_document(&document);
        per_document.push((
            path.to_owned(),
            systems.len().saturating_sub(before_systems),
            lists.len().saturating_sub(before_lists),
        ));
    }

    println!("== step 1: the reader ==");
    for (path, added_systems, added_lists) in &per_document {
        println!("  {path}: +{added_systems} system(s), +{added_lists} FXList(s)");
    }
    println!(
        "  {statements} statement(s); {stray_ends} stray End(s), {unclosed} unclosed block(s)"
    );
    println!(
        "  {} distinct particle system(s): {} block-form header(s), {} flat, {} flat shadowed by \
         a block definition, {} redefinition(s) within one form",
        systems.len(),
        systems.block_definitions(),
        systems.flat_definitions(),
        systems.shadowed_flat(),
        systems.redefinitions()
    );
    let flat_only = systems
        .iter()
        .filter(|(_, definition)| definition.form == DefinitionForm::Flat)
        .count();
    println!("  {flat_only} name(s) survive from the flat file alone");
    println!("  {} FXList(s)", lists.len());
    for refusal in systems
        .value_errors()
        .take()
        .iter()
        .chain(lists.value_errors().take().iter())
    {
        println!("  REFUSED {refusal}");
    }
    for failure in &failures {
        println!("  FAILURE {failure}");
    }

    println!("== cross-checks ==");
    let mut w3d_named = 0u64;
    let mut drawable = 0u64;
    let mut w3d_and_drawable = 0u64;
    let mut no_alpha_block = 0u64;
    let mut no_alpha_and_no_shader = 0u64;
    let mut named_shader = 0u64;
    let mut named_shader_no_alpha = 0u64;
    let mut minimum_viable = 0u64;
    let mut block_form = 0u64;
    let mut volumes: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut velocities: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut with_motion = 0u64;
    let mut no_size = 0u64;
    let mut no_size_no_rate = 0u64;
    let mut draw_modules: BTreeMap<String, u64> = BTreeMap::new();
    let mut kinds: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut flipbooks = 0u64;
    let mut flipbook_on_gpu_draw = 0u64;
    let mut gpu_draw = 0u64;
    let mut flipbook_layout_divides = 0u64;
    let mut flipbook_static = 0u64;
    let mut total_frames: BTreeMap<u32, u64> = BTreeMap::new();
    let mut frames_per_row: BTreeMap<u32, u64> = BTreeMap::new();
    let mut speed_multipliers: BTreeMap<String, u64> = BTreeMap::new();
    let mut detail_textures = 0u64;
    let mut gpu_over_four_keys = 0u64;
    let mut other_over_four_keys = 0u64;
    let mut w3d_shader_on_drawable = 0u64;
    let mut w3d_shader_total = 0u64;
    let mut slaves = 0u64;
    let mut slaves_resolved = 0u64;
    let mut slave_offsets_nonzero = 0u64;
    let mut per_particle = 0u64;
    let mut per_particle_resolved = 0u64;
    let mut events = 0u64;
    let mut event_kill = 0u64;
    let mut event_orient = 0u64;
    let mut event_per_particle = 0u64;
    let mut event_offset_nonzero = 0u64;
    let mut ground_aligned = 0u64;
    let mut sort_levels: BTreeMap<String, u64> = BTreeMap::new();
    let mut priorities: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut priority_absent = 0u64;
    let mut alpha_test = 0u64;
    let mut exempt = 0u64;
    let mut wind_blocks = 0u64;
    let mut wind_motions: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut wind_moving = 0u64;
    let mut wind_no_strength = 0u64;
    let mut wind_authored_full = 0u64;
    let mut wind_authored_zero = 0u64;
    let mut wind_inverted_span = 0u64;
    let mut wind_strengths: BTreeMap<String, u64> = BTreeMap::new();
    for (_, definition) in systems.iter() {
        if definition.form != DefinitionForm::Block {
            continue;
        }
        block_form = block_form.saturating_add(1);
        let is_w3d = definition
            .particle_name
            .as_deref()
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".w3d"));
        let is_drawable = definition.kind == ParticleKind::Drawable;
        w3d_named = w3d_named.saturating_add(u64::from(is_w3d));
        drawable = drawable.saturating_add(u64::from(is_drawable));
        w3d_and_drawable = w3d_and_drawable.saturating_add(u64::from(is_w3d && is_drawable));
        if !definition.has_alpha_block {
            no_alpha_block = no_alpha_block.saturating_add(1);
            if definition.declared_shader.is_none() {
                no_alpha_and_no_shader = no_alpha_and_no_shader.saturating_add(1);
            }
        }
        if definition.declared_shader.is_some() {
            named_shader = named_shader.saturating_add(1);
            if !definition.has_alpha_block {
                named_shader_no_alpha = named_shader_no_alpha.saturating_add(1);
            }
        }
        minimum_viable = minimum_viable.saturating_add(u64::from(definition.is_sprite_path()));
        *volumes
            .entry(match definition.volume {
                EmissionVolume::Point => "POINT",
                EmissionVolume::Line { .. } => "LINE",
                EmissionVolume::Box { .. } => "BOX",
                EmissionVolume::Sphere { .. } => "SPHERE",
                EmissionVolume::Cylinder { .. } => "CYLINDER",
                EmissionVolume::Lightning { .. } => "LIGHTNING",
                EmissionVolume::Unread { .. } => "not decoded",
            })
            .or_default() += 1;
        *velocities
            .entry(match definition.velocity {
                EmissionVelocity::Ortho => "ORTHO",
                EmissionVelocity::Spherical { .. } => "SPHERICAL",
                EmissionVelocity::Hemispherical { .. } => "HEMISPHERICAL",
                EmissionVelocity::Cylindrical { .. } => "CYLINDRICAL",
                EmissionVelocity::Outward { .. } => "OUTWARD",
                EmissionVelocity::Unread { .. } => "not decoded",
            })
            .or_default() += 1;
        let motion = definition.gravity != 0.0
            || definition.drift_velocity != [0.0; 3]
            || definition.velocity_damping != RandomRange::default()
            || definition.size_rate != RandomRange::default()
            || definition.angular_rate_z != RandomRange::default()
            || definition.angle_z != RandomRange::default();
        with_motion = with_motion.saturating_add(u64::from(motion));
        ground_aligned = ground_aligned.saturating_add(u64::from(definition.ground_aligned));
        *sort_levels
            .entry(match definition.sort_level {
                Some(level) => level.to_string(),
                None => "absent".to_owned(),
            })
            .or_default() += 1;
        match definition.declared_priority {
            Some(priority) => *priorities.entry(priority.as_str()).or_default() += 1,
            None => priority_absent = priority_absent.saturating_add(1),
        }
        exempt = exempt.saturating_add(u64::from(definition.priority().is_exempt()));
        alpha_test =
            alpha_test.saturating_add(u64::from(definition.shader() == ParticleShader::AlphaTest));
        *draw_modules
            .entry(
                definition
                    .draw_module
                    .clone()
                    .unwrap_or_else(|| "(no Draw block)".to_owned()),
            )
            .or_default() += 1;
        *kinds.entry(definition.kind.as_str()).or_default() += 1;
        let is_gpu_draw = definition
            .draw_module
            .as_deref()
            .is_some_and(|module| module.eq_ignore_ascii_case("GpuDraw"));
        gpu_draw = gpu_draw.saturating_add(u64::from(is_gpu_draw));
        let over_four = definition.colors_authored > 4 || definition.alphas_authored > 4;
        if is_gpu_draw {
            gpu_over_four_keys = gpu_over_four_keys.saturating_add(u64::from(over_four));
        } else {
            other_over_four_keys = other_over_four_keys.saturating_add(u64::from(over_four));
        }
        if let Some(book) = definition.flipbook {
            flipbooks = flipbooks.saturating_add(1);
            flipbook_on_gpu_draw = flipbook_on_gpu_draw.saturating_add(u64::from(is_gpu_draw));
            if book.frames_per_row > 0 && book.total_frames % book.frames_per_row == 0 {
                flipbook_layout_divides = flipbook_layout_divides.saturating_add(1);
            }
            if !book.animates() {
                flipbook_static = flipbook_static.saturating_add(1);
            }
            *total_frames.entry(book.total_frames).or_default() += 1;
            *frames_per_row.entry(book.frames_per_row).or_default() += 1;
            *speed_multipliers
                .entry(format!("{}", book.speed_multiplier))
                .or_default() += 1;
        }
        detail_textures =
            detail_textures.saturating_add(u64::from(definition.detail_texture.is_some()));
        if matches!(
            definition.declared_shader,
            Some(
                ParticleShader::W3dEmissive | ParticleShader::W3dAlpha | ParticleShader::W3dDiffuse
            )
        ) {
            w3d_shader_total = w3d_shader_total.saturating_add(1);
            w3d_shader_on_drawable = w3d_shader_on_drawable.saturating_add(u64::from(is_drawable));
        }
        if let Some(slave) = &definition.slave_system {
            slaves = slaves.saturating_add(1);
            slaves_resolved =
                slaves_resolved.saturating_add(u64::from(systems.get(slave).is_some()));
            if definition.slave_pos_offset != [0.0; 3] {
                slave_offsets_nonzero = slave_offsets_nonzero.saturating_add(1);
            }
        }
        if let Some(attached) = &definition.per_particle_system {
            per_particle = per_particle.saturating_add(1);
            per_particle_resolved =
                per_particle_resolved.saturating_add(u64::from(systems.get(attached).is_some()));
        }
        if let Some(event) = &definition.event {
            events = events.saturating_add(1);
            event_kill = event_kill.saturating_add(u64::from(event.kill_after_event));
            event_orient = event_orient.saturating_add(u64::from(event.orient_to_terrain));
            event_per_particle = event_per_particle.saturating_add(u64::from(event.per_particle));
            if event.height_offset != RandomRange::default() {
                event_offset_nonzero = event_offset_nonzero.saturating_add(1);
            }
        }
        if definition.size.low == 0.0 && definition.size.high == 0.0 {
            no_size = no_size.saturating_add(1);
            if definition.size_rate == RandomRange::default() {
                no_size_no_rate = no_size_no_rate.saturating_add(1);
            }
        }
        if let Some(wind) = &definition.wind {
            wind_blocks = wind_blocks.saturating_add(1);
            *wind_motions.entry(wind.motion.as_str()).or_default() += 1;
            wind_moving = wind_moving.saturating_add(u64::from(wind.motion.moves()));
            if wind.declared_strength.is_none() {
                wind_no_strength = wind_no_strength.saturating_add(1);
            } else {
                *wind_strengths
                    .entry(format!("{}", wind.strength()))
                    .or_default() += 1;
            }
            wind_authored_full = wind_authored_full
                .saturating_add(u64::from(wind.declared_full_strength_dist.is_some()));
            wind_authored_zero = wind_authored_zero
                .saturating_add(u64::from(wind.declared_zero_strength_dist.is_some()));
            if wind.ping_pong_end.high < wind.ping_pong_end.low
                || wind.ping_pong_start.high < wind.ping_pong_start.low
            {
                wind_inverted_span = wind_inverted_span.saturating_add(1);
            }
        }
    }
    println!(
        "  ParticleName ends .w3d on {w3d_named}, Type = DRAWABLE on {drawable}, both on \
         {w3d_and_drawable}"
    );
    println!(
        "  no Alpha block on {no_alpha_block} block-form system(s), of which {no_alpha_and_no_shader} \
         also name no Shader; {named_shader_no_alpha} of {named_shader} with a named Shader carry none"
    );
    println!(
        "  {minimum_viable} of {block_form} block-form system(s) are in the sprite path's scope \
         (a sprite, a decoded volume and velocity, DefaultDraw or none)"
    );
    println!("== step 5: the volumes and velocities ==");
    let show = |label: &str, counts: &BTreeMap<&'static str, u64>| {
        let mut line = String::new();
        for (name, count) in counts {
            line.push_str(&format!("{name} {count}  "));
        }
        println!("  {label}: {}", line.trim_end());
    };
    show("volume", &volumes);
    show("velocity", &velocities);
    println!("== step 6: the motion ==");
    println!(
        "  {with_motion} of {block_form} block-form system(s) author at least one of Gravity, \
         DriftVelocity, VelocityDamping, SizeRate, AngleZ or AngularRateZ"
    );
    println!(
        "  {no_size} author no Size at all and are invisible until SizeRate grows them; \
         {no_size_no_rate} of those author no SizeRate either and are invisible whatever \
         mint does"
    );

    println!("== steps 9, 10 and 15: alignment, sorting and the budget ==");
    println!(
        "  IsGroundAligned on {ground_aligned} block-form system(s) - these lie flat (step 9)"
    );
    let mut levels = String::new();
    for (level, count) in &sort_levels {
        levels.push_str(&format!("{level} {count}  "));
    }
    println!("  SortLevel: {} (undecoded - step 10)", levels.trim_end());
    let mut ladder = String::new();
    for (name, count) in &priorities {
        ladder.push_str(&format!("{name} {count}  "));
    }
    println!("  Priority: {}absent {priority_absent}", ladder);
    println!(
        "  {exempt} system(s) are ALWAYS_RENDER and so exempt from the whole count          mechanism (step 15)"
    );
    println!(
        "  Shader = ALPHA_TEST on {alpha_test}: opaque, depth-writing, and the only          preset that needs no sort"
    );

    println!("== steps 12, 13 and 14: the GPU path, the non-sprite types and the compounds ==");
    let show_u32 = |label: &str, counts: &BTreeMap<u32, u64>| {
        let mut line = String::new();
        for (value, count) in counts {
            line.push_str(&format!("{value} x{count}  "));
        }
        println!("    {label}: {}", line.trim_end());
    };
    let mut modules = String::new();
    for (name, count) in &draw_modules {
        modules.push_str(&format!("{name} {count}  "));
    }
    println!("  Draw module: {}", modules.trim_end());
    let mut types = String::new();
    for (name, count) in &kinds {
        types.push_str(&format!("{name} {count}  "));
    }
    println!("  Type: {}", types.trim_end());
    println!(
        "  step 12: flipbook fields on {flipbooks} system(s), {flipbook_on_gpu_draw} of them \
         GpuDraw, of {gpu_draw} GpuDraw system(s)"
    );
    println!(
        "    FramesPerRow divides TotalFrames on {flipbook_layout_divides} of {flipbooks}; \
         {flipbook_static} have TotalFrames = 1 and so do not animate at all"
    );
    show_u32("TotalFrames", &total_frames);
    show_u32("FramesPerRow", &frames_per_row);
    let mut speeds = String::new();
    for (value, count) in &speed_multipliers {
        speeds.push_str(&format!("{value} x{count}  "));
    }
    println!("    SpeedMultiplier: {}", speeds.trim_end());
    println!("    DetailTexture on {detail_textures} (undecoded - section 7e)");
    println!(
        "    the four-key limit: {gpu_over_four_keys} GpuDraw system(s) author more than four \
         Color/Alpha keys, against {other_over_four_keys} of the rest"
    );
    println!(
        "  step 13: the W3D_* shader values are on {w3d_shader_on_drawable} of \
         {w3d_shader_total} DRAWABLE system(s)"
    );
    println!(
        "  step 14: SlaveSystem on {slaves} ({slaves_resolved} resolve, \
         {slave_offsets_nonzero} write a non-zero SlavePosOffset), \
         PerParticleAttachedSystem on {per_particle} ({per_particle_resolved} resolve)"
    );
    println!(
        "    Event = TerrainCollision on {events}: {event_kill} KillAfterEvent, \
         {event_orient} OrientFXToTerrain, {event_per_particle} PerParticle, \
         {event_offset_nonzero} with a non-zero HeightOffset"
    );
    for (name, definition) in systems.iter() {
        let Some(event) = &definition.event else {
            continue;
        };
        if event.per_particle {
            continue;
        }
        println!("      {name} writes no PerParticle - a redefinition dropped it");
    }
    let mut event_fx = 0u64;
    let mut event_fx_resolved = 0u64;
    let mut event_nuggets = 0u64;
    let mut event_nuggets_defined = 0u64;
    for (_, definition) in systems.iter() {
        let Some(event) = &definition.event else {
            continue;
        };
        event_fx = event_fx.saturating_add(1);
        let Some(list) = lists.get(&event.fx) else {
            continue;
        };
        event_fx_resolved = event_fx_resolved.saturating_add(1);
        for nugget in &list.nuggets {
            if !nugget.is_particle_system() {
                continue;
            }
            event_nuggets = event_nuggets.saturating_add(1);
            if nugget
                .name
                .as_deref()
                .is_some_and(|name| systems.get(name).is_some())
            {
                event_nuggets_defined = event_nuggets_defined.saturating_add(1);
            }
        }
    }
    println!(
        "    {event_fx_resolved} of {event_fx} EventFX name a defined FXList; those lists hold \
         {event_nuggets} ParticleSystem nugget(s), {event_nuggets_defined} of them defined"
    );

    let mut references = 0u64;
    let mut resolved = 0u64;
    let mut dangling: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (list, system) in lists.particle_references() {
        references = references.saturating_add(1);
        if systems.get(system).is_some() {
            resolved = resolved.saturating_add(1);
        } else {
            dangling
                .entry(system.to_owned())
                .or_default()
                .push(list.to_owned());
        }
    }
    println!("  FXList particle references: {resolved} of {references} resolve");
    for (system, from) in &dangling {
        println!("    dangling {system:?} named by {} list(s)", from.len());
    }

    println!("== step 11: the wind ==");
    let mut motions = String::new();
    for (name, count) in &wind_motions {
        motions.push_str(&format!("{name} {count}  "));
    }
    println!(
        "  Wind block on {wind_blocks} block-form system(s); WindMotion: {}",
        motions.trim_end()
    );
    println!(
        "  {wind_moving} of {wind_blocks} name a motion that moves - the rest carry angles \
         and a strength with the wind switched off"
    );
    println!(
        "  {wind_no_strength} omit WindStrength (read as 1.0), {wind_authored_full} author \
         WindFullStrengthDist and {wind_authored_zero} WindZeroStrengthDist (EA's 75 and 200 \
         otherwise)"
    );
    let mut strengths = String::new();
    for (value, count) in &wind_strengths {
        strengths.push_str(&format!("{value} x{count}  "));
    }
    println!("    WindStrength: {}", strengths.trim_end());
    println!(
        "  {wind_inverted_span} author a ping-pong range whose Min exceeds its Max, so the \
         sweep must not assume an ordered pair"
    );

    println!("== step 8: the FXList ParticleSystem nugget ==");
    let mut nuggets = 0u64;
    let mut with_offset = 0u64;
    let mut orienting = 0u64;
    let mut attaching = 0u64;
    let mut not_attaching = 0u64;
    let mut at_ground = 0u64;
    let mut delayed = 0u64;
    let mut delay_min = f32::INFINITY;
    let mut delay_max: f32 = 0.0;
    let mut sort_levels_reachable: BTreeMap<String, u64> = BTreeMap::new();
    for (_, list) in lists.iter() {
        for nugget in &list.nuggets {
            let Some(particle) = &nugget.particle else {
                continue;
            };
            nuggets = nuggets.saturating_add(1);
            if particle.offset != [0.0; 3] {
                with_offset = with_offset.saturating_add(1);
            }
            orienting = orienting.saturating_add(u64::from(particle.orient_to_object));
            if particle.attach_to_object {
                attaching = attaching.saturating_add(1);
            } else {
                not_attaching = not_attaching.saturating_add(1);
            }
            at_ground = at_ground.saturating_add(u64::from(particle.create_at_ground_height));
            if particle.initial_delay != RandomRange::default() {
                delayed = delayed.saturating_add(1);
                delay_min = delay_min.min(particle.initial_delay.low);
                delay_max = delay_max.max(particle.initial_delay.high);
            }
            let level = nugget
                .name
                .as_deref()
                .and_then(|name| systems.get(name))
                .map(|definition| match definition.sort_level {
                    Some(level) => level.to_string(),
                    None => "absent".to_owned(),
                });
            if let Some(level) = level {
                *sort_levels_reachable.entry(level).or_default() += 1;
            }
        }
    }
    println!(
        "  {nuggets} ParticleSystem nugget(s): {with_offset} with a non-zero Offset, \
         {orienting} OrientToObject, {attaching} AttachToObject Yes and {not_attaching} No, \
         {at_ground} CreateAtGroundHeight"
    );
    println!(
        "  {delayed} author an InitialDelay; converted from milliseconds it spans {} to {} \
         frame(s)",
        if delay_min.is_finite() {
            delay_min
        } else {
            0.0
        },
        delay_max
    );
    let mut reachable = String::new();
    for (level, count) in &sort_levels_reachable {
        reachable.push_str(&format!("{level} {count}  "));
    }
    println!(
        "  SortLevel of the systems these nuggets name: {}",
        reachable.trim_end()
    );

    println!("== step 2: texture resolution ==");
    let mut wanted: BTreeMap<String, u64> = BTreeMap::new();
    let mut wanted_block_form: BTreeSet<String> = BTreeSet::new();
    for (_, definition) in systems.iter() {
        if let Some(name) = &definition.particle_name {
            *wanted.entry(name.to_ascii_lowercase()).or_default() += 1;
            if definition.form == DefinitionForm::Block {
                wanted_block_form.insert(name.to_ascii_lowercase());
            }
        }
    }
    let mut sprites = 0u64;
    let mut sprites_found = 0u64;
    let mut models = 0u64;
    let mut models_found = 0u64;
    let mut systems_missing_art = 0u64;
    let mut missing: Vec<String> = Vec::new();
    let mut present: BTreeSet<String> = BTreeSet::new();
    for (name, uses) in &wanted {
        let is_model = name.ends_with(".w3d");
        let found = candidates(name)
            .iter()
            .any(|path| every_entry.contains(path.as_str()));
        if is_model {
            models = models.saturating_add(1);
            models_found = models_found.saturating_add(u64::from(found));
        } else {
            sprites = sprites.saturating_add(1);
            sprites_found = sprites_found.saturating_add(u64::from(found));
        }
        if found {
            present.insert(name.clone());
        } else {
            systems_missing_art = systems_missing_art.saturating_add(*uses);
            missing.push(format!("{name} ({uses} system(s))"));
        }
    }
    let block_form_found = wanted_block_form
        .iter()
        .filter(|name| present.contains(*name))
        .count();
    println!(
        "  {block_form_found} of the {} ParticleName value(s) a block-form definition names \
         resolve; {} distinct value(s) over both forms",
        wanted_block_form.len(),
        wanted.len()
    );
    println!(
        "  {sprites_found} of {sprites} sprite texture(s) and {models_found} of {models} model(s) \
         resolve; the {} that do not are named by {systems_missing_art} system(s)",
        missing.len()
    );
    let drawable_now = systems
        .iter()
        .filter(|(_, definition)| {
            definition.form == DefinitionForm::Block && definition.is_sprite_path()
        })
        .filter(|(_, definition)| {
            definition
                .particle_name
                .as_deref()
                .is_some_and(|name| present.contains(&name.to_ascii_lowercase()))
        })
        .count();
    println!("  {drawable_now} of the {minimum_viable} in-scope system(s) have a shipped texture");
    for name in &missing {
        println!("    unresolved {name}");
    }

    println!("  {:.2}s", started.elapsed().as_secs_f64());
    !failures.is_empty() || stray_ends > 0 || unclosed > 0
}

fn candidates(name: &str) -> Vec<String> {
    let lowered = name.to_ascii_lowercase();
    let Some((stem, extension)) = lowered.rsplit_once('.') else {
        return Vec::new();
    };
    if stem.is_empty() {
        return Vec::new();
    }
    let prefix: String = stem.chars().take(PREFIX_LEN).collect();
    if extension == "w3d" {
        return vec![format!("{W3D_DIR}{prefix}/{stem}.w3d")];
    }
    TEXTURE_EXTENSIONS
        .iter()
        .map(|extension| format!("{COMPILED_TEXTURES_DIR}{prefix}/{stem}.{extension}"))
        .collect()
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
