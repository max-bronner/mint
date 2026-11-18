mod citadel_slaughter;
mod horde_garrison;
mod siege;
mod slaughter_horde;
mod transport;
mod tunnel;

pub use citadel_slaughter::{
    citadel_slaughter_horde_contain_from_fields, CitadelSlaughterHordeContain, RingEntry,
};
pub use horde_garrison::{horde_garrison_contain_from_fields, HordeGarrisonContain};
pub use siege::{siege_engine_contain_from_fields, InitialCrew, SiegeEngineContain};
pub use slaughter_horde::{slaughter_horde_contain_from_fields, SlaughterHordeContain};
pub use transport::{
    transport_contain_from_fields, Fade, Grab, Payload, TransportContain, UpgradeCreation,
};
pub use tunnel::{tunnel_contain_from_fields, TunnelContain};

use mint_sim::{FieldError, LogicFrames, ModuleFieldSource, StateHasher};

use crate::effects::{names, Names};
use crate::fire_weapon_update::milliseconds;
use crate::object_filter::ObjectFilter;

pub const MAX_CONTAIN: u32 = 4096;

const MAX_OFFSET: f32 = 100_000.0;

const MAX_EXIT_PATHS: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Offset {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenContain {
    contain_max: u32,
    passenger_filter: ObjectFilter,
    status_of_contained: Names,
    allow_allies_inside: bool,
    allow_enemies_inside: bool,
    allow_neutral_inside: bool,
    allow_own_player_inside_override: Option<bool>,
    damage_percent_to_units: f32,
    show_pips: bool,
    enter_sound: Option<String>,
    exit_sound: Option<String>,
    number_of_exit_paths: u32,
    passenger_bone_prefix: Option<String>,
    passenger_bone_kind: Option<String>,
    kill_passengers_on_death: bool,
    eject_passengers_on_death: bool,
    enabled: bool,
    entry_offset: Offset,
    entry_position: Offset,
    exit_offset: Offset,
    exit_delay: LogicFrames,
    force_orientation: bool,
    passengers_test_collision_height: f32,
}

impl OpenContain {
    #[must_use]
    pub const fn contain_max(&self) -> u32 {
        self.contain_max
    }

    #[must_use]
    pub const fn passenger_filter(&self) -> &ObjectFilter {
        &self.passenger_filter
    }

    #[must_use]
    pub fn status_of_contained(&self) -> &[std::sync::Arc<str>] {
        &self.status_of_contained
    }

    #[must_use]
    pub const fn allow_allies_inside(&self) -> bool {
        self.allow_allies_inside
    }

    #[must_use]
    pub const fn allow_enemies_inside(&self) -> bool {
        self.allow_enemies_inside
    }

    #[must_use]
    pub const fn allow_neutral_inside(&self) -> bool {
        self.allow_neutral_inside
    }

    #[must_use]
    pub const fn allow_own_player_inside(&self) -> bool {
        match self.allow_own_player_inside_override {
            Some(allowed) => allowed,
            None => true,
        }
    }

    #[must_use]
    pub const fn damage_percent_to_units(&self) -> f32 {
        self.damage_percent_to_units
    }

    #[must_use]
    pub const fn show_pips(&self) -> bool {
        self.show_pips
    }

    #[must_use]
    pub fn enter_sound(&self) -> Option<&str> {
        self.enter_sound.as_deref()
    }

    #[must_use]
    pub fn exit_sound(&self) -> Option<&str> {
        self.exit_sound.as_deref()
    }

    #[must_use]
    pub const fn number_of_exit_paths(&self) -> u32 {
        self.number_of_exit_paths
    }

    #[must_use]
    pub fn passenger_bone_prefix(&self) -> Option<&str> {
        self.passenger_bone_prefix.as_deref()
    }

    #[must_use]
    pub fn passenger_bone_kind(&self) -> Option<&str> {
        self.passenger_bone_kind.as_deref()
    }

    #[must_use]
    pub const fn kill_passengers_on_death(&self) -> bool {
        self.kill_passengers_on_death
    }

    #[must_use]
    pub const fn eject_passengers_on_death(&self) -> bool {
        self.eject_passengers_on_death
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn entry_offset(&self) -> Offset {
        self.entry_offset
    }

    #[must_use]
    pub const fn entry_position(&self) -> Offset {
        self.entry_position
    }

    #[must_use]
    pub const fn exit_offset(&self) -> Offset {
        self.exit_offset
    }

    #[must_use]
    pub const fn exit_delay(&self) -> LogicFrames {
        self.exit_delay
    }

    #[must_use]
    pub const fn forces_orientation(&self) -> bool {
        self.force_orientation
    }

    #[must_use]
    pub const fn passengers_test_collision_height(&self) -> f32 {
        self.passengers_test_collision_height
    }

    pub(crate) fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.contain_max);
        hasher.write_u32(self.number_of_exit_paths);
        hasher.write_f32(self.damage_percent_to_units);
        hasher.write_bool(self.kill_passengers_on_death);
        hasher.write_bool(self.eject_passengers_on_death);
        hasher.write_bool(self.enabled);
        hasher.write_bool(self.force_orientation);
        hasher.write_f32(self.passengers_test_collision_height);
        for point in [self.entry_offset, self.entry_position, self.exit_offset] {
            hasher.write_f32(point.x);
            hasher.write_f32(point.y);
        }
        hasher.write_u32(self.exit_delay.get());
    }
}

