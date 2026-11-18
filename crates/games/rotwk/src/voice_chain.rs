use std::collections::BTreeMap;

pub(crate) const VOICE_ATTACK_UNIT: &str = "VoiceAttackUnit";
pub(crate) const VOICE_ENTER_UNIT: &str = "VoiceEnterUnit";
pub(crate) const VOICE_ATTACK: &str = "VoiceAttack";
pub(crate) const VOICE_ATTACK_CHARGE: &str = "VoiceAttackCharge";
pub(crate) const VOICE_ATTACK_STRUCTURE: &str = "VoiceAttackStructure";
pub(crate) const VOICE_ATTACK_MACHINE: &str = "VoiceAttackMachine";
pub(crate) const VOICE_ATTACK_AIR: &str = "VoiceAttackAir";
pub(crate) const VOICE_GET_HEALED: &str = "VoiceGetHealed";
pub(crate) const VOICE_GARRISON: &str = "VoiceGarrison";
pub(crate) const VOICE_ENTER: &str = "VoiceEnter";
pub(crate) const VOICE_ENTER_HOSTILE: &str = "VoiceEnterHostile";
pub(crate) const SOUND_STEALTH_ON: &str = "SoundStealthOn";
pub(crate) const SOUND_STEALTH_OFF: &str = "SoundStealthOff";
pub(crate) const VOICE_MOVE: &str = "VoiceMove";
pub(crate) const VOICE_MOVE_TO_HIGHER_GROUND: &str = "VoiceMoveToHigherGround";
pub(crate) const VOICE_RETREAT_TO_CASTLE: &str = "VoiceRetreatToCastle";
pub(crate) const VOICE_MOVE_TO_CAMP: &str = "VoiceMoveToCamp";
pub(crate) const VOICE_MOVE_WHILE_ATTACKING: &str = "VoiceMoveWhileAttacking";
pub(crate) const VOICE_ENTER_STATE_ATTACK: &str = "VoiceEnterStateAttack";
pub(crate) const VOICE_ENTER_STATE_ATTACK_CHARGE: &str = "VoiceEnterStateAttackCharge";
pub(crate) const VOICE_ENTER_STATE_ATTACK_AIR: &str = "VoiceEnterStateAttackAir";
pub(crate) const VOICE_ENTER_STATE_ATTACK_STRUCTURE: &str = "VoiceEnterStateAttackStructure";
pub(crate) const VOICE_ENTER_STATE_ATTACK_MACHINE: &str = "VoiceEnterStateAttackMachine";
pub(crate) const VOICE_ENTER_STATE_MOVE: &str = "VoiceEnterStateMove";
pub(crate) const VOICE_ENTER_STATE_MOVE_TO_HIGHER_GROUND: &str =
    "VoiceEnterStateMoveToHigherGround";
pub(crate) const VOICE_ENTER_STATE_RETREAT_TO_CASTLE: &str = "VoiceEnterStateRetreatToCastle";
pub(crate) const VOICE_ENTER_STATE_MOVE_TO_CAMP: &str = "VoiceEnterStateMoveToCamp";
pub(crate) const VOICE_ENTER_STATE_MOVE_WHILE_ATTACKING: &str = "VoiceEnterStateMoveWhileAttacking";

