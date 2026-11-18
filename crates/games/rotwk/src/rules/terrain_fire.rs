use mint_sage_ini::fire_logic_system::{FireLogicSystem, FireMaterial};
use mint_sim::{Component, Entity, HashedVec, StateHasher};

use crate::combat::{DamageEvent, DeathNaming};

pub(crate) const CELL_SIZE: f32 = 10.0;

#[derive(Debug, Clone, Copy, Default)]
struct Cell {
    fuel: u16,
    burn: u16,
    decay: u16,
    resistance: u16,
    max_burn: u16,
    pending: bool,
    burned_out: bool,
    terrain_type: u8,
    overridden: bool,
}

impl Cell {
    fn hash_into(&self, hasher: &mut StateHasher) {
        if hasher.is_legacy() {
            hasher.write_u16(self.fuel);
            hasher.write_u16(self.burn);
            hasher.write_u16(self.decay);
            hasher.write_u16(self.resistance);
            hasher.write_u16(self.max_burn);
            hasher.write_bool(self.pending);
            hasher.write_bool(self.burned_out);
            hasher.write_u8(self.terrain_type);
            hasher.write_bool(self.overridden);
            return;
        }
        hasher.write_u64(
            u64::from(self.fuel)
                | u64::from(self.burn) << 16
                | u64::from(self.decay) << 32
                | u64::from(self.resistance) << 48,
        );
        hasher.write_u64(
            u64::from(self.max_burn)
                | u64::from(self.pending) << 16
                | u64::from(self.burned_out) << 24
                | u64::from(self.terrain_type) << 32
                | u64::from(self.overridden) << 40,
        );
    }
}

pub(crate) const DEFAULT_DAMAGE_ARC: f32 = std::f32::consts::PI;

#[derive(Debug, Clone, Copy)]
pub(crate) struct FireLighting {
    pub(crate) logic_type: FireLogicKind,
    pub(crate) radius: f32,
    pub(crate) damage: f32,
    pub(crate) damage_arc: f32,
    pub(crate) min_max_burn_rate: Option<u16>,
    pub(crate) min_decay: Option<u16>,
    pub(crate) max_resistance: u16,
}

