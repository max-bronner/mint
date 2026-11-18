use std::sync::Arc;

use mint_render::{Aabb, Frustum, Mat4, Vec3};
use mint_sage_ini::{AnimationClip, AnimationMode, AnimationStateKind};
use mint_sage_model::{Completion, ModelAnimation, PartPose};

use crate::anim_script;
use crate::animation_state::{phase_offset, pick_clip, rate_scale, speed_factor, Conditions};

use super::blend::{blend_seconds, Blend, Blending, Slot};
use super::fx::{crossed, queue_fx_events, MAX_FX_EVENTS_QUEUED};
use super::{
    is_drawable_for, model_part_overrides, LookKey, Scratch, UnitDrawable, UnitPose, UnitTemplate,
};

#[cfg(test)]
thread_local! {
    pub(crate) static STAND_IN_RECOMPUTES: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

const MAX_TRANSITION_CHAIN: u32 = 8;

pub(super) const MOVING_DWELL: f32 = 0.25;

pub(super) const SPEED_EASING: f32 = 0.15;

pub(super) fn pair_drawables(
    drawables: &[UnitDrawable],
    poses: &[UnitPose],
    out: &mut Vec<Option<usize>>,
) {
    out.clear();
    out.resize(drawables.len(), None);
    let mut next = 0usize;
    for (index, pose) in poses.iter().enumerate() {
        let Some(drawable) = drawables.get(next) else {
            break;
        };
        if !is_drawable_for(drawable.id, drawable.generation, drawable.template, pose) {
            continue;
        }
        if let Some(slot) = out.get_mut(next) {
            *slot = Some(index);
        }
        next = next.saturating_add(1);
    }
}

const PARALLEL_POSE_MIN: usize = 512;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct PoseCounts {
    pub(super) paired: usize,
    pub(super) sampled: usize,
    pub(super) out_of_view: usize,
    pub(super) reused: usize,
    pub(super) cpu_ns: u128,
    pub(super) worst_ns: u128,
    pub(super) sample_ns: u128,
    pub(super) entries: usize,
    pub(super) threads: usize,
}

impl PoseCounts {
    fn add(self, other: Self) -> Self {
        Self {
            paired: self.paired.saturating_add(other.paired),
            sampled: self.sampled.saturating_add(other.sampled),
            out_of_view: self.out_of_view.saturating_add(other.out_of_view),
            reused: self.reused.saturating_add(other.reused),
            cpu_ns: self.cpu_ns.saturating_add(other.cpu_ns),
            worst_ns: self.worst_ns.max(other.worst_ns),
            sample_ns: self.sample_ns.saturating_add(other.sample_ns),
            entries: self.entries.saturating_add(other.entries),
            threads: self.threads.max(other.threads),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Posed {
    Sampled,
    Held,
    Reused,
    OutOfView,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn pose_all(
    drawables: &mut [UnitDrawable],
    paired: &[Option<usize>],
    poses: &[UnitPose],
    templates: &[UnitTemplate],
    now: f32,
    threads: usize,
    views: &[Frustum],
    blending: Blending,
    transitions_enabled: bool,
    profiling: bool,
) -> (PoseCounts, crate::anim_script::Refusals) {
    let run = |chunk: &mut [UnitDrawable], pairs: &[Option<usize>]| {
        let mut counts = PoseCounts::default();
        let mut scratch = Scratch {
            profiling,
            ..Scratch::default()
        };
        for (drawable, pair) in chunk.iter_mut().zip(pairs) {
            drawable.drawn = false;
            drawable.present = false;
            let (Some(pose), Some(art)) = (
                pair.and_then(|at| poses.get(at)),
                templates.get(drawable.template),
            ) else {
                continue;
            };
            counts.paired = counts.paired.saturating_add(1);
            let started = profiling.then(std::time::Instant::now);
            let posed = pose_drawable(
                drawable,
                art,
                pose,
                now,
                views,
                blending,
                transitions_enabled,
                &mut scratch,
            );
            if let Some(started) = started {
                let ns = started.elapsed().as_nanos();
                counts.cpu_ns = counts.cpu_ns.saturating_add(ns);
                counts.worst_ns = counts.worst_ns.max(ns);
            }
            match posed {
                Posed::Sampled => counts.sampled = counts.sampled.saturating_add(1),
                Posed::OutOfView => counts.out_of_view = counts.out_of_view.saturating_add(1),
                Posed::Reused => counts.reused = counts.reused.saturating_add(1),
                Posed::Held => {}
            }
        }
        counts.sample_ns = scratch.sample_ns;
        counts.entries = scratch.entries;
        (counts, scratch.refused)
    };
    let used = PoseCounts {
        threads: chunk_threads(threads, drawables.len(), PARALLEL_POSE_MIN),
        ..PoseCounts::default()
    };
    run_chunked(drawables, paired, threads, PARALLEL_POSE_MIN, run)
        .into_iter()
        .fold(
            (used, crate::anim_script::Refusals::default()),
            |(counts, mut refused), (chunk_counts, chunk_refused)| {
                refused.add(&chunk_refused);
                (counts.add(chunk_counts), refused)
            },
        )
}

pub(super) fn chunk_threads(threads: usize, len: usize, min: usize) -> usize {
    if len < min {
        return 1;
    }
    if threads == 0 {
        std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
    } else {
        threads
    }
}

const CHUNKS_PER_THREAD: usize = 8;

pub(super) fn run_chunked<T, P, R, F>(
    items: &mut [T],
    pairs: &[P],
    threads: usize,
    min: usize,
    run: F,
) -> Vec<R>
where
    T: Send,
    P: Sync,
    R: Send,
    F: Fn(&mut [T], &[P]) -> R + Sync,
{
    let threads = chunk_threads(threads, items.len(), min);
    if threads <= 1 {
        return vec![run(items, pairs)];
    }
    let size = items
        .len()
        .div_ceil(threads.saturating_mul(CHUNKS_PER_THREAD))
        .max(1);
    let queue: std::sync::Mutex<Vec<(&mut [T], &[P])>> =
        std::sync::Mutex::new(items.chunks_mut(size).zip(pairs.chunks(size)).collect());
    let run = &run;
    let queue = &queue;
    let take = || {
        let mut results = Vec::new();
        loop {
            let next = queue.lock().ok().and_then(|mut queue| queue.pop());
            let Some((chunk, pairs)) = next else { break };
            results.push(run(chunk, pairs));
        }
        results
    };
    let take = &take;
    std::thread::scope(|scope| {
        let handles: Vec<_> = (1..threads).map(|_| scope.spawn(take)).collect();
        let mut results = take();
        for handle in handles {
            if let Ok(mut theirs) = handle.join() {
                results.append(&mut theirs);
            }
        }
        results
    })
}

#[must_use]
pub fn may_be_seen(placement: Mat4, radius: f32, views: &[Frustum]) -> bool {
    if views.is_empty() {
        return true;
    }
    let centre = placement.transform_point(Vec3::ZERO);
    let reach = radius * stretch_bound(placement) * (1.0 + REACH_MARGIN) + REACH_MARGIN;
    if !reach.is_finite() || !centre.is_finite() {
        return true;
    }
    let bounds = Aabb::from_corners(
        Vec3::new(centre.x - reach, centre.y - reach, centre.z - reach),
        Vec3::new(centre.x + reach, centre.y + reach, centre.z + reach),
    );
    views.iter().any(|view| view.intersects(&bounds))
}

const REACH_MARGIN: f32 = 1e-3;

#[must_use]
pub fn stretch_bound(m: Mat4) -> f32 {
    let mut frobenius = 0.0f32;
    let mut gershgorin = 0.0f32;
    for i in 0..3 {
        let mut row = 0.0f32;
        for j in 0..3 {
            let dot =
                m.get(0, i) * m.get(0, j) + m.get(1, i) * m.get(1, j) + m.get(2, i) * m.get(2, j);
            row += dot.abs();
            frobenius += m.get(j, i) * m.get(j, i);
        }
        gershgorin = gershgorin.max(row);
    }
    frobenius.sqrt().min(gershgorin.sqrt())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn pose_drawable(
    drawable: &mut UnitDrawable,
    art: &UnitTemplate,
    pose: &UnitPose,
    now: f32,
    views: &[Frustum],
    blending: Blending,
    transitions_enabled: bool,
    scratch: &mut Scratch,
) -> Posed {
    drawable.drawn = true;
    drawable.present = true;
    drawable.placement = pose.placement;
    drawable.house_color = pose.house_color;
    drawable.hero_colours = pose.hero_colours;
    drawable.shroud_masked = pose.shroud_masked;
    drawable.opacity = if pose.opacity.is_finite() {
        pose.opacity.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let step = (now - drawable.advanced_at).max(0.0);
    drawable.advanced_at = now;
    let conditions = if pose.look == LookKey::PLAIN {
        pose.conditions
    } else {
        art.look_conditions_of(pose.look)
            .map_or(pose.conditions, |extra| pose.conditions.union(extra))
    };
    let conditions = if conditions.contains(Conditions::MOVING) {
        drawable.moving_until = now + MOVING_DWELL;
        conditions
    } else if now < drawable.moving_until
        && !conditions.intersects(Conditions::ATTACKING.union(Conditions::DYING))
    {
        conditions.union(Conditions::MOVING)
    } else {
        conditions
    };
    let target_speed = if pose.speed.is_finite() {
        pose.speed
    } else {
        0.0
    };
    drawable.drawn_speed += (target_speed - drawable.drawn_speed) * (step / (SPEED_EASING + step));
    if !drawable.tire_pivots.is_empty() {
        drawable.tire_angle += tire_rotation_multiplier(art) * drawable.drawn_speed * step;
    }
    let model_state = art.model_machine.select(conditions);
    let chosen = art.machine.select(conditions);
    let selected = match drawable.state {
        Some(showing)
            if !drawable.mid_transition
                && chosen != Some(showing)
                && conditions.in_follow_through(art.machine.required(showing))
                && art
                    .clip(drawable.body, showing, drawable.clip)
                    .zip(drawable.phase)
                    .is_some_and(|(clip, phase)| {
                        !clip.mode.repeats() && phase < natural_seconds(clip.frames, clip.rate)
                    }) =>
        {
            Some(showing)
        }
        _ => chosen,
    };
    if conditions != drawable.last_conditions
        || drawable.companion_states.len() != art.companions.len()
    {
        drawable.companion_states = art
            .companions
            .iter()
            .map(|companion| {
                (
                    companion.machine.select(conditions),
                    companion.model_machine.select(conditions),
                )
            })
            .collect();
    }
    let previous_conditions = drawable.last_conditions;
    let conditions_changed = conditions != previous_conditions;
    drawable.last_conditions = conditions;
    let began = pose.attack_cycle != drawable.last_attack_cycle;
    drawable.last_attack_cycle = pose.attack_cycle;
    let began_here = began
        && pose.attack_slot.is_some_and(|slot| {
            selected.is_some_and(|state| art.machine.required(state).names_slot_shot(slot))
        });
    drawable.swing_queued = if selected != drawable.state {
        None
    } else if began_here {
        Some(now)
    } else {
        drawable.swing_queued
    };
    let swing_running = drawable.swing_queued.is_some()
        && !drawable.swing_repicked
        && drawable.state.is_some_and(|state| {
            art.clip(drawable.body, state, drawable.clip)
                .zip(drawable.phase)
                .is_some_and(|(clip, phase)| phase < natural_seconds(clip.frames, clip.rate))
        });
    let restart = drawable.swing_queued.filter(|_| !swing_running);
    if restart.is_some() {
        drawable.swing_queued = None;
    }
    let mut rebuilt = false;
    if model_state != drawable.model_state || selected != drawable.state {
        if model_state != drawable.model_state {
            rebuilt = art.rebuilds_model(drawable.model_state, model_state);
            if rebuilt && !art.retains_sub_objects(model_state) {
                drawable.plain_parts.clear();
            }
        }
        drawable.model_state = model_state;
        let mut wanted = art.body_for(model_state, drawable.pool_body);
        if let Some(clip_body) = selected.and_then(|state| art.clip_body(state)) {
            wanted = Some(clip_body);
        }
        drawable.body_missing = wanted.is_none();
        if let Some(body) = wanted.filter(|body| *body != drawable.body) {
            wear(drawable, art, body, pose.id);
        }
        drawable.skin =
            model_state.and_then(|state| art.skins.get(&(drawable.body, state)).cloned());
    }
    drawable.hidden = drawable.body_missing || conditions.intersects(art.hide_if);
    let look_at = (drawable.body, pose.look);
    if look_at != drawable.look_at {
        drawable.look = art.looks.get(&look_at).cloned();
        if drawable.look.is_some() || pose.look == LookKey::PLAIN {
            drawable.look_at = look_at;
        }
    }
    drawable.ghost = pose.ghost;
    drawable.casts_shadow = pose.casts_shadow;
    if drawable.hidden {
        drawable.drawn = false;
        return Posed::Held;
    }
    let entered = selected != drawable.script_state;
    if entered {
        drawable.script_state = selected;
        run_state_script_parts(art, selected, drawable, conditions, scratch);
    }
    if entered || rebuilt || drawable.body != drawable.parts_body {
        drawable.parts_body = drawable.body;
        drawable.state_parts = resolve_sub_objects(drawable);
    }
    if pose.ghost.is_some() {
        return Posed::Held;
    }
    let interrupted =
        drawable.mid_transition && conditions_changed && selected != drawable.transition_target;
    if interrupted {
        drawable.queue.clear();
    }
    let entered = (!drawable.mid_transition && (selected != drawable.state || restart.is_some()))
        || interrupted;
    if entered {
        enter(
            drawable,
            art,
            selected,
            selected,
            conditions,
            pose.id,
            now,
            scratch,
            transitions_enabled && drawable.state.is_some(),
        );
        drawable.swing_late_by = restart.map(|began_at| (now - began_at).max(0.0));
    }
    let own = drawable.state.and_then(|state| {
        art.clip(drawable.body, state, drawable.clip)
            .map(|animation| (state, drawable.clip, animation))
    });
    let source = match own {
        Some(own) => Some(own),
        None if drawable.mid_transition => None,
        None => {
            let (body, picks) = (drawable.body, drawable.picks);
            let key = (body, conditions, picks);
            if drawable.stand_in_at != Some(key) {
                let same_context =
                    drawable
                        .stand_in_at
                        .is_some_and(|(cached_body, _, cached_picks)| {
                            cached_body == body && cached_picks == picks
                        });
                let previous_choice = drawable.stand_in_choice;
                let chosen = stand_in(art, body, conditions, pose.id, picks);
                let new_choice = chosen.map(|(state, clip, _)| (state, clip));
                if conditions_changed && !entered && same_context && new_choice != previous_choice {
                    drawable.phase = None;
                    drawable.fit_cycle = None;
                    drawable.fx_frame = None;
                    drawable.blend_request = shown(drawable);
                }
                drawable.stand_in_choice = new_choice;
                drawable.stand_in_at = Some(key);
            }
            drawable.stand_in_choice.and_then(|(state, clip)| {
                art.clip(body, state, clip)
                    .map(|animation| (state, clip, animation))
            })
        }
    };
    let Some((state, clip_index, animation)) = source else {
        if let Some((state, clip, frame)) = drawable.pending.take() {
            if let Some(animation) = art.clip(drawable.body, state, clip) {
                sample_into(drawable, animation, frame, scratch);
                drawable.sampled = Some((state, clip, frame.to_bits()));
            }
        }
        return Posed::Held;
    };
    let clip = art
        .states
        .get(state)
        .and_then(|state| state.clips.get(clip_index));
    let natural = natural_seconds(animation.frames, animation.rate);
    let weapon_cycle = *drawable.fit_cycle.get_or_insert(pose.weapon_cycle);
    let scale = clip.map_or(1.0, |clip| {
        rate_scale(clip, drawable.drawn_speed, natural, pose.id, weapon_cycle)
    });
    if let Some(outgoing) = drawable.blend_request.take() {
        blend_in(
            drawable, art, outgoing, animation, clip, pose.id, now, blending,
        );
    }
    if let Some(blend) = &mut drawable.blend {
        let advance = step * scale.max(0.0);
        blend.from.advance(advance);
        if let Some(into) = &mut blend.into {
            into.advance(advance);
        }
        if blend.weight(now) >= 1.0 {
            let duration = incoming_seconds(animation, clip, pose.id, blending);
            drawable.blend = blend.into.filter(|_| duration > 0.0).map(|from| Blend {
                from,
                into: None,
                started: now,
                duration,
            });
        }
    }
    let picks_again = Some(state) == art.machine.fallback()
        || art
            .states
            .get(state)
            .is_some_and(|def| def.flags.restart_when_complete);
    let seed = if drawable.phase.is_some() {
        0.0
    } else {
        drawable
            .swing_late_by
            .take()
            .map(|late| late * scale.max(0.0))
            .filter(|phase| *phase < natural)
            .unwrap_or_else(|| {
                let random_start = art
                    .states
                    .get(state)
                    .is_some_and(|def| def.flags.random_start);
                let offset = !drawable.mid_transition
                    && (animation.mode.repeats()
                        || (random_start && !art.machine.required(state).names_a_shot()));
                phase_offset(offset, natural, pose.id, state)
            })
    };
    let elapsed = advance_phase(drawable.phase, step, scale, seed);
    drawable.phase = Some(elapsed);
    if drawable.mid_transition && natural > 0.0 && elapsed >= natural {
        advance_transition_queue(drawable, art, conditions, pose.id, now, scratch);
    }
    let frame = match (animation.mode, pose.progress) {
        (AnimationMode::Manual, Some(progress)) => manual_frame(animation.frames, progress),
        _ => animation.frame_at(Completion::Hold, elapsed),
    };
    #[allow(clippy::cast_precision_loss)]
    let last = (animation.frames.max(1) - 1) as f32;
    let complete = match animation.mode {
        AnimationMode::Once => frame >= last,
        AnimationMode::OnceBackwards => frame <= 0.0,
        _ => false,
    };
    if picks_again
        && complete
        && natural > 0.0
        && !drawable.mid_transition
        && drawable.blend.is_none()
        && drawable.swing_queued.is_none()
    {
        drawable.swing_repicked = true;
        drawable.picks = drawable.picks.wrapping_add(1);
        if own.is_some() {
            drawable.clip =
                resolve_clip(art, state, None, pose.id, drawable.picks, Some(clip_index));
        }
        drawable.phase = Some(0.0);
        drawable.fit_cycle = None;
        drawable.blend_request = Some((state, clip_index, frame));
    }
    if let Some(events) = own
        .and_then(|_| art.states.get(state))
        .map(|state| &state.fx_events)
        .filter(|events| !events.is_empty())
    {
        queue_fx_events(
            events,
            drawable.fx_frame,
            frame,
            last,
            &mut drawable.fx_pending,
            &mut drawable.bone_fx_pending,
        );
    }
    if let Some(cues) = own
        .and_then(|_| art.sound_cues.get(&(state, clip_index)))
        .filter(|cues| !cues.is_empty())
    {
        for (sound, frames) in cues {
            let hit = frames.iter().any(|&target| {
                #[allow(clippy::cast_precision_loss)]
                let target = target as f32;
                crossed(target, drawable.fx_frame, frame, last, true)
            });
            if hit && drawable.sound_pending.len() < MAX_FX_EVENTS_QUEUED {
                drawable.sound_pending.push(sound.clone());
            }
        }
    }
    if let Some(events) = own
        .and_then(|_| art.states.get(state))
        .map(|state| &state.lua_frame_events)
        .filter(|events| !events.is_empty())
    {
        #[allow(clippy::cast_precision_loss)]
        let last = (animation.frames.max(1) - 1) as f32;
        let writes: Vec<(String, bool)> = events
            .iter()
            .filter(|event| {
                #[allow(clippy::cast_precision_loss)]
                let target = event.frame as f32;
                crossed(target, drawable.fx_frame, frame, last, true)
            })
            .filter_map(|event| art.frame_event_parts.get(event.data.as_deref()?))
            .flatten()
            .cloned()
            .collect();
        if !writes.is_empty() {
            write_permanent_parts(drawable, writes);
        }
    }
    drawable.fx_frame = Some(frame);
    let radius = art
        .radius
        .get(&(drawable.body, state, clip_index))
        .copied()
        .unwrap_or(f32::INFINITY);
    if !may_be_seen(pose.placement, radius, views) {
        drawable.drawn = false;
        drawable.pending = Some((state, clip_index, frame));
        drawable.sampled = None;
        return Posed::OutOfView;
    }
    drawable.pending = None;
    if let Some(blend) = drawable.blend {
        let outgoing = art.clip(drawable.body, blend.from.state, blend.from.clip);
        let incoming: Option<(&ModelAnimation, f32)> = match blend.into {
            Some(slot) => art
                .clip(drawable.body, slot.state, slot.clip)
                .map(|incoming| (incoming.as_ref(), slot.frame(incoming))),
            None => Some((animation.as_ref(), frame)),
        };
        if let (Some(outgoing), Some((incoming, incoming_frame))) = (outgoing, incoming) {
            let started = scratch.profiling.then(std::time::Instant::now);
            incoming.pose_blend_into(
                &drawable.model,
                incoming_frame,
                outgoing,
                blend.from.frame(outgoing),
                blend.weight(now),
                &mut drawable.bones,
                &mut drawable.poses,
                &mut scratch.pose,
            );
            let tire_angle = drawable.tire_angle;
            spin_tire_bones(&mut drawable.bones, &drawable.tire_pivots, tire_angle);
            drawable.tire_angle_applied = tire_angle;
            box_from_bones(drawable);
            if let Some(started) = started {
                scratch.sample_ns = scratch
                    .sample_ns
                    .saturating_add(started.elapsed().as_nanos());
            }
            drawable.sampled = None;
            drawable.pending = Some((state, clip_index, frame));
            return Posed::Sampled;
        }
        drawable.blend = None;
    }
    let key = (state, clip_index, frame.to_bits());
    if drawable.sampled == Some(key) {
        if !drawable.tire_pivots.is_empty() {
            let delta = drawable.tire_angle - drawable.tire_angle_applied;
            spin_tire_bones(&mut drawable.bones, &drawable.tire_pivots, delta);
            drawable.tire_angle_applied = drawable.tire_angle;
            box_from_bones(drawable);
        }
        return Posed::Reused;
    }
    sample_into(drawable, animation, frame, scratch);
    drawable.sampled = Some(key);
    Posed::Sampled
}

fn wear(drawable: &mut UnitDrawable, art: &UnitTemplate, body: usize, pose_id: u32) {
    let Some(Some(body_art)) = art.art.get(body) else {
        return;
    };
    let same_skeleton = match (&drawable.model.hierarchy, &body_art.model.hierarchy) {
        (Some(worn), Some(next)) => worn.header.name.eq_ignore_ascii_case(&next.header.name),
        _ => false,
    };
    let showing = shown(drawable);
    drawable.body = body;
    drawable.art = Arc::clone(body_art);
    drawable.model = Arc::clone(&body_art.model);
    drawable.poses = vec![PartPose::default(); body_art.model.parts.len()];
    drawable.bones = if body_art.gpu.any() {
        body_art.gpu.bind.clone()
    } else {
        Vec::new()
    };
    drawable.tire_pivots = super::tire_pivots_for(art.truck_tires.as_ref(), &body_art.model);
    drawable.tire_angle_applied = 0.0;
    drawable.sampled = None;
    if same_skeleton {
        drawable.pending = showing;
        return;
    }
    drawable.pending = None;
    drawable.blend = None;
    drawable.blend_request = None;
    if let Some(state) = drawable.state {
        drawable.picks = drawable.picks.wrapping_add(1);
        drawable.clip = resolve_clip(art, state, None, pose_id, drawable.picks, None);
        drawable.phase = None;
        drawable.fit_cycle = None;
        drawable.fx_frame = None;
    }
}

#[must_use]
pub fn manual_frame(frames: u32, progress: f32) -> f32 {
    let progress = if progress.is_finite() {
        progress.clamp(0.0, 1.0)
    } else {
        0.0
    };
    #[allow(clippy::cast_precision_loss)]
    let last = frames.saturating_sub(1) as f32;
    progress * last
}

fn sample_into(
    drawable: &mut UnitDrawable,
    animation: &ModelAnimation,
    frame: f32,
    scratch: &mut Scratch,
) {
    let started = scratch.profiling.then(std::time::Instant::now);
    if drawable.bones.len() != animation.bone_count() {
        drawable
            .bones
            .resize(animation.bone_count(), Mat4::IDENTITY);
    }
    animation.pose_into(
        &drawable.model,
        frame,
        &mut drawable.bones,
        &mut drawable.poses,
        &mut scratch.pose,
    );
    let tire_angle = drawable.tire_angle;
    spin_tire_bones(&mut drawable.bones, &drawable.tire_pivots, tire_angle);
    drawable.tire_angle_applied = tire_angle;
    box_from_bones(drawable);
    if let Some(started) = started {
        scratch.sample_ns = scratch
            .sample_ns
            .saturating_add(started.elapsed().as_nanos());
    }
}

fn box_from_bones(drawable: &mut UnitDrawable) {
    for (index, slot) in drawable.poses.iter_mut().enumerate() {
        if let Some(bounds) = drawable.art.gpu.posed_bounds(index, &drawable.bones) {
            slot.bounds = Some(bounds);
        }
    }
}

fn tire_rotation_multiplier(art: &UnitTemplate) -> f32 {
    art.truck_tires
        .as_ref()
        .map_or(0.0, |tires| tires.rotation_multiplier)
}

fn spin_tire_bones(bones: &mut [Mat4], pivots: &[usize], angle: f32) {
    if angle == 0.0 || !angle.is_finite() {
        return;
    }
    let turn = Mat4::from_rotation_y(angle);
    for &pivot in pivots {
        if let Some(bone) = bones.get_mut(pivot) {
            *bone = bone.mul_mat4(turn);
        }
    }
}

#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn natural_seconds(frames: u32, rate: u32) -> f32 {
    if rate == 0 || frames < 2 {
        return 0.0;
    }
    frames.saturating_sub(1) as f32 / rate as f32
}

#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn carried_start_frame(out_frame: f32, out_frames: u32, in_frames: u32) -> Option<f32> {
    let last = out_frames.saturating_sub(1);
    if last == 0 {
        return None;
    }
    let fraction = out_frame / last as f32;
    (fraction >= 0.0).then(|| (in_frames as f32 * fraction).trunc())
}

#[must_use]
pub fn advance_phase(previous: Option<f32>, step: f32, scale: f32, seed: f32) -> f32 {
    match previous {
        Some(previous) => previous + step.max(0.0) * scale.max(0.0),
        None => seed,
    }
}

#[allow(clippy::too_many_arguments)]
fn enter(
    drawable: &mut UnitDrawable,
    art: &UnitTemplate,
    target: Option<usize>,
    destination: Option<usize>,
    conditions: Conditions,
    pose_id: u32,
    now: f32,
    scratch: &mut Scratch,
    run_scripts: bool,
) {
    drawable.transition_target = destination;
    scratch.entries = scratch.entries.saturating_add(1);
    let outgoing = shown(drawable);
    let mut left = art.identity_of(drawable.state);
    let mut current = target;
    let mut label: Option<String> = None;
    if run_scripts {
        let mut steps = 0u32;
        while let Some(t) = current {
            steps = steps.saturating_add(1);
            if steps > MAX_TRANSITION_CHAIN {
                break;
            }
            let run = art.script_of(t).map(|script| {
                anim_script::run(
                    script,
                    &left,
                    conditions,
                    &art.extras,
                    pose_id,
                    u32::try_from(t).unwrap_or(u32::MAX),
                    &mut scratch.script_env,
                    &mut scratch.refused,
                )
            });
            let Some(run) = run else { break };
            let resolved: Vec<usize> = run
                .transitions
                .iter()
                .filter_map(|name| art.transition_by_name(name))
                .collect();
            let Some((first, rest)) = resolved.split_first() else {
                label = run.label;
                break;
            };
            drawable.queue.splice(0..0, rest.iter().copied());
            left = art.identity_of(Some(t));
            current = Some(*first);
        }
    }
    commit(
        drawable,
        art,
        current,
        label.as_deref(),
        outgoing,
        pose_id,
        now,
        run_scripts,
    );
}

#[allow(clippy::too_many_arguments)]
fn commit(
    drawable: &mut UnitDrawable,
    art: &UnitTemplate,
    target: Option<usize>,
    label: Option<&str>,
    outgoing: Option<(usize, usize, f32)>,
    pose_id: u32,
    now: f32,
    fire_entering_fx: bool,
) {
    let previous = drawable.state;
    let cut = previous
        .filter(|state| target != previous && art.machine.required(*state).names_a_shot())
        .and_then(|state| art.clip(drawable.body, state, drawable.clip))
        .zip(drawable.phase)
        .filter(|(clip, _)| !clip.mode.repeats())
        .map(|(clip, phase)| phase / natural_seconds(clip.frames, clip.rate))
        .filter(|at| *at < 1.0);
    if let Some(at) = cut {
        drawable.cut_swings = drawable.cut_swings.saturating_add(1);
        drawable.last_cut = target;
        drawable.last_cut_at = at;
    }
    drawable.state = target;
    drawable.entered_at = now;
    drawable.phase = None;
    drawable.fit_cycle = None;
    drawable.swing_repicked = false;
    drawable.fx_frame = None;
    drawable.mid_transition = target.is_some_and(|state| {
        art.states
            .get(state)
            .is_some_and(|def| def.kind == AnimationStateKind::Transition)
    });
    if fire_entering_fx && target != previous {
        if let Some(list) = target
            .and_then(|state| art.states.get(state))
            .and_then(|def| def.entering_state_fx.as_deref())
        {
            if drawable.fx_pending.len() < MAX_FX_EVENTS_QUEUED {
                drawable.fx_pending.push(list.to_owned());
            }
        }
    }
    drawable.picks = drawable.picks.wrapping_add(1);
    let showing = (target == previous).then_some(drawable.clip);
    drawable.clip = target
        .map(|state| resolve_clip(art, state, label, pose_id, drawable.picks, showing))
        .unwrap_or(0);
    if let (Some(outgoing), Some(previous), Some(incoming)) = (outgoing, previous, target) {
        let body = drawable.body;
        if let Some(phase) =
            maintained_phase(art, body, outgoing, previous, incoming, drawable.clip)
        {
            drawable.phase = Some(phase);
        }
    }
    drawable.blend_request = outgoing;
}

fn maintained_phase(
    art: &UnitTemplate,
    body: usize,
    (out_state, out_clip, out_frame): (usize, usize, f32),
    previous: usize,
    incoming: usize,
    clip: usize,
) -> Option<f32> {
    let groups = |state: usize| {
        art.states
            .get(state)
            .map_or(0, |def| def.flags.maintain_frame)
    };
    if previous == incoming || groups(previous) & groups(incoming) == 0 {
        return None;
    }
    let outgoing = art.clip(body, out_state, out_clip)?;
    let animation = art.clip(body, incoming, clip)?;
    let start = carried_start_frame(out_frame, outgoing.frames, animation.frames)?;
    let frames = match animation.mode {
        AnimationMode::OnceBackwards | AnimationMode::LoopBackwards => animation.start - start,
        _ => start - animation.start,
    };
    #[allow(clippy::cast_precision_loss)]
    (animation.rate > 0).then(|| frames / animation.rate as f32)
}

fn shown(drawable: &UnitDrawable) -> Option<(usize, usize, f32)> {
    drawable
        .sampled
        .map(|(state, clip, bits)| (state, clip, f32::from_bits(bits)))
        .or(drawable.pending)
}

#[allow(clippy::too_many_arguments)]
fn blend_in(
    drawable: &mut UnitDrawable,
    art: &UnitTemplate,
    outgoing: (usize, usize, f32),
    animation: &ModelAnimation,
    clip: Option<&AnimationClip>,
    pose_id: u32,
    now: f32,
    blending: Blending,
) {
    let duration = incoming_seconds(animation, clip, pose_id, blending);
    if duration <= 0.0 {
        drawable.blend = None;
        return;
    }
    let from = match drawable.blend.filter(|blend| blend.weight(now) < 1.0) {
        None => Slot::at(outgoing),
        Some(mut blend) => {
            let (state, clip, _) = outgoing;
            let protected = blend.into.is_some()
                || art
                    .states
                    .get(state)
                    .and_then(|def| def.clips.get(clip))
                    .is_some_and(|clip| clip.must_complete_blend);
            if protected {
                blend.into.get_or_insert(Slot::at(outgoing));
                drawable.blend = Some(blend);
                return;
            }
            if blend.weight(now) > 0.5 {
                Slot::at(outgoing)
            } else {
                blend.from
            }
        }
    };
    drawable.blend = Some(Blend {
        from,
        into: None,
        started: now,
        duration,
    });
}

fn incoming_seconds(
    animation: &ModelAnimation,
    clip: Option<&AnimationClip>,
    pose_id: u32,
    blending: Blending,
) -> f32 {
    if blending == Blending::Off {
        return 0.0;
    }
    blend_seconds(
        clip.and_then(|clip| clip.blend_time),
        animation.frames,
        animation.rate,
        clip.map_or(1.0, |clip| speed_factor(clip, pose_id)),
    )
}

fn stand_in(
    art: &UnitTemplate,
    body: usize,
    conditions: Conditions,
    pose_id: u32,
    picks: u32,
) -> Option<(usize, usize, &Arc<ModelAnimation>)> {
    #[cfg(test)]
    STAND_IN_RECOMPUTES.with(|calls| calls.set(calls.get() + 1));
    let state = art
        .machine
        .select_playable(conditions, |state| art.first_bound(body, state).is_some())?;
    let picked = resolve_clip(art, state, None, pose_id, picks, None);
    let clip = if art.clip(body, state, picked).is_some() {
        picked
    } else {
        art.first_bound(body, state)?
    };
    art.clip(body, state, clip)
        .map(|animation| (state, clip, animation))
}

impl UnitTemplate {
    fn first_bound(&self, body: usize, state: usize) -> Option<usize> {
        self.bound
            .range((body, state, 0)..=(body, state, usize::MAX))
            .next()
            .map(|(&(_, _, clip), _)| clip)
    }
}

fn resolve_clip(
    art: &UnitTemplate,
    state: usize,
    label: Option<&str>,
    pose_id: u32,
    entry: u32,
    avoid: Option<usize>,
) -> usize {
    if let Some(label) = label {
        let found = art.states.get(state).and_then(|def| {
            def.clips
                .iter()
                .position(|clip| clip.label.as_deref() == Some(label))
        });
        if let Some(index) = found {
            return index;
        }
    }
    let Some(def) = art.states.get(state) else {
        return 0;
    };
    let avoid = avoid.filter(|_| !def.allow_repeat_in_random_pick);
    let priorities: Vec<Option<i32>> = def.clips.iter().map(|clip| clip.priority).collect();
    pick_clip(&priorities, pose_id, state, entry, avoid).unwrap_or(0)
}

#[allow(clippy::too_many_arguments)]
fn advance_transition_queue(
    drawable: &mut UnitDrawable,
    art: &UnitTemplate,
    conditions: Conditions,
    pose_id: u32,
    now: f32,
    scratch: &mut Scratch,
) {
    let (next, destination) = if drawable.queue.is_empty() {
        let reselected = art.machine.select(conditions);
        (reselected, reselected)
    } else {
        (Some(drawable.queue.remove(0)), drawable.transition_target)
    };
    enter(
        drawable,
        art,
        next,
        destination,
        conditions,
        pose_id,
        now,
        scratch,
        true,
    );
}

fn run_state_script_parts(
    art: &UnitTemplate,
    selected: Option<usize>,
    drawable: &mut UnitDrawable,
    conditions: Conditions,
    scratch: &mut Scratch,
) {
    let (Some(state), Some(script)) = (selected, selected.and_then(|at| art.script_of(at))) else {
        return;
    };
    let prev = art.identity_of(drawable.state);
    let calls = anim_script::run(
        script,
        &prev,
        conditions,
        &art.extras,
        drawable.id,
        u32::try_from(state).unwrap_or(u32::MAX),
        &mut scratch.script_env,
        &mut scratch.refused,
    )
    .sub_objects;
    record_sub_object_writes(
        &mut drawable.plain_parts,
        &mut drawable.permanent_parts,
        calls,
    );
}

pub(super) fn write_permanent_parts(drawable: &mut UnitDrawable, writes: Vec<(String, bool)>) {
    let calls = writes
        .into_iter()
        .map(|(name, shown)| (name, shown, true))
        .collect();
    record_sub_object_writes(
        &mut drawable.plain_parts,
        &mut drawable.permanent_parts,
        calls,
    );
    drawable.state_parts = resolve_sub_objects(drawable);
}

pub(super) fn record_sub_object_writes(
    plain: &mut Vec<(String, bool)>,
    permanent: &mut Vec<(String, bool)>,
    calls: Vec<(String, bool, bool)>,
) {
    for (pattern, shown, is_permanent) in calls {
        let list = if is_permanent {
            &mut *permanent
        } else {
            &mut *plain
        };
        match list
            .iter_mut()
            .find(|(name, _)| name.eq_ignore_ascii_case(&pattern))
        {
            Some(entry) => entry.1 = shown,
            None => list.push((pattern, shown)),
        }
    }
}

pub(super) fn sub_object_overrides(
    plain: &[(String, bool)],
    permanent: &[(String, bool)],
) -> Vec<(String, bool)> {
    plain.iter().chain(permanent).cloned().collect()
}

fn resolve_sub_objects(drawable: &UnitDrawable) -> Option<Arc<[Option<bool>]>> {
    if drawable.plain_parts.is_empty() && drawable.permanent_parts.is_empty() {
        return None;
    }
    let overrides = sub_object_overrides(&drawable.plain_parts, &drawable.permanent_parts);
    let parts: Arc<[Option<bool>]> = model_part_overrides(&overrides, &drawable.model).into();
    parts.iter().any(Option::is_some).then_some(parts)
}

#[cfg(test)]
mod tire_tests {
    use super::{spin_tire_bones, Mat4};
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn only_the_named_pivot_turns() {
        let mut bones = vec![Mat4::IDENTITY, Mat4::IDENTITY];
        spin_tire_bones(&mut bones, &[1], FRAC_PI_2);
        assert_eq!(bones[0], Mat4::IDENTITY, "an unnamed pivot must not move");
        assert_ne!(bones[1], Mat4::IDENTITY, "the named pivot must turn");
    }

    #[test]
    fn a_zero_angle_leaves_every_bone_untouched() {
        let mut bones = vec![Mat4::IDENTITY];
        spin_tire_bones(&mut bones, &[0], 0.0);
        assert_eq!(bones[0], Mat4::IDENTITY);
    }

    #[test]
    fn an_out_of_range_pivot_does_not_panic() {
        let mut bones = vec![Mat4::IDENTITY];
        spin_tire_bones(&mut bones, &[5], FRAC_PI_2);
        assert_eq!(bones[0], Mat4::IDENTITY);
    }
}
