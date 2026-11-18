#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

pub mod anim_state;
pub mod armor;
pub mod army_definition;
pub mod audio;
pub mod award_system;
pub mod client_behavior;
pub mod cloud_effect;
pub mod command_button;
pub mod command_map;
pub mod command_set;
pub mod control_bar_scheme;
#[cfg(feature = "coverage")]
pub mod coverage;
pub mod create_a_hero;
pub mod damage_fx;
pub mod diagnostic;
pub mod emotions;
pub mod eva;
pub mod fire_logic_system;
pub mod game_data;
mod glow_effect;
pub mod house_color;
pub mod lex;
pub mod linear_campaign;
pub mod living_world;
pub mod locomotor;
pub mod mapped_image;
pub mod melee_behavior;
pub mod model_condition_flags;
pub mod model_state;
pub mod mouse_cursor;
pub mod multiplayer_color;
pub mod name_fields;
pub mod notification_box;
pub mod object;
pub mod object_creation_list;
pub mod particle;
pub mod player_template;
pub mod radius_cursor;
pub mod rank;
mod reader;
pub mod science;
mod source;
pub mod special_power;
pub mod stances;
pub mod strict;
pub mod subsystem_legend;
pub mod table;
pub mod tables;
mod tree;
pub mod upgrade;
pub mod value;
pub mod video;
pub mod weapon;
pub mod weather;

pub use anim_state::{
    draw_animation_states, AnimationClip, AnimationFxEvent, AnimationLuaEvent, AnimationStateDef,
    AnimationStateKind,
};
pub use armor::{read_armor, ArmorIndex, ArmorSet};
pub use army_definition::{
    read_army_definition, ArmyDefinitionDef, ArmyDefinitionIndex, ArmyMember,
    ARMY_DEFINITION_KEYWORD, ARMY_MEMBER_KEYWORD,
};
pub use audio::{
    read_audio_event, read_audio_settings, AudioEventDef, AudioEventKind, AudioIndex,
    AudioPriority, AudioSettingsDef, ControlFlags, SoundVariant, Submix, TypeFlags,
};
pub use award_system::{
    read_award_system, read_thing_stats, AwardDef, AwardTrigger, ThingStatDef, AWARD_SYSTEM_KEYWORD,
};
pub use client_behavior::{
    animation_sound_behavior, AnimationSoundBehavior, AnimationSoundEntry, AnimationSoundError,
    ANIMATION_SOUND_CLIENT_BEHAVIOR,
};
pub use cloud_effect::{cloud_darkening, CloudDarkeningSetting, CLOUD_EFFECT_KEYWORD};
pub use command_button::{read_command_button, ButtonFace, CommandButtonDef, CommandButtonIndex};
pub use command_set::{read_command_set, CommandSetDef, CommandSetIndex};
pub use control_bar_scheme::{
    read_control_bar_scheme, Argb8, ControlBarSchemeDef, ControlBarSchemeIndex,
    CONTROL_BAR_SCHEME_KEYWORD,
};
pub use create_a_hero::{
    read_create_a_hero_system, AttributeDef, BlingBinderDef, BlingChoices, BlingDef, BlingType,
    ClassDef, CreateAHeroSystemDef, SubClassDef, ViewInfoDef, ViewPreset,
    CREATE_A_HERO_SYSTEM_KEYWORD,
};
pub use damage_fx::{read_damage_fx, DamageFxDef, DamageFxIndex, DamageFxRow, VETERANCY_LEVELS};
pub use diagnostic::{ValueError, ValueErrors};
pub use emotions::{
    read_emotion_nugget, EmotionAiState, EmotionIndex, EmotionNuggetDef, EmotionType,
    ADD_EMOTION_KEYWORD, EMOTION_NUGGET_FIELDS, EMOTION_NUGGET_KEYWORD, OVERRIDE_TOKEN,
};
pub use game_data::{
    merged_ai_data, merged_game_data, merged_side_info, AI_DATA_KEYWORD, GAME_DATA_KEYWORD,
    SIDE_INFO_KEYWORD,
};
pub use glow_effect::{glow_effect, GlowEffectSetting, GLOW_EFFECT_KEYWORD};
pub use house_color::{texture_key, HouseColorTable, HOUSE_COLOR_KEYWORD};
pub use linear_campaign::{
    ms_to_frames, read_linear_campaign, LinearCampaignDef, LinearCampaignIndex, MissionDef,
    DEFAULT_FADE_UP_FRAMES, LINEAR_CAMPAIGN_KEYWORD,
};
pub use locomotor::{read_locomotor, LocomotorDef, LocomotorIndex};
pub use mapped_image::{read_mapped_image, MappedImageDef, MappedImageIndex, MappedRect};
pub use melee_behavior::{
    find_melee_behavior, read_melee_behavior, AmoebaDef, MeleeBehaviorDef, MeleeBehaviorKind,
    MELEE_BEHAVIOR_KEYWORD,
};
pub use model_state::{
    draw_model_states, draw_model_states_collecting, DecalBlend, DecalOpacity, ModelStateDef,
    StateDecal, WEAPON_SLOT_COUNT,
};
pub use mouse_cursor::{cursor_file, read_mouse_cursor, MouseCursorDef, MouseCursorIndex};
pub use multiplayer_color::{
    read_multiplayer_color, MultiplayerColorDef, MultiplayerColorIndex, Rgb8,
    MULTIPLAYER_COLOR_KEYWORD,
};
pub use name_fields::{is_name_shaped, read_name_fields, NameField, NameFieldIndex};
pub use notification_box::{
    read_notification_box, MessageFont, NotificationBoxDef, NotificationType,
};
pub use object::{
    parse_particle_sys_bone, read_object_fields, AnimationFlags, AnimationMode, AnimationSource,
    DefaultAnimation, DrawModuleView, NoModelReason, ObjectFields, ObjectIndex, ParticleBones,
    ParticleSysBone, Persist, RoadDefinition, StreakDraw, Template, TreeTopple, TruckTires,
    ANIMATION_FIELDS, ROAD_FIELDS, STREAK_DRAW_MODULE, TREE_DRAW_MODULE,
};
pub use object_creation_list::{
    read_object_creation_list, CreationEntry, ObjectCreationListDef, ObjectCreationListIndex,
    DEFAULT_DISPOSITION,
};
pub use particle::{
    water_gate_admits, weather_gate_admits, AlphaKey, CameraShakerVolume, ColorKey, DecalShader,
    DefinitionForm, EmissionVelocity, EmissionVolume, Flipbook, FxAtBonePos, FxDynamicDecal,
    FxEvaEvent, FxList, FxListIndex, FxNugget, FxParticleNugget, LightningWave, NuggetConditions,
    NuggetWeather, ParticleIndex, ParticleKind, ParticlePriority, ParticleRotation, ParticleShader,
    ParticleSystemDef, RandomRange, TerrainCollisionEvent, Wind, WindMotion,
    DEFAULT_FULL_STRENGTH_DIST, DEFAULT_ZERO_STRENGTH_DIST, DEFINE_DOCUMENTS,
    FX_DELAY_FRAMES_PER_MS, PARTICLE_DOCUMENTS,
};
pub use player_template::{
    read_player_template, PlayerTemplateDef, PlayerTemplateIndex, StartingUnit, MAX_STARTING_UNITS,
    PLAYER_TEMPLATE_KEYWORD,
};
pub use radius_cursor::{
    read_floating_text_fields, read_radius_cursor_template, FloatingTextFields, RadiusCursorIndex,
    RadiusCursorTemplateDef, RADIUS_CURSOR_TEMPLATE_KEYWORD,
};
pub use rank::{read_rank, RankDef, RankTable, RANK_KEYWORD};
pub use reader::{Limits, Reader, Stmt};
pub use science::{
    prerequisite_alternatives, read_science, ScienceDef, ScienceIndex, SCIENCE_KEYWORD,
};
pub use source::{included_paths, normalise_path, resolve_include, MemorySources, Sources};
pub use special_power::{read_special_power, SpecialPowerDef, SpecialPowerIndex};
pub use stances::{
    Stance, StanceDef, StanceTemplateDef, StanceTemplateIndex, STANCE_TEMPLATE_KEYWORD,
};
pub use strict::{StrictError, StrictReport};
pub use subsystem_legend::{read_legend, retail_load_order, LegendEntry, LOAD_SUBSYSTEM_KEYWORD};
pub use table::{BlockOpener, BlockTable, Scope};
pub use tree::{Block, Define, Diagnostic, Document, Field, Node, Script};
pub use upgrade::{read_upgrade, UpgradeDef, UpgradeIndex};
pub use value::{Duration, MacroDiagnostic, MacroTable};
pub use video::{read_video, VideoDef, VideoIndex, VIDEO_KEYWORD};
pub use weapon::{read_weapon, LinearTarget, WeaponDef, WeaponIndex};
pub use weather::{weather_sounds, HardwareFog, SnowSetting, WeatherSound};

