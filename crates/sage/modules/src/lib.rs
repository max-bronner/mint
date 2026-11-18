#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

mod activate_module_special_power;
mod ai_gate_update;
mod ai_power_types;
mod ai_special_power_update;
mod aim_weapon_behavior;
mod armor_upgrade;
mod arrow_storm_update;
mod attach_update;
mod attribute_modifier_aura_update;
mod attribute_modifier_upgrade;
mod auto_ability_behavior;
mod auto_heal_behavior;
mod auto_pick_up_update;
mod banner_carrier_update;
mod bezier_projectile_behavior;
pub mod body;
mod buildable_hero_list_upgrade;
mod building_behavior;
pub mod captured;
mod castle_behavior;
mod castle_member_behavior;
mod castle_upgrade;
mod command_points_upgrade;
mod command_set_upgrade;
pub mod contain;
mod cost_modifier_upgrade;
mod curse_special_power;
mod destroy_die;
pub mod die_mux;
mod do_command_upgrade;
mod dominate_enemy_special_power;
mod dual_weapon_behavior;
mod dynamic_portal_behaviour;
pub mod effects;
mod emotion_tracker_update;
pub mod experience_record;
pub mod experience_scalar;
mod fake_pathfind_portal_behaviour;
mod fields;
mod fire_weapon_update;
pub mod flammable;
mod fling_passenger_special_ability_update;
mod flood_update;
mod fx_list_die;
mod gate_open_and_close_behavior;
mod geometry_upgrade;
mod getting_built_behavior;
mod grant_upgrade_create;
mod hero_mode_special_ability_update;
mod hit_reaction_behavior;
pub mod horde_ai_update;
mod inherit_upgrade_create;
mod invisibility_special_power;
mod invisibility_update;
mod keep_object_die;
mod large_group_audio_update;
mod large_group_bonus_update;
mod level_grant_special_power;
mod level_up_upgrade;
mod lifetime_update;
mod model_condition_special_ability_update;
mod model_condition_upgrade;
pub mod object_creation_upgrade;
pub mod object_filter;
pub mod object_scripts;
mod passive_area_effect_behavior;
mod physics_behavior;
mod pickup_stuff_update;
mod production_update;
mod radiate_fear_update;
mod rebuild_hole_behavior;
mod rebuild_hole_expose_die;
pub mod record;
mod refund_die;
mod register_effects;
pub mod register_modifiers;
mod remove_upgrade_upgrade;
mod replace_object_update;
mod replace_self_upgrade;
mod salvage_crate_collide;
pub mod script_events;
mod set_conditions;
pub mod slave_watcher_behavior;
pub mod slaved_update;
mod slow_death_behavior;
pub mod spawn_behavior;
mod spawn_point_production_exit_update;
mod special_ability_update;
mod special_disguise_update;
pub mod special_power_effects;
pub mod spell_powers;
mod spell_recharge_modifier_upgrade;
mod squish_collide;
mod stances_behavior;
mod status_bits_upgrade;
mod stealth_detector_update;
mod stealth_update;
mod store_objects_special_power;
mod structure_collapse_update;
mod sub_objects_upgrade;
mod teleport_special_ability_update;
mod teleport_to_caster_special_power;
pub mod template;
mod terrain_resource_behavior;
mod toggle_hidden_special_ability_update;
mod toggle_mounted_special_ability_update;
mod transition_damage_fx;
mod unpause_special_power_upgrade;
pub mod upgrade_bits;
pub mod upgrade_mux;
pub mod upgrade_record;
mod wall_hub_behavior;
mod weapon_change_special_power_module;
mod weapon_fire_special_ability_update;
mod weapon_mode_special_power_update;
mod weapon_set_upgrade;
pub mod world_reads;

