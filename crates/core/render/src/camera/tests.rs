use super::*;

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-5
}

#[test]
fn a_pixels_ray_reprojects_to_that_pixel() {
    let camera = Camera {
        eye: Vec3::new(120.0, -80.0, 260.0),
        target: Vec3::new(10.0, 30.0, 5.0),
        up: Vec3::Z,
        ..Camera::default()
    };
    let viewport = (1280, 720);
    let aspect = 1280.0 / 720.0;
    let view_projection = camera.view_projection(aspect);
    for (px, py) in [(640.0, 360.0), (0.0, 0.0), (1279.0, 719.0), (100.0, 600.0)] {
        let ray = camera.ray_through_pixel(viewport, px, py).unwrap();
        let clip = view_projection.transform_point(ray.at(500.0));
        let back_x = (clip.x + 1.0) * 0.5 * 1280.0;
        let back_y = (1.0 - clip.y) * 0.5 * 720.0;
        assert!(
            (back_x - px).abs() < 0.05 && (back_y - py).abs() < 0.05,
            "pixel ({px}, {py}) came back as ({back_x}, {back_y})"
        );
    }
}

#[test]
fn the_centre_pixels_ray_looks_where_the_camera_looks() {
    let camera = Camera {
        eye: Vec3::new(0.0, -100.0, 40.0),
        target: Vec3::ZERO,
        up: Vec3::Z,
        ..Camera::default()
    };
    let ray = camera.ray_through_pixel((800, 600), 400.0, 300.0).unwrap();
    let forward = camera.forward();
    assert!(close(ray.direction.dot(forward), 1.0), "{ray:?}");
    assert_eq!(ray.origin, camera.eye);
}

#[test]
fn a_degenerate_viewport_or_pixel_has_no_ray_rather_than_a_nan_one() {
    let camera = Camera::default();
    assert!(camera.ray_through_pixel((0, 600), 1.0, 1.0).is_none());
    assert!(camera.ray_through_pixel((800, 0), 1.0, 1.0).is_none());
    assert!(camera
        .ray_through_pixel((800, 600), f32::NAN, 1.0)
        .is_none());
    assert!(camera.ray_through_pixel((100_000, 600), 1.0, 1.0).is_none());
}

#[test]
fn no_pixel_of_any_camera_produces_a_non_finite_ray() {
    let cameras = [
        Camera::default(),
        Camera {
            eye: Vec3::new(0.0, 0.0, 5.0),
            target: Vec3::ZERO,
            up: Vec3::Z,
            ..Camera::default()
        },
        Camera {
            eye: Vec3::ZERO,
            target: Vec3::ZERO,
            ..Camera::default()
        },
    ];
    for camera in cameras {
        for px in [0.0, 400.0, 799.0] {
            for py in [0.0, 300.0, 599.0] {
                if let Some(ray) = camera.ray_through_pixel((800, 600), px, py) {
                    assert!(ray.direction.is_finite(), "{camera:?} at ({px}, {py})");
                    assert!(ray.at(10.0).is_finite());
                }
            }
        }
    }
}

#[test]
fn the_target_projects_to_the_centre_of_the_screen() {
    let camera = Camera {
        eye: Vec3::new(10.0, 4.0, -3.0),
        target: Vec3::new(1.0, 2.0, 3.0),
        up: Vec3::Z,
        ..Camera::default()
    };
    let ndc = camera
        .view_projection(16.0 / 9.0)
        .transform_point(camera.target);

    assert!(close(ndc.x, 0.0), "x {}", ndc.x);
    assert!(close(ndc.y, 0.0), "y {}", ndc.y);
    assert!(
        ndc.z > 0.0 && ndc.z < 1.0,
        "target should be inside the frustum, z {}",
        ndc.z
    );
}

#[test]
fn a_point_behind_the_camera_lands_outside_the_depth_range() {
    let camera = Camera {
        eye: Vec3::new(0.0, 0.0, 5.0),
        target: Vec3::ZERO,
        up: Vec3::Y,
        ..Camera::default()
    };
    let behind = camera
        .view_projection(1.0)
        .transform_point(Vec3::new(0.0, 0.0, 15.0));
    assert!(behind.z < 0.0 || behind.z > 1.0, "z {}", behind.z);
}

#[test]
fn a_degenerate_aspect_ratio_does_not_produce_a_matrix_of_infinities() {
    let camera = Camera::default();
    for aspect in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let m = camera.projection(aspect);
        assert!(
            m.to_cols_array().iter().all(|v| v.is_finite()),
            "aspect {aspect} produced {m:?}"
        );
    }
}

