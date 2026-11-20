use mint_sage_combat::{SourceMask, WeaponChoiceCriteria, WeaponSlot};
use mint_sage_ini::diagnostic::{ValueError, ValueErrors};
use mint_sage_ini::value::{
    bit_string_edits, field_expression, parse_bool, parse_float_or_macro, parse_int,
    parse_percent_shaped, resolve_percent, tagged_value, BitEdit, BIT_STRING_EXPECTED,
};
use mint_sage_ini::{Block, CommandButtonIndex, CommandSetIndex, Field, MacroTable};
use mint_sim::LogicFrames;

use crate::captured::{CapturedField, CapturedModule};
use crate::effects::{build_modifier_rule, names, ModifierFieldValues, ModifierRule};
use crate::set_conditions::{condition_flags, SetKind};
use crate::template::{ModuleDeclaration, TemplateIndex};

mod contact;
mod formation_preview;
mod geometry;
pub use contact::{nearest_labelled_point, ContactLabels, ContactPoint, ContactTags};
pub use formation_preview::FormationPreviewDecal;
pub use geometry::{Geometry, GeometryPiece, PieceShape};

pub const UNIT_BUILD: &str = "UNIT_BUILD";

const PRIMARY: &str = "PRIMARY";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisionBonus {
    pub per_foot: f32,
    pub max: f32,
    pub min: f32,
    pub test_radius: f32,
    pub test_segments: u8,
}

impl VisionBonus {
    const DEFAULT_MAX: f32 = 0.5;
    const DEFAULT_MIN: f32 = -0.5;
    const DEFAULT_TEST_RADIUS: f32 = 100.0;
    const DEFAULT_TEST_SEGMENTS: u8 = 8;
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Castle {
    pub unpack: Vec<(String, String)>,
    pub owned_filter: Vec<String>,
    pub pre_built: Vec<(String, i64)>,
    pub instant_unpack: bool,
    pub keep_death_kills_everything: bool,
    pub max_radius: Option<f32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Foundation {
    pub build_variation: Option<u8>,
}

impl Foundation {
    #[must_use]
    pub const fn build_variation_condition(&self) -> Option<&'static str> {
        match self.build_variation {
            Some(1) => Some("BUILD_VARIATION_ONE"),
            Some(2) => Some("BUILD_VARIATION_TWO"),
            _ => None,
        }
    }
}

impl Castle {
    #[must_use]
    pub fn for_side(&self, side: &str) -> Option<&str> {
        self.unpack
            .iter()
            .find(|(declared, _)| declared.eq_ignore_ascii_case(side))
            .map(|(_, castle)| castle.as_str())
    }

