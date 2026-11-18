use mint_render::{Aabb, Blend, Instance, Mat4, Renderer};
use mint_sage_asset::{
    pivot_reach, sample_blend_into, sample_pose, sample_pose_into, PoseBuffers, SampledPose,
};
use mint_sage_ini::{AnimationFlags, AnimationMode, DefaultAnimation};
use mint_sage_w3d::{Animation, Hierarchy};

use crate::cache::CachedModel;
use crate::convert::{pose_to_matrices, skin_mesh};
use crate::load::LoadedAnimation;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Completion {
    Hold,
    #[default]
    Restart,
}

#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn frame_at(
    mode: AnimationMode,
    completion: Completion,
    num_frames: u32,
    rate: u32,
    start: f32,
    seconds: f32,
) -> f32 {
    let last = (num_frames.max(1) - 1) as f32;
    let start = if start.is_finite() {
        start.clamp(0.0, last)
    } else {
        0.0
    };
    if last <= 0.0 || !seconds.is_finite() || mode == AnimationMode::Manual {
        return start;
    }
    let advanced = seconds * rate as f32;
    if !advanced.is_finite() {
        return start;
    }

    let frame = match mode {
        AnimationMode::Manual => start,
        AnimationMode::Once => match completion {
            Completion::Hold => (start + advanced).min(last),
            Completion::Restart => start + wrapped(advanced, last - start),
        },
        AnimationMode::OnceBackwards => match completion {
            Completion::Hold => (start - advanced).max(0.0),
            Completion::Restart => start - wrapped(advanced, start),
        },
        AnimationMode::Loop => wrapped(start + advanced, last),
        AnimationMode::LoopBackwards => wrapped(start - advanced, last),
        AnimationMode::PingPong => {
            let phase = wrapped(start + advanced, last * 2.0);
            if phase <= last {
                phase
            } else {
                last * 2.0 - phase
            }
        }
    };
    if frame.is_finite() {
        frame.clamp(0.0, last)
    } else {
        start
    }
}

fn wrapped(value: f32, period: f32) -> f32 {
    if !period.is_finite() || period <= 0.0 || !value.is_finite() {
        return 0.0;
    }
    let out = value.rem_euclid(period);
    if out.is_finite() && out < period {
        out
    } else {
        0.0
    }
}

