use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

pub const AI_MODULE_NAMES: &[&str] = &[
    "AIUpdateInterface",
    "AnimalAIUpdate",
    "AssaultTransportAIUpdate",
    "DeployStyleAIUpdate",
    "DozerAIUpdate",
    "FoundationAIUpdate",
    "GiantBirdAIUpdate",
    "HordeAIUpdate",
    "HordeWorkerAIUpdate",
    "SiegeAIUpdate",
    "SupplyTruckAIUpdate",
    "TransportAIUpdate",
    "WanderAIUpdate",
    "WorkerAIUpdate",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AiUpdate;

impl Component for AiUpdate {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for AiUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn ai_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<AiUpdate, FieldError> {
    grab_toss_from_fields(fields)?;
    follow_through_from_fields(fields)?;
    Ok(AiUpdate)
}

fn non_negative(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Option<f32>, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(None);
    };
    fields
        .field_numeric(keyword)
        .filter(|value| *value >= 0.0)
        .map(Some)
        .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a non-negative number")))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FollowThrough {
    pub distance: f32,
    pub check_step: f32,
    pub gradient: f32,
}

impl Default for FollowThrough {
    fn default() -> Self {
        Self {
            distance: 100.0,
            check_step: 10.0,
            gradient: 1.0,
        }
    }
}

pub fn follow_through_from_fields(
    fields: &dyn ModuleFieldSource,
) -> Result<FollowThrough, FieldError> {
    let defaults = FollowThrough::default();
    let check_step = match non_negative(fields, "FollowThroughCheckStep")? {
        Some(step) if step <= 0.0 => {
            return Err(FieldError::new(format!(
                "FollowThroughCheckStep '{step}' is not a positive number"
            )));
        }
        step => step.unwrap_or(defaults.check_step),
    };
    Ok(FollowThrough {
        distance: non_negative(fields, "FollowThroughDistance")?.unwrap_or(defaults.distance),
        check_step,
        gradient: non_negative(fields, "FollowThroughGradient")?.unwrap_or(defaults.gradient),
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct GrabToss {
    pub time: Option<f32>,
    pub height: Option<f32>,
    pub fx: Option<String>,
}

pub fn grab_toss_from_fields(
    fields: &dyn ModuleFieldSource,
) -> Result<Option<GrabToss>, FieldError> {
    let time = non_negative(fields, "GrabTossTimeTrigger")?;
    let height = non_negative(fields, "GrabTossHeightTrigger")?;
    if time.is_none() && height.is_none() {
        return Ok(None);
    }
    Ok(Some(GrabToss {
        time,
        height,
        fx: fields.field("TossFX").map(str::to_owned),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sage_ini::tables::field_table;

    struct Fields(&'static [(&'static str, &'static str)]);

    impl ModuleFieldSource for Fields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(keyword))
                .map(|(_, value)| *value)
        }
    }

    #[test]
    fn a_grab_toss_reads_both_triggers_and_refuses_a_malformed_one() {
        let toss = grab_toss_from_fields(&Fields(&[
            ("GrabTossTimeTrigger", "2.5"),
            ("GrabTossHeightTrigger", "100.0"),
            ("TossFX", "FX_Fall"),
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(
            (toss.time, toss.height, toss.fx.as_deref()),
            (Some(2.5), Some(100.0), Some("FX_Fall"))
        );
        assert_eq!(grab_toss_from_fields(&Fields(&[])).unwrap(), None);
        assert!(grab_toss_from_fields(&Fields(&[("GrabTossTimeTrigger", "soon")])).is_err());
        assert!(grab_toss_from_fields(&Fields(&[("GrabTossHeightTrigger", "-1")])).is_err());
        assert!(
            ai_update_from_fields(&Fields(&[("GrabTossTimeTrigger", "x")]), Frame::ZERO).is_err()
        );
        assert_eq!(
            follow_through_from_fields(&Fields(&[("FollowThroughDistance", "200")])).unwrap(),
            FollowThrough {
                distance: 200.0,
                ..FollowThrough::default()
            }
        );
        assert!(follow_through_from_fields(&Fields(&[("FollowThroughCheckStep", "0")])).is_err());
        assert!(
            ai_update_from_fields(&Fields(&[("FollowThroughDistance", "far")]), Frame::ZERO)
                .is_err()
        );
    }

    #[test]
    fn every_ai_module_name_is_a_module_the_parse_tables_declare() {
        for name in AI_MODULE_NAMES {
            assert!(
                field_table::module(name).is_some(),
                "{name} names no module context"
            );
        }
    }
}
