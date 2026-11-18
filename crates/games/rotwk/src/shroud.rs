use mint_movement::{Facing, Vec2};
use mint_sage_asset::KindOf;
use mint_sage_modules::ObjectRecord;
use mint_sage_shroud::{
    cells_touched, shroudedness, CellVisibility, GridExtent, LookerSet, ObjectTraits,
    ObjectVisibility, PlayerMask, Sighting, UndoQueue, VisibilityGrid, VisionShape,
    DEFAULT_UNLOOK_PERSIST, PARTITION_CELL_SIZE,
};
use mint_sim::{Component, ComponentId, Entity, PlayerId, Result, StateHasher, Tick, World};

use crate::templates::KindTable;
use crate::unit::{Body, Kind, Owner, Shape};

pub(crate) const SHROUD_PLAYERS: u32 = 8;

pub(crate) const SHROUD_PLAYERS_U8: u8 = 8;

pub(crate) const HORDE_SHROUD_RANGE: f32 = 400.0;

const VISION_REPORT_FRAMES: u64 = 30;

#[derive(Debug, Clone)]
pub(crate) struct Shroud(pub(crate) Option<ShroudState>);

#[derive(Debug, Clone)]
pub(crate) struct ShroudState {
    pub(crate) grid: VisibilityGrid,
    pub(crate) queue: UndoQueue,
    pub(crate) watchers: LookerSet,
    desired_scratch: Vec<(u64, Sighting)>,
    summary_scratch: Vec<(Entity, Seen)>,
}

impl Component for Shroud {
    fn hash_into(&self, hasher: &mut StateHasher) {
        match &self.0 {
            Some(state) => {
                hasher.write_bool(true);
                state.grid.hash_into(hasher);
                state.queue.hash_into(hasher);
                state.watchers.hash_into(hasher);
            }
            None => hasher.write_bool(false),
        }
    }