pub use activate_module_special_power::{
    activate_module_special_power_from_fields, ActivateModuleSpecialPower,
};
pub use ai_gate_update::{ai_gate_update_from_fields, AIGateUpdate};
pub use ai_power_types::{
    ai_power_registry, AGGRESSIVE_MODE, SELF_BUFF_SCAN_RADIUS, STANCE_HEALTH_FLOOR,
};
pub use ai_special_power_update::{ai_special_power_update_from_fields, AISpecialPowerUpdate};
pub use aim_weapon_behavior::{
    aim_weapon_behavior_from_fields, AimWeaponBehavior, Distance as AimDistance, Pitch as AimPitch,
    ALL as AIM_CONDITIONS,
};
pub use armor_upgrade::{armor_upgrade_from_fields, ArmorUpgrade, DEFAULT_UPGRADE_FLAG};
pub use arrow_storm_update::{arrow_storm_update_from_fields, ArrowStormUpdate};
pub use attach_update::{attach_update_from_fields, AttachUpdate};
pub use attribute_modifier_aura_update::{
    attribute_modifier_aura_update_from_fields, AttributeModifierAuraUpdate,
};
pub use attribute_modifier_upgrade::{
    attribute_modifier_upgrade_from_fields, AttributeModifierUpgrade,
};
pub use auto_ability_behavior::{
    auto_ability_behavior_from_fields, AutoAbilityBehavior, Query, UNBOUNDED_RANGE,
};
pub use auto_heal_behavior::{auto_heal_behavior_from_fields, AutoHealBehavior};
pub use auto_pick_up_update::{auto_pick_up_update_from_fields, AutoPickUpUpdate, EatObjectEntry};
pub use banner_carrier_update::{
    banner_carrier_update_from_fields, BannerCarrierUpdate, BannerClocks,
};
pub use bezier_projectile_behavior::{
    bezier_projectile_behavior_from_fields, ArcFields, BezierProjectileBehavior,
};
pub use body::{
    active_body_from_fields, delayed_death_body_from_fields, detachable_rider_body_from_fields,
    highlander_body_from_fields, immortal_body_from_fields, inactive_body_from_fields,
    porcupine_formation_body_from_fields, respawn_body_from_fields, structure_body_from_fields,
    ActiveBody, DelayedDeathBody, DetachableRiderBody, HighlanderBody, ImmortalBody, InactiveBody,
    PorcupineFormationBodyModule, RespawnBody, StructureBody,
};
pub use buildable_hero_list_upgrade::{
    buildable_hero_list_upgrade_from_fields, BuildableHeroListUpgrade,
};
pub use building_behavior::{building_behavior_from_fields, BuildingBehavior};
pub use captured::{CapturedField, CapturedFields, CapturedModule};
pub use castle_behavior::{castle_behavior_from_fields, CastleBehavior};
pub use castle_member_behavior::{castle_member_behavior_from_fields, CastleMemberBehavior};
pub use castle_upgrade::{castle_upgrade_from_fields, CastleUpgrade};
pub use command_points_upgrade::{command_points_upgrade_from_fields, CommandPointsUpgrade};
pub use command_set_upgrade::{command_set_upgrade_from_fields, CommandSetUpgrade};
pub use contain::{
    citadel_slaughter_horde_contain_from_fields, horde_garrison_contain_from_fields,
    open_contain_from_fields, siege_engine_contain_from_fields,
    slaughter_horde_contain_from_fields, transport_contain_from_fields, tunnel_contain_from_fields,
    CitadelSlaughterHordeContain, HordeGarrisonContain, InitialCrew, Offset, OpenContain,
    RingEntry, SiegeEngineContain, SlaughterHordeContain, TransportContain, TunnelContain,
};
pub use cost_modifier_upgrade::CostModifierRecord;
pub use curse_special_power::{curse_special_power_from_fields, CurseSpecialPower};
pub use destroy_die::{destroy_die_from_fields, DestroyDie};
pub use die_mux::DieMuxData;
pub use do_command_upgrade::{do_command_upgrade_from_fields, DoCommandUpgrade};
pub use dominate_enemy_special_power::{
    dominate_enemy_special_power_from_fields, DominateEnemySpecialPower,
};
pub use dual_weapon_behavior::{dual_weapon_behavior_from_fields, DualWeaponBehavior};
pub use dynamic_portal_behaviour::{
    dynamic_portal_behaviour_from_fields, DynamicPortalBehaviour, Link, WayPoint, WayPointType,
};
pub use effects::{
    CastEdge, Changed, CreateLocation, Effect, EffectLog, ModifierRule, OclFrom, PowerPlace,
    PowerSides, ReplaceCarry, TextureSwap, UpgradeState, WeatherKind,
};
pub use emotion_tracker_update::{
    emotion_tracker_update_from_fields, frightens, points_at, taunt_excluded, EmotionTrackerUpdate,
};
pub use experience_record::{
    ExperienceReport, ExperienceTables, LevelRecord, LevelUpTint, SelectionDecal,
    EXPERIENCE_LEVEL_KEYWORD,
};
pub use experience_scalar::{
    ExperienceScalarReport, ExperienceScalarTables, EXPERIENCE_SCALAR_TABLE_KEYWORD,
};
pub use fake_pathfind_portal_behaviour::{
    fake_pathfind_portal_behaviour_from_fields, FakePathfindPortalBehaviour,
};
pub use fields::BlockFields;
pub use fire_weapon_update::{
    deletion_update_from_fields, fire_weapon_update_from_fields, DeletionUpdate, FireWeaponUpdate,
    MAX_FIRE_NUGGETS,
};
pub use fling_passenger_special_ability_update::{
    fling_passenger_special_ability_update_from_fields, FlingPassengerSpecialAbilityUpdate,
};
pub use flood_update::{flood_update_from_fields, FloodMember, FloodSpec, FloodUpdate};
pub use fx_list_die::{fx_list_die_from_fields, FxListDie};
pub use gate_open_and_close_behavior::{
    gate_open_and_close_behavior_from_fields, GateOpenAndCloseBehavior,
};
pub use geometry_upgrade::{geometry_upgrade_from_fields, GeometryUpgrade};
pub use getting_built_behavior::{getting_built_behavior_from_fields, GettingBuiltBehavior};
pub use grant_upgrade_create::{grant_upgrade_create_from_fields, GrantUpgradeCreate};
pub use hero_mode_special_ability_update::{
    hero_mode_special_ability_update_from_fields, HeroModeSpecialAbilityUpdate,
};
pub use hit_reaction_behavior::{
    hit_level_condition, hit_reaction_behavior_from_fields, HitReactionBehavior,
    HIT_REACTION_CONDITION,
};
pub use horde_ai_update::{
    ai_update_from_fields, follow_through_from_fields, grab_toss_from_fields, AiUpdate,
    FollowThrough, GrabToss, AI_MODULE_NAMES,
};
pub use inherit_upgrade_create::{inherit_upgrade_create_from_fields, InheritUpgradeCreate};
pub use invisibility_special_power::{
    invisibility_special_power_from_fields, InvisibilitySpecialPower,
};
pub use invisibility_update::{
    invisibility_update_from_fields, InvisibilityNugget, InvisibilityOption, InvisibilityType,
    InvisibilityUpdate, StealthCondition,
};
pub use keep_object_die::{keep_object_die_from_fields, KeepObjectDie};
pub use large_group_audio_update::{large_group_audio_update_from_fields, LargeGroupAudioUpdate};
pub use large_group_bonus_update::{large_group_bonus_update_from_fields, LargeGroupBonusUpdate};
pub use level_grant_special_power::{
    level_grant_special_power_from_fields, LevelGrantSpecialPower,
};
pub use level_up_upgrade::{level_up_upgrade_from_fields, LevelUpUpgrade};
pub use lifetime_update::{lifetime_update_from_fields, LifetimeUpdate};
pub use model_condition_special_ability_update::{
    model_condition_special_ability_update_from_fields, ModelConditionSpecialAbilityUpdate,
};
pub use model_condition_upgrade::{model_condition_upgrade_from_fields, ModelConditionUpgrade};
pub use object_creation_upgrade::{object_creation_upgrade_from_fields, ObjectCreationUpgrade};
pub use object_filter::{ObjectFilter, Relation};
pub use passive_area_effect_behavior::{
    passive_area_effect_behavior_from_fields, PassiveAreaEffectBehavior,
};
pub use physics_behavior::{physics_behavior_from_fields, PhysicsBehavior};
pub use pickup_stuff_update::{pickup_stuff_update_from_fields, PickupStuffUpdate};
pub use production_update::{
    production_update_from_fields, ProductionDoors, ProductionUpdate, DOOR_COUNT_MAX,
};
pub use radiate_fear_update::{radiate_fear_update_from_fields, RadiateFearUpdate};
pub use rebuild_hole_behavior::{rebuild_hole_behavior_from_fields, RebuildHoleBehavior};
pub use rebuild_hole_expose_die::{rebuild_hole_expose_die_from_fields, RebuildHoleExposeDie};
pub use record::{
    dozer_constructs, foundation_constructs, is_ai_module, nearest_labelled_point, object_upgrades,
    player_upgrades, purchase_science_buttons, revive_buttons, special_power_buttons,
    spell_book_buttons, unit_builds, weapon_toggle_buttons, AiKindOf, AiTable, AnimalWander,
    AodCrush, AutoAcquire, BackAway, BuildCompletion, BuildableStatus, Castle, ConditionedSet,
    ContactLabels, ContactPoint, ContactTags, CritterEmitter, Crush, CrushResistance,
    DeadCollideSize, DeployStyleDef, FormationPreviewDecal, Foundation, Geometry, GeometryPiece,
    HarvestDef, LocomotorSlot, ObjectRecord, PieceShape, PowerButton, PowerSlot,
    ProductionModifierDef, RadarPriority, RespawnEntry, RespawnRule, SiegeDeployDef, SiegeRecord,
    SupplyCenterDockDef, TemplateRecords, ToggleDeployDef, TurretDef, UnitBuild, UpgradeButton,
    VisionBonus, DOZER_CONSTRUCT, FOUNDATION_CONSTRUCT, OBJECT_UPGRADE, PLAYER_UPGRADE, REVIVE,
    UNCRUSHABLE,
};
pub use refund_die::{refund_die_from_fields, RefundDie};
pub use register_effects::{
    create_object_die_from_fields, damage_filtered_create_object_die_from_fields,
    fire_weapon_when_dead_behavior_from_fields, register_effects, CreateObjectDie,
    DamageFilteredCreateObjectDie, FireWeaponWhenDeadBehavior,
};
pub use remove_upgrade_upgrade::{remove_upgrade_upgrade_from_fields, RemoveUpgradeUpgrade};
pub use replace_self_upgrade::{replace_self_upgrade_from_fields, ReplaceSelfUpgrade};
pub use salvage_crate_collide::{
    salvage_crate_collide_from_fields, SalvageCrateCollide, SalvageCrateData,
};
pub use slave_watcher_behavior::{slave_watcher_behavior_from_fields, SlaveWatcherBehavior};
pub use slaved_update::{slaved_update_from_fields, SlavedUpdate};
pub use slow_death_behavior::{
    sink_delay_from_fields, sink_rate_from_fields, slow_death_behavior_from_fields,
    SlowDeathBehavior,
};
pub use spawn_behavior::{is_slaved, spawn_behavior_from_fields, SlavedTo, SpawnBehavior};
pub use spawn_point_production_exit_update::{
    spawn_point_production_exit_update_from_fields, SpawnPointProductionExitUpdate,
    MAX_SPAWN_POINTS,
};
pub use special_ability_update::{
    grab_passenger_eat, retail_duration_ticks, retail_duration_ticks_f32,
    special_ability_update_from_fields, GrabPassengerEat, SpecialAbilityUpdate, RETAIL_LOGIC_HZ,
};
pub use special_disguise_update::{
    special_disguise_update_from_fields, SpecialDisguiseUpdate, DISGUISED,
};
pub use spell_recharge_modifier_upgrade::{
    spell_recharge_modifier_upgrade_from_fields, SpellRechargeModifierUpgrade,
};
pub use squish_collide::{squish_collide_from_fields, SquishCollide};
pub use stances_behavior::{stances_behavior_from_fields, StancesBehavior};
pub use status_bits_upgrade::{status_bits_upgrade_from_fields, StatusBitsUpgrade};
pub use stealth_detector_update::{stealth_detector_update_from_fields, StealthDetectorUpdate};
pub use stealth_update::{stealth_update_from_fields, StealthUpdate};
pub use structure_collapse_update::{
    structure_collapse_update_from_fields, CollapseState, StructureCollapseUpdate,
};
pub use sub_objects_upgrade::{sub_objects_upgrade_from_fields, SubObjectsUpgrade};
pub use teleport_special_ability_update::{
    teleport_special_ability_update_from_fields, TeleportSpecialAbilityUpdate,
};
pub use template::{spawn_template, ModuleDeclaration, OverrideLoad, TemplateIndex};
pub use terrain_resource_behavior::{
    terrain_resource_behavior_from_fields, TerrainResourceBehavior,
};
pub use toggle_hidden_special_ability_update::{
    toggle_hidden_special_ability_update_from_fields, ToggleHiddenSpecialAbilityUpdate,
};
pub use toggle_mounted_special_ability_update::{
    toggle_mounted_special_ability_update_from_fields, ToggleMountedSpecialAbilityUpdate,
};
pub use transition_damage_fx::{
    transition_damage_fx_from_fields, transitions as transition_damage_fx_transitions,
    DamageFxLocation, DamageFxSlot, TdfxTransition, TransitionDamageFX,
};
pub use unpause_special_power_upgrade::{
    unpause_special_power_upgrade_from_fields, UnpauseSpecialPowerUpgrade,
};
pub use upgrade_bits::UpgradeBits;
pub use upgrade_mux::{attempt_upgrade_via_mux, UpgradeEffect, UpgradeMuxData};
pub use upgrade_record::{build_upgrade_costs, build_upgrade_records, UpgradeCost, UpgradeRecord};
pub use wall_hub_behavior::{
    wall_hub_behavior_from_fields, ModuleOption, WallHubBehavior, BUILDER_RADIUS_USES_HUB,
};
pub use weapon_change_special_power_module::{
    weapon_change_special_power_module_from_fields, WeaponChangeSpecialPowerModule,
};
pub use weapon_fire_special_ability_update::{
    special_weapon_condition, weapon_fire_special_ability_update_from_fields,
    WeaponFireSpecialAbilityUpdate,
};
pub use weapon_mode_special_power_update::{
    weapon_mode_special_power_update_from_fields, WeaponModeSpecialPowerUpdate,
};
pub use weapon_set_upgrade::{weapon_set_upgrade_from_fields, WeaponSetUpgrade};
pub use world_reads::{
    Hordes, Immobile, MasterSlaves, MasterView, ModelConditions, Movers, NeutralControlled,
    PlayerOrdered, Positions, SlaveView, Slaves, StillMoving, Structures, UnderConstruction,
    ViewBlocked,
};

