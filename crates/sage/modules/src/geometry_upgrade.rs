use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask, UpgradeMask};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, ModuleKind, StateHasher};

use crate::effects::{names, Effect, Names};
use crate::upgrade_mux::{attempt_upgrade_via_mux, UpgradeEffect, UpgradeMuxData};

#[derive(Debug, Clone, PartialEq)]
pub struct GeometryUpgrade {
    mux: UpgradeMuxData,
    data: Arc<(Names, Names)>,
}

impl GeometryUpgrade {
    #[must_use]
    pub fn show_geometry(&self) -> Vec<&str> {
        self.data.0.iter().map(AsRef::as_ref).collect()
    }

    #[must_use]
    pub fn hide_geometry(&self) -> Vec<&str> {
        self.data.1.iter().map(AsRef::as_ref).collect()
    }
}

impl UpgradeEffect for GeometryUpgrade {
    fn mux_mut(&mut self) -> &mut UpgradeMuxData {
        &mut self.mux
    }

    fn on_fire(&mut self, _active: &UpgradeMask, out: &mut Vec<Effect>) {
        let (show, hide) = &*self.data;
        if show.is_empty() && hide.is_empty() {
            return;
        }
        out.push(Effect::Geometry {
            show: Names::clone(show),
            hide: Names::clone(hide),
        });
    }
}

impl Component for GeometryUpgrade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
    }
}

impl ModuleKind for GeometryUpgrade {
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

pub fn geometry_upgrade_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<GeometryUpgrade, FieldError> {
    let list = |keyword: &str| -> Names {
        names(
            fields
                .field_args(keyword)
                .into_iter()
                .map(str::to_owned)
                .collect(),
        )
    };
    Ok(GeometryUpgrade {
        mux: UpgradeMuxData::from_fields(fields)?,
        data: Arc::new((list("ShowGeometry"), list("HideGeometry"))),
    })
}

#[cfg(test)]
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

    #[test]
    fn reads_show_and_hide_geometry_lists() {
        let fields = TestFields(BTreeMap::from([
            ("ShowGeometry", vec!["Geom_Orig", "Geom_V2"]),
            ("HideGeometry", vec!["Geom_V3"]),
        ]));
        let module = geometry_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.show_geometry(), ["Geom_Orig", "Geom_V2"]);
        assert_eq!(module.hide_geometry(), ["Geom_V3"]);
    }

    #[test]
    fn absent_lists_default_to_empty() {
        let fields = TestFields(BTreeMap::new());
        let module = geometry_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert!(module.show_geometry().is_empty());
        assert!(module.hide_geometry().is_empty());
    }
}
