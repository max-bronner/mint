use mint_render::{Camera, Frustum, RenderTargetHandle, Renderer, ShadowSample, TextureHandle};
use mint_sage_terrain::WaterReflectionSource;

use crate::scene::MatchScene;

const PLANAR_CLIP_BIAS: f32 = 0.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ReflectionRung {
    MirrorRetail,
    MirrorLow,
    #[default]
    Mirror,
}

impl ReflectionRung {
    pub(crate) const ALL: [Self; 3] = [Self::MirrorRetail, Self::MirrorLow, Self::Mirror];

    pub(crate) fn next(self) -> Self {
        let at = self.ordinal().saturating_sub(1);
        Self::ALL
            .get(at.saturating_add(1))
            .copied()
            .unwrap_or(Self::MirrorRetail)
    }

    pub(crate) fn ordinal(self) -> usize {
        Self::ALL
            .iter()
            .position(|rung| *rung == self)
            .map_or(1, |i| i.saturating_add(1))
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::MirrorRetail => "mirror-retail",
            Self::MirrorLow => "mirror-low",
            Self::Mirror => "mirror",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "planar" => Some(Self::Mirror),
            other => Self::ALL.into_iter().find(|rung| rung.label() == other),
        }
    }

    pub(crate) fn source(self) -> Option<WaterReflectionSource> {
        Some(WaterReflectionSource::Planar)
    }

    pub(crate) fn planar(self) -> bool {
        self.source() == Some(WaterReflectionSource::Planar)
    }

    pub(crate) fn reflects_units(self) -> bool {
        matches!(self, Self::MirrorLow | Self::Mirror)
    }

    fn divisor(self) -> u32 {
        if self == Self::MirrorLow {
            4
        } else {
            2
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum BorderInMirror {
    #[default]
    Off,
    Retail,
}

impl BorderInMirror {
    pub(crate) const ALL: [Self; 2] = [Self::Off, Self::Retail];

    pub(crate) fn next(self) -> Self {
        match self {
            Self::Off => Self::Retail,
            Self::Retail => Self::Off,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Off => "off (default, owner)",
            Self::Retail => "retail (ZH renderMirror)",
        }
    }

    pub(crate) fn ordinal(self) -> usize {
        Self::ALL
            .iter()
            .position(|rung| *rung == self)
            .map_or(1, |i| i.saturating_add(1))
    }
}

pub(crate) fn reflecting_templates(vfs: &mint_vfs::Vfs) -> std::collections::BTreeSet<String> {
    let mut flagged = std::collections::BTreeSet::new();
    let mut has_kind_of = std::collections::BTreeSet::new();
    let mut parents: Vec<(String, String)> = Vec::new();
    for path in vfs.paths_under("data/ini/object") {
        let Ok(bytes) = vfs.read(path.as_str()) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let mut current: Option<String> = None;
        for raw in text.lines() {
            let line = raw.split(';').next().unwrap_or("").trim();
            let mut words = line.split_whitespace();
            match words.next() {
                Some(head)
                    if !raw.starts_with(char::is_whitespace)
                        && ["Object", "ChildObject", "ObjectReskin"]
                            .iter()
                            .any(|k| head.eq_ignore_ascii_case(k)) =>
                {
                    current = words.next().map(str::to_ascii_lowercase);
                    if let (Some(name), Some(parent)) = (&current, words.next()) {
                        parents.push((name.clone(), parent.to_ascii_lowercase()));
                    }
                }
                Some(head) if head.eq_ignore_ascii_case("KindOf") => {
                    if let Some(name) = &current {
                        has_kind_of.insert(name.clone());
                        let upper = line.to_ascii_uppercase();
                        if upper.contains("CAN_CAST_REFLECTIONS") || upper.contains("SKYBOX") {
                            flagged.insert(name.clone());
                        }
                    }
                }
                _ => {}
            }
        }
    }
    for (child, parent) in parents {
        if !has_kind_of.contains(&child) && flagged.contains(&parent) {
            flagged.insert(child);
        }
    }
    flagged
}

pub(crate) struct FrameView {
    pub(crate) camera: Camera,
    pub(crate) target: Option<RenderTargetHandle>,
    pub(crate) first: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct MirrorCounts {
    pub(crate) rendered: u32,
    pub(crate) skipped: u32,
    pub(crate) draw_calls: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MirrorTally {
    frames: u64,
    rendered: u64,
    skipped: u64,
}

impl MirrorTally {
    pub(crate) fn add(&mut self, counts: MirrorCounts) {
        self.frames = self.frames.saturating_add(1);
        self.rendered = self.rendered.saturating_add(u64::from(counts.rendered));
        self.skipped = self.skipped.saturating_add(u64::from(counts.skipped));
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn line(&self) -> String {
        #[allow(clippy::cast_precision_loss)]
        let per = |n: u64| n as f64 / self.frames.max(1) as f64;
        format!(
            "mirror passes {:.2} rendered / {:.2} skipped a frame over {} frames",
            per(self.rendered),
            per(self.skipped),
            self.frames
        )
    }
}

fn planes_in_view(renderer: &Renderer, scene: &MatchScene, frame: &FrameView) -> Vec<bool> {
    let textures: Vec<Option<TextureHandle>> = scene
        .water_scene
        .planes
        .iter()
        .map(|(_, target)| renderer.render_target_texture(*target))
        .collect();
    let Some(main) = renderer.view_frustum(&frame.camera, frame.target) else {
        return vec![true; textures.len()];
    };
    let views: Vec<Frustum> = std::iter::once(main)
        .chain(scene.fx_water.view_frustum())
        .collect();
    let mut needed: Vec<bool> = textures.iter().map(Option::is_none).collect();
    for (slot, instance) in scene.drawn.iter().enumerate().skip(frame.first) {
        if needed.iter().all(|n| *n) {
            break;
        }
        if !instance.visible {
            continue;
        }
        let mut seen: Option<bool> = None;
        for (texture, need) in textures.iter().zip(needed.iter_mut()) {
            let Some(texture) = *texture else { continue };
            if *need || !renderer.samples_texture(instance, texture) {
                continue;
            }
            *need = *seen.get_or_insert_with(|| {
                let visible = |reach: &mint_render::Aabb| views.iter().any(|v| v.intersects(reach));
                if let Some(bounds) = instance.bounds {
                    return visible(&mint_render::transformed_bounds(
                        &bounds,
                        instance.transform,
                    ));
                }
                slot.checked_sub(scene.fx_span.start)
                    .filter(|_| scene.fx_span.contains(&slot))
                    .and_then(|n| scene.fx_water.surface_reach(n))
                    .is_none_or(|boxes| boxes.iter().any(visible))
            });
        }
    }
    needed
}

pub(crate) fn render_planar(
    renderer: &mut Renderer,
    scene: &mut MatchScene,
    camera: Camera,
    shadow: Option<ShadowSample>,
    frame: &FrameView,
) -> mint_render::Result<usize> {
    if scene.water_scene.planes.is_empty() {
        return Ok(0);
    }
    let needed = if scene.mirror_skip {
        planes_in_view(renderer, scene, frame)
    } else {
        vec![true; scene.water_scene.planes.len()]
    };
    let wanted = needed.iter().filter(|n| **n).count();
    scene.mirror_counts.skipped = u32::try_from(needed.len().saturating_sub(wanted)).unwrap_or(0);
    if wanted == 0 {
        return Ok(0);
    }
    scene.reflected.clear();
    let split = scene.water_span.start.min(scene.drawn.len());
    let resume = scene.water_span.end.min(scene.drawn.len());
    if let Some(before) = scene.drawn.get(..split) {
        scene.reflected.extend_from_slice(before);
    }
    if let Some(after) = scene.drawn.get(resume..) {
        if scene.reflection == ReflectionRung::MirrorRetail {
            let objects = scene.object_span.clone();
            let flags = &scene.object_reflects;
            scene.reflected.extend(
                after
                    .iter()
                    .enumerate()
                    .filter(|(offset, _)| {
                        let slot = resume.saturating_add(*offset);
                        objects.contains(&slot)
                            && flags
                                .get(slot.saturating_sub(objects.start))
                                .copied()
                                .unwrap_or(false)
                    })
                    .map(|(_, instance)| *instance),
            );
        } else {
            scene.reflected.extend_from_slice(after);
        }
    }
    if scene.reflected.is_empty() {
        return Ok(0);
    }

    let (width, height) = renderer.size();
    let divisor = scene.reflection.divisor();
    let (target_width, target_height) = (
        width.div_ceil(divisor).max(1),
        height.div_ceil(divisor).max(1),
    );
    let restore = renderer.cull_override();
    renderer.set_cull_override(Some(false));
    let mut result = Ok(());
    let mut drawn = 0usize;
    let unmask_border = scene.border_in_mirror == BorderInMirror::Off;
    if unmask_border {
        push_mirror_shroud_strength(renderer, scene, 0.0);
    }
    let planes: Vec<(f32, mint_render::RenderTargetHandle)> = scene.water_scene.planes.clone();
    for ((plane, target), needed) in planes.into_iter().zip(needed) {
        if !needed {
            continue;
        }
        if let Err(error) = renderer.resize_render_target(target, target_width, target_height) {
            if result.is_ok() {
                result = Err(error);
            }
            continue;
        }
        let mut mirrored = mirrored_camera(camera, plane);
        mirrored.up = mint_render::Vec3::new(-mirrored.up.x, -mirrored.up.y, -mirrored.up.z);
        let sky = scene
            .mirror_sky
            .as_ref()
            .map(|sky| sky.instance(mirrored.eye));
        if let Some(sky) = sky {
            scene.reflected.push(sky);
        }
        let mut mirror_scene =
            mint_sage_scene::terrain_scene(mirrored, &scene.reflected, &scene.rig, shadow);
        scene.model_lights.apply(
            &mut mirror_scene,
            scene.terrain_shading.cloud_cookie(&scene.tuning),
        );
        mirror_scene.fog = crate::scene::drawn_fog(scene);
        mirror_scene.visibility_mask = crate::scene::drawn_visibility_mask(scene);
        mirror_scene.clip_plane = Some([0.0, 0.0, 1.0, -(plane - PLANAR_CLIP_BIAS)]);
        let outcome = renderer.render_to_target(target, &mirror_scene);
        if sky.is_some() {
            scene.reflected.pop();
        }
        match outcome {
            Ok(()) => {
                drawn = drawn.saturating_add(1);
                scene.mirror_counts.draw_calls = scene
                    .mirror_counts
                    .draw_calls
                    .saturating_add(renderer.target_draw_calls());
            }
            Err(error) => {
                if result.is_ok() {
                    result = Err(error);
                }
            }
        }
    }
    if unmask_border {
        push_mirror_shroud_strength(renderer, scene, crate::scene::shroud_strength(scene));
    }
    renderer.set_cull_override(restore);
    scene.mirror_counts.rendered = u32::try_from(drawn).unwrap_or(u32::MAX);
    result.map(|()| drawn)
}

fn push_mirror_shroud_strength(renderer: &mut Renderer, scene: &mut MatchScene, strength: f32) {
    let was_loaded = scene.terrain_shading.modulation.shroud_loaded;
    scene.terrain_shading.modulation.shroud_loaded = was_loaded && strength > 0.0;
    let seconds = scene.terrain_shading.modulation.seconds;
    scene.terrain_shading.push(renderer, &scene.tuning, seconds);
    scene.terrain_shading.modulation.shroud_loaded = was_loaded;
    mint_sage_scene::push_shroud_scene(
        renderer,
        scene.road_scene.shader,
        scene.shroud_extent,
        strength,
    );
}

#[must_use]
pub(crate) fn mirrored_camera(camera: Camera, plane: f32) -> Camera {
    let mut mirrored = camera;
    mirrored.eye.z = mirror_about(plane, camera.eye.z);
    mirrored.target.z = mirror_about(plane, camera.target.z);
    mirrored.up.z = -camera.up.z;
    mirrored
}

#[must_use]
pub(crate) fn mirror_about(plane: f32, z: f32) -> f32 {
    2.0 * plane - z
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_reflects_to_the_far_side_of_the_plane() {
        assert!((mirror_about(10.0, 10.0) - 10.0).abs() < 1e-6);
        assert!((mirror_about(10.0, 30.0) - -10.0).abs() < 1e-6);
        assert!((mirror_about(0.0, 5.0) - -5.0).abs() < 1e-6);
    }

    #[test]
    fn reflecting_twice_returns_the_original() {
        for z in [-100.0_f32, 0.0, 3.5, 900.0] {
            assert!(
                (mirror_about(17.5, mirror_about(17.5, z)) - z).abs() < 1e-3,
                "{z}"
            );
        }
    }
}
