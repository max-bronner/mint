use super::*;
use mint_sage_w3d::{HierarchyHeader, Pivot};

#[test]
fn a_blend_mixes_channels_before_composing_down_the_hierarchy() {
    let hierarchy = Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![
            pivot("ROOT", None, Vec3::default(), IDENTITY_Q),
            pivot("ARM", Some(0), Vec3::default(), IDENTITY_Q),
            pivot(
                "HAND",
                Some(1),
                Vec3 {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                },
                IDENTITY_Q,
            ),
        ],
    };
    let turn = |degrees: f32| {
        animation_with(
            vec![dense_channel(
                1,
                ChannelKind::Rotation,
                0,
                vec![Value::Rotation(quat_about_z(degrees))],
            )],
            1,
        )
    };
    let (from, to) = (turn(0.0), turn(90.0));
    let mut buffers = PoseBuffers::default();
    let hand = |weight: f32, buffers: &mut PoseBuffers| {
        sample_blend_into(&hierarchy, &from, 0.0, &to, 0.0, weight, buffers);
        buffers.pose.world[2].translation
    };
    assert!(vec3_approx(
        hand(0.0, &mut buffers),
        Vec3 {
            x: 1.0,
            y: 0.0,
            z: 0.0
        }
    ));
    assert!(vec3_approx(
        hand(1.0, &mut buffers),
        Vec3 {
            x: 0.0,
            y: 1.0,
            z: 0.0
        }
    ));
    let half = std::f32::consts::FRAC_1_SQRT_2;
    assert!(vec3_approx(
        hand(0.5, &mut buffers),
        Vec3 {
            x: half,
            y: half,
            z: 0.0
        }
    ));
}

const IDENTITY_Q: Quat = Quat {
    x: 0.0,
    y: 0.0,
    z: 0.0,
    w: 1.0,
};

fn quat_about_z(degrees: f32) -> Quat {
    let radians = degrees.to_radians();
    let (sin, cos) = mint_math::f32::sin_cos(radians / 2.0);
    Quat {
        x: 0.0,
        y: 0.0,
        z: sin,
        w: cos,
    }
}

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

fn vec3_approx(a: Vec3, b: Vec3) -> bool {
    approx(a.x, b.x) && approx(a.y, b.y) && approx(a.z, b.z)
}

fn quat_approx(a: Quat, b: Quat) -> bool {
    approx(a.x, b.x) && approx(a.y, b.y) && approx(a.z, b.z) && approx(a.w, b.w)
}

fn pivot(name: &str, parent: Option<u32>, translation: Vec3, rotation: Quat) -> Pivot {
    Pivot {
        name: name.to_owned(),
        parent,
        translation,
        euler_angles: Vec3::default(),
        rotation,
    }
}

fn one_pivot_hierarchy() -> Hierarchy {
    Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![pivot("ROOT", None, Vec3::default(), IDENTITY_Q)],
    }
}

fn dense_channel(pivot: u32, kind: ChannelKind, first_frame: u32, values: Vec<Value>) -> Channel {
    Channel {
        pivot,
        kind,
        declared_vector_len: None,
        data: ChannelData::Dense {
            first_frame,
            values,
        },
    }
}

fn time_coded_channel(pivot: u32, kind: ChannelKind, keys: Vec<TimeCodedKey>) -> Channel {
    Channel {
        pivot,
        kind,
        declared_vector_len: None,
        data: ChannelData::TimeCoded(keys),
    }
}

fn animation_with(channels: Vec<Channel>, num_frames: u32) -> Animation {
    Animation {
        name: "TEST".to_owned(),
        hierarchy_name: "TEST".to_owned(),
        num_frames,
        frame_rate: 30,
        compressed: false,
        flavor: None,
        channel_chunks: u32::try_from(channels.len()).unwrap_or(0),
        channels,
    }
}