    fn hash_into_cached(&mut self, hasher: &mut StateHasher) {
        match &mut self.0 {
            Some(state) => {
                hasher.write_bool(true);
                state.grid.hash_into_cached(hasher);
                state.queue.hash_into(hasher);
                state.watchers.hash_into(hasher);
            }
            None => hasher.write_bool(false),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Vision {
    pub(crate) clearing_range: f32,
}

impl Component for Vision {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.clearing_range);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Seen {
    pub(crate) states: [u8; SHROUD_PLAYERS as usize],
    pub(crate) ever: u16,
}

impl Seen {
    pub(crate) const fn hidden() -> Self {
        Self {
            states: [0; SHROUD_PLAYERS as usize],
            ever: 0,
        }
    }

    pub(crate) fn visibility(&self, player: PlayerId) -> ObjectVisibility {
        match self
            .states
            .get(usize::from(player.get()))
            .copied()
            .unwrap_or(0)
        {
            1 => ObjectVisibility::Fogged,
            2 => ObjectVisibility::PartialClear,
            3 => ObjectVisibility::Clear,
            _ => ObjectVisibility::Shrouded,
        }
    }

    pub(crate) fn ever_seen(&self, player: PlayerId) -> bool {
        bit_of(player).is_some_and(|bit| self.ever & bit != 0)
    }

    fn set(&mut self, player: PlayerId, visibility: ObjectVisibility, ever: bool) {
        if let Some(slot) = self.states.get_mut(usize::from(player.get())) {
            *slot = visibility.code();
        }
        if let Some(bit) = bit_of(player) {
            if ever {
                self.ever |= bit;
            } else {
                self.ever &= !bit;
            }
        }
    }
}

fn bit_of(player: PlayerId) -> Option<u16> {
    let index = u32::from(player.get());
    (index < SHROUD_PLAYERS).then(|| 1u16.checked_shl(index).unwrap_or(0))
}

impl Component for Seen {
    fn hash_into(&self, hasher: &mut StateHasher) {
        for state in self.states {
            hasher.write_u8(state);
        }
        hasher.write_u16(self.ever);
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ShroudIds {
    pub(crate) shroud: ComponentId<Shroud>,
    pub(crate) vision: ComponentId<Vision>,
    pub(crate) seen: ComponentId<Seen>,
    pub(crate) body: ComponentId<Body>,
    pub(crate) owner: ComponentId<Owner>,
    pub(crate) shape: ComponentId<Shape>,
    pub(crate) bonuses: ComponentId<crate::experience::Bonuses>,
    pub(crate) kind: ComponentId<Kind>,
}

pub(crate) struct Sight<'a, G> {
    pub(crate) kinds: &'a KindTable,
    pub(crate) ground: G,
}

const FACING_STEPS: f32 = 32.0;

#[derive(Debug, Default)]
struct VisionCensus {
    lookers: u32,
    shaped: u32,
    raised: u32,
    lowered: u32,
    most: f32,
    least: f32,
    peak: Option<(u32, (f32, f32), f32, f32, f32)>,
}

pub(crate) fn grid_for(low: (f32, f32), high: (f32, f32), players: u8) -> Option<VisibilityGrid> {
    let extent = GridExtent::covering(low, high, PARTITION_CELL_SIZE);
    VisibilityGrid::new(extent, u32::from(players.min(SHROUD_PLAYERS_U8))).ok()
}

pub(crate) fn reveal_map_at_start(grid: &mut VisibilityGrid) {
    for slot in 0..grid.players() {
        let Ok(index) = u8::try_from(slot) else {
            continue;
        };
        grid.reveal_all(PlayerId::new(index));
    }
}

pub(crate) fn advance_visibility(
    tick: &mut Tick<'_>,
    ids: ShroudIds,
    neutral_to: &impl Fn(PlayerId, PlayerId) -> bool,
    sight: &Sight<'_, impl Fn(f32, f32) -> Option<f32>>,
) -> Result<()> {
    let Some(singleton) = singleton_of(tick.state(), ids) else {
        return Ok(());
    };
    let Some(mut state) = tick
        .state_mut()
        .get_mut(ids.shroud, singleton)
        .and_then(|shroud| shroud.0.take())
    else {
        return Ok(());
    };
    let now = tick.frame();

    let census = fill_desired_watchers(
        tick.state(),
        ids,
        sight,
        &state.grid,
        &mut state.desired_scratch,
    );
    if now.get() % VISION_REPORT_FRAMES == 0 {
        crate::diag::diag!(
            "vision: frame {}, {} looker(s), {} shaped by facing, {} raised and {} lowered by \
             height (bonus {:+.0}% to {:+.0}%)",
            now.get(),
            census.lookers,
            census.shaped,
            census.raised,
            census.lowered,
            census.least * 100.0,
            census.most * 100.0
        );
        if let Some((entity, (x, y), rise, from, to)) = census.peak {
            crate::diag::diag!(
                "vision: furthest from its ring is entity {entity} at ({x:.0}, {y:.0}), \
                 {rise:+.1} above it, radius {from:.0} -> {to:.0}"
            );
        }
    }
    state.watchers.reconcile(
        &mut state.grid,
        &mut state.queue,
        now,
        DEFAULT_UNLOOK_PERSIST,
        &state.desired_scratch,
    );
    state.queue.drain_due(&mut state.grid, now);

    fill_summaries(
        tick.state(),
        ids,
        &state.grid,
        neutral_to,
        &mut state.summary_scratch,
    );
    for &(entity, seen) in &state.summary_scratch {
        tick.state_mut().insert(ids.seen, entity, seen)?;
    }

    if let Some(shroud) = tick.state_mut().get_mut(ids.shroud, singleton) {
        shroud.0 = Some(state);
    }
    Ok(())
}

fn singleton_of(world: &World, ids: ShroudIds) -> Option<Entity> {
    world
        .store(ids.shroud)
        .and_then(|store| store.entities().first().copied())
}

fn fill_desired_watchers(
    world: &World,
    ids: ShroudIds,
    sight: &Sight<'_, impl Fn(f32, f32) -> Option<f32>>,
    grid: &VisibilityGrid,
    out: &mut Vec<(u64, Sighting)>,
) -> VisionCensus {
    out.clear();
    let mut census = VisionCensus::default();
    let Some(visions) = world.store(ids.vision) else {
        return census;
    };
    out.reserve(visions.len());
    for (entity, vision) in visions.iter() {
        let (Some(body), Some(owner)) = (world.get(ids.body, entity), world.get(ids.owner, entity))
        else {
            continue;
        };
        let base = world
            .get(ids.bonuses, entity)
            .map_or(vision.clearing_range, |b| {
                b.0.shroud_range(vision.clearing_range)
            });
        let record = world
            .get(ids.kind, entity)
            .and_then(|kind| sight.kinds.get(*kind))
            .map(|entry| &entry.record);
        let (radius, shape) = record.map_or((base, VisionShape::ROUND), |record| {
            let at = (entity.index(), body.0.position, body.0.facing);
            shaped_reach(record, base, at, grid, sight, &mut census)
        });
        census.lookers = census.lookers.saturating_add(1);
        let mask = if record.is_some_and(|record| record.has_kind("REVEAL_TO_ALL")) {
            PlayerMask::ALL
        } else {
            PlayerMask::of(owner.0)
        };
        out.push((
            pack(entity),
            Sighting {
                world_x: body.0.position.x,
                world_y: body.0.position.y,
                radius: radius.min(grid.map_diagonal()),
                shape,
                mask,
            },
        ));
    }
    census
}

fn shaped_reach(
    record: &ObjectRecord,
    base: f32,
    (entity, position, facing): (u32, Vec2, Facing),
    grid: &VisibilityGrid,
    sight: &Sight<'_, impl Fn(f32, f32) -> Option<f32>>,
    census: &mut VisionCensus,
) -> (f32, VisionShape) {
    let radius = record.vision_bonus.as_ref().map_or(base, |bonus| {
        let extent = grid.extent();
        let (cell_x, cell_y) = grid.world_to_cell(position.x, position.y);
        #[allow(clippy::cast_precision_loss)]
        let centre = (
            extent.low_x + (cell_x as f32 + 0.5) * extent.cell_size,
            extent.low_y + (cell_y as f32 + 0.5) * extent.cell_size,
        );
        let Some(rise) = crate::combat::ground_rise(bonus, &sight.ground, centre.0, centre.1)
        else {
            return base;
        };
        let bonus = crate::combat::height_bonus(bonus, rise);
        let radius = crate::combat::vision_reach(base, bonus);
        if bonus > 0.0 {
            census.raised = census.raised.saturating_add(1);
        } else if bonus < 0.0 {
            census.lowered = census.lowered.saturating_add(1);
        }
        census.most = census.most.max(bonus);
        census.least = census.least.min(bonus);
        if census
            .peak
            .is_none_or(|(_, _, peak, _, _)| rise.abs() > peak.abs())
        {
            census.peak = Some((entity, centre, rise, base, radius));
        }
        radius
    });
    let (side, rear) = (
        record.vision_side.unwrap_or(1.0),
        record.vision_rear.unwrap_or(1.0),
    );
    if side >= 1.0 && rear >= 1.0 {
        return (radius, VisionShape::ROUND);
    }
    census.shaped = census.shaped.saturating_add(1);
    let step = core::f32::consts::TAU / FACING_STEPS;
    let facing = Facing::from_radians((facing.radians() / step).round() * step).to_vector();
    (radius, VisionShape::new(facing.x, facing.y, side, rear))
}

fn fill_summaries(
    world: &World,
    ids: ShroudIds,
    grid: &VisibilityGrid,
    neutral_to: &impl Fn(PlayerId, PlayerId) -> bool,
    out: &mut Vec<(Entity, Seen)>,
) {
    out.clear();
    let Some(shapes) = world.store(ids.shape) else {
        return;
    };
    out.reserve(shapes.len());
    for (entity, shape) in shapes.iter() {
        let Some(body) = world.get(ids.body, entity) else {
            continue;
        };
        let owner = world.get(ids.owner, entity).map(|owner| owner.0);
        let mut seen = world
            .get(ids.seen, entity)
            .copied()
            .unwrap_or(Seen::hidden());
        let small = shape.radius <= PARTITION_CELL_SIZE * 0.5;
        let cells = cells_touched(
            grid,
            body.0.position.x,
            body.0.position.y,
            shape.radius,
            small,
        );
        for slot in 0..grid.players() {
            let Ok(index) = u8::try_from(slot) else {
                continue;
            };
            let viewer = PlayerId::new(index);
            let traits = ObjectTraits {
                immobile: shape.kind_of.contains(KindOf::IMMOBILE),
                mine: shape.kind_of.contains(KindOf::MINE),
                neutral_to_viewer: owner.is_none_or(|owner| neutral_to(owner, viewer)),
            };
            let result = shroudedness(grid, viewer, &cells, traits, seen.ever_seen(viewer));
            seen.set(viewer, result.visibility, result.ever_seen);
        }
        out.push((entity, seen));
    }
}

fn pack(entity: Entity) -> u64 {
    (u64::from(entity.index()) << 32) | u64::from(entity.generation())
}

pub(crate) fn census(world: &World, ids: ShroudIds) -> Vec<String> {
    let Some(state) = world
        .store(ids.shroud)
        .and_then(|store| store.iter().next())
        .and_then(|(_, shroud)| shroud.0.as_ref())
    else {
        return Vec::new();
    };
    let extent = state.grid.extent();
    let mut lines = Vec::new();
    let (unbalanced, watched) = conservation(world, ids);
    lines.push(format!(
        "shroud: {}x{} cells of {:.0} world units, {} player slot(s), {} watcher(s), \
         {} undo(s) pending, {watched} watched cell(s), {unbalanced} unbalanced",
        extent.cells_x,
        extent.cells_y,
        extent.cell_size,
        state.grid.players(),
        state.watchers.len(),
        pending_undos(world, ids)
    ));
    for slot in 0..state.grid.players() {
        let Ok(index) = u8::try_from(slot) else {
            continue;
        };
        let (unexplored, explored, visible) = cell_counts(world, ids, PlayerId::new(index));
        lines.push(format!(
            "shroud: player {index} sees {visible} cell(s), remembers {explored}, \
             has never seen {unexplored}"
        ));
    }
    lines
}

pub(crate) fn cell_counts(world: &World, ids: ShroudIds, player: PlayerId) -> (u32, u32, u32) {
    let Some(state) = world
        .store(ids.shroud)
        .and_then(|store| store.iter().next())
        .and_then(|(_, shroud)| shroud.0.as_ref())
    else {
        return (0, 0, 0);
    };
    let extent = state.grid.extent();
    let (mut unexplored, mut explored, mut visible) = (0u32, 0u32, 0u32);
    for y in 0..extent.cells_y {
        for x in 0..extent.cells_x {
            let (Ok(cx), Ok(cy)) = (i32::try_from(x), i32::try_from(y)) else {
                continue;
            };
            match state.grid.cell_state(player, cx, cy) {
                CellVisibility::Unexplored => unexplored = unexplored.saturating_add(1),
                CellVisibility::Explored => explored = explored.saturating_add(1),
                CellVisibility::Visible => visible = visible.saturating_add(1),
            }
        }
    }
    (unexplored, explored, visible)
}

pub(crate) fn conservation(world: &World, ids: ShroudIds) -> (usize, usize) {
    world
        .store(ids.shroud)
        .and_then(|store| store.iter().next())
        .and_then(|(_, shroud)| shroud.0.as_ref())
        .map_or((0, 0), |state| {
            (state.grid.unbalanced_cells(), state.grid.watched_cells())
        })
}

pub(crate) fn grid_shape(world: &World, ids: ShroudIds) -> Option<(u32, u32, [f32; 4])> {
    let state = world
        .store(ids.shroud)
        .and_then(|store| store.iter().next())
        .and_then(|(_, shroud)| shroud.0.as_ref())?;
    let extent = state.grid.extent();
    #[allow(clippy::cast_precision_loss)]
    let size_x = extent.cells_x as f32 * extent.cell_size;
    #[allow(clippy::cast_precision_loss)]
    let size_y = extent.cells_y as f32 * extent.cell_size;
    Some((
        extent.cells_x,
        extent.cells_y,
        [extent.low_x, extent.low_y, size_x, size_y],
    ))
}

#[must_use]
pub(crate) fn padded_shroud_extent(extent: [f32; 4], cells_x: u32, cells_y: u32) -> [f32; 4] {
    #[allow(clippy::cast_precision_loss)]
    let (cells_x, cells_y) = (cells_x.max(1) as f32, cells_y.max(1) as f32);
    let cell_w = extent[2] / cells_x;
    let cell_h = extent[3] / cells_y;
    [
        extent[0] - cell_w,
        extent[1] - cell_h,
        extent[2] + 2.0 * cell_w,
        extent[3] + 2.0 * cell_h,
    ]
}

pub(crate) fn rebuild_view(
    world: &World,
    ids: ShroudIds,
    viewer: PlayerId,
    view: &mut mint_sage_scene::ShroudView,
) -> Option<(u32, u32, u32)> {
    let state = world
        .store(ids.shroud)
        .and_then(|store| store.iter().next())
        .and_then(|(_, shroud)| shroud.0.as_ref())?;
    Some(view.rebuild_from(&state.grid, viewer))
}

pub(crate) fn pending_undos(world: &World, ids: ShroudIds) -> usize {
    world
        .store(ids.shroud)
        .and_then(|store| store.iter().next())
        .and_then(|(_, shroud)| shroud.0.as_ref())
        .map_or(0, |state| state.queue.len())
}

pub(crate) fn with_grid<R>(
    world: &World,
    ids: ShroudIds,
    f: impl FnOnce(&VisibilityGrid) -> R,
) -> Option<R> {
    world
        .store(ids.shroud)
        .and_then(|store| store.iter().next())
        .and_then(|(_, shroud)| shroud.0.as_ref())
        .map(|state| f(&state.grid))
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum MapShroudOp {
    RevealAll,
    RevealAllPermanently,
    UndoRevealAllPermanently,
    ShroudAll,
}

pub(crate) fn map_wide(tick: &mut Tick<'_>, ids: ShroudIds, op: MapShroudOp, player: PlayerId) {
    let Some(singleton) = singleton_of(tick.state(), ids) else {
        return;
    };
    let Some(state) = tick
        .state_mut()
        .get_mut(ids.shroud, singleton)
        .and_then(|shroud| shroud.0.as_mut())
    else {
        return;
    };
    match op {
        MapShroudOp::RevealAll => state.grid.reveal_all(player),
        MapShroudOp::RevealAllPermanently => state.grid.reveal_all_permanently(player),
        MapShroudOp::UndoRevealAllPermanently => {
            state.queue.drain_all(&mut state.grid);
            state.grid.undo_reveal_all_permanently(player);
        }
        MapShroudOp::ShroudAll => {
            state.queue.drain_all(&mut state.grid);
            state.grid.shroud_all(player);
        }
    }
}

pub(crate) fn set_window(tick: &mut Tick<'_>, ids: ShroudIds, high: Option<(f32, f32)>) {
    let Some(singleton) = singleton_of(tick.state(), ids) else {
        return;
    };
    if let Some(state) = tick
        .state_mut()
        .get_mut(ids.shroud, singleton)
        .and_then(|shroud| shroud.0.as_mut())
    {
        state.grid.set_window(high);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LookOp {
    Keep,
    Undo,
    Glance,
}

impl LookOp {
    fn reveals(self) -> bool {
        self != Self::Undo
    }
    fn undoes(self) -> bool {
        self != Self::Keep
    }
}

pub(crate) fn look_disc(
    tick: &mut Tick<'_>,
    ids: ShroudIds,
    (world_x, world_y, radius): (f32, f32, f32),
    mask: PlayerMask,
    op: LookOp,
) {
    let Some(singleton) = singleton_of(tick.state(), ids) else {
        return;
    };
    if let Some(state) = tick
        .state_mut()
        .get_mut(ids.shroud, singleton)
        .and_then(|shroud| shroud.0.as_mut())
    {
        if op.reveals() {
            state.grid.reveal(world_x, world_y, radius, mask);
        }
        if op.undoes() {
            state.grid.unreveal(world_x, world_y, radius, mask);
        }
    }
}

pub(crate) fn look_polygon(
    tick: &mut Tick<'_>,
    ids: ShroudIds,
    points: &[(f32, f32)],
    mask: PlayerMask,
    op: LookOp,
) {
    let Some(singleton) = singleton_of(tick.state(), ids) else {
        return;
    };
    if let Some(state) = tick
        .state_mut()
        .get_mut(ids.shroud, singleton)
        .and_then(|shroud| shroud.0.as_mut())
    {
        if op.reveals() {
            state.grid.reveal_polygon_permanently(points, mask);
        }
        if op.undoes() {
            state.grid.unreveal_polygon(points, mask);
        }
    }
}

pub(crate) fn state_over(grid: VisibilityGrid) -> ShroudState {
    ShroudState {
        grid,
        queue: UndoQueue::new(),
        watchers: LookerSet::new(),
        desired_scratch: Vec::new(),
        summary_scratch: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padding_widens_by_exactly_one_cell_on_every_edge() {
        let extent = [100.0, 200.0, 300.0, 400.0];
        let padded = padded_shroud_extent(extent, 30, 40);
        assert!((padded[0] - 90.0).abs() < 1e-3, "low x: {padded:?}");
        assert!((padded[1] - 190.0).abs() < 1e-3, "low y: {padded:?}");
        assert!((padded[2] - 320.0).abs() < 1e-3, "size x: {padded:?}");
        assert!((padded[3] - 420.0).abs() < 1e-3, "size y: {padded:?}");
    }

    #[test]
    fn a_zero_cell_count_does_not_divide_by_zero() {
        let padded = padded_shroud_extent([0.0, 0.0, 300.0, 400.0], 0, 0);
        assert!(padded.iter().all(|value| value.is_finite()), "{padded:?}");
    }
}
