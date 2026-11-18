use std::collections::BTreeMap;

use crate::diagnostic::{ValueError, ValueErrors};
use crate::tree::{Block, Document};
use crate::value::{parse_bool, parse_float, parse_percent, Duration};

pub const LOCOMOTOR_KEYWORD: &str = "Locomotor";

pub const LOCOMOTOR_FIELDS: [&str; 48] = [
    "Surfaces",
    "ZAxisBehavior",
    "Appearance",
    "TurnTime",
    "TurnTimeDamaged",
    "Braking",
    "Acceleration",
    "MinTurnSpeed",
    "MinSpeed",
    "StickToGround",
    "SlowTurnRadius",
    "FastTurnRadius",
    "TurnThreshold",
    "CloseEnoughDist",
    "CanMoveBackwards",
    "HasSuspension",
    "RollStiffness",
    "RollDamping",
    "PitchStiffness",
    "PitchDamping",
    "Lift",
    "LiftDamaged",
    "PreferredHeight",
    "PreferredAttackHeight",
    "AirborneTargetingHeight",
    "MaxThrustAngle",
    "ChargeSpeed",
    "ChargeAvailable",
    "ChargeIgnoresCondition",
    "SwoopAccelerationRate",
    "SwoopStandoffHeight",
    "SwoopStandoffRadius",
    "SwoopTerminalVelocity",
    "LookAheadMult",
    "BurningDeathRadius",
    "BurningDeathIsCavalry",
    "MaxTurnWithoutReform",
    "TurnWhileMoving",
    "WalkDistance",
    "EnableHighSpeedTurnModelconditions",
    "FormationPriority",
    "RiverModifier",
    "TurnThresholdHS",
    "TurnPivotOffset",
    "CrewPowered",
    "ScalesWalls",
    "PreferredHeightDamping",
    "WanderAboutPointRadius",
];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LocomotorDef {
    pub surfaces: Vec<String>,
    pub z_axis_behavior: Option<String>,
    pub appearance: Option<String>,
    pub turn_time: Option<Duration>,
    pub turn_time_damaged: Option<Duration>,
    pub braking: Option<f32>,
    pub acceleration: Option<f32>,
    pub min_turn_speed: Option<f32>,
    pub min_speed: Option<f32>,
    pub stick_to_ground: Option<bool>,
    pub slow_turn_radius: Option<f32>,
    pub fast_turn_radius: Option<f32>,
    pub turn_threshold: Option<f32>,
    pub acc_dec_trigger: Option<f32>,
    pub close_enough_dist: Option<f32>,
    pub slide_into_place_time: Option<f32>,
    pub can_move_backwards: Option<bool>,
    pub crew_powered: Option<bool>,
    pub scales_walls: Option<bool>,
    pub wander_about_point_radius: Option<f32>,
    pub has_suspension: Option<bool>,
    pub roll_stiffness: Option<f32>,
    pub roll_damping: Option<f32>,
    pub pitch_stiffness: Option<f32>,
    pub pitch_damping: Option<f32>,
    pub lift: Option<f32>,
    pub lift_damaged: Option<f32>,
    pub preferred_height: Option<f32>,
    pub preferred_attack_height: Option<f32>,
    pub airborne_targeting_height: Option<f32>,
    pub max_thrust_angle: Option<f32>,
    pub charge_speed: Option<f32>,
    pub speed_damaged: Option<f32>,
    pub charge_available: Option<bool>,
    pub non_dirty_transform: Option<bool>,
    pub look_ahead_mult: Option<f32>,
    pub charge_ignores_condition: Option<bool>,
    pub swoop_acceleration_rate: Option<f32>,
    pub swoop_standoff_height: Option<f32>,
    pub swoop_standoff_radius: Option<f32>,
    pub swoop_terminal_velocity: Option<f32>,
    pub burning_death_radius: Option<f32>,
    pub burning_death_is_cavalry: Option<bool>,
    pub max_turn_without_reform: Option<f32>,
    pub turn_while_moving: Option<bool>,
    pub walk_distance: Option<f32>,
    pub enable_high_speed_turn_model_conditions: Option<bool>,
    pub formation_priority: Option<u8>,
    pub river_modifier: Option<f32>,
    pub turn_threshold_hs: Option<f32>,
    pub turn_pivot_offset: Option<f32>,
    pub preferred_height_damping: Option<f32>,
}

pub const FORMATION_PRIORITY_NAMES: [&str; 13] = [
    "NO_FORMATION",
    "CAVALRY1",
    "CAVALRY2",
    "CAVALRY3",
    "MELEE1",
    "MELEE2",
    "MELEE3",
    "RANGED1",
    "RANGED2",
    "RANGED3",
    "ARTILLERY1",
    "ARTILLERY2",
    "ARTILLERY3",
];

