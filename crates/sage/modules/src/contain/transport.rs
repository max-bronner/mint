use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

use super::{
    boolean, count, milliseconds, open_contain_from_fields, owned, OpenContain, MAX_CONTAIN,
};
use crate::object_filter::ObjectFilter;

#[derive(Debug, Clone, PartialEq)]
pub struct Payload {
    pub template: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpgradeCreation {
    pub upgrade: String,
    pub template: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Grab {
    pub weapon: String,
    pub structures: bool,
    pub on_victim: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fade {
    pub filter: ObjectFilter,
    pub on_enter: bool,
    pub on_exit: bool,
    pub enter_time: LogicFrames,
    pub exit_time: LogicFrames,
    pub reverse: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransportContain {
    open: OpenContain,
    slots: u32,
    initial_payload: Option<Payload>,
    upgrade_creations: Vec<UpgradeCreation>,
    grab: Option<Grab>,
    manual_pick_up_filter: Option<ObjectFilter>,
    collide_pickup: bool,
    weapon_set_kinds: [Option<String>; 2],
    weapon_state_kinds: [Option<String>; 3],
    fade: Fade,
}

impl TransportContain {
    #[must_use]
    pub const fn open(&self) -> &OpenContain {
        &self.open
    }

    #[must_use]
    pub const fn slots(&self) -> u32 {
        self.slots
    }

    #[must_use]
    pub const fn initial_payload(&self) -> Option<&Payload> {
        self.initial_payload.as_ref()
    }

    #[must_use]
    pub fn upgrade_creations(&self) -> &[UpgradeCreation] {
        &self.upgrade_creations
    }

    #[must_use]
    pub const fn grab(&self) -> Option<&Grab> {
        self.grab.as_ref()
    }

    #[must_use]
    pub const fn manual_pick_up_filter(&self) -> Option<&ObjectFilter> {
        self.manual_pick_up_filter.as_ref()
    }

    #[must_use]
    pub const fn collide_pickup(&self) -> bool {
        self.collide_pickup
    }

    #[must_use]
    pub fn weapon_set_kind(&self, ordinal: usize) -> Option<&str> {
        self.weapon_set_kinds.get(ordinal)?.as_deref()
    }

    #[must_use]
    pub fn weapon_state_kind(&self, ordinal: usize) -> Option<&str> {
        self.weapon_state_kinds.get(ordinal)?.as_deref()
    }

    #[must_use]
    pub const fn fade(&self) -> &Fade {
        &self.fade
    }
}

impl Component for TransportContain {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.open.hash_into(hasher);
        hasher.write_u32(self.slots);
        hasher.write_u32(
            self.initial_payload
                .as_ref()
                .map_or(0, |payload| payload.count),
        );
        hasher.write_usize(self.upgrade_creations.len());
        hasher.write_bool(self.collide_pickup);
    }
}

impl ModuleKind for TransportContain {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

fn payload(args: &[&str], keyword: &str) -> Result<Option<(String, u32)>, FieldError> {
    let Some(template) = args.first() else {
        return Ok(None);
    };
    if template.eq_ignore_ascii_case("NONE") {
        return Ok(None);
    }
    let count = match args.get(1) {
        None => 1,
        Some(raw) => raw
            .parse::<u32>()
            .ok()
            .filter(|count| *count <= MAX_CONTAIN)
            .ok_or_else(|| {
                FieldError::new(format!(
                    "{keyword} '{raw}' is not a count of 0 to {MAX_CONTAIN}"
                ))
            })?,
    };
    Ok(Some(((*template).to_owned(), count)))
}

pub fn transport_contain_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<TransportContain, FieldError> {
    let mut upgrade_creations = Vec::new();
    for args in fields.field_args_each("UpgradeCreationTrigger") {
        let (Some(upgrade), rest) = (args.first(), args.get(1..).unwrap_or_default()) else {
            return Err(FieldError::new(
                "UpgradeCreationTrigger names no upgrade".to_owned(),
            ));
        };
        let Some((template, count)) = payload(rest, "UpgradeCreationTrigger")? else {
            return Err(FieldError::new(format!(
                "UpgradeCreationTrigger '{upgrade}' names no template"
            )));
        };
        upgrade_creations.push(UpgradeCreation {
            upgrade: (*upgrade).to_owned(),
            template,
            count,
        });
    }
    let grab = fields.field("GrabWeapon").map(str::to_owned);
    let grab = match grab {
        None => None,
        Some(weapon) => Some(Grab {
            weapon,
            structures: boolean(fields, "CanGrabStructure")?.unwrap_or(false),
            on_victim: boolean(fields, "FireGrabWeaponOnVictim")?.unwrap_or(false),
        }),
    };
    let kind_of = |keyword: &str| fields.field(keyword).map(str::to_owned);
    let filter = |keyword: &str| {
        let args = fields.field_args(keyword);
        (!args.is_empty()).then(|| ObjectFilter::parse(&owned(args)))
    };
    Ok(TransportContain {
        open: open_contain_from_fields(fields)?,
        slots: count(fields, "Slots", 0, MAX_CONTAIN)?,
        initial_payload: payload(&fields.field_args("InitialPayload"), "InitialPayload")?
            .map(|(template, count)| Payload { template, count }),
        upgrade_creations,
        grab,
        manual_pick_up_filter: filter("ManualPickUpFilter"),
        collide_pickup: boolean(fields, "CollidePickup")?.unwrap_or(true),
        weapon_set_kinds: [
            kind_of("TypeOneForWeaponSet"),
            kind_of("TypeTwoForWeaponSet"),
        ],
        weapon_state_kinds: [
            kind_of("TypeOneForWeaponState"),
            kind_of("TypeTwoForWeaponState"),
            kind_of("TypeThreeForWeaponState"),
        ],
        fade: Fade {
            filter: ObjectFilter::parse(&owned(fields.field_args("FadeFilter"))),
            on_enter: boolean(fields, "FadePassengerOnEnter")?.unwrap_or(false),
            on_exit: boolean(fields, "FadePassengerOnExit")?.unwrap_or(false),
            enter_time: milliseconds(fields, "EnterFadeTime"),
            exit_time: milliseconds(fields, "ExitFadeTime"),
            reverse: boolean(fields, "FadeReverse")?.unwrap_or(false),
        },
    })
}
