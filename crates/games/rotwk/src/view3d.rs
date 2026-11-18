use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use mint_app::OrbitCamera;
use mint_render::{
    transformed_bounds, Aabb, Blend, Camera, DirectionalLight, Instance, Mat4, RenderTargetHandle,
    Renderer, Scene, Vec3, MAX_DIRECTIONAL_LIGHTS,
};

const UI_FOV_Y: f32 = 0.1;
use mint_sage_apt_player::{InstanceId, MovieId, RenderCommand, RenderList, RenderOptions};
use mint_sage_apt_viewer::host::{stage_space_aspect, MovieView, SurfaceContent, SurfaceIcon};
use mint_sage_ini::{AnimationFlags, AnimationMode, AnimationSource, DefaultAnimation};
use mint_sage_model::{CachedModel, ModelAnimation, ModelCache, PartPose, Resolver};
use mint_vfs::Vfs;

const CAMERA_BONE: &str = "CAMERA";
const W3D_HALF_HFOV_TAN: f32 = 0.466_307_66;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Framing {
    #[default]
    Camera,
    Cover,
    Contain,
}

impl Framing {
    pub(crate) const fn next(self) -> Self {
        match self {
            Self::Camera => Self::Cover,
            Self::Cover => Self::Contain,
            Self::Contain => Self::Camera,
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Camera => "camera (the model's CAMERA pivot, WW3D's 50-degree lens)",
            Self::Cover => "cover (the model fills the surface)",
            Self::Contain => "contain (the whole model fits inside)",
        }
    }
}