fn parse_formation_priority(value: &str) -> Option<u8> {
    let index = FORMATION_PRIORITY_NAMES
        .iter()
        .position(|name| name.eq_ignore_ascii_case(value.trim()))?;
    u8::try_from(index).ok()
}

pub const SURFACE_NAMES: [&str; 8] = [
    "GROUND",
    "WATER",
    "CLIFF",
    "AIR",
    "RUBBLE",
    "DEEP_WATER",
    "OBSTACLE",
    "IMPASSABLE",
];

fn scalar<T>(
    block: &Block,
    keyword: &str,
    errors: &ValueErrors,
    expected: &'static str,
    parser: impl FnOnce(&str) -> Option<T>,
) -> Option<T> {
    let field = block.field(keyword)?;
    let read = field.value().and_then(parser);
    if read.is_none() {
        errors.field(name_of(block), field, expected);
    }
    read
}

fn surfaces(block: &Block, errors: &ValueErrors) -> Vec<String> {
    let Some(field) = block.field("Surfaces") else {
        return Vec::new();
    };
    let edits = match crate::value::bit_string_edits(&field.args) {
        Ok(edits) => edits,
        Err(token) => {
            errors.value(
                name_of(block),
                field,
                token,
                crate::value::BIT_STRING_EXPECTED,
            );
            return Vec::new();
        }
    };
    let mut named: Vec<String> = Vec::new();
    for edit in edits {
        let (set, name) = match edit {
            crate::value::BitEdit::Clear => {
                named.clear();
                continue;
            }
            crate::value::BitEdit::Set(name) => (true, name),
            crate::value::BitEdit::Unset(name) => (false, name),
        };
        let Some(known) = SURFACE_NAMES
            .iter()
            .find(|known| known.eq_ignore_ascii_case(name))
        else {
            errors.value(name_of(block), field, name, "a surface name");
            continue;
        };
        named.retain(|held| held != known);
        if set {
            named.push((*known).to_owned());
        }
    }
    named
}

fn name_of(block: &Block) -> &str {
    block.name().unwrap_or(LOCOMOTOR_KEYWORD)
}

