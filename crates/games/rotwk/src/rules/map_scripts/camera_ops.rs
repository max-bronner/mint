use std::sync::Arc;

use mint_movement::Vec2;
use mint_sage_map::Parameter;
use mint_sim::{Entity, StateHasher, Tick};

use super::camera::CameraMove;
use super::cinematic::{bump, seconds_to_ms, simple, Parsed, Simple};
use super::{parameter_int, parameter_real, parameter_string, MapScriptState, ParseCtx};
use crate::diag::diag;
use crate::rules::{Ids, RotwkRules};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ChannelMove {
    pub(crate) seq: u32,
    pub(crate) target: f32,
    pub(crate) ms: i32,
    pub(crate) ease_in_ms: i32,
    pub(crate) ease_out_ms: i32,
    pub(crate) in_place: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LookObject {
    pub(crate) seq: u32,
    pub(crate) entity: Entity,
    pub(crate) ms: i32,
    pub(crate) hold_ms: i32,
    pub(crate) ease_in_ms: i32,
    pub(crate) ease_out_ms: i32,
    pub(crate) raise: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LookWaypoint {
    pub(crate) seq: u32,
    pub(crate) target: Vec2,
    pub(crate) ms: i32,
    pub(crate) ease_in_ms: i32,
    pub(crate) ease_out_ms: i32,
    pub(crate) reverse: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Restriction {
    pub(crate) seq: u32,
    pub(crate) area: Option<Vec<(f32, f32)>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ResetMove {
    pub(crate) seq: u32,
    pub(crate) target: Vec2,
    pub(crate) ms: i32,
    pub(crate) ease_in_ms: i32,
    pub(crate) ease_out_ms: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Follow {
    pub(crate) seq: u32,
    pub(crate) entity: Option<Entity>,
    pub(crate) snap: bool,
    pub(crate) height: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AnimationPlay {
    pub(crate) seq: u32,
    pub(crate) name: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct CameraOps {
    pub(crate) zoom: Option<ChannelMove>,
    pub(crate) pitch: Option<ChannelMove>,
    pub(crate) rotate: Option<ChannelMove>,
    pub(crate) focal: Option<ChannelMove>,
    pub(crate) look_object: Option<LookObject>,
    pub(crate) look_waypoint: Option<LookWaypoint>,
    pub(crate) restrict: Option<Restriction>,
    pub(crate) reset: Option<ResetMove>,
    pub(crate) follow: Option<Follow>,
    pub(crate) animation: Option<AnimationPlay>,
    pub(crate) lock_all: bool,
    pub(crate) lock_angle_height: bool,
    pub(crate) freeze_seq: u32,
}

impl CameraOps {
    pub(crate) fn hash_into(&self, hasher: &mut StateHasher) {
        for channel in [&self.zoom, &self.pitch, &self.rotate] {
            hasher.write_u32(channel.map_or(0, |c| c.seq));
        }
        let later = [
            self.focal.map_or(0, |c| c.seq),
            self.look_object.map_or(0, |l| l.seq),
            self.look_waypoint.map_or(0, |l| l.seq),
            self.restrict.as_ref().map_or(0, |r| r.seq),
        ];
        for seq in later.into_iter().filter(|seq| *seq != 0) {
            hasher.write_u32(seq);
        }
        hasher.write_u32(self.reset.map_or(0, |r| r.seq));
        hasher.write_u32(self.follow.map_or(0, |f| f.seq));
        hasher.write_u32(self.animation.as_ref().map_or(0, |a| a.seq));
        hasher.write_bool(self.lock_all);
        hasher.write_bool(self.lock_angle_height);
        hasher.write_u32(self.freeze_seq);
    }
}

fn channel(
    params: &[Parameter],
    slot: fn(&mut CameraOps) -> &mut Option<ChannelMove>,
    in_place: bool,
) -> Parsed {
    let target = parameter_real(params, 0)?;
    let ms = seconds_to_ms(parameter_real(params, 1)?);
    let ease_in_ms = seconds_to_ms(parameter_real(params, 2)?);
    let ease_out_ms = seconds_to_ms(parameter_real(params, 3)?);
    simple(move |_, _, _, state| {
        let slot = slot(&mut state.camera_ops);
        *slot = Some(ChannelMove {
            seq: bump(slot.map(|c| c.seq)),
            target,
            ms,
            ease_in_ms,
            ease_out_ms,
            in_place,
        });
    })
}

pub(super) fn parse_zoom(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    channel(params, |ops| &mut ops.zoom, false)
}

pub(super) fn parse_pitch(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    channel(params, |ops| &mut ops.pitch, false)
}

pub(super) fn parse_rotate(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    channel(params, |ops| &mut ops.rotate, false)
}

pub(super) fn parse_rotate_locked(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    channel(params, |ops| &mut ops.rotate, true)
}

pub(super) fn parse_focal_length(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    channel(params, |ops| &mut ops.focal, false)
}

pub(super) fn parse_look_toward_object(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let name = parameter_string(params, 0)?;
    let ms = seconds_to_ms(parameter_real(params, 1)?);
    let hold_ms = seconds_to_ms(parameter_real(params, 2)?);
    let ease_in_ms = seconds_to_ms(parameter_real(params, 3)?);
    let ease_out_ms = seconds_to_ms(parameter_real(params, 4)?);
    let raise = parameter_real(params, 5)?;
    let lookup = name.clone();
    Some(Arc::new(Simple {
        unit: Some(name),
        run: move |rules: &RotwkRules, tick: &Tick<'_>, ids: Ids, state: &mut MapScriptState| {
            let Some(entity) = rules.object_named(tick, ids, &state.rt, &lookup, "") else {
                return;
            };
            state.camera_ops.look_object = Some(LookObject {
                seq: bump(state.camera_ops.look_object.map(|l| l.seq)),
                entity,
                ms,
                hold_ms,
                ease_in_ms,
                ease_out_ms,
                raise,
            });
        },
    }))
}

pub(super) fn parse_look_toward_waypoint(params: &[Parameter], ctx: &ParseCtx<'_>) -> Parsed {
    let name = parameter_string(params, 0)?;
    let ms = seconds_to_ms(parameter_real(params, 1)?);
    let ease_in_ms = seconds_to_ms(parameter_real(params, 2)?);
    let ease_out_ms = seconds_to_ms(parameter_real(params, 3)?);
    let reverse = parameter_int(params, 4)? != 0;
    let Some(target) = ctx.waypoints.get(&name).map(|(at, _)| *at) else {
        diag!("script: CAMERA_LOOK_TOWARD_WAYPOINT names no waypoint {name} - no turn");
        return simple(|_, _, _, _| {});
    };
    simple(move |_, _, _, state| {
        state.camera_ops.look_waypoint = Some(LookWaypoint {
            seq: bump(state.camera_ops.look_waypoint.map(|l| l.seq)),
            target,
            ms,
            ease_in_ms,
            ease_out_ms,
            reverse,
        });
    })
}

pub(super) fn parse_restrict_to_area(params: &[Parameter], ctx: &ParseCtx<'_>) -> Parsed {
    let name = parameter_string(params, 0)?;
    let Some(area) = ctx.trigger_areas.get(&name).cloned() else {
        diag!("script: CAMERA_RESTRICT_TO_AREA names no area {name} - no restriction");
        return simple(|_, _, _, _| {});
    };
    simple(move |_, _, _, state| {
        state.camera_ops.restrict = Some(Restriction {
            seq: bump(state.camera_ops.restrict.as_ref().map(|r| r.seq)),
            area: Some(area.clone()),
        });
    })
}

pub(super) fn parse_remove_area_restriction(_: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    simple(|_, _, _, state| {
        state.camera_ops.restrict = Some(Restriction {
            seq: bump(state.camera_ops.restrict.as_ref().map(|r| r.seq)),
            area: None,
        });
    })
}

pub(super) fn parse_spline_path(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let name = parameter_string(params, 0).unwrap_or_default();
    simple(move |_, _, _, _| {
        diag!("script: camera spline path {name} - the curve is not decoded, no camera move");
    })
}

pub(super) fn parse_reset(params: &[Parameter], ctx: &ParseCtx<'_>) -> Parsed {
    let name = parameter_string(params, 0)?;
    let Some(target) = ctx.waypoints.get(&name).map(|(at, _)| *at) else {
        diag!("script: RESET_CAMERA names no waypoint {name} - no reset");
        return simple(|_, _, _, _| {});
    };
    let ms = seconds_to_ms(parameter_real(params, 1)?);
    let ease_in_ms = seconds_to_ms(parameter_real(params, 2)?);
    let ease_out_ms = seconds_to_ms(parameter_real(params, 3)?);
    simple(move |_, _, _, state| {
        state.camera_ops.reset = Some(ResetMove {
            seq: bump(state.camera_ops.reset.map(|r| r.seq)),
            target,
            ms,
            ease_in_ms,
            ease_out_ms,
        });
    })
}

pub(super) fn parse_look_toward(params: &[Parameter], ctx: &ParseCtx<'_>) -> Parsed {
    let at = ctx
        .waypoints
        .get(&parameter_string(params, 0)?)
        .map(|(at, _)| *at);
    simple(move |_, _, _, state| {
        if let (Some(at), Some(command)) = (at, state.camera_move.as_mut()) {
            command.look_toward = Some(at);
        }
    })
}

pub(super) fn parse_setup(params: &[Parameter], ctx: &ParseCtx<'_>) -> Parsed {
    let target = ctx
        .waypoints
        .get(&parameter_string(params, 0)?)
        .map(|(at, _)| *at)?;
    let zoom = parameter_real(params, 1)?;
    let pitch = parameter_real(params, 2)?;
    let look = ctx
        .waypoints
        .get(&parameter_string(params, 3)?)
        .map(|(at, _)| *at);
    simple(move |_, _, _, state| {
        state.camera_move = Some(CameraMove {
            seq: bump(state.camera_move.as_ref().map(|m| m.seq)),
            target,
            camera: None,
            ms: 1,
            ease_in_ms: 0,
            ease_out_ms: 0,
            zoom: Some(zoom),
            pitch: Some(pitch),
            look_toward: look,
        });
    })
}

pub(super) fn parse_follow(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let name = parameter_string(params, 0)?;
    let snap = parameter_int(params, 1)? != 0;
    let height = parameter_real(params, 2)?;
    let lookup = name.clone();
    Some(Arc::new(Simple {
        unit: Some(name),
        run: move |rules: &RotwkRules, tick: &Tick<'_>, ids: Ids, state: &mut MapScriptState| {
            let Some(entity) = rules.object_named(tick, ids, &state.rt, &lookup, "") else {
                return;
            };
            state.camera_ops.follow = Some(Follow {
                seq: bump(state.camera_ops.follow.map(|f| f.seq)),
                entity: Some(entity),
                snap,
                height,
            });
        },
    }))
}

pub(super) fn parse_stop_follow(_: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    simple(|_, _, _, state| {
        state.camera_ops.follow = Some(Follow {
            seq: bump(state.camera_ops.follow.map(|f| f.seq)),
            entity: None,
            snap: false,
            height: 0.0,
        });
    })
}

pub(super) fn parse_lock_camera(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let locked = parameter_int(params, 0)? != 0;
    simple(move |_, _, _, state| state.camera_ops.lock_all = locked)
}

pub(super) fn parse_lock_angle_and_height(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let locked = parameter_int(params, 0)? != 0;
    simple(move |_, _, _, state| state.camera_ops.lock_angle_height = locked)
}

pub(super) fn parse_move_home(_: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    simple(|_, _, _, _| {})
}

pub(super) fn parse_by_animation(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let name = parameter_string(params, 0)?;
    simple(move |_, _, _, state| {
        state.camera_ops.animation = Some(AnimationPlay {
            seq: bump(state.camera_ops.animation.as_ref().map(|a| a.seq)),
            name: name.clone(),
        });
    })
}

pub(super) fn parse_mod_freeze_time(_: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    simple(|_, _, _, state| {
        state.camera_ops.freeze_seq = state.camera_ops.freeze_seq.wrapping_add(1);
    })
}
