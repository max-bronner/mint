use mint_math::f32::{acos, atan, cos, sin, tan};
use mint_render::{Camera, Vec3};
use mint_sage_map::{CameraAnimation, CameraTrack, FreeKey, Interpolation, Key};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraPose {
    pub matrix: [[f32; 4]; 3],
    pub lens: f32,
}

impl CameraPose {
    #[must_use]
    pub fn eye(&self) -> Vec3 {
        Vec3::new(self.matrix[0][3], self.matrix[1][3], self.matrix[2][3])
    }

    #[must_use]
    pub fn camera(&self, aspect: f32, near: f32, far: f32) -> Camera {
        let eye = self.eye();
        let back = Vec3::new(self.matrix[0][2], self.matrix[1][2], self.matrix[2][2]);
        let up = Vec3::new(self.matrix[0][1], self.matrix[1][1], self.matrix[2][1]);
        let aspect = if aspect.is_finite() && aspect > 0.0 {
            aspect
        } else {
            1.0
        };
        Camera {
            eye,
            target: Vec3::new(eye.x - back.x, eye.y - back.y, eye.z - back.z),
            up,
            fov_y: 2.0 * atan(tan(self.lens * 0.5) / aspect),
            near,
            far,
        }
    }
}

enum Segment {
    Key(usize),
    Between {
        before: usize,
        from: usize,
        to: usize,
        after: usize,
        u: f32,
    },
}

#[allow(clippy::cast_precision_loss)]
fn locate<T>(keys: &[Key<T>], t: f32) -> Option<Segment> {
    let last = keys.len().checked_sub(1)?;
    if t < 0.0 {
        return Some(Segment::Key(0));
    }
    if keys.get(last).is_some_and(|key| t >= key.frame as f32) {
        return Some(Segment::Key(last));
    }
    let whole = t.floor();
    let from = keys
        .iter()
        .rposition(|key| (key.frame as f32) <= whole)
        .unwrap_or(0);
    let to = from.checked_add(1)?;
    let (f_from, f_to) = (keys.get(from)?.frame as f32, keys.get(to)?.frame as f32);
    if f_from == f_to {
        return Some(Segment::Key(from));
    }
    Some(Segment::Between {
        before: from.saturating_sub(1),
        from,
        to,
        after: to.saturating_add(1).min(last),
        u: (t - f_from) / (f_to - f_from),
    })
}

#[allow(clippy::cast_precision_loss)]
fn blend<T: Copy, const N: usize>(
    keys: &[Key<T>],
    t: f32,
    values: impl Fn(&T) -> [f32; N],
) -> Option<[f32; N]> {
    let (from_idx, before, to, after, u) = match locate(keys, t)? {
        Segment::Key(i) => return keys.get(i).map(|key| values(&key.value)),
        Segment::Between {
            before,
            from,
            to,
            after,
            u,
        } => (from, before, to, after, u),
    };
    let from = keys.get(from_idx)?;
    let (p0, p1, p2, p3) = (
        values(&keys.get(before)?.value),
        values(&from.value),
        values(&keys.get(to)?.value),
        values(&keys.get(after)?.value),
    );
    let mut out = p1;
    match from.interpolation {
        Interpolation::Step => {}
        Interpolation::Linear => {
            for (o, (a, b)) in out.iter_mut().zip(p1.iter().zip(&p2)) {
                *o = (1.0 - u) * a + u * b;
            }
        }
        Interpolation::CatmullRom => {
            let f = |i: usize| keys.get(i).map_or(0.0, |key| key.frame as f32);
            let (f0, f1, f2, f3) = (f(before), f(from_idx), f(to), f(after));
            let span = f2 - f1;
            for (k, o) in out.iter_mut().enumerate() {
                let (Some(&a0), Some(&a1), Some(&a2), Some(&a3)) =
                    (p0.get(k), p1.get(k), p2.get(k), p3.get(k))
                else {
                    continue;
                };
                let a0 = if f1 > f0 {
                    a1 + (a0 - a1) * span / (f1 - f0)
                } else {
                    a0
                };
                let a3 = if f3 > f2 {
                    a2 + (a3 - a2) * span / (f3 - f2)
                } else {
                    a3
                };
                *o = 0.5
                    * (2.0 * a1
                        + (a2 - a0) * u
                        + (2.0 * a0 - 5.0 * a1 + 4.0 * a2 - a3) * u * u
                        + (3.0 * a1 - a0 - 3.0 * a2 + a3) * u * u * u);
            }
        }
    }
    Some(out)
}

