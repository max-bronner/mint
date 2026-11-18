use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

use crate::effects::{Effect, Name, Names};

pub const DOOR_COUNT_MAX: usize = 4;

const MAX_MILLISECONDS: f32 = 86_400_000.0;

struct DoorFlags {
    opening: &'static str,
    waiting_open: &'static str,
    closing: &'static str,
}

const FLAGS: [DoorFlags; DOOR_COUNT_MAX] = [
    DoorFlags {
        opening: "DOOR_1_OPENING",
        waiting_open: "DOOR_1_WAITING_OPEN",
        closing: "DOOR_1_CLOSING",
    },
    DoorFlags {
        opening: "DOOR_2_OPENING",
        waiting_open: "DOOR_2_WAITING_OPEN",
        closing: "DOOR_2_CLOSING",
    },
    DoorFlags {
        opening: "DOOR_3_OPENING",
        waiting_open: "DOOR_3_WAITING_OPEN",
        closing: "DOOR_3_CLOSING",
    },
    DoorFlags {
        opening: "DOOR_4_OPENING",
        waiting_open: "DOOR_4_WAITING_OPEN",
        closing: "DOOR_4_CLOSING",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductionUpdate {
    has_doors: bool,
    opening: LogicFrames,
    wait_open: LogicFrames,
    closing: LogicFrames,
}

impl ProductionUpdate {
    #[must_use]
    pub fn has_doors(&self) -> bool {
        self.has_doors
    }
}

impl Component for ProductionUpdate {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for ProductionUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn production_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<ProductionUpdate, FieldError> {
    Ok(ProductionUpdate {
        has_doors: door_count(fields)? > 0.0,
        opening: duration(fields, "DoorOpeningTime")?,
        wait_open: duration(fields, "DoorWaitOpenTime")?,
        closing: duration(fields, "DoorCloseTime")?,
    })
}

fn door_count(fields: &dyn ModuleFieldSource) -> Result<f32, FieldError> {
    const KEYWORD: &str = "NumDoorAnimations";
    let Some(raw) = fields.field(KEYWORD) else {
        return Ok(0.0);
    };
    fields
        .field_numeric(KEYWORD)
        .filter(|count| count.is_finite() && count.fract() == 0.0)
        .ok_or_else(|| FieldError::new(format!("{KEYWORD} '{raw}' is not a whole number")))
}

fn duration(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<LogicFrames, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(LogicFrames::ZERO);
    };
    let ms = fields
        .field_numeric(keyword)
        .filter(|ms| ms.is_finite() && *ms >= 0.0)
        .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not milliseconds")))?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = ms.min(MAX_MILLISECONDS) as u32;
    Ok(LogicFrames::from_milliseconds(whole))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DoorPhase {
    Opening,
    WaitingOpen,
    Closing,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProductionDoors {
    doors: [Option<(DoorPhase, Frame)>; DOOR_COUNT_MAX],
}

impl ProductionDoors {
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.doors.iter().all(Option::is_none)
    }

    pub fn step(&mut self, data: &ProductionUpdate, now: Frame) -> Vec<Effect> {
        let mut changes = Vec::new();
        for (door, flags) in self.doors.iter_mut().zip(&FLAGS) {
            let Some((phase, since)) = *door else {
                continue;
            };
            let limit = match phase {
                DoorPhase::Opening => data.opening,
                DoorPhase::WaitingOpen => data.wait_open,
                DoorPhase::Closing => data.closing,
            };
            if now.get().saturating_sub(since.get()) <= u64::from(limit.get()) {
                continue;
            }
            let (next, change) = match phase {
                DoorPhase::Opening => (
                    Some((DoorPhase::WaitingOpen, now)),
                    conditions(&[flags.waiting_open], &[flags.opening]),
                ),
                DoorPhase::WaitingOpen => (
                    Some((DoorPhase::Closing, now)),
                    conditions(&[flags.closing], &[flags.waiting_open]),
                ),
                DoorPhase::Closing => (None, conditions(&[], &[flags.closing])),
            };
            *door = next;
            changes.push(change);
        }
        changes
    }

    pub fn exit(&mut self, door: usize, now: Frame) -> (bool, Option<Effect>) {
        let (Some(slot), Some(flags)) = (self.doors.get_mut(door), FLAGS.get(door)) else {
            return (true, None);
        };
        match slot.map(|(phase, _)| phase) {
            None => {
                *slot = Some((DoorPhase::Opening, now));
                (false, Some(conditions(&[flags.opening], &[])))
            }
            Some(DoorPhase::Opening) => (false, None),
            Some(DoorPhase::WaitingOpen) => {
                *slot = Some((DoorPhase::WaitingOpen, now));
                (true, None)
            }
            Some(DoorPhase::Closing) => {
                *slot = Some((DoorPhase::WaitingOpen, now));
                let change = conditions(&[flags.waiting_open], &[flags.opening, flags.closing]);
                (true, Some(change))
            }
        }
    }
}

fn conditions(add: &[&str], remove: &[&str]) -> Effect {
    let names = |flags: &[&str]| -> Names { flags.iter().map(|flag| Name::from(*flag)).collect() };
    Effect::ModelConditions {
        add: names(add),
        remove: names(remove),
    }
}

impl Component for ProductionDoors {
    fn hash_into(&self, hasher: &mut StateHasher) {
        for door in &self.doors {
            let (tag, since) = match door {
                None => (0, Frame::ZERO),
                Some((DoorPhase::Opening, since)) => (1, *since),
                Some((DoorPhase::WaitingOpen, since)) => (2, *since),
                Some((DoorPhase::Closing, since)) => (3, *since),
            };
            hasher.write_u8(tag);
            since.hash_into(hasher);
        }
    }
}

#[cfg(test)]
mod tests;
