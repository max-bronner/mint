#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TintSlot {
    Selection,
    Effect,
}

impl TintSlot {
    fn index(self) -> usize {
        match self {
            Self::Selection => 0,
            Self::Effect => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TintEnvelope {
    pub color: [f32; 3],
    pub attack: f32,
    pub sustain: f32,
    pub decay: f32,
}

impl TintEnvelope {
    #[must_use]
    pub fn sample(&self, elapsed: f32) -> [f32; 3] {
        let (attack, sustain, decay) = (span(self.attack), span(self.sustain), span(self.decay));
        if elapsed.is_nan() || elapsed < 0.0 {
            return [0.0; 3];
        }
        let level = if elapsed < attack {
            elapsed / attack
        } else if elapsed < attack + sustain {
            1.0
        } else if elapsed < attack + sustain + decay {
            1.0 - (elapsed - attack - sustain) / decay
        } else {
            0.0
        };
        self.color.map(|channel| channel * level)
    }

    fn ended(&self, elapsed: f32) -> bool {
        elapsed.is_nan() || elapsed >= span(self.attack) + span(self.sustain) + span(self.decay)
    }
}

fn span(seconds: f32) -> f32 {
    if seconds.is_finite() {
        seconds.max(0.0)
    } else {
        0.0
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct Tints([Option<(TintEnvelope, f32)>; 2]);

impl Tints {
    pub(super) fn play(&mut self, slot: TintSlot, envelope: TintEnvelope, now: f32) {
        if let Some(entry) = self.0.get_mut(slot.index()) {
            *entry = Some((envelope, now));
        }
    }

    pub(super) fn at(&self, now: f32) -> Option<[f32; 3]> {
        let mut sum = None;
        for (envelope, start) in self.0.iter().flatten() {
            let elapsed = now - start;
            if envelope.ended(elapsed) {
                continue;
            }
            let add = envelope.sample(elapsed);
            let total: &mut [f32; 3] = sum.get_or_insert([0.0; 3]);
            for (total, add) in total.iter_mut().zip(add) {
                *total += add;
            }
        }
        sum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_envelope_ramps_in_holds_ramps_out_and_ends_at_zero() {
        let envelope = TintEnvelope {
            color: [1.0, 0.5, 0.0],
            attack: 0.5,
            sustain: 0.5,
            decay: 3.0,
        };
        let red = |t: f32| envelope.sample(t)[0];
        assert_eq!(red(-0.1), 0.0);
        assert!((red(0.25) - 0.5).abs() < 1e-6, "halfway up the attack");
        assert_eq!(red(0.75), 1.0, "sustained");
        assert!((red(2.5) - 0.5).abs() < 1e-6, "halfway down the decay");
        assert_eq!(red(4.0), 0.0, "ended");
        assert_eq!(envelope.sample(0.75), [1.0, 0.5, 0.0]);
        let mut tints = Tints::default();
        tints.play(TintSlot::Effect, envelope, 10.0);
        assert!(tints.at(13.9).is_some());
        assert_eq!(tints.at(14.0), None, "an ended envelope stops adding");
    }
}
