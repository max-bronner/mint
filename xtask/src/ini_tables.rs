mod binary;
mod bind;
mod corpus;
mod emit;
mod pe;
mod scan;

use pe::Image;
use scan::{
    pair_name_and_proc, scan_data_dwords, scan_imm_stores, scan_strings, NameProcPair, StringLit,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const KNOWN_REGISTRY: &[&str] = &[
    "AIBase",
    "AIData",
    "AIDozerAssignment",
    "AerialPathfindNoFlyZone",
    "AmbientStream",
    "AnimationSoundClientBehaviorGlobalSetting",
    "AptButtonTooltipMap",
    "Armor",
    "ArmyDefinition",
    "ArmySummaryDescription",
    "AudioEvent",
    "AudioLOD",
    "AudioLowMHz",
    "AudioSettings",
    "AutoResolveArmor",
    "AutoResolveBody",
    "AutoResolveCombatChain",
    "AutoResolveHandicapLevel",
    "AutoResolveLeadership",
    "AutoResolveReinforcementSchedule",
    "AutoResolveWeapon",
    "AwardSystem",
    "BannerType",
    "BenchProfile",
    "Bridge",
    "ChildObject",
    "CloudBreakEffect",
    "CloudEffect",
    "CommandButton",
    "CommandMap",
    "CommandSet",
    "ControlBarResizer",
    "ControlBarScheme",
    "CrateData",
    "CreateAHeroSystem",
    "CrowdResponse",
    "DamageFX",
    "DebugCommandMap",
    "DialogEvent",
    "DrawGroupInfo",
    "DynamicGameLOD",
    "EmotionNugget",
    "EvaEventForwardReference",
    "ExperienceLevel",
    "ExperienceScalarTable",
    "FXList",
    "FXParticleSystem",
    "FactionVictoryData",
    "Fire",
    "FireEffect",
    "FireLogicSystem",
    "FontDefaultSettings",
    "FontSubstitution",
    "FormationAssistant",
    "GameData",
    "GlowEffect",
    "HeaderTemplate",
    "HouseColor",
    "InGameNotificationBox",
    "LODPreset",
    "Language",
    "LargeGroupAudioMap",
    "LargeGroupAudioUnusedKnownKeys",
    "LightPointLevel",
    "LinearCampaign",
    "LivingWorldAITemplate",
    "LivingWorldAnimObject",
    "LivingWorldArmyIcon",
    "LivingWorldAutoResolveResourceBonus",
    "LivingWorldAutoResolveSciencePurchasePointBonus",
    "LivingWorldBuildPlotIcon",
    "LivingWorldBuilding",
    "LivingWorldBuildingIcon",
    "LivingWorldCampaign",
    "LivingWorldMapInfo",
    "LivingWorldObject",
    "LivingWorldPlayerArmy",
    "LivingWorldPlayerTemplate",
    "LivingWorldRegionCampaign",
    "LivingWorldRegionEffects",
    "LivingWorldSound",
    "Locomotor",
    "MappedImage",
    "MeshNameMatches",
    "MiscAudio",
    "MiscEvaData",
    "MissionObjectiveList",
    "ModifierList",
    "Mouse",
    "MouseCursor",
    "MultiplayerColor",
    "MultiplayerSettings",
    "Multisound",
    "MusicTrack",
    "NewEvaEvent",
    "Object",
    "ObjectCreationList",
    "ObjectReskin",
    "OnlineChatColors",
    "Pathfinder",
    "PlayerAIType",
    "PlayerTemplate",
    "PredefinedEvaEvent",
    "Rank",
    "ReallyLowMHz",
    "RingEffect",
    "Road",
    "Science",
    "ScoredKillEvaAnnouncer",
    "ScriptAction",
    "ScriptCondition",
    "ShadowMap",
    "ShellMenuScheme",
    "SkirmishAIData",
    "SkyboxTextureSet",
    "SpecialPower",
    "StanceTemplate",
    "StaticGameLOD",
    "StrategicHUD",
    "StreamedSound",
    "Terrain",
    "Upgrade",
    "VictorySystemData",
    "WaterSet",
    "WaterTextureList",
    "WaterTransparency",
    "Weapon",
    "Weather",
    "WeatherData",
    "WindowTransition",
];

const KNOWN_SPECIAL_POWER_FLAGS: &[&str] = &[
    "NEEDS_TARGET",
    "WATER_OK",
    "NEEDS_OBJECT_FILTER",
    "LIMIT_DISTANCE",
    "NO_FORBIDDEN_OBJECTS",
    "RESPECT_RECHARGE_TIME_DISCOUNT",
    "PATHABLE_ONLY",
];

struct Extraction {
    title: String,
    image: Image,
    registrations: Vec<NameProcPair>,
    field_runs: Vec<Vec<NameProcPair>>,
    strings_by_rva: BTreeMap<u32, String>,
}

fn find_worldbuilder(install_dir: &Path) -> Option<PathBuf> {
    let direct = install_dir.join("Worldbuilder.exe");
    if direct.is_file() {
        return Some(direct);
    }
    let entries = std::fs::read_dir(install_dir).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name
            .to_string_lossy()
            .eq_ignore_ascii_case("worldbuilder.exe")
        {
            return Some(entry.path());
        }
    }
    None
}

