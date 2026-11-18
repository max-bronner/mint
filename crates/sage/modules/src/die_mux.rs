use std::sync::Arc;
use std::sync::OnceLock;

use mint_sim::module::Death;
use mint_sim::{Entity, FieldError, ModuleFieldSource, StateHasher};

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeathTypeFilter {
    all: bool,
    added: Vec<String>,
    removed: Vec<String>,
}

impl DeathTypeFilter {
    fn read(tokens: &[&str]) -> Self {
        let mut filter = Self {
            all: true,
            added: Vec::new(),
            removed: Vec::new(),
        };
        for token in tokens {
            if let Some(name) = token.strip_prefix('+') {
                filter.added.push(name.to_ascii_uppercase());
            } else if let Some(name) = token.strip_prefix('-') {
                filter.removed.push(name.to_ascii_uppercase());
            } else if token.eq_ignore_ascii_case("NONE") {
                filter.all = false;
            } else if token.eq_ignore_ascii_case("ALL") {
                filter.all = true;
            } else {
                filter.all = false;
                filter.added.push(token.to_ascii_uppercase());
            }
        }
        filter
    }

    fn admits(&self, kind: &str) -> bool {
        if kind.is_empty() {
            return true;
        }
        let named = |list: &[String]| list.iter().any(|name| name.eq_ignore_ascii_case(kind));
        if named(&self.removed) {
            return false;
        }
        self.all || named(&self.added)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Filter {
    death_types: DeathTypeFilter,
    required_status: Vec<String>,
    exempt_status: Vec<String>,
    damage_amount_required: Option<f32>,
    killer_angle: Option<(f32, f32)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DieMuxData {
    filter: Arc<Filter>,
}

impl Default for DieMuxData {
    fn default() -> Self {
        Self {
            filter: Arc::new(Filter {
                death_types: DeathTypeFilter {
                    all: true,
                    added: Vec::new(),
                    removed: Vec::new(),
                },
                required_status: Vec::new(),
                exempt_status: Vec::new(),
                damage_amount_required: None,
                killer_angle: None,
            }),
        }
    }
}

impl DieMuxData {
    pub fn from_fields(fields: &dyn ModuleFieldSource) -> Result<Self, FieldError> {
        let angle = |keyword: &str| match fields.field(keyword) {
            None => Ok(None),
            Some(raw) => raw
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|degrees| degrees.is_finite() && degrees.abs() <= 3_600.0)
                .map(Some)
                .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a bearing"))),
        };
        let killer_angle = match (angle("MinKillerAngle")?, angle("MaxKillerAngle")?) {
            (Some(min), Some(max)) => Some((min, max)),
            (None, None) => None,
            _ => return Err(FieldError::new(
                "MinKillerAngle and MaxKillerAngle are a pair; one is declared without the other",
            )),
        };
        let damage_amount_required = match fields.field("DamageAmountRequired") {
            None => None,
            Some(raw) => raw
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|amount| amount.is_finite() && *amount >= 0.0)
                .map(Some)
                .ok_or_else(|| {
                    FieldError::new(format!(
                        "DamageAmountRequired '{raw}' is not a non-negative number"
                    ))
                })?,
        };
        let names = |keyword: &str| -> Vec<String> {
            fields
                .field_args(keyword)
                .into_iter()
                .map(str::to_ascii_uppercase)
                .collect()
        };
        Ok(Self {
            filter: Arc::new(Filter {
                death_types: DeathTypeFilter::read(&fields.field_args("DeathTypes")),
                required_status: names("RequiredStatus"),
                exempt_status: names("ExemptStatus"),
                damage_amount_required,
                killer_angle,
            }),
        })
    }

    #[must_use]
    pub fn admits(&self, death: &Death<'_>) -> bool {
        let filter = &self.filter;
        if !filter.death_types.admits(death.kind) {
            return false;
        }
        let set = |name: &String| {
            death
                .status
                .iter()
                .any(|carried| carried.eq_ignore_ascii_case(name))
        };
        if filter.exempt_status.iter().any(set) {
            return false;
        }
        if !filter.required_status.iter().all(set) {
            return false;
        }
        if filter
            .damage_amount_required
            .is_some_and(|required| death.amount < required)
        {
            return false;
        }
        match filter.killer_angle {
            None => true,
            Some((min, max)) => death
                .killer_bearing
                .is_some_and(|bearing| in_sector(bearing, min, max)),
        }
    }

    #[must_use]
    pub fn gate(&self, entity: Entity, module: &str, death: &Death<'_>) -> bool {
        let run = self.admits(death);
        if diagnostics() {
            println!(
                "diemux: {} {module} death={} damage={} sub={} status=[{}] required=[{}] amount={} -> {}",
                entity.index(),
                blank(death.kind),
                blank(death.cause),
                blank(death.sub_cause),
                death.status.join(" "),
                self.filter.required_status.join(" "),
                death.amount,
                if run { "run" } else { "skip" }
            );
        }
        run
    }

    pub fn hash_into(&self, _hasher: &mut StateHasher) {}
}

fn blank(name: &str) -> &str {
    if name.is_empty() {
        "-"
    } else {
        name
    }
}

fn in_sector(bearing: f32, min: f32, max: f32) -> bool {
    let (bearing, min, max) = (wrap(bearing), wrap(min), wrap(max));
    if min <= max {
        bearing >= min && bearing <= max
    } else {
        bearing >= min || bearing <= max
    }
}

fn wrap(degrees: f32) -> f32 {
    let folded = degrees % 360.0;
    if folded < 0.0 {
        folded + 360.0
    } else {
        folded
    }
}

fn diagnostics() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("MINT_ROTWK_DIAG").is_some())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Fields(BTreeMap<&'static str, Vec<&'static str>>);

    impl ModuleFieldSource for Fields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(keyword))
                .and_then(|(_, args)| args.first().copied())
        }

        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.0
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(keyword))
                .map(|(_, args)| args.clone())
                .unwrap_or_default()
        }
    }

    fn mux(pairs: &[(&'static str, &[&'static str])]) -> DieMuxData {
        let fields = Fields(pairs.iter().map(|(k, v)| (*k, v.to_vec())).collect());
        DieMuxData::from_fields(&fields).unwrap()
    }

    fn death<'a>(kind: &'a str, status: &'a [&'a str]) -> Death<'a> {
        Death {
            kind,
            status,
            ..Death::NONE
        }
    }

    #[test]
    fn a_removed_death_type_refuses_and_the_rest_of_all_still_runs() {
        let mux = mux(&[("DeathTypes", &["ALL", "-FADED"])]);
        assert!(!mux.admits(&death("FADED", &[])));
        assert!(mux.admits(&death("NORMAL", &[])));
    }

    #[test]
    fn none_admits_only_what_is_added_back() {
        let mux = mux(&[("DeathTypes", &["NONE", "+KNOCKBACK"])]);
        assert!(mux.admits(&death("KNOCKBACK", &[])));
        assert!(!mux.admits(&death("NORMAL", &[])));
    }

    #[test]
    fn any_exempt_status_the_object_carries_refuses() {
        let mux = mux(&[("ExemptStatus", &["SOLD"])]);
        assert!(!mux.admits(&death("NORMAL", &["SOLD"])));
        assert!(mux.admits(&death("NORMAL", &["DEATH_1"])));
    }

    #[test]
    fn every_required_status_must_be_set_not_just_one() {
        let mux = mux(&[("RequiredStatus", &["DEATH_1", "DEPLOYED"])]);
        assert!(!mux.admits(&death("NORMAL", &["DEATH_1"])));
        assert!(mux.admits(&death("NORMAL", &["DEATH_1", "DEPLOYED"])));
    }

    #[test]
    fn a_blow_under_the_required_amount_refuses() {
        let mux = mux(&[("DamageAmountRequired", &["15"])]);
        let under = Death {
            amount: 14.0,
            ..Death::NONE
        };
        let over = Death {
            amount: 15.0,
            ..Death::NONE
        };
        assert!(!mux.admits(&under));
        assert!(mux.admits(&over));
    }

    #[test]
    fn a_killer_sector_wraps_through_zero() {
        let mux = mux(&[("MinKillerAngle", &["-46"]), ("MaxKillerAngle", &["46"])]);
        let at = |bearing: f32| Death {
            killer_bearing: Some(bearing),
            ..Death::NONE
        };
        assert!(mux.admits(&at(350.0)));
        assert!(mux.admits(&at(10.0)));
        assert!(!mux.admits(&at(90.0)));
        assert!(!mux.admits(&Death::NONE));
    }

    #[test]
    fn a_declaration_that_writes_no_filter_admits_every_death() {
        let mux = mux(&[]);
        assert!(mux.admits(&Death::NONE));
        assert!(mux.admits(&death("CRUSHED", &[])));
    }

    #[test]
    fn a_malformed_angle_is_refused_rather_than_read_as_no_sector() {
        let fields = Fields(
            [
                ("MinKillerAngle", vec!["north"]),
                ("MaxKillerAngle", vec!["46"]),
            ]
            .into_iter()
            .collect(),
        );
        assert!(DieMuxData::from_fields(&fields).is_err());
    }
}