#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn start_frame(mode: AnimationMode, flags: AnimationFlags, num_frames: u32) -> f32 {
    let last = (num_frames.max(1) - 1) as f32;
    if flags.start_frame_first {
        return 0.0;
    }
    if flags.start_frame_last {
        return last;
    }
    match mode {
        AnimationMode::OnceBackwards | AnimationMode::LoopBackwards => last,
        _ => 0.0,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindRefusal {
    NoHierarchy,
}

impl core::fmt::Display for BindRefusal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BindRefusal::NoHierarchy => write!(f, "the model has no hierarchy to pose"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PartPose {
    pub transform: Option<Mat4>,
    pub bounds: Option<Aabb>,
    pub opacity: f32,
    pub visible: bool,
}

impl Default for PartPose {
    fn default() -> Self {
        Self {
            transform: None,
            bounds: None,
            opacity: 1.0,
            visible: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelAnimation {
    pub asset: String,
    pub mode: AnimationMode,
    pub flags: AnimationFlags,
    pub frames: u32,
    pub rate: u32,
    pub start: f32,
    pub skinned: Vec<usize>,
    pub rigid: Vec<(usize, usize)>,
    pub rigid_static: usize,
    reach: Vec<f32>,
    animation: Animation,
    hierarchy: Hierarchy,
}

impl ModelAnimation {
    pub fn bind(
        model: &CachedModel,
        declared: &DefaultAnimation,
        loaded: LoadedAnimation,
    ) -> Result<Self, BindRefusal> {
        let Some(hierarchy) = model.hierarchy.clone() else {
            return Err(BindRefusal::NoHierarchy);
        };

        let mut driven: Vec<u32> = loaded
            .animation
            .channels
            .iter()
            .map(|channel| channel.pivot)
            .filter(|pivot| *pivot != 0)
            .collect();
        driven.sort_unstable();
        driven.dedup();

        let moves = pivots_that_move(&hierarchy, &driven);

        let mut skinned = Vec::new();
        let mut rigid = Vec::new();
        let mut rigid_static = 0usize;
        for (index, part) in model.parts.iter().enumerate() {
            if part.skin.is_some() {
                skinned.push(index);
            } else if let Some(pivot) = part.pivot {
                if moves.get(pivot).copied().unwrap_or(false) {
                    rigid.push((index, pivot));
                } else {
                    rigid_static = rigid_static.saturating_add(1);
                }
            }
        }
        let mode = declared.mode.unwrap_or(AnimationMode::Loop);
        let frames = loaded.animation.num_frames;
        Ok(Self {
            asset: declared.asset().to_owned(),
            mode,
            flags: declared.flags,
            frames,
            rate: loaded.animation.frame_rate,
            start: start_frame(mode, declared.flags, frames),
            skinned,
            rigid,
            rigid_static,
            reach: pivot_reach(&hierarchy, &loaded.animation),
            animation: loaded.animation,
            hierarchy,
        })
    }

    #[must_use]
    pub fn reach(&self) -> &[f32] {
        &self.reach
    }

    #[must_use]
    pub fn frame_at(&self, completion: Completion, seconds: f32) -> f32 {
        frame_at(
            self.mode,
            completion,
            self.frames,
            self.rate,
            self.start,
            seconds,
        )
    }

    #[must_use]
    pub fn moves(&self) -> bool {
        self.mode != AnimationMode::Manual && self.frames > 1
    }

    pub fn reset(&self, renderer: &mut Renderer, model: &CachedModel, out: &mut Vec<PartPose>) {
        out.clear();
        out.resize(model.parts.len(), PartPose::default());
        let bind = mint_sage_asset::SampledPose::bind(&self.hierarchy);
        let bones = pose_to_matrices(&bind.world);
        for index in &self.skinned {
            let Some(part) = model.parts.get(*index) else {
                continue;
            };
            let Some(source) = &part.skin else {
                continue;
            };
            let rest = skin_mesh(source, &bones);
            let _ = renderer.update_mesh_vertices(part.instance.mesh, &rest);
        }
    }

    pub fn bones(&self, frame: f32, out: &mut Vec<Mat4>) {
        let pose = sample_pose(&self.hierarchy, &self.animation, frame);
        out.clear();
        out.extend(pose_to_matrices(&pose.world));
    }

    #[must_use]
    pub fn bone_count(&self) -> usize {
        self.hierarchy.len()
    }

    pub fn pose_into(
        &self,
        model: &CachedModel,
        frame: f32,
        bones: &mut [Mat4],
        out: &mut Vec<PartPose>,
        scratch: &mut PoseBuffers,
    ) {
        sample_pose_into(&self.hierarchy, &self.animation, frame, scratch);
        self.write_pose(model, &scratch.pose, bones, out);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn pose_blend_into(
        &self,
        model: &CachedModel,
        frame: f32,
        from: &ModelAnimation,
        from_frame: f32,
        weight: f32,
        bones: &mut [Mat4],
        out: &mut Vec<PartPose>,
        scratch: &mut PoseBuffers,
    ) {
        sample_blend_into(
            &self.hierarchy,
            &from.animation,
            from_frame,
            &self.animation,
            frame,
            weight,
            scratch,
        );
        self.write_pose(model, &scratch.pose, bones, out);
    }

    fn write_pose(
        &self,
        model: &CachedModel,
        pose: &SampledPose,
        bones: &mut [Mat4],
        out: &mut Vec<PartPose>,
    ) {
        out.clear();
        out.resize(model.parts.len(), PartPose::default());
        for (slot, transform) in bones.iter_mut().zip(&pose.world) {
            *slot = crate::convert::pose_matrix(transform);
        }
        for (index, pivot) in &self.rigid {
            let (Some(matrix), Some(slot)) = (bones.get(*pivot), out.get_mut(*index)) else {
                continue;
            };
            slot.transform = Some(*matrix);
            slot.opacity = pose.opacity.get(*pivot).copied().unwrap_or(1.0);
            slot.visible = pose.visible.get(*pivot).copied().unwrap_or(true);
        }
    }

    pub fn apply(
        &self,
        renderer: &mut Renderer,
        model: &CachedModel,
        frame: f32,
        out: &mut Vec<PartPose>,
    ) -> ApplyReport {
        out.clear();
        out.resize(model.parts.len(), PartPose::default());
        let pose = sample_pose(&self.hierarchy, &self.animation, frame);
        let bones = pose_to_matrices(&pose.world);
        let mut report = ApplyReport::default();

        for index in &self.skinned {
            let (Some(part), Some(slot)) = (model.parts.get(*index), out.get_mut(*index)) else {
                continue;
            };
            let Some(source) = &part.skin else {
                continue;
            };
            let reposed = skin_mesh(source, &bones);
            slot.bounds = Some(reposed.bounds());
            if renderer
                .update_mesh_vertices(part.instance.mesh, &reposed)
                .is_err()
            {
                report.mesh_updates_refused = report.mesh_updates_refused.saturating_add(1);
                slot.bounds = None;
            }
            report.skinned = report.skinned.saturating_add(1);
        }

        for (index, pivot) in &self.rigid {
            let (Some(matrix), Some(slot)) = (bones.get(*pivot), out.get_mut(*index)) else {
                continue;
            };
            slot.transform = Some(*matrix);
            slot.opacity = pose.opacity.get(*pivot).copied().unwrap_or(1.0);
            slot.visible = pose.visible.get(*pivot).copied().unwrap_or(true);
            report.rigid = report.rigid.saturating_add(1);
        }
        report
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ApplyReport {
    pub skinned: usize,
    pub rigid: usize,
    pub mesh_updates_refused: usize,
}

impl ApplyReport {
    pub fn add(&mut self, other: ApplyReport) {
        self.skinned = self.skinned.saturating_add(other.skinned);
        self.rigid = self.rigid.saturating_add(other.rigid);
        self.mesh_updates_refused = self
            .mesh_updates_refused
            .saturating_add(other.mesh_updates_refused);
    }
}

#[must_use]
pub fn faded(blend: Blend, tint: [f32; 4], opacity: f32) -> (Blend, [f32; 4]) {
    if opacity >= 1.0 || opacity.is_nan() {
        return (blend, tint);
    }
    let a = opacity.max(0.0);
    let [r, g, b, alpha] = tint;
    match blend {
        Blend::Opaque => (Blend::AlphaBlend, [r, g, b, alpha * a]),
        Blend::AlphaBlend
        | Blend::AlphaAdditive
        | Blend::AlphaBlendWritesDepth
        | Blend::MergeMask
        | Blend::MergeAlphaBlend => (blend, [r, g, b, alpha * a]),
        Blend::Additive
        | Blend::AddScaledByAlpha
        | Blend::AddScaledByColor
        | Blend::AddInverseScaledByColor
        | Blend::Multiply => (blend, [r * a, g * a, b * a, alpha]),
    }
}

impl CachedModel {
    pub fn place_many_posed(
        &self,
        placements: &[Mat4],
        poses: &[PartPose],
        out: &mut Vec<Instance>,
    ) {
        out.reserve(self.parts.len().saturating_mul(placements.len()));
        for (index, part) in self.parts.iter().enumerate() {
            let pose = poses.get(index).copied().unwrap_or_default();
            let local = pose.transform.unwrap_or(part.instance.transform);
            let (blend, tint) = faded(part.instance.blend, part.instance.tint, pose.opacity);
            for placement in placements {
                out.push(Instance {
                    transform: crate::cache::placed_transform(*placement, local),
                    bounds: pose.bounds.or(part.instance.bounds),
                    blend,
                    tint,
                    visible: part.instance.visible && pose.visible,
                    ..part.instance
                });
            }
        }
    }
}

fn pivots_that_move(hierarchy: &Hierarchy, driven: &[u32]) -> Vec<bool> {
    let mut moves: Vec<bool> = Vec::with_capacity(hierarchy.pivots.len());
    for (index, pivot) in hierarchy.pivots.iter().enumerate() {
        let inherited = pivot
            .parent
            .and_then(|parent| usize::try_from(parent).ok())
            .filter(|parent| *parent < index)
            .and_then(|parent| moves.get(parent).copied())
            .unwrap_or(false);
        let own = u32::try_from(index).is_ok_and(|index| driven.binary_search(&index).is_ok());
        moves.push(own || inherited);
    }
    moves
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::unwrap_used)]
mod tests;
