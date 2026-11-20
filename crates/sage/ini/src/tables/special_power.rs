use super::{BlockSpec, FieldSpec, Primitive};

const SPECIAL_POWER_FLAGS: &[&str] = &[
    "NEEDS_TARGET",
    "WATER_OK",
    "NEEDS_OBJECT_FILTER",
    "LIMIT_DISTANCE",
    "NO_FORBIDDEN_OBJECTS",
    "RESPECT_RECHARGE_TIME_DISCOUNT",
    "PATHABLE_ONLY",
];

const FIELDS: &[FieldSpec] = &[
    FieldSpec {
        token: "ReloadTime",
        primitive: Primitive::DurationMs,
        rotwk_only: false,
        source: "ini-bfme-delta.md §4.4, [V] WBR",
        consumer: "special_power.rs::reload_time",
    },
    FieldSpec {
        token: "RequiredSciences",
        primitive: Primitive::IdentifierList,
        rotwk_only: false,
        source: "ini-bfme-delta.md §4.4, [V] WBR",
        consumer: "special_power.rs::read_special_power",
    },
    FieldSpec {
        token: "SharedSyncedTimer",
        primitive: Primitive::Bool,
        rotwk_only: false,
        source: "ini-bfme-delta.md §4.4, [V] WBR",
        consumer: "special_power.rs::read_special_power",
    },
    FieldSpec {
        token: "MaxCastRange",
        primitive: Primitive::Real,
        rotwk_only: false,
        source: "ini-bfme-delta.md §4.4, [V] WBR",
        consumer: "special_power.rs::read_special_power",
    },
    FieldSpec {
        token: "UnitCost",
        primitive: Primitive::Int,
        rotwk_only: true,
        source: "ini-bfme-delta.md §4.4, §8, [V] WBR",
        consumer: "special_power.rs::read_special_power",
    },
    FieldSpec {
        token: "ObjectFilter",
        primitive: Primitive::ObjectFilter,
        rotwk_only: false,
        source: "ini-bfme-delta.md §4.4, [V] WBR",
        consumer: "special_power.rs::filter_tokens",
    },
    FieldSpec {
        token: "ForbiddenObjectFilter",
        primitive: Primitive::ObjectFilter,
        rotwk_only: false,
        source: "ini-bfme-delta.md §4.4, [V] WBR",
        consumer: "special_power.rs::filter_tokens",
    },
    FieldSpec {
        token: "ForbiddenObjectRange",
        primitive: Primitive::Real,
        rotwk_only: false,
        source: "ini-bfme-delta.md §4.4, [V] WBR",
        consumer: "special_power.rs::read_special_power",
    },
    FieldSpec {
        token: "PreventActivationConditions",
        primitive: Primitive::IdentifierList,
        rotwk_only: false,
        source: "unit-d-abilities.md AB42, [V] 2.01 (0x942733)",
        consumer: "special_power.rs::read_special_power",
    },
    FieldSpec {
        token: "Flags",
        primitive: Primitive::BitFlags(SPECIAL_POWER_FLAGS),
        rotwk_only: false,
        source: "ini-bfme-delta.md §10, [V] WBR; independently reproduced by \
                 `xtask ini-tables` this round in both titles (B345)",
        consumer: "special_power.rs::SpecialPowerFlags::parse",
    },
];

pub const SPECIAL_POWER: BlockSpec = BlockSpec {
    name: "SpecialPower",
    tables: &[FIELDS],
};
