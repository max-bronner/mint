mod animation;
mod armor;
pub mod field_table;
#[rustfmt::skip]
mod generated;
mod special_power;
pub mod structure;
mod weapon;

pub use animation::{ANIMATION, ANIMATION_STATE, MODEL_CONDITION_STATE};
pub use armor::ARMOR;
pub use special_power::SPECIAL_POWER;
pub use weapon::WEAPON;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Primitive {
    Int,
    Real,
    Percent,
    Bool,
    DurationMs,
    AsciiString,
    IdentifierList,
    ObjectFilter,
    BitFlags(&'static [&'static str]),
    Block(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct FieldSpec {
    pub token: &'static str,
    pub primitive: Primitive,
    pub rotwk_only: bool,
    pub source: &'static str,
    pub consumer: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct BlockSpec {
    pub name: &'static str,
    pub tables: &'static [&'static [FieldSpec]],
}

impl BlockSpec {
    pub fn field_tokens(&self) -> impl Iterator<Item = &'static FieldSpec> {
        self.tables.iter().flat_map(|table| table.iter())
    }
}

pub const BLOCKS: &[BlockSpec] = &[
    SPECIAL_POWER,
    ARMOR,
    WEAPON,
    MODEL_CONDITION_STATE,
    ANIMATION_STATE,
    ANIMATION,
];

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn every_block_name_and_field_token_is_unique_within_its_block() {
        let mut block_names = std::collections::BTreeSet::new();
        for block in BLOCKS {
            assert!(
                block_names.insert(block.name),
                "duplicate block {}",
                block.name
            );
            let mut tokens = std::collections::BTreeSet::new();
            for field in block.field_tokens() {
                assert!(
                    tokens.insert(field.token),
                    "duplicate field {}.{}",
                    block.name,
                    field.token
                );
                assert!(!field.token.is_empty());
                assert!(!field.consumer.is_empty());
                assert!(!field.source.is_empty());
            }
        }
    }

    #[test]
    fn special_power_flags_carries_the_seven_names_b345_settled() {
        let flags = SPECIAL_POWER
            .field_tokens()
            .find(|f| f.token == "Flags")
            .unwrap();
        let Primitive::BitFlags(names) = flags.primitive else {
            panic!("Flags should be a BitFlags primitive");
        };
        assert_eq!(names.len(), 7);
        assert!(names.contains(&"NEEDS_TARGET"));
    }
}
