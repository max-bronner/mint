use super::*;

#[test]
fn a_faded_part_meets_its_material_as_ww3d_does() {
    let tint = [0.5, 1.0, 1.0, 0.8];
    assert_eq!(
        faded(Blend::Opaque, tint, 0.5),
        (Blend::AlphaBlend, [0.5, 1.0, 1.0, 0.4])
    );
    assert_eq!(
        faded(Blend::AlphaBlend, tint, 0.5),
        (Blend::AlphaBlend, [0.5, 1.0, 1.0, 0.4])
    );
    assert_eq!(
        faded(Blend::Additive, tint, 0.25),
        (Blend::Additive, [0.125, 0.25, 0.25, 0.8])
    );
    for blend in [Blend::Opaque, Blend::AlphaBlend, Blend::Additive] {
        assert_eq!(faded(blend, tint, 1.0), (blend, tint));
    }
    assert_eq!(faded(Blend::Additive, tint, -1.0).1, [0.0, 0.0, 0.0, 0.8]);
}

const FRAMES: u32 = 31;
const RATE: u32 = 30;

fn at(mode: AnimationMode, completion: Completion, start: f32, seconds: f32) -> f32 {
    frame_at(mode, completion, FRAMES, RATE, start, seconds)
}

#[test]
fn the_bone_matrices_are_one_per_pivot_in_hierarchy_order() {
    use mint_sage_w3d::{Animation, HierarchyHeader, Pivot, Quat, Vec3 as W3dVec3};
    let zero = W3dVec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let pivot = |name: &str, parent, z| Pivot {
        name: name.to_owned(),
        parent,
        translation: W3dVec3 { x: 0.0, y: 0.0, z },
        euler_angles: zero,
        rotation: Quat {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        },
    };
    let hierarchy = Hierarchy {
        header: HierarchyHeader {
            name: "TEST".to_owned(),
            num_pivots: 3,
            center: zero,
            version: 0,
        },
        pivots: vec![
            pivot("ROOT", None, 0.0),
            pivot("MID", Some(0), 4.0),
            pivot("TIP", Some(1), 6.0),
        ],
    };
    let animation = ModelAnimation {
        asset: "test".to_owned(),
        mode: AnimationMode::Loop,
        flags: AnimationFlags::default(),
        frames: 2,
        rate: 30,
        start: 0.0,
        skinned: Vec::new(),
        rigid: Vec::new(),
        rigid_static: 0,
        reach: Vec::new(),
        animation: Animation {
            name: "test".to_owned(),
            hierarchy_name: "TEST".to_owned(),
            num_frames: 2,
            frame_rate: 30,
            compressed: false,
            flavor: None,
            channel_chunks: 0,
            channels: Vec::new(),
        },
        hierarchy,
    };
    let mut bones = Vec::new();
    animation.bones(0.0, &mut bones);
    assert_eq!(bones.len(), 3, "one per pivot");
    assert_eq!(
        bones[2].transform_point(mint_render::Vec3::ZERO).to_array(),
        [0.0, 0.0, 10.0]
    );
    animation.bones(1.0, &mut bones);
    assert_eq!(bones.len(), 3);
}

