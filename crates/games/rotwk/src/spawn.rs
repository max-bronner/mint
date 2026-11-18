use mint_movement::{Facing, Vec2};
use mint_sim::{PlayerId, Simulation};

use crate::army::Line;
use crate::rules::{DebugCommand, RotwkCommand, RotwkRules, SpawnRole, StructureEntry, UnitEntry};

pub(crate) const DEFAULT_DUMMY_COUNT: u32 = 10;
pub(crate) const DEFAULT_DUMMY_TEMPLATE: &str = crate::game::EVIL_HORDE;

const MAX_UNIT_SPAWNS: u32 = 2_000;
const MAX_BUILDING_SPAWNS: u32 = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpecItem {
    pub(crate) count: u32,
    pub(crate) name: String,
}

pub(crate) fn parse_spec(spec: &str) -> Result<Vec<SpecItem>, String> {
    let mut items = Vec::new();
    let mut tokens = spec
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|token| !token.is_empty())
        .peekable();
    while let Some(token) = tokens.next() {
        let (count, name) = match token.parse::<u32>() {
            Ok(count) => {
                let name = tokens
                    .next()
                    .ok_or_else(|| format!("{token} names no object"))?;
                (count, name)
            }
            Err(_) => (1, token),
        };
        if count == 0 {
            return Err(format!(
                "{name}: a count of 0 - omit it instead of writing 0"
            ));
        }
        items.push(SpecItem {
            count,
            name: name.to_owned(),
        });
    }
    if items.is_empty() {
        return Err(String::from("an empty spec"));
    }
    Ok(items)
}

pub(crate) trait Named {
    fn label(&self) -> &str;
    fn side(&self) -> Option<&str>;
    fn listed(&self) -> bool;
}

impl Named for UnitEntry {
    fn label(&self) -> &str {
        &self.label
    }

    fn side(&self) -> Option<&str> {
        self.side.as_deref()
    }

    fn listed(&self) -> bool {
        self.listed
    }
}

impl Named for StructureEntry {
    fn label(&self) -> &str {
        &self.label
    }

    fn side(&self) -> Option<&str> {
        self.side.as_deref()
    }

    fn listed(&self) -> bool {
        true
    }
}

pub(crate) fn resolve<T: Named>(
    items: &[SpecItem],
    catalogue: &[T],
) -> Result<Vec<(u32, usize)>, String> {
    let mut out = Vec::new();
    for item in items {
        if item.name.eq_ignore_ascii_case("all") {
            out.extend(
                catalogue
                    .iter()
                    .enumerate()
                    .filter(|(_, entry)| entry.listed())
                    .map(|(at, _)| (item.count, at)),
            );
            continue;
        }
        if let Some(at) = catalogue
            .iter()
            .position(|entry| entry.label() == item.name)
        {
            out.push((item.count, at));
            continue;
        }
        let side_matches: Vec<usize> = catalogue
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.side() == Some(item.name.as_str()))
            .map(|(at, _)| at)
            .collect();
        if side_matches.is_empty() {
            return Err(unknown_name(&item.name, catalogue));
        }
        out.extend(side_matches.into_iter().map(|at| (item.count, at)));
    }
    Ok(out)
}

fn unknown_name<T: Named>(name: &str, catalogue: &[T]) -> String {
    let mut ranked: Vec<&str> = catalogue.iter().map(Named::label).collect();
    ranked.sort_by(|a, b| {
        common_prefix_len(b, name)
            .cmp(&common_prefix_len(a, name))
            .then_with(|| a.cmp(b))
    });
    ranked.dedup();
    let nearest: Vec<&str> = ranked.into_iter().take(3).collect();
    format!(
        "{name}: not an object template, a faction/side name, or \"all\" - nearest: {}",
        nearest.join(", ")
    )
}

fn common_prefix_len(a: &str, b: &str) -> usize {
    a.to_ascii_lowercase()
        .chars()
        .zip(b.to_ascii_lowercase().chars())
        .take_while(|(x, y)| x == y)
        .count()
}

const UNIT_SLOT_SPACING: f32 = 60.0;
const UNIT_ROW_DEPTH: f32 = 55.0;
const UNIT_ROW_HALF_WIDTH: f32 = 600.0;
const UNIT_MARGIN: f32 = 10.0;

