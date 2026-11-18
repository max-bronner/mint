use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use mint_movement::{Cell, GridExtent, Passability as _, Vec2};
use mint_sage_big::BigArchive;
use mint_sage_ini::{BlockTable, MemorySources, Reader};
use mint_sage_map::{MapFile, MAP_XY_FACTOR};
use mint_vfs::Vfs;

use mint_sage_passability::{
    footprints, Obstacle, Passability, TemplateIndex, WaterDepths, CLIFF, GROUND, PINCHED,
};

const VERTICAL_SCALE: f32 = 5.0 / 128.0;

const INI_ROOT: &str = "data/ini";

const RUNGS: [&str; 4] = ["cliffs", "+water", "+objects", "+boundary"];

const CLIFF_SWEEP: [f32; 5] = [0.25, 0.5, 1.0, 2.0, 4.0];

fn main() -> std::process::ExitCode {
    let mut installs: Vec<PathBuf> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--install" => match args.next() {
                Some(path) => installs.push(PathBuf::from(path)),
                None => {
                    eprintln!("--install wants a path");
                    return std::process::ExitCode::FAILURE;
                }
            },
            other => installs.push(PathBuf::from(other)),
        }
    }
    if installs.is_empty() {
        eprintln!(
            "usage: passability_corpus <install dir> [--install <dir>]\n\
             builds a passability grid for every .map in the mounted archives"
        );
        return std::process::ExitCode::FAILURE;
    }

    let vfs = mount(&installs);
    let templates = object_templates(&vfs);
    println!(
        "object templates: {} of {} occupy ground; {} dropped for PROJECTILE/BRIDGE_TOWER/\
         DO_NOT_CLASSIFY, {} not STRUCTURE, {} STRUCTURE but mobile, {} GeometryIsSmall and \
         {} carry no radius",
        templates.len(),
        templates.templates_seen,
        templates.excluded_kind,
        templates.not_structure,
        templates.mobile,
        templates.too_small,
        templates.no_radius
    );

    let mut names: Vec<String> = vfs
        .mounts()
        .flat_map(|mount| mount.entries())
        .filter(|path| path.extension().is_some_and(|ext| ext == "map"))
        .map(|path| path.as_str().to_owned())
        .collect();
    names.sort();
    names.dedup();
    println!("{} distinct .map entr(ies)", names.len());

    let mut totals = Totals::default();
    let started = Instant::now();

    for name in &names {
        let Ok(bytes) = vfs.read(name) else {
            totals
                .parse_failures
                .push(format!("{name}: could not be read out of the archive"));
            continue;
        };
        let map = match MapFile::parse(&bytes) {
            Ok(map) => map,
            Err(error) => {
                totals.parse_failures.push(format!("{name}: {error}"));
                continue;
            }
        };
        if map.height_map.width < 2 || map.height_map.height < 2 {
            totals.too_small.push(format!(
                "{name}: {}x{} samples",
                map.height_map.width, map.height_map.height
            ));
            continue;
        }
        tally(name, &map, &templates, &mut totals);
    }

    totals.report(started.elapsed().as_secs_f64(), names.len());
    if totals.clean() {
        println!("\nall invariants held");
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}