fn extract(install_dir: &Path) -> Result<Extraction, String> {
    let exe = find_worldbuilder(install_dir)
        .ok_or_else(|| format!("no Worldbuilder.exe under {}", install_dir.display()))?;
    let image = Image::load(&exe)?;

    let text = image.section(".text").ok_or("no .text section")?;
    let text_range = (
        text.virtual_address,
        text.virtual_address.saturating_add(text.virtual_size),
    );
    let text_bytes = image.raw_bytes(text);
    let text_stores = scan_imm_stores(text_bytes);

    let mut literals: Vec<StringLit> = Vec::new();
    let mut data_stores = Vec::new();
    for name in [".rdata", ".data"] {
        if let Some(sec) = image.section(name) {
            let bytes = image.raw_bytes(sec);
            literals.extend(scan_strings(bytes, sec.virtual_address));
            data_stores.extend(scan_data_dwords(
                bytes,
                sec.virtual_address,
                image.image_base,
            ));
        }
    }
    let strings_by_rva: BTreeMap<u32, String> = literals
        .into_iter()
        .map(|lit| (lit.rva, lit.text))
        .collect();

    let registrations = find_runs(&image, &text_stores, &strings_by_rva, text_range)
        .into_iter()
        .filter_map(|run| {
            if run.len() == 1 {
                run.into_iter().next()
            } else {
                None
            }
        })
        .collect();

    let mut all_stores = text_stores;
    all_stores.extend(data_stores);
    let field_runs = find_runs(&image, &all_stores, &strings_by_rva, text_range)
        .into_iter()
        .filter(|run| run.len() >= 2)
        .collect();

    Ok(Extraction {
        title: install_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        image,
        registrations,
        field_runs,
        strings_by_rva,
    })
}

fn find_runs(
    img: &Image,
    stores: &[scan::ImmStore],
    strings_by_rva: &BTreeMap<u32, String>,
    text_range: (u32, u32),
) -> Vec<Vec<NameProcPair>> {
    let pairs = pair_name_and_proc(img, stores, strings_by_rva, text_range);
    let by_base: BTreeMap<u32, &NameProcPair> = pairs.iter().map(|p| (p.target_rva, p)).collect();
    let mut runs = Vec::new();
    for pair in &pairs {
        let starts_run = pair
            .target_rva
            .checked_sub(16)
            .is_none_or(|prev| !by_base.contains_key(&prev));
        if !starts_run {
            continue;
        }
        let mut run = vec![clone_pair(pair)];
        let mut next = pair.target_rva.saturating_add(16);
        while let Some(p) = by_base.get(&next) {
            run.push(clone_pair(p));
            next = next.saturating_add(16);
        }
        runs.push(run);
    }
    runs
}

fn clone_pair(p: &NameProcPair) -> NameProcPair {
    NameProcPair {
        target_rva: p.target_rva,
        name: p.name.clone(),
        proc_rva: p.proc_rva,
    }
}

fn read_vocabulary(image: &Image, array_rva: u32) -> Vec<String> {
    let mut out = Vec::new();
    let mut rva = array_rva;
    for _ in 0..64 {
        let Some(ptr_va) = image.read_u32(rva) else {
            break;
        };
        if ptr_va == 0 {
            break;
        }
        let Some(name_rva) = image.va_to_rva(ptr_va) else {
            break;
        };
        let Some(name) = image.read_cstr(name_rva, 64) else {
            break;
        };
        out.push(name);
        rva = rva.saturating_add(4);
    }
    out
}

