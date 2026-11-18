use mint_sim::{Component, Entity, Frame, StateHasher};

use crate::fx::FxPoint;
use crate::unit::Kind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeathPhase {
    Initial,
    Midpoint,
    Final,
    HitGround,
}

impl DeathPhase {
    pub(crate) fn parse(token: &str) -> Option<Self> {
        match token.to_ascii_uppercase().as_str() {
            "INITIAL" => Some(Self::Initial),
            "MIDPOINT" => Some(Self::Midpoint),
            "FINAL" => Some(Self::Final),
            "HIT_GROUND" => Some(Self::HitGround),
            _ => None,
        }
    }

    pub(crate) const fn index(self) -> usize {
        self as usize
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Initial => "INITIAL",
            Self::Midpoint => "MIDPOINT",
            Self::Final => "FINAL",
            Self::HitGround => "HIT_GROUND",
        }
    }
}

pub(crate) type PhaseLists = [Vec<String>; 4];

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DeathPhaseEvent {
    pub(crate) entity: Entity,
    pub(crate) kind: Kind,
    pub(crate) death: u32,
    pub(crate) phase: DeathPhase,
    pub(crate) at: FxPoint,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct DeathPhaseEvents {
    frame: Frame,
    events: Vec<DeathPhaseEvent>,
}

impl DeathPhaseEvents {
    pub(crate) fn push(&mut self, now: Frame, event: DeathPhaseEvent) {
        if self.frame != now {
            self.events.clear();
            self.frame = now;
        }
        self.events.push(event);
    }

    pub(crate) fn of(&self, now: Frame) -> &[DeathPhaseEvent] {
        if self.frame == now {
            &self.events
        } else {
            &[]
        }
    }
}

impl Component for DeathPhaseEvents {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.frame.hash_into(hasher);
        hasher.write_usize(self.events.len());
        for event in &self.events {
            event.entity.hash_into(hasher);
            hasher.write_u16(event.kind.0);
            hasher.write_u32(event.death);
            hasher.write_u8(event.phase as u8);
            hasher.write_f32(event.at.x);
            hasher.write_f32(event.at.y);
            hasher.write_bool(event.at.z.is_some());
            if let Some(z) = event.at.z {
                hasher.write_f32(z);
            }
        }
    }
}