fn tally(name: &str, map: &MapFile, templates: &TemplateIndex, totals: &mut Totals) {
    let obstacles = footprints::resolve(&map.objects, templates);
    totals.placements = totals.placements.saturating_add(map.objects.len() as u64);
    totals.obstacles = totals.obstacles.saturating_add(obstacles.len() as u64);
    totals.water_areas = totals
        .water_areas
        .saturating_add((map.water.standing_areas().len() + map.water.river_areas().len()) as u64);

    let ladder = match build_ladder(map, &obstacles) {
        Ok(ladder) => ladder,
        Err(error) => {
            totals.build_failures.push(format!("{name}: {error}"));
            return;
        }
    };
    totals.built = totals.built.saturating_add(1);

    let extent = ladder[0].grid.extent();
    let cells = u64::from(extent.cell_count());
    totals.cells = totals.cells.saturating_add(cells);
    if extent.cell_count() > totals.largest.0 {
        totals.largest = (extent.cell_count(), name.to_owned(), ladder[3].report());
    }

    let deck_cells: std::collections::HashSet<(u16, u16)> = map
        .objects
        .iter()
        .filter(|object| templates.is_wall(&object.template_name))
        .filter_map(|object| {
            let records = templates.lookup(&object.template_name)?;
            if !object.x.is_finite() || !object.y.is_finite() {
                return None;
            }
            let angle = if object.angle.is_finite() {
                object.angle
            } else {
                0.0
            };
            Some(mint_sage_passability::Obstacle {
                id: 0,
                centre: Vec2::new(object.x, object.y),
                angle,
                records,
            })
        })
        .flat_map(|deck| mint_sage_passability::covered_cells(ladder[0].geometry, extent, &deck))
        .map(|cell| (cell.x, cell.y))
        .collect();

    for (rung, grid) in ladder.iter().enumerate() {
        let mut blocked = 0u64;
        let mut costly = 0u64;
        let mut orphan_pinched = 0u64;
        let mut orphan_obstacle = 0u64;
        for y in 0..extent.height {
            for x in 0..extent.width {
                let cell = Cell::new(x, y);
                let flags = grid.grid.flags(cell);
                let is_blocked = !grid.grid.is_passable(cell, GROUND);
                if is_blocked {
                    blocked = blocked.saturating_add(1);
                }
                if grid.grid.extra_cost(cell) > 0 {
                    costly = costly.saturating_add(1);
                }
                if flags.intersects(PINCHED) {
                    let orphaned = flags.intersects(CLIFF)
                        || (!touches_cliff(grid, extent, x, y)
                            && !touches_deck(&deck_cells, extent, x, y));
                    if orphaned {
                        orphan_pinched = orphan_pinched.saturating_add(1);
                    }
                }
                let stamped = flags.intersects(mint_sage_passability::OBSTACLE);
                if stamped != grid.obstacle_at(cell).is_some() {
                    orphan_obstacle = orphan_obstacle.saturating_add(1);
                }
                if let Some(id) = grid.obstacle_at(cell) {
                    if usize::try_from(id).map(|id| id >= map.objects.len()) != Ok(false) {
                        orphan_obstacle = orphan_obstacle.saturating_add(1);
                    }
                }
            }
        }
        if let Some(slot) = totals.blocked.get_mut(rung) {
            *slot = slot.saturating_add(blocked);
        }
        if let Some(slot) = totals.costly.get_mut(rung) {
            *slot = slot.saturating_add(costly);
        }
        if orphan_pinched > 0 {
            totals.invariant_failures.push(format!(
                "{name} [{}]: {orphan_pinched} pinched cell(s) are blocked or touch no cliff",
                RUNGS.get(rung).copied().unwrap_or("?")
            ));
        }
        if orphan_obstacle > 0 {
            totals.invariant_failures.push(format!(
                "{name} [{}]: {orphan_obstacle} cell(s) disagree between the OBSTACLE flag and \
                 the recorded id",
                RUNGS.get(rung).copied().unwrap_or("?")
            ));
        }
    }

    for rung in 1..ladder.len() {
        let (Some(lower), Some(higher)) = (ladder.get(rung.saturating_sub(1)), ladder.get(rung))
        else {
            continue;
        };
        let mut freed = 0u64;
        for y in 0..extent.height {
            for x in 0..extent.width {
                let cell = Cell::new(x, y);
                if deck_cells.contains(&(x, y)) {
                    continue;
                }
                if !lower.grid.is_passable(cell, GROUND) && higher.grid.is_passable(cell, GROUND) {
                    freed = freed.saturating_add(1);
                }
            }
        }
        if freed > 0 {
            totals.invariant_failures.push(format!(
                "{name}: rung {} freed {freed} cell(s) that {} blocked",
                RUNGS.get(rung).copied().unwrap_or("?"),
                RUNGS.get(rung.saturating_sub(1)).copied().unwrap_or("?")
            ));
        }
    }

    cliff_cross_check(map, extent, totals);

    let border = f32::from(u16::try_from(map.height_map.border_size).unwrap_or(u16::MAX));
    let expected = Vec2::new(
        (0.5 - border) * MAP_XY_FACTOR,
        (0.5 - border) * MAP_XY_FACTOR,
    );
    let centre = ladder[0].geometry.centre_of(Cell::new(0, 0));
    if (centre.x - expected.x).abs() > 1e-3 || (centre.y - expected.y).abs() > 1e-3 {
        totals.invariant_failures.push(format!(
            "{name}: cell (0,0) centres at {centre:?}, expected {expected:?}"
        ));
    }
    if ladder[0].cell_at(centre) != Some(Cell::new(0, 0)) {
        totals.invariant_failures.push(format!(
            "{name}: {centre:?} does not resolve back to cell (0,0)"
        ));
    }
}