fn swinging_arm() -> (Hierarchy, Animation) {
    let hierarchy = Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![
            pivot(
                "ROOT",
                None,
                Vec3 {
                    x: 6.0,
                    y: 0.0,
                    z: 2.0,
                },
                IDENTITY_Q,
            ),
            pivot(
                "UPPER",
                Some(0),
                Vec3 {
                    x: 0.0,
                    y: 2.0,
                    z: 0.0,
                },
                quat_about_z(30.0),
            ),
            pivot(
                "LOWER",
                Some(1),
                Vec3 {
                    x: 3.0,
                    y: 0.0,
                    z: 0.5,
                },
                IDENTITY_Q,
            ),
        ],
    };
    let spin: Vec<Value> = (0..9)
        .map(|i| Value::Rotation(quat_about_z(i as f32 * 40.0)))
        .collect();
    let slide: Vec<Value> = [0.0, 1.5, -2.5, 0.75, 2.0]
        .into_iter()
        .map(Value::Scalar)
        .collect();
    let animation = animation_with(
        vec![
            dense_channel(1, ChannelKind::Rotation, 0, spin.clone()),
            dense_channel(1, ChannelKind::TranslationX, 2, slide.clone()),
            dense_channel(1, ChannelKind::TranslationZ, 0, slide),
            time_coded_channel(
                2,
                ChannelKind::TranslationY,
                vec![key(0, -1.0, false), key(4, 4.0, false), key(8, 0.5, true)],
            ),
            dense_channel(2, ChannelKind::Rotation, 0, spin),
            dense_channel(0, ChannelKind::TranslationX, 0, vec![Value::Scalar(100.0)]),
        ],
        9,
    );
    (hierarchy, animation)
}

