use mint_sage_map::{Object, Value};

const UPGRADES_LIST: &str = "objectUpgradesList";
const EXPERIENCE_LEVEL: &str = "objectExperienceLevel";
const AGGRESSIVENESS: &str = "objectAggressiveness";
const TARGETABLE: &str = "objectTargetable";
const INITIAL_HEALTH: &str = "objectInitialHealth";
const RECRUITABLE_AI: &str = "objectRecruitableAI";
const SELECTABLE: &str = "objectSelectable";
const INITIAL_STANCE: &str = "objectInitialStance";
const ENABLED: &str = "objectEnabled";
const POWERED: &str = "objectPowered";
const UNSELLABLE: &str = "objectUnsellable";
const EVENTS_LIST: &str = "objectEventsList";
const WEATHER: &str = "objectWeather";
const TIME: &str = "objectTime";

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ObjectKeys {
    pub(crate) upgrades: Vec<String>,
    pub(crate) experience: Option<i32>,
    pub(crate) aggressiveness: Option<i32>,
    pub(crate) targetable: bool,
    pub(crate) initial_health: Option<i32>,
    pub(crate) recruitable: Option<bool>,
    pub(crate) selectable: Option<bool>,
    pub(crate) initial_stance: Option<i32>,
    pub(crate) enabled: Option<bool>,
    pub(crate) powered: Option<bool>,
    pub(crate) unsellable: Option<bool>,
    pub(crate) events_list: Option<String>,
    pub(crate) weather: Option<i32>,
    pub(crate) time: Option<i32>,
}

fn int(object: &Object, key: &str) -> Option<i32> {
    match object.properties.get(key)? {
        Value::Int(value) => Some(*value),
        _ => None,
    }
}

fn flag(object: &Object, key: &str) -> Option<bool> {
    match object.properties.get(key)? {
        Value::Bool(value) => Some(*value),
        _ => None,
    }
}

impl ObjectKeys {
    pub(crate) fn from_object(object: &Object) -> Self {
        let upgrades = match object.properties.get(UPGRADES_LIST) {
            Some(Value::Str(list) | Value::WideStr(list)) => {
                list.split_whitespace().map(str::to_owned).collect()
            }
            _ => Vec::new(),
        };
        Self {
            upgrades,
            experience: int(object, EXPERIENCE_LEVEL),
            aggressiveness: int(object, AGGRESSIVENESS),
            targetable: matches!(object.properties.get(TARGETABLE), Some(Value::Bool(true))),
            initial_health: int(object, INITIAL_HEALTH),
            recruitable: flag(object, RECRUITABLE_AI),
            selectable: flag(object, SELECTABLE),
            initial_stance: int(object, INITIAL_STANCE),
            enabled: flag(object, ENABLED),
            powered: flag(object, POWERED),
            unsellable: flag(object, UNSELLABLE),
            events_list: match object.properties.get(EVENTS_LIST) {
                Some(Value::Str(name) | Value::WideStr(name)) if !name.is_empty() => {
                    Some(name.clone())
                }
                _ => None,
            },
            weather: int(object, WEATHER),
            time: int(object, TIME),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}
