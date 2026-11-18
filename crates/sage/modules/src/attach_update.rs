use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

use crate::fire_weapon_update::flag;
use crate::object_filter::ObjectFilter;

#[derive(Debug, Clone, PartialEq)]
struct AttachUpdateData {
    object_filter: ObjectFilter,
    scan_range: f32,
    parent_status: Option<String>,
    always_teleport: bool,
    anchor_to_top_of_geometry: bool,
    owner_attachment_event: Option<String>,
    ally_attachment_event: Option<String>,
    enemy_attachment_event: Option<String>,
    owner_died_event: Option<String>,
    ally_died_event: Option<String>,
    enemy_died_event: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttachUpdate {
    data: AttachUpdateData,
}

impl AttachUpdate {
    #[must_use]
    pub fn object_filter(&self) -> &ObjectFilter {
        &self.data.object_filter
    }

    #[must_use]
    pub const fn scan_range(&self) -> f32 {
        self.data.scan_range
    }

    #[must_use]
    pub fn parent_status(&self) -> Option<&str> {
        self.data.parent_status.as_deref()
    }

    #[must_use]
    pub const fn always_teleport(&self) -> bool {
        self.data.always_teleport
    }

    #[must_use]
    pub const fn anchor_to_top_of_geometry(&self) -> bool {
        self.data.anchor_to_top_of_geometry
    }

    #[must_use]
    pub fn owner_attachment_event(&self) -> Option<&str> {
        self.data.owner_attachment_event.as_deref()
    }

    #[must_use]
    pub fn ally_attachment_event(&self) -> Option<&str> {
        self.data.ally_attachment_event.as_deref()
    }

    #[must_use]
    pub fn enemy_attachment_event(&self) -> Option<&str> {
        self.data.enemy_attachment_event.as_deref()
    }

    #[must_use]
    pub fn owner_died_event(&self) -> Option<&str> {
        self.data.owner_died_event.as_deref()
    }

    #[must_use]
    pub fn ally_died_event(&self) -> Option<&str> {
        self.data.ally_died_event.as_deref()
    }

    #[must_use]
    pub fn enemy_died_event(&self) -> Option<&str> {
        self.data.enemy_died_event.as_deref()
    }
}

impl Component for AttachUpdate {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for AttachUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn attach_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<AttachUpdate, FieldError> {
    let object_filter = {
        let tokens: Vec<String> = fields
            .field_args("ObjectFilter")
            .into_iter()
            .map(str::to_owned)
            .collect();
        ObjectFilter::parse(&tokens)
    };
    let scan_range = match fields.field_numeric("ScanRange") {
        Some(range) if range.is_finite() && range >= 0.0 => range,
        Some(range) => {
            return Err(FieldError::new(format!(
                "ScanRange '{range}' is not finite and non-negative"
            )))
        }
        None => 0.0,
    };
    let string_field = |key: &str| fields.field(key).map(str::to_owned);
    Ok(AttachUpdate {
        data: AttachUpdateData {
            object_filter,
            scan_range,
            parent_status: string_field("ParentStatus"),
            always_teleport: flag(fields, "AlwaysTeleport").unwrap_or(false),
            anchor_to_top_of_geometry: flag(fields, "AnchorToTopOfGeometry").unwrap_or(false),
            owner_attachment_event: string_field("ParentOwnerAttachmentEvaEvent"),
            ally_attachment_event: string_field("ParentAllyAttachmentEvaEvent"),
            enemy_attachment_event: string_field("ParentEnemyAttachmentEvaEvent"),
            owner_died_event: string_field("ParentOwnerDiedEvaEvent"),
            ally_died_event: string_field("ParentAllyDiedEvaEvent")
                .or_else(|| string_field("ParentAllyDiedtEvaEvent")),
            enemy_died_event: string_field("ParentEnemyDiedEvaEvent")
                .or_else(|| string_field("ParentEnemyDiedtEvaEvent")),
        },
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    struct Fields(Vec<(&'static str, Vec<&'static str>)>);

    impl ModuleFieldSource for Fields {
        fn field(&self, key: &str) -> Option<&str> {
            self.field_args(key).first().copied()
        }

        fn field_args(&self, key: &str) -> Vec<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        }
    }

    #[test]
    fn every_shipped_field_is_read() {
        let fields = Fields(vec![
            ("ObjectFilter", vec!["ANY", "+INFANTRY", "-NEUTRALGOLLUM"]),
            ("ScanRange", vec!["10"]),
            ("ParentStatus", vec!["HOLDING_THE_RING"]),
            ("AlwaysTeleport", vec!["No"]),
            ("AnchorToTopOfGeometry", vec!["Yes"]),
            ("ParentOwnerAttachmentEvaEvent", vec!["RingPickedUpLocal"]),
            ("ParentEnemyAttachmentEvaEvent", vec!["RingPickedUpEnemy"]),
            ("ParentOwnerDiedEvaEvent", vec!["LocalPlayerLosesRing"]),
        ]);
        let module = attach_update_from_fields(&fields, Frame::ZERO).unwrap();
        assert!((module.scan_range() - 10.0).abs() < 1e-6);
        assert_eq!(module.parent_status(), Some("HOLDING_THE_RING"));
        assert!(!module.always_teleport());
        assert!(module.anchor_to_top_of_geometry());
        assert_eq!(module.owner_attachment_event(), Some("RingPickedUpLocal"));
        assert_eq!(module.ally_attachment_event(), None);
        assert_eq!(module.enemy_attachment_event(), Some("RingPickedUpEnemy"));
        assert_eq!(module.owner_died_event(), Some("LocalPlayerLosesRing"));
        assert!(module
            .object_filter()
            .passes(&[String::from("INFANTRY")], "AnOrc"));
        assert!(!module
            .object_filter()
            .passes(&[String::from("NEUTRALGOLLUM")], "Gollum"));
    }

    #[test]
    fn a_negative_scan_range_errors() {
        let fields = Fields(vec![("ScanRange", vec!["-1"])]);
        assert!(attach_update_from_fields(&fields, Frame::ZERO).is_err());
    }
}
