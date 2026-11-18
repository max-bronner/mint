use std::collections::BTreeMap;

use crate::tree::{Block, Document, Field};
use crate::value::{is_none, parse_bool, parse_float, parse_int, Duration};

pub const WEAPON_KEYWORD: &str = "Weapon";

pub const WEAPON_NUGGET_KEYWORDS: [&str; 19] = [
    "DamageNugget",
    "ProjectileNugget",
    "MetaImpactNugget",
    "FireLogicNugget",
    "HordeAttackNugget",
    "DOTNugget",
    "AttributeModifierNugget",
    "WeaponOCLNugget",
    "ParalyzeNugget",
    "SlaveAttackNugget",
    "LuaEventNugget",
    "GrabNugget",
    "SpecialModelConditionNugget",
    "EmotionWeaponNugget",
    "OpenGateNugget",
    "StealMoneyNugget",
    "SpawnAndFadeNugget",
    "DamageContainedNugget",
    "DamageFieldNugget",
];

pub const WEAPON_FIELDS: [&str; 15] = [
    "AttackRange",
    "DelayBetweenShots",
    "FiringDuration",
    "PreAttackDelay",
    "PreAttackType",
    "FireFX",
    "LeechRangeWeapon",
    "RadiusDamageAffects",
    "WeaponSpeed",
    "MeleeWeapon",
    "ClipSize",
    "LinearTarget",
    "RangeBonusPerFoot",
    "IsAimingWeapon",
    "AcceptableAimDelta",
];

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LinearTarget {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub t: Option<f32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WeaponDef {
    pub attack_range: Option<f32>,
    pub delay_between_shots: Option<Duration>,
    pub firing_duration: Option<Duration>,
    pub pre_attack_delay: Option<Duration>,
    pub pre_attack_type: Option<String>,
    pub fire_fx: Option<String>,
    pub leech_range_weapon: Option<bool>,
    pub radius_damage_affects: Vec<String>,
    pub weapon_speed: Option<f32>,
    pub melee_weapon: Option<bool>,
    pub clip_size: Option<i64>,
    pub linear_target: Option<LinearTarget>,
    pub range_bonus_per_foot_raw: Option<String>,
    pub is_aiming_weapon: Option<bool>,
    pub acceptable_aim_delta: Option<f32>,
    pub nuggets: Vec<String>,
}

fn scalar<T>(block: &Block, keyword: &str, parser: impl FnOnce(&str) -> Option<T>) -> Option<T> {
    block.field(keyword).and_then(Field::value).and_then(parser)
}

fn reference(block: &Block, keyword: &str) -> Option<String> {
    let value = block.field(keyword).and_then(Field::value)?;
    (!is_none(value)).then(|| value.to_owned())
}

#[must_use]
pub fn read_weapon(block: &Block) -> WeaponDef {
    let linear_target = block.field("LinearTarget").map(|f| LinearTarget {
        x: crate::value::tagged_value(&f.args, "X").and_then(parse_float),
        y: crate::value::tagged_value(&f.args, "Y").and_then(parse_float),
        t: crate::value::tagged_value(&f.args, "T").and_then(parse_float),
    });

    WeaponDef {
        attack_range: scalar(block, "AttackRange", parse_float),
        delay_between_shots: scalar(block, "DelayBetweenShots", parse_float)
            .map(Duration::Milliseconds),
        firing_duration: scalar(block, "FiringDuration", parse_float).map(Duration::Milliseconds),
        pre_attack_delay: scalar(block, "PreAttackDelay", parse_float).map(Duration::Milliseconds),
        pre_attack_type: scalar(block, "PreAttackType", |v| Some(v.to_owned())),
        fire_fx: reference(block, "FireFX"),
        leech_range_weapon: scalar(block, "LeechRangeWeapon", parse_bool),
        radius_damage_affects: block
            .field("RadiusDamageAffects")
            .map(|f| f.args.clone())
            .unwrap_or_default(),
        weapon_speed: scalar(block, "WeaponSpeed", parse_float),
        melee_weapon: scalar(block, "MeleeWeapon", parse_bool),
        clip_size: scalar(block, "ClipSize", parse_int),
        linear_target,
        range_bonus_per_foot_raw: block.field("RangeBonusPerFoot").map(|f| f.args.join(" ")),
        is_aiming_weapon: scalar(block, "IsAimingWeapon", parse_bool),
        acceptable_aim_delta: scalar(block, "AcceptableAimDelta", parse_float),
        nuggets: block.blocks().map(|b| b.keyword.clone()).collect(),
    }
}

#[derive(Debug, Clone, Default)]
pub struct WeaponIndex {
    defs: BTreeMap<String, WeaponDef>,
    definitions: usize,
}

impl WeaponIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_document(&mut self, document: &Document) {
        for block in document.roots() {
            self.insert_block(block);
        }
    }

    pub fn insert_block(&mut self, block: &Block) {
        if !block.keyword.eq(WEAPON_KEYWORD) {
            return;
        }
        let Some(name) = block.name() else {
            return;
        };
        self.definitions = self.definitions.saturating_add(1);
        self.defs
            .insert(name.to_ascii_lowercase(), read_weapon(block));
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&WeaponDef> {
        self.defs.get(&name.to_ascii_lowercase())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    #[must_use]
    pub fn definitions(&self) -> usize {
        self.definitions
    }
}

#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]
mod tests {
    use super::*;
    use crate::table::BlockTable;

    #[test]
    fn a_weapon_reads_its_top_fields_and_nugget_list() {
        let doc = crate::tree::Document::parse(
            "weapon.ini",
            "Weapon TrebuchetWeapon\n  \
                AttackRange = 1200.0\n  \
                DelayBetweenShots = 7000\n  \
                PreAttackDelay = 1133\n  \
                PreAttackType = PER_SHOT\n  \
                FireFX = FX_TrebuchetImpactHit\n  \
                RadiusDamageAffects = ENEMIES NEUTRALS ALLIES NOT_SIMILAR\n  \
                WeaponSpeed = 321\n  \
                RangeBonusPerFoot = #MULTIPLY( LEGOLAS_BOW_RANGE .01 )\n  \
                DamageNugget\n    \
                    Damage = 100\n  \
                End\n  \
                ProjectileNugget\n    \
                    ProjectileTemplateName = TrebuchetProjectile\n  \
                End\n\
            End\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let def = read_weapon(&doc.roots()[0]);
        assert_eq!(def.attack_range, Some(1200.0));
        assert_eq!(
            def.delay_between_shots,
            Some(Duration::Milliseconds(7000.0))
        );
        assert_eq!(def.pre_attack_delay, Some(Duration::Milliseconds(1133.0)));
        assert_eq!(def.fire_fx.as_deref(), Some("FX_TrebuchetImpactHit"));
        assert_eq!(
            def.radius_damage_affects,
            vec!["ENEMIES", "NEUTRALS", "ALLIES", "NOT_SIMILAR"]
        );
        assert_eq!(def.weapon_speed, Some(321.0));
        assert_eq!(
            def.range_bonus_per_foot_raw.as_deref(),
            Some("#MULTIPLY( LEGOLAS_BOW_RANGE .01 )")
        );
        assert_eq!(def.nuggets, vec!["DamageNugget", "ProjectileNugget"]);
    }

    #[test]
    fn fire_fx_of_none_reads_as_absent() {
        let doc = crate::tree::Document::parse(
            "weapon.ini",
            "Weapon Silent\n  FireFX = NONE\nEnd\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        let def = read_weapon(&doc.roots()[0]);
        assert_eq!(def.fire_fx, None);
    }

    #[test]
    fn a_linear_target_reads_its_three_tagged_components() {
        let doc = crate::tree::Document::parse(
            "weapon.ini",
            "Weapon Straight\n  LinearTarget = X: 0.0 Y: 0.0 T:5\nEnd\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        let def = read_weapon(&doc.roots()[0]);
        let target = def.linear_target.unwrap();
        assert_eq!(target.x, Some(0.0));
        assert_eq!(target.t, Some(5.0));
    }
}