use mint_sim::module::ModuleRegistry;
use mint_sim::World;

pub fn register_body_and_death_basics(registry: &mut ModuleRegistry, world: &mut World) {
    register_body_family(registry, world);
    registry.register_loadable(world, "DestroyDie", destroy_die::destroy_die_from_fields);
    registry.register_loadable(
        world,
        "KeepObjectDie",
        keep_object_die::keep_object_die_from_fields,
    );
    registry.register_loadable(
        world,
        "LifetimeUpdate",
        lifetime_update::lifetime_update_from_fields,
    );
    registry.register_loadable(
        world,
        "SlowDeathBehavior",
        slow_death_behavior::slow_death_behavior_from_fields,
    );
}

pub fn register_body_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(world, "ActiveBody", body::active_body_from_fields);
    registry.register_loadable(world, "StructureBody", body::structure_body_from_fields);
    registry.register_loadable(world, "ImmortalBody", body::immortal_body_from_fields);
    registry.register_loadable(world, "HighlanderBody", body::highlander_body_from_fields);
    registry.register_loadable(world, "InactiveBody", body::inactive_body_from_fields);
    registry.register_loadable(world, "RespawnBody", body::respawn_body_from_fields);
    registry.register_loadable(
        world,
        "DelayedDeathBody",
        body::delayed_death_body_from_fields,
    );
    registry.register_loadable(
        world,
        "DetachableRiderBody",
        body::detachable_rider_body_from_fields,
    );
    registry.register_loadable(
        world,
        "PorcupineFormationBodyModule",
        body::porcupine_formation_body_from_fields,
    );
}

