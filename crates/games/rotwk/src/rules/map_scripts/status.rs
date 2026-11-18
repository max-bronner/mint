use std::borrow::Cow;
use std::collections::BTreeMap;

use mint_sage_map::Parameter;
use mint_sim::{Entity, StateHasher};

const PARAM_OBJECT_STATUS: i32 = 41;

const STATUS_NAMES: [&str; 106] = [
    "DESTROYED",
    "CAN_ATTACK",
    "UNDER_CONSTRUCTION",
    "UNSELECTABLE",
    "NO_COLLISIONS",
    "NO_ATTACK",
    "AIRBORNE_TARGET",
    "PARACHUTING",
    "REPULSOR",
    "HIJACKED",
    "AFLAME",
    "BURNED",
    "WET",
    "IS_FIRING_WEAPON",
    "IS_BRAKING",
    "STEALTHED",
    "HIDDEN",
    "DETECTED",
    "CAN_STEALTH",
    "SOLD",
    "UNDERGOING_REPAIR",
    "RECONSTRUCTING",
    "IS_ATTACKING",
    "NO_AUTO_ACQUIRE",
    "USING_ABILITY",
    "IS_AIMING_WEAPON",
    "NO_ATTACK_FROM_AI",
    "IGNORING_STEALTH",
    "IS_MELEE_ATTACKING",
    "GUARD_SELECTION",
    "LEASHED_RETURNING",
    "DEATH_1",
    "DEATH_2",
    "DEATH_3",
    "DEATH_4",
    "DEATH_5",
    "CONTESTED",
    "CONTESTING_BUILDING",
    "HORDE_MEMBER",
    "RIDERLESS",
    "RIDER_IS_PILOT",
    "RIDER1",
    "RIDER2",
    "RIDER3",
    "RIDER4",
    "RIDER5",
    "RIDER6",
    "RIDER7",
    "RIDER8",
    "IMMOBILE",
    "FLEE_OFF_MAP",
    "NOT_IN_WORLD",
    "INAUDIBLE",
    "CHANTING",
    "ENRAGED",
    "CREATE_DRAWABLE_WITH_LOW_DETAIL",
    "SINKING",
    "RAMPAGING",
    "INSIDE_GARRISON",
    "DEPLOYED",
    "UNATTACKABLE",
    "ENCLOSED",
    "TEMPORARILY_DEFECTED",
    "TAGGED",
    "DEPLOYING",
    "BLOODTHIRSTY",
    "PORTER_TAGGED",
    "GRAB_AND_DROP",
    "STAND_GROUND",
    "UNCONTROLLABLY_SCARED",
    "SPECIAL_ABILITY_PACKING_UNPACKING_OR_USING",
    "PLEASE_EAT_ME",
    "UPDATING_AI",
    "HUNT_WHEN_IDLE",
    "IGNORE_AI_COMMAND",
    "RUNNING_DOWN_FROM_BEHIND",
    "DO_NOT_SCORE",
    "CAN_NOT_WALK_ON",
    "MARCH_OF_DEATH",
    "DO_NOT_PICK_ME",
    "INHERITED_FROM_ALLY_TEAM",
    "SWITCHED_WEAPONS",
    "END_FIRE_STATE",
    "BOOKENDING",
    "ELVISH_EXPRESSLY",
    "INSIDE_CASTLE",
    "BUILD_BEING_CANCELED",
    "PENDING_CONSTRUCTION",
    "PHANTOM_STRUCTURE",
    "IN_FORMATION_TEMPLATE",
    "IS_LEAVING_FACTORY",
    "MOVING_TO_DISMOUNT",
    "NO_HERO_PROPERTIES",
    "CAN_ENTER_ANYTHING",
    "HOLDING_THE_RING",
    "INVISIBLE_DETECTED_BY_FRIEND",
    "INVISIBLE_DETECTED",
    "WORKER_REPAIRING",
    "ATTACHED",
    "WONT_RIDE_WITH_YOU",
    "COMMAND_BUTTON_TOGGLED",
    "IGNORE_PARALYZE_NUGGET",
    "SUMMONING_REPLACEMENT",
    "HOLDING_THE_SHARD",
    "USER_DEFINED_1",
    "USER_DEFINED_2",
];

