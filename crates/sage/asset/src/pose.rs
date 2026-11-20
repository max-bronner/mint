use mint_sage_w3d::{
    Animation, Channel, ChannelData, ChannelKind, Hierarchy, Quat, TimeCodedKey, Value, Vec3,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
}

impl Transform {
    pub const IDENTITY: Self = Self {
        translation: Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        rotation: Quat {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        },
    };

    #[must_use]
    pub fn new(translation: Vec3, rotation: Quat) -> Self {
        Self {
            translation,
            rotation,
        }
    }

    #[must_use]
    pub fn compose(&self, inner: &Transform) -> Transform {
        let rotation = quat_normalize(self.rotation);
        Transform {
            translation: vec3_add(quat_rotate(rotation, inner.translation), self.translation),
            rotation: quat_mul(rotation, quat_normalize(inner.rotation)),
        }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SampledPose {
    pub world: Vec<Transform>,
    pub opacity: Vec<f32>,
    pub opacity_clamped: usize,
    pub out_of_range_channels: usize,
    pub root_channels_ignored: usize,
    pub visible: Vec<bool>,
}

impl SampledPose {
    #[must_use]
    pub fn bind(hierarchy: &Hierarchy) -> Self {
        let mut world = Vec::with_capacity(hierarchy.len());
        for (index, pivot) in hierarchy.pivots.iter().enumerate() {
            let local = Transform::new(pivot.translation, pivot.rotation);
            let parent_world = parent_transform(hierarchy, index, &world);
            world.push(match parent_world {
                Some(parent) => parent.compose(&local),
                None => local,
            });
        }
        let pivot_count = world.len();
        Self {
            world,
            opacity: vec![1.0; pivot_count],
            opacity_clamped: 0,
            out_of_range_channels: 0,
            root_channels_ignored: 0,
            visible: vec![true; pivot_count],
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct PivotDelta {
    translation: [Option<f32>; 3],
    rotation: Option<Quat>,
    opacity: Option<f32>,
    visible: Option<bool>,
}

#[must_use]
fn type15_opacity(raw: f32) -> (f32, bool) {
    if !raw.is_finite() {
        return (0.0, true);
    }
    let out_of_range = !(-1.0..=1.0).contains(&raw);
    let alpha = raw.clamp(-1.0, 1.0).max(0.0);
    (alpha, out_of_range)
}

#[must_use]
pub fn sample_pose(hierarchy: &Hierarchy, animation: &Animation, frame: f32) -> SampledPose {
    let mut buffers = PoseBuffers::default();
    sample_pose_into(hierarchy, animation, frame, &mut buffers);
    buffers.pose
}

#[derive(Debug, Clone, Default)]
pub struct PoseBuffers {
    deltas: Vec<PivotDelta>,
    from_deltas: Vec<PivotDelta>,
    pub pose: SampledPose,
}

pub fn sample_pose_into(
    hierarchy: &Hierarchy,
    animation: &Animation,
    frame: f32,
    buffers: &mut PoseBuffers,
) {
    let counts = sample_deltas(hierarchy, animation, frame, &mut buffers.deltas);
    compose_pose(hierarchy, &buffers.deltas, counts, &mut buffers.pose);
}

#[allow(clippy::too_many_arguments)]
pub fn sample_blend_into(
    hierarchy: &Hierarchy,
    from: &Animation,
    from_frame: f32,
    to: &Animation,
    to_frame: f32,
    weight: f32,
    buffers: &mut PoseBuffers,
) {
    let w = if weight.is_finite() {
        weight.clamp(0.0, 1.0)
    } else {
        1.0
    };
    sample_deltas(hierarchy, from, from_frame, &mut buffers.from_deltas);
    let counts = sample_deltas(hierarchy, to, to_frame, &mut buffers.deltas);
    for (delta, source) in buffers.deltas.iter_mut().zip(&buffers.from_deltas) {
        for (axis, start) in delta.translation.iter_mut().zip(source.translation) {
            if axis.is_none() && start.is_none() {
                continue;
            }
            let (a, b) = (start.unwrap_or(0.0), axis.unwrap_or(0.0));
            *axis = Some(a + (b - a) * w);
        }
        if delta.rotation.is_some() || source.rotation.is_some() {
            delta.rotation = Some(quat_slerp(
                source.rotation.unwrap_or(IDENTITY),
                delta.rotation.unwrap_or(IDENTITY),
                w,
            ));
        }
    }
    compose_pose(hierarchy, &buffers.deltas, counts, &mut buffers.pose);
}

const IDENTITY: Quat = Quat {
    x: 0.0,
    y: 0.0,
    z: 0.0,
    w: 1.0,
};

type ChannelCounts = (usize, usize, usize);

fn sample_deltas(
    hierarchy: &Hierarchy,
    animation: &Animation,
    frame: f32,
    deltas: &mut Vec<PivotDelta>,
) -> ChannelCounts {
    let pivot_count = hierarchy.len();
    deltas.clear();
    deltas.resize(pivot_count, PivotDelta::default());
    let mut out_of_range_channels = 0usize;
    let mut root_channels_ignored = 0usize;
    let mut opacity_clamped = 0usize;

    for channel in &animation.channels {
        let Ok(pivot) = usize::try_from(channel.pivot) else {
            out_of_range_channels = out_of_range_channels.saturating_add(1);
            continue;
        };
        if pivot >= pivot_count {
            out_of_range_channels = out_of_range_channels.saturating_add(1);
            continue;
        }
        if pivot == 0 {
            root_channels_ignored = root_channels_ignored.saturating_add(1);
            continue;
        }
        apply_channel(channel, frame, deltas, pivot, &mut opacity_clamped);
    }
    (
        out_of_range_channels,
        root_channels_ignored,
        opacity_clamped,
    )
}

fn compose_pose(
    hierarchy: &Hierarchy,
    deltas: &[PivotDelta],
    (out_of_range_channels, root_channels_ignored, opacity_clamped): ChannelCounts,
    pose: &mut SampledPose,
) {
    let pivot_count = hierarchy.len();
    let world = &mut pose.world;
    let opacity = &mut pose.opacity;
    let visible = &mut pose.visible;
    world.clear();
    opacity.clear();
    visible.clear();
    world.reserve(pivot_count);
    opacity.reserve(pivot_count);
    visible.reserve(pivot_count);
    for (index, pivot) in hierarchy.pivots.iter().enumerate() {
        let bind = Transform::new(pivot.translation, pivot.rotation);
        let delta = deltas.get(index).copied().unwrap_or_default();
        let animated = Transform::new(
            Vec3 {
                x: delta.translation[0].unwrap_or(0.0),
                y: delta.translation[1].unwrap_or(0.0),
                z: delta.translation[2].unwrap_or(0.0),
            },
            delta.rotation.unwrap_or(IDENTITY),
        );
        let local = bind.compose(&animated);
        let parent_world = parent_transform(hierarchy, index, world);
        world.push(match parent_world {
            Some(parent) => parent.compose(&local),
            None => local,
        });
        opacity.push(delta.opacity.unwrap_or(1.0));
        visible.push(delta.visible.unwrap_or(true));
    }

    pose.opacity_clamped = opacity_clamped;
    pose.out_of_range_channels = out_of_range_channels;
    pose.root_channels_ignored = root_channels_ignored;
}

#[must_use]
pub fn pivot_reach(hierarchy: &Hierarchy, animation: &Animation) -> Vec<f32> {
    let pivot_count = hierarchy.len();
    let mut delta = vec![[0.0f32; 3]; pivot_count];
    for channel in &animation.channels {
        let axis = match channel.kind {
            ChannelKind::TranslationX => 0,
            ChannelKind::TranslationY => 1,
            ChannelKind::TranslationZ => 2,
            _ => continue,
        };
        let Ok(pivot) = usize::try_from(channel.pivot) else {
            continue;
        };
        if pivot == 0 {
            continue;
        }
        let largest = largest_scalar(&channel.data);
        if let Some(slot) = delta.get_mut(pivot).and_then(|d| d.get_mut(axis)) {
            *slot = slot.max(largest);
        }
    }
    let mut reach: Vec<f32> = Vec::with_capacity(pivot_count);
    for (index, pivot) in hierarchy.pivots.iter().enumerate() {
        let t = pivot.translation;
        let bind = (t.x * t.x + t.y * t.y + t.z * t.z).sqrt();
        let [dx, dy, dz] = delta.get(index).copied().unwrap_or([0.0; 3]);
        let moved = (dx * dx + dy * dy + dz * dz).sqrt();
        let parent = pivot
            .parent
            .and_then(|p| usize::try_from(p).ok())
            .filter(|p| *p < index)
            .and_then(|p| reach.get(p).copied())
            .unwrap_or(0.0);
        reach.push(parent + bind + moved);
    }
    reach
}

fn largest_scalar(data: &ChannelData) -> f32 {
    let scalar = |value: &Value| match value {
        Value::Scalar(v) if v.is_finite() => v.abs(),
        _ => 0.0,
    };
    match data {
        ChannelData::Dense { values, .. } | ChannelData::AdaptiveDelta { values } => {
            values.iter().map(scalar).fold(0.0, f32::max)
        }
        ChannelData::TimeCoded(keys) => keys.iter().map(|k| scalar(&k.value)).fold(0.0, f32::max),
        ChannelData::Bits { .. } | ChannelData::SparseBits { .. } => 0.0,
    }
}

fn apply_channel(
    channel: &Channel,
    frame: f32,
    deltas: &mut [PivotDelta],
    pivot: usize,
    opacity_clamped: &mut usize,
) {
    match channel.kind {
        ChannelKind::TranslationX | ChannelKind::TranslationY | ChannelKind::TranslationZ => {
            let axis = match channel.kind {
                ChannelKind::TranslationX => 0,
                ChannelKind::TranslationY => 1,
                _ => 2,
            };
            if let Some(Value::Scalar(value)) = sample_channel(channel, frame) {
                if let Some(slot) = deltas
                    .get_mut(pivot)
                    .and_then(|d| d.translation.get_mut(axis))
                {
                    *slot = Some(value);
                }
            }
        }
        ChannelKind::Rotation => {
            if let Some(Value::Rotation(value)) = sample_channel(channel, frame) {
                if let Some(slot) = deltas.get_mut(pivot) {
                    slot.rotation = Some(value);
                }
            }
        }
        ChannelKind::Unestablished => {
            if let Some(Value::Scalar(raw)) = sample_channel(channel, frame) {
                let (alpha, out_of_range) = type15_opacity(raw);
                if out_of_range {
                    *opacity_clamped = opacity_clamped.saturating_add(1);
                }
                if let Some(slot) = deltas.get_mut(pivot) {
                    slot.opacity = Some(alpha);
                }
            }
        }
        ChannelKind::Visibility => {
            if let Some(value) = sample_visibility(channel, frame) {
                if let Some(slot) = deltas.get_mut(pivot) {
                    slot.visible = Some(value);
                }
            }
        }
        ChannelKind::Other(_) => {}
    }
}

fn parent_transform(hierarchy: &Hierarchy, index: usize, world: &[Transform]) -> Option<Transform> {
    let pivot = hierarchy.pivot(index)?;
    let parent = usize::try_from(pivot.parent?).ok()?;
    if parent >= index {
        return None;
    }
    world.get(parent).copied()
}

fn sample_channel(channel: &Channel, frame: f32) -> Option<Value> {
    match &channel.data {
        ChannelData::Dense {
            first_frame,
            values,
        } => sample_dense(channel.kind, *first_frame, values, frame),
        ChannelData::AdaptiveDelta { values } => sample_dense_clamped(values, frame),
        ChannelData::TimeCoded(keys) => sample_time_coded(keys, frame),
        ChannelData::Bits { .. } | ChannelData::SparseBits { .. } => None,
    }
}

fn sample_visibility(channel: &Channel, frame: f32) -> Option<bool> {
    match &channel.data {
        ChannelData::Bits {
            first_frame,
            last_frame,
            default,
            packed,
        } => Some(sample_bits(
            *first_frame,
            *last_frame,
            *default,
            packed,
            frame,
        )),
        ChannelData::SparseBits { default, keys } => {
            Some(sample_sparse_bits(*default, keys, frame))
        }
        ChannelData::Dense { .. }
        | ChannelData::AdaptiveDelta { .. }
        | ChannelData::TimeCoded(_) => None,
    }
}

fn floor_frame_index(frame: f32) -> Option<u32> {
    if !frame.is_finite() {
        return None;
    }
    let floored = frame.floor();
    if floored < 0.0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(floored as u32)
}

fn sample_bits(
    first_frame: u32,
    last_frame: u32,
    default: bool,
    packed: &[u8],
    frame: f32,
) -> bool {
    let Some(index) = floor_frame_index(frame) else {
        return default;
    };
    if index < first_frame || index > last_frame {
        return default;
    }
    let Some(local) = index.checked_sub(first_frame) else {
        return default;
    };
    let byte = usize::try_from(local / 8).unwrap_or(usize::MAX);
    let bit = local % 8;
    packed
        .get(byte)
        .map(|b| (b >> bit) & 1 != 0)
        .unwrap_or(default)
}

fn sample_sparse_bits(default: bool, keys: &[(u32, bool)], frame: f32) -> bool {
    let Some(index) = floor_frame_index(frame) else {
        return default;
    };
    let split = keys.partition_point(|&(key_frame, _)| key_frame <= index);
    let Some(latest) = split.checked_sub(1) else {
        return default;
    };
    keys.get(latest).map_or(default, |&(_, value)| value)
}

fn dense_identity(kind: ChannelKind, sample: Value) -> Value {
    match sample {
        Value::Scalar(_) if kind == ChannelKind::Unestablished => Value::Scalar(1.0),
        Value::Scalar(_) => Value::Scalar(0.0),
        Value::Rotation(_) => Value::Rotation(Quat {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        }),
    }
}

fn sample_dense(
    kind: ChannelKind,
    first_frame: u32,
    values: &[Value],
    frame: f32,
) -> Option<Value> {
    let last_index = values.len().checked_sub(1)?;
    #[allow(clippy::cast_precision_loss)]
    let first = first_frame as f32;
    let local = frame - first;
    if local < 0.0 {
        return Some(dense_identity(kind, *values.first()?));
    }
    #[allow(clippy::cast_precision_loss)]
    let last = last_index as f32;
    if local > last {
        return Some(dense_identity(kind, *values.first()?));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let nearest = local.round_ties_even() as usize;
    let index = if nearest > last_index { 0 } else { nearest };
    values.get(index).copied()
}

fn sample_dense_clamped(values: &[Value], frame: f32) -> Option<Value> {
    let last_index = values.len().checked_sub(1)?;
    if frame <= 0.0 {
        return values.first().copied();
    }
    #[allow(clippy::cast_precision_loss)]
    let last = last_index as f32;
    if frame >= last {
        return values.get(last_index).copied();
    }
    interpolate_within(values, last_index, frame)
}

fn interpolate_within(values: &[Value], last_index: usize, local: f32) -> Option<Value> {
    let floor = local.floor();
    let t = local - floor;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let lo = (floor as usize).min(last_index);
    let hi = lo.saturating_add(1).min(last_index);
    let a = values.get(lo).copied()?;
    let b = values.get(hi).copied()?;
    Some(interpolate(a, b, t))
}

fn sample_time_coded(keys: &[TimeCodedKey], frame: f32) -> Option<Value> {
    if keys.is_empty() {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let split = keys.partition_point(|k| (k.frame as f32) <= frame);
    if split == 0 {
        return keys.first().map(|k| k.value);
    }
    let Some(later) = keys.get(split) else {
        return keys.last().map(|k| k.value);
    };
    let earlier = keys.get(split.checked_sub(1)?)?;

    if later.hold {
        return Some(earlier.value);
    }

    #[allow(clippy::cast_precision_loss)]
    let (earlier_frame, later_frame) = (earlier.frame as f32, later.frame as f32);
    let span = later_frame - earlier_frame;
    let t = if span > 0.0 {
        (frame - earlier_frame) / span
    } else {
        0.0
    };
    Some(interpolate(earlier.value, later.value, t))
}

fn interpolate(a: Value, b: Value, t: f32) -> Value {
    match (a, b) {
        (Value::Scalar(a), Value::Scalar(b)) => Value::Scalar(mint_math::f32::lerp(a, b, t)),
        (Value::Rotation(a), Value::Rotation(b)) => Value::Rotation(quat_slerp(a, b, t)),
        (a, _) => a,
    }
}

fn vec3_add(a: Vec3, b: Vec3) -> Vec3 {
    Vec3 {
        x: a.x + b.x,
        y: a.y + b.y,
        z: a.z + b.z,
    }
}

fn vec3_scale(v: Vec3, s: f32) -> Vec3 {
    Vec3 {
        x: v.x * s,
        y: v.y * s,
        z: v.z * s,
    }
}

fn vec3_cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3 {
        x: a.y * b.z - a.z * b.y,
        y: a.z * b.x - a.x * b.z,
        z: a.x * b.y - a.y * b.x,
    }
}

fn quat_dot(a: Quat, b: Quat) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w
}

fn quat_normalize(q: Quat) -> Quat {
    let length = quat_dot(q, q).sqrt();
    if length <= 0.0 || !length.is_finite() {
        return Quat {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        };
    }
    Quat {
        x: q.x / length,
        y: q.y / length,
        z: q.z / length,
        w: q.w / length,
    }
}

fn quat_negate(q: Quat) -> Quat {
    Quat {
        x: -q.x,
        y: -q.y,
        z: -q.z,
        w: -q.w,
    }
}

fn quat_scale(q: Quat, s: f32) -> Quat {
    Quat {
        x: q.x * s,
        y: q.y * s,
        z: q.z * s,
        w: q.w * s,
    }
}

fn quat_add(a: Quat, b: Quat) -> Quat {
    Quat {
        x: a.x + b.x,
        y: a.y + b.y,
        z: a.z + b.z,
        w: a.w + b.w,
    }
}

fn quat_mul(a: Quat, b: Quat) -> Quat {
    Quat {
        w: a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z,
        x: a.w * b.x + a.x * b.w + a.y * b.z - a.z * b.y,
        y: a.w * b.y - a.x * b.z + a.y * b.w + a.z * b.x,
        z: a.w * b.z + a.x * b.y - a.y * b.x + a.z * b.w,
    }
}

fn quat_rotate(q: Quat, v: Vec3) -> Vec3 {
    let q = quat_normalize(q);
    let axis = Vec3 {
        x: q.x,
        y: q.y,
        z: q.z,
    };
    let uv = vec3_cross(axis, v);
    let uuv = vec3_cross(axis, uv);
    vec3_add(v, vec3_add(vec3_scale(uv, 2.0 * q.w), vec3_scale(uuv, 2.0)))
}

fn quat_slerp(a: Quat, b: Quat, t: f32) -> Quat {
    let a = quat_normalize(a);
    let mut b = quat_normalize(b);
    let mut dot = quat_dot(a, b);
    if dot < 0.0 {
        b = quat_negate(b);
        dot = -dot;
    }
    let dot = mint_math::f32::clamp(dot, -1.0, 1.0);
    if dot > 0.9995 {
        return quat_normalize(quat_add(quat_scale(a, 1.0 - t), quat_scale(b, t)));
    }
    let theta_0 = mint_math::f32::acos(dot);
    let theta = theta_0 * t;
    let (sin_theta, cos_theta) = mint_math::f32::sin_cos(theta);
    let sin_theta_0 = mint_math::f32::sin(theta_0);
    let s0 = cos_theta - dot * sin_theta / sin_theta_0;
    let s1 = sin_theta / sin_theta_0;
    quat_add(quat_scale(a, s0), quat_scale(b, s1))
}

#[cfg(test)]
mod tests;