    #[must_use]
    pub fn keeps(&self, kind_of: &[String]) -> bool {
        let has = |token: &str| kind_of.iter().any(|kind| kind.eq_ignore_ascii_case(token));
        let mut mode = "ANY";
        let mut wanted: Vec<&str> = Vec::new();
        for token in &self.owned_filter {
            if let Some(kind) = token.strip_prefix('+') {
                wanted.push(kind);
            } else if let Some(kind) = token.strip_prefix('-') {
                if has(kind) {
                    return false;
                }
            } else {
                mode = token.as_str();
            }
        }
        if wanted.is_empty() {
            return true;
        }
        if mode.eq_ignore_ascii_case("ALL") {
            wanted.iter().all(|kind| has(kind))
        } else if mode.eq_ignore_ascii_case("NONE") {
            !wanted.iter().any(|kind| has(kind))
        } else {
            wanted.iter().any(|kind| has(kind))
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PowerSlot {
    pub power: String,
    pub starts_paused: bool,
    pub defers_recharge: bool,
    pub set_model_condition: Option<(String, LogicFrames)>,
    pub disable_during_anim: bool,
    pub initiate_sound: Option<String>,
    pub initiate_fx: Option<String>,
    pub on_trigger_recharge: Option<String>,
    pub chained_button: Option<String>,
    pub available_at_start: bool,
    pub obey_recharge_on_unpause: bool,
    pub unlock_upgrade: Option<String>,
    pub trigger_fx: Option<String>,
    pub modifier: Option<ModifierRule>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConditionedSet {
    pub conditions: Vec<String>,
    pub value: Option<String>,
    pub slots: Vec<SlotDeclaration>,
    pub damage_fx: Option<String>,
    pub ready_status_shared_within_set: bool,
    pub share_weapon_reload_time: bool,
    pub weapon_lock_shared_across_sets: bool,
    pub default_weapon_choice_criteria: Option<WeaponChoiceCriteria>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotDeclaration {
    pub slot: WeaponSlot,
    pub weapon: String,
    pub sources: SourceMask,
    pub preferred_against: Vec<String>,
    pub only_against: Vec<String>,
    pub only_in_condition: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LocomotorSlot {
    pub condition: String,
    pub locomotor: Option<String>,
    pub speed: Option<f32>,
}

fn upgraded_command_sets(modules: &[ModuleDeclaration<'_>]) -> Vec<String> {
    let mut sets: Vec<String> = Vec::new();
    for decl in modules
        .iter()
        .filter(|decl| decl.name.eq_ignore_ascii_case("CommandSetUpgrade"))
    {
        for keyword in ["CommandSet", "CommandSetAlt"] {
            let Some(set) = decl
                .block
                .field(keyword)
                .and_then(Field::value)
                .filter(|value| !value.eq_ignore_ascii_case("NONE"))
            else {
                continue;
            };
            if !sets.iter().any(|held| held.eq_ignore_ascii_case(set)) {
                sets.push(set.to_owned());
            }
        }
    }
    sets
}

pub const SPAWNED_OBJECT_FIELDS: [&str; 23] = [
    "GrabObject",
    "InitialPayload",
    "InitialCrew",
    "TaintObject",
    "ElvenGroveObject",
    "SunbeamObject",
    "HoleName",
    "ThingToSpawn",
    "WorkerName",
    "EvilWorkerName",
    "WorkerObjectName",
    "ReplaceWith",
    "AndThenAddA",
    "MountedTemplate",
    "SegmentTemplateName",
    "DefaultSegmentTemplateName",
    "HubCapTemplateName",
    "CliffCapTemplateName",
    "ShoreCapTemplateName",
    "BorderCapTemplateName",
    "ElevatedSegmentTemplateName",
    "AlternateFormation",
    "MemberTemplateName",
];

pub const UNCRUSHABLE: i8 = i8::MAX;

#[derive(Debug, Clone, PartialEq)]
pub struct Crush {
    pub crusher_level: u8,
    pub crushable_level: i8,
    pub mounted_crusher_level: Option<u8>,
    pub mounted_crushable_level: Option<i8>,
    pub min_velocity: f32,
    pub deceleration: f32,
    pub knockback: f32,
    pub z_factor: f32,
    pub ram_power: f32,
    pub ram_z_mult: f32,
    pub only_while_charging: bool,
    pub allies: bool,
    pub use_crush_attack: bool,
    pub weapon: Option<String>,
    pub revenge_weapon: Option<String>,
    pub resisted: Option<CrushResistance>,
}

impl Default for Crush {
    fn default() -> Self {
        Self {
            crusher_level: 0,
            crushable_level: UNCRUSHABLE,
            mounted_crusher_level: None,
            mounted_crushable_level: None,
            min_velocity: 0.01,
            deceleration: 0.0,
            knockback: 0.0,
            z_factor: 1.0,
            ram_power: 0.0,
            ram_z_mult: 0.0,
            only_while_charging: false,
            allies: false,
            use_crush_attack: true,
            weapon: None,
            revenge_weapon: None,
            resisted: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrushResistance {
    pub level: u8,
    pub weapon: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildableStatus {
    Yes,
    IgnorePrerequisites,
    No,
    OnlyByAi,
}

impl BuildableStatus {
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.trim();
        [
            ("Yes", Self::Yes),
            ("Ignore_Prerequisites", Self::IgnorePrerequisites),
            ("No", Self::No),
            ("Only_By_AI", Self::OnlyByAi),
        ]
        .into_iter()
        .find(|(word, _)| word.eq_ignore_ascii_case(name))
        .map(|(_, status)| status)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ObjectRecord {
    pub name: String,
    pub depth: usize,
    pub side: Option<String>,
    pub kind_of: Vec<String>,
    pub ai_kind_of: Option<AiKindOf>,
    pub geometry: Geometry,
    pub flammability: Option<mint_sage_ini::fire_logic_system::FireMaterial>,
    pub geometry_is_small: Option<bool>,
    pub can_path_through_gates: Option<bool>,
    pub rotation_anchor: Option<(f32, f32)>,
    pub pathfind_diameter: Option<f32>,
    pub fence_width: Option<f32>,
    pub max_health: Option<f32>,
    pub initial_health: Option<f32>,
    pub max_health_damaged: Option<f32>,
    pub max_health_really_damaged: Option<f32>,
    pub use_default_damage_settings: Option<bool>,
    pub entering_damaged_transition_ms: Option<f32>,
    pub entering_really_damaged_transition_ms: Option<f32>,
    pub dodge_percent: Option<f32>,
    pub cheer_radius: Option<f32>,
    pub damage_creations: Vec<crate::body::DamageCreation>,
    pub grab: crate::body::BodyGrab,
    pub permanently_killed_by_filter: Vec<String>,
    pub vision_range: Option<f32>,
    pub shroud_clearing_range: Option<f32>,
    pub vision_side: Option<f32>,
    pub vision_rear: Option<f32>,
    pub vision_bonus: Option<VisionBonus>,
    pub emotion_range: Option<f32>,
    pub voice_attack_charge_timeout_ms: Option<f32>,
    pub threat_value: Option<f32>,
    pub threat_level: Option<f32>,
    pub max_distance_for_engaged: Option<f32>,
    pub buildable: Option<BuildableStatus>,
    pub engaged_state_timeout_ms: Option<f32>,
    pub min_z_increase_for_voice_move_to_higher_ground: Option<f32>,
    pub campness_value: Option<f32>,
    pub campness_value_radius: Option<f32>,
    pub back_away: Option<BackAway>,
    pub siege: SiegeRecord,
    pub shockwave_resistance: Option<f32>,
    pub auto_acquire: AutoAcquire,
    pub mood_attack_check_ms: Option<f32>,
    pub crush: Crush,
    pub command_set: Option<String>,
    pub auto_resolve_unit_type: Option<String>,
    pub build_variations: Vec<String>,
    pub equivalent_to: Vec<String>,
    pub experience_scalar_table: Option<String>,
    pub build_cost: Option<f32>,
    pub bounty_value: Option<f32>,
    pub command_point_bonus: Option<f32>,
    pub refund_value: Option<f32>,
    pub build_time: Option<f32>,
    pub build_completion: Option<BuildCompletion>,
    pub command_points: Option<f32>,
    pub max_simultaneous_of_type: Option<f32>,
    pub placement_view_angle: Option<f32>,
    pub formation_width: Option<f32>,
    pub formation_depth: Option<f32>,
    pub firing_arc: Option<f32>,
    pub front_angle: Option<f32>,
    pub flanked_delay_ms: Option<f32>,
    pub flanked_duration_ms: Option<f32>,
    pub health_box_offset: Option<f32>,
    pub health_box_scale: Option<f32>,
    pub select_portrait: Option<String>,
    pub weapon: Option<String>,
    pub armor: Option<String>,
    pub locomotor_sets: Vec<LocomotorSlot>,
    pub income: Option<(f32, f32)>,
    pub claim_radius: Option<f32>,
    pub claim_decal_visible: bool,
    pub claim_high_priority: bool,
    pub claim_max_income: Option<f32>,
    pub income_scales_with_claim: bool,
    pub initial_capture_bonus: Option<f32>,
    pub income_gives_no_xp: bool,
    pub produces: bool,
    pub max_queue_entries: Option<f32>,
    pub production_gives_no_xp: bool,
    pub natural_rally_point: Option<[f32; 2]>,
    pub unit_create_point: Option<[f32; 2]>,
    pub production_exit_delay: Option<f32>,
    pub production_exit_angle: Option<f32>,
    pub no_exit_path: bool,
    pub production_initial_burst: Option<f32>,
    pub use_return_to_formation: bool,
    pub can_rally_to_slaughter: bool,
    pub dozer: bool,
    pub castle: Option<Castle>,
    pub radar_priority: RadarPriority,
    pub foundation: Option<Foundation>,
    pub modules: usize,
    pub button_image: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub hotkey: Option<String>,
    pub revive_text: Option<String>,
    pub recruit_text: Option<String>,
    pub powers: Vec<PowerSlot>,
    pub menu_point: Option<[f32; 3]>,
    pub contact_points: Vec<ContactPoint>,
    pub upgraded_command_sets: Vec<String>,
    pub weapon_sets: Vec<ConditionedSet>,
    pub armor_sets: Vec<ConditionedSet>,
    pub declared_modules: Vec<CapturedModule>,
    pub is_trainable: Option<bool>,
    pub is_auto_built: bool,
    pub hero_sort_order: Option<i32>,
    pub level_on_create: Option<u16>,
    pub level_on_create_mp_only: bool,
    pub respawn: Vec<RespawnRule>,
    pub respawn_entries: Vec<RespawnEntry>,
    pub death_anim: Option<String>,
    pub respawn_fx: Option<String>,
    pub death_fx: Option<String>,
    pub initial_spawn_fx: Option<String>,
    pub auto_respawn_at_object_filter: Vec<String>,
    pub production_modifiers: Vec<ProductionModifierDef>,
    pub cost_modifiers: Vec<crate::cost_modifier_upgrade::CostModifierRecord>,
    pub keeps_corpse: bool,
    pub keep_selectable_when_dead: bool,
    pub rebuild_seconds: Option<f32>,
    pub spawn_timer: Option<f32>,
    pub rebuild_cost_percent: [f32; 4],
    pub rebuild_when_dead: bool,
    pub disallow_rebuild_range: Option<f32>,
    pub disallow_rebuild_filter: Vec<String>,
    pub use_spawn_timer_without_worker: bool,
    pub worker_name: Option<String>,
    pub evil_worker_name: Option<String>,
    pub test_faction: bool,
    pub lua_events: Option<String>,
    pub lua_event_states: Vec<LuaEventState>,
    pub stance_template: Option<String>,
    pub ai: AiTable,
    pub camouflage_detection_multiplier: Option<f32>,
    pub is_grabbable: bool,
    pub is_harvestable: bool,
    pub supply_override: Option<f32>,
    pub supply_center_dock: Option<SupplyCenterDockDef>,
    pub aod_crush: Option<AodCrush>,
    pub teleport_max_distance: Option<f32>,
    pub critter_emitter: Option<CritterEmitter>,
    pub dead_collide_size: DeadCollideSize,
    pub formation_preview_decal: Option<FormationPreviewDecal>,
    pub formation_preview_item_decal: Option<FormationPreviewDecal>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AodCrush {
    pub small_fx: Option<String>,
    pub small_ocl: Option<String>,
    pub medium_fx: Option<String>,
    pub medium_ocl: Option<String>,
    pub large_fx: Option<String>,
    pub large_ocl: Option<String>,
    pub damage_type: Option<String>,
    pub death_type: Option<String>,
    pub damage: f32,
    pub special_object: Vec<String>,
    pub special_damage_type: Option<String>,
    pub special_death_type: Option<String>,
    pub special_damage: f32,
    pub self_damage_type: Option<String>,
    pub self_death_type: Option<String>,
    pub self_damage: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DeadCollideSize {
    #[default]
    Small,
    Medium,
    Large,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SupplyCenterDockDef {
    pub value_multiplier: f32,
    pub bonus_science: Option<String>,
    pub bonus_science_multiplier: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AiTable {
    pub attack_priority: Option<String>,
    pub stop_chase_distance: Option<f32>,
    pub stand_ground: bool,
    pub can_attack_while_contained: bool,
    pub burning_death_time_ms: Option<f32>,
    pub fade_on_portals: bool,
    pub rampage_time_ms: Option<f32>,
    pub eject_passengers_on_rampage_ms: Option<f32>,
    pub rampage_requires_aflame: bool,
    pub special_contact_points: ContactLabels,
    pub repair_health_per_second: Option<f32>,
    pub hold_ground_close_range: Option<f32>,
    pub min_cower_time_ms: Option<f32>,
    pub max_cower_time_ms: Option<f32>,
    pub turret: Option<TurretDef>,
    pub harvest: Option<HarvestDef>,
    pub animal_wander: Option<AnimalWander>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimalWander {
    pub wander_percentage: f32,
    pub max_wander_distance: f32,
    pub max_wander_radius: f32,
    pub flee_range: f32,
    pub flee_distance: f32,
    pub update_timer: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CritterEmitter {
    pub fx: Vec<String>,
    pub spawn_objects: Vec<String>,
    pub reload_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HarvestDef {
    pub max_boxes: u32,
    pub scan_distance: f32,
    pub activation_range: f32,
    pub preparation_time_ms: f32,
    pub action_time_ms: f32,
    pub action_delay_ms: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TurretDef {
    pub turn_rate: Option<f32>,
    pub controlled_weapon_slots: Vec<String>,
    pub art_angle: Option<f32>,
    pub recenter_time_ms: Option<f32>,
    pub max_deflection_cw: Option<f32>,
    pub max_deflection_acw: Option<f32>,
    pub fires_while_turning: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LuaEventState {
    pub conditions: Vec<String>,
    pub data: Option<String>,
}

fn lua_event_states(lineage: &[&Block]) -> Vec<LuaEventState> {
    lineage
        .iter()
        .rev()
        .find(|block| {
            block
                .blocks()
                .any(|draw| draw.keyword.eq_ignore_ascii_case("Draw"))
        })
        .into_iter()
        .flat_map(|block| block.blocks())
        .filter(|draw| draw.keyword.eq_ignore_ascii_case("Draw"))
        .flat_map(Block::blocks)
        .filter_map(|state| {
            let event = state
                .fields_named("LuaEvent")
                .find(|field| field.args.iter().any(|arg| arg == "OnStateEnter"))?;
            Some(LuaEventState {
                conditions: state.args.clone(),
                data: tagged_value(&event.args, "Data").map(str::to_owned),
            })
        })
        .collect()
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RespawnRule {
    pub level: Option<u16>,
    pub auto_spawn: bool,
    pub cost: Option<f32>,
    pub time_ms: Option<f32>,
    pub health: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RespawnEntry {
    pub level: Option<u16>,
    pub cost: Option<f32>,
    pub time_ms: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProductionModifierDef {
    pub required_upgrade: Option<String>,
    pub cost_multiplier: f32,
    pub time_multiplier: f32,
    pub hero_purchase: bool,
    pub hero_revive: bool,
    pub filter: Vec<String>,
}

impl ObjectRecord {
    #[must_use]
    pub fn power(&self, power: &str) -> Option<&PowerSlot> {
        self.powers
            .iter()
            .find(|slot| slot.power.eq_ignore_ascii_case(power))
    }

    #[must_use]
    pub fn locomotor_for(&self, condition: &str) -> Option<&LocomotorSlot> {
        self.locomotor_sets
            .iter()
            .find(|slot| slot.condition.eq_ignore_ascii_case(condition))
            .or_else(|| {
                self.locomotor_sets
                    .iter()
                    .find(|slot| slot.condition == "SET_NORMAL")
            })
            .or_else(|| self.locomotor_sets.first())
    }

    #[must_use]
    pub fn creation_lists(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for module in &self.declared_modules {
            let slow_death = module.name.eq_ignore_ascii_case("SlowDeathBehavior");
            for field in &module.fields {
                let keyword = field.keyword.to_ascii_uppercase();
                if !keyword.ends_with("OCL") && keyword != "CREATIONLIST" {
                    continue;
                }
                let list = if slow_death {
                    field.args.get(1)
                } else {
                    field.args.first()
                };
                if let Some(list) = list.filter(|list| !list.eq_ignore_ascii_case("NONE")) {
                    out.push(list.clone());
                }
            }
        }
        out.extend(self.damage_creations.iter().map(|line| line.list.clone()));
        out
    }

    #[must_use]
    pub fn created_templates(&self) -> Vec<String> {
        let mut fields: Vec<&CapturedField> = Vec::new();
        for module in &self.declared_modules {
            Self::collect_fields(module, &mut fields);
        }
        fields
            .into_iter()
            .flat_map(|field| {
                if field.keyword.eq_ignore_ascii_case("SpawnTemplateName")
                    || field.keyword.eq_ignore_ascii_case("ReplacementObjectName")
                {
                    field.args.as_slice()
                } else if SPAWNED_OBJECT_FIELDS
                    .iter()
                    .any(|keyword| field.keyword.eq_ignore_ascii_case(keyword))
                {
                    field.args.get(..1).unwrap_or_default()
                } else if field.keyword.eq_ignore_ascii_case("UpgradeCreationTrigger") {
                    field.args.get(1..2).unwrap_or_default()
                } else {
                    &[]
                }
            })
            .filter(|name| !name.eq_ignore_ascii_case("NONE"))
            .cloned()
            .collect()
    }

    fn collect_fields<'a>(module: &'a CapturedModule, out: &mut Vec<&'a CapturedField>) {
        out.extend(module.fields.iter());
        for block in &module.blocks {
            Self::collect_fields(block, out);
        }
    }

    #[must_use]
    pub fn weapon_names(&self) -> Vec<String> {
        const NAMED: [&str; 3] = ["WeaponName", "WeaponTemplateName", "WarheadTemplateName"];
        let mut out: Vec<String> = self.weapon.iter().cloned().collect();
        out.extend(
            self.weapon_sets
                .iter()
                .flat_map(|set| set.slots.iter().map(|slot| slot.weapon.clone())),
        );
        for module in &self.declared_modules {
            for field in &module.fields {
                let named = NAMED
                    .iter()
                    .any(|keyword| field.keyword.eq_ignore_ascii_case(keyword));
                let name = if field.keyword.eq_ignore_ascii_case("Weapon") {
                    field.args.last()
                } else if named || field.keyword.to_ascii_uppercase().ends_with("WEAPON") {
                    field.args.first()
                } else {
                    None
                };
                if let Some(name) = name.filter(|name| !name.eq_ignore_ascii_case("NONE")) {
                    out.push(name.clone());
                }
            }
        }
        out
    }

    #[must_use]
    pub fn has_kind(&self, token: &str) -> bool {
        self.kind_of
            .iter()
            .any(|kind| kind.eq_ignore_ascii_case(token))
    }

    #[must_use]
    pub fn read(index: &TemplateIndex<'_>, name: &str, errors: &ValueErrors) -> Option<Self> {
        let lineage = index.lineage(name);
        let leaf = *lineage.last()?;
        let macros = index.macros();
        let modules = index.declared_modules(name);
        let reads = Reads {
            context: leaf.name().unwrap_or(name),
            macros,
            errors,
        };
        let numeric = |keyword: &str| reads.number(nearest(&lineage, keyword));
        let text = |keyword: &str| {
            nearest(&lineage, keyword)
                .and_then(Field::value)
                .filter(|value| !value.eq_ignore_ascii_case("NONE"))
                .map(str::to_owned)
        };
        let kind_of = resolve_kind_of(&lineage, &reads);
        let percent = |keyword: &str| reads.percent(nearest(&lineage, keyword));
        let (vision_side, vision_rear) = (percent("VisionSide"), percent("VisionRear"));
        let horde = module(&modules, "HordeContain");
        let horde_side = horde.and_then(|block| reads.percent(block.field("VisionSideOverride")));
        let horde_rear = horde.and_then(|block| reads.percent(block.field("VisionRearOverride")));
        let respawn_update = module(&modules, "RespawnUpdate");
        let body = modules
            .iter()
            .rev()
            .find(|decl| decl.block.keyword.eq_ignore_ascii_case("Body"));
        let dozer = kind_of.iter().any(|kind| kind == "DOZER")
            || modules
                .iter()
                .any(|decl| decl.name.eq_ignore_ascii_case("DozerAIUpdate"));
        let radar_priority = nearest(&lineage, "RadarPriority")
            .and_then(|field| reads.radar_priority(field))
            .filter(|priority| *priority != RadarPriority::Invalid)
            .unwrap_or_else(|| {
                let capturable = kind_of.iter().any(|kind| kind == "CAPTURABLE");
                if capturable || module(&modules, "GarrisonContain").is_some() {
                    RadarPriority::Structure
                } else {
                    RadarPriority::Invalid
                }
            });
        Some(Self {
            name: leaf.name().unwrap_or(name).to_owned(),
            depth: lineage.len().saturating_sub(1),
            side: text("Side"),
            kind_of,
            ai_kind_of: threat_breakdown(&lineage)
                .and_then(|block| block.field("AIKindOf"))
                .and_then(|field| reads.ai_kind_of(field)),
            geometry: geometry_block(&lineage)
                .map(|block| geometry::geometry(block, &reads))
                .unwrap_or_default(),
            flammability: flammability(&lineage, reads.context, errors),
            geometry_is_small: reads.flag(nearest(&lineage, "GeometryIsSmall")),
            can_path_through_gates: reads.flag(nearest(&lineage, "CanPathThroughGates")),
            rotation_anchor: nearest(&lineage, "GeometryRotationAnchorOffset").map(|field| {
                let axis = |tag: &str| reads.tagged(field, tag).unwrap_or(0.0);
                (axis("X"), axis("Y"))
            }),
            pathfind_diameter: numeric("PathfindDiameter").filter(|value| *value > 0.0),
            fence_width: numeric("FenceWidth"),
            max_health: body.and_then(|decl| reads.number(decl.block.field("MaxHealth"))),
            initial_health: body.and_then(|decl| reads.number(decl.block.field("InitialHealth"))),
            max_health_damaged: body
                .and_then(|decl| reads.number(decl.block.field("MaxHealthDamaged"))),
            max_health_really_damaged: body
                .and_then(|decl| reads.number(decl.block.field("MaxHealthReallyDamaged"))),
            use_default_damage_settings: body.and_then(|decl| {
                decl.block
                    .field("UseDefaultDamageSettings")
                    .and_then(Field::value)
                    .and_then(parse_bool)
            }),
            entering_damaged_transition_ms: body
                .and_then(|decl| reads.number(decl.block.field("EnteringDamagedTransitionTime"))),
            entering_really_damaged_transition_ms: body.and_then(|decl| {
                reads.number(decl.block.field("EnteringReallyDamagedTransitionTime"))
            }),
            dodge_percent: body.and_then(|decl| reads.percent(decl.block.field("DodgePercent"))),
            cheer_radius: body.and_then(|decl| reads.number(decl.block.field("CheerRadius"))),
            damage_creations: body
                .map(|decl| reads.damage_creations(decl.block))
                .unwrap_or_default(),
            grab: body
                .map(|decl| reads.body_grab(decl.block))
                .unwrap_or_default(),
            permanently_killed_by_filter: body
                .and_then(|decl| decl.block.field("PermanentlyKilledByFilter"))
                .map(|field| field.args.clone())
                .unwrap_or_default(),
            vision_range: numeric("VisionRange"),
            shroud_clearing_range: numeric("ShroudClearingRange"),
            vision_side: horde_side.or(vision_side),
            vision_rear: horde_rear.or(vision_rear),
            vision_bonus: vision_bonus(&lineage, &reads),
            emotion_range: numeric("EmotionRange"),
            voice_attack_charge_timeout_ms: numeric("VoiceAttackChargeTimeout"),
            threat_value: numeric("ThreatValue"),
            threat_level: numeric("ThreatLevel"),
            max_distance_for_engaged: numeric("MaxDistanceForEngaged"),
            buildable: text("Buildable").and_then(|value| BuildableStatus::from_name(&value)),
            engaged_state_timeout_ms: numeric("EngagedStateTimeout"),
            min_z_increase_for_voice_move_to_higher_ground: reads
                .number_or_expression(nearest(&lineage, "MinZIncreaseForVoiceMoveToHigherGround")),
            campness_value: reads.number_or_expression(nearest(&lineage, "CampnessValue")),
            campness_value_radius: reads
                .number_or_expression(nearest(&lineage, "CampnessValueRadius")),
            back_away: horde.map(|block| back_away(block, &reads)),
            siege: siege_record(&modules, &reads),
            shockwave_resistance: numeric("ShockwaveResistance"),
            auto_acquire: ai_module(&modules)
                .and_then(|block| block.field("AutoAcquireEnemiesWhenIdle"))
                .and_then(|field| reads.acquire(field))
                .unwrap_or_default(),
            mood_attack_check_ms: ai_module(&modules)
                .and_then(|block| reads.number(block.field("MoodAttackCheckRate"))),
            crush: Crush {
                crusher_level: level(numeric("CrusherLevel")).unwrap_or(0),
                crushable_level: signed_level(numeric("CrushableLevel")).unwrap_or(UNCRUSHABLE),
                mounted_crusher_level: level(numeric("MountedCrusherLevel")),
                mounted_crushable_level: signed_level(numeric("MountedCrushableLevel")),
                min_velocity: percent("MinCrushVelocityPercent").unwrap_or(0.01),
                deceleration: percent("CrushDecelerationPercent").unwrap_or(0.0),
                knockback: numeric("CrushKnockback").unwrap_or(0.0),
                z_factor: numeric("CrushZFactor").unwrap_or(1.0),
                ram_power: numeric("RamPower").unwrap_or(0.0),
                ram_z_mult: numeric("RamZMult").unwrap_or(0.0),
                only_while_charging: reads
                    .flag(nearest(&lineage, "CrushOnlyWhileCharging"))
                    .unwrap_or(false),
                allies: reads
                    .flag(nearest(&lineage, "CrushAllies"))
                    .unwrap_or(false),
                use_crush_attack: reads
                    .flag(nearest(&lineage, "UseCrushAttack"))
                    .unwrap_or(true),
                weapon: text("CrushWeapon"),
                revenge_weapon: text("CrushRevengeWeapon"),
                resisted: body.and_then(|decl| {
                    let level = level(reads.number(decl.block.field("CrusherLevelResisted")))?;
                    let weapon = decl
                        .block
                        .field("CrushDamageWeaponTemplate")
                        .and_then(Field::value)
                        .filter(|value| !value.eq_ignore_ascii_case("NONE"))
                        .map(str::to_owned);
                    Some(CrushResistance { level, weapon })
                }),
            },
            command_set: text("CommandSet"),
            auto_resolve_unit_type: text("AutoResolveUnitType"),
            build_variations: nearest(&lineage, "BuildVariations")
                .map(|field| field.args.clone())
                .unwrap_or_default(),
            equivalent_to: nearest(&lineage, "EquivalentTo")
                .map(|field| field.args.clone())
                .unwrap_or_default(),
            experience_scalar_table: nearest(&lineage, "ExperienceScalarTable")
                .and_then(Field::value)
                .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case("NONE"))
                .map(str::to_owned),
            build_cost: numeric("BuildCost"),
            bounty_value: numeric("BountyValue"),
            command_point_bonus: numeric("CommandPointBonus"),
            refund_value: numeric("RefundValue"),
            build_time: numeric("BuildTime"),
            build_completion: nearest(&lineage, "BuildCompletion")
                .and_then(|field| reads.build_completion(field)),
            command_points: numeric("CommandPoints"),
            max_simultaneous_of_type: numeric("MaxSimultaneousOfType"),
            placement_view_angle: numeric("PlacementViewAngle"),
            formation_width: numeric("FormationWidth"),
            formation_depth: numeric("FormationDepth"),
            firing_arc: numeric("FiringArc"),
            front_angle: horde.and_then(|block| reads.number(block.field("FrontAngle"))),
            flanked_delay_ms: horde.and_then(|block| reads.number(block.field("FlankedDelay"))),
            flanked_duration_ms: horde
                .and_then(|block| reads.number(block.field("FlankedDuration"))),
            health_box_offset: numeric("HealthBoxHeightOffset"),
            health_box_scale: numeric("HealthBoxScale"),
            select_portrait: text("SelectPortrait"),
            weapon: default_set(&lineage, "WeaponSet").and_then(primary_weapon),
            armor: default_set(&lineage, "ArmorSet").and_then(armor_of),
            locomotor_sets: locomotor_sets(&lineage, &reads),
            income: module(&modules, "AutoDepositUpdate")
                .and_then(|block| {
                    Some((
                        reads.number(block.field("DepositTiming"))?,
                        reads.number(block.field("DepositAmount"))?,
                    ))
                })
                .or_else(|| {
                    module(&modules, "TerrainResourceBehavior").and_then(|block| {
                        Some((
                            reads.number(block.field("IncomeInterval"))?,
                            reads.number(block.field("MaxIncome"))?,
                        ))
                    })
                }),
            claim_radius: module(&modules, "TerrainResourceBehavior")
                .and_then(|block| reads.number(block.field("Radius"))),
            claim_decal_visible: module(&modules, "TerrainResourceBehavior")
                .and_then(|block| reads.flag(block.field("Visible")))
                .unwrap_or(crate::terrain_resource_behavior::VISIBLE_DEFAULT),
            claim_high_priority: module(&modules, "TerrainResourceBehavior")
                .and_then(|block| reads.flag(block.field("HighPriority")))
                .unwrap_or(crate::terrain_resource_behavior::HIGH_PRIORITY_DEFAULT),
            claim_max_income: module(&modules, "TerrainResourceBehavior")
                .and_then(|block| reads.number(block.field("MaxIncome"))),
            income_scales_with_claim: module(&modules, "AutoDepositUpdate").is_none()
                && module(&modules, "TerrainResourceBehavior").is_some(),
            initial_capture_bonus: module(&modules, "AutoDepositUpdate")
                .and_then(|block| reads.number(block.field("InitialCaptureBonus"))),
            income_gives_no_xp: module(&modules, "AutoDepositUpdate")
                .and_then(|block| reads.flag(block.field("GiveNoXP")))
                .unwrap_or(false),
            produces: module(&modules, "ProductionUpdate").is_some(),
            max_queue_entries: module(&modules, "ProductionUpdate")
                .and_then(|block| reads.number(block.field("MaxQueueEntries"))),
            production_gives_no_xp: module(&modules, "ProductionUpdate")
                .and_then(|block| reads.flag(block.field("GiveNoXP")))
                .unwrap_or(false),
            natural_rally_point: exit_modules(&modules)
                .into_iter()
                .find_map(|block| exit_point(block, "NaturalRallyPoint", &reads)),
            unit_create_point: exit_modules(&modules)
                .into_iter()
                .find_map(|block| exit_point(block, "UnitCreatePoint", &reads)),
            production_exit_delay: exit_modules(&modules)
                .into_iter()
                .find_map(|block| reads.number(block.field("ExitDelay"))),
            production_exit_angle: module(&modules, "QueueProductionExitUpdate")
                .and_then(|block| reads.number(block.field("PlacementViewAngle"))),
            no_exit_path: exit_modules(&modules)
                .into_iter()
                .find_map(|block| reads.flag(block.field("NoExitPath")))
                .unwrap_or(false),
            production_initial_burst: exit_modules(&modules)
                .into_iter()
                .find_map(|block| reads.number(block.field("InitialBurst"))),
            use_return_to_formation: exit_modules(&modules)
                .into_iter()
                .find_map(|block| reads.flag(block.field("UseReturnToFormation")))
                .unwrap_or(true),
            can_rally_to_slaughter: exit_modules(&modules)
                .into_iter()
                .find_map(|block| reads.flag(block.field("CanRallyToSlaughter")))
                .unwrap_or(false),
            dozer,
            castle: module(&modules, "CastleBehavior").map(|block| castle(block, &reads)),
            radar_priority,
            foundation: module(&modules, "FoundationAIUpdate").map(|block| Foundation {
                build_variation: reads.byte(block.field("BuildVariation")),
            }),
            modules: modules.len(),
            button_image: text("ButtonImage"),
            display_name: text("DisplayName"),
            description: text("Description"),
            hotkey: text("Hotkey"),
            revive_text: text("ReviveText"),
            recruit_text: text("RecruitText"),
            powers: powers(&modules, &reads),
            menu_point: geometry_block(&lineage).and_then(|block| menu_point(block, &reads)),
            contact_points: contact::contact_points(&lineage, &reads),
            upgraded_command_sets: upgraded_command_sets(&modules),
            weapon_sets: sets(&lineage, "WeaponSet", primary_weapon, &reads),
            armor_sets: sets(&lineage, "ArmorSet", armor_of, &reads),
            declared_modules: modules
                .iter()
                .map(|decl| CapturedModule::capture(decl, macros))
                .collect(),
            is_trainable: reads.flag(nearest(&lineage, "IsTrainable")),
            is_auto_built: reads
                .flag(nearest(&lineage, "IsAutoBuilt"))
                .unwrap_or(false),
            hero_sort_order: reads.integer(nearest(&lineage, "HeroSortOrder")),
            level_on_create: module(&modules, "ExperienceLevelCreate")
                .and_then(|block| reads.number(block.field("LevelToGrant")))
                .filter(|rank| (0.0..=f32::from(u16::MAX)).contains(rank))
                .map(|rank| {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let rank = rank as u16;
                    rank
                }),
            level_on_create_mp_only: module(&modules, "ExperienceLevelCreate")
                .and_then(|block| reads.flag(block.field("MPOnly")))
                .unwrap_or(false),
            respawn: respawn_update
                .map(|block| respawn_rules(block, &reads))
                .unwrap_or_default(),
            respawn_entries: respawn_update
                .map(|block| respawn_entries(block, &reads))
                .unwrap_or_default(),
            death_anim: respawn_update
                .and_then(|block| block.field("DeathAnim"))
                .and_then(Field::value)
                .filter(|value| !value.eq_ignore_ascii_case("NONE"))
                .map(str::to_owned),
            respawn_fx: respawn_update
                .and_then(|block| block.field("RespawnFX"))
                .and_then(Field::value)
                .filter(|value| !value.eq_ignore_ascii_case("NONE"))
                .map(str::to_owned),
            death_fx: respawn_update
                .and_then(|block| block.field("DeathFX"))
                .and_then(Field::value)
                .filter(|value| !value.eq_ignore_ascii_case("NONE"))
                .map(str::to_owned),
            initial_spawn_fx: respawn_update
                .and_then(|block| block.field("InitialSpawnFX"))
                .and_then(Field::value)
                .filter(|value| !value.eq_ignore_ascii_case("NONE"))
                .map(str::to_owned),
            auto_respawn_at_object_filter: respawn_update
                .and_then(|block| block.field("AutoRespawnAtObjectFilter"))
                .map(|field| field.args.clone())
                .unwrap_or_default(),
            production_modifiers: module(&modules, "ProductionUpdate")
                .map(|block| production_modifiers(block, &reads))
                .unwrap_or_default(),
            cost_modifiers: crate::cost_modifier_upgrade::cost_modifiers(&modules, macros),
            keeps_corpse: module(&modules, "KeepObjectDie").is_some(),
            keep_selectable_when_dead: reads
                .flag(nearest(&lineage, "KeepSelectableWhenDead"))
                .unwrap_or(false),
            rebuild_seconds: module(&modules, "GettingBuiltBehavior")
                .and_then(|block| reads.number(block.field("RebuildTimeSeconds")))
                .filter(|seconds| *seconds > 0.0),
            spawn_timer: module(&modules, "GettingBuiltBehavior").map(|block| {
                reads
                    .number(block.field("SpawnTimer"))
                    .unwrap_or(SPAWN_TIMER_DEFAULT)
            }),
            rebuild_cost_percent: {
                let block = module(&modules, "GettingBuiltBehavior");
                let mut percent = REBUILD_COST_PERCENT_DEFAULT;
                for (slot, keyword) in percent.iter_mut().zip(REBUILD_COST_PERCENT_FIELDS) {
                    if let Some(read) = block.and_then(|block| reads.percent(block.field(keyword)))
                    {
                        *slot = read;
                    }
                }
                percent
            },
            rebuild_when_dead: module(&modules, "GettingBuiltBehavior")
                .and_then(|block| reads.flag(block.field("RebuildWhenDead")))
                .unwrap_or(false),
            disallow_rebuild_range: module(&modules, "GettingBuiltBehavior")
                .and_then(|block| reads.number(block.field("DisallowRebuildRange")))
                .filter(|range| *range > 0.0),
            disallow_rebuild_filter: module(&modules, "GettingBuiltBehavior")
                .and_then(|block| block.field("DisallowRebuildFilter"))
                .map(|field| field.args.clone())
                .unwrap_or_default(),
            use_spawn_timer_without_worker: module(&modules, "GettingBuiltBehavior")
                .and_then(|block| reads.flag(block.field("UseSpawnTimerWithoutWorker")))
                .unwrap_or(false),
            worker_name: module(&modules, "GettingBuiltBehavior")
                .and_then(|block| block.field("WorkerName"))
                .and_then(Field::value)
                .filter(|value| !value.eq_ignore_ascii_case("NONE"))
                .map(str::to_owned),
            evil_worker_name: module(&modules, "GettingBuiltBehavior")
                .and_then(|block| block.field("EvilWorkerName"))
                .and_then(Field::value)
                .filter(|value| !value.eq_ignore_ascii_case("NONE"))
                .map(str::to_owned),
            test_faction: module(&modules, "GettingBuiltBehavior")
                .and_then(|block| reads.flag(block.field("TestFaction")))
                .unwrap_or(false),
            lua_events: ai_module(&modules)
                .and_then(|block| block.field("AILuaEventsList"))
                .and_then(Field::value)
                .filter(|value| !value.eq_ignore_ascii_case("NONE"))
                .map(str::to_owned),
            lua_event_states: lua_event_states(&lineage),
            stance_template: module(&modules, "StancesBehavior")
                .and_then(|block| block.field("StanceTemplate"))
                .and_then(Field::value)
                .filter(|value| !value.eq_ignore_ascii_case("NONE"))
                .map(str::to_owned),
            ai: ai_table(ai_module(&modules), is_animal_ai(&modules), &reads),
            camouflage_detection_multiplier: numeric("CamouflageDetectionMultiplier"),
            is_grabbable: reads
                .flag(nearest(&lineage, "IsGrabbable"))
                .unwrap_or(false),
            is_harvestable: reads
                .flag(nearest(&lineage, "IsHarvestable"))
                .unwrap_or(false),
            supply_override: numeric("SupplyOverride").filter(|value| *value > 0.0),
            supply_center_dock: module(&modules, "SupplyCenterDockUpdate").map(|block| {
                SupplyCenterDockDef {
                    value_multiplier: reads.percent(block.field("ValueMultiplier")).unwrap_or(1.0),
                    bonus_science: block
                        .field("BonusScience")
                        .and_then(Field::value)
                        .filter(|value| !value.eq_ignore_ascii_case("NONE"))
                        .map(str::to_owned),
                    bonus_science_multiplier: reads
                        .percent(block.field("BonusScienceMultiplier"))
                        .unwrap_or(1.0),
                }
            }),
            aod_crush: module(&modules, "AODCrushCollide").map(|block| {
                let string = |name: &str| {
                    block
                        .field(name)
                        .and_then(Field::value)
                        .filter(|value| !value.eq_ignore_ascii_case("NONE"))
                        .map(str::to_owned)
                };
                AodCrush {
                    small_fx: string("SmallFXList"),
                    small_ocl: string("SmallObjectCreationList"),
                    medium_fx: string("MediumFXList"),
                    medium_ocl: string("MediumObjectCreationList"),
                    large_fx: string("LargeFXList"),
                    large_ocl: string("LargeObjectCreationList"),
                    damage_type: string("DamageType"),
                    death_type: string("DeathType"),
                    damage: reads.number(block.field("Damage")).unwrap_or(0.0),
                    special_object: block
                        .field("SpecialObject")
                        .map(|field| field.args.clone())
                        .unwrap_or_default(),
                    special_damage_type: string("SpecialDamageType"),
                    special_death_type: string("SpecialDeathType"),
                    special_damage: reads.number(block.field("SpecialDamage")).unwrap_or(0.0),
                    self_damage_type: string("SelfDamageType"),
                    self_death_type: string("SelfDeathType"),
                    self_damage: reads.number(block.field("SelfDamage")).unwrap_or(0.0),
                }
            }),
            teleport_max_distance: module(&modules, "TeleportSpecialAbilityUpdate")
                .and_then(|block| reads.number(block.field("MaxDistance")))
                .filter(|value| *value > 0.0),
            critter_emitter: module(&modules, "CritterEmitterUpdate").map(|block| {
                let names = |keyword: &str| -> Vec<String> {
                    block
                        .fields_named(keyword)
                        .flat_map(|field| field.args.iter().cloned())
                        .collect()
                };
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let reload_ms = reads
                    .number(block.field("ReloadTime"))
                    .map_or(0, |ms| ms.max(0.0) as u32);
                CritterEmitter {
                    fx: names("FX"),
                    spawn_objects: names("SpawnObject"),
                    reload_ms,
                }
            }),
            dead_collide_size: nearest(&lineage, "DeadCollideSize")
                .and_then(Field::value)
                .map_or(DeadCollideSize::Small, |value| {
                    if value.eq_ignore_ascii_case("SMALL") {
                        DeadCollideSize::Small
                    } else if value.eq_ignore_ascii_case("MEDIUM") {
                        DeadCollideSize::Medium
                    } else {
                        DeadCollideSize::Large
                    }
                }),
            formation_preview_decal: formation_preview::formation_preview_decal(
                &lineage,
                "FormationPreviewDecal",
                &reads,
            ),
            formation_preview_item_decal: formation_preview::formation_preview_decal(
                &lineage,
                "FormationPreviewItemDecal",
                &reads,
            ),
        })
    }
}

fn respawn_rules(block: &Block, reads: &Reads<'_>) -> Vec<RespawnRule> {
    block
        .fields_named("RespawnRules")
        .map(|field| {
            let number = |name: &str| reads.tagged(field, name).filter(|v| *v >= 0.0);
            RespawnRule {
                level: reads.rank(field, "Level"),
                auto_spawn: reads.tagged_flag(field, "AutoSpawn").unwrap_or(false),
                cost: number("Cost"),
                time_ms: number("Time"),
                health: reads.tagged_percent(field, "Health").filter(|v| *v > 0.0),
            }
        })
        .collect()
}

fn respawn_entries(block: &Block, reads: &Reads<'_>) -> Vec<RespawnEntry> {
    block
        .fields_named("RespawnEntry")
        .map(|field| {
            let number = |name: &str| reads.tagged(field, name).filter(|v| *v >= 0.0);
            RespawnEntry {
                level: reads.rank(field, "Level"),
                cost: number("Cost"),
                time_ms: number("Time"),
            }
        })
        .collect()
}

fn production_modifiers(block: &Block, reads: &Reads<'_>) -> Vec<ProductionModifierDef> {
    block
        .blocks_named("ProductionModifier")
        .map(|sub| {
            let multiplier = |keyword: &str| {
                reads
                    .number(sub.field(keyword))
                    .filter(|v| (0.0..=1_000.0).contains(v))
                    .unwrap_or(1.0)
            };
            let flag = |keyword: &str| reads.flag(sub.field(keyword)).unwrap_or(false);
            ProductionModifierDef {
                required_upgrade: sub
                    .field("RequiredUpgrade")
                    .and_then(Field::value)
                    .filter(|name| !name.eq_ignore_ascii_case("NONE"))
                    .map(str::to_owned),
                cost_multiplier: multiplier("CostMultiplier"),
                time_multiplier: multiplier("TimeMultiplier"),
                hero_purchase: flag("HeroPurchase"),
                hero_revive: flag("HeroRevive"),
                filter: sub
                    .field("ModifierFilter")
                    .map(|field| field.args.clone())
                    .unwrap_or_default(),
            }
        })
        .collect()
}

#[derive(Debug, Clone, Default)]
pub struct TemplateRecords {
    records: Vec<(String, ObjectRecord)>,
}

impl TemplateRecords {
    pub fn build(index: &TemplateIndex<'_>) -> Result<Self, Vec<ValueError>> {
        match Self::build_collecting(index) {
            (_, errors) if !errors.is_empty() => Err(errors),
            (records, _) => Ok(records),
        }
    }

    #[must_use]
    pub fn build_collecting(index: &TemplateIndex<'_>) -> (Self, Vec<ValueError>) {
        let errors = ValueErrors::new();
        let records = index
            .names()
            .filter_map(|name| Some((name.to_owned(), ObjectRecord::read(index, name, &errors)?)))
            .collect();
        (Self { records }, errors.take())
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ObjectRecord> {
        let key = name.to_ascii_lowercase();
        let at = self
            .records
            .binary_search_by(|(held, _)| held.as_str().cmp(key.as_str()))
            .ok()?;
        self.records.get(at).map(|(_, record)| record)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ObjectRecord> {
        self.records.iter().map(|(_, record)| record)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn reread(&mut self, index: &TemplateIndex<'_>, names: &[String]) -> Vec<ValueError> {
        let errors = ValueErrors::new();
        for name in names {
            let key = name.to_ascii_lowercase();
            let Some(record) = ObjectRecord::read(index, &key, &errors) else {
                continue;
            };
            match self
                .records
                .binary_search_by(|(held, _)| held.as_str().cmp(key.as_str()))
            {
                Ok(at) => {
                    if let Some(slot) = self.records.get_mut(at) {
                        slot.1 = record;
                    }
                }
                Err(at) => self.records.insert(at, (key, record)),
            }
        }
        errors.take()
    }
}

pub const DOZER_CONSTRUCT: &str = "DOZER_CONSTRUCT";

pub const FOUNDATION_CONSTRUCT: &str = "FOUNDATION_CONSTRUCT";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitBuild {
    pub button: String,
    pub object: String,
}

fn command_objects(
    command_set: &str,
    sets: &CommandSetIndex,
    buttons: &CommandButtonIndex,
    command: &str,
) -> Vec<UnitBuild> {
    let Some(set) = sets.get(command_set) else {
        return Vec::new();
    };
    set.slots
        .values()
        .filter_map(|name| {
            let button = buttons.get(name)?;
            if !button
                .command
                .as_deref()
                .is_some_and(|found| found.eq_ignore_ascii_case(command))
            {
                return None;
            }
            Some(UnitBuild {
                button: name.clone(),
                object: button.object.clone()?,
            })
        })
        .collect()
}

#[must_use]
pub fn unit_builds(
    command_set: &str,
    sets: &CommandSetIndex,
    buttons: &CommandButtonIndex,
) -> Vec<UnitBuild> {
    command_objects(command_set, sets, buttons, UNIT_BUILD)
}

#[must_use]
pub fn dozer_constructs(
    command_set: &str,
    sets: &CommandSetIndex,
    buttons: &CommandButtonIndex,
) -> Vec<UnitBuild> {
    command_objects(command_set, sets, buttons, DOZER_CONSTRUCT)
}

#[must_use]
pub fn foundation_constructs(
    command_set: &str,
    sets: &CommandSetIndex,
    buttons: &CommandButtonIndex,
) -> Vec<UnitBuild> {
    command_objects(command_set, sets, buttons, FOUNDATION_CONSTRUCT)
}

pub const OBJECT_UPGRADE: &str = "OBJECT_UPGRADE";

pub const PLAYER_UPGRADE: &str = "PLAYER_UPGRADE";

pub const CASTLE_UPGRADE: &str = "CASTLE_UPGRADE";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradeButton {
    pub button: String,
    pub upgrade: String,
}

fn command_upgrades(
    command_set: &str,
    sets: &CommandSetIndex,
    buttons: &CommandButtonIndex,
    commands: &[&str],
) -> Vec<UpgradeButton> {
    let Some(set) = sets.get(command_set) else {
        return Vec::new();
    };
    set.slots
        .values()
        .filter_map(|name| {
            let button = buttons.get(name)?;
            if !button.command.as_deref().is_some_and(|found| {
                commands
                    .iter()
                    .any(|command| found.eq_ignore_ascii_case(command))
            }) {
                return None;
            }
            Some(UpgradeButton {
                button: name.clone(),
                upgrade: button.upgrade.clone()?,
            })
        })
        .collect()
}

#[must_use]
pub fn object_upgrades(
    command_set: &str,
    sets: &CommandSetIndex,
    buttons: &CommandButtonIndex,
) -> Vec<UpgradeButton> {
    command_upgrades(
        command_set,
        sets,
        buttons,
        &[OBJECT_UPGRADE, CASTLE_UPGRADE],
    )
}

#[must_use]
pub fn player_upgrades(
    command_set: &str,
    sets: &CommandSetIndex,
    buttons: &CommandButtonIndex,
) -> Vec<UpgradeButton> {
    command_upgrades(command_set, sets, buttons, &[PLAYER_UPGRADE])
}

pub const REVIVE: &str = "REVIVE";

#[must_use]
pub fn revive_buttons(
    command_set: &str,
    sets: &CommandSetIndex,
    buttons: &CommandButtonIndex,
) -> Vec<String> {
    let Some(set) = sets.get(command_set) else {
        return Vec::new();
    };
    set.slots
        .values()
        .filter(|name| {
            buttons
                .get(name)
                .and_then(|button| button.command.as_deref())
                .is_some_and(|command| command.eq_ignore_ascii_case(REVIVE))
        })
        .cloned()
        .collect()
}

#[must_use]
pub fn weapon_toggle_buttons(
    command_set: &str,
    sets: &CommandSetIndex,
    buttons: &CommandButtonIndex,
) -> Vec<(String, Vec<String>)> {
    let Some(set) = sets.get(command_set) else {
        return Vec::new();
    };
    set.slots
        .values()
        .filter_map(|name| {
            let button = buttons.get(name)?;
            let toggles = button
                .command
                .as_deref()
                .is_some_and(|command| command.eq_ignore_ascii_case("TOGGLE_WEAPONSET"));
            (toggles && !button.flags_used_for_toggle.is_empty())
                .then(|| (name.clone(), button.flags_used_for_toggle.clone()))
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct PowerButton {
    pub button: String,
    pub power: String,
    pub options: Vec<String>,
    pub need_damaged_target: bool,
    pub show_button: bool,
    pub auto_ability: bool,
    pub auto_delay: Option<f32>,
    pub auto_ability_disallowed_on_model_condition: Vec<String>,
    pub preset_range: Option<f32>,
    pub enable_on_model_condition: Vec<String>,
    pub disable_on_model_condition: Vec<String>,
    pub trigger_when_ready: bool,
}

#[must_use]
pub fn special_power_buttons(
    command_set: &str,
    sets: &CommandSetIndex,
    buttons: &CommandButtonIndex,
) -> Vec<PowerButton> {
    let Some(set) = sets.get(command_set) else {
        return Vec::new();
    };
    set.slots
        .values()
        .filter_map(|name| {
            let button = buttons.get(name)?;
            let command = button.command.as_deref()?;
            if !(command.eq_ignore_ascii_case("SPECIAL_POWER")
                || command.eq_ignore_ascii_case("SPECIAL_POWER_TOGGLE"))
            {
                return None;
            }
            Some(PowerButton {
                button: name.clone(),
                power: button.special_power.clone()?,
                options: button.options.clone(),
                need_damaged_target: button.need_damaged_target,
                show_button: button.show_button,
                auto_ability: button.auto_ability.unwrap_or(false),
                auto_delay: button.auto_delay,
                auto_ability_disallowed_on_model_condition: button
                    .auto_ability_disallowed_on_model_condition
                    .clone(),
                preset_range: button.preset_range,
                enable_on_model_condition: button.enable_on_model_condition.clone(),
                disable_on_model_condition: button.disable_on_model_condition.clone(),
                trigger_when_ready: button.trigger_when_ready,
            })
        })
        .collect()
}

#[must_use]
pub fn spell_book_buttons(
    command_set: &str,
    sets: &CommandSetIndex,
    buttons: &CommandButtonIndex,
) -> Vec<PowerButton> {
    let Some(set) = sets.get(command_set) else {
        return Vec::new();
    };
    set.slots
        .values()
        .filter_map(|name| {
            let button = buttons.get(name)?;
            if !button
                .command
                .as_deref()?
                .eq_ignore_ascii_case("SPELL_BOOK")
            {
                return None;
            }
            Some(PowerButton {
                button: name.clone(),
                power: button.special_power.clone()?,
                options: button.options.clone(),
                need_damaged_target: button.need_damaged_target,
                show_button: button.show_button,
                auto_ability: button.auto_ability.unwrap_or(false),
                auto_delay: button.auto_delay,
                auto_ability_disallowed_on_model_condition: button
                    .auto_ability_disallowed_on_model_condition
                    .clone(),
                preset_range: button.preset_range,
                enable_on_model_condition: button.enable_on_model_condition.clone(),
                disable_on_model_condition: button.disable_on_model_condition.clone(),
                trigger_when_ready: button.trigger_when_ready,
            })
        })
        .collect()
}

#[must_use]
pub fn purchase_science_buttons(
    command_set: &str,
    sets: &CommandSetIndex,
    buttons: &CommandButtonIndex,
) -> Vec<(String, String)> {
    let Some(set) = sets.get(command_set) else {
        return Vec::new();
    };
    set.slots
        .values()
        .filter_map(|name| {
            let button = buttons.get(name)?;
            if !button
                .command
                .as_deref()?
                .eq_ignore_ascii_case("PURCHASE_SCIENCE")
            {
                return None;
            }
            Some((name.clone(), button.science.first()?.clone()))
        })
        .collect()
}

fn powers(modules: &[ModuleDeclaration<'_>], reads: &Reads<'_>) -> Vec<PowerSlot> {
    let mut slots: Vec<PowerSlot> = Vec::new();
    let mut effect_captured: Vec<bool> = Vec::new();
    for decl in modules {
        let Some(power) = decl
            .block
            .field("SpecialPowerTemplate")
            .and_then(Field::value)
        else {
            continue;
        };
        let at = match slots
            .iter()
            .position(|slot| slot.power.eq_ignore_ascii_case(power))
        {
            Some(at) => at,
            None => {
                slots.push(PowerSlot {
                    power: power.to_owned(),
                    starts_paused: false,
                    defers_recharge: false,
                    set_model_condition: None,
                    disable_during_anim: false,
                    initiate_sound: None,
                    initiate_fx: None,
                    on_trigger_recharge: None,
                    chained_button: None,
                    available_at_start: true,
                    obey_recharge_on_unpause: false,
                    unlock_upgrade: None,
                    trigger_fx: None,
                    modifier: None,
                });
                effect_captured.push(false);
                slots.len().saturating_sub(1)
            }
        };
        if decl.name.eq_ignore_ascii_case("UnpauseSpecialPowerUpgrade") {
            if reads.flag(decl.block.field("ObeyRechageOnTrigger")) == Some(true) {
                if let Some(slot) = slots.get_mut(at) {
                    slot.obey_recharge_on_unpause = true;
                }
            }
            let unlock = decl
                .block
                .field("TriggeredBy")
                .and_then(Field::value)
                .filter(|name| !mint_sage_ini::value::is_none(name));
            if let Some(slot) = slots.get_mut(at) {
                if slot.unlock_upgrade.is_none() {
                    slot.unlock_upgrade = unlock.map(str::to_owned);
                }
            }
            continue;
        }
        let paused = reads.flag(decl.block.field("StartsPaused")) == Some(true);
        let defers = reads.flag(decl.block.field("UpdateModuleStartsAttack")) == Some(true);
        let condition =
            set_model_condition(decl.block.field("SetModelCondition"), reads).and_then(|flag| {
                let seconds = reads.number(decl.block.field("SetModelConditionTime"))?;
                (seconds > 0.0).then(|| (flag, retail_condition_frames(seconds)))
            });
        let disable = reads.flag(decl.block.field("DisableDuringAnimDuration")) == Some(true);
        let initiate_sound = decl
            .block
            .field("InitiateSound")
            .and_then(Field::value)
            .filter(|name| !mint_sage_ini::value::is_none(name));
        let initiate_fx = decl
            .block
            .field("InitiateFX")
            .and_then(Field::value)
            .filter(|name| !mint_sage_ini::value::is_none(name));
        let recharges = decl
            .block
            .field("OnTriggerRechargeSpecialPower")
            .and_then(Field::value)
            .filter(|value| !value.eq_ignore_ascii_case("NONE"));
        let chained = decl
            .block
            .field("ChainedButton")
            .and_then(Field::value)
            .filter(|name| !mint_sage_ini::value::is_none(name));
        let available_at_start = reads
            .flag(decl.block.field("AvailableAtStart"))
            .unwrap_or(true);
        if let Some(slot) = slots.get_mut(at) {
            slot.starts_paused |= paused;
            slot.defers_recharge |= defers;
            if slot.set_model_condition.is_none() {
                slot.set_model_condition = condition;
            }
            slot.disable_during_anim |= disable;
            if slot.initiate_sound.is_none() {
                slot.initiate_sound = initiate_sound.map(str::to_owned);
            }
            if slot.initiate_fx.is_none() {
                slot.initiate_fx = initiate_fx.map(str::to_owned);
            }
            if slot.on_trigger_recharge.is_none() {
                slot.on_trigger_recharge = recharges.map(str::to_owned);
            }
            if slot.chained_button.is_none() {
                slot.chained_button = chained.map(str::to_owned);
            }
            slot.available_at_start &= available_at_start;
            if effect_captured.get(at) == Some(&false)
                && defers
                && (decl.name.eq_ignore_ascii_case("SpecialPowerModule")
                    || decl
                        .name
                        .eq_ignore_ascii_case("SpecialPowerTimerRefreshSpecialPower"))
            {
                slot.trigger_fx = decl
                    .block
                    .field("TriggerFX")
                    .and_then(Field::value)
                    .filter(|name| !name.eq_ignore_ascii_case("NONE"))
                    .map(str::to_owned);
                slot.modifier = modifier_rule(decl, reads);
                if let Some(captured) = effect_captured.get_mut(at) {
                    *captured = true;
                }
            }
        }
    }
    slots
}

fn modifier_rule(decl: &ModuleDeclaration<'_>, reads: &Reads<'_>) -> Option<ModifierRule> {
    if reads.flag(decl.block.field("AttributeModifierWeatherBased")) == Some(true) {
        return None;
    }
    let anti = names(
        decl.block
            .field("AntiCategory")
            .map(|field| field.args.clone())
            .unwrap_or_default(),
    );
    build_modifier_rule(ModifierFieldValues {
        list: decl
            .block
            .field("AttributeModifier")
            .and_then(Field::value)
            .map(Into::into),
        range: reads
            .number(decl.block.field("AttributeModifierRange"))
            .filter(|value| *value >= 0.0)
            .unwrap_or(0.0),
        affects: names(
            decl.block
                .field("AttributeModifierAffects")
                .map(|field| field.args.clone())
                .unwrap_or_default(),
        ),
        affects_self: reads.flag(decl.block.field("AttributeModifierAffectsSelf")) == Some(true),
        all_sides: reads.flag(decl.block.field("TargetAllSides")) == Some(true),
        target_enemy: reads.flag(decl.block.field("TargetEnemy")) == Some(true),
        anti,
        re_enable: reads.flag(decl.block.field("ReEnableAntiCategory")) == Some(true),
        anti_fx: decl
            .block
            .field("AntiFX")
            .and_then(Field::value)
            .filter(|name| !name.eq_ignore_ascii_case("NONE"))
            .map(Into::into),
        modifier_fx: decl
            .block
            .field("AttributeModifierFX")
            .and_then(Field::value)
            .filter(|name| !name.eq_ignore_ascii_case("NONE"))
            .map(Into::into),
        affect_good: reads.flag(decl.block.field("AffectGood")) == Some(true),
        affect_evil: reads.flag(decl.block.field("AffectEvil")) == Some(true),
        affect_allies: reads.flag(decl.block.field("AffectAllies")).unwrap_or(true),
    })
}

fn set_model_condition(field: Option<&Field>, reads: &Reads<'_>) -> Option<String> {
    let field = field?;
    match field.value().and_then(|raw| raw.split_once(':')) {
        Some((attribute, flag))
            if attribute.eq_ignore_ascii_case("ModelConditionState") && !flag.is_empty() =>
        {
            Some(flag.to_owned())
        }
        _ => {
            reads
                .errors
                .field(reads.context, field, "ModelConditionState:<flag>");
            None
        }
    }
}

fn retail_condition_frames(seconds: f32) -> LogicFrames {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let fifths = (seconds * 5.0).clamp(0.0, f32::from(u16::MAX)) as u32;
    LogicFrames::from_engine_milliseconds(fifths.saturating_mul(200))
}

fn sets(
    lineage: &[&Block],
    keyword: &str,
    value: fn(&Block) -> Option<String>,
    reads: &Reads<'_>,
) -> Vec<ConditionedSet> {
    let Some(owner) = lineage
        .iter()
        .rev()
        .find(|block| block.blocks_named(keyword).next().is_some())
    else {
        return Vec::new();
    };
    let weapons = keyword.eq_ignore_ascii_case("WeaponSet");
    let kind = if weapons {
        SetKind::Weapon
    } else {
        SetKind::Armor
    };
    owner
        .blocks_named(keyword)
        .map(|set| {
            let slots = if weapons {
                weapon_slots(set, reads)
            } else {
                Vec::new()
            };
            ConditionedSet {
                conditions: set
                    .field("Conditions")
                    .map(|field| {
                        condition_flags(kind, &field.args).unwrap_or_else(|| {
                            reads.errors.field(
                                reads.context,
                                field,
                                "a list of set condition flags",
                            );
                            Vec::new()
                        })
                    })
                    .unwrap_or_default(),
                value: value(set),
                slots,
                damage_fx: set
                    .field("DamageFX")
                    .and_then(Field::value)
                    .filter(|name| !name.eq_ignore_ascii_case("None"))
                    .map(str::to_owned),
                ready_status_shared_within_set: weapons
                    && reads
                        .flag(set.field("ReadyStatusSharedWithinSet"))
                        .unwrap_or(false),
                share_weapon_reload_time: weapons
                    && reads
                        .flag(set.field("ShareWeaponReloadTime"))
                        .unwrap_or(false),
                weapon_lock_shared_across_sets: weapons
                    && reads
                        .flag(set.field("WeaponLockSharedAcrossSets"))
                        .unwrap_or(false),
                default_weapon_choice_criteria: weapons
                    .then(|| set.field("DefaultWeaponChoiceCritera"))
                    .flatten()
                    .and_then(|field| reads.weapon_choice_criteria(field)),
            }
        })
        .collect()
}

fn locomotor_sets(lineage: &[&Block], reads: &Reads<'_>) -> Vec<LocomotorSlot> {
    const KEYWORD: &str = "LocomotorSet";
    let Some(owner) = lineage
        .iter()
        .rev()
        .find(|block| block.blocks_named(KEYWORD).next().is_some())
    else {
        return Vec::new();
    };
    owner
        .blocks_named(KEYWORD)
        .map(|set| LocomotorSlot {
            condition: set
                .field("Condition")
                .and_then(Field::value)
                .map(str::to_ascii_uppercase)
                .unwrap_or_default(),
            locomotor: set
                .field("Locomotor")
                .and_then(Field::value)
                .filter(|value| !value.eq_ignore_ascii_case("NONE"))
                .map(str::to_owned),
            speed: reads.number(set.field("Speed")),
        })
        .collect()
}

fn armor_of(set: &Block) -> Option<String> {
    set.field("Armor")
        .and_then(Field::value)
        .filter(|value| !value.eq_ignore_ascii_case("NONE"))
        .map(str::to_owned)
}

fn geometry_block<'b>(lineage: &[&'b Block]) -> Option<&'b Block> {
    lineage.iter().rev().copied().find(|block| {
        block.field("Geometry").is_some()
            || block.fields_named("AdditionalGeometry").next().is_some()
    })
}

fn threat_breakdown<'b>(lineage: &[&'b Block]) -> Option<&'b Block> {
    lineage
        .iter()
        .rev()
        .find_map(|block| block.blocks_named("ThreatBreakdown").last())
}

fn flammability(
    lineage: &[&Block],
    context: &str,
    errors: &ValueErrors,
) -> Option<mint_sage_ini::fire_logic_system::FireMaterial> {
    let block = lineage
        .iter()
        .rev()
        .find_map(|block| block.blocks_named("Flammability").last())?;
    match mint_sage_ini::fire_logic_system::read_flammability(block) {
        Ok(flammability) => Some(flammability.material),
        Err(mint_sage_ini::fire_logic_system::FireDataError::FieldOutOfRange {
            field,
            value,
            max: _,
            file,
            line,
        }) => {
            errors.at(
                &file.into(),
                line,
                context,
                field,
                &value.to_string(),
                "a value in its bound",
            );
            None
        }
        Err(mint_sage_ini::fire_logic_system::FireDataError::CellTypeIndex { .. }) => None,
    }
}

fn menu_point(block: &Block, reads: &Reads<'_>) -> Option<[f32; 3]> {
    block
        .fields_named("GeometryContactPoint")
        .filter(|f| f.args.iter().any(|a| a.eq_ignore_ascii_case("Menu")))
        .filter_map(|f| {
            let axis = |tag: &str| reads.tagged(f, tag);
            Some([axis("X")?, axis("Y")?, axis("Z")?])
        })
        .last()
}

fn vision_bonus(lineage: &[&Block], reads: &Reads<'_>) -> Option<VisionBonus> {
    let percent = |keyword: &str| reads.percent(nearest(lineage, keyword));
    let per_foot = percent("VisionBonusPercentPerFoot");
    let max = percent("MaxVisionBonusPercent");
    let min = percent("MinVisionBonusPercent");
    let test_radius = reads.number(nearest(lineage, "VisionBonusTestRadius"));
    let test_segments = reads.byte(nearest(lineage, "VisionBonusTestSegments"));
    Some(VisionBonus {
        per_foot: per_foot?,
        max: max.unwrap_or(VisionBonus::DEFAULT_MAX),
        min: min.unwrap_or(VisionBonus::DEFAULT_MIN),
        test_radius: test_radius.unwrap_or(VisionBonus::DEFAULT_TEST_RADIUS),
        test_segments: test_segments.unwrap_or(VisionBonus::DEFAULT_TEST_SEGMENTS),
    })
}

fn nearest<'b>(lineage: &[&'b Block], keyword: &str) -> Option<&'b Field> {
    lineage.iter().rev().find_map(|block| block.field(keyword))
}

struct Reads<'a> {
    context: &'a str,
    macros: &'a MacroTable,
    errors: &'a ValueErrors,
}

impl Reads<'_> {
    fn number(&self, field: Option<&Field>) -> Option<f32> {
        let field = field?;
        let read = field
            .value()
            .and_then(|raw| parse_float_or_macro(raw, self.macros))
            .filter(|value| value.is_finite());
        if read.is_none() {
            self.errors.field(self.context, field, "a number");
        }
        read
    }

    fn number_or_expression(&self, field: Option<&Field>) -> Option<f32> {
        let field = field?;
        let read = field_expression(field, self.macros).filter(|value| value.is_finite());
        if read.is_none() {
            self.errors.field(self.context, field, "a number");
        }
        read
    }

    fn percent(&self, field: Option<&Field>) -> Option<f32> {
        let field = field?;
        let read = resolve_percent(&field.args, self.macros).filter(|value| value.is_finite());
        if read.is_none() {
            self.errors.field(self.context, field, "a percentage");
        }
        read
    }

    fn flag(&self, field: Option<&Field>) -> Option<bool> {
        let field = field?;
        let read = field.value().and_then(parse_bool);
        if read.is_none() {
            self.errors.field(self.context, field, "Yes or No");
        }
        read
    }

    fn integer(&self, field: Option<&Field>) -> Option<i32> {
        let field = field?;
        let read = field
            .value()
            .and_then(parse_int)
            .and_then(|value| i32::try_from(value).ok());
        if read.is_none() {
            self.errors.field(self.context, field, "a whole number");
        }
        read
    }

    fn byte(&self, field: Option<&Field>) -> Option<u8> {
        let field = field?;
        let read = field.value().and_then(|raw| raw.parse::<u8>().ok());
        if read.is_none() {
            self.errors
                .field(self.context, field, "a whole number from 0 to 255");
        }
        read
    }

    fn radar_priority(&self, field: &Field) -> Option<RadarPriority> {
        let read = field.value().and_then(RadarPriority::parse);
        if read.is_none() {
            self.errors.field(self.context, field, "a radar priority");
        }
        read
    }

    fn ai_kind_of(&self, field: &Field) -> Option<AiKindOf> {
        let read = field.value().and_then(AiKindOf::parse);
        if read.is_none() {
            self.errors
                .field(self.context, field, "an AIKindOf category");
        }
        read
    }

    fn acquire(&self, field: &Field) -> Option<AutoAcquire> {
        let read = AutoAcquire::parse(&field.args);
        if read.is_none() {
            self.errors
                .field(self.context, field, "a list of idle-acquisition flags");
        }
        read
    }

    fn body_grab(&self, body: &Block) -> crate::body::BodyGrab {
        let name = |keyword: &str| {
            body.field(keyword)
                .and_then(|field| field.args.first())
                .cloned()
        };
        let defaults = crate::body::BodyGrab::default();
        let offset = body.field("GrabOffset").map_or(defaults.offset, |field| {
            (
                self.tagged(field, "X").unwrap_or(0.0),
                self.tagged(field, "Y").unwrap_or(0.0),
            )
        });
        crate::body::BodyGrab {
            object: name("GrabObject"),
            fx: name("GrabFX"),
            damage: self
                .number(body.field("GrabDamage"))
                .unwrap_or(defaults.damage),
            offset,
        }
    }

    fn damage_creations(&self, body: &Block) -> Vec<crate::body::DamageCreation> {
        body.fields_named("DamageCreationList")
            .filter_map(|field| {
                let read = crate::body::DamageCreation::parse(&field.args);
                if read.is_none() {
                    self.errors
                        .field(self.context, field, crate::body::DamageCreation::EXPECTED);
                }
                read
            })
            .collect()
    }

    fn build_completion(&self, field: &Field) -> Option<BuildCompletion> {
        let read = field.value().and_then(BuildCompletion::parse);
        if read.is_none() {
            self.errors.field(self.context, field, "a build completion");
        }
        read
    }

    fn weapon_choice_criteria(&self, field: &Field) -> Option<WeaponChoiceCriteria> {
        let read = field.value().and_then(WeaponChoiceCriteria::parse);
        if read.is_none() {
            self.errors
                .field(self.context, field, "a weapon choice criteria");
        }
        read
    }

    fn tagged(&self, field: &Field, tag: &str) -> Option<f32> {
        self.tagged_with(field, tag, "a number", |raw| {
            parse_float_or_macro(raw, self.macros).filter(|value| value.is_finite())
        })
    }

    fn tagged_percent(&self, field: &Field, tag: &str) -> Option<f32> {
        self.tagged_with(field, tag, "a percentage", |raw| {
            parse_percent_shaped(raw, self.macros)
        })
    }

    fn rank(&self, field: &Field, tag: &str) -> Option<u16> {
        self.tagged_with(field, tag, "a level", |raw| {
            raw.eq_ignore_ascii_case("Any")
                .then_some(None)
                .or_else(|| raw.parse::<u16>().ok().map(Some))
        })?
    }

    fn tagged_flag(&self, field: &Field, tag: &str) -> Option<bool> {
        self.tagged_with(field, tag, "Yes or No", parse_bool)
    }

    fn tagged_with<T>(
        &self,
        field: &Field,
        tag: &str,
        expected: &'static str,
        parser: impl FnOnce(&str) -> Option<T>,
    ) -> Option<T> {
        let raw = tagged_value(&field.args, tag)?;
        let read = parser(raw);
        if read.is_none() {
            self.errors
                .value(self.context, field, &format!("{tag}:{raw}"), expected);
        }
        read
    }
}

fn level(value: Option<f32>) -> Option<u8> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    value
        .filter(|v| v.is_finite())
        .map(|v| v.clamp(0.0, 255.0) as u8)
}

fn signed_level(value: Option<f32>) -> Option<i8> {
    #[allow(clippy::cast_possible_truncation)]
    value
        .filter(|v| v.is_finite())
        .map(|v| v.clamp(-128.0, 127.0) as i8)
}

fn module<'b>(modules: &[ModuleDeclaration<'b>], name: &str) -> Option<&'b Block> {
    modules
        .iter()
        .rev()
        .find(|decl| decl.name.eq_ignore_ascii_case(name))
        .map(|decl| decl.block)
}

#[must_use]
pub fn is_ai_module(name: &str) -> bool {
    crate::horde_ai_update::AI_MODULE_NAMES
        .iter()
        .any(|known| known.eq_ignore_ascii_case(name))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackAway {
    pub min_delay_ms: f32,
    pub max_delay_ms: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub percentage: f32,
    pub cower_radius: f32,
}

fn back_away(block: &Block, reads: &Reads<'_>) -> BackAway {
    let number =
        |keyword: &str, default: f32| reads.number(block.field(keyword)).unwrap_or(default);
    BackAway {
        min_delay_ms: number("BackUpMinDelayTime", 400.0),
        max_delay_ms: number("BackUpMaxDelayTime", 3000.0),
        min_distance: number("BackUpMinDistance", 3.0),
        max_distance: number("BackUpMaxDistance", 5.0),
        percentage: reads
            .percent(block.field("BackupPercentage"))
            .unwrap_or(0.5),
        cower_radius: number("CowerRadius", 0.0),
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SiegeRecord {
    pub docking: bool,
    pub gated: bool,
    pub deploy: Option<SiegeDeployDef>,
    pub stop_powers: Vec<(String, String)>,
    pub deploy_style: Option<DeployStyleDef>,
    pub toggle_deploy: Option<ToggleDeployDef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToggleDeployDef {
    pub power: String,
    pub sound_deploy: Option<String>,
    pub sound_undeploy: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SiegeDeployDef {
    pub power: String,
    pub lower_delay_ms: f32,
    pub raise_delay_ms: f32,
    pub evacuate_passengers_on_deploy: bool,
    pub evacuate_crew_on_deploy: bool,
    pub skip_adjust_position: bool,
    pub wall_search_distance: f32,
    pub away_from_wall_wait_dist: f32,
    pub extra_wall_distance: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeployStyleDef {
    pub unpack_time_ms: f32,
    pub pack_time_ms: f32,
    pub must_deploy_to_attack: bool,
    pub deployed_attribute_modifier: Option<String>,
    pub turrets_must_center_before_packing: bool,
}

fn siege_record(modules: &[ModuleDeclaration<'_>], reads: &Reads<'_>) -> SiegeRecord {
    let named = |name: &str| module(modules, name);
    let text = |block: &Block, keyword: &str| {
        block
            .field(keyword)
            .and_then(Field::value)
            .filter(|value| !value.eq_ignore_ascii_case("NONE"))
            .map(str::to_owned)
    };
    let flag = |block: &Block, keyword: &str, default: bool| {
        reads.flag(block.field(keyword)).unwrap_or(default)
    };
    let number = |block: &Block, keyword: &str, default: f32| {
        reads.number(block.field(keyword)).unwrap_or(default)
    };
    SiegeRecord {
        docking: named("SiegeDockingBehavior").is_some(),
        gated: named("SiegeAIUpdate").is_some(),
        deploy: named("SiegeDeploySpecialPower").map(|block| SiegeDeployDef {
            power: text(block, "SpecialPowerTemplate").unwrap_or_default(),
            lower_delay_ms: number(block, "LowerDelay", 0.0),
            raise_delay_ms: number(block, "RaiseDelay", 0.0),
            evacuate_passengers_on_deploy: flag(block, "EvacuatePassengersOnDeploy", true),
            evacuate_crew_on_deploy: flag(block, "EvacuateCrewOnDeploy", false),
            skip_adjust_position: flag(block, "SkipAdjustPosition", false),
            wall_search_distance: number(block, "WallSearchDistance", 500.0),
            away_from_wall_wait_dist: number(block, "AwayFromWallWaitDist", 200.0),
            extra_wall_distance: number(block, "ExtraWallDistance", 0.0),
        }),
        stop_powers: modules
            .iter()
            .filter(|decl| decl.name.eq_ignore_ascii_case("StopSpecialPower"))
            .filter_map(|decl| {
                Some((
                    text(decl.block, "SpecialPowerTemplate")?,
                    text(decl.block, "StopPowerTemplate")?,
                ))
            })
            .collect(),
        deploy_style: named("DeployStyleAIUpdate").map(|block| DeployStyleDef {
            unpack_time_ms: number(block, "UnpackTime", 0.0),
            pack_time_ms: number(block, "PackTime", 0.0),
            must_deploy_to_attack: flag(block, "MustDeployToAttack", false),
            deployed_attribute_modifier: text(block, "DeployedAttributeModifier"),
            turrets_must_center_before_packing: flag(
                block,
                "TurretsMustCenterBeforePacking",
                false,
            ),
        }),
        toggle_deploy: named("ToggleDeploySpecialAbilityUpdate").and_then(|block| {
            Some(ToggleDeployDef {
                power: text(block, "SpecialPowerTemplate")?,
                sound_deploy: text(block, "SoundDeploy"),
                sound_undeploy: text(block, "SoundUndeploy"),
            })
        }),
    }
}

const SPAWN_TIMER_DEFAULT: f32 = 30.0;

const REBUILD_COST_PERCENT_FIELDS: [&str; 4] = [
    "PercentOfBuildCostToRebuildPristine",
    "PercentOfBuildCostToRebuildDamaged",
    "PercentOfBuildCostToRebuildReallyDamaged",
    "PercentOfBuildCostToRebuildRubble",
];

const REBUILD_COST_PERCENT_DEFAULT: [f32; 4] = [0.25, 0.5, 0.75, 1.0];

fn ai_module<'b>(modules: &[ModuleDeclaration<'b>]) -> Option<&'b Block> {
    modules
        .iter()
        .rev()
        .find(|decl| is_ai_module(decl.name))
        .map(|decl| decl.block)
}

fn is_animal_ai(modules: &[ModuleDeclaration<'_>]) -> bool {
    modules
        .iter()
        .rev()
        .find(|decl| is_ai_module(decl.name))
        .is_some_and(|decl| decl.name.eq_ignore_ascii_case("AnimalAIUpdate"))
}

fn ai_table(block: Option<&Block>, is_animal: bool, reads: &Reads<'_>) -> AiTable {
    let Some(block) = block else {
        return AiTable::default();
    };
    let number = |keyword: &str| reads.number(block.field(keyword));
    let flag = |keyword: &str| reads.flag(block.field(keyword)).unwrap_or(false);
    AiTable {
        attack_priority: block
            .field("AttackPriority")
            .and_then(Field::value)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        stop_chase_distance: number("StopChaseDistance"),
        stand_ground: flag("StandGround"),
        can_attack_while_contained: flag("CanAttackWhileContained"),
        burning_death_time_ms: number("BurningDeathTime"),
        fade_on_portals: flag("FadeOnPortals"),
        rampage_time_ms: number("RampageTime"),
        eject_passengers_on_rampage_ms: number("TimeToEjectPassengersOnRampage"),
        rampage_requires_aflame: flag("RampageRequiresAflame"),
        special_contact_points: contact::special_contact_points(
            block.field("SpecialContactPoints"),
            reads,
        ),
        repair_health_per_second: reads.percent(block.field("RepairHealthPercentPerSecond")),
        hold_ground_close_range: number("HoldGroundCloseRangeDistance"),
        min_cower_time_ms: number("MinCowerTime"),
        max_cower_time_ms: number("MaxCowerTime"),
        turret: block.blocks_named("Turret").last().map(|turret| TurretDef {
            turn_rate: reads.number(turret.field("TurretTurnRate")),
            controlled_weapon_slots: turret
                .field("ControlledWeaponSlots")
                .map(|field| field.args.clone())
                .unwrap_or_default(),
            art_angle: reads.number(turret.field("TurretArtAngle")),
            recenter_time_ms: reads.number(turret.field("RecenterTime")),
            max_deflection_cw: reads.number(turret.field("TurretMaxDeflectionCW")),
            max_deflection_acw: reads.number(turret.field("TurretMaxDeflectionACW")),
            fires_while_turning: reads
                .flag(turret.field("FiresWhileTurning"))
                .unwrap_or(false),
        }),
        harvest: flag("HarvestTrees").then(|| HarvestDef {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            max_boxes: number("MaxBoxes").filter(|n| *n >= 0.0).unwrap_or(0.0) as u32,
            scan_distance: number("SupplyWarehouseScanDistance").unwrap_or(0.0),
            activation_range: number("HarvestActivationRange").unwrap_or(0.0),
            preparation_time_ms: number("HarvestPreparationTime").unwrap_or(0.0),
            action_time_ms: number("HarvestActionTime").unwrap_or(0.0),
            action_delay_ms: number("SupplyCenterActionDelay").unwrap_or(0.0),
        }),
        animal_wander: is_animal.then(|| AnimalWander {
            wander_percentage: number("WanderPercentage").unwrap_or(50.0),
            max_wander_distance: number("MaxWanderDistance").unwrap_or(10.0),
            max_wander_radius: number("MaxWanderRadius").unwrap_or(10.0),
            flee_range: number("FleeRange").unwrap_or(20.0),
            flee_distance: number("FleeDistance").unwrap_or(100.0),
            #[allow(clippy::cast_possible_truncation)]
            update_timer: number("UpdateTimer").map_or(10, |n| n as i32),
        }),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AiKindOf {
    Infantry,
    Archer,
    Pikeman,
    Cavalry,
    Creep,
    CreepStructure,
    Structure,
    SiegeWeapon,
    ExplorableArea,
    Wall,
    Hero,
    BattleTower,
    ShipBattleship,
    ShipBombard,
    ShipTransport,
    ShipSuicide,
    Support,
}

impl AiKindOf {
    const NAMES: [(&'static str, Self); 17] = [
        ("INFANTRY", Self::Infantry),
        ("ARCHER", Self::Archer),
        ("PIKEMAN", Self::Pikeman),
        ("CAVALRY", Self::Cavalry),
        ("CREEP", Self::Creep),
        ("CREEP_STRUCTURE", Self::CreepStructure),
        ("STRUCTURE", Self::Structure),
        ("SIEGEWEAPON", Self::SiegeWeapon),
        ("EXPLORABLE_AREA", Self::ExplorableArea),
        ("WALL", Self::Wall),
        ("HERO", Self::Hero),
        ("BATTLE_TOWER", Self::BattleTower),
        ("SHIP_BATTLESHIP", Self::ShipBattleship),
        ("SHIP_BOMBARD", Self::ShipBombard),
        ("SHIP_TRANSPORT", Self::ShipTransport),
        ("SHIP_SUICIDE", Self::ShipSuicide),
        ("SUPPORT", Self::Support),
    ];

    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(token))
            .map(|(_, value)| *value)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum RadarPriority {
    #[default]
    Invalid,
    NotOnRadar,
    Structure,
    Unit,
    LocalUnitOnly,
}

impl RadarPriority {
    const NAMES: [(&'static str, Self); 5] = [
        ("INVALID", Self::Invalid),
        ("NOT_ON_RADAR", Self::NotOnRadar),
        ("STRUCTURE", Self::Structure),
        ("UNIT", Self::Unit),
        ("LOCAL_UNIT_ONLY", Self::LocalUnitOnly),
    ];

    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(token))
            .map(|(_, priority)| *priority)
    }

    #[must_use]
    pub const fn is_visible(self) -> bool {
        !matches!(self, Self::Invalid | Self::NotOnRadar)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildCompletion {
    Invalid,
    AppearsAtRallyPoint,
    PlacedByPlayer,
}

impl BuildCompletion {
    const NAMES: [(&'static str, Self); 3] = [
        ("INVALID", Self::Invalid),
        ("APPEARS_AT_RALLY_POINT", Self::AppearsAtRallyPoint),
        ("PLACED_BY_PLAYER", Self::PlacedByPlayer),
    ];

    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(token))
            .map(|(_, value)| *value)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct AutoAcquire(u8);

impl AutoAcquire {
    pub const IDLE: Self = Self(0x01);
    pub const STEALTHED: Self = Self(0x02);
    pub const NO: Self = Self(0x04);
    pub const NOT_WHILE_ATTACKING: Self = Self(0x08);
    pub const ATTACK_BUILDINGS: Self = Self(0x10);

    const NAMES: [(&'static str, Self); 5] = [
        ("YES", Self::IDLE),
        ("STEALTHED", Self::STEALTHED),
        ("NO", Self::NO),
        ("NOTWHILEATTACKING", Self::NOT_WHILE_ATTACKING),
        ("ATTACK_BUILDINGS", Self::ATTACK_BUILDINGS),
    ];

    #[must_use]
    pub fn parse(tokens: &[String]) -> Option<Self> {
        let mut bits = 0;
        for token in tokens {
            let (_, flag) = Self::NAMES
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(token))?;
            bits |= flag.0;
        }
        Some(Self(bits))
    }

    #[must_use]
    pub const fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 == flag.0
    }
}

fn resolve_kind_of(lineage: &[&Block], reads: &Reads<'_>) -> Vec<String> {
    let mut kinds: Vec<String> = Vec::new();
    for block in lineage {
        for field in block.fields_named("KindOf") {
            let edits = match bit_string_edits(&field.args) {
                Ok(edits) => edits,
                Err(token) => {
                    reads
                        .errors
                        .value(reads.context, field, token, BIT_STRING_EXPECTED);
                    continue;
                }
            };
            for edit in edits {
                match edit {
                    BitEdit::Clear => kinds.clear(),
                    BitEdit::Set(kind) => insert_kind(&mut kinds, &kind.to_ascii_uppercase()),
                    BitEdit::Unset(kind) => {
                        let kind = kind.to_ascii_uppercase();
                        kinds.retain(|held| *held != kind);
                    }
                }
            }
        }
    }
    kinds
}

fn insert_kind(kinds: &mut Vec<String>, kind: &str) {
    if !kind.is_empty() && !kinds.iter().any(|held| held == kind) {
        kinds.push(kind.to_owned());
    }
}

fn default_set<'b>(lineage: &[&'b Block], keyword: &str) -> Option<&'b Block> {
    let owner: &'b Block = lineage
        .iter()
        .rev()
        .find(|block| block.blocks_named(keyword).next().is_some())?;
    let mut best: Option<(usize, &'b Block)> = None;
    for set in owner
        .blocks()
        .filter(|set| set.keyword.eq_ignore_ascii_case(keyword))
    {
        let kind = if keyword.eq_ignore_ascii_case("WeaponSet") {
            SetKind::Weapon
        } else {
            SetKind::Armor
        };
        let conditions = set.field("Conditions").map_or(0, |field| {
            condition_flags(kind, &field.args).map_or(0, |flags| flags.len())
        });
        if best.is_none_or(|(fewest, _)| conditions < fewest) {
            best = Some((conditions, set));
        }
    }
    best.map(|(_, set)| set)
}

fn weapon_slots(set: &Block, reads: &Reads<'_>) -> Vec<SlotDeclaration> {
    let mut slots: Vec<SlotDeclaration> = Vec::new();
    let slot_of = |field: &Field| {
        let read = field
            .args
            .split_first()
            .and_then(|(token, rest)| Some((WeaponSlot::parse(token)?, rest.to_vec())));
        if read.is_none() {
            reads
                .errors
                .field(reads.context, field, "a weapon slot and its value");
        }
        read
    };
    for field in set.fields_named("Weapon") {
        let Some((slot, rest)) = slot_of(field) else {
            continue;
        };
        let Some(name) = rest
            .first()
            .filter(|name| !name.eq_ignore_ascii_case("NONE"))
        else {
            continue;
        };
        match slots.iter_mut().find(|held| held.slot == slot) {
            Some(held) => held.weapon.clone_from(name),
            None => slots.push(SlotDeclaration {
                slot,
                weapon: name.clone(),
                sources: SourceMask::default(),
                preferred_against: Vec::new(),
                only_against: Vec::new(),
                only_in_condition: Vec::new(),
            }),
        }
    }
    for field in set.fields_named("AutoChooseSources") {
        let Some((slot, rest)) = slot_of(field) else {
            continue;
        };
        match SourceMask::parse(&rest) {
            Ok(mask) => {
                if let Some(held) = slots.iter_mut().find(|held| held.slot == slot) {
                    held.sources = mask;
                }
            }
            Err(_) => reads
                .errors
                .field(reads.context, field, "a list of command sources"),
        }
    }
    for (keyword, only) in [("PreferredAgainst", false), ("OnlyAgainst", true)] {
        for field in set.fields_named(keyword) {
            let Some((slot, rest)) = slot_of(field) else {
                continue;
            };
            let kinds: Vec<String> = rest.iter().map(|kind| kind.to_ascii_uppercase()).collect();
            if let Some(held) = slots.iter_mut().find(|held| held.slot == slot) {
                if only {
                    held.only_against = kinds;
                } else {
                    held.preferred_against = kinds;
                }
            }
        }
    }
    for field in set.fields_named("OnlyInCondition") {
        let Some((slot, rest)) = slot_of(field) else {
            continue;
        };
        if rest.is_empty() {
            reads
                .errors
                .field(reads.context, field, "a weapon slot and its conditions");
            continue;
        }
        if let Some(held) = slots.iter_mut().find(|held| held.slot == slot) {
            held.only_in_condition = rest
                .iter()
                .map(|token| token.to_ascii_uppercase())
                .collect();
        }
    }
    slots.sort_by_key(|held| held.slot);
    slots
}

fn primary_weapon(set: &Block) -> Option<String> {
    set.fields_named("Weapon").find_map(|field| {
        let [slot, name, ..] = field.args.as_slice() else {
            return None;
        };
        (slot.eq_ignore_ascii_case(PRIMARY) && !name.eq_ignore_ascii_case("NONE"))
            .then(|| name.clone())
    })
}

fn exit_modules<'b>(modules: &[ModuleDeclaration<'b>]) -> Vec<&'b Block> {
    modules
        .iter()
        .rev()
        .filter(|decl| {
            decl.name
                .to_ascii_lowercase()
                .ends_with("productionexitupdate")
        })
        .map(|decl| decl.block)
        .collect()
}

fn exit_point(block: &Block, keyword: &str, reads: &Reads<'_>) -> Option<[f32; 2]> {
    let field = block.field(keyword)?;
    let axis = |tag: &str| reads.tagged(field, tag);
    Some([axis("X")?, axis("Y")?])
}

fn castle(block: &Block, reads: &Reads<'_>) -> Castle {
    Castle {
        unpack: block
            .fields_named("CastleToUnpackForFaction")
            .filter_map(|field| match field.args.as_slice() {
                [side, castle, ..] => Some((side.clone(), castle.clone())),
                _ => None,
            })
            .collect(),
        pre_built: block
            .fields_named("PreBuiltList")
            .filter_map(|field| {
                let [template, number] = field.args.as_slice() else {
                    reads
                        .errors
                        .field(reads.context, field, "a template and a number");
                    return None;
                };
                let Some(number) = parse_int(number) else {
                    reads
                        .errors
                        .value(reads.context, field, number, "a whole number");
                    return None;
                };
                Some((template.clone(), number))
            })
            .collect(),
        owned_filter: block
            .field("FilterValidOwnedEntries")
            .map(|field| field.args.clone())
            .unwrap_or_default(),
        instant_unpack: reads.flag(block.field("InstantUnpack")).unwrap_or(false),
        keep_death_kills_everything: reads
            .flag(block.field("KeepDeathKillsEverything"))
            .unwrap_or(false),
        max_radius: reads.number(block.field("MaxCastleRadius")),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