fn length(v: Vec3) -> f32 {
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

#[test]
fn no_frame_carries_a_pivot_past_its_reach() {
    let (hierarchy, animation) = swinging_arm();
    let reach = pivot_reach(&hierarchy, &animation);
    assert_eq!(reach.len(), 3);
    let mut closest = f32::INFINITY;
    for step in -20..=140 {
        let frame = step as f32 * 0.1;
        let pose = sample_pose(&hierarchy, &animation, frame);
        for (index, world) in pose.world.iter().enumerate() {
            let d = length(world.translation);
            assert!(
                d <= reach[index] + 1e-4,
                "pivot {index} at frame {frame} is {d} from the origin, reach {}",
                reach[index]
            );
            if reach[index] > 0.0 {
                closest = closest.min((reach[index] - d) / reach[index]);
            }
        }
    }
    assert!(closest < 0.2, "the bound is never approached: {closest}");
}

#[test]
fn a_root_channel_and_a_rotation_add_nothing_to_the_reach() {
    let (hierarchy, animation) = swinging_arm();
    let reach = pivot_reach(&hierarchy, &animation);
    assert!(
        (reach[0]
            - length(Vec3 {
                x: 6.0,
                y: 0.0,
                z: 2.0
            }))
        .abs()
            < 1e-5
    );
}

#[test]
fn sampling_into_reused_buffers_gives_the_same_pose_bit_for_bit() {
    let (hierarchy, animation) = swinging_arm();
    let mut buffers = PoseBuffers::default();
    for frame in [0.0f32, 3.7, 8.0, 1.25, -2.0, 11.0] {
        sample_pose_into(&hierarchy, &animation, frame, &mut buffers);
        assert_eq!(buffers.pose, sample_pose(&hierarchy, &animation, frame));
    }
    let small = one_pivot_hierarchy();
    sample_pose_into(&small, &animation, 2.0, &mut buffers);
    assert_eq!(buffers.pose, sample_pose(&small, &animation, 2.0));
    assert_eq!(buffers.pose.world.len(), 1);
}

#[test]
fn a_dense_channel_samples_exactly_at_frame_boundaries() {
    let values = vec![Value::Scalar(1.0), Value::Scalar(2.0), Value::Scalar(4.0)];
    assert_eq!(
        sample_dense(ChannelKind::TranslationX, 10, &values, 10.0),
        Some(Value::Scalar(1.0))
    );
    assert_eq!(
        sample_dense(ChannelKind::TranslationX, 10, &values, 11.0),
        Some(Value::Scalar(2.0))
    );
    assert_eq!(
        sample_dense(ChannelKind::TranslationX, 10, &values, 12.0),
        Some(Value::Scalar(4.0))
    );
}

#[test]
fn a_dense_channel_interpolates_between_frames() {
    let values = vec![Value::Scalar(0.0), Value::Scalar(10.0)];
    match sample_dense(ChannelKind::TranslationX, 0, &values, 0.25) {
        Some(Value::Scalar(v)) => assert!(approx(v, 2.5), "got {v}"),
        other => panic!("expected a scalar, got {other:?}"),
    }
}

#[test]
fn a_dense_scalar_channel_reads_zero_not_the_nearest_key_outside_its_own_range() {
    let values = vec![Value::Scalar(5.0), Value::Scalar(9.0)];
    assert_eq!(
        sample_dense(ChannelKind::TranslationX, 10, &values, 0.0),
        Some(Value::Scalar(0.0))
    );
    assert_eq!(
        sample_dense(ChannelKind::TranslationX, 10, &values, 999.0),
        Some(Value::Scalar(0.0))
    );
    assert_eq!(
        sample_dense(ChannelKind::TranslationX, 10, &values, 10.0),
        Some(Value::Scalar(5.0))
    );
    assert_eq!(
        sample_dense(ChannelKind::TranslationX, 10, &values, 11.0),
        Some(Value::Scalar(9.0))
    );
}

#[test]
fn a_dense_rotation_channel_reads_the_identity_quaternion_outside_its_own_range() {
    let values = vec![Value::Rotation(quat_about_z(90.0))];
    assert_eq!(
        sample_dense(ChannelKind::TranslationX, 10, &values, 0.0),
        Some(Value::Rotation(IDENTITY_Q))
    );
}

#[test]
fn an_adaptive_delta_channel_still_clamps_outside_its_own_range() {
    let values = vec![Value::Scalar(5.0), Value::Scalar(9.0)];
    assert_eq!(
        sample_dense_clamped(&values, -1.0),
        Some(Value::Scalar(5.0))
    );
    assert_eq!(
        sample_dense_clamped(&values, 999.0),
        Some(Value::Scalar(9.0))
    );
}

#[test]
fn an_empty_dense_channel_has_no_value() {
    assert_eq!(sample_dense(ChannelKind::TranslationX, 0, &[], 0.0), None);
    assert_eq!(sample_dense_clamped(&[], 0.0), None);
}

fn key(frame: u32, value: f32, hold: bool) -> TimeCodedKey {
    TimeCodedKey {
        frame,
        value: Value::Scalar(value),
        hold,
    }
}

#[test]
fn bracketing_key_search_finds_the_right_pair_on_a_sparse_channel() {
    let keys = vec![
        key(0, 0.0, false),
        key(10, 100.0, false),
        key(100, 1000.0, false),
    ];
    match sample_time_coded(&keys, 2.5) {
        Some(Value::Scalar(v)) => assert!(approx(v, 25.0), "got {v}"),
        other => panic!("expected a scalar, got {other:?}"),
    }
}

#[test]
fn a_frame_before_the_first_key_holds_it() {
    let keys = vec![key(10, 7.0, false), key(20, 9.0, false)];
    assert_eq!(sample_time_coded(&keys, 0.0), Some(Value::Scalar(7.0)));
}

#[test]
fn a_frame_after_the_last_key_holds_it() {
    let keys = vec![key(10, 7.0, false), key(20, 9.0, false)];
    assert_eq!(sample_time_coded(&keys, 500.0), Some(Value::Scalar(9.0)));
}

#[test]
fn the_hold_flag_on_the_later_key_steps_instead_of_interpolating() {
    let keys = vec![key(0, 1.0, false), key(10, 0.0, true)];
    for frame in [0.0, 2.5, 5.0, 9.99] {
        assert_eq!(
            sample_time_coded(&keys, frame),
            Some(Value::Scalar(1.0)),
            "frame {frame} should still read the held value"
        );
    }
    assert_eq!(sample_time_coded(&keys, 10.0), Some(Value::Scalar(0.0)));
}

#[test]
fn without_the_hold_flag_the_same_keys_interpolate() {
    let keys = vec![key(0, 1.0, false), key(10, 0.0, false)];
    match sample_time_coded(&keys, 5.0) {
        Some(Value::Scalar(v)) => assert!(approx(v, 0.5), "got {v}"),
        other => panic!("expected a scalar, got {other:?}"),
    }
}

#[test]
fn an_empty_time_coded_channel_has_no_value() {
    assert_eq!(sample_time_coded(&[], 0.0), None);
}

#[test]
fn slerp_at_the_endpoints_returns_the_endpoints() {
    let a = quat_about_z(0.0);
    let b = quat_about_z(90.0);
    assert!(quat_approx(quat_slerp(a, b, 0.0), a));
    assert!(quat_approx(quat_slerp(a, b, 1.0), b));
}

#[test]
fn slerp_takes_the_shortest_arc_across_the_double_cover() {
    let a = IDENTITY_Q;
    let ten_degrees = quat_about_z(10.0);
    let b = quat_negate(ten_degrees);
    assert!(quat_dot(a, b) < 0.0, "test setup: dot should be negative");

    let halfway = quat_slerp(a, b, 0.5);
    let expected = quat_about_z(5.0);
    assert!(
        quat_approx(halfway, expected),
        "shortest-arc halfway should be a 5 degree rotation, got {halfway:?}"
    );
}

#[test]
fn slerp_of_nearly_identical_rotations_does_not_divide_by_zero() {
    let a = quat_about_z(10.0);
    let b = quat_about_z(10.0001);
    let mid = quat_slerp(a, b, 0.5);
    assert!(mid.x.is_finite() && mid.y.is_finite() && mid.z.is_finite() && mid.w.is_finite());
}

#[test]
fn compose_applies_the_inner_translation_in_the_outer_rotations_frame() {
    let outer = Transform::new(Vec3::default(), quat_about_z(90.0));
    let inner = Transform::new(
        Vec3 {
            x: 1.0,
            y: 0.0,
            z: 0.0,
        },
        IDENTITY_Q,
    );
    let composed = outer.compose(&inner);
    assert!(
        vec3_approx(
            composed.translation,
            Vec3 {
                x: 0.0,
                y: 1.0,
                z: 0.0
            }
        ),
        "got {:?}",
        composed.translation
    );
}

#[test]
fn a_bone_with_no_channels_sits_exactly_at_its_bind_pose() {
    let hierarchy = one_pivot_hierarchy();
    let animation = animation_with(vec![], 10);
    let pose = sample_pose(&hierarchy, &animation, 5.0);
    assert_eq!(pose.world.len(), 1);
    assert!(vec3_approx(pose.world[0].translation, Vec3::default()));
    assert!(quat_approx(pose.world[0].rotation, IDENTITY_Q));
}

#[test]
fn a_translation_delta_is_applied_in_the_bind_rotations_frame() {
    let hierarchy = Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![
            pivot("ROOT", None, Vec3::default(), IDENTITY_Q),
            pivot("BONE", Some(0), Vec3::default(), quat_about_z(90.0)),
        ],
    };
    let channel = dense_channel(
        1,
        ChannelKind::TranslationX,
        0,
        vec![Value::Scalar(1.0), Value::Scalar(1.0)],
    );
    let animation = animation_with(vec![channel], 2);
    let pose = sample_pose(&hierarchy, &animation, 0.0);

    let bone = pose.world.get(1).expect("bone pivot");
    assert!(
        vec3_approx(
            bone.translation,
            Vec3 {
                x: 0.0,
                y: 1.0,
                z: 0.0
            }
        ),
        "got {:?}",
        bone.translation
    );
    assert_eq!(pose.out_of_range_channels, 0);
    assert_eq!(pose.root_channels_ignored, 0);
}

