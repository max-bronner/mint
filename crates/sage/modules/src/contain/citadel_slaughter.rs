use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

use super::{boolean, cash_back_percent, milliseconds, open_contain_from_fields, OpenContain};
use crate::effects::{names, Name, Names};
use crate::object_filter::ObjectFilter;

#[derive(Debug, Clone, PartialEq)]
pub struct RingEntry {
    status: Name,
    to_destroy: ObjectFilter,
    upgrades: Names,
    fx: Option<Name>,
}

impl RingEntry {
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    #[must_use]
    pub const fn to_destroy(&self) -> &ObjectFilter {
        &self.to_destroy
    }

    #[must_use]
    pub fn upgrades(&self) -> &[Name] {
        &self.upgrades
    }

    #[must_use]
    pub fn fx(&self) -> Option<&str> {
        self.fx.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CitadelSlaughterHordeContain {
    open: OpenContain,
    cash_back_percent: f32,
    can_always_enter: bool,
    status_required_for_can_always_enter: Names,
    heal_objects: bool,
    time_for_full_heal: LogicFrames,
    ring: Option<RingEntry>,
}

impl CitadelSlaughterHordeContain {
    #[must_use]
    pub const fn open(&self) -> &OpenContain {
        &self.open
    }

    #[must_use]
    pub const fn cash_back_percent(&self) -> f32 {
        self.cash_back_percent
    }

    #[must_use]
    pub const fn can_always_enter(&self) -> bool {
        self.can_always_enter
    }

    #[must_use]
    pub fn status_required_for_can_always_enter(&self) -> &[Name] {
        &self.status_required_for_can_always_enter
    }

    #[must_use]
    pub const fn heal_objects(&self) -> bool {
        self.heal_objects
    }

    #[must_use]
    pub const fn time_for_full_heal(&self) -> LogicFrames {
        self.time_for_full_heal
    }

    #[must_use]
    pub const fn ring(&self) -> Option<&RingEntry> {
        self.ring.as_ref()
    }
}

impl Component for CitadelSlaughterHordeContain {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.open.hash_into(hasher);
        hasher.write_f32(self.cash_back_percent);
        hasher.write_bool(self.can_always_enter);
        hasher.write_bool(self.ring.is_some());
    }
}

impl ModuleKind for CitadelSlaughterHordeContain {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn citadel_slaughter_horde_contain_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<CitadelSlaughterHordeContain, FieldError> {
    Ok(CitadelSlaughterHordeContain {
        open: open_contain_from_fields(fields)?,
        cash_back_percent: cash_back_percent(fields)?,
        can_always_enter: boolean(fields, "CanAlwaysEnter")?.unwrap_or(false),
        status_required_for_can_always_enter: names(
            fields
                .field_args("StatusRequiredForCanAlwaysEnter")
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
        heal_objects: boolean(fields, "HealObjects")?.unwrap_or(false),
        time_for_full_heal: milliseconds(fields, "TimeForFullHeal"),
        ring: ring_entry(fields),
    })
}

fn ring_entry(fields: &dyn ModuleFieldSource) -> Option<RingEntry> {
    let status: Name = fields.field("StatusForRingEntry")?.into();
    Some(RingEntry {
        status,
        to_destroy: ObjectFilter::parse(
            &fields
                .field_args("ObjectToDestroyForRingEntry")
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<String>>(),
        ),
        upgrades: names(
            fields
                .field_args("UpgradeForRingEntry")
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
        fx: fields.field("FXForRingEntry").map(Into::into),
    })
}
