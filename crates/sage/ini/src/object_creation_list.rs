use std::collections::BTreeMap;

use crate::diagnostic::ValueErrors;
use crate::tree::{Block, Document};
use crate::value::{parse_bool, parse_int, tagged_value};

pub const OBJECT_CREATION_LIST_KEYWORD: &str = "ObjectCreationList";

pub const CREATE_OBJECT_KEYWORD: &str = "CreateObject";

pub const CREATE_DEBRIS_KEYWORD: &str = "CreateDebris";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CreationEntry {
    pub names: Vec<String>,
    pub offset: [f32; 3],
    pub count: Option<i64>,
    pub disposition: Vec<String>,
    pub disposition_angle: Option<f32>,
    pub disposition_intensity: Option<f32>,
    pub velocity_scale: Option<f32>,
    pub ignore_command_point_limit: bool,
    pub use_just_built_flag: bool,
    pub starting_busy_time_ms: Option<f32>,
    pub force_magnitude: [Option<f32>; 2],
    pub force_pitch_degrees: [Option<f32>; 2],
    pub clear_removables: bool,
    pub invulnerable_time_ms: Option<f32>,
    pub spread_formation: bool,
    pub min_distance_formation: [Option<f32>; 2],
    pub max_distance_formation: Option<f32>,
    pub ignore_all_objects: bool,
    pub orient_in_primary_direction: bool,
    pub orient_in_secondary_direction: bool,
    pub orientation_offset_degrees: Option<f32>,
    pub required_upgrades: Vec<String>,
    pub forbidden_upgrades: Vec<String>,
}

fn flag(block: &Block, keyword: &str, context: &str, errors: &ValueErrors) -> bool {
    let Some(field) = block.field(keyword) else {
        return false;
    };
    let read = field.value().and_then(parse_bool);
    if read.is_none() {
        errors.field(context, field, "Yes or No");
    }
    read.unwrap_or(false)
}

