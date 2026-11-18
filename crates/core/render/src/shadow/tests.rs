use super::*;

#[test]
fn the_prelude_offers_both_shadow_lookups_and_the_weighted_one_is_the_raw_one() {
    let prelude = crate::CUSTOM_SHADER_PRELUDE;
    assert!(
        prelude.contains("fn mint_shadow_visibility(world_position: vec3<f32>) -> f32 {"),
        "the unweighted lookup is what a per-material shadow floor needs"
    );
    assert!(
        prelude.contains("fn mint_shadow_factor(world_position: vec3<f32>) -> f32 {"),
        "and the weighted one is what a caller with no opinion calls"
    );
    assert!(
            prelude.contains("mint_shadow_visibility(world_position),"),
            "mint_shadow_factor must be mix(lightness, 1, visibility) over the              same lookup, not a second copy of the filter"
        );
    assert_eq!(
        prelude.matches("textureSampleCompareLevel(").count(),
        11,
        "one set of shadow filters in the prelude - one tap, four taps, the tent's four, and the          contact path's fallback tap and disk - not a second copy"
    );
}

fn orbiting(pitch: f32, distance: f32) -> Camera {
    let (sin, cos) = mint_math::f32::sin_cos(pitch);
    Camera {
        eye: Vec3::new(distance * cos, 0.0, distance * sin),
        target: Vec3::ZERO,
        up: Vec3::Z,
        fov_y: std::f32::consts::FRAC_PI_4,
        near: (distance * 0.005).max(1e-3),
        far: distance * 10.0,
    }
}

fn fit() -> ShadowFit {
    ShadowFit {
        distance: 4000.0,
        margin: 0.0,
        depth: 600.0,
        texels: 1024,
        bounds: None,
    }
}

fn inside(view_proj: Mat4, point: Vec3, slack: f32) -> bool {
    let ndc = view_proj.transform_point(point);
    ndc.x.abs() <= 1.0 + slack
        && ndc.y.abs() <= 1.0 + slack
        && ndc.z >= -slack
        && ndc.z <= 1.0 + slack
}

fn world() -> Aabb {
    Aabb::from_corners(
        Vec3::new(-3000.0, -3000.0, -50.0),
        Vec3::new(3000.0, 3000.0, 250.0),
    )
}

#[test]
fn a_world_box_keeps_the_far_ground_a_shallow_view_can_see() {
    let light = Vec3::new(0.67, 0.55, -0.5);
    let camera = orbiting(0.05, 300.0);
    let fitted = fit_directional_light_to_view(
        light,
        &camera,
        16.0 / 9.0,
        ShadowFit {
            distance: f32::INFINITY,
            bounds: Some(world()),
            ..fit()
        },
    );
    let far_ground = Vec3::new(-2000.0, 0.0, 0.0);
    assert!(
        inside(fitted.view_proj, far_ground, 1e-3),
        "{far_ground:?} is in front of the camera and outside the volume"
    );
}

#[test]
fn the_reach_alone_loses_the_ground_the_world_box_keeps() {
    let light = Vec3::new(0.67, 0.55, -0.5);
    let camera = orbiting(0.05, 300.0);
    let fitted = fit_directional_light_to_view(
        light,
        &camera,
        16.0 / 9.0,
        ShadowFit {
            distance: 300.0 * 1.15,
            bounds: None,
            ..fit()
        },
    );
    assert!(!inside(
        fitted.view_proj,
        Vec3::new(-2000.0, 0.0, 0.0),
        1e-3
    ));
}

#[test]
fn a_world_box_prunes_the_sky_a_steep_view_would_spend_the_volume_on() {
    let light = Vec3::new(0.4, 0.7, -0.6);
    let camera = orbiting(1.2, 300.0);
    let unbounded = fit_directional_light_to_view(
        light,
        &camera,
        16.0 / 9.0,
        ShadowFit {
            distance: f32::INFINITY,
            bounds: None,
            ..fit()
        },
    );
    let bounded = fit_directional_light_to_view(
        light,
        &camera,
        16.0 / 9.0,
        ShadowFit {
            distance: f32::INFINITY,
            bounds: Some(world()),
            ..fit()
        },
    );
    assert!(
        bounded.radius * 4.0 < unbounded.radius,
        "the box saved nothing: {} against {}",
        bounded.radius,
        unbounded.radius
    );
}

