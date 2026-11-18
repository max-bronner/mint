use super::{BlockSpec, FieldSpec, Primitive};

const MODEL_CONDITION_STATE_FIELDS: &[FieldSpec] = &[
    FieldSpec {
        token: "Model",
        primitive: Primitive::AsciiString,
        rotwk_only: false,
        source: "ini-bfme-delta.md §4.3, [V] WBR",
        consumer: "object.rs::draw_animation / model naming",
    },
    FieldSpec {
        token: "ParticleSysBone",
        primitive: Primitive::AsciiString,
        rotwk_only: false,
        source: "ini-bfme-delta.md §4.3, [V] WBR",
        consumer: "object.rs::draw_particle_bones",
    },
];

const ANIMATION_STATE_FIELDS: &[FieldSpec] = &[
    FieldSpec {
        token: "StateName",
        primitive: Primitive::AsciiString,
        rotwk_only: false,
        source: "ini-bfme-delta.md §4.3, [V] WBR",
        consumer: "anim_state.rs::STATE_NAME_KEYWORD",
    },
    FieldSpec {
        token: "Flags",
        primitive: Primitive::IdentifierList,
        rotwk_only: false,
        source: "ini-bfme-delta.md §4.3/§10, [V] WBR (field present; \
                 vocabulary incomplete - 7 of 9 members read)",
        consumer: "anim_state.rs (read via crate::object::ANIMATION_FLAGS_KEYWORD)",
    },
    FieldSpec {
        token: "Animation",
        primitive: Primitive::Block("Animation"),
        rotwk_only: false,
        source: "ini-bfme-delta.md §4.3, [V] WBR",
        consumer: "anim_state.rs::AnimationStateDef",
    },
];

const ANIMATION_FIELDS: &[FieldSpec] = &[
    FieldSpec {
        token: "AnimationSpeedFactorRange",
        primitive: Primitive::Real,
        rotwk_only: false,
        source: "ini-bfme-delta.md §4.3, [V] WBR",
        consumer: "anim_state.rs::SPEED_FACTOR_KEYWORD",
    },
    FieldSpec {
        token: "Distance",
        primitive: Primitive::Real,
        rotwk_only: false,
        source: "ini-bfme-delta.md §4.3, [V] WBR",
        consumer: "anim_state.rs::DISTANCE_KEYWORD",
    },
    FieldSpec {
        token: "AnimationPriority",
        primitive: Primitive::Int,
        rotwk_only: false,
        source: "ini-bfme-delta.md §4.3, [V] WBR",
        consumer: "anim_state.rs::PRIORITY_KEYWORD",
    },
];

pub const MODEL_CONDITION_STATE: BlockSpec = BlockSpec {
    name: "ModelConditionState",
    tables: &[MODEL_CONDITION_STATE_FIELDS],
};

pub const ANIMATION_STATE: BlockSpec = BlockSpec {
    name: "AnimationState",
    tables: &[ANIMATION_STATE_FIELDS],
};

pub const ANIMATION: BlockSpec = BlockSpec {
    name: "Animation",
    tables: &[ANIMATION_FIELDS],
};
