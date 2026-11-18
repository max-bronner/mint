use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use mint_movement::{
    Agent, Asker, AvoidanceMode, Cell, Facing, GoalId, MotionLimits, MotionStatus, MoveContext,
    MoverClass, MoverId, MoverState, Neighbour, NeighbourQuery, NodeBudget, Passability as _,
    PathRequest, Pipeline, PipelineConfig, PlanOutcome, Reachability, Route, SearchMode,
    SmoothingMode, Vec2,
};
use mint_sim::{
    Command, CommandKey, Component, ComponentId, Effects, Entity, Frame, LogicFrames, PlayerId,
    Result, Rules, StateHasher, Tick, UpgradeBit, UpgradeMask, World, SIMULATION_HZ,
};

use crate::combat::{
    armour_of, crush_damage_events, grid_cell, grid_radius_cells, in_range, launch_point,
    run_crush, slow_death_options, sort_damage, staggered_first_acquire, target_allowed,
    tick_weapon, Armed, Armour, AttackMoveOrder, AttackOrder, CrushState, DamageEvent, DeathNaming,
    Doomed, Flight, Foe, Health, HordeTarget, Shot, TargetGrid, WeaponSlots,
    DEFAULT_DESTRUCTION_DELAY, MOOD_ATTACK_CHECK,
};
use mint_sage_formation::FormationTable;

use crate::horde::{FormationMode, Horde, HordeIds, HordeSpawn, Member};
use crate::unit::{Body, Modes, Owner, Path, PathKind, Selected, Shape, Spawn, PLACEHOLDER_LIMITS};
use mint_sage_asset::KindOf;
use mint_sage_combat::{
    choose_slow_death, ArmorTemplate, BodyKind, CombatData, HealthPool, ProjectileFlight, WeaponId,
    WeaponState, WeaponStatus,
};
use mint_sage_passability::{DynamicView, FootprintRecord, Obstacle, Passability, GROUND};
use mint_sage_shroud::PARTITION_CELL_SIZE;

use crate::construction::{
    footprint_of, price_of, BuildTask, Concealed, Construction, Reason, SiteWorker, WorkerStage,
};
use crate::defeat::{DefeatIds, MatchOutcome, ObjectFilter};
use crate::economy::{
    Buildable, EconomyIds, Income, ObjectUpgrades, PlayerSlot, PlayerUpgrades, Points, Producer,
    Purse, Upkeep,
};
use crate::obstacles::{ObstacleLayer, ScaleableWalls};
use crate::powers::PowerBook;
use crate::shroud::{Seen, Shroud, ShroudIds, Vision};
use crate::templates::{KindTable, Production, TargetKind};

mod ai_attack;
mod ai_build;
mod ai_powers;
mod amphibious;
mod animal_ai;
mod aod_crush;
mod attach;
mod attack;
mod attack_ground;
mod auto_cast;
mod avoidance;
pub(crate) mod border;
mod busy;
mod cah;
mod cah_preview;
pub(crate) use cah_preview::{CahPreview, ExamineRaise};
mod capture;
pub(crate) mod carryover;
mod castle;
mod cell_change;
mod charge_pass;
mod command;
mod construct;
mod contact;
pub(crate) mod contain;
mod crates;
mod crush_notice;
mod damage_creation;
mod damage_fx;
pub(crate) mod death;
mod debug;
pub(crate) mod difficulty;
mod disguise;
mod doors;
mod drag_formation;
mod economy;
mod emotion_states;
mod emotions;
mod enemy_contact;
mod engaged;
mod match_stats;
mod misc_sounds;
pub(crate) use misc_sounds::MiscSound;
mod experience;
mod fire;
mod flank;
mod forest_powers;
mod formation_arrow;
pub(crate) use formation_arrow::ArrowPiece;
mod formation_preview;
pub(crate) use formation_preview::{FormationPreviewItem, PreviewDecals};
mod gates;
mod geometry;
mod giant_bird;
pub(crate) use engaged::EngagedDefaults;
mod group_bonus;
mod group_move;
mod guard;
mod harvest;
mod heroes;
mod hit_reaction;
mod horde_melee;
mod horde_swarm;
mod knockback;
mod loose_melee;
mod map_keys;
pub(crate) mod map_scripts;
mod melee_engage;
mod move_allies;
mod movement;
mod order_queue;
mod orders;
mod path_footprint;
mod pick_up;
mod porcupine;
mod projectile;
mod rally;
mod rampage;
mod rebuild;
mod regeneration;
mod relationship;
mod repair;
mod replenish;
mod script_events;
mod selection;
mod sell;
mod shooting;
pub(crate) mod siege;
mod spawning;
mod spell_effects;
mod spells;
mod stances;
mod stealth;
mod targeting;
pub(crate) mod terrain_fire;
mod unit_cells;
mod unit_costs;
mod unit_index;
mod upgrades;
mod wall;
mod wall_link;
mod wall_upgrade;
mod weapon_fire;
mod weather;
use crate::diag::diag;
pub(crate) use aod_crush::{AodCrusher, LargeUnitTouch};
use avoidance::{AvoidanceGrid, IgnoringTarget};
pub(crate) use command::{RotwkCommand, UiPanel};
pub(crate) use crates::CrateTextKind;
use debug::NoAttack;
pub(crate) use debug::{DebugCommand, DebugTarget, LevelStep, SpawnRole, UnitEntry};
pub(crate) use economy::IncomeView;
pub(crate) use gates::{GateAction, GateButtons};
pub(crate) use group_move::GroupFormation;
pub(crate) use melee_engage::MeleePlacement;
#[cfg(test)]
use movement::{last_straggler_rescue_spent, STRAGGLER_RESCUE_BUDGET_PER_TICK};
pub(crate) use movement::{Held, IgnoredObstacle, PathBudgets, PathQueue, PathRetry, ReplannedAt};
use movement::{LOOSE_GROUP_OFFSET_CAP_RADII, PATH_NODE_BUDGET};
pub(crate) use rebuild::SelfRepair;
#[cfg(test)]
use shooting::force_victim_on_wall_layer;
pub(crate) use spells::{PowerButton, SpellBarReadiness};
pub(crate) use stances::CombatStance;
pub(crate) use upgrades::{ModuleWiring, UsingAbility};

pub(crate) const MAX_SELECTION_UNITS: usize = 512;

pub(crate) const DEFAULT_MAX_QUEUE_ENTRIES: u32 = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MatchClock(pub(crate) u64);

