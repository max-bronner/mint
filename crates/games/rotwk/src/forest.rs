use mint_movement::{Facing, Vec2};
use mint_sage_map::Object;
use mint_sage_modules::{ObjectRecord, TemplateRecords};
use mint_sim::{Component, ComponentId, Entity, StateHasher, World};

use crate::unit::Kind;

mod motion;

pub(crate) use motion::{
    ToppleMotion, TreeFx, TreeMotion, TreePose, DEVASTATION_TOPPLE_SPEED, NO_SPEED_OVERRIDE,
};

const TREE_KIND: &str = "TREE";

pub(crate) fn is_tree(record: &ObjectRecord) -> bool {
    record.has_kind(TREE_KIND)
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TreeSeed {
    pub(crate) pos: Vec2,
    pub(crate) template: String,
    pub(crate) grabbable: bool,
    pub(crate) harvestable: bool,
    pub(crate) supply_override: Option<f32>,
    pub(crate) source_index: u32,
}

pub(crate) fn tree_seeds(templates: &TemplateRecords, objects: &[Object]) -> Vec<TreeSeed> {
    objects
        .iter()
        .enumerate()
        .filter_map(|(index, object)| {
            let record = templates.get(&object.template_name)?;
            is_tree(record).then(|| TreeSeed {
                pos: Vec2::new(object.x, object.y),
                template: record.name.clone(),
                grabbable: record.is_grabbable,
                harvestable: record.is_harvestable,
                supply_override: record.supply_override,
                source_index: u32::try_from(index).unwrap_or(u32::MAX),
            })
        })
        .collect()
}

pub(crate) const TREE_RADIUS_APPROX: f32 = 7.0;

pub(crate) const TREE_CLEAR_RADIUS: f32 = 14.0;

const TAKE_ALL_SUPPLY: u32 = 99_999;

const GRID_CELLS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TreeQuery {
    Any,
    Grabbable,
    Harvestable,
}

impl TreeQuery {
    fn admits(self, tree: &Tree) -> bool {
        match self {
            Self::Any => true,
            Self::Harvestable => tree.harvestable,
            Self::Grabbable => tree.grabbable,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum MoverShape {
    Disc,
    Box {
        major: f32,
        minor: f32,
        facing: Facing,
    },
}

fn exact_overlap(mover_pos: Vec2, shape: MoverShape, tree_pos: Vec2) -> bool {
    match shape {
        MoverShape::Disc => true,
        MoverShape::Box {
            major,
            minor,
            facing,
        } => {
            let axis = facing.to_vector();
            let perp = Vec2::new(-axis.y, axis.x);
            let offset = tree_pos.minus(mover_pos);
            let local_x = offset.dot(axis).clamp(-major, major);
            let local_y = offset.dot(perp).clamp(-minor, minor);
            let closest = mover_pos
                .plus(axis.scale(local_x))
                .plus(perp.scale(local_y));
            tree_pos.minus(closest).length_squared() < TREE_RADIUS_APPROX * TREE_RADIUS_APPROX
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TreeTraits {
    pub(crate) grabbable: bool,
    pub(crate) harvestable: bool,
    pub(crate) supply: u32,
    pub(crate) tainted: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TreeTemplates {
    pub(crate) supply_per_tree: f32,
    pub(crate) tainted: Vec<bool>,
    pub(crate) motion: Vec<Option<ToppleMotion>>,
    pub(crate) default_motion: ToppleMotion,
}

impl Default for TreeTemplates {
    fn default() -> Self {
        Self {
            supply_per_tree: crate::game_data::DEFAULT_SUPPLY_BOXES_PER_TREE,
            tainted: Vec::new(),
            motion: Vec::new(),
            default_motion: ToppleMotion::default(),
        }
    }
}

impl TreeTemplates {
    pub(crate) fn motion(&self, kind: Kind) -> &ToppleMotion {
        self.motion
            .get(usize::from(kind.0))
            .and_then(Option::as_ref)
            .unwrap_or(&self.default_motion)
    }

    pub(crate) fn traits(&self, kind: Kind, record: &ObjectRecord) -> Option<TreeTraits> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let supply = record
            .supply_override
            .unwrap_or(self.supply_per_tree)
            .max(0.0) as u32;
        is_tree(record).then(|| TreeTraits {
            grabbable: record.is_grabbable,
            harvestable: record.is_harvestable,
            supply,
            tainted: self
                .tainted
                .get(usize::from(kind.0))
                .copied()
                .unwrap_or(false),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TreeOrigin {
    Placed(u32),
    Planted(u32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PlantedTree {
    pub(crate) serial: u32,
    pub(crate) kind: Kind,
    pub(crate) pos: Vec2,
    pub(crate) pose: TreePose,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Tree {
    pub(crate) pos: Vec2,
    pub(crate) kind: Kind,
    pub(crate) toppled: bool,
    pub(crate) topple_dir: Vec2,
    pub(crate) supply: u32,
    pub(crate) grabbable: bool,
    pub(crate) harvestable: bool,
    pub(crate) tainted: bool,
    pub(crate) origin: TreeOrigin,
    pub(crate) motion: TreeMotion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TreeHandle {
    bucket: usize,
    index: usize,
}

impl TreeHandle {
    pub(crate) fn reverse_order(a: &Self, b: &Self) -> std::cmp::Ordering {
        b.bucket.cmp(&a.bucket).then(b.index.cmp(&a.index))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ForestTreeInit {
    pub(crate) pos: Vec2,
    pub(crate) kind: Kind,
    pub(crate) grabbable: bool,
    pub(crate) harvestable: bool,
    pub(crate) supply: u32,
    pub(crate) tainted: bool,
    pub(crate) source_index: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Forest {
    low: Vec2,
    high: Vec2,
    buckets: Vec<Vec<Tree>>,
    planted: u32,
    sinking: Vec<Tree>,
    animating: bool,
    revision: u32,
}

fn hash_tree(hasher: &mut StateHasher, tree: &Tree) {
    hasher.write_u32(tree.pos.x.to_bits());
    hasher.write_u32(tree.pos.y.to_bits());
    hasher.write_u16(tree.kind.0);
    hasher.write_bool(tree.toppled);
    hasher.write_u32(tree.topple_dir.x.to_bits());
    hasher.write_u32(tree.topple_dir.y.to_bits());
    hasher.write_u32(tree.supply);
    hasher.write_bool(tree.grabbable);
    hasher.write_bool(tree.harvestable);
    hasher.write_bool(tree.tainted);
    let (tag, index) = match tree.origin {
        TreeOrigin::Placed(index) => (0, index),
        TreeOrigin::Planted(serial) => (1, serial),
    };
    hasher.write_u8(tag);
    hasher.write_u32(index);
    tree.motion.hash_into(hasher);
}

impl Component for Forest {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.buckets.len());
        for bucket in &self.buckets {
            hasher.write_usize(bucket.len());
            for tree in bucket {
                hash_tree(hasher, tree);
            }
        }
        hasher.write_u32(self.planted);
        hasher.write_usize(self.sinking.len());
        for tree in &self.sinking {
            hash_tree(hasher, tree);
        }
        hasher.write_bool(self.animating);
    }
}

impl Forest {
    pub(crate) fn new(low: Vec2, high: Vec2) -> Self {
        Self {
            low,
            high,
            buckets: vec![Vec::new(); GRID_CELLS * GRID_CELLS],
            planted: 0,
            sinking: Vec::new(),
            animating: false,
            revision: 0,
        }
    }

    pub(crate) fn revision(&self) -> u32 {
        self.revision
    }

    fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    fn start_sink(&mut self, mut tree: Tree) {
        tree.motion.sink();
        self.sinking.push(tree);
        self.animating = true;
        self.changed();
    }

    pub(crate) fn advance_motion(&mut self, templates: &TreeTemplates) -> Vec<(Kind, TreeFx)> {
        let mut played = Vec::new();
        if !self.animating {
            return played;
        }
        let mut still_moving = false;
        let mut fx = Vec::new();
        for tree in self
            .buckets
            .iter_mut()
            .flatten()
            .chain(self.sinking.iter_mut())
        {
            if !tree.motion.active() {
                continue;
            }
            tree.motion
                .step(templates.motion(tree.kind), tree.pos, &mut fx);
            played.extend(fx.drain(..).map(|fx| (tree.kind, fx)));
            still_moving |= tree.motion.active();
        }
        self.sinking.retain(|tree| !tree.motion.gone());
        self.animating = still_moving;
        self.changed();
        played
    }

    pub(crate) fn placed_poses(&self) -> Vec<(u32, TreePose)> {
        self.buckets
            .iter()
            .flatten()
            .chain(&self.sinking)
            .filter(|tree| tree.motion.moved())
            .filter_map(|tree| match tree.origin {
                TreeOrigin::Placed(index) => Some((index, tree.motion.pose())),
                TreeOrigin::Planted(_) => None,
            })
            .collect()
    }

    pub(crate) fn len(&self) -> usize {
        self.buckets.iter().map(Vec::len).sum()
    }

    fn cell_size(&self) -> Vec2 {
        Vec2::new(
            (self.high.x - self.low.x).max(1.0) / GRID_CELLS as f32,
            (self.high.y - self.low.y).max(1.0) / GRID_CELLS as f32,
        )
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn cell_of(&self, pos: Vec2) -> (usize, usize) {
        let size = self.cell_size();
        let clamp = |value: f32| {
            if !value.is_finite() {
                0
            } else {
                (value.max(0.0) as usize).min(GRID_CELLS - 1)
            }
        };
        (
            clamp((pos.x - self.low.x) / size.x),
            clamp((pos.y - self.low.y) / size.y),
        )
    }

    pub(crate) fn add_tree(
        &mut self,
        pos: Vec2,
        kind: Kind,
        traits: TreeTraits,
        origin: TreeOrigin,
    ) {
        let (cx, cy) = self.cell_of(pos);
        let bucket = cy * GRID_CELLS + cx;
        if let Some(trees) = self.buckets.get_mut(bucket) {
            trees.insert(
                0,
                Tree {
                    pos,
                    kind,
                    toppled: false,
                    topple_dir: Vec2::ZERO,
                    supply: traits.supply,
                    grabbable: traits.grabbable,
                    harvestable: traits.harvestable,
                    tainted: traits.tainted,
                    origin,
                    motion: TreeMotion::default(),
                },
            );
        }
        self.changed();
    }

    pub(crate) fn plant(&mut self, pos: Vec2, kind: Kind, traits: TreeTraits) {
        let serial = self.planted;
        self.planted = self.planted.saturating_add(1);
        self.add_tree(pos, kind, traits, TreeOrigin::Planted(serial));
    }

    pub(crate) fn planted_trees(&self) -> Vec<PlantedTree> {
        let mut planted: Vec<PlantedTree> = self
            .buckets
            .iter()
            .flatten()
            .chain(&self.sinking)
            .filter(|tree| !tree.motion.gone())
            .filter_map(|tree| match tree.origin {
                TreeOrigin::Planted(serial) => Some(PlantedTree {
                    serial,
                    kind: tree.kind,
                    pos: tree.pos,
                    pose: tree.motion.pose(),
                }),
                TreeOrigin::Placed(_) => None,
            })
            .collect();
        planted.sort_unstable_by_key(|tree| tree.serial);
        planted
    }

    pub(crate) fn present_source_indices(&self) -> Vec<u32> {
        self.buckets
            .iter()
            .flatten()
            .chain(&self.sinking)
            .filter(|tree| !tree.motion.gone())
            .filter_map(|tree| match tree.origin {
                TreeOrigin::Placed(index) => Some(index),
                TreeOrigin::Planted(_) => None,
            })
            .collect()
    }

    fn cell_range(&self, pos: Vec2, radius: f32) -> (usize, usize, usize, usize) {
        let (lo_cx, lo_cy) = self.cell_of(Vec2::new(pos.x - radius, pos.y - radius));
        let (hi_cx, hi_cy) = self.cell_of(Vec2::new(pos.x + radius, pos.y + radius));
        (lo_cx, lo_cy, hi_cx, hi_cy)
    }

    pub(crate) fn find_closest(
        &self,
        pos: Vec2,
        radius: f32,
        skip_toppled: bool,
        mode: TreeQuery,
    ) -> Option<TreeHandle> {
        let radius = radius + TREE_RADIUS_APPROX;
        let radius_sq = radius * radius;
        let (lo_cx, lo_cy, hi_cx, hi_cy) = self.cell_range(pos, radius);
        let mut best: Option<(TreeHandle, f32)> = None;
        for cy in lo_cy..=hi_cy {
            for cx in lo_cx..=hi_cx {
                let bucket = cy * GRID_CELLS + cx;
                let Some(trees) = self.buckets.get(bucket) else {
                    continue;
                };
                for (index, tree) in trees.iter().enumerate() {
                    if skip_toppled && tree.toppled {
                        continue;
                    }
                    if !mode.admits(tree) {
                        continue;
                    }
                    let dist_sq = tree.pos.minus(pos).length_squared();
                    if dist_sq >= radius_sq {
                        continue;
                    }
                    if best.is_none_or(|(_, best_dist)| dist_sq < best_dist) {
                        best = Some((TreeHandle { bucket, index }, dist_sq));
                    }
                }
            }
        }
        best.map(|(handle, _)| handle)
    }

    fn standing_within(&self, pos: Vec2, radius: f32) -> Vec<TreeHandle> {
        let radius = radius + TREE_RADIUS_APPROX;
        let radius_sq = radius * radius;
        let (lo_cx, lo_cy, hi_cx, hi_cy) = self.cell_range(pos, radius);
        let mut found = Vec::new();
        for cy in lo_cy..=hi_cy {
            for cx in lo_cx..=hi_cx {
                let bucket = cy * GRID_CELLS + cx;
                let Some(trees) = self.buckets.get(bucket) else {
                    continue;
                };
                for (index, tree) in trees.iter().enumerate() {
                    if tree.toppled {
                        continue;
                    }
                    if tree.pos.minus(pos).length_squared() < radius_sq {
                        found.push(TreeHandle { bucket, index });
                    }
                }
            }
        }
        found
    }

    pub(crate) fn topple_near(
        &mut self,
        mover_pos: Vec2,
        mover_radius: f32,
        shape: MoverShape,
    ) -> Vec<Vec2> {
        let mut felled = Vec::new();
        for handle in self.standing_within(mover_pos, mover_radius) {
            let Some(trees) = self.buckets.get_mut(handle.bucket) else {
                continue;
            };
            let Some(tree) = trees.get_mut(handle.index) else {
                continue;
            };
            if tree.toppled {
                continue;
            }
            if !exact_overlap(mover_pos, shape, tree.pos) {
                continue;
            }
            tree.toppled = true;
            tree.topple_dir = tree.pos.minus(mover_pos);
            tree.motion.topple(tree.topple_dir, NO_SPEED_OVERRIDE);
            felled.push(tree.pos);
        }
        if !felled.is_empty() {
            self.animating = true;
            self.changed();
        }
        felled
    }

    fn all_within_exact(&self, pos: Vec2, radius: f32) -> Vec<TreeHandle> {
        let radius_sq = radius * radius;
        let (lo_cx, lo_cy, hi_cx, hi_cy) = self.cell_range(pos, radius);
        let mut found = Vec::new();
        for cy in lo_cy..=hi_cy {
            for cx in lo_cx..=hi_cx {
                let bucket = cy * GRID_CELLS + cx;
                let Some(trees) = self.buckets.get(bucket) else {
                    continue;
                };
                for (index, tree) in trees.iter().enumerate() {
                    if tree.pos.minus(pos).length_squared() < radius_sq {
                        found.push(TreeHandle { bucket, index });
                    }
                }
            }
        }
        found
    }

    pub(crate) fn clear_within(&mut self, pos: Vec2, radius: f32) -> usize {
        let mut handles = self.all_within_exact(pos, radius + TREE_CLEAR_RADIUS);
        handles.sort_unstable_by(|a, b| b.bucket.cmp(&a.bucket).then(b.index.cmp(&a.index)));
        let mut removed = 0usize;
        for handle in handles {
            let Some(trees) = self.buckets.get_mut(handle.bucket) else {
                continue;
            };
            if handle.index < trees.len() {
                trees.remove(handle.index);
                removed = removed.saturating_add(1);
            }
        }
        if removed > 0 {
            self.changed();
        }
        removed
    }

    pub(crate) fn take_tree(&mut self, handle: TreeHandle) -> Option<Tree> {
        let trees = self.buckets.get_mut(handle.bucket)?;
        let taken = (handle.index < trees.len()).then(|| trees.remove(handle.index));
        if taken.is_some() {
            self.changed();
        }
        taken
    }

    pub(crate) fn take_supply(&mut self, handle: TreeHandle, want: u32) -> u32 {
        let Some(trees) = self.buckets.get_mut(handle.bucket) else {
            return 0;
        };
        let Some(tree) = trees.get_mut(handle.index) else {
            return 0;
        };
        let taken = want.min(tree.supply);
        tree.supply = tree.supply.saturating_sub(taken);
        if tree.supply == 0 {
            let emptied = trees.remove(handle.index);
            self.start_sink(emptied);
        }
        taken
    }

    pub(crate) fn trees_in_rect(&self, low: Vec2, high: Vec2) -> Vec<TreeHandle> {
        let (lo_cx, lo_cy) = self.cell_of(low);
        let (hi_cx, hi_cy) = self.cell_of(Vec2::new(high.x - f32::EPSILON, high.y - f32::EPSILON));
        let mut found = Vec::new();
        for cy in lo_cy..=hi_cy {
            for cx in lo_cx..=hi_cx {
                let bucket = cy * GRID_CELLS + cx;
                let Some(trees) = self.buckets.get(bucket) else {
                    continue;
                };
                for (index, tree) in trees.iter().enumerate() {
                    if tree.pos.x >= low.x
                        && tree.pos.x < high.x
                        && tree.pos.y >= low.y
                        && tree.pos.y < high.y
                    {
                        found.push(TreeHandle { bucket, index });
                    }
                }
            }
        }
        found
    }

    pub(crate) fn topple_at(
        &mut self,
        handle: TreeHandle,
        away_from: Vec2,
        speed_override: f32,
    ) -> Option<Vec2> {
        let tree = self.buckets.get_mut(handle.bucket)?.get_mut(handle.index)?;
        if tree.toppled {
            return None;
        }
        tree.toppled = true;
        tree.topple_dir = tree.pos.minus(away_from);
        tree.motion.topple(tree.topple_dir, speed_override);
        let pos = tree.pos;
        self.animating = true;
        self.changed();
        Some(pos)
    }

    pub(crate) fn harvestable_within(&self, pos: Vec2, radius: f32) -> Vec<(TreeHandle, Vec2)> {
        let mut found: Vec<(TreeHandle, Vec2, f32)> = self
            .all_within_exact(pos, radius + TREE_RADIUS_APPROX)
            .into_iter()
            .filter_map(|handle| {
                let tree = self.buckets.get(handle.bucket)?.get(handle.index)?;
                tree.harvestable
                    .then(|| (handle, tree.pos, tree.pos.minus(pos).length_squared()))
            })
            .collect();
        found.sort_by(|a, b| a.2.total_cmp(&b.2));
        found
            .into_iter()
            .map(|(handle, pos, _)| (handle, pos))
            .collect()
    }

    pub(crate) fn tree(&self, handle: TreeHandle) -> Option<&Tree> {
        self.buckets.get(handle.bucket)?.get(handle.index)
    }

    pub(crate) fn fell_for_supply(&mut self, at: Vec2, radius: f32) -> Vec<(Vec2, u32)> {
        let mut felled = Vec::new();
        while let Some(handle) = self.find_closest(at, radius, false, TreeQuery::Harvestable) {
            let Some(pos) = self.tree(handle).map(|tree| tree.pos) else {
                break;
            };
            self.topple_at(handle, at, DEVASTATION_TOPPLE_SPEED);
            felled.push((pos, self.take_supply(handle, TAKE_ALL_SUPPLY)));
        }
        felled
    }

    pub(crate) fn morph_within(&mut self, at: Vec2, radius: f32, tainted: bool) -> usize {
        let mut handles: Vec<TreeHandle> = self
            .all_within_exact(at, radius + TREE_RADIUS_APPROX)
            .into_iter()
            .filter(|handle| {
                self.tree(*handle)
                    .is_some_and(|tree| tree.toppled && tree.tainted == tainted)
            })
            .collect();
        handles.sort_unstable_by(|a, b| b.bucket.cmp(&a.bucket).then(b.index.cmp(&a.index)));
        let mut removed = 0usize;
        for handle in handles {
            let Some(trees) = self.buckets.get_mut(handle.bucket) else {
                continue;
            };
            if handle.index < trees.len() {
                let morphed = trees.remove(handle.index);
                self.start_sink(morphed);
                removed = removed.saturating_add(1);
            }
        }
        removed
    }
}

pub(crate) fn singleton_of(world: &World, forest: ComponentId<Forest>) -> Option<Entity> {
    world
        .store(forest)
        .and_then(|store| store.entities().first().copied())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn kind(n: u16) -> Kind {
        Kind(n)
    }

    fn forest() -> Forest {
        Forest::new(Vec2::new(0.0, 0.0), Vec2::new(1000.0, 1000.0))
    }

    fn traits(grabbable: bool, harvestable: bool, supply: u32) -> TreeTraits {
        TreeTraits {
            grabbable,
            harvestable,
            supply,
            tainted: false,
        }
    }

    fn add(
        f: &mut Forest,
        pos: Vec2,
        kind: Kind,
        grabbable: bool,
        harvestable: bool,
        supply: u32,
        index: u32,
    ) {
        f.add_tree(
            pos,
            kind,
            traits(grabbable, harvestable, supply),
            TreeOrigin::Placed(index),
        );
    }

    #[test]
    fn find_closest_admits_the_approx_radius_past_the_query_radius() {
        let mut f = forest();
        add(&mut f, Vec2::new(105.0, 100.0), kind(1), true, true, 90, 0);
        let found = f.find_closest(Vec2::new(100.0, 100.0), 0.0, false, TreeQuery::Any);
        assert!(
            found.is_some(),
            "5.0 is within 0.0 + TREE_RADIUS_APPROX (7.0)"
        );
    }

    #[test]
    fn ties_keep_the_first_tree_found() {
        let mut f = forest();
        add(&mut f, Vec2::new(100.0, 100.0), kind(1), true, true, 90, 0);
        add(&mut f, Vec2::new(100.0, 100.0), kind(2), true, true, 90, 1);
        let found = f
            .find_closest(Vec2::new(100.0, 100.0), 5.0, false, TreeQuery::Any)
            .unwrap();
        assert_eq!(f.tree(found).unwrap().kind, kind(2));
    }

    #[test]
    fn skip_toppled_excludes_a_fallen_tree() {
        let mut f = forest();
        add(&mut f, Vec2::new(100.0, 100.0), kind(1), true, true, 90, 0);
        f.topple_near(Vec2::new(100.0, 100.0), 0.0, MoverShape::Disc);
        assert!(f
            .find_closest(Vec2::new(100.0, 100.0), 5.0, true, TreeQuery::Any)
            .is_none());
        assert!(f
            .find_closest(Vec2::new(100.0, 100.0), 5.0, false, TreeQuery::Any)
            .is_some());
    }

    #[test]
    fn topple_near_fells_once_and_away_from_the_mover() {
        let mut f = forest();
        add(&mut f, Vec2::new(110.0, 100.0), kind(1), true, true, 90, 5);
        let felled = f.topple_near(Vec2::new(100.0, 100.0), 10.0, MoverShape::Disc);
        assert_eq!(felled, vec![Vec2::new(110.0, 100.0)]);
        let handle = f
            .find_closest(Vec2::new(110.0, 100.0), 0.0, false, TreeQuery::Any)
            .unwrap();
        let tree = f.tree(handle).unwrap();
        assert!(tree.toppled);
        assert_eq!(tree.topple_dir, Vec2::new(10.0, 0.0));
        assert!(f
            .topple_near(Vec2::new(100.0, 100.0), 10.0, MoverShape::Disc)
            .is_empty());
    }

    #[test]
    fn topple_near_box_mover_needs_the_exact_overlap_not_just_the_coarse_circle() {
        let mut f = forest();
        add(&mut f, Vec2::new(126.0, 100.0), kind(1), true, true, 90, 0);
        let shape = MoverShape::Box {
            major: 2.0,
            minor: 20.0,
            facing: Facing::EAST,
        };
        let felled = f.topple_near(Vec2::new(100.0, 100.0), 20.0, shape);
        assert!(felled.is_empty());
    }

    #[test]
    fn an_emptied_tree_sinks_out_of_the_queries_and_then_out_of_sight() {
        let mut f = forest();
        add(&mut f, Vec2::new(100.0, 100.0), kind(1), true, true, 90, 7);
        add(&mut f, Vec2::new(300.0, 300.0), kind(2), true, true, 90, 9);
        assert!(f.placed_poses().is_empty());
        let felled = f.fell_for_supply(Vec2::new(90.0, 100.0), 20.0);
        assert_eq!(felled, vec![(Vec2::new(100.0, 100.0), 90)]);
        assert!(f
            .find_closest(Vec2::new(100.0, 100.0), 5.0, false, TreeQuery::Any)
            .is_none());
        let poses = f.placed_poses();
        assert_eq!(poses.len(), 1);
        assert_eq!(poses.first().map(|(index, _)| *index), Some(7));
        assert_eq!(f.present_source_indices().len(), 2);
        let templates = TreeTemplates::default();
        let mut ticks = 0;
        while f.present_source_indices().len() == 2 && ticks < 1000 {
            f.advance_motion(&templates);
            ticks += 1;
        }
        assert_eq!(f.present_source_indices(), vec![9]);
        assert_eq!(ticks, 301, "the default SinkTime, 10 s of ticks, then gone");
    }

    #[test]
    fn mode_filters_grabbable_independently_of_harvestable() {
        let mut f = forest();
        add(&mut f, Vec2::new(100.0, 100.0), kind(1), false, true, 90, 0);
        assert!(f
            .find_closest(Vec2::new(100.0, 100.0), 5.0, false, TreeQuery::Grabbable)
            .is_none());
        assert!(f
            .find_closest(Vec2::new(100.0, 100.0), 5.0, false, TreeQuery::Any)
            .is_some());
    }

    #[test]
    fn take_tree_removes_the_record_outright() {
        let mut f = forest();
        add(&mut f, Vec2::new(100.0, 100.0), kind(3), true, true, 90, 0);
        let handle = f
            .find_closest(Vec2::new(100.0, 100.0), 5.0, false, TreeQuery::Grabbable)
            .unwrap();
        let taken = f.take_tree(handle).unwrap();
        assert_eq!(taken.kind, kind(3));
        assert_eq!(f.len(), 0);
        assert!(f
            .find_closest(Vec2::new(100.0, 100.0), 5.0, false, TreeQuery::Any)
            .is_none());
    }

    #[test]
    fn clear_within_removes_standing_and_toppled_trees_with_no_yield() {
        let mut f = forest();
        add(&mut f, Vec2::new(100.0, 100.0), kind(1), true, true, 90, 0);
        add(&mut f, Vec2::new(110.0, 100.0), kind(2), true, true, 90, 1);
        f.topple_near(Vec2::new(110.0, 100.0), 0.0, MoverShape::Disc);
        assert_eq!(f.clear_within(Vec2::new(100.0, 100.0), 0.0), 2);
        assert_eq!(f.len(), 0);
    }

    #[test]
    fn clear_within_admits_the_clear_radius_past_the_footprint() {
        let mut f = forest();
        add(&mut f, Vec2::new(112.0, 100.0), kind(1), true, true, 90, 0);
        assert_eq!(f.clear_within(Vec2::new(100.0, 100.0), 0.0), 1);
    }

    #[test]
    fn take_supply_sinks_an_empty_tree() {
        let mut f = forest();
        add(&mut f, Vec2::new(100.0, 100.0), kind(1), true, true, 5, 0);
        let handle = f
            .find_closest(Vec2::new(100.0, 100.0), 5.0, false, TreeQuery::Any)
            .unwrap();
        assert_eq!(f.take_supply(handle, 3), 3);
        assert_eq!(f.len(), 1);
        let handle = f
            .find_closest(Vec2::new(100.0, 100.0), 5.0, false, TreeQuery::Any)
            .unwrap();
        assert_eq!(f.take_supply(handle, 10), 2);
        assert_eq!(f.len(), 0);
    }

    #[test]
    fn a_morph_sinks_only_toppled_trees_of_its_taint() {
        let mut f = forest();
        let tainted = TreeTraits {
            tainted: true,
            ..traits(true, true, 90)
        };
        add(&mut f, Vec2::new(100.0, 100.0), kind(1), true, true, 90, 0);
        add(&mut f, Vec2::new(130.0, 100.0), kind(2), true, true, 90, 1);
        f.add_tree(
            Vec2::new(70.0, 100.0),
            kind(3),
            tainted,
            TreeOrigin::Placed(2),
        );
        f.topple_near(Vec2::new(100.0, 100.0), 0.0, MoverShape::Disc);
        f.topple_near(Vec2::new(70.0, 100.0), 0.0, MoverShape::Disc);
        assert_eq!(f.morph_within(Vec2::new(100.0, 100.0), 50.0, false), 1);
        assert_eq!(f.len(), 2);
    }

    #[test]
    fn harvestable_within_orders_nearest_first_and_skips_non_harvestable() {
        let mut f = forest();
        add(&mut f, Vec2::new(130.0, 100.0), kind(1), false, true, 90, 0);
        add(&mut f, Vec2::new(110.0, 100.0), kind(2), false, true, 90, 1);
        add(
            &mut f,
            Vec2::new(105.0, 100.0),
            kind(3),
            false,
            false,
            90,
            2,
        );
        let found = f.harvestable_within(Vec2::new(100.0, 100.0), 40.0);
        let positions: Vec<Vec2> = found.into_iter().map(|(_, pos)| pos).collect();
        assert_eq!(
            positions,
            vec![Vec2::new(110.0, 100.0), Vec2::new(130.0, 100.0)]
        );
    }
}