#[test]
fn a_missing_axis_contributes_zero_not_the_other_axes_value() {
    let hierarchy = Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![
            pivot("ROOT", None, Vec3::default(), IDENTITY_Q),
            pivot("BONE", Some(0), Vec3::default(), IDENTITY_Q),
        ],
    };
    let channel = dense_channel(1, ChannelKind::TranslationY, 0, vec![Value::Scalar(3.0)]);
    let animation = animation_with(vec![channel], 1);
    let pose = sample_pose(&hierarchy, &animation, 0.0);
    let bone = pose.world.get(1).expect("bone pivot");
    assert!(vec3_approx(
        bone.translation,
        Vec3 {
            x: 0.0,
            y: 3.0,
            z: 0.0
        }
    ));
}

#[test]
fn a_channel_naming_pivot_zero_is_ignored_and_counted() {
    let hierarchy = one_pivot_hierarchy();
    let channel = dense_channel(0, ChannelKind::TranslationX, 0, vec![Value::Scalar(500.0)]);
    let animation = animation_with(vec![channel], 1);
    let pose = sample_pose(&hierarchy, &animation, 0.0);
    assert_eq!(pose.root_channels_ignored, 1);
    assert_eq!(pose.out_of_range_channels, 0);
    assert!(vec3_approx(pose.world[0].translation, Vec3::default()));
}

