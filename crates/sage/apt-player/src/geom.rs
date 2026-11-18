use mint_sage_apt::Placement;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Matrix {
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    #[must_use]
    pub fn from_placement(p: &Placement) -> Self {
        Self {
            a: p.transform.m00,
            b: p.transform.m01,
            c: p.transform.m10,
            d: p.transform.m11,
            tx: p.translate.x,
            ty: p.translate.y,
        }
    }

    #[must_use]
    pub fn concat(&self, child: &Self) -> Self {
        Self {
            a: self.a * child.a + self.c * child.b,
            b: self.b * child.a + self.d * child.b,
            c: self.a * child.c + self.c * child.d,
            d: self.b * child.c + self.d * child.d,
            tx: self.a * child.tx + self.c * child.ty + self.tx,
            ty: self.b * child.tx + self.d * child.ty + self.ty,
        }
    }

    #[must_use]
    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.tx,
            self.b * x + self.d * y + self.ty,
        )
    }

    #[must_use]
    pub fn inverse(&self) -> Option<Self> {
        let det = self.a * self.d - self.b * self.c;
        if !det.is_finite() || det.abs() < 1e-12 {
            return None;
        }
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        Some(Self {
            a,
            b,
            c,
            d,
            tx: -(a * self.tx + c * self.ty),
            ty: -(b * self.tx + d * self.ty),
        })
    }

    #[must_use]
    pub fn transform_rect(&self, r: [f32; 4]) -> [f32; 4] {
        let corners = [
            self.apply(r[0], r[1]),
            self.apply(r[2], r[1]),
            self.apply(r[0], r[3]),
            self.apply(r[2], r[3]),
        ];
        let mut out = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for (x, y) in corners {
            out[0] = out[0].min(x);
            out[1] = out[1].min(y);
            out[2] = out[2].max(x);
            out[3] = out[3].max(y);
        }
        out
    }

    #[must_use]
    pub fn rect_contains(&self, r: [f32; 4], p: [f32; 2]) -> bool {
        self.inverse().is_some_and(|inv| {
            let (x, y) = inv.apply(p[0], p[1]);
            x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3]
        })
    }
}

impl Default for Matrix {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorTransform {
    pub mult: [f32; 4],
    pub add: [f32; 4],
}

impl ColorTransform {
    pub const IDENTITY: Self = Self {
        mult: [1.0; 4],
        add: [0.0; 4],
    };

    #[must_use]
    pub fn from_apt(mult: u32, add: u32) -> Self {
        let [r, g, b, a] = mult.to_le_bytes();
        let [ab, ag, ar, _] = add.to_le_bytes();
        Self {
            mult: [unit(r), unit(g), unit(b), unit(a)],
            add: [unit(ar), unit(ag), unit(ab), 0.0],
        }
    }

    #[must_use]
    pub fn concat(&self, child: &Self) -> Self {
        let mut out = Self::IDENTITY;
        for i in 0..4 {
            let (Some(pm), Some(pa), Some(cm), Some(ca)) = (
                self.mult.get(i),
                self.add.get(i),
                child.mult.get(i),
                child.add.get(i),
            ) else {
                continue;
            };
            if let (Some(m), Some(a)) = (out.mult.get_mut(i), out.add.get_mut(i)) {
                *m = pm * cm;
                *a = pa + pm * ca;
            }
        }
        out
    }
}

impl Default for ColorTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

fn unit(byte: u8) -> f32 {
    f32::from(byte) / 255.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concat_applies_the_parent_scale_to_the_childs_translation() {
        let parent = Matrix {
            a: 2.0,
            d: 3.0,
            tx: 10.0,
            ty: 20.0,
            ..Matrix::IDENTITY
        };
        let child = Matrix {
            tx: 1.0,
            ty: 1.0,
            ..Matrix::IDENTITY
        };
        let m = parent.concat(&child);
        assert_eq!(m.apply(0.0, 0.0), (12.0, 23.0));
    }

    #[test]
    fn a_childs_add_term_is_scaled_by_the_parents_multiply() {
        let parent = ColorTransform {
            mult: [0.5; 4],
            add: [0.1, 0.0, 0.0, 0.0],
        };
        let child = ColorTransform {
            mult: [1.0; 4],
            add: [0.4, 0.0, 0.0, 0.0],
        };
        let c = parent.concat(&child);
        assert!((c.add[0] - 0.3).abs() < 1e-6, "0.1 + 0.5 * 0.4");
        assert_eq!(c.mult[0], 0.5);
    }

    #[test]
    fn apt_add_word_is_0x00rrggbb_and_mult_is_memory_rgba() {
        let c = ColorTransform::from_apt(0x7200_0000, 0x0000_8CE0);
        assert_eq!(c.mult[3], f32::from(0x72u8) / 255.0);
        assert_eq!(
            c.add,
            [
                0.0,
                f32::from(0x8Cu8) / 255.0,
                f32::from(0xE0u8) / 255.0,
                0.0
            ]
        );
    }
}
