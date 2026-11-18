use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

const MAX_TRIGGER_WIDTH: f32 = 10_000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AIGateUpdate {
    trigger_width_x: f32,
    trigger_width_y: f32,
}

impl AIGateUpdate {
    #[must_use]
    pub const fn trigger_width_x(self) -> f32 {
        self.trigger_width_x
    }

    #[must_use]
    pub const fn trigger_width_y(self) -> f32 {
        self.trigger_width_y
    }
}

impl Component for AIGateUpdate {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for AIGateUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn ai_gate_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<AIGateUpdate, FieldError> {
    Ok(AIGateUpdate {
        trigger_width_x: width(fields, "TriggerWidthX")?,
        trigger_width_y: width(fields, "TriggerWidthY")?,
    })
}

fn width(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<f32, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(0.0);
    };
    fields
        .field_numeric(keyword)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value.min(MAX_TRIGGER_WIDTH))
        .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a width")))
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

    #[test]
    fn the_shipped_box_reads_both_widths() {
        let fields = TestFields(BTreeMap::from([
            ("TriggerWidthX", "300.0"),
            ("TriggerWidthY", "150.0"),
        ]));
        let module = ai_gate_update_from_fields(&fields, Frame::ZERO).unwrap();
        assert!((module.trigger_width_x() - 300.0).abs() < 1e-6);
        assert!((module.trigger_width_y() - 150.0).abs() < 1e-6);
    }

    #[test]
    fn a_width_that_is_not_a_number_is_refused() {
        let fields = TestFields(BTreeMap::from([("TriggerWidthX", "wide")]));
        assert!(ai_gate_update_from_fields(&fields, Frame::ZERO).is_err());
    }
}
