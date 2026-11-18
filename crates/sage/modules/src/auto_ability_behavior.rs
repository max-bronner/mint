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
    pub idle_since: Option<Frame>,
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

    pub fn note_order(&mut self, now: Frame, ordered: bool) {
        if ordered {
            self.idle_since = None;
        } else if self.idle_since.is_none() {
            self.idle_since = Some(now);
        }
    }

    #[must_use]
    pub fn idle_long_enough(&self, now: Frame) -> bool {
        let idle_time = self.idle_time();
        idle_time.is_zero()
            || self
                .idle_since
                .is_some_and(|since| now.since(since).is_some_and(|elapsed| elapsed >= idle_time))
    }
}

impl Component for AutoAbilityBehavior {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.next_scan.hash_into(hasher);
        hasher.write_bool(self.idle_since.is_some());
        self.idle_since.unwrap_or(Frame::ZERO).hash_into(hasher);
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
    let allow_self = flag(fields, "AllowSelf").unwrap_or(false);
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
        idle_since: None,
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
    fn idle_time_tracks_a_continuous_idle_stretch() {
        let mut module =
            auto_ability_behavior_from_fields(&fields(&[("IdleTimeSeconds", "1")]), Frame::ZERO)
                .unwrap();
        assert_eq!(module.idle_time(), LogicFrames::from_seconds(1.0));
        module.note_order(Frame::new(0), true);
        assert!(!module.idle_long_enough(Frame::new(10)), "still ordered");
        module.note_order(Frame::new(10), false);
        assert!(
            !module.idle_long_enough(Frame::new(20)),
            "not yet a full second"
        );
        assert!(module.idle_long_enough(Frame::new(40)));
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