#[test]
fn a_clip_that_drives_no_part_still_binds() {
    use crate::load::LoadedAnimation;
    use mint_sage_ini::AnimationSource;
    use mint_sage_w3d::{HierarchyHeader, Pivot, Quat, Vec3 as W3dVec3};
    use mint_vfs::VfsPath;

    let zero = W3dVec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let hierarchy = Hierarchy {
        header: HierarchyHeader {
            name: "TEST".to_owned(),
            num_pivots: 1,
            center: zero,
            version: 0,
        },
        pivots: vec![Pivot {
            name: "ROOT".to_owned(),
            parent: None,
            translation: zero,
            euler_angles: zero,
            rotation: Quat {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 1.0,
            },
        }],
    };
    let model = CachedModel {
        name: "test".to_owned(),
        hierarchy: Some(hierarchy.clone()),
        parts: Vec::new(),
        bounds: Aabb::EMPTY,
        vertices: 0,
        triangles: 0,
    };
    let declared = DefaultAnimation {
        name: "TEST.CLIP".to_owned(),
        mode: None,
        blend_frames: None,
        flags: AnimationFlags::default(),
        source: AnimationSource::IdleAnimationState,
    };
    let loaded = LoadedAnimation {
        path: VfsPath::new("clip.w3d"),
        animation: Animation {
            name: "CLIP".to_owned(),
            hierarchy_name: "TEST".to_owned(),
            num_frames: 2,
            frame_rate: 30,
            compressed: false,
            flavor: None,
            channels: Vec::new(),
            channel_chunks: 0,
        },
        hierarchy_path: VfsPath::new("test_skl.w3d"),
        hierarchy,
    };
    let bound = ModelAnimation::bind(&model, &declared, loaded).expect("binds regardless");
    assert!(bound.skinned.is_empty());
    assert!(bound.rigid.is_empty());
}

#[test]
fn a_clip_binds_to_a_skeleton_with_more_or_fewer_pivots_than_it_has_channels() {
    use crate::load::LoadedAnimation;
    use mint_render::Vec3 as RVec3;
    use mint_sage_ini::AnimationSource;
    use mint_sage_w3d::{
        Channel, ChannelData, ChannelKind, HierarchyHeader, Pivot, Quat, Value, Vec3 as W3dVec3,
    };
    use mint_vfs::VfsPath;

    let zero = W3dVec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let pivot = |name: &str, parent: Option<u32>| Pivot {
        name: name.to_owned(),
        parent,
        translation: zero,
        euler_angles: zero,
        rotation: Quat {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        },
    };
    let skeleton = |name: &str, pivots: &[&str]| Hierarchy {
        header: HierarchyHeader {
            name: name.to_owned(),
            num_pivots: u32::try_from(pivots.len()).unwrap(),
            center: zero,
            version: 0,
        },
        pivots: pivots
            .iter()
            .enumerate()
            .map(|(i, n)| pivot(n, i.checked_sub(1).and_then(|p| u32::try_from(p).ok())))
            .collect(),
    };
    let model = CachedModel {
        name: "rig".to_owned(),
        hierarchy: Some(skeleton("RIG", &["ROOT", "A", "B"])),
        parts: Vec::new(),
        bounds: Aabb::EMPTY,
        vertices: 0,
        triangles: 0,
    };
    let declared = DefaultAnimation {
        name: "OTHER.MOVE".to_owned(),
        mode: None,
        blend_frames: None,
        flags: AnimationFlags::default(),
        source: AnimationSource::IdleAnimationState,
    };
    let shift_x = |pivot: u32| Channel {
        pivot,
        kind: ChannelKind::TranslationX,
        declared_vector_len: None,
        data: ChannelData::Dense {
            first_frame: 0,
            values: vec![Value::Scalar(5.0), Value::Scalar(5.0)],
        },
    };
    let clip = |pivots: &[&str], channels: Vec<Channel>| LoadedAnimation {
        path: VfsPath::new("move.w3d"),
        animation: Animation {
            name: "MOVE".to_owned(),
            hierarchy_name: "OTHER".to_owned(),
            num_frames: 2,
            frame_rate: 30,
            compressed: false,
            flavor: None,
            channels,
            channel_chunks: 0,
        },
        hierarchy_path: VfsPath::new("other.w3d"),
        hierarchy: skeleton("OTHER", pivots),
    };
    let origin_of = |bound: &ModelAnimation, bone: usize| {
        let mut bones = Vec::new();
        bound.bones(0.0, &mut bones);
        assert_eq!(bones.len(), 3);
        bones[bone].transform_point(RVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        })
    };

    let more = clip(
        &["ROOT", "A", "B", "ROCK"],
        vec![shift_x(1), shift_x(2), shift_x(3)],
    );
    let bound = ModelAnimation::bind(&model, &declared, more).expect("extra channels are ignored");
    assert!((origin_of(&bound, 1).x - 5.0).abs() < 1e-5);
    assert!((origin_of(&bound, 2).x - 10.0).abs() < 1e-5);

    let fewer = clip(&["ROOT", "A"], vec![shift_x(1)]);
    let bound = ModelAnimation::bind(&model, &declared, fewer).expect("extra pivots keep bind");
    assert!((origin_of(&bound, 1).x - 5.0).abs() < 1e-5);
    assert!((origin_of(&bound, 2).x - 5.0).abs() < 1e-5);
}

