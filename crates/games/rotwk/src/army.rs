use mint_movement::{Facing, Vec2};
use mint_sim::{PlayerId, Simulation};

use crate::rules::{RosterEntry, RotwkCommand, RotwkRules};
use crate::skirmish::standable;

pub(crate) const DEFAULT_ARMY_PER_SIDE: u32 = 5_000;

const MAX_SPAWNS_PER_SIDE: u32 = 4_000;

const MAX_HEROES_PER_SIDE: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Category {
    Siege,
    Archer,
    Pike,
    Infantry,
    Cavalry,
    Hero,
}

const SHARES: [(Category, f32); 5] = [
    (Category::Infantry, 0.40),
    (Category::Pike, 0.15),
    (Category::Archer, 0.25),
    (Category::Cavalry, 0.15),
    (Category::Siege, 0.05),
];

impl Category {
    fn of(kind_of: &[String]) -> Option<Self> {
        let has = |flag: &str| kind_of.iter().any(|f| f == flag);
        if has("DOZER") {
            None
        } else if has("HERO") {
            Some(Self::Hero)
        } else if has("SIEGEENGINE") {
            Some(Self::Siege)
        } else if has("CAVALRY") {
            Some(Self::Cavalry)
        } else if has("PIKE") {
            Some(Self::Pike)
        } else if has("ARCHER") {
            Some(Self::Archer)
        } else if has("INFANTRY") {
            Some(Self::Infantry)
        } else {
            None
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Siege => "siege",
            Self::Archer => "archers",
            Self::Pike => "pikes",
            Self::Infantry => "infantry",
            Self::Cavalry => "cavalry",
            Self::Hero => "heroes",
        }
    }
}

const SLOT_SPACING: f32 = 60.0;
const ROW_WIDTH: u32 = 14;
const ROW_DEPTH: f32 = 55.0;

const CATEGORY_DEPTH: [(Category, f32); 3] = [
    (Category::Siege, 60.0),
    (Category::Archer, 260.0),
    (Category::Infantry, 460.0),
];

const FLANK_MARGIN: f32 = 90.0;

struct Assignment {
    entry: RosterEntry,
    category: Category,
    spawns: u32,
    units: u32,
}

fn assign(
    category: Category,
    entries: &[RosterEntry],
    target_units: u32,
    spawn_budget: &mut u32,
) -> Vec<Assignment> {
    if entries.is_empty() || target_units == 0 {
        return Vec::new();
    }
    #[allow(clippy::cast_possible_truncation)]
    let share = entries.len() as u32;
    let base = target_units / share;
    let remainder = target_units % share;
    let mut out = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let index = index as u32;
        let want_units = base + u32::from(index < remainder);
        if want_units == 0 || *spawn_budget == 0 {
            continue;
        }
        let member_count = entry.member_count.max(1);
        let spawns = want_units.div_ceil(member_count).min(*spawn_budget).max(1);
        *spawn_budget = spawn_budget.saturating_sub(spawns);
        out.push(Assignment {
            entry: entry.clone(),
            category,
            spawns,
            units: spawns.saturating_mul(member_count),
        });
    }
    out
}

pub(crate) struct Line {
    origin: Vec2,
    forward: Vec2,
    right: Vec2,
}

impl Line {
    pub(crate) fn towards(origin: Vec2, enemy: Vec2) -> Self {
        let forward = enemy
            .minus(origin)
            .normalised()
            .unwrap_or(Vec2::new(1.0, 0.0));
        let right = Vec2::new(-forward.y, forward.x);
        Self {
            origin,
            forward,
            right,
        }
    }

    pub(crate) fn slot(&self, depth: f32, lateral: f32) -> Vec2 {
        self.origin
            .plus(self.forward.scale(depth))
            .plus(self.right.scale(lateral))
    }

    pub(crate) fn facing(&self) -> Facing {
        Facing::from_vector(self.forward).unwrap_or(Facing::from_radians(0.0))
    }
}

pub(crate) struct SideStart {
    pub(crate) player: PlayerId,
    pub(crate) at: Vec2,
    pub(crate) side: Option<String>,
}

pub(crate) fn deploy(
    simulation: &mut Simulation<RotwkRules>,
    sides: &[SideStart],
    per_side: u32,
) -> Result<Vec<String>, String> {
    let (Some(mine), Some(theirs)) = (sides.first(), sides.get(1)) else {
        return Ok(vec![String::from(
            "army: fewer than two player starts - nothing deployed",
        )]);
    };
    let catalogue = simulation.rules().buildable_catalogue();
    let mut lines = Vec::new();
    let mut total_all_sides = 0u32;

    for (side, enemy_at) in [(mine, theirs.at), (theirs, mine.at)] {
        let Some(side_name) = side.side.as_deref() else {
            lines.push(format!(
                "army: player {} - its faction declares no Side, nothing deployed",
                side.player.get()
            ));
            continue;
        };
        let roster = roster_for_side(&catalogue, side_name);
        let line = Line::towards(side.at, enemy_at);
        let (side_lines, spawned, centre) =
            deploy_side(simulation, side.player, &line, &roster, per_side)?;
        lines.push(format!(
            "army: player {} ({side_name}) - {spawned} unit(s) total, centred on ({:.0}, {:.0})",
            side.player.get(),
            centre.x,
            centre.y
        ));
        lines.extend(side_lines);
        total_all_sides = total_all_sides.saturating_add(spawned);
    }
    lines.push(format!("army: {total_all_sides} unit(s) on the map in all"));
    Ok(lines)
}