fn slerp(a: [f32; 4], b: [f32; 4], u: f32) -> [f32; 4] {
    let mut dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let mut b = b;
    if dot < 0.0 {
        dot = -dot;
        b = [-b[0], -b[1], -b[2], -b[3]];
    }
    let (s0, s1) = if 1.0 - dot > 1e-8 {
        let omega = acos(dot.min(1.0));
        let sinom = sin(omega);
        (sin((1.0 - u) * omega) / sinom, sin(u * omega) / sinom)
    } else {
        (1.0 - u, u)
    };
    [
        s0 * a[0] + s1 * b[0],
        s0 * a[1] + s1 * b[1],
        s0 * a[2] + s1 * b[2],
        s0 * a[3] + s1 * b[3],
    ]
}

fn sample_rotation(keys: &[Key<FreeKey>], t: f32) -> Option<[f32; 4]> {
    match locate(keys, t)? {
        Segment::Key(i) => keys.get(i).map(|key| key.value.rotation),
        Segment::Between { from, to, u, .. } => {
            let a = keys.get(from)?;
            let b = keys.get(to)?;
            Some(match a.interpolation {
                Interpolation::Step => a.value.rotation,
                _ => slerp(a.value.rotation, b.value.rotation, u),
            })
        }
    }
}

fn matrix_from_quaternion(q: [f32; 4], position: [f32; 3]) -> [[f32; 4]; 3] {
    let [x, y, z, w] = q;
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - z * w),
            2.0 * (z * x + y * w),
            position[0],
        ],
        [
            2.0 * (x * y + z * w),
            1.0 - 2.0 * (z * z + x * x),
            2.0 * (y * z - x * w),
            position[1],
        ],
        [
            2.0 * (z * x - y * w),
            2.0 * (y * z + x * w),
            1.0 - 2.0 * (x * x + y * y),
            position[2],
        ],
    ]
}

fn look_at(p: [f32; 3], t: [f32; 3], roll: f32) -> [[f32; 4]; 3] {
    let (dx, dy, dz) = (t[0] - p[0], t[1] - p[1], t[2] - p[2]);
    let len1 = (dx * dx + dy * dy + dz * dz).sqrt();
    let len2 = (dx * dx + dy * dy).sqrt();
    let (sinp, cosp) = if len1 == 0.0 {
        (0.0, 1.0)
    } else {
        (dz / len1, len2 / len1)
    };
    let (siny, cosy) = if len2 == 0.0 {
        (0.0, 1.0)
    } else {
        (dy / len2, dx / len2)
    };
    let mut m = [
        [0.0, 0.0, -1.0, p[0]],
        [-1.0, 0.0, 0.0, p[1]],
        [0.0, 1.0, 0.0, p[2]],
    ];
    for row in &mut m {
        let (a, b) = (row[0], row[2]);
        row[0] = cosy * a - siny * b;
        row[2] = siny * a + cosy * b;
    }
    for row in &mut m {
        let (a, b) = (row[1], row[2]);
        row[1] = cosp * a + sinp * b;
        row[2] = -sinp * a + cosp * b;
    }
    let (s, c) = (sin(-roll), cos(-roll));
    for row in &mut m {
        let (a, b) = (row[0], row[1]);
        row[0] = c * a + s * b;
        row[1] = -s * a + c * b;
    }
    m
}