fn build_ladder(map: &MapFile, obstacles: &[Obstacle]) -> Result<[Passability; 4], String> {
    let hm = &map.height_map;
    let depths = WaterDepths::SHIPPED;
    Ok([
        Passability::builder(hm, VERTICAL_SCALE)
            .cliff_source(&map.blend_tile)
            .build()?,
        Passability::builder(hm, VERTICAL_SCALE)
            .cliff_source(&map.blend_tile)
            .water(&map.water, depths)
            .build()?,
        Passability::builder(hm, VERTICAL_SCALE)
            .cliff_source(&map.blend_tile)
            .water(&map.water, depths)
            .obstacles(obstacles)
            .build()?,
        Passability::builder(hm, VERTICAL_SCALE)
            .cliff_source(&map.blend_tile)
            .water(&map.water, depths)
            .obstacles(obstacles)
            .playable_border(true)
            .build()?,
    ])
}

fn cliff_cross_check(map: &MapFile, extent: GridExtent, totals: &mut Totals) {
    let width = map.height_map.width;
    for y in 0..extent.height {
        for x in 0..extent.width {
            let index = usize::try_from(u32::from(y).saturating_mul(width))
                .ok()
                .and_then(|row| row.checked_add(usize::from(x)));
            let Some(textured) = index
                .and_then(|index| map.blend_tile.cliff_info_index.get(index))
                .map(|value| *value != 0)
            else {
                continue;
            };
            totals.cliff_compared = totals.cliff_compared.saturating_add(1);
            if textured {
                totals.cliff_textured = totals.cliff_textured.saturating_add(1);
            }
            for (slot, multiplier) in CLIFF_SWEEP.iter().enumerate() {
                let seeded = mint_sage_passability::is_cliff(
                    &map.height_map,
                    VERTICAL_SCALE * multiplier,
                    u32::from(x),
                    u32::from(y),
                );
                if seeded {
                    if let Some(count) = totals.cliff_seeded.get_mut(slot) {
                        *count = count.saturating_add(1);
                    }
                }
                if seeded && textured {
                    if let Some(count) = totals.cliff_agreed.get_mut(slot) {
                        *count = count.saturating_add(1);
                    }
                }
            }
        }
    }
}

fn touches_cliff(grid: &Passability, extent: GridExtent, x: u16, y: u16) -> bool {
    (-1i32..=1).any(|dy| {
        (-1i32..=1).any(|dx| {
            if dx == 0 && dy == 0 {
                return false;
            }
            let (nx, ny) = (i32::from(x) + dx, i32::from(y) + dy);
            u16::try_from(nx)
                .ok()
                .zip(u16::try_from(ny).ok())
                .map(|(nx, ny)| Cell::new(nx, ny))
                .filter(|cell| extent.contains(*cell))
                .is_some_and(|cell| grid.grid.flags(cell).intersects(CLIFF))
        })
    })
}

fn touches_deck(
    deck_cells: &std::collections::HashSet<(u16, u16)>,
    extent: GridExtent,
    x: u16,
    y: u16,
) -> bool {
    (-1i32..=1).any(|dy| {
        (-1i32..=1).any(|dx| {
            let (nx, ny) = (i32::from(x) + dx, i32::from(y) + dy);
            u16::try_from(nx)
                .ok()
                .zip(u16::try_from(ny).ok())
                .map(|(nx, ny)| Cell::new(nx, ny))
                .filter(|cell| extent.contains(*cell))
                .is_some_and(|cell| deck_cells.contains(&(cell.x, cell.y)))
        })
    })
}

#[derive(Default)]
struct Totals {
    built: u64,
    cells: u64,
    blocked: [u64; 4],
    costly: [u64; 4],
    placements: u64,
    obstacles: u64,
    water_areas: u64,
    cliff_compared: u64,
    cliff_textured: u64,
    cliff_seeded: [u64; CLIFF_SWEEP.len()],
    cliff_agreed: [u64; CLIFF_SWEEP.len()],
    largest: (u32, String, String),
    parse_failures: Vec<String>,
    build_failures: Vec<String>,
    too_small: Vec<String>,
    invariant_failures: Vec<String>,
}

impl Totals {
    fn clean(&self) -> bool {
        self.parse_failures.is_empty()
            && self.build_failures.is_empty()
            && self.invariant_failures.is_empty()
    }