#[test]
fn a_world_the_camera_cannot_see_leaves_the_fit_alone() {
    let light = Vec3::new(0.4, 0.7, -0.6);
    let camera = orbiting(0.5, 300.0);
    let elsewhere = Aabb::from_corners(
        Vec3::new(90_000.0, 90_000.0, 0.0),
        Vec3::new(91_000.0, 91_000.0, 10.0),
    );
    let plain = fit_directional_light_to_view(light, &camera, 16.0 / 9.0, fit());
    let clipped = fit_directional_light_to_view(
        light,
        &camera,
        16.0 / 9.0,
        ShadowFit {
            bounds: Some(elsewhere),
            ..fit()
        },
    );
    assert_eq!(plain, clipped);
}

#[test]
fn the_ball_is_centred_on_the_region_and_not_on_the_whole_view() {
    let light = Vec3::new(0.67, 0.55, -0.5);
    let camera = orbiting(0.05, 300.0);
    let settings = ShadowFit {
        distance: f32::INFINITY,
        bounds: Some(world()),
        ..fit()
    };
    let fitted = fit_directional_light_to_view(light, &camera, 16.0 / 9.0, settings);
    let corners = camera.frustum_corners(16.0 / 9.0, camera.near, camera.far);
    let region = crate::frustum::visible_region(&corners, &world()).expect("in view");
    let mut box_of_region = Aabb::EMPTY;
    for point in region.points() {
        box_of_region.insert(*point);
    }
    assert!(
        fitted.radius <= box_of_region.radius() * 1.2,
        "the ball is {} wide against a {} box around the same points",
        fitted.radius,
        box_of_region.radius()
    );
}

#[test]
fn every_visible_point_of_the_world_lands_in_the_volume_at_every_pitch() {
    let light = Vec3::new(0.67, 0.55, -0.5);
    let aspect = 16.0 / 9.0;
    for pitch in [0.05_f32, 0.2, 0.5, 0.9, 1.3] {
        for distance in [300.0_f32, 3000.0] {
            let camera = orbiting(pitch, distance);
            let fitted = fit_directional_light_to_view(
                light,
                &camera,
                aspect,
                ShadowFit {
                    distance: f32::INFINITY,
                    bounds: Some(world()),
                    ..fit()
                },
            );
            let view = camera.view_projection(aspect);
            let mut seen = 0usize;
            for (half, step) in [(3000.0_f32, 125.0_f32), (600.0, 25.0)] {
                let mut x = -half;
                while x <= half {
                    let mut y = -half;
                    while y <= half {
                        for zi in 0..5u8 {
                            let point = Vec3::new(x, y, -50.0 + 300.0 * f32::from(zi) / 4.0);
                            if !inside(view, point, 0.0) {
                                continue;
                            }
                            seen += 1;
                            assert!(
                                inside(fitted.view_proj, point, 2e-3),
                                "pitch {pitch} distance {distance}: {point:?} is on \
                                     screen and outside the light's volume"
                            );
                        }
                        y += step;
                    }
                    x += step;
                }
            }
            assert!(
                seen > 20,
                "pitch {pitch} distance {distance} saw only {seen} sample(s)"
            );
        }
    }
}
#[test]
fn the_fitted_volume_contains_the_whole_visible_slice() {
    let light = Vec3::new(0.4, 0.7, -0.6);
    for pitch in [0.1_f32, 0.5, 0.9, 1.3] {
        for distance in [500.0_f32, 4000.0, 20000.0] {
            let camera = orbiting(pitch, distance);
            let fitted = fit_directional_light_to_view(light, &camera, 16.0 / 9.0, fit());
            let far = camera.far.min(fit().distance);
            for (index, corner) in camera
                .frustum_corners(16.0 / 9.0, camera.near, far)
                .into_iter()
                .enumerate()
            {
                assert!(
                    inside(fitted.view_proj, corner, 1e-3),
                    "pitch {pitch} distance {distance} corner {index} \
                         {corner:?} is outside the fitted volume"
                );
            }
        }
    }
}