fn pack_rows(widths: &[(f32, f32)]) -> (Vec<(f32, f32)>, f32) {
    let mut placed = Vec::with_capacity(widths.len());
    let mut row_start = 0usize;
    let mut row_width = 0.0f32;
    let mut row_depth = 0.0f32;
    let mut depth = 0.0f32;
    for &(width, own_depth) in widths {
        if row_width > 0.0 && row_width + width > UNIT_ROW_HALF_WIDTH * 2.0 {
            centre_row(&mut placed, row_start, row_width, row_depth, depth);
            depth += row_depth;
            row_start = placed.len();
            row_width = 0.0;
            row_depth = 0.0;
        }
        placed.push((width, 0.0));
        row_width += width;
        row_depth = row_depth.max(own_depth);
    }
    centre_row(&mut placed, row_start, row_width, row_depth, depth);
    (placed, depth + row_depth)
}

fn centre_row(placed: &mut [(f32, f32)], from: usize, width: f32, row_depth: f32, depth: f32) {
    let mut cursor = -width * 0.5;
    for (lateral, at) in placed.iter_mut().skip(from) {
        let own = *lateral;
        *lateral = cursor + own * 0.5;
        *at = depth + row_depth * 0.5;
        cursor += own;
    }
}

const UNIT_START_DEPTH: f32 = 120.0;

const DUMMY_GAP: f32 = 250.0;

pub(crate) struct Deployed {
    pub(crate) lines: Vec<String>,
    pub(crate) far_depth: f32,
}

pub(crate) fn dummy_start_depth(after_depth: f32) -> f32 {
    after_depth.max(UNIT_START_DEPTH) + DUMMY_GAP
}

pub(crate) const SPAWN_START_DEPTH: f32 = UNIT_START_DEPTH;

pub(crate) fn deploy_units(
    simulation: &mut Simulation<RotwkRules>,
    player: PlayerId,
    line: &Line,
    start_depth: f32,
    facing: Facing,
    spec: &str,
    role: SpawnRole,
) -> Result<Deployed, String> {
    let flag = match role {
        SpawnRole::Normal => "spawn",
        SpawnRole::Dummy | SpawnRole::FightingDummy => "dummies",
    };
    let effective = if role.is_dummy() && spec.trim().is_empty() {
        format!("{DEFAULT_DUMMY_COUNT} {DEFAULT_DUMMY_TEMPLATE}")
    } else {
        spec.to_owned()
    };
    let items = parse_spec(&effective).map_err(|e| format!("--{flag}: {e}"))?;
    let catalogue = simulation.rules().unit_catalogue();
    let resolved = resolve(&items, &catalogue).map_err(|e| format!("--{flag}: {e}"))?;
    let requested: u32 = resolved.iter().map(|(count, _)| *count).sum();
    let cap = requested.min(MAX_UNIT_SPAWNS);

    let mut lines = Vec::new();
    let mut spawns: Vec<(usize, (f32, f32))> = Vec::new();
    for (count, index) in &resolved {
        let Some(entry) = catalogue.get(*index) else {
            continue;
        };
        let (across, along) = simulation
            .rules()
            .unit_spawn_reach(entry.kind)
            .unwrap_or((0.0, 0.0));
        let size = (
            (across * 2.0 + UNIT_MARGIN).max(UNIT_SLOT_SPACING),
            (along * 2.0 + UNIT_MARGIN).max(UNIT_ROW_DEPTH),
        );
        let room = usize::try_from(cap)
            .unwrap_or(usize::MAX)
            .saturating_sub(spawns.len());
        let copies = usize::try_from(*count).unwrap_or(usize::MAX).min(room);
        spawns.extend(std::iter::repeat_n((*index, size), copies));
    }
    let sizes: Vec<(f32, f32)> = spawns.iter().map(|(_, size)| *size).collect();
    let (centres, rows_depth) = pack_rows(&sizes);
    let mut placed_of: Vec<(usize, u32)> = Vec::new();
    for ((index, _), (lateral, depth)) in spawns.iter().zip(&centres) {
        let Some(entry) = catalogue.get(*index) else {
            continue;
        };
        let position = crate::skirmish::standable(
            simulation.rules().passability(),
            line.slot(start_depth + depth, *lateral),
        );
        let command = RotwkCommand::Debug(DebugCommand::Spawn {
            kind: entry.kind.0,
            x: position.x,
            y: position.y,
            facing: facing.radians(),
            role,
        });
        simulation
            .submit(player, command)
            .map_err(|e| e.to_string())?;
        match placed_of.last_mut() {
            Some((last, n)) if last == index => *n = n.saturating_add(1),
            _ => placed_of.push((*index, 1)),
        }
    }
    for (index, placed) in placed_of {
        if let Some(entry) = catalogue.get(index) {
            lines.push(format!("{flag}:   {} x{placed}", entry.label));
        }
    }
    let slot = u32::try_from(spawns.len()).unwrap_or(u32::MAX);
    simulation.tick().map_err(|e| e.to_string())?;
    if requested > cap {
        lines.push(format!(
            "{flag}: {requested} requested, {cap} placed - past the {MAX_UNIT_SPAWNS} cap"
        ));
    }
    lines.insert(
        0,
        format!("{flag}: {slot} spawn(s) for player {}", player.get()),
    );
    let far_depth = start_depth + rows_depth;
    Ok(Deployed { lines, far_depth })
}

