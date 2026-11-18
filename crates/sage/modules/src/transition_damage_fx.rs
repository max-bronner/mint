use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

#[derive(Debug, Clone, PartialEq)]
pub enum DamageFxLocation {
    Coord(f32, f32, f32),
    Bone { name: String, random_bone: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DamageFxSlot {
    pub location: DamageFxLocation,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TransitionDamageFX {
    damaged_fx_list: Option<DamageFxSlot>,
    really_damaged_fx_list: Option<DamageFxSlot>,
    rubble_fx_list: Option<DamageFxSlot>,
    damaged_particle_system: Option<DamageFxSlot>,
    really_damaged_particle_system: Option<DamageFxSlot>,
    rubble_particle_system: Option<DamageFxSlot>,
    fx_types: Vec<String>,
    particle_types: Vec<String>,
    pristine_show_sub_object: Vec<String>,
    pristine_hide_sub_object: Vec<String>,
    damaged_show_sub_object: Vec<String>,
    damaged_hide_sub_object: Vec<String>,
    really_damaged_show_sub_object: Vec<String>,
    really_damaged_hide_sub_object: Vec<String>,
}

impl TransitionDamageFX {
    #[must_use]
    pub fn damaged_fx_list(&self) -> Option<&DamageFxSlot> {
        self.damaged_fx_list.as_ref()
    }

    #[must_use]
    pub fn really_damaged_fx_list(&self) -> Option<&DamageFxSlot> {
        self.really_damaged_fx_list.as_ref()
    }

    #[must_use]
    pub fn rubble_fx_list(&self) -> Option<&DamageFxSlot> {
        self.rubble_fx_list.as_ref()
    }

    #[must_use]
    pub fn damaged_particle_system(&self) -> Option<&DamageFxSlot> {
        self.damaged_particle_system.as_ref()
    }

    #[must_use]
    pub fn really_damaged_particle_system(&self) -> Option<&DamageFxSlot> {
        self.really_damaged_particle_system.as_ref()
    }

    #[must_use]
    pub fn rubble_particle_system(&self) -> Option<&DamageFxSlot> {
        self.rubble_particle_system.as_ref()
    }

    #[must_use]
    pub fn pristine_show_sub_object(&self) -> &[String] {
        &self.pristine_show_sub_object
    }

    #[must_use]
    pub fn pristine_hide_sub_object(&self) -> &[String] {
        &self.pristine_hide_sub_object
    }

    #[must_use]
    pub fn damaged_show_sub_object(&self) -> &[String] {
        &self.damaged_show_sub_object
    }

    #[must_use]
    pub fn damaged_hide_sub_object(&self) -> &[String] {
        &self.damaged_hide_sub_object
    }

    #[must_use]
    pub fn really_damaged_show_sub_object(&self) -> &[String] {
        &self.really_damaged_show_sub_object
    }

    #[must_use]
    pub fn really_damaged_hide_sub_object(&self) -> &[String] {
        &self.really_damaged_hide_sub_object
    }
}

impl Component for TransitionDamageFX {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hash_slot(hasher, self.damaged_fx_list.as_ref());
        hash_slot(hasher, self.really_damaged_fx_list.as_ref());
        hash_slot(hasher, self.rubble_fx_list.as_ref());
        hash_slot(hasher, self.damaged_particle_system.as_ref());
        hash_slot(hasher, self.really_damaged_particle_system.as_ref());
        hash_slot(hasher, self.rubble_particle_system.as_ref());
        hash_string_list(hasher, &self.fx_types);
        hash_string_list(hasher, &self.particle_types);
        hash_string_list(hasher, &self.pristine_show_sub_object);
        hash_string_list(hasher, &self.pristine_hide_sub_object);
        hash_string_list(hasher, &self.damaged_show_sub_object);
        hash_string_list(hasher, &self.damaged_hide_sub_object);
        hash_string_list(hasher, &self.really_damaged_show_sub_object);
        hash_string_list(hasher, &self.really_damaged_hide_sub_object);
    }
}

fn hash_string_list(hasher: &mut StateHasher, list: &[String]) {
    hasher.write_usize(list.len());
    for s in list {
        hasher.write_usize(s.len());
        hasher.write_bytes(s.as_bytes());
    }
}

fn hash_slot(hasher: &mut StateHasher, slot: Option<&DamageFxSlot>) {
    hasher.write_bool(slot.is_some());
    let Some(slot) = slot else { return };
    hasher.write_usize(slot.name.len());
    hasher.write_bytes(slot.name.as_bytes());
    match &slot.location {
        DamageFxLocation::Coord(x, y, z) => {
            hasher.write_bool(false);
            hasher.write_f32(*x);
            hasher.write_f32(*y);
            hasher.write_f32(*z);
        }
        DamageFxLocation::Bone { name, random_bone } => {
            hasher.write_bool(true);
            hasher.write_bool(*random_bone);
            hasher.write_usize(name.len());
            hasher.write_bytes(name.as_bytes());
        }
    }
}

impl ModuleKind for TransitionDamageFX {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn transition_damage_fx_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<TransitionDamageFX, FieldError> {
    Ok(TransitionDamageFX {
        damaged_fx_list: fx_slot(fields, "DamagedFXList1", "fxlist")?,
        really_damaged_fx_list: fx_slot(fields, "ReallyDamagedFXList1", "fxlist")?,
        rubble_fx_list: fx_slot(fields, "RubbleFXList1", "fxlist")?,
        damaged_particle_system: fx_slot(fields, "DamagedParticleSystem1", "psys")?,
        really_damaged_particle_system: fx_slot(fields, "ReallyDamagedParticleSystem1", "psys")?,
        rubble_particle_system: fx_slot(fields, "RubbleParticleSystem1", "psys")?,
        fx_types: string_list(fields, "DamageFXTypes"),
        particle_types: string_list(fields, "DamageParticleTypes"),
        pristine_show_sub_object: string_list(fields, "PristineShowSubObject"),
        pristine_hide_sub_object: string_list(fields, "PristineHideSubObject"),
        damaged_show_sub_object: string_list(fields, "DamagedShowSubObject"),
        damaged_hide_sub_object: string_list(fields, "DamagedHideSubObject"),
        really_damaged_show_sub_object: string_list(fields, "ReallyDamagedShowSubObject"),
        really_damaged_hide_sub_object: string_list(fields, "ReallyDamagedHideSubObject"),
    })
}

fn string_list(fields: &dyn ModuleFieldSource, keyword: &str) -> Vec<String> {
    fields
        .field_args(keyword)
        .into_iter()
        .map(str::to_owned)
        .collect()
}

fn fx_slot(
    fields: &dyn ModuleFieldSource,
    keyword: &str,
    target_key: &str,
) -> Result<Option<DamageFxSlot>, FieldError> {
    let args = fields.field_args(keyword);
    if args.is_empty() {
        return Ok(None);
    }
    parse_damage_fx_slot(&args, target_key)
        .map(Some)
        .ok_or_else(|| FieldError::new(format!("{keyword}: not a valid FX/particle slot")))
}

fn split_colon(token: &str) -> (&str, Option<&str>) {
    match token.split_once(':') {
        Some((key, value)) if !value.is_empty() => (key, Some(value)),
        Some((key, _)) => (key, None),
        None => (token, None),
    }
}

fn parse_damage_fx_slot(tokens: &[&str], target_key: &str) -> Option<DamageFxSlot> {
    let mut rest = tokens.iter();
    let (key, value) = split_colon(rest.next()?);
    let location = if key.eq_ignore_ascii_case("bone") {
        let name = value?.to_owned();
        let (rb_key, rb_value) = split_colon(rest.next()?);
        if !rb_key.eq_ignore_ascii_case("randombone") {
            return None;
        }
        let random_bone = mint_sage_ini::value::parse_bool(rb_value?)?;
        DamageFxLocation::Bone { name, random_bone }
    } else if key.eq_ignore_ascii_case("loc") {
        let axis = |rest: &mut std::slice::Iter<'_, &str>, axis: &str| {
            let (axis_key, axis_value) = split_colon(rest.next()?);
            if !axis_key.eq_ignore_ascii_case(axis) {
                return None;
            }
            axis_value?.parse::<f32>().ok()
        };
        let x = axis(&mut rest, "x")?;
        let y = axis(&mut rest, "y")?;
        let z = axis(&mut rest, "z")?;
        DamageFxLocation::Coord(x, y, z)
    } else {
        return None;
    };
    let (last_key, last_value) = split_colon(rest.next()?);
    if !last_key.eq_ignore_ascii_case(target_key) || rest.next().is_some() {
        return None;
    }
    Some(DamageFxSlot {
        location,
        name: last_value?.to_owned(),
    })
}

#[must_use]
pub fn transitions(
    old: u8,
    new: u8,
    damage_type: Option<&str>,
    tdfx: &TransitionDamageFX,
) -> TdfxTransition {
    let (show, hide): (Vec<String>, Vec<String>) = match new {
        0 => (
            tdfx.pristine_show_sub_object.clone(),
            tdfx.pristine_hide_sub_object.clone(),
        ),
        1 => (
            tdfx.damaged_show_sub_object.clone(),
            tdfx.damaged_hide_sub_object.clone(),
        ),
        2 => (
            tdfx.really_damaged_show_sub_object.clone(),
            tdfx.really_damaged_hide_sub_object.clone(),
        ),
        3 => (Vec::new(), Vec::new()),
        _ => return TdfxTransition::default(),
    };
    let mut out = TdfxTransition {
        show,
        hide,
        fx: None,
        particles: None,
    };
    if new <= old {
        return out;
    }
    let (fx, particles) = match new {
        1 => (
            tdfx.damaged_fx_list.as_ref(),
            tdfx.damaged_particle_system.as_ref(),
        ),
        2 => (
            tdfx.really_damaged_fx_list.as_ref(),
            tdfx.really_damaged_particle_system.as_ref(),
        ),
        3 => (
            tdfx.rubble_fx_list.as_ref(),
            tdfx.rubble_particle_system.as_ref(),
        ),
        _ => (None, None),
    };
    if type_allows(&tdfx.fx_types, damage_type) {
        out.fx = fx.cloned();
    }
    if type_allows(&tdfx.particle_types, damage_type) {
        out.particles = particles.cloned();
    }
    out
}

fn type_allows(mask: &[String], damage_type: Option<&str>) -> bool {
    match damage_type {
        None => true,
        Some(kind) => mask.is_empty() || mask.iter().any(|name| name.eq_ignore_ascii_case(kind)),
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TdfxTransition {
    pub show: Vec<String>,
    pub hide: Vec<String>,
    pub fx: Option<DamageFxSlot>,
    pub particles: Option<DamageFxSlot>,
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
    fn no_fields_at_all_succeeds() {
        let fields = TestFields(BTreeMap::new());
        let module = transition_damage_fx_from_fields(&fields, Frame::ZERO).unwrap();
        assert!(module.really_damaged_fx_list().is_none());
        assert!(module.pristine_show_sub_object().is_empty());
    }

    #[test]
    fn a_shipped_shape_declaration_reads_its_fields() {
        let fields = TestFields(BTreeMap::from([
            (
                "ReallyDamagedFXList1",
                vec![
                    "Loc:",
                    "X:0",
                    "Y:0",
                    "Z:0",
                    "FXList:FX_GondorTrebuchetTransitionMedium",
                ],
            ),
            ("PristineShowSubObject", vec!["HDG_GATER", "HDG_GATEL"]),
            (
                "PristineHideSubObject",
                vec![
                    "HDG_GATER_D1",
                    "HDG_GATER_D2",
                    "HDG_GATEL_D1",
                    "HDG_GATEL_D2",
                ],
            ),
        ]));
        let module = transition_damage_fx_from_fields(&fields, Frame::ZERO).unwrap();
        let slot = module.really_damaged_fx_list().unwrap();
        assert_eq!(slot.location, DamageFxLocation::Coord(0.0, 0.0, 0.0));
        assert_eq!(slot.name, "FX_GondorTrebuchetTransitionMedium");
        assert_eq!(
            module.pristine_show_sub_object(),
            ["HDG_GATER", "HDG_GATEL"]
        );
        assert_eq!(module.pristine_hide_sub_object().len(), 4);
    }

    #[test]
    fn a_bone_random_slot_round_trips() {
        let fields = TestFields(BTreeMap::from([(
            "DamagedFXList1",
            vec!["Bone:B", "RandomBone:Yes", "FXList:F"],
        )]));
        let module = transition_damage_fx_from_fields(&fields, Frame::ZERO).unwrap();
        let slot = module.damaged_fx_list().unwrap();
        assert_eq!(
            slot.location,
            DamageFxLocation::Bone {
                name: "B".to_owned(),
                random_bone: true
            }
        );
        assert_eq!(slot.name, "F");
    }

    #[test]
    fn a_bone_slot_without_randombone_is_refused() {
        let fields = TestFields(BTreeMap::from([(
            "DamagedFXList1",
            vec!["Bone:B", "FXList:F"],
        )]));
        assert!(transition_damage_fx_from_fields(&fields, Frame::ZERO).is_err());
    }

    fn slot(name: &str) -> DamageFxSlot {
        DamageFxSlot {
            location: DamageFxLocation::Coord(0.0, 0.0, 0.0),
            name: name.to_owned(),
        }
    }

    fn tdfx_with(
        damaged: Option<DamageFxSlot>,
        really_damaged: Option<DamageFxSlot>,
    ) -> TransitionDamageFX {
        TransitionDamageFX {
            damaged_fx_list: damaged,
            really_damaged_fx_list: really_damaged,
            ..TransitionDamageFX::default()
        }
    }

    #[test]
    fn a_worse_edge_plays_the_new_rungs_fx_and_a_better_edge_does_not() {
        let tdfx = tdfx_with(Some(slot("Damaged")), Some(slot("ReallyDamaged")));
        let worse = transitions(1, 2, None, &tdfx);
        assert_eq!(worse.fx, Some(slot("ReallyDamaged")));
        let better = transitions(2, 1, None, &tdfx);
        assert_eq!(better.fx, None);
    }

    #[test]
    fn the_damage_type_mask_filters_by_name() {
        let mut tdfx = tdfx_with(Some(slot("Damaged")), None);
        tdfx.fx_types = vec!["CRUSH".to_owned()];
        assert_eq!(
            transitions(0, 1, Some("CRUSH"), &tdfx).fx,
            Some(slot("Damaged"))
        );
        assert_eq!(transitions(0, 1, Some("FLAME"), &tdfx).fx, None);
        assert_eq!(
            transitions(0, 1, None, &tdfx).fx,
            Some(slot("Damaged")),
            "no last damage info admits every slot"
        );
    }

    #[test]
    fn a_really_damaged_to_rubble_edge_plays_the_rubble_fx_and_particles() {
        let mut tdfx = tdfx_with(None, None);
        tdfx.rubble_fx_list = Some(slot("Rubble"));
        tdfx.rubble_particle_system = Some(slot("RubbleParticles"));
        let edge = transitions(2, 3, None, &tdfx);
        assert_eq!(edge.fx, Some(slot("Rubble")));
        assert_eq!(edge.particles, Some(slot("RubbleParticles")));
        assert!(edge.show.is_empty());
        assert!(edge.hide.is_empty());
    }
}
