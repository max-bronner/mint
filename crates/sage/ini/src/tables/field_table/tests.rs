use super::*;
use crate::tables::{
    BlockSpec, ANIMATION, ANIMATION_STATE, ARMOR, MODEL_CONDITION_STATE, SPECIAL_POWER, WEAPON,
};

#[test]
fn name_maps_are_sorted_for_binary_search() {
    for map in [FILE_SCOPE, MODULES] {
        assert!(map.windows(2).all(|w| w[0].0 < w[1].0));
    }
}

#[test]
fn the_hand_written_tables_agree_with_the_generated_one() {
    let draw = module("W3DScriptedModelDraw").unwrap();
    let model_state = draw
        .child("ModelConditionState", None, Titles::BOTH)
        .unwrap();
    let anim_state = draw.child("AnimationState", None, Titles::BOTH).unwrap();
    let animation = anim_state.child("Animation", None, Titles::BOTH).unwrap();
    let cases: [(&BlockSpec, &Context); 6] = [
        (&SPECIAL_POWER, file_scope("SpecialPower").unwrap()),
        (&ARMOR, file_scope("Armor").unwrap()),
        (&WEAPON, file_scope("Weapon").unwrap()),
        (&MODEL_CONDITION_STATE, model_state),
        (&ANIMATION_STATE, anim_state),
        (&ANIMATION, animation),
    ];
    for (spec, context) in cases {
        for field in spec.field_tokens() {
            let titles: &[Titles] = if field.rotwk_only {
                &[Titles::ROTWK]
            } else {
                &[Titles::BFME2, Titles::ROTWK]
            };
            for &title in titles {
                assert!(
                    context.accepts(field.token, title),
                    "{}.{} is not in the generated table for {title:?}",
                    spec.name,
                    field.token
                );
            }
        }
    }
}