#[must_use]
pub fn read_locomotor(block: &Block, errors: &ValueErrors) -> LocomotorDef {
    LocomotorDef {
        surfaces: surfaces(block, errors),
        z_axis_behavior: scalar(block, "ZAxisBehavior", errors, "a name", |v| {
            Some(v.to_owned())
        }),
        appearance: scalar(block, "Appearance", errors, "a name", |v| {
            Some(v.to_owned())
        }),
        turn_time: scalar(block, "TurnTime", errors, "a number", parse_float)
            .map(Duration::Unknown),
        turn_time_damaged: scalar(block, "TurnTimeDamaged", errors, "a number", parse_float)
            .map(Duration::Unknown),
        braking: scalar(block, "Braking", errors, "a number", parse_float),
        acceleration: scalar(block, "Acceleration", errors, "a number", parse_float),
        min_turn_speed: scalar(block, "MinTurnSpeed", errors, "a percentage", parse_percent),
        min_speed: scalar(block, "MinSpeed", errors, "a number", parse_float),
        stick_to_ground: scalar(block, "StickToGround", errors, "Yes or No", parse_bool),
        slow_turn_radius: scalar(block, "SlowTurnRadius", errors, "a number", parse_float),
        fast_turn_radius: scalar(block, "FastTurnRadius", errors, "a number", parse_float),
        turn_threshold: scalar(block, "TurnThreshold", errors, "a number", parse_float),
        acc_dec_trigger: scalar(block, "AccDecTrigger", errors, "a number", parse_float),
        close_enough_dist: scalar(block, "CloseEnoughDist", errors, "a number", parse_float),
        slide_into_place_time: scalar(block, "SlideIntoPlaceTime", errors, "a number", parse_float),
        can_move_backwards: scalar(block, "CanMoveBackwards", errors, "Yes or No", parse_bool),
        crew_powered: scalar(block, "CrewPowered", errors, "Yes or No", parse_bool),
        scales_walls: scalar(block, "ScalesWalls", errors, "Yes or No", parse_bool),
        wander_about_point_radius: scalar(
            block,
            "WanderAboutPointRadius",
            errors,
            "a number",
            parse_float,
        ),
        has_suspension: scalar(block, "HasSuspension", errors, "Yes or No", parse_bool),
        roll_stiffness: scalar(block, "RollStiffness", errors, "a number", parse_float),
        roll_damping: scalar(block, "RollDamping", errors, "a number", parse_float),
        pitch_stiffness: scalar(block, "PitchStiffness", errors, "a number", parse_float),
        pitch_damping: scalar(block, "PitchDamping", errors, "a number", parse_float),
        lift: scalar(block, "Lift", errors, "a percentage", parse_percent),
        lift_damaged: scalar(block, "LiftDamaged", errors, "a percentage", parse_percent),
        preferred_height: scalar(block, "PreferredHeight", errors, "a number", parse_float),
        preferred_attack_height: scalar(
            block,
            "PreferredAttackHeight",
            errors,
            "a number",
            parse_float,
        ),
        airborne_targeting_height: scalar(
            block,
            "AirborneTargetingHeight",
            errors,
            "a number",
            parse_float,
        ),
        max_thrust_angle: scalar(block, "MaxThrustAngle", errors, "a number", parse_float),
        charge_speed: scalar(block, "ChargeSpeed", errors, "a percentage", parse_percent),
        speed_damaged: scalar(block, "SpeedDamaged", errors, "a percentage", parse_percent),
        charge_available: scalar(block, "ChargeAvailable", errors, "Yes or No", parse_bool),
        non_dirty_transform: scalar(block, "NonDirtyTransform", errors, "Yes or No", parse_bool),
        look_ahead_mult: scalar(block, "LookAheadMult", errors, "a number", parse_float),
        charge_ignores_condition: scalar(
            block,
            "ChargeIgnoresCondition",
            errors,
            "Yes or No",
            parse_bool,
        ),
        swoop_acceleration_rate: scalar(
            block,
            "SwoopAccelerationRate",
            errors,
            "a number",
            parse_float,
        ),
        swoop_standoff_height: scalar(
            block,
            "SwoopStandoffHeight",
            errors,
            "a number",
            parse_float,
        ),
        swoop_standoff_radius: scalar(
            block,
            "SwoopStandoffRadius",
            errors,
            "a number",
            parse_float,
        ),
        swoop_terminal_velocity: scalar(
            block,
            "SwoopTerminalVelocity",
            errors,
            "a number",
            parse_float,
        ),
        burning_death_radius: scalar(block, "BurningDeathRadius", errors, "a number", parse_float),
        burning_death_is_cavalry: scalar(
            block,
            "BurningDeathIsCavalry",
            errors,
            "Yes or No",
            parse_bool,
        ),
        max_turn_without_reform: scalar(
            block,
            "MaxTurnWithoutReform",
            errors,
            "a number",
            parse_float,
        ),
        turn_while_moving: scalar(block, "TurnWhileMoving", errors, "Yes or No", parse_bool),
        walk_distance: scalar(block, "WalkDistance", errors, "a number", parse_float),
        enable_high_speed_turn_model_conditions: scalar(
            block,
            "EnableHighSpeedTurnModelconditions",
            errors,
            "Yes or No",
            parse_bool,
        ),
        formation_priority: scalar(
            block,
            "FormationPriority",
            errors,
            "a FormationPriority name",
            parse_formation_priority,
        ),
        river_modifier: scalar(
            block,
            "RiverModifier",
            errors,
            "a percentage",
            parse_percent,
        ),
        turn_threshold_hs: scalar(block, "TurnThresholdHS", errors, "a number", parse_float),
        turn_pivot_offset: scalar(block, "TurnPivotOffset", errors, "a number", parse_float),
        preferred_height_damping: scalar(
            block,
            "PreferredHeightDamping",
            errors,
            "a number",
            parse_float,
        ),
    }
}

#[derive(Debug, Clone, Default)]
pub struct LocomotorIndex {
    defs: BTreeMap<String, LocomotorDef>,
    definitions: usize,
    errors: Vec<ValueError>,
}

