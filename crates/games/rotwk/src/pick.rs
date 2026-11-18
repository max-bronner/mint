use mint_render::{Camera, Ray, Vec3};
use mint_sim::{Entity, PlayerId};

use crate::render_state::UnitView;
use mint_sage_asset::{KindOf, PickType};
use mint_sage_modules::effects::Name;
use mint_sage_modules::{Geometry, PieceShape};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum BoxMode {
    #[default]
    Centre,
    Bounds,
}

impl BoxMode {
    pub(crate) const ALL: [Self; 2] = [Self::Centre, Self::Bounds];

    pub(crate) const fn next(self) -> Self {
        match self {
            Self::Centre => Self::Bounds,
            Self::Bounds => Self::Centre,
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Centre => "box=centre-point (faithful)",
            Self::Bounds => "box=bounds-overlap (modern)",
        }
    }
}

pub(crate) fn can_select(unit: &UnitView, dragging: bool, local: PlayerId) -> bool {
    if unit.pick_type != PickType::Selectable {
        return false;
    }
    if dragging {
        if unit.kind_of.contains(KindOf::STRUCTURE) {
            return false;
        }
        if unit.owner != local {
            return false;
        }
    }
    true
}

pub(crate) fn ray_hits_cylinder(ray: Ray, base: Vec3, radius: f32, height: f32) -> Option<f32> {
    if !(radius.is_finite() && height.is_finite()) || radius <= 0.0 || height <= 0.0 {
        return None;
    }
    if !ray.origin.is_finite() || !ray.direction.is_finite() || !base.is_finite() {
        return None;
    }
    let ox = ray.origin.x - base.x;
    let oy = ray.origin.y - base.y;
    let (dx, dy) = (ray.direction.x, ray.direction.y);
    let a = dx * dx + dy * dy;
    let b = 2.0 * (ox * dx + oy * dy);
    let c = ox * ox + oy * oy - radius * radius;
    let (mut lo, mut hi) = if a <= f32::EPSILON {
        if c > 0.0 {
            return None;
        }
        (f32::NEG_INFINITY, f32::INFINITY)
    } else {
        let discriminant = b * b - 4.0 * a * c;
        if discriminant < 0.0 {
            return None;
        }
        let root = discriminant.sqrt();
        let t0 = (-b - root) / (2.0 * a);
        let t1 = (-b + root) / (2.0 * a);
        (t0.min(t1), t0.max(t1))
    };

    let (z0, z1) = (base.z, base.z + height);
    if ray.direction.z.abs() <= f32::EPSILON {
        if ray.origin.z < z0 || ray.origin.z > z1 {
            return None;
        }
    } else {
        let ta = (z0 - ray.origin.z) / ray.direction.z;
        let tb = (z1 - ray.origin.z) / ray.direction.z;
        lo = lo.max(ta.min(tb));
        hi = hi.min(ta.max(tb));
    }

    let entry = lo.max(0.0);
    (hi >= entry && entry.is_finite()).then_some(entry)
}

fn ray_hits_box(
    ray: Ray,
    base: Vec3,
    (cos, sin): (f32, f32),
    (major, minor, height): (f32, f32, f32),
) -> Option<f32> {
    if !(major.is_finite() && minor.is_finite() && height.is_finite())
        || major < 0.0
        || minor < 0.0
        || height <= 0.0
    {
        return None;
    }
    if !ray.origin.is_finite() || !ray.direction.is_finite() || !base.is_finite() {
        return None;
    }
    let (px, py) = (ray.origin.x - base.x, ray.origin.y - base.y);
    let (dx, dy) = (ray.direction.x, ray.direction.y);
    let mut span = (f32::NEG_INFINITY, f32::INFINITY);
    let inside = slab(px * cos + py * sin, dx * cos + dy * sin, major, &mut span)
        && slab(py * cos - px * sin, dy * cos - dx * sin, minor, &mut span)
        && slab(
            ray.origin.z - base.z - height * 0.5,
            ray.direction.z,
            height * 0.5,
            &mut span,
        );
    let entry = span.0.max(0.0);
    (inside && span.1 >= entry && entry.is_finite()).then_some(entry)
}

