use mint_render::{MeshData, Vec3, Vertex};
use mint_sage_map::{HeightMapData, Object, MAP_XY_FACTOR};

use std::collections::BTreeMap;

use crate::surface::{drawn_surface_height, max_cell_height, surface_normal};

pub const ROAD_START_FLAG: i32 = 0x2;

pub const ROAD_END_FLAG: i32 = 0x4;

pub const ROAD_ANGLED_FLAG: i32 = 0x8;

pub const ROAD_TIGHT_FLAG: i32 = 0x40;

pub const ROAD_END_CAP_FLAG: i32 = 0x80;

pub const STRAIGHT_BAND_V: f32 = 85.0 / 512.0;

pub const JUNCTION_CELL_U: f32 = 425.0 / 512.0;

pub const TEE_CELL_V: f32 = 255.0 / 512.0;

pub const CROSSROADS_CELL_V: f32 = 425.0 / 512.0;

pub const JUNCTION_WIDTH_ADJUSTMENT: f32 = 1.03;

pub const JUNCTION_SLANT_COSINE: f32 = 0.5;

pub const JUNCTION_STRAIGHT_COSINE: f32 = 0.866;

pub const END_CAP_CELL_U: f32 = 106.0 / 512.0;

pub const END_CAP_CELL_V: f32 = 425.0 / 512.0;

pub const END_CAP_LENGTH: f32 = 48.0 / 128.0;

pub const END_CAP_WIDTH: f32 = 1.0 + 8.0 / 128.0;

pub const END_CAP_OVERHANG: f32 = 0.65;

pub const TEXTURE_REPEAT_WIDTHS: f32 = 4.0;

pub const ROAD_LIFT_XY_128: f32 = 0.078_125;

pub const ROAD_LIFT_HEIGHT_STEP_EIGHTH: f32 = 0.004_882_812_5;

pub const DEGENERATE_NUDGE: f32 = 0.25;

pub const MAX_MITRE: f32 = 4.0;

const MAX_STATIONS: usize = 2048;

const MAX_COLUMNS: usize = 64;

const MAX_QUAD_VERTICES: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoadStyle {
    pub width: f32,
    pub width_in_texture: f32,
}

