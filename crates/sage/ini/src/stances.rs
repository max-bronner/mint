use std::collections::BTreeMap;
use std::sync::Arc;

use crate::diagnostic::ValueErrors;
use crate::melee_behavior::{find_melee_behavior, MeleeBehaviorDef};
use crate::tree::{Document, Field};

pub const STANCE_TEMPLATE_KEYWORD: &str = "StanceTemplate";

pub const STANCE_KEYWORD: &str = "Stance";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stance {
    Uninitialized,
    Battle,
    HoldGround,
    Porcupine,
    HoldGroundMoving,
    Aggressive,
}

impl Stance {
    pub const ALL: [Self; 6] = [
        Self::Uninitialized,
        Self::Battle,
        Self::HoldGround,
        Self::Porcupine,
        Self::HoldGroundMoving,
        Self::Aggressive,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Uninitialized => "Uninitialized",
            Self::Battle => "Battle",
            Self::HoldGround => "HoldGround",
            Self::Porcupine => "Porcupine",
            Self::HoldGroundMoving => "HoldGroundMoving",
            Self::Aggressive => "Aggressive",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|stance| name.eq_ignore_ascii_case(stance.as_str()))
    }

    #[must_use]
    pub const fn index(self) -> u8 {
        match self {
            Self::Uninitialized => 0,
            Self::Battle => 1,
            Self::HoldGround => 2,
            Self::Porcupine => 3,
            Self::HoldGroundMoving => 4,
            Self::Aggressive => 5,
        }
    }

    #[must_use]
    pub fn from_index(index: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.index() == index)
    }

    #[must_use]
    pub const fn from_engine_ordinal(ordinal: i32) -> Option<Self> {
        match ordinal {
            0 => Some(Self::Uninitialized),
            1 => Some(Self::Battle),
            2 => Some(Self::Aggressive),
            3 => Some(Self::HoldGround),
            4 => Some(Self::Porcupine),
            5 => Some(Self::HoldGroundMoving),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StanceDef {
    pub stance: Stance,
    pub attribute_modifier: Option<String>,
    pub melee_behavior: Option<MeleeBehaviorDef>,
    pub file: Arc<str>,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StanceTemplateDef {
    pub name: String,
    pub stances: Vec<StanceDef>,
}

impl StanceTemplateDef {
    #[must_use]
    pub fn stance(&self, stance: Stance) -> Option<&StanceDef> {
        self.stances.iter().find(|def| def.stance == stance)
    }
}

#[derive(Debug, Clone, Default)]
pub struct StanceTemplateIndex {
    defs: BTreeMap<String, StanceTemplateDef>,
}

impl StanceTemplateIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_document(&mut self, document: &Document, errors: &ValueErrors) {
        for block in document.roots() {
            if !block.keyword.eq_ignore_ascii_case(STANCE_TEMPLATE_KEYWORD) {
                continue;
            }
            let Some(name) = block.name() else {
                continue;
            };
            let mut def = StanceTemplateDef {
                name: name.to_owned(),
                stances: Vec::new(),
            };
            for stance_block in block.blocks() {
                if !stance_block.keyword.eq_ignore_ascii_case(STANCE_KEYWORD) {
                    continue;
                }
                let written = stance_block.name().unwrap_or_default();
                let Some(stance) = Stance::from_name(written) else {
                    errors.at(
                        &stance_block.file,
                        stance_block.line,
                        STANCE_TEMPLATE_KEYWORD,
                        STANCE_KEYWORD,
                        written,
                        "a stance name",
                    );
                    continue;
                };
                if def.stance(stance).is_some() {
                    errors.at(
                        &stance_block.file,
                        stance_block.line,
                        STANCE_TEMPLATE_KEYWORD,
                        STANCE_KEYWORD,
                        written,
                        "a stance this template has not already defined",
                    );
                    continue;
                }
                def.stances.push(StanceDef {
                    stance,
                    attribute_modifier: stance_block
                        .field("AttributeModifier")
                        .and_then(Field::value)
                        .map(str::to_owned),
                    melee_behavior: find_melee_behavior(stance_block, errors),
                    file: Arc::clone(&stance_block.file),
                    line: stance_block.line,
                });
            }
            self.defs.insert(name.to_ascii_lowercase(), def);
        }
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&StanceTemplateDef> {
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

    pub fn iter(&self) -> impl Iterator<Item = &StanceTemplateDef> {
        self.defs.values()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::table::BlockTable;
    use crate::tree::Document;

    fn index(text: &str) -> (StanceTemplateIndex, Vec<String>) {
        let document = Document::parse("stances.ini", text, &BlockTable::corpus_path()).unwrap();
        let errors = ValueErrors::new();
        let mut index = StanceTemplateIndex::new();
        index.insert_document(&document, &errors);
        (
            index,
            errors.take().iter().map(ToString::to_string).collect(),
        )
    }

    #[test]
    fn a_stance_the_engine_does_not_declare_is_refused() {
        let (index, errors) = index(
            "StanceTemplate ModTemplate\n  Stance Berserk\n    AttributeModifier ModList\n  \
             End\nEnd\n",
        );
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(index
            .get("ModTemplate")
            .is_some_and(|d| d.stances.is_empty()));
    }

    #[test]
    fn one_template_may_not_define_a_stance_twice() {
        let (_, errors) = index(
            "StanceTemplate ModTemplate\n  Stance Aggressive\n    AttributeModifier A\n  End\n  \
             Stance Aggressive\n    AttributeModifier B\n  End\nEnd\n",
        );
        assert_eq!(errors.len(), 1, "{errors:?}");
    }

    #[test]
    fn a_stance_carries_its_list_and_its_nested_melee_behavior() {
        let (index, errors) = index(
            "StanceTemplate ModTemplate\n  Stance Aggressive\n    AttributeModifier ModList\n    \
             MeleeBehavior = Amoeba\n    End\n  End\nEnd\n",
        );
        assert!(errors.is_empty(), "{errors:?}");
        let def = index
            .get("modtemplate")
            .and_then(|t| t.stance(Stance::Aggressive))
            .unwrap();
        assert_eq!(def.attribute_modifier.as_deref(), Some("ModList"));
        assert_eq!(
            def.melee_behavior.as_ref().map(|m| m.kind),
            Some(crate::melee_behavior::MeleeBehaviorKind::Amoeba)
        );
    }
}