#[test]
fn the_up_axis_is_honoured_rather_than_assumed_to_be_y() {
    let camera = Camera {
        eye: Vec3::new(5.0, 0.0, 0.0),
        target: Vec3::ZERO,
        up: Vec3::Z,
        ..Camera::default()
    };
    let above = camera
        .view_projection(1.0)
        .transform_point(Vec3::new(0.0, 0.0, 1.0));
    assert!(
        above.y > 0.0,
        "world +Z should be up on screen, got {above:?}"
    );
}

#[test]
fn forward_points_from_the_eye_to_the_target() {
    let camera = Camera {
        eye: Vec3::new(0.0, 0.0, 5.0),
        target: Vec3::ZERO,
        ..Camera::default()
    };
    assert_eq!(camera.forward(), Vec3::new(0.0, 0.0, -1.0));
}

#[test]
fn every_frustum_corner_lands_on_the_edge_of_clip_space() {
    let camera = Camera {
        eye: Vec3::new(120.0, -40.0, 90.0),
        target: Vec3::new(-10.0, 30.0, 5.0),
        up: Vec3::Z,
        fov_y: 0.9,
        near: 2.0,
        far: 500.0,
    };
    let aspect = 16.0 / 9.0;
    let view_proj = camera
        .projection_with_depth(DepthDirection::Standard, aspect)
        .mul_mat4(camera.view());
    for (index, corner) in camera
        .frustum_corners(aspect, camera.near, camera.far)
        .into_iter()
        .enumerate()
    {
        let ndc = view_proj.transform_point(corner);
        assert!(
            (ndc.x.abs() - 1.0).abs() < 1e-3,
            "corner {index} x {} is not on a side plane",
            ndc.x
        );
        assert!(
            (ndc.y.abs() - 1.0).abs() < 1e-3,
            "corner {index} y {} is not on a side plane",
            ndc.y
        );
        let want = if index < 4 { 0.0 } else { 1.0 };
        assert!(
            (ndc.z - want).abs() < 1e-3,
            "corner {index} z {} should be {want}",
            ndc.z
        );
    }
}

#[test]
fn the_far_quad_is_further_from_the_eye_than_the_near_quad() {
    let camera = Camera {
        eye: Vec3::new(0.0, 0.0, 10.0),
        target: Vec3::ZERO,
        up: Vec3::Y,
        ..Camera::default()
    };
    let corners = camera.frustum_corners(1.0, 1.0, 50.0);
    for index in 0..4 {
        let near = corners[index];
        let far = corners[index + 4];
        assert!(
            (near.z - 9.0).abs() < 1e-3,
            "near corner {index} z {}",
            near.z
        );
        assert!(
            (far.z + 40.0).abs() < 1e-3,
            "far corner {index} z {}",
            far.z
        );
    }
}

#[test]
fn a_field_of_view_outside_the_sensible_range_does_not_invert_the_frustum() {
    for fov_y in [-1.0_f32, 0.0, std::f32::consts::PI, 4.0, 100.0] {
        let camera = Camera {
            eye: Vec3::new(0.0, 0.0, 10.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y,
            near: 1.0,
            far: 50.0,
        };
        let corners = camera.frustum_corners(1.0, 1.0, 50.0);
        assert!(
            corners[4].x < 0.0 && corners[5].x > 0.0,
            "fov {fov_y}: the far quad runs right to left, {:?} .. {:?}",
            corners[4],
            corners[5]
        );
        assert!(
            corners[4].y < 0.0 && corners[6].y > 0.0,
            "fov {fov_y}: the far quad runs top to bottom"
        );
    }
}

#[test]
fn degenerate_frustum_corners_are_finite_rather_than_nan() {
    let degenerate = [
        Camera {
            fov_y: f32::NAN,
            ..Camera::default()
        },
        Camera {
            eye: Vec3::ZERO,
            target: Vec3::ZERO,
            ..Camera::default()
        },
        Camera {
            eye: Vec3::new(0.0, 5.0, 0.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            ..Camera::default()
        },
        Camera {
            fov_y: std::f32::consts::PI,
            ..Camera::default()
        },
    ];
    for (index, camera) in degenerate.into_iter().enumerate() {
        for aspect in [1.0, 0.0, f32::NAN] {
            for (near, far) in [(1.0, 100.0), (100.0, 1.0), (f32::NAN, f32::NAN)] {
                for corner in camera.frustum_corners(aspect, near, far) {
                    assert!(
                        corner.is_finite(),
                        "camera {index} aspect {aspect} near {near} far {far} gave {corner:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_camera_sitting_on_its_own_target_yields_finite_matrices() {
    let camera = Camera {
        eye: Vec3::ZERO,
        target: Vec3::ZERO,
        ..Camera::default()
    };
    assert!(camera
        .view_projection(1.0)
        .to_cols_array()
        .iter()
        .all(|v| v.is_finite()));
}