fn bone_camera(bone: Mat4, aspect: f32) -> Option<Camera> {
    let eye = bone.transform_point(Vec3::ZERO);
    let forward = bone.transform_vector(Vec3::new(0.0, 0.0, -1.0)).normalize();
    let up = bone.transform_vector(Vec3::new(0.0, 1.0, 0.0)).normalize();
    if !(eye.is_finite() && forward.is_finite() && up.is_finite()) || aspect <= 0.0 {
        return None;
    }
    let half_vertical = W3D_HALF_HFOV_TAN / aspect;
    Some(Camera {
        eye,
        target: eye + forward,
        up,
        fov_y: 2.0 * mint_math::f32::atan(half_vertical),
        near: 1.0,
        far: 100_000.0,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Playback {
    Hold(f32),
    PlayTo(f32),
    Loop,
}

impl Playback {
    pub(crate) fn parse(mode: &str, frame: f32) -> Self {
        match mode.to_ascii_uppercase().as_str() {
            "PLAY_TO_FRAME" => Self::PlayTo(frame),
            "LOOP" => Self::Loop,
            _ => Self::Hold(frame),
        }
    }

    pub(crate) fn advance(self, current: f32, dt: f32, rate: f32, frames: f32) -> f32 {
        let last = (frames - 1.0).max(0.0);
        let step = (dt * rate).max(0.0);
        match self {
            Self::Hold(frame) => frame.clamp(0.0, last),
            Self::PlayTo(frame) => {
                let target = frame.clamp(0.0, last);
                if current < target {
                    (current + step).min(target)
                } else {
                    (current - step).max(target)
                }
            }
            Self::Loop => {
                if last <= 0.0 {
                    0.0
                } else {
                    (current + step) % (last + 1.0)
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Clock {
    frame: f32,
    pending: Duration,
    shown: bool,
}

impl Clock {
    fn tick(&mut self, dt: Duration) {
        if self.shown {
            self.pending = self.pending.saturating_add(dt);
        } else {
            *self = Self::default();
        }
        self.shown = false;
    }
}

struct RenderObject {
    model: Arc<CachedModel>,
    animation: Option<ModelAnimation>,
    clock: Clock,
    poses: Vec<PartPose>,
    instances: Vec<Instance>,
}

type SurfaceKey = (MovieId, InstanceId);

pub(crate) struct View3d {
    vfs: Arc<Vfs>,
    resolver: Resolver,
    cache: ModelCache,
    objects: BTreeMap<String, Option<RenderObject>>,
    targets: HashMap<SurfaceKey, (RenderTargetHandle, u32, u32)>,
    free: Vec<RenderTargetHandle>,
    framing: Framing,
    clock: f32,
    list: RenderList,
}

impl View3d {
    pub(crate) fn new(vfs: Arc<Vfs>) -> Self {
        Self {
            vfs,
            resolver: Resolver::new(),
            cache: ModelCache::new(),
            objects: BTreeMap::new(),
            targets: HashMap::new(),
            free: Vec::new(),
            framing: Framing::default(),
            clock: 0.0,
            list: RenderList::default(),
        }
    }

    pub(crate) fn advance(&mut self, dt: Duration) {
        self.clock += dt.as_secs_f32();
        for object in self.objects.values_mut().flatten() {
            object.clock.tick(dt);
        }
    }

    pub(crate) fn toggle_framing(&mut self) -> Framing {
        self.framing = self.framing.next();
        self.framing
    }

    pub(crate) fn draw(&mut self, renderer: &mut Renderer, view: &mut MovieView) {
        let movie = view.movie_id();
        self.list.commands.clear();
        view.player
            .render_with(&mut self.list, RenderOptions { surfaces: true });
        let surfaces: Vec<InstanceId> = self
            .list
            .commands
            .iter()
            .filter_map(|command| match command {
                RenderCommand::Surface { instance, .. } => Some(*instance),
                _ => None,
            })
            .filter(|&id| view.player.own_string(id, "_type").as_deref() == Some("View3D"))
            .collect();
        for &id in &surfaces {
            let content = self.draw_surface(renderer, view, id);
            view.set_surface(id, content);
        }
        let gone: Vec<SurfaceKey> = self
            .targets
            .keys()
            .filter(|(m, id)| *m == movie && !surfaces.contains(id))
            .copied()
            .collect();
        for key in gone {
            if let Some((target, _, _)) = self.targets.remove(&key) {
                if renderer.resize_render_target(target, 1, 1).is_ok() {
                    self.free.push(target);
                }
            }
        }
    }

    fn draw_surface(
        &mut self,
        renderer: &mut Renderer,
        view: &mut MovieView,
        id: InstanceId,
    ) -> Option<SurfaceContent> {
        let name = view.player.own_string(id, "_RenderObj")?.to_string();
        if name.is_empty() {
            return Some(SurfaceContent::Hidden);
        }
        let mode = view
            .player
            .own_string(id, "_AnimMode")
            .map_or_else(String::new, |m| m.to_string());
        let frame = match view.player.own_value(id, "_Frame") {
            Some(mint_sage_apt_player::Value::Number(n)) => {
                #[allow(clippy::cast_possible_truncation)]
                let n = n as f32;
                n
            }
            _ => 0.0,
        };
        let [l, t, r, b] = view.slot_rect(id)?;
        let (rw, rh) = ((r - l).max(1.0), (b - t).max(1.0));
        let stage_aspect = stage_space_aspect(&view.player, id).unwrap_or(rw / rh);
        let framing = self.framing;
        let clock = self.clock;
        let object = self.object(renderer, &name)?;
        object.clock.shown = true;
        let dt = std::mem::take(&mut object.clock.pending).as_secs_f32();
        let playback = Playback::parse(&mode, frame);
        let camera_pivot = object.model.pivot_named(CAMERA_BONE);
        let mut camera_bone = camera_pivot.and_then(|p| object.model.bind_bone(p));
        if let Some(animation) = &object.animation {
            #[allow(clippy::cast_precision_loss)]
            let (rate, frames) = (animation.rate as f32, animation.frames as f32);
            object.clock.frame = playback.advance(object.clock.frame, dt, rate, frames);
            let mut bones = vec![mint_render::Mat4::IDENTITY; animation.bone_count()];
            let mut scratch = mint_sage_model::PoseBuffers::default();
            animation.pose_into(
                &object.model,
                object.clock.frame,
                &mut bones,
                &mut object.poses,
                &mut scratch,
            );
            if let Some(bone) = camera_pivot.and_then(|p| bones.get(p)) {
                camera_bone = Some(*bone);
            }
        }
        object.instances.clear();
        object.model.place_many_posed(
            &[mint_render::Mat4::IDENTITY],
            &object.poses,
            &mut object.instances,
        );
        let _ = object
            .model
            .apply_uv_mappers(1, &mut object.instances, clock);
        let through_bone = match (framing, camera_bone) {
            (Framing::Camera, Some(bone)) => bone_camera(bone, stage_aspect),
            _ => None,
        };
        let (camera, crop, aspect) = match through_bone {
            Some(camera) => (camera, 1.0, stage_aspect),
            None => {
                let fallback = if framing == Framing::Contain {
                    Framing::Contain
                } else {
                    Framing::Cover
                };
                let (orbit, crop) =
                    frame_camera(&part_boxes(&object.instances), rw / rh, fallback)?;
                (orbit.camera(), crop, rw / rh)
            }
        };
        let instances = std::mem::take(&mut object.instances);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (w, h) = (
            (rh * aspect / crop).round().clamp(1.0, 8192.0) as u32,
            (rh / crop).round().clamp(1.0, 8192.0) as u32,
        );
        let target = self.target(renderer, (view.movie_id(), id), w, h);
        let rendered = target.and_then(|target| {
            let mut scene = Scene::new(camera).with_instances(&instances);
            scene.lights = [DirectionalLight::default(); MAX_DIRECTIONAL_LIGHTS];
            scene.ambient_color = [1.0, 1.0, 1.0];
            scene.clear_color = [0.0, 0.0, 0.0, 0.0];
            match renderer.render_to_target(target, &scene) {
                Ok(()) => renderer.render_target_texture(target),
                Err(error) => {
                    println!("view3d: {name} did not render ({error})");
                    None
                }
            }
        });
        if let Some(object) = self
            .objects
            .get_mut(&name.to_ascii_lowercase())
            .and_then(Option::as_mut)
        {
            object.instances = instances;
        }
        let margin = (1.0 - crop) * 0.5;
        rendered.map(|texture| {
            SurfaceContent::Icon(SurfaceIcon {
                texture,
                uv_rect: [margin, margin, crop, crop],
                tint: [1.0, 1.0, 1.0, 1.0],
                premultiplied: true,
                rotated: false,
            })
        })
    }

    fn target(
        &mut self,
        renderer: &mut Renderer,
        id: SurfaceKey,
        w: u32,
        h: u32,
    ) -> Option<RenderTargetHandle> {
        match self.targets.get(&id).copied() {
            Some((target, tw, th)) if (tw, th) == (w, h) => Some(target),
            Some((target, _, _)) => {
                renderer.resize_render_target(target, w, h).ok()?;
                self.targets.insert(id, (target, w, h));
                Some(target)
            }
            None => {
                let target = match self.free.pop() {
                    Some(target) => {
                        renderer.resize_render_target(target, w, h).ok()?;
                        target
                    }
                    None => renderer.create_render_target(w, h).ok()?,
                };
                self.targets.insert(id, (target, w, h));
                Some(target)
            }
        }
    }

    fn object(&mut self, renderer: &mut Renderer, name: &str) -> Option<&mut RenderObject> {
        let key = name.to_ascii_lowercase();
        if !self.objects.contains_key(&key) {
            let loaded = self
                .cache
                .get_or_load(renderer, &self.vfs, &self.resolver, name)
                .map(|model| {
                    let animation = embedded_animation(&self.vfs, &self.resolver, &model, name);
                    println!(
                        "view3d: {name} - {} part(s), {}",
                        model.parts.len(),
                        animation.as_ref().map_or_else(
                            || "no animation".to_owned(),
                            |a| format!("animation {} frame(s) at {} fps", a.frames, a.rate)
                        )
                    );
                    RenderObject {
                        model,
                        animation,
                        clock: Clock::default(),
                        poses: Vec::new(),
                        instances: Vec::new(),
                    }
                });
            if loaded.is_none() {
                println!("view3d: {name} did not load - its surface keeps its placeholder");
            }
            self.objects.insert(key.clone(), loaded);
        }
        self.objects.get_mut(&key)?.as_mut()
    }
}

fn embedded_animation(
    vfs: &Vfs,
    resolver: &Resolver,
    model: &CachedModel,
    name: &str,
) -> Option<ModelAnimation> {
    let loaded = mint_sage_model::load_animation(vfs, resolver, name).ok()?;
    let declared = DefaultAnimation {
        name: name.to_owned(),
        mode: Some(AnimationMode::Manual),
        blend_frames: None,
        flags: AnimationFlags::default(),
        source: AnimationSource::IdleAnimationState,
    };
    ModelAnimation::bind(model, &declared, loaded).ok()
}

fn part_boxes(instances: &[Instance]) -> Vec<(Aabb, bool)> {
    instances
        .iter()
        .filter(|i| i.visible)
        .filter_map(|i| {
            i.bounds.map(|local| {
                (
                    transformed_bounds(&local, i.transform),
                    i.blend == Blend::Opaque,
                )
            })
        })
        .collect()
}

fn frame_camera(
    boxes: &[(Aabb, bool)],
    aspect: f32,
    framing: Framing,
) -> Option<(OrbitCamera, f32)> {
    let mut bounds = Aabb::EMPTY;
    for (b, _) in boxes {
        bounds.union(b);
    }
    if bounds.is_empty() {
        return None;
    }
    let mut orbit = OrbitCamera::new(Vec3::Z);
    orbit.fov_y = UI_FOV_Y;
    orbit.frame(&bounds, aspect);
    if framing == Framing::Contain {
        return Some((orbit, 1.0));
    }
    let view_projection = orbit.camera().view_projection(aspect);
    let any_opaque = boxes.iter().any(|(_, opaque)| *opaque);
    let mut best: Option<(f32, f32)> = None;
    for (b, _) in boxes.iter().filter(|(_, opaque)| *opaque || !any_opaque) {
        let (lo, hi) = (b.min(), b.max());
        let mut ndc = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for corner in 0..8u8 {
            let p = Vec3::new(
                if corner & 1 == 0 { lo.x } else { hi.x },
                if corner & 2 == 0 { lo.y } else { hi.y },
                if corner & 4 == 0 { lo.z } else { hi.z },
            );
            let q = view_projection.transform_point(p);
            ndc = [
                ndc[0].min(q.x),
                ndc[1].min(q.y),
                ndc[2].max(q.x),
                ndc[3].max(q.y),
            ];
        }
        let area = (ndc[2] - ndc[0]) * (ndc[3] - ndc[1]);
        let window = (-ndc[0]).min(-ndc[1]).min(ndc[2]).min(ndc[3]);
        if area.is_finite() && window.is_finite() && best.is_none_or(|(a, _)| area > a) {
            best = Some((area, window));
        }
    }
    let crop = best.map_or(1.0, |(_, window)| window.clamp(0.05, 1.0));
    Some((orbit, crop))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_surface_plays_its_animation_as_its_mode_says() {
        let hold = Playback::parse("MANUAL", 1.0);
        assert_eq!(hold.advance(30.0, 0.5, 30.0, 51.0), 1.0);
        let show = Playback::parse("PLAY_TO_FRAME", 50.0);
        assert_eq!(show.advance(0.0, 1.0, 30.0, 51.0), 30.0);
        assert_eq!(show.advance(30.0, 1.0, 30.0, 51.0), 50.0, "stops there");
        let hide = Playback::parse("play_to_frame", 1.0);
        assert_eq!(hide.advance(50.0, 1.0, 30.0, 51.0), 20.0, "backwards");
        let lp = Playback::parse("LOOP", 0.0);
        assert_eq!(lp.advance(50.0, 1.0 / 30.0, 30.0, 51.0), 0.0, "wraps");
        assert_eq!(
            Playback::parse("?", 99.0).advance(0.0, 1.0, 30.0, 51.0),
            50.0
        );
    }

    #[test]
    fn a_render_object_nothing_shows_for_a_frame_starts_again() {
        let dt = Duration::from_millis(16);
        let mut clock = Clock {
            frame: 30.0,
            pending: Duration::ZERO,
            shown: true,
        };
        clock.tick(dt);
        assert_eq!((clock.frame, clock.pending, clock.shown), (30.0, dt, false));
        clock.shown = true;
        clock.tick(dt);
        assert_eq!(clock.frame, 30.0, "shown again: kept");
        clock.tick(dt);
        assert_eq!(clock, Clock::default(), "a frame unshown: released");
    }

    #[test]
    fn a_camera_pivot_looks_down_its_minus_z_with_a_fifty_degree_width() {
        let bone = Mat4::from_cols([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0, 0.0],
            [5.0, -100.0, 2.0, 1.0],
        ]);
        let seen = |camera: &Camera, aspect: f32, local: Vec3| {
            camera
                .view_projection(aspect)
                .transform_point(bone.transform_point(local))
        };
        for aspect in [4.0 / 3.0, 16.0 / 9.0] {
            let camera = bone_camera(bone, aspect).expect("a camera");
            let ahead = seen(&camera, aspect, Vec3::new(0.0, 0.0, -500.0));
            assert!(ahead.x.abs() < 1e-4 && ahead.y.abs() < 1e-4, "{ahead:?}");
            let edge = seen(
                &camera,
                aspect,
                Vec3::new(W3D_HALF_HFOV_TAN * 500.0, 0.0, -500.0),
            );
            assert!(
                (edge.x - 1.0).abs() < 1e-3,
                "right edge at {aspect}: {edge:?}"
            );
            let above = seen(&camera, aspect, Vec3::new(0.0, 100.0, -500.0));
            assert!(above.y > 0.0, "local +Y is up: {above:?}");
        }
        assert!(bone_camera(bone, 0.0).is_none());
    }

    #[test]
    fn cover_crops_to_the_biggest_part_and_keeps_the_camera() {
        let far = Aabb::from_corners(
            Vec3::new(-400.0, 500.0, -300.0),
            Vec3::new(400.0, 500.0, 300.0),
        );
        let near = Aabb::from_corners(Vec3::new(-50.0, 0.0, -50.0), Vec3::new(50.0, 0.0, 50.0));
        let glow = Aabb::from_corners(
            Vec3::new(-600.0, 100.0, -600.0),
            Vec3::new(600.0, 100.0, 600.0),
        );
        let parts = [(far, true), (near, true), (glow, false)];
        let (contain, whole) = frame_camera(&parts, 16.0 / 9.0, Framing::Contain).expect("framed");
        let (cover, crop) = frame_camera(&parts, 16.0 / 9.0, Framing::Cover).expect("framed");
        assert_eq!(whole, 1.0);
        assert_eq!(cover, contain, "a crop, not a dolly");
        assert!(crop > 0.3 && crop < 1.0, "{crop}");
        let vp = cover.camera().view_projection(16.0 / 9.0);
        let (lo, hi) = (far.min(), far.max());
        let mut rect = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for p in [
            Vec3::new(lo.x, lo.y, lo.z),
            Vec3::new(hi.x, lo.y, lo.z),
            Vec3::new(lo.x, lo.y, hi.z),
            Vec3::new(hi.x, lo.y, hi.z),
        ] {
            let q = vp.transform_point(p);
            rect = [
                rect[0].min(q.x),
                rect[1].min(q.y),
                rect[2].max(q.x),
                rect[3].max(q.y),
            ];
        }
        let inner = (-rect[0]).min(-rect[1]).min(rect[2]).min(rect[3]);
        assert!(
            (crop - inner).abs() < 1e-4,
            "{crop} vs {inner}: the far plane fills the crop"
        );
        assert!(frame_camera(&[], 1.0, Framing::Cover).is_none());
        assert!(part_boxes(&[]).is_empty());
    }
}