pub fn open_contain_from_fields(fields: &dyn ModuleFieldSource) -> Result<OpenContain, FieldError> {
    let (bone_prefix, bone_kind) = passenger_bones(fields);
    Ok(OpenContain {
        contain_max: count(fields, "ContainMax", 0, MAX_CONTAIN)?,
        passenger_filter: ObjectFilter::parse(&owned(fields.field_args("PassengerFilter"))),
        status_of_contained: names(owned(fields.field_args("ObjectStatusOfContained"))),
        allow_allies_inside: boolean(fields, "AllowAlliesInside")?.unwrap_or(true),
        allow_enemies_inside: boolean(fields, "AllowEnemiesInside")?.unwrap_or(true),
        allow_neutral_inside: boolean(fields, "AllowNeutralInside")?.unwrap_or(true),
        allow_own_player_inside_override: boolean(fields, "AllowOwnPlayerInsideOverride")?,
        damage_percent_to_units: percent(fields, "DamagePercentToUnits")?,
        show_pips: boolean(fields, "ShowPips")?.unwrap_or(true),
        enter_sound: fields.field("EnterSound").map(str::to_owned),
        exit_sound: fields.field("ExitSound").map(str::to_owned),
        number_of_exit_paths: count(fields, "NumberOfExitPaths", 1, MAX_EXIT_PATHS)?,
        passenger_bone_prefix: bone_prefix,
        passenger_bone_kind: bone_kind,
        kill_passengers_on_death: boolean(fields, "KillPassengersOnDeath")?.unwrap_or(false),
        eject_passengers_on_death: boolean(fields, "EjectPassengersOnDeath")?.unwrap_or(true),
        enabled: boolean(fields, "Enabled")?.unwrap_or(true),
        entry_offset: offset(fields, "EntryOffset")?,
        entry_position: offset(fields, "EntryPosition")?,
        exit_offset: offset(fields, "ExitOffset")?,
        exit_delay: milliseconds(fields, "ExitDelay"),
        force_orientation: boolean(fields, "ForceOrientationContainer")?.unwrap_or(true),
        passengers_test_collision_height: match fields.field("PassengersTestCollisionHeight") {
            None => -1000.0,
            Some(raw) => fields
                .field_numeric("PassengersTestCollisionHeight")
                .filter(|v| v.is_finite())
                .ok_or_else(|| {
                    FieldError::new(format!(
                        "PassengersTestCollisionHeight '{raw}' is not a number"
                    ))
                })?,
        },
    })
}

fn owned(args: Vec<&str>) -> Vec<String> {
    args.into_iter().map(str::to_owned).collect()
}

fn passenger_bones(fields: &dyn ModuleFieldSource) -> (Option<String>, Option<String>) {
    let mut prefix = None;
    let mut kind = None;
    for token in fields.field_args("PassengerBonePrefix") {
        match token.split_once(':') {
            Some((tag, value)) if tag.eq_ignore_ascii_case("PassengerBone") => {
                prefix = Some(value.to_owned());
            }
            Some((tag, value)) if tag.eq_ignore_ascii_case("KindOf") => {
                kind = Some(value.to_owned());
            }
            _ => {}
        }
    }
    (prefix, kind)
}

fn count(
    fields: &dyn ModuleFieldSource,
    keyword: &str,
    default: u32,
    cap: u32,
) -> Result<u32, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(default);
    };
    #[allow(clippy::cast_precision_loss)]
    let ceiling = cap as f32;
    let value = fields
        .field_numeric(keyword)
        .filter(|v| (0.0..=ceiling).contains(v))
        .ok_or_else(|| {
            FieldError::new(format!("{keyword} '{raw}' is not a count of 0 to {cap}"))
        })?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(value as u32)
}

fn percent(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<f32, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(0.0);
    };
    fields
        .field_percent(keyword)
        .filter(|v| v.is_finite())
        .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a percentage")))
}

fn boolean(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Option<bool>, FieldError> {
    match fields.field(keyword) {
        None => Ok(None),
        Some(raw) => mint_sage_ini::value::parse_bool(raw)
            .map(Some)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No"))),
    }
}

fn offset(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Offset, FieldError> {
    let args = fields.field_args(keyword);
    if args.is_empty() {
        return Ok(Offset::default());
    }
    let bad = || {
        FieldError::new(format!(
            "{keyword} '{}' is not X:.. Y:.. Z:..",
            args.join(" ")
        ))
    };
    let mut found = [None; 3];
    let mut tokens = args.iter();
    while let Some(token) = tokens.next() {
        let (tag, value) = token.split_once(':').ok_or_else(bad)?;
        let slot = match tag.to_ascii_uppercase().as_str() {
            "X" => 0,
            "Y" => 1,
            "Z" => 2,
            _ => return Err(bad()),
        };
        let raw = if value.is_empty() {
            tokens.next().copied().ok_or_else(bad)?
        } else {
            value
        };
        let number = raw
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && v.abs() <= MAX_OFFSET)
            .ok_or_else(bad)?;
        match found.get_mut(slot) {
            Some(cell @ None) => *cell = Some(number),
            _ => return Err(bad()),
        }
    }
    let [x, y, _] = found;
    Ok(Offset {
        x: x.unwrap_or(0.0),
        y: y.unwrap_or(0.0),
    })
}

fn cash_back_percent(fields: &dyn ModuleFieldSource) -> Result<f32, FieldError> {
    let value = percent(fields, "CashBackPercent")?;
    if !(0.0..=100.0).contains(&value) {
        return Err(FieldError::new(format!(
            "CashBackPercent '{}' is out of range",
            fields.field("CashBackPercent").unwrap_or_default()
        )));
    }
    Ok(value)
}

#[cfg(test)]
mod tests;
