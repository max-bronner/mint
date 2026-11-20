use std::sync::Arc;

use mint_render::{Instance, Mat4, Renderer};
use mint_sage_ini::{AnimationFlags, AnimationSource, DefaultAnimation};
use mint_sage_model::{
    faded, load_animation, CachedModel, Completion, GpuSkin, ModelAnimation, PartPose, PoseBuffers,
    Resolver,
};
use mint_sage_particle::ModelDraw;
use mint_vfs::Vfs;

pub(crate) struct ParticleModel {
    model: Arc<CachedModel>,
    animation: Option<ModelAnimation>,
    gpu: GpuSkin,
}

impl ParticleModel {
    pub(crate) fn bind(
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        name: &str,
        model: Arc<CachedModel>,
    ) -> Self {
        let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
        let declared = DefaultAnimation {
            name: format!("{stem}.{stem}"),
            mode: Some(mint_sage_ini::AnimationMode::Loop),
            blend_frames: None,
            flags: AnimationFlags::default(),
            source: AnimationSource::IdleAnimationState,
        };
        let animation = match load_animation(vfs, resolver, declared.asset()) {
            Ok(loaded) => match ModelAnimation::bind(&model, &declared, loaded) {
                Ok(bound) => Some(bound),
                Err(refusal) => {
                    crate::once::report_once(&format!("  particle clip {name}: {refusal}"));
                    None
                }
            },
            Err(error) => {
                crate::once::report_once(&format!("  particle clip {name}: {error}"));
                None
            }
        }
        .filter(ModelAnimation::moves);
        let gpu = if animation.is_some() {
            GpuSkin::upload(renderer, &model)
        } else {
            GpuSkin::default()
        };
        let bound = Self {
            model,
            animation,
            gpu,
        };
        if bound.needs_frame() {
            crate::once::report_once(&format!(
                "  particle model {name}: clip {} frame(s), UV mapper {}",
                bound.animation.as_ref().map_or(0, |clip| clip.frames),
                bound.model.has_uv_mappers(),
            ));
        }
        bound
    }

    pub(crate) fn needs_frame(&self) -> bool {
        self.animation.is_some() || self.model.has_uv_mappers()
    }

    pub(crate) fn animate(
        &self,
        draw: &ModelDraw,
        out: &mut [Instance],
        seconds: f32,
        scratch: &mut PoseScratch,
        palette: &mut Vec<Mat4>,
        palette_base: usize,
    ) {
        let particles = draw.placements.len();
        let parts = self.model.parts.len();
        let Some(end) = parts
            .checked_mul(particles)
            .and_then(|span| draw.start.checked_add(span))
        else {
            return;
        };
        let Some(span) = out.get_mut(draw.start..end) else {
            return;
        };
        self.model.apply_uv_mappers(particles, span, seconds);
        let Some(animation) = &self.animation else {
            return;
        };
        scratch.bones.resize(animation.bone_count(), Mat4::IDENTITY);
        for (particle, (placement, age)) in draw.placements.iter().zip(&draw.ages).enumerate() {
            let frame = animation.frame_at(Completion::Restart, *age);
            animation.pose_into(
                &self.model,
                frame,
                &mut scratch.bones,
                &mut scratch.poses,
                &mut scratch.buffers,
            );
            let bone_base = if self.gpu.any() {
                let at = palette_base.saturating_add(palette.len());
                palette.extend_from_slice(&scratch.bones);
                u32::try_from(at).ok()
            } else {
                None
            };
            for (part_index, part) in self.model.parts.iter().enumerate() {
                let Some(pose) = scratch.poses.get(part_index).copied() else {
                    continue;
                };
                let slot = part_index
                    .saturating_mul(particles)
                    .saturating_add(particle);
                let Some(instance) = span.get_mut(slot) else {
                    continue;
                };
                apply_pose(instance, *placement, part.instance.transform, pose);
                let skinned = self.gpu.parts.get(part_index).and_then(Option::as_ref);
                if let (Some(skinned), Some(base)) = (skinned, bone_base) {
                    instance.mesh = skinned.mesh;
                    instance.bone_base = Some(base);
                    instance.bounds = self.gpu.posed_bounds(part_index, &scratch.bones);
                }
            }
        }
    }
}

fn apply_pose(instance: &mut Instance, placement: Mat4, bind: Mat4, pose: PartPose) {
    let local = pose.transform.unwrap_or(bind);
    instance.transform = placement.mul_mat4(local);
    let (blend, tint) = faded(instance.blend, instance.tint, pose.opacity);
    instance.blend = blend;
    instance.tint = tint;
    instance.visible = instance.visible && pose.visible;
}

#[derive(Default)]
pub(crate) struct PoseScratch {
    bones: Vec<Mat4>,
    poses: Vec<PartPose>,
    buffers: PoseBuffers,
}
