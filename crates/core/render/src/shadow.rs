use crate::camera::Camera;
use crate::frustum::visible_region;
use crate::math::{Mat4, Vec3};
use crate::mesh::Aabb;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShadowMapHandle(pub(crate) usize);

pub const MAX_SHADOW_CASCADES: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowCascade {
    pub view_proj: Mat4,
    pub bias: f32,
    pub texel: f32,
    pub depth_range: f32,
}

impl ShadowCascade {
    #[must_use]
    pub fn new(view_proj: Mat4, bias: f32) -> Self {
        Self {
            view_proj,
            bias,
            texel: 0.0,
            depth_range: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShadowEdge {
    Taps(f32),
    Tent,
    Contact(ContactHardening),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowSample {
    pub map: ShadowMapHandle,
    pub cascades: [Option<ShadowCascade>; MAX_SHADOW_CASCADES],
    pub lightness: f32,
    pub edge: ShadowEdge,
    pub blend: f32,
}

impl ShadowSample {
    #[must_use]
    pub fn single(map: ShadowMapHandle, view_proj: Mat4, bias: f32) -> Self {
        let mut cascades = [None; MAX_SHADOW_CASCADES];
        if let Some(first) = cascades.first_mut() {
            *first = Some(ShadowCascade::new(view_proj, bias));
        }
        Self {
            map,
            cascades,
            lightness: 0.0,
            edge: ShadowEdge::Taps(0.0),
            blend: 0.0,
        }
    }

    pub fn active_cascades(&self) -> impl Iterator<Item = &ShadowCascade> {
        self.cascades.iter().map_while(Option::as_ref)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactHardening {
    pub growth: f32,
    pub min_texels: f32,
    pub max_texels: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactShadows {
    pub length: f32,
    pub steps: u32,
    pub thickness: f32,
    pub lightness: f32,
}

#[must_use]
pub fn directional_light_view_proj(direction: Vec3, centre: Vec3, radius: f32, depth: f32) -> Mat4 {
    let radius = radius.max(1.0);
    let depth = depth.max(radius);
    light_view_proj(direction, centre, radius, depth, (0.0, 0.0))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowFit {
    pub distance: f32,
    pub margin: f32,
    pub depth: f32,
    pub texels: u32,
    pub bounds: Option<Aabb>,
}

impl Default for ShadowFit {
    fn default() -> Self {
        Self {
            distance: f32::INFINITY,
            margin: 0.0,
            depth: 0.0,
            texels: 0,
            bounds: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FittedShadow {
    pub view_proj: Mat4,
    pub centre: Vec3,
    pub radius: f32,
    pub range: f32,
    pub texel: f32,
    pub view_extent: f32,
    pub view_start: f32,
}

#[must_use]
pub fn fit_directional_light_to_view(
    direction: Vec3,
    camera: &Camera,
    aspect: f32,
    fit: ShadowFit,
) -> FittedShadow {
    let near = if camera.near.is_finite() {
        camera.near.max(0.0)
    } else {
        0.0
    };
    let reach = if fit.distance.is_finite() && fit.distance > 0.0 {
        fit.distance
    } else {
        f32::MAX
    };
    let far = if camera.far.is_finite() {
        camera.far.min(reach).max(near)
    } else {
        reach.max(near)
    };
    let corners = camera.frustum_corners(aspect, near, far);
    let region = fit
        .bounds
        .as_ref()
        .and_then(|b| visible_region(&corners, b));
    let points: &[Vec3] = match &region {
        Some(region) => region.points(),
        None => &corners,
    };
    let mut sum = Vec3::ZERO;
    let mut count = 0.0f32;
    for corner in points {
        sum = Vec3::new(sum.x + corner.x, sum.y + corner.y, sum.z + corner.z);
        count += 1.0;
    }
    let centre = if count > 0.0 {
        Vec3::new(sum.x / count, sum.y / count, sum.z / count)
    } else {
        Vec3::ZERO
    };
    let centre = if centre.is_finite() {
        centre
    } else {
        Vec3::ZERO
    };
    let view_forward = camera.forward();
    let mut view_extent = 0.0f32;
    let mut view_start = f32::INFINITY;
    for point in points {
        let along = view_forward.dot(Vec3::new(
            point.x - camera.eye.x,
            point.y - camera.eye.y,
            point.z - camera.eye.z,
        ));
        if along.is_finite() {
            view_extent = view_extent.max(along);
            view_start = view_start.min(along);
        }
    }
    let mut fitted = 0.0f32;
    for corner in points {
        let offset = Vec3::new(
            corner.x - centre.x,
            corner.y - centre.y,
            corner.z - centre.z,
        );
        fitted = fitted.max(offset.length());
    }
    let margin = if fit.margin.is_finite() && fit.margin > 0.0 {
        fit.margin
    } else {
        0.0
    };
    let fitted = if fitted.is_finite() {
        (fitted + margin).max(1.0)
    } else {
        1.0
    };

    let (radius, texel) = if fit.texels >= 2 {
        let texels = fit.texels as f32;
        let radius = fitted * texels / (texels - 1.0);
        (radius, 2.0 * radius / texels)
    } else {
        (fitted, 0.0)
    };

    let (forward, up) = light_basis(direction);
    let back = Vec3::new(-forward.x, -forward.y, -forward.z);
    let right = up.cross(back).normalize();
    let true_up = back.cross(right);
    let offset = if texel > 0.0 && texel.is_finite() {
        let along_right = right.dot(centre);
        let along_up = true_up.dot(centre);
        (
            (along_right / texel).round() * texel - along_right,
            (along_up / texel).round() * texel - along_up,
        )
    } else {
        (0.0, 0.0)
    };
    let headroom = if fit.depth.is_finite() && fit.depth > 0.0 {
        fit.depth
    } else {
        0.0
    };
    let depth = radius + headroom;
    FittedShadow {
        view_proj: light_view_proj(direction, centre, radius, depth, offset),
        centre,
        radius,
        range: depth + radius,
        texel,
        view_extent,
        view_start: if view_start.is_finite() {
            view_start.max(0.0)
        } else {
            0.0
        },
    }
}

fn light_view_proj(
    direction: Vec3,
    centre: Vec3,
    half: f32,
    depth: f32,
    offset: (f32, f32),
) -> Mat4 {
    let (forward, up) = light_basis(direction);
    let eye = Vec3::new(
        centre.x - forward.x * depth,
        centre.y - forward.y * depth,
        centre.z - forward.z * depth,
    );
    let view = Mat4::look_at(eye, centre, up);
    let (dx, dy) = offset;
    let projection = Mat4::orthographic(
        dx - half,
        dx + half,
        dy - half,
        dy + half,
        0.0,
        depth + half,
    );
    projection.mul_mat4(view)
}

fn light_basis(direction: Vec3) -> (Vec3, Vec3) {
    let forward = normalized_or(direction, Vec3::new(0.0, 0.0, -1.0));
    let up = if forward.z.abs() > 0.99 {
        Vec3::new(0.0, 1.0, 0.0)
    } else {
        Vec3::new(0.0, 0.0, 1.0)
    };
    (forward, up)
}

fn normalized_or(vector: Vec3, fallback: Vec3) -> Vec3 {
    if vector.length_squared() > f32::EPSILON {
        vector.normalize()
    } else {
        fallback
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests;