impl RoadStyle {
    #[must_use]
    pub fn half_width(&self) -> Option<f32> {
        let scale = self.width_in_texture.clamp(0.0, 1.0);
        if self.width.is_finite() && self.width > 0.0 && scale > 0.0 {
            Some(self.width * scale / 2.0)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoadHeight {
    Surface,
    StationMax,
    #[default]
    CellMax,
}

impl RoadHeight {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Surface => "surface (each vertex on the drawn ground under it)",
            Self::StationMax => "station max (drawn ground, highest across the ribbon)",
            Self::CellMax => "cell max (highest corner of each cell, highest across the ribbon)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoadTessellation {
    Ribbon,
    Grid(StepsPerCell),
}

impl Default for RoadTessellation {
    fn default() -> Self {
        Self::Ribbon
    }
}

impl Default for RoadOptions {
    fn default() -> Self {
        Self {
            lift: ROAD_LIFT_XY_128,
            height: RoadHeight::default(),
            tessellation: RoadTessellation::default(),
            chain_order: RoadChainOrder::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StepsPerCell {
    One,
    Two,
    Four,
}

impl StepsPerCell {
    #[must_use]
    pub fn steps(self) -> f32 {
        match self {
            Self::One => 1.0,
            Self::Two => 2.0,
            Self::Four => 4.0,
        }
    }
}

impl RoadTessellation {
    fn steps(self) -> (f32, f32) {
        match self {
            Self::Ribbon => (1.0, 0.0),
            Self::Grid(per_cell) => (per_cell.steps(), per_cell.steps()),
        }
    }

    fn rounds_up(self) -> bool {
        matches!(self, Self::Grid(_))
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Ribbon => "ribbon (2 vertices per station, the release's own)",
            Self::Grid(StepsPerCell::One) => "grid, 1 step per cell",
            Self::Grid(StepsPerCell::Two) => "grid, 2 steps per cell",
            Self::Grid(StepsPerCell::Four) => "grid, 4 steps per cell",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoadChainOrder {
    AsFile,
    #[default]
    Chained,
    Reversed,
}

impl RoadChainOrder {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::AsFile => "as filed (each segment its own way round - the artefact)",
            Self::Chained => "chained (a chain runs the way its first segment is filed)",
            Self::Reversed => "chained, reversed (every chain turned round)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoadOptions {
    pub lift: f32,
    pub height: RoadHeight,
    pub tessellation: RoadTessellation,
    pub chain_order: RoadChainOrder,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoadSegment {
    pub first: usize,
    pub second: usize,
    pub start: [f32; 2],
    pub end: [f32; 2],
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RoadSegments {
    pub segments: Vec<RoadSegment>,
    pub unpaired_starts: usize,
    pub orphan_ends: usize,
    pub both_bits: usize,
}

#[must_use]
pub fn road_segments(objects: &[Object]) -> RoadSegments {
    let mut out = RoadSegments::default();
    for (index, object) in objects.iter().enumerate() {
        let start = object.flags & ROAD_START_FLAG != 0;
        let end = object.flags & ROAD_END_FLAG != 0;
        if start && end {
            out.both_bits = out.both_bits.saturating_add(1);
        }
        if end && !preceded_by_start(objects, index) {
            out.orphan_ends = out.orphan_ends.saturating_add(1);
        }
        if !start {
            continue;
        }
        let next = index.saturating_add(1);
        match objects
            .get(next)
            .filter(|other| other.flags & ROAD_END_FLAG != 0)
        {
            Some(other) => out.segments.push(RoadSegment {
                first: index,
                second: next,
                start: [object.x, object.y],
                end: [other.x, other.y],
            }),
            None => out.unpaired_starts = out.unpaired_starts.saturating_add(1),
        }
    }
    out
}

fn preceded_by_start(objects: &[Object], index: usize) -> bool {
    index
        .checked_sub(1)
        .and_then(|before| objects.get(before))
        .is_some_and(|object| object.flags & ROAD_START_FLAG != 0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoadSegmentBuild {
    pub first: usize,
    pub second: usize,
    pub reversed: bool,
    pub chain_seed: usize,
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub dir: [f32; 2],
    pub origin: [f32; 2],
    pub corners: [[f32; 2]; 4],
    pub half_width: f32,
    pub vertices: (usize, usize),
}

#[derive(Debug, Clone)]
pub struct RoadMesh {
    pub name: String,
    pub style: RoadStyle,
    pub mesh: MeshData,
    pub segments: usize,
    pub segment_vertices: usize,
    pub builds: Vec<RoadSegmentBuild>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RoadSkip {
    UndeclaredType,
    UnusableWidth,
    NonFinite,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoadStats {
    pub placements: usize,
    pub segments: usize,
    pub drawn: usize,
    pub skipped: BTreeMap<RoadSkip, usize>,
    pub degenerate: usize,
    pub off_map: usize,
    pub name_mismatch: usize,
    pub endpoint_classes: [usize; 5],
    pub endpoint_ends: usize,
    pub opposed_joins: usize,
    pub reoriented: usize,
    pub duplicate_spans: usize,
    pub orientation_conflicts: usize,
    pub end_caps: usize,
    pub end_caps_drawn: usize,
    pub chains: usize,
    pub chains_open: usize,
    pub chains_one_capped_end: usize,
    pub mitred: usize,
    pub mitre_capped: usize,
    pub tees: usize,
    pub crossroads: usize,
    pub junction_y: usize,
    pub junction_h: usize,
    pub junction_degenerate: usize,
    pub vertices: usize,
    pub triangles: usize,
}

impl RoadStats {
    #[must_use]
    pub fn skipped_total(&self) -> usize {
        self.skipped
            .values()
            .fold(0usize, |sum, count| sum.saturating_add(*count))
    }
}

#[derive(Debug, Clone, Default)]
pub struct RoadNetwork {
    pub meshes: Vec<RoadMesh>,
    pub stats: RoadStats,
    pub undeclared: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    Start,
    Finish,
}

impl End {
    fn opposite(self) -> Self {
        match self {
            Self::Start => Self::Finish,
            Self::Finish => Self::Start,
        }
    }

    fn after_turn(self, turned: bool) -> Self {
        if turned {
            self.opposite()
        } else {
            self
        }
    }
}

struct Prepared {
    type_id: usize,
    start: [f32; 2],
    end: [f32; 2],
    dir: [f32; 2],
    normal: [f32; 2],
    centres: [Option<[f32; 2]>; 2],
    corners: [Option<[f32; 2]>; 4],
    caps: [bool; 2],
    placements: [usize; 2],
    reversed: bool,
    chain_seed: usize,
}

impl Prepared {
    fn turn_round(&mut self) {
        std::mem::swap(&mut self.start, &mut self.end);
        self.caps.swap(0, 1);
        self.dir = [-self.dir[0], -self.dir[1]];
        self.normal = [-self.normal[0], -self.normal[1]];
    }

    fn point(&self, end: End) -> [f32; 2] {
        match end {
            End::Start => self.start,
            End::Finish => self.end,
        }
    }
}

fn end_key(entry: &Prepared, end: End) -> (usize, u32, u32) {
    let point = entry.point(end);
    (entry.type_id, bits(point[0]), bits(point[1]))
}

fn bucket_ends(prepared: &[Prepared]) -> BTreeMap<(usize, u32, u32), Vec<(usize, End)>> {
    let mut buckets: BTreeMap<(usize, u32, u32), Vec<(usize, End)>> = BTreeMap::new();
    for (index, entry) in prepared.iter().enumerate() {
        for end in [End::Start, End::Finish] {
            buckets
                .entry(end_key(entry, end))
                .or_default()
                .push((index, end));
        }
    }
    buckets
}

fn same_span(prepared: &[Prepared], a: usize, b: usize) -> bool {
    let (Some(first), Some(second)) = (prepared.get(a), prepared.get(b)) else {
        return false;
    };
    let key = |entry: &Prepared, end: End| end_key(entry, end);
    let ends = [
        key(first, End::Start),
        key(first, End::Finish),
        key(second, End::Start),
        key(second, End::Finish),
    ];
    (ends[0] == ends[2] && ends[1] == ends[3]) || (ends[0] == ends[3] && ends[1] == ends[2])
}

fn orient_chains(prepared: &mut [Prepared], order: RoadChainOrder, stats: &mut RoadStats) {
    let buckets = bucket_ends(prepared);
    let mut spans: BTreeMap<((usize, u32, u32), (usize, u32, u32)), usize> = BTreeMap::new();
    for entry in prepared.iter() {
        let (a, b) = (end_key(entry, End::Start), end_key(entry, End::Finish));
        let slot = spans
            .entry(if a <= b { (a, b) } else { (b, a) })
            .or_default();
        *slot = slot.saturating_add(1);
    }
    for held in spans.into_values() {
        stats.duplicate_spans = stats.duplicate_spans.saturating_add(held.saturating_sub(1));
    }
    for ends in buckets.values() {
        if let [(first, mine), (second, theirs)] = ends[..] {
            if first != second && mine == theirs && !same_span(prepared, first, second) {
                stats.opposed_joins = stats.opposed_joins.saturating_add(1);
            }
        }
    }

    let reversed = matches!(order, RoadChainOrder::Reversed);
    let mut turn = vec![false; prepared.len()];
    let mut settled = vec![false; prepared.len()];
    let mut stack: Vec<usize> = Vec::new();
    let mut chain: Vec<usize> = Vec::new();
    for seed in 0..prepared.len() {
        if settled.get(seed).copied().unwrap_or(true) {
            continue;
        }
        if let Some(slot) = settled.get_mut(seed) {
            *slot = true;
        }
        if let Some(slot) = turn.get_mut(seed) {
            *slot = reversed;
        }
        stack.push(seed);
        chain.clear();
        chain.push(seed);
        while let Some(index) = stack.pop() {
            let Some(entry) = prepared.get(index) else {
                continue;
            };
            let turned = turn.get(index).copied().unwrap_or(false);
            for mine in [End::Start, End::Finish] {
                let Some(ends) = buckets.get(&end_key(entry, mine)) else {
                    continue;
                };
                if ends.len() != 2 {
                    continue;
                }
                let Some(&(other, theirs)) = ends.iter().find(|(at, _)| *at != index) else {
                    continue;
                };
                let along = same_span(prepared, index, other);
                let here = mine.after_turn(turned);
                let wanted = if along { here } else { here.opposite() };
                let needed = theirs != wanted;
                match settled.get(other).copied() {
                    Some(true) => {
                        if turn.get(other).copied() != Some(needed) {
                            stats.orientation_conflicts =
                                stats.orientation_conflicts.saturating_add(1);
                        }
                    }
                    Some(false) => {
                        if let Some(slot) = settled.get_mut(other) {
                            *slot = true;
                        }
                        if let Some(slot) = turn.get_mut(other) {
                            *slot = needed;
                        }
                        chain.push(other);
                        stack.push(other);
                    }
                    None => {}
                }
            }
        }
        stats.chains = stats.chains.saturating_add(1);
        for &index in &chain {
            if let Some(entry) = prepared.get_mut(index) {
                entry.chain_seed = seed;
            }
        }
        let (mut free, mut capped) = (0usize, 0usize);
        for &index in &chain {
            let Some(entry) = prepared.get(index) else {
                continue;
            };
            for (slot, end) in [End::Start, End::Finish].into_iter().enumerate() {
                if buckets
                    .get(&end_key(entry, end))
                    .is_some_and(|at| at.len() == 1)
                {
                    free = free.saturating_add(1);
                    if entry.caps.get(slot).copied().unwrap_or(false) {
                        capped = capped.saturating_add(1);
                    }
                }
            }
        }
        if free == 2 {
            stats.chains_open = stats.chains_open.saturating_add(1);
            if capped == 1 {
                stats.chains_one_capped_end = stats.chains_one_capped_end.saturating_add(1);
            }
        }
    }

    if matches!(order, RoadChainOrder::AsFile) {
        return;
    }
    for (index, turned) in turn.into_iter().enumerate() {
        if !turned {
            continue;
        }
        if let Some(entry) = prepared.get_mut(index) {
            entry.turn_round();
            entry.reversed = true;
            stats.reoriented = stats.reoriented.saturating_add(1);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resolved {
    Type(usize),
    Unusable,
    Undeclared,
}

struct RoadType {
    name: String,
    style: RoadStyle,
    half_width: f32,
    mesh: MeshData,
    builds: Vec<RoadSegmentBuild>,
    segments: usize,
    segment_vertices: usize,
}

#[must_use]
pub fn build_roads<F>(
    height_map: &HeightMapData,
    vertical_scale: f32,
    objects: &[Object],
    options: &RoadOptions,
    mut style_of: F,
) -> RoadNetwork
where
    F: FnMut(&str) -> Option<RoadStyle>,
{
    let paired = road_segments(objects);
    let mut network = RoadNetwork {
        stats: RoadStats {
            placements: objects
                .iter()
                .filter(|object| object.flags & (ROAD_START_FLAG | ROAD_END_FLAG) != 0)
                .count(),
            segments: paired.segments.len(),
            ..RoadStats::default()
        },
        ..RoadNetwork::default()
    };

    let mut by_name: BTreeMap<String, Resolved> = BTreeMap::new();
    let mut types: Vec<RoadType> = Vec::new();
    let mut prepared: Vec<Prepared> = Vec::with_capacity(paired.segments.len());

    for segment in &paired.segments {
        let Some(first) = objects.get(segment.first) else {
            continue;
        };
        if objects.get(segment.second).is_some_and(|second| {
            !second
                .template_name
                .eq_ignore_ascii_case(&first.template_name)
        }) {
            network.stats.name_mismatch = network.stats.name_mismatch.saturating_add(1);
        }
        let key = first.template_name.to_ascii_lowercase();
        let resolved = match by_name.get(&key) {
            Some(resolved) => *resolved,
            None => {
                let resolved = match style_of(&key) {
                    None => {
                        network.undeclared.push(key.clone());
                        Resolved::Undeclared
                    }
                    Some(style) => match style.half_width() {
                        None => Resolved::Unusable,
                        Some(half_width) => {
                            types.push(RoadType {
                                name: first.template_name.clone(),
                                style,
                                half_width,
                                mesh: MeshData::default(),
                                builds: Vec::new(),
                                segments: 0,
                                segment_vertices: 0,
                            });
                            match types.len().checked_sub(1) {
                                Some(slot) => Resolved::Type(slot),
                                None => Resolved::Unusable,
                            }
                        }
                    },
                };
                by_name.insert(key.clone(), resolved);
                resolved
            }
        };
        let type_id = match resolved {
            Resolved::Type(slot) => slot,
            Resolved::Unusable => {
                count_skip(&mut network.stats, RoadSkip::UnusableWidth);
                continue;
            }
            Resolved::Undeclared => {
                count_skip(&mut network.stats, RoadSkip::UndeclaredType);
                continue;
            }
        };

        let cap = |slot: usize| {
            objects
                .get(slot)
                .is_some_and(|object| object.flags & ROAD_END_CAP_FLAG != 0)
        };
        let caps = [cap(segment.first), cap(segment.second)];
        let Some(entry) = prepare(segment, type_id, caps, &mut network.stats) else {
            count_skip(&mut network.stats, RoadSkip::NonFinite);
            continue;
        };
        prepared.push(entry);
    }

    orient_chains(&mut prepared, options.chain_order, &mut network.stats);
    let pieces = join_ends(&mut prepared, &types, &mut network.stats);

    for entry in &prepared {
        let Some(road) = types.get(entry.type_id) else {
            continue;
        };
        let quad = segment_quad(entry, road);
        let half_width = road.half_width;
        let built = build_quad(height_map, vertical_scale, options, &quad);
        if built.off_map {
            network.stats.off_map = network.stats.off_map.saturating_add(1);
        }
        if let Some(road) = types.get_mut(entry.type_id) {
            let from = road.mesh.vertices.len();
            append(&mut road.mesh, built.mesh);
            road.builds.push(RoadSegmentBuild {
                first: entry.placements[0],
                second: entry.placements[1],
                reversed: entry.reversed,
                chain_seed: entry.chain_seed,
                start: quad.tail(),
                end: quad.head(),
                dir: quad.u_axis,
                origin: quad.origin,
                corners: quad.corners,
                half_width,
                vertices: (from, road.mesh.vertices.len()),
            });
            road.segments = road.segments.saturating_add(1);
        }
        network.stats.drawn = network.stats.drawn.saturating_add(1);
    }

    for road in &mut types {
        road.segment_vertices = road.mesh.vertices.len();
    }

    for (type_id, quad) in &pieces {
        let built = build_quad(height_map, vertical_scale, options, quad);
        if let Some(road) = types.get_mut(*type_id) {
            append(&mut road.mesh, built.mesh);
        }
    }

    for road in types {
        if road.mesh.indices.is_empty() {
            continue;
        }
        network.stats.vertices = network
            .stats
            .vertices
            .saturating_add(road.mesh.vertices.len());
        network.stats.triangles = network
            .stats
            .triangles
            .saturating_add(road.mesh.indices.len() / 3);
        network.meshes.push(RoadMesh {
            name: road.name,
            style: road.style,
            segment_vertices: road.segment_vertices.min(road.mesh.vertices.len()),
            mesh: road.mesh,
            builds: road.builds,
            segments: road.segments,
        });
    }
    network
}

fn count_skip(stats: &mut RoadStats, reason: RoadSkip) {
    let slot = stats.skipped.entry(reason).or_default();
    *slot = slot.saturating_add(1);
}

fn prepare(
    segment: &RoadSegment,
    type_id: usize,
    caps: [bool; 2],
    stats: &mut RoadStats,
) -> Option<Prepared> {
    let [sx, sy] = segment.start;
    let [mut ex, ey] = segment.end;
    if ![sx, sy, ex, ey].into_iter().all(f32::is_finite) {
        return None;
    }
    let mut end = [ex, ey];
    if sx == ex && sy == ey {
        stats.degenerate = stats.degenerate.saturating_add(1);
        ex = sx + DEGENERATE_NUDGE;
        end = [ex, ey];
    }
    let (dx, dy) = (end[0] - sx, end[1] - sy);
    let length = (dx * dx + dy * dy).sqrt();
    if !(length.is_finite() && length > 0.0) {
        return None;
    }
    let dir = [dx / length, dy / length];
    Some(Prepared {
        type_id,
        start: segment.start,
        end,
        dir,
        normal: [-dir[1], dir[0]],
        centres: [None; 2],
        corners: [None; 4],
        caps,
        placements: [segment.first, segment.second],
        reversed: false,
        chain_seed: usize::MAX,
    })
}

fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn scale2(a: [f32; 2], k: f32) -> [f32; 2] {
    [a[0] * k, a[1] * k]
}

fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[1] - a[1] * b[0]
}

fn turn_sign(a: [f32; 2], b: [f32; 2]) -> i32 {
    let z = cross(a, b);
    if z < 0.0 {
        -1
    } else if z > 0.0 {
        1
    } else {
        0
    }
}

fn left90(a: [f32; 2]) -> [f32; 2] {
    [-a[1], a[0]]
}

fn right90(a: [f32; 2]) -> [f32; 2] {
    [a[1], -a[0]]
}

fn unit(a: [f32; 2]) -> Option<[f32; 2]> {
    let length = (a[0] * a[0] + a[1] * a[1]).sqrt();
    (length.is_finite() && length > 0.0).then(|| [a[0] / length, a[1] / length])
}

fn lerp2(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

fn midpoint(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}

fn join_ends(
    prepared: &mut [Prepared],
    types: &[RoadType],
    stats: &mut RoadStats,
) -> Vec<(usize, Quad)> {
    let buckets = bucket_ends(prepared);
    let mut pieces: Vec<(usize, Quad)> = Vec::new();
    for ((type_id, xb, yb), ends) in buckets {
        stats.endpoint_ends = stats.endpoint_ends.saturating_add(ends.len());
        let class = ends.len().clamp(1, 5).saturating_sub(1);
        if let Some(slot) = stats.endpoint_classes.get_mut(class) {
            *slot = slot.saturating_add(1);
        }
        let point = [f32::from_bits(xb), f32::from_bits(yb)];
        let Some(road) = types.get(type_id) else {
            continue;
        };
        let piece = match ends[..] {
            [a] => end_cap(prepared, a, point, road, stats),
            [a, b] => {
                mitre(prepared, a, b, point, road.half_width, stats);
                None
            }
            [a, b, c] => tee(prepared, [a, b, c], point, road, stats),
            [a, b, c, d] => crossroads(prepared, [a, b, c, d], point, road, stats),
            _ => None,
        };
        if let Some(quad) = piece {
            pieces.push((type_id, quad));
        }
    }
    pieces
}

fn outgoing(prepared: &[Prepared], (index, end): (usize, End)) -> Option<[f32; 2]> {
    let entry = prepared.get(index)?;
    Some(match end {
        End::Start => entry.dir,
        End::Finish => [-entry.dir[0], -entry.dir[1]],
    })
}

fn end_cap(
    prepared: &[Prepared],
    end: (usize, End),
    point: [f32; 2],
    road: &RoadType,
    stats: &mut RoadStats,
) -> Option<Quad> {
    let capped = prepared
        .get(end.0)
        .and_then(|entry| entry.caps.get(usize::from(end.1 == End::Finish)).copied())
        .unwrap_or(false);
    if !capped {
        return None;
    }
    stats.end_caps = stats.end_caps.saturating_add(1);
    let along_unit = outgoing(prepared, end)?;
    let width = road.style.width;
    if !(width.is_finite() && width > 0.0) {
        return None;
    }
    let along = scale2(along_unit, width * END_CAP_LENGTH);
    let across = scale2(left90(along_unit), width * END_CAP_WIDTH * 0.5);
    let tail = sub(point, scale2(along, END_CAP_OVERHANG));
    let head = add(tail, along);
    stats.end_caps_drawn = stats.end_caps_drawn.saturating_add(1);
    Some(Quad {
        corners: [
            add(tail, across),
            sub(tail, across),
            add(head, across),
            sub(head, across),
        ],
        origin: point,
        u_axis: along_unit,
        v_axis: left90(along_unit),
        u_offset: END_CAP_CELL_U,
        v_offset: END_CAP_CELL_V,
        scale: width,
    })
}

fn mitre(
    prepared: &mut [Prepared],
    a: (usize, End),
    b: (usize, End),
    point: [f32; 2],
    half_width: f32,
    stats: &mut RoadStats,
) {
    let Some(pair) = mitre_points(prepared, a, b, point, half_width) else {
        stats.mitre_capped = stats.mitre_capped.saturating_add(1);
        return;
    };
    stats.mitred = stats.mitred.saturating_add(1);
    for end in [a, b] {
        set_corners(prepared, end, pair);
    }
}

fn set_corners(prepared: &mut [Prepared], (index, end): (usize, End), pair: [[f32; 2]; 2]) {
    let Some(entry) = prepared.get_mut(index) else {
        return;
    };
    let [p, q] = pair;
    let side = |point: [f32; 2]| {
        (point[0] - entry.start[0]) * entry.normal[0]
            + (point[1] - entry.start[1]) * entry.normal[1]
    };
    let (left, right) = if side(p) >= side(q) { (p, q) } else { (q, p) };
    let base = match end {
        End::Start => 0,
        End::Finish => 2,
    };
    if let Some(slot) = entry.corners.get_mut(base) {
        *slot = Some(left);
    }
    if let Some(slot) = entry.corners.get_mut(base.saturating_add(1)) {
        *slot = Some(right);
    }
}

fn set_centre(prepared: &mut [Prepared], (index, end): (usize, End), centre: [f32; 2]) {
    let Some(entry) = prepared.get_mut(index) else {
        return;
    };
    let slot = match end {
        End::Start => 0,
        End::Finish => 1,
    };
    if let Some(slot) = entry.centres.get_mut(slot) {
        *slot = Some(centre);
    }
}

fn mitre_points(
    prepared: &[Prepared],
    a: (usize, End),
    b: (usize, End),
    point: [f32; 2],
    half_width: f32,
) -> Option<[[f32; 2]; 2]> {
    let u1 = outgoing(prepared, a)?;
    let u2 = outgoing(prepared, b)?;
    let n_a = right90(u1);
    let n_b = left90(u2);
    let m = unit(add(n_a, n_b))?;
    let cosine = dot(m, n_a);
    if !(cosine.is_finite() && cosine > 1.0 / MAX_MITRE) {
        return None;
    }
    let reach = half_width / cosine;
    Some([add(point, scale2(m, reach)), sub(point, scale2(m, reach))])
}

fn tee(
    prepared: &mut [Prepared],
    ends: [(usize, End); 3],
    point: [f32; 2],
    road: &RoadType,
    stats: &mut RoadStats,
) -> Option<Quad> {
    let [e0, e1, e2] = ends;
    let (d0, d1, d2) = (
        outgoing(prepared, e0)?,
        outgoing(prepared, e1)?,
        outgoing(prepared, e2)?,
    );
    if is_y([d0, d1, d2]) {
        stats.junction_y = stats.junction_y.saturating_add(1);
        return None;
    }
    let (d01, d02, d21) = (dot(d0, d1), dot(d0, d2), dot(d2, d1));
    let ((first, a), (second, b), (stem, c)) = if d01 < d02 {
        if d01 < d21 {
            ((e0, d0), (e1, d1), (e2, d2))
        } else {
            ((e2, d2), (e1, d1), (e0, d0))
        }
    } else if d02 < d21 {
        ((e0, d0), (e2, d2), (e1, d1))
    } else {
        ((e2, d2), (e1, d1), (e0, d0))
    };

    let through = unit(sub(b, a))?;
    if dot(through, c).abs() > JUNCTION_SLANT_COSINE {
        stats.junction_h = stats.junction_h.saturating_add(1);
        return None;
    }
    let up = scale2(through, road.style.width / 2.0);
    let turn = if cross(up, c) >= 0.0 {
        left90(up)
    } else {
        right90(up)
    };
    let across = scale2(left90(through), road.half_width);
    let along = scale2(through, road.half_width);

    for (end, centre, offset) in [
        (first, sub(point, up), across),
        (second, add(point, up), across),
        (stem, add(point, turn), along),
    ] {
        set_centre(prepared, end, centre);
        set_corners(prepared, end, [add(centre, offset), sub(centre, offset)]);
    }

    stats.tees = stats.tees.saturating_add(1);
    let axis = unit(turn)?;
    Some(junction_quad(
        point,
        axis,
        road.half_width,
        JUNCTION_WIDTH_ADJUSTMENT * road.style.width / 2.0,
        road.style.width,
        TEE_CELL_V,
    ))
}

fn crossroads(
    prepared: &mut [Prepared],
    ends: [(usize, End); 4],
    point: [f32; 2],
    road: &RoadType,
    stats: &mut RoadStats,
) -> Option<Quad> {
    let [e0, e1, e2, e3] = ends;
    let (d0, d1, d2, d3) = (
        outgoing(prepared, e0)?,
        outgoing(prepared, e1)?,
        outgoing(prepared, e2)?,
        outgoing(prepared, e3)?,
    );
    let mut chosen = 0u8;
    let mut best = f32::INFINITY;
    for (value, slot) in [
        (dot(d0, d1), 0u8),
        (dot(d0, d2), 1),
        (dot(d0, d3), 2),
        (dot(d1, d2), 3),
        (dot(d1, d3), 4),
        (dot(d2, d3), 5),
    ] {
        if value < best {
            best = value;
            chosen = slot;
        }
    }
    let ((first, a), (second, b), (third, c), (fourth, _)) = match chosen {
        0 => ((e0, d0), (e1, d1), (e2, d2), (e3, d3)),
        1 => ((e0, d0), (e2, d2), (e1, d1), (e3, d3)),
        2 => ((e0, d0), (e3, d3), (e2, d2), (e1, d1)),
        3 => ((e1, d1), (e2, d2), (e0, d0), (e3, d3)),
        4 => ((e1, d1), (e3, d3), (e0, d0), (e2, d2)),
        _ => ((e2, d2), (e3, d3), (e0, d0), (e1, d1)),
    };

    let align = unit(sub(b, a))?;
    let axis = scale2(align, road.style.width / 2.0);
    let turn = if cross(axis, c) >= 0.0 {
        left90(axis)
    } else {
        right90(axis)
    };
    let across = scale2(left90(align), road.half_width);
    let along = scale2(align, road.half_width);

    for (end, centre, offset) in [
        (first, sub(point, axis), across),
        (second, add(point, axis), across),
        (third, add(point, turn), along),
        (fourth, sub(point, turn), along),
    ] {
        set_centre(prepared, end, centre);
        set_corners(prepared, end, [add(centre, offset), sub(centre, offset)]);
    }

    stats.crossroads = stats.crossroads.saturating_add(1);
    let piece_axis = if align[0] < 0.0 {
        [-align[0], -align[1]]
    } else {
        align
    };
    let reach = JUNCTION_WIDTH_ADJUSTMENT * road.style.width / 2.0;
    Some(junction_quad(
        point,
        piece_axis,
        reach,
        reach,
        road.style.width,
        CROSSROADS_CELL_V,
    ))
}

fn junction_quad(
    point: [f32; 2],
    axis: [f32; 2],
    back: f32,
    reach: f32,
    scale: f32,
    v_offset: f32,
) -> Quad {
    let normal = left90(axis);
    let tail = sub(point, scale2(axis, back));
    let head = add(point, scale2(axis, reach));
    let side = scale2(normal, reach);
    Quad {
        corners: [
            add(tail, side),
            sub(tail, side),
            add(head, side),
            sub(head, side),
        ],
        origin: point,
        u_axis: axis,
        v_axis: normal,
        u_offset: JUNCTION_CELL_U,
        v_offset,
        scale,
    }
}

fn is_y(dirs: [[f32; 2]; 3]) -> bool {
    let [a, b, c] = dirs;
    if dot(a, b) < -JUNCTION_STRAIGHT_COSINE
        || dot(a, c) < -JUNCTION_STRAIGHT_COSINE
        || dot(c, b) < -JUNCTION_STRAIGHT_COSINE
    {
        return false;
    }
    y_legs(a, b, c) || y_legs(c, a, b) || y_legs(b, a, c)
}

fn y_legs(base: [f32; 2], x: [f32; 2], y: [f32; 2]) -> bool {
    let (sx, sy) = (turn_sign(base, x), turn_sign(base, y));
    let ahead = left90(base);
    sx != sy && sx.saturating_add(sy) == 0 && turn_sign(ahead, x) == 1 && turn_sign(ahead, y) == 1
}

fn bits(value: f32) -> u32 {
    if value == 0.0 {
        0.0f32.to_bits()
    } else {
        value.to_bits()
    }
}

struct Quad {
    corners: [[f32; 2]; 4],
    origin: [f32; 2],
    u_axis: [f32; 2],
    v_axis: [f32; 2],
    u_offset: f32,
    v_offset: f32,
    scale: f32,
}

impl Quad {
    fn tail(&self) -> [f32; 2] {
        let [left, right, _, _] = self.corners;
        midpoint(left, right)
    }

    fn head(&self) -> [f32; 2] {
        let [_, _, left, right] = self.corners;
        midpoint(left, right)
    }
}

fn segment_quad(entry: &Prepared, road: &RoadType) -> Quad {
    let moved = |slot: usize, fallback: [f32; 2]| {
        entry
            .centres
            .get(slot)
            .copied()
            .flatten()
            .unwrap_or(fallback)
    };
    let (mut start, mut end) = (moved(0, entry.start), moved(1, entry.end));
    if dot(sub(end, start), entry.dir) <= 0.0 {
        start = entry.start;
        end = entry.end;
    }
    let (origin, axis) = (start, unit(sub(end, start)).unwrap_or(entry.dir));
    let normal = left90(axis);
    let offset = scale2(normal, road.half_width);
    let mut corners = [
        add(start, offset),
        sub(start, offset),
        add(end, offset),
        sub(end, offset),
    ];
    for (slot, corner) in corners.iter_mut().enumerate() {
        if let Some(Some(moved)) = entry.corners.get(slot) {
            *corner = *moved;
        }
    }
    Quad {
        corners,
        origin,
        u_axis: axis,
        v_axis: normal,
        u_offset: 0.0,
        v_offset: STRAIGHT_BAND_V,
        scale: road.style.width,
    }
}

struct BuiltSegment {
    mesh: MeshData,
    off_map: bool,
}

fn build_quad(
    height_map: &HeightMapData,
    vertical_scale: f32,
    options: &RoadOptions,
    quad: &Quad,
) -> BuiltSegment {
    let [start_left, start_right, end_left, end_right] = quad.corners;
    let (tail, head) = (
        midpoint(start_left, start_right),
        midpoint(end_left, end_right),
    );
    let span = sub(head, tail);
    let (along, across) = options.tessellation.steps();
    let up = options.tessellation.rounds_up();
    let stations = station_count((span[0] * span[0] + span[1] * span[1]).sqrt(), along, up);
    let columns = column_count(
        width_of(start_left, start_right).max(width_of(end_left, end_right)),
        across,
        up,
    );
    let (stations, columns) = fit_budget(stations, columns);
    let lift = if options.lift.is_finite() {
        options.lift
    } else {
        0.0
    };
    let repeat = TEXTURE_REPEAT_WIDTHS * quad.scale;
    let cells = stations.saturating_mul(columns);
    let mut mesh = MeshData {
        vertices: Vec::with_capacity(cells),
        indices: Vec::with_capacity(cells.saturating_mul(6)),
    };
    let mut off_map = false;

    let sample = |point: [f32; 2]| match options.height {
        RoadHeight::CellMax => max_cell_height(height_map, vertical_scale, point[0], point[1]),
        RoadHeight::Surface | RoadHeight::StationMax => {
            drawn_surface_height(height_map, vertical_scale, point[0], point[1])
        }
    };
    let normal = |point: [f32; 2]| {
        surface_normal(height_map, vertical_scale, point[0], point[1]).unwrap_or(Vec3::Z)
    };

    let last_station = stations.saturating_sub(1).max(1);
    let last_column = columns.saturating_sub(1).max(1);
    let probes = column_count(
        width_of(start_left, start_right).max(width_of(end_left, end_right)),
        1.0,
        true,
    )
    .max(columns);
    let last_probe = probes.saturating_sub(1).max(1);
    for index in 0..stations {
        #[allow(clippy::cast_precision_loss)]
        let t = index as f32 / last_station as f32;
        let left = lerp2(start_left, end_left, t);
        let right = lerp2(start_right, end_right, t);

        let across_station = |column: usize, last: usize| {
            #[allow(clippy::cast_precision_loss)]
            let s = column as f32 / last.max(1) as f32;
            lerp2(left, right, s)
        };
        let heights: Vec<Option<f32>> = (0..probes)
            .map(|c| sample(across_station(c, last_probe)))
            .collect();
        if heights.iter().any(Option::is_none) {
            off_map = true;
        }
        let fallback = heights.iter().flatten().copied().next().unwrap_or(0.0);
        let station_max = heights
            .iter()
            .flatten()
            .fold(None::<f32>, |best, value| {
                Some(best.map_or(*value, |best| best.max(*value)))
            })
            .unwrap_or(fallback);

        for column in 0..columns {
            let point = across_station(column, last_column);
            let z = match options.height {
                RoadHeight::Surface => sample(point).unwrap_or(fallback),
                RoadHeight::StationMax | RoadHeight::CellMax => station_max,
            };
            let from_origin = sub(point, quad.origin);
            let (u, v) = if repeat.is_finite() && repeat != 0.0 {
                (
                    quad.u_offset + dot(quad.u_axis, from_origin) / repeat,
                    quad.v_offset - dot(quad.v_axis, from_origin) / repeat,
                )
            } else {
                (quad.u_offset, quad.v_offset)
            };
            mesh.vertices.push(Vertex::new(
                [point[0], point[1], z + lift],
                normal(point).to_array(),
                [u, v],
            ));
        }
    }

    for station in 0..stations.saturating_sub(1) {
        for column in 0..columns.saturating_sub(1) {
            let corner = |station: usize, column: usize| {
                u32::try_from(station.saturating_mul(columns).saturating_add(column)).unwrap_or(0)
            };
            let (next_station, next_column) = (station.saturating_add(1), column.saturating_add(1));
            let (a, b) = (corner(station, column), corner(station, next_column));
            let (c, d) = (
                corner(next_station, column),
                corner(next_station, next_column),
            );
            mesh.indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }

    BuiltSegment { mesh, off_map }
}

fn width_of(a: [f32; 2], b: [f32; 2]) -> f32 {
    let d = sub(a, b);
    (d[0] * d[0] + d[1] * d[1]).sqrt()
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn column_count(width: f32, steps: f32, round_up: bool) -> usize {
    if steps <= 0.0 {
        return 2;
    }
    divisions(width * steps, round_up, MAX_COLUMNS)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn divisions(extent: f32, round_up: bool, cap: usize) -> usize {
    let cells = extent / MAP_XY_FACTOR;
    if !cells.is_finite() || cells <= 0.0 {
        return 2;
    }
    let rounded = if round_up {
        cells.ceil()
    } else {
        cells.floor()
    };
    #[allow(clippy::cast_precision_loss)]
    let capped = rounded.min(cap as f32);
    (capped as usize).saturating_add(1).clamp(2, cap)
}

fn fit_budget(stations: usize, columns: usize) -> (usize, usize) {
    if stations.saturating_mul(columns) <= MAX_QUAD_VERTICES {
        return (stations, columns);
    }
    let columns = MAX_QUAD_VERTICES
        .checked_div(stations.max(1))
        .unwrap_or(2)
        .clamp(2, columns);
    let stations = MAX_QUAD_VERTICES
        .checked_div(columns.max(1))
        .unwrap_or(2)
        .clamp(2, stations);
    (stations, columns)
}

fn station_count(length: f32, steps: f32, round_up: bool) -> usize {
    divisions(length * steps, round_up, MAX_STATIONS)
}

fn append(mesh: &mut MeshData, other: MeshData) {
    let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    mesh.vertices.extend(other.vertices);
    mesh.indices
        .extend(other.indices.into_iter().map(|i| i.saturating_add(base)));
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests;