pub fn register_physics_and_collapse(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "PhysicsBehavior",
        physics_behavior::physics_behavior_from_fields,
    );
    registry.register_loadable(
        world,
        "SquishCollide",
        squish_collide::squish_collide_from_fields,
    );
    registry.register_loadable(
        world,
        "StructureCollapseUpdate",
        structure_collapse_update::structure_collapse_update_from_fields,
    );
    registry.register_loadable(world, "FXListDie", fx_list_die::fx_list_die_from_fields);
}

pub fn register_death_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "StructureCollapseUpdate",
        structure_collapse_update::structure_collapse_update_from_fields,
    );
    registry.register_loadable(world, "FXListDie", fx_list_die::fx_list_die_from_fields);
    registry.register_loadable(world, "RefundDie", refund_die::refund_die_from_fields);
}

pub fn register_rebuild_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "RebuildHoleExposeDie",
        rebuild_hole_expose_die::rebuild_hole_expose_die_from_fields,
    );
    registry.register_loadable(
        world,
        "RebuildHoleBehavior",
        rebuild_hole_behavior::rebuild_hole_behavior_from_fields,
    );
}

pub fn register_upgrade_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(world, "SubObjectsUpgrade", sub_objects_upgrade_from_fields);
    registry.register_loadable(
        world,
        "RemoveUpgradeUpgrade",
        remove_upgrade_upgrade_from_fields,
    );
    registry.register_loadable(
        world,
        "BuildableHeroListUpgrade",
        buildable_hero_list_upgrade_from_fields,
    );
    registry.register_loadable(world, "CommandSetUpgrade", command_set_upgrade_from_fields);
    registry.register_loadable(world, "WeaponSetUpgrade", weapon_set_upgrade_from_fields);
    registry.register_loadable(world, "ArmorUpgrade", armor_upgrade_from_fields);
    registry.register_loadable(
        world,
        "ModelConditionUpgrade",
        model_condition_upgrade_from_fields,
    );
    registry.register_loadable(
        world,
        "AttributeModifierUpgrade",
        attribute_modifier_upgrade_from_fields,
    );
    registry.register_loadable(world, "GeometryUpgrade", geometry_upgrade_from_fields);
    registry.register_loadable(world, "LevelUpUpgrade", level_up_upgrade_from_fields);
    registry.register_loadable(world, "StatusBitsUpgrade", status_bits_upgrade_from_fields);
    registry.register_loadable(
        world,
        "UnpauseSpecialPowerUpgrade",
        unpause_special_power_upgrade_from_fields,
    );
    registry.register_loadable(
        world,
        "GrantUpgradeCreate",
        grant_upgrade_create_from_fields,
    );
}

