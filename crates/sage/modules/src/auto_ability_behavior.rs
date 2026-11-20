use std::sync::Arc;

use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

use crate::effects::{names, Name, Names};
use crate::fire_weapon_update::flag;
use crate::object_filter::ObjectFilter;

pub type Query = (u32, ObjectFilter);

pub const UNBOUNDED_RANGE: f32 = 1.0e7;

#[derive(Debug, PartialEq)]
struct AutoAbilityData {
    query: Vec<Query>,
    power: Option<Name>,
    max_scan_range: f32,
    min_scan_range: f32,
    allow_self: bool,
    forbidden_status: Names,
    idle_time: LogicFrames,
    starts_active: bool,
    base_max_range_from_start_pos: bool,
    adjust_attack_melee_position: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AutoAbilityBehavior {
    data: Arc<AutoAbilityData>,
    pub next_scan: Frame,
    pub active: ActiveButton,
    pub start_position: Option<(f32, f32)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ActiveButton {
    #[default]
    Unresolved,
    Off,
    On(String),
}

impl AutoAbilityBehavior {
    #[must_use]
    pub fn query(&self) -> &[Query] {
        &self.data.query
    }

    #[must_use]
    pub fn power(&self) -> Option<&str> {
        self.data.power.as_deref()
    }

    #[must_use]
    pub fn max_scan_range(&self) -> f32 {
        self.data.max_scan_range
    }

    #[must_use]
    pub fn min_scan_range(&self) -> f32 {
        self.data.min_scan_range
    }

    #[must_use]
    pub fn allow_self(&self) -> bool {
        self.data.allow_self
    }

    #[must_use]
    pub fn forbidden_status(&self) -> &Names {
        &self.data.forbidden_status
    }

    #[must_use]
    pub fn idle_time(&self) -> LogicFrames {
        self.data.idle_time
    }

    #[must_use]
    pub fn starts_active(&self) -> bool {
        self.data.starts_active
    }

    #[must_use]
    pub fn base_max_range_from_start_pos(&self) -> bool {
        self.data.base_max_range_from_start_pos
    }

    #[must_use]
    pub fn adjust_attack_melee_position(&self) -> bool {
        self.data.adjust_attack_melee_position
    }

    #[must_use]
    pub fn ready_to_scan(&self, now: Frame) -> bool {
        now >= self.next_scan
    }

    pub fn defer_scan(&mut self, until: Frame) {
        self.next_scan = until;
    }
}

impl Component for AutoAbilityBehavior {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.next_scan.hash_into(hasher);
        match &self.active {
            ActiveButton::Unresolved => hasher.write_u8(0),
            ActiveButton::Off => hasher.write_u8(1),
            ActiveButton::On(power) => {
                hasher.write_u8(2);
                hasher.write_usize(power.len());
                hasher.write_bytes(power.as_bytes());
            }
        }
        hasher.write_bool(self.start_position.is_some());
        let (x, y) = self.start_position.unwrap_or((0.0, 0.0));
        hasher.write_f32(x);
        hasher.write_f32(y);
    }
}

impl ModuleKind for AutoAbilityBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn auto_ability_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<AutoAbilityBehavior, FieldError> {
    let query: Vec<Query> = fields
        .field_args_each("Query")
        .into_iter()
        .filter_map(|tokens| {
            let (first, rest) = tokens.split_first()?;
            let min: u32 = first.trim().parse().ok()?;
            let rest: Vec<String> = rest.iter().map(|token| (*token).to_owned()).collect();
            Some((min, ObjectFilter::parse(&rest)))
        })
        .collect();
    let power = fields
        .field("SpecialAbility")
        .filter(|name| !name.eq_ignore_ascii_case("NONE"))
        .map(Name::from);
    let max_scan_range = fields
        .field_numeric("MaxScanRange")
        .filter(|range| range.is_finite() && *range > 0.0)
        .unwrap_or(UNBOUNDED_RANGE);
    let min_scan_range = fields
        .field_numeric("MinScanRange")
        .filter(|range| range.is_finite() && *range > 0.0)
        .unwrap_or(0.0);
    let allow_self = flag(fields, "AllowSelf").unwrap_or(true);
    let forbidden_status = names(
        fields
            .field_args("ForbiddenStatus")
            .into_iter()
            .map(str::to_owned)
            .collect(),
    );
    let idle_time = fields
        .field_numeric("IdleTimeSeconds")
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
        .map_or(LogicFrames::ZERO, LogicFrames::from_seconds);
    let starts_active = flag(fields, "StartsActive").unwrap_or(false);
    let base_max_range_from_start_pos = flag(fields, "BaseMaxRangeFromStartPos").unwrap_or(false);
    let adjust_attack_melee_position = flag(fields, "AdjustAttackMeleePosition").unwrap_or(false);
    Ok(AutoAbilityBehavior {
        data: Arc::new(AutoAbilityData {
            query,
            power,
            max_scan_range,
            min_scan_range,
            allow_self,
            forbidden_status,
            idle_time,
            starts_active,
            base_max_range_from_start_pos,
            adjust_attack_melee_position,
        }),
        next_scan: Frame::ZERO,
        active: ActiveButton::Unresolved,
        start_position: None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct TestFields(BTreeMap<&'static str, Vec<Vec<&'static str>>>);

    impl ModuleFieldSource for TestFields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .and_then(|(_, v)| v.first())
                .and_then(|line| line.first())
                .copied()
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .and_then(|(_, v)| v.first())
                .cloned()
                .unwrap_or_default()
        }
        fn field_args_each(&self, keyword: &str) -> Vec<Vec<&str>> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        }
    }

    fn fields(pairs: &[(&'static str, &'static str)]) -> TestFields {
        let mut map: BTreeMap<&'static str, Vec<Vec<&'static str>>> = BTreeMap::new();
        for (k, v) in pairs {
            map.entry(k)
                .or_default()
                .push(v.split_whitespace().collect());
        }
        TestFields(map)
    }

    #[test]
    fn a_query_reads_its_leading_integer_and_the_rest_as_a_filter() {
        let module =
            auto_ability_behavior_from_fields(&fields(&[("Query", "1 ALL ENEMIES")]), Frame::ZERO)
                .unwrap();
        assert_eq!(module.query().len(), 1);
        let (min, filter) = &module.query()[0];
        assert_eq!(*min, 1);
        assert!(filter.passes(&[String::from("INFANTRY")], "Orc"));
    }

    #[test]
    fn a_configured_block_reads_its_named_power() {
        let module = auto_ability_behavior_from_fields(
            &fields(&[
                ("SpecialAbility", "SpecialAbilityAragornAthelas"),
                ("Query", "1 NONE +HERO ALLIES"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(module.power(), Some("SpecialAbilityAragornAthelas"));
    }

    #[test]
    fn ready_to_scan_gates_on_next_scan() {
        let mut module = auto_ability_behavior_from_fields(&fields(&[]), Frame::ZERO).unwrap();
        assert!(module.ready_to_scan(Frame::ZERO));
        module.defer_scan(Frame::new(30));
        assert!(!module.ready_to_scan(Frame::new(29)));
        assert!(module.ready_to_scan(Frame::new(30)));
    }
}