fn slab(origin: f32, direction: f32, half: f32, span: &mut (f32, f32)) -> bool {
    if direction.abs() <= f32::EPSILON {
        return origin.abs() <= half;
    }
    let ta = (-half - origin) / direction;
    let tb = (half - origin) / direction;
    span.0 = span.0.max(ta.min(tb));
    span.1 = span.1.min(ta.max(tb));
    span.0 <= span.1
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Declared<'g> {
    pub(crate) geometry: Option<&'g Geometry>,
    pub(crate) overrides: &'g [(Name, bool)],
}

pub(crate) fn ray_hits_unit(ray: Ray, unit: &UnitView, declared: Declared<'_>) -> Option<f32> {
    let base = Vec3::new(unit.position.x, unit.position.y, unit.ground);
    let facing = (
        mint_math::f32::cos(unit.facing),
        mint_math::f32::sin(unit.facing),
    );
    let (cos, sin) = facing;
    let mut any = false;
    let mut nearest: Option<f32> = None;
    let pieces = declared
        .geometry
        .into_iter()
        .flat_map(|geometry| geometry.shown(declared.overrides))
        .filter(|piece| piece.radius().is_some());
    for piece in pieces {
        any = true;
        let height = piece.height.unwrap_or(unit.height);
        let (ox, oy) = piece.offset;
        let centre = Vec3::new(
            base.x + ox * cos - oy * sin,
            base.y + ox * sin + oy * cos,
            base.z,
        );
        let hit = match piece.shape {
            PieceShape::Disc { radius } => ray_hits_cylinder(ray, centre, radius, height),
            PieceShape::Box { major, minor } => {
                ray_hits_box(ray, centre, facing, (major, minor, height))
            }
        };
        if let Some(t) = hit {
            if nearest.is_none_or(|best| t < best) {
                nearest = Some(t);
            }
        }
    }
    if any {
        nearest
    } else {
        ray_hits_cylinder(ray, base, unit.radius, unit.height)
    }
}

fn nearest<'g>(
    units: &[UnitView],
    ray: Ray,
    declared: impl Fn(&UnitView) -> Declared<'g>,
    accept: impl Fn(&UnitView) -> bool,
) -> Option<Entity> {
    let mut best: Option<(f32, Entity)> = None;
    for unit in units {
        if !accept(unit) {
            continue;
        }
        let Some(t) = ray_hits_unit(ray, unit, declared(unit)) else {
            continue;
        };
        if best.is_none_or(|(closest, _)| t < closest) {
            best = Some((t, unit.entity));
        }
    }
    best.map(|(_, entity)| entity)
}

pub(crate) fn pick_unit<'g>(
    units: &[UnitView],
    ray: Ray,
    local: PlayerId,
    declared: impl Fn(&UnitView) -> Declared<'g>,
) -> Option<Entity> {
    nearest(units, ray, declared, |unit| can_select(unit, false, local))
}

pub(crate) fn pick_capturable<'g>(
    units: &[UnitView],
    ray: Ray,
    declared: impl Fn(&UnitView) -> Declared<'g>,
    capturable: impl Fn(Entity) -> bool,
) -> Option<Entity> {
    nearest(units, ray, declared, |unit| {
        unit.pick_type == PickType::Selectable && capturable(unit.entity)
    })
}

pub(crate) fn pick_target<'g>(
    units: &[UnitView],
    ray: Ray,
    declared: impl Fn(&UnitView) -> Declared<'g>,
    relation: impl Fn(&UnitView) -> mint_sage_combat::victim::Relation,
) -> Option<Entity> {
    pick_target_as(
        units,
        ray,
        declared,
        relation,
        mint_sage_combat::Mode::Order,
    )
}

pub(crate) fn pick_force_target<'g>(
    units: &[UnitView],
    ray: Ray,
    declared: impl Fn(&UnitView) -> Declared<'g>,
    relation: impl Fn(&UnitView) -> mint_sage_combat::victim::Relation,
) -> Option<Entity> {
    pick_target_as(
        units,
        ray,
        declared,
        relation,
        mint_sage_combat::Mode::Force,
    )
}