fn report(ext: &Extraction) {
    println!("\n=== {} ({}) ===", ext.title, ext.image.data.len());
    println!(
        "printable strings indexed in .rdata/.data: {}",
        ext.strings_by_rva.len()
    );
    let isolated: std::collections::BTreeSet<&str> =
        ext.registrations.iter().map(|p| p.name.as_str()).collect();
    let known: std::collections::BTreeSet<&str> = KNOWN_REGISTRY.iter().copied().collect();

    let mut all_found: std::collections::BTreeSet<&str> = isolated.clone();
    for run in &ext.field_runs {
        for p in run {
            all_found.insert(p.name.as_str());
        }
    }
    let matched = all_found.intersection(&known).count();
    println!(
        "registration names recovered (isolated pairs + inside a 16-byte run): {} total names seen, {} match the known 130-name registry, {} known names not found",
        all_found.len(),
        matched,
        known.len() - matched,
    );
    println!(
        "  of those matches, {} were isolated pairs and {} were inside a longer run (the conflation above)",
        isolated.intersection(&known).count(),
        matched - isolated.intersection(&known).count(),
    );
    println!(
        "field-table runs recovered (length >= 2): {}",
        ext.field_runs.len()
    );
    if std::env::var("INI_TABLES_VERBOSE").is_ok() {
        for run in &ext.field_runs {
            let names: Vec<&str> = run.iter().map(|p| p.name.as_str()).collect();
            println!("  run[{}]: {:?}", run.len(), names);
        }
        let mut unmatched: Vec<&str> = all_found.difference(&known).copied().collect();
        unmatched.sort_unstable();
        println!("  unmatched names: {unmatched:?}");
        let mut missing: Vec<&str> = known.difference(&all_found).copied().collect();
        missing.sort_unstable();
        println!("  known names not found: {missing:?}");
    }

    for run in &ext.field_runs {
        let names: Vec<&str> = run.iter().map(|p| p.name.as_str()).collect();
        if names.first() == Some(&"DetectionTime") && names.contains(&"UnitCostDeathType") {
            let Some(second_run_base) = run.first().map(|p| p.target_rva) else {
                continue;
            };
            let Some(flags_base) = second_run_base.checked_sub(11 * 16) else {
                continue;
            };
            let (Some(token_va), Some(proc_va), Some(user_data_va)) = (
                ext.image.read_u32(flags_base),
                ext.image.read_u32(flags_base + 4),
                ext.image.read_u32(flags_base + 8),
            ) else {
                continue;
            };
            let token = ext
                .image
                .va_to_rva(token_va)
                .and_then(|rva| ext.image.read_cstr(rva, 32));
            let text_range = ext
                .image
                .section(".text")
                .map(|s| {
                    (
                        s.virtual_address,
                        s.virtual_address.saturating_add(s.virtual_size),
                    )
                })
                .unwrap_or((0, 0));
            let proc_in_text = ext
                .image
                .va_to_rva(proc_va)
                .is_some_and(|rva| rva >= text_range.0 && rva < text_range.1);
            let vocab = ext
                .image
                .va_to_rva(user_data_va)
                .map(|rva| read_vocabulary(&ext.image, rva))
                .unwrap_or_default();
            let vocab_set: std::collections::BTreeSet<&str> =
                vocab.iter().map(String::as_str).collect();
            let known_set: std::collections::BTreeSet<&str> =
                KNOWN_SPECIAL_POWER_FLAGS.iter().copied().collect();
            println!(
                "SpecialPower record 0 token={token:?} proc_in_text={proc_in_text} Flags vocabulary={vocab:?} (matches known 7: {})",
                vocab_set == known_set
            );
        }
    }
}

