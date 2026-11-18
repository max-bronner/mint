use std::collections::BTreeMap;
use std::sync::Arc;

use crate::diagnostic::ValueErrors;
use crate::tree::{Block, Document, Field, Node};
use crate::value::{parse_bool, parse_float, parse_int, parse_percent};

pub const EMOTION_NUGGET_KEYWORD: &str = "EmotionNugget";

pub const ADD_EMOTION_KEYWORD: &str = "AddEmotion";

pub const OVERRIDE_TOKEN: &str = "OVERRIDE";

pub const EMOTION_NUGGET_FIELDS: [&str; 29] = [
    "CopyFrom",
    "Type",
    "IgnoreIfUnitIdle",
    "IgnoreIfUnitBusy",
    "Duration",
    "InactiveDuration",
    "InactiveDurationSameObject",
    "InactiveDurationSameType",
    "OnlyIfEnemyThreatAbove",
    "OnlyIfEnemyThreatBelow",
    "OnlyIfFriendThreatAbove",
    "OnlyIfFriendThreatBelow",
    "IgnoreIfAI",
    "IgnoreIfHuman",
    "StartFXList",
    "UpdateFXList",
    "EndFXList",
    "AttributeModifier",
    "AttributeStartDelay",
    "AttributeModifierWhileEmotionActive",
    "AttributeDuration",
    "AIState",
    "AILockDuration",
    "ModelConditions",
    "ModelConditionsClear",
    "ModelConditionsSetOnExit",
    "ModelConditionsClearOnExit",
    "LuaEvent",
    "PreventPlayerCommands",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EmotionType {
    Taunt,
    Alert,
    Cheer,
    HeroCheer,
    CheerForAboutToCrush,
    Fear,
    UncontrollableFear,
    Terror,
    Doom,
    BraceForBeingCrushed,
    Point,
    Quarrelsome,
}

impl EmotionType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Taunt => "TAUNT",
            Self::Alert => "ALERT",
            Self::Cheer => "CHEER",
            Self::HeroCheer => "HERO_CHEER",
            Self::CheerForAboutToCrush => "CHEER_FOR_ABOUT_TO_CRUSH",
            Self::Fear => "FEAR",
            Self::UncontrollableFear => "UNCONTROLLABLE_FEAR",
            Self::Terror => "TERROR",
            Self::Doom => "DOOM",
            Self::BraceForBeingCrushed => "BRACE_FOR_BEING_CRUSHED",
            Self::Point => "POINT",
            Self::Quarrelsome => "QUARRELSOME",
        }
    }

    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        const ALL: [EmotionType; 12] = [
            EmotionType::Taunt,
            EmotionType::Alert,
            EmotionType::Cheer,
            EmotionType::HeroCheer,
            EmotionType::CheerForAboutToCrush,
            EmotionType::Fear,
            EmotionType::UncontrollableFear,
            EmotionType::Terror,
            EmotionType::Doom,
            EmotionType::BraceForBeingCrushed,
            EmotionType::Point,
            EmotionType::Quarrelsome,
        ];
        ALL.into_iter()
            .find(|kind| kind.as_str().eq_ignore_ascii_case(token))
    }

    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Taunt => 0,
            Self::Alert => 1,
            Self::Cheer => 2,
            Self::HeroCheer => 3,
            Self::CheerForAboutToCrush => 4,
            Self::Fear => 5,
            Self::UncontrollableFear => 6,
            Self::Terror => 7,
            Self::Doom => 8,
            Self::BraceForBeingCrushed => 9,
            Self::Point => 10,
            Self::Quarrelsome => 11,
        }
    }

    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Taunt),
            1 => Some(Self::Alert),
            2 => Some(Self::Cheer),
            3 => Some(Self::HeroCheer),
            4 => Some(Self::CheerForAboutToCrush),
            5 => Some(Self::Fear),
            6 => Some(Self::UncontrollableFear),
            7 => Some(Self::Terror),
            8 => Some(Self::Doom),
            9 => Some(Self::BraceForBeingCrushed),
            10 => Some(Self::Point),
            11 => Some(Self::Quarrelsome),
            _ => None,
        }
    }

    #[must_use]
    pub const fn is_negative(self) -> bool {
        matches!(
            self,
            Self::Point
                | Self::Fear
                | Self::UncontrollableFear
                | Self::Terror
                | Self::Doom
                | Self::BraceForBeingCrushed
        )
    }

    #[must_use]
    pub const fn is_fear(self) -> bool {
        matches!(
            self,
            Self::Fear | Self::UncontrollableFear | Self::Terror | Self::Doom
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmotionAiState {
    FaceObject,
    BackAway,
    AvoidScarer,
    RunAwayPanic,
    Idle,
    Quarrel,
}

impl EmotionAiState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FaceObject => "FACE_OBJECT",
            Self::BackAway => "BACK_AWAY",
            Self::AvoidScarer => "AVOID_SCARER",
            Self::RunAwayPanic => "RUN_AWAY_PANIC",
            Self::Idle => "IDLE",
            Self::Quarrel => "QUARREL",
        }
    }

    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        const ALL: [EmotionAiState; 6] = [
            EmotionAiState::FaceObject,
            EmotionAiState::BackAway,
            EmotionAiState::AvoidScarer,
            EmotionAiState::RunAwayPanic,
            EmotionAiState::Idle,
            EmotionAiState::Quarrel,
        ];
        ALL.into_iter()
            .find(|state| state.as_str().eq_ignore_ascii_case(token))
    }

    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::FaceObject => 0,
            Self::BackAway => 1,
            Self::AvoidScarer => 2,
            Self::RunAwayPanic => 3,
            Self::Idle => 4,
            Self::Quarrel => 5,
        }
    }

    #[must_use]
    pub const fn retreats(self) -> bool {
        matches!(
            self,
            Self::BackAway | Self::AvoidScarer | Self::RunAwayPanic
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EmotionNuggetDef {
    pub name: String,
    pub emotion_type: Option<EmotionType>,
    pub ignore_if_unit_idle: bool,
    pub ignore_if_unit_busy: bool,
    pub duration_ms: Option<u32>,
    pub inactive_duration_ms: Option<u32>,
    pub inactive_duration_same_object_ms: Option<u32>,
    pub inactive_duration_same_type_ms: Option<u32>,
    pub only_if_enemy_threat_above: Option<f32>,
    pub only_if_enemy_threat_below: Option<f32>,
    pub only_if_friend_threat_above: Option<f32>,
    pub only_if_friend_threat_below: Option<f32>,
    pub ignore_if_ai: bool,
    pub ignore_if_human: bool,
    pub start_fx: Option<String>,
    pub update_fx: Option<String>,
    pub end_fx: Option<String>,
    pub attribute_modifier: Option<String>,
    pub attribute_start_delay_ms: Option<u32>,
    pub attribute_modifier_while_active: bool,
    pub attribute_duration_ms: Option<u32>,
    pub ai_state: Option<EmotionAiState>,
    pub ai_lock_duration_ms: Option<u32>,
    pub model_conditions: Vec<String>,
    pub model_conditions_clear: Vec<String>,
    pub model_conditions_set_on_exit: Vec<String>,
    pub model_conditions_clear_on_exit: Vec<String>,
    pub lua_event: Option<String>,
    pub prevent_player_commands: bool,
}

fn scalar<T>(
    block: &Block,
    keyword: &str,
    context: &str,
    errors: &ValueErrors,
    expected: &'static str,
    parser: impl FnOnce(&str) -> Option<T>,
) -> Option<T> {
    let field = block.field(keyword)?;
    let read = field.value().and_then(parser);
    if read.is_none() {
        errors.field(context, field, expected);
    }
    read
}

fn parse_ms(token: &str) -> Option<u32> {
    if let Some(whole) = parse_int(token) {
        return u32::try_from(whole).ok();
    }
    let value = parse_float(token)?;
    (value.is_finite() && (0.0..=f32::from(u16::MAX) * 2000.0).contains(&value)).then(|| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let whole = value as u32;
        whole
    })
}

fn parse_threat(token: &str) -> Option<f32> {
    parse_float(token).or_else(|| parse_percent(token))
}

fn flag(block: &Block, keyword: &str, context: &str, errors: &ValueErrors, fallback: bool) -> bool {
    scalar(block, keyword, context, errors, "Yes or No", parse_bool).unwrap_or(fallback)
}

fn name_of(block: &Block, keyword: &str) -> Option<String> {
    block
        .field(keyword)
        .and_then(Field::value)
        .filter(|token| !crate::value::is_none(token))
        .map(str::to_owned)
}

fn tokens(block: &Block, keyword: &str, fallback: Vec<String>) -> Vec<String> {
    block.field(keyword).map_or(fallback, |field| {
        field
            .args
            .iter()
            .map(|token| token.to_ascii_uppercase())
            .collect()
    })
}

#[must_use]
pub fn read_emotion_nugget(
    block: &Block,
    base: Option<&EmotionNuggetDef>,
    errors: &ValueErrors,
) -> EmotionNuggetDef {
    let from = base.cloned().unwrap_or_default();
    let context = block.name().unwrap_or(EMOTION_NUGGET_KEYWORD).to_owned();
    let ms = |keyword: &str, fallback: Option<u32>| -> Option<u32> {
        scalar(
            block,
            keyword,
            &context,
            errors,
            "a whole number of milliseconds",
            parse_ms,
        )
        .or(fallback)
    };
    let threat = |keyword: &str, fallback: Option<f32>| -> Option<f32> {
        scalar(block, keyword, &context, errors, "a number", parse_threat).or(fallback)
    };
    EmotionNuggetDef {
        name: from.name,
        emotion_type: scalar(
            block,
            "Type",
            &context,
            errors,
            "an emotion type",
            EmotionType::parse,
        )
        .or(from.emotion_type),
        ignore_if_unit_idle: flag(
            block,
            "IgnoreIfUnitIdle",
            &context,
            errors,
            from.ignore_if_unit_idle,
        ),
        ignore_if_unit_busy: flag(
            block,
            "IgnoreIfUnitBusy",
            &context,
            errors,
            from.ignore_if_unit_busy,
        ),
        duration_ms: ms("Duration", from.duration_ms),
        inactive_duration_ms: ms("InactiveDuration", from.inactive_duration_ms),
        inactive_duration_same_object_ms: ms(
            "InactiveDurationSameObject",
            from.inactive_duration_same_object_ms,
        ),
        inactive_duration_same_type_ms: ms(
            "InactiveDurationSameType",
            from.inactive_duration_same_type_ms,
        ),
        only_if_enemy_threat_above: threat(
            "OnlyIfEnemyThreatAbove",
            from.only_if_enemy_threat_above,
        ),
        only_if_enemy_threat_below: threat(
            "OnlyIfEnemyThreatBelow",
            from.only_if_enemy_threat_below,
        ),
        only_if_friend_threat_above: threat(
            "OnlyIfFriendThreatAbove",
            from.only_if_friend_threat_above,
        ),
        only_if_friend_threat_below: threat(
            "OnlyIfFriendThreatBelow",
            from.only_if_friend_threat_below,
        ),
        ignore_if_ai: flag(block, "IgnoreIfAI", &context, errors, from.ignore_if_ai),
        ignore_if_human: flag(
            block,
            "IgnoreIfHuman",
            &context,
            errors,
            from.ignore_if_human,
        ),
        start_fx: name_of(block, "StartFXList").or(from.start_fx),
        update_fx: name_of(block, "UpdateFXList").or(from.update_fx),
        end_fx: name_of(block, "EndFXList").or(from.end_fx),
        attribute_modifier: name_of(block, "AttributeModifier").or(from.attribute_modifier),
        attribute_start_delay_ms: ms("AttributeStartDelay", from.attribute_start_delay_ms),
        attribute_modifier_while_active: flag(
            block,
            "AttributeModifierWhileEmotionActive",
            &context,
            errors,
            from.attribute_modifier_while_active,
        ),
        attribute_duration_ms: ms("AttributeDuration", from.attribute_duration_ms),
        ai_state: scalar(
            block,
            "AIState",
            &context,
            errors,
            "an AI state",
            EmotionAiState::parse,
        )
        .or(from.ai_state),
        ai_lock_duration_ms: ms("AILockDuration", from.ai_lock_duration_ms),
        model_conditions: tokens(block, "ModelConditions", from.model_conditions),
        model_conditions_clear: tokens(block, "ModelConditionsClear", from.model_conditions_clear),
        model_conditions_set_on_exit: tokens(
            block,
            "ModelConditionsSetOnExit",
            from.model_conditions_set_on_exit,
        ),
        model_conditions_clear_on_exit: tokens(
            block,
            "ModelConditionsClearOnExit",
            from.model_conditions_clear_on_exit,
        ),
        lua_event: name_of(block, "LuaEvent").or(from.lua_event),
        prevent_player_commands: flag(
            block,
            "PreventPlayerCommands",
            &context,
            errors,
            from.prevent_player_commands,
        ),
    }
}

#[derive(Debug, Clone, Default)]
pub struct EmotionIndex {
    defs: BTreeMap<String, EmotionNuggetDef>,
}

impl EmotionIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_document(&mut self, document: &Document, errors: &ValueErrors) {
        for block in document.roots() {
            if !block.keyword.eq_ignore_ascii_case(EMOTION_NUGGET_KEYWORD) {
                continue;
            }
            let Some(name) = block.name() else {
                continue;
            };
            let base = block
                .field("CopyFrom")
                .and_then(Field::value)
                .and_then(|from| self.get(from))
                .cloned();
            let mut def = read_emotion_nugget(block, base.as_ref(), errors);
            def.name = name.to_ascii_lowercase();
            self.defs.insert(def.name.clone(), def);
        }
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&EmotionNuggetDef> {
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
    pub fn resolve(&self, declaration: &Block, errors: &ValueErrors) -> Option<EmotionNuggetDef> {
        let named = declaration
            .args
            .iter()
            .find(|token| !token.eq_ignore_ascii_case(OVERRIDE_TOKEN));
        match named.and_then(|name| self.get(name)) {
            Some(base) => Some(read_emotion_nugget(declaration, Some(base), errors)),
            None => {
                errors.value(
                    ADD_EMOTION_KEYWORD,
                    &Field {
                        keyword: declaration.keyword.clone(),
                        args: declaration.args.clone(),
                        file: Arc::clone(&declaration.file),
                        line: declaration.line,
                    },
                    named.map_or("", String::as_str),
                    "a declared EmotionNugget",
                );
                None
            }
        }
    }

    #[must_use]
    pub fn compose_tracker(&self, module: &Block, errors: &ValueErrors) -> Vec<EmotionNuggetDef> {
        let context = module
            .args
            .get(1)
            .map_or(ADD_EMOTION_KEYWORD, String::as_str);
        let mut composed = Vec::new();
        for child in &module.children {
            match child {
                Node::Field(field) if field.keyword.eq_ignore_ascii_case(ADD_EMOTION_KEYWORD) => {
                    let Some(name) = field.args.first() else {
                        errors.field(context, field, "a nugget name");
                        continue;
                    };
                    match self.get(name) {
                        Some(def) => composed.push(def.clone()),
                        None => errors.value(context, field, name, "a declared EmotionNugget"),
                    }
                }
                Node::Block(block) if block.keyword.eq_ignore_ascii_case(ADD_EMOTION_KEYWORD) => {
                    composed.extend(self.resolve(block, errors));
                }
                _ => {}
            }
        }
        composed
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::table::BlockTable;

    fn parse(text: &str) -> Document {
        Document::parse("emotions.ini", text, &BlockTable::corpus_path()).expect("parses")
    }

    const FEAR_IDLE: &str = "EmotionNugget FearIdle_Base\n  \
           Type = FEAR\n  \
           IgnoreIfUnitBusy = Yes\n  \
           AIState = BACK_AWAY\n  \
           AILockDuration = 4000\n  \
           ModelConditions = EMOTION_AFRAID\n  \
           InactiveDurationSameObject = 10000\n  \
           StartFXList = FX_EmotionCower\n  \
           EndFXList = FX_EmotionRegroup\n\
         End\n";

    fn index_of(text: &str) -> (EmotionIndex, ValueErrors) {
        let document = parse(text);
        let errors = ValueErrors::new();
        let mut index = EmotionIndex::new();
        index.insert_document(&document, &errors);
        (index, errors)
    }

    #[test]
    fn a_nugget_reads_every_field_it_writes_as_its_own_type() {
        let (index, errors) = index_of(FEAR_IDLE);
        assert!(errors.is_empty());
        let fear = index.get("fearidle_base").unwrap();
        assert_eq!(fear.emotion_type, Some(EmotionType::Fear));
        assert_eq!(fear.ai_state, Some(EmotionAiState::BackAway));
        assert_eq!(fear.ai_lock_duration_ms, Some(4000));
        assert_eq!(fear.inactive_duration_same_object_ms, Some(10_000));
        assert!(fear.ignore_if_unit_busy);
        assert!(!fear.ignore_if_unit_idle);
        assert_eq!(fear.model_conditions, ["EMOTION_AFRAID"]);
        assert_eq!(fear.start_fx.as_deref(), Some("FX_EmotionCower"));
        assert_eq!(fear.end_fx.as_deref(), Some("FX_EmotionRegroup"));
        assert_eq!(fear.duration_ms, None, "absent means until the cause goes");
    }

    #[test]
    fn a_value_that_is_not_its_fields_type_is_refused_rather_than_defaulted() {
        let (_, errors) = index_of(
            "EmotionNugget Bad\n  Type = SMUG\n  AIState = DANCE\n  \
             Duration = soon\n  PreventPlayerCommands = Maybe\nEnd\n",
        );
        assert_eq!(errors.len(), 4);
    }

    #[test]
    fn an_override_block_copies_the_named_nugget_and_replaces_what_it_writes() {
        let (index, _) = index_of(FEAR_IDLE);
        let document = parse(
            "Object T\n  Behavior = EmotionTrackerUpdate Module_Emotion\n    \
               AddEmotion = FearIdle_Base\n    \
               AddEmotion = OVERRIDE FearIdle_Base\n      Duration = 7000\n    End\n  \
             End\nEnd\n",
        );
        let module = document.roots()[0].blocks().next().unwrap();
        let errors = ValueErrors::new();
        let composed = index.compose_tracker(module, &errors);
        assert!(errors.is_empty());
        assert_eq!(composed.len(), 2, "a reference and an override, in order");
        assert_eq!(composed[0].duration_ms, None);
        assert_eq!(composed[1].duration_ms, Some(7000));
        assert_eq!(
            composed[1].ai_state,
            Some(EmotionAiState::BackAway),
            "a field the override does not write keeps the base's"
        );
        assert_eq!(composed[1].ai_lock_duration_ms, Some(4000));
    }

    #[test]
    fn a_tracker_naming_an_undeclared_nugget_is_refused() {
        let (index, _) = index_of(FEAR_IDLE);
        let document = parse(
            "Object T\n  Behavior = EmotionTrackerUpdate Module_Emotion\n    \
               AddEmotion = NoSuchNugget\n  End\nEnd\n",
        );
        let module = document.roots()[0].blocks().next().unwrap();
        let errors = ValueErrors::new();
        assert!(index.compose_tracker(module, &errors).is_empty());
        assert_eq!(errors.len(), 1);
    }
}