fn pick_target_as<'g>(
    units: &[UnitView],
    ray: Ray,
    declared: impl Fn(&UnitView) -> Declared<'g>,
    relation: impl Fn(&UnitView) -> mint_sage_combat::victim::Relation,
    mode: mint_sage_combat::Mode,
) -> Option<Entity> {
    nearest(units, ray, declared, |unit| {
        let attacker = mint_sage_combat::Attacker {
            relation: relation(unit),
            class_allowed: true,
            scans_structures: true,
        };
        let victim = mint_sage_combat::Victim {
            live: true,
            concealed: false,
            contained: false,
            kind_of: unit.kind_of,
            is_structure: unit.kind_of.contains(KindOf::STRUCTURE),
            seen: true,
            script_targetable: unit.targetable,
        };
        unit.pick_type == PickType::Selectable
            && mint_sage_combat::can_target(attacker, victim, mode)
    })
}

pub(crate) fn project(
    camera: &Camera,
    viewport: (u32, u32),
    point: Vec3,
) -> Option<(f32, f32, f32)> {
    let width = f32::from(u16::try_from(viewport.0).ok()?);
    let height = f32::from(u16::try_from(viewport.1).ok()?);
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let forward = camera.forward();
    let depth = (point.x - camera.eye.x) * forward.x
        + (point.y - camera.eye.y) * forward.y
        + (point.z - camera.eye.z) * forward.z;
    if depth.is_nan() || depth <= camera.near {
        return None;
    }
    let clip = camera
        .view_projection(width / height)
        .transform_point(point);
    if !clip.x.is_finite() || !clip.y.is_finite() {
        return None;
    }
    Some((
        (clip.x + 1.0) * 0.5 * width,
        (1.0 - clip.y) * 0.5 * height,
        depth,
    ))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ScreenRect {
    pub(crate) min_x: f32,
    pub(crate) min_y: f32,
    pub(crate) max_x: f32,
    pub(crate) max_y: f32,
}

impl ScreenRect {
    pub(crate) fn from_corners(a: (f32, f32), b: (f32, f32)) -> Self {
        Self {
            min_x: a.0.min(b.0),
            min_y: a.1.min(b.1),
            max_x: a.0.max(b.0),
            max_y: a.1.max(b.1),
        }
    }

    pub(crate) fn contains(self, x: f32, y: f32) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }

    pub(crate) fn overlaps_circle(self, x: f32, y: f32, radius: f32) -> bool {
        if !(radius.is_finite()
            && self.min_x.is_finite()
            && self.min_y.is_finite()
            && self.max_x.is_finite()
            && self.max_y.is_finite())
        {
            return false;
        }
        let nearest = |value: f32, low: f32, high: f32| {
            if value < low {
                low
            } else if value > high {
                high
            } else {
                value
            }
        };
        let dx = x - nearest(x, self.min_x, self.max_x);
        let dy = y - nearest(y, self.min_y, self.max_y);
        dx * dx + dy * dy <= radius * radius
    }
}

pub(crate) fn box_select(
    units: &[UnitView],
    camera: &Camera,
    viewport: (u32, u32),
    rect: ScreenRect,
    mode: BoxMode,
    local: PlayerId,
) -> Vec<Entity> {
    let height_px = f32::from(u16::try_from(viewport.1).unwrap_or(u16::MAX));
    let focal = camera.projection(1.0).get(1, 1);
    let mut hits: Vec<(u64, Entity)> = Vec::new();
    for unit in units {
        if !can_select(unit, true, local) {
            continue;
        }
        let inside = match mode {
            BoxMode::Centre => {
                let origin = Vec3::new(unit.position.x, unit.position.y, unit.ground);
                project(camera, viewport, origin).is_some_and(|(x, y, _)| rect.contains(x, y))
            }
            BoxMode::Bounds => {
                let centre = Vec3::new(
                    unit.position.x,
                    unit.position.y,
                    unit.ground + unit.height * 0.5,
                );
                let bound = (unit.radius * unit.radius + unit.height * unit.height * 0.25).sqrt();
                project(camera, viewport, centre).is_some_and(|(x, y, depth)| {
                    let pixels = bound * focal / depth * height_px * 0.5;
                    rect.overlaps_circle(x, y, pixels.max(0.0))
                })
            }
        };
        if inside {
            hits.push((unit.created, unit.entity));
        }
    }
    hits.sort_unstable_by_key(|(created, entity)| core::cmp::Reverse((*created, entity.index())));
    hits.into_iter().map(|(_, entity)| entity).collect()
}

#[cfg(test)]
pub(crate) mod tests;