impl FireLighting {
    pub(crate) fn clear_fuel(radius: f32) -> Self {
        Self {
            logic_type: FireLogicKind::IncreaseFuel {
                existing_only: false,
            },
            radius,
            damage: -2_147_483_648.0,
            damage_arc: DEFAULT_DAMAGE_ARC,
            min_max_burn_rate: None,
            min_decay: None,
            max_resistance: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FireLogicKind {
    IncreaseFuel { existing_only: bool },
    IncreaseBurnRate { existing_only: bool, arc: bool },
    DecreaseBurnRate,
}

impl FireLogicKind {
    #[must_use]
    pub(crate) fn from_keyword(keyword: &str, damage_arc: f32) -> Option<Self> {
        let arc = (0.0..std::f32::consts::PI).contains(&damage_arc);
        match keyword {
            "INCREASE_FUEL" => Some(Self::IncreaseFuel {
                existing_only: false,
            }),
            "INCREASE_FUEL_ON_EXISTING_FIRE" => Some(Self::IncreaseFuel {
                existing_only: true,
            }),
            "INCREASE_BURN_RATE" => Some(Self::IncreaseBurnRate {
                existing_only: false,
                arc,
            }),
            "INCREASE_BURN_RATE_ON_EXISTING_FIRE" => Some(Self::IncreaseBurnRate {
                existing_only: true,
                arc,
            }),
            "DECREASE_BURN_RATE" => Some(Self::DecreaseBurnRate),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FireGrid {
    nx: usize,
    ny: usize,
    cells: HashedVec<Cell>,
    types: [FireMaterial; 4],
    max_cells_burning: i64,
    fuel_regeneration_rate: i64,
    phase: u8,
    burning: Vec<u32>,
    settled: bool,
    overwritten: std::collections::BTreeMap<Entity, Placement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Placement {
    x: u32,
    y: u32,
    facing: u32,
    radius: u32,
    kind_of: mint_sage_asset::KindOf,
    small: bool,
    kind: crate::unit::Kind,
}

impl Component for FireGrid {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.hash_head(hasher);
        self.cells.hash_into(hasher, Cell::hash_into);
        self.hash_tail(hasher);
    }

    fn hash_into_cached(&mut self, hasher: &mut StateHasher) {
        self.hash_head(hasher);
        self.cells.hash_into_cached(hasher, Cell::hash_into);
        self.hash_tail(hasher);
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_u16_saturating(value: f32) -> u16 {
    value.clamp(0.0, f32::from(u16::MAX)) as u16
}

impl FireGrid {
    fn hash_head(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.nx);
        hasher.write_usize(self.ny);
    }

    fn hash_tail(&self, hasher: &mut StateHasher) {
        for material in &self.types {
            hasher.write_u16(material.fuel);
            hasher.write_u16(material.max_burn_rate);
            hasher.write_u16(material.decay);
            hasher.write_u16(material.resistance);
        }
        hasher.write_i64(self.max_cells_burning);
        hasher.write_i64(self.fuel_regeneration_rate);
        hasher.write_u8(self.phase);
        hasher.write_usize(self.burning.len());
        for cell_index in &self.burning {
            hasher.write_u32(*cell_index);
        }
    }

    #[must_use]
    pub(crate) fn build(
        nx: usize,
        ny: usize,
        cell_type: impl Fn(usize, usize) -> Option<u8>,
        system: &FireLogicSystem,
    ) -> Self {
        let mut cells = Vec::with_capacity(nx.saturating_mul(ny));
        for y in 0..ny {
            for x in 0..nx {
                let terrain_type = cell_type(x, y).filter(|t| (1..=3).contains(t)).unwrap_or(0);
                let material = if terrain_type == 0 {
                    FireMaterial::default()
                } else {
                    system
                        .cell_types
                        .get(usize::from(terrain_type))
                        .copied()
                        .unwrap_or_default()
                };
                cells.push(Cell {
                    fuel: material.fuel,
                    decay: material.decay,
                    resistance: material.resistance,
                    max_burn: material.max_burn_rate,
                    burn: 0,
                    pending: false,
                    burned_out: false,
                    terrain_type,
                    overridden: false,
                });
            }
        }
        Self {
            nx,
            ny,
            cells: HashedVec::from(cells),
            types: system.cell_types,
            max_cells_burning: system.max_cells_burning,
            fuel_regeneration_rate: system.fuel_regeneration_rate,
            phase: 0,
            burning: Vec::new(),
            settled: false,
            overwritten: std::collections::BTreeMap::new(),
        }
    }

    fn index(&self, x: usize, y: usize) -> Option<usize> {
        if x >= self.nx || y >= self.ny {
            return None;
        }
        x.checked_add(y.checked_mul(self.nx)?)
    }

    pub(crate) fn overwrite_once(&mut self, x: usize, y: usize, material: FireMaterial) -> bool {
        let Some(index) = self.index(x, y) else {
            return false;
        };
        let Some(cell) = self.cells.get_mut(index) else {
            return false;
        };
        if cell.overridden {
            return false;
        }
        self.settled = false;
        cell.overridden = true;
        cell.fuel = material.fuel;
        cell.max_burn = material.max_burn_rate;
        cell.decay = material.decay;
        cell.resistance = material.resistance;
        true
    }

    fn remove_from_burning(&mut self, cell_index: u32) {
        if let Some(pos) = self.burning.iter().position(|&c| c == cell_index) {
            self.burning.swap_remove(pos);
        }
    }

    pub(crate) fn light(
        &mut self,
        nugget: &FireLighting,
        centre: (f32, f32),
        source_direction: Option<(f32, f32)>,
        underwater: &dyn Fn(f32, f32) -> bool,
    ) {
        if nugget.radius <= 0.0 || nugget.damage == 0.0 {
            return;
        }
        #[allow(clippy::cast_possible_truncation)]
        let cx = (centre.0 * 0.1 + 0.5).floor() as i32;
        #[allow(clippy::cast_possible_truncation)]
        let cy = (centre.1 * 0.1 + 0.5).floor() as i32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let r0 = (nugget.radius * 0.1).ceil() as i32;
        let grid_span = i32::try_from(self.nx.max(self.ny)).unwrap_or(i32::MAX);
        let r0 = r0.clamp(1, grid_span.saturating_add(1));
        let dir = source_direction.unwrap_or((1.0, 0.0));
        let cos_limit = mint_math::f32::cos(nugget.damage_arc);
        disc_spans(cx, cy, r0, |row, x0, x1| {
            self.light_row(nugget, centre, dir, cos_limit, underwater, (row, x0, x1));
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn light_row(
        &mut self,
        nugget: &FireLighting,
        centre: (f32, f32),
        dir: (f32, f32),
        cos_limit: f32,
        underwater: &dyn Fn(f32, f32) -> bool,
        (row, x0, x1): (i32, i32, i32),
    ) {
        let (Ok(y), Ok(lo), Ok(hi)) = (
            usize::try_from(row),
            usize::try_from(x0.max(0)),
            usize::try_from(x1),
        ) else {
            return;
        };
        for x in lo..=hi {
            let Some(index) = self.index(x, y) else {
                continue;
            };
            match nugget.logic_type {
                FireLogicKind::IncreaseFuel { existing_only } => {
                    #[allow(clippy::cast_precision_loss)]
                    let (wx, wy) = (
                        x as f32 * CELL_SIZE + CELL_SIZE * 0.5,
                        y as f32 * CELL_SIZE + CELL_SIZE * 0.5,
                    );
                    if underwater(wx, wy) {
                        continue;
                    }
                    self.apply_increase_fuel(index, nugget, existing_only);
                }
                FireLogicKind::IncreaseBurnRate { existing_only, arc } => {
                    if arc {
                        #[allow(clippy::cast_precision_loss)]
                        let delta = (
                            (x as f32 * CELL_SIZE + CELL_SIZE * 0.5) - centre.0,
                            (y as f32 * CELL_SIZE + CELL_SIZE * 0.5) - centre.1,
                        );
                        let dot = delta.0 * dir.0 + delta.1 * dir.1;
                        if dot < cos_limit {
                            continue;
                        }
                    }
                    self.apply_burn_delta(index, nugget.damage, existing_only);
                }
                FireLogicKind::DecreaseBurnRate => {
                    #[allow(clippy::cast_possible_truncation)]
                    let amount = -(nugget.damage as i32);
                    self.apply_burn_delta_signed(index, amount);
                }
            }
        }
    }

    fn apply_increase_fuel(&mut self, index: usize, nugget: &FireLighting, existing_only: bool) {
        self.settled = false;
        let Some(cell) = self.cells.get_mut(index) else {
            return;
        };
        if existing_only && cell.burn < 1 {
            return;
        }
        cell.fuel = to_u16_saturating(f32::from(cell.fuel) + nugget.damage);
        if let Some(m) = nugget.min_max_burn_rate {
            cell.max_burn = cell.max_burn.max(m);
        }
        if let Some(d) = nugget.min_decay {
            cell.decay = cell.decay.max(d);
        }
        cell.resistance = cell.resistance.min(nugget.max_resistance);
    }

    fn apply_burn_delta(&mut self, index: usize, damage: f32, existing_only: bool) {
        self.settled = false;
        let Some(cell) = self.cells.get_mut(index) else {
            return;
        };
        if existing_only && cell.burn < 1 {
            return;
        }
        if cell.burn == 0 {
            cell.pending = true;
        }
        cell.burn = to_u16_saturating(f32::from(cell.burn) + damage);
    }

    fn apply_burn_delta_signed(&mut self, index: usize, amount: i32) {
        self.settled = false;
        let cell_index = u32::try_from(index).unwrap_or(u32::MAX);
        let Some(cell) = self.cells.get_mut(index) else {
            return;
        };
        if cell.burn == 0 {
            return;
        }
        let magnitude = amount.unsigned_abs();
        if u32::from(cell.burn) <= magnitude {
            let pending = cell.pending;
            cell.pending = false;
            cell.burn = 0;
            if !pending {
                self.remove_from_burning(cell_index);
            }
        } else {
            cell.burn = to_u16_saturating((f32::from(cell.burn) + amount as f32).max(0.0));
        }
    }

    #[must_use]
    pub(crate) fn burning_count(&self) -> usize {
        self.burning.len()
    }

    pub(crate) fn step(&mut self) -> Vec<(usize, u16)> {
        if self.settled {
            self.phase = (self.phase + 1) % 10;
            return Vec::new();
        }
        let drains = self.row_step();
        self.spread_step();
        self.phase = (self.phase + 1) % 10;
        self.settled = !self.any_cell_active();
        drains
    }

    #[must_use]
    pub(crate) fn is_settled(&self) -> bool {
        self.settled
    }

    fn claim(&mut self, candidates: &[Candidate]) -> Vec<(Entity, usize)> {
        let mut claimed = vec![false; self.cells.len()];
        let mut registrations = Vec::new();
        for candidate in candidates {
            for &(x, y) in &candidate.cells {
                let Some(index) = self.index(x, y) else {
                    continue;
                };
                match claimed.get_mut(index) {
                    Some(slot) if !*slot => *slot = true,
                    _ => continue,
                }
                registrations.push((candidate.entity, index));
                if let Some(material) = candidate.immobile_material {
                    self.overwrite_once(x, y, material);
                }
            }
        }
        registrations
    }

    #[must_use]
    pub(crate) fn overwrite_pending(&self, x: usize, y: usize) -> bool {
        self.index(x, y)
            .and_then(|index| self.cells.get(index))
            .is_some_and(|cell| !cell.overridden)
    }

    fn any_cell_active(&self) -> bool {
        let burning = |index: &u32| {
            usize::try_from(*index)
                .ok()
                .and_then(|index| self.cells.get(index))
                .is_some_and(|cell| cell.burn > 0)
        };
        if self.burning.iter().any(burning) {
            return true;
        }
        let regen = self.fuel_regeneration_rate > 0;
        self.cells.iter().any(|cell| {
            cell.burn > 0
                || (cell.fuel == 0
                    && cell.burned_out
                    && (cell.terrain_type == 0
                        || regen
                        || self
                            .types
                            .get(usize::from(cell.terrain_type))
                            .is_none_or(|t| t.fuel == 0)))
        })
    }

    fn band(&self, phase: u8) -> (usize, usize) {
        let nx = self.nx;
        let p = usize::from(phase);
        (
            p.saturating_mul(nx) / 10,
            (p.saturating_add(1)).saturating_mul(nx) / 10,
        )
    }

    fn row_step(&mut self) -> Vec<(usize, u16)> {
        let (start, end) = self.band(self.phase);
        let mut drains = Vec::new();
        for x in start..end.min(self.nx) {
            for y in (0..self.ny).rev() {
                self.row_step_cell(x, y, &mut drains);
            }
        }
        drains
    }

    fn row_step_cell(&mut self, x: usize, y: usize, drains: &mut Vec<(usize, u16)>) {
        let Some(index) = self.index(x, y) else {
            return;
        };
        let cell_index = u32::try_from(index).unwrap_or(u32::MAX);
        let Some(before) = self.cells.get(index).copied() else {
            return;
        };
        if before.burn == 0 {
            return;
        }
        let threshold = before.decay;
        if before.burn <= threshold {
            if let Some(cell) = self.cells.get_mut(index) {
                cell.pending = false;
                cell.burn = 0;
            }
            if !before.pending {
                self.remove_from_burning(cell_index);
            }
            return;
        }
        if before.pending {
            let at_cap = u32::try_from(self.burning.len()).unwrap_or(u32::MAX)
                >= u32::try_from(self.max_cells_burning.max(0)).unwrap_or(u32::MAX);
            if at_cap {
                if let Some(cell) = self.cells.get_mut(index) {
                    cell.pending = false;
                    cell.burn = 0;
                }
                return;
            }
            self.burning.push(cell_index);
        }
        let burn_after_decay = before.burn - threshold;
        let burns_out = before.fuel <= burn_after_decay;
        let amount = if burns_out {
            before.fuel
        } else {
            burn_after_decay
        };
        if amount > 0 {
            drains.push((index, amount));
        }
        if let Some(cell) = self.cells.get_mut(index) {
            cell.pending = false;
            cell.burn = if burns_out { 0 } else { burn_after_decay };
            cell.burned_out = cell.burned_out || burns_out;
            cell.fuel = cell.fuel.saturating_sub(amount);
        }
        if burns_out {
            self.remove_from_burning(cell_index);
        }
    }

    fn spread_band(&self) -> (usize, usize) {
        let (s, e) = self.band(self.phase);
        let start = if s > 0 { s - 1 } else { s };
        let end = if e.saturating_add(1) < self.nx {
            e.saturating_sub(1)
        } else {
            e
        };
        (start, end.max(start))
    }

    fn spread_step(&mut self) {
        let (start, end) = self.spread_band();
        let mut deferred: Vec<(usize, u16)> = Vec::new();
        for x in start..end.min(self.nx) {
            for y in 0..self.ny {
                self.spread_cell(x, y, &mut deferred);
            }
        }
        for (index, burn) in deferred {
            if let Some(cell) = self.cells.get_mut(index) {
                cell.burn = burn;
            }
        }
    }

    fn spread_cell(&mut self, x: usize, y: usize, deferred: &mut Vec<(usize, u16)>) {
        let Some(index) = self.index(x, y) else {
            return;
        };
        let cell_index = u32::try_from(index).unwrap_or(u32::MAX);
        let (fuel, burned_out, type_index) = {
            let Some(cell) = self.cells.get(index) else {
                return;
            };
            (cell.fuel, cell.burned_out, usize::from(cell.terrain_type))
        };
        if fuel == 0 && burned_out {
            let regen = self.fuel_regeneration_rate.max(0);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let regen = regen.min(i64::from(u16::MAX)) as u16;
            let Some(cell) = self.cells.get_mut(index) else {
                return;
            };
            if type_index == 0 {
                cell.burned_out = false;
            } else {
                let cap = self.types.get(type_index).map_or(0, |t| t.fuel);
                cell.fuel = cell.fuel.saturating_add(regen).min(cap);
                if cell.fuel >= cap {
                    cell.burned_out = false;
                }
            }
            return;
        }
        if fuel == 0 {
            return;
        }
        let resistance = self.cells.get(index).map_or(0, |c| c.resistance);
        let mut sum: i32 = 0;
        for (nx_, ny_) in self.neighbours(x, y) {
            if let Some(nb_index) = self.index(nx_, ny_) {
                if let Some(nb) = self.cells.get(nb_index) {
                    sum += (i32::from(nb.burn) - i32::from(resistance)).max(0);
                }
            }
        }
        if sum <= 0 {
            return;
        }
        let Some(cell) = self.cells.get_mut(index) else {
            return;
        };
        if cell.burn == 0 {
            if u32::try_from(self.burning.len()).unwrap_or(u32::MAX)
                >= u32::try_from(self.max_cells_burning.max(0)).unwrap_or(u32::MAX)
            {
                return;
            }
            cell.pending = false;
            self.burning.push(cell_index);
        }
        let max_burn = cell.max_burn;
        let new_burn = (i32::from(cell.burn) + sum).clamp(0, i32::from(max_burn.max(cell.burn)));
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let new_burn = (new_burn as u32).min(u32::from(max_burn.max(cell.burn))) as u16;
        deferred.push((index, new_burn.min(max_burn.max(cell.burn))));
    }

    fn neighbours(&self, x: usize, y: usize) -> [(usize, usize); 4] {
        [
            (x.wrapping_sub(1), y),
            (x.saturating_add(1), y),
            (x, y.wrapping_sub(1)),
            (x, y.saturating_add(1)),
        ]
        .map(|(nx_, ny_)| {
            if nx_ < self.nx && ny_ < self.ny {
                (nx_, ny_)
            } else {
                (usize::MAX, usize::MAX)
            }
        })
    }

    #[cfg(test)]
    pub(crate) fn test_burn(&self, x: usize, y: usize) -> Option<u16> {
        self.index(x, y)
            .and_then(|i| self.cells.get(i))
            .map(|c| c.burn)
    }

    #[cfg(test)]
    pub(crate) fn test_fuel(&self, x: usize, y: usize) -> Option<u16> {
        self.index(x, y)
            .and_then(|i| self.cells.get(i))
            .map(|c| c.fuel)
    }

    #[cfg(test)]
    pub(crate) fn test_burned_out(&self, x: usize, y: usize) -> Option<bool> {
        self.index(x, y)
            .and_then(|i| self.cells.get(i))
            .map(|c| c.burned_out)
    }

    #[cfg(test)]
    pub(crate) fn test_burning_list_len(&self) -> usize {
        self.burning.len()
    }
}

fn disc_spans(cx: i32, cy: i32, r0: i32, mut emit: impl FnMut(i32, i32, i32)) {
    let mut a = 0i32;
    let mut x0 = cx;
    let mut x1 = cx;
    let mut err = 2 - 2 * r0;
    let mut r = r0;
    loop {
        if err + r > 0 {
            if r == 0 && r0 == 1 {
                a += 1;
                x0 -= 1;
                x1 += 1;
            }
            emit(cy + r, x0, x1);
            if r == 0 {
                return;
            }
            emit(cy - r, x0, x1);
            r -= 1;
            err += 1 - 2 * r;
        }
        if a > err {
            a += 1;
            x0 -= 1;
            x1 += 1;
            err += 2 * a + 1;
        }
    }
}

#[must_use]
pub(crate) fn burning_damage(
    grid: &FireGrid,
    registrations: &[(Entity, usize)],
    damage_type: Option<mint_sage_combat::DamageTypeId>,
    death: DeathNaming,
) -> Vec<DamageEvent> {
    let mut totals: Vec<(Entity, f32)> = Vec::new();
    for &(entity, cell) in registrations {
        let Some(burn) = grid.cells.get(cell).map(|c| c.burn) else {
            continue;
        };
        if burn == 0 {
            continue;
        }
        let amount = f32::from(burn);
        if let Some(slot) = totals.iter_mut().find(|(e, _)| *e == entity) {
            slot.1 += amount;
        } else {
            totals.push((entity, amount));
        }
    }
    let Some(damage_type) = damage_type else {
        return Vec::new();
    };
    totals
        .into_iter()
        .map(|(entity, amount)| DamageEvent {
            victim: entity,
            attacker: Entity::NONE,
            amount,
            damage_type,
            scalar: 1.0,
            fx_type: None,
            dealt: 0.0,
            death,
            sub_type: None,
            flanking_bonus: 0.0,
            weapon: None,
            launched: None,
            nugget: 0,
            accepts_add: true,
            bonuses_applied: false,
        })
        .collect()
}

pub(crate) fn singleton_of(
    world: &mint_sim::World,
    fire_grid: mint_sim::ComponentId<FireGrid>,
) -> Option<Entity> {
    world
        .store(fire_grid)
        .and_then(|store| store.entities().first().copied())
}

#[allow(clippy::cast_possible_truncation)]
fn nearest_cell(v: f32) -> i64 {
    (v * 0.1 + 0.5).floor() as i64
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn disc_registration(nx: usize, ny: usize, centre: (f32, f32), major: f32) -> Vec<(usize, usize)> {
    let r = major * 0.1 + 0.4;
    let (cxu, cyu) = (centre.0 * 0.1, centre.1 * 0.1);
    let ix0 = ((centre.0 - major) * 0.1 + 0.5).floor() as i64 - 1;
    let iy0 = ((centre.1 - major) * 0.1 + 0.5).floor() as i64 - 1;
    let span = (2.0 * r + 2.0).trunc() as i64;
    let mut cells = Vec::new();
    for iy in iy0..iy0.saturating_add(span) {
        for ix in ix0..ix0.saturating_add(span) {
            let dx = (ix as f32 + 0.5) - cxu;
            let dy = (iy as f32 + 0.5) - cyu;
            if dx * dx + dy * dy > r * r {
                continue;
            }
            if let (Ok(ux), Ok(uy)) = (usize::try_from(ix), usize::try_from(iy)) {
                if ux < nx && uy < ny {
                    cells.push((ux, uy));
                }
            }
        }
    }
    cells
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn box_lattice_registration(
    nx: usize,
    ny: usize,
    centre: (f32, f32),
    major: f32,
    minor: f32,
    angle: f32,
) -> Vec<(usize, usize)> {
    let (sin_a, cos_a) = mint_math::f32::sin_cos(angle);
    let u = (cos_a, sin_a);
    let v = (-sin_a, cos_a);
    let n_a = (0.4 * major).ceil() as i64;
    let n_b = (0.4 * minor).ceil() as i64;
    let mut cells = Vec::new();
    for j in 0..n_b {
        for i in 0..n_a {
            let (fi, fj) = (i as f32, j as f32);
            let px = centre.0 - major * u.0 + minor * v.0 + 5.0 * fi * u.0 - 5.0 * fj * v.0;
            let py = centre.1 - major * u.1 + minor * v.1 + 5.0 * fi * u.1 - 5.0 * fj * v.1;
            let cx = ((px + 0.5) * 0.1).floor() as i64;
            let cy = ((py + 0.5) * 0.1).floor() as i64;
            if let (Ok(ux), Ok(uy)) = (usize::try_from(cx), usize::try_from(cy)) {
                if ux < nx && uy < ny {
                    cells.push((ux, uy));
                }
            }
        }
    }
    cells
}

#[allow(clippy::float_cmp, clippy::too_many_arguments)]
fn registration_cells(
    nx: usize,
    ny: usize,
    kind_of: mint_sage_asset::KindOf,
    is_small: bool,
    radius: f32,
    position: (f32, f32),
    facing_radians: f32,
    pieces: &[mint_sage_modules::GeometryPiece],
) -> Vec<(usize, usize)> {
    if kind_of.contains(mint_sage_asset::KindOf::PROJECTILE) {
        return Vec::new();
    }
    if position.0 == 0.0 && position.1 == 0.0 {
        return Vec::new();
    }
    let immobile = kind_of.contains(mint_sage_asset::KindOf::IMMOBILE);
    if !immobile && (is_small || radius < 30.0) {
        let (cx, cy) = (nearest_cell(position.0), nearest_cell(position.1));
        return match (usize::try_from(cx), usize::try_from(cy)) {
            (Ok(x), Ok(y)) if x < nx && y < ny => vec![(x, y)],
            _ => Vec::new(),
        };
    }
    let mut cells: Vec<(usize, usize)> = Vec::new();
    for piece in pieces {
        if !piece.shown(&[]) || piece.offset_z > 10.0 {
            continue;
        }
        let (sin_a, cos_a) = mint_math::f32::sin_cos(facing_radians);
        let centre = (
            position.0 + cos_a * piece.offset.0 - sin_a * piece.offset.1,
            position.1 + sin_a * piece.offset.0 + cos_a * piece.offset.1,
        );
        cells.extend(match piece.shape {
            mint_sage_modules::PieceShape::Disc { radius: major } => {
                disc_registration(nx, ny, centre, major)
            }
            mint_sage_modules::PieceShape::Box { major, minor } => {
                box_lattice_registration(nx, ny, centre, major, minor, facing_radians)
            }
        });
    }
    cells
}

fn registration_reach(
    kind_of: mint_sage_asset::KindOf,
    is_small: bool,
    radius: f32,
    pieces: &[mint_sage_modules::GeometryPiece],
) -> f32 {
    const CELL_SLACK: f32 = 3.0 * CELL_SIZE;
    let immobile = kind_of.contains(mint_sage_asset::KindOf::IMMOBILE);
    if !immobile && (is_small || radius < 30.0) {
        return CELL_SLACK;
    }
    pieces.iter().fold(CELL_SLACK, |reach, piece| {
        let (major, minor) = match piece.shape {
            mint_sage_modules::PieceShape::Disc { radius } => (radius, 0.0),
            mint_sage_modules::PieceShape::Box { major, minor } => (major, minor),
        };
        let lattice = piece.offset.0.abs() + piece.offset.1.abs() + 3.0 * (major + minor);
        reach.max(lattice + CELL_SLACK)
    })
}

struct Candidate {
    entity: Entity,
    immobile_material: Option<FireMaterial>,
    cells: Vec<(usize, usize)>,
}

impl crate::rules::RotwkRules {
    pub(super) fn step_terrain_fire(
        &self,
        tick: &mut mint_sim::Tick<'_>,
        ids: crate::rules::Ids,
        events: &mut Vec<DamageEvent>,
        living: &[(crate::combat::Foe, mint_sim::PlayerId)],
    ) -> mint_sim::Result<()> {
        let Some(fire_entity) = singleton_of(tick.state(), ids.fire_grid) else {
            return Ok(());
        };
        let Some((nx, ny)) = tick
            .state()
            .get(ids.fire_grid, fire_entity)
            .map(|grid| (grid.nx, grid.ny))
        else {
            return Ok(());
        };
        let settled = tick
            .state()
            .get(ids.fire_grid, fire_entity)
            .is_some_and(FireGrid::is_settled);
        if settled {
            self.overwrite_on_settled_grid(tick, ids, fire_entity, living);
            if let Some(grid) = tick.state_mut().get_mut(ids.fire_grid, fire_entity) {
                grid.step();
            }
            return Ok(());
        }

        let mut candidates: Vec<Candidate> = Vec::new();
        for (foe, _) in living {
            let Some(shape) = tick.state().get(ids.shape, foe.entity).copied() else {
                continue;
            };
            let facing = tick
                .state()
                .get(ids.body, foe.entity)
                .map_or(0.0, |body| body.0.facing.radians());
            let record = tick
                .state()
                .get(ids.kind, foe.entity)
                .copied()
                .and_then(|kind| self.kinds.get(kind));
            let pieces: &[mint_sage_modules::GeometryPiece] = record
                .map(|entry| entry.record.geometry.pieces.as_slice())
                .unwrap_or(&[]);
            let cells = registration_cells(
                nx,
                ny,
                shape.kind_of,
                shape.geometry_is_small,
                foe.radius,
                (foe.position.x, foe.position.y),
                facing,
                pieces,
            );
            if cells.is_empty() {
                continue;
            }
            let immobile_material = shape
                .kind_of
                .contains(mint_sage_asset::KindOf::IMMOBILE)
                .then(|| record.and_then(|entry| entry.record.flammability))
                .flatten();
            candidates.push(Candidate {
                entity: foe.entity,
                immobile_material,
                cells,
            });
        }

        let registrations = tick
            .state_mut()
            .get_mut(ids.fire_grid, fire_entity)
            .map(|grid| grid.claim(&candidates))
            .unwrap_or_default();

        let Some(grid) = tick.state().get(ids.fire_grid, fire_entity) else {
            return Ok(());
        };
        let damage_type = self.combat.damage_types.lookup("LOGICAL_FIRE");
        let death = DeathNaming::of(self.combat.death_types.lookup("BURNED"));
        let hits = burning_damage(grid, &registrations, damage_type, death);
        if !hits.is_empty() {
            crate::diag::diag!(
                "fire: {} cell(s) burning, {} registered entit(y/ies), {} LOGICAL_FIRE hit(s)",
                grid.burning_count(),
                registrations.len(),
                hits.len()
            );
        }
        events.extend(hits);

        let drains = tick
            .state_mut()
            .get_mut(ids.fire_grid, fire_entity)
            .map(FireGrid::step)
            .unwrap_or_default();
        if !drains.is_empty() {
            if let Some(forest_entity) = crate::forest::singleton_of(tick.state(), ids.forest) {
                if let Some(forest) = tick.state_mut().get_mut(ids.forest, forest_entity) {
                    let mut sunk = 0usize;
                    for (index, amount) in drains {
                        let Some((x, y)) = cell_coords(nx, index) else {
                            continue;
                        };
                        let low = mint_movement::Vec2::new(
                            (x as f32) * CELL_SIZE,
                            (y as f32) * CELL_SIZE,
                        );
                        let high = mint_movement::Vec2::new(low.x + CELL_SIZE, low.y + CELL_SIZE);
                        let mut handles = forest.trees_in_rect(low, high);
                        handles.sort_unstable_by(crate::forest::TreeHandle::reverse_order);
                        for handle in handles {
                            if forest.take_supply(handle, u32::from(amount)) > 0 {
                                sunk = sunk.saturating_add(1);
                            }
                        }
                    }
                    if sunk > 0 {
                        crate::diag::diag!(
                            "forest: fire drained {sunk} tree supply box(es) this frame"
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

impl crate::rules::RotwkRules {
    fn overwrite_on_settled_grid(
        &self,
        tick: &mut mint_sim::Tick<'_>,
        ids: crate::rules::Ids,
        fire_entity: Entity,
        living: &[(crate::combat::Foe, mint_sim::PlayerId)],
    ) {
        let world = tick.state();
        let Some(grid) = world.get(ids.fire_grid, fire_entity) else {
            return;
        };
        let (nx, ny) = (grid.nx, grid.ny);
        let mut pending: Vec<usize> = Vec::new();
        let mut done: Vec<(Entity, Placement)> = Vec::new();
        for (foe, _) in living {
            let Some(shape) = world.get(ids.shape, foe.entity) else {
                continue;
            };
            if !shape.kind_of.contains(mint_sage_asset::KindOf::IMMOBILE) {
                continue;
            }
            let Some(kind) = world.get(ids.kind, foe.entity).copied() else {
                continue;
            };
            let Some(entry) = self
                .kinds
                .get(kind)
                .filter(|entry| entry.record.flammability.is_some())
            else {
                continue;
            };
            let facing = world
                .get(ids.body, foe.entity)
                .map_or(0.0, |body| body.0.facing.radians());
            let placement = Placement {
                x: foe.position.x.to_bits(),
                y: foe.position.y.to_bits(),
                facing: facing.to_bits(),
                radius: foe.radius.to_bits(),
                kind_of: shape.kind_of,
                small: shape.geometry_is_small,
                kind,
            };
            if grid.overwritten.get(&foe.entity) == Some(&placement) {
                continue;
            }
            let before = pending.len();
            for (x, y) in registration_cells(
                nx,
                ny,
                shape.kind_of,
                shape.geometry_is_small,
                foe.radius,
                (foe.position.x, foe.position.y),
                facing,
                &entry.record.geometry.pieces,
            ) {
                if grid.overwrite_pending(x, y) {
                    pending.extend(grid.index(x, y));
                }
            }
            if pending.len() == before {
                done.push((foe.entity, placement));
            }
        }
        pending.sort_unstable();
        pending.dedup();
        #[allow(clippy::cast_precision_loss)]
        let world_units = |cell: usize| cell as f32 * CELL_SIZE;
        let span = |pick: fn((usize, usize)) -> usize| {
            let cells = || {
                pending
                    .iter()
                    .filter_map(|index| cell_coords(nx, *index).map(pick))
            };
            (
                cells().min().map_or(f32::MAX, world_units),
                cells().max().map_or(f32::MIN, world_units),
            )
        };
        let (x_span, y_span) = (span(|(x, _)| x), span(|(_, y)| y));

        let mut claimed = vec![false; pending.len()];
        let mut overwrites: Vec<(usize, usize, FireMaterial)> = Vec::new();
        for (foe, _) in living.iter().filter(|_| !pending.is_empty()) {
            let Some(shape) = world.get(ids.shape, foe.entity) else {
                continue;
            };
            let record = world
                .get(ids.kind, foe.entity)
                .and_then(|kind| self.kinds.get(*kind))
                .map(|entry| &entry.record);
            let pieces = record.map_or(&[][..], |record| record.geometry.pieces.as_slice());
            let reach =
                registration_reach(shape.kind_of, shape.geometry_is_small, foe.radius, pieces);
            let (px, py) = (foe.position.x, foe.position.y);
            if px + reach < x_span.0
                || px - reach > x_span.1
                || py + reach < y_span.0
                || py - reach > y_span.1
            {
                continue;
            }
            let material = shape
                .kind_of
                .contains(mint_sage_asset::KindOf::IMMOBILE)
                .then(|| record.and_then(|record| record.flammability))
                .flatten();
            let facing = world
                .get(ids.body, foe.entity)
                .map_or(0.0, |body| body.0.facing.radians());
            for (x, y) in registration_cells(
                nx,
                ny,
                shape.kind_of,
                shape.geometry_is_small,
                foe.radius,
                (px, py),
                facing,
                pieces,
            ) {
                let Some(at) = grid
                    .index(x, y)
                    .and_then(|index| pending.binary_search(&index).ok())
                else {
                    continue;
                };
                if let Some(slot) = claimed.get_mut(at).filter(|slot| !**slot) {
                    *slot = true;
                    if let Some(material) = material {
                        overwrites.push((x, y, material));
                    }
                }
            }
        }
        if let Some(grid) = tick.state_mut().get_mut(ids.fire_grid, fire_entity) {
            grid.overwritten.extend(done);
            for (x, y, material) in overwrites {
                grid.overwrite_once(x, y, material);
            }
        }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn cell_coords(nx: usize, index: usize) -> Option<(usize, usize)> {
    if nx == 0 {
        return None;
    }
    Some((index % nx, index / nx))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    fn distinct_registration_cells(
        nx: usize,
        ny: usize,
        kind_of: mint_sage_asset::KindOf,
        is_small: bool,
        radius: f32,
        position: (f32, f32),
        facing_radians: f32,
        pieces: &[mint_sage_modules::GeometryPiece],
    ) -> Vec<(usize, usize)> {
        let mut cells: Vec<(usize, usize)> = Vec::new();
        for cell in registration_cells(
            nx,
            ny,
            kind_of,
            is_small,
            radius,
            position,
            facing_radians,
            pieces,
        ) {
            if !cells.contains(&cell) {
                cells.push(cell);
            }
        }
        cells
    }

    use mint_sage_asset::KindOf;
    use mint_sage_ini::fire_logic_system::FireMaterial;

    fn material(fuel: u16, max_burn: u16, decay: u16, resistance: u16) -> FireMaterial {
        FireMaterial {
            fuel,
            max_burn_rate: max_burn,
            decay,
            resistance,
        }
    }

    fn system_with(types: [FireMaterial; 4]) -> FireLogicSystem {
        FireLogicSystem {
            max_cells_burning: i64::from(i32::MAX),
            fuel_regeneration_rate: 1,
            cell_types: types,
        }
    }

    #[test]
    fn two_players_in_one_burning_cell_each_take_the_full_burn_rate() {
        let system = system_with([
            FireMaterial::default(),
            material(100, 20, 0, 0),
            FireMaterial::default(),
            FireMaterial::default(),
        ]);
        let mut grid = FireGrid::build(1, 1, |_, _| Some(1), &system);
        grid.apply_burn_delta(0, 5.0, false);
        let a = Entity::from_parts(1, 1);
        let b = Entity::from_parts(2, 1);
        let mut types = mint_sage_combat::DamageTypes::new();
        let logical_fire = types.intern("LOGICAL_FIRE");
        let events = burning_damage(
            &grid,
            &[(a, 0), (b, 0), (a, 0)],
            logical_fire,
            DeathNaming::Normal,
        );
        let a_total: f32 = events
            .iter()
            .filter(|e| e.victim == a)
            .map(|e| e.amount)
            .sum();
        let b_total: f32 = events
            .iter()
            .filter(|e| e.victim == b)
            .map(|e| e.amount)
            .sum();
        assert_eq!(a_total, 10.0, "registered twice -> 5 + 5");
        assert_eq!(b_total, 5.0);
    }

    #[test]
    fn the_cached_grid_hash_tracks_a_spreading_fire() {
        let system = system_with([
            FireMaterial::default(),
            material(100, 20, 0, 4),
            FireMaterial::default(),
            FireMaterial::default(),
        ]);
        let mut grid = FireGrid::build(40, 40, |_, _| Some(1), &system);
        let hashes = |grid: &mut FireGrid| {
            let mut full = StateHasher::new();
            grid.hash_into(&mut full);
            let mut cached = StateHasher::new();
            grid.hash_into_cached(&mut cached);
            (full.finish(), cached.finish())
        };
        let (full, cached) = hashes(&mut grid);
        assert_eq!(full, cached);
        grid.apply_burn_delta(820, 12.0, false);
        let mut burning_frames = 0;
        for frame in 0..200 {
            grid.step();
            let (full, cached) = hashes(&mut grid);
            assert_eq!(full, cached, "frame {frame}");
            if grid.burning_count() > 1 {
                burning_frames += 1;
            }
        }
        assert!(burning_frames > 10, "the fire must have spread");
    }

    #[test]
    fn a_1x3_column_spreads_from_its_lit_middle_cell_over_one_sweep() {
        let system = system_with([
            FireMaterial::default(),
            material(100, 20, 0, 4),
            FireMaterial::default(),
            FireMaterial::default(),
        ]);
        let mut grid = FireGrid::build(1, 3, |_, _| Some(1), &system);
        grid.apply_burn_delta(1, 12.0, false);
        for _ in 0..10 {
            grid.step();
        }
        assert_eq!(grid.test_burn(0, 0), Some(8));
        assert_eq!(grid.test_burn(0, 1), Some(12));
        assert_eq!(grid.test_burn(0, 2), Some(8));
    }

    #[test]
    fn a_1x3_row_spreads_from_its_lit_middle_cell_over_one_sweep() {
        let system = system_with([
            FireMaterial::default(),
            material(100, 20, 0, 4),
            FireMaterial::default(),
            FireMaterial::default(),
        ]);
        let mut grid = FireGrid::build(3, 1, |_, _| Some(1), &system);
        grid.apply_burn_delta(1, 12.0, false);
        for _ in 0..10 {
            grid.step();
        }
        assert_eq!(grid.test_burn(0, 0), Some(8));
        assert_eq!(grid.test_burn(1, 0), Some(20));
        assert_eq!(grid.test_burn(2, 0), Some(16));
    }

    #[test]
    fn a_radius_25_disc_lights_the_decoded_rings() {
        let mut rows: Vec<i32> = Vec::new();
        disc_spans(0, 0, 3, |row, _, _| rows.push(row));
        rows.sort_unstable();
        rows.dedup();
        assert_eq!(rows, vec![-3, -2, -1, 0, 1, 2, 3]);
    }

    #[test]
    fn a_cell_with_less_fuel_than_burn_rate_burns_out() {
        let system = system_with([
            FireMaterial::default(),
            material(3, 20, 2, 0),
            FireMaterial::default(),
            FireMaterial::default(),
        ]);
        let mut grid = FireGrid::build(1, 1, |_, _| Some(1), &system);
        grid.apply_burn_delta(0, 10.0, false);
        for _ in 0..10 {
            grid.step();
        }
        assert_eq!(grid.test_fuel(0, 0), Some(1));
        assert_eq!(grid.test_burned_out(0, 0), Some(true));
        assert_eq!(grid.test_burning_list_len(), 0);
    }

    #[test]
    fn lighting_a_cell_changes_the_grids_hash() {
        let system = system_with([
            FireMaterial::default(),
            material(100, 20, 0, 0),
            FireMaterial::default(),
            FireMaterial::default(),
        ]);
        let mut grid = FireGrid::build(1, 1, |_, _| Some(1), &system);
        let mut before = StateHasher::new();
        grid.hash_into(&mut before);
        grid.apply_burn_delta(0, 5.0, false);
        let mut after = StateHasher::new();
        grid.hash_into(&mut after);
        assert_ne!(before.finish(), after.finish());
    }

    fn one_type(fuel: u16, max_burn: u16, resistance: u16) -> FireLogicSystem {
        system_with([
            FireMaterial::default(),
            material(fuel, max_burn, 0, resistance),
            FireMaterial::default(),
            FireMaterial::default(),
        ])
    }

    #[test]
    fn a_grid_with_nothing_burning_settles_and_then_steps_only_its_phase() {
        let mut grid = FireGrid::build(20, 1, |_, _| Some(1), &one_type(100, 20, 0));
        grid.step();
        assert!(grid.is_settled());
        let cells = format!("{:?}", grid.cells);
        let phase = grid.phase;
        assert!(grid.step().is_empty());
        assert_eq!(grid.phase, (phase + 1) % 10);
        assert_eq!(format!("{:?}", grid.cells), cells);
        assert!(grid.is_settled());
    }

    #[test]
    fn a_cell_lit_on_a_settled_grid_wakes_the_band() {
        let mut grid = FireGrid::build(1, 3, |_, _| Some(1), &one_type(100, 20, 4));
        grid.step();
        assert!(grid.is_settled());
        grid.apply_burn_delta(1, 12.0, false);
        assert!(!grid.is_settled());
        let drained: usize = (0..10).map(|_| grid.step().len()).sum();
        assert!(drained > 0, "the lit cell burns its fuel");
        assert!(
            grid.test_burn(0, 0).is_some_and(|burn| burn > 0),
            "and spreads"
        );
    }

    #[test]
    fn a_burnt_out_empty_cell_keeps_the_grid_awake_until_it_regenerates() {
        let mut grid = FireGrid::build(20, 1, |_, _| Some(1), &one_type(100, 20, 0));
        let index = grid.index(15, 0).unwrap();
        let cell = grid.cells.get_mut(index).unwrap();
        cell.fuel = 0;
        cell.burned_out = true;
        for _ in 0..10 {
            grid.step();
        }
        assert_eq!(grid.test_fuel(15, 0), Some(1));
        assert!(grid.is_settled());
    }

    fn disc_piece(offset: (f32, f32), major: f32) -> mint_sage_modules::GeometryPiece {
        mint_sage_modules::GeometryPiece {
            offset,
            offset_z: 0.0,
            shape: mint_sage_modules::PieceShape::Disc { radius: major },
            name: None,
            active: true,
            height: None,
            for_health_box: true,
        }
    }

    #[test]
    fn a_box_geometry_record_registers_its_whole_lattice() {
        let piece = mint_sage_modules::GeometryPiece {
            offset: (0.0, 0.0),
            offset_z: 0.0,
            shape: mint_sage_modules::PieceShape::Box {
                major: 20.0,
                minor: 10.0,
            },
            name: None,
            active: true,
            height: None,
            for_health_box: true,
        };
        let mut cells = distinct_registration_cells(
            20,
            20,
            KindOf::IMMOBILE,
            false,
            0.0,
            (100.0, 100.0),
            0.0,
            std::slice::from_ref(&piece),
        );
        cells.sort_unstable();
        let mut expected: Vec<(usize, usize)> = (8..=11)
            .flat_map(|x| (9..=11).map(move |y| (x, y)))
            .collect();
        expected.sort_unstable();
        assert_eq!(cells, expected);
    }

    #[test]
    fn a_disc_geometry_record_registers_the_decoded_disc() {
        let piece = disc_piece((0.0, 0.0), 30.0);
        let cells = distinct_registration_cells(
            20,
            20,
            KindOf::IMMOBILE,
            false,
            0.0,
            (55.0, 55.0),
            0.0,
            std::slice::from_ref(&piece),
        );
        assert_eq!(cells.len(), 37, "{cells:?}");
        assert!(cells.contains(&(8, 6)), "the +0.4 margin keeps this cell");
        assert!(
            !cells.iter().any(|&(x, _)| x == 9),
            "r^2 excludes dx = 4 entirely"
        );
    }

    #[test]
    fn a_projectile_registers_no_cell() {
        let cells = distinct_registration_cells(
            20,
            20,
            KindOf::PROJECTILE,
            false,
            5.0,
            (55.0, 55.0),
            0.0,
            &[],
        );
        assert!(cells.is_empty());
    }

    #[test]
    fn two_geometry_records_over_one_cell_register_it_once() {
        let pieces = [disc_piece((0.0, 0.0), 0.0), disc_piece((0.0, 0.0), 0.0)];
        let cells = registration_cells(
            20,
            20,
            KindOf::IMMOBILE,
            false,
            0.0,
            (25.0, 25.0),
            0.0,
            &pieces,
        );
        assert_eq!(cells, vec![(2, 2), (2, 2)], "each record walks the cell");
        let mut grid = FireGrid::build(20, 20, |_, _| Some(1), &one_type(100, 20, 0));
        let (first, second) = (Entity::from_parts(1, 1), Entity::from_parts(2, 1));
        let candidate = |entity, cells| Candidate {
            entity,
            immobile_material: None,
            cells,
        };
        let registrations = grid.claim(&[candidate(first, cells), candidate(second, vec![(2, 2)])]);
        assert_eq!(registrations, vec![(first, grid.index(2, 2).unwrap())]);
    }

    #[test]
    fn a_tree_loses_its_supply_capped_while_the_cell_loses_the_full_amount() {
        let system = system_with([
            FireMaterial::default(),
            material(100, 20, 0, 0),
            FireMaterial::default(),
            FireMaterial::default(),
        ]);
        let mut grid = FireGrid::build(1, 1, |_, _| Some(1), &system);
        grid.apply_burn_delta(0, 8.0, false);
        let mut drains: Vec<(usize, u16)> = Vec::new();
        for _ in 0..10 {
            drains = grid.step();
        }
        assert_eq!(drains, vec![(0, 8)], "the lone cell drains 8 at phase 9");
        assert_eq!(grid.test_fuel(0, 0), Some(92), "fuel loses the full amount");

        let mut forest = crate::forest::Forest::new(
            mint_movement::Vec2::new(0.0, 0.0),
            mint_movement::Vec2::new(10.0, 10.0),
        );
        forest.add_tree(
            mint_movement::Vec2::new(5.0, 5.0),
            crate::unit::Kind(0),
            crate::forest::TreeTraits {
                grabbable: false,
                harvestable: true,
                supply: 5,
                tainted: false,
            },
            crate::forest::TreeOrigin::Placed(0),
        );
        let (index, amount) = drains[0];
        let (x, y) = (index % grid.nx, index / grid.nx);
        let low = mint_movement::Vec2::new((x as f32) * CELL_SIZE, (y as f32) * CELL_SIZE);
        let high = mint_movement::Vec2::new(low.x + CELL_SIZE, low.y + CELL_SIZE);
        let handles = forest.trees_in_rect(low, high);
        assert_eq!(handles.len(), 1);
        let taken = forest.take_supply(handles[0], u32::from(amount));
        assert_eq!(taken, 5, "capped at the tree's own supply");
        assert_eq!(forest.len(), 0, "an emptied tree sinks");
    }

    #[test]
    fn an_increase_fuel_row_skips_a_cell_whose_centre_is_wet() {
        let system = system_with([
            FireMaterial::default(),
            material(100, 20, 0, 0),
            FireMaterial::default(),
            FireMaterial::default(),
        ]);
        let mut grid = FireGrid::build(5, 5, |_, _| Some(1), &system);
        let wet = |x: f32, y: f32| (x, y) == (25.0, 25.0);
        grid.light(&FireLighting::clear_fuel(30.0), (20.0, 20.0), None, &wet);
        assert_eq!(
            grid.test_fuel(2, 2),
            Some(100),
            "the wet cell keeps its fuel"
        );
        assert_eq!(
            grid.test_fuel(1, 1),
            Some(0),
            "a dry cell in the disc is cleared"
        );
    }
}
