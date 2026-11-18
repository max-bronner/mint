use super::{BlockSpec, FieldSpec, Primitive};

const FIELDS: &[FieldSpec] = &[
    FieldSpec {
        token: "Armor",
        primitive: Primitive::Percent,
        rotwk_only: false,
        source: "ini-bfme-delta.md §2.2, [V] WBR (field count); \
                 ini-reference.md §5.3 (percent primitive)",
        consumer: "armor.rs::ArmorSet::by_damage_type",
    },
    FieldSpec {
        token: "FlankedPenalty",
        primitive: Primitive::Percent,
        rotwk_only: false,
        source: "ini-bfme-delta.md §2.2, [V] WBR (field count)",
        consumer: "armor.rs::ArmorSet::flanked_penalty",
    },
    FieldSpec {
        token: "DamageScalar",
        primitive: Primitive::Percent,
        rotwk_only: false,
        source: "ini-bfme-delta.md §2.2, [V] WBR (field count)",
        consumer: "armor.rs::ArmorSet::damage_scalar",
    },
];

pub const ARMOR: BlockSpec = BlockSpec {
    name: "Armor",
    tables: &[FIELDS],
};
