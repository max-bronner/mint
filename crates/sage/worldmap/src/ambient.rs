use std::sync::Arc;

use mint_render::{Instance, Mat4, Renderer};
use mint_sage_ini::living_world::presentation::{AnimObjectDef, MapInfoDef};
use mint_sage_ini::living_world::{Coord3, LivingWorldData};
use mint_sage_ini::{AnimationFlags, AnimationMode, AnimationSource, DefaultAnimation};
use mint_sage_model::{
    faded, load_animation, CachedModel, Completion, ModelAnimation, ModelCache, PartPose, Resolver,
};
use mint_vfs::Vfs;

use crate::curve::CLIENT_HZ;

const EYE_DWELL_FRAMES: u32 = 150;
const EYE_TURN_PER_FRAME: f32 = 2.0;

pub(crate) fn placement(at: [f32; 3], degrees: f32, scale: f32) -> Mat4 {
    let (sin, cos) = mint_math::f32::sin_cos(degrees * (core::f32::consts::PI / 180.0));
    Mat4::from_cols([
        [cos * scale, sin * scale, 0.0, 0.0],
        [-sin * scale, cos * scale, 0.0, 0.0],
        [0.0, 0.0, scale, 0.0],
        [at[0], at[1], at[2], 1.0],
    ])
}

fn shortest_turn(from: f32, to: f32) -> f32 {
    let mut d = (to - from) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d < -180.0 {
        d += 360.0;
    }
    d
}

pub(crate) fn turn_toward(current: f32, target: f32) -> f32 {
    current + shortest_turn(current, target).clamp(-EYE_TURN_PER_FRAME, EYE_TURN_PER_FRAME)
}

pub(crate) fn facing(from: [f32; 2], target: [f32; 2]) -> f32 {
    mint_math::f32::atan2(target[1] - from[1], target[0] - from[0])
        * (180.0 / core::f32::consts::PI)
}

pub(crate) fn look_point(points: &[[f32; 2]], frame: u32) -> Option<[f32; 2]> {
    let index = usize::try_from(frame / EYE_DWELL_FRAMES).ok()? % points.len().max(1);
    points.get(index).copied()
}

struct SceneryObject {
    model: Arc<CachedModel>,
    animation: Option<ModelAnimation>,
    at: [f32; 3],
    degrees: f32,
    phase_seconds: f32,
    pupil: bool,
    poses: Vec<PartPose>,
}

#[derive(Default)]
struct Eye {
    points: Vec<[f32; 2]>,
    yaw: f32,
}

struct Clouds {
    model: Arc<CachedModel>,
    count: u32,
    lifetime: u32,
    min: [f32; 3],
    max: [f32; 3],
}

struct Rays {
    model: Arc<CachedModel>,
    shells: u32,
    diameter: f32,
    lifetime: u32,
    color: [f32; 3],
    at: [f32; 3],
    started: Option<u32>,
}

#[derive(Default)]
pub(crate) struct Ambient {
    objects: Vec<SceneryObject>,
    eye: Eye,
    clouds: Option<Clouds>,
    rays: Option<Rays>,
    pub(crate) enabled: bool,
}

fn hash(i: u32, cycle: u32, salt: u32) -> f32 {
    let mut x = i
        .wrapping_mul(0x9E37_79B9)
        .wrapping_add(cycle.wrapping_mul(0x85EB_CA6B))
        .wrapping_add(salt.wrapping_mul(0xC2B2_AE35));
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    #[allow(clippy::cast_precision_loss)]
    let unit = (x >> 8) as f32 / 16_777_216.0;
    unit
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub(crate) fn envelope(age: u32, lifetime: u32) -> f32 {
    if lifetime == 0 || age >= lifetime {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let t = age as f32 / lifetime as f32;
    mint_math::f32::sin(t * core::f32::consts::PI)
}

pub(crate) fn cloud_at(
    index: u32,
    count: u32,
    frame: u32,
    lifetime: u32,
    min: [f32; 3],
    max: [f32; 3],
) -> ([f32; 3], f32) {
    let lifetime = lifetime.max(1);
    let offset = index.saturating_mul(lifetime) / count.max(1);
    let t = frame.saturating_add(offset);
    let cycle = t / lifetime;
    let at = [
        lerp(min[0], max[0], hash(index, cycle, 1)),
        lerp(min[1], max[1], hash(index, cycle, 2)),
        lerp(min[2], max[2], hash(index, cycle, 3)),
    ];
    (at, envelope(t % lifetime, lifetime))
}

pub(crate) fn ray_shell(age: u32, lifetime: u32, shell: u32, shells: u32) -> f32 {
    let delay = shell.saturating_mul(lifetime / 2) / shells.max(1);
    if age < delay {
        return 0.0;
    }
    envelope(age - delay, lifetime.saturating_sub(delay))
}

fn corner(a: Option<Coord3>) -> [f32; 3] {
    a.map_or([0.0; 3], |c| [c.x, c.y, c.z])
}

fn bound_box(a: [f32; 3], b: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    (
        [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])],
        [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])],
    )
}

