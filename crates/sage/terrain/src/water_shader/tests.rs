use super::*;

fn params() -> WaterShaderParams {
    WaterShaderParams {
        wind_angle: 2.4,
        wave_angle: 0.6299997,
        wind_speed: 0.015,
        wave_speed: 0.0,
        bump_speed: 0.0,
        bump_scale: 2.0,
        bump_uv_scale: [1.2, 1.2],
        wave_uv_scale: [7.0, 7.0],
        env_uv_scale: [0.2, 0.2],
        dimming_factor: 0.8,
        reflection_strength: 0.7,
        reflection_perturbation: 0.02,
        specular_color: [1.0, 1.0, 1.0],
        specular_exponent: 50.0,
        shadow_lightness: Some(0.8),
    }
}

#[test]
fn the_body_row_flags_a_uniform_colour_in_w() {
    let tuning = crate::water_tuning::WaterTuning::default();
    let row = |uniform_colour: bool| {
        let depth = WaterDepth {
            body: [0.15, 0.3, 0.35],
            uniform_colour,
            ..WaterDepth::default()
        };
        WaterUniforms::new(
            &params(),
            WaterTechnique::Default,
            WaterReflection::Flat,
            WaterDistortion::default(),
            WaterSurfaceLook {
                source: WaterReflectionSource::Planar,
                composite: WaterReflectionComposite::Surface,
                highlights: WaterHighlights::Off,
            },
            depth,
            &tuning,
            0.0,
        )
        .rows()[9]
            .1
    };
    assert_eq!(row(true), [0.15, 0.3, 0.35, 1.0]);
    assert_eq!(row(false), [0.15, 0.3, 0.35, 0.0]);
}

#[test]
fn each_layers_translation_cancels_to_pure_u_under_its_own_rotation() {
    let motion = water_motion(&params(), 10.0);
    let speed = 0.015 + 0.0;
    for (rotation, offset) in [
        (motion.rotation1, motion.bump1),
        (motion.rotation2, motion.bump2),
        (motion.rotation1, motion.wave1),
        (motion.rotation2, motion.wave2),
    ] {
        let u = offset[0] * rotation[0] + offset[1] * rotation[1];
        let v = offset[1] * rotation[0] - offset[0] * rotation[1];
        assert!((u - speed * 10.0).abs() < 1e-6, "u was {u}");
        assert!(v.abs() < 1e-6, "v was {v}");
    }
}

#[test]
fn the_two_layers_cross_at_wave_angle_about_the_wind_angle() {
    let p = params();
    let motion = water_motion(&p, 1.0);
    let dot = motion.rotation1[0] * motion.rotation2[0] + motion.rotation1[1] * motion.rotation2[1];
    let (_, cos_wave) = sin_cos(p.wave_angle);
    assert!(
        (dot - cos_wave).abs() < 1e-5,
        "the layers are {dot} apart, not cos(WaveAngle)"
    );
    let sum = [
        motion.rotation1[0] + motion.rotation2[0],
        motion.rotation1[1] + motion.rotation2[1],
    ];
    let (sin_wind, cos_wind) = sin_cos(p.wind_angle);
    let cross = sum[0] * sin_wind - sum[1] * cos_wind;
    assert!(cross.abs() < 1e-5, "the bisector is off by {cross}");
}

#[test]
fn an_out_of_range_wind_angle_gives_the_same_directions_as_its_wrapped_form() {
    let mut wide = params();
    wide.wind_angle = 8.73;
    let mut narrow = params();
    narrow.wind_angle = 8.73 - std::f32::consts::TAU;
    let a = water_motion(&wide, 3.0);
    let b = water_motion(&narrow, 3.0);
    for (x, y) in a.rotation1.iter().zip(b.rotation1.iter()) {
        assert!((x - y).abs() < 1e-5);
    }
}

#[test]
fn a_zero_speed_body_never_translates() {
    let mut still = params();
    still.wind_speed = 0.0;
    for seconds in [0.0, 1.0, 600.0, 1e6] {
        let motion = water_motion(&still, seconds);
        assert_eq!(motion.bump1, [0.0, 0.0]);
        assert_eq!(motion.wave2, [0.0, 0.0]);
    }
}

#[test]
fn a_non_finite_time_or_angle_yields_a_still_unrotated_surface() {
    let motion = water_motion(&params(), f32::INFINITY);
    assert!(motion.bump1.iter().all(|v| v.is_finite()));
    let mut broken = params();
    broken.wind_angle = f32::NAN;
    broken.wave_angle = f32::NAN;
    let motion = water_motion(&broken, 1.0);
    assert!(motion.rotation1.iter().all(|v| v.is_finite()));
}

