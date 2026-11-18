use mint_movement::{Facing, Vec2};
use mint_render::Mat4;
use mint_sage_asset::Resolver;
use mint_sage_ini::{AnimationStateDef, AnimationStateKind};
use mint_sage_model::convert::{pivot_world_transforms, pose_to_matrices};
use mint_sage_model::ObjectIndex;
use mint_sage_w3d::{Animation, Hierarchy};
use mint_vfs::Vfs;

const MAX_NUMBERED: u32 = 99;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Perch {
    pub(crate) position: [f32; 3],
    pub(crate) facing: Facing,
}

pub(crate) fn numbered_bones(hierarchy: &Hierarchy, prefix: &str) -> Vec<Perch> {
    numbered_bones_in(hierarchy, &pivot_world_transforms(hierarchy), prefix)
}

fn numbered_bones_in(hierarchy: &Hierarchy, world: &[Mat4], prefix: &str) -> Vec<Perch> {
    let perch = |name: &str| {
        hierarchy
            .pivots
            .iter()
            .position(|pivot| pivot.name.eq_ignore_ascii_case(name))
            .and_then(|index| world.get(index))
            .map(|at| Perch {
                position: [at.get(0, 3), at.get(1, 3), at.get(2, 3)],
                facing: Facing::from_vector(Vec2::new(at.get(0, 0), at.get(1, 0)))
                    .unwrap_or(Facing::EAST),
            })
    };
    let mut found = Vec::new();
    for number in 1..=MAX_NUMBERED {
        let Some(at) = perch(&format!("{prefix}{number:02}")) else {
            break;
        };
        found.push(at);
    }
    if found.is_empty() {
        found.extend(perch(prefix));
    }
    found
}

