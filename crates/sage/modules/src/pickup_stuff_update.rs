use mint_sage_ini::value::parse_bool;
use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

use crate::object_filter::ObjectFilter;

#[derive(Debug, Clone, PartialEq)]
pub struct PickupStuffUpdate {
    skirmish_ai_only: bool,
    scan_range: f32,
    scan_interval_seconds: f32,
    stuff_to_pick_up: ObjectFilter,
}

impl PickupStuffUpdate {
    #[must_use]
    pub const fn skirmish_ai_only(&self) -> bool {
        self.skirmish_ai_only
    }

    #[must_use]
    pub const fn scan_range(&self) -> f32 {
        self.scan_range
    }

    #[must_use]
    pub const fn scan_interval_seconds(&self) -> f32 {
        self.scan_interval_seconds
    }

    #[must_use]
    pub const fn stuff_to_pick_up(&self) -> &ObjectFilter {
        &self.stuff_to_pick_up
    }
}

impl Component for PickupStuffUpdate {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for PickupStuffUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn pickup_stuff_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<PickupStuffUpdate, FieldError> {
    let skirmish_ai_only = match fields.field("SkirmishAIOnly") {
        None => false,
        Some(raw) => parse_bool(raw)
            .ok_or_else(|| FieldError::new(format!("SkirmishAIOnly '{raw}' is not Yes/No")))?,
    };
    Ok(PickupStuffUpdate {
        skirmish_ai_only,
        scan_range: number(fields, "ScanRange")?,
        scan_interval_seconds: number(fields, "ScanIntervalSeconds")?,
        stuff_to_pick_up: ObjectFilter::parse(
            &fields
                .field_args("StuffToPickUp")
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<String>>(),
        ),
    })
}

fn number(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<f32, FieldError> {
    match fields.field(keyword) {
        None => Ok(0.0),
        Some(raw) => fields
            .field_numeric(keyword)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a number"))),
    }
}