pub fn register_power_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "OCLSpecialPower",
        special_power_effects::ocl_special_power_from_fields,
    );
    registry.register_loadable(
        world,
        "PlayerHealSpecialPower",
        special_power_effects::player_heal_special_power_from_fields,
    );
    registry.register_loadable(
        world,
        "SpecialPowerModule",
        special_power_effects::special_power_module_from_fields,
    );
    registry.register_loadable(
        world,
        "HordeDispatchSpecialPower",
        special_power_effects::special_power_module_from_fields,
    );
    registry.register_loadable(
        world,
        "PlayerUpgradeSpecialPower",
        special_power_effects::player_upgrade_special_power_from_fields,
    );
}

pub fn register_spell_family(registry: &mut ModuleRegistry, world: &mut World) {
    use spell_powers as s;
    registry.register_loadable(
        world,
        "TaintSpecialPower",
        s::taint_special_power_from_fields,
    );
    registry.register_loadable(
        world,
        "ElvenWoodSpecialPower",
        s::elven_wood_special_power_from_fields,
    );
    registry.register_loadable(
        world,
        "DarknessSpecialPower",
        s::darkness_special_power_from_fields,
    );
    registry.register_loadable(
        world,
        "FreezingRainSpecialPower",
        s::freezing_rain_special_power_from_fields,
    );
    registry.register_loadable(
        world,
        "CloudBreakSpecialPower",
        s::cloud_break_special_power_from_fields,
    );
    registry.register_loadable(
        world,
        "DevastateSpecialPower",
        s::devastate_special_power_from_fields,
    );
    registry.register_loadable(
        world,
        "ScavengerSpecialPower",
        s::scavenger_special_power_from_fields,
    );
    registry.register_loadable(
        world,
        "UntamedAllegianceSpecialPower",
        s::untamed_allegiance_special_power_from_fields,
    );
    registry.register_loadable(
        world,
        "ProductionSpeedBonus",
        s::production_speed_bonus_from_fields,
    );
    registry.register_loadable(world, "FireWeaponUpdate", fire_weapon_update_from_fields);
    registry.register_loadable(world, "DeletionUpdate", deletion_update_from_fields);
    registry.register_loadable(world, "FloodUpdate", flood_update_from_fields);
    registry.register_loadable(
        world,
        "AttributeModifierAuraUpdate",
        attribute_modifier_aura_update_from_fields,
    );
    registry.register_loadable(world, "AutoHealBehavior", auto_heal_behavior_from_fields);
}

pub fn register_data_holders(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "LargeGroupAudioUpdate",
        large_group_audio_update_from_fields,
    );
    registry.register_loadable(
        world,
        "GettingBuiltBehavior",
        getting_built_behavior_from_fields,
    );
    registry.register_loadable(
        world,
        "TerrainResourceBehavior",
        terrain_resource_behavior_from_fields,
    );
    registry.register_loadable(world, "CastleBehavior", castle_behavior_from_fields);
    registry.register_loadable(
        world,
        "CastleMemberBehavior",
        castle_member_behavior_from_fields,
    );
    registry.register_loadable(world, "BuildingBehavior", building_behavior_from_fields);
    registry.register_loadable(
        world,
        "AttributeModifierAuraUpdate",
        attribute_modifier_aura_update_from_fields,
    );
    registry.register_loadable(
        world,
        "TransitionDamageFX",
        transition_damage_fx_from_fields,
    );
}

pub fn register_ability_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "SpecialAbilityUpdate",
        special_ability_update::special_ability_update_from_fields,
    );
    registry.register_loadable(
        world,
        "WeaponModeSpecialPowerUpdate",
        weapon_mode_special_power_update::weapon_mode_special_power_update_from_fields,
    );
    registry.register_loadable(
        world,
        "HeroModeSpecialAbilityUpdate",
        hero_mode_special_ability_update::hero_mode_special_ability_update_from_fields,
    );
    registry.register_loadable(
        world,
        "WeaponFireSpecialAbilityUpdate",
        weapon_fire_special_ability_update::weapon_fire_special_ability_update_from_fields,
    );
    registry.register_loadable(
        world,
        "ToggleMountedSpecialAbilityUpdate",
        toggle_mounted_special_ability_update::toggle_mounted_special_ability_update_from_fields,
    );
    registry.register_loadable(
        world,
        "SpecialDisguiseUpdate",
        special_disguise_update::special_disguise_update_from_fields,
    );
}

pub fn register_spawn_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(world, "SpawnBehavior", spawn_behavior_from_fields);
    registry.register_loadable(world, "SlavedUpdate", slaved_update_from_fields);
}

pub fn register_object_creation_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "ObjectCreationUpgrade",
        object_creation_upgrade_from_fields,
    );
    registry.register_loadable(
        world,
        "SlaveWatcherBehavior",
        slave_watcher_behavior_from_fields,
    );
    registry.register_loadable(
        world,
        "CommandPointsUpgrade",
        command_points_upgrade_from_fields,
    );
}

pub fn register_do_command_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(world, "DoCommandUpgrade", do_command_upgrade_from_fields);
}

pub fn register_auto_ability_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "AutoAbilityBehavior",
        auto_ability_behavior::auto_ability_behavior_from_fields,
    );
}

pub fn register_castle_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(world, "CastleUpgrade", castle_upgrade_from_fields);
    registry.register_loadable(
        world,
        "InheritUpgradeCreate",
        inherit_upgrade_create_from_fields,
    );
}

pub fn register_ability_family_2(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "LevelGrantSpecialPower",
        level_grant_special_power::level_grant_special_power_from_fields,
    );
    registry.register_loadable(
        world,
        "ActivateModuleSpecialPower",
        activate_module_special_power::activate_module_special_power_from_fields,
    );
    registry.register_loadable(
        world,
        "FellBeastSwoopPower",
        special_ability_update::special_ability_update_from_fields,
    );
    registry.register_loadable(
        world,
        "SiegeDeployHordeSpecialPower",
        special_power_effects::special_power_module_from_fields,
    );
}

pub fn register_ability_family_3(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "DominateEnemySpecialPower",
        dominate_enemy_special_power::dominate_enemy_special_power_from_fields,
    );
    registry.register_loadable(
        world,
        "CurseSpecialPower",
        curse_special_power::curse_special_power_from_fields,
    );
}