impl Ambient {
    pub(crate) fn load(
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        cache: &mut ModelCache,
        data: &LivingWorldData,
        info: &MapInfoDef,
        look_points: Vec<[f32; 2]>,
    ) -> Self {
        let pupil_names: Vec<&str> = info
            .eye_towers
            .iter()
            .filter_map(|e| e.pupil_anim_object.as_deref())
            .collect();
        let mut objects = Vec::new();
        for def in data.anim_objects.iter() {
            let Some(name) = def.model.as_deref() else {
                continue;
            };
            let Some(model) = cache.load_uncached(renderer, vfs, resolver, name) else {
                continue;
            };
            let pupil = pupil_names
                .iter()
                .any(|p| p.eq_ignore_ascii_case(&def.name));
            objects.push(scenery_object(vfs, resolver, def, name, model, pupil));
        }
        let clouds = info
            .anim_cloud
            .as_deref()
            .zip(info.num_anim_clouds.filter(|n| *n > 0))
            .and_then(|(name, count)| {
                let model = cache.get_or_load(renderer, vfs, resolver, name)?;
                let (min, max) = bound_box(
                    corner(info.anim_cloud_region_min),
                    corner(info.anim_cloud_region_max),
                );
                Some(Clouds {
                    model,
                    count: u32::try_from(count).unwrap_or(0),
                    lifetime: info
                        .anim_cloud_lifetime
                        .and_then(|l| u32::try_from(l).ok())
                        .unwrap_or(300),
                    min,
                    max,
                })
            });
        let rays = info.anim_rays.as_deref().and_then(|name| {
            let model = cache.get_or_load(renderer, vfs, resolver, name)?;
            let scale = info.anim_rays_color_scale.unwrap_or(1.0);
            let color = info.anim_rays_color.map_or([1.0; 3], |c| [c.r, c.g, c.b]);
            let offset = corner(info.anim_rays_part_sys_offset);
            let base = corner(info.cloud_pos);
            Some(Rays {
                model,
                shells: info
                    .anim_rays_effect_shells
                    .and_then(|n| u32::try_from(n).ok())
                    .unwrap_or(1),
                diameter: info
                    .anim_rays_effect_diameter
                    .map_or(0.0, |d| f32::from(i16::try_from(d).unwrap_or(i16::MAX))),
                lifetime: info
                    .anim_rays_effect_lifetime
                    .and_then(|l| u32::try_from(l).ok())
                    .unwrap_or(300),
                color: [color[0] * scale, color[1] * scale, color[2] * scale],
                at: [
                    base[0] + offset[0],
                    base[1] + offset[1],
                    base[2] + offset[2],
                ],
                started: None,
            })
        });
        Self {
            objects,
            eye: Eye {
                points: look_points,
                yaw: 0.0,
            },
            clouds,
            rays,
            enabled: true,
        }
    }

    pub(crate) fn part_the_heavens(&mut self, frame: u32) -> bool {
        match self.rays.as_mut() {
            Some(rays) => {
                rays.started = Some(frame);
                true
            }
            None => false,
        }
    }

    pub(crate) fn step(&mut self, frame: u32) {
        let Some(pupil) = self.objects.iter().find(|o| o.pupil) else {
            return;
        };
        if let Some(target) = look_point(&self.eye.points, frame) {
            let wanted = facing([pupil.at[0], pupil.at[1]], target);
            self.eye.yaw = turn_toward(self.eye.yaw, wanted);
        }
    }

    pub(crate) fn summary(&self) -> String {
        format!(
            "{} anim objects ({} animated, eye {}), clouds {}, rays {}",
            self.objects.len(),
            self.objects
                .iter()
                .filter(|o| o.animation.is_some())
                .count(),
            self.objects.iter().any(|o| o.pupil),
            self.clouds.as_ref().map_or(0, |c| c.count),
            self.rays.as_ref().map_or(0, |r| r.shells),
        )
    }

    pub(crate) fn push(&mut self, renderer: &mut Renderer, frame: u32, out: &mut Vec<Instance>) {
        if !self.enabled {
            return;
        }
        let seconds = f32_from(frame) / CLIENT_HZ;
        for object in &mut self.objects {
            let degrees = if object.pupil {
                self.eye.yaw
            } else {
                object.degrees
            };
            if let Some(animation) = &object.animation {
                let at = animation.frame_at(Completion::Restart, seconds + object.phase_seconds);
                animation.apply(renderer, &object.model, at, &mut object.poses);
            }
            object.model.place_many_posed(
                &[placement(object.at, degrees, 1.0)],
                &object.poses,
                out,
            );
        }
        if let Some(clouds) = &self.clouds {
            for index in 0..clouds.count {
                let (at, opacity) = cloud_at(
                    index,
                    clouds.count,
                    frame,
                    clouds.lifetime,
                    clouds.min,
                    clouds.max,
                );
                push_faded(
                    &clouds.model,
                    placement(at, 0.0, 1.0),
                    [1.0; 3],
                    opacity,
                    out,
                );
            }
        }
        if let Some(rays) = &self.rays {
            if let Some(age) = rays.started.map(|s| frame.saturating_sub(s)) {
                let widest = rays.model.bounds.max().x - rays.model.bounds.min().x;
                for shell in 0..rays.shells {
                    let opacity = ray_shell(age, rays.lifetime, shell, rays.shells);
                    if opacity <= 0.0 {
                        continue;
                    }
                    let share = f32_from(shell.saturating_add(1)) / f32_from(rays.shells.max(1));
                    let scale = if widest > 0.0 {
                        rays.diameter * share / widest
                    } else {
                        1.0
                    };
                    push_faded(
                        &rays.model,
                        placement(rays.at, 0.0, scale),
                        rays.color,
                        opacity,
                        out,
                    );
                }
            }
        }
    }
}