pub(crate) fn passenger_bones(
    vfs: &Vfs,
    resolver: &Resolver,
    index: &ObjectIndex,
    template: &str,
    prefix: &str,
) -> Vec<Perch> {
    let drawn = index.lookup(template);
    let Some(model) = drawn.model() else {
        return Vec::new();
    };
    let Ok(loaded) = mint_sage_model::load_model(vfs, resolver, model) else {
        return Vec::new();
    };
    let Some(hierarchy) = loaded.hierarchy.as_ref() else {
        return Vec::new();
    };
    numbered_bones(hierarchy, prefix)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct SlotLaunch {
    pub(crate) rest: Vec<Perch>,
    pub(crate) poses: Vec<FiringPose>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FiringPose {
    pub(crate) requires: Vec<String>,
    pub(crate) bones: Vec<Perch>,
}

impl SlotLaunch {
    pub(crate) fn bones(&self, held: impl Fn(&str) -> bool) -> &[Perch] {
        self.poses
            .iter()
            .find(|pose| pose.requires.iter().all(|token| held(token)))
            .map_or(self.rest.as_slice(), |pose| pose.bones.as_slice())
    }
}

impl From<Vec<Perch>> for SlotLaunch {
    fn from(rest: Vec<Perch>) -> Self {
        Self {
            rest,
            poses: Vec::new(),
        }
    }
}

fn shot_tokens(slot: usize) -> Option<[String; 4]> {
    let letter = char::from(b'A'.checked_add(u8::try_from(slot).ok()?)?);
    Some([
        format!("FIRING_{letter}"),
        format!("FIRING_OR_PREATTACK_{letter}"),
        format!("FIRING_OR_RELOADING_{letter}"),
        format!("USING_WEAPON_{letter}"),
    ])
}

fn firing_states(
    states: &[AnimationStateDef],
    slot: usize,
) -> Vec<(&AnimationStateDef, Vec<String>)> {
    let Some(tokens) = shot_tokens(slot) else {
        return Vec::new();
    };
    let firing = |token: &String| tokens.get(..3).is_some_and(|own| own.contains(token));
    states
        .iter()
        .filter(|state| state.kind == AnimationStateKind::Conditioned)
        .filter(|state| state.conditions.iter().any(firing))
        .map(|state| {
            let requires = state
                .conditions
                .iter()
                .filter(|token| !tokens.contains(token))
                .cloned()
                .collect();
            (state, requires)
        })
        .collect()
}

fn posed_bones(
    hierarchy: &Hierarchy,
    animation: &Animation,
    frame: u32,
    prefix: &str,
) -> Vec<Perch> {
    let last = animation.num_frames.saturating_sub(1);
    #[allow(clippy::cast_precision_loss)]
    let frame = frame.min(last) as f32;
    let pose = mint_sage_asset::sample_pose(hierarchy, animation, frame);
    numbered_bones_in(hierarchy, &pose_to_matrices(&pose.world), prefix)
}

pub(crate) fn launch_bones(
    vfs: &Vfs,
    resolver: &Resolver,
    index: &ObjectIndex,
    template: &str,
) -> Vec<SlotLaunch> {
    let mut found = vec![SlotLaunch::default(); mint_sage_ini::WEAPON_SLOT_COUNT];
    for module in index.draw_modules(template) {
        if module.attach_to_bone.is_some() {
            continue;
        }
        let Some(state) = module.model_states.iter().find(|state| state.is_default()) else {
            continue;
        };
        let wanted: Vec<(usize, &str)> = state
            .launch_bones
            .iter()
            .enumerate()
            .filter_map(|(slot, bone)| Some((slot, bone.as_deref()?)))
            .filter(|(slot, _)| found.get(*slot).is_some_and(|held| held.rest.is_empty()))
            .collect();
        let Some(model) = state.model.as_deref().or(module.model) else {
            continue;
        };
        if wanted.is_empty() {
            continue;
        }
        let Ok(loaded) = mint_sage_model::load_model(vfs, resolver, model) else {
            continue;
        };
        let Some(hierarchy) = loaded.hierarchy.as_ref() else {
            continue;
        };
        for (slot, bone) in wanted {
            let rest = numbered_bones(hierarchy, bone);
            crate::diag::diag!(
                "weapon: {template} slot {slot} launches from {} {bone} bone(s) on {model}, the first at {:?}",
                rest.len(),
                rest.first().map(|perch| perch.position)
            );
            let poses = firing_states(module.animation_states, slot)
                .into_iter()
                .map(|(state, requires)| {
                    let frame = state.frame_for_pristine_bone_positions.unwrap_or(0);
                    let posed = state
                        .clips
                        .first()
                        .and_then(|clip| {
                            mint_sage_model::load_animation(vfs, resolver, clip.asset()).ok()
                        })
                        .map(|clip| posed_bones(&clip.hierarchy, &clip.animation, frame, bone))
                        .filter(|bones| !bones.is_empty());
                    crate::diag::diag!(
                        "weapon: {template} slot {slot} [{}] launches at frame {frame} of {}: {:?}",
                        state.conditions.join(" "),
                        state.clips.first().map_or("no clip", |clip| clip.asset()),
                        posed
                            .as_ref()
                            .and_then(|bones| bones.first())
                            .map(|perch| perch.position)
                    );
                    FiringPose {
                        requires,
                        bones: posed.unwrap_or_else(|| rest.clone()),
                    }
                })
                .collect();
            if let Some(held) = found.get_mut(slot) {
                *held = SlotLaunch { rest, poses };
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sage_w3d::{HierarchyHeader, Pivot, Quat, Vec3};

    const TURN_Z_90: Quat = Quat {
        x: 0.0,
        y: 0.0,
        z: core::f32::consts::FRAC_1_SQRT_2,
        w: core::f32::consts::FRAC_1_SQRT_2,
    };
    const NONE: Quat = Quat {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };

    fn pivot(name: &str, parent: Option<u32>, translation: Vec3, rotation: Quat) -> Pivot {
        Pivot {
            name: name.to_string(),
            parent,
            translation,
            euler_angles: Vec3::default(),
            rotation,
        }
    }

    #[test]
    fn numbered_bones_are_read_in_number_order_through_their_parents_and_stop_at_a_gap() {
        let hierarchy = Hierarchy {
            header: HierarchyHeader::default(),
            pivots: vec![
                pivot("ROOTTRANSFORM", None, Vec3::default(), NONE),
                pivot(
                    "TOP",
                    Some(0),
                    Vec3 {
                        x: 0.0,
                        y: 0.0,
                        z: 60.0,
                    },
                    TURN_Z_90,
                ),
                pivot(
                    "Arrow_02",
                    Some(1),
                    Vec3 {
                        x: 10.0,
                        y: 0.0,
                        z: 5.0,
                    },
                    NONE,
                ),
                pivot(
                    "ARROW_01",
                    Some(0),
                    Vec3 {
                        x: -18.0,
                        y: 4.0,
                        z: 66.0,
                    },
                    NONE,
                ),
                pivot(
                    "ARROW_04",
                    Some(0),
                    Vec3 {
                        x: 1.0,
                        y: 1.0,
                        z: 1.0,
                    },
                    NONE,
                ),
            ],
        };
        let bones = numbered_bones(&hierarchy, "ARROW_");
        assert_eq!(
            bones.len(),
            2,
            "ARROW_03 is missing, so ARROW_04 is not read"
        );
        assert_eq!(bones[0].position, [-18.0, 4.0, 66.0]);
        assert!(
            (bones[1].position[0] - 0.0).abs() < 1e-4 && (bones[1].position[1] - 10.0).abs() < 1e-4
        );
        assert!((bones[1].position[2] - 65.0).abs() < 1e-4);
    }

    #[test]
    fn a_bare_prefix_bone_is_read_when_no_numbered_bone_exists() {
        let hierarchy = Hierarchy {
            header: HierarchyHeader::default(),
            pivots: vec![
                pivot("ROOTTRANSFORM", None, Vec3::default(), NONE),
                pivot(
                    "B_PASSENGER",
                    Some(0),
                    Vec3 {
                        x: 3.0,
                        y: 4.0,
                        z: 5.0,
                    },
                    NONE,
                ),
            ],
        };
        let bones = numbered_bones(&hierarchy, "B_PASSENGER");
        assert_eq!(bones.len(), 1);
        assert_eq!(bones[0].position, [3.0, 4.0, 5.0]);
    }

    fn perch(x: f32) -> Perch {
        Perch {
            position: [x, 0.0, 0.0],
            facing: Facing::EAST,
        }
    }

    #[test]
    fn a_shot_leaves_by_the_first_firing_state_it_holds_every_token_of() {
        let launch = SlotLaunch {
            rest: vec![perch(0.0)],
            poses: vec![
                FiringPose {
                    requires: vec!["AIM_NEAR".to_owned(), "WEAPONSET_TOGGLE_1".to_owned()],
                    bones: vec![perch(1.0)],
                },
                FiringPose {
                    requires: vec!["WEAPONSET_TOGGLE_1".to_owned()],
                    bones: vec![perch(2.0)],
                },
            ],
        };
        let holding = |tokens: &'static [&'static str]| {
            move |token: &str| tokens.iter().any(|held| held.eq_ignore_ascii_case(token))
        };
        let x = |bones: &[Perch]| bones[0].position[0];
        assert_eq!(
            x(launch.bones(holding(&["WEAPONSET_TOGGLE_1", "AIM_NEAR"]))),
            1.0
        );
        assert_eq!(x(launch.bones(holding(&["weaponset_toggle_1"]))), 2.0);
        assert_eq!(x(launch.bones(holding(&["AIM_NEAR"]))), 0.0);
    }

    #[test]
    fn firing_states_are_the_slots_own_in_declaration_order() {
        let table = mint_sage_ini::BlockTable::object_model_path();
        let document = mint_sage_ini::Document::parse(
            "t.ini",
            "\
Object A
  Draw = W3DScriptedModelDraw Tag
    DefaultModelConditionState
      Model = M_SKN
    End
    AnimationState = FIRING_OR_PREATTACK_A AIM_NEAR WEAPONSET_TOGGLE_1
      Animation
        AnimationName = M_THRC
      End
      FrameForPristineBonePositions = 39
    End
    AnimationState = USING_WEAPON_A
      Animation
        AnimationName = M_USEA
      End
    End
    AnimationState = FIRING_B
      Animation
        AnimationName = M_FIRB
      End
    End
    AnimationState = MOVING FIRING_A USING_WEAPON_A
      Animation
        AnimationName = M_RUNF
      End
    End
  End
End
",
            &table,
        )
        .unwrap();
        let draw = document.roots()[0].blocks().next().unwrap().clone();
        let states = mint_sage_ini::draw_animation_states(&draw);
        let found: Vec<(&str, Vec<String>)> = firing_states(&states, 0)
            .into_iter()
            .map(|(state, requires)| (state.clips[0].name.as_str(), requires))
            .collect();
        assert_eq!(
            found,
            [
                (
                    "M_THRC",
                    vec!["AIM_NEAR".to_owned(), "WEAPONSET_TOGGLE_1".to_owned()]
                ),
                ("M_RUNF", vec!["MOVING".to_owned()]),
            ]
        );
    }

    #[test]
    fn a_clip_poses_the_launch_bone_at_its_pristine_frame() {
        use mint_sage_w3d::{Animation, Channel, ChannelData, ChannelKind, Value};
        let hierarchy = Hierarchy {
            header: HierarchyHeader::default(),
            pivots: vec![
                pivot("ROOTTRANSFORM", None, Vec3::default(), NONE),
                pivot(
                    "FIREPOINT01",
                    Some(0),
                    Vec3 {
                        x: 3.0,
                        y: 0.0,
                        z: 30.0,
                    },
                    NONE,
                ),
            ],
        };
        let animation = Animation {
            num_frames: 4,
            frame_rate: 30,
            channels: vec![Channel {
                pivot: 1,
                kind: ChannelKind::TranslationZ,
                declared_vector_len: Some(1),
                data: ChannelData::Dense {
                    first_frame: 0,
                    values: [0.0, 10.0, 20.0, 30.0].map(Value::Scalar).to_vec(),
                },
            }],
            ..Animation::default()
        };
        let z = |frame: u32| posed_bones(&hierarchy, &animation, frame, "Firepoint")[0].position;
        assert_eq!(
            numbered_bones(&hierarchy, "Firepoint")[0].position,
            [3.0, 0.0, 30.0]
        );
        assert_eq!(z(2), [3.0, 0.0, 50.0]);
        assert_eq!(z(40), [3.0, 0.0, 60.0]);
    }

    #[test]
    fn a_bones_facing_is_its_own_world_rotation() {
        let hierarchy = Hierarchy {
            header: HierarchyHeader::default(),
            pivots: vec![
                pivot("ROOTTRANSFORM", None, Vec3::default(), NONE),
                pivot(
                    "SEAT01",
                    Some(0),
                    Vec3 {
                        x: 5.0,
                        y: 0.0,
                        z: 0.0,
                    },
                    TURN_Z_90,
                ),
            ],
        };
        let bones = numbered_bones(&hierarchy, "SEAT");
        assert_eq!(bones.len(), 1);
        assert!(
            (bones[0].facing.radians() - core::f32::consts::FRAC_PI_2).abs() < 1e-3,
            "{:?}",
            bones[0].facing
        );
    }
}
