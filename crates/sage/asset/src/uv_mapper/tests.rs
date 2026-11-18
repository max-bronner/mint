use super::*;

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-5
}

#[test]
fn identity_leaves_a_coordinate_untouched() {
    assert_eq!(UvTransform::IDENTITY.apply([0.3, 0.7]), [0.3, 0.7]);
}

#[test]
fn an_unimplemented_mapper_samples_as_the_identity() {
    let args = MapperArgs::default();
    assert_eq!(
        sample_uv_transform(UvMapperType::StepLinearOffset, &args, 5.0),
        UvTransform::IDENTITY
    );
    assert_eq!(
        sample_uv_transform(UvMapperType::Environment, &args, 5.0),
        UvTransform::IDENTITY
    );
}

fn grid_15_4x4() -> MapperArgs {
    MapperArgs {
        fps: 15.0,
        log2_width: 2,
        ..MapperArgs::default()
    }
}

fn grid_at(args: &MapperArgs, seconds: f32) -> [f32; 2] {
    sample_uv_transform(UvMapperType::Grid, args, seconds).offset
}

#[test]
fn grid_walks_the_sheet_left_to_right_then_down_and_wraps() {
    let args = grid_15_4x4();
    let step = 0.066;
    assert_eq!(grid_at(&args, 0.0), [0.0, 0.0], "cell 0");
    assert_eq!(grid_at(&args, step), [0.25, 0.0], "cell 1: one column on");
    assert_eq!(grid_at(&args, step * 3.0), [0.75, 0.0], "cell 3");
    assert_eq!(grid_at(&args, step * 4.0), [0.0, 0.25], "cell 4: next row");
    assert_eq!(grid_at(&args, step * 15.0), [0.75, 0.75], "cell 15, last");
    assert_eq!(grid_at(&args, step * 16.0), [0.0, 0.0], "wrapped");
    let sampled = sample_uv_transform(
        UvMapperType::Grid,
        &MapperArgs {
            u_scale: 7.0,
            v_scale: 9.0,
            ..grid_15_4x4()
        },
        step,
    );
    assert_eq!(sampled.linear, UvTransform::IDENTITY.linear);
}

#[test]
fn grid_advances_on_whole_milliseconds_the_way_the_original_does() {
    let args = MapperArgs {
        fps: 30.0,
        log2_width: 3,
        ..MapperArgs::default()
    };
    let cell = 1.0 / 8.0;
    assert_eq!(grid_at(&args, 10.0), [7.0 * cell, 5.0 * cell]);
    assert_ne!(grid_at(&args, 10.0), [4.0 * cell, 5.0 * cell]);
}

#[test]
fn fps_zero_freezes_the_sheet_on_its_starting_cell() {
    let args = MapperArgs {
        fps: 0.0,
        log2_width: 2,
        offset: 6,
        ..MapperArgs::default()
    };
    for t in [0.0, 1.0, 600.0] {
        assert_eq!(grid_at(&args, t), [0.5, 0.25], "t = {t}");
    }
}

#[test]
fn a_negative_fps_runs_the_sheet_backwards_from_its_last_cell() {
    let args = MapperArgs {
        fps: -15.0,
        log2_width: 2,
        ..MapperArgs::default()
    };
    let step = 0.066;
    assert_eq!(grid_at(&args, 0.0), [0.75, 0.75], "starts at cell 15");
    assert_eq!(grid_at(&args, step), [0.5, 0.75], "cell 14");
    assert_eq!(grid_at(&args, step * 15.0), [0.0, 0.0], "cell 0");
    assert_eq!(grid_at(&args, step * 16.0), [0.75, 0.75], "wrapped back");
}

#[test]
fn offset_starts_the_sheet_partway_in_and_is_taken_modulo_last() {
    let args = MapperArgs {
        offset: 4,
        ..grid_15_4x4()
    };
    assert_eq!(grid_at(&args, 0.0), [0.0, 0.25], "cell 4");
    for offset in [4, 20] {
        let backwards = MapperArgs {
            fps: -15.0,
            offset,
            ..grid_15_4x4()
        };
        assert_eq!(
            grid_at(&backwards, 0.0),
            [0.75, 0.5],
            "backwards from cell 11, offset {offset}"
        );
    }
}

#[test]
fn last_stops_the_sheet_short_of_its_own_width() {
    let args = MapperArgs {
        last: 3,
        ..grid_15_4x4()
    };
    let step = 0.066;
    assert_eq!(grid_at(&args, step * 2.0), [0.5, 0.0]);
    assert_eq!(grid_at(&args, step * 3.0), [0.0, 0.0], "wraps at Last");
    assert_eq!(grid_at(&grid_15_4x4(), step * 3.0), [0.75, 0.0]);
}

