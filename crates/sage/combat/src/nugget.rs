use mint_sim::{LogicFrames, StateHasher};

use crate::damage::{DamageFxTypeId, DamageTypeId, DeathTypeId};

#[derive(Debug, Clone, PartialEq)]
pub enum Nugget {
    Damage(DamageNugget),
    Projectile(ProjectileNugget),
    MetaImpact(MetaImpactNugget),
    Dot(DotNugget),
    Modifier(ModifierNugget),
    Unmodelled(String),
}

impl Nugget {
    #[must_use]
    pub const fn is_gated(&self) -> bool {
        match self {
            Nugget::Damage(nugget) => nugget.requires_upgrade,
            Nugget::Projectile(nugget) => nugget.requires_upgrade,
            Nugget::Dot(nugget) => nugget.requires_upgrade,
            Nugget::MetaImpact(_) | Nugget::Modifier(_) | Nugget::Unmodelled(_) => false,
        }
    }

    #[must_use]
    pub fn keyword(&self) -> &str {
        match self {
            Nugget::Damage(_) => "DamageNugget",
            Nugget::Projectile(_) => "ProjectileNugget",
            Nugget::MetaImpact(_) => "MetaImpactNugget",
            Nugget::Dot(_) => "DOTNugget",
            Nugget::Modifier(_) => "AttributeModifierNugget",
            Nugget::Unmodelled(name) => name.as_str(),
        }
    }

