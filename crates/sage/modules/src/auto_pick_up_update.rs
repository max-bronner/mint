use std::sync::Arc;

use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

use crate::fire_weapon_update::flag;
use crate::object_filter::ObjectFilter;

#[derive(Debug, Clone, PartialEq)]
pub struct EatObjectEntry {
    pub my_health: f32,
    pub target_health: f32,
    pub filter: ObjectFilter,
}

#[derive(Debug, PartialEq)]
struct AutoPickUpData {
    scan_delay: LogicFrames,
    scan_distance: f32,
    pick_up_filter: Option<ObjectFilter>,
    eat: Option<EatObjectEntry>,
    button: Option<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AutoPickUpUpdate {
    data: Arc<AutoPickUpData>,
    pub next_scan: Frame,
    pub woken: bool,
}

const DEFAULT_SCAN_DISTANCE: f32 = 300.0;

impl AutoPickUpUpdate {
    #[must_use]
    pub fn scan_delay(&self) -> LogicFrames {
        self.data.scan_delay
    }

    #[must_use]
    pub fn scan_distance(&self) -> f32 {
        self.data.scan_distance
    }

    #[must_use]
    pub fn pick_up_filter(&self) -> Option<&ObjectFilter> {
        self.data.pick_up_filter.as_ref()
    }

    #[must_use]
    pub fn eat(&self) -> Option<&EatObjectEntry> {
        self.data.eat.as_ref()
    }

    #[must_use]
    pub fn button(&self) -> Option<u8> {
        self.data.button
    }

    pub fn wake(&mut self, option: u8) -> bool {
        let woken = self.data.button == Some(option);
        self.woken |= woken;
        woken
    }

    #[must_use]
    pub fn due(&self, now: Frame) -> bool {
        if self.data.button.is_some() {
            self.woken
        } else {
            now >= self.next_scan
        }
    }
}

impl Component for AutoPickUpUpdate {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.next_scan.hash_into(hasher);
        hasher.write_bool(self.woken);
    }
}

impl ModuleKind for AutoPickUpUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

fn percent_token(token: &str, key: &str) -> Option<f32> {
    let (found, value) = token.split_once(':')?;
    if !found.eq_ignore_ascii_case(key) {
        return None;
    }
    let value = value.trim();
    value
        .strip_suffix('%')
        .unwrap_or(value)
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| v / 100.0)
}

fn eat_object_entry(args: &[&str]) -> Result<EatObjectEntry, FieldError> {
    let bad = || {
        FieldError::new(format!(
            "EatObjectEntry '{}' is not MyHealth:<pct> TargetHealth:<pct> Filter:<filter>",
            args.join(" ")
        ))
    };
    let [mine, theirs, filter_head, rest @ ..] = args else {
        return Err(bad());
    };
    let my_health = percent_token(mine, "MyHealth").ok_or_else(bad)?;
    let target_health = percent_token(theirs, "TargetHealth").ok_or_else(bad)?;
    let (key, first) = filter_head.split_once(':').ok_or_else(bad)?;
    if !key.eq_ignore_ascii_case("Filter") {
        return Err(bad());
    }
    let tokens: Vec<String> = std::iter::once(first)
        .filter(|token| !token.is_empty())
        .chain(rest.iter().copied())
        .map(str::to_owned)
        .collect();
    if tokens.is_empty() {
        return Err(bad());
    }
    Ok(EatObjectEntry {
        my_health,
        target_health,
        filter: ObjectFilter::parse(&tokens),
    })
}

pub fn auto_pick_up_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<AutoPickUpUpdate, FieldError> {
    let scan_delay = fields
        .field_numeric("ScanDelayTime")
        .filter(|ms| ms.is_finite() && *ms > 0.0)
        .map_or(LogicFrames::ZERO, |ms| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let whole = ms.min(86_400_000.0) as u32;
            crate::special_ability_update::retail_duration_ticks(whole)
        });
    let scan_distance = fields
        .field_numeric("ScanDistance")
        .filter(|range| range.is_finite() && *range >= 0.0)
        .unwrap_or(DEFAULT_SCAN_DISTANCE);
    let pick_up_filter = {
        let tokens: Vec<String> = fields
            .field_args("PickUpFilter")
            .into_iter()
            .map(str::to_owned)
            .collect();
        (!tokens.is_empty()).then(|| ObjectFilter::parse(&tokens))
    };
    let eat_args = fields.field_args("EatObjectEntry");
    let eat = if eat_args.is_empty() {
        None
    } else {
        Some(eat_object_entry(&eat_args)?)
    };
    let button = if flag(fields, "RunFromButton") == Some(true) {
        let number = fields
            .field_numeric("RunFromButtonNumber")
            .filter(|n| (1.0..=3.0).contains(n) && n.fract() == 0.0)
            .ok_or_else(|| {
                FieldError::new("RunFromButton = Yes needs a RunFromButtonNumber of 1 to 3")
            })?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(number as u8)
    } else {
        None
    };
    Ok(AutoPickUpUpdate {
        data: Arc::new(AutoPickUpData {
            scan_delay,
            scan_distance,
            pick_up_filter,
            eat,
            button,
        }),
        next_scan: Frame::ZERO,
        woken: false,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn eat_object_entry_reads_both_thresholds_and_a_filter_split_over_tokens() {
        let entry =
            eat_object_entry(&["MyHealth:99%", "TargetHealth:50", "Filter:NONE", "+ORC"]).unwrap();
        assert!((entry.my_health - 0.99).abs() < 1e-6);
        assert!((entry.target_health - 0.5).abs() < 1e-6);
        assert!(entry.filter.passes(&["ORC".to_owned()], "AnOrc"));
        assert!(!entry.filter.passes(&["ELF".to_owned()], "AnElf"));
        for broken in [
            &["MyHealth:99%", "TargetHealth:100%"][..],
            &["TargetHealth:100%", "MyHealth:99%", "Filter:NONE", "+ORC"][..],
            &["MyHealth:lots", "TargetHealth:100%", "Filter:NONE", "+ORC"][..],
            &["MyHealth:99%", "TargetHealth:100%", "Filter:"][..],
        ] {
            assert!(eat_object_entry(broken).is_err(), "{broken:?} was accepted");
        }
    }
}