#[test]
fn an_out_of_range_pivot_is_dropped_and_counted_not_applied_to_the_whole_model() {
    let hierarchy = one_pivot_hierarchy();
    let channel = dense_channel(9, ChannelKind::TranslationX, 0, vec![Value::Scalar(500.0)]);
    let animation = animation_with(vec![channel], 1);
    let pose = sample_pose(&hierarchy, &animation, 0.0);
    assert_eq!(pose.out_of_range_channels, 1);
    assert_eq!(pose.root_channels_ignored, 0);
    assert_eq!(pose.world.len(), 1, "the one real pivot is still posed");
    assert!(vec3_approx(pose.world[0].translation, Vec3::default()));
}

#[test]
fn bind_matches_sample_pose_of_an_animation_with_no_channels() {
    let hierarchy = Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![
            pivot(
                "ROOT",
                None,
                Vec3 {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                },
                IDENTITY_Q,
            ),
            pivot("CHILD", Some(0), Vec3::default(), quat_about_z(45.0)),
        ],
    };
    let bind = SampledPose::bind(&hierarchy);
    let sampled = sample_pose(&hierarchy, &animation_with(vec![], 1), 0.0);
    assert_eq!(bind.world, sampled.world);
}

#[test]
fn a_step_sampled_rotation_channel_does_not_interpolate_through_the_hold() {
    let hierarchy = Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![
            pivot("ROOT", None, Vec3::default(), IDENTITY_Q),
            pivot("BONE", Some(0), Vec3::default(), IDENTITY_Q),
        ],
    };
    let keys = vec![
        TimeCodedKey {
            frame: 0,
            value: Value::Rotation(quat_about_z(0.0)),
            hold: false,
        },
        TimeCodedKey {
            frame: 10,
            value: Value::Rotation(quat_about_z(90.0)),
            hold: true,
        },
    ];
    let channel = time_coded_channel(1, ChannelKind::Rotation, keys);
    let animation = animation_with(vec![channel], 11);
    let pose = sample_pose(&hierarchy, &animation, 5.0);
    let bone = pose.world.get(1).expect("bone pivot");
    assert!(
        quat_approx(bone.rotation, quat_about_z(0.0)),
        "held value should still be the earlier key, got {:?}",
        bone.rotation
    );
}

#[test]
fn an_empty_hierarchy_poses_to_nothing_without_panicking() {
    let hierarchy = Hierarchy::default();
    let animation = animation_with(
        vec![dense_channel(
            0,
            ChannelKind::TranslationX,
            0,
            vec![Value::Scalar(1.0)],
        )],
        1,
    );
    let pose = sample_pose(&hierarchy, &animation, 0.0);
    assert!(pose.world.is_empty());
    assert_eq!(pose.out_of_range_channels, 1);
    assert_eq!(pose.root_channels_ignored, 0);
}

#[test]
fn a_channel_pivot_of_u32_max_does_not_panic_or_overflow() {
    let hierarchy = one_pivot_hierarchy();
    let channel = dense_channel(
        u32::MAX,
        ChannelKind::TranslationX,
        0,
        vec![Value::Scalar(1.0)],
    );
    let animation = animation_with(vec![channel], 1);
    let pose = sample_pose(&hierarchy, &animation, 0.0);
    assert_eq!(pose.out_of_range_channels, 1);
    assert_eq!(pose.world.len(), 1);
}