impl Component for MatchClock {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u64(self.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlayerOrder;

impl Component for PlayerOrder {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct SuspendedOrder {
    pub(crate) attack_order: Option<crate::combat::AttackOrder>,
    pub(crate) player_order: bool,
}

impl Component for SuspendedOrder {
    fn hash_into(&self, hasher: &mut StateHasher) {
        match self.attack_order {
            Some(order) => {
                hasher.write_u8(1);
                order.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        hasher.write_bool(self.player_order);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RosterEntry {
    pub(crate) index: u16,
    pub(crate) label: String,
    pub(crate) is_horde: bool,
    pub(crate) member_count: u32,
    pub(crate) side: Option<String>,
    pub(crate) kind_of: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct StructureEntry {
    pub(crate) kind: crate::unit::Kind,
    pub(crate) label: String,
    pub(crate) side: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Ids {
    pub(crate) clock: ComponentId<MatchClock>,
    pub(crate) body: ComponentId<Body>,
    pub(crate) shape: ComponentId<Shape>,
    pub(crate) owner: ComponentId<Owner>,
    pub(crate) selected: ComponentId<Selected>,
    pub(crate) path: ComponentId<Path>,
    pub(crate) modes: ComponentId<Modes>,
    pub(crate) health: ComponentId<Health>,
    pub(crate) armour: ComponentId<Armour>,
    pub(crate) armed: ComponentId<Armed>,
    pub(crate) weapon_slots: ComponentId<WeaponSlots>,
    pub(crate) attack_order: ComponentId<AttackOrder>,
    pub(crate) flight: ComponentId<Flight>,
    pub(crate) doomed: ComponentId<Doomed>,
    pub(crate) horde: ComponentId<Horde>,
    pub(crate) member: ComponentId<Member>,
    pub(crate) kind: ComponentId<crate::unit::Kind>,
    pub(crate) slot: ComponentId<PlayerSlot>,
    pub(crate) purse: ComponentId<Purse>,
    pub(crate) points: ComponentId<Points>,
    pub(crate) producer: ComponentId<Producer>,
    pub(crate) income: ComponentId<Income>,
    pub(crate) upkeep: ComponentId<Upkeep>,
    pub(crate) shroud: ComponentId<Shroud>,
    pub(crate) vision: ComponentId<Vision>,
    pub(crate) seen: ComponentId<Seen>,
    pub(crate) obstacles: ComponentId<ObstacleLayer>,
    pub(crate) portals: ComponentId<crate::portals::PortalLayer>,
    pub(crate) gate_cycle: ComponentId<gates::GateCycle>,
    pub(crate) replanned_at: ComponentId<ReplannedAt>,
    pub(crate) construction: ComponentId<Construction>,
    pub(crate) build_task: ComponentId<BuildTask>,
    pub(crate) concealed: ComponentId<Concealed>,
    pub(crate) outcome: ComponentId<MatchOutcome>,
    pub(crate) ai_progress: ComponentId<crate::skirmish_ai::AiProgress>,
    pub(crate) ai_army_progress: ComponentId<crate::skirmish_ai::AiArmyProgress>,
    pub(crate) ai_attack_progress: ComponentId<crate::skirmish_ai::AiAttackProgress>,
    pub(crate) path_queue: ComponentId<PathQueue>,
    pub(crate) horde_target: ComponentId<HordeTarget>,
    pub(crate) attack_move_order: ComponentId<AttackMoveOrder>,
    pub(crate) player_upgrades: ComponentId<crate::economy::PlayerUpgrades>,
    pub(crate) player_upgrades_in_progress: ComponentId<crate::economy::PlayerUpgradesInProgress>,
    pub(crate) object_upgrades: ComponentId<crate::economy::ObjectUpgrades>,
    pub(crate) power_book: ComponentId<crate::powers::PowerBook>,
    pub(crate) player_power_cooldowns: ComponentId<crate::powers::PlayerPowerCooldowns>,
    pub(crate) experience: ComponentId<crate::experience::Experience>,
    pub(crate) bonuses: ComponentId<crate::experience::Bonuses>,
    pub(crate) hero_roster: ComponentId<crate::heroes::HeroRoster>,
    pub(crate) player_science: ComponentId<crate::sciences::PlayerScience>,
    pub(crate) timed_modifiers: ComponentId<crate::experience::TimedModifiers>,
    pub(crate) lifetime: ComponentId<crate::summons::Lifetime>,
    pub(crate) fx: ComponentId<crate::fx::FxEvents>,
    pub(crate) pending_shots: ComponentId<crate::spell_state::PendingShots>,
    pub(crate) weapon_clocks: ComponentId<crate::spell_state::WeaponClocks>,
    pub(crate) dots: ComponentId<crate::spell_state::DamageOverTime>,
    pub(crate) suppressed: ComponentId<crate::spell_state::Suppressed>,
    pub(crate) cash_bounty: ComponentId<crate::spell_state::CashBounty>,
    pub(crate) production_bonuses: ComponentId<crate::spell_state::ProductionBonuses>,
    pub(crate) last_hurt: ComponentId<crate::spell_state::LastHurt>,
    pub(crate) sole_healer: ComponentId<crate::spell_state::SoleHealer>,
    pub(crate) healed_at: ComponentId<crate::spell_state::HealedAt>,
    pub(crate) point_bonus: ComponentId<crate::spell_state::PointBonus>,
    pub(crate) point_rule: ComponentId<crate::spell_state::PointRule>,
    pub(crate) castle_member: ComponentId<crate::castle::CastleMember>,
    pub(crate) pad_occupant: ComponentId<crate::castle::PadOccupant>,
    pub(crate) voice: ComponentId<crate::voice::VoiceEvents>,
    pub(crate) locomotion: ComponentId<crate::unit::Locomotion>,
    pub(crate) player_order: ComponentId<PlayerOrder>,
    pub(crate) no_attack: ComponentId<NoAttack>,
    pub(crate) capture_bonus: ComponentId<capture::CaptureBonus>,
    pub(crate) capture_links: ComponentId<capture::CaptureLinks>,
    pub(crate) capturer: ComponentId<capture::Capturer>,
    pub(crate) rebuild_hole: ComponentId<rebuild::RebuildHole>,
    pub(crate) sold: ComponentId<rebuild::Sold>,
    pub(crate) wall_links: ComponentId<wall_link::WallLinks>,
    pub(crate) ground_offset: ComponentId<crate::unit::GroundOffset>,
    pub(crate) corpse_sink: ComponentId<death::CorpseSink>,
    pub(crate) player_point_bonus: ComponentId<crate::spell_state::PointBonus>,
    pub(crate) auto_cast: ComponentId<auto_cast::AutoCastSlot>,
    pub(crate) using_ability: ComponentId<upgrades::UsingAbility>,
    pub(crate) no_auto_acquire: ComponentId<upgrades::NoAutoAcquire>,
    pub(crate) motive: ComponentId<crate::unit::Motive>,
    pub(crate) castle_upgrades: ComponentId<crate::castle::CastleUpgrades>,
    pub(crate) delayed_hits: ComponentId<crate::spell_state::DelayedHits>,
    pub(crate) defect_revert: ComponentId<capture::DefectRevert>,
    pub(crate) arrow_storm_hits: ComponentId<crate::spell_state::ArrowStormHits>,
    pub(crate) path_retry: ComponentId<movement::PathRetry>,
    pub(crate) emotion: ComponentId<emotions::Emotion>,
    pub(crate) emotion_table: ComponentId<emotions::EmotionTable>,
    pub(crate) horde_combat: ComponentId<replenish::HordeCombat>,
    pub(crate) banner: ComponentId<replenish::Banner>,
    pub(crate) passengers: ComponentId<contain::Passengers>,
    pub(crate) contained: ComponentId<contain::Contained>,
    pub(crate) entering: ComponentId<contain::Entering>,
    pub(crate) guard: ComponentId<guard::Guard>,
    pub(crate) stance: ComponentId<CombatStance>,
    pub(crate) knocked: ComponentId<knockback::Knocked>,
    pub(crate) contact_claim: ComponentId<contact::ContactClaim>,
    pub(crate) stealth: ComponentId<stealth::Stealth>,
    pub(crate) group_bonus: ComponentId<group_bonus::GroupBonus>,
    pub(crate) hit_reaction: ComponentId<hit_reaction::HitReaction>,
    pub(crate) payload_filled: ComponentId<contain::PayloadFilled>,
    pub(crate) special_conditions: ComponentId<hit_reaction::SpecialConditions>,
    pub(crate) damage_fx_throttle: ComponentId<damage_fx::DamageFxThrottle>,
    pub(crate) killing_blow: ComponentId<death::KillingBlow>,
    pub(crate) elevation: ComponentId<crate::unit::Elevation>,
    pub(crate) site_worker: ComponentId<SiteWorker>,
    pub(crate) ignored_obstacle: ComponentId<movement::IgnoredObstacle>,
    pub(crate) claimant: ComponentId<crate::claims::Claimant>,
    pub(crate) claims: ComponentId<crate::claims::ClaimLayer>,
    pub(crate) flight_level: ComponentId<movement::FlightLevel>,
    pub(crate) held: ComponentId<movement::Held>,
    pub(crate) burning: ComponentId<death::Burning>,
    pub(crate) ground_order: ComponentId<crate::combat::GroundOrder>,
    pub(crate) flanked: ComponentId<flank::FlankMemory>,
    pub(crate) crush_contacts: ComponentId<crate::combat::CrushContacts>,
    pub(crate) dynamic_portal_stamp: ComponentId<gates::DynamicPortalStamp>,
    pub(crate) corpse_fade: ComponentId<death::CorpseFade>,
    pub(crate) giant_bird: ComponentId<giant_bird::GiantBird>,
    pub(crate) horde_melee: ComponentId<crate::melee::HordeMelee>,
    pub(crate) member_melee: ComponentId<crate::melee::MemberMelee>,
    pub(crate) leader_trail: ComponentId<horde_swarm::LeaderTrail>,
    pub(crate) member_attack: ComponentId<crate::melee::MemberAttack>,
    pub(crate) loose_melee: ComponentId<loose_melee::LooseMelee>,
    pub(crate) charge_pass: ComponentId<charge_pass::ChargePass>,
    pub(crate) goal_cells: ComponentId<crate::goal_cells::GoalCells>,
    pub(crate) dummy: ComponentId<debug::Dummy>,
    pub(crate) contact: ComponentId<enemy_contact::Contact>,
    pub(crate) cell_mark: ComponentId<cell_change::CellMark>,
    pub(crate) crush_speed_cap: ComponentId<crate::combat::CrushSpeedCap>,
    pub(crate) desired_speed: ComponentId<crate::unit::DesiredSpeed>,
    pub(crate) formation_speed: ComponentId<crate::unit::FormationSpeed>,
    pub(crate) crush_notice: ComponentId<crush_notice::CrushNotice>,
    pub(crate) siege_deploy: ComponentId<siege::SiegeDeploy>,
    pub(crate) dock_occupants: ComponentId<siege::DockOccupants>,
    pub(crate) deploy_style: ComponentId<siege::DeployStyle>,
    pub(crate) forest: ComponentId<crate::forest::Forest>,
    pub(crate) last_attacker: ComponentId<guard::LastAttacker>,
    pub(crate) deferred_order: ComponentId<busy::DeferredOrder>,
    pub(crate) harvesting: ComponentId<harvest::Harvesting>,
    pub(crate) approach_wait: ComponentId<shooting::ApproachWait>,
    pub(crate) ranged_approach: ComponentId<attack::RangedApproach>,
    pub(crate) locomotor_override: ComponentId<upgrades::LocomotorOverride>,
    pub(crate) horde_last_shot: ComponentId<replenish::HordeLastShot>,
    pub(crate) turret_state: ComponentId<shooting::TurretState>,
    pub(crate) straggler_clock: ComponentId<contain::StragglerClock>,
    pub(crate) production_stream: ComponentId<crate::horde::ProductionStream>,
    pub(crate) in_formation_template: ComponentId<drag_formation::InFormationTemplate>,
    pub(crate) rampaging: ComponentId<rampage::Rampaging>,
    pub(crate) flame_heat: ComponentId<fire::FlameHeat>,
    pub(crate) aflame: ComponentId<fire::Aflame>,
    pub(crate) pending_eject: ComponentId<rampage::PendingEject>,
    pub(crate) animal_origin: ComponentId<animal_ai::AnimalOrigin>,
    pub(crate) aod_crusher: ComponentId<aod_crush::AodCrusher>,
    pub(crate) amphibious_unload: ComponentId<amphibious::AmphibiousUnload>,
    pub(crate) ability_weapon_clocks: ComponentId<crate::spell_state::AbilityWeaponClocks>,
    pub(crate) suspended_order: ComponentId<SuspendedOrder>,
    pub(crate) auto_repair: ComponentId<crate::construction::AutoRepair>,
    pub(crate) horde_trail: ComponentId<crate::horde_trail::HordeTrail>,
    pub(crate) glided: ComponentId<crate::horde_trail::Glided>,
    pub(crate) ai_power_refire: ComponentId<ai_powers::AiPowerRefire>,
    pub(crate) rally_events: ComponentId<rally::RallyEvents>,
    pub(crate) join_fade: ComponentId<crate::horde::JoinFade>,
    pub(crate) death_phase: ComponentId<crate::death_phase::DeathPhaseEvents>,
    pub(crate) death_phase_clock: ComponentId<death::DeathPhaseClock>,
    pub(crate) spell_recharge: ComponentId<crate::spell_state::SpellRecharge>,
    pub(crate) undetected_defector: ComponentId<crates::UndetectedDefector>,
    pub(crate) geometry_from: ComponentId<crate::unit::GeometryFrom>,
    pub(crate) crate_texts: ComponentId<crates::CrateTexts>,
    pub(crate) combat_facts: ComponentId<crate::combat_facts::CombatFacts>,
    pub(crate) engaged: ComponentId<engaged::Engaged>,
    pub(crate) misc_sounds: ComponentId<misc_sounds::MiscSounds>,
    pub(crate) map_placement: ComponentId<crate::unit::MapPlacement>,
    pub(crate) wall_layer: ComponentId<crate::wall_layer::WallLayer>,
    pub(crate) layer: ComponentId<crate::wall_layer::Layer>,
    pub(crate) map_script_state: ComponentId<map_scripts::MapScriptState>,
    pub(crate) script_name: ComponentId<map_scripts::ScriptObjectName>,
    pub(crate) attach_link: ComponentId<attach::AttachLink>,
    pub(crate) disallowed_buildings: ComponentId<crate::economy::DisallowedBuildings>,
    pub(crate) forced_model_condition: ComponentId<map_scripts::ForcedModelCondition>,
    pub(crate) fire_grid: ComponentId<terrain_fire::FireGrid>,
    pub(crate) build_switches: ComponentId<crate::economy::BuildSwitches>,
    pub(crate) order_queue: ComponentId<crate::combat::OrderQueue>,
    pub(crate) scaleable_walls: ComponentId<ScaleableWalls>,
    pub(crate) horde_member_mark: ComponentId<crate::horde::HordeMemberMark>,
    pub(crate) defeat_facts: ComponentId<crate::defeat::DefeatFacts>,
    pub(crate) timed_conditions: ComponentId<crate::experience::TimedConditions>,
    pub(crate) granted_hiding: ComponentId<stealth::GrantedHiding>,
    pub(crate) weather: ComponentId<crate::spell_state::Weather>,
    pub(crate) weather_effect: ComponentId<crate::spell_state::WeatherEffect>,
    pub(crate) bonus_conditions: ComponentId<crate::experience::WeaponBonusConditions>,
    pub(crate) ability_loop_sound: ComponentId<upgrades::AbilityLoop>,
    pub(crate) capture_flashes: ComponentId<capture::CaptureFlashes>,
    pub(crate) stored_objects: ComponentId<crate::spell_state::StoredObjectLists>,
    pub(crate) ignore_ai_command: ComponentId<busy::IgnoreAiCommand>,
    pub(crate) die_ocl_stamp: ComponentId<damage_creation::DieOclStamp>,
    pub(crate) cah_record: ComponentId<crate::create_a_hero::PlayerHero>,
    pub(crate) match_stats: ComponentId<crate::match_stats::MatchStats>,
    pub(crate) object_value: ComponentId<crate::economy::ObjectValue>,
    pub(crate) border: ComponentId<crate::rules::border::ActiveBorder>,
    pub(crate) human_barred: ComponentId<crate::rules::border::HumanBarred>,
    pub(crate) events_list: ComponentId<map_keys::EventsList>,
}

pub(crate) struct RotwkRules {
    passability: Passability,
    spawns: Vec<Spawn>,
    ids: Option<Ids>,
    emotions: Arc<mint_sage_ini::EmotionIndex>,
    stances: Arc<mint_sage_ini::StanceTemplateIndex>,
    ai_power_buttons: Arc<mint_sage_ini::CommandButtonIndex>,
    command_sets: Arc<mint_sage_ini::CommandSetIndex>,
    ai_power_activation_tuning: [(i64, i64); 4],
    ai_power_difficulty: Vec<(PlayerId, crate::lobby::Difficulty)>,
    difficulty: difficulty::DifficultySetup,
    combat_chains: crate::combat::CombatChainTable,
    combat: CombatData,
    unarmoured: ArmorTemplate,
    formations: Vec<FormationTable>,
    melee_hordes: Vec<Option<f32>>,
    horde_rangefinders: Vec<Option<f32>>,
    hordes: Vec<HordeSpawn>,
    buildables: Vec<Buildable>,
    menus: Vec<Vec<u16>>,
    alignments: Vec<mint_sage_economy::Alignment>,
    players: u8,
    starting_cash: i64,
    starting_cash_overrides: Vec<Option<i64>>,
    initial_upgrades: Vec<UpgradeMask>,
    production_cost_change: Vec<f32>,
    use_shroud: bool,
    structure_footprints: Vec<(usize, Vec<mint_sage_passability::FootprintRecord>)>,
    wall_geometry: Vec<mint_sage_passability::WallGeometry>,
    map_placements: Vec<(usize, u32)>,
    script_object_names: Vec<(usize, String)>,
    script_teams: Vec<(usize, String)>,
    indestructible: Vec<usize>,
    placed_keys: Vec<(usize, crate::map_keys::ObjectKeys)>,
    placed_hordes: Vec<PlacedHorde>,
    forest_seeds: Vec<crate::forest::ForestTreeInit>,
    tree_templates: crate::forest::TreeTemplates,
    fire_grid: Option<terrain_fire::FireGrid>,
    castle_members: Vec<(usize, usize)>,
    castle_layouts: Arc<crate::castle::CastleLayouts>,
    kinds: Arc<KindTable>,
    ai_kinds: Vec<bool>,
    production: Arc<Production>,
    construct_menus: Vec<Vec<crate::unit::Kind>>,
    construct_menu_of: Vec<Option<u16>>,
    construction_cancel_refund_percent: f32,
    max_cells_adjust_horde_melee: u32,
    max_cells_adjust_destination: u32,
    max_cells_adjust_to_possible_destination: u32,
    max_tunnel_capacity: u32,
    melee_placement: MeleePlacement,
    group_formation: GroupFormation,
    drag_formation: Arc<mint_sage_formation::FormationAssistant>,
    castle_siege_stand_back_distance: f32,
    victory_structure_filter: ObjectFilter,
    empty_counts: Vec<u8>,
    reachability: std::cell::RefCell<Reachability>,
    reachability_extra: std::cell::RefCell<Vec<(MoverClass, Reachability)>>,
    path_search: std::cell::RefCell<Pipeline>,
    attack_spot_search: std::cell::RefCell<mint_movement::AStar<mint_movement::Octile>>,
    unit_index: std::cell::RefCell<unit_index::UnitIndex>,
    victory_enabled: bool,
    ai_seeds: Vec<crate::skirmish_ai::AiSeed>,
    ai_timing: crate::combat::AiTiming,
    guard_ranges: crate::combat::GuardRanges,
    pub(crate) attack_priorities: std::sync::Arc<crate::combat::priority::AttackPriorityData>,
    ai_plan: Vec<(PlayerId, Vec<crate::skirmish_ai::AiTask>)>,
    ai_army_plans: Vec<crate::skirmish_ai::ArmyPlan>,
    attack_urgency_values: crate::combat::AttackUrgencyValues,
    route_markers: Vec<crate::skirmish_ai::RouteMarker>,
    player_sides: Vec<(PlayerId, String)>,
    player_starts: Vec<(PlayerId, Vec2)>,
    object_scripts: Option<mint_sage_modules::object_scripts::ObjectScripts>,
    alliances: crate::alliance::Alliances,
    map_players: Vec<String>,
    computer_sides: Vec<crate::economy::ComputerSide>,
    map_scripts: Option<map_scripts::MapScriptEngine>,
    map_script_starts: Vec<(u8, PlayerId)>,
    map_script_named: Vec<(String, PlayerId)>,
    script_local_player: Option<PlayerId>,
    base_check_frames: u32,
    handicaps: Vec<crate::handicap::Handicap>,
    command_point_factor: u32,
    upgrade_bits: Arc<mint_sage_modules::UpgradeBits>,
    passenger_perches: Arc<Vec<Vec<crate::perches::Perch>>>,
    grab_tosses: Vec<Option<mint_sage_modules::GrabToss>>,
    giant_birds: Vec<Option<mint_sage_modules::FollowThrough>>,
    bird_kinds: Vec<giant_bird::BirdKind>,
    spawn_points: Arc<Vec<Vec<crate::perches::Perch>>>,
    siege_docks: Arc<Vec<Vec<siege::DockBone>>>,
    siege_above_wall: Arc<Vec<Option<Vec2>>>,
    siege_ladder_exit: Arc<Vec<Option<crate::perches::Perch>>>,
    launch_bones: Arc<Vec<Vec<crate::perches::SlotLaunch>>>,
    wall_bounds: Arc<Vec<Option<crate::wall_meshes::WallBounds>>>,
    ramp_meshes: Arc<Vec<Vec<crate::wall_meshes::RampMesh>>>,
    raised_wall_mesh: Arc<Vec<bool>>,
    upgrade_buildables: Vec<crate::economy::UpgradeBuildable>,
    object_upgrade_menus: Vec<Vec<u16>>,
    player_upgrade_menus: Vec<Vec<u16>>,
    object_upgrade_menu_of: Vec<Option<u16>>,
    player_upgrade_menu_of: Vec<Option<u16>>,
    power_names: crate::powers::PowerNames,
    power_reload: Vec<(Option<LogicFrames>, bool)>,
    power_cast_rules: Vec<crate::powers::PowerCastRule>,
    upgrade_variant_menus: Vec<(crate::unit::Kind, String, Option<u16>, Option<u16>)>,
    modules: Option<ModuleWiring>,
    experience_tables: Arc<mint_sage_modules::ExperienceTables>,
    experience_scalars: Arc<mint_sage_modules::ExperienceScalarTables>,
    level_tables: crate::experience::LevelTables,
    armor_bonus_cap: f32,
    damage_thresholds: crate::combat::DamageThresholds,
    engaged_defaults: engaged::EngagedDefaults,
    menu_needs: Vec<Vec<crate::templates::Needs>>,
    object_upgrade_needs: Vec<Vec<crate::templates::Needs>>,
    player_upgrade_needs: Vec<Vec<crate::templates::Needs>>,
    hero_rosters: Vec<Vec<Option<crate::unit::Kind>>>,
    cah_plans: Vec<crate::create_a_hero::HeroPlan>,
    cah_can_build: Option<mint_sim::UpgradeBit>,
    cah_revival_discount: u32,
    hero_tables: Vec<(PlayerId, crate::unit::Kind, crate::experience::LevelTable)>,
    hero_buildables: Vec<Buildable>,
    sciences: Arc<crate::sciences::ScienceTable>,
    ranks: Arc<crate::sciences::Ranks>,
    spell_plans: Vec<crate::sciences::SpellPlan>,
    power_sciences: Vec<Vec<crate::sciences::ScienceId>>,
    ocls: Arc<mint_sage_ini::ObjectCreationListIndex>,
    lifetimes: crate::summons::Lifetimes,
    death_plans: crate::summons::DeathPlans,
    burning_deaths: death::BurningDeaths,
    contain_damage: contain::ContainDamageReactions,
    rampage_specs: rampage::RampageSpecs,
    flammable_specs: fire::FlammableSpecs,
    sell_percentage: f32,
    gravity: f32,
    ground_stiffness: f32,
    claim_cell_size: Option<f32>,
    victim_rules: weapon_fire::ResolvedVictims,
    fx_names: crate::fx::FxNames,
    weapon_fx: Vec<Option<crate::fx::FxId>>,
    weapon_pre_attack_fx: Vec<Option<crate::fx::FxId>>,
    body_kinds: Vec<BodyKind>,
    command_point_table: Option<crate::combat::CommandPointTable>,
    campaign_points: Option<crate::game_data::CommandPointsSolo>,
    multiplay: crate::combat::MultiplayMultipliers,
    score_weights: crate::game_data::ScoreWeights,
    objects_that_score: ObjectFilter,
    banners: Vec<Option<crate::horde::BannerPlan>>,
    projectile_kinds: Vec<Option<crate::combat::ProjectileKind>>,
    stealth: stealth::StealthTables,
    value_per_supply_box: f32,
}

impl core::fmt::Debug for RotwkRules {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RotwkRules")
            .field("spawns", &self.spawns.len())
            .field("extent", &self.passability.grid.extent())
            .finish_non_exhaustive()
    }
}

fn ai_kinds_of(kinds: &KindTable) -> Vec<bool> {
    kinds
        .iter()
        .map(|entry| {
            entry
                .record
                .declared_modules
                .iter()
                .any(|module| mint_sage_modules::is_ai_module(&module.name))
        })
        .collect()
}

impl RotwkRules {
    pub(crate) fn new(passability: Passability, spawns: Vec<Spawn>) -> Self {
        let empty_counts =
            vec![0u8; usize::try_from(passability.grid.extent().cell_count()).unwrap_or(0)];
        let reachability = Reachability::build(
            &mint_sage_passability::DynamicView {
                grid: &passability.grid,
                counts: &empty_counts,
                rubble: &[],
                wall_rim: &[],
                tags: &[],
                connect: &[],
                released: &[],
                closed: &[],
                obstacle_pinched: &[],
                ramp_side: &[],
                scaleable: &[],
                barred: &[],
            },
            GROUND,
        );
        Self {
            passability,
            reachability: std::cell::RefCell::new(reachability),
            reachability_extra: std::cell::RefCell::new(Vec::new()),
            path_search: std::cell::RefCell::new(Pipeline::new(PipelineConfig::default())),
            attack_spot_search: std::cell::RefCell::new(mint_movement::AStar::octile()),
            unit_index: std::cell::RefCell::default(),
            spawns,
            ids: None,
            combat: CombatData::new(),
            emotions: Arc::new(mint_sage_ini::EmotionIndex::new()),
            stances: Arc::new(mint_sage_ini::StanceTemplateIndex::new()),
            ai_power_buttons: Arc::new(mint_sage_ini::CommandButtonIndex::new()),
            command_sets: Arc::new(mint_sage_ini::CommandSetIndex::new()),
            ai_power_activation_tuning: [(1, 1); 4],
            combat_chains: crate::combat::CombatChainTable::default(),
            ai_power_difficulty: Vec::new(),
            difficulty: difficulty::DifficultySetup::default(),
            unarmoured: ArmorTemplate::default(),
            formations: Vec::new(),
            melee_hordes: Vec::new(),
            horde_rangefinders: Vec::new(),
            hordes: Vec::new(),
            buildables: Vec::new(),
            menus: Vec::new(),
            alignments: Vec::new(),
            players: 4,
            starting_cash: mint_sage_economy::DEFAULT_STARTING_CASH,
            starting_cash_overrides: Vec::new(),
            initial_upgrades: Vec::new(),
            production_cost_change: Vec::new(),
            use_shroud: false,
            structure_footprints: Vec::new(),
            wall_geometry: Vec::new(),
            map_placements: Vec::new(),
            script_object_names: Vec::new(),
            indestructible: Vec::new(),
            placed_keys: Vec::new(),
            script_teams: Vec::new(),
            placed_hordes: Vec::new(),
            forest_seeds: Vec::new(),
            tree_templates: crate::forest::TreeTemplates::default(),
            fire_grid: None,
            castle_members: Vec::new(),
            castle_layouts: Arc::default(),
            kinds: Arc::new(KindTable::default()),
            ai_kinds: Vec::new(),
            production: Arc::new(Production::default()),
            construct_menus: Vec::new(),
            construct_menu_of: Vec::new(),
            construction_cancel_refund_percent: 1.0,
            max_cells_adjust_horde_melee: 200,
            max_cells_adjust_destination: 400,
            max_cells_adjust_to_possible_destination: 400,
            max_tunnel_capacity: 5,
            melee_placement: MeleePlacement::default(),
            group_formation: GroupFormation::default(),
            drag_formation: Arc::new(mint_sage_formation::FormationAssistant::default()),
            castle_siege_stand_back_distance: 500.0,
            empty_counts,
            victory_structure_filter: ObjectFilter::default(),
            victory_enabled: false,
            ai_seeds: Vec::new(),
            ai_timing: crate::combat::AiTiming::default(),
            guard_ranges: crate::combat::GuardRanges::default(),
            attack_priorities: std::sync::Arc::default(),
            ai_plan: Vec::new(),
            ai_army_plans: Vec::new(),
            attack_urgency_values: crate::combat::AttackUrgencyValues::default(),
            route_markers: Vec::new(),
            player_sides: Vec::new(),
            player_starts: Vec::new(),
            object_scripts: None,
            alliances: crate::alliance::Alliances::default(),
            map_players: Vec::new(),
            computer_sides: Vec::new(),
            map_scripts: None,
            map_script_starts: Vec::new(),
            map_script_named: Vec::new(),
            script_local_player: None,
            base_check_frames: crate::game_data::DEFAULT_BASE_CHECK_FRAMES,
            handicaps: Vec::new(),
            command_point_factor: 100,
            upgrade_bits: Arc::new(mint_sage_modules::UpgradeBits::default()),
            passenger_perches: Arc::new(Vec::new()),
            grab_tosses: Vec::new(),
            giant_birds: Vec::new(),
            bird_kinds: Vec::new(),
            spawn_points: Arc::new(Vec::new()),
            siege_docks: Arc::new(Vec::new()),
            siege_above_wall: Arc::new(Vec::new()),
            siege_ladder_exit: Arc::new(Vec::new()),
            launch_bones: Arc::new(Vec::new()),
            wall_bounds: Arc::new(Vec::new()),
            ramp_meshes: Arc::new(Vec::new()),
            raised_wall_mesh: Arc::new(Vec::new()),
            upgrade_buildables: Vec::new(),
            object_upgrade_menus: Vec::new(),
            player_upgrade_menus: Vec::new(),
            object_upgrade_menu_of: Vec::new(),
            player_upgrade_menu_of: Vec::new(),
            power_names: crate::powers::PowerNames::default(),
            power_reload: Vec::new(),
            power_cast_rules: Vec::new(),
            upgrade_variant_menus: Vec::new(),
            modules: None,
            experience_tables: Arc::new(mint_sage_modules::ExperienceTables::default()),
            experience_scalars: Arc::new(mint_sage_modules::ExperienceScalarTables::default()),
            level_tables: crate::experience::LevelTables::default(),
            armor_bonus_cap: crate::combat::SHIPPED_ARMOR_BONUS_CAP,
            damage_thresholds: crate::combat::DamageThresholds::DEFAULT,
            engaged_defaults: engaged::EngagedDefaults::default(),
            menu_needs: Vec::new(),
            object_upgrade_needs: Vec::new(),
            player_upgrade_needs: Vec::new(),
            hero_rosters: Vec::new(),
            cah_plans: Vec::new(),
            cah_can_build: None,
            cah_revival_discount: 100,
            hero_tables: Vec::new(),
            hero_buildables: Vec::new(),
            sciences: Arc::new(crate::sciences::ScienceTable::default()),
            ranks: Arc::new(crate::sciences::Ranks::default()),
            spell_plans: Vec::new(),
            power_sciences: Vec::new(),
            ocls: Arc::new(mint_sage_ini::ObjectCreationListIndex::new()),
            lifetimes: crate::summons::Lifetimes::default(),
            death_plans: crate::summons::DeathPlans::default(),
            burning_deaths: death::BurningDeaths::default(),
            contain_damage: contain::ContainDamageReactions::default(),
            rampage_specs: rampage::RampageSpecs::default(),
            flammable_specs: fire::FlammableSpecs::default(),
            sell_percentage: crate::combat::SHIPPED_SELL_PERCENTAGE,
            gravity: crate::combat::DEFAULT_GRAVITY,
            ground_stiffness: crate::combat::DEFAULT_GROUND_STIFFNESS,
            claim_cell_size: None,
            victim_rules: weapon_fire::ResolvedVictims::default(),
            fx_names: crate::fx::FxNames::default(),
            weapon_fx: Vec::new(),
            weapon_pre_attack_fx: Vec::new(),
            body_kinds: Vec::new(),
            command_point_table: None,
            campaign_points: None,
            multiplay: crate::combat::MultiplayMultipliers::default(),
            score_weights: crate::game_data::ScoreWeights::default(),
            objects_that_score: ObjectFilter::default(),
            banners: Vec::new(),
            projectile_kinds: Vec::new(),
            stealth: stealth::StealthTables::default(),
            value_per_supply_box: crate::game_data::DEFAULT_VALUE_PER_SUPPLY_BOX,
        }
    }

    pub(crate) fn with_economy(mut self, buildables: Vec<Buildable>, starting_cash: i64) -> Self {
        self.menus = (0..buildables.len())
            .filter_map(|index| u16::try_from(index).ok().map(|index| vec![index]))
            .collect();
        self.buildables = buildables;
        self.starting_cash = starting_cash;
        self
    }

    pub(crate) fn with_menus(mut self, menus: Vec<Vec<u16>>) -> Self {
        self.menus = menus;
        self
    }

    pub(crate) fn with_menu_needs(mut self, needs: Vec<Vec<crate::templates::Needs>>) -> Self {
        self.menu_needs = needs;
        self
    }

    pub(crate) fn with_create_a_hero(
        mut self,
        plans: Vec<crate::create_a_hero::HeroPlan>,
        can_build: Option<mint_sim::UpgradeBit>,
        revival_discount: u32,
    ) -> Self {
        self.cah_plans = plans;
        self.cah_can_build = can_build;
        self.cah_revival_discount = revival_discount;
        self
    }

    pub(crate) fn with_heroes(
        mut self,
        rosters: Vec<Vec<Option<crate::unit::Kind>>>,
        buildables: Vec<Buildable>,
    ) -> Self {
        self.hero_rosters = rosters;
        self.hero_buildables = buildables;
        self
    }

    pub(crate) fn with_alignments(mut self, alignments: Vec<mint_sage_economy::Alignment>) -> Self {
        self.alignments = alignments;
        self
    }

    pub(crate) fn with_starting_cash_overrides(mut self, overrides: Vec<Option<i64>>) -> Self {
        self.starting_cash_overrides = overrides;
        self
    }

    pub(crate) fn with_initial_upgrades(mut self, upgrades: Vec<UpgradeMask>) -> Self {
        self.initial_upgrades = upgrades;
        self
    }

    pub(crate) fn with_production_cost_change(mut self, percents: Vec<f32>) -> Self {
        self.production_cost_change = percents;
        self
    }

    pub(crate) fn production_cost_change_percent(&self, player: PlayerId) -> f32 {
        self.production_cost_change
            .get(usize::from(player.get()))
            .copied()
            .unwrap_or(0.0)
    }

    fn point_rule_for(
        &self,
        alignment: mint_sage_economy::Alignment,
        human: bool,
    ) -> crate::spell_state::PointRule {
        let evil = alignment == mint_sage_economy::Alignment::Evil;
        if let Some(solo) = self.campaign_points {
            let pair = match (human, evil) {
                (true, false) => solo.good,
                (true, true) => solo.evil,
                (false, false) => solo.good_ai,
                (false, true) => solo.evil_ai,
            };
            if let Some((start, cap)) = pair {
                return crate::spell_state::PointRule {
                    start,
                    cap: Some(cap),
                    factor: 100,
                    overridden: false,
                };
            }
        }
        match self
            .command_point_table
            .and_then(|table| table.of(evil, self.players))
        {
            Some((start, cap)) => crate::spell_state::PointRule {
                start,
                cap: Some(cap),
                factor: self.command_point_factor,
                overridden: false,
            },
            None => crate::spell_state::PointRule {
                start: alignment.command_point_limit(),
                cap: None,
                factor: self.command_point_factor,
                overridden: false,
            },
        }
    }

    fn starting_cash_for(&self, slot: u8) -> i64 {
        self.starting_cash_overrides
            .get(usize::from(slot))
            .copied()
            .flatten()
            .unwrap_or(self.starting_cash)
    }

    pub(crate) const fn with_shroud(mut self, use_shroud: bool) -> Self {
        self.use_shroud = use_shroud;
        self
    }

    pub(crate) fn with_hordes(
        mut self,
        formations: Vec<FormationTable>,
        hordes: Vec<HordeSpawn>,
    ) -> Self {
        self.formations = formations;
        self.hordes = hordes;
        self
    }

    pub(crate) fn with_melee_hordes(mut self, melee_hordes: Vec<Option<f32>>) -> Self {
        self.melee_hordes = melee_hordes;
        self
    }

    pub(crate) fn with_horde_rangefinders(mut self, rangefinders: Vec<Option<f32>>) -> Self {
        self.horde_rangefinders = rangefinders;
        self
    }

    pub(crate) fn with_combat(mut self, combat: CombatData) -> Self {
        self.combat = combat;
        self
    }

    pub(crate) fn with_emotions(mut self, emotions: Arc<mint_sage_ini::EmotionIndex>) -> Self {
        self.emotions = emotions;
        self
    }

    pub(crate) fn with_stances(mut self, stances: Arc<mint_sage_ini::StanceTemplateIndex>) -> Self {
        self.stances = stances;
        self
    }

    pub(crate) fn with_ai_power_buttons(
        mut self,
        buttons: Arc<mint_sage_ini::CommandButtonIndex>,
    ) -> Self {
        self.ai_power_buttons = buttons;
        self
    }

    pub(crate) fn with_command_sets(mut self, sets: Arc<mint_sage_ini::CommandSetIndex>) -> Self {
        self.command_sets = sets;
        self
    }

    pub(crate) fn with_difficulty(mut self, difficulty: difficulty::DifficultySetup) -> Self {
        self.difficulty = difficulty;
        self
    }

    pub(crate) fn with_ai_power_tuning(
        mut self,
        activation_tuning: [(i64, i64); 4],
        difficulty: Vec<(PlayerId, crate::lobby::Difficulty)>,
    ) -> Self {
        self.ai_power_activation_tuning = activation_tuning;
        self.ai_power_difficulty = difficulty;
        self
    }

    pub(crate) fn with_combat_chains(mut self, chains: crate::combat::CombatChainTable) -> Self {
        self.combat_chains = chains;
        self
    }

    pub(crate) fn with_gravity(mut self, per_second_squared: Option<f32>) -> Self {
        if let Some(gravity) = per_second_squared {
            let hz = f32::from(u16::try_from(mint_sim::SIMULATION_HZ).unwrap_or(30));
            self.gravity = gravity / (hz * hz);
        }
        self
    }

    pub(crate) fn with_ground_stiffness(mut self, stiffness: f32) -> Self {
        self.ground_stiffness = stiffness.clamp(0.01, 0.99);
        self
    }

    pub(crate) const fn with_claim_cell_size(mut self, cell_size: Option<f32>) -> Self {
        self.claim_cell_size = cell_size;
        self
    }

    pub(crate) fn with_attack_priorities(
        mut self,
        data: std::sync::Arc<crate::combat::priority::AttackPriorityData>,
    ) -> Self {
        self.attack_priorities = data;
        self
    }

    pub(crate) const fn with_guard_ranges(mut self, ranges: crate::combat::GuardRanges) -> Self {
        self.guard_ranges = ranges;
        self
    }

    pub(crate) fn with_structure_footprints(
        mut self,
        footprints: Vec<(usize, Vec<mint_sage_passability::FootprintRecord>)>,
    ) -> Self {
        self.structure_footprints = footprints;
        self
    }

    pub(crate) fn with_wall_geometry(
        mut self,
        mut walls: Vec<mint_sage_passability::WallGeometry>,
    ) -> Self {
        walls.sort_by_key(|wall| wall.id);
        self.wall_geometry = walls;
        self
    }

    pub(crate) fn wall_geometry_at(&self, id: u32) -> Option<mint_sage_passability::WallGeometry> {
        let at = self
            .wall_geometry
            .binary_search_by_key(&id, |wall| wall.id)
            .ok()?;
        self.wall_geometry.get(at).copied()
    }

    pub(crate) fn with_map_placements(mut self, placements: Vec<(usize, u32)>) -> Self {
        self.map_placements = placements;
        self
    }

    pub(crate) fn with_script_teams(mut self, teams: Vec<(usize, String)>) -> Self {
        self.script_teams = teams;
        self
    }

    pub(crate) fn with_script_object_names(mut self, names: Vec<(usize, String)>) -> Self {
        self.script_object_names = names;
        self
    }

    pub(crate) fn with_indestructible(mut self, spawns: Vec<usize>) -> Self {
        self.indestructible = spawns;
        self
    }

    pub(crate) fn with_placed_keys(
        mut self,
        keys: Vec<(usize, crate::map_keys::ObjectKeys)>,
    ) -> Self {
        self.placed_keys = keys;
        self
    }

    pub(crate) fn with_placed_hordes(mut self, hordes: Vec<PlacedHorde>) -> Self {
        self.placed_hordes = hordes;
        self
    }

    pub(crate) fn map_placements(&self, world: &World) -> Vec<(Entity, u32)> {
        let Some(ids) = self.ids else {
            return Vec::new();
        };
        world
            .store(ids.map_placement)
            .map_or_else(Vec::new, |store| {
                store.iter().map(|(entity, at)| (entity, at.0)).collect()
            })
    }

    pub(crate) fn with_forest_seeds(mut self, seeds: Vec<crate::forest::ForestTreeInit>) -> Self {
        self.forest_seeds = seeds;
        self
    }

    pub(crate) fn with_tree_templates(mut self, templates: crate::forest::TreeTemplates) -> Self {
        self.tree_templates = templates;
        self
    }

    pub(crate) fn with_fire_grid(mut self, grid: terrain_fire::FireGrid) -> Self {
        self.fire_grid = Some(grid);
        self
    }

    pub(crate) fn with_castles(
        mut self,
        members: Vec<(usize, usize)>,
        layouts: Arc<crate::castle::CastleLayouts>,
    ) -> Self {
        self.castle_members = members;
        self.castle_layouts = layouts;
        self
    }

    pub(crate) fn with_construction(
        mut self,
        kinds: Arc<KindTable>,
        production: Production,
    ) -> Self {
        let menus = kinds.construct_menus();
        self.construct_menus = menus.menus;
        self.construct_menu_of = menus.menu_of;
        self.production = Arc::new(production);
        for entry in kinds.iter() {
            let Some(crush) = &entry.record.aod_crush else {
                continue;
            };
            for name in [
                crush.damage_type.as_deref(),
                crush.special_damage_type.as_deref(),
                crush.self_damage_type.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                self.combat.damage_types.intern(name);
            }
            for name in [
                crush.death_type.as_deref(),
                crush.special_death_type.as_deref(),
                crush.self_death_type.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                self.combat.death_types.intern(name);
            }
        }
        self.ai_kinds = ai_kinds_of(&kinds);
        self.kinds = kinds;
        self
    }

    pub(crate) const fn with_cancel_refund_percent(mut self, percent: f32) -> Self {
        self.construction_cancel_refund_percent = percent;
        self
    }

    pub(crate) const fn with_max_cells_adjust_horde_melee(mut self, cells: u32) -> Self {
        self.max_cells_adjust_horde_melee = cells;
        self
    }

    pub(crate) const fn with_melee_placement(mut self, placement: MeleePlacement) -> Self {
        self.melee_placement = placement;
        self
    }

    pub(crate) const fn with_group_formation(mut self, formation: GroupFormation) -> Self {
        self.group_formation = formation;
        self
    }

    pub(crate) fn with_drag_formation(
        mut self,
        assistant: Arc<mint_sage_formation::FormationAssistant>,
    ) -> Self {
        self.drag_formation = assistant;
        self
    }

    pub(crate) const fn with_castle_siege_stand_back_distance(mut self, distance: f32) -> Self {
        self.castle_siege_stand_back_distance = distance;
        self
    }

    pub(crate) const fn with_max_cells_adjust_destination(mut self, cells: u32) -> Self {
        self.max_cells_adjust_destination = cells;
        self
    }

    pub(crate) const fn with_max_cells_adjust_to_possible_destination(
        mut self,
        cells: u32,
    ) -> Self {
        self.max_cells_adjust_to_possible_destination = cells;
        self
    }

    pub(crate) const fn with_max_tunnel_capacity(mut self, capacity: u32) -> Self {
        self.max_tunnel_capacity = capacity;
        self
    }

    pub(crate) fn with_victory(mut self, kinds: Arc<KindTable>, filter: ObjectFilter) -> Self {
        self.ai_kinds = ai_kinds_of(&kinds);
        self.kinds = kinds;
        self.victory_structure_filter = filter;
        self.victory_enabled = true;
        self
    }

    pub(crate) fn player_defeated(&self, world: &World, ids: Ids, player: PlayerId) -> bool {
        self.victory_enabled
            && self.players >= 2
            && player.get() < self.players
            && crate::defeat::has_been_defeated(
                world,
                Self::defeat_ids(ids),
                &self.kinds,
                &self.victory_structure_filter,
                player,
            )
    }

    pub(crate) fn with_passenger_perches(
        mut self,
        perches: Vec<Vec<crate::perches::Perch>>,
    ) -> Self {
        self.passenger_perches = Arc::new(perches);
        self
    }

    pub(crate) fn with_spawn_points(mut self, points: Vec<Vec<crate::perches::Perch>>) -> Self {
        self.spawn_points = Arc::new(points);
        self
    }

    pub(crate) fn with_siege_bones(
        mut self,
        docks: Vec<Vec<siege::DockBone>>,
        above_wall: Vec<Option<Vec2>>,
        ladder_exit: Vec<Option<crate::perches::Perch>>,
    ) -> Self {
        self.siege_docks = Arc::new(docks);
        self.siege_above_wall = Arc::new(above_wall);
        self.siege_ladder_exit = Arc::new(ladder_exit);
        self
    }

    pub(crate) fn with_launch_bones(
        mut self,
        bones: Vec<Vec<impl Into<crate::perches::SlotLaunch>>>,
    ) -> Self {
        self.launch_bones = Arc::new(
            bones
                .into_iter()
                .map(|slots| slots.into_iter().map(Into::into).collect())
                .collect(),
        );
        self
    }

    pub(crate) fn with_wall_bounds(
        mut self,
        bounds: Vec<Option<crate::wall_meshes::WallBounds>>,
    ) -> Self {
        self.wall_bounds = Arc::new(bounds);
        self
    }

    pub(crate) fn wall_bounds_of(
        &self,
        kind: crate::unit::Kind,
    ) -> Option<&crate::wall_meshes::WallBounds> {
        self.wall_bounds
            .get(usize::from(kind.0))
            .and_then(Option::as_ref)
    }

    pub(crate) fn with_ramp_meshes(
        mut self,
        ramps: Vec<Vec<crate::wall_meshes::RampMesh>>,
    ) -> Self {
        self.ramp_meshes = Arc::new(ramps);
        self
    }

    pub(crate) fn ramp_meshes_of(
        &self,
        kind: crate::unit::Kind,
    ) -> &[crate::wall_meshes::RampMesh] {
        self.ramp_meshes
            .get(usize::from(kind.0))
            .map_or(&[], Vec::as_slice)
    }

    pub(crate) fn with_raised_wall_meshes(mut self, has_raised_wall_mesh: Vec<bool>) -> Self {
        self.raised_wall_mesh = Arc::new(has_raised_wall_mesh);
        self
    }

    pub(crate) fn has_raised_wall_mesh_of(&self, kind: crate::unit::Kind) -> bool {
        self.raised_wall_mesh
            .get(usize::from(kind.0))
            .copied()
            .unwrap_or(false)
    }

    pub(crate) fn is_walk_on_top_of_wall(&self, kind: crate::unit::Kind) -> bool {
        self.kinds.get(kind).is_some_and(|entry| {
            entry
                .record
                .kind_of
                .iter()
                .any(|flag| flag.eq_ignore_ascii_case("WALK_ON_TOP_OF_WALL"))
        })
    }

    pub(crate) fn scaleable_wall_geometry_of(
        &self,
        kind: crate::unit::Kind,
    ) -> Option<(f32, f32, f32)> {
        let entry = self.kinds.get(kind)?;
        entry
            .record
            .kind_of
            .iter()
            .any(|flag| flag.eq_ignore_ascii_case("SCALEABLE_WALL"))
            .then(|| {
                (
                    entry.record.geometry.major_radius.unwrap_or(0.0),
                    entry.record.geometry.minor_radius.unwrap_or(0.0),
                    entry.record.geometry.height.unwrap_or(0.0),
                )
            })
    }

    pub(crate) fn with_skirmish_ai(
        mut self,
        seeds: Vec<crate::skirmish_ai::AiSeed>,
        timing: crate::combat::AiTiming,
        army_plans: Vec<crate::skirmish_ai::ArmyPlan>,
    ) -> Self {
        self.ai_seeds = seeds;
        self.ai_timing = timing;
        self.ai_army_plans = army_plans;
        self
    }

    pub(crate) fn with_attack_ai(
        mut self,
        values: crate::combat::AttackUrgencyValues,
        route_markers: Vec<crate::skirmish_ai::RouteMarker>,
        player_sides: Vec<(PlayerId, String)>,
    ) -> Self {
        self.attack_urgency_values = values;
        self.route_markers = route_markers;
        self.player_sides = player_sides;
        self
    }

    pub(crate) fn with_player_starts(mut self, starts: Vec<(PlayerId, Vec2)>) -> Self {
        self.player_starts = starts;
        self
    }

    pub(crate) fn with_object_scripts(
        mut self,
        scripts: Option<mint_sage_modules::object_scripts::ObjectScripts>,
    ) -> Self {
        self.object_scripts = scripts;
        self
    }

    pub(crate) fn with_players(mut self, players: u8) -> Self {
        self.players = players.clamp(1, crate::shroud::SHROUD_PLAYERS_U8);
        self
    }

    pub(crate) fn with_teams(mut self, teams: Vec<Option<u8>>) -> Self {
        self.alliances = crate::alliance::Alliances::new(teams);
        self
    }

    pub(crate) fn with_map_players(
        mut self,
        first: u8,
        names: Vec<String>,
        relations: Vec<crate::alliance::MapRelations>,
    ) -> Self {
        self.alliances = self.alliances.with_map_players(first, relations);
        self.map_players = names;
        self
    }

    pub(crate) fn with_computer_sides(mut self, sides: Vec<crate::economy::ComputerSide>) -> Self {
        self.computer_sides = sides;
        self
    }

    pub(crate) fn with_contestant_sides(
        mut self,
        sides: Vec<(PlayerId, crate::alliance::MapRelations)>,
    ) -> Self {
        self.alliances = self.alliances.with_contestant_sides(sides);
        self
    }

    pub(crate) fn with_map_scripts(
        mut self,
        engine: Option<map_scripts::MapScriptEngine>,
        starts: Vec<(u8, PlayerId)>,
    ) -> Self {
        self.map_scripts = engine;
        self.map_script_starts = starts;
        self
    }

    pub(crate) const fn with_base_check_frames(mut self, frames: u32) -> Self {
        self.base_check_frames = frames;
        self
    }

    pub(crate) fn with_script_named_sides(mut self, named: Vec<(String, PlayerId)>) -> Self {
        self.map_script_named = named;
        self
    }

    pub(crate) fn with_script_local_player(mut self, player: Option<PlayerId>) -> Self {
        self.script_local_player = player;
        self
    }

    pub(crate) fn map_player_name(&self, player: PlayerId) -> Option<&str> {
        let offset = player.get().checked_sub(self.players)?;
        self.map_players
            .get(usize::from(offset))
            .map(String::as_str)
    }

    pub(crate) fn with_handicaps(mut self, handicaps: Vec<crate::handicap::Handicap>) -> Self {
        self.handicaps = handicaps;
        self
    }

    pub(crate) fn with_command_point_factor(mut self, percent: u32) -> Self {
        self.command_point_factor = percent;
        self
    }

    pub(crate) fn with_campaign_command_points(
        mut self,
        solo: Option<crate::game_data::CommandPointsSolo>,
    ) -> Self {
        self.campaign_points = solo;
        self
    }

    pub(crate) fn with_command_point_table(
        mut self,
        table: Option<crate::combat::CommandPointTable>,
    ) -> Self {
        self.command_point_table = table;
        self
    }

    pub(crate) fn with_multiplay_multipliers(
        mut self,
        multiplay: crate::combat::MultiplayMultipliers,
    ) -> Self {
        self.multiplay = multiplay;
        self
    }

    pub(crate) fn with_objects_that_score(mut self, filter: ObjectFilter) -> Self {
        self.objects_that_score = filter;
        self
    }

    pub(crate) fn with_score_weights(mut self, weights: crate::game_data::ScoreWeights) -> Self {
        self.score_weights = weights;
        self
    }

    pub(crate) fn with_banners(mut self, banners: Vec<Option<crate::horde::BannerPlan>>) -> Self {
        self.banners = banners;
        self
    }

    pub(crate) fn allied(&self, a: PlayerId, b: PlayerId) -> bool {
        self.alliances.allied(a, b)
    }

    pub(crate) fn player_sides(&self) -> &[(PlayerId, String)] {
        &self.player_sides
    }

    pub(crate) fn hostile(&self, a: PlayerId, b: PlayerId) -> bool {
        self.alliances.hostile(a, b)
    }

    pub(crate) fn command_source(&self, player: PlayerId) -> mint_sage_combat::CommandSource {
        if self.ai_seeds.iter().any(|seed| seed.player == player) {
            mint_sage_combat::CommandSource::Ai
        } else {
            mint_sage_combat::CommandSource::Player
        }
    }

    fn handicap_of(&self, player: Option<PlayerId>) -> crate::handicap::Handicap {
        player
            .and_then(|p| self.handicaps.get(usize::from(p.get())))
            .copied()
            .unwrap_or_default()
    }

    pub(crate) fn with_upgrades(
        mut self,
        menus: crate::templates::UpgradeMenus,
        bits: Arc<mint_sage_modules::UpgradeBits>,
    ) -> Self {
        self.upgrade_buildables = menus.upgrades;
        self.object_upgrade_menus = menus.object_menus;
        self.player_upgrade_menus = menus.player_menus;
        self.object_upgrade_needs = menus.object_needs;
        self.player_upgrade_needs = menus.player_needs;
        self.object_upgrade_menu_of = menus.object_menu_of;
        self.player_upgrade_menu_of = menus.player_menu_of;
        self.upgrade_variant_menus = menus.variant_menus;
        self.upgrade_bits = bits;
        self
    }

    pub(crate) fn with_powers(
        mut self,
        names: crate::powers::PowerNames,
        reload: Vec<(Option<LogicFrames>, bool)>,
    ) -> Self {
        self.power_names = names;
        self.power_reload = reload;
        self
    }

    pub(crate) fn with_power_cast_rules(
        mut self,
        rules: Vec<crate::powers::PowerCastRule>,
    ) -> Self {
        self.power_cast_rules = rules;
        self
    }

    pub(crate) fn with_experience(
        mut self,
        tables: Arc<mint_sage_modules::ExperienceTables>,
        armor_bonus_cap: f32,
    ) -> Self {
        self.experience_tables = tables;
        self.armor_bonus_cap = armor_bonus_cap;
        self
    }

    pub(crate) fn with_experience_scalars(
        mut self,
        tables: Arc<mint_sage_modules::ExperienceScalarTables>,
    ) -> Self {
        self.experience_scalars = tables;
        self
    }

    pub(crate) const fn with_engaged_defaults(mut self, defaults: EngagedDefaults) -> Self {
        self.engaged_defaults = defaults;
        self
    }

    pub(crate) const fn with_damage_thresholds(
        mut self,
        thresholds: crate::combat::DamageThresholds,
    ) -> Self {
        self.damage_thresholds = thresholds;
        self
    }

    pub(crate) fn geometry_kind(
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> Option<crate::unit::Kind> {
        world
            .get(ids.geometry_from, entity)
            .map(|from| from.0)
            .or_else(|| world.get(ids.kind, entity).copied())
    }

    pub(crate) const fn ids(&self) -> Option<Ids> {
        self.ids
    }

    pub(crate) fn attach_view<'a>(
        &self,
        world: &'a mint_sim::World,
        entity: Entity,
    ) -> Option<&'a mint_sage_modules::AttachUpdate> {
        self.modules.as_ref()?.attach_view(world, entity)
    }

    fn with_ui_facts(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        apply: impl FnOnce(&mut map_scripts::UiFacts),
    ) {
        if let Some((_, state)) = tick
            .state_mut()
            .store_mut(ids.map_script_state)
            .and_then(|store| store.iter_mut().next())
        {
            apply(&mut state.ui);
        }
    }

    pub(crate) fn script_bloom(&self, world: &mint_sim::World) -> Option<bool> {
        let ids = self.ids()?;
        world
            .store(ids.map_script_state)?
            .iter()
            .find_map(|(_, state)| state.bloom)
    }

    pub(crate) fn script_cine<'a>(
        &self,
        world: &'a mint_sim::World,
    ) -> Option<&'a map_scripts::CineState> {
        let ids = self.ids()?;
        world
            .store(ids.map_script_state)?
            .iter()
            .next()
            .map(|(_, state)| &state.cine)
    }

    pub(crate) fn script_audio<'a>(
        &self,
        world: &'a mint_sim::World,
    ) -> Option<&'a [map_scripts::AudioCmd]> {
        let ids = self.ids()?;
        world
            .store(ids.map_script_state)?
            .iter()
            .next()
            .map(|(_, state)| state.audio.pending.as_slice())
    }

    pub(crate) fn script_camera_ops<'a>(
        &self,
        world: &'a mint_sim::World,
    ) -> Option<&'a map_scripts::CameraOps> {
        let ids = self.ids()?;
        world
            .store(ids.map_script_state)?
            .iter()
            .next()
            .map(|(_, state)| &state.camera_ops)
    }

    pub(crate) fn entity_follow_point(
        &self,
        world: &mint_sim::World,
        entity: Entity,
    ) -> Option<[f32; 3]> {
        let ids = self.ids()?;
        let at = world.get(ids.body, entity)?.0.position;
        let lift = world.get(ids.elevation, entity).map_or(0.0, |e| e.0);
        Some([at.x, at.y, self.ground_height(at.x, at.y) + lift])
    }

    pub(crate) fn script_view(&self, world: &mint_sim::World) -> map_scripts::ScriptView {
        let Some(ids) = self.ids() else {
            return map_scripts::ScriptView::default();
        };
        world
            .store(ids.map_script_state)
            .and_then(|store| store.iter().next())
            .map(|(_, state)| state.view())
            .unwrap_or_default()
    }

    pub(crate) fn collapse_view(
        &self,
        world: &mint_sim::World,
        entity: Entity,
    ) -> Option<(mint_sage_modules::CollapseState, f32, f32)> {
        self.modules.as_ref()?.collapse_view(world, entity)
    }

    pub(crate) fn tdfx_view<'a>(
        &self,
        world: &'a mint_sim::World,
        entity: Entity,
    ) -> Option<&'a mint_sage_modules::TransitionDamageFX> {
        self.modules.as_ref()?.tdfx_view(world, entity)
    }

    pub(crate) fn mount_opacity(&self, world: &mint_sim::World, entity: Entity) -> f32 {
        self.modules
            .as_ref()
            .map_or(1.0, |modules| modules.mount_opacity(world, entity))
    }

    pub(crate) fn disguise_opacity(&self, world: &mint_sim::World, entity: Entity) -> f32 {
        self.modules
            .as_ref()
            .map_or(1.0, |modules| modules.disguise_opacity(world, entity))
    }

    pub(crate) fn art_kind(
        &self,
        world: &mint_sim::World,
        viewer: PlayerId,
        entity: Entity,
    ) -> Option<crate::unit::Kind> {
        let ids = self.ids?;
        let true_kind = world.get(ids.kind, entity).copied();
        let owner = world
            .get(ids.owner, entity)
            .map_or(PlayerId::new(0), |o| o.0);
        if viewer == owner || !self.is_disguised(world, entity) {
            return true_kind;
        }
        self.disguise_as_template(world, entity)
            .and_then(|name| self.kinds.kind(name))
            .or(true_kind)
    }

    pub(crate) fn told_kind(
        &self,
        world: &mint_sim::World,
        viewer: PlayerId,
        entity: Entity,
    ) -> Option<crate::unit::Kind> {
        let ids = self.ids?;
        let true_kind = world.get(ids.kind, entity).copied();
        if !self.is_disguised(world, entity) {
            return true_kind;
        }
        if self.view_relation(world, viewer, entity).victim()
            != mint_sage_combat::victim::Relation::Enemies
        {
            return true_kind;
        }
        self.disguise_enemy_perspective_template(world, entity)
            .and_then(|name| self.kinds.kind(name))
            .or(true_kind)
    }

    pub(crate) fn hover_kind(
        &self,
        world: &mint_sim::World,
        viewer: PlayerId,
        entity: Entity,
    ) -> Option<crate::unit::Kind> {
        let ids = self.ids?;
        let true_kind = world.get(ids.kind, entity).copied();
        if !self.is_disguised(world, entity) {
            return true_kind;
        }
        if self.view_relation(world, viewer, entity).victim()
            == mint_sage_combat::victim::Relation::Allies
        {
            return true_kind;
        }
        self.disguise_as_template(world, entity)
            .and_then(|name| self.kinds.kind(name))
            .or(true_kind)
    }

    fn is_disguised(&self, world: &mint_sim::World, entity: Entity) -> bool {
        self.modules
            .as_ref()
            .is_some_and(|modules| modules.is_disguised(world, entity))
    }

    fn disguise_as_template<'a>(
        &self,
        world: &'a mint_sim::World,
        entity: Entity,
    ) -> Option<&'a str> {
        self.modules.as_ref()?.disguise_as_template(world, entity)
    }

    fn disguise_enemy_perspective_template<'a>(
        &self,
        world: &'a mint_sim::World,
        entity: Entity,
    ) -> Option<&'a str> {
        self.modules
            .as_ref()?
            .disguise_enemy_perspective_template(world, entity)
    }

    pub(crate) fn corpse_fade_opacity(
        &self,
        world: &mint_sim::World,
        entity: Entity,
        now: Frame,
    ) -> f32 {
        let Some(ids) = self.ids else {
            return 1.0;
        };
        world
            .get(ids.corpse_fade, entity)
            .map_or(1.0, |fade| fade.opacity(now))
    }

    pub(crate) fn join_fade_opacity(
        &self,
        world: &mint_sim::World,
        entity: Entity,
        now: Frame,
    ) -> f32 {
        let Some(ids) = self.ids else {
            return 1.0;
        };
        world
            .get(ids.join_fade, entity)
            .map_or(1.0, |fade| fade.opacity(now))
    }

    pub(crate) const fn player_count(&self) -> u8 {
        self.players
    }

    pub(crate) fn ground_height(&self, x: f32, y: f32) -> f32 {
        self.passability.heights.height_at(x, y).unwrap_or(0.0)
    }

    pub(crate) const fn passability(&self) -> &Passability {
        &self.passability
    }

    pub(crate) fn buildable_catalogue(&self) -> Vec<RosterEntry> {
        self.buildables
            .iter()
            .enumerate()
            .filter_map(|(index, buildable)| {
                let entry = self.kinds.get(buildable.kind)?;
                let index = u16::try_from(index).ok()?;
                let (is_horde, member_count) = match &buildable.output {
                    crate::economy::Output::Unit(_) => (false, 1),
                    crate::economy::Output::Horde { count, .. } => (true, *count),
                };
                Some(RosterEntry {
                    index,
                    label: buildable.label.clone(),
                    is_horde,
                    member_count,
                    side: entry.record.side.clone(),
                    kind_of: entry.record.kind_of.clone(),
                })
            })
            .collect()
    }

    pub(crate) fn structure_catalogue(&self) -> Vec<StructureEntry> {
        self.kinds
            .iter()
            .enumerate()
            .filter_map(|(at, entry)| {
                if !entry.record.has_kind("STRUCTURE") {
                    return None;
                }
                let kind = crate::unit::Kind(u16::try_from(at).ok()?);
                Some(StructureEntry {
                    kind,
                    label: entry.record.name.clone(),
                    side: entry.record.side.clone(),
                })
            })
            .collect()
    }

    pub(crate) fn structure_footprint_reach(&self, kind: crate::unit::Kind) -> f32 {
        let Some(entry) = self.kinds.get(kind) else {
            return 0.0;
        };
        let footprint = footprint_of(&entry.record);
        if footprint.is_empty() {
            return 0.0;
        }
        Obstacle {
            id: 0,
            centre: Vec2::ZERO,
            angle: 0.0,
            records: footprint,
        }
        .reach()
    }

    pub(crate) fn unit_spawn_reach(&self, kind: crate::unit::Kind) -> Option<(f32, f32)> {
        let entry = self.kinds.get(kind)?;
        let body = |kind| {
            self.kinds
                .spawn(
                    &self.combat,
                    kind,
                    Vec2::ZERO,
                    mint_movement::Facing::EAST,
                    PlayerId::new(0),
                )
                .map_or(0.0, |spawn| spawn.shape.radius)
        };
        let (Some(formation), Some(member)) = (entry.formation, entry.member) else {
            let radius = body(kind);
            return Some((radius, radius));
        };
        let table = self.formations.get(usize::from(formation))?;
        let (jitter_x, jitter_y) = table.jitter();
        let radius = body(member);
        let (across, along) = table.slots().iter().fold((0.0f32, 0.0f32), |(y, x), slot| {
            (y.max(slot.offset.y.abs()), x.max(slot.offset.x.abs()))
        });
        Some((across + jitter_y + radius, along + jitter_x + radius))
    }

    pub(crate) fn projectile_template(&self, weapon: WeaponId, nugget: u16) -> Option<&str> {
        match self
            .combat
            .weapons
            .get(weapon)?
            .nuggets
            .get(usize::from(nugget))?
        {
            mint_sage_combat::Nugget::Projectile(projectile) => {
                projectile.projectile_template.as_deref()
            }
            _ => None,
        }
    }

    pub(crate) fn weapon_cycle_seconds(
        &self,
        weapon: WeaponId,
        mut state: WeaponState,
        moved: bool,
        now: Frame,
        conditions: mint_sage_combat::BonusConditions,
    ) -> f32 {
        let Some(template) = self.combat.weapons.get(weapon) else {
            return 0.0;
        };
        let pre_attack =
            mint_sage_combat::compute_bonus(&self.combat.weapon_bonus, &template.bonus, conditions)
                .pre_attack();
        let frames: u64 = if state.status(now) == WeaponStatus::ReloadingClip {
            state
                .when_we_can_fire_again()
                .get()
                .saturating_sub(state.when_last_reload_started().get())
        } else {
            u64::from(
                template
                    .fit_wind_up(&state, moved, pre_attack)
                    .saturating_add(template.firing_duration)
                    .get(),
            )
        };
        #[allow(clippy::cast_precision_loss)]
        let seconds =
            frames as f32 / f32::from(u16::try_from(mint_sim::SIMULATION_HZ).unwrap_or(30));
        seconds
    }

    pub(crate) fn weapon_firing_duration(&self, weapon: WeaponId) -> mint_sim::LogicFrames {
        self.combat
            .weapons
            .get(weapon)
            .map_or(mint_sim::LogicFrames::ZERO, |template| {
                template.firing_duration
            })
    }

    pub(crate) fn projectile_templates(&self) -> Vec<String> {
        crate::combat::projectile_template_names(&self.combat)
    }

    fn passable_cell(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        point: mint_movement::Vec2,
    ) -> Option<mint_movement::Cell> {
        self.passable_cell_for(tick, ids, point, GROUND)
    }

    fn passable_cell_for(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        point: mint_movement::Vec2,
        mover: mint_movement::MoverClass,
    ) -> Option<mint_movement::Cell> {
        let cell = self.passability.cell_at(point)?;
        let view = self.dynamic_view(tick, ids);
        mint_sage_passability::nearest_passable_over(&view, cell, mover)
    }

    fn start_cell_toward(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        from: mint_movement::Vec2,
        toward: mint_movement::Vec2,
        mover: mint_movement::MoverClass,
    ) -> Option<mint_movement::Cell> {
        let origin = map_entry(self.active_high(tick.state(), ids), from, toward).unwrap_or(from);
        self.passable_cell_for(tick, ids, origin, mover)
    }

    fn both_off_extent(&self, from: Vec2, to: Vec2) -> bool {
        let high = self.passability.playable_high;
        let inside = |p: Vec2| p.x > 0.0 && p.y > 0.0 && p.x < high.x && p.y < high.y;
        !inside(from) && !inside(to)
    }

    fn give_off_map_route(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        (from, to): (Vec2, Vec2),
        kind: PathKind,
    ) -> Result<bool> {
        if !self.both_off_extent(from, to) {
            return Ok(false);
        }
        crate::unit::give_path(
            tick.state_mut(),
            (ids.body, ids.path),
            entity,
            Path {
                route: Route::to_point(to),
                goal: to,
                kind,
                facing: None,
            },
        )?;
        Ok(true)
    }

    fn order_goal_cell(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        point: mint_movement::Vec2,
        mover: MoverClass,
    ) -> Option<mint_movement::Cell> {
        self.passable_cell_for(tick, ids, point, mover)
            .or_else(|| self.passability.cell_at(point))
    }

    fn passable_cell_near(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        point: mint_movement::Vec2,
        reach: f32,
        mover: MoverClass,
    ) -> Option<mint_movement::Cell> {
        if reach <= 0.0 {
            return self.passable_cell_for(tick, ids, point, mover);
        }
        let view = self.dynamic_view(tick, ids);
        if let Some(rect) = self.passability.covering_rect(point, reach) {
            if let Some(cell) =
                mint_sage_passability::nearest_passable_around_over(&view, rect, mover)
            {
                return Some(cell);
            }
        }
        self.passable_cell_for(tick, ids, point, mover)
    }

    fn dynamic_view<'a>(
        &'a self,
        tick: &'a Tick<'_>,
        ids: Ids,
    ) -> mint_sage_passability::GatedView<'a> {
        mint_sage_passability::GatedView {
            ground: mint_sage_passability::DynamicView {
                grid: &self.passability.grid,
                counts: crate::obstacles::counts_of(tick.state(), ids.obstacles),
                rubble: crate::obstacles::rubble_of(tick.state(), ids.obstacles),
                wall_rim: crate::wall_layer::rim_of(tick.state(), ids.wall_layer),
                tags: crate::wall_layer::tags_of(tick.state(), ids.wall_layer),
                connect: crate::wall_layer::connect_of(tick.state(), ids.wall_layer),
                released: crate::wall_layer::released_of(tick.state(), ids.wall_layer),
                closed: crate::obstacles::closed_of(tick.state(), ids.obstacles),
                obstacle_pinched: crate::obstacles::pinched_of(tick.state(), ids.obstacles),
                ramp_side: crate::wall_layer::side_of(tick.state(), ids.wall_layer),
                scaleable: crate::obstacles::scaleable_of(tick.state(), ids.obstacles),
                barred: border::barred_of(tick.state(), ids.human_barred),
            },
            portals: crate::portals::masks_of(tick.state(), ids.portals),
            tags: crate::portals::tags_of(tick.state(), ids.portals),
        }
    }

    pub(crate) fn rebuild_reachability(
        &self,
        wall_layer: &crate::wall_layer::WallLayer,
        scaleable: &[u8],
    ) {
        let view = DynamicView {
            grid: &self.passability.grid,
            counts: &self.empty_counts,
            rubble: &[],
            wall_rim: &[],
            tags: wall_layer.tags(),
            connect: wall_layer.connect(),
            released: wall_layer.released(),
            closed: &[],
            obstacle_pinched: &[],
            ramp_side: wall_layer.side(),
            scaleable,
            barred: &[],
        };
        let mover = self.reachability.borrow().mover();
        let ground = Reachability::build(&view, mover);
        let extra: Vec<(MoverClass, Reachability)> = self
            .reachability_extra
            .borrow()
            .iter()
            .map(|(terrain, _)| (*terrain, Reachability::build(&view, *terrain)))
            .collect();
        *self.reachability.borrow_mut() = ground;
        *self.reachability_extra.borrow_mut() = extra;
    }

    fn static_view(&self) -> DynamicView<'_> {
        DynamicView {
            grid: &self.passability.grid,
            counts: &self.empty_counts,
            rubble: &[],
            wall_rim: &[],
            tags: &[],
            connect: &[],
            released: &[],
            closed: &[],
            obstacle_pinched: &[],
            ramp_side: &[],
            scaleable: &[],
            barred: &[],
        }
    }

    fn map_low(passability: &Passability) -> (f32, f32) {
        let centre = passability
            .geometry
            .centre_of(mint_movement::Cell::new(0, 0));
        let half = passability.geometry.cell_size() * 0.5;
        (centre.x - half, centre.y - half)
    }

    fn map_high(passability: &Passability) -> (f32, f32) {
        let (low_x, low_y) = Self::map_low(passability);
        let extent = passability.grid.extent();
        let size = passability.geometry.cell_size();
        (
            low_x + f32::from(extent.width) * size,
            low_y + f32::from(extent.height) * size,
        )
    }

    fn playable_low(_passability: &Passability) -> (f32, f32) {
        (0.0, 0.0)
    }

    fn playable_high(passability: &Passability) -> (f32, f32) {
        (passability.playable_high.x, passability.playable_high.y)
    }

    fn clamp_move_goal(&self, world: &World, ids: Ids, player: PlayerId, goal: Vec2) -> Vec2 {
        if !self.is_human(player) {
            return goal;
        }
        let fudge = PARTITION_CELL_SIZE * 0.5;
        let high = self.active_high(world, ids);
        let (lo_x, lo_y) = (fudge, fudge);
        let (hi_x, hi_y) = (high.x - fudge, high.y - fudge);
        if lo_x > hi_x || lo_y > hi_y {
            return goal;
        }
        Vec2::new(goal.x.clamp(lo_x, hi_x), goal.y.clamp(lo_y, hi_y))
    }

    fn ticks_per_second() -> f32 {
        f32::from(u16::try_from(SIMULATION_HZ).unwrap_or(30))
    }

    fn tick_seconds() -> f32 {
        1.0 / Self::ticks_per_second()
    }

    fn insert_spawn(
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        spawn: &Spawn,
        combat: &CombatData,
    ) -> Result<()> {
        map_scripts::note_object_count_changed(tick, ids);
        tick.state_mut().insert(
            ids.body,
            entity,
            Body(MoverState::new(spawn.position, spawn.facing)),
        )?;
        tick.state_mut().insert(ids.shape, entity, spawn.shape)?;
        if let Some(offset) = spawn.ground_offset {
            tick.state_mut().insert(
                ids.ground_offset,
                entity,
                crate::unit::GroundOffset(offset),
            )?;
        }
        tick.state_mut()
            .insert(ids.owner, entity, Owner(spawn.owner))?;
        if let Some(max) = spawn.health {
            if let Some(pool) = HealthPool::new(max, BodyKind::Active) {
                tick.state_mut().insert(ids.health, entity, Health(pool))?;
            }
        }
        if let Some(armour) = spawn.armour {
            tick.state_mut()
                .insert(ids.armour, entity, Armour(armour))?;
        }
        if let Some(kind) = spawn.kind {
            tick.state_mut().insert(ids.kind, entity, kind)?;
        }
        if let Some(weapon) = spawn.weapon {
            if let Some(template) = combat.weapons.get(weapon) {
                let next_acquire = staggered_first_acquire(entity, tick.frame());
                tick.state_mut().insert(
                    ids.armed,
                    entity,
                    Armed {
                        weapon,
                        state: WeaponState::new(template),
                        target: None,
                        winding: false,
                        shots_at_target: 0,
                        last_shot_at: None,
                        next_acquire,
                        reacquire_ready: None,
                        squish_reacquire: false,
                    },
                )?;
            }
        }
        if let Some(range) = spawn.vision {
            tick.state_mut().insert(
                ids.vision,
                entity,
                Vision {
                    clearing_range: range,
                },
            )?;
        }
        tick.state_mut().insert(ids.seen, entity, Seen::hidden())?;
        if let Some(producer) = spawn.producer {
            tick.state_mut().insert(
                ids.producer,
                entity,
                Producer {
                    queue: mint_sage_economy::ProductionQueue::new(producer.depth),
                    menu: Some(producer.menu),
                    object_menu: None,
                    player_menu: None,
                    rally: producer.rally,
                    player_rally: None,
                },
            )?;
        }
        if let Some((timing, amount)) = spawn.income {
            tick.state_mut().insert(
                ids.income,
                entity,
                Income(mint_sage_economy::AutoDeposit::new(timing, amount)),
            )?;
        }
        if let Some(points) = spawn.upkeep {
            tick.state_mut()
                .insert(ids.upkeep, entity, Upkeep(points))?;
        }
        if let Some(claim) = spawn.claim {
            tick.state_mut().insert(ids.claimant, entity, claim)?;
        }
        Ok(())
    }

    fn attach_upgrade_state(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        kind: Option<crate::unit::Kind>,
    ) -> Result<()> {
        let Some(kind) = kind else {
            return Ok(());
        };
        if self.kinds.get(kind).is_none() {
            return Ok(());
        }
        self.attach_capture_bonus(tick, ids, entity, kind)?;
        let index = usize::from(kind.0);
        let body = self.body_kinds.get(index).copied().unwrap_or_default();
        if body != BodyKind::Active {
            if let Some(health) = tick.state_mut().get_mut(ids.health, entity) {
                if let Some(mut pool) = HealthPool::new(health.0.max(), body) {
                    pool.set_current(health.0.current());
                    health.0 = pool;
                }
            }
        }
        self.attach_regeneration(tick, ids, entity, kind)?;
        if self
            .kinds
            .get(kind)
            .is_some_and(|entry| entry.record.ai.animal_wander.is_some())
        {
            if let Some(at) = tick
                .state()
                .get(ids.body, entity)
                .map(|body| body.0.position)
            {
                tick.state_mut()
                    .insert(ids.animal_origin, entity, animal_ai::AnimalOrigin(at))?;
            }
        }
        let object_menu = self.object_upgrade_menu_of.get(index).copied().flatten();
        let player_menu = self.player_upgrade_menu_of.get(index).copied().flatten();
        if object_menu.is_some() || player_menu.is_some() {
            tick.state_mut().insert(
                ids.object_upgrades,
                entity,
                ObjectUpgrades(UpgradeMask::EMPTY),
            )?;
            if let Some(producer) = tick.state_mut().get_mut(ids.producer, entity) {
                producer.object_menu = object_menu;
                producer.player_menu = player_menu;
            } else if let Some(record) = self.kinds.get(kind).map(|entry| &entry.record) {
                let depth = crate::templates::queue_depth(record);
                let rally = tick
                    .state()
                    .get(ids.body, entity)
                    .map_or(Vec2::ZERO, |body| {
                        crate::templates::exit_rally(record, body.0.position, body.0.facing)
                    });
                tick.state_mut().insert(
                    ids.producer,
                    entity,
                    Producer {
                        queue: mint_sage_economy::ProductionQueue::new(depth),
                        menu: None,
                        object_menu,
                        player_menu,
                        rally,
                        player_rally: None,
                    },
                )?;
            }
        }
        let kind_entry = self.kinds.get(kind);
        let instances = kind_entry
            .map(|entry| {
                entry
                    .record
                    .powers
                    .iter()
                    .filter_map(|slot| {
                        Some(crate::powers::PowerInstance {
                            power: self.power_names.id(&slot.power)?,
                            available_on: Frame::ZERO,
                            reload: LogicFrames::ZERO,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !instances.is_empty() {
            tick.state_mut()
                .insert(ids.power_book, entity, PowerBook(instances))?;
            if let Some(entry) = kind_entry {
                if !tick.state().contains(ids.construction, entity) {
                    for slot in &entry.record.powers {
                        if slot.available_at_start || slot.starts_paused {
                            continue;
                        }
                        let shared = self
                            .power_names
                            .id(&slot.power)
                            .and_then(|id| self.power_reload.get(usize::from(id.get())))
                            .is_some_and(|(_, shared)| *shared);
                        if shared {
                            continue;
                        }
                        self.begin_recharge(tick, ids, entity, &slot.power)?;
                    }
                }
            }
        }
        self.attach_lifetime(tick, ids, entity, kind)?;
        self.enter_first_level(tick, ids, entity)?;
        self.create_modules(tick, ids, entity, Some(kind))?;
        self.grant_difficulty_upgrade(tick, ids, entity)?;
        self.apply_weather_to_new_object(tick, ids, entity)?;
        self.fire_object_event(tick, ids, entity, "OnCreated", None)
    }

    fn effective_upgrades(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> UpgradeMask {
        self.effective_upgrades_in(tick.state(), ids, entity)
    }

    fn effective_upgrades_in(&self, world: &World, ids: Ids, entity: Entity) -> UpgradeMask {
        let object = world
            .get(ids.object_upgrades, entity)
            .map_or_else(UpgradeMask::default, |mask| mask.0.clone());
        let player = world
            .get(ids.owner, entity)
            .and_then(|owner| crate::economy::player_entity(world, Self::economy_ids(ids), owner.0))
            .and_then(|player| world.get(ids.player_upgrades, player))
            .map_or_else(UpgradeMask::default, |mask| mask.0.clone());
        let held = object.union(&player);
        match self.level_upgrades(world, ids, entity) {
            Some(granted) if !granted.is_empty() => held.union(granted),
            _ => held,
        }
    }

    fn needs_gate(
        &self,
        world: &World,
        ids: Ids,
        structure: Entity,
        needs: &[Vec<crate::templates::Needs>],
        menu: u16,
        button: u8,
    ) -> Option<&'static str> {
        let gate = needs
            .get(usize::from(menu))
            .and_then(|menu| menu.get(usize::from(button)))?;
        (!gate.met(
            &self.effective_upgrades_in(world, ids, structure),
            &self.upgrade_bits,
        ))
        .then_some("its NeededUpgrade is not held")
    }

    pub(crate) fn upgrade_gate(
        &self,
        world: &World,
        ids: Ids,
        structure: Entity,
        is_player: bool,
        menu: u16,
        button: u8,
    ) -> Option<&'static str> {
        let (menus, needs) = if is_player {
            (&self.player_upgrade_menus, &self.player_upgrade_needs)
        } else {
            (&self.object_upgrade_menus, &self.object_upgrade_needs)
        };
        if let Some(reason) = self.needs_gate(world, ids, structure, needs, menu, button) {
            return Some(reason);
        }
        if is_player {
            return None;
        }
        let bit = menus
            .get(usize::from(menu))
            .and_then(|menu| menu.get(usize::from(button)))
            .and_then(|index| self.upgrade_buildables.get(usize::from(*index)))
            .map(|upgrade| upgrade.bit)?;
        (!self.affected_by_upgrade(world, ids, structure, bit))
            .then_some("no module of this object reacts to the upgrade")
    }

    pub(crate) fn effective_upgrade_names(&self, world: &World, entity: Entity) -> Vec<String> {
        let Some(ids) = self.ids else {
            return Vec::new();
        };
        let held = self.effective_upgrades_in(world, ids, entity);
        self.upgrade_bits
            .names()
            .filter(|(_, bit)| held.contains(*bit))
            .map(|(name, _)| name.to_owned())
            .collect()
    }

    pub(crate) fn object_upgrade_names(&self, world: &World, entity: Entity) -> Vec<String> {
        let Some(ids) = self.ids else {
            return Vec::new();
        };
        let held = world
            .get(ids.object_upgrades, entity)
            .map_or_else(UpgradeMask::default, |mask| mask.0.clone());
        self.upgrade_bits
            .names()
            .filter(|(_, bit)| held.contains(*bit))
            .map(|(name, _)| name.to_owned())
            .collect()
    }

    pub(crate) fn player_upgrade_names(&self, world: &World, entity: Entity) -> Vec<String> {
        let Some(ids) = self.ids else {
            return Vec::new();
        };
        let held = world
            .get(ids.owner, entity)
            .and_then(|owner| crate::economy::player_entity(world, Self::economy_ids(ids), owner.0))
            .and_then(|player| world.get(ids.player_upgrades, player))
            .map_or_else(UpgradeMask::default, |mask| mask.0.clone());
        self.upgrade_bits
            .names()
            .filter(|(_, bit)| held.contains(*bit))
            .map(|(name, _)| name.to_owned())
            .collect()
    }

    pub(crate) fn castle_held_upgrade_names(&self, world: &World, entity: Entity) -> Vec<String> {
        let Some(ids) = self.ids else {
            return Vec::new();
        };
        let held = world
            .get(ids.castle_member, entity)
            .map(|link| link.castle)
            .and_then(|castle| world.get(ids.castle_upgrades, castle))
            .map_or_else(UpgradeMask::default, |held| held.0.clone());
        self.upgrade_bits
            .names()
            .filter(|(_, bit)| held.contains(*bit))
            .map(|(name, _)| name.to_owned())
            .collect()
    }

    pub(crate) fn object_upgrade_affected(
        &self,
        world: &World,
        entity: Entity,
        upgrade: &str,
    ) -> bool {
        let Some(ids) = self.ids else {
            return true;
        };
        self.upgrade_buildables
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(upgrade))
            .is_none_or(|buildable| self.affected_by_upgrade(world, ids, entity, buildable.bit))
    }

    pub(crate) fn upgrade_affordable(&self, world: &World, entity: Entity, upgrade: &str) -> bool {
        let Some(ids) = self.ids else {
            return true;
        };
        let Some(buildable) = self
            .upgrade_buildables
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(upgrade))
        else {
            return true;
        };
        let Some(player) = world.get(ids.owner, entity).map(|owner| owner.0) else {
            return true;
        };
        let economy = Self::economy_ids(ids);
        let cost = crate::economy::upgrade_cost(
            world,
            economy,
            &self.kinds,
            &self.upgrade_bits,
            player,
            buildable,
            self.production_cost_change_percent(player),
        );
        crate::economy::purse_of(world, economy, player).can_afford(cost)
    }

    pub(crate) fn upgrade_names_in_production(&self, world: &World, entity: Entity) -> Vec<String> {
        let Some(ids) = self.ids else {
            return Vec::new();
        };
        let mut bits: Vec<mint_sim::UpgradeBit> = world
            .get(ids.producer, entity)
            .map(|producer| {
                producer
                    .queue
                    .entries()
                    .iter()
                    .filter(|entry| matches!(entry.kind, mint_sage_economy::EntryKind::Upgrade))
                    .filter_map(|entry| {
                        usize::try_from(entry.item.index())
                            .ok()
                            .and_then(|index| self.upgrade_buildables.get(index))
                            .map(|upgrade| upgrade.bit)
                    })
                    .collect()
            })
            .unwrap_or_default();
        if let Some(queued) = world
            .get(ids.owner, entity)
            .and_then(|owner| crate::economy::player_entity(world, Self::economy_ids(ids), owner.0))
            .and_then(|player| world.get(ids.player_upgrades_in_progress, player))
        {
            bits.extend(queued.0.iter().copied());
        }
        self.upgrade_bits
            .names()
            .filter(|(_, bit)| bits.contains(bit))
            .map(|(name, _)| name.to_owned())
            .collect()
    }

    pub(crate) const fn shroud_ids(ids: Ids) -> ShroudIds {
        ShroudIds {
            shroud: ids.shroud,
            vision: ids.vision,
            seen: ids.seen,
            body: ids.body,
            owner: ids.owner,
            shape: ids.shape,
            bonuses: ids.bonuses,
            kind: ids.kind,
        }
    }

    fn sight(&self) -> crate::shroud::Sight<'_, impl Fn(f32, f32) -> Option<f32> + '_> {
        crate::shroud::Sight {
            kinds: &self.kinds,
            ground: |x, y| self.passability.heights.height_at(x, y),
        }
    }

    pub(crate) const fn economy_ids(ids: Ids) -> EconomyIds {
        EconomyIds {
            slot: ids.slot,
            purse: ids.purse,
            points: ids.points,
            producer: ids.producer,
            income: ids.income,
            upkeep: ids.upkeep,
            owner: ids.owner,
            kind: ids.kind,
            construction: ids.construction,
            player_upgrades: ids.player_upgrades,
            player_upgrades_in_progress: ids.player_upgrades_in_progress,
            object_upgrades: ids.object_upgrades,
            bonuses: ids.bonuses,
            doomed: ids.doomed,
            claimant: ids.claimant,
            match_stats: ids.match_stats,
            object_value: ids.object_value,
        }
    }

    pub(crate) const fn defeat_ids(ids: Ids) -> DefeatIds {
        DefeatIds {
            owner: ids.owner,
            kind: ids.kind,
            health: ids.health,
            outcome: ids.outcome,
            facts: ids.defeat_facts,
            dying: Self::dying_ids(ids),
        }
    }

    pub(crate) const fn dying_ids(ids: Ids) -> death::DyingIds {
        death::DyingIds {
            selected: ids.selected,
            attack_order: ids.attack_order,
            path: ids.path,
            armed: ids.armed,
            doomed: ids.doomed,
        }
    }

    pub(crate) const fn horde_ids(ids: Ids) -> HordeIds {
        HordeIds {
            horde: ids.horde,
            member: ids.member,
            member_mark: ids.horde_member_mark,
            body: ids.body,
            path: ids.path,
            doomed: ids.doomed,
            health: ids.health,
            shape: ids.shape,
            bonuses: ids.bonuses,
            kind: ids.kind,
            locomotion: ids.locomotion,
            motive: ids.motive,
            owner: ids.owner,
            portals: ids.portals,
            wall_layer: ids.wall_layer,
            horde_melee: ids.horde_melee,
            member_melee: ids.member_melee,
            member_attack: ids.member_attack,
            goal_cells: ids.goal_cells,
            horde_trail: ids.horde_trail,
            glided: ids.glided,
            layer: ids.layer,
            aod_crusher: ids.aod_crusher,
            using_ability: ids.using_ability,
            join_fade: ids.join_fade,
            elevation: ids.elevation,
            scaleable_walls: ids.scaleable_walls,
            combat_facts: ids.combat_facts,
            human_barred: ids.human_barred,
        }
    }

    fn modes(&self, tick: &Tick<'_>, ids: Ids) -> Modes {
        tick.state()
            .store(ids.modes)
            .and_then(|store| store.iter().next().map(|(_, modes)| *modes))
            .unwrap_or_default()
    }
}

impl RotwkRules {
    fn reset_shroud(&self, tick: &mut Tick<'_>, ids: Ids, use_shroud: bool) -> Result<()> {
        let Some(entity) = tick
            .state()
            .store(ids.shroud)
            .and_then(|store| store.entities().first().copied())
        else {
            return Ok(());
        };
        let Some(mut grid) = crate::shroud::grid_for(
            Self::playable_low(&self.passability),
            Self::playable_high(&self.passability),
            self.players,
        ) else {
            return Ok(());
        };
        if !use_shroud {
            crate::shroud::reveal_map_at_start(&mut grid);
        }
        tick.state_mut().insert(
            ids.shroud,
            entity,
            Shroud(Some(crate::shroud::state_over(grid))),
        )?;
        let objects: Vec<Entity> = tick
            .state()
            .store(ids.seen)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        for object in objects {
            tick.state_mut().insert(ids.seen, object, Seen::hidden())?;
        }
        diag!(
            "shroud: reset - the map starts {} for every player [UseShroud = {}]",
            if use_shroud { "unexplored" } else { "explored" },
            if use_shroud { "Yes" } else { "No" }
        );
        Ok(())
    }

    fn fire_special_power(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        button: u8,
        target: Option<Entity>,
        at: Option<(f32, f32)>,
    ) -> Result<()> {
        let selected = self.command_subjects(tick, ids, player);
        for structure in selected {
            let owned = tick
                .state()
                .get(ids.owner, structure)
                .is_some_and(|owner| owner.0 == player);
            if !owned || tick.state().contains(ids.no_attack, structure) {
                continue;
            }
            let Some(kind) = tick.state().get(ids.kind, structure).copied() else {
                continue;
            };
            let Some(entry) = self.kinds.get(kind) else {
                continue;
            };
            let buttons =
                entry.power_buttons_under(self.command_set_override(tick.state(), structure));
            let Some((power_name, target_kind)) = buttons.get(usize::from(button)).cloned() else {
                diag!(
                    "power: entity {} has no button {} - it offers {}",
                    structure.index(),
                    u16::from(button).saturating_add(1),
                    buttons.len()
                );
                continue;
            };
            let set = self.command_set_override(tick.state(), structure);
            let needs_met = entry
                .power_needs_under(set)
                .get(usize::from(button))
                .is_none_or(|needs| {
                    needs.met(
                        &self.effective_upgrades(tick, ids, structure),
                        &self.upgrade_bits,
                    )
                });
            if !needs_met {
                diag!(
                    "power: {power_name} at entity {} refused - its NeededUpgrade is not held",
                    structure.index()
                );
                continue;
            }
            let relation = entry
                .auto_power_under(set)
                .iter()
                .find(|auto| auto.power.eq_ignore_ascii_case(&power_name))
                .map_or_else(ObjectFilter::default, |auto| auto.relation.clone());
            if let Some(refusal) =
                self.manual_pick_up_refusal(tick, ids, structure, &power_name, target)
            {
                diag!(
                    "power: {power_name} at entity {} refused - {refusal}",
                    structure.index()
                );
                continue;
            }
            self.fire_power_from(
                tick,
                ids,
                player,
                structure,
                &power_name,
                target_kind,
                &relation,
                target,
                at,
                false,
            )?;
        }
        Ok(())
    }

    fn toggle_auto_cast_button(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        button: u8,
    ) -> Result<()> {
        let selected = self.command_subjects(tick, ids, player);
        for structure in selected {
            let owned = tick
                .state()
                .get(ids.owner, structure)
                .is_some_and(|owner| owner.0 == player);
            if !owned || tick.state().contains(ids.no_attack, structure) {
                continue;
            }
            let Some(kind) = tick.state().get(ids.kind, structure).copied() else {
                continue;
            };
            let Some(entry) = self.kinds.get(kind) else {
                continue;
            };
            let set = self.command_set_override(tick.state(), structure);
            let Some((power_name, _)) = entry
                .power_buttons_under(set)
                .get(usize::from(button))
                .cloned()
            else {
                continue;
            };
            let offers_auto = entry
                .auto_power_under(set)
                .iter()
                .any(|auto| auto.power.eq_ignore_ascii_case(&power_name));
            if !offers_auto {
                continue;
            }
            self.toggle_auto_cast(tick, ids, structure, &power_name)?;
        }
        Ok(())
    }

    fn spawn_horde_entity(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        spawn: &HordeSpawn,
    ) -> Result<Option<Entity>> {
        let Some(table) = self
            .formations
            .get(usize::from(spawn.formation))
            .filter(|table| !table.is_empty())
        else {
            return Ok(None);
        };
        let banner_offset = self
            .banners
            .get(usize::from(spawn.formation))
            .copied()
            .flatten()
            .map(|plan| plan.offset);
        let horde = crate::horde::spawn_horde(
            tick,
            Self::horde_ids(ids),
            spawn,
            table,
            banner_offset,
            |name| self.is_machine(name),
            &self.passability,
            |tick, member, position, facing| {
                let built = member.kind.and_then(|kind| {
                    let drawn = self.resolve_build_variation(tick, kind);
                    self.kinds
                        .spawn(&self.combat, drawn, position, facing, member.owner)
                });
                let built = built.unwrap_or(Spawn {
                    position,
                    facing,
                    ..*member
                });
                let entity = tick.state_mut().spawn()?;
                Self::insert_spawn(tick, ids, entity, &built, &self.combat)?;
                self.attach_upgrade_state(tick, ids, entity, built.kind)?;
                Ok(entity)
            },
        )?;
        tick.state_mut()
            .insert(ids.owner, horde, Owner(spawn.member.owner))?;
        tick.state_mut().insert(
            ids.vision,
            horde,
            Vision {
                clearing_range: spawn.vision,
            },
        )?;
        Ok(Some(horde))
    }

    fn is_machine(&self, name: &str) -> bool {
        self.kinds
            .kind(name)
            .and_then(|kind| self.kinds.get(kind))
            .is_some_and(|entry| entry.record.kind_of.iter().any(|flag| flag == "MACHINE"))
    }

    fn horde_kind_of(&self, spawn: &HordeSpawn) -> Option<crate::unit::Kind> {
        spawn
            .member
            .kind
            .and_then(|member_kind| self.kinds.horde_kind(spawn.formation, member_kind))
    }
}

impl Rules for RotwkRules {
    type Command = RotwkCommand;

    fn setup(&mut self, tick: &mut Tick<'_>) -> Result<()> {
        let ids = Ids {
            clock: tick.state_mut().register::<MatchClock>(),
            body: tick.state_mut().register::<Body>(),
            shape: tick.state_mut().register::<Shape>(),
            owner: tick.state_mut().register::<Owner>(),
            selected: tick.state_mut().register::<Selected>(),
            path: tick.state_mut().register::<Path>(),
            modes: tick.state_mut().register::<Modes>(),
            health: tick.state_mut().register::<Health>(),
            armour: tick.state_mut().register::<Armour>(),
            armed: tick.state_mut().register::<Armed>(),
            attack_order: tick.state_mut().register::<AttackOrder>(),
            flight: tick.state_mut().register::<Flight>(),
            doomed: tick.state_mut().register::<Doomed>(),
            horde: tick.state_mut().register::<Horde>(),
            member: tick.state_mut().register::<Member>(),
            kind: tick.state_mut().register::<crate::unit::Kind>(),
            slot: tick.state_mut().register::<PlayerSlot>(),
            purse: tick.state_mut().register::<Purse>(),
            points: tick.state_mut().register::<Points>(),
            producer: tick.state_mut().register::<Producer>(),
            income: tick.state_mut().register::<Income>(),
            upkeep: tick.state_mut().register::<Upkeep>(),
            shroud: tick.state_mut().register::<Shroud>(),
            vision: tick.state_mut().register::<Vision>(),
            seen: tick.state_mut().register::<Seen>(),
            obstacles: tick.state_mut().register::<ObstacleLayer>(),
            replanned_at: tick.state_mut().register::<ReplannedAt>(),
            construction: tick.state_mut().register::<Construction>(),
            build_task: tick.state_mut().register::<BuildTask>(),
            concealed: tick.state_mut().register::<Concealed>(),
            outcome: tick.state_mut().register::<MatchOutcome>(),
            ai_progress: tick
                .state_mut()
                .register::<crate::skirmish_ai::AiProgress>(),
            ai_army_progress: tick
                .state_mut()
                .register::<crate::skirmish_ai::AiArmyProgress>(),
            ai_attack_progress: tick
                .state_mut()
                .register::<crate::skirmish_ai::AiAttackProgress>(),
            path_queue: tick.state_mut().register::<PathQueue>(),
            horde_target: tick.state_mut().register::<HordeTarget>(),
            attack_move_order: tick.state_mut().register::<AttackMoveOrder>(),
            player_upgrades: tick
                .state_mut()
                .register::<crate::economy::PlayerUpgrades>(),
            player_upgrades_in_progress: tick
                .state_mut()
                .register::<crate::economy::PlayerUpgradesInProgress>(),
            object_upgrades: tick
                .state_mut()
                .register::<crate::economy::ObjectUpgrades>(),
            power_book: tick.state_mut().register::<crate::powers::PowerBook>(),
            player_power_cooldowns: tick
                .state_mut()
                .register::<crate::powers::PlayerPowerCooldowns>(),
            experience: tick.state_mut().register::<crate::experience::Experience>(),
            bonuses: tick.state_mut().register::<crate::experience::Bonuses>(),
            hero_roster: tick.state_mut().register::<crate::heroes::HeroRoster>(),
            player_science: tick
                .state_mut()
                .register::<crate::sciences::PlayerScience>(),
            timed_modifiers: tick
                .state_mut()
                .register::<crate::experience::TimedModifiers>(),
            lifetime: tick.state_mut().register::<crate::summons::Lifetime>(),
            fx: tick.state_mut().register::<crate::fx::FxEvents>(),
            pending_shots: tick
                .state_mut()
                .register::<crate::spell_state::PendingShots>(),
            weapon_clocks: tick
                .state_mut()
                .register::<crate::spell_state::WeaponClocks>(),
            dots: tick
                .state_mut()
                .register::<crate::spell_state::DamageOverTime>(),
            suppressed: tick
                .state_mut()
                .register::<crate::spell_state::Suppressed>(),
            cash_bounty: tick
                .state_mut()
                .register::<crate::spell_state::CashBounty>(),
            production_bonuses: tick
                .state_mut()
                .register::<crate::spell_state::ProductionBonuses>(),
            last_hurt: tick.state_mut().register::<crate::spell_state::LastHurt>(),
            sole_healer: tick
                .state_mut()
                .register::<crate::spell_state::SoleHealer>(),
            healed_at: tick.state_mut().register::<crate::spell_state::HealedAt>(),
            point_bonus: tick
                .state_mut()
                .register::<crate::spell_state::PointBonus>(),
            point_rule: tick.state_mut().register::<crate::spell_state::PointRule>(),
            castle_member: tick.state_mut().register::<crate::castle::CastleMember>(),
            pad_occupant: tick.state_mut().register::<crate::castle::PadOccupant>(),
            voice: tick.state_mut().register::<crate::voice::VoiceEvents>(),
            locomotion: tick.state_mut().register::<crate::unit::Locomotion>(),
            player_order: tick.state_mut().register::<PlayerOrder>(),
            no_attack: tick.state_mut().register::<NoAttack>(),
            capture_bonus: tick.state_mut().register::<capture::CaptureBonus>(),
            capture_links: tick.state_mut().register::<capture::CaptureLinks>(),
            rebuild_hole: tick.state_mut().register::<rebuild::RebuildHole>(),
            sold: tick.state_mut().register::<rebuild::Sold>(),
            wall_links: tick.state_mut().register::<wall_link::WallLinks>(),
            ground_offset: tick.state_mut().register::<crate::unit::GroundOffset>(),
            corpse_sink: tick.state_mut().register::<death::CorpseSink>(),
            player_point_bonus: tick
                .state_mut()
                .register::<crate::spell_state::PointBonus>(),
            auto_cast: tick.state_mut().register::<auto_cast::AutoCastSlot>(),
            using_ability: tick.state_mut().register::<upgrades::UsingAbility>(),
            capturer: tick.state_mut().register::<capture::Capturer>(),
            motive: tick.state_mut().register::<crate::unit::Motive>(),
            castle_upgrades: tick.state_mut().register::<crate::castle::CastleUpgrades>(),
            delayed_hits: tick
                .state_mut()
                .register::<crate::spell_state::DelayedHits>(),
            defect_revert: tick.state_mut().register::<capture::DefectRevert>(),
            arrow_storm_hits: tick
                .state_mut()
                .register::<crate::spell_state::ArrowStormHits>(),
            weapon_slots: tick.state_mut().register::<WeaponSlots>(),
            path_retry: tick.state_mut().register::<PathRetry>(),
            portals: tick.state_mut().register::<crate::portals::PortalLayer>(),
            gate_cycle: tick.state_mut().register::<gates::GateCycle>(),
            emotion: tick.state_mut().register::<emotions::Emotion>(),
            emotion_table: tick.state_mut().register::<emotions::EmotionTable>(),
            horde_combat: tick.state_mut().register::<replenish::HordeCombat>(),
            banner: tick.state_mut().register::<replenish::Banner>(),
            passengers: tick.state_mut().register::<contain::Passengers>(),
            contained: tick.state_mut().register::<contain::Contained>(),
            entering: tick.state_mut().register::<contain::Entering>(),
            guard: tick.state_mut().register::<guard::Guard>(),
            stance: tick.state_mut().register::<CombatStance>(),
            knocked: tick.state_mut().register::<knockback::Knocked>(),
            contact_claim: tick.state_mut().register::<contact::ContactClaim>(),
            stealth: tick.state_mut().register::<stealth::Stealth>(),
            group_bonus: tick.state_mut().register::<group_bonus::GroupBonus>(),
            hit_reaction: tick.state_mut().register::<hit_reaction::HitReaction>(),
            payload_filled: tick.state_mut().register::<contain::PayloadFilled>(),
            special_conditions: tick
                .state_mut()
                .register::<hit_reaction::SpecialConditions>(),
            damage_fx_throttle: tick.state_mut().register::<damage_fx::DamageFxThrottle>(),
            killing_blow: tick.state_mut().register::<death::KillingBlow>(),
            elevation: tick.state_mut().register::<crate::unit::Elevation>(),
            site_worker: tick.state_mut().register::<SiteWorker>(),
            ignored_obstacle: tick.state_mut().register::<movement::IgnoredObstacle>(),
            claimant: tick.state_mut().register::<crate::claims::Claimant>(),
            claims: tick.state_mut().register::<crate::claims::ClaimLayer>(),
            flight_level: tick.state_mut().register::<movement::FlightLevel>(),
            held: tick.state_mut().register::<movement::Held>(),
            burning: tick.state_mut().register::<death::Burning>(),
            ground_order: tick.state_mut().register::<crate::combat::GroundOrder>(),
            flanked: tick.state_mut().register::<flank::FlankMemory>(),
            crush_contacts: tick.state_mut().register::<crate::combat::CrushContacts>(),
            dynamic_portal_stamp: tick.state_mut().register::<gates::DynamicPortalStamp>(),
            corpse_fade: tick.state_mut().register::<death::CorpseFade>(),
            giant_bird: tick.state_mut().register::<giant_bird::GiantBird>(),
            horde_melee: tick.state_mut().register::<crate::melee::HordeMelee>(),
            member_melee: tick.state_mut().register::<crate::melee::MemberMelee>(),
            leader_trail: tick.state_mut().register::<horde_swarm::LeaderTrail>(),
            member_attack: tick.state_mut().register::<crate::melee::MemberAttack>(),
            loose_melee: tick.state_mut().register::<loose_melee::LooseMelee>(),
            charge_pass: tick.state_mut().register::<charge_pass::ChargePass>(),
            goal_cells: tick.state_mut().register::<crate::goal_cells::GoalCells>(),
            dummy: tick.state_mut().register::<debug::Dummy>(),
            contact: tick.state_mut().register::<enemy_contact::Contact>(),
            cell_mark: tick.state_mut().register::<cell_change::CellMark>(),
            crush_speed_cap: tick.state_mut().register::<crate::combat::CrushSpeedCap>(),
            desired_speed: tick.state_mut().register::<crate::unit::DesiredSpeed>(),
            formation_speed: tick.state_mut().register::<crate::unit::FormationSpeed>(),
            crush_notice: tick.state_mut().register::<crush_notice::CrushNotice>(),
            siege_deploy: tick.state_mut().register::<siege::SiegeDeploy>(),
            dock_occupants: tick.state_mut().register::<siege::DockOccupants>(),
            deploy_style: tick.state_mut().register::<siege::DeployStyle>(),
            forest: tick.state_mut().register::<crate::forest::Forest>(),
            last_attacker: tick.state_mut().register::<guard::LastAttacker>(),
            deferred_order: tick.state_mut().register::<busy::DeferredOrder>(),
            harvesting: tick.state_mut().register::<harvest::Harvesting>(),
            approach_wait: tick.state_mut().register::<shooting::ApproachWait>(),
            ranged_approach: tick.state_mut().register::<attack::RangedApproach>(),
            locomotor_override: tick.state_mut().register::<upgrades::LocomotorOverride>(),
            horde_last_shot: tick.state_mut().register::<replenish::HordeLastShot>(),
            turret_state: tick.state_mut().register::<shooting::TurretState>(),
            straggler_clock: tick.state_mut().register::<contain::StragglerClock>(),
            no_auto_acquire: tick.state_mut().register::<upgrades::NoAutoAcquire>(),
            production_stream: tick
                .state_mut()
                .register::<crate::horde::ProductionStream>(),
            in_formation_template: tick
                .state_mut()
                .register::<drag_formation::InFormationTemplate>(),
            rampaging: tick.state_mut().register::<rampage::Rampaging>(),
            flame_heat: tick.state_mut().register::<fire::FlameHeat>(),
            aflame: tick.state_mut().register::<fire::Aflame>(),
            pending_eject: tick.state_mut().register::<rampage::PendingEject>(),
            animal_origin: tick.state_mut().register::<animal_ai::AnimalOrigin>(),
            aod_crusher: tick.state_mut().register::<aod_crush::AodCrusher>(),
            amphibious_unload: tick.state_mut().register::<amphibious::AmphibiousUnload>(),
            ability_weapon_clocks: tick
                .state_mut()
                .register::<crate::spell_state::AbilityWeaponClocks>(),
            suspended_order: tick.state_mut().register::<SuspendedOrder>(),
            auto_repair: tick
                .state_mut()
                .register::<crate::construction::AutoRepair>(),
            horde_trail: tick
                .state_mut()
                .register::<crate::horde_trail::HordeTrail>(),
            glided: tick.state_mut().register::<crate::horde_trail::Glided>(),
            ai_power_refire: tick.state_mut().register::<ai_powers::AiPowerRefire>(),
            rally_events: tick.state_mut().register::<rally::RallyEvents>(),
            join_fade: tick.state_mut().register::<crate::horde::JoinFade>(),
            death_phase: tick
                .state_mut()
                .register::<crate::death_phase::DeathPhaseEvents>(),
            death_phase_clock: tick.state_mut().register::<death::DeathPhaseClock>(),
            spell_recharge: tick
                .state_mut()
                .register::<crate::spell_state::SpellRecharge>(),
            undetected_defector: tick.state_mut().register::<crates::UndetectedDefector>(),
            geometry_from: tick.state_mut().register::<crate::unit::GeometryFrom>(),
            crate_texts: tick.state_mut().register::<crates::CrateTexts>(),
            combat_facts: tick
                .state_mut()
                .register::<crate::combat_facts::CombatFacts>(),
            engaged: tick.state_mut().register::<engaged::Engaged>(),
            misc_sounds: tick.state_mut().register::<misc_sounds::MiscSounds>(),
            map_placement: tick.state_mut().register::<crate::unit::MapPlacement>(),
            wall_layer: tick.state_mut().register::<crate::wall_layer::WallLayer>(),
            layer: tick.state_mut().register::<crate::wall_layer::Layer>(),
            map_script_state: tick.state_mut().register::<map_scripts::MapScriptState>(),
            script_name: tick.state_mut().register::<map_scripts::ScriptObjectName>(),
            attach_link: tick.state_mut().register::<attach::AttachLink>(),
            disallowed_buildings: tick
                .state_mut()
                .register::<crate::economy::DisallowedBuildings>(),
            forced_model_condition: tick
                .state_mut()
                .register::<map_scripts::ForcedModelCondition>(),
            fire_grid: tick.state_mut().register::<terrain_fire::FireGrid>(),
            build_switches: tick.state_mut().register::<crate::economy::BuildSwitches>(),
            order_queue: tick.state_mut().register::<crate::combat::OrderQueue>(),
            scaleable_walls: tick.state_mut().register::<ScaleableWalls>(),
            horde_member_mark: tick.state_mut().register::<crate::horde::HordeMemberMark>(),
            defeat_facts: tick.state_mut().register::<crate::defeat::DefeatFacts>(),
            timed_conditions: tick
                .state_mut()
                .register::<crate::experience::TimedConditions>(),
            granted_hiding: tick.state_mut().register::<stealth::GrantedHiding>(),
            weather: tick.state_mut().register::<crate::spell_state::Weather>(),
            weather_effect: tick
                .state_mut()
                .register::<crate::spell_state::WeatherEffect>(),
            bonus_conditions: tick
                .state_mut()
                .register::<crate::experience::WeaponBonusConditions>(),
            ability_loop_sound: tick.state_mut().register::<upgrades::AbilityLoop>(),
            capture_flashes: tick.state_mut().register::<capture::CaptureFlashes>(),
            stored_objects: tick
                .state_mut()
                .register::<crate::spell_state::StoredObjectLists>(),
            ignore_ai_command: tick.state_mut().register::<busy::IgnoreAiCommand>(),
            die_ocl_stamp: tick.state_mut().register::<damage_creation::DieOclStamp>(),
            cah_record: tick
                .state_mut()
                .register::<crate::create_a_hero::PlayerHero>(),
            match_stats: tick
                .state_mut()
                .register::<crate::match_stats::MatchStats>(),
            object_value: tick.state_mut().register::<crate::economy::ObjectValue>(),
            border: tick
                .state_mut()
                .register::<crate::rules::border::ActiveBorder>(),
            human_barred: tick
                .state_mut()
                .register::<crate::rules::border::HumanBarred>(),
            events_list: tick.state_mut().register::<map_keys::EventsList>(),
        };
        let bits = Arc::clone(&self.upgrade_bits);
        self.combat.resolve_upgrade_filters(|name| bits.bit(name));
        self.setup_weapon_fire();
        self.grab_tosses = (0..self.kinds.len())
            .map(|index| {
                let entry = u16::try_from(index)
                    .ok()
                    .and_then(|k| self.kinds.get(crate::unit::Kind(k)))?;
                entry
                    .record
                    .declared_modules
                    .iter()
                    .filter(|module| module.name.eq_ignore_ascii_case("GiantBirdAIUpdate"))
                    .find_map(|module| {
                        mint_sage_modules::grab_toss_from_fields(&module.source(&self.upgrade_bits))
                            .ok()
                            .flatten()
                    })
            })
            .collect();
        self.giant_birds = (0..self.kinds.len())
            .map(|index| {
                let entry = u16::try_from(index)
                    .ok()
                    .and_then(|k| self.kinds.get(crate::unit::Kind(k)))
                    .filter(|entry| entry.limits.is_some())?;
                entry
                    .record
                    .declared_modules
                    .iter()
                    .filter(|module| module.name.eq_ignore_ascii_case("GiantBirdAIUpdate"))
                    .map(|module| {
                        mint_sage_modules::follow_through_from_fields(
                            &module.source(&self.upgrade_bits),
                        )
                        .unwrap_or_default()
                    })
                    .next()
            })
            .collect();
        self.bird_kinds = if self.giant_birds.iter().any(Option::is_some) {
            (0..self.kinds.len())
                .map(|index| {
                    u16::try_from(index)
                        .ok()
                        .and_then(|k| self.kinds.get(crate::unit::Kind(k)))
                        .map_or_else(giant_bird::BirdKind::default, |entry| {
                            giant_bird::BirdKind::from_kind_of(&entry.record.kind_of)
                        })
                })
                .collect()
        } else {
            Vec::new()
        };
        self.report_flank_census();
        self.report_crush_census();
        self.setup_projectiles();
        self.setup_stealth();
        let wiring = ModuleWiring::setup(
            tick.state_mut(),
            &self.kinds,
            &self.upgrade_bits,
            &self.combat,
            &self.emotions,
        );
        println!("{}", wiring.describe());
        self.modules = Some(wiring);
        self.level_tables = crate::experience::LevelTables::build(
            &self.kinds,
            &self.experience_tables,
            &self.experience_scalars,
            &self.combat.modifiers,
            &self.upgrade_bits,
        );
        println!("{}", self.level_tables.describe());
        self.hero_tables = self.build_hero_tables();
        self.lifetimes =
            crate::summons::Lifetimes::build(&self.kinds, &mut self.combat.death_types);
        self.death_plans = crate::summons::DeathPlans::build(&self.kinds);
        println!("{}", self.death_plans.describe());
        let mut reachability_extra = Vec::new();
        for entry in self.kinds.iter() {
            let Some(terrain) = entry.mover.map(MoverClass::terrain) else {
                continue;
            };
            if terrain == self.reachability.borrow().mover()
                || reachability_extra
                    .iter()
                    .any(|(class, _): &(MoverClass, Reachability)| *class == terrain)
            {
                continue;
            }
            let oracle = Reachability::build(
                &DynamicView {
                    grid: &self.passability.grid,
                    counts: &self.empty_counts,
                    rubble: &[],
                    wall_rim: &[],
                    tags: &[],
                    connect: &[],
                    released: &[],
                    closed: &[],
                    obstacle_pinched: &[],
                    ramp_side: &[],
                    scaleable: &[],
                    barred: &[],
                },
                terrain,
            );
            reachability_extra.push((terrain, oracle));
        }
        *self.reachability_extra.borrow_mut() = reachability_extra;
        self.burning_deaths = death::BurningDeaths::build(&self.kinds);
        println!("{}", self.burning_deaths.describe());
        self.contain_damage =
            contain::ContainDamageReactions::build(&self.kinds, &self.combat.weapons);
        println!("{}", self.contain_damage.describe());
        self.rampage_specs = rampage::RampageSpecs::build(&self.kinds);
        self.flammable_specs = fire::FlammableSpecs::build(&self.kinds, &self.combat.damage_types);
        println!("{}", self.flammable_specs.describe());
        if !self.spell_plans.is_empty() {
            println!("{}; {} rank(s)", self.sciences.describe(), self.ranks.len());
        }

        let clock_entity = tick.state_mut().spawn()?;
        tick.state_mut()
            .insert(ids.clock, clock_entity, MatchClock(0))?;
        tick.state_mut()
            .insert(ids.modes, clock_entity, Modes::default())?;
        tick.state_mut()
            .insert(ids.path_queue, clock_entity, PathQueue::default())?;
        tick.state_mut()
            .insert(ids.fx, clock_entity, crate::fx::FxEvents::default())?;
        tick.state_mut().insert(
            ids.voice,
            clock_entity,
            crate::voice::VoiceEvents::default(),
        )?;
        tick.state_mut().insert(
            ids.rally_events,
            clock_entity,
            rally::RallyEvents::default(),
        )?;
        if let Some(engine) = self.map_scripts.as_ref() {
            let mut script_state = engine.initial_state();
            self.seed_team_builders(&mut script_state);
            tick.state_mut()
                .insert(ids.map_script_state, clock_entity, script_state)?;
            self.restore_carryover(tick, ids)?;
        }
        tick.state_mut()
            .insert(ids.crate_texts, clock_entity, crates::CrateTexts::default())?;
        tick.state_mut().insert(
            ids.death_phase,
            clock_entity,
            crate::death_phase::DeathPhaseEvents::default(),
        )?;
        tick.state_mut().insert(
            ids.combat_facts,
            clock_entity,
            crate::combat_facts::CombatFacts::default(),
        )?;
        tick.state_mut().insert(
            ids.defeat_facts,
            clock_entity,
            crate::defeat::DefeatFacts::default(),
        )?;
        tick.state_mut().insert(
            ids.misc_sounds,
            clock_entity,
            misc_sounds::MiscSounds::default(),
        )?;
        tick.state_mut().insert(
            ids.capture_flashes,
            clock_entity,
            capture::CaptureFlashes::default(),
        )?;

        if let Some(mut grid) = crate::shroud::grid_for(
            Self::playable_low(&self.passability),
            Self::playable_high(&self.passability),
            self.players,
        ) {
            if !self.use_shroud {
                crate::shroud::reveal_map_at_start(&mut grid);
            }
            tick.state_mut().insert(
                ids.shroud,
                clock_entity,
                Shroud(Some(crate::shroud::state_over(grid))),
            )?;
        }

        {
            let (low_x, low_y) = Self::map_low(&self.passability);
            let (high_x, high_y) = Self::map_high(&self.passability);
            let mut forest = crate::forest::Forest::new(
                mint_movement::Vec2::new(low_x, low_y),
                mint_movement::Vec2::new(high_x, high_y),
            );
            for seed in &self.forest_seeds {
                forest.add_tree(
                    seed.pos,
                    seed.kind,
                    crate::forest::TreeTraits {
                        grabbable: seed.grabbable,
                        harvestable: seed.harvestable,
                        supply: seed.supply,
                        tainted: seed.tainted,
                    },
                    crate::forest::TreeOrigin::Placed(seed.source_index),
                );
            }
            let harvestable = self.forest_seeds.iter().filter(|s| s.harvestable).count();
            let grabbable = self.forest_seeds.iter().filter(|s| s.grabbable).count();
            println!(
                "forest: {} tree(s) seeded from the map ({harvestable} harvestable, {grabbable} grabbable)",
                forest.len()
            );
            for seed in self.forest_seeds.iter().filter(|s| s.harvestable).take(8) {
                diag!(
                    "forest: harvestable tree at ({:.0}, {:.0})",
                    seed.pos.x,
                    seed.pos.y
                );
            }
            tick.state_mut().insert(ids.forest, clock_entity, forest)?;
        }

        if let Some(mut fire_grid) = self.fire_grid.clone() {
            for seed in &self.forest_seeds {
                let Some(material) = self
                    .kinds
                    .get(seed.kind)
                    .and_then(|entry| entry.record.flammability)
                else {
                    continue;
                };
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let cell = |v: f32| (v / terrain_fire::CELL_SIZE).floor().max(0.0) as usize;
                fire_grid.overwrite_once(cell(seed.pos.x), cell(seed.pos.y), material);
            }
            tick.state_mut()
                .insert(ids.fire_grid, clock_entity, fire_grid)?;
        }

        let mut player_entities: Vec<Entity> = Vec::with_capacity(usize::from(self.players));
        for slot in 0..self.players {
            let player = PlayerId::new(slot);
            let entity = tick.state_mut().spawn()?;
            player_entities.push(entity);
            tick.state_mut()
                .insert(ids.slot, entity, PlayerSlot(player))?;
            tick.state_mut().insert(
                ids.purse,
                entity,
                Purse(mint_sage_economy::Money::new(self.starting_cash_for(slot))),
            )?;
            tick.state_mut().insert(
                ids.match_stats,
                entity,
                crate::match_stats::MatchStats::default(),
            )?;
            let mut starting = self
                .initial_upgrades
                .get(usize::from(slot))
                .cloned()
                .unwrap_or_default();
            if let Some(plan) = self.cah_plans.iter().find(|plan| plan.player == player) {
                tick.state_mut().insert(
                    ids.cah_record,
                    entity,
                    crate::create_a_hero::PlayerHero(Arc::clone(&plan.record)),
                )?;
                if let Some(bit) = self.cah_can_build {
                    starting.set(bit);
                }
            }
            if !starting.is_empty() {
                tick.state_mut()
                    .insert(ids.player_upgrades, entity, PlayerUpgrades(starting))?;
                println!("upgrade: player {slot} starts with its faction's InitialUpgrades");
            }
            let alignment = self
                .alignments
                .get(usize::from(slot))
                .copied()
                .unwrap_or_else(|| crate::economy::alignment_of(player));
            let rule = self.point_rule_for(alignment, true);
            tick.state_mut().insert(ids.point_rule, entity, rule)?;
            tick.state_mut().insert(
                ids.points,
                entity,
                Points(mint_sage_economy::CommandPointPool::new(rule.limit(0))),
            )?;
            if let Some(roster) = self.hero_rosters.get(usize::from(slot)) {
                tick.state_mut().insert(
                    ids.hero_roster,
                    entity,
                    crate::heroes::HeroRoster(
                        roster
                            .iter()
                            .map(|kind| crate::heroes::HeroSlot {
                                kind: *kind,
                                state: crate::heroes::HeroState::Available,
                            })
                            .collect(),
                    ),
                )?;
            }
        }
        self.setup_spell_books(tick, ids, &player_entities)?;

        for side in &self.computer_sides {
            let entity = tick.state_mut().spawn()?;
            tick.state_mut()
                .insert(ids.slot, entity, PlayerSlot(side.player))?;
            tick.state_mut().insert(
                ids.purse,
                entity,
                Purse(mint_sage_economy::Money::new(side.cash)),
            )?;
            tick.state_mut().insert(
                ids.match_stats,
                entity,
                crate::match_stats::MatchStats::default(),
            )?;
            let alignment = if side.evil {
                mint_sage_economy::Alignment::Evil
            } else {
                mint_sage_economy::Alignment::Good
            };
            let rule = self.point_rule_for(alignment, false);
            tick.state_mut().insert(ids.point_rule, entity, rule)?;
            tick.state_mut().insert(
                ids.points,
                entity,
                Points(mint_sage_economy::CommandPointPool::new(rule.limit(0))),
            )?;
            tick.state_mut().insert(
                ids.build_switches,
                entity,
                crate::economy::BuildSwitches {
                    can_build_units: false,
                    can_build_base: true,
                },
            )?;
        }

        for seed in &self.ai_seeds {
            if seed.candidates.is_empty() {
                println!(
                    "skirmish-ai: player {} has no AIBase candidate for its faction - base \
                     building disabled",
                    seed.player.get()
                );
                continue;
            }
            let count = u32::try_from(seed.candidates.len()).unwrap_or(1);
            let index = usize::try_from(tick.rng().below(count)).unwrap_or(0);
            let Some(chosen) = seed.candidates.get(index) else {
                continue;
            };
            println!(
                "skirmish-ai: player {} chose base \"{}\" ({} of {} candidate(s)), {} \
                 building(s) to place",
                seed.player.get(),
                chosen.name,
                index.saturating_add(1),
                seed.candidates.len(),
                chosen.tasks.len()
            );
            if let Some(&entity) = player_entities.get(usize::from(seed.player.get())) {
                tick.state_mut().insert(
                    ids.ai_progress,
                    entity,
                    crate::skirmish_ai::AiProgress {
                        tasks: vec![crate::skirmish_ai::AiTaskState::Pending; chosen.tasks.len()],
                        structure_timer: 0,
                    },
                )?;
            }
            self.ai_plan.push((seed.player, chosen.tasks.clone()));
        }

        for plan in &self.ai_army_plans {
            if let Some(&entity) = player_entities.get(usize::from(plan.player.get())) {
                tick.state_mut().insert(
                    ids.ai_army_progress,
                    entity,
                    crate::skirmish_ai::AiArmyProgress::default(),
                )?;
            }
        }

        let ai_players: Vec<PlayerId> = self.ai_seeds.iter().map(|seed| seed.player).collect();
        let regards = |a: PlayerId, b: PlayerId| {
            let base: map_scripts::Relation = self.alliances.stance(a, b).into();
            self.script_state(tick.state(), ids).map_or(base, |state| {
                state.rt.relations.player_to_player(a, b, base)
            })
        };
        let groups = crate::skirmish_ai::enemy_groups(&ai_players, |a, b| {
            self.script_allied(tick.state(), ids, a, b)
        });
        let mut records = Vec::with_capacity(ai_players.len());
        for group in &groups {
            let Some(&lead) = group.first() else {
                continue;
            };
            for &member in group {
                let enemies = (member == lead).then(|| crate::skirmish_ai::EnemyManager {
                    candidates: (0..self.players)
                        .map(PlayerId::new)
                        .filter(|&other| {
                            other != lead && regards(lead, other) == map_scripts::Relation::Enemies
                        })
                        .collect(),
                    target: None,
                });
                records.push((member, lead, enemies));
            }
        }
        for (member, lead, enemies) in records {
            if let Some(&entity) = player_entities.get(usize::from(member.get())) {
                tick.state_mut().insert(
                    ids.ai_attack_progress,
                    entity,
                    crate::skirmish_ai::AiAttackProgress::new(
                        &self.attack_urgency_values,
                        lead,
                        enemies,
                    ),
                )?;
            }
        }

        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        let cell_count = usize::try_from(extent.cell_count()).unwrap_or(0);
        let mut obstacles = ObstacleLayer::empty(cell_count);
        let mut scaleable_walls = ScaleableWalls::empty();
        let mut wall_layer = crate::wall_layer::WallLayer::empty(cell_count);
        let mut pending_ramps: Vec<(Entity, Vec<crate::wall_layer::Ramp>, Vec<Cell>)> = Vec::new();
        let mut spawned: Vec<Entity> = Vec::with_capacity(self.spawns.len());
        for (index, spawn) in self.spawns.iter().enumerate() {
            let entity = tick.state_mut().spawn()?;
            spawned.push(entity);
            Self::insert_spawn(tick, ids, entity, spawn, &self.combat)?;
            if let Some((_, keys)) = self.placed_keys.iter().find(|(at, _)| *at == index) {
                Self::place_events_list(tick, ids, entity, keys)?;
            }
            self.attach_upgrade_state(tick, ids, entity, spawn.kind)?;
            if let Some(&(_, placement)) = self.map_placements.iter().find(|(at, _)| *at == index) {
                tick.state_mut().insert(
                    ids.map_placement,
                    entity,
                    crate::unit::MapPlacement(placement),
                )?;
            }
            if self.indestructible.contains(&index) {
                Self::set_indestructible_with_contents(tick, ids, entity, true, 0)?;
            }
            if let Some((_, name)) = self.script_object_names.iter().find(|(at, _)| *at == index) {
                tick.state_mut().insert(
                    ids.script_name,
                    entity,
                    map_scripts::ScriptObjectName(name.clone()),
                )?;
            }
            if let Some((_, records)) = self
                .structure_footprints
                .iter()
                .find(|(at, _)| *at == index)
            {
                let ground = self
                    .passability
                    .heights
                    .height_at_point(spawn.position)
                    .unwrap_or(0.0);
                let bounds = spawn.kind.and_then(|kind| self.wall_bounds_of(kind));
                if let Some(bounds) = bounds {
                    let mut polygon = bounds.placed(spawn.position, spawn.facing.radians());
                    polygon.height += ground;
                    wall_layer.stamp(geometry, extent, entity, polygon, obstacles.counts());
                    println!(
                        "walls: setup stamped entity {} into the wall layer at ({:.0}, {:.0})",
                        entity.index(),
                        spawn.position.x,
                        spawn.position.y
                    );
                } else if spawn
                    .kind
                    .is_some_and(|kind| self.is_walk_on_top_of_wall(kind))
                    && !spawn
                        .kind
                        .is_some_and(|kind| self.has_raised_wall_mesh_of(kind))
                {
                } else if let Err(message) = obstacles.stamp(
                    &self.passability.grid,
                    geometry,
                    extent,
                    entity,
                    spawn.position,
                    spawn.facing.radians(),
                    records.clone(),
                    spawn
                        .kind
                        .is_some_and(|kind| self.scaleable_wall_geometry_of(kind).is_some()),
                ) {
                    println!("obstacles: {message}");
                } else if let Some((major_radius, minor_radius, height)) = spawn
                    .kind
                    .and_then(|kind| self.scaleable_wall_geometry_of(kind))
                {
                    scaleable_walls.insert(
                        entity,
                        mint_sage_passability::WallGeometry {
                            id: 0,
                            centre: spawn.position,
                            angle: spawn.facing.radians(),
                            major_radius,
                            minor_radius,
                            height,
                        },
                    );
                }
                let ramps = spawn.kind.map_or(&[][..], |kind| self.ramp_meshes_of(kind));
                if !ramps.is_empty() {
                    let placed: Vec<crate::wall_layer::Ramp> = ramps
                        .iter()
                        .map(|ramp_mesh| {
                            ramp_mesh.placed(spawn.position, spawn.facing.radians(), ground)
                        })
                        .collect();
                    let released = mint_sage_passability::covered_cells(
                        geometry,
                        extent,
                        &Obstacle {
                            id: 0,
                            centre: spawn.position,
                            angle: spawn.facing.radians(),
                            records: records.clone(),
                        },
                    );
                    pending_ramps.push((entity, placed, released));
                }
            }
        }
        for (entity, placed, released) in pending_ramps {
            for message in wall_layer.stamp_resolved_ramps(
                geometry,
                extent,
                entity,
                &self.passability.heights,
                &placed,
                &released,
            ) {
                println!("{message}");
            }
        }
        self.rebuild_reachability(&wall_layer, obstacles.scaleable());
        if !self.script_object_names.is_empty() || !self.placed_hordes.is_empty() {
            if let Some(state) = tick.state_mut().get_mut(ids.map_script_state, clock_entity) {
                let placed = self.placed_hordes.iter().filter_map(|h| h.name.as_ref());
                for name in self
                    .script_object_names
                    .iter()
                    .map(|(_, n)| n)
                    .chain(placed)
                {
                    state.ever_existed.insert(name.clone());
                }
            }
        }
        let mut keyed: Vec<(Entity, crate::map_keys::ObjectKeys)> = self
            .placed_keys
            .iter()
            .filter_map(|(index, keys)| Some((*spawned.get(*index)?, keys.clone())))
            .collect();
        let mut seats: Vec<(Entity, String)> = self
            .script_teams
            .iter()
            .filter_map(|(index, team)| Some((*spawned.get(*index)?, team.clone())))
            .collect();
        tick.state_mut()
            .insert(ids.obstacles, clock_entity, obstacles)?;
        tick.state_mut()
            .insert(ids.scaleable_walls, clock_entity, scaleable_walls)?;
        tick.state_mut()
            .insert(ids.wall_layer, clock_entity, wall_layer)?;
        tick.state_mut().insert(
            ids.goal_cells,
            clock_entity,
            crate::goal_cells::GoalCells::empty(extent),
        )?;
        tick.state_mut().insert(
            ids.portals,
            clock_entity,
            crate::portals::PortalLayer::empty(cell_count),
        )?;
        if let Some(grid) = self.claim_grid() {
            tick.state_mut().insert(
                ids.claims,
                clock_entity,
                crate::claims::ClaimLayer::empty(&grid),
            )?;
        }
        for &(member, castle) in &self.castle_members {
            let (Some(&member), Some(&castle)) = (spawned.get(member), spawned.get(castle)) else {
                continue;
            };
            let Some(&castle_kind) = tick.state().get(ids.kind, castle) else {
                continue;
            };
            tick.state_mut().insert(
                ids.castle_member,
                member,
                crate::castle::CastleMember {
                    castle,
                    castle_kind,
                },
            )?;
        }
        self.link_capture_flags(tick, ids)?;
        for spawn in &self.hordes {
            let Some(horde) = self.spawn_horde_entity(tick, ids, spawn)? else {
                continue;
            };
            if let Some(horde_kind) = self.horde_kind_of(spawn) {
                tick.state_mut().insert(ids.kind, horde, horde_kind)?;
                self.attach_horde_declared_modules(tick, ids, horde, horde_kind)?;
            }
        }
        let (mut placed_count, mut placed_members) = (0usize, 0usize);
        for placed in &self.placed_hordes {
            let Some(horde) = self.spawn_horde_entity(tick, ids, &placed.spawn)? else {
                continue;
            };
            placed_count = placed_count.saturating_add(1);
            placed_members = placed_members.saturating_add(
                tick.state()
                    .get(ids.horde, horde)
                    .map_or(0, |horde| horde.members.len()),
            );
            tick.state_mut().insert(ids.kind, horde, placed.kind)?;
            Self::place_events_list(tick, ids, horde, &placed.keys)?;
            self.attach_upgrade_state(tick, ids, horde, Some(placed.kind))?;
            if placed.indestructible {
                Self::set_indestructible_with_contents(tick, ids, horde, true, 0)?;
            }
            if let Some(at) = placed.placement {
                tick.state_mut()
                    .insert(ids.map_placement, horde, crate::unit::MapPlacement(at))?;
            }
            if let Some(name) = &placed.name {
                tick.state_mut().insert(
                    ids.script_name,
                    horde,
                    map_scripts::ScriptObjectName(name.clone()),
                )?;
            }
            if let Some(team) = &placed.team {
                seats.push((horde, team.clone()));
            }
            if !placed.keys.is_empty() {
                keyed.push((horde, placed.keys.clone()));
            }
        }
        if !self.placed_hordes.is_empty() {
            println!(
                "hordes: {placed_count} of {} map-placed horde(s) created with {placed_members} member(s)",
                self.placed_hordes.len()
            );
        }
        self.seat_placed_team_members(tick, ids, clock_entity, &seats);
        self.apply_placed_keys(tick, ids, clock_entity, &keyed)?;
        let overrides = self.player_overrides(tick.state(), ids);
        let neutral_to = |a: PlayerId, b: PlayerId| {
            overrides.player_to_player(a, b, self.alliances.stance(a, b).into())
                == map_scripts::Relation::Neutral
        };
        crate::shroud::advance_visibility(tick, Self::shroud_ids(ids), &neutral_to, &self.sight())?;
        self.stand_on_terrain(tick, ids)?;
        self.ids = Some(ids);
        Ok(())
    }

    fn apply(&self, tick: &mut Tick<'_>, key: CommandKey, command: &Self::Command) -> Result<()> {
        if !crate::perf::enabled() {
            return self.apply_command(tick, key, command);
        }
        let started = std::time::Instant::now();
        let result = self.apply_command(tick, key, command);
        crate::perf::note_apply(started.elapsed());
        result
    }

    fn step(&self, tick: &mut Tick<'_>) -> Result<()> {
        let Some(ids) = self.ids else { return Ok(()) };
        self.unit_index.borrow_mut().begin();
        let result = if crate::perf::enabled() {
            let mut profile = crate::perf::TickProfile::new(tick.frame().get());
            let started = std::time::Instant::now();
            let result = self.step_rules(tick, ids, Some(&mut profile));
            profile.finish(started.elapsed());
            result
        } else {
            self.step_rules(tick, ids, None)
        };
        self.unit_index.borrow_mut().end();
        result
    }
}

impl RotwkRules {
    fn apply_command(
        &self,
        tick: &mut Tick<'_>,
        key: CommandKey,
        command: &RotwkCommand,
    ) -> Result<()> {
        let Some(ids) = self.ids else { return Ok(()) };
        match command {
            RotwkCommand::SetSelection { replace, units } => {
                self.set_selection(tick, ids, key.player, *replace, units)
            }
            RotwkCommand::ClearSelection => self.clear_selection(tick, ids, key.player),
            RotwkCommand::MoveTo { x, y, queue } => self.issue_order(
                tick,
                ids,
                key.player,
                busy::HeldOrder::MoveTo(
                    self.clamp_move_goal(tick.state(), ids, key.player, Vec2::new(*x, *y)),
                    *queue,
                ),
            ),
            RotwkCommand::AttackObject { target } => self.issue_order(
                tick,
                ids,
                key.player,
                busy::HeldOrder::AttackObject(*target),
            ),
            RotwkCommand::AttackMove { x, y, queue } => self.issue_order(
                tick,
                ids,
                key.player,
                busy::HeldOrder::AttackMove(
                    self.clamp_move_goal(tick.state(), ids, key.player, Vec2::new(*x, *y)),
                    *queue,
                ),
            ),
            RotwkCommand::ForceAttackObject { target } => self.issue_order(
                tick,
                ids,
                key.player,
                busy::HeldOrder::ForceAttackObject(*target),
            ),
            RotwkCommand::Stop => self.stop(tick, ids, key.player),
            RotwkCommand::GateCommand { action } => {
                self.command_gate(tick, ids, key.player, *action)
            }
            RotwkCommand::ToggleHordeFormation => {
                self.toggle_horde_formation(tick, ids, key.player)
            }
            RotwkCommand::MoveToFormation {
                x,
                y,
                dx,
                dy,
                attack,
            } => {
                self.with_ui_facts(tick, ids, |facts| facts.note_formation_order(key.player));
                self.move_to_drag_formation(
                    tick,
                    ids,
                    key.player,
                    Vec2::new(*x, *y),
                    Vec2::new(*dx, *dy),
                    *attack,
                )
            }
            RotwkCommand::SetUiPanelOpen { panel, open } => {
                self.with_ui_facts(tick, ids, |facts| {
                    facts.set_panel(key.player, *panel, *open);
                });
                Ok(())
            }
            RotwkCommand::CameraScrolled { distance } => {
                self.with_ui_facts(tick, ids, |facts| facts.scrolled(key.player, *distance));
                Ok(())
            }
            RotwkCommand::CameraReset => {
                self.with_ui_facts(tick, ids, |facts| facts.camera_reset(key.player));
                Ok(())
            }
            RotwkCommand::AttackGround { slot, x, y } => self.issue_order(
                tick,
                ids,
                key.player,
                busy::HeldOrder::AttackGround {
                    slot: *slot,
                    at: Vec2::new(*x, *y),
                },
            ),
            RotwkCommand::WakeAutoPickUp { option } => {
                self.wake_auto_pick_up(tick, ids, key.player, *option)
            }
            RotwkCommand::SetMovementModes(modes) => {
                let was = self.modes(tick, ids).formation;
                let entities: Vec<Entity> = tick
                    .state()
                    .store(ids.modes)
                    .map(|store| store.entities().to_vec())
                    .unwrap_or_default();
                for entity in entities {
                    tick.state_mut().insert(ids.modes, entity, *modes)?;
                }
                if was == FormationMode::Off && modes.formation == FormationMode::On {
                    crate::horde::reassign_slots(tick, Self::horde_ids(ids))?;
                }
                Ok(())
            }
            RotwkCommand::QueueUnit { button } => {
                let selected: Vec<Entity> = self
                    .selection(tick, ids, key.player)
                    .into_iter()
                    .filter(|entity| !tick.state().contains(ids.doomed, *entity))
                    .collect();
                let gate = |world: &World, structure: Entity, menu: u16, button: u8| {
                    self.needs_gate(world, ids, structure, &self.menu_needs, menu, button)
                };
                for line in crate::economy::queue_at(
                    tick,
                    Self::economy_ids(ids),
                    key.player,
                    &selected,
                    *button,
                    crate::economy::QueueRules {
                        menus: &self.menus,
                        buildables: &self.buildables,
                        gate: &gate,
                        kinds: &self.kinds,
                        upgrade_bits: &self.upgrade_bits,
                    },
                    self.multiplay.unit_speed(self.players),
                    self.production_cost_change_percent(key.player),
                )? {
                    diag!("{line}");
                }
                Ok(())
            }
            RotwkCommand::CancelProduction => {
                let selected = self.selection(tick, ids, key.player);
                let front = |producer: &crate::economy::Producer| {
                    producer.queue.entries().first().map(|entry| entry.id)
                };
                self.cancel_production(tick, ids, key.player, &selected, &front)
            }
            RotwkCommand::CancelBuild { button } => {
                let selected = self.selection(tick, ids, key.player);
                let last = |producer: &crate::economy::Producer| {
                    let item = *self
                        .menus
                        .get(usize::from(producer.menu?))?
                        .get(usize::from(*button))?;
                    producer
                        .queue
                        .last_of(mint_sage_economy::ItemId::new(u32::from(item)))
                        .map(|entry| entry.id)
                };
                self.cancel_production(tick, ids, key.player, &selected, &last)
            }
            RotwkCommand::CancelHero { button } => self.cancel_hero(tick, ids, key.player, *button),
            RotwkCommand::ObjectUpgrade { button } => {
                let selected = self.upgrade_selection(tick, ids, key.player);
                let gate = |world: &World, structure: Entity, menu: u16, button: u8| {
                    self.upgrade_gate(world, ids, structure, false, menu, button)
                };
                for line in crate::economy::queue_upgrade_at(
                    tick,
                    Self::economy_ids(ids),
                    &self.kinds,
                    &self.upgrade_bits,
                    key.player,
                    &selected,
                    *button,
                    false,
                    &self.object_upgrade_menus,
                    &self.upgrade_buildables,
                    &gate,
                    self.production_cost_change_percent(key.player),
                )? {
                    diag!("{line}");
                }
                Ok(())
            }
            RotwkCommand::PlayerUpgrade { button } => {
                let selected = self.upgrade_selection(tick, ids, key.player);
                let gate = |world: &World, structure: Entity, menu: u16, button: u8| {
                    self.upgrade_gate(world, ids, structure, true, menu, button)
                };
                for line in crate::economy::queue_upgrade_at(
                    tick,
                    Self::economy_ids(ids),
                    &self.kinds,
                    &self.upgrade_bits,
                    key.player,
                    &selected,
                    *button,
                    true,
                    &self.player_upgrade_menus,
                    &self.upgrade_buildables,
                    &gate,
                    self.production_cost_change_percent(key.player),
                )? {
                    diag!("{line}");
                }
                Ok(())
            }
            RotwkCommand::RecruitHero { button } => {
                self.recruit_hero(tick, ids, key.player, *button)
            }
            RotwkCommand::FireSpecialPower { button, target, at } => {
                self.fire_special_power(tick, ids, key.player, *button, *target, *at)
            }
            RotwkCommand::ToggleAutoCast { button } => {
                self.toggle_auto_cast_button(tick, ids, key.player, *button)
            }
            RotwkCommand::SetRallyPoint {
                structures,
                x,
                y,
                global,
                target,
            } => self.set_rally_point(
                tick,
                ids,
                key.player,
                structures,
                Vec2::new(*x, *y),
                *global,
                *target,
            ),
            RotwkCommand::PurchaseScience { science } => {
                self.purchase_science(tick, ids, key.player, *science)
            }
            RotwkCommand::CastSpell { button, target, at } => {
                self.cast_spell(tick, ids, key.player, *button, *target, *at)
            }
            RotwkCommand::Sell => self.sell(tick, ids, key.player),
            RotwkCommand::Enter { target } => self.enter_container(tick, ids, key.player, *target),
            RotwkCommand::Exit { passenger } => {
                self.exit_container(tick, ids, key.player, *passenger)
            }
            RotwkCommand::SetStance { stance } => {
                match mint_sage_ini::Stance::from_index(*stance) {
                    Some(stance) => self.set_stance(tick, ids, key.player, stance),
                    None => Ok(()),
                }
            }
            RotwkCommand::ToggleWeaponSet { button, lead } => {
                self.toggle_weapon_set(tick, ids, key.player, button, *lead)
            }
            RotwkCommand::StartSelfRepair => self.start_self_repair(tick, ids, key.player),
            RotwkCommand::CaptureObject { target } => {
                self.capture_object(tick, ids, key.player, *target)
            }
            RotwkCommand::CancelUpgrade { button, player } => {
                self.cancel_upgrade(tick, ids, key.player, *button, *player)
            }
            RotwkCommand::DebugSkillPoints { points } => {
                self.award_skill_points(tick, ids, key.player, *points)
            }
            RotwkCommand::DebugSpellPoints { points } => {
                self.award_spell_points(tick, ids, key.player, *points)
            }
            RotwkCommand::DebugMoney { amount } => {
                let economy = Self::economy_ids(ids);
                let Some(player) = crate::economy::player_entity(tick.state(), economy, key.player)
                else {
                    return Ok(());
                };
                let mut purse = crate::economy::purse_of(tick.state(), economy, key.player);
                purse.deposit(i64::from(*amount));
                tick.state_mut()
                    .insert(ids.purse, player, crate::economy::Purse(purse))?;
                Ok(())
            }
            RotwkCommand::DebugExperience { points } => {
                let selected = self.selection(tick, ids, key.player);
                self.debug_experience(tick, ids, &selected, *points)
            }
            RotwkCommand::DebugKill => {
                let selected = self.selection(tick, ids, key.player);
                let mut killed: Vec<(Entity, f32)> = Vec::new();
                for entity in selected {
                    let Some(mut health) = tick.state().get(ids.health, entity).copied() else {
                        continue;
                    };
                    health.0.set_current(0.0);
                    tick.state_mut().insert(ids.health, entity, health)?;
                    killed.push((entity, 0.0));
                }
                self.resolve_deaths(tick, ids, &killed)
            }
            RotwkCommand::DebugOath => {
                let selected = self.selection(tick, ids, key.player);
                let mut hordes: Vec<Entity> = selected
                    .into_iter()
                    .filter_map(|entity| tick.state().get(ids.member, entity).map(|m| m.horde))
                    .collect();
                hordes.sort_unstable_by_key(|horde| horde.index());
                hordes.dedup();
                let effects: Vec<(Entity, mint_sage_modules::Effect)> = hordes
                    .into_iter()
                    .map(|horde| {
                        (
                            horde,
                            mint_sage_modules::Effect::ModelConditions {
                                add: mint_sage_modules::effects::names(vec![
                                    "OATH_FULLFILLED".to_owned()
                                ]),
                                remove: mint_sage_modules::effects::names(Vec::new()),
                            },
                        )
                    })
                    .collect();
                self.apply_effects(tick, ids, effects)
            }
            RotwkCommand::ResetShroud { use_shroud } => self.reset_shroud(tick, ids, *use_shroud),
            RotwkCommand::Construct { kind, x, y, angle } => self.construct(
                tick,
                ids,
                key.player,
                crate::unit::Kind(*kind),
                *x,
                *y,
                *angle,
            ),
            RotwkCommand::WallSpan { option, x, y } => {
                self.wall_span(tick, ids, key.player, *option, *x, *y)
            }
            RotwkCommand::CancelConstruction => self.cancel_construction(tick, ids, key.player),
            RotwkCommand::FoundationConstruct { button } => {
                self.foundation_construct(tick, ids, key.player, *button)
            }
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x,
                y,
                facing,
            } => {
                let Some(template) = self.buildables.get(usize::from(*recipe)) else {
                    return Ok(());
                };
                let crate::economy::Output::Horde {
                    formation,
                    member,
                    count,
                    vision,
                } = &template.output
                else {
                    return Ok(());
                };
                let spawn = HordeSpawn {
                    formation: *formation,
                    origin: Vec2::new(*x, *y),
                    facing: mint_movement::Facing::from_radians(*facing),
                    member: Spawn {
                        owner: key.player,
                        ..*member
                    },
                    count: *count,
                    vision: *vision,
                };
                let horde = self.spawn_horde_entity(tick, ids, &spawn)?;
                if let Some(horde) = horde {
                    tick.state_mut().insert(ids.kind, horde, template.kind)?;
                    self.attach_upgrade_state(tick, ids, horde, Some(template.kind))?;
                }
                Ok(())
            }
            RotwkCommand::DebugSpawnUnit {
                buildable,
                x,
                y,
                facing,
            } => {
                let Some(template) = self.buildable_at(*buildable) else {
                    return Ok(());
                };
                let crate::economy::Output::Unit(spawn) = &template.output else {
                    return Ok(());
                };
                let entity = tick.state_mut().spawn()?;
                Self::insert_spawn(
                    tick,
                    ids,
                    entity,
                    &Spawn {
                        position: Vec2::new(*x, *y),
                        facing: mint_movement::Facing::from_radians(*facing),
                        owner: key.player,
                        ..*spawn
                    },
                    &self.combat,
                )?;
                self.attach_upgrade_state(tick, ids, entity, spawn.kind)
            }
            RotwkCommand::Debug(command) => self.apply_debug(tick, ids, key.player, command),
            RotwkCommand::CahPreview(command) => self.apply_cah_preview(tick, ids, command),
            RotwkCommand::DebugSpawnStructure { kind, x, y, angle } => self.spawn_structure(
                tick,
                ids,
                key.player,
                crate::unit::Kind(*kind),
                *x,
                *y,
                *angle,
            ),
        }
    }

    fn step_rules(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        mut profile: Option<&mut crate::perf::TickProfile>,
    ) -> Result<()> {
        macro_rules! rule {
            ($call:expr) => {
                $call.map_err(|error| mint_sim::Error::in_rule(stringify!($call), error))
            };
        }
        macro_rules! pass {
            ($row:literal, $body:expr) => {{
                if let Some(profile) = profile.as_deref_mut() {
                    let started = std::time::Instant::now();
                    let result = $body;
                    profile.record(crate::perf::row!($row), started.elapsed());
                    result
                } else {
                    $body
                }
            }};
        }

        let clock_entity = pass!("clock", {
            let entities: Vec<Entity> = tick
                .state()
                .store(ids.clock)
                .map(|store| store.entities().to_vec())
                .unwrap_or_default();
            let clock_entity = entities.first().copied();
            for entity in entities {
                if let Some(clock) = tick.state_mut().get_mut(ids.clock, entity) {
                    clock.0 = clock.0.saturating_add(1);
                }
            }
            clock_entity
        });
        let mut map_script_kills = pass!(
            "step_map_scripts",
            rule!(self.step_map_scripts(tick, ids, clock_entity))
        )?;
        map_script_kills.extend(pass!(
            "step_team_builders",
            rule!(self.step_team_builders(tick, ids, clock_entity))
        )?);
        pass!(
            "replay_deferred_orders",
            rule!(self.replay_deferred_orders(tick, ids))
        )?;
        pass!("ai", rule!(self.step_skirmish_ai(tick, ids)))?;

        let mut budgets = pass!("path_budgets", {
            let head = rule!(self.pop_path_queue_head(tick, ids, clock_entity))?;
            let deferred = Self::deferred_path_requests(tick, ids, tick.frame());
            PathBudgets::new(head, deferred)
        });
        pass!("gates", rule!(self.run_gates(tick, ids)))?;
        pass!("run_burning", rule!(self.run_burning(tick, ids)))?;
        pass!(
            "check_builder_arrivals",
            rule!(self.check_builder_arrivals(tick, ids))
        )?;
        pass!(
            "movers",
            rule!(self.advance_movers(tick, ids, &mut budgets))
        )?;
        pass!(
            "coast_while_waiting",
            rule!(self.coast_while_waiting(tick, ids))
        )?;
        pass!("hold_still", rule!(self.hold_still(tick, ids)))?;
        pass!(
            "run_enemy_contact",
            rule!(self.run_enemy_contact(tick, ids, &mut budgets))
        )?;
        pass!("run_cell_changes", rule!(self.run_cell_changes(tick, ids)))?;
        pass!("fly_giant_birds", rule!(self.fly_giant_birds(tick, ids)))?;
        pass!(
            "contain",
            rule!(self.advance_contain(tick, ids, &mut budgets))
        )?;

        let now = tick.frame();
        let (living, grid, global_max_radius) = pass!("grid", {
            let living = self.living(tick, ids);
            let grid = TargetGrid::build(&living);
            let global_max_radius = living
                .iter()
                .map(|(foe, _)| foe.radius)
                .fold(0.0f32, f32::max);
            (living, grid, global_max_radius)
        });
        pass!("members", rule!(self.advance_members(tick, ids)))?;

        pass!(
            "advance_knockback",
            rule!(self.advance_knockback(tick, ids))
        )?;

        let overrides = pass!("player_overrides", self.player_overrides(tick.state(), ids));
        let neutral_to = |a: PlayerId, b: PlayerId| {
            overrides.player_to_player(a, b, self.alliances.stance(a, b).into())
                == map_scripts::Relation::Neutral
        };
        pass!(
            "visibility",
            rule!(crate::shroud::advance_visibility(
                tick,
                Self::shroud_ids(ids),
                &neutral_to,
                &self.sight()
            ))
        )?;
        pass!(
            "advance_stealth",
            rule!(self.advance_stealth(tick, ids, &living, &grid))
        )?;
        pass!(
            "advance_detectors",
            rule!(self.advance_detectors(tick, ids, &living, &grid))
        )?;
        pass!(
            "advance_pick_ups",
            rule!(self.advance_pick_ups(tick, ids, &living))
        )?;
        pass!(
            "advance_attach",
            rule!(self.advance_attach(tick, ids, &living))
        )?;

        let mut events: Vec<DamageEvent> = Vec::new();
        pass!(
            "advance_guards",
            rule!(self.advance_guards(
                tick,
                ids,
                &mut budgets,
                &grid,
                &living,
                global_max_radius,
                now,
            ))
        )?;
        pass!(
            "projectiles",
            rule!(self.advance_projectiles(tick, ids, &mut events))
        )?;
        pass!(
            "weapons",
            rule!(self.run_weapons(
                tick,
                ids,
                &mut events,
                &mut budgets,
                &living,
                &grid,
                global_max_radius,
                now,
            ))
        )?;
        pass!(
            "crush",
            rule!(self.run_crush_pass(tick, ids, &mut events, &living, &grid))
        )?;
        pass!(
            "tick_rampage",
            rule!(self.tick_rampage(tick, ids, &mut events, &living))
        )?;
        pass!(
            "run_topple_pass",
            rule!(self.run_topple_pass(tick, ids, &living))
        )?;
        pass!(
            "run_aod_crush_pass",
            rule!(self.run_aod_crush_pass(tick, ids, &mut events, &living, &grid))
        )?;
        let mut destroyed = pass!(
            "modules",
            rule!(self.step_modules(tick, ids, &mut events, &living, &grid, &mut budgets))
        )?;
        pass!(
            "run_crate_pickups",
            rule!(self.run_crate_pickups(
                tick,
                ids,
                &living,
                &grid,
                global_max_radius,
                &mut destroyed
            ))
        )?;
        pass!(
            "step_terrain_fire",
            rule!(self.step_terrain_fire(tick, ids, &mut events, &living))
        )?;
        pass!(
            "step_crush_notices",
            rule!(self.step_crush_notices(tick, ids, &living, &grid))
        )?;
        pass!(
            "step_emotions",
            rule!(self.step_emotions(tick, ids, &living, &grid))
        )?;
        pass!(
            "step_animals",
            rule!(self.step_animals(tick, ids, &living, &grid))
        )?;
        pass!("step_amphibious", rule!(self.step_amphibious(tick, ids)))?;
        pass!("step_siege", rule!(self.step_siege(tick, ids)))?;
        pass!(
            "step_group_bonuses",
            rule!(self.step_group_bonuses(tick, ids, &living, &grid, global_max_radius))
        )?;
        pass!(
            "settle_path_requests",
            rule!(self.settle_path_requests(tick, ids, &budgets))
        )?;
        let killed = pass!("damage", rule!(self.apply_damage(tick, ids, &mut events)))?;
        pass!(
            "tick_fire",
            rule!(self.tick_fire(tick, ids, &living, &grid))
        )?;
        pass!(
            "note_attackers",
            rule!(self.note_attackers(tick, ids, &events))
        )?;
        pass!("step_engaged", rule!(self.step_engaged(tick, ids)))?;
        pass!(
            "step_hit_reactions",
            rule!(self.step_hit_reactions(tick, ids))
        )?;
        pass!("deaths", {
            let mut killed = rule!(self.begin_burning_deaths(tick, ids, killed))?;
            killed.extend(rule!(self.expire_lifetimes(tick, ids))?);
            killed.extend(rule!(self.fade_released_workers(tick, ids))?);
            killed.extend(self.end_burning_deaths(tick, ids));
            killed.extend(map_script_kills);
            rule!(self.resolve_deaths(tick, ids, &killed))
        })?;
        pass!("victory", {
            if self.victory_enabled {
                if let Some(clock_entity) = clock_entity {
                    rule!(crate::defeat::advance_victory(
                        tick,
                        Self::defeat_ids(ids),
                        clock_entity,
                        &self.kinds,
                        &self.victory_structure_filter,
                        self.players,
                        &self.alliances,
                    ))?;
                }
            }
            Ok::<(), mint_sim::Error>(())
        })?;
        pass!("corpses", {
            rule!(self.run_death_phase_updates(tick, ids))?;
            rule!(self.remove_corpses(tick, ids, &destroyed))?;
            crate::portals::reap_dead_portals(
                tick.state_mut(),
                ids.portals,
                self.passability.geometry,
                self.passability.grid.extent(),
            );
            rule!(crate::obstacles::reap_dead_stamps(
                tick,
                ids.obstacles,
                &self.passability.grid,
                self.passability.geometry,
                self.passability.grid.extent(),
            ))?;
            rule!(crate::wall_layer::reap_dead_stamps(
                tick,
                ids.wall_layer,
                self.passability.geometry,
                self.passability.grid.extent(),
            ))?;
            crate::goal_cells::reap_dead_claims(tick, ids.goal_cells, ids.health);
            rule!(self.settle_claims(tick, ids))
        })?;
        pass!("hordes", {
            let retired = rule!(crate::horde::prune_hordes(
                tick,
                Self::horde_ids(ids),
                &self.formations
            ))?;
            if !retired.is_empty() {
                let now = tick.frame();
                if let Some((_, facts)) = tick
                    .state_mut()
                    .store_mut(ids.combat_facts)
                    .and_then(|store| store.iter_mut().next())
                {
                    for last in retired.iter().filter_map(|(_, members)| members.first()) {
                        facts.push_last_member_death(now, *last);
                    }
                }
                map_scripts::hand_retired_hordes_to_soldiers(tick, ids, &retired);
            }
            rule!(self.respawn_banners(tick, ids))
        })?;
        pass!("construction", {
            rule!(self.advance_rebuild_holes(tick, ids))?;
            rule!(self.tend_site_workers(tick, ids))?;
            rule!(self.tend_auto_repair(tick, ids))?;
            rule!(self.advance_construction(tick, ids))
        })?;

        pass!(
            "advance_harvesting",
            rule!(self.advance_harvesting(tick, ids, &living))
        )?;
        pass!(
            "advance_tree_motion",
            rule!(self.advance_tree_motion(tick, ids))
        )?;
        pass!(
            "cancel_unaffected_object_upgrades",
            rule!(self.cancel_unaffected_object_upgrades(tick, ids))
        )?;
        pass!("economy", rule!(self.advance_economy(tick, ids)))?;
        pass!(
            "advance_match_stats",
            rule!(self.advance_match_stats(tick, ids))
        )?;
        let result = pass!("modifiers", {
            rule!(self.expire_timed_modifiers(tick, ids))?;
            rule!(self.expire_timed_conditions(tick, ids))?;
            rule!(self.expire_timed_holds(tick, ids))?;
            rule!(self.expire_spell_state(tick, ids))?;
            rule!(self.expire_weather(tick, ids))?;
            rule!(self.expire_defects(tick, ids))?;
            rule!(self.pulse_modifier_heals(tick, ids))
        });
        pass!("stand_on_terrain", rule!(self.stand_on_terrain(tick, ids)))?;
        result
    }
}

#[cfg(test)]
mod tests;

fn map_entry(high: Vec2, from: Vec2, to: Vec2) -> Option<Vec2> {
    const INSET: f32 = 0.05;
    let inside = |p: Vec2| p.x > 0.0 && p.y > 0.0 && p.x < high.x && p.y < high.y;
    if inside(from) {
        return None;
    }
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let (mut enter, mut leave) = (0.0f32, 1.0f32);
    for (delta, start, low, top) in [(dx, from.x, 0.0, high.x), (dy, from.y, 0.0, high.y)] {
        if delta == 0.0 {
            if start <= low || start >= top {
                return None;
            }
            continue;
        }
        let (a, b) = ((low - start) / delta, (top - start) / delta);
        let (near, far) = if a < b { (a, b) } else { (b, a) };
        enter = enter.max(near);
        leave = leave.min(far);
    }
    if enter > leave {
        return None;
    }
    let at = Vec2::new(from.x + dx * enter, from.y + dy * enter);
    Some(Vec2::new(
        at.x.clamp(INSET, (high.x - INSET).max(INSET)),
        at.y.clamp(INSET, (high.y - INSET).max(INSET)),
    ))
}

#[derive(Debug, Clone)]
pub(crate) struct PlacedHorde {
    pub(crate) spawn: HordeSpawn,
    pub(crate) kind: crate::unit::Kind,
    pub(crate) placement: Option<u32>,
    pub(crate) name: Option<String>,
    pub(crate) team: Option<String>,
    pub(crate) indestructible: bool,
    pub(crate) keys: crate::map_keys::ObjectKeys,
}