pub type Result<T> = std::result::Result<T, Error>;

pub fn expand_macros(documents: &mut [Document]) -> MacroTable {
    let mut macros = MacroTable::new();
    for document in documents.iter() {
        macros.insert_defines(document.defines());
    }
    for document in documents.iter_mut() {
        document.expand_token_list_macros(&macros);
    }
    macros
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod expand_macros_tests {
    use super::*;

    #[test]
    fn a_kind_of_macro_declared_in_one_document_resolves_in_another() {
        let table = BlockTable::corpus_path();
        let gamedata = Document::parse(
            "gamedata.ini",
            "#define GONDOR_RANGER_KINDOF SELECTABLE\n",
            &table,
        )
        .expect("parses");
        let object = Document::parse(
            "object.ini",
            "Object GondorRanger\n  KindOf = GONDOR_RANGER_KINDOF SUMMONED\nEnd\n",
            &table,
        )
        .expect("parses");
        let mut documents = vec![gamedata, object];
        expand_macros(&mut documents);
        let kind_of = documents[1].roots()[0].field("KindOf").expect("field");
        assert_eq!(kind_of.args, vec!["SELECTABLE", "SUMMONED"]);
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("no source named {path}")]
    NotFound { path: String },

    #[error("{file}:{line}: include nesting deeper than {limit}")]
    IncludeDepth {
        file: String,
        line: u32,
        limit: usize,
    },

    #[error("{file}:{line}: include cycle through {path}")]
    IncludeCycle {
        file: String,
        line: u32,
        path: String,
    },

    #[error("more than {limit} includes from one document")]
    TooManyIncludes { limit: usize },

    #[error("more than {limit} statements from one document")]
    TooManyStatements { limit: usize },

    #[error("{file}:{line}: block nesting deeper than {limit}")]
    TooDeep {
        file: String,
        line: u32,
        limit: usize,
    },
}
