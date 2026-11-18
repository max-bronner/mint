use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask, UpgradeMask};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, ModuleKind, StateHasher};

use crate::effects::Effect;
use crate::object_filter::ObjectFilter;
use crate::upgrade_mux::{attempt_upgrade_via_mux, UpgradeEffect, UpgradeMuxData};

#[derive(Debug, PartialEq)]
struct Data {
    command_points: i32,
    required_object: ObjectFilter,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandPointsUpgrade {
    data: Arc<Data>,
    mux: UpgradeMuxData,
}

impl CommandPointsUpgrade {
    #[must_use]
    pub fn command_points(&self) -> i32 {
        self.data.command_points
    }

    #[must_use]
    pub fn required_object(&self) -> &ObjectFilter {
        &self.data.required_object
    }
}

impl Component for CommandPointsUpgrade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
    }
}

impl UpgradeEffect for CommandPointsUpgrade {
    fn mux_mut(&mut self) -> &mut UpgradeMuxData {
        &mut self.mux
    }

    fn on_fire(&mut self, _active: &UpgradeMask, out: &mut Vec<Effect>) {
        if self.data.command_points != 0 {
            out.push(Effect::CommandPointBonus(self.data.command_points));
        }
    }
}

impl ModuleKind for CommandPointsUpgrade {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Upgrade)
        .union(CapabilityMask::single(Capability::Update));

    fn update(&mut self, ctx: &mut mint_sim::module::UpdateContext<'_>) -> mint_sim::module::Sleep {
        self.mux.update(ctx)
    }

    fn attempt_upgrade(&mut self, active: &UpgradeMask, ctx: &mut mint_sim::UpgradeContext<'_>) {
        attempt_upgrade_via_mux(self, active, ctx);
    }

    fn would_upgrade(&self, active: &UpgradeMask) -> bool {
        self.mux.would_fire(active)
    }

    fn on_upgrade_removed(&mut self, removed: &UpgradeMask) {
        self.mux.reset_if_triggered_by(removed);
    }
}

pub fn command_points_upgrade_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<CommandPointsUpgrade, FieldError> {
    let command_points = match fields.field_numeric("CommandPoints") {
        Some(value) if value.is_finite() && value.fract() == 0.0 => {
            #[allow(clippy::cast_possible_truncation)]
            let whole = value.clamp(-1_000_000.0, 1_000_000.0) as i32;
            whole
        }
        Some(raw) => {
            return Err(FieldError::new(format!(
                "CommandPoints {raw} is not a whole number"
            )))
        }
        None => 0,
    };
    let data = Data {
        command_points,
        required_object: ObjectFilter::parse(
            &fields
                .field_args("RequiredObject")
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>(),
        ),
    };
    Ok(CommandPointsUpgrade {
        data: Arc::new(data),
        mux: UpgradeMuxData::from_fields(fields)?,
    })
}