#[test]
fn a_non_finite_frame_does_not_panic() {
    let hierarchy = Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![
            pivot("ROOT", None, Vec3::default(), IDENTITY_Q),
            pivot("BONE", Some(0), Vec3::default(), IDENTITY_Q),
        ],
    };
    let channel = dense_channel(
        1,
        ChannelKind::Rotation,
        0,
        vec![
            Value::Rotation(quat_about_z(0.0)),
            Value::Rotation(quat_about_z(90.0)),
        ],
    );
    let animation = animation_with(vec![channel], 2);
    for frame in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let pose = sample_pose(&hierarchy, &animation, frame);
        assert_eq!(pose.world.len(), 2);
    }
}

#[test]
fn a_hierarchy_pivot_with_an_out_of_range_parent_does_not_panic() {
    let hierarchy = Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![pivot("ODD", Some(99), Vec3::default(), IDENTITY_Q)],
    };
    let pose = sample_pose(&hierarchy, &animation_with(vec![], 1), 0.0);
    assert_eq!(pose.world.len(), 1);
    assert!(vec3_approx(pose.world[0].translation, Vec3::default()));
}

#[test]
fn type15_opacity_passes_the_0_1_convention_through_unchanged() {
    assert_eq!(type15_opacity(0.0), (0.0, false));
    assert_eq!(type15_opacity(1.0), (1.0, false));
    assert_eq!(type15_opacity(0.5), (0.5, false));
}

#[test]
fn type15_opacity_maps_the_negative_one_convention_to_the_same_hidden_answer_as_zero() {
    assert_eq!(type15_opacity(-1.0), (0.0, false));
    assert_eq!(type15_opacity(-0.5), (0.0, false));
}

#[test]
fn type15_opacity_clamps_and_flags_values_outside_the_documented_domain() {
    assert_eq!(type15_opacity(1e14), (1.0, true));
    assert_eq!(type15_opacity(-5.0), (0.0, true));
    assert_eq!(type15_opacity(100.0), (1.0, true));
}

#[test]
fn type15_opacity_treats_non_finite_input_as_fully_transparent_and_flagged() {
    for raw in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(type15_opacity(raw), (0.0, true));
    }
}

fn type15_channel(pivot: u32, keys: Vec<TimeCodedKey>) -> Channel {
    Channel {
        pivot,
        kind: ChannelKind::Unestablished,
        declared_vector_len: None,
        data: ChannelData::TimeCoded(keys),
    }
}

#[test]
fn a_dense_type15_channel_reads_fully_shown_outside_its_own_range() {
    let hierarchy = Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![
            pivot("ROOT", None, Vec3::default(), IDENTITY_Q),
            pivot("PLANE", Some(0), Vec3::default(), IDENTITY_Q),
        ],
    };
    let channel = Channel {
        pivot: 1,
        kind: ChannelKind::Unestablished,
        declared_vector_len: Some(1),
        data: ChannelData::Dense {
            first_frame: 2,
            values: vec![Value::Scalar(0.0), Value::Scalar(0.5), Value::Scalar(0.9)],
        },
    };
    let animation = animation_with(vec![channel], 10);
    let at = |frame: f32| sample_pose(&hierarchy, &animation, frame).opacity[1];
    assert_eq!(at(0.0), 1.0, "before the range");
    assert_eq!(at(2.0), 0.0);
    assert_eq!(at(3.0), 0.5);
    assert_eq!(at(4.0), 0.9);
    assert_eq!(at(9.0), 1.0, "after the range");
    assert_eq!(
        sample_dense(ChannelKind::TranslationX, 2, &[Value::Scalar(7.0)], 0.0),
        Some(Value::Scalar(0.0))
    );
}

#[test]
fn a_pivot_with_no_type15_channel_is_fully_opaque() {
    let hierarchy = Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![
            pivot("ROOT", None, Vec3::default(), IDENTITY_Q),
            pivot("BONE", Some(0), Vec3::default(), IDENTITY_Q),
        ],
    };
    let pose = sample_pose(&hierarchy, &animation_with(vec![], 1), 0.0);
    assert_eq!(pose.opacity, vec![1.0, 1.0]);
    assert_eq!(pose.opacity_clamped, 0);
}

