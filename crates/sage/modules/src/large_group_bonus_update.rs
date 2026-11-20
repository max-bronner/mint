use std::sync::Arc;

use mint_sage_ini::value::parse_bool;
use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, Entity, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

use crate::object_filter::ObjectFilter;

const DEFAULT_RATE_MS: u32 = 1000;

#[derive(Debug, PartialEq)]
struct GroupBonusData {
    cadence: LogicFrames,
    member_filter: ObjectFilter,
    count: u32,
    radius: f32,
    rub_off_radius: f32,
    allies_only: bool,
    attribute_modifier: Option<String>,
    flag_sub_object_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LargeGroupBonusUpdate {
    data: Arc<GroupBonusData>,
    next_scan: Frame,
    started: bool,
}

impl LargeGroupBonusUpdate {
    #[must_use]
    pub fn cadence(&self) -> LogicFrames {
        self.data.cadence
    }

    #[must_use]
    pub fn member_filter(&self) -> &ObjectFilter {
        &self.data.member_filter
    }

    #[must_use]
    pub fn count(&self) -> u32 {
        self.data.count
    }

    #[must_use]
    pub fn radius(&self) -> f32 {
        self.data.radius
    }

    #[must_use]
    pub fn rub_off_radius(&self) -> f32 {
        self.data.rub_off_radius
    }

    #[must_use]
    pub fn allies_only(&self) -> bool {
        self.data.allies_only
    }

    #[must_use]
    pub fn attribute_modifier(&self) -> Option<&str> {
        self.data.attribute_modifier.as_deref()
    }

    #[must_use]
    pub fn flag_sub_object_names(&self) -> &[String] {
        &self.data.flag_sub_object_names
    }

    #[must_use]
    pub const fn started(&self) -> bool {
        self.started
    }

    #[must_use]
    pub fn due(&self, now: Frame) -> bool {
        now >= self.next_scan
    }

    pub fn scanned(&mut self, now: Frame, extra: LogicFrames) {
        self.next_scan = now
            .checked_add(self.data.cadence)
            .and_then(|due| due.checked_add(extra))
            .unwrap_or(Frame::FOREVER);
    }

    pub fn stagger(&mut self, entity: Entity, now: Frame) {
        let cadence = self.data.cadence.get().max(1);
        let offset = entity.index().checked_rem(cadence).unwrap_or(0);
        self.next_scan = now
            .checked_add(LogicFrames::from_frames(offset))
            .unwrap_or(now);
        self.started = true;
    }
}

impl Component for LargeGroupBonusUpdate {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u64(self.next_scan.get());
        hasher.write_bool(self.started);
    }
}

impl ModuleKind for LargeGroupBonusUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn large_group_bonus_update_from_fields(
    fields: &dyn ModuleFieldSource,
    now: Frame,
) -> Result<LargeGroupBonusUpdate, FieldError> {
    let rate_ms = match fields.field("UpdateRate") {
        None => DEFAULT_RATE_MS,
        Some(raw) => {
            let ms = fields
                .field_numeric("UpdateRate")
                .filter(|value| value.is_finite() && *value >= 0.0)
                .ok_or_else(|| FieldError::new(format!("UpdateRate '{raw}' is not a duration")))?;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let whole = ms.min(86_400_000.0) as u32;
            whole
        }
    };
    let radius = distance(fields, "Radius")?;
    let rub_off = match fields.field("RubOffRadius") {
        None => radius,
        Some(_) => distance(fields, "RubOffRadius")?,
    };
    let count = match fields.field("Count") {
        None => 0,
        Some(raw) => {
            let value = fields
                .field_numeric("Count")
                .filter(|value| value.is_finite() && *value >= 0.0)
                .ok_or_else(|| FieldError::new(format!("Count '{raw}' is not a count")))?;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let whole = value.min(f64::from(u32::MAX) as f32) as u32;
            whole
        }
    };
    let allies_only = match fields.field("AlliesOnly") {
        None => false,
        Some(raw) => parse_bool(raw)
            .ok_or_else(|| FieldError::new(format!("AlliesOnly '{raw}' is not Yes/No")))?,
    };
    let mut member_tokens: Vec<String> = fields
        .field_args("HordeMemberFilter")
        .into_iter()
        .map(str::to_owned)
        .collect();
    if member_tokens.is_empty() {
        member_tokens.push(String::from("NONE"));
    }
    Ok(LargeGroupBonusUpdate {
        data: Arc::new(GroupBonusData {
            cadence: crate::special_ability_update::retail_duration_ticks(rate_ms)
                .max(LogicFrames::ONE),
            member_filter: ObjectFilter::parse(&member_tokens),
            count,
            radius,
            rub_off_radius: rub_off,
            allies_only,
            attribute_modifier: fields
                .field("AttributeModifier")
                .filter(|name| !name.eq_ignore_ascii_case("NONE"))
                .map(str::to_owned),
            flag_sub_object_names: fields
                .field_args("FlagSubObjectNames")
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }),
        next_scan: now,
        started: false,
    })
}

fn distance(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<f32, FieldError> {
    match fields.field(keyword) {
        None => Ok(0.0),
        Some(raw) => fields
            .field_numeric(keyword)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a distance"))),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct TestFields(BTreeMap<&'static str, Vec<&'static str>>);
    impl ModuleFieldSource for TestFields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .and_then(|(_, v)| v.first())
                .copied()
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        }
    }

    fn fields(pairs: &[(&'static str, &'static str)]) -> TestFields {
        TestFields(
            pairs
                .iter()
                .map(|(k, v)| (*k, v.split_whitespace().collect()))
                .collect(),
        )
    }

    #[test]
    fn an_absent_rub_off_radius_reads_as_the_gaining_radius() {
        let module =
            large_group_bonus_update_from_fields(&fields(&[("Radius", "160.0")]), Frame::ZERO)
                .unwrap();
        assert_eq!(module.rub_off_radius(), 160.0);
    }

    #[test]
    fn an_unreadable_field_is_refused() {
        for pair in [
            ("Radius", "sideways"),
            ("Count", "many"),
            ("AlliesOnly", "Sometimes"),
            ("UpdateRate", "often"),
        ] {
            assert!(
                large_group_bonus_update_from_fields(&fields(&[pair]), Frame::ZERO).is_err(),
                "{pair:?}"
            );
        }
    }
}
