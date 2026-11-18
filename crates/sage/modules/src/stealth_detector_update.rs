use mint_sim::module::{CapabilityMask, ModuleKind, UpgradeMask};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};
use std::sync::Arc;

use crate::invisibility_update::{flag, number};

#[derive(Debug, PartialEq)]
struct StealthDetectorData {
    detection_rate: LogicFrames,
    detection_range: Option<f32>,
    required_upgrade: UpgradeMask,
    initially_disabled: bool,
    can_detect_while_garrisoned: bool,
    can_detect_while_contained: bool,
    cancel_one_ring_effect: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StealthDetectorUpdate {
    data: Arc<StealthDetectorData>,
}

impl StealthDetectorUpdate {
    #[must_use]
    pub fn detection_rate(&self) -> LogicFrames {
        self.data.detection_rate
    }

    #[must_use]
    pub fn detection_range(&self) -> Option<f32> {
        self.data.detection_range
    }

    #[must_use]
    pub fn required_upgrade(&self) -> &UpgradeMask {
        &self.data.required_upgrade
    }

    #[must_use]
    pub fn initially_disabled(&self) -> bool {
        self.data.initially_disabled
    }

    #[must_use]
    pub fn can_detect_while_garrisoned(&self) -> bool {
        self.data.can_detect_while_garrisoned
    }

    #[must_use]
    pub fn can_detect_while_contained(&self) -> bool {
        self.data.can_detect_while_contained
    }

    #[must_use]
    pub fn cancel_one_ring_effect(&self) -> bool {
        self.data.cancel_one_ring_effect
    }

    #[must_use]
    pub fn mark_frames(&self) -> LogicFrames {
        self.data.detection_rate.saturating_add(LogicFrames::ONE)
    }
}

impl Component for StealthDetectorUpdate {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for StealthDetectorUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn stealth_detector_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<StealthDetectorUpdate, FieldError> {
    let rate_ms = number(fields, "DetectionRate")?;
    let detection_rate = rate_ms.map_or(LogicFrames::ONE, |ms| {
        LogicFrames::from_milliseconds(milliseconds(ms)).max(LogicFrames::ONE)
    });
    Ok(StealthDetectorUpdate {
        data: Arc::new(StealthDetectorData {
            detection_rate,
            detection_range: number(fields, "DetectionRange")?.filter(|range| *range > 0.0),
            required_upgrade: fields.field_upgrade_mask("RequiredUpgrade"),
            initially_disabled: flag(fields, "InitiallyDisabled")?.unwrap_or(false),
            can_detect_while_garrisoned: flag(fields, "CanDetectWhileGarrisoned")?.unwrap_or(true),
            can_detect_while_contained: flag(fields, "CanDetectWhileContained")?.unwrap_or(true),
            cancel_one_ring_effect: flag(fields, "CancelOneRingEffect")?.unwrap_or(false),
        }),
    })
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn milliseconds(value: f32) -> u32 {
    value.min(86_400_000.0) as u32
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Fields(BTreeMap<&'static str, &'static str>);
    impl ModuleFieldSource for Fields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(keyword))
                .map(|(_, value)| *value)
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.field(keyword)
                .map(|value| value.split_whitespace().collect())
                .unwrap_or_default()
        }
        fn field_numeric(&self, keyword: &str) -> Option<f32> {
            self.field(keyword).and_then(|value| value.parse().ok())
        }
    }
    fn fields(pairs: &[(&'static str, &'static str)]) -> Fields {
        Fields(pairs.iter().copied().collect())
    }

    #[test]
    fn a_declaration_without_a_detection_range_asks_for_its_own_vision() {
        let read =
            stealth_detector_update_from_fields(&fields(&[("DetectionRate", "500")]), Frame::ZERO)
                .unwrap();
        assert_eq!(read.detection_range(), None);
        assert_eq!(read.detection_rate(), LogicFrames::from_milliseconds(500));
        assert_eq!(
            read.mark_frames().get(),
            read.detection_rate().get().saturating_add(1)
        );
        let zero = stealth_detector_update_from_fields(
            &fields(&[("DetectionRate", "500"), ("DetectionRange", "0")]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(zero.detection_range(), None);
    }

    #[test]
    fn an_unreadable_gate_is_refused() {
        assert!(stealth_detector_update_from_fields(
            &fields(&[("DetectionRate", "500"), ("InitiallyDisabled", "Sometimes")]),
            Frame::ZERO
        )
        .is_err());
    }
}