fn committed_table(root: &Path) -> Result<String, String> {
    let output = std::process::Command::new("cargo")
        .args([
            "run",
            "--release",
            "-q",
            "-p",
            "mint-sage-ini",
            "--example",
            "dump_tables",
        ])
        .current_dir(root)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn diff(ext: &Extraction, root: &Path) {
    println!(
        "\n--- diff against crates/sage/ini/src/tables/ ({}) ---",
        ext.title
    );
    let table = match committed_table(root) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("  could not read the committed table: {e}");
            return;
        }
    };

    let mut found_names: std::collections::BTreeSet<&str> =
        ext.registrations.iter().map(|p| p.name.as_str()).collect();
    let mut recovered_tokens: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for run in &ext.field_runs {
        for p in run {
            found_names.insert(p.name.as_str());
            recovered_tokens.insert(p.name.as_str());
        }
    }
    let known_registry: std::collections::BTreeSet<&str> = KNOWN_REGISTRY.iter().copied().collect();

    let mut opener_checked = 0u32;
    let mut opener_confirmed = 0u32;
    let mut opener_not_found = 0u32;
    let mut opener_documentary = 0u32;
    let mut closed_count = 0u32;
    let mut alias_count = 0u32;

    for line in table.lines() {
        let mut cols = line.split('\t');
        match (
            cols.next(),
            cols.next(),
            cols.next(),
            cols.next(),
            cols.next(),
            cols.next(),
        ) {
            (Some("BLOCK"), Some(name), _, _, _, _) => {
                let status = if !known_registry.contains(name) {
                    "not a file-scope registration (nested opener - expected)"
                } else if found_names.contains(name) {
                    "yes"
                } else {
                    "NOT FOUND by this scan"
                };
                println!("  block {name:<24} registered in retail: {status}");
            }
            (Some("FIELD"), Some(block), Some(token), Some(primitive), _, _) => {
                let present = recovered_tokens.contains(token);
                println!(
                    "    field {block}.{token:<28} primitive={primitive:<12} in a recovered table: {}",
                    if present { "yes" } else { "not reached by this scan" }
                );
            }
            (Some("OPENER"), Some(context), Some(keyword), Some(_opens), Some(kind), _) => {
                if kind == "registry" {
                    opener_checked += 1;
                    if !known_registry.contains(keyword) {
                        opener_not_found += 1;
                        println!(
                            "  opener {context:<3}.{keyword:<28} NOT in retail's 130-name file-scope registry"
                        );
                    } else if found_names.contains(keyword) {
                        opener_confirmed += 1;
                    } else {
                        opener_not_found += 1;
                        println!(
                            "  opener {context:<3}.{keyword:<28} in the known registry but NOT FOUND by this scan"
                        );
                    }
                } else {
                    opener_documentary += 1;
                }
            }
            (Some("CLOSED"), Some(_context), Some(_group), _, _, _) => {
                closed_count += 1;
            }
            (Some("ALIAS"), Some(_from), Some(_to), _, _, _) => {
                alias_count += 1;
            }
            _ => {}
        }
    }

    if opener_checked > 0 || opener_documentary > 0 {
        println!(
            "  structure: {opener_checked} opener row(s) checkable against the file-scope registry \
             ({opener_confirmed} confirmed, {opener_not_found} not found), {opener_documentary} \
             nested row(s) not reachable by this scan (documentary)"
        );
    }
    if closed_count > 0 {
        println!(
            "  structure: {closed_count} closed-context declaration(s), none checkable by this \
             scan (no nesting information recovered)"
        );
    }
    if alias_count > 0 {
        println!(
            "  structure: {alias_count} context alias(es), none checkable by this scan (no nesting \
             information recovered)"
        );
    }
}

pub(crate) fn run(root: &Path, args: &[String]) -> bool {
    let mut do_diff = false;
    let mut do_emit = false;
    let mut dirs: Vec<PathBuf> = Vec::new();
    for a in args {
        if a == "--emit" {
            do_emit = true;
        } else if a == "--diff" {
            do_diff = true;
        } else {
            dirs.push(PathBuf::from(a));
        }
    }
    if let Ok(extra) = std::env::var("MINT_CORPUS") {
        for p in extra.split(';').filter(|s| !s.is_empty()) {
            dirs.push(PathBuf::from(p));
        }
    }
    if dirs.is_empty() {
        eprintln!(
            "usage: cargo xtask ini-tables [--diff | --emit] <install dir> [<install dir> ...]\n\
             \n\
             <install dir> is a BFME II or ROTWK install directory containing\n\
             Worldbuilder.exe. Installs may also come from MINT_CORPUS\n\
             (';'-separated). Writes nothing into the repository."
        );
        return false;
    }

    if do_emit {
        return emit::run(root, &dirs);
    }
    let mut ok = true;
    for dir in &dirs {
        match extract(dir) {
            Ok(ext) => {
                report(&ext);
                if do_diff {
                    diff(&ext, root);
                }
            }
            Err(e) => {
                eprintln!("FAILED: {}: {e}", dir.display());
                ok = false;
            }
        }
    }
    ok
}
