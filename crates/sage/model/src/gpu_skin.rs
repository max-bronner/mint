use mint_render::{transformed_bounds, Aabb, Mat4, MeshHandle, Renderer, Vec3};

use crate::cache::CachedModel;
use crate::convert::skin_vertices;

#[derive(Debug, Clone)]
pub struct GpuSkinnedPart {
    pub mesh: MeshHandle,
    pub bone_boxes: Vec<(usize, Aabb)>,
}

#[derive(Debug, Clone, Default)]
pub struct GpuSkin {
    pub parts: Vec<Option<GpuSkinnedPart>>,
    pub bones: usize,
    pub bind: Vec<Mat4>,
}

impl GpuSkin {
    pub fn upload(renderer: &mut Renderer, model: &CachedModel) -> Self {
        let bones = model.hierarchy.as_ref().map_or(0, |h| h.pivots.len());
        let bind = model.hierarchy.as_ref().map_or_else(Vec::new, |h| {
            crate::convert::pose_to_matrices(&mint_sage_asset::SampledPose::bind(h).world)
        });
        let parts = model
            .parts
            .iter()
            .map(|part| {
                let source = part.skin.as_ref()?;
                let skin = skin_vertices(source, bones);
                let mesh = renderer
                    .upload_skinned_mesh(&source.bone_space, &skin)
                    .ok()?;
                Some(GpuSkinnedPart {
                    mesh,
                    bone_boxes: bone_boxes(&source.bone_space.vertices, &skin),
                })
            })
            .collect();
        Self { parts, bones, bind }
    }

    #[must_use]
    pub fn any(&self) -> bool {
        self.parts.iter().any(Option::is_some)
    }

    #[must_use]
    pub fn posed_bounds(&self, part: usize, bones: &[Mat4]) -> Option<Aabb> {
        let skinned = self.parts.get(part)?.as_ref()?;
        Some(posed_bounds(&skinned.bone_boxes, bones))
    }
}

#[must_use]
pub fn posed_radius(model: &CachedModel, gpu: &GpuSkin, animation: &crate::ModelAnimation) -> f32 {
    let parts = model.parts.iter().enumerate().map(|(index, part)| {
        let skinned = gpu.parts.get(index).and_then(Option::as_ref);
        let driven = animation
            .rigid
            .iter()
            .find(|(at, _)| *at == index)
            .map(|(_, pivot)| *pivot);
        match (skinned, driven, part.instance.bounds) {
            (Some(skinned), _, _) => PartReach::Skinned(&skinned.bone_boxes),
            (None, Some(pivot), Some(local)) => PartReach::Driven(pivot, local),
            (None, None, Some(local)) => {
                PartReach::Fixed(transformed_bounds(&local, part.instance.transform))
            }
            (None, _, None) => PartReach::Unbounded,
        }
    });
    radius_of(animation.reach(), parts)
}

#[derive(Debug, Clone, Copy)]
pub enum PartReach<'a> {
    Skinned(&'a [(usize, Aabb)]),
    Driven(usize, Aabb),
    Fixed(Aabb),
    Unbounded,
}

#[must_use]
pub fn radius_of<'a>(reach: &[f32], parts: impl IntoIterator<Item = PartReach<'a>>) -> f32 {
    let far = |b: &Aabb| {
        if b.is_empty() {
            return 0.0;
        }
        let (lo, hi) = (b.min(), b.max());
        let x = lo.x.abs().max(hi.x.abs());
        let y = lo.y.abs().max(hi.y.abs());
        let z = lo.z.abs().max(hi.z.abs());
        (x * x + y * y + z * z).sqrt()
    };
    let carried =
        |pivot: usize, local: &Aabb| reach.get(pivot).map_or(f32::INFINITY, |r| r + far(local));
    parts
        .into_iter()
        .map(|part| match part {
            PartReach::Skinned(boxes) => boxes
                .iter()
                .map(|(bone, local)| carried(*bone, local))
                .fold(0.0, f32::max),
            PartReach::Driven(pivot, local) => carried(pivot, &local),
            PartReach::Fixed(local) => far(&local),
            PartReach::Unbounded => f32::INFINITY,
        })
        .fold(0.0, f32::max)
}

#[must_use]
pub fn bone_boxes(
    vertices: &[mint_render::Vertex],
    skin: &[mint_render::SkinVertex],
) -> Vec<(usize, Aabb)> {
    let mut boxes: Vec<(usize, Aabb)> = Vec::new();
    for (vertex, entry) in vertices.iter().zip(skin) {
        let copies = [vertex.position, entry.position2];
        for ((bone, weight), copy) in entry.bones.iter().zip(entry.weights).zip(copies) {
            if weight <= 0.0 {
                continue;
            }
            let bone = usize::from(*bone);
            let at = match boxes.binary_search_by_key(&bone, |(b, _)| *b) {
                Ok(at) => at,
                Err(at) => {
                    boxes.insert(at, (bone, Aabb::EMPTY));
                    at
                }
            };
            if let Some((_, found)) = boxes.get_mut(at) {
                found.insert(Vec3::from_array(copy));
            }
        }
    }
    boxes
}

#[must_use]
pub fn posed_bounds(boxes: &[(usize, Aabb)], bones: &[Mat4]) -> Aabb {
    let mut out = Aabb::EMPTY;
    for (bone, local) in boxes {
        if let Some(matrix) = bones.get(*bone) {
            out.union(&transformed_bounds(local, *matrix));
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests;