#[test]
fn the_uniform_block_is_the_size_the_shader_declares() {
    let bytes = WaterUniforms::new(
        &params(),
        WaterTechnique::Default,
        WaterReflection::Off,
        WaterDistortion::default(),
        WaterSurfaceLook::default(),
        WaterDepth::default(),
        &crate::water_tuning::WaterTuning::default(),
        0.0,
    )
    .to_bytes(
        &crate::wave_surface::WaveSurface::default(),
        &crate::water_tuning::WaterTuning::default(),
        0.0,
    );
    assert_eq!(bytes.len(), WATER_UNIFORM_SIZE as usize);
}

#[test]
fn bump_scale_multiplies_the_bump_uv_scale_and_nothing_else() {
    let block = WaterUniforms::new(
        &params(),
        WaterTechnique::Default,
        WaterReflection::Off,
        WaterDistortion::default(),
        WaterSurfaceLook::default(),
        WaterDepth::default(),
        &crate::water_tuning::WaterTuning::default(),
        0.0,
    );
    assert_eq!(block.uv_scales[0], 1.2 * 2.0);
    assert_eq!(block.uv_scales[2], 7.0);
}

#[test]
fn the_low_tier_carries_a_tenth_strength_foam_and_the_default_a_doubled_one() {
    let low = WaterUniforms::new(
        &params(),
        WaterTechnique::Low,
        WaterReflection::Off,
        WaterDistortion::default(),
        WaterSurfaceLook::default(),
        WaterDepth::default(),
        &crate::water_tuning::WaterTuning::default(),
        0.0,
    );
    let default = WaterUniforms::new(
        &params(),
        WaterTechnique::Default,
        WaterReflection::Off,
        WaterDistortion::default(),
        WaterSurfaceLook::default(),
        WaterDepth::default(),
        &crate::water_tuning::WaterTuning::default(),
        0.0,
    );
    assert_eq!(low.terms[1], 0.1);
    assert_eq!(default.terms[1], 2.0);
}

fn block(params: &WaterShaderParams, tuning: &crate::water_tuning::WaterTuning) -> WaterUniforms {
    WaterUniforms::new(
        params,
        WaterTechnique::Default,
        WaterReflection::Off,
        WaterDistortion::default(),
        WaterSurfaceLook::default(),
        WaterDepth::default(),
        tuning,
        0.0,
    )
}

fn declared_rows(source: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in source.lines() {
        if line.starts_with("struct Water {") {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        if line.starts_with('}') {
            break;
        }
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_suffix(": vec4<f32>,") {
            out.push(name);
        }
    }
    out
}

#[test]
fn every_uniform_row_is_written_in_the_order_the_shader_declares_it() {
    let declared = declared_rows(WATER_SHADER_WGSL);
    let written: Vec<&str> = block(&params(), &crate::water_tuning::WaterTuning::default())
        .rows()
        .iter()
        .map(|(name, _)| *name)
        .collect();
    assert!(
        declared.len() > written.len(),
        "the shader declares the wave tail after these rows"
    );
    assert_eq!(
        declared[..written.len()],
        written[..],
        "water_shader.wgsl's `struct Water` and `WaterUniforms::rows` disagree"
    );
    assert_eq!(
        &declared[written.len()..],
        &["wave_terms", "wave_curve"],
        "the wave tail is the only thing allowed after the block's own rows"
    );
}

#[test]
fn the_bodys_own_shadow_lightness_is_what_the_block_carries() {
    let mut params = params();
    params.shadow_lightness = Some(0.5);
    let tuning = crate::water_tuning::WaterTuning::default();
    assert_ne!(
        tuning.shadow_lightness, 0.5,
        "the scene's default must differ, or this test cannot tell them apart"
    );
    assert_eq!(block(&params, &tuning).shadow[0], 0.5);
}

#[test]
fn a_body_that_authors_none_takes_the_scenes_shadow_lightness() {
    let mut params = params();
    params.shadow_lightness = None;
    let tuning = crate::water_tuning::WaterTuning::default();
    assert_eq!(block(&params, &tuning).shadow[0], tuning.shadow_lightness);
}

#[test]
fn a_shadow_lightness_no_artist_would_author_is_clamped_or_falls_back() {
    let tuning = crate::water_tuning::WaterTuning::default();
    for (authored, want) in [
        (Some(-2.0), 0.0),
        (Some(9.0), 1.0),
        (Some(f32::NAN), tuning.shadow_lightness),
        (Some(f32::INFINITY), tuning.shadow_lightness),
    ] {
        let mut params = params();
        params.shadow_lightness = authored;
        assert_eq!(
            block(&params, &tuning).shadow[0],
            want,
            "authored {authored:?}"
        );
    }
}

#[test]
fn the_water_shader_asks_for_the_shadow() {
    assert!(
        WATER_SHADER_WGSL.contains("mint_shadow_visibility(in.world_pos)"),
        "water_shader.wgsl must sample the shadow map"
    );
    assert!(
        WATER_SHADER_WGSL.contains("water.shadow.x"),
        "and weight it by the body's own ShadowLightness, not the scene's"
    );
}
