use super::camera::*;
use super::cli::*;
use super::lighting::*;
use super::reflection::*;
use super::*;

pub(crate) const SUPERSAMPLE: u32 = 4;

pub(crate) const ALIAS_STEP: f32 = 0.000_5;

pub(crate) fn alias_check(
    terrain: &Terrain,
    loaded: &mint_sage_terrain::ClassTextures,
    options: &Options,
    focus: Option<Vec3>,
    frames: u32,
    rig: &TerrainRig,
) -> Result<(), String> {
    let (width, height) = (1280u32, 720u32);
    let (big_width, big_height) = (
        width.saturating_mul(SUPERSAMPLE),
        height.saturating_mul(SUPERSAMPLE),
    );

    let single = loaded_without_mips(loaded);
    let mut test = Renderer::offscreen(width, height).map_err(|e| e.to_string())?;
    test.set_anisotropy(options.aniso);
    let test_instances = upload_all(&mut test, terrain, loaded, options.blend)?;

    let mut reference = Renderer::offscreen(big_width, big_height).map_err(|e| e.to_string())?;
    reference.set_anisotropy(1);
    let reference_instances = upload_all(&mut reference, terrain, &single, options.blend)?;
    drop(single);

    let (test_width, test_height) = test.size();
    let aspect = test_width as f32 / (test_height.max(1) as f32);
    let mut camera = frame_terrain_camera(terrain, aspect, options.pitch, options.zoom);
    camera.yaw = options.yaw;
    if let Some(target) = focus {
        camera.target = target;
    }
    let step = camera.distance * ALIAS_STEP;

    println!();
    println!(
        "aliasing over {frames} frame(s) at {width}x{height}, reference {big_width}x{big_height} at one level, {} instance(s), anisotropy {}x",
        test_instances.len(),
        test.anisotropy()
    );

    let (mut error_sum, mut test_motion, mut reference_motion) = (0.0f64, 0.0f64, 0.0f64);
    let (mut previous_test, mut previous_reference): (Vec<u8>, Vec<u8>) = (Vec::new(), Vec::new());
    let mut previous_error: Vec<i16> = Vec::new();
    let mut shimmer_sum = 0.0f64;
    let mut motion_frames = 0u32;
    for frame in 0..frames {
        if frame > 0 {
            camera.target = Vec3::new(camera.target.x + step, camera.target.y, camera.target.z);
        }
        let scene = terrain_scene(
            camera.camera(),
            &test_instances,
            rig,
            1.0,
            LightMask::default(),
            None,
        );
        test.render(&scene).map_err(|e| e.to_string())?;
        let test_pixels = test.read_pixels().map_err(|e| e.to_string())?;

        let scene = terrain_scene(
            camera.camera(),
            &reference_instances,
            rig,
            1.0,
            LightMask::default(),
            None,
        );
        reference.render(&scene).map_err(|e| e.to_string())?;
        let big = reference.read_pixels().map_err(|e| e.to_string())?;
        let reference_pixels = box_downsample(&big, big_width, big_height, SUPERSAMPLE);

        error_sum += mean_abs_difference(&test_pixels, &reference_pixels);
        let error: Vec<i16> = test_pixels
            .iter()
            .zip(reference_pixels.iter())
            .map(|(a, b)| i16::from(*a).saturating_sub(i16::from(*b)))
            .collect();
        if !previous_test.is_empty() {
            test_motion += mean_abs_difference(&test_pixels, &previous_test);
            reference_motion += mean_abs_difference(&reference_pixels, &previous_reference);
            shimmer_sum += mean_abs_change(&error, &previous_error);
            motion_frames = motion_frames.saturating_add(1);
        }
        previous_test = test_pixels;
        previous_reference = reference_pixels;
        previous_error = error;
    }

    let per = |sum: f64, n: u32| if n == 0 { 0.0 } else { sum / f64::from(n) };
    let test_moved = per(test_motion, motion_frames);
    let reference_moved = per(reference_motion, motion_frames);
    println!(
        "  error against the reference   {:.4}",
        per(error_sum, frames)
    );
    println!(
        "  shimmer: change in the error  {:.4}",
        per(shimmer_sum, motion_frames)
    );
    println!("  frame-to-frame, this config   {test_moved:.4}   (context, not the measurement)");
    println!("  frame-to-frame, reference     {reference_moved:.4}");
    Ok(())
}

pub(crate) fn loaded_without_mips(
    loaded: &mint_sage_terrain::ClassTextures,
) -> mint_sage_terrain::ClassTextures {
    let mut out = mint_sage_terrain::ClassTextures {
        textures: loaded.textures.clone(),
        failures: Vec::new(),
        alpha_discarded: Vec::new(),
    };
    for texture in out.textures.iter_mut().flatten() {
        texture.levels.truncate(1);
    }
    out
}

pub(crate) fn upload_all(
    renderer: &mut Renderer,
    terrain: &Terrain,
    loaded: &mint_sage_terrain::ClassTextures,
    blend: bool,
) -> Result<Vec<Instance>, String> {
    let textures = upload_class_textures(renderer, loaded).map_err(|e| e.to_string())?;
    let (_handles, instances) = upload_terrain(
        renderer,
        terrain,
        &textures,
        blend,
        &TerrainShading::default(),
    )
    .map_err(|e| e.to_string())?;
    Ok(instances)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) fn box_downsample(pixels: &[u8], width: u32, height: u32, factor: u32) -> Vec<u8> {
    let factor = factor.max(1);
    let (out_width, out_height) = (width / factor, height / factor);
    let mut out = Vec::with_capacity((out_width as usize) * (out_height as usize) * 4);
    let taps = f64::from(factor) * f64::from(factor);
    for y in 0..out_height {
        for x in 0..out_width {
            let mut sums = [0.0f64; 4];
            for sy in 0..factor {
                for sx in 0..factor {
                    let px = x * factor + sx;
                    let py = y * factor + sy;
                    let index = ((py as usize) * (width as usize) + (px as usize)) * 4;
                    let Some(texel) = pixels.get(index..index + 4) else {
                        continue;
                    };
                    for (sum, value) in sums.iter_mut().zip(texel) {
                        *sum += f64::from(*value);
                    }
                }
            }
            for sum in sums {
                out.push((sum / taps).round().clamp(0.0, 255.0) as u8);
            }
        }
    }
    out
}

pub(crate) fn mean_abs_change(a: &[i16], b: &[i16]) -> f64 {
    if a.is_empty() || a.len() != b.len() {
        return f64::NAN;
    }
    let total: i64 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| i64::from((*x).saturating_sub(*y).abs()))
        .sum();
    total as f64 / a.len() as f64
}

pub(crate) fn mean_abs_difference(a: &[u8], b: &[u8]) -> f64 {
    if a.is_empty() || a.len() != b.len() {
        return f64::NAN;
    }
    let total: u64 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| u64::from(x.abs_diff(*y)))
        .sum();
    total as f64 / a.len() as f64
}
