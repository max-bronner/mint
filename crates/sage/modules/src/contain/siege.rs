use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

use super::{count, open_contain_from_fields, owned, OpenContain, MAX_CONTAIN};
use crate::effects::{names, Names};
use crate::object_filter::ObjectFilter;

#[derive(Debug, Clone, PartialEq)]
pub struct SiegeEngineContain {
    open: OpenContain,
    crew_filter: ObjectFilter,
    crew_max: u32,
    initial_crew: Option<InitialCrew>,
    speed_percent_per_crew: f32,
    status_of_crew: Names,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialCrew {
    pub template: String,
    pub count: u32,
}

impl SiegeEngineContain {
    #[must_use]
    pub const fn open(&self) -> &OpenContain {
        &self.open
    }

    #[must_use]
    pub const fn crew_filter(&self) -> &ObjectFilter {
        &self.crew_filter
    }

    #[must_use]
    pub const fn crew_max(&self) -> u32 {
        self.crew_max
    }

    #[must_use]
    pub const fn initial_crew(&self) -> Option<&InitialCrew> {
        self.initial_crew.as_ref()
    }

    #[must_use]
    pub fn status_of_crew(&self) -> &[std::sync::Arc<str>] {
        &self.status_of_crew
    }

    #[must_use]
    pub const fn speed_percent_per_crew(&self) -> f32 {
        self.speed_percent_per_crew
    }
}

impl Component for SiegeEngineContain {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.open.hash_into(hasher);
        hasher.write_u32(self.crew_max);
        match &self.initial_crew {
            Some(crew) => {
                hasher.write_u32(1);
                hasher.write_bytes(crew.template.as_bytes());
                hasher.write_u32(crew.count);
            }
            None => hasher.write_u32(0),
        }
        hasher.write_f32(self.speed_percent_per_crew);
        for status in self.status_of_crew.iter() {
            hasher.write_bytes(status.as_bytes());
        }
    }
}

impl ModuleKind for SiegeEngineContain {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

fn percent_or(
    fields: &dyn ModuleFieldSource,
    keyword: &str,
    default: f32,
) -> Result<f32, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(default);
    };
    fields
        .field_percent(keyword)
        .filter(|v| v.is_finite())
        .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a percentage")))
}

fn initial_crew(fields: &dyn ModuleFieldSource) -> Result<Option<InitialCrew>, FieldError> {
    let Some(raw) = fields.field("InitialCrew") else {
        return Ok(None);
    };
    let args = owned(fields.field_args("InitialCrew"));
    let refuse = || FieldError::new(format!("InitialCrew '{raw}' is not an object and a count"));
    let (template, count) = match args.as_slice() {
        [template] => (template, 1),
        [template, count] => (
            template,
            count
                .parse::<u32>()
                .ok()
                .filter(|n| *n <= MAX_CONTAIN)
                .ok_or_else(refuse)?,
        ),
        _ => return Err(refuse()),
    };
    Ok(Some(InitialCrew {
        template: template.clone(),
        count,
    }))
}

pub fn siege_engine_contain_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<SiegeEngineContain, FieldError> {
    Ok(SiegeEngineContain {
        open: open_contain_from_fields(fields)?,
        crew_filter: ObjectFilter::parse(&owned(fields.field_args("CrewFilter"))),
        crew_max: count(fields, "CrewMax", 0, MAX_CONTAIN)?,
        initial_crew: initial_crew(fields)?,
        speed_percent_per_crew: percent_or(fields, "SpeedPercentPerCrew", 1.0)?,
        status_of_crew: names(owned(fields.field_args("ObjectStatusOfCrew"))),
    })
}
