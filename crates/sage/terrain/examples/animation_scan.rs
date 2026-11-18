use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_sage_map::MapFile;
use mint_sage_model::{
    load_animation, load_model, load_object_index, AnimationMode, ObjectIndex, Resolver,
};
use mint_vfs::Vfs;

const MAPS_LISTED: usize = 20;

const NAMES_PER_CATEGORY: usize = 12;

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let Some(primary) = roots.first() else {
        eprintln!("usage: animation_scan <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    };

    let mut vfs = Vfs::new();
    for install in roots.iter().skip(1).rev().chain([primary]) {
        let mut archives = 0usize;
        for path in big_archives(install) {
            if let Ok(archive) = BigArchive::open(&path) {
                vfs.mount(Arc::new(archive));
                archives = archives.saturating_add(1);
            }
        }
        println!("mounted {archives} archive(s) from {}", install.display());
    }
    println!("{} entries visible", vfs.len());

    let started = Instant::now();
    let (index, report) = load_object_index(&vfs);
    println!(
        "object index: {} document(s), {} top-level, {} parsed, {} object(s), {} declaring a \
         default animation, in {:.1} s",
        report.entries,
        report.top_level,
        report.parsed,
        index.objects(),
        index.animations().count(),
        started.elapsed().as_secs_f64()
    );
    for failure in &report.failures {
        println!("  FAILED {failure}");
    }

    let mut scan = Scan::default();
    scan.walk_maps(primary, &index);
    scan.check_templates(&vfs, &index);
    scan.report();

    if scan.maps.is_empty() {
        eprintln!("no .map/.bse entry found under {}", primary.display());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

struct MapCount {
    name: String,
    placements: usize,
    animated: usize,
    distinct: usize,
}

struct TemplateCheck {
    placements: usize,
    asset: String,
    problem: String,
    frames: u32,
    rate: u32,
    mode: &'static str,
    flags: mint_sage_model::AnimationFlags,
    skinned: usize,
    rigid_driven: usize,
    rigid_static: usize,
}

#[derive(Default)]
struct Scan {
    maps: Vec<MapCount>,
    animated_templates: BTreeMap<String, usize>,
    checks: BTreeMap<String, TemplateCheck>,
    placements: usize,
    animated: usize,
    animation_without_model: usize,
    templates_without_model: BTreeSet<String>,
    animations_per_model: BTreeMap<String, BTreeSet<String>>,
    unreadable: Vec<String>,
    failures: Vec<String>,
}

impl Scan {
    fn walk_maps(&mut self, root: &Path, index: &ObjectIndex) {
        for archive_path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&archive_path) else {
                continue;
            };
            for entry in archive.entries() {
                let lower = entry.name.to_ascii_lowercase();
                if !(lower.ends_with(".map") || lower.ends_with(".bse")) {
                    continue;
                }
                let Ok(raw) = archive.read_entry(entry) else {
                    self.unreadable.push(entry.name.clone());
                    continue;
                };
                match MapFile::parse(&raw) {
                    Ok(map) => self.add_map(&lower, &map, index),
                    Err(error) => self.failures.push(format!("{}: {error}", entry.name)),
                }
            }
        }
        self.maps.sort_by(|a, b| {
            b.animated
                .cmp(&a.animated)
                .then_with(|| a.name.cmp(&b.name))
        });
    }

    fn add_map(&mut self, name: &str, map: &MapFile, index: &ObjectIndex) {
        let mut animated = 0usize;
        let mut distinct: BTreeSet<String> = BTreeSet::new();
        for object in &map.objects {
            if index.animation(&object.template_name).is_none() {
                continue;
            }
            if index.lookup(&object.template_name).model().is_none() {
                self.animation_without_model = self.animation_without_model.saturating_add(1);
                self.templates_without_model
                    .insert(object.template_name.to_ascii_lowercase());
                continue;
            }
            animated = animated.saturating_add(1);
            let key = object.template_name.to_ascii_lowercase();
            distinct.insert(key.clone());
            let slot = self.animated_templates.entry(key).or_insert(0);
            *slot = slot.saturating_add(1);
        }
        self.placements = self.placements.saturating_add(map.objects.len());
        self.animated = self.animated.saturating_add(animated);
        self.maps.push(MapCount {
            name: name.to_owned(),
            placements: map.objects.len(),
            animated,
            distinct: distinct.len(),
        });
    }

    fn check_templates(&mut self, vfs: &Vfs, index: &ObjectIndex) {
        let resolver = Resolver::new();
        for (name, placements) in &self.animated_templates {
            let Some(animation) = index.animation(name) else {
                continue;
            };
            let template = index.lookup(name);
            let Some(model_name) = template.model() else {
                continue;
            };
            let asset = animation.asset().to_owned();
            self.animations_per_model
                .entry(model_name.to_ascii_lowercase())
                .or_default()
                .insert(asset.clone());
            let mut check = TemplateCheck {
                placements: *placements,
                asset: asset.clone(),
                problem: String::new(),
                frames: 0,
                rate: 0,
                mode: animation.mode.map_or("(absent)", AnimationMode::as_str),
                flags: animation.flags,
                skinned: 0,
                rigid_driven: 0,
                rigid_static: 0,
            };
            let model = match load_model(vfs, &resolver, model_name) {
                Ok(model) => model,
                Err(error) => {
                    check.problem = format!("the model {model_name} did not load: {error}");
                    self.checks.insert(name.clone(), check);
                    continue;
                }
            };
            let loaded = match load_animation(vfs, &resolver, &asset) {
                Ok(loaded) => loaded,
                Err(error) => {
                    check.problem = format!("the animation did not load: {error}");
                    self.checks.insert(name.clone(), check);
                    continue;
                }
            };
            check.frames = loaded.animation.num_frames;
            check.rate = loaded.animation.frame_rate;

            let Some(hierarchy) = &model.hierarchy else {
                check.problem = "the model has no hierarchy to pose".to_owned();
                self.checks.insert(name.clone(), check);
                continue;
            };
            if !hierarchy
                .header
                .name
                .eq_ignore_ascii_case(&loaded.animation.hierarchy_name)
            {
                check.problem = format!(
                    "the model's hierarchy is {:?}, the animation names {:?}",
                    hierarchy.header.name, loaded.animation.hierarchy_name
                );
                self.checks.insert(name.clone(), check);
                continue;
            }

            let named: BTreeSet<u32> = loaded
                .animation
                .channels
                .iter()
                .filter(|channel| channel.pivot != 0)
                .map(|channel| channel.pivot)
                .collect();
            let mut moves: Vec<bool> = Vec::with_capacity(hierarchy.pivots.len());
            for (index, pivot) in hierarchy.pivots.iter().enumerate() {
                let inherited = pivot
                    .parent
                    .and_then(|parent| usize::try_from(parent).ok())
                    .filter(|parent| *parent < index)
                    .and_then(|parent| moves.get(parent).copied())
                    .unwrap_or(false);
                let own = u32::try_from(index).is_ok_and(|index| named.contains(&index));
                moves.push(own || inherited);
            }
            for part in &model.model.parts {
                if part.pick_proxy || part.hidden {
                    continue;
                }
                if part.skin.is_some() {
                    check.skinned = check.skinned.saturating_add(1);
                } else if let Some(pivot) = part.pivot {
                    if moves.get(pivot).copied().unwrap_or(false) {
                        check.rigid_driven = check.rigid_driven.saturating_add(1);
                    } else {
                        check.rigid_static = check.rigid_static.saturating_add(1);
                    }
                }
            }
            if check.skinned == 0 && check.rigid_driven == 0 {
                check.problem = "no part is skinned and no rigid part's pivot is driven".to_owned();
            }
            self.checks.insert(name.clone(), check);
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn report(&self) {
        println!();
        println!(
            "{} map(s) walked, {} placement(s), {} of them ({:.2}%) name a template with a \
             default animation and a model",
            self.maps.len(),
            self.placements,
            self.animated,
            100.0 * self.animated as f64 / (self.placements.max(1) as f64)
        );
        println!(
            "  a further {} placement(s) over {} template name(s) resolve to a default animation but draw no model",
            self.animation_without_model,
            self.templates_without_model.len()
        );
        for name in &self.unreadable {
            println!("  UNREADABLE {name}");
        }
        for failure in &self.failures {
            println!("  FAILED {failure}");
        }

        println!();
        println!("maps with the most animated placements:");
        println!(
            "  {:>8}  {:>8}  {:>6}  {:>8}  map",
            "animated", "objects", "share", "distinct"
        );
        for map in self.maps.iter().take(MAPS_LISTED) {
            println!(
                "  {:>8}  {:>8}  {:>5.1}%  {:>8}  {}",
                map.animated,
                map.placements,
                100.0 * map.animated as f64 / (map.placements.max(1) as f64),
                map.distinct,
                map.name
            );
        }

        let playable: Vec<&TemplateCheck> = self
            .checks
            .values()
            .filter(|check| check.problem.is_empty())
            .collect();
        let playable_placements: usize = playable
            .iter()
            .fold(0usize, |acc, check| acc.saturating_add(check.placements));
        println!();
        println!(
            "{} distinct animated template(s); {} of them play, covering {} of {} animated \
             placement(s)",
            self.checks.len(),
            playable.len(),
            playable_placements,
            self.animated
        );
        let skinned_only = playable
            .iter()
            .filter(|check| check.skinned > 0 && check.rigid_driven == 0)
            .count();
        let rigid_only = playable
            .iter()
            .filter(|check| check.skinned == 0 && check.rigid_driven > 0)
            .count();
        let both = playable
            .iter()
            .filter(|check| check.skinned > 0 && check.rigid_driven > 0)
            .count();
        let skinned_placements: usize = playable
            .iter()
            .filter(|check| check.skinned > 0)
            .fold(0usize, |acc, check| acc.saturating_add(check.placements));
        println!(
            "  {skinned_only} skinned only, {rigid_only} rigidly bound only, {both} both; \
             {skinned_placements} placement(s) draw a skinned part"
        );

        let mut distinct_assets: BTreeSet<&str> = BTreeSet::new();
        for check in &playable {
            distinct_assets.insert(check.asset.as_str());
        }
        println!("  {} distinct animation asset(s)", distinct_assets.len());

        let mut by_mode: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        for check in &playable {
            let slot = by_mode.entry(check.mode).or_insert((0, 0));
            slot.0 = slot.0.saturating_add(1);
            slot.1 = slot.1.saturating_add(check.placements);
        }
        println!("  AnimationMode over the playable templates:");
        for (mode, (templates, placements)) in &by_mode {
            println!("    {templates:>4} template(s), {placements:>5} placement(s)  {mode}");
        }

        let flag = |pick: fn(&mint_sage_model::AnimationFlags) -> bool| -> (usize, usize) {
            playable
                .iter()
                .filter(|c| pick(&c.flags))
                .fold((0usize, 0usize), |acc, check| {
                    (
                        acc.0.saturating_add(1),
                        acc.1.saturating_add(check.placements),
                    )
                })
        };
        let random = flag(|f| f.random_start);
        let first = flag(|f| f.start_frame_first);
        let last = flag(|f| f.start_frame_last);
        println!(
            "  state Flags over the playable templates: RANDOMSTART {} template(s) / {}              placement(s), START_FRAME_FIRST {} / {}, START_FRAME_LAST {} / {}",
            random.0, random.1, first.0, first.1, last.0, last.1
        );
        for (name, check) in &self.checks {
            if check.problem.is_empty() && check.flags.random_start {
                println!("    RANDOMSTART {name} ({} placement(s))", check.placements);
            }
        }

        let conflicts: Vec<(&String, &BTreeSet<String>)> = self
            .animations_per_model
            .iter()
            .filter(|(_, assets)| assets.len() > 1)
            .collect();
        println!(
            "  {} of {} model name(s) are asked to play more than one animation",
            conflicts.len(),
            self.animations_per_model.len()
        );
        for (model, assets) in conflicts.iter().take(NAMES_PER_CATEGORY) {
            let listed: Vec<&str> = assets.iter().map(String::as_str).collect();
            println!("    {model}: {}", listed.join(", "));
        }

        let mut by_problem: BTreeMap<&str, Vec<(&str, usize)>> = BTreeMap::new();
        for (name, check) in &self.checks {
            if check.problem.is_empty() {
                continue;
            }
            by_problem
                .entry(problem_category(&check.problem))
                .or_default()
                .push((name.as_str(), check.placements));
        }
        println!();
        println!("templates that do not play, by reason:");
        if by_problem.is_empty() {
            println!("  none");
        }
        for (category, mut names) in by_problem {
            names.sort_by(|a, b| b.1.cmp(&a.1));
            let placements = names
                .iter()
                .fold(0usize, |acc, (_, count)| acc.saturating_add(*count));
            println!(
                "  {} template(s), {placements} placement(s): {category}",
                names.len()
            );
            for (name, count) in names.iter().take(NAMES_PER_CATEGORY) {
                let detail = self
                    .checks
                    .get(*name)
                    .map_or("", |check| check.problem.as_str());
                println!("    {name} ({count} placement(s)): {detail}");
            }
            if names.len() > NAMES_PER_CATEGORY {
                println!("    ... and {} more", names.len() - NAMES_PER_CATEGORY);
            }
        }

        println!();
        println!("the playable templates with the most placements:");
        let mut ranked: Vec<(&String, &TemplateCheck)> = self
            .checks
            .iter()
            .filter(|(_, check)| check.problem.is_empty())
            .collect();
        ranked.sort_by(|a, b| b.1.placements.cmp(&a.1.placements));
        for (name, check) in ranked.iter().take(NAMES_PER_CATEGORY * 2) {
            println!(
                "  {:>5}  {name} -> {} ({} frame(s) @ {} fps, {} skinned, {} rigid driven, {} \
                 rigid static)",
                check.placements,
                check.asset,
                check.frames,
                check.rate,
                check.skinned,
                check.rigid_driven,
                check.rigid_static
            );
        }
    }
}

fn problem_category(problem: &str) -> &'static str {
    if problem.starts_with("the model") && problem.contains("did not load") {
        "the model did not load"
    } else if problem.starts_with("the animation did not load") {
        "the animation did not load"
    } else if problem.starts_with("the model has no hierarchy") {
        "the model has no hierarchy"
    } else if problem.starts_with("the model's hierarchy is") {
        "the animation names a different hierarchy than the model"
    } else {
        "nothing in the model is driven by the animation"
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
