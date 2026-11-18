use std::sync::Arc;

use mint_sage_ini::value::parse_bool;
use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

const MAX_MILLISECONDS: f32 = 86_400_000.0;

#[derive(Debug, PartialEq)]
struct Data {
    open_by_default: bool,
    reset_time: LogicFrames,
    percent_open_for_pathing: f32,
    repel_colliding_units: bool,
    sound_opening_loop: Option<String>,
    sound_closing_loop: Option<String>,
    sound_finished_opening: Option<String>,
    sound_finished_closing: Option<String>,
    time_before_open_sound: LogicFrames,
    time_before_closed_sound: LogicFrames,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GateOpenAndCloseBehavior {
    data: Arc<Data>,
}

impl GateOpenAndCloseBehavior {
    #[must_use]
    pub fn open_by_default(&self) -> bool {
        self.data.open_by_default
    }

    #[must_use]
    pub fn reset_time(&self) -> LogicFrames {
        self.data.reset_time
    }

    #[must_use]
    pub fn percent_open_for_pathing(&self) -> f32 {
        self.data.percent_open_for_pathing
    }

    #[must_use]
    pub fn repel_colliding_units(&self) -> bool {
        self.data.repel_colliding_units
    }

    #[must_use]
    pub fn sound_opening_loop(&self) -> Option<&str> {
        self.data.sound_opening_loop.as_deref()
    }

    #[must_use]
    pub fn sound_closing_loop(&self) -> Option<&str> {
        self.data.sound_closing_loop.as_deref()
    }

    #[must_use]
    pub fn sound_finished_opening(&self) -> Option<&str> {
        self.data.sound_finished_opening.as_deref()
    }

    #[must_use]
    pub fn sound_finished_closing(&self) -> Option<&str> {
        self.data.sound_finished_closing.as_deref()
    }

    #[must_use]
    pub fn time_before_open_sound(&self) -> LogicFrames {
        self.data.time_before_open_sound
    }

    #[must_use]
    pub fn time_before_closed_sound(&self) -> LogicFrames {
        self.data.time_before_closed_sound
    }
}

impl Component for GateOpenAndCloseBehavior {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for GateOpenAndCloseBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn gate_open_and_close_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<GateOpenAndCloseBehavior, FieldError> {
    Ok(GateOpenAndCloseBehavior {
        data: Arc::new(Data {
            open_by_default: boolean(fields, "OpenByDefault")?.unwrap_or(false),
            reset_time: duration(fields, "ResetTimeInMilliseconds")?,
            percent_open_for_pathing: percentage(fields, "PercentOpenForPathing")?.unwrap_or(0.0),
            repel_colliding_units: boolean(fields, "RepelCollidingUnits")?.unwrap_or(false),
            sound_opening_loop: name(fields, "SoundOpeningGateLoop"),
            sound_closing_loop: name(fields, "SoundClosingGateLoop"),
            sound_finished_opening: name(fields, "SoundFinishedOpeningGate"),
            sound_finished_closing: name(fields, "SoundFinishedClosingGate"),
            time_before_open_sound: duration(fields, "TimeBeforePlayingOpenSound")?,
            time_before_closed_sound: duration(fields, "TimeBeforePlayingClosedSound")?,
        }),
    })
}

fn name(fields: &dyn ModuleFieldSource, keyword: &str) -> Option<String> {
    fields.field(keyword).map(str::to_owned)
}

pub(crate) fn boolean(
    fields: &dyn ModuleFieldSource,
    keyword: &str,
) -> Result<Option<bool>, FieldError> {
    match fields.field(keyword) {
        None => Ok(None),
        Some(raw) => parse_bool(raw)
            .map(Some)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No"))),
    }
}

fn duration(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<LogicFrames, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(LogicFrames::ZERO);
    };
    let ms = fields
        .field_numeric(keyword)
        .filter(|ms| ms.is_finite())
        .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not milliseconds")))?;
    if ms <= 0.0 {
        return Ok(LogicFrames::ZERO);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = ms.min(MAX_MILLISECONDS) as u32;
    Ok(LogicFrames::from_milliseconds(whole))
}

fn percentage(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Option<f32>, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(None);
    };
    fields
        .field_percent(keyword)
        .filter(|fraction| fraction.is_finite() && (0.0..=1.0).contains(fraction))
        .map(Some)
        .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a percentage")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct TestFields(BTreeMap<&'static str, &'static str>);
    impl ModuleFieldSource for TestFields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| *v)
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.field(keyword).into_iter().collect()
        }
    }

    fn shipped() -> TestFields {
        TestFields(BTreeMap::from([
            ("ResetTimeInMilliseconds", "3000"),
            ("OpenByDefault", "No"),
            ("PercentOpenForPathing", "50"),
            ("SoundOpeningGateLoop", "GateOpenStart"),
            ("SoundClosingGateLoop", "GateCloseStart"),
            ("SoundFinishedOpeningGate", "GateOpenEnd"),
            ("SoundFinishedClosingGate", "GateCloseEnd"),
            ("TimeBeforePlayingOpenSound", "9500"),
            ("TimeBeforePlayingClosedSound", "9500"),
        ]))
    }

    #[test]
    fn the_shipped_shape_reads_its_cycle_and_its_sounds() {
        let module = gate_open_and_close_behavior_from_fields(&shipped(), Frame::ZERO).unwrap();
        assert!(!module.open_by_default());
        assert_eq!(module.reset_time().get(), 90);
        assert!((module.percent_open_for_pathing() - 0.5).abs() < 1e-6);
        assert_eq!(module.sound_finished_opening(), Some("GateOpenEnd"));
    }

    #[test]
    fn a_wrong_typed_value_is_refused_rather_than_read_as_a_default() {
        let mut fields = shipped();
        fields.0.insert("OpenByDefault", "Maybe");
        assert!(gate_open_and_close_behavior_from_fields(&fields, Frame::ZERO).is_err());

        let mut fields = shipped();
        fields.0.insert("ResetTimeInMilliseconds", "soon");
        assert!(gate_open_and_close_behavior_from_fields(&fields, Frame::ZERO).is_err());
    }
}