#[test]
fn a_loop_wraps_at_one_less_than_the_frame_count() {
    assert!((at(AnimationMode::Loop, Completion::Restart, 0.0, 0.0) - 0.0).abs() < 1e-4);
    assert!((at(AnimationMode::Loop, Completion::Restart, 0.0, 0.5) - 15.0).abs() < 1e-3);
    assert!(at(AnimationMode::Loop, Completion::Restart, 0.0, 1.0).abs() < 1e-3);
    for step in 0..400 {
        let seconds = step as f32 * 0.017;
        let frame = at(AnimationMode::Loop, Completion::Restart, 0.0, seconds);
        assert!((0.0..30.0).contains(&frame), "{seconds} -> {frame}");
    }
}

#[test]
fn once_holds_the_last_frame_or_restarts_by_completion() {
    assert!((at(AnimationMode::Once, Completion::Hold, 0.0, 0.5) - 15.0).abs() < 1e-3);
    assert!((at(AnimationMode::Once, Completion::Restart, 0.0, 0.5) - 15.0).abs() < 1e-3);
    assert!((at(AnimationMode::Once, Completion::Hold, 0.0, 10.0) - 30.0).abs() < 1e-3);
    assert!(at(AnimationMode::Once, Completion::Restart, 0.0, 10.0) < 1e-3);
}

#[test]
fn a_restart_returns_to_the_start_frame_not_to_zero() {
    let frame = at(AnimationMode::Once, Completion::Restart, 20.0, 0.5);
    assert!((20.0..30.0).contains(&frame), "{frame}");
    assert!((at(AnimationMode::Once, Completion::Restart, 30.0, 5.0) - 30.0).abs() < 1e-3);
}

#[test]
fn manual_never_advances() {
    assert!((at(AnimationMode::Manual, Completion::Restart, 7.0, 0.0) - 7.0).abs() < 1e-4);
    assert!((at(AnimationMode::Manual, Completion::Restart, 7.0, 900.0) - 7.0).abs() < 1e-4);
}