pub fn register_ability_family_4(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "ArrowStormUpdate",
        arrow_storm_update::arrow_storm_update_from_fields,
    );
    registry.register_loadable(
        world,
        "SpecialPowerTimerRefreshSpecialPower",
        special_power_effects::special_power_timer_refresh_special_power_from_fields,
    );
}

pub fn register_structure_conversion_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "ReplaceSelfUpgrade",
        replace_self_upgrade::replace_self_upgrade_from_fields,
    );
    registry.register_loadable(
        world,
        "GateOpenAndCloseBehavior",
        gate_open_and_close_behavior::gate_open_and_close_behavior_from_fields,
    );
    registry.register_loadable(
        world,
        "AIGateUpdate",
        ai_gate_update::ai_gate_update_from_fields,
    );
    registry.register_loadable(
        world,
        "FakePathfindPortalBehaviour",
        fake_pathfind_portal_behaviour::fake_pathfind_portal_behaviour_from_fields,
    );
    registry.register_loadable(
        world,
        "DynamicPortalBehaviour",
        dynamic_portal_behaviour::dynamic_portal_behaviour_from_fields,
    );
}

pub fn register_unit_emotion_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "EmotionTrackerUpdate",
        emotion_tracker_update::emotion_tracker_update_from_fields,
    );
}

pub fn register_fear_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "RadiateFearUpdate",
        radiate_fear_update::radiate_fear_update_from_fields,
    );
    registry.register_loadable(
        world,
        "ModelConditionSpecialAbilityUpdate",
        model_condition_special_ability_update::model_condition_special_ability_update_from_fields,
    );
}

pub fn register_unit_behaviour_family(registry: &mut ModuleRegistry, world: &mut World) {
    for name in [
        "HordeAIUpdate",
        "HordeWorkerAIUpdate",
        "SiegeAIUpdate",
        "TransportAIUpdate",
    ] {
        registry.register_loadable(world, name, horde_ai_update::ai_update_from_fields);
    }
    registry.register_loadable(
        world,
        "BannerCarrierUpdate",
        banner_carrier_update::banner_carrier_update_from_fields,
    );
}

pub fn register_ai_family(registry: &mut ModuleRegistry, world: &mut World) {
    for name in AI_MODULE_NAMES {
        registry.register_loadable(world, name, horde_ai_update::ai_update_from_fields);
    }
}

pub fn register_weapon_behaviour_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "DualWeaponBehavior",
        dual_weapon_behavior::dual_weapon_behavior_from_fields,
    );
    registry.register_loadable(
        world,
        "AimWeaponBehavior",
        aim_weapon_behavior::aim_weapon_behavior_from_fields,
    );
}

pub fn register_live_families(registry: &mut ModuleRegistry, world: &mut World) {
    register_upgrade_family(registry, world);
    register_power_family(registry, world);
    register_spell_family(registry, world);
    register_death_family(registry, world);
    register_modifiers::register_modifiers(registry, world);
    register_effects(registry, world);
    register_ability_family(registry, world);
    register_spawn_family(registry, world);
    register_rebuild_family(registry, world);
    register_object_creation_family(registry, world);
    register_do_command_family(registry, world);
    register_auto_ability_family(registry, world);
    register_castle_family(registry, world);
    register_ability_family_2(registry, world);
    register_ability_family_3(registry, world);
    register_ability_family_4(registry, world);
    register_structure_conversion_family(registry, world);
    register_wall_family(registry, world);
    register_unit_emotion_family(registry, world);
    register_unit_behaviour_family(registry, world);
    register_contain_family(registry, world);
    register_fear_family(registry, world);
    register_projectile_family(registry, world);
    register_stance_family(registry, world);
    register_ai_family(registry, world);
    register_weapon_behaviour_family(registry, world);
    register_stealth_family(registry, world);
    register_group_family(registry, world);
    register_detector_family(registry, world);
    register_production_exit_family(registry, world);
    register_production_door_family(registry, world);
    register_pick_up_family(registry, world);
    register_fling_family(registry, world);
    register_body_family(registry, world);
    register_ai_power_family(registry, world);
    register_teleport_family(registry, world);
    register_passive_area_effect_family(registry, world);
    register_neutral_building_family(registry, world);
    register_attach_family(registry, world);
    register_summon_replacement_family(registry, world);
    register_small_power_family(registry, world);
}

pub fn register_summon_replacement_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "SummonReplacementSpecialAbilityUpdate",
        toggle_mounted_special_ability_update::toggle_mounted_special_ability_update_from_fields,
    );
}

pub fn register_small_power_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "InvisibilitySpecialPower",
        invisibility_special_power::invisibility_special_power_from_fields,
    );
    registry.register_loadable(
        world,
        "WeaponChangeSpecialPowerModule",
        weapon_change_special_power_module::weapon_change_special_power_module_from_fields,
    );
}

pub fn register_attach_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(world, "AttachUpdate", attach_update_from_fields);
}

pub fn register_ai_power_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "AISpecialPowerUpdate",
        ai_special_power_update::ai_special_power_update_from_fields,
    );
}

pub fn register_teleport_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "TeleportSpecialAbilityUpdate",
        teleport_special_ability_update::teleport_special_ability_update_from_fields,
    );
    registry.register_loadable(
        world,
        "StoreObjectsSpecialPower",
        store_objects_special_power::store_objects_special_power_from_fields,
    );
    registry.register_loadable(
        world,
        "TeleportToCasterSpecialPower",
        teleport_to_caster_special_power::teleport_to_caster_special_power_from_fields,
    );
    registry.register_loadable(
        world,
        "ReplaceObjectUpdate",
        replace_object_update::replace_object_update_from_fields,
    );
}

pub fn register_passive_area_effect_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "PassiveAreaEffectBehavior",
        passive_area_effect_behavior::passive_area_effect_behavior_from_fields,
    );
}

