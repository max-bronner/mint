use crate::diagnostic::ValueErrors;
use crate::tree::{Block, Field};
use crate::value::parse_float;

pub const MELEE_BEHAVIOR_KEYWORD: &str = "MeleeBehavior";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeleeBehaviorKind {
    Swarm,
    WaitForLeader,
    HoldGround,
    Amoeba,
}

impl MeleeBehaviorKind {
    pub const ALL: [Self; 4] = [
        Self::Swarm,
        Self::WaitForLeader,
        Self::HoldGround,
        Self::Amoeba,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Swarm => "Swarm",
            Self::WaitForLeader => "WaitForLeader",
            Self::HoldGround => "HoldGround",
            Self::Amoeba => "Amoeba",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| name.eq_ignore_ascii_case(kind.as_str()))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AmoebaDef {
    pub facing_bonus: f32,
    pub angle_limit_cos: f32,
    pub inner_range: f32,
    pub outer_range: f32,
    pub outer_range_buildings: f32,
    pub idle_model_conditions: Vec<String>,
    pub delay_until_idle_ms: f32,
    pub delay_random_activate_min_ms: f32,
    pub delay_random_activate_max_ms: f32,
}

impl Default for AmoebaDef {
    fn default() -> Self {
        Self {
            facing_bonus: 10.0,
            angle_limit_cos: -0.17,
            inner_range: 60.0,
            outer_range: 90.0,
            outer_range_buildings: 140.0,
            idle_model_conditions: vec!["EMOTION_TAUNTING".to_owned()],
            delay_until_idle_ms: 2000.0,
            delay_random_activate_min_ms: 2000.0,
            delay_random_activate_max_ms: 3000.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeleeBehaviorDef {
    pub kind: MeleeBehaviorKind,
    pub amoeba: Option<AmoebaDef>,
}

#[must_use]
pub fn find_melee_behavior(parent: &Block, errors: &ValueErrors) -> Option<MeleeBehaviorDef> {
    let block = parent
        .blocks()
        .find(|child| child.keyword.eq_ignore_ascii_case(MELEE_BEHAVIOR_KEYWORD))?;
    read_melee_behavior(block, errors)
}

#[must_use]
pub fn read_melee_behavior(block: &Block, errors: &ValueErrors) -> Option<MeleeBehaviorDef> {
    let written = block.selector().unwrap_or_default();
    let Some(kind) = MeleeBehaviorKind::from_name(written) else {
        errors.at(
            &block.file,
            block.line,
            MELEE_BEHAVIOR_KEYWORD,
            MELEE_BEHAVIOR_KEYWORD,
            written,
            "Swarm, WaitForLeader, HoldGround or Amoeba",
        );
        return None;
    };
    let amoeba = (kind == MeleeBehaviorKind::Amoeba).then(|| read_amoeba(block, errors));
    Some(MeleeBehaviorDef { kind, amoeba })
}

fn read_amoeba(block: &Block, errors: &ValueErrors) -> AmoebaDef {
    let mut def = AmoebaDef::default();
    let number = |field: &Field, into: &mut f32| match field.value().and_then(parse_float) {
        Some(value) => *into = value,
        None => errors.field(MELEE_BEHAVIOR_KEYWORD, field, "a number"),
    };
    for field in block.fields() {
        let keyword = field.keyword.as_str();
        if keyword.eq_ignore_ascii_case("FacingBonus") {
            number(field, &mut def.facing_bonus);
        } else if keyword.eq_ignore_ascii_case("AngleLimitCos") {
            number(field, &mut def.angle_limit_cos);
        } else if keyword.eq_ignore_ascii_case("InnerRange") {
            number(field, &mut def.inner_range);
        } else if keyword.eq_ignore_ascii_case("OuterRange") {
            number(field, &mut def.outer_range);
        } else if keyword.eq_ignore_ascii_case("OuterRangeBuildings") {
            number(field, &mut def.outer_range_buildings);
        } else if keyword.eq_ignore_ascii_case("DelayUntilIdle") {
            number(field, &mut def.delay_until_idle_ms);
        } else if keyword.eq_ignore_ascii_case("DelayRandomActivateMin") {
            number(field, &mut def.delay_random_activate_min_ms);
        } else if keyword.eq_ignore_ascii_case("DelayRandomActivateMax") {
            number(field, &mut def.delay_random_activate_max_ms);
        } else if keyword.eq_ignore_ascii_case("IdleModelConditions") {
            def.idle_model_conditions = field.args.clone();
        }
    }
    def
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::table::BlockTable;
    use crate::tree::Document;

    fn first(text: &str) -> (Option<MeleeBehaviorDef>, usize) {
        let document = Document::parse("x.ini", text, &BlockTable::corpus_path()).unwrap();
        let errors = ValueErrors::new();
        let object = document.roots().first().unwrap();
        let contain = object.blocks().next().unwrap();
        let def = find_melee_behavior(contain, &errors);
        (def, errors.len())
    }

    #[test]
    fn an_amoeba_block_takes_retail_defaults_under_what_it_writes() {
        let (def, errors) = first(
            "Object X\n  Behavior = HordeContain ModuleTag_H\n    MeleeBehavior = Amoeba\n      \
             InnerRange = 30\n    End\n  End\nEnd\n",
        );
        let def = def.unwrap();
        assert_eq!(errors, 0);
        assert_eq!(def.kind, MeleeBehaviorKind::Amoeba);
        let amoeba = def.amoeba.unwrap();
        assert_eq!(amoeba.inner_range, 30.0);
        assert_eq!(
            AmoebaDef {
                inner_range: 30.0,
                ..AmoebaDef::default()
            },
            amoeba
        );
    }

    #[test]
    fn an_unknown_kind_is_refused() {
        let (def, errors) = first(
            "Object X\n  Behavior = HordeContain ModuleTag_H\n    MeleeBehavior = Blob\n    End\n  \
             End\nEnd\n",
        );
        assert_eq!(def, None);
        assert_eq!(errors, 1);
    }
}
