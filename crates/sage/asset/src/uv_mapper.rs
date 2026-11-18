use std::f32::consts::TAU;

use mint_sage_w3d::{MapperArgs, UvMapperType, VertexMaterialInfo};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UvTransform {
    pub linear: [[f32; 2]; 2],
    pub offset: [f32; 2],
}

impl UvTransform {
    pub const IDENTITY: Self = Self {
        linear: [[1.0, 0.0], [0.0, 1.0]],
        offset: [0.0, 0.0],
    };

    #[must_use]
    pub fn apply(&self, uv: [f32; 2]) -> [f32; 2] {
        let [[a, b], [c, d]] = self.linear;
        let [ox, oy] = self.offset;
        [a * uv[0] + b * uv[1] + ox, c * uv[0] + d * uv[1] + oy]
    }

    #[must_use]
    pub fn then(&self, next: Self) -> Self {
        let [[a1, b1], [c1, d1]] = self.linear;
        let [[a2, b2], [c2, d2]] = next.linear;
        let linear = [
            [a2 * a1 + b2 * c1, a2 * b1 + b2 * d1],
            [c2 * a1 + d2 * c1, c2 * b1 + d2 * d1],
        ];
        let [ox1, oy1] = self.offset;
        let [ox2, oy2] = next.offset;
        let offset = [a2 * ox1 + b2 * oy1 + ox2, c2 * ox1 + d2 * oy1 + oy2];
        Self { linear, offset }
    }
}

#[must_use]
pub fn sample_uv_transform(mapper: UvMapperType, args: &MapperArgs, seconds: f32) -> UvTransform {
    match mapper {
        UvMapperType::LinearOffset | UvMapperType::BumpEnv => linear_offset(args, seconds),
        UvMapperType::Rotate => rotate(args, seconds),
        UvMapperType::SineLinearOffset => sine_linear_offset(args, seconds),
        UvMapperType::Grid => grid(args, seconds),
        _ => UvTransform::IDENTITY,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialUvTransforms {
    pub stage0: UvTransform,
    pub stage1: UvTransform,
    pub bump: [[f32; 2]; 2],
}

#[must_use]
pub fn sample_material_uv_transforms(
    info: &VertexMaterialInfo,
    args0: &MapperArgs,
    args1: &MapperArgs,
    seconds: f32,
) -> MaterialUvTransforms {
    MaterialUvTransforms {
        stage0: sample_uv_transform(info.stage0_mapper(), args0, seconds),
        stage1: sample_uv_transform(info.stage1_mapper(), args1, seconds),
        bump: bump_matrix(info.stage0_mapper(), args0, seconds),
    }
}

#[must_use]
fn bump_matrix(mapper: UvMapperType, args: &MapperArgs, seconds: f32) -> [[f32; 2]; 2] {
    if mapper != UvMapperType::BumpEnv {
        return UvTransform::IDENTITY.linear;
    }
    let angle = mint_math::f32::wrap_tau(TAU * args.bump_rotation * seconds);
    let (sin, cos) = mint_math::f32::sin_cos(angle);
    let (c, s) = (args.bump_scale * cos, args.bump_scale * sin);
    [[c, -s], [s, c]]
}

fn linear_offset(args: &MapperArgs, seconds: f32) -> UvTransform {
    UvTransform {
        linear: [[args.u_scale, 0.0], [0.0, args.v_scale]],
        offset: [
            (-(args.u_per_sec * seconds)).rem_euclid(1.0),
            (-(args.v_per_sec * seconds)).rem_euclid(1.0),
        ],
    }
}

const MAX_GRID_LOG2_WIDTH: u32 = 8;

fn grid(args: &MapperArgs, seconds: f32) -> UvTransform {
    let log2 = args.log2_width.min(MAX_GRID_LOG2_WIDTH);
    let width = 1u32 << log2;
    let cells = width.saturating_mul(width);
    let last = if args.last == 0 { cells } else { args.last };
    if last == 0 {
        return UvTransform::IDENTITY;
    }
    let start = args.offset.checked_rem(last).unwrap_or(0);
    let origin = if args.fps < 0.0 {
        last.saturating_sub(1).saturating_sub(start)
    } else {
        start
    };
    let frame = grid_frame(args.fps, seconds, origin, last);

    let mask = width.saturating_sub(1);
    let col = frame & mask;
    let row = frame.checked_shr(log2).unwrap_or(0) & mask;
    #[allow(clippy::cast_precision_loss)]
    let (width, col, row) = (width as f32, col as f32, row as f32);
    let cell = 1.0 / width;
    UvTransform {
        linear: UvTransform::IDENTITY.linear,
        offset: [col * cell, row * cell],
    }
}

fn grid_frame(fps: f32, seconds: f32, origin: u32, last: u32) -> u32 {
    if fps == 0.0 || !fps.is_finite() || !seconds.is_finite() || last == 0 {
        return origin;
    }
    let period = 1000.0 / fps.abs();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let period = if period >= 1.0 { period as u32 } else { 1 };
    #[allow(clippy::cast_possible_truncation)]
    let elapsed_ms = (seconds * 1000.0) as i64;
    let steps = elapsed_ms
        .checked_div_euclid(i64::from(period))
        .unwrap_or(0);
    let advance = if fps < 0.0 {
        steps.checked_neg().unwrap_or(0)
    } else {
        steps
    };
    let frame = i64::from(origin)
        .checked_add(advance)
        .unwrap_or_else(|| i64::from(origin))
        .rem_euclid(i64::from(last));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frame = frame as u32;
    frame
}

fn rotate(args: &MapperArgs, seconds: f32) -> UvTransform {
    let angle = mint_math::f32::wrap_tau(-(TAU * args.speed * seconds));
    let (sin, cos) = mint_math::f32::sin_cos(angle);
    let linear = [
        [args.u_scale * cos, -args.u_scale * sin],
        [args.v_scale * sin, args.v_scale * cos],
    ];
    let center = [args.u_center, args.v_center];
    let m_center = [
        linear[0][0] * center[0] + linear[0][1] * center[1],
        linear[1][0] * center[0] + linear[1][1] * center[1],
    ];
    UvTransform {
        linear,
        offset: [center[0] - m_center[0], center[1] - m_center[1]],
    }
}

fn sine_linear_offset(args: &MapperArgs, seconds: f32) -> UvTransform {
    let u_phase = mint_math::f32::wrap_tau(TAU * args.u_freq * seconds + args.u_phase);
    let v_phase = mint_math::f32::wrap_tau(TAU * args.v_freq * seconds + args.v_phase);
    UvTransform {
        linear: UvTransform::IDENTITY.linear,
        offset: [
            args.u_amp * mint_math::f32::sin(u_phase),
            args.v_amp * mint_math::f32::sin(v_phase),
        ],
    }
}

#[cfg(test)]
mod tests;