pub(crate) const OBJECT_FIELDS: [&str; 20] = [
    VOICE_ATTACK_CHARGE,
    VOICE_ATTACK_STRUCTURE,
    VOICE_ATTACK_MACHINE,
    VOICE_ATTACK_AIR,
    SOUND_STEALTH_ON,
    SOUND_STEALTH_OFF,
    VOICE_MOVE_TO_HIGHER_GROUND,
    VOICE_RETREAT_TO_CASTLE,
    VOICE_MOVE_TO_CAMP,
    VOICE_MOVE_WHILE_ATTACKING,
    VOICE_ENTER_STATE_ATTACK,
    VOICE_ENTER_STATE_ATTACK_CHARGE,
    VOICE_ENTER_STATE_ATTACK_AIR,
    VOICE_ENTER_STATE_ATTACK_STRUCTURE,
    VOICE_ENTER_STATE_ATTACK_MACHINE,
    VOICE_ENTER_STATE_MOVE,
    VOICE_ENTER_STATE_MOVE_TO_HIGHER_GROUND,
    VOICE_ENTER_STATE_RETREAT_TO_CASTLE,
    VOICE_ENTER_STATE_MOVE_TO_CAMP,
    VOICE_ENTER_STATE_MOVE_WHILE_ATTACKING,
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MoveVoiceFacts {
    pub(crate) campness_start: f32,
    pub(crate) campness_goal: f32,
    pub(crate) higher_ground: bool,
    pub(crate) engaged: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TargetFacts {
    pub(crate) kind: u16,
    pub(crate) structure: bool,
    pub(crate) machine: bool,
    pub(crate) heal_pad: bool,
    pub(crate) airborne: bool,
    pub(crate) contested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Slot {
    Field(&'static str),
    Unit(String),
}

impl Slot {
    pub(crate) fn label(&self) -> &str {
        match self {
            Self::Field(field) => field,
            Self::Unit(key) => key,
        }
    }
}

fn chain_field(enter_state: bool, plain: &'static str, enter_state_name: &'static str) -> Slot {
    Slot::Field(if enter_state { enter_state_name } else { plain })
}

pub(crate) fn attack(
    target: Option<(&str, TargetFacts)>,
    charge_due: bool,
    enter_state: bool,
) -> Vec<Slot> {
    let mut chain = Vec::with_capacity(5);
    if let Some((name, facts)) = target {
        chain.push(Slot::Unit(format!("{VOICE_ATTACK_UNIT}{name}")));
        if facts.structure {
            chain.push(chain_field(
                enter_state,
                VOICE_ATTACK_STRUCTURE,
                VOICE_ENTER_STATE_ATTACK_STRUCTURE,
            ));
        } else if facts.machine {
            chain.push(chain_field(
                enter_state,
                VOICE_ATTACK_MACHINE,
                VOICE_ENTER_STATE_ATTACK_MACHINE,
            ));
        }
    }
    if charge_due {
        chain.push(chain_field(
            enter_state,
            VOICE_ATTACK_CHARGE,
            VOICE_ENTER_STATE_ATTACK_CHARGE,
        ));
    }
    if target.is_some_and(|(_, facts)| facts.airborne) {
        chain.push(chain_field(
            enter_state,
            VOICE_ATTACK_AIR,
            VOICE_ENTER_STATE_ATTACK_AIR,
        ));
    }
    chain.push(chain_field(
        enter_state,
        VOICE_ATTACK,
        VOICE_ENTER_STATE_ATTACK,
    ));
    chain
}

pub(crate) fn move_order(
    camp_holds: bool,
    higher_ground: bool,
    engaged: bool,
    enter_state: bool,
) -> Vec<Slot> {
    let mut chain = Vec::with_capacity(4);
    if camp_holds {
        if engaged {
            chain.push(chain_field(
                enter_state,
                VOICE_RETREAT_TO_CASTLE,
                VOICE_ENTER_STATE_RETREAT_TO_CASTLE,
            ));
        }
        chain.push(chain_field(
            enter_state,
            VOICE_MOVE_TO_CAMP,
            VOICE_ENTER_STATE_MOVE_TO_CAMP,
        ));
    }
    if higher_ground {
        chain.push(chain_field(
            enter_state,
            VOICE_MOVE_TO_HIGHER_GROUND,
            VOICE_ENTER_STATE_MOVE_TO_HIGHER_GROUND,
        ));
    }
    if engaged {
        chain.push(chain_field(
            enter_state,
            VOICE_MOVE_WHILE_ATTACKING,
            VOICE_ENTER_STATE_MOVE_WHILE_ATTACKING,
        ));
    }
    chain.push(chain_field(enter_state, VOICE_MOVE, VOICE_ENTER_STATE_MOVE));
    chain
}

pub(crate) fn enter(target: Option<(&str, TargetFacts)>) -> Vec<Slot> {
    let Some((name, facts)) = target else {
        return vec![Slot::Unit(VOICE_ENTER.to_owned())];
    };
    let last = if facts.contested {
        VOICE_ENTER_HOSTILE
    } else if facts.heal_pad {
        VOICE_GET_HEALED
    } else if facts.structure {
        VOICE_GARRISON
    } else {
        VOICE_ENTER
    };
    vec![
        Slot::Unit(format!("{VOICE_ENTER_UNIT}{name}")),
        Slot::Unit(last.to_owned()),
    ]
}

#[derive(Debug, Default)]
pub(crate) struct ChargeClocks {
    ready_at: BTreeMap<u64, u64>,
}

impl ChargeClocks {
    pub(crate) fn due(&self, owner: u64, attacking: bool, now: u64) -> bool {
        !attacking && self.ready_at.get(&owner).is_none_or(|ready| now >= *ready)
    }

    pub(crate) fn restart(&mut self, owner: u64, now: u64, timeout: u64) {
        self.ready_at.insert(owner, now.saturating_add(timeout));
    }

    pub(crate) fn retain(&mut self, seen: &[u64]) {
        self.ready_at
            .retain(|owner, _| seen.binary_search(owner).is_ok());
    }
}

#[cfg(test)]
mod tests;