#[must_use]
pub(super) fn status_name(index: i32) -> Option<&'static str> {
    STATUS_NAMES.get(usize::try_from(index).ok()?).copied()
}

#[must_use]
pub(super) fn status_index(name: &str) -> i32 {
    STATUS_NAMES
        .iter()
        .position(|candidate| candidate.eq_ignore_ascii_case(name))
        .and_then(|index| i32::try_from(index).ok())
        .unwrap_or(-1)
}

#[must_use]
pub(super) fn resolve_status_params(params: &[Parameter]) -> Cow<'_, [Parameter]> {
    let is_status = |p: &Parameter| matches!(p, Parameter::Value { param_type, .. } if *param_type == PARAM_OBJECT_STATUS);
    if !params.iter().any(is_status) {
        return Cow::Borrowed(params);
    }
    Cow::Owned(
        params
            .iter()
            .map(|p| match p {
                Parameter::Value {
                    param_type,
                    real_value,
                    string_value,
                    ..
                } if *param_type == PARAM_OBJECT_STATUS => Parameter::Value {
                    param_type: *param_type,
                    int_value: status_index(string_value),
                    real_value: *real_value,
                    string_value: string_value.clone(),
                },
                other => other.clone(),
            })
            .collect(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ScriptStatusBit {
    Disabled = 0x01,
    Unpowered = 0x02,
    Unsellable = 0x04,
    Unstealthed = 0x08,
    Targetable = 0x10,
    NotRecruitable = 0x20,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ScriptStatuses(BTreeMap<Entity, u8>);

impl ScriptStatuses {
    pub(crate) fn get(&self, entity: Entity) -> u8 {
        self.0.get(&entity).copied().unwrap_or(0)
    }

    pub(crate) fn has(&self, entity: Entity, bit: ScriptStatusBit) -> bool {
        self.get(entity) & bit as u8 != 0
    }

    pub(crate) fn set(&mut self, entity: Entity, bit: ScriptStatusBit, on: bool) -> bool {
        let old = self.get(entity);
        let new = if on {
            old | bit as u8
        } else {
            old & !(bit as u8)
        };
        if new == 0 {
            self.0.remove(&entity);
        } else {
            self.0.insert(entity, new);
        }
        new != old
    }

    pub(crate) fn retain_live(&mut self, is_live: impl Fn(Entity) -> bool) {
        self.0.retain(|entity, _| is_live(*entity));
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for (entity, bits) in &self.0 {
            entity.hash_into(hasher);
            hasher.write_u8(*bits);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_holds_the_indices_the_image_documents() {
        for (name, index) in [
            ("DESTROYED", 0),
            ("UNDER_CONSTRUCTION", 2),
            ("IS_ATTACKING", 22),
            ("HORDE_MEMBER", 38),
            ("STAND_GROUND", 68),
            ("HOLDING_THE_SHARD", 103),
        ] {
            assert_eq!(status_index(name), index, "{name}");
        }
        assert_eq!(STATUS_NAMES.len(), 106);
    }

    #[test]
    fn a_status_parameter_is_read_by_name_not_by_its_stored_int() {
        let stored = |name: &str| Parameter::Value {
            param_type: PARAM_OBJECT_STATUS,
            int_value: 0,
            real_value: 0.0,
            string_value: name.to_owned(),
        };
        let params = [stored("is_attacking"), stored("NOT_A_STATUS")];
        let resolved = resolve_status_params(&params);
        let ints: Vec<i32> = resolved
            .iter()
            .filter_map(|p| match p {
                Parameter::Value { int_value, .. } => Some(*int_value),
                Parameter::Coord3D { .. } => None,
            })
            .collect();
        assert_eq!(ints, [22, -1]);
    }
}