pub fn register_neutral_building_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "SpellRechargeModifierUpgrade",
        spell_recharge_modifier_upgrade_from_fields,
    );
    registry.register_loadable(
        world,
        "SalvageCrateCollide",
        salvage_crate_collide_from_fields,
    );
}

pub fn register_fling_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "FlingPassengerSpecialAbilityUpdate",
        fling_passenger_special_ability_update_from_fields,
    );
}

pub fn register_pick_up_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "GrabPassengerSpecialPower",
        special_power_effects::special_power_module_from_fields,
    );
    registry.register_loadable(world, "AutoPickUpUpdate", auto_pick_up_update_from_fields);
}

pub fn register_production_door_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(world, "ProductionUpdate", production_update_from_fields);
}

pub fn register_production_exit_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "SpawnPointProductionExitUpdate",
        spawn_point_production_exit_update_from_fields,
    );
}

pub fn register_group_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "LargeGroupBonusUpdate",
        large_group_bonus_update::large_group_bonus_update_from_fields,
    );
    registry.register_loadable(
        world,
        "HitReactionBehavior",
        hit_reaction_behavior::hit_reaction_behavior_from_fields,
    );
    registry.register_loadable(
        world,
        "PickupStuffUpdate",
        pickup_stuff_update::pickup_stuff_update_from_fields,
    );
}

pub fn register_stance_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "StancesBehavior",
        stances_behavior::stances_behavior_from_fields,
    );
}

pub const CONTAIN_MODULE_NAMES: &[&str] = &[
    "HordeGarrisonContain",
    "CitadelSlaughterHordeContain",
    "SlaughterHordeContain",
    "TunnelContain",
    "TransportContain",
    "HordeTransportContain",
    "SiegeEngineContain",
    "HordeSiegeEngineContain",
];

pub fn register_contain_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "HordeGarrisonContain",
        contain::horde_garrison_contain_from_fields,
    );
    registry.register_loadable(
        world,
        "CitadelSlaughterHordeContain",
        contain::citadel_slaughter_horde_contain_from_fields,
    );
    registry.register_loadable(
        world,
        "SlaughterHordeContain",
        contain::slaughter_horde_contain_from_fields,
    );
    registry.register_loadable(world, "TunnelContain", contain::tunnel_contain_from_fields);
    for name in ["TransportContain", "HordeTransportContain"] {
        registry.register_loadable(world, name, contain::transport_contain_from_fields);
    }
    for name in ["SiegeEngineContain", "HordeSiegeEngineContain"] {
        registry.register_loadable(world, name, contain::siege_engine_contain_from_fields);
    }
}

pub fn register_projectile_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "BezierProjectileBehavior",
        bezier_projectile_behavior::bezier_projectile_behavior_from_fields,
    );
}

pub fn register_stealth_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "InvisibilityUpdate",
        invisibility_update::invisibility_update_from_fields,
    );
}

pub fn register_detector_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "StealthDetectorUpdate",
        stealth_detector_update::stealth_detector_update_from_fields,
    );
    registry.register_loadable(
        world,
        "StealthUpdate",
        stealth_update::stealth_update_from_fields,
    );
    registry.register_loadable(
        world,
        "ToggleHiddenSpecialAbilityUpdate",
        toggle_hidden_special_ability_update::toggle_hidden_special_ability_update_from_fields,
    );
}

pub fn register_wall_family(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "WallHubBehavior",
        wall_hub_behavior::wall_hub_behavior_from_fields,
    );
}