#[test]
fn a_degenerate_animation_yields_a_finite_in_range_frame() {
    for frames in [0u32, 1, 2] {
        for rate in [0u32, 30] {
            for mode in [
                AnimationMode::Manual,
                AnimationMode::Once,
                AnimationMode::OnceBackwards,
                AnimationMode::Loop,
                AnimationMode::LoopBackwards,
                AnimationMode::PingPong,
            ] {
                for seconds in [0.0, 1.0, f32::INFINITY, f32::NAN, -3.0, 1e30] {
                    for completion in [Completion::Hold, Completion::Restart] {
                        let frame = frame_at(mode, completion, frames, rate, 0.0, seconds);
                        let last = f32::from(u8::try_from(frames.max(1) - 1).unwrap());
                        assert!(
                            frame.is_finite() && (0.0..=last).contains(&frame),
                            "{mode:?} {completion:?} frames={frames} rate={rate} \
                                 seconds={seconds} -> {frame}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn a_negative_elapsed_time_cannot_leave_the_clip() {
    for mode in [
        AnimationMode::Once,
        AnimationMode::OnceBackwards,
        AnimationMode::Loop,
        AnimationMode::LoopBackwards,
        AnimationMode::PingPong,
    ] {
        for completion in [Completion::Hold, Completion::Restart] {
            let frame = at(mode, completion, 10.0, -3.0);
            assert!(
                (0.0..=30.0).contains(&frame),
                "{mode:?} {completion:?} -> {frame}"
            );
        }
    }
}

#[test]
fn wrapping_never_returns_the_period_itself_or_a_nan() {
    assert!(
        wrapped(5.0, 0.0).abs() < 1e-6,
        "a zero period cannot divide"
    );
    assert!(wrapped(5.0, -1.0).abs() < 1e-6);
    assert!(wrapped(5.0, f32::NAN).abs() < 1e-6);
    assert!(wrapped(f32::NAN, 30.0).abs() < 1e-6);
    assert!(wrapped(f32::INFINITY, 30.0).abs() < 1e-6);
    for step in -200..200 {
        let value = step as f32 * 0.37;
        let out = wrapped(value, 30.0);
        assert!((0.0..30.0).contains(&out), "{value} -> {out}");
    }
}

#[test]
fn the_start_frame_follows_the_flags_before_the_mode() {
    let none = AnimationFlags::default();
    let first = AnimationFlags {
        start_frame_first: true,
        ..AnimationFlags::default()
    };
    let last = AnimationFlags {
        start_frame_last: true,
        ..AnimationFlags::default()
    };
    assert!((start_frame(AnimationMode::Loop, none, FRAMES)).abs() < 1e-4);
    assert!((start_frame(AnimationMode::LoopBackwards, none, FRAMES) - 30.0).abs() < 1e-4);
    assert!((start_frame(AnimationMode::LoopBackwards, first, FRAMES)).abs() < 1e-4);
    assert!((start_frame(AnimationMode::Loop, last, FRAMES) - 30.0).abs() < 1e-4);
    let both = AnimationFlags {
        start_frame_first: true,
        start_frame_last: true,
        ..AnimationFlags::default()
    };
    assert!(
        start_frame(AnimationMode::LoopBackwards, both, FRAMES).abs() < 1e-4,
        "START_FRAME_FIRST is tested before START_FRAME_LAST"
    );
    let random = AnimationFlags {
        random_start: true,
        ..AnimationFlags::default()
    };
    assert!((start_frame(AnimationMode::Loop, random, FRAMES)).abs() < 1e-4);
}

#[test]
fn ping_pong_stays_inside_the_clip_and_turns_around() {
    let quarter = at(AnimationMode::PingPong, Completion::Restart, 0.0, 0.5);
    let three_quarters = at(AnimationMode::PingPong, Completion::Restart, 0.0, 1.5);
    assert!((quarter - 15.0).abs() < 1e-2, "{quarter}");
    assert!((three_quarters - 15.0).abs() < 1e-2, "{three_quarters}");
    assert!((at(AnimationMode::PingPong, Completion::Restart, 0.0, 1.0) - 30.0).abs() < 1e-2);
    assert!(at(AnimationMode::PingPong, Completion::Restart, 0.0, 2.0).abs() < 1e-2);
}

#[test]
fn a_pivot_under_a_driven_one_moves_even_with_no_channel_of_its_own() {
    use mint_sage_w3d::{HierarchyHeader, Pivot, Quat, Vec3 as W3dVec3};
    let zero = W3dVec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let pivot = |name: &str, parent| Pivot {
        name: name.to_owned(),
        parent,
        translation: zero,
        euler_angles: zero,
        rotation: Quat {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        },
    };
    let hierarchy = Hierarchy {
        header: HierarchyHeader {
            name: "TEST".to_owned(),
            num_pivots: 4,
            center: zero,
            version: 0,
        },
        pivots: vec![
            pivot("ROOT", None),
            pivot("ARM", Some(0)),
            pivot("LIGHT", Some(1)),
            pivot("CAPE", Some(0)),
        ],
    };
    let moves = pivots_that_move(&hierarchy, &[1]);
    assert_eq!(moves, vec![false, true, true, false]);
    assert_eq!(
        pivots_that_move(&hierarchy, &[]),
        vec![false, false, false, false]
    );
}
