use crate::dictionary::{Dictionary, Value};
use crate::objects::{Object, WAYPOINT_TEMPLATE};

pub const BASE_ANCHOR_KEY: &str = "objectIsABase";

pub const BASE_PRIORITY_KEY: &str = "objectBasePriority";

pub const BASE_PHASE_KEY: &str = "objectBasePhase";

#[must_use]
pub fn base_path(name: &str) -> String {
    let name = name.to_ascii_lowercase();
    format!("bases/{name}/{name}.bse")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BasePhase {
    One,
    Two,
    Three,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BaseEntry {
    pub template_name: String,
    pub offset: [f32; 2],
    pub angle: f32,
    pub build_order: Option<(i32, BasePhase)>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BaseLayout {
    pub entries: Vec<BaseEntry>,
    pub skipped: usize,
    pub refused: usize,
}

fn build_order(properties: &Dictionary) -> Option<(i32, BasePhase)> {
    let priority = match properties.get(BASE_PRIORITY_KEY) {
        Some(Value::Int(value)) => *value,
        _ => return None,
    };
    let phase = match properties.get(BASE_PHASE_KEY) {
        Some(Value::Int(1)) => BasePhase::One,
        Some(Value::Int(2)) => BasePhase::Two,
        Some(Value::Int(3)) => BasePhase::Three,
        _ => return None,
    };
    Some((priority, phase))
}

impl BaseLayout {
    #[must_use]
    pub fn from_objects(objects: &[Object]) -> Option<Self> {
        let anchor = objects.iter().find(|object| {
            matches!(
                object.properties.get(BASE_ANCHOR_KEY),
                Some(Value::Bool(true))
            )
        })?;
        if !anchor.x.is_finite() || !anchor.y.is_finite() {
            return None;
        }
        let mut layout = Self::default();
        for object in objects {
            let finite = object.x.is_finite() && object.y.is_finite() && object.angle.is_finite();
            if !finite || object.template_name.eq_ignore_ascii_case(WAYPOINT_TEMPLATE) {
                layout.skipped = layout.skipped.saturating_add(1);
                continue;
            }
            let is_anchor = std::ptr::eq(object, anchor);
            let build_order = if is_anchor {
                None
            } else {
                match build_order(&object.properties) {
                    Some(order) => Some(order),
                    None => {
                        layout.refused = layout.refused.saturating_add(1);
                        continue;
                    }
                }
            };
            layout.entries.push(BaseEntry {
                template_name: object.template_name.clone(),
                offset: [object.x - anchor.x, object.y - anchor.y],
                angle: object.angle,
                build_order,
            });
        }
        Some(layout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictionary::Dictionary;

    fn placed(x: f32, y: f32, angle: f32, template: &str, anchor: Option<bool>) -> Object {
        placed_with(x, y, angle, template, anchor, None)
    }

    fn placed_with(
        x: f32,
        y: f32,
        angle: f32,
        template: &str,
        anchor: Option<bool>,
        order: Option<(i32, i32)>,
    ) -> Object {
        let mut pairs: Vec<(String, Value)> = anchor
            .map(|flag| vec![(String::from(BASE_ANCHOR_KEY), Value::Bool(flag))])
            .unwrap_or_default();
        if let Some((priority, phase)) = order {
            pairs.push((String::from(BASE_PRIORITY_KEY), Value::Int(priority)));
            pairs.push((String::from(BASE_PHASE_KEY), Value::Int(phase)));
        }
        Object {
            x,
            y,
            z: 0.0,
            angle,
            flags: 0,
            template_name: template.to_owned(),
            properties: Dictionary::from_pairs(pairs),
        }
    }

    #[test]
    fn offsets_are_measured_from_the_anchor_and_file_order_is_kept() {
        let objects = vec![
            placed_with(1064.0, 936.0, -0.785, "Pad", None, Some((100, 1))),
            placed_with(1000.0, 1000.0, 0.0, "Citadel", Some(false), Some((20, 2))),
            placed(1000.0, 1000.0, 0.0, "Centre", Some(true)),
            placed(990.0, 990.0, 0.0, "*Waypoints/Waypoint", None),
            placed(f32::NAN, 0.0, 0.0, "Broken", None),
        ];
        let layout = BaseLayout::from_objects(&objects).unwrap();
        let names: Vec<&str> = layout
            .entries
            .iter()
            .map(|entry| entry.template_name.as_str())
            .collect();
        assert_eq!(names, ["Pad", "Citadel", "Centre"]);
        assert_eq!(layout.entries[0].offset, [64.0, -64.0]);
        assert_eq!(layout.entries[0].angle, -0.785);
        assert_eq!(layout.entries[0].build_order, Some((100, BasePhase::One)));
        assert_eq!(layout.entries[1].build_order, Some((20, BasePhase::Two)));
        assert_eq!(layout.entries[2].offset, [0.0, 0.0]);
        assert_eq!(
            layout.entries[2].build_order, None,
            "the anchor carries neither key"
        );
        assert_eq!(layout.skipped, 2);
        assert_eq!(layout.refused, 0);
    }

    #[test]
    fn a_non_anchor_entry_missing_or_carrying_a_bad_phase_is_refused_and_counted() {
        let objects = vec![
            placed(1000.0, 1000.0, 0.0, "Centre", Some(true)),
            placed(1010.0, 1000.0, 0.0, "NoOrder", None),
            placed_with(1020.0, 1000.0, 0.0, "BadPhase", None, Some((50, 4))),
            placed_with(1030.0, 1000.0, 0.0, "Good", None, Some((50, 3))),
        ];
        let layout = BaseLayout::from_objects(&objects).unwrap();
        let names: Vec<&str> = layout
            .entries
            .iter()
            .map(|entry| entry.template_name.as_str())
            .collect();
        assert_eq!(names, ["Centre", "Good"]);
        assert_eq!(layout.refused, 2);
    }

    #[test]
    fn a_layout_with_no_anchor_or_a_broken_one_is_refused() {
        assert!(BaseLayout::from_objects(&[placed(0.0, 0.0, 0.0, "A", Some(false))]).is_none());
        assert!(BaseLayout::from_objects(&[]).is_none());
        assert!(
            BaseLayout::from_objects(&[placed(f32::INFINITY, 0.0, 0.0, "A", Some(true))]).is_none()
        );
    }

    #[test]
    fn the_path_is_lower_cased_and_doubled() {
        assert_eq!(
            base_path("Fortress_Men"),
            "bases/fortress_men/fortress_men.bse"
        );
    }
}