#[test]
fn a_type15_fade_reaches_sample_pose_as_the_pivots_opacity() {
    let hierarchy = Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![
            pivot("ROOT", None, Vec3::default(), IDENTITY_Q),
            pivot("ICON", Some(0), Vec3::default(), IDENTITY_Q),
        ],
    };
    let keys = vec![
        TimeCodedKey {
            frame: 0,
            value: Value::Scalar(0.0),
            hold: false,
        },
        TimeCodedKey {
            frame: 10,
            value: Value::Scalar(1.0),
            hold: false,
        },
    ];
    let animation = animation_with(vec![type15_channel(1, keys)], 11);

    let start = sample_pose(&hierarchy, &animation, 0.0);
    assert_eq!(start.opacity.get(1).copied(), Some(0.0));

    let mid = sample_pose(&hierarchy, &animation, 5.0);
    assert!(
        approx(mid.opacity.get(1).copied().unwrap_or(-1.0), 0.5),
        "got {:?}",
        mid.opacity
    );

    let end = sample_pose(&hierarchy, &animation, 10.0);
    assert_eq!(end.opacity.get(1).copied(), Some(1.0));

    assert_eq!(start.opacity.first().copied(), Some(1.0));
}

#[test]
fn an_out_of_range_type15_value_is_clamped_and_counted() {
    let hierarchy = Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![
            pivot("ROOT", None, Vec3::default(), IDENTITY_Q),
            pivot("BONE", Some(0), Vec3::default(), IDENTITY_Q),
        ],
    };
    let keys = vec![TimeCodedKey {
        frame: 0,
        value: Value::Scalar(1e14),
        hold: false,
    }];
    let animation = animation_with(vec![type15_channel(1, keys)], 1);
    let pose = sample_pose(&hierarchy, &animation, 0.0);
    assert_eq!(pose.opacity.get(1).copied(), Some(1.0));
    assert_eq!(pose.opacity_clamped, 1);
}

#[test]
fn type15_does_not_drive_translation_or_rotation() {
    let hierarchy = Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![
            pivot("ROOT", None, Vec3::default(), IDENTITY_Q),
            pivot("BONE", Some(0), Vec3::default(), IDENTITY_Q),
        ],
    };
    let keys = vec![TimeCodedKey {
        frame: 0,
        value: Value::Scalar(0.0),
        hold: false,
    }];
    let animation = animation_with(vec![type15_channel(1, keys)], 1);
    let pose = sample_pose(&hierarchy, &animation, 0.0);
    let bone = pose.world.get(1).expect("bone pivot");
    assert!(vec3_approx(bone.translation, Vec3::default()));
    assert!(quat_approx(bone.rotation, IDENTITY_Q));
}

#[test]
fn bind_is_fully_opaque_everywhere() {
    let hierarchy = Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![pivot("ROOT", None, Vec3::default(), IDENTITY_Q)],
    };
    let bind = SampledPose::bind(&hierarchy);
    assert_eq!(bind.opacity, vec![1.0]);
    assert_eq!(bind.opacity_clamped, 0);
    assert_eq!(bind.visible, vec![true]);
}

fn bits_channel(
    pivot: u32,
    first_frame: u32,
    last_frame: u32,
    default: bool,
    packed: Vec<u8>,
) -> Channel {
    Channel {
        pivot,
        kind: ChannelKind::Visibility,
        declared_vector_len: None,
        data: ChannelData::Bits {
            first_frame,
            last_frame,
            default,
            packed,
        },
    }
}

fn sparse_bits_channel(pivot: u32, default: bool, keys: Vec<(u32, bool)>) -> Channel {
    Channel {
        pivot,
        kind: ChannelKind::Visibility,
        declared_vector_len: None,
        data: ChannelData::SparseBits { default, keys },
    }
}

fn two_pivot_hierarchy_for_visibility() -> Hierarchy {
    Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![
            pivot("ROOT", None, Vec3::default(), IDENTITY_Q),
            pivot("BONE", Some(0), Vec3::default(), IDENTITY_Q),
        ],
    }
}

#[test]
fn a_pivot_with_no_visibility_channel_is_visible() {
    let hierarchy = two_pivot_hierarchy_for_visibility();
    let pose = sample_pose(&hierarchy, &animation_with(vec![], 1), 0.0);
    assert_eq!(pose.visible, vec![true, true]);
}