fn quaternion_from_matrix(m: &[[f32; 3]; 3]) -> [f32; 4] {
    let trace = m[0][0] + m[1][1] + m[2][2];
    if trace > 0.0 {
        let s = (trace + 1.0).sqrt();
        let k = 0.5 / s;
        return [
            (m[2][1] - m[1][2]) * k,
            (m[0][2] - m[2][0]) * k,
            (m[1][0] - m[0][1]) * k,
            s * 0.5,
        ];
    }
    if m[0][0] >= m[1][1] && m[0][0] >= m[2][2] {
        let s = (m[0][0] - m[1][1] - m[2][2] + 1.0).sqrt();
        let k = 0.5 / s;
        [
            s * 0.5,
            (m[1][0] + m[0][1]) * k,
            (m[2][0] + m[0][2]) * k,
            (m[2][1] - m[1][2]) * k,
        ]
    } else if m[1][1] >= m[2][2] {
        let s = (m[1][1] - m[2][2] - m[0][0] + 1.0).sqrt();
        let k = 0.5 / s;
        [
            (m[0][1] + m[1][0]) * k,
            s * 0.5,
            (m[2][1] + m[1][2]) * k,
            (m[0][2] - m[2][0]) * k,
        ]
    } else {
        let s = (m[2][2] - m[0][0] - m[1][1] + 1.0).sqrt();
        let k = 0.5 / s;
        [
            (m[0][2] + m[2][0]) * k,
            (m[1][2] + m[2][1]) * k,
            s * 0.5,
            (m[1][0] - m[0][1]) * k,
        ]
    }
}

fn camera_rotation(camera: &Camera) -> [[f32; 3]; 3] {
    let forward = (camera.target - camera.eye).normalize();
    let right = forward.cross(camera.up).normalize();
    let up = right.cross(forward);
    [
        [right.x, up.x, -forward.x],
        [right.y, up.y, -forward.y],
        [right.z, up.z, -forward.z],
    ]
}

#[must_use]
pub fn blend_cameras(a: &Camera, b: &Camera, t: f32) -> Camera {
    let qa = quaternion_from_matrix(&camera_rotation(a));
    let qb = quaternion_from_matrix(&camera_rotation(b));
    let q = slerp(qa, qb, t);
    let matrix = matrix_from_quaternion(q, [0.0; 3]);
    let eye = a.eye + (b.eye - a.eye) * t;
    let back = Vec3::new(matrix[0][2], matrix[1][2], matrix[2][2]);
    let up = Vec3::new(matrix[0][1], matrix[1][1], matrix[2][1]);
    let reach = (a.target - a.eye).length() * (1.0 - t) + (b.target - b.eye).length() * t;
    Camera {
        eye,
        target: eye - back * reach,
        up,
        ..*b
    }
}

#[must_use]
pub fn evaluate(animation: &CameraAnimation, t: f32) -> Option<CameraPose> {
    match &animation.track {
        CameraTrack::Free(keys) => {
            let position_lens = blend(keys, t, |k| {
                [k.position[0], k.position[1], k.position[2], k.lens]
            })?;
            let rotation = sample_rotation(keys, t)?;
            Some(CameraPose {
                matrix: matrix_from_quaternion(
                    rotation,
                    [position_lens[0], position_lens[1], position_lens[2]],
                ),
                lens: position_lens[3],
            })
        }
        CameraTrack::Look { camera, target } => {
            let c = blend(camera, t, |k| {
                [k.position[0], k.position[1], k.position[2], k.roll, k.lens]
            })?;
            let tg = blend(target, t, |k| *k)?;
            Some(CameraPose {
                matrix: look_at([c[0], c[1], c[2]], tg, c[3]),
                lens: c[4],
            })
        }
    }
}

#[cfg(test)]
mod tests;