fn f32_from(n: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let f = n as f32;
    f
}

fn push_faded(
    model: &CachedModel,
    at: Mat4,
    color: [f32; 3],
    opacity: f32,
    out: &mut Vec<Instance>,
) {
    let start = out.len();
    model.place_many_posed(&[at], &[], out);
    for instance in out.iter_mut().skip(start) {
        let [r, g, b, a] = instance.tint;
        let tint = [r * color[0], g * color[1], b * color[2], a];
        (instance.blend, instance.tint) = faded(instance.blend, tint, opacity);
    }
}

fn scenery_object(
    vfs: &Vfs,
    resolver: &Resolver,
    def: &AnimObjectDef,
    name: &str,
    model: Arc<CachedModel>,
    pupil: bool,
) -> SceneryObject {
    let mut animation = None;
    let mut phase_seconds = 0.0;
    if def.has_anim == Some(true) {
        let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
        let declared = DefaultAnimation {
            name: format!("{stem}.{stem}"),
            mode: Some(AnimationMode::Loop),
            blend_frames: None,
            flags: AnimationFlags::default(),
            source: AnimationSource::IdleAnimationState,
        };
        animation = load_animation(vfs, resolver, declared.asset())
            .ok()
            .and_then(|loaded| ModelAnimation::bind(&model, &declared, loaded).ok())
            .filter(ModelAnimation::moves);
        if let Some(bound) = &animation {
            let share = def.frame.unwrap_or(0.0).clamp(0.0, 1.0);
            phase_seconds = share * f32_from(bound.frames) / f32_from(bound.rate.max(1));
        }
    }
    SceneryObject {
        model,
        animation,
        at: corner(def.pos),
        degrees: def.orient_angle.unwrap_or(0.0),
        phase_seconds,
        pupil,
        poses: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cloud_flashes_up_and_down_and_takes_a_new_spot_each_lifetime() {
        let (min, max) = ([0.0; 3], [100.0; 3]);
        let (_, start) = cloud_at(0, 1, 0, 300, min, max);
        let (first, mid) = cloud_at(0, 1, 150, 300, min, max);
        let (second, _) = cloud_at(0, 1, 450, 300, min, max);
        assert!(start.abs() < 1e-6 && (mid - 1.0).abs() < 1e-3);
        assert_ne!(first, second);
        assert!(first.iter().all(|v| (0.0..=100.0).contains(v)));
    }

    #[test]
    fn clouds_are_staggered_across_the_lifetime() {
        let (min, max) = ([0.0; 3], [100.0; 3]);
        let a = cloud_at(0, 4, 0, 400, min, max).1;
        let b = cloud_at(2, 4, 0, 400, min, max).1;
        assert!(
            a < b,
            "cloud 2 is half a lifetime into its flash at frame 0"
        );
    }

    #[test]
    fn ray_shells_light_in_turn_and_all_are_dark_before_and_after() {
        assert_eq!(ray_shell(0, 300, 3, 6), 0.0);
        assert!(ray_shell(100, 300, 0, 6) > 0.0);
        assert_eq!(ray_shell(100, 300, 5, 6), 0.0);
        assert_eq!(ray_shell(300, 300, 0, 6), 0.0);
    }

    #[test]
    fn the_pupil_turns_the_short_way_at_a_limited_rate() {
        assert_eq!(turn_toward(179.0, -179.0), 181.0);
        assert_eq!(turn_toward(-179.0, 179.0), -181.0);
        assert_eq!(turn_toward(0.0, 1.0), 1.0);
        assert_eq!(turn_toward(0.0, -90.0), -EYE_TURN_PER_FRAME);
        assert!((facing([0.0, 0.0], [0.0, 5.0]) - 90.0).abs() < 1e-3);
    }

    #[test]
    fn the_eye_rests_on_each_look_point_in_turn_and_wraps() {
        let points = [[1.0, 0.0], [2.0, 0.0]];
        assert_eq!(look_point(&points, 0), Some([1.0, 0.0]));
        assert_eq!(look_point(&points, EYE_DWELL_FRAMES), Some([2.0, 0.0]));
        assert_eq!(look_point(&points, 2 * EYE_DWELL_FRAMES), Some([1.0, 0.0]));
        assert_eq!(look_point(&[], 5), None);
    }
}