#[test]
fn an_absurd_log2_width_is_clamped_rather_than_shifting_out_of_range() {
    for log2_width in [8, 9, 32, 33, 1000, u32::MAX] {
        let args = MapperArgs {
            log2_width,
            ..grid_15_4x4()
        };
        let offset = grid_at(&args, 1.0);
        assert!(
            (0.0..1.0).contains(&offset[0]) && (0.0..1.0).contains(&offset[1]),
            "log2_width {log2_width} gave {offset:?}"
        );
    }
}

#[test]
fn a_non_finite_clock_or_rate_leaves_the_sheet_where_it_started() {
    let args = grid_15_4x4();
    for t in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(grid_at(&args, t), [0.0, 0.0], "t = {t}");
    }
    for fps in [f32::NAN, f32::INFINITY] {
        let args = MapperArgs {
            fps,
            ..grid_15_4x4()
        };
        assert_eq!(grid_at(&args, 3.0), [0.0, 0.0], "fps = {fps}");
    }
}

#[test]
fn every_shipped_grid_shape_stays_on_a_cell_boundary_inside_the_sheet() {
    for log2_width in 0..=4u32 {
        for fps in [-30.0, -15.0, 0.0, 10.0, 12.0, 15.0, 20.0, 24.0, 30.0] {
            for last in [0, 16] {
                for offset in [0, 3] {
                    let args = MapperArgs {
                        fps,
                        log2_width,
                        last,
                        offset,
                        ..MapperArgs::default()
                    };
                    let width = f64::from(1u32 << log2_width);
                    for tick in 0..400 {
                        let seconds = f64::from(tick) * 0.037;
                        #[allow(clippy::cast_possible_truncation)]
                        let uv = grid_at(&args, seconds as f32);
                        for value in uv {
                            assert!(
                                (0.0..1.0).contains(&value),
                                "{args:?} at {seconds}s gave {uv:?}"
                            );
                            let cells = f64::from(value) * width;
                            assert!(
                                (cells - cells.round()).abs() < 1e-6,
                                "{args:?} at {seconds}s is off a cell edge: {uv:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn uv_mapper_samples_as_the_identity_at_every_time() {
    let args = MapperArgs::default();
    for t in [0.0, 1.0, 100.0] {
        assert_eq!(
            sample_uv_transform(UvMapperType::Uv, &args, t),
            UvTransform::IDENTITY
        );
    }
}

#[test]
fn linear_offset_moves_the_visible_image_forward_not_the_sample_point() {
    let args = MapperArgs {
        u_per_sec: 0.3,
        v_per_sec: 0.3,
        u_scale: 0.001,
        v_scale: 0.001,
        ..MapperArgs::default()
    };
    let at0 = sample_uv_transform(UvMapperType::LinearOffset, &args, 0.0);
    assert_eq!(at0.offset, [0.0, 0.0]);

    let at1 = sample_uv_transform(UvMapperType::LinearOffset, &args, 1.0);
    assert!(close(at1.offset[0], 0.7), "got {}", at1.offset[0]);
    assert!(close(at1.offset[1], 0.7), "got {}", at1.offset[1]);
    assert_eq!(at1.linear, [[0.001, 0.0], [0.0, 0.001]]);

    assert_ne!(at0.apply([0.0, 0.0]), at1.apply([0.0, 0.0]));
}

#[test]
fn linear_offset_wraps_so_a_long_session_does_not_grow_without_bound() {
    let args = MapperArgs {
        u_per_sec: 1.0,
        ..MapperArgs::default()
    };
    let far = sample_uv_transform(UvMapperType::LinearOffset, &args, 10_007.25);
    assert!((0.0..1.0).contains(&far.offset[0]));
    assert!(close(far.offset[0], 0.75), "got {}", far.offset[0]);
}

#[test]
fn rotate_turns_the_visible_image_counter_clockwise() {
    let args = MapperArgs {
        speed: 1.0,
        u_center: 0.5,
        v_center: 0.5,
        u_scale: 1.0,
        v_scale: 1.0,
        ..MapperArgs::default()
    };
    let quarter = sample_uv_transform(UvMapperType::Rotate, &args, 0.25);
    let moved = quarter.apply([0.5, 1.0]);
    assert!(close(moved[0], 1.0), "u: got {moved:?}");
    assert!(close(moved[1], 0.5), "v: got {moved:?}");

    let full = sample_uv_transform(UvMapperType::Rotate, &args, 1.0);
    let back = full.apply([0.5, 1.0]);
    assert!(close(back[0], 0.5), "u: got {back:?}");
    assert!(close(back[1], 1.0), "v: got {back:?}");
}

#[test]
fn rotate_leaves_its_own_center_fixed() {
    let args = MapperArgs {
        speed: 0.37,
        u_center: 0.2,
        v_center: 0.8,
        u_scale: 1.0,
        v_scale: 1.0,
        ..MapperArgs::default()
    };
    let transform = sample_uv_transform(UvMapperType::Rotate, &args, 3.3);
    let moved = transform.apply([0.2, 0.8]);
    assert!(close(moved[0], 0.2));
    assert!(close(moved[1], 0.8));
}

#[test]
fn sine_linear_offset_is_zero_at_zero_phase_and_moves_at_amplitude_over_a_quarter_cycle() {
    let args = MapperArgs {
        u_amp: -0.1,
        u_freq: 0.25,
        u_phase: 0.0,
        v_amp: 0.1,
        v_freq: 0.25,
        v_phase: 0.0,
        ..MapperArgs::default()
    };
    let at0 = sample_uv_transform(UvMapperType::SineLinearOffset, &args, 0.0);
    assert!(close(at0.offset[0], 0.0));
    assert!(close(at0.offset[1], 0.0));

    let quarter = sample_uv_transform(UvMapperType::SineLinearOffset, &args, 1.0);
    assert!(close(quarter.offset[0], -0.1));
    assert!(close(quarter.offset[1], 0.1));
}

#[test]
fn compose_applies_the_first_transform_then_the_second() {
    let scale = UvTransform {
        linear: [[2.0, 0.0], [0.0, 2.0]],
        offset: [0.0, 0.0],
    };
    let shift = UvTransform {
        linear: [[1.0, 0.0], [0.0, 1.0]],
        offset: [1.0, 0.0],
    };
    let composed = scale.then(shift);
    assert_eq!(composed.apply([0.5, 0.5]), [2.0, 1.0]);
}

#[test]
fn composing_with_identity_on_either_side_is_a_no_op() {
    let t = sample_uv_transform(
        UvMapperType::Rotate,
        &MapperArgs {
            speed: 0.5,
            ..MapperArgs::default()
        },
        1.7,
    );
    assert_eq!(t.then(UvTransform::IDENTITY), t);
    assert_eq!(UvTransform::IDENTITY.then(t), t);
}

#[test]
fn sfe_armydecal_samples_both_stages_independently_and_each_moves_over_time() {
    let info = VertexMaterialInfo {
        attributes: 0x0004_0800,
        ..VertexMaterialInfo::default()
    };
    let args0 = MapperArgs::parse("UPerSec=0.3\r\nVPerSec=0.3\r\nUScale=.001\r\nVScale=.001");
    let args1 = MapperArgs::parse(
        "Speed=0.03;In Hertz. 1 = 1 rotate per second\r\nUCenter=0.5\r\nVCenter=0.5\r\n\
             UScale=1.0\r\nVScale=1.0",
    );
    let at0 = sample_material_uv_transforms(&info, &args0, &args1, 0.0);
    let at1 = sample_material_uv_transforms(&info, &args0, &args1, 1.0);
    assert_ne!(
        at0.stage0.apply([0.5, 0.5]),
        at1.stage0.apply([0.5, 0.5]),
        "stage 0's scroll must move over time"
    );
    assert_ne!(
        at0.stage1.apply([0.5, 1.0]),
        at1.stage1.apply([0.5, 1.0]),
        "stage 1's rotation must move over time, independent of stage 0"
    );
}

#[test]
fn bump_env_turns_its_matrix_at_bump_rotation_scaled_by_bump_scale() {
    let args = MapperArgs {
        bump_rotation: 1.0,
        bump_scale: 2.0,
        ..MapperArgs::default()
    };
    let quarter = bump_matrix(UvMapperType::BumpEnv, &args, 0.25);
    let want = [[0.0, -2.0], [2.0, 0.0]];
    for (got, want) in quarter.iter().flatten().zip(want.iter().flatten()) {
        assert!(close(*got, *want), "{quarter:?}");
    }
    assert_eq!(
        bump_matrix(UvMapperType::LinearOffset, &args, 0.25),
        UvTransform::IDENTITY.linear
    );
}