fn read_entry(
    block: &Block,
    names_keyword: &str,
    context: &str,
    errors: &ValueErrors,
) -> CreationEntry {
    let offset_args = block
        .field("Offset")
        .map(|f| f.args.clone())
        .unwrap_or_default();
    let axis = |tag: &str| tagged_value(&offset_args, tag).and_then(crate::value::parse_float);
    let real = |keyword: &str| {
        block
            .field(keyword)
            .and_then(crate::tree::Field::value)
            .and_then(crate::value::parse_float)
    };
    let names = |keyword: &str| {
        block
            .field(keyword)
            .map(|f| f.args.clone())
            .unwrap_or_default()
    };
    CreationEntry {
        names: block
            .field(names_keyword)
            .map(|f| f.args.clone())
            .unwrap_or_default(),
        offset: [
            axis("X").unwrap_or(0.0),
            axis("Y").unwrap_or(0.0),
            axis("Z").unwrap_or(0.0),
        ],
        count: block
            .field("Count")
            .and_then(crate::tree::Field::value)
            .and_then(parse_int),
        disposition: block
            .field("Disposition")
            .map(|f| f.args.clone())
            .unwrap_or_default(),
        disposition_angle: block
            .field("DispositionAngle")
            .and_then(crate::tree::Field::value)
            .and_then(crate::value::parse_float),
        disposition_intensity: block
            .field("DispositionIntensity")
            .and_then(crate::tree::Field::value)
            .and_then(crate::value::parse_float),
        velocity_scale: block
            .field("VelocityScale")
            .and_then(crate::tree::Field::value)
            .and_then(crate::value::parse_float),
        ignore_command_point_limit: block
            .field("IgnoreCommandPointLimit")
            .and_then(crate::tree::Field::value)
            .and_then(crate::value::parse_bool)
            .unwrap_or(false),
        use_just_built_flag: block
            .field("UseJustBuiltFlag")
            .and_then(crate::tree::Field::value)
            .and_then(crate::value::parse_bool)
            .unwrap_or(false),
        starting_busy_time_ms: block
            .field("StartingBusyTime")
            .and_then(crate::tree::Field::value)
            .and_then(crate::value::parse_float),
        force_magnitude: [real("MinForceMagnitude"), real("MaxForceMagnitude")],
        force_pitch_degrees: [real("MinForcePitch"), real("MaxForcePitch")],
        clear_removables: flag(block, "ClearRemovables", context, errors),
        invulnerable_time_ms: real("InvulnerableTime"),
        spread_formation: flag(block, "SpreadFormation", context, errors),
        min_distance_formation: [real("MinDistanceAFormation"), real("MinDistanceBFormation")],
        max_distance_formation: real("MaxDistanceFormation"),
        ignore_all_objects: flag(block, "IgnoreAllObjects", context, errors),
        orient_in_primary_direction: flag(block, "OrientInPrimaryDirection", context, errors),
        orient_in_secondary_direction: flag(block, "OrientInSecondaryDirection", context, errors),
        orientation_offset_degrees: real("OrientationOffset"),
        required_upgrades: names("RequiredUpgrades"),
        forbidden_upgrades: names("ForbiddenUpgrades"),
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ObjectCreationListDef {
    pub create_object: Vec<CreationEntry>,
    pub create_debris: Vec<CreationEntry>,
}

#[must_use]
pub fn read_object_creation_list(block: &Block, errors: &ValueErrors) -> ObjectCreationListDef {
    let context = block.name().unwrap_or(OBJECT_CREATION_LIST_KEYWORD);
    let mut def = ObjectCreationListDef::default();
    for nugget in block.blocks() {
        if nugget.keyword.eq(CREATE_OBJECT_KEYWORD) {
            def.create_object
                .push(read_entry(nugget, "ObjectNames", context, errors));
        } else if nugget.keyword.eq(CREATE_DEBRIS_KEYWORD) {
            def.create_debris
                .push(read_entry(nugget, "ModelNames", context, errors));
        }
    }
    def
}

#[derive(Debug, Clone, Default)]
pub struct ObjectCreationListIndex {
    defs: BTreeMap<String, ObjectCreationListDef>,
    definitions: usize,
}

impl ObjectCreationListIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_document(&mut self, document: &Document, errors: &ValueErrors) {
        for block in document.roots() {
            self.insert_block(block, errors);
        }
    }

    pub fn insert_block(&mut self, block: &Block, errors: &ValueErrors) {
        if !block.keyword.eq(OBJECT_CREATION_LIST_KEYWORD) {
            return;
        }
        let Some(name) = block.name() else {
            return;
        };
        self.definitions = self.definitions.saturating_add(1);
        self.defs.insert(
            name.to_ascii_lowercase(),
            read_object_creation_list(block, errors),
        );
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ObjectCreationListDef> {
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

    pub fn iter(&self) -> impl Iterator<Item = &ObjectCreationListDef> {
        self.defs.values()
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
    fn create_object_and_create_debris_nuggets_are_both_read() {
        let doc = crate::tree::Document::parse(
            "objectcreationlist.ini",
            "ObjectCreationList OCL_Test\n  \
                CreateObject\n    \
                    ObjectNames = Chicken\n    \
                    Offset = X:1.0 Y:2.0 Z:3.0\n    \
                    Count = 1\n    \
                    Disposition = SPAWN_AROUND\n    \
                    DispositionAngle = 72\n    \
                    DispositionIntensity = 300\n  \
                End\n  \
                CreateDebris\n    \
                    ModelNames = Debris01\n    \
                    Count = 2\n  \
                End\n\
            End\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let errors = ValueErrors::new();
        let def = read_object_creation_list(&doc.roots()[0], &errors);
        assert!(errors.is_empty());
        assert_eq!(def.create_object.len(), 1);
        assert_eq!(def.create_object[0].names, vec!["Chicken"]);
        assert_eq!(def.create_object[0].offset, [1.0, 2.0, 3.0]);
        assert_eq!(def.create_object[0].count, Some(1));
        assert_eq!(def.create_object[0].disposition, vec!["SPAWN_AROUND"]);
        assert_eq!(def.create_object[0].disposition_angle, Some(72.0));
        assert_eq!(def.create_object[0].disposition_intensity, Some(300.0));
        assert_eq!(def.create_debris.len(), 1);
        assert_eq!(def.create_debris[0].names, vec!["Debris01"]);
        assert_eq!(def.create_debris[0].count, Some(2));
    }

    fn one_create_object(clear_removables: &str) -> (ObjectCreationListDef, ValueErrors) {
        let doc = crate::tree::Document::parse(
            "objectcreationlist.ini",
            &format!(
                "ObjectCreationList OCL_Test\n  \
                    CreateObject\n    \
                        ObjectNames = Chicken\n    \
                        {clear_removables}\n  \
                    End\n\
                End\n"
            ),
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        let errors = ValueErrors::new();
        let def = read_object_creation_list(&doc.roots()[0], &errors);
        (def, errors)
    }

    #[test]
    fn clear_removables_absent_reads_no() {
        let (def, errors) = one_create_object("");
        assert!(errors.is_empty());
        assert!(!def.create_object[0].clear_removables);
    }

    #[test]
    fn clear_removables_yes_reads_true() {
        let (def, errors) = one_create_object("ClearRemovables = Yes");
        assert!(errors.is_empty());
        assert!(def.create_object[0].clear_removables);
    }

    #[test]
    fn clear_removables_an_unreadable_value_is_refused() {
        let (def, errors) = one_create_object("ClearRemovables = Sideways");
        assert!(!def.create_object[0].clear_removables);
        assert_eq!(errors.len(), 1);
    }
}