    pub fn hash_into(&self, hasher: &mut StateHasher) {
        match self {
            Nugget::Damage(nugget) => {
                hasher.write_u8(0);
                nugget.hash_into(hasher);
            }
            Nugget::Projectile(nugget) => {
                hasher.write_u8(1);
                nugget.hash_into(hasher);
            }
            Nugget::MetaImpact(nugget) => {
                hasher.write_u8(2);
                nugget.hash_into(hasher);
            }
            Nugget::Unmodelled(name) => {
                hasher.write_u8(3);
                hasher.write_bytes(name.as_bytes());
            }
            Nugget::Dot(nugget) => {
                hasher.write_u8(4);
                nugget.hash_into(hasher);
            }
            Nugget::Modifier(nugget) => {
                hasher.write_u8(5);
                hasher.write_bytes(nugget.list.as_bytes());
                hasher.write_f32(nugget.radius);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EffectNugget {
    WeaponOcl(WeaponOclNugget),
    FireLogic(FireLogicNugget),
    HordeAttack(HordeAttackNugget),
    SlaveAttack,
    Paralyze(ParalyzeNugget),
    Grab(GrabNugget),
    SpawnAndFade(SpawnAndFadeNugget),
    SpecialModelCondition(SpecialModelConditionNugget),
    EmotionWeapon(EmotionWeaponNugget),
    OpenGate(OpenGateNugget),
    StealMoney(StealMoneyNugget),
}

impl EffectNugget {
    pub fn hash_into(&self, hasher: &mut StateHasher) {
        match self {
            EffectNugget::WeaponOcl(nugget) => {
                hasher.write_u8(0);
                nugget.hash_into(hasher);
            }
            EffectNugget::FireLogic(nugget) => {
                hasher.write_u8(1);
                nugget.hash_into(hasher);
            }
            EffectNugget::HordeAttack(nugget) => {
                hasher.write_u8(2);
                nugget.hash_into(hasher);
            }
            EffectNugget::SlaveAttack => {
                hasher.write_u8(3);
            }
            EffectNugget::Paralyze(nugget) => {
                hasher.write_u8(4);
                nugget.hash_into(hasher);
            }
            EffectNugget::Grab(nugget) => {
                hasher.write_u8(5);
                nugget.hash_into(hasher);
            }
            EffectNugget::SpawnAndFade(nugget) => {
                hasher.write_u8(6);
                nugget.hash_into(hasher);
            }
            EffectNugget::SpecialModelCondition(nugget) => {
                hasher.write_u8(7);
                nugget.hash_into(hasher);
            }
            EffectNugget::EmotionWeapon(nugget) => {
                hasher.write_u8(8);
                nugget.hash_into(hasher);
            }
            EffectNugget::OpenGate(nugget) => {
                hasher.write_u8(9);
                hasher.write_f32(nugget.radius);
            }
            EffectNugget::StealMoney(nugget) => {
                hasher.write_u8(10);
                hasher.write_i32(nugget.amount);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeaponOclNugget {
    pub list: String,
}

impl WeaponOclNugget {
    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bytes(self.list.as_bytes());
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FireLogicNugget {
    pub logic_type: String,
    pub radius: f32,
    pub damage: f32,
    pub damage_arc: Option<f32>,
    pub min_max_burn_rate: Option<u16>,
    pub min_decay: Option<u16>,
    pub max_resistance: u16,
}

impl FireLogicNugget {
    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bytes(self.logic_type.as_bytes());
        hasher.write_f32(self.radius);
        hasher.write_f32(self.damage);
        match self.damage_arc {
            Some(value) => {
                hasher.write_bool(true);
                hasher.write_f32(value);
            }
            None => hasher.write_bool(false),
        }
        hasher.write_u16(self.min_max_burn_rate.unwrap_or(0));
        hasher.write_bool(self.min_max_burn_rate.is_some());
        hasher.write_u16(self.min_decay.unwrap_or(0));
        hasher.write_bool(self.min_decay.is_some());
        hasher.write_u16(self.max_resistance);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HordeAttackNugget {
    pub lock_weapon_slot: Option<String>,
}

impl HordeAttackNugget {
    pub fn hash_into(&self, hasher: &mut StateHasher) {
        match &self.lock_weapon_slot {
            Some(slot) => {
                hasher.write_bool(true);
                hasher.write_bytes(slot.as_bytes());
            }
            None => hasher.write_bool(false),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParalyzeNugget {
    pub radius: f32,
    pub duration: LogicFrames,
    pub special_object_filter: Vec<String>,
    pub fx: Option<String>,
}

impl ParalyzeNugget {
    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.radius);
        self.duration.hash_into(hasher);
        hasher.write_usize(self.special_object_filter.len());
        for token in &self.special_object_filter {
            hasher.write_bytes(token.as_bytes());
        }
        match &self.fx {
            Some(fx) => {
                hasher.write_bool(true);
                hasher.write_bytes(fx.as_bytes());
            }
            None => hasher.write_bool(false),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GrabNugget {
    pub special_object_filter: Vec<String>,
    pub contain_target_on_effect: bool,
    pub remove_target_from_other_contain: bool,
    pub impact_target_on_effect: bool,
    pub shock_wave_amount: f32,
    pub shock_wave_radius: f32,
    pub shock_wave_taper_off: f32,
    pub shock_wave_z_mult: f32,
}

impl GrabNugget {
    #[must_use]
    pub fn shove(&self) -> MetaImpactNugget {
        MetaImpactNugget {
            amount: self.shock_wave_amount,
            radius: self.shock_wave_radius,
            taper_off: self.shock_wave_taper_off,
            z_mult: self.shock_wave_z_mult,
            ..MetaImpactNugget::default()
        }
    }

    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.special_object_filter.len());
        for token in &self.special_object_filter {
            hasher.write_bytes(token.as_bytes());
        }
        hasher.write_bool(self.contain_target_on_effect);
        hasher.write_bool(self.remove_target_from_other_contain);
        hasher.write_bool(self.impact_target_on_effect);
        hasher.write_f32(self.shock_wave_amount);
        hasher.write_f32(self.shock_wave_radius);
        hasher.write_f32(self.shock_wave_taper_off);
        hasher.write_f32(self.shock_wave_z_mult);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpawnAndFadeNugget {
    pub object_target_filter: Vec<String>,
    pub spawned_object: String,
    pub offset: [f32; 3],
}

impl SpawnAndFadeNugget {
    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.object_target_filter.len());
        for token in &self.object_target_filter {
            hasher.write_bytes(token.as_bytes());
        }
        hasher.write_bytes(self.spawned_object.as_bytes());
        for axis in self.offset {
            hasher.write_f32(axis);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecialModelConditionNugget {
    pub conditions: Vec<String>,
    pub duration: LogicFrames,
}

impl SpecialModelConditionNugget {
    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.conditions.len());
        for name in &self.conditions {
            hasher.write_bytes(name.as_bytes());
        }
        self.duration.hash_into(hasher);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmotionWeaponNugget {
    pub emotion: String,
    pub radius: f32,
    pub duration: LogicFrames,
}

impl EmotionWeaponNugget {
    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bytes(self.emotion.as_bytes());
        hasher.write_f32(self.radius);
        self.duration.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGateNugget {
    pub radius: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StealMoneyNugget {
    pub amount: i32,
}

#[must_use]
pub fn carries_upgrade_gate(keyword: &str) -> bool {
    const WITHOUT: [&str; 8] = [
        "DamageContainedNugget",
        "DamageFieldNugget",
        "FireLogicNugget",
        "HordeAttackNugget",
        "LuaEventNugget",
        "OpenGateNugget",
        "SpawnAndFadeNugget",
        "SlaveAttackNugget",
    ];
    !WITHOUT
        .iter()
        .any(|name| keyword.eq_ignore_ascii_case(name))
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffectNuggets(std::collections::BTreeMap<(usize, usize), EffectNugget>);

impl EffectNuggets {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, weapon: usize, nugget: usize, effect: EffectNugget) {
        self.0.insert((weapon, nugget), effect);
    }

    pub fn clear_weapon(&mut self, weapon: usize) {
        self.0.retain(|(held, _), _| *held != weapon);
    }

    #[must_use]
    pub fn get(&self, weapon: usize, nugget: usize) -> Option<&EffectNugget> {
        self.0.get(&(weapon, nugget))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DamageFxTypes {
    by_nugget: std::collections::BTreeMap<(usize, usize), DamageFxTypeId>,
    by_weapon: std::collections::BTreeMap<usize, DamageFxTypeId>,
}

impl DamageFxTypes {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, weapon: usize, nugget: usize, fx_type: DamageFxTypeId) {
        self.by_nugget.insert((weapon, nugget), fx_type);
    }

    pub fn insert_weapon(&mut self, weapon: usize, fx_type: DamageFxTypeId) {
        self.by_weapon.insert(weapon, fx_type);
    }

    pub fn clear_weapon(&mut self, weapon: usize) {
        self.by_nugget.retain(|(held, _), _| *held != weapon);
        self.by_weapon.remove(&weapon);
    }

    #[must_use]
    pub fn get(&self, weapon: usize, nugget: usize) -> Option<DamageFxTypeId> {
        self.by_nugget.get(&(weapon, nugget)).copied()
    }

    #[must_use]
    pub fn for_hit(&self, weapon: usize, nugget: usize) -> Option<DamageFxTypeId> {
        self.get(weapon, nugget)
            .or_else(|| self.by_weapon.get(&weapon).copied())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_nugget.len()
    }

    #[must_use]
    pub fn weapons(&self) -> usize {
        self.by_weapon.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_nugget.is_empty() && self.by_weapon.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DotNugget {
    pub damage: f32,
    pub damage_type: DamageTypeId,
    pub radius: f32,
    pub interval: LogicFrames,
    pub duration: LogicFrames,
    pub death_type: Option<DeathTypeId>,
    pub sub_type: Option<DamageTypeId>,
    pub requires_upgrade: bool,
    pub accepts_add: bool,
}

impl DotNugget {
    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.damage);
        self.damage_type.hash_into(hasher);
        hasher.write_f32(self.radius);
        self.interval.hash_into(hasher);
        self.duration.hash_into(hasher);
        match self.death_type {
            Some(death) => {
                hasher.write_u8(1);
                death.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        match self.sub_type {
            Some(sub) => {
                hasher.write_u8(1);
                sub.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        hasher.write_bool(self.requires_upgrade);
        hasher.write_bool(self.accepts_add);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModifierNugget {
    pub list: String,
    pub radius: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DamageNugget {
    pub damage: f32,
    pub damage_type: DamageTypeId,
    pub death_type: Option<DeathTypeId>,
    pub sub_type: Option<DamageTypeId>,
    pub radius: f32,
    pub damage_scalar: f32,
    pub delay: LogicFrames,
    pub arc: Option<f32>,
    pub arc_inverted: bool,
    pub flanking_bonus: f32,
    pub requires_upgrade: bool,
    pub accepts_add: bool,
}

impl DamageNugget {
    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.damage);
        self.damage_type.hash_into(hasher);
        match self.death_type {
            Some(death) => {
                hasher.write_u8(1);
                death.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        match self.sub_type {
            Some(sub) => {
                hasher.write_u8(1);
                sub.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        hasher.write_f32(self.radius);
        hasher.write_f32(self.damage_scalar);
        self.delay.hash_into(hasher);
        hasher.write_bool(self.arc.is_some());
        hasher.write_f32(self.arc.unwrap_or(0.0));
        hasher.write_bool(self.arc_inverted);
        hasher.write_f32(self.flanking_bonus);
        hasher.write_bool(self.requires_upgrade);
        hasher.write_bool(self.accepts_add);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectileNugget {
    pub projectile_template: Option<String>,
    pub warhead: String,
    pub requires_upgrade: bool,
}

impl ProjectileNugget {
    pub fn hash_into(&self, hasher: &mut StateHasher) {
        match &self.projectile_template {
            Some(name) => {
                hasher.write_u8(1);
                hasher.write_bytes(name.as_bytes());
            }
            None => hasher.write_u8(0),
        }
        hasher.write_bytes(self.warhead.as_bytes());
        hasher.write_bool(self.requires_upgrade);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MetaImpactNugget {
    pub amount: f32,
    pub radius: f32,
    pub taper_off: f32,
    pub z_mult: f32,
    pub arc: Option<f32>,
    pub speed: f32,
    pub delay: LogicFrames,
    pub hero_resist: f32,
    pub invert: bool,
    pub flip_direction: bool,
    pub arc_inverted: bool,
    pub only_when_just_died: bool,
    pub affect_hordes: bool,
    pub cyclonic_factor: f32,
    pub clear_radius: bool,
    pub clear_mult: f32,
    pub clear_fling_height: f32,
    pub kill_filter: Vec<String>,
    pub special_filter: Vec<String>,
}

impl Default for MetaImpactNugget {
    fn default() -> Self {
        Self {
            amount: 0.0,
            radius: 0.0,
            taper_off: 0.0,
            z_mult: 1.0,
            arc: None,
            speed: 0.0,
            delay: LogicFrames::ZERO,
            hero_resist: 0.0,
            invert: false,
            flip_direction: false,
            arc_inverted: false,
            only_when_just_died: false,
            affect_hordes: false,
            cyclonic_factor: 0.0,
            clear_radius: false,
            clear_mult: 0.0,
            clear_fling_height: 0.0,
            kill_filter: Vec::new(),
            special_filter: Vec::new(),
        }
    }
}

impl MetaImpactNugget {
    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.amount);
        hasher.write_f32(self.radius);
        hasher.write_f32(self.taper_off);
        hasher.write_f32(self.z_mult);
        hasher.write_bool(self.arc.is_some());
        hasher.write_f32(self.arc.unwrap_or(0.0));
        hasher.write_f32(self.speed);
        self.delay.hash_into(hasher);
        hasher.write_f32(self.hero_resist);
        hasher.write_bool(self.invert);
        hasher.write_bool(self.flip_direction);
        hasher.write_bool(self.arc_inverted);
        hasher.write_bool(self.only_when_just_died);
        hasher.write_bool(self.affect_hordes);
        hasher.write_f32(self.cyclonic_factor);
        hasher.write_bool(self.clear_radius);
        hasher.write_f32(self.clear_mult);
        hasher.write_f32(self.clear_fling_height);
        for tokens in [&self.kill_filter, &self.special_filter] {
            hasher.write_u32(u32::try_from(tokens.len()).unwrap_or(u32::MAX));
            for token in tokens {
                hasher.write_bytes(token.as_bytes());
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::damage::{DamageTypes, DeathTypes};

    fn hash_of(nugget: &Nugget) -> u64 {
        let mut hasher = StateHasher::new();
        nugget.hash_into(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn every_field_of_a_damage_nugget_changes_its_hash() {
        let mut types = DamageTypes::new();
        let pierce = types.intern("PIERCE").unwrap();
        let slash = types.intern("SLASH").unwrap();
        let base = DamageNugget {
            damage: 30.0,
            damage_type: pierce,
            death_type: None,
            sub_type: None,
            radius: 0.0,
            damage_scalar: 1.0,
            delay: LogicFrames::ZERO,
            arc: None,
            arc_inverted: false,
            flanking_bonus: 0.0,
            requires_upgrade: false,
            accepts_add: true,
        };
        let baseline = hash_of(&Nugget::Damage(base));
        let mut harder = base;
        harder.damage = 31.0;
        let mut other_type = base;
        other_type.damage_type = slash;
        let mut wider = base;
        wider.radius = 5.0;
        let mut scaled = base;
        scaled.damage_scalar = 2.0;
        let mut later = base;
        later.delay = LogicFrames::from_frames(3);
        let mut gated = base;
        gated.requires_upgrade = true;
        let mut burned = base;
        burned.death_type = DeathTypes::new().intern("BURNED");
        let mut undead = base;
        undead.sub_type = Some(slash);
        let mut flanking = base;
        flanking.flanking_bonus = 0.5;
        for other in [
            harder, other_type, wider, scaled, later, gated, burned, undead, flanking,
        ] {
            assert_ne!(baseline, hash_of(&Nugget::Damage(other)), "{other:?}");
        }
    }

    #[test]
    fn only_a_nugget_with_a_required_upgrade_is_gated() {
        let mut types = DamageTypes::new();
        let pierce = types.intern("PIERCE").unwrap();
        let plain = Nugget::Projectile(ProjectileNugget {
            projectile_template: None,
            warhead: "W".to_owned(),
            requires_upgrade: false,
        });
        let fire = Nugget::Projectile(ProjectileNugget {
            projectile_template: None,
            warhead: "W".to_owned(),
            requires_upgrade: true,
        });
        assert!(!plain.is_gated());
        assert!(fire.is_gated());
        assert!(!Nugget::Damage(DamageNugget {
            damage: 1.0,
            damage_type: pierce,
            death_type: None,
            sub_type: None,
            radius: 0.0,
            damage_scalar: 1.0,
            delay: LogicFrames::ZERO,
            arc: None,
            arc_inverted: false,
            flanking_bonus: 0.0,
            requires_upgrade: false,
            accepts_add: true,
        })
        .is_gated());
        assert!(!Nugget::Unmodelled("X".to_owned()).is_gated());
    }

    #[test]
    fn the_four_variants_hash_differently() {
        let mut types = DamageTypes::new();
        let pierce = types.intern("PIERCE").unwrap();
        let nuggets = [
            Nugget::Damage(DamageNugget {
                damage: 0.0,
                damage_type: pierce,
                death_type: None,
                sub_type: None,
                radius: 0.0,
                damage_scalar: 1.0,
                delay: LogicFrames::ZERO,
                arc: None,
                arc_inverted: false,
                flanking_bonus: 0.0,
                requires_upgrade: false,
                accepts_add: true,
            }),
            Nugget::Projectile(ProjectileNugget {
                projectile_template: None,
                warhead: String::new(),
                requires_upgrade: false,
            }),
            Nugget::MetaImpact(MetaImpactNugget::default()),
            Nugget::Unmodelled(String::new()),
        ];
        let mut hashes: Vec<u64> = nuggets.iter().map(hash_of).collect();
        let count = hashes.len();
        hashes.sort_unstable();
        hashes.dedup();
        assert_eq!(hashes.len(), count);
    }
}