fn roster_for_side(catalogue: &[RosterEntry], side_name: &str) -> Vec<RosterEntry> {
    catalogue
        .iter()
        .filter(|entry| entry.side.as_deref() == Some(side_name))
        .cloned()
        .collect()
}

fn deploy_side(
    simulation: &mut Simulation<RotwkRules>,
    player: PlayerId,
    line: &Line,
    roster: &[RosterEntry],
    per_side: u32,
) -> Result<(Vec<String>, u32, Vec2), String> {
    let mut spawn_budget = MAX_SPAWNS_PER_SIDE;
    let mut assignments: Vec<Assignment> = Vec::new();

    for &(category, share) in &SHARES {
        let entries: Vec<RosterEntry> = roster
            .iter()
            .filter(|entry| Category::of(&entry.kind_of) == Some(category))
            .cloned()
            .collect();
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let target = (f64::from(per_side) * f64::from(share)).round() as u32;
        assignments.extend(assign(category, &entries, target, &mut spawn_budget));
    }
    let heroes: Vec<RosterEntry> = roster
        .iter()
        .filter(|entry| Category::of(&entry.kind_of) == Some(Category::Hero))
        .cloned()
        .collect();
    let one_hero: &[RosterEntry] = heroes.get(..1).unwrap_or(&heroes);
    assignments.extend(assign(
        Category::Hero,
        one_hero,
        MAX_HEROES_PER_SIDE,
        &mut spawn_budget,
    ));

    let mut lines = Vec::new();
    let mut total_units = 0u32;
    let mut slot_index_by_category: [u32; 6] = [0; 6];
    let facing = line.facing().radians();
    let (mut sum, mut spawned) = (Vec2::ZERO, 0u32);

    for assignment in &assignments {
        lines.push(format!(
            "army:   {} ({}) -> {} spawn(s) x{} member(s) = {} unit(s)",
            assignment.entry.label,
            assignment.category.label(),
            assignment.spawns,
            assignment.entry.member_count,
            assignment.units,
        ));
        total_units = total_units.saturating_add(assignment.units);
        let depth = depth_of(assignment.category, line);
        let Some(slot_start) = slot_index_by_category.get_mut(category_index(assignment.category))
        else {
            continue;
        };
        for _ in 0..assignment.spawns {
            let slot = *slot_start;
            *slot_start = slot_start.saturating_add(1);
            let (lateral, row_depth) = lateral_and_row(assignment.category, slot);
            let position = line.slot(depth + row_depth, lateral);
            let position = standable(simulation.rules().passability(), position);
            sum = sum.plus(position);
            spawned = spawned.saturating_add(1);
            let command = if assignment.entry.is_horde {
                RotwkCommand::DebugSpawnHorde {
                    recipe: assignment.entry.index,
                    x: position.x,
                    y: position.y,
                    facing,
                }
            } else {
                RotwkCommand::DebugSpawnUnit {
                    buildable: assignment.entry.index,
                    x: position.x,
                    y: position.y,
                    facing,
                }
            };
            simulation
                .submit(player, command)
                .map_err(|e| e.to_string())?;
        }
    }
    simulation.tick().map_err(|e| e.to_string())?;
    #[allow(clippy::cast_precision_loss)]
    let centre = sum.scale(1.0 / (spawned.max(1) as f32));
    Ok((lines, total_units, centre))
}

fn depth_of(category: Category, _line: &Line) -> f32 {
    match category {
        Category::Pike | Category::Cavalry | Category::Hero => CATEGORY_DEPTH
            .iter()
            .find(|(row, _)| *row == Category::Infantry)
            .map_or(0.0, |(_, depth)| *depth),
        other => CATEGORY_DEPTH
            .iter()
            .find(|(row, _)| *row == other)
            .map_or(0.0, |(_, depth)| *depth),
    }
}

const fn category_index(category: Category) -> usize {
    match category {
        Category::Siege => 0,
        Category::Archer => 1,
        Category::Pike => 2,
        Category::Infantry => 3,
        Category::Cavalry => 4,
        Category::Hero => 5,
    }
}

