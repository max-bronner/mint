use mint_math::rng::Pcg32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Roll {
    Int {
        lo: i32,
        hi: i32,
        value: i32,
    },
    Real {
        lo: f32,
        hi: f32,
        drew: bool,
        value: f32,
    },
}

#[derive(Debug, Clone)]
pub struct Rolls {
    rng: Pcg32,
    log: Option<Vec<Roll>>,
}

impl Rolls {
    pub(crate) fn new(rng: Pcg32, record: bool) -> Self {
        Self {
            rng,
            log: record.then(Vec::new),
        }
    }

    pub(crate) fn rng(&self) -> &Pcg32 {
        &self.rng
    }

    pub(crate) fn rng_mut(&mut self) -> &mut Pcg32 {
        &mut self.rng
    }

    pub(crate) fn load(&mut self, rng: Pcg32) {
        self.rng = rng;
    }

    pub(crate) fn take_log(&mut self) -> Vec<Roll> {
        self.log.as_mut().map(std::mem::take).unwrap_or_default()
    }

    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    pub fn int(&mut self, lo: i32, hi: i32) -> i32 {
        let span = hi.wrapping_sub(lo).wrapping_add(1) as u32;
        let value = if span == 0 {
            lo
        } else {
            let raw = self.rng.next_u32();
            lo.wrapping_add(raw.checked_rem(span).unwrap_or(0) as i32)
        };
        if let Some(log) = &mut self.log {
            log.push(Roll::Int { lo, hi, value });
        }
        value
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn real(&mut self, lo: f32, hi: f32) -> f32 {
        let drew = hi > lo;
        let value = if drew {
            let raw = f64::from(self.rng.next_u32());
            let span = f64::from(hi) - f64::from(lo);
            (f64::from(lo) + raw * (1.0 / 4_294_967_296.0) * span) as f32
        } else {
            hi
        };
        if let Some(log) = &mut self.log {
            log.push(Roll::Real {
                lo,
                hi,
                drew,
                value,
            });
        }
        value
    }

    pub(crate) fn pick(&mut self, len: usize) -> usize {
        let top = i32::try_from(len.saturating_sub(1)).unwrap_or(i32::MAX);
        usize::try_from(self.int(0, top)).unwrap_or(0)
    }
}