pub fn register_every_family(registry: &mut ModuleRegistry, world: &mut World) {
    register_body_and_death_basics(registry, world);
    register_physics_and_collapse(registry, world);
    register_data_holders(registry, world);
    register_live_families(registry, world);
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sim::Frame;

    #[test]
    fn register_body_and_death_basics_registers_all_thirteen_names() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        register_body_and_death_basics(&mut registry, &mut world);
        for name in [
            "ActiveBody",
            "StructureBody",
            "ImmortalBody",
            "HighlanderBody",
            "InactiveBody",
            "RespawnBody",
            "DelayedDeathBody",
            "DetachableRiderBody",
            "PorcupineFormationBodyModule",
            "DestroyDie",
            "KeepObjectDie",
            "LifetimeUpdate",
            "SlowDeathBehavior",
        ] {
            assert!(registry.is_registered(name), "{name} was not registered");
        }
    }

    #[test]
    fn register_power_family_registers_horde_dispatch_special_power() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        register_power_family(&mut registry, &mut world);
        assert!(registry.is_registered("HordeDispatchSpecialPower"));
    }

    #[test]
    fn register_physics_and_collapse_registers_all_four_names() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        register_physics_and_collapse(&mut registry, &mut world);
        for name in [
            "PhysicsBehavior",
            "SquishCollide",
            "StructureCollapseUpdate",
            "FXListDie",
        ] {
            assert!(registry.is_registered(name), "{name} was not registered");
        }
    }

    #[test]
    fn a_real_template_with_no_unregistered_module_loads_with_no_stub() {
        let doc = mint_sage_ini::Document::parse(
            "t.ini",
            "Object Archer\n  \
                Body = ActiveBody Tag_01\n    MaxHealth = 100.0\n  End\n  \
                Behavior = SlowDeathBehavior Tag_02\n    ProbabilityModifier = 15\n  End\n  \
                Behavior = DestroyDie Tag_03\n  End\n\
            End\n",
            &mint_sage_ini::BlockTable::object_model_path(),
        )
        .unwrap();
        let mut index = TemplateIndex::new();
        index.insert_document(&doc);

        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        register_body_and_death_basics(&mut registry, &mut world);

        let entity = spawn_template(
            &mut world,
            &registry,
            Frame::ZERO,
            &index,
            &UpgradeBits::default(),
            "archer",
        )
        .unwrap();
        assert!(world.is_live(entity));
        assert!(
            registry.tally().is_empty(),
            "every declared module is registered; nothing should stub"
        );
    }

    #[test]
    fn a_synthetic_mod_object_with_made_up_names_loads_a_dynamic_portal() {
        let doc = mint_sage_ini::Document::parse(
            "t.ini",
            "Object ModPortalDoor\n  \
                Behavior = DynamicPortalBehaviour Tag_01\n    \
                    GenerateNow = Yes\n    \
                    ObjectFilter = ANY +ModCreature\n    \
                    BonePrefix = ModBone\n    \
                    NumberOfBones = 2\n    \
                    WayPoint = Index:0 Type:Walk\n    \
                    WayPoint = Index:1 Type:Walk\n    \
                    Link = From:0 To:1\n  \
                End\n\
            End\n",
            &mint_sage_ini::BlockTable::object_model_path(),
        )
        .unwrap();
        let mut index = TemplateIndex::new();
        index.insert_document(&doc);

        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        register_structure_conversion_family(&mut registry, &mut world);

        let entity = spawn_template(
            &mut world,
            &registry,
            Frame::ZERO,
            &index,
            &UpgradeBits::default(),
            "modportaldoor",
        )
        .unwrap();
        assert!(world.is_live(entity));
        assert!(
            registry.tally().is_empty(),
            "DynamicPortalBehaviour is registered; nothing should stub"
        );
    }

    #[test]
    fn register_upgrade_family_registers_all_eleven_names() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        register_upgrade_family(&mut registry, &mut world);
        for name in [
            "SubObjectsUpgrade",
            "RemoveUpgradeUpgrade",
            "BuildableHeroListUpgrade",
            "CommandSetUpgrade",
            "WeaponSetUpgrade",
            "ArmorUpgrade",
            "ModelConditionUpgrade",
            "AttributeModifierUpgrade",
            "GeometryUpgrade",
            "LevelUpUpgrade",
            "StatusBitsUpgrade",
            "UnpauseSpecialPowerUpgrade",
        ] {
            assert!(registry.is_registered(name), "{name} was not registered");
        }
    }

    #[test]
    fn register_data_holders_registers_all_eight_names() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        register_data_holders(&mut registry, &mut world);
        for name in [
            "LargeGroupAudioUpdate",
            "GettingBuiltBehavior",
            "TerrainResourceBehavior",
            "CastleBehavior",
            "CastleMemberBehavior",
            "BuildingBehavior",
            "AttributeModifierAuraUpdate",
            "TransitionDamageFX",
        ] {
            assert!(registry.is_registered(name), "{name} was not registered");
        }
    }

    #[test]
    fn an_object_with_several_upgrade_modules_loads_clean_and_fires_on_broadcast() {
        let upgrade_doc = mint_sage_ini::Document::parse(
            "upgrade.ini",
            "Upgrade Upgrade_StructureLevel2\nEnd\n",
            &mint_sage_ini::BlockTable::priority_blocks_path(),
        )
        .unwrap();

        let doc = mint_sage_ini::Document::parse(
            "t.ini",
            "Object Fortress\n  \
                Behavior = ArmorUpgrade Tag_01\n    TriggeredBy = Upgrade_StructureLevel2\n  End\n  \
                Behavior = WeaponSetUpgrade Tag_02\n    TriggeredBy = Upgrade_StructureLevel2\n  End\n  \
                Behavior = SubObjectsUpgrade Tag_03\n    \
                    TriggeredBy = Upgrade_StructureLevel2\n    ShowSubObjects = Sub_Flag\n  End\n\
            End\n",
            &mint_sage_ini::BlockTable::object_model_path(),
        )
        .unwrap();

        let mut index = TemplateIndex::new();
        index.insert_document(&upgrade_doc);
        index.insert_document(&doc);
        let upgrade_bits = UpgradeBits::from_index(index.upgrades());

        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        register_upgrade_family(&mut registry, &mut world);

        let entity = spawn_template(
            &mut world,
            &registry,
            Frame::ZERO,
            &index,
            &upgrade_bits,
            "fortress",
        )
        .unwrap();
        assert!(
            registry.tally().is_empty(),
            "every declared module is registered; nothing should stub"
        );

        let bit = upgrade_bits.bit("Upgrade_StructureLevel2").unwrap();
        let mut active = mint_sim::module::UpgradeMask::EMPTY;
        active.set(bit);
        let mut log = EffectLog::new();
        registry.broadcast_upgrade_into(
            &mut world,
            entity,
            &active,
            Frame::ZERO,
            mint_sim::Effects::into_sink(&mut log),
        );
        let effects: Vec<Effect> = log.drain().into_iter().map(|(_, effect)| effect).collect();
        assert_eq!(
            effects,
            vec![
                Effect::ArmorSetFlag("PLAYER_UPGRADE".into()),
                Effect::WeaponSetFlag("PLAYER_UPGRADE".into()),
                Effect::SubObjects {
                    show: vec![effects::Name::from("Sub_Flag")].into(),
                    hide: Vec::new().into(),
                    permanent: true,
                },
            ],
            "declaration order"
        );

        let armor_id = world.register::<mint_sim::module::ModuleColumn<ArmorUpgrade>>();
        let armor = world.store(armor_id).unwrap().get(entity).unwrap();
        assert!(armor.0[0].state.is_raised(), "ArmorUpgrade must have fired");

        let weapon_id = world.register::<mint_sim::module::ModuleColumn<WeaponSetUpgrade>>();
        let weapon = world.store(weapon_id).unwrap().get(entity).unwrap();
        assert!(
            weapon.0[0].state.is_raised(),
            "WeaponSetUpgrade must have fired"
        );

        registry.broadcast_upgrade(&mut world, entity, &active, Frame::new(1));
        let armor_again = world.store(armor_id).unwrap().get(entity).unwrap();
        assert!(armor_again.0[0].state.is_raised());
    }
}