#[test]
fn a_bit_channel_is_hidden_over_its_range_and_reads_its_default_outside_it() {
    let hierarchy = two_pivot_hierarchy_for_visibility();
    let channel = bits_channel(1, 2, 6, true, vec![0b0000_0000]);
    let animation = animation_with(vec![channel], 10);

    let before = sample_pose(&hierarchy, &animation, 0.0);
    assert_eq!(before.visible.get(1).copied(), Some(true));

    let inside = sample_pose(&hierarchy, &animation, 4.0);
    assert_eq!(inside.visible.get(1).copied(), Some(false));

    let after = sample_pose(&hierarchy, &animation, 8.0);
    assert_eq!(after.visible.get(1).copied(), Some(true));

    assert_eq!(before.visible.first().copied(), Some(true));
}

#[test]
fn a_sparse_visibility_channel_holds_its_value_between_keys() {
    let hierarchy = two_pivot_hierarchy_for_visibility();
    let channel = sparse_bits_channel(1, true, vec![(5, false), (12, true)]);
    let animation = animation_with(vec![channel], 20);

    let before_first_key = sample_pose(&hierarchy, &animation, 2.0);
    assert_eq!(before_first_key.visible.get(1).copied(), Some(true));

    let at_first_key = sample_pose(&hierarchy, &animation, 5.0);
    assert_eq!(at_first_key.visible.get(1).copied(), Some(false));

    let holds_after_first_key = sample_pose(&hierarchy, &animation, 8.0);
    assert_eq!(holds_after_first_key.visible.get(1).copied(), Some(false));

    let at_second_key = sample_pose(&hierarchy, &animation, 12.0);
    assert_eq!(at_second_key.visible.get(1).copied(), Some(true));

    let holds_after_second_key = sample_pose(&hierarchy, &animation, 15.0);
    assert_eq!(holds_after_second_key.visible.get(1).copied(), Some(true));
}

#[test]
fn a_fractional_frame_takes_the_floor_for_visibility_never_interpolates() {
    let hierarchy = two_pivot_hierarchy_for_visibility();
    let channel = bits_channel(1, 0, 1, true, vec![0b0000_0010]);
    let animation = animation_with(vec![channel], 2);

    for frame in [0.0, 0.1, 0.5, 0.999] {
        let pose = sample_pose(&hierarchy, &animation, frame);
        assert_eq!(
            pose.visible.get(1).copied(),
            Some(false),
            "frame {frame} should still read frame 0's value"
        );
    }
    let pose = sample_pose(&hierarchy, &animation, 1.5);
    assert_eq!(pose.visible.get(1).copied(), Some(true));
}

#[test]
fn a_negative_or_non_finite_frame_reads_the_bit_channels_default_without_panicking() {
    let hierarchy = two_pivot_hierarchy_for_visibility();
    let channel = bits_channel(1, 0, 3, true, vec![0b0000_0000]);
    let animation = animation_with(vec![channel], 4);
    for frame in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let pose = sample_pose(&hierarchy, &animation, frame);
        assert_eq!(pose.visible.get(1).copied(), Some(true), "frame {frame}");
    }
}

#[test]
fn a_packed_array_shorter_than_its_declared_range_reads_the_default_rather_than_panicking() {
    let hierarchy = two_pivot_hierarchy_for_visibility();
    let channel = bits_channel(1, 0, 100, false, vec![]);
    let animation = animation_with(vec![channel], 101);
    let pose = sample_pose(&hierarchy, &animation, 50.0);
    assert_eq!(pose.visible.get(1).copied(), Some(false));
}

#[test]
fn visibility_does_not_drive_translation_rotation_or_opacity() {
    let hierarchy = two_pivot_hierarchy_for_visibility();
    let channel = bits_channel(1, 0, 0, true, vec![0b0000_0000]);
    let animation = animation_with(vec![channel], 1);
    let pose = sample_pose(&hierarchy, &animation, 0.0);
    let bone = pose.world.get(1).expect("bone pivot");
    assert!(vec3_approx(bone.translation, Vec3::default()));
    assert!(quat_approx(bone.rotation, IDENTITY_Q));
    assert_eq!(pose.opacity.get(1).copied(), Some(1.0));
    assert_eq!(pose.visible.get(1).copied(), Some(false));
}