    fn report(&self, elapsed: f64, entries: usize) {
        println!();
        println!(
            "{} of {entries} map(s) built a grid in {elapsed:.1} s; {} hold no cell at all",
            self.built,
            self.too_small.len()
        );
        let stray: Vec<&String> = self
            .too_small
            .iter()
            .filter(|entry| !entry.starts_with("libraries/"))
            .collect();
        println!(
            "  of those, {} are outside libraries/ - the expected count is 0",
            stray.len()
        );
        for entry in stray {
            println!("    {entry}");
        }
        println!("{} cell(s) in all", self.cells);
        for (rung, label) in RUNGS.iter().enumerate() {
            let blocked = self.blocked.get(rung).copied().unwrap_or(0);
            let costly = self.costly.get(rung).copied().unwrap_or(0);
            println!(
                "  {label:<10} {blocked:>9} blocked ({:.2}%), {costly:>8} with an extra cost \
                 ({:.2}%)",
                percent(blocked, self.cells),
                percent(costly, self.cells)
            );
        }
        println!(
            "{} water area(s) across the corpus; {} of {} placed object(s) occupy ground",
            self.water_areas, self.obstacles, self.placements
        );
        println!(
            "cliff cross-check against BlendTileData::cliff_info_index over {} cell(s), \n             {} of which the map textures as cliff ({:.3}%):",
            self.cliff_compared,
            self.cliff_textured,
            percent(self.cliff_textured, self.cliff_compared)
        );
        for (slot, multiplier) in CLIFF_SWEEP.iter().enumerate() {
            let seeded = self.cliff_seeded.get(slot).copied().unwrap_or(0);
            let agreed = self.cliff_agreed.get(slot).copied().unwrap_or(0);
            println!(
                "  threshold {:>6.2} world units: {seeded:>9} seeded ({:>5.2}%), \n                 {:>5.1}% of the textured cliffs caught, {:>4.1}% of the seeds textured",
                f64::from(mint_sage_passability::CLIFF_SLOPE_LIMIT) / f64::from(*multiplier),
                percent(seeded, self.cliff_compared),
                percent(agreed, self.cliff_textured),
                percent(agreed, seeded)
            );
        }
        println!(
            "largest grid: {} cells in {}",
            self.largest.0, self.largest.1
        );
        println!("  {}", self.largest.2);
        report("parse failure", &self.parse_failures);
        report("grid build failure", &self.build_failures);
        report("invariant failure", &self.invariant_failures);
    }
}

fn object_templates(vfs: &Vfs) -> TemplateIndex {
    let started = Instant::now();
    let mut names: Vec<String> = vfs
        .mounts()
        .flat_map(|mount| mount.entries())
        .filter(|path| path.as_str().starts_with(INI_ROOT))
        .map(|path| path.as_str().to_owned())
        .collect();
    names.sort();
    names.dedup();
    let mut sources = MemorySources::new();
    for name in &names {
        if let Ok(bytes) = vfs.read(name) {
            sources.insert(name, bytes);
        }
    }
    let table = BlockTable::corpus_path();
    let reader = Reader::new(&sources);
    let paths: Vec<String> = sources.paths().map(str::to_owned).collect();
    let mut documents = Vec::new();
    let mut unreadable = 0usize;
    for path in &paths {
        match reader.read(path, &table) {
            Ok(document) => documents.push(document),
            Err(_) => unreadable = unreadable.saturating_add(1),
        }
    }
    println!(
        "{} INI document(s) parsed ({unreadable} unreadable) in {:.2} s",
        documents.len(),
        started.elapsed().as_secs_f64()
    );
    TemplateIndex::build(&documents)
}

fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        100.0 * part as f64 / whole as f64
    }
}

fn report(label: &str, failures: &[String]) {
    if failures.is_empty() {
        println!("no {label}s");
        return;
    }
    println!("\n{} {label}(s):", failures.len());
    for failure in failures {
        println!("  {failure}");
    }
}

fn mount(installs: &[PathBuf]) -> Vfs {
    let mut vfs = Vfs::new();
    for install in installs {
        let mut here = 0usize;
        for path in mint_sage_big::archives_under(Path::new(install)) {
            if let Ok(archive) = BigArchive::open(&path) {
                vfs.mount(Arc::new(archive));
                here += 1;
            }
        }
        println!("{here} archive(s) mounted from {}", install.display());
    }
    vfs
}