fn lateral_and_row(category: Category, slot: u32) -> (f32, f32) {
    let row = slot / ROW_WIDTH;
    let col = slot % ROW_WIDTH;
    #[allow(clippy::cast_precision_loss)]
    let row_depth = (row as f32) * ROW_DEPTH;
    #[allow(clippy::cast_precision_loss)]
    let centred_col = (col as f32) - (ROW_WIDTH as f32 - 1.0) / 2.0;
    let lateral = centred_col * SLOT_SPACING;
    match category {
        Category::Cavalry => {
            #[allow(clippy::cast_precision_loss)]
            let half_width = (ROW_WIDTH as f32) * SLOT_SPACING / 2.0;
            let side = if slot % 2 == 0 { 1.0 } else { -1.0 };
            let flank_row = slot / 2;
            #[allow(clippy::cast_precision_loss)]
            let flank_depth = (flank_row as f32) * SLOT_SPACING;
            (side * (half_width + FLANK_MARGIN), flank_depth)
        }
        Category::Hero => (0.0, row_depth + ROW_DEPTH),
        _ => (lateral, row_depth),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(index: u16, kind_of: &[&str], member_count: u32) -> RosterEntry {
        RosterEntry {
            index,
            label: format!("Synthetic{index}"),
            is_horde: member_count > 1,
            member_count,
            side: Some(String::from("Test")),
            kind_of: kind_of.iter().map(|flag| (*flag).to_owned()).collect(),
        }
    }

    #[test]
    fn classification_reads_kindof_flags_not_the_template_name() {
        assert_eq!(
            Category::of(&[String::from("INFANTRY")]),
            Some(Category::Infantry)
        );
        assert_eq!(
            Category::of(&[String::from("INFANTRY"), String::from("PIKE")]),
            Some(Category::Pike),
            "a pike horde also carries INFANTRY in the shipped data - PIKE must win"
        );
        assert_eq!(
            Category::of(&[String::from("INFANTRY"), String::from("CAVALRY")]),
            Some(Category::Cavalry),
            "a cavalry horde also carries INFANTRY in the shipped data - CAVALRY must win"
        );
        assert_eq!(
            Category::of(&[String::from("ARCHER")]),
            Some(Category::Archer)
        );
        assert_eq!(
            Category::of(&[String::from("SIEGEENGINE")]),
            Some(Category::Siege)
        );
        assert_eq!(
            Category::of(&[String::from("HERO"), String::from("INFANTRY")]),
            Some(Category::Hero),
            "a hero also carries INFANTRY in the shipped data - HERO must win"
        );
        assert_eq!(
            Category::of(&[String::from("STRUCTURE")]),
            None,
            "a structure is none of the five shapes and must not default to one"
        );
    }

    #[test]
    fn a_dozer_is_excluded_even_though_its_own_kindof_carries_infantry() {
        assert_eq!(
            Category::of(&[String::from("INFANTRY"), String::from("DOZER")]),
            None
        );
    }

    #[test]
    fn roster_for_side_admits_only_an_exact_side_match() {
        let catalogue = vec![entry(0, &["INFANTRY"], 10), entry(1, &["ARCHER"], 10)];
        let mut other_side = entry(2, &["CAVALRY"], 10);
        other_side.side = Some(String::from("Other"));
        let mut no_side = entry(3, &["PIKE"], 10);
        no_side.side = None;
        let catalogue: Vec<RosterEntry> =
            catalogue.into_iter().chain([other_side, no_side]).collect();

        let mine = roster_for_side(&catalogue, "Test");
        assert_eq!(mine.len(), 2, "only the two \"Test\"-side entries belong");
        assert!(mine.iter().all(|e| e.side.as_deref() == Some("Test")));

        let theirs = roster_for_side(&catalogue, "Other");
        assert_eq!(theirs.len(), 1);

        let nobody = roster_for_side(&catalogue, "Nonexistent");
        assert!(nobody.is_empty());
    }

    #[test]
    fn assign_splits_the_remainder_onto_the_earliest_entries() {
        let entries = vec![entry(0, &["INFANTRY"], 10), entry(1, &["INFANTRY"], 10)];
        let mut budget = 100;
        let out = assign(Category::Infantry, &entries, 21, &mut budget);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].entry.index, 0);
        assert_eq!(out[0].units, 20, "entry 0 should carry the remainder");
        assert_eq!(out[1].entry.index, 1);
        assert_eq!(out[1].units, 10);
        assert_eq!(budget, 100 - 2 - 1, "3 spawns issued in total");
    }

    #[test]
    fn assign_never_spends_past_its_own_budget() {
        let entries = vec![entry(0, &["INFANTRY"], 1), entry(1, &["INFANTRY"], 1)];
        let mut budget = 1;
        let out = assign(Category::Infantry, &entries, 1_000, &mut budget);
        let spent: u32 = out.iter().map(|a| a.spawns).sum();
        assert_eq!(spent, 1, "the shared budget is 1 spawn, not one per entry");
        assert_eq!(budget, 0);
    }

    #[test]
    fn assign_returns_nothing_for_an_empty_roster_or_a_zero_target() {
        let entries = vec![entry(0, &["INFANTRY"], 10)];
        let mut budget = 100;
        assert!(assign(Category::Infantry, &[], 500, &mut budget).is_empty());
        assert!(assign(Category::Infantry, &entries, 0, &mut budget).is_empty());
        assert_eq!(budget, 100, "neither call should have spent anything");
    }
}
