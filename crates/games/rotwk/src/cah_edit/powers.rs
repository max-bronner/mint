use std::collections::BTreeMap;
use std::sync::Arc;

use mint_sage_cah::PowerEntry;

use super::edit::HeroEdit;
use super::system::{eq, CahSystem};

pub(crate) const NO_POWER_BUTTON: &str = "Command_SpecialAbilityNoPowerDummy";

const COLUMNS: usize = 4;
const MAX_PICKS: usize = 10;
const NO_POWER_POSITION: u32 = 7;
const COLUMN_FOR_LEVEL: [usize; 10] = [0, 0, 1, 1, 1, 1, 2, 2, 2, 3];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CellState {
    NoButton = 0,
    Available = 1,
    NeedsPrereq = 2,
    LevelTooLow = 3,
    Selected = 4,
    PalantirFull = 5,
    BookFull = 6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PowerSelect {
    Committed { flash: Option<u32> },
    Rejected(CellState),
    Ignored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WizardLine {
    NoAvailablePowers,
    SelectFirstPower,
    SelectSecondPower,
    SelectionComplete,
    SelectNextPower { group: usize, level: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WizardSuffix {
    PalantirFull,
    UpgradeOrNewPower,
}

#[derive(Debug, Clone, Copy)]
struct Cell {
    button: usize,
    at: Option<(usize, usize)>,
    position: Option<u32>,
}

#[derive(Debug, Clone)]
pub(crate) struct PowerPage {
    cells: Vec<Cell>,
    rows: Vec<[Option<usize>; COLUMNS]>,
    no_power: Option<usize>,
    picks: Vec<usize>,
    used: usize,
}

fn sort_key(row: &[Option<usize>; COLUMNS]) -> [bool; COLUMNS] {
    row.map(|cell| cell.is_none())
}

impl PowerPage {
    fn build(sys: &CahSystem, class_upgrade: Option<&str>) -> Self {
        let mut by_name: BTreeMap<&str, usize> = BTreeMap::new();
        if let Some(upgrade) = class_upgrade {
            for (i, b) in sys.buttons.iter().enumerate() {
                if b.allowable_upgrades.iter().any(|u| eq(u, upgrade)) {
                    by_name.entry(b.name.as_str()).or_insert(i);
                }
            }
        }
        let index: BTreeMap<&str, usize> = by_name
            .keys()
            .enumerate()
            .map(|(cell, &name)| (name, cell))
            .collect();
        let mut cells: Vec<Cell> = by_name
            .values()
            .map(|&button| Cell {
                button,
                at: None,
                position: None,
            })
            .collect();
        let no_power = index.get(NO_POWER_BUTTON).copied();
        let mut rows: Vec<[Option<usize>; COLUMNS]> = Vec::new();

        for _ in 0..=cells.len() {
            let mut pending = false;
            for c in 0..cells.len() {
                let Some(cell) = cells.get(c).copied() else {
                    continue;
                };
                if cell.at.is_some() || Some(c) == no_power {
                    continue;
                }
                let prerequisite = sys
                    .buttons
                    .get(cell.button)
                    .and_then(|b| b.prerequisite.as_deref())
                    .filter(|n| !n.is_empty())
                    .and_then(|n| index.get(n).copied());
                let target = match prerequisite {
                    None => {
                        rows.push([None; COLUMNS]);
                        Some((rows.len().saturating_sub(1), 0))
                    }
                    Some(p) => match cells.get(p).and_then(|q| q.at) {
                        Some((r, col)) => Some((r, col.saturating_add(1))),
                        None => {
                            pending = true;
                            None
                        }
                    },
                };
                let Some((r, col)) = target else { continue };
                if let Some(slot) = rows.get_mut(r).and_then(|row| row.get_mut(col)) {
                    if slot.is_none() {
                        *slot = Some(c);
                        if let Some(cell) = cells.get_mut(c) {
                            cell.at = Some((r, col));
                        }
                    }
                }
            }
            if !pending {
                break;
            }
        }

        for row in &mut rows {
            let mut last = COLUMNS.saturating_sub(1);
            for k in (0..COLUMNS).rev() {
                let Some(c) = row.get(k).copied().flatten() else {
                    continue;
                };
                if k == last {
                    continue;
                }
                let level = sys.buttons.get(cells.get(c).map_or(0, |x| x.button));
                let level = level.map_or(0, |b| b.min_level);
                if !(0..12).contains(&level) {
                    continue;
                }
                let want = usize::try_from(level.saturating_sub(1))
                    .ok()
                    .and_then(|i| COLUMN_FOR_LEVEL.get(i).copied())
                    .unwrap_or(COLUMNS);
                let target = want.clamp(k, last);
                if target != k {
                    if let Some(slot) = row.get_mut(k) {
                        *slot = None;
                    }
                    if let Some(old) = row.get_mut(target).and_then(|s| s.replace(c)) {
                        if let Some(dropped) = cells.get_mut(old) {
                            dropped.at = None;
                        }
                    }
                }
                last = target.saturating_sub(1);
            }
        }

        rows.reverse();
        rows.sort_by_key(sort_key);
        for (r, row) in rows.iter().enumerate() {
            for (col, cell) in row.iter().enumerate() {
                if let Some(cell) = cell.and_then(|c| cells.get_mut(c)) {
                    cell.at = Some((r, col));
                }
            }
        }
        Self {
            cells,
            rows,
            no_power,
            picks: Vec::new(),
            used: 0,
        }
    }

    fn left_of(&self, cell: usize) -> Option<usize> {
        let (r, col) = self.cells.get(cell)?.at?;
        let row = self.rows.get(r)?;
        (0..col).rev().find_map(|k| row.get(k).copied().flatten())
    }
}

fn power_cost(sys: &CahSystem, name: &str, level: i64) -> u32 {
    let Some(button) = sys.buttons.iter().find(|b| eq(&b.name, name)) else {
        return 0;
    };
    let cost = button.cost_if_selected;
    if cost == 0 {
        return 0;
    }
    if level < button.min_level {
        return cost;
    }
    let steps = u32::try_from(level.saturating_sub(button.min_level)).unwrap_or(u32::MAX);
    let off = cost.saturating_mul(steps.saturating_mul(sys.discount_per_level)) / 100;
    cost.saturating_sub(off)
}

impl HeroEdit {
    pub(crate) fn open_powers(&mut self) {
        let sys = Arc::clone(&self.sys);
        let (class, _) = self.class_subclass();
        let upgrade = sys
            .def
            .classes
            .get(class)
            .and_then(|c| c.upgrade.as_deref());
        let mut page = PowerPage::build(&sys, upgrade);
        let entries: Vec<Vec<u8>> = self
            .record
            .powers
            .iter()
            .take_while(|e| !e.button.is_empty())
            .map(|e| e.button.clone())
            .collect();
        self.record.powers = Default::default();
        self.page = None;
        for name in entries {
            let cell = page.cells.iter().position(|c| {
                sys.buttons
                    .get(c.button)
                    .is_some_and(|b| b.name.as_bytes() == name.as_slice())
            });
            if let Some(cell) = cell {
                self.commit_pick(&mut page, cell);
            }
        }
        self.page = Some(page);
    }

    #[must_use]
    pub(crate) fn power_rows(&self) -> usize {
        self.page.as_ref().map_or(0, |p| p.rows.len())
    }

    #[must_use]
    pub(crate) fn power_button(&self, row: usize, col: usize) -> Option<&str> {
        let page = self.page.as_ref()?;
        let cell = page.rows.get(row)?.get(col).copied().flatten()?;
        self.cell_button(page, cell)
    }

    fn cell_button<'a>(&'a self, page: &PowerPage, cell: usize) -> Option<&'a str> {
        let button = page.cells.get(cell)?.button;
        self.sys.buttons.get(button).map(|b| b.name.as_str())
    }

    #[must_use]
    pub(crate) fn power_count(&self) -> usize {
        self.page.as_ref().map_or(0, |p| p.picks.len())
    }

    #[must_use]
    pub(crate) fn palantir_used(&self) -> usize {
        self.page.as_ref().map_or(0, |p| p.used)
    }

    #[must_use]
    pub(crate) fn power_state(&self, row: usize, col: usize) -> CellState {
        let Some(page) = self.page.as_ref() else {
            return CellState::NoButton;
        };
        match page
            .rows
            .get(row)
            .and_then(|r| r.get(col))
            .copied()
            .flatten()
        {
            Some(cell) => self.cell_state(page, cell),
            None => CellState::NoButton,
        }
    }

    fn cell_state(&self, page: &PowerPage, cell: usize) -> CellState {
        let Some(button) = page
            .cells
            .get(cell)
            .and_then(|c| self.sys.buttons.get(c.button))
        else {
            return CellState::NoButton;
        };
        let count = page.picks.len();
        let left = page.left_of(cell);
        if page.picks.contains(&cell) && page.no_power != Some(cell) {
            CellState::Selected
        } else if i64::try_from(count).unwrap_or(i64::MAX) < button.min_level.saturating_sub(1) {
            CellState::LevelTooLow
        } else if left.is_some_and(|l| !page.picks.contains(&l)) {
            CellState::NeedsPrereq
        } else if count >= MAX_PICKS {
            CellState::BookFull
        } else if page.used >= self.sys.palantir_cap() && left.is_none() {
            CellState::PalantirFull
        } else {
            CellState::Available
        }
    }

    pub(crate) fn select_power(&mut self, row: usize, col: usize) -> PowerSelect {
        let Some(mut page) = self.page.take() else {
            return PowerSelect::Ignored;
        };
        let cell = page
            .rows
            .get(row)
            .and_then(|r| r.get(col))
            .copied()
            .flatten();
        let outcome = match cell.map(|c| (c, self.cell_state(&page, c))) {
            None | Some((_, CellState::NoButton | CellState::Selected)) => PowerSelect::Ignored,
            Some((cell, CellState::Available)) => self.commit_pick(&mut page, cell),
            Some((_, state)) => PowerSelect::Rejected(state),
        };
        self.page = Some(page);
        outcome
    }

    pub(crate) fn select_no_power(&mut self) -> PowerSelect {
        let Some(mut page) = self.page.take() else {
            return PowerSelect::Ignored;
        };
        let outcome = match page.no_power {
            Some(cell) => self.commit_pick(&mut page, cell),
            None => PowerSelect::Ignored,
        };
        self.page = Some(page);
        outcome
    }

    fn commit_pick(&mut self, page: &mut PowerPage, cell: usize) -> PowerSelect {
        let count = page.picks.len();
        let is_no_power = page.no_power == Some(cell);
        if count >= MAX_PICKS || (page.picks.contains(&cell) && !is_no_power) {
            return PowerSelect::Ignored;
        }
        let Some(name) = self.cell_button(page, cell).map(str::to_owned) else {
            return PowerSelect::Ignored;
        };
        let position = match page.left_of(cell).and_then(|l| page.cells.get(l)) {
            Some(left) => left.position,
            None if is_no_power => Some(NO_POWER_POSITION),
            None => {
                if page.used >= self.sys.palantir_cap() {
                    return PowerSelect::Ignored;
                }
                let position = u32::try_from(page.used).ok();
                page.used = page.used.saturating_add(1);
                position
            }
        };
        let Some(position) = position else {
            return PowerSelect::Ignored;
        };
        if let Some(c) = page.cells.get_mut(cell) {
            c.position = Some(position);
        }
        page.picks.push(cell);
        let slot = u32::try_from(self.sys.leading_template_slots).unwrap_or(0);
        if let Some(entry) = self.record.powers.get_mut(count) {
            *entry = PowerEntry {
                button: name.into_bytes(),
                exp_level: u32::try_from(count).unwrap_or(0),
                button_index: position.saturating_add(slot),
            };
        }
        PowerSelect::Committed {
            flash: (!is_no_power).then(|| position.saturating_add(1)),
        }
    }

    pub(crate) fn undo_power(&mut self, n: usize) {
        if n == self.power_count() && n > 0 {
            self.rollback(n.saturating_sub(1));
        }
    }

    pub(crate) fn reset_powers(&mut self) {
        self.rollback(0);
    }

    fn rollback(&mut self, target: usize) {
        let Some(page) = self.page.as_mut() else {
            return;
        };
        while page.picks.len() > target {
            let Some(cell) = page.picks.pop() else { break };
            let index = page.picks.len();
            if let Some(entry) = self.record.powers.get_mut(index) {
                *entry = PowerEntry::default();
            }
            let in_palantir = page
                .cells
                .get(cell)
                .and_then(|c| self.sys.buttons.get(c.button))
                .is_some_and(|b| b.in_palantir);
            if page.left_of(cell).is_none() && in_palantir {
                page.used = page.used.saturating_sub(1);
            }
            if let Some(c) = page.cells.get_mut(cell) {
                c.position = None;
            }
        }
    }

    #[must_use]
    pub(crate) fn any_power_available(&self) -> bool {
        self.page.as_ref().is_some_and(|page| {
            (0..page.cells.len()).any(|c| {
                page.cells.get(c).is_some_and(|x| x.at.is_some())
                    && self.cell_state(page, c) == CellState::Available
            })
        })
    }

    #[must_use]
    pub(crate) fn wizard(&self) -> Option<(WizardLine, Option<WizardSuffix>)> {
        let page = self.page.as_ref()?;
        let count = page.picks.len();
        let placed =
            || (0..page.cells.len()).filter(|&c| page.cells.get(c).is_some_and(|x| x.at.is_some()));
        if !self.any_power_available() {
            return Some((WizardLine::NoAvailablePowers, None));
        }
        let line = match count {
            0 => WizardLine::SelectFirstPower,
            1 => WizardLine::SelectSecondPower,
            n if n >= MAX_PICKS => WizardLine::SelectionComplete,
            n => WizardLine::SelectNextPower {
                group: COLUMN_FOR_LEVEL.get(n).copied().unwrap_or(0),
                level: n.saturating_add(1),
            },
        };
        let palantir_full = placed().any(|c| self.cell_state(page, c) == CellState::PalantirFull);
        let suffix = if palantir_full {
            Some(WizardSuffix::PalantirFull)
        } else if count > 0 {
            Some(WizardSuffix::UpgradeOrNewPower)
        } else {
            None
        };
        Some((line, suffix))
    }

    #[must_use]
    pub(crate) fn power_cost(&self) -> u32 {
        self.record
            .powers
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.button.is_empty())
            .fold(0u32, |sum, (k, e)| {
                let name = String::from_utf8_lossy(&e.button);
                let level = i64::try_from(k).unwrap_or(0).saturating_add(1);
                sum.saturating_add(power_cost(&self.sys, &name, level))
            })
    }

    #[must_use]
    pub(crate) fn build_cost(&self, base: u32) -> u32 {
        base.saturating_add(self.power_cost())
    }
}