pub(crate) fn named_templates(spec: &str, is_template: impl Fn(&str) -> bool) -> Vec<String> {
    parse_spec(spec)
        .unwrap_or_default()
        .into_iter()
        .map(|item| item.name)
        .filter(|name| is_template(name))
        .collect()
}

const BUILDING_MARGIN: f32 = 20.0;
const BUILDING_ROW_HALF_WIDTH: f32 = 700.0;
const BUILDING_START_DEPTH: f32 = 200.0;
const BUILDING_MIN_REACH: f32 = 30.0;

pub(crate) fn deploy_buildings(
    simulation: &mut Simulation<RotwkRules>,
    player: PlayerId,
    at: Vec2,
    enemy_at: Vec2,
    spec: &str,
) -> Result<Deployed, String> {
    let items = parse_spec(spec).map_err(|e| format!("--spawn-buildings: {e}"))?;
    let catalogue = simulation.rules().structure_catalogue();
    let resolved = resolve(&items, &catalogue).map_err(|e| format!("--spawn-buildings: {e}"))?;
    let requested: u32 = resolved.iter().map(|(count, _)| *count).sum();
    let cap = requested.min(MAX_BUILDING_SPAWNS);

    let line = Line::towards(at, enemy_at);
    let angle = line.facing().radians();
    let mut lines = Vec::new();
    let mut cursor_lateral = -BUILDING_ROW_HALF_WIDTH;
    let mut cursor_depth = BUILDING_START_DEPTH;
    let mut row_max_reach = 0f32;
    let mut spawned = 0u32;
    'outer: for (count, index) in resolved {
        let Some(entry) = catalogue.get(index) else {
            continue;
        };
        let reach = simulation
            .rules()
            .structure_footprint_reach(entry.kind)
            .max(BUILDING_MIN_REACH);
        let width = reach * 2.0 + BUILDING_MARGIN;
        let mut placed = 0u32;
        for _ in 0..count {
            if spawned >= cap {
                break 'outer;
            }
            if cursor_lateral + width > BUILDING_ROW_HALF_WIDTH {
                cursor_lateral = -BUILDING_ROW_HALF_WIDTH;
                cursor_depth += row_max_reach * 2.0 + BUILDING_MARGIN;
                row_max_reach = 0.0;
            }
            let lateral = cursor_lateral + reach;
            let position = line.slot(cursor_depth, lateral);
            cursor_lateral += width;
            row_max_reach = row_max_reach.max(reach);
            simulation
                .submit(
                    player,
                    RotwkCommand::DebugSpawnStructure {
                        kind: entry.kind.0,
                        x: position.x,
                        y: position.y,
                        angle,
                    },
                )
                .map_err(|e| e.to_string())?;
            spawned = spawned.saturating_add(1);
            placed = placed.saturating_add(1);
        }
        if placed > 0 {
            lines.push(format!("spawn-buildings:   {} x{placed}", entry.label));
        }
    }
    simulation.tick().map_err(|e| e.to_string())?;
    if requested > cap {
        lines.push(format!(
            "spawn-buildings: {requested} requested, {cap} placed - past the {MAX_BUILDING_SPAWNS} cap"
        ));
    }
    lines.insert(
        0,
        format!(
            "spawn-buildings: {spawned} placement(s) submitted for player {} - a refusal line \
             above names any that did not fit",
            player.get()
        ),
    );
    Ok(Deployed {
        lines,
        far_depth: cursor_depth + row_max_reach * 2.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(label: &str, side: Option<&str>) -> UnitEntry {
        UnitEntry {
            kind: crate::unit::Kind(0),
            label: label.to_owned(),
            side: side.map(str::to_owned),
            listed: true,
        }
    }

    #[test]
    fn packed_rows_keep_every_spawn_clear_of_its_neighbours() {
        let (centres, depth) =
            pack_rows(&[(100.0, 80.0), (60.0, 55.0), (1100.0, 120.0), (100.0, 80.0)]);
        assert_eq!(centres[0], (-30.0, 40.0));
        assert_eq!(centres[1], (50.0, 40.0));
        assert_eq!(centres[2], (-50.0, 140.0));
        assert_eq!(centres[3], (550.0, 140.0));
        assert!((depth - 200.0).abs() < 1e-3);
    }

    #[test]
    fn parse_spec_reads_counts_and_defaults_to_one() {
        let items = parse_spec("2 ElvenMirkwoodArcher 5 AngmarHillTroll Gandalf").unwrap();
        assert_eq!(
            items,
            vec![
                SpecItem {
                    count: 2,
                    name: String::from("ElvenMirkwoodArcher")
                },
                SpecItem {
                    count: 5,
                    name: String::from("AngmarHillTroll")
                },
                SpecItem {
                    count: 1,
                    name: String::from("Gandalf")
                },
            ]
        );
    }

    #[test]
    fn parse_spec_accepts_commas_and_mixed_whitespace() {
        let items = parse_spec(" 3,Foo ,  Bar,4 Baz ").unwrap();
        assert_eq!(
            items,
            vec![
                SpecItem {
                    count: 3,
                    name: String::from("Foo")
                },
                SpecItem {
                    count: 1,
                    name: String::from("Bar")
                },
                SpecItem {
                    count: 4,
                    name: String::from("Baz")
                },
            ]
        );
    }

    #[test]
    fn parse_spec_refuses_a_count_with_nothing_after_it() {
        assert!(parse_spec("3").is_err());
    }

    #[test]
    fn parse_spec_refuses_a_zero_count_and_an_empty_spec() {
        assert!(parse_spec("0 Foo").is_err());
        assert!(parse_spec("   ").is_err());
    }

    #[test]
    fn resolve_matches_an_exact_template_name() {
        let catalogue = vec![entry("Foo", Some("Men")), entry("Bar", Some("Men"))];
        let items = vec![SpecItem {
            count: 2,
            name: String::from("Bar"),
        }];
        assert_eq!(resolve(&items, &catalogue).unwrap(), vec![(2, 1)]);
    }

    #[test]
    fn resolve_matches_a_faction_to_every_entry_of_that_side() {
        let catalogue = vec![
            entry("Foo", Some("Men")),
            entry("Bar", Some("Mordor")),
            entry("Baz", Some("Men")),
        ];
        let items = vec![SpecItem {
            count: 1,
            name: String::from("Men"),
        }];
        assert_eq!(resolve(&items, &catalogue).unwrap(), vec![(1, 0), (1, 2)]);
    }

    #[test]
    fn resolve_all_is_the_union_over_every_entry() {
        let catalogue = vec![entry("Foo", Some("Men")), entry("Bar", Some("Mordor"))];
        let items = vec![SpecItem {
            count: 1,
            name: String::from("all"),
        }];
        assert_eq!(resolve(&items, &catalogue).unwrap(), vec![(1, 0), (1, 1)]);
    }

    #[test]
    fn resolve_refuses_an_unknown_name_with_nearest_matches() {
        let catalogue = vec![entry("ElvenMirkwoodArcher", Some("Elves"))];
        let items = vec![SpecItem {
            count: 1,
            name: String::from("ElvenMirkwoodArche"),
        }];
        let error = resolve(&items, &catalogue).unwrap_err();
        assert!(error.contains("ElvenMirkwoodArcher"), "{error}");
    }
}