impl LocomotorIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_document(&mut self, document: &Document) {
        for block in document.roots() {
            self.insert_block(block);
        }
    }

    pub fn insert_block(&mut self, block: &Block) {
        if !block.keyword.eq(LOCOMOTOR_KEYWORD) {
            return;
        }
        let Some(name) = block.name() else {
            return;
        };
        self.definitions = self.definitions.saturating_add(1);
        let errors = ValueErrors::new();
        let def = read_locomotor(block, &errors);
        self.errors.append(&mut errors.take());
        self.defs.insert(name.to_ascii_lowercase(), def);
    }

    #[must_use]
    pub fn errors(&self) -> &[ValueError] {
        &self.errors
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&LocomotorDef> {
        self.defs.get(&name.to_ascii_lowercase())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    #[must_use]
    pub fn definitions(&self) -> usize {
        self.definitions
    }
}

#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]
mod tests {
    use super::*;
    use crate::table::BlockTable;

    #[test]
    fn a_locomotor_reads_its_flat_fields() {
        let doc = crate::tree::Document::parse(
            "locomotor.ini",
            "Locomotor HumanLocomotor\n  \
                Surfaces = GROUND RUBBLE\n  \
                ZAxisBehavior = NO_Z_MOTIVE_FORCE\n  \
                TurnTime = 500\n  \
                Braking = 510\n  \
                Acceleration = 510\n  \
                MinTurnSpeed = 0%\n  \
                StickToGround = Yes\n  \
                TurnThreshold = 5\n\
            End\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let def = read_locomotor(&doc.roots()[0], &ValueErrors::new());
        assert_eq!(def.surfaces, vec!["GROUND", "RUBBLE"]);
        assert_eq!(def.z_axis_behavior.as_deref(), Some("NO_Z_MOTIVE_FORCE"));
        assert_eq!(def.braking, Some(510.0));
        assert_eq!(def.acceleration, Some(510.0));
        assert_eq!(def.turn_time, Some(Duration::Unknown(500.0)));
        assert_eq!(def.min_turn_speed, Some(0.0));
        assert_eq!(def.stick_to_ground, Some(true));
        assert_eq!(def.turn_threshold, Some(5.0));
    }

    #[test]
    fn a_value_that_does_not_read_is_refused_with_where_it_was_written() {
        for (line, field, value) in [
            ("Braking = sideways", "Braking", "sideways"),
            ("StickToGround = maybe", "StickToGround", "maybe"),
            ("Surfaces = GROUND LAVA", "Surfaces", "LAVA"),
        ] {
            let doc = crate::tree::Document::parse(
                "locomotor.ini",
                &format!(
                    "Locomotor HumanLocomotor
  {line}
End
"
                ),
                &BlockTable::priority_blocks_path(),
            )
            .unwrap();
            let errors = ValueErrors::new();
            let _def = read_locomotor(&doc.roots()[0], &errors);
            let refused = errors.take();
            assert_eq!(refused.len(), 1, "{line}: {refused:?}");
            assert_eq!(
                (
                    refused[0].context.as_str(),
                    refused[0].field.as_str(),
                    refused[0].value.as_str()
                ),
                ("HumanLocomotor", field, value),
                "{line}"
            );
        }
    }

    #[test]
    fn a_field_left_out_of_the_definition_is_none_not_zero() {
        let doc = crate::tree::Document::parse(
            "locomotor.ini",
            "Locomotor Bare\n  Surfaces = GROUND\nEnd\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        let def = read_locomotor(&doc.roots()[0], &ValueErrors::new());
        assert_eq!(def.braking, None);
        assert_eq!(def.has_suspension, None);
        assert_eq!(def.turn_threshold, None);
    }

    #[test]
    fn flight_and_charge_fields_read_as_written() {
        let doc = crate::tree::Document::parse(
            "locomotor.ini",
            "Locomotor FellBeastLocomotor\n  \
                Surfaces = AIR\n  \
                Lift = 100%\n  \
                LiftDamaged = 100%\n  \
                PreferredHeight = 121\n  \
                PreferredAttackHeight = 5\n  \
                AirborneTargetingHeight = 25\n  \
                SwoopStandoffRadius = 310.0\n  \
                SwoopStandoffHeight = 119.0\n  \
                SwoopTerminalVelocity = 0.18\n  \
                SwoopAccelerationRate = 0.012\n\
            End\n\
            Locomotor CrebainLocomotor\n  \
                Surfaces = AIR\n  \
                AirborneTargetingHeight = -1\n\
            End\n\
            Locomotor RohirrimLocomotor\n  \
                Surfaces = GROUND\n  \
                ChargeSpeed = 200%\n  \
                ChargeAvailable = Yes\n  \
                ChargeIgnoresCondition = Yes\n\
            End\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        let fell_beast = read_locomotor(&doc.roots()[0], &ValueErrors::new());
        assert_eq!(fell_beast.surfaces, vec!["AIR"]);
        assert_eq!(fell_beast.lift, Some(1.0));
        assert_eq!(fell_beast.preferred_height, Some(121.0));
        assert_eq!(fell_beast.preferred_attack_height, Some(5.0));
        assert_eq!(fell_beast.swoop_standoff_radius, Some(310.0));
        assert_eq!(fell_beast.swoop_terminal_velocity, Some(0.18));

        let crebain = read_locomotor(&doc.roots()[1], &ValueErrors::new());
        assert_eq!(
            crebain.airborne_targeting_height,
            Some(-1.0),
            "a negative height is read as written, not clamped"
        );

        let rohirrim = read_locomotor(&doc.roots()[2], &ValueErrors::new());
        assert_eq!(rohirrim.charge_speed, Some(2.0));
        assert_eq!(rohirrim.charge_available, Some(true));
        assert_eq!(rohirrim.charge_ignores_condition, Some(true));
    }
}
