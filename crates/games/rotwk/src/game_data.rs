use std::sync::Arc;
use std::time::Instant;

use mint_app::Key;
use mint_sage_combat::CombatData;
use mint_sage_formation::FormationTable;
use mint_sage_ini::{included_paths, BlockTable, MacroTable, MemorySources, Reader, Sources};
use mint_sage_modules::object_scripts::ObjectScripts;
use mint_vfs::Vfs;

const INI_ROOT: &str = "data/ini/";

const LANGUAGE_COMMAND_MAP: &str = "commandmap.ini";

mod map_ini;
pub(crate) use map_ini::compose_map_ini;

pub(crate) fn compose_for_match(
    data: &mut GameData,
    vfs: &Vfs,
    map_path: &str,
) -> Result<(), String> {
    match compose_map_ini(data, vfs, map_path) {
        Ok(Some(line)) => {
            println!("{line}");
            Ok(())
        }
        Ok(None) => Ok(()),
        Err(refusal) => {
            refusal.report();
            Err(format!("{map_path}: map.ini refused"))
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct GameData {
    pub(crate) combat: CombatData,
    pub(crate) formations: FormationLibrary,
    pub(crate) templates: Arc<mint_sage_modules::TemplateRecords>,
    pub(crate) command_sets: Arc<mint_sage_ini::CommandSetIndex>,
    pub(crate) command_buttons: Arc<mint_sage_ini::CommandButtonIndex>,
    pub(crate) mapped_images: Arc<mint_sage_ini::MappedImageIndex>,
    pub(crate) players: Arc<mint_sage_ini::PlayerTemplateIndex>,
    pub(crate) footprints: Arc<mint_sage_passability::TemplateIndex>,
    pub(crate) water_depths: mint_sage_passability::WaterDepths,
    pub(crate) victory_structure_filter: crate::defeat::ObjectFilter,
    pub(crate) victory_unit_filter: crate::defeat::ObjectFilter,
    pub(crate) construction_cancel_refund_percent: f32,
    pub(crate) max_cells_adjust_horde_melee: u32,
    pub(crate) max_cells_adjust_destination: u32,
    pub(crate) max_cells_adjust_to_possible_destination: u32,
    pub(crate) melee_placement: crate::rules::MeleePlacement,
    pub(crate) group_formation: crate::rules::GroupFormation,
    pub(crate) drag_formation: Arc<mint_sage_formation::FormationAssistant>,
    pub(crate) castle_siege_stand_back_distance: f32,
    pub(crate) move_hint_name: String,
    pub(crate) shell_map: crate::shell_map::ShellMapData,
    pub(crate) tutorials: crate::tutorial::Tutorials,
    pub(crate) selection_flash_saturation: f32,
    pub(crate) selection_flash_house_color: bool,
    pub(crate) damage_thresholds: crate::render_state::DamageThresholds,
    pub(crate) movement_penalty_state: crate::combat::DamageRung,
    pub(crate) engaged_defaults: crate::rules::EngagedDefaults,
    pub(crate) default_voice_attack_charge_timeout_ms: Option<f32>,
    pub(crate) ai_bases: Arc<Vec<AiBaseDef>>,
    pub(crate) ai_timing: AiTiming,
    pub(crate) ai_power_activation_tuning: [(i64, i64); 4],
    pub(crate) combat_chains: Arc<crate::combat::CombatChainTable>,
    pub(crate) order_mode_key: Option<Key>,
    pub(crate) view_last_radar_event_key: Option<Key>,
    pub(crate) diplomacy_key: Option<KeyBinding>,
    pub(crate) spell_store_key: Option<KeyBinding>,
    pub(crate) guard_ranges: crate::combat::GuardRanges,
    pub(crate) attack_priorities: Arc<crate::combat::priority::AttackPriorityData>,
    pub(crate) army_definitions: Arc<mint_sage_ini::ArmyDefinitionIndex>,
    pub(crate) multiplayer_colors: Arc<mint_sage_ini::MultiplayerColorIndex>,
    pub(crate) handicaps: Arc<crate::handicap::HandicapTable>,
    pub(crate) create_a_hero: Arc<crate::create_a_hero::CahData>,
    pub(crate) rule_choices: Arc<crate::setup_rules::RuleChoices>,
    pub(crate) hero_subclasses: Arc<Vec<Vec<crate::setup_heroes::SubclassFactions>>>,
    pub(crate) cah_system: Option<Arc<crate::cah_edit::CahSystem>>,
    pub(crate) porter_targets: Arc<Vec<(String, i64)>>,
    pub(crate) attack_urgency_values: AttackUrgencyValues,
    pub(crate) object_scripts: Option<ObjectScripts>,
    pub(crate) object_index: Option<mint_sage_model::ObjectIndex>,
    pub(crate) builder_cycle_timeout_ms: u32,
    pub(crate) drawable_caption_points: f32,
    pub(crate) floating_text: crate::floating_text::FloatingTextTiming,
    pub(crate) military_caption: crate::military_caption::CaptionStyle,
    pub(crate) named_timer: crate::named_timer::TimerStyle,
    pub(crate) notification_box: Arc<mint_sage_ini::NotificationBoxDef>,
    pub(crate) camera: CameraDefaults,
    pub(crate) shake: ScreenShake,
    pub(crate) upgrades: Arc<Vec<mint_sage_modules::UpgradeRecord>>,
    pub(crate) upgrade_bits: Arc<mint_sage_modules::UpgradeBits>,
    pub(crate) radius_cursors: Arc<mint_sage_ini::RadiusCursorIndex>,
    pub(crate) special_powers: Arc<mint_sage_ini::SpecialPowerIndex>,
    pub(crate) experience: Arc<mint_sage_modules::ExperienceTables>,
    pub(crate) experience_scalars: Arc<mint_sage_modules::ExperienceScalarTables>,
    pub(crate) armor_bonus_cap: f32,
    pub(crate) sciences: Arc<mint_sage_ini::ScienceIndex>,
    pub(crate) ranks: Arc<mint_sage_ini::RankTable>,
    pub(crate) ocls: Arc<mint_sage_ini::ObjectCreationListIndex>,
    pub(crate) sell_percentage: f32,
    pub(crate) base_check_frames: u32,
    pub(crate) gravity: Option<f32>,
    pub(crate) ground_stiffness: f32,
    pub(crate) terrain_resource_cell_size: Option<f32>,
    pub(crate) supply_boxes_per_tree: f32,
    pub(crate) value_per_supply_box: f32,
    pub(crate) macros: Arc<MacroTable>,
    pub(crate) locomotors: Arc<mint_sage_ini::LocomotorIndex>,
    pub(crate) emotions: Arc<mint_sage_ini::EmotionIndex>,
    pub(crate) stances: Arc<mint_sage_ini::StanceTemplateIndex>,
    pub(crate) command_points: Option<CommandPointTable>,
    pub(crate) multiplay: MultiplayMultipliers,
    pub(crate) score_weights: ScoreWeights,
    pub(crate) objects_that_score: crate::defeat::ObjectFilter,
    pub(crate) command_points_solo: CommandPointsSolo,
    pub(crate) max_tunnel_capacity: u32,
    pub(crate) fire_logic_system: mint_sage_ini::fire_logic_system::FireLogicSystem,
    pub(crate) ini_documents: Arc<Vec<mint_sage_ini::Document>>,
    pub(crate) map_ini: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct CommandPointTable {
    good: [Option<(i32, i32)>; 7],
    evil: [Option<(i32, i32)>; 7],
}

impl CommandPointTable {
    pub(crate) fn of(&self, evil: bool, players: u8) -> Option<(i32, i32)> {
        let row = usize::from(players.clamp(2, 8)).saturating_sub(2);
        let table = if evil { &self.evil } else { &self.good };
        table.get(row).copied().flatten()
    }

    pub(crate) fn fields(&self) -> usize {
        self.good.iter().chain(&self.evil).flatten().count()
    }
}

fn read_command_point_table(block: &mint_sage_ini::Block) -> Option<CommandPointTable> {
    let mut table = CommandPointTable::default();
    for (row, players) in (2u8..=8).enumerate() {
        for (evil, side) in [(false, "Good"), (true, "Evil")] {
            let Some(field) = block.field(&format!("{side}CommandPointsMP{players}")) else {
                continue;
            };
            let mut values = field
                .args
                .iter()
                .filter_map(|arg| mint_sage_ini::value::parse_int(arg))
                .filter_map(|v| i32::try_from(v).ok());
            let (Some(start), Some(max)) = (values.next(), values.next()) else {
                continue;
            };
            let slot = if evil {
                table.evil.get_mut(row)
            } else {
                table.good.get_mut(row)
            };
            if let Some(slot) = slot {
                *slot = Some((start.max(0), max.max(0)));
            }
        }
    }
    (table.fields() > 0).then_some(table)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MultiplayMultipliers {
    money: [f32; 8],
    unit_xp: [f32; 8],
    building_xp: [f32; 8],
    unit_speed: [f32; 8],
    building_speed: [f32; 8],
}

impl Default for MultiplayMultipliers {
    fn default() -> Self {
        Self {
            money: [1.0; 8],
            unit_xp: [1.0; 8],
            building_xp: [1.0; 8],
            unit_speed: [1.0; 8],
            building_speed: [1.0; 8],
        }
    }
}

impl MultiplayMultipliers {
    fn slot(players: u8) -> usize {
        usize::from(players.clamp(1, 8)) - 1
    }

    pub(crate) fn money(&self, players: u8) -> f32 {
        self.money.get(Self::slot(players)).copied().unwrap_or(1.0)
    }

    pub(crate) fn unit_xp(&self, players: u8) -> f32 {
        self.unit_xp
            .get(Self::slot(players))
            .copied()
            .unwrap_or(1.0)
    }

    pub(crate) fn building_xp(&self, players: u8) -> f32 {
        self.building_xp
            .get(Self::slot(players))
            .copied()
            .unwrap_or(1.0)
    }

    pub(crate) fn unit_speed(&self, players: u8) -> f32 {
        self.unit_speed
            .get(Self::slot(players))
            .copied()
            .unwrap_or(1.0)
    }

    pub(crate) fn building_speed(&self, players: u8) -> f32 {
        self.building_speed
            .get(Self::slot(players))
            .copied()
            .unwrap_or(1.0)
    }
}

fn read_multiplay_multipliers(block: &mint_sage_ini::Block) -> MultiplayMultipliers {
    let mut out = MultiplayMultipliers::default();
    read_multiplay_slot_field(block, "MultiPlayMoneyMult", &mut out.money);
    read_multiplay_slot_field(block, "MultiPlayUnitXPMult", &mut out.unit_xp);
    read_multiplay_slot_field(block, "MultiPlayBuildingXPMult", &mut out.building_xp);
    read_multiplay_slot_field(block, "MultiPlayUnitSpeedMult", &mut out.unit_speed);
    read_multiplay_slot_field(block, "MultiPlayBuildingSpeedMult", &mut out.building_speed);
    out
}

fn read_multiplay_slot_field(block: &mint_sage_ini::Block, name: &str, slots: &mut [f32; 8]) {
    let Some(field) = block.field(name) else {
        return;
    };
    for (index, slot) in slots.iter_mut().enumerate() {
        let tag = format!("MP{}", index + 1);
        let Some(value) = mint_sage_ini::value::tagged_value(&field.args, &tag)
            .and_then(mint_sage_ini::value::parse_float)
        else {
            continue;
        };
        if value.is_finite() && value >= 0.0 {
            *slot = value;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct ScoreWeights {
    pub(crate) skill_points: f32,
    pub(crate) supplies_collected: f32,
    pub(crate) structures_built: f32,
    pub(crate) units_built: f32,
    pub(crate) units_destroyed: f32,
    pub(crate) structures_destroyed: f32,
    pub(crate) power_points: f32,
    pub(crate) heroes_vetted: f32,
    pub(crate) units_vetted: f32,
    pub(crate) player_eliminated: f32,
}

fn read_score_weights(block: &mint_sage_ini::Block) -> ScoreWeights {
    let read = |name: &str| {
        block
            .field(name)
            .and_then(mint_sage_ini::Field::value)
            .and_then(mint_sage_ini::value::parse_float)
            .filter(|value| value.is_finite())
            .unwrap_or(0.0)
    };
    ScoreWeights {
        skill_points: read("ScoreKeeper_SkillPointsMultiplier"),
        supplies_collected: read("ScoreKeeper_SuppliesCollectedMultiplier"),
        structures_built: read("ScoreKeeper_StructuresBuiltMultiplier"),
        units_built: read("ScoreKeeper_UnitsBuiltMultiplier"),
        units_destroyed: read("ScoreKeeper_UnitsDestroyedMultiplier"),
        structures_destroyed: read("ScoreKeeper_StructuresDestroyedMultiplier"),
        power_points: read("ScoreKeeper_PowerPointsMultiplier"),
        heroes_vetted: read("ScoreKeeper_HeroesVettedMultiplier"),
        units_vetted: read("ScoreKeeper_UnitsVettedMultiplier"),
        player_eliminated: read("ScoreKeeper_PlayerEliminatedMultiplier"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct CommandPointsSolo {
    pub(crate) good: Option<(i32, i32)>,
    pub(crate) evil: Option<(i32, i32)>,
    pub(crate) good_ai: Option<(i32, i32)>,
    pub(crate) evil_ai: Option<(i32, i32)>,
    pub(crate) good_bonus: Option<i32>,
    pub(crate) evil_bonus: Option<i32>,
    pub(crate) good_limit: Option<i32>,
    pub(crate) evil_limit: Option<i32>,
}

impl CommandPointsSolo {
    pub(crate) fn fields(&self) -> usize {
        [
            self.good.is_some(),
            self.evil.is_some(),
            self.good_ai.is_some(),
            self.evil_ai.is_some(),
            self.good_bonus.is_some(),
            self.evil_bonus.is_some(),
            self.good_limit.is_some(),
            self.evil_limit.is_some(),
        ]
        .into_iter()
        .filter(|read| *read)
        .count()
    }
}

fn read_command_points_solo(block: &mint_sage_ini::Block) -> CommandPointsSolo {
    let pair = |name: &str| -> Option<(i32, i32)> {
        let field = block.field(name)?;
        let mut values = field
            .args
            .iter()
            .filter_map(|arg| mint_sage_ini::value::parse_int(arg))
            .filter_map(|v| i32::try_from(v).ok());
        let (start, max) = (values.next()?, values.next()?);
        Some((start.max(0), max.max(0)))
    };
    let single = |name: &str| -> Option<i32> {
        block
            .field(name)
            .and_then(mint_sage_ini::Field::value)
            .and_then(mint_sage_ini::value::parse_int)
            .and_then(|v| i32::try_from(v).ok())
    };
    CommandPointsSolo {
        good: pair("GoodCommandPoints"),
        evil: pair("EvilCommandPoints"),
        good_ai: pair("GoodCommandPointsAI"),
        evil_ai: pair("EvilCommandPointsAI"),
        good_bonus: single("GoodCommandPointsBonus"),
        evil_bonus: single("EvilCommandPointsBonus"),
        good_limit: single("GoodCommandPointLimit"),
        evil_limit: single("EvilCommandPointLimit"),
    }
}

pub(crate) const SHIPPED_SELL_PERCENTAGE: f32 = 0.5;

pub(crate) const DEFAULT_GRAVITY: f32 = -1.0;

pub(crate) const DEFAULT_GROUND_STIFFNESS: f32 = 0.5;

pub(crate) const DEFAULT_SUPPLY_BOXES_PER_TREE: f32 = 90.0;

pub(crate) const DEFAULT_VALUE_PER_SUPPLY_BOX: f32 = 10.0;

fn read_sell_percentage(block: &mint_sage_ini::Block) -> Option<f32> {
    let raw = block
        .field("SellPercentage")
        .and_then(mint_sage_ini::Field::value)?;
    mint_sage_ini::value::parse_percent(raw)
        .or_else(|| mint_sage_ini::value::parse_float(raw))
        .filter(|v| v.is_finite() && (0.0..=10.0).contains(v))
}

fn read_refund_percent_field(block: &mint_sage_ini::Block) -> Option<f32> {
    let raw = block
        .field("RefundPercent")
        .and_then(mint_sage_ini::Field::value)?;
    mint_sage_ini::value::parse_percent(raw)
        .or_else(|| mint_sage_ini::value::parse_float(raw))
        .filter(|v| v.is_finite() && (0.0..=10.0).contains(v))
}

pub(crate) const SHIPPED_ARMOR_BONUS_CAP: f32 = 0.75;

fn read_armor_bonus_cap(block: &mint_sage_ini::Block) -> Option<f32> {
    let raw = block
        .field("AttributeModifierArmorMaxBonus")
        .and_then(mint_sage_ini::Field::value)?;
    mint_sage_ini::value::parse_percent(raw)
        .or_else(|| mint_sage_ini::value::parse_float(raw))
        .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CameraDefaults {
    pub(crate) pitch_degrees: f32,
    pub(crate) min_height: f32,
    pub(crate) max_height: f32,
}

impl CameraDefaults {
    pub(crate) const SHIPPED: Self = Self {
        pitch_degrees: 37.5,
        min_height: 120.0,
        max_height: 300.0,
    };
}

fn read_camera_defaults(block: &mint_sage_ini::Block) -> CameraDefaults {
    let mut out = CameraDefaults::SHIPPED;
    let real = |name: &str| {
        block
            .field(name)
            .and_then(mint_sage_ini::Field::value)
            .and_then(mint_sage_ini::value::parse_float)
            .filter(|v| v.is_finite() && *v > 0.0)
    };
    if let Some(v) = real("DefaultCameraPitchAngle") {
        out.pitch_degrees = v;
    }
    if let Some(v) = real("DefaultCameraMinHeight") {
        out.min_height = v;
    }
    if let Some(v) = real("DefaultCameraMaxHeight") {
        out.max_height = v;
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct ScreenShake {
    intensities: [f32; 6],
    max_intensity: f32,
    pub(crate) max_range: f32,
}

impl ScreenShake {
    const FIELDS: [&'static str; 6] = [
        "ShakeSubtleIntensity",
        "ShakeNormalIntensity",
        "ShakeStrongIntensity",
        "ShakeSevereIntensity",
        "ShakeCineExtremeIntensity",
        "ShakeCineInsaneIntensity",
    ];

    fn read(block: &mint_sage_ini::Block) -> Self {
        let real = |name: &str| {
            block
                .field(name)
                .and_then(mint_sage_ini::Field::value)
                .and_then(mint_sage_ini::value::parse_float)
                .filter(|v| v.is_finite() && *v >= 0.0)
                .unwrap_or(0.0)
        };
        Self {
            intensities: Self::FIELDS.map(real),
            max_intensity: real("MaxShakeIntensity"),
            max_range: real("MaxShakeRange"),
        }
    }

    pub(crate) fn strength(&self, kind: i32) -> Option<f32> {
        let strength = *self.intensities.get(usize::try_from(kind).ok()?)?;
        Some(strength.min(self.max_intensity))
    }
}

pub(crate) const SHIPPED_BUILDER_CYCLE_MS: u32 = 3500;

pub(crate) const SHIPPED_DRAWABLE_CAPTION_POINTS: f32 = 12.0;

fn read_drawable_caption_points(documents: &[mint_sage_ini::Document]) -> Option<f32> {
    documents
        .iter()
        .flat_map(|document| document.roots())
        .filter(|block| block.keyword.eq_ignore_ascii_case("InGameUI"))
        .filter_map(|block| {
            block
                .field("DrawableCaptionPointSize")
                .and_then(mint_sage_ini::Field::value)
                .and_then(mint_sage_ini::value::parse_float)
                .filter(|points| points.is_finite() && *points > 0.0)
        })
        .next_back()
}

fn read_builder_cycle_timeout(documents: &[mint_sage_ini::Document]) -> Option<u32> {
    documents
        .iter()
        .flat_map(|document| document.roots())
        .filter(|block| block.keyword.eq_ignore_ascii_case("InGameUI"))
        .filter_map(|block| {
            block
                .field("SelectNearestBuilderCycleTimeOut")
                .and_then(mint_sage_ini::Field::value)
                .and_then(mint_sage_ini::value::parse_int)
                .and_then(|ms| u32::try_from(ms).ok())
        })
        .next_back()
}

fn read_combat_chains(documents: &[mint_sage_ini::Document]) -> crate::combat::CombatChainTable {
    let rows = mint_sage_ini::game_data::merged_combat_chain_definitions(documents)
        .into_iter()
        .filter_map(|(unit, targets)| {
            let attacker = mint_sage_modules::AiKindOf::parse(&unit)?;
            let targets = targets
                .into_iter()
                .filter_map(|(target, modifier)| {
                    Some((mint_sage_modules::AiKindOf::parse(&target)?, modifier))
                })
                .collect();
            Some((attacker, targets))
        })
        .collect();
    crate::combat::CombatChainTable::new(rows)
}

fn read_order_mode_key(documents: &[mint_sage_ini::Document]) -> Option<Key> {
    let keys = mint_sage_ini::command_map::command_map_keys(documents);
    match keys.get("ORDERMODE_WAYPOINT").map(String::as_str) {
        Some("ALT") => Some(Key::Alt),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KeyBinding {
    pub(crate) key: Key,
    pub(crate) on_press: bool,
}

fn read_diplomacy_key(documents: &[mint_sage_ini::Document]) -> Option<KeyBinding> {
    let keys = mint_sage_ini::command_map::command_map_keys(documents);
    let transitions = mint_sage_ini::command_map::command_map_transitions(documents);
    let key = match keys.get("DIPLOMACY").map(String::as_str) {
        Some("KEY_TAB") => Key::Tab,
        _ => return None,
    };
    Some(KeyBinding {
        key,
        on_press: transitions.get("DIPLOMACY").map(String::as_str) == Some("DOWN"),
    })
}

fn read_spell_store_key(documents: &[mint_sage_ini::Document]) -> Option<KeyBinding> {
    let keys = mint_sage_ini::command_map::command_map_keys(documents);
    let transitions = mint_sage_ini::command_map::command_map_transitions(documents);
    let key = match keys.get("SPELL_STORE").map(String::as_str) {
        Some("KEY_TICK") => Key::Character('`'),
        _ => return None,
    };
    Some(KeyBinding {
        key,
        on_press: transitions.get("SPELL_STORE").map(String::as_str) == Some("DOWN"),
    })
}

fn read_view_last_radar_event_key(documents: &[mint_sage_ini::Document]) -> Option<Key> {
    let keys = mint_sage_ini::command_map::command_map_keys(documents);
    match keys.get("VIEW_LAST_RADAR_EVENT").map(String::as_str) {
        Some("KEY_SPACE") => Some(Key::Space),
        _ => None,
    }
}

impl Default for GameData {
    fn default() -> Self {
        Self {
            combat: CombatData::default(),
            object_scripts: None,
            object_index: None,
            formations: FormationLibrary::default(),
            templates: Arc::new(mint_sage_modules::TemplateRecords::default()),
            command_sets: Arc::new(mint_sage_ini::CommandSetIndex::new()),
            command_buttons: Arc::new(mint_sage_ini::CommandButtonIndex::new()),
            mapped_images: Arc::new(mint_sage_ini::MappedImageIndex::new()),
            players: Arc::new(mint_sage_ini::PlayerTemplateIndex::new()),
            footprints: Arc::new(mint_sage_passability::TemplateIndex::default()),
            water_depths: mint_sage_passability::WaterDepths::SHIPPED,
            victory_structure_filter: crate::defeat::ObjectFilter::default(),
            victory_unit_filter: crate::defeat::ObjectFilter::default(),
            construction_cancel_refund_percent: 1.0,
            max_cells_adjust_horde_melee: MAX_CELLS_ADJUST_HORDE_MELEE,
            max_cells_adjust_destination: MAX_CELLS_ADJUST_DESTINATION,
            max_cells_adjust_to_possible_destination: MAX_CELLS_ADJUST_DESTINATION,
            melee_placement: crate::rules::MeleePlacement::default(),
            group_formation: crate::rules::GroupFormation::default(),
            drag_formation: Arc::new(mint_sage_formation::FormationAssistant::default()),
            castle_siege_stand_back_distance: DEFAULT_CASTLE_SIEGE_STAND_BACK_DISTANCE,
            max_tunnel_capacity: DEFAULT_MAX_TUNNEL_CAPACITY,
            fire_logic_system: mint_sage_ini::fire_logic_system::FireLogicSystem::default(),
            ini_documents: Arc::new(Vec::new()),
            map_ini: None,
            move_hint_name: String::new(),
            shell_map: crate::shell_map::ShellMapData::default(),
            tutorials: crate::tutorial::Tutorials::default(),
            selection_flash_saturation: SELECTION_FLASH_SATURATION,
            selection_flash_house_color: false,
            damage_thresholds: crate::render_state::DamageThresholds::DEFAULT,
            movement_penalty_state: crate::combat::DamageRung::ReallyDamaged,
            engaged_defaults: crate::rules::EngagedDefaults::default(),
            default_voice_attack_charge_timeout_ms: None,
            ai_bases: Arc::new(Vec::new()),
            ai_timing: AiTiming::default(),
            ai_power_activation_tuning: [(1, 1); 4],
            combat_chains: Arc::new(crate::combat::CombatChainTable::default()),
            order_mode_key: None,
            view_last_radar_event_key: None,
            diplomacy_key: None,
            spell_store_key: None,
            guard_ranges: crate::combat::GuardRanges::default(),
            attack_priorities: Arc::default(),
            army_definitions: Arc::new(mint_sage_ini::ArmyDefinitionIndex::new()),
            multiplayer_colors: Arc::new(mint_sage_ini::MultiplayerColorIndex::new()),
            handicaps: Arc::new(crate::handicap::HandicapTable::default()),
            create_a_hero: Arc::new(crate::create_a_hero::CahData::default()),
            rule_choices: Arc::new(crate::setup_rules::RuleChoices::default()),
            hero_subclasses: Arc::new(Vec::new()),
            cah_system: None,
            porter_targets: Arc::new(Vec::new()),
            attack_urgency_values: AttackUrgencyValues::default(),
            builder_cycle_timeout_ms: SHIPPED_BUILDER_CYCLE_MS,
            drawable_caption_points: SHIPPED_DRAWABLE_CAPTION_POINTS,
            floating_text: crate::floating_text::FloatingTextTiming::default(),
            military_caption: crate::military_caption::CaptionStyle::default(),
            named_timer: crate::named_timer::TimerStyle::default(),
            notification_box: Arc::new(mint_sage_ini::NotificationBoxDef::default()),
            camera: CameraDefaults::SHIPPED,
            shake: ScreenShake::default(),
            upgrades: Arc::new(Vec::new()),
            upgrade_bits: Arc::new(mint_sage_modules::UpgradeBits::default()),
            radius_cursors: Arc::new(mint_sage_ini::RadiusCursorIndex::new()),
            special_powers: Arc::new(mint_sage_ini::SpecialPowerIndex::new()),
            experience: Arc::new(mint_sage_modules::ExperienceTables::default()),
            experience_scalars: Arc::new(mint_sage_modules::ExperienceScalarTables::default()),
            armor_bonus_cap: SHIPPED_ARMOR_BONUS_CAP,
            sciences: Arc::new(mint_sage_ini::ScienceIndex::new()),
            ranks: Arc::new(mint_sage_ini::RankTable::default()),
            ocls: Arc::new(mint_sage_ini::ObjectCreationListIndex::new()),
            sell_percentage: SHIPPED_SELL_PERCENTAGE,
            base_check_frames: DEFAULT_BASE_CHECK_FRAMES,
            gravity: None,
            ground_stiffness: DEFAULT_GROUND_STIFFNESS,
            terrain_resource_cell_size: None,
            supply_boxes_per_tree: DEFAULT_SUPPLY_BOXES_PER_TREE,
            value_per_supply_box: DEFAULT_VALUE_PER_SUPPLY_BOX,
            macros: Arc::new(MacroTable::new()),
            locomotors: Arc::new(mint_sage_ini::LocomotorIndex::new()),
            emotions: Arc::new(mint_sage_ini::EmotionIndex::new()),
            stances: Arc::new(mint_sage_ini::StanceTemplateIndex::new()),
            command_points: None,
            multiplay: MultiplayMultipliers::default(),
            score_weights: ScoreWeights::default(),
            objects_that_score: crate::defeat::ObjectFilter::default(),
            command_points_solo: CommandPointsSolo::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiBaseDef {
    pub(crate) side: String,
    pub(crate) map: String,
    pub(crate) game_map_to_use_on: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AiTiming {
    pub(crate) structure_seconds: f32,
    pub(crate) wealthy_rate: f32,
    pub(crate) poor_rate: f32,
    pub(crate) wealthy_threshold: i64,
    pub(crate) poor_threshold: i64,
    pub(crate) rebuild_delay_seconds: f32,
    pub(crate) team_seconds: f32,
    pub(crate) teams_wealthy_rate: f32,
    pub(crate) teams_poor_rate: f32,
    pub(crate) team_resources_to_start: f32,
    pub(crate) max_recruit_radius: f32,
}

impl Default for AiTiming {
    fn default() -> Self {
        Self {
            structure_seconds: 0.0,
            wealthy_rate: 2.0,
            poor_rate: 0.6,
            wealthy_threshold: 7_000,
            poor_threshold: 2_000,
            rebuild_delay_seconds: 30.0,
            team_seconds: 10.0,
            teams_wealthy_rate: 2.0,
            teams_poor_rate: 0.6,
            team_resources_to_start: 0.1,
            max_recruit_radius: 400.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AttackUrgencyValues {
    pub(crate) attack_initial: i32,
    pub(crate) defense_initial: i32,
    pub(crate) economy_initial: i32,
    pub(crate) change_med: i32,
    pub(crate) change_high: i32,
    pub(crate) upper_bound_med_base: i32,
    pub(crate) upper_bound_high_base: i32,
    pub(crate) build_wave: [i32; 5],
    pub(crate) launch: i32,
    pub(crate) low_command_points: i32,
}

impl Default for AttackUrgencyValues {
    fn default() -> Self {
        Self {
            attack_initial: 250,
            defense_initial: 250,
            economy_initial: 500,
            change_med: 50,
            change_high: 100,
            upper_bound_med_base: 1_000,
            upper_bound_high_base: 1_000,
            build_wave: [100, 200, 300, 400, 500],
            launch: 600,
            low_command_points: 20,
        }
    }
}

const AI_INITIALIZE_LIBRARY: &str = "libraries/ai_initialize/ai_initialize.map";

const SCRIPT_EVENTS: &str = "data/scripts/scriptevents.xml";
const SCRIPT_HANDLERS: &str = "data/scripts/scripts.lua";

fn read_object_scripts(vfs: &Vfs) -> Result<(Option<ObjectScripts>, String), LoadRefusal> {
    let (Ok(events), Ok(handlers)) = (vfs.read(SCRIPT_EVENTS), vfs.read(SCRIPT_HANDLERS)) else {
        return Ok((None, String::from("scripts: no object event handlers")));
    };
    let text = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).into_owned();
    let scripts =
        ObjectScripts::load(&text(events), &text(handlers)).map_err(|error| LoadRefusal {
            errors: vec![error.to_string()],
            documents: 2,
        })?;
    let line = scripts.describe();
    Ok((Some(scripts), line))
}

fn read_attack_urgency_values(vfs: &Vfs) -> (AttackUrgencyValues, String) {
    let tree = vfs
        .read(AI_INITIALIZE_LIBRARY)
        .ok()
        .and_then(|bytes| mint_sage_map::read_script_library(&bytes).ok().flatten());
    let counters = tree
        .as_ref()
        .map(|tree| mint_sage_map::initial_int_values(tree, &["SET_COUNTER"]))
        .unwrap_or_default();
    let fallback = AttackUrgencyValues::default();
    let mut resolved = 0usize;
    let mut named = |name: &str, default: i32| -> i32 {
        match counters.get(name) {
            Some(&value) => {
                resolved = resolved.saturating_add(1);
                value
            }
            None => {
                println!(
                    "ai-scripts: {name} not found in {AI_INITIALIZE_LIBRARY} - using the \
                     shipped value {default}"
                );
                default
            }
        }
    };
    let values = AttackUrgencyValues {
        attack_initial: named("g_Attack_Urgency", fallback.attack_initial),
        defense_initial: named("g_Defense_Urgency", fallback.defense_initial),
        economy_initial: named("g_Economy_Urgency", fallback.economy_initial),
        change_med: named("g_ATTACK_URGENCY_CHANGE_MED", fallback.change_med),
        change_high: named("g_ATTACK_URGENCY_CHANGE_HIGH", fallback.change_high),
        upper_bound_med_base: named("g_Urgency_UpperBound_Med", fallback.upper_bound_med_base),
        upper_bound_high_base: named("g_Urgency_UpperBound_High", fallback.upper_bound_high_base),
        build_wave: [
            named("g_ATTACK_BUILD_WAVE_1", fallback.build_wave[0]),
            named("g_ATTACK_BUILD_WAVE_2", fallback.build_wave[1]),
            named("g_ATTACK_BUILD_WAVE_3", fallback.build_wave[2]),
            named("g_ATTACK_BUILD_WAVE_4", fallback.build_wave[3]),
            named("g_ATTACK_BUILD_WAVE_5", fallback.build_wave[4]),
        ],
        launch: named("g_ATTACK_LAUNCH", fallback.launch),
        low_command_points: named("g_ATTACK_LOW_COMMAND_POINTS", fallback.low_command_points),
    };
    let line = format!(
        "attack urgency: {resolved} of 14 constant(s) resolved from {AI_INITIALIZE_LIBRARY} \
         (launch {}, change med/high {}/{}, build waves {:?})",
        values.launch, values.change_med, values.change_high, values.build_wave
    );
    (values, line)
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FormationLibrary {
    names: Vec<String>,
    tables: Vec<FormationTable>,
    members: Vec<Option<String>>,
    banners: Vec<Option<BannerDef>>,
    melee: Vec<Option<f32>>,
    rangefinders: Vec<Option<f32>>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BannerDef {
    pub(crate) template: String,
    pub(crate) offset: (f32, f32),
    pub(crate) min_level: u32,
    pub(crate) destroy_horde_on_death: bool,
}

impl BannerDef {
    pub(crate) fn of(def: &mint_sage_formation::HordeContainDef) -> Option<Self> {
        let template = def.banner_carriers_allowed.clone()?;
        Some(Self {
            template,
            offset: def.banner_carrier_position.unwrap_or((0.0, 0.0)),
            min_level: def.banner_carrier_min_level.unwrap_or(1),
            destroy_horde_on_death: def.banner_carrier_destroy_horde_on_death.unwrap_or(false),
        })
    }
}

impl FormationLibrary {
    pub(crate) fn banners(&self) -> &[Option<BannerDef>] {
        &self.banners
    }

    pub(crate) fn banner(&self, index: u16) -> Option<&BannerDef> {
        self.banners.get(usize::from(index))?.as_ref()
    }

    pub(crate) fn index_of(&self, name: &str) -> Option<u16> {
        let wanted = name.to_ascii_lowercase();
        let at = self.names.iter().position(|found| *found == wanted)?;
        u16::try_from(at).ok()
    }

    pub(crate) fn tables(&self) -> &[FormationTable] {
        &self.tables
    }

    pub(crate) fn len(&self) -> usize {
        self.tables.len()
    }

    pub(crate) fn insert(&mut self, name: &str, entry: HordeFormation) {
        let HordeFormation {
            table,
            member,
            banner,
            ranges: (melee, rangefinder),
        } = entry;
        let key = name.to_ascii_lowercase();
        match self.names.iter().position(|found| *found == key) {
            Some(at) => {
                if let Some(slot) = self.melee.get_mut(at) {
                    *slot = melee;
                }
                if let Some(slot) = self.rangefinders.get_mut(at) {
                    *slot = rangefinder;
                }
                if let Some(slot) = self.tables.get_mut(at) {
                    *slot = table;
                }
                if let Some(slot) = self.members.get_mut(at) {
                    *slot = member;
                }
                if let Some(slot) = self.banners.get_mut(at) {
                    *slot = banner;
                }
            }
            None => {
                self.names.push(key);
                self.tables.push(table);
                self.members.push(member);
                self.banners.push(banner);
                self.melee.push(melee);
                self.rangefinders.push(rangefinder);
            }
        }
    }

    pub(crate) fn melee(&self) -> &[Option<f32>] {
        &self.melee
    }

    pub(crate) fn rangefinders(&self) -> &[Option<f32>] {
        &self.rangefinders
    }

    pub(crate) fn member(&self, index: u16) -> Option<&str> {
        self.members.get(usize::from(index))?.as_deref()
    }
}

pub(crate) struct HordeFormation {
    table: FormationTable,
    member: Option<String>,
    banner: Option<BannerDef>,
    ranges: (Option<f32>, Option<f32>),
}

pub(crate) fn horde_formation(
    name: &str,
    index: &mint_sage_modules::TemplateIndex<'_>,
    templates: &mint_sage_modules::TemplateRecords,
    combat: &CombatData,
    macros: &MacroTable,
    errors: &mint_sage_ini::ValueErrors,
) -> Option<(bool, Option<HordeFormation>)> {
    let lineage = index.lineage(name);
    let (at, module) = lineage
        .iter()
        .rev()
        .enumerate()
        .find_map(|(at, block)| Some((at, mint_sage_formation::find_horde_contain(block)?)))?;
    let def = mint_sage_formation::read_horde_contain(module, macros, errors);
    let table = FormationTable::from_contain(&def);
    if table.is_empty() {
        return Some((at > 0, None));
    }
    let member = (def.payload_entries == 1)
        .then(|| def.initial_payload.as_ref().map(|(name, _)| name.clone()))
        .flatten()
        .or_else(|| {
            def.ranks
                .iter()
                .map(|rank| rank.unit_type.trim())
                .find(|unit_type| !unit_type.is_empty())
                .map(str::to_owned)
        });
    let record = templates.get(name);
    let own_weapon = record
        .and_then(|record| {
            record
                .weapon_sets
                .iter()
                .find(|set| set.conditions.is_empty())?
                .value
                .as_deref()
        })
        .and_then(|weapon| combat.weapons.get(combat.weapons.id(weapon)?));
    let melee = own_weapon
        .filter(|weapon| weapon.melee)
        .filter(|_| {
            record.is_some_and(|record| record.kind_of.iter().any(|flag| flag == "MELEE_HORDE"))
        })
        .map(|weapon| weapon.attack_range);
    let rangefinder = own_weapon.map(|weapon| weapon.attack_range);
    Some((
        at > 0,
        Some(HordeFormation {
            table,
            member,
            banner: BannerDef::of(&def),
            ranges: (melee, rangefinder),
        }),
    ))
}

const SUBSYSTEM_LEGEND_PATHS: [&str; 2] = [
    "data/ini/default/subsystemlegendexpansion1.ini",
    "data/ini/default/subsystemlegend.ini",
];

const NOT_NAMED_BY_ANY_LEGEND: [&str; 3] = [
    "data/ini/optionregistry.ini",
    "data/ini/webpages.ini",
    "data/ini/fxparticlesystemcustom.ini",
];

fn retail_ini_order(sources: &MemorySources, table: &BlockTable) -> Vec<String> {
    let available: std::collections::BTreeSet<String> = sources
        .paths()
        .filter(|path| path.ends_with(".ini"))
        .map(str::to_owned)
        .collect();

    let legend = SUBSYSTEM_LEGEND_PATHS
        .iter()
        .find_map(|path| sources.read(path).map(|bytes| (*path, bytes)))
        .and_then(|(path, bytes)| {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            mint_sage_ini::Document::parse(path, &text, table).ok()
        });
    let mut order = match &legend {
        Some(document) => {
            let entries = mint_sage_ini::read_legend(document);
            mint_sage_ini::retail_load_order(&entries, &available)
        }
        None => Vec::new(),
    };

    let mut seen: std::collections::BTreeSet<&str> = order.iter().map(String::as_str).collect();
    seen.extend(SUBSYSTEM_LEGEND_PATHS);
    let included = included_paths(sources);
    let rest: Vec<String> = available
        .iter()
        .filter(|path| {
            !seen.contains(path.as_str())
                && !included.contains(path.as_str())
                && !NOT_NAMED_BY_ANY_LEGEND.contains(&path.as_str())
        })
        .cloned()
        .collect();
    order.extend(rest);
    order
}

struct ParsedCorpus {
    documents: Vec<mint_sage_ini::Document>,
    macros: mint_sage_ini::MacroTable,
    errors: usize,
    strict_unchecked: u64,
}

fn parse_corpus(vfs: &Vfs) -> Result<ParsedCorpus, LoadRefusal> {
    let mut names: Vec<String> = vfs
        .mounts()
        .flat_map(|mount| mount.entries())
        .filter(|path| path.as_str().starts_with(INI_ROOT))
        .map(|path| path.as_str().to_owned())
        .collect();
    if vfs.contains(LANGUAGE_COMMAND_MAP) {
        names.push(LANGUAGE_COMMAND_MAP.to_owned());
    }
    names.sort();
    names.dedup();

    let mut sources = MemorySources::new();
    let mut failures: Vec<String> = Vec::new();
    for name in &names {
        match vfs.read(name) {
            Ok(bytes) => sources.insert(name, bytes),
            Err(error) => failures.push(format!("{name}: {error}")),
        }
    }
    let table = BlockTable::corpus_path();
    let reader = Reader::new(&sources);
    let paths = retail_ini_order(&sources, &table);
    let mut documents = Vec::new();
    for (path, read) in paths.iter().zip(read_documents(&reader, &paths, &table)) {
        match read {
            Ok(document) => documents.push(document),
            Err(error) => failures.push(format!("{path}: {error}")),
        }
    }
    let errors = failures.len();
    let macros = mint_sage_ini::expand_macros(&mut documents);
    let (mut strict, strict_unchecked) = strict_errors(&documents, &macros);
    if !failures.is_empty() || !strict.is_empty() {
        failures.append(&mut strict);
        return Err(LoadRefusal {
            errors: failures,
            documents: documents.len(),
        });
    }
    Ok(ParsedCorpus {
        documents,
        macros,
        errors,
        strict_unchecked,
    })
}

fn read_documents(
    reader: &Reader<'_, MemorySources>,
    paths: &[String],
    table: &BlockTable,
) -> Vec<mint_sage_ini::Result<mint_sage_ini::Document>> {
    mint_pool::map_ordered(paths, |path| reader.read(path, table))
}

#[cfg(feature = "ini-census")]
pub(crate) fn load_documents_for_census(
    vfs: &Vfs,
) -> Result<Vec<mint_sage_ini::Document>, LoadRefusal> {
    Ok(parse_corpus(vfs)?.documents)
}

pub(crate) fn load_game_data(vfs: &Vfs) -> Result<(GameData, String), LoadRefusal> {
    let started = Instant::now();
    let parsed = parse_corpus(vfs)?;
    let index_started = Instant::now();
    let object_index =
        mint_sage_model::object_index_from_documents(&parsed.documents, &parsed.macros);
    let index_ms = index_started.elapsed().as_secs_f64() * 1000.0;
    let (mut data, line, mut value_errors) =
        game_data_from_expanded(&parsed.documents, parsed.macros, parsed.errors, started);
    let documents = parsed.documents;
    data.drag_formation = Arc::new(
        documents
            .iter()
            .flat_map(mint_sage_ini::Document::roots)
            .find(|block| block.keyword.eq_ignore_ascii_case("FormationAssistant"))
            .map(mint_sage_formation::FormationAssistant::read)
            .transpose()
            .map_err(|error| LoadRefusal {
                errors: vec![format!("FormationAssistant.ini: {error}")],
                documents: documents.len(),
            })?
            .unwrap_or_default(),
    );
    data.fire_logic_system = documents
        .iter()
        .flat_map(mint_sage_ini::Document::roots)
        .filter(|block| block.keyword.eq_ignore_ascii_case("FireLogicSystem"))
        .try_fold(
            mint_sage_ini::fire_logic_system::FireLogicSystem::default(),
            |system, block| mint_sage_ini::fire_logic_system::read_fire_logic_system(block, system),
        )
        .map_err(|error| LoadRefusal {
            errors: vec![format!("FireLogicSystem: {error}")],
            documents: documents.len(),
        })?;
    let mut systems = mint_sage_ini::ParticleIndex::new();
    let mut lists = mint_sage_ini::FxListIndex::new();
    let mut sounds = mint_sage_ini::AudioIndex::new();
    for document in &documents {
        sounds.insert_defines(document.defines());
    }
    for document in &documents {
        systems.insert_document(document);
        lists.insert_document(document);
        sounds.insert_document(document);
    }
    value_errors.extend(systems.value_errors().take());
    value_errors.extend(lists.value_errors().take());
    value_errors.extend(sounds.value_errors().take());
    if !value_errors.is_empty() {
        return Err(LoadRefusal {
            errors: value_errors.iter().map(ToString::to_string).collect(),
            documents: documents.len(),
        });
    }
    let modules = crate::validate::declared_modules(&data).map_err(|errors| LoadRefusal {
        errors,
        documents: documents.len(),
    })?;
    let line = format!(
        "{line}; strict: clean, {} statement(s) unchecked (no established field set); {modules}",
        parsed.strict_unchecked
    );
    let (attack_urgency_values, urgency_line) = read_attack_urgency_values(vfs);
    data.attack_urgency_values = attack_urgency_values;
    let (object_scripts, scripts_line) = read_object_scripts(vfs)?;
    data.object_scripts = object_scripts;
    let index_line = format!(
        "object index: {} object name(s) from the same parse in {index_ms:.0} ms",
        object_index.len()
    );
    data.object_index = Some(object_index);
    data.ini_documents = Arc::new(documents);
    let order_mode_line = format!(
        "order mode key: ORDERMODE_WAYPOINT -> {:?}",
        data.order_mode_key
    );
    Ok((
        data,
        format!("{line}; {urgency_line}\n{scripts_line}\n{index_line}\n{order_mode_line}"),
    ))
}

pub(crate) struct LoadRefusal {
    errors: Vec<String>,
    documents: usize,
}

impl LoadRefusal {
    pub(crate) fn report(&self) {
        for error in &self.errors {
            eprintln!("strict: {error}");
        }
        eprintln!(
            "strict: load refused - {} error(s) across {} document(s)",
            self.errors.len(),
            self.documents
        );
    }
}

fn strict_errors(
    documents: &[mint_sage_ini::Document],
    macros: &mint_sage_ini::MacroTable,
) -> (Vec<String>, u64) {
    let mut errors = Vec::new();
    let mut unchecked = 0u64;
    for document in documents {
        let report = mint_sage_ini::strict::check(
            document,
            macros,
            mint_sage_ini::tables::field_table::Titles::BOTH,
        );
        errors.extend(report.errors.iter().map(ToString::to_string));
        unchecked = unchecked.saturating_add(report.unchecked);
    }
    (errors, unchecked)
}

#[cfg(test)]
pub(crate) fn game_data_from_documents(
    documents: &[mint_sage_ini::Document],
    errors: usize,
    started: Instant,
) -> (GameData, String) {
    let mut documents: Vec<mint_sage_ini::Document> = documents.to_vec();
    let macros = mint_sage_ini::expand_macros(&mut documents);
    let (mut data, line, _) = game_data_from_expanded(&documents, macros, errors, started);
    data.ini_documents = Arc::new(documents);
    (data, line)
}

pub(crate) use mint_sage_modules::RETAIL_LOGIC_HZ;

pub(crate) fn retail_duration(ms: f32) -> mint_sim::LogicFrames {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let retail = (ms.max(0.0) * RETAIL_LOGIC_HZ as f32 / 1000.0).ceil() as u32;
    mint_sim::LogicFrames::from_frames(
        retail.saturating_mul(mint_sim::SIMULATION_HZ / RETAIL_LOGIC_HZ),
    )
}

const DEFAULT_BASE_CHECK_SECONDS: f32 = 25.0;

pub(crate) const DEFAULT_BASE_CHECK_FRAMES: u32 = 25 * RETAIL_LOGIC_HZ;

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn read_base_check_frames(block: &mint_sage_ini::Block) -> u32 {
    let seconds = game_data_float(
        block,
        "SecondsBeforeBaseCheckActive",
        DEFAULT_BASE_CHECK_SECONDS,
    );
    (seconds * RETAIL_LOGIC_HZ as f32) as u32
}

fn game_data_float(block: &mint_sage_ini::Block, name: &str, default: f32) -> f32 {
    block
        .field(name)
        .and_then(mint_sage_ini::Field::value)
        .and_then(mint_sage_ini::value::parse_float)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(default)
}

fn game_data_from_expanded(
    documents: &[mint_sage_ini::Document],
    macros: mint_sage_ini::MacroTable,
    errors: usize,
    started: Instant,
) -> (GameData, String, Vec<mint_sage_ini::ValueError>) {
    let game_data_block = mint_sage_ini::game_data::merged_game_data(documents);
    crate::user_data::note_game_data_leaf(
        game_data_block
            .field("UserDataLeafName")
            .and_then(mint_sage_ini::Field::value),
    );
    let ai_data_block = mint_sage_ini::game_data::merged_ai_data(documents);
    let side_info = mint_sage_ini::game_data::merged_side_info(documents);

    let mut data = CombatData::new();
    data.authored_logic_hz = core::num::NonZeroU32::new(RETAIL_LOGIC_HZ);
    for document in documents {
        data.load_document(document, &macros);
    }
    data.weapon_bonus = data.weapon_bonus(&game_data_block, &macros);

    let mut index = mint_sage_modules::TemplateIndex::new();
    for document in documents {
        index.insert_document(document);
    }
    let records_started = Instant::now();
    let (templates, mut value_errors) =
        mint_sage_modules::TemplateRecords::build_collecting(&index);
    let records_ms = records_started.elapsed().as_secs_f64() * 1000.0;
    let inherited = templates.iter().filter(|record| record.depth > 0).count();

    let mut formations = FormationLibrary::default();
    let contain_errors = mint_sage_ini::ValueErrors::new();
    let mut modules = 0usize;
    let mut inherited_hordes = 0usize;
    for name in index.names() {
        let Some((inherited, entry)) =
            horde_formation(name, &index, &templates, &data, &macros, &contain_errors)
        else {
            continue;
        };
        modules = modules.saturating_add(1);
        if inherited {
            inherited_hordes = inherited_hordes.saturating_add(1);
        }
        if let Some(entry) = entry {
            formations.insert(name, entry);
        }
    }

    let mut command_sets = mint_sage_ini::CommandSetIndex::new();
    let mut command_buttons = mint_sage_ini::CommandButtonIndex::new();
    let mut mapped_images = mint_sage_ini::MappedImageIndex::new();
    let mut players = mint_sage_ini::PlayerTemplateIndex::new();
    let mut army_definitions = mint_sage_ini::ArmyDefinitionIndex::new();
    let mut multiplayer_colors = mint_sage_ini::MultiplayerColorIndex::new();
    let mut upgrade_index = mint_sage_ini::UpgradeIndex::new();
    let mut special_powers = mint_sage_ini::SpecialPowerIndex::new();
    let mut radius_cursors = mint_sage_ini::RadiusCursorIndex::new();
    for document in documents {
        command_sets.insert_document(document);
        command_buttons.insert_document(document, &macros);
        mapped_images.insert_document(document);
        players.insert_document(document);
        army_definitions.insert_document(document);
        multiplayer_colors.insert_document(document);
        upgrade_index.insert_document(document);
        special_powers.insert_document(document, &macros);
        radius_cursors.insert_document(document);
    }
    let upgrades = mint_sage_modules::build_upgrade_records(&upgrade_index, &macros);
    let upgrade_bits = mint_sage_modules::UpgradeBits::from_index(&upgrade_index);

    let line = format!(
        "combat: {} weapon(s), {} armour template(s), {} damage type(s) from {} INI document(s) \
         ({errors} unreadable, {} macro(s)) in {:.2} s; {} nugget(s), {} modelled, {} field(s) \
         unresolved, {} DamageScalar line(s) unreadable",
        data.weapons.len(),
        data.armors.len(),
        data.damage_types.len(),
        documents.len(),
        macros.len(),
        started.elapsed().as_secs_f64(),
        data.report.nuggets,
        data.report.modelled_nuggets,
        data.report.unresolved_fields,
        data.report.unreadable_scalars,
    );
    let line = format!(
        "{line}; {modules} horde-contain module(s) ({inherited_hordes} inherited), {} distinct \
         formation(s), {} with a banner carrier; {} object template(s) read in {records_ms:.1} \
         ms, {inherited} through ChildObject/ObjectReskin; {} PlayerTemplate(s)",
        formations.len(),
        formations.banners().iter().flatten().count(),
        templates.len(),
        players.len(),
    );
    let footprints = mint_sage_passability::TemplateIndex::build(documents);
    let (water_depths, depth_fields) = mint_sage_passability::WaterDepths::read(&ai_data_block);
    let (ai_timing, ai_timing_fields) = read_ai_timing(&ai_data_block);
    let guard_errors = mint_sage_ini::ValueErrors::new();
    let (guard_ranges, guard_range_fields) =
        crate::combat::read_guard_ranges(&ai_data_block, &guard_errors);
    let group_formation = read_group_formation(&ai_data_block, &guard_errors);
    let mut attack_priorities = crate::combat::priority::AttackPriorityData::read(
        &ai_data_block,
        &mint_sage_ini::game_data::attack_priority_blocks(documents),
        &guard_errors,
    );
    attack_priorities.defined_templates = templates
        .iter()
        .map(|record| record.name.to_ascii_lowercase())
        .collect();
    let attack_priorities = Arc::new(attack_priorities);
    let castle_siege_stand_back_distance = ai_data_block
        .field("CastleSiegeStandBackDistance")
        .map_or(DEFAULT_CASTLE_SIEGE_STAND_BACK_DISTANCE, |field| {
            let value = field
                .value()
                .and_then(|v| v.trim().parse::<f32>().ok())
                .filter(|v| v.is_finite() && *v >= 0.0);
            if value.is_none() {
                guard_errors.field("AIData", field, "a non-negative number");
            }
            value.unwrap_or(DEFAULT_CASTLE_SIEGE_STAND_BACK_DISTANCE)
        });
    value_errors.extend(guard_errors.take());
    value_errors.extend(contain_errors.take());
    value_errors.extend(data.value_errors.take());
    value_errors.extend_from_slice(command_buttons.errors());
    let floating_errors = mint_sage_ini::ValueErrors::new();
    let floating_text = crate::floating_text::read_timing(documents, &floating_errors);
    value_errors.extend(floating_errors.take());
    let notification_errors = mint_sage_ini::ValueErrors::new();
    let notification_box = mint_sage_ini::read_notification_box(documents, &notification_errors);
    value_errors.extend(notification_errors.take());
    let porter_targets: Vec<(String, i64)> = side_info
        .iter()
        .filter_map(|(side, block)| {
            let normal = block
                .field("ResourceGatherersNormal")
                .and_then(mint_sage_ini::Field::value)
                .and_then(mint_sage_ini::value::parse_int)?;
            Some((side.clone(), normal))
        })
        .collect();

    let mut ai_bases = Vec::new();
    for document in documents {
        for block in document.roots() {
            if !block.keyword.eq_ignore_ascii_case("AIBase") {
                continue;
            }
            let field = |name: &str| {
                block
                    .field(name)
                    .and_then(mint_sage_ini::Field::value)
                    .map(str::to_owned)
            };
            let (Some(side), Some(map)) = (field("Side"), field("Map")) else {
                continue;
            };
            ai_bases.push(AiBaseDef {
                side,
                map,
                game_map_to_use_on: field("GameMapToUseOn").unwrap_or_default(),
            });
        }
    }
    let line = format!(
        "{line}; passability: {} of {} object template(s) occupy ground \
         ({} PROJECTILE/BRIDGE_TOWER/DO_NOT_CLASSIFY, {} not STRUCTURE, {} mobile, \
         {} GeometryIsSmall, {} without a radius), \
         wade {:.1} / deep {:.1} from {depth_fields} of 2 AIData field(s)",
        footprints.len(),
        footprints.templates_seen,
        footprints.excluded_kind,
        footprints.not_structure,
        footprints.mobile,
        footprints.too_small,
        footprints.no_radius,
        water_depths.wade,
        water_depths.deep,
    );

    let (
        victory_structure_filter,
        victory_unit_filter,
        construction_cancel_refund_percent,
        victory_fields,
    ) = read_game_data_block(&game_data_block);
    let line = format!(
        "{line}; victory: {victory_fields} of 3 GameData field(s) read, cancel refund {:.0}%, \
         unit filter {} (V1: parsed, not consulted for defeat)",
        construction_cancel_refund_percent * 100.0,
        if victory_unit_filter == crate::defeat::ObjectFilter::default() {
            "unconfigured"
        } else {
            "read"
        },
    );

    let line = format!(
        "{line}; skirmish AI: {} of 11 AIData timing field(s) read (StructureSeconds {:.1}, \
         Wealthy/Poor {}/{} at {:.1}x/{:.1}x, rebuild after {:.0}s), {} AIBase block(s), \
         guard: {} of 10 field(s) read (chase {} frame(s), scan {} frame(s), return scan {}          frame(s); the six modifier and retaliation fields have no reader), \
         {} ArmyDefinition(s), {} SideInfo porter target(s)",
        ai_timing_fields,
        ai_timing.structure_seconds,
        ai_timing.wealthy_threshold,
        ai_timing.poor_threshold,
        ai_timing.wealthy_rate,
        ai_timing.poor_rate,
        ai_timing.rebuild_delay_seconds,
        ai_bases.len(),
        guard_range_fields,
        guard_ranges.chase.get(),
        guard_ranges.enemy_scan.get(),
        guard_ranges.return_scan.get(),
        army_definitions.len(),
        porter_targets.len(),
    );
    let handicaps = crate::handicap::HandicapTable::read(documents, &game_data_block);
    let chained = upgrades
        .iter()
        .filter(|u| !u.sub_upgrade_template_names.is_empty())
        .count();
    let no_discount = upgrades.iter().filter(|u| u.no_upgrade_discount).count();
    let radius_cursor_counts = {
        let named = command_buttons
            .iter()
            .filter(|(_, def)| def.radius_cursor_type.is_some())
            .count();
        let drawn = command_buttons
            .names()
            .filter(|name| {
                crate::visuals::RadiusCursor::of(
                    name,
                    &command_buttons,
                    &special_powers,
                    &radius_cursors,
                )
                .is_some()
            })
            .count();
        (named, drawn)
    };
    let line = format!(
        "{line}; {} handicap percentage(s), {} MultiplayerColor(s); upgrades: {} of {} Upgrade \
         definition(s) priced ({} PLAYER-scoped, {chained} chaining a SubUpgradeTemplateNames, \
         {no_discount} NoUpgradeDiscount - read, not evaluated against ProductionModifier); {} \
         SpecialPower definition(s) ({} ReloadTime(s) unresolved); radius cursors: {} \
         RadiusCursorTemplate(s), {} button(s) name one, {} of them resolve a texture and a \
         positive radius",
        handicaps.len(),
        multiplayer_colors.len(),
        upgrades.len(),
        upgrade_index.definitions(),
        upgrades.iter().filter(|u| u.is_player).count(),
        special_powers.len(),
        special_powers.unresolved_reload_times(),
        radius_cursors.len(),
        radius_cursor_counts.0,
        radius_cursor_counts.1,
    );
    let experience = mint_sage_modules::ExperienceTables::build(documents, &macros);
    let experience_scalar_errors = mint_sage_ini::ValueErrors::new();
    let experience_scalars = mint_sage_modules::ExperienceScalarTables::build(
        documents,
        &macros,
        &experience_scalar_errors,
    );
    value_errors.extend(experience_scalar_errors.take());
    let armor_bonus_cap = read_armor_bonus_cap(&game_data_block).unwrap_or(SHIPPED_ARMOR_BONUS_CAP);
    let line = format!(
        "{line}; experience: {} ExperienceLevel block(s) into {} target table(s), {} dropped \
         (requirement or rank unresolved), {} award(s) unresolved, {} table(s) out of rank \
         order {:?}; {} ExperienceScalarTable(s) ({} dropped); {} ModifierList(s), {} modifier \
         line(s) of an unapplied kind, {} value(s) unresolved; armour bonus cap {:.0}%",
        experience.report.blocks,
        experience.report.targets,
        experience.report.dropped,
        experience.report.unresolved_awards,
        experience.out_of_order().len(),
        experience.out_of_order(),
        experience_scalars.len(),
        experience_scalars.report.dropped,
        data.modifiers.len(),
        data.modifiers.unapplied(),
        data.modifiers.unresolved(),
        armor_bonus_cap * 100.0,
    );
    let mut sciences = mint_sage_ini::ScienceIndex::new();
    let mut ocls = mint_sage_ini::ObjectCreationListIndex::new();
    let mut locomotors = mint_sage_ini::LocomotorIndex::new();
    let mut emotions = mint_sage_ini::EmotionIndex::new();
    let emotion_errors = mint_sage_ini::ValueErrors::new();
    let mut stances = mint_sage_ini::StanceTemplateIndex::new();
    let mut damage_fx = mint_sage_ini::DamageFxIndex::new();
    for document in documents {
        sciences.insert_document(document, &macros);
        ocls.insert_document(document, &emotion_errors);
        locomotors.insert_document(document);
        emotions.insert_document(document, &emotion_errors);
        stances.insert_document(document, &emotion_errors);
        damage_fx.insert_document(document, &emotion_errors);
    }
    let stance_holders = check_stance_references(
        &stances,
        &data,
        &index,
        &templates,
        documents,
        &emotion_errors,
    );
    value_errors.extend_from_slice(locomotors.errors());
    value_errors.extend(emotion_errors.take());
    let line = format!(
        "{line}; stances: {} StanceTemplate(s), {} Stance block(s), {} holder(s)",
        stances.len(),
        stances.iter().map(|t| t.stances.len()).sum::<usize>(),
        stance_holders,
    );
    let ranks = mint_sage_ini::RankTable::build(documents, &macros);
    let sell_percentage = read_sell_percentage(&game_data_block).unwrap_or(SHIPPED_SELL_PERCENTAGE);
    let base_check_frames = read_base_check_frames(&game_data_block);
    let gravity = game_data_block
        .field("Gravity")
        .and_then(mint_sage_ini::Field::value)
        .and_then(mint_sage_ini::value::parse_float)
        .filter(|g| g.is_finite());
    let ground_stiffness = game_data_block
        .field("GroundStiffness")
        .and_then(mint_sage_ini::Field::value)
        .and_then(mint_sage_ini::value::parse_float)
        .filter(|s| s.is_finite())
        .unwrap_or(DEFAULT_GROUND_STIFFNESS);
    let terrain_resource_cell_size = game_data_block
        .field("TerrainResourceCellSize")
        .and_then(mint_sage_ini::Field::value)
        .and_then(mint_sage_ini::value::parse_float)
        .filter(|size| size.is_finite() && *size > 0.0);
    let supply_boxes_per_tree = game_data_block
        .field("SupplyBoxesPerTree")
        .and_then(mint_sage_ini::Field::value)
        .and_then(mint_sage_ini::value::parse_float)
        .filter(|boxes| boxes.is_finite() && *boxes > 0.0)
        .unwrap_or(DEFAULT_SUPPLY_BOXES_PER_TREE);
    let value_per_supply_box = game_data_block
        .field("ValuePerSupplyBox")
        .and_then(mint_sage_ini::Field::value)
        .and_then(mint_sage_ini::value::parse_float)
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(DEFAULT_VALUE_PER_SUPPLY_BOX);
    let refund_percent_field = read_refund_percent_field(&game_data_block);
    let command_points = read_command_point_table(&game_data_block);
    let multiplay = read_multiplay_multipliers(&game_data_block);
    let score_weights = read_score_weights(&game_data_block);
    let objects_that_score = game_data_block
        .field("ObjectsThatScore")
        .map_or_else(crate::defeat::ObjectFilter::default, |field| {
            crate::defeat::ObjectFilter::parse(&field.args)
        });
    let command_points_solo = read_command_points_solo(&game_data_block);
    let penalty_errors = mint_sage_ini::ValueErrors::new();
    let movement_penalty_state = read_movement_penalty_state(&game_data_block, &penalty_errors);
    value_errors.extend(penalty_errors.take());
    let tutorial_errors = mint_sage_ini::ValueErrors::new();
    let tutorials = crate::tutorial::Tutorials::read(&game_data_block, &tutorial_errors);
    value_errors.extend(tutorial_errors.take());
    let line = format!(
        "{line}; multiplayer pacing (2 players): money x{:.2}, unit XP x{:.2}, building XP \
         x{:.2} (read, unconsumed), unit build speed x{:.2}, building speed x{:.2}; {} of 8 \
         solo/campaign command-point field(s) read (unconsumed by skirmish)",
        multiplay.money(2),
        multiplay.unit_xp(2),
        multiplay.building_xp(2),
        multiplay.unit_speed(2),
        multiplay.building_speed(2),
        command_points_solo.fields(),
    );
    let point_bonuses = templates
        .iter()
        .filter(|record| record.command_point_bonus.is_some_and(|v| v != 0.0))
        .count();
    let damage_creations = templates
        .iter()
        .flat_map(|record| &record.damage_creations)
        .collect::<Vec<_>>();
    let damage_creation_kinds = templates
        .iter()
        .filter(|record| !record.damage_creations.is_empty())
        .count();
    let damage_creation_unresolved = damage_creations
        .iter()
        .filter(|line| ocls.get(&line.list).is_none())
        .count();
    let triggering_weapons = data
        .weapons
        .iter()
        .filter(|(_, weapon)| weapon.fx_trigger != mint_sage_combat::FxTrigger::None)
        .count();
    let line = format!(
        "{line}; spell book: {} Science definition(s), {} rank(s) ({} dropped, {} value(s) \
         unresolved), {} ObjectCreationList(s); sell refund {:.0}% (RefundPercent {} - read, \
         unconsumed, a different field - see read_refund_percent_field's own doc comment); \
         command points: {} of 14 multiplayer field(s) (2 players: good {:?}, evil {:?}), \
         {point_bonuses} template(s) with a CommandPointBonus; {damage_creation_kinds} \
         template(s) with a DamageCreationList ({} line(s), {damage_creation_unresolved} \
         naming no ObjectCreationList), {triggering_weapons} weapon(s) with an FXTrigger; {} \
         Locomotor definition(s)",
        sciences.definitions(),
        ranks.len(),
        ranks.dropped,
        ranks.unresolved,
        ocls.len(),
        sell_percentage * 100.0,
        refund_percent_field.map_or_else(|| "unset".to_owned(), |v| format!("{:.0}%", v * 100.0)),
        command_points.map_or(0, |table| table.fields()),
        command_points.and_then(|table| table.of(false, 2)),
        command_points.and_then(|table| table.of(true, 2)),
        damage_creations.len(),
        locomotors.definitions(),
    );

    data.damage_fx = damage_fx;
    let mut sets_naming = 0usize;
    let mut sets_resolved = 0usize;
    let mut kinds_disagreeing = 0usize;
    for record in templates.iter() {
        let mut named: Option<&str> = None;
        for set in &record.armor_sets {
            let Some(name) = set.damage_fx.as_deref() else {
                continue;
            };
            sets_naming = sets_naming.saturating_add(1);
            if data.damage_fx.get(name).is_some() {
                sets_resolved = sets_resolved.saturating_add(1);
            }
            match named {
                Some(held) if !held.eq_ignore_ascii_case(name) => {
                    kinds_disagreeing = kinds_disagreeing.saturating_add(1);
                    named = Some(name);
                }
                _ => named = Some(name),
            }
        }
    }
    let fx_types = data.damage_fx_type_names.names();
    let fx_types_with_row = fx_types
        .iter()
        .filter(|name| data.damage_fx.any_names_row(name))
        .count();
    let line = format!(
        "{line}; damage FX: {} DamageFX definition(s), {sets_naming} ArmorSet.DamageFX line(s) of which {sets_resolved} resolve, {kinds_disagreeing} template(s) whose armour sets name more than one definition; {} DamageFXType value(s) of which {fx_types_with_row} name a row ({} weapon-level, {} nugget-level declaration(s))",
        data.damage_fx.definitions(),
        fx_types.len(),
        data.damage_fx_types.weapons(),
        data.damage_fx_types.len(),
    );

    let cah_system = crate::cah_edit::load_system(
        documents,
        &macros,
        &upgrade_index,
        &command_buttons,
        &command_sets,
        &templates,
    )
    .map(Arc::new);

    (
        GameData {
            combat: data,
            object_scripts: None,
            object_index: None,
            formations,
            templates: Arc::new(templates),
            command_sets: Arc::new(command_sets),
            command_buttons: Arc::new(command_buttons),
            mapped_images: Arc::new(mapped_images),
            players: Arc::new(players),
            footprints: Arc::new(footprints),
            water_depths,
            victory_structure_filter,
            victory_unit_filter,
            construction_cancel_refund_percent,
            max_cells_adjust_horde_melee: game_data_block
                .field("MaxCellsAdjustHordeMeleeDestination")
                .and_then(mint_sage_ini::Field::value)
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(MAX_CELLS_ADJUST_HORDE_MELEE),
            max_cells_adjust_destination: game_data_block
                .field("MaxCellsAdjustDestination")
                .and_then(mint_sage_ini::Field::value)
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(MAX_CELLS_ADJUST_DESTINATION),
            max_cells_adjust_to_possible_destination: game_data_block
                .field("MaxCellsAdjustToPossibleDestination")
                .and_then(mint_sage_ini::Field::value)
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(MAX_CELLS_ADJUST_DESTINATION),
            melee_placement: read_melee_placement(&game_data_block, &ai_data_block),
            group_formation,
            drag_formation: Arc::new(mint_sage_formation::FormationAssistant::default()),
            castle_siege_stand_back_distance,
            shell_map: crate::shell_map::ShellMapData::from_block(&game_data_block),
            tutorials,
            move_hint_name: game_data_block
                .field("MoveHintName")
                .and_then(mint_sage_ini::Field::value)
                .unwrap_or_default()
                .to_owned(),
            selection_flash_saturation: game_data_block
                .field("SelectionFlashSaturationFactor")
                .and_then(mint_sage_ini::Field::value)
                .and_then(mint_sage_ini::value::parse_float)
                .filter(|factor| factor.is_finite())
                .unwrap_or(SELECTION_FLASH_SATURATION),
            selection_flash_house_color: game_data_block
                .field("SelectionFlashHouseColor")
                .and_then(mint_sage_ini::Field::value)
                .and_then(mint_sage_ini::value::parse_bool)
                .unwrap_or(false),
            damage_thresholds: crate::render_state::DamageThresholds {
                damaged: threshold(
                    &game_data_block,
                    "UnitDamagedThreshold",
                    crate::render_state::DamageThresholds::DEFAULT.damaged,
                ),
                really: threshold(
                    &game_data_block,
                    "UnitReallyDamagedThreshold",
                    crate::render_state::DamageThresholds::DEFAULT.really,
                ),
            },
            movement_penalty_state,
            engaged_defaults: crate::rules::EngagedDefaults {
                max_distance: game_data_float(
                    &game_data_block,
                    "DefaultMaxDistanceForEngaged",
                    33.0,
                ),
                timeout_ms: game_data_float(&game_data_block, "DefaultEngagedStateTimeout", 1000.0),
            },
            default_voice_attack_charge_timeout_ms: game_data_block
                .field("DefaultVoiceAttackChargeTimeout")
                .and_then(mint_sage_ini::Field::value)
                .and_then(mint_sage_ini::value::parse_float),
            ai_bases: Arc::new(ai_bases),
            ai_timing,
            ai_power_activation_tuning:
                mint_sage_ini::game_data::skirmish_special_power_activation_tuning(documents),
            combat_chains: Arc::new(read_combat_chains(documents)),
            order_mode_key: read_order_mode_key(documents),
            view_last_radar_event_key: read_view_last_radar_event_key(documents),
            diplomacy_key: read_diplomacy_key(documents),
            spell_store_key: read_spell_store_key(documents),
            guard_ranges,
            attack_priorities,
            army_definitions: Arc::new(army_definitions),
            multiplayer_colors: Arc::new(multiplayer_colors),
            handicaps: Arc::new(handicaps),
            rule_choices: Arc::new(crate::setup_rules::RuleChoices::read(documents)),
            hero_subclasses: Arc::new(crate::setup_heroes::read_subclasses(documents, &macros)),
            cah_system,
            porter_targets: Arc::new(porter_targets),
            attack_urgency_values: AttackUrgencyValues::default(),
            builder_cycle_timeout_ms: read_builder_cycle_timeout(documents)
                .unwrap_or(SHIPPED_BUILDER_CYCLE_MS),
            drawable_caption_points: read_drawable_caption_points(documents)
                .unwrap_or(SHIPPED_DRAWABLE_CAPTION_POINTS),
            floating_text,
            military_caption: crate::military_caption::read_style(documents),
            named_timer: crate::named_timer::read_style(documents),
            notification_box: Arc::new(notification_box),
            camera: read_camera_defaults(&game_data_block),
            shake: ScreenShake::read(&game_data_block),
            create_a_hero: Arc::new(crate::create_a_hero::CahData::read(
                documents,
                &macros,
                &upgrade_index,
            )),
            upgrades: Arc::new(upgrades),
            upgrade_bits: Arc::new(upgrade_bits),
            radius_cursors: Arc::new(radius_cursors),
            special_powers: Arc::new(special_powers),
            experience: Arc::new(experience),
            experience_scalars: Arc::new(experience_scalars),
            armor_bonus_cap,
            sciences: Arc::new(sciences),
            ranks: Arc::new(ranks),
            ocls: Arc::new(ocls),
            sell_percentage,
            base_check_frames,
            gravity,
            ground_stiffness,
            terrain_resource_cell_size,
            supply_boxes_per_tree,
            value_per_supply_box,
            macros: Arc::new(macros),
            locomotors: Arc::new(locomotors),
            emotions: Arc::new(emotions),
            stances: Arc::new(stances),
            command_points,
            multiplay,
            score_weights,
            objects_that_score,
            command_points_solo,
            max_tunnel_capacity: game_data_block
                .field("MaxTunnelCapacity")
                .and_then(mint_sage_ini::Field::value)
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(DEFAULT_MAX_TUNNEL_CAPACITY),
            fire_logic_system: mint_sage_ini::fire_logic_system::FireLogicSystem::default(),
            ini_documents: Arc::new(Vec::new()),
            map_ini: None,
        },
        line,
        value_errors,
    )
}

fn check_stance_references(
    stances: &mint_sage_ini::StanceTemplateIndex,
    data: &CombatData,
    index: &mint_sage_modules::TemplateIndex<'_>,
    templates: &mint_sage_modules::TemplateRecords,
    documents: &[mint_sage_ini::Document],
    errors: &mint_sage_ini::ValueErrors,
) -> usize {
    for template in stances.iter() {
        for stance in &template.stances {
            let Some(name) = stance.attribute_modifier.as_deref() else {
                continue;
            };
            if data.modifiers.id(name).is_none() {
                errors.at(
                    &stance.file,
                    stance.line,
                    "Stance",
                    "AttributeModifier",
                    name,
                    "a declared ModifierList",
                );
            }
        }
    }
    let mut holders = 0usize;
    for record in templates.iter() {
        let Some(name) = record.stance_template.as_deref() else {
            continue;
        };
        holders = holders.saturating_add(1);
        if stances.get(name).is_some() {
            continue;
        }
        let (file, line) = index.lineage(&record.name).last().map_or_else(
            || (Arc::from(record.name.as_str()), 0),
            |block| (Arc::clone(&block.file), block.line),
        );
        errors.at(
            &file,
            line,
            "StancesBehavior",
            "StanceTemplate",
            name,
            "a declared StanceTemplate",
        );
    }
    for document in documents {
        for block in document.roots() {
            if !block.keyword.eq_ignore_ascii_case("CommandButton") {
                continue;
            }
            let Some(field) = block.field("Stances") else {
                continue;
            };
            for written in &field.args {
                if mint_sage_ini::Stance::from_name(written).is_none() {
                    errors.value("CommandButton", field, written, "a stance name");
                }
            }
        }
    }
    holders
}

fn read_ai_timing(block: &mint_sage_ini::Block) -> (AiTiming, usize) {
    let mut timing = AiTiming::default();
    let mut read = 0usize;
    let float = |name: &str| {
        block
            .field(name)
            .and_then(mint_sage_ini::Field::value)
            .and_then(|v| v.trim().parse::<f32>().ok())
            .filter(|v| v.is_finite())
    };
    if let Some(v) = float("StructureSeconds").filter(|v| *v >= 0.0) {
        timing.structure_seconds = v;
        read += 1;
    }
    if let Some(v) = float("StructuresWealthyRate").filter(|v| *v > 0.0) {
        timing.wealthy_rate = v;
        read += 1;
    }
    if let Some(v) = float("StructuresPoorRate").filter(|v| *v > 0.0) {
        timing.poor_rate = v;
        read += 1;
    }
    if let Some(v) = float("Wealthy").filter(|v| *v >= 0.0) {
        timing.wealthy_threshold = whole(v).unwrap_or(timing.wealthy_threshold);
        read += 1;
    }
    if let Some(v) = float("Poor").filter(|v| *v >= 0.0) {
        timing.poor_threshold = whole(v).unwrap_or(timing.poor_threshold);
        read += 1;
    }
    if let Some(v) = float("RebuildDelayTimeSeconds").filter(|v| *v >= 0.0) {
        timing.rebuild_delay_seconds = v;
        read += 1;
    }
    if let Some(v) = float("TeamSeconds").filter(|v| *v >= 0.0) {
        timing.team_seconds = v;
        read += 1;
    }
    if let Some(v) = float("TeamsWealthyRate").filter(|v| *v > 0.0) {
        timing.teams_wealthy_rate = v;
        read += 1;
    }
    if let Some(v) = float("TeamsPoorRate").filter(|v| *v > 0.0) {
        timing.teams_poor_rate = v;
        read += 1;
    }
    if let Some(v) = float("TeamResourcesToStart").filter(|v| *v >= 0.0) {
        timing.team_resources_to_start = v;
        read += 1;
    }
    if let Some(v) = float("MaxRecruitRadius").filter(|v| *v >= 0.0) {
        timing.max_recruit_radius = v;
        read += 1;
    }
    (timing, read)
}

fn whole(value: f32) -> Option<i64> {
    crate::templates::whole(value)
}

const SELECTION_FLASH_SATURATION: f32 = 0.5;

fn threshold(block: &mint_sage_ini::Block, keyword: &str, default: f32) -> f32 {
    block
        .field(keyword)
        .and_then(mint_sage_ini::Field::value)
        .and_then(mint_sage_ini::value::parse_float)
        .filter(|value| *value >= 0.0 && *value <= 1.0)
        .unwrap_or(default)
}

fn read_movement_penalty_state(
    block: &mint_sage_ini::Block,
    errors: &mint_sage_ini::ValueErrors,
) -> crate::combat::DamageRung {
    use crate::combat::DamageRung;
    const RUNGS: [(&str, DamageRung); 4] = [
        ("PRISTINE", DamageRung::Pristine),
        ("DAMAGED", DamageRung::Damaged),
        ("REALLYDAMAGED", DamageRung::ReallyDamaged),
        ("RUBBLE", DamageRung::Rubble),
    ];
    let Some(field) = block.field("MovementPenaltyDamageState") else {
        return DamageRung::ReallyDamaged;
    };
    let rung = field.value().and_then(|token| {
        RUNGS
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(token))
            .map(|&(_, rung)| rung)
    });
    rung.unwrap_or_else(|| {
        errors.field(
            "GameData",
            field,
            "PRISTINE, DAMAGED, REALLYDAMAGED or RUBBLE",
        );
        DamageRung::ReallyDamaged
    })
}

const MAX_CELLS_ADJUST_HORDE_MELEE: u32 = 200;

const MAX_CELLS_ADJUST_DESTINATION: u32 = 400;

const DEFAULT_CASTLE_SIEGE_STAND_BACK_DISTANCE: f32 = 500.0;

const DEFAULT_MAX_TUNNEL_CAPACITY: u32 = 5;

fn read_melee_placement(
    game_data: &mint_sage_ini::Block,
    ai_data: &mint_sage_ini::Block,
) -> crate::rules::MeleePlacement {
    let defaults = crate::rules::MeleePlacement::default();
    let cells = |name: &str, default: u32| {
        game_data
            .field(name)
            .and_then(mint_sage_ini::Field::value)
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(default)
    };
    let distance = |name: &str, default: f32| {
        ai_data
            .field(name)
            .and_then(mint_sage_ini::Field::value)
            .and_then(|value| value.trim().parse::<f32>().ok())
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or(default)
    };
    crate::rules::MeleePlacement {
        engagement_cells: cells(
            "MaxCellsFindMeleeEngagementLocation",
            defaults.engagement_cells,
        ),
        approach_cells: cells("MaxCellsAdjustToMeleeDestination", defaults.approach_cells),
        approach_dist: distance("MeleeApproachDist", defaults.approach_dist),
        approach_tolerance: distance("MeleeApproachTolerance", defaults.approach_tolerance),
        chase_from_behind: distance("ChaseFromBehindLimit", defaults.chase_from_behind),
    }
}

fn read_group_formation(
    ai_data: &mint_sage_ini::Block,
    errors: &mint_sage_ini::ValueErrors,
) -> crate::rules::GroupFormation {
    fn value<T>(
        block: &mint_sage_ini::Block,
        errors: &mint_sage_ini::ValueErrors,
        name: &str,
        expected: &'static str,
        parse: impl FnOnce(&str) -> Option<T>,
    ) -> Option<T> {
        let field = block.field(name)?;
        let read = field.value().and_then(|text| parse(text.trim()));
        if read.is_none() {
            errors.field("AIData", field, expected);
        }
        read
    }
    let defaults = crate::rules::GroupFormation::default();
    let distance = |name: &str, default: f32| {
        value(ai_data, errors, name, "a non-negative number", |text| {
            text.parse::<f32>()
                .ok()
                .filter(|v| v.is_finite() && *v >= 0.0)
        })
        .unwrap_or(default)
    };
    let flag = |name: &str, default: bool| {
        value(
            ai_data,
            errors,
            name,
            "Yes or No",
            mint_sage_ini::value::parse_bool,
        )
        .unwrap_or(default)
    };
    crate::rules::GroupFormation {
        spacing: mint_sage_formation::GroupSpacing {
            column_width: distance("FormationColumnWidth", defaults.spacing.column_width),
            row_depth: distance("FormationRowDepth", defaults.spacing.row_depth),
            squad_spacing: distance("FormationSquadSpacing", defaults.spacing.squad_spacing),
            columns: value(ai_data, errors, "FormationColumns", "an integer", |text| {
                text.parse::<i32>().ok()
            })
            .unwrap_or(defaults.spacing.columns),
        },
        use_formations: flag("UseFormations", defaults.use_formations),
        attack_move_uses_formations: flag(
            "AttackMoveUsesFormations",
            defaults.attack_move_uses_formations,
        ),
        hordes_wait_for_hordes: flag("HordesWaitForHordes", defaults.hordes_wait_for_hordes),
        click_to_gather_area_factor: value(
            ai_data,
            errors,
            "GroupMoveClickToGatherAreaFactor",
            "a number",
            |text| text.parse::<f32>().ok().filter(|v| v.is_finite()),
        )
        .unwrap_or(defaults.click_to_gather_area_factor),
        narrow_passage_scale: distance("NarrowPassageScale", defaults.narrow_passage_scale),
    }
}

fn read_game_data_block(
    block: &mint_sage_ini::Block,
) -> (
    crate::defeat::ObjectFilter,
    crate::defeat::ObjectFilter,
    f32,
    usize,
) {
    let mut structure_filter = crate::defeat::ObjectFilter::default();
    let mut unit_filter = crate::defeat::ObjectFilter::default();
    let mut refund_percent = 1.0f32;
    let mut read = 0usize;
    if let Some(field) = block.field("VictoryConditionStructureObjectFilter") {
        structure_filter = crate::defeat::ObjectFilter::parse(&field.args);
        read = read.saturating_add(1);
    }
    if let Some(field) = block.field("VictoryConditionUnitObjectFilter") {
        unit_filter = crate::defeat::ObjectFilter::parse(&field.args);
        read = read.saturating_add(1);
    }
    if let Some(percent) = block
        .field("ConstructionCancelRefundPercent")
        .and_then(mint_sage_ini::Field::value)
        .and_then(mint_sage_ini::value::parse_percent)
        .filter(|value| value.is_finite() && *value >= 0.0)
    {
        refund_percent = percent;
        read = read.saturating_add(1);
    }
    (structure_filter, unit_filter, refund_percent, read)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
