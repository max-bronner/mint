#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

pub mod armor;
pub mod attributes;
pub mod body;
pub mod collide;
pub mod damage;
pub mod geom;
pub mod load;
pub mod match_set;
pub mod nugget;
pub mod projectile;
pub mod slow_death;
pub mod target;
pub mod targeting;
pub mod upgrade_filter;
pub mod victim;
pub mod victim_test;
pub mod weapon;
pub mod weapon_bonus;
pub mod weapon_slots;

pub use armor::{ArmorId, ArmorStore, ArmorTemplate, UNARMOURED};
pub use attributes::{
    AttributeKind, Attributes, ListUpgrade, Modifier, ModifierId, ModifierList, ModifierStore,
    MODIFIER_LIST_KEYWORD,
};
pub use body::{BodyKind, DamageOutcome, DeathRecord, HealthPool};
pub use collide::{CollideClass, CollideMask};
pub use damage::{
    DamageFxTypeId, DamageFxTypeNames, DamageTypeId, DamageTypes, DeathTypeId, DeathTypes,
    MAX_NAMES,
};
pub use geom::{
    border_distance_squared, surface_distance_squared_2d, within_attack_range, BoxVolume, Point3,
};
pub use load::{CombatData, LoadReport};
pub use match_set::{best_match, ConditionMask, ConditionTable};
pub use nugget::{
    DamageContainedNugget, DamageNugget, DamageShape, DotNugget, EffectNugget, GrabNugget,
    MetaImpactNugget, ModifierNugget, Nugget, ProjectileNugget,
};
pub use projectile::{
    bezier_length, bezier_points, highest_terrain_along, BezierArc, ProjectileFlight,
    DEFAULT_FIRST_PERCENT_HEIGHT, DEFAULT_SECOND_PERCENT_HEIGHT,
};
pub use slow_death::{choose_slow_death, SlowDeathOption};
pub use target::{nearest_target, sort_candidates, sort_victims, Candidate};
pub use targeting::{can_target, Attacker, Mode, Victim};
pub use upgrade_filter::{UpgradeFilter, UpgradeFilters};
pub use victim::{RadiusAffects, Relation, VictimRuleTable, VictimRules};
pub use victim_test::VictimFacts;
pub use weapon::{
    scatter_landing, AntiMask, FrameRange, FxTrigger, LinearPoint, PreAttackType, RangeBonus,
    ShotOutcome, WeaponId, WeaponState, WeaponStatus, WeaponStore, WeaponTemplate,
};
pub use weapon_bonus::{compute_bonus, BonusConditions, WeaponBonus, WeaponBonusSet};
pub use weapon_slots::{
    choose_slot, choose_slot_without_victim, CommandSource, NoVictimSlot, SlotArm, SlotCandidate,
    SlotFitness, SourceMask, WeaponChoiceCriteria, WeaponSlot, SLOT_COUNT,
};
