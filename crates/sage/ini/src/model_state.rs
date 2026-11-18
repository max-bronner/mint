use std::sync::Arc;

use crate::diagnostic::ValueErrors;
use crate::object::{
    parse_particle_sys_bone, state_models, ParticleSysBone, MODEL_STATE_KEYWORDS, NONE_VALUE,
    PARTICLE_SYS_BONE_KEYWORD,
};
use crate::tree::{Block, Field};
use crate::value::{parse_bool, parse_float_or_macro, MacroTable};

pub const MAX_MODEL_STATES_PER_DRAW: usize = 256;

pub const MAX_TEXTURE_SWAPS: usize = 32;

pub const WEAPON_SLOT_COUNT: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecalBlend {
    Alpha,
    Additive,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StateDecal {
    pub blend: DecalBlend,
    pub texture: String,
    pub size_x: f32,
    pub size_y: f32,
}

const KNOWN_SHADOW_STYLES: [&str; 11] = [
    "SHADOW_NONE",
    "SHADOW_DECAL",
    "SHADOW_VOLUME",
    "SHADOW_PROJECTION",
    "SHADOW_DYNAMIC_PROJECTION",
    "SHADOW_DIRECTIONAL_PROJECTION",
    "SHADOW_ALPHA_DECAL",
    "SHADOW_ADDITIVE_DECAL",
    "SHADOW_ALPHA_DECAL_DYNAMIC",
    "SHADOW_ADDITIVE_DECAL_DYNAMIC",
    "SHADOW_MERGE_DECAL",
];

#[derive(Debug, Clone, Default)]
struct DecalFields {
    style: Option<Option<DecalBlend>>,
    texture: Option<String>,
    size_x: Option<f32>,
    size_y: Option<f32>,
}

impl DecalFields {
    fn read(base: &Self, state: &Block, macros: &MacroTable, errors: &ValueErrors) -> Self {
        let context = context_of(state);
        let size = |name: &str| -> Option<f32> {
            let field = state.field(name)?;
            let read = field
                .value()
                .and_then(|raw| parse_float_or_macro(raw, macros));
            if read.is_none() {
                errors.field(&context, field, "a number or a #define");
            }
            read
        };
        Self {
            style: state
                .field("Shadow")
                .map(|field| {
                    let found = field.args.iter().find_map(|token| {
                        if token.eq_ignore_ascii_case("SHADOW_ALPHA_DECAL") {
                            Some(DecalBlend::Alpha)
                        } else if token.eq_ignore_ascii_case("SHADOW_ADDITIVE_DECAL") {
                            Some(DecalBlend::Additive)
                        } else {
                            None
                        }
                    });
                    let recognised = found.is_some()
                        || field.args.iter().any(|token| {
                            KNOWN_SHADOW_STYLES
                                .iter()
                                .any(|known| known.eq_ignore_ascii_case(token))
                        });
                    if !recognised {
                        errors.field(&context, field, "a shadow style");
                    }
                    found
                })
                .or(base.style),
            texture: state
                .field("ShadowTexture")
                .and_then(Field::value)
                .map(str::to_owned)
                .or_else(|| base.texture.clone()),
            size_x: size("ShadowSizeX").or(base.size_x),
            size_y: size("ShadowSizeY").or(base.size_y),
        }
    }

    fn decal(&self) -> Option<StateDecal> {
        let texture = self
            .texture
            .clone()
            .filter(|name| !name.eq_ignore_ascii_case(NONE_VALUE))?;
        let size_x = self.size_x.filter(|x| x.is_finite() && *x > 0.0)?;
        Some(StateDecal {
            blend: self.style.flatten()?,
            texture,
            size_x,
            size_y: self
                .size_y
                .filter(|y| y.is_finite() && *y > 0.0)
                .unwrap_or(size_x),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelStateDef {
    pub conditions: Vec<String>,
    pub model: Option<String>,
    pub textures: Vec<(String, String)>,
    pub particles: Vec<ParticleSysBone>,
    pub decal: Option<StateDecal>,
    pub launch_bones: [Option<String>; WEAPON_SLOT_COUNT],
    pub retain_sub_objects: bool,
    pub model_animation_prefix: Option<String>,
    pub portrait_image: Option<String>,
    pub button_image: Option<String>,
    pub file: Arc<str>,
    pub line: u32,
}

impl ModelStateDef {
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.conditions.is_empty()
    }
}

#[must_use]
pub fn draw_model_states(draw: &Block, macros: &MacroTable) -> Vec<ModelStateDef> {
    draw_model_states_collecting(draw, macros, &ValueErrors::new())
}

#[must_use]
pub fn draw_model_states_collecting(
    draw: &Block,
    macros: &MacroTable,
    errors: &ValueErrors,
) -> Vec<ModelStateDef> {
    let module_flag = draw
        .field(crate::object::PARTICLES_ATTACHED_TO_ANIMATED_BONES_KEYWORD)
        .and_then(Field::value)
        .and_then(parse_bool)
        == Some(true);
    let mut states: Vec<ModelStateDef> = Vec::new();
    let mut default: Option<ModelStateDef> = None;
    let mut default_decal = DecalFields::default();
    for state in draw.blocks() {
        if states.len() >= MAX_MODEL_STATES_PER_DRAW {
            break;
        }
        if !MODEL_STATE_KEYWORDS
            .iter()
            .any(|keyword| state.keyword.eq(keyword))
        {
            continue;
        }
        let conditions: Vec<String> = state
            .args
            .iter()
            .filter(|token| !token.eq_ignore_ascii_case(NONE_VALUE))
            .map(|token| token.to_ascii_uppercase())
            .collect();
        let own_model = own_model(state);
        let mut textures = if conditions.is_empty() {
            Vec::new()
        } else {
            default
                .as_ref()
                .map(|d| d.textures.clone())
                .unwrap_or_default()
        };
        for field in state.fields_named("Texture") {
            let (Some(from), Some(to)) = (field.args.first(), field.args.get(1)) else {
                continue;
            };
            textures.retain(|(held, _)| !held.eq_ignore_ascii_case(from));
            if textures.len() < MAX_TEXTURE_SWAPS {
                textures.push((from.clone(), to.clone()));
            }
        }
        let model = match own_model {
            OwnModel::Named(name) => Some(name),
            OwnModel::None => None,
            OwnModel::Absent => {
                if conditions.is_empty() {
                    None
                } else {
                    default.as_ref().and_then(|d| d.model.clone())
                }
            }
        };
        let mut particles = if conditions.is_empty() {
            Vec::new()
        } else {
            default
                .as_ref()
                .map(|d| d.particles.clone())
                .unwrap_or_default()
        };
        particles.extend(
            state
                .fields_named(PARTICLE_SYS_BONE_KEYWORD)
                .filter_map(|field| parse_particle_sys_bone(&field.args))
                .map(|mut attachment| {
                    attachment.module_flag = module_flag;
                    attachment
                }),
        );
        let mut launch_bones = if conditions.is_empty() {
            Default::default()
        } else {
            default
                .as_ref()
                .map(|d| d.launch_bones.clone())
                .unwrap_or_default()
        };
        read_launch_bones(state, &mut launch_bones, errors);
        let inherited_retain =
            !conditions.is_empty() && default.as_ref().is_some_and(|d| d.retain_sub_objects);
        let retain_sub_objects = match state.field("RetainSubObjects") {
            Some(field) => field.value().and_then(parse_bool).unwrap_or_else(|| {
                errors.field(&context_of(state), field, "Yes or No");
                inherited_retain
            }),
            None => inherited_retain,
        };
        let inherited = |own: &str, pick: fn(&ModelStateDef) -> &Option<String>| {
            state
                .field(own)
                .and_then(Field::value)
                .map(str::to_owned)
                .or_else(|| {
                    default
                        .as_ref()
                        .filter(|_| !conditions.is_empty())
                        .and_then(|d| pick(d).clone())
                })
        };
        let model_animation_prefix =
            inherited("ModelAnimationPrefix", |d| &d.model_animation_prefix);
        let portrait_image = inherited("PortraitImageName", |d| &d.portrait_image);
        let button_image = inherited("ButtonImageName", |d| &d.button_image);
        let decal_fields = if conditions.is_empty() {
            DecalFields::read(&DecalFields::default(), state, macros, errors)
        } else {
            DecalFields::read(&default_decal, state, macros, errors)
        };
        let def = ModelStateDef {
            conditions,
            model,
            textures,
            particles,
            decal: decal_fields.decal(),
            launch_bones,
            retain_sub_objects,
            model_animation_prefix,
            portrait_image,
            button_image,
            file: state.file.clone(),
            line: state.line,
        };
        if def.is_default() && default.is_none() {
            default = Some(def.clone());
            default_decal = decal_fields;
        }
        states.push(def);
    }
    states
}

fn context_of(state: &Block) -> String {
    if state.args.is_empty() {
        "DefaultModelConditionState".to_owned()
    } else {
        state.args.join(" ")
    }
}

fn read_launch_bones(
    state: &Block,
    bones: &mut [Option<String>; WEAPON_SLOT_COUNT],
    errors: &ValueErrors,
) {
    for field in state.fields_named("WeaponLaunchBone") {
        let slot = field
            .args
            .first()
            .and_then(|token| crate::command_button::weapon_slot(token));
        let (Some(slot), Some(bone)) = (slot, field.args.get(1)) else {
            errors.field(
                &context_of(state),
                field,
                "a weapon slot, PRIMARY to QUINARY, and a bone name",
            );
            continue;
        };
        if let Some(held) = bones.get_mut(usize::from(slot)) {
            *held = (!bone.eq_ignore_ascii_case(NONE_VALUE)).then(|| bone.clone());
        }
    }
}

enum OwnModel {
    Named(String),
    None,
    Absent,
}

fn own_model(state: &Block) -> OwnModel {
    if let Some(name) = state_models(state)
        .find(|(_, extra)| !extra)
        .or_else(|| state_models(state).next())
        .map(|(name, _)| name)
    {
        return OwnModel::Named(name.to_owned());
    }
    if state.field("Model").is_some() {
        OwnModel::None
    } else {
        OwnModel::Absent
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{BlockTable, Document};

    const KEEP: &str = "\
Object Keep
  Draw = W3DScriptedModelDraw Tag
    DefaultModelConditionState
      Model = KeepBody
    End
    ModelConditionState = DAMAGED
      Model = KeepBody_D
    End
    ModelConditionState = SNOW
      Texture = KeepSkin.tga KeepSkin_Snow.tga
      Texture = KeepFlag.tga KeepFlag_Snow.tga
    End
    ModelConditionState = DAMAGED SNOW
      Model = KeepBody_D
      Texture = KeepSkin.tga KeepSkin_DSnow.tga
    End
    ModelConditionState = RUBBLE
      Model = NONE
    End
  End
End
";

    fn states(text: &str) -> Vec<ModelStateDef> {
        let table = BlockTable::object_model_path();
        let document = Document::parse("t.ini", text, &table).unwrap();
        let draw = document.roots()[0].blocks().next().unwrap().clone();
        draw_model_states(&draw, &MacroTable::new())
    }

    #[test]
    fn every_state_is_read_in_order_with_its_conditions() {
        let found = states(KEEP);
        assert_eq!(found.len(), 5);
        assert!(found[0].is_default());
        assert_eq!(found[1].conditions, ["DAMAGED"]);
        assert_eq!(found[3].conditions, ["DAMAGED", "SNOW"]);
        assert_eq!(found[3].line, 13);
    }

    #[test]
    fn a_state_with_no_model_line_wears_the_defaults() {
        let found = states(KEEP);
        assert_eq!(found[2].model.as_deref(), Some("KeepBody"));
        assert_eq!(found[1].model.as_deref(), Some("KeepBody_D"));
        assert_eq!(found[4].model, None);
    }

    #[test]
    fn texture_lines_are_substitution_pairs_and_a_later_one_wins() {
        let found = states(KEEP);
        assert_eq!(
            found[2].textures,
            [
                ("KeepSkin.tga".to_owned(), "KeepSkin_Snow.tga".to_owned()),
                ("KeepFlag.tga".to_owned(), "KeepFlag_Snow.tga".to_owned()),
            ]
        );
        assert_eq!(
            found[3].textures,
            [("KeepSkin.tga".to_owned(), "KeepSkin_DSnow.tga".to_owned())]
        );
        assert!(found[0].textures.is_empty());
    }

    #[test]
    fn a_states_own_substitution_replaces_the_defaults_for_the_same_texture() {
        let text = "Object A\n  Draw = W3DScriptedModelDraw T\n    DefaultModelConditionState\n      Model = M\n      Texture = Skin.tga Skin_Base.tga\n      Texture = Flag.tga Flag_Base.tga\n    End\n    ModelConditionState = SNOW\n      Texture = skin.TGA Skin_Snow.tga\n    End\n  End\nEnd\n";
        let found = states(text);
        assert_eq!(
            found[1].textures,
            [
                ("Flag.tga".to_owned(), "Flag_Base.tga".to_owned()),
                ("skin.TGA".to_owned(), "Skin_Snow.tga".to_owned()),
            ]
        );
    }

    #[test]
    fn a_child_inherits_the_model_states_of_the_ancestor_with_the_model() {
        let text = format!("{KEEP}ChildObject KeepVariant Keep\n  Side = Men\nEnd\n");
        let table = BlockTable::object_model_path();
        let document = Document::parse("t.ini", &text, &table).unwrap();
        let mut index = crate::ObjectIndex::new();
        index.insert_document(&document);
        assert_eq!(index.model_states("Keep").len(), 5);
        assert_eq!(
            index.model_states("keepvariant"),
            index.model_states("Keep")
        );
        assert!(index.model_states("Nobody").is_empty());
    }

    #[test]
    fn particle_sys_bone_follows_the_default_states_leaf() {
        let text = "Object A\n  Draw = W3DScriptedModelDraw T\n    DefaultModelConditionState\n      Model = M\n      ParticleSysBone = AMBIENTBONE01 FireFlies02\n    End\n    ModelConditionState = ACTIVELY_BEING_CONSTRUCTED PARTIALLY_CONSTRUCTED\n      Model = M_A\n      ParticleSysBone = CONSTDUSTBONE01 BuildingContructDust FollowBone:Yes\n    End\n  End\nEnd\n";
        let found = states(text);
        assert_eq!(found[0].particles.len(), 1);
        assert_eq!(found[1].particles.len(), 2);
        let inherited = &found[1].particles[0];
        assert_eq!(inherited.bone.as_deref(), Some("AMBIENTBONE01"));
        assert_eq!(inherited.system, "FireFlies02");
        let own = &found[1].particles[1];
        assert_eq!(own.bone.as_deref(), Some("CONSTDUSTBONE01"));
        assert_eq!(own.system, "BuildingContructDust");
        assert!(own.follow_bone);
    }

    #[test]
    fn a_states_shadow_fields_make_its_ground_decal() {
        let text = "#define RING 440\nObject A\n  Draw = W3DScriptedModelDraw T\n    DefaultModelConditionState\n      Model = M\n    End\n    ModelConditionState = BUILD_PLACEMENT_CURSOR\n      Shadow = SHADOW_ALPHA_DECAL\n      ShadowTexture = Ring\n      ShadowSizeX = RING\n      ShadowSizeY = RING\n    End\n    ModelConditionState = USER_1\n      Shadow = SHADOW_ADDITIVE_DECAL_DYNAMIC\n      ShadowTexture = Glow\n      ShadowSizeX = 200\n    End\n    ModelConditionState = SNOW\n    End\n  End\nEnd\nObject B\n  Draw = W3DScriptedModelDraw T\n    DefaultModelConditionState\n      Model = M\n      Shadow = SHADOW_ADDITIVE_DECAL\n      ShadowTexture = Glow\n      ShadowSizeX = 200\n    End\n    ModelConditionState = DAMAGED\n    End\n  End\nEnd\n";
        let table = BlockTable::object_model_path();
        let document = Document::parse("t.ini", text, &table).unwrap();
        let mut macros = MacroTable::new();
        macros.insert_defines(document.defines());
        let read = |at: usize| {
            let draw = document.roots()[at].blocks().next().unwrap().clone();
            draw_model_states(&draw, &macros)
        };
        let a = read(0);
        assert_eq!(
            a[1].decal,
            Some(StateDecal {
                blend: DecalBlend::Alpha,
                texture: "Ring".to_owned(),
                size_x: 440.0,
                size_y: 440.0,
            })
        );
        assert_eq!(a[0].decal, None);
        assert_eq!(a[2].decal, None);
        assert_eq!(a[3].decal, None);
        let b = read(1);
        let glow = b[0].decal.clone().unwrap();
        assert_eq!((glow.blend, glow.size_y), (DecalBlend::Additive, 200.0));
        assert_eq!(b[1].decal, b[0].decal);
    }

    #[test]
    fn an_unreadable_decal_field_is_refused_not_dropped() {
        let text = "Object A\n  Draw = W3DScriptedModelDraw T\n    DefaultModelConditionState\n      Model = M\n    End\n    ModelConditionState = BAD_STYLE\n      Shadow = SHADOW_MADE_UP\n      ShadowTexture = Ring\n      ShadowSizeX = 200\n    End\n    ModelConditionState = BAD_SIZE\n      Shadow = SHADOW_ALPHA_DECAL\n      ShadowTexture = Ring\n      ShadowSizeX = sideways\n    End\n    ModelConditionState = BAD_MACRO\n      Shadow = SHADOW_ALPHA_DECAL\n      ShadowTexture = Ring\n      ShadowSizeX = UNDEFINED_MACRO\n    End\n    ModelConditionState = USER_1\n      Shadow = SHADOW_ADDITIVE_DECAL_DYNAMIC\n      ShadowTexture = Glow\n      ShadowSizeX = 200\n    End\n  End\nEnd\n";
        let table = BlockTable::object_model_path();
        let document = Document::parse("t.ini", text, &table).unwrap();
        let draw = document.roots()[0].blocks().next().unwrap().clone();
        let errors = ValueErrors::new();
        let found = draw_model_states_collecting(&draw, &MacroTable::new(), &errors);
        assert_eq!(found[1].decal, None, "an unknown style draws nothing");
        assert_eq!(found[2].decal, None, "an unparseable size draws nothing");
        assert_eq!(found[3].decal, None, "an unresolved macro draws nothing");
        assert_eq!(
            found[4].decal, None,
            "_DYNAMIC is recognised but undrawn, not refused"
        );
        let refused = errors.take();
        assert_eq!(refused.len(), 3, "{refused:?}");
        assert_eq!(refused[0].context, "BAD_STYLE");
        assert_eq!(refused[0].field, "Shadow");
        assert_eq!(refused[1].context, "BAD_SIZE");
        assert_eq!(refused[1].field, "ShadowSizeX");
        assert_eq!(refused[2].context, "BAD_MACRO");
        assert_eq!(refused[2].field, "ShadowSizeX");
    }

    #[test]
    fn retain_sub_objects_inherits_the_default_and_refuses_a_non_boolean() {
        let text = "Object A
  Draw = W3DScriptedModelDraw T
    DefaultModelConditionState
      Model = M
      RetainSubObjects = Yes
    End
    ModelConditionState = DAMAGED
      Model = M_D
    End
    ModelConditionState = RUBBLE
      RetainSubObjects = No
    End
    ModelConditionState = SNOW
      RetainSubObjects = Sometimes
    End
  End
End
";
        let table = BlockTable::object_model_path();
        let document = Document::parse("t.ini", text, &table).unwrap();
        let draw = document.roots()[0].blocks().next().unwrap().clone();
        let errors = ValueErrors::new();
        let found = draw_model_states_collecting(&draw, &MacroTable::new(), &errors);
        let retained: Vec<bool> = found.iter().map(|state| state.retain_sub_objects).collect();
        assert_eq!(retained, [true, true, false, true], "inherited");
        let refused = errors.take();
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert_eq!(refused[0].context, "SNOW");
        assert_eq!(refused[0].field, "RetainSubObjects");
    }

    #[test]
    fn prefix_portrait_and_button_are_read_per_state_and_inherit_the_default() {
        let text = "Object A
  Draw = W3DScriptedModelDraw T
    DefaultModelConditionState
      Model = M
      ModelAnimationPrefix = PFX_A
      PortraitImageName = PortA
      ButtonImageName = ButtA
    End
    ModelConditionState = DAMAGED
      ModelAnimationPrefix = PFX_B
      PortraitImageName = PortB
      ButtonImageName = ButtB
    End
    ModelConditionState = RUBBLE
      ButtonImageName = ButtC
    End
  End
End
";
        let table = BlockTable::object_model_path();
        let document = Document::parse("t.ini", text, &table).unwrap();
        let draw = document.roots()[0].blocks().next().unwrap().clone();
        let found = draw_model_states(&draw, &MacroTable::new());
        let read: Vec<_> = found
            .iter()
            .map(|s| {
                (
                    s.model_animation_prefix.as_deref(),
                    s.portrait_image.as_deref(),
                    s.button_image.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            read,
            [
                (Some("PFX_A"), Some("PortA"), Some("ButtA")),
                (Some("PFX_B"), Some("PortB"), Some("ButtB")),
                (Some("PFX_A"), Some("PortA"), Some("ButtC")),
            ]
        );
    }

    #[test]
    fn launch_bones_fold_per_slot_and_refuse_a_malformed_line() {
        let text = "Object A
  Draw = W3DScriptedModelDraw T
    DefaultModelConditionState
      Model = M
      WeaponLaunchBone = PRIMARY Muzzle
      WeaponLaunchBone = secondary Pot
    End
    ModelConditionState = DAMAGED
      WeaponLaunchBone = SECONDARY Pot_D
    End
    ModelConditionState = RUBBLE
      WeaponLaunchBone = PRIMARY NONE
      WeaponLaunchBone = SEPTENARY Elsewhere
      WeaponLaunchBone = TERTIARY
    End
  End
End
";
        let table = BlockTable::object_model_path();
        let document = Document::parse("t.ini", text, &table).unwrap();
        let draw = document.roots()[0].blocks().next().unwrap().clone();
        let errors = ValueErrors::new();
        let found = draw_model_states_collecting(&draw, &MacroTable::new(), &errors);
        let slot = |at: usize, slot: usize| found[at].launch_bones[slot].as_deref();
        assert_eq!(
            (slot(0, 0), slot(0, 1), slot(0, 2)),
            (Some("Muzzle"), Some("Pot"), None)
        );
        assert_eq!((slot(1, 0), slot(1, 1)), (Some("Muzzle"), Some("Pot_D")));
        assert_eq!(
            (slot(2, 0), slot(2, 1), slot(2, 2)),
            (None, Some("Pot"), None)
        );
        let refused = errors.take();
        assert_eq!(refused.len(), 2, "{refused:?}");
        assert!(refused.iter().all(|error| error.context == "RUBBLE"));
    }

    #[test]
    fn a_malformed_or_oversized_module_is_bounded() {
        let one = "Object A\n  Draw = W3DScriptedModelDraw T\n    ModelConditionState = SNOW\n      Texture = Lonely.tga\n    End\n  End\nEnd\n";
        assert!(states(one)[0].textures.is_empty());
        let mut big = String::from("Object A\n  Draw = W3DScriptedModelDraw T\n");
        for _ in 0..(MAX_MODEL_STATES_PER_DRAW + 40) {
            big.push_str("    ModelConditionState = SNOW\n      Model = M\n    End\n");
        }
        big.push_str("  End\nEnd\n");
        assert_eq!(states(&big).len(), MAX_MODEL_STATES_PER_DRAW);
    }
}
