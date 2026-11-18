use mint_app::RtsCamera;
use mint_math::f32::{atan, atan2, cos, sin, tan};
use mint_render::{Camera, Vec3};
use mint_sage_scene::camera_pose::blend_cameras;
use mint_sage_scene::camera_transform::{bfme_camera, BfmeView};

use crate::camera_limits::CameraLimits;
use crate::diag::diag;
use crate::rules::map_scripts::{CameraMove, CameraOps, ChannelMove};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ParabolicEase {
    in_: f32,
    out: f32,
}

impl ParabolicEase {
    pub(crate) fn new(ease_in: f32, ease_out: f32) -> Self {
        let out = (1.0 - ease_out).clamp(0.0, 1.0);
        let in_ = ease_in.clamp(0.0, 1.0).min(out);
        Self { in_, out }
    }

    pub(crate) fn at(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        let v0 = 1.0 + self.out - self.in_;
        if t < self.in_ {
            t * t / (v0 * self.in_)
        } else if t <= self.out {
            (self.in_ + 2.0 * (t - self.in_)) / v0
        } else {
            (self.in_
                + 2.0 * (self.out - self.in_)
                + (2.0 * (t - self.out) + self.out * self.out - t * t) / (1.0 - self.out))
                / v0
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Move {
    elapsed_ms: f32,
    total_ms: f32,
    ease: ParabolicEase,
    from: BfmeView,
    to: BfmeView,
}

const FRAME_MS: f32 = 33.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ChannelAnim {
    start: f32,
    end: f32,
    frames: f32,
    elapsed_ms: f32,
    ease: ParabolicEase,
}

impl ChannelAnim {
    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn new(start: f32, end: f32, ms: i32, ease_in_ms: i32, ease_out_ms: i32) -> Self {
        let ms = ms.max(1);
        let frames = (ms / 33).max(1) as f32;
        Self {
            start,
            end,
            frames,
            elapsed_ms: if ms == 1 { frames * FRAME_MS } else { 0.0 },
            ease: ParabolicEase::new(
                ease_in_ms as f32 / ms as f32,
                ease_out_ms as f32 / ms as f32,
            ),
        }
    }

    pub(crate) fn advance(&mut self, dt_ms: f32) {
        self.elapsed_ms += dt_ms.max(0.0);
    }

    pub(crate) fn done(&self) -> bool {
        self.elapsed_ms >= self.frames * FRAME_MS
    }

    pub(crate) fn at_frame(&self, frame: f32) -> f32 {
        if frame >= self.frames {
            return self.end;
        }
        self.start + (self.end - self.start) * self.ease.at(frame / self.frames)
    }

    pub(crate) fn value(&self) -> f32 {
        self.at_frame(self.elapsed_ms / FRAME_MS)
    }
}

#[derive(Debug, Default)]
struct Channel {
    seen: Option<u32>,
    anim: Option<ChannelAnim>,
}

impl Channel {
    fn start(
        &mut self,
        command: Option<&ChannelMove>,
        current: f32,
        end: impl Fn(f32, f32) -> f32,
    ) {
        let Some(command) = command.filter(|c| self.seen != Some(c.seq)) else {
            return;
        };
        self.seen = Some(command.seq);
        self.anim = Some(ChannelAnim::new(
            current,
            end(current, command.target),
            command.ms,
            command.ease_in_ms,
            command.ease_out_ms,
        ));
    }

    fn step(&mut self, dt_ms: f32) -> Option<f32> {
        let anim = self.anim.as_mut()?;
        anim.advance(dt_ms);
        let value = anim.value();
        if anim.done() {
            self.anim = None;
        }
        Some(value)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CameraInput<'a> {
    pub(crate) moves: Option<&'a CameraMove>,
    pub(crate) ops: &'a CameraOps,
    pub(crate) follow_at: Option<[f32; 3]>,
    pub(crate) look_object_at: Option<[f32; 3]>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DefaultView {
    pub(crate) max_height: f32,
    pub(crate) pitch: f32,
    pub(crate) aspect: f32,
    pub(crate) limits: CameraLimits,
}

const DEFAULT_HFOV: f32 = 0.872_664_6;

const SENSOR_HALF_WIDTH: f32 = 12.0;

const MIN_LOOK_DISTANCE: f32 = 0.1;

const CLIENT_FPS: f32 = 30.0;

const FRAME_EPSILON: f32 = 1e-4;

const FOLLOW_RAMP: f32 = 0.05;

#[derive(Debug, Clone, Copy)]
struct Look {
    frames: u32,
    hold: u32,
    cur: u32,
    ease: ParabolicEase,
    raise: f32,
    last: [f32; 3],
}

#[derive(Debug, Clone, Copy)]
struct ResetBlend {
    from: Camera,
    to: Camera,
    frames: u32,
    cur: u32,
    ease: ParabolicEase,
}

impl ResetBlend {
    fn camera(&self) -> Option<Camera> {
        if self.cur.saturating_add(1) >= self.frames {
            return None;
        }
        #[allow(clippy::cast_precision_loss)]
        let t = self.ease.at(self.cur as f32 / self.frames as f32);
        Some(blend_cameras(&self.from, &self.to, t))
    }
}

#[derive(Debug, Default)]
pub(crate) struct ScriptCamera {
    seen: Option<u32>,
    reset_seen: Option<u32>,
    follow_seen: Option<u32>,
    look_seen: Option<u32>,
    look_waypoint_seen: Option<u32>,
    look: Option<Look>,
    rotate_pin: Option<[f32; 2]>,
    focal: Channel,
    active: Option<Move>,
    view: Option<BfmeView>,
    zoom: Channel,
    pitch: Channel,
    rotate: Channel,
    following: bool,
    follow_factor: f32,
    frame_clock: f32,
    blend: Option<ResetBlend>,
}

impl ScriptCamera {
    pub(crate) fn busy(&self) -> bool {
        self.active.is_some()
            || self.zoom.anim.is_some()
            || self.pitch.anim.is_some()
            || self.rotate.anim.is_some()
            || self.focal.anim.is_some()
            || self.look.is_some()
            || self.blend.is_some()
    }

    pub(crate) fn blending(&self) -> bool {
        self.blend.is_some()
    }

    pub(crate) fn follows(&self) -> bool {
        self.following
    }

    pub(crate) fn end_blend(&mut self) {
        self.blend = None;
    }

    fn cancel_moves(&mut self) {
        self.active = None;
        self.blend = None;
        self.zoom.anim = None;
        self.pitch.anim = None;
        self.rotate.anim = None;
        self.focal.anim = None;
        self.look = None;
        self.rotate_pin = None;
        self.following = false;
        self.follow_factor = -1.0;
    }

    fn client_frames(&mut self, dt: f32) -> u32 {
        self.frame_clock += dt.max(0.0) * CLIENT_FPS + FRAME_EPSILON;
        let whole = self.frame_clock.floor();
        self.frame_clock -= whole;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frames = whole.min(CLIENT_FPS) as u32;
        frames
    }

    pub(crate) fn update(
        &mut self,
        input: &CameraInput<'_>,
        camera: &mut RtsCamera,
        defaults: DefaultView,
        ground_at: impl Fn(f32, f32) -> f32,
        dt: f32,
    ) -> bool {
        if !self.busy() && !self.following && !camera.is_scripted() {
            self.view = None;
        }
        let mut started = false;
        let dt_ms = dt.max(0.0) * 1000.0;
        if let Some(command) = input.moves.filter(|c| self.seen != Some(c.seq)) {
            self.seen = Some(command.seq);
            self.active = Some(begin(command, self.base(camera, defaults), &ground_at));
            started = true;
        }
        let frames = self.client_frames(dt);
        if let Some(blend) = self.blend.as_mut() {
            blend.cur = blend.cur.saturating_add(frames);
        }
        if let Some(command) = input.ops.reset.filter(|c| self.reset_seen != Some(c.seq)) {
            self.reset_seen = Some(command.seq);
            let from = self.base(camera, defaults);
            let drawn = camera.camera();
            self.cancel_moves();
            let ground = ground_at(command.target.x, command.target.y);
            let (offset, angle) = defaults.limits.reset_offset();
            let to = BfmeView {
                look_at: [command.target.x, command.target.y],
                ground,
                zoom: if offset.z > 0.0 {
                    (defaults.limits.max_height() + ground) / offset.z
                } else {
                    1.0
                },
                yaw: angle,
                fx_pitch: 1.0,
                roll: 0.0,
                hfov: DEFAULT_HFOV,
                offset,
                ..from
            };
            self.view = Some(to);
            diag!(
                "script: camera reset seq={} ms={} height={:.0} ground={:.1} zoom={:.3} yaw={:.3}",
                command.seq,
                command.ms,
                defaults.limits.max_height(),
                ground,
                to.zoom,
                angle
            );
            if command.ms > 1 {
                let ms = command.ms as f32;
                self.blend = Some(ResetBlend {
                    from: drawn,
                    to: bfme_camera(&to, defaults.aspect, drawn.near, drawn.far),
                    frames: u32::try_from(command.ms / 33).unwrap_or(0).max(1),
                    cur: 0,
                    ease: ParabolicEase::new(
                        command.ease_in_ms as f32 / ms,
                        command.ease_out_ms as f32 / ms,
                    ),
                });
            }
            started = true;
        }
        self.start_channels(input, camera, defaults);
        if let Some(follow) = input.ops.follow.filter(|f| self.follow_seen != Some(f.seq)) {
            self.follow_seen = Some(follow.seq);
            diag!(
                "script: camera follow seq={} {} snap={}",
                follow.seq,
                if follow.entity.is_some() {
                    "on"
                } else {
                    "stop"
                },
                follow.snap
            );
            if follow.entity.is_none() {
                self.cancel_moves();
            } else {
                self.following = true;
                if let (true, Some([x, y, z])) = (follow.snap, input.follow_at) {
                    let base = self.base(camera, defaults);
                    self.view = Some(BfmeView {
                        look_at: [x, y],
                        ground: z,
                        ..base
                    });
                }
            }
        }
        if let Some(active) = self.active.as_mut() {
            if let Some(wp) = input.moves.and_then(|m| m.look_toward) {
                active.to.yaw = yaw_towards(active.to.look_at, [wp.x, wp.y]);
            }
            active.elapsed_ms += dt_ms;
            let t = active.elapsed_ms / active.total_ms;
            self.view = Some(active.from.lerp(&active.to, active.ease.at(t)));
            if t >= 1.0 {
                self.active = None;
            }
        }
        let was_looking = self.look.is_some();
        self.step_channels(dt_ms, defaults.aspect);
        self.step_look(frames, input.look_object_at, was_looking);
        if self.following {
            self.blend = None;
            match input.follow_at {
                None => self.cancel_moves(),
                Some([x, y, z]) => {
                    if self.view.is_none() {
                        self.view = Some(self.base(camera, defaults));
                    }
                    for _ in 0..frames {
                        self.follow_factor = if self.follow_factor < 0.0 {
                            FOLLOW_RAMP
                        } else {
                            (self.follow_factor + FOLLOW_RAMP).min(1.0)
                        };
                        if let Some(view) = self.view.as_mut() {
                            view.look_at = [
                                view.look_at[0] + (x - view.look_at[0]) * self.follow_factor,
                                view.look_at[1] + (y - view.look_at[1]) * self.follow_factor,
                            ];
                        }
                    }
                    if let Some(view) = self.view.as_mut() {
                        view.ground = z;
                    }
                }
            }
        }
        if let Some(view) = self.view {
            let aim = self.look.map(|look| {
                [
                    look.last[0],
                    look.last[1],
                    look.last[2] + view.height_adjust,
                ]
            });
            apply(&view, camera, defaults, aim);
            if let Some(blended) = self.blend.as_ref().and_then(ResetBlend::camera) {
                if self.blend.is_some_and(|b| b.cur % 10 == 0) {
                    diag!(
                        "script: camera reset blend frame {} eye {:.1},{:.1},{:.1}",
                        self.blend.map_or(0, |b| b.cur),
                        blended.eye.x,
                        blended.eye.y,
                        blended.eye.z
                    );
                }
                camera.set_scripted(Some(blended));
            } else {
                self.blend = None;
            }
        }
        started
    }

    fn base(&self, camera: &RtsCamera, defaults: DefaultView) -> BfmeView {
        self.view
            .filter(|_| camera.is_scripted())
            .unwrap_or_else(|| current_view(camera, defaults))
    }

    fn start_channels(
        &mut self,
        input: &CameraInput<'_>,
        camera: &RtsCamera,
        defaults: DefaultView,
    ) {
        let ops = input.ops;
        let base = self.base(camera, defaults);
        self.zoom.start(ops.zoom.as_ref(), base.zoom, |_, end| end);
        self.pitch
            .start(ops.pitch.as_ref(), base.fx_pitch, |_, end| end);
        self.focal.start(ops.focal.as_ref(), base.hfov, |_, focal| {
            if focal > 0.0 {
                2.0 * atan(SENSOR_HALF_WIDTH / focal)
            } else {
                core::f32::consts::FRAC_PI_2
            }
        });
        if let Some(focal) = ops.focal.filter(|f| self.focal.seen != Some(f.seq)) {
            diag!(
                "script: camera focal length {} mm over {} ms",
                focal.target,
                focal.ms
            );
        }
        let rotating = self.rotate.seen;
        self.rotate
            .start(ops.rotate.as_ref(), base.yaw, |yaw, turns| {
                yaw + turns * core::f32::consts::TAU
            });
        if self.rotate.seen != rotating {
            self.look = None;
            self.rotate_pin = ops.rotate.filter(|r| r.in_place).map(|_| {
                let eye = bfme_camera(&base, defaults.aspect, 1.0, 1000.0).eye;
                [eye.x, eye.y]
            });
        }
        if let Some(command) = ops
            .look_waypoint
            .filter(|c| self.look_waypoint_seen != Some(c.seq))
        {
            self.look_waypoint_seen = Some(command.seq);
            self.look = None;
            self.rotate_pin = None;
            let to = [command.target.x, command.target.y];
            let (dx, dy) = (to[0] - base.look_at[0], to[1] - base.look_at[1]);
            if dx * dx + dy * dy >= MIN_LOOK_DISTANCE * MIN_LOOK_DISTANCE {
                let mut end = wrap_angle(yaw_towards(base.look_at, to));
                if command.reverse {
                    end += if end > base.yaw {
                        -core::f32::consts::TAU
                    } else {
                        core::f32::consts::TAU
                    };
                }
                diag!(
                    "script: camera look at waypoint seq={} yaw {:.3} -> {:.3}",
                    command.seq,
                    base.yaw,
                    end
                );
                self.rotate.anim = Some(ChannelAnim::new(
                    base.yaw,
                    end,
                    command.ms,
                    command.ease_in_ms,
                    command.ease_out_ms,
                ));
            }
        }
        if let Some(command) = ops.look_object.filter(|c| self.look_seen != Some(c.seq)) {
            self.look_seen = Some(command.seq);
            self.rotate.anim = None;
            self.rotate_pin = None;
            let ms = command.ms.max(1);
            #[allow(clippy::cast_precision_loss)]
            let ease = ParabolicEase::new(
                command.ease_in_ms as f32 / ms as f32,
                command.ease_out_ms as f32 / ms as f32,
            );
            diag!(
                "script: camera look at unit seq={} ms={} hold={} raise={}",
                command.seq,
                command.ms,
                command.hold_ms,
                command.raise
            );
            self.look = Some(Look {
                frames: u32::try_from(ms / 33).unwrap_or(0).max(1),
                hold: u32::try_from(command.hold_ms / 33).unwrap_or(0),
                cur: 0,
                ease,
                raise: command.raise,
                last: input.look_object_at.unwrap_or([
                    base.look_at[0],
                    base.look_at[1],
                    base.ground,
                ]),
            });
        }
        if self.view.is_none() && self.busy() {
            self.view = Some(base);
        }
    }

    fn step_look(&mut self, frames: u32, target: Option<[f32; 3]>, was_looking: bool) {
        let Some(view) = self.view.as_mut() else {
            return;
        };
        if !was_looking {
            for _ in 0..frames {
                view.height_adjust = decay_raise(view.height_adjust);
            }
        }
        for _ in 0..frames {
            let Some(look) = self.look.as_mut() else {
                break;
            };
            look.cur = look.cur.saturating_add(1);
            let point = target.unwrap_or(look.last);
            look.last = point;
            #[allow(clippy::cast_precision_loss)]
            {
                view.height_adjust = look.cur as f32 / look.frames as f32 * look.raise;
            }
            let (dx, dy) = (point[0] - view.look_at[0], point[1] - view.look_at[1]);
            if dx * dx + dy * dy >= MIN_LOOK_DISTANCE * MIN_LOOK_DISTANCE {
                let facing = wrap_angle(yaw_towards(view.look_at, [point[0], point[1]]));
                view.yaw = if look.cur <= look.frames {
                    #[allow(clippy::cast_precision_loss)]
                    let progress = look.ease.at(look.cur as f32 / look.frames as f32);
                    wrap_angle(view.yaw + wrap_angle(facing - view.yaw) * progress)
                } else {
                    facing
                };
            }
            if look.cur >= look.frames.saturating_add(look.hold) {
                self.look = None;
            }
        }
    }

    fn step_channels(&mut self, dt_ms: f32, aspect: f32) {
        let Some(view) = self.view.as_mut() else {
            return;
        };
        if let Some(hfov) = self.focal.step(dt_ms) {
            view.hfov = hfov;
        }
        if let Some(zoom) = self.zoom.step(dt_ms) {
            view.zoom = zoom;
        }
        if let Some(pitch) = self.pitch.step(dt_ms) {
            view.fx_pitch = pitch;
        }
        if let Some(yaw) = self.rotate.step(dt_ms) {
            if let Some(eye) = self.rotate_pin {
                let held = bfme_camera(view, aspect, 1.0, 1000.0).eye;
                let (rx, ry) = (held.x - view.look_at[0], held.y - view.look_at[1]);
                let reach = (rx * rx + ry * ry).sqrt();
                view.look_at = [eye[0] - reach * sin(yaw), eye[1] + reach * cos(yaw)];
            }
            view.yaw = yaw;
        }
        if self.rotate.anim.is_none() {
            self.rotate_pin = None;
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct FreezeGate {
    seen: u32,
    armed: bool,
}

impl FreezeGate {
    pub(crate) fn update(&mut self, seq: u32, camera_busy: bool) {
        if seq != self.seen {
            self.seen = seq;
            self.armed = true;
        }
        if !camera_busy {
            self.armed = false;
        }
    }

    pub(crate) fn holds_simulation(&self) -> bool {
        self.armed
    }
}

fn yaw_towards(from: [f32; 2], to: [f32; 2]) -> f32 {
    atan2(-(to[0] - from[0]), to[1] - from[1])
}

fn offset(defaults: DefaultView) -> mint_render::Vec3 {
    BfmeView::default_offset(defaults.max_height, defaults.pitch)
}

fn current_view(camera: &RtsCamera, defaults: DefaultView) -> BfmeView {
    let offset = offset(defaults);
    let focus = camera.focus();
    let base = camera.camera();
    let zoom = if offset.z > 0.0 {
        base.eye.z / offset.z
    } else {
        1.0
    };
    BfmeView {
        look_at: [focus.x, focus.y],
        ground: focus.z,
        zoom,
        yaw: camera.yaw(),
        fx_pitch: tan(camera.pitch()) / tan(defaults.pitch),
        roll: 0.0,
        hfov: 2.0 * atan(tan(base.fov_y * 0.5) * defaults.aspect),
        offset,
        height_adjust: 0.0,
    }
}

fn apply(view: &BfmeView, camera: &mut RtsCamera, defaults: DefaultView, aim: Option<[f32; 3]>) {
    let base = camera.camera();
    let mut scripted = bfme_camera(view, defaults.aspect, base.near, base.far);
    let focus = Vec3::new(view.look_at[0], view.look_at[1], view.ground);
    let (dx, dy, dz) = (
        scripted.eye.x - focus.x,
        scripted.eye.y - focus.y,
        scripted.eye.z - focus.z,
    );
    camera.place(
        focus,
        view.yaw,
        atan2(dz, (dx * dx + dy * dy).sqrt()),
        (dx * dx + dy * dy + dz * dz).sqrt(),
    );
    camera.set_fov_y(scripted.fov_y);
    if let Some([x, y, z]) = aim {
        scripted.target = Vec3::new(x, y, z);
    }
    camera.set_scripted(Some(scripted));
}

fn wrap_angle(angle: f32) -> f32 {
    use core::f32::consts::{PI, TAU};
    angle - TAU * ((angle + PI) / TAU).floor()
}

fn decay_raise(raise: f32) -> f32 {
    if raise.abs() < 3.0 {
        0.0
    } else {
        raise - raise.signum() * (raise.abs() * 0.1).max(3.0)
    }
}

fn begin(command: &CameraMove, from: BfmeView, ground_at: &impl Fn(f32, f32) -> f32) -> Move {
    let (from, to) = match &command.camera {
        None => (
            from,
            BfmeView {
                look_at: [command.target.x, command.target.y],
                ground: ground_at(command.target.x, command.target.y),
                ..from
            },
        ),
        Some(record) => {
            let hfov = if record.fov > 0.0 && record.fov < core::f32::consts::PI {
                record.fov
            } else {
                from.hfov
            };
            let keep_positive = |new: f32, old: f32| if new > 0.0 { new } else { old };
            (
                BfmeView { hfov, ..from },
                BfmeView {
                    look_at: [record.look_at[0], record.look_at[1]],
                    ground: record.ground,
                    zoom: keep_positive(record.zoom, from.zoom),
                    yaw: record.yaw,
                    fx_pitch: record.pitch,
                    roll: record.roll,
                    hfov,
                    ..from
                },
            )
        }
    };
    let to = BfmeView {
        zoom: command.zoom.unwrap_or(to.zoom),
        fx_pitch: command.pitch.unwrap_or(to.fx_pitch),
        yaw: command
            .look_toward
            .map_or(to.yaw, |wp| yaw_towards(to.look_at, [wp.x, wp.y])),
        ..to
    };
    new_move(
        from,
        to,
        command.ms,
        command.ease_in_ms,
        command.ease_out_ms,
    )
}

#[allow(clippy::cast_precision_loss)]
fn new_move(from: BfmeView, to: BfmeView, ms: i32, ease_in_ms: i32, ease_out_ms: i32) -> Move {
    let (total_ms, ease_in, ease_out) = (ms.max(1) as f32, ease_in_ms as f32, ease_out_ms as f32);
    Move {
        elapsed_ms: 0.0,
        total_ms,
        ease: ParabolicEase::new(ease_in / total_ms, ease_out / total_ms),
        from,
        to,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::map_scripts::{Follow, LookObject, LookWaypoint, ResetMove};

    #[test]
    fn the_ease_covers_the_whole_distance_and_is_slow_at_both_ends() {
        let ease = ParabolicEase::new(0.25, 0.25);
        assert_eq!(ease.at(0.0), 0.0);
        assert!((ease.at(1.0) - 1.0).abs() < 1e-5);
        assert!((ease.at(0.5) - 0.5).abs() < 1e-5);
        assert!(ease.at(0.1) < 0.1 && ease.at(0.9) > 0.9);
    }

    #[test]
    fn a_waypoint_move_keeps_yaw_pitch_and_distance_and_lands_on_the_target() {
        let defaults = DefaultView {
            max_height: 300.0,
            pitch: 0.9,
            aspect: 1.5,
            limits: CameraLimits::resolve(
                crate::camera_limits::MapCameraKeys::default(),
                crate::game_data::CameraDefaults::SHIPPED,
            ),
        };
        let mut camera = RtsCamera::new(Vec3::Z);
        let (yaw, pitch, distance) = (camera.yaw(), camera.pitch(), camera.distance());
        let command = CameraMove {
            seq: 1,
            target: mint_movement::Vec2::new(100.0, 40.0),
            camera: None,
            ms: 1000,
            ease_in_ms: 0,
            ease_out_ms: 0,
            zoom: None,
            pitch: None,
            look_toward: None,
        };
        let ops = CameraOps::default();
        let input = CameraInput {
            moves: Some(&command),
            ops: &ops,
            follow_at: None,
            look_object_at: None,
        };
        let mut script = ScriptCamera::default();
        assert!(script.update(&input, &mut camera, defaults, |_, _| 0.0, 0.5));
        assert!(camera.focus().x > 0.0 && camera.focus().x < 100.0);
        assert!(!script.update(&input, &mut camera, defaults, |_, _| 0.0, 0.6));
        let at = camera.focus();
        assert!((at.x - 100.0).abs() < 1e-3 && (at.y - 40.0).abs() < 1e-3);
        assert!((camera.yaw() - yaw).abs() < 1e-3);
        assert!((camera.pitch() - pitch).abs() < 1e-3);
        assert!((camera.distance() - distance).abs() < 0.1);
        assert!(camera.is_scripted());
    }

    #[test]
    fn the_view_taken_from_the_orbit_camera_puts_the_eye_where_the_orbit_has_it() {
        let defaults = DefaultView {
            max_height: 300.0,
            pitch: 0.9,
            aspect: 1.5,
            limits: CameraLimits::resolve(
                crate::camera_limits::MapCameraKeys::default(),
                crate::game_data::CameraDefaults::SHIPPED,
            ),
        };
        let mut camera = RtsCamera::new(Vec3::Z);
        camera.place(Vec3::new(50.0, 20.0, 0.0), 0.7, 0.9, 200.0);
        let orbit = camera.eye();
        let focus = camera.focus();
        let built = bfme_camera(&current_view(&camera, defaults), 1.5, 1.0, 1000.0).eye;
        let (ox, oy) = (orbit.x - focus.x, orbit.y - focus.y);
        let (bx, by) = (built.x - focus.x, built.y - focus.y);
        let (ol, bl) = ((ox * ox + oy * oy).sqrt(), (bx * bx + by * by).sqrt());
        assert!((ox / ol - bx / bl).abs() < 1e-3 && (oy / ol - by / bl).abs() < 1e-3);
    }

    #[test]
    fn a_named_camera_keeps_its_record_values_and_a_negative_pitch_looks_up() {
        let record = mint_sage_map::NamedCamera {
            name: "shot".to_owned(),
            look_at: [100.0, 40.0, 0.0],
            pitch: -0.25,
            roll: 0.0,
            yaw: 0.0,
            zoom: 0.5,
            fov: 0.0,
            ground: 5.0,
        };
        let command = CameraMove {
            seq: 1,
            target: mint_movement::Vec2::new(100.0, 40.0),
            camera: Some(record),
            ms: 1,
            ease_in_ms: 0,
            ease_out_ms: 0,
            zoom: None,
            pitch: None,
            look_toward: None,
        };
        let from = BfmeView {
            look_at: [0.0, 0.0],
            ground: 0.0,
            zoom: 1.0,
            yaw: 1.0,
            fx_pitch: 1.0,
            roll: 0.0,
            hfov: 0.8,
            offset: BfmeView::default_offset(300.0, 0.9),
            height_adjust: 0.0,
        };
        let to = begin(&command, from, &|_, _| 0.0).to;
        assert_eq!((to.yaw, to.fx_pitch), (0.0, -0.25));
        let camera = bfme_camera(&to, 1.5, 1.0, 1000.0);
        assert!(camera.eye.y < 40.0 && (camera.eye.x - 100.0).abs() < 1e-3);
        assert!(camera.target.z > camera.eye.z, "{camera:?}");
    }

    #[test]
    fn a_channel_move_is_on_the_ease_curve_and_ends_exactly_on_its_target() {
        let anim = ChannelAnim::new(1.0, 3.0, 1000, 250, 250);
        let ease = ParabolicEase::new(0.25, 0.25);
        assert!((anim.at_frame(15.0) - (1.0 + 2.0 * ease.at(0.5))).abs() < 1e-5);
        assert!((anim.at_frame(7.0) - (1.0 + 2.0 * ease.at(7.0 / 30.0))).abs() < 1e-5);
        assert_eq!(anim.at_frame(30.0), 3.0);
        let mut stepped = anim;
        for _ in 0..29 {
            stepped.advance(FRAME_MS);
        }
        assert!(!stepped.done());
        stepped.advance(FRAME_MS);
        assert!(stepped.done() && stepped.value() == 3.0);
    }

    #[test]
    fn a_rotate_command_turns_the_yaw_by_whole_turns() {
        let defaults = DefaultView {
            max_height: 300.0,
            pitch: 0.9,
            aspect: 1.5,
            limits: CameraLimits::resolve(
                crate::camera_limits::MapCameraKeys::default(),
                crate::game_data::CameraDefaults::SHIPPED,
            ),
        };
        let mut camera = RtsCamera::new(Vec3::Z);
        let yaw = camera.yaw();
        let ops = CameraOps {
            rotate: Some(ChannelMove {
                seq: 1,
                target: 0.25,
                ms: 330,
                ease_in_ms: 0,
                ease_out_ms: 0,
                in_place: false,
            }),
            ..CameraOps::default()
        };
        let input = CameraInput {
            moves: None,
            ops: &ops,
            follow_at: None,
            look_object_at: None,
        };
        let mut script = ScriptCamera::default();
        for _ in 0..12 {
            script.update(&input, &mut camera, defaults, |_, _| 0.0, 0.033);
        }
        assert!(!script.busy());
        let turned = camera.yaw() - yaw;
        assert!(
            (turned - core::f32::consts::FRAC_PI_2).abs() < 1e-2,
            "{turned}"
        );
    }

    fn test_defaults() -> DefaultView {
        DefaultView {
            max_height: 300.0,
            pitch: 0.9,
            aspect: 1.5,
            limits: CameraLimits::resolve(
                crate::camera_limits::MapCameraKeys::default(),
                crate::game_data::CameraDefaults::SHIPPED,
            ),
        }
    }

    const STEP: f32 = 1.0 / 30.0;

    #[test]
    fn a_reset_draws_the_straight_line_between_the_two_eyes_and_ends_on_the_orbit() {
        let defaults = test_defaults();
        let mut camera = RtsCamera::new(Vec3::Z);
        camera.place(Vec3::new(50.0, 20.0, 0.0), 0.7, 0.9, 200.0);
        let from = camera.camera();
        let ops = CameraOps {
            reset: Some(ResetMove {
                seq: 1,
                target: mint_movement::Vec2::new(400.0, 300.0),
                ms: 1000,
                ease_in_ms: 0,
                ease_out_ms: 0,
            }),
            ..CameraOps::default()
        };
        let input = CameraInput {
            moves: None,
            ops: &ops,
            follow_at: None,
            look_object_at: None,
        };
        let mut script = ScriptCamera::default();
        script.update(&input, &mut camera, defaults, |_, _| 0.0, STEP);
        assert_eq!(camera.camera().eye, from.eye);
        let to = bfme_camera(
            &script.view.expect("view"),
            defaults.aspect,
            from.near,
            from.far,
        );
        assert!((camera.camera().fov_y - to.fov_y).abs() < 1e-5);
        for _ in 0..14 {
            script.update(&input, &mut camera, defaults, |_, _| 0.0, STEP);
        }
        let t = ParabolicEase::new(0.0, 0.0).at(14.0 / 30.0);
        let want = from.eye + (to.eye - from.eye) * t;
        let got = camera.camera().eye;
        assert!((got - want).length() < 0.05, "{got:?} {want:?}");
        for _ in 0..15 {
            script.update(&input, &mut camera, defaults, |_, _| 0.0, STEP);
        }
        assert!(!script.busy());
        assert!((camera.camera().eye - to.eye).length() < 0.05);
    }

    #[test]
    fn a_followed_unit_is_chased_by_a_ramping_fraction_of_the_gap_at_the_units_own_height() {
        let defaults = test_defaults();
        let mut camera = RtsCamera::new(Vec3::Z);
        let ops = CameraOps {
            follow: Some(Follow {
                seq: 1,
                entity: Some(mint_sim::Entity::from_parts(1, 0)),
                snap: true,
                height: 0.0,
            }),
            ..CameraOps::default()
        };
        let mut script = ScriptCamera::default();
        let input = |at: [f32; 3]| CameraInput {
            moves: None,
            ops: &ops,
            follow_at: Some(at),
            look_object_at: None,
        };
        script.update(
            &input([0.0, 0.0, 0.0]),
            &mut camera,
            defaults,
            |_, _| 0.0,
            STEP,
        );
        let gap = |camera: &RtsCamera| 100.0 - camera.focus().x;
        let mut gaps = Vec::new();
        for _ in 0..3 {
            script.update(
                &input([100.0, 0.0, 50.0]),
                &mut camera,
                defaults,
                |_, _| 0.0,
                STEP,
            );
            gaps.push(gap(&camera));
        }
        assert!((gaps[0] - 90.0).abs() < 0.05, "{gaps:?}");
        assert!((gaps[1] - 76.5).abs() < 0.05, "{gaps:?}");
        assert!((gaps[2] - 61.2).abs() < 0.05, "{gaps:?}");
        assert!((camera.focus().z - 50.0).abs() < 1e-3);
    }

    #[test]
    fn stopping_the_follow_ends_a_zoom_in_flight() {
        let defaults = test_defaults();
        let mut camera = RtsCamera::new(Vec3::Z);
        let mut ops = CameraOps {
            zoom: Some(ChannelMove {
                seq: 1,
                target: 2.0,
                ms: 2000,
                ease_in_ms: 0,
                ease_out_ms: 0,
                in_place: false,
            }),
            ..CameraOps::default()
        };
        let mut script = ScriptCamera::default();
        let run = |ops: &CameraOps, script: &mut ScriptCamera, camera: &mut RtsCamera| {
            let input = CameraInput {
                moves: None,
                ops,
                follow_at: None,
                look_object_at: None,
            };
            script.update(&input, camera, defaults, |_, _| 0.0, STEP);
        };
        run(&ops, &mut script, &mut camera);
        assert!(script.busy());
        ops.follow = Some(Follow {
            seq: 1,
            entity: None,
            snap: false,
            height: 0.0,
        });
        run(&ops, &mut script, &mut camera);
        assert!(!script.busy());
    }

    fn drive(
        ops: &CameraOps,
        camera: &mut RtsCamera,
        frames: u32,
        look_object_at: Option<[f32; 3]>,
    ) {
        let mut script = ScriptCamera::default();
        for _ in 0..frames {
            let input = CameraInput {
                moves: None,
                ops,
                follow_at: None,
                look_object_at,
            };
            script.update(&input, camera, test_defaults(), |_, _| 0.0, STEP);
        }
    }

    #[test]
    fn a_look_toward_a_unit_with_no_time_faces_it_in_one_frame() {
        let mut camera = RtsCamera::new(Vec3::Z);
        camera.place(Vec3::ZERO, 0.0, 0.9, 200.0);
        let ops = CameraOps {
            look_object: Some(LookObject {
                seq: 1,
                entity: mint_sim::Entity::from_parts(1, 0),
                ms: 0,
                hold_ms: 0,
                ease_in_ms: 0,
                ease_out_ms: 0,
                raise: 0.0,
            }),
            ..CameraOps::default()
        };
        drive(&ops, &mut camera, 2, Some([100.0, 0.0, 0.0]));
        assert!((camera.yaw() + core::f32::consts::FRAC_PI_2).abs() < 1e-3);
    }

    #[test]
    fn a_look_toward_a_unit_closes_the_remaining_gap_each_frame() {
        let mut camera = RtsCamera::new(Vec3::Z);
        camera.place(Vec3::ZERO, 0.0, 0.9, 200.0);
        let ops = CameraOps {
            look_object: Some(LookObject {
                seq: 1,
                entity: mint_sim::Entity::from_parts(1, 0),
                ms: 99,
                hold_ms: 0,
                ease_in_ms: 0,
                ease_out_ms: 0,
                raise: 0.0,
            }),
            ..CameraOps::default()
        };
        drive(&ops, &mut camera, 2, Some([100.0, 0.0, 0.0]));
        let want = -core::f32::consts::FRAC_PI_2 * (1.0 / 3.0 + (1.0 - 1.0 / 3.0) * 2.0 / 3.0);
        assert!(
            (camera.yaw() - want).abs() < 1e-3,
            "{} {want}",
            camera.yaw()
        );
    }

    #[test]
    fn a_reversed_look_toward_a_waypoint_turns_the_long_way_round() {
        let mut camera = RtsCamera::new(Vec3::Z);
        camera.place(Vec3::ZERO, 0.0, 0.9, 200.0);
        let ops = CameraOps {
            look_waypoint: Some(LookWaypoint {
                seq: 1,
                target: mint_movement::Vec2::new(-sin(1.0) * 50.0, cos(1.0) * 50.0),
                ms: 330,
                ease_in_ms: 0,
                ease_out_ms: 0,
                reverse: true,
            }),
            ..CameraOps::default()
        };
        drive(&ops, &mut camera, 14, None);
        assert!((camera.yaw() - (1.0 - core::f32::consts::TAU)).abs() < 1e-3);
    }

    #[test]
    fn a_focal_length_sets_the_horizontal_field_of_view_at_once() {
        let mut camera = RtsCamera::new(Vec3::Z);
        let ops = CameraOps {
            focal: Some(ChannelMove {
                seq: 1,
                target: 30.0,
                ms: 0,
                ease_in_ms: 0,
                ease_out_ms: 0,
                in_place: false,
            }),
            ..CameraOps::default()
        };
        drive(&ops, &mut camera, 2, None);
        let want = 2.0 * atan(0.4 / 1.5);
        let got = camera.camera().fov_y;
        assert!((got - want).abs() < 1e-3, "{got} {want}");
    }

    #[test]
    fn an_in_place_rotation_keeps_the_eye_where_it_was() {
        let mut camera = RtsCamera::new(Vec3::Z);
        camera.place(Vec3::new(40.0, 10.0, 0.0), 0.3, 0.9, 200.0);
        let rotate = |in_place| CameraOps {
            rotate: Some(ChannelMove {
                seq: 1,
                target: 0.25,
                ms: 330,
                ease_in_ms: 0,
                ease_out_ms: 0,
                in_place,
            }),
            ..CameraOps::default()
        };
        let mut script = ScriptCamera::default();
        let ops = rotate(true);
        let input = CameraInput {
            moves: None,
            ops: &ops,
            follow_at: None,
            look_object_at: None,
        };
        script.update(&input, &mut camera, test_defaults(), |_, _| 0.0, STEP);
        let eye = camera.camera().eye;
        for _ in 0..12 {
            script.update(&input, &mut camera, test_defaults(), |_, _| 0.0, STEP);
        }
        let moved = camera.camera().eye - eye;
        assert!(
            (moved.x * moved.x + moved.y * moved.y).sqrt() < 0.05,
            "{moved:?}"
        );
        assert!((camera.yaw() - 0.3 - core::f32::consts::FRAC_PI_2).abs() < 1e-2);
    }

    #[test]
    fn the_freeze_gate_holds_the_simulation_only_while_the_camera_moves() {
        let mut gate = FreezeGate::default();
        gate.update(0, true);
        assert!(!gate.holds_simulation(), "no firing, no hold");
        gate.update(1, true);
        assert!(gate.holds_simulation());
        gate.update(1, true);
        assert!(gate.holds_simulation());
        gate.update(1, false);
        assert!(!gate.holds_simulation());
        gate.update(1, true);
        assert!(!gate.holds_simulation(), "the same firing does not re-arm");
    }
}