#[test]
fn the_volume_reaches_exactly_the_promised_distance_up_light_of_the_view() {
    let light = Vec3::new(0.3, 0.5, -0.8).normalize();
    let camera = orbiting(0.5, 4000.0);
    let fitted = fit_directional_light_to_view(light, &camera, 16.0 / 9.0, fit());
    let brink = Vec3::new(
        fitted.centre.x - light.x * fitted.radius,
        fitted.centre.y - light.y * fitted.radius,
        fitted.centre.z - light.z * fitted.radius,
    );
    let up_light = |back: f32| {
        Vec3::new(
            brink.x - light.x * back,
            brink.y - light.y * back,
            brink.z - light.z * back,
        )
    };
    for back in [0.0_f32, fit().depth * 0.5, fit().depth * 0.99] {
        assert!(
            inside(fitted.view_proj, up_light(back), 1e-3),
            "a caster {back} world units up-light of the fitted ball is \
                 outside the volume"
        );
    }
    assert!(
        !inside(fitted.view_proj, up_light(fit().depth * 1.05), 0.0),
        "the volume reached past the headroom it was given"
    );
}

#[test]
fn snapping_moves_the_volume_a_whole_texel_at_a_time_and_nothing_else_does() {
    let light = Vec3::new(0.4, 0.7, -0.6);
    let probe = Vec3::new(37.0, -11.0, 3.0);
    let base = orbiting(0.5, 4000.0);
    let reference = fit_directional_light_to_view(light, &base, 16.0 / 9.0, fit());
    let step = reference.texel / 8.0;
    assert!(step > 0.0, "the fit produced no texel size");
    let texels = f32::from(u16::try_from(fit().texels).unwrap());

    let column = |camera: &Camera, snapping: bool| {
        let mut this = fit();
        if !snapping {
            this.texels = 0;
        }
        let fitted = fit_directional_light_to_view(light, camera, 16.0 / 9.0, this);
        let ndc = fitted.view_proj.transform_point(probe);
        (ndc.x * 0.5 + 0.5) * texels
    };

    let (snapped_origin, loose_origin) = (column(&base, true), column(&base, false));
    let mut loose_moved_a_fraction = 0;
    for offset in 1..8 {
        let mut camera = base;
        let slide = step * offset as f32;
        camera.eye.x += slide;
        camera.target.x += slide;

        let moved = column(&camera, true) - snapped_origin;
        assert!(
            (moved - moved.round()).abs() < 2e-2,
            "offset {offset}: the snapped volume moved {moved} texels, \
                 which is not a whole number"
        );

        let loose = column(&camera, false) - loose_origin;
        if (loose - loose.round()).abs() >= 2e-2 {
            loose_moved_a_fraction += 1;
        }
    }
    assert!(
        loose_moved_a_fraction >= 4,
        "the unsnapped fit moved by whole texels {} times of 7, so the \
             assertion above proves nothing",
        7 - loose_moved_a_fraction
    );
}

#[test]
fn the_view_fit_reaches_where_a_target_fit_does_not() {
    let light = Vec3::new(0.4, 0.7, -0.6);
    let camera = orbiting(0.3, 4000.0);
    let aspect = 16.0 / 9.0;
    let target_fit = directional_light_view_proj(light, camera.target, 900.0, 2700.0);
    let view_fit = fit_directional_light_to_view(light, &camera, aspect, fit()).view_proj;
    let corners = camera.frustum_corners(aspect, camera.near, 4000.0);
    let missed = corners
        .into_iter()
        .filter(|corner| !inside(target_fit, *corner, 0.0))
        .count();
    assert!(
        missed > 0,
        "the target fit already covered the whole slice, so there was \
             nothing to fix"
    );
    for corner in corners {
        assert!(
            inside(view_fit, corner, 1e-3),
            "the view fit missed {corner:?}"
        );
    }
}

#[test]
fn the_reach_caps_the_slice_and_the_camera_caps_the_reach() {
    let light = Vec3::new(0.0, 0.0, -1.0);
    let camera = orbiting(0.5, 4000.0);
    let radius = |distance: f32| {
        fit_directional_light_to_view(light, &camera, 16.0 / 9.0, ShadowFit { distance, ..fit() })
            .radius
    };
    assert!(
        radius(2000.0) < radius(8000.0),
        "a longer reach must fit a larger volume"
    );
    assert!(
        (radius(100_000.0) - radius(f32::INFINITY)).abs() < 1e-3,
        "a reach past the far plane must not grow the volume"
    );
}

#[test]
fn the_reported_range_is_the_depth_the_volume_actually_spans() {
    let light = Vec3::new(0.3, 0.5, -0.8).normalize();
    for distance in [1000.0_f32, 4000.0, 16000.0] {
        let camera = orbiting(0.5, 4000.0);
        let fitted = fit_directional_light_to_view(
            light,
            &camera,
            16.0 / 9.0,
            ShadowFit { distance, ..fit() },
        );
        let far = Vec3::new(
            fitted.centre.x + light.x * fitted.radius,
            fitted.centre.y + light.y * fitted.radius,
            fitted.centre.z + light.z * fitted.radius,
        );
        let near = Vec3::new(
            far.x - light.x * fitted.range,
            far.y - light.y * fitted.range,
            far.z - light.z * fitted.range,
        );
        let at = |p: Vec3| fitted.view_proj.transform_point(p).z;
        assert!(
            (at(far) - 1.0).abs() < 1e-3,
            "distance {distance}: the far end of `range` is at {}",
            at(far)
        );
        assert!(
            at(near).abs() < 1e-3,
            "distance {distance}: the near end of `range` is at {}",
            at(near)
        );
    }
}

#[test]
fn a_degenerate_fit_produces_a_finite_matrix() {
    let camera = Camera {
        eye: Vec3::ZERO,
        target: Vec3::ZERO,
        ..Camera::default()
    };
    let hostile = [
        ShadowFit {
            distance: f32::NAN,
            margin: f32::NAN,
            depth: f32::NAN,
            texels: 0,
            bounds: Some(Aabb::from_corners(
                Vec3::splat(f32::NAN),
                Vec3::splat(f32::INFINITY),
            )),
        },
        ShadowFit {
            distance: -1.0,
            margin: -1.0,
            depth: -1.0,
            texels: 1,
            bounds: Some(Aabb::EMPTY),
        },
        ShadowFit {
            distance: 0.0,
            margin: f32::INFINITY,
            depth: f32::INFINITY,
            texels: u32::MAX,
            bounds: Some(Aabb::from_corners(Vec3::ZERO, Vec3::ZERO)),
        },
    ];
    for (index, fit) in hostile.into_iter().enumerate() {
        for direction in [Vec3::ZERO, Vec3::new(f32::NAN, 0.0, 0.0), Vec3::Z] {
            let fitted = fit_directional_light_to_view(direction, &camera, f32::NAN, fit);
            assert!(
                fitted
                    .view_proj
                    .to_cols_array()
                    .iter()
                    .all(|v| v.is_finite()),
                "fit {index} direction {direction:?} gave {:?}",
                fitted.view_proj
            );
            assert!(fitted.radius.is_finite() && fitted.radius > 0.0);
            assert!(fitted.texel.is_finite() && fitted.texel >= 0.0);
        }
    }
}

#[test]
fn the_fitted_radius_does_not_change_when_the_camera_turns() {
    let light = Vec3::new(0.4, 0.7, -0.6);
    let mut radii = Vec::new();
    for pitch in [0.1_f32, 0.5, 0.9, 1.3] {
        radii.push(fit_directional_light_to_view(
            light,
            &orbiting(pitch, 4000.0),
            16.0 / 9.0,
            fit(),
        ));
    }
    let first = radii[0].radius;
    for fitted in &radii {
        assert!(
            (fitted.radius - first).abs() < 1e-2,
            "radius moved from {first} to {} when the camera turned",
            fitted.radius
        );
    }
    assert!(
        (radii[0].centre.x - radii[3].centre.x).abs() > 1.0,
        "the cameras were not actually different"
    );
}

#[test]
fn the_view_extent_stops_where_the_world_does() {
    let light = Vec3::new(0.4, 0.7, -0.6);
    let camera = orbiting(1.2, 300.0);
    let open = fit_directional_light_to_view(
        light,
        &camera,
        16.0 / 9.0,
        ShadowFit {
            distance: 3000.0,
            bounds: None,
            ..fit()
        },
    );
    assert!(
        (open.view_extent - 3000.0).abs() < 1.0,
        "unclipped, the extent is the reach: {}",
        open.view_extent
    );
    let clipped = fit_directional_light_to_view(
        light,
        &camera,
        16.0 / 9.0,
        ShadowFit {
            distance: 3000.0,
            bounds: Some(world()),
            ..fit()
        },
    );
    assert!(
        clipped.view_extent > 250.0 && clipped.view_extent < 600.0,
        "clipped to the world, the extent is the ground's: {}",
        clipped.view_extent
    );
    assert!(
        clipped.view_start > camera.near && clipped.view_start < clipped.view_extent,
        "and it begins where the view first meets the world, not at the near plane: {}",
        clipped.view_start
    );
}
