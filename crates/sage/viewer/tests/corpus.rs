#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use mint_image::dds::Dds;
use mint_image::{jpeg, tga};
use mint_sage_asset::{sample_pose, Resolver};
use mint_sage_model::Error as ModelError;
use mint_sage_model::{convert, load_animation, load_model};
use mint_sage_viewer::mount_install;
use mint_sage_w3d::Model;
use mint_vfs::Vfs;

fn corpus() -> Vec<PathBuf> {
    let Some(value) = std::env::var_os("MINT_CORPUS") else {
        return Vec::new();
    };
    value
        .to_string_lossy()
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn mount_all(roots: &[PathBuf]) -> Vfs {
    let mut vfs = Vfs::new();
    for root in roots {
        let summary = mount_install(&mut vfs, root)
            .expect("each MINT_CORPUS entry should be an install root");
        assert!(
            summary.archives > 0,
            "{} has no .big archives; is it an install root?",
            root.display()
        );
    }
    vfs
}

fn is_not_an_image(path: &str, bytes: &[u8]) -> bool {
    if path.ends_with(".dds") {
        return bytes.get(0..4) != Some(b"DDS ");
    }
    if is_jpeg_path(path) {
        return bytes.get(0..2) != Some(&[0xFF, 0xD8]);
    }
    !matches!(bytes.get(1), Some(0 | 1))
}

fn is_jpeg_path(path: &str) -> bool {
    path.ends_with(".jpg") || path.ends_with(".jpeg")
}

fn jpeg_frame_size(bytes: &[u8]) -> Option<(u16, u16)> {
    if bytes.get(0..2)? != [0xFF, 0xD8] {
        return None;
    }
    let mut at = 2usize;
    loop {
        while bytes.get(at) == Some(&0xFF) && bytes.get(at + 1) == Some(&0xFF) {
            at += 1;
        }
        if *bytes.get(at)? != 0xFF {
            return None;
        }
        let marker = *bytes.get(at + 1)?;
        match marker {
            0x01 | 0xD0..=0xD7 => {
                at += 2;
                continue;
            }
            0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => {
                let height = u16::from_be_bytes([*bytes.get(at + 5)?, *bytes.get(at + 6)?]);
                let width = u16::from_be_bytes([*bytes.get(at + 7)?, *bytes.get(at + 8)?]);
                return Some((width, height));
            }
            0xDA | 0xD9 => return None,
            _ => {
                let length = usize::from(u16::from_be_bytes([
                    *bytes.get(at + 2)?,
                    *bytes.get(at + 3)?,
                ]));
                if length < 2 {
                    return None;
                }
                at = at.checked_add(2)?.checked_add(length)?;
            }
        }
    }
}

fn preview(bytes: &[u8]) -> String {
    let hex: Vec<String> = bytes
        .iter()
        .take(12)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let text: String = bytes
        .iter()
        .take(12)
        .map(|byte| {
            let c = char::from(*byte);
            if c.is_ascii_graphic() {
                c
            } else {
                '.'
            }
        })
        .collect();
    format!("{} |{text}|", hex.join(" "))
}

#[derive(Default)]
struct Totals {
    seen: usize,
    ok: usize,
    bytes: u64,
    failures: Vec<String>,
    by_kind: BTreeMap<&'static str, usize>,
}

impl Totals {
    fn note(&mut self, kind: &'static str) {
        *self.by_kind.entry(kind).or_default() += 1;
    }

    fn fail(&mut self, message: String) {
        if self.failures.len() < 50 {
            self.failures.push(message);
        }
    }
}

#[test]
#[ignore = "needs a retail installation; set MINT_CORPUS"]
fn every_shipped_texture_decodes() {
    let roots = corpus();
    if roots.is_empty() {
        println!("SKIPPED: MINT_CORPUS is not set, so nothing was validated");
        return;
    }
    let vfs = mount_all(&roots);

    let mut not_images: Vec<String> = Vec::new();
    let mut dds = Totals::default();
    let mut tga_totals = Totals::default();
    let mut jpeg_totals = Totals::default();
    let mut linear_size_agreed = 0usize;
    let mut chain_fits = 0usize;
    let mut tga_pixels_agreed = 0usize;
    let mut jpeg_frame_agreed = 0usize;
    let mut jpeg_elapsed = std::time::Duration::ZERO;
    let mut jpeg_pixels = 0u64;

    let paths: Vec<String> = vfs
        .paths()
        .filter(|path| {
            matches!(
                path.extension(),
                Some("dds") | Some("tga") | Some("jpg") | Some("jpeg")
            )
        })
        .map(|path| path.as_str().to_owned())
        .collect();

    let started = Instant::now();
    for path in &paths {
        let is_dds = path.ends_with(".dds");
        let is_jpeg = is_jpeg_path(path);
        let totals = if is_dds {
            &mut dds
        } else if is_jpeg {
            &mut jpeg_totals
        } else {
            &mut tga_totals
        };
        totals.seen += 1;

        let bytes = match vfs.read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                totals.fail(format!("{path}: {error}"));
                continue;
            }
        };
        totals.bytes += bytes.len() as u64;

        if is_dds {
            match Dds::parse(&bytes) {
                Ok(image) => {
                    dds.ok += 1;
                    dds.note(match image.format() {
                        mint_image::dds::BlockFormat::Bc1 => "BC1",
                        mint_image::dds::BlockFormat::Bc3 => "BC3",
                    });

                    if u64::from(image.declared_linear_size())
                        == image.computed_linear_size() as u64
                    {
                        linear_size_agreed += 1;
                    }
                    let chain: usize = image
                        .levels()
                        .iter()
                        .map(|level| level.blocks().len())
                        .sum();
                    if chain <= bytes.len() {
                        chain_fits += 1;
                    } else {
                        dds.fail(format!(
                            "{path}: mip chain is {chain} bytes but the file is {}",
                            bytes.len()
                        ));
                    }
                }
                Err(error) => {
                    if is_not_an_image(path, &bytes) {
                        not_images.push(format!("{path}: {error}\n      {}", preview(&bytes)));
                    } else {
                        dds.fail(format!("{path}: {error}\n      {}", preview(&bytes)));
                    }
                }
            }
        } else if is_jpeg {
            let decode_started = Instant::now();
            let decoded = jpeg::decode(&bytes);
            jpeg_elapsed += decode_started.elapsed();
            match decoded {
                Ok(image) => {
                    jpeg_totals.ok += 1;
                    jpeg_pixels += u64::from(image.width()) * u64::from(image.height());
                    jpeg_totals.note(match image.width().max(image.height()) {
                        0..=255 => "<= 256",
                        256..=1023 => "257..1023",
                        _ => "1024+",
                    });
                    match jpeg_frame_size(&bytes) {
                        Some((width, height))
                            if u32::from(width) == image.width()
                                && u32::from(height) == image.height() =>
                        {
                            jpeg_frame_agreed += 1;
                        }
                        Some((width, height)) => jpeg_totals.fail(format!(
                            "{path}: frame header says {width}x{height}, decoder produced {}x{}",
                            image.width(),
                            image.height()
                        )),
                        None => jpeg_totals.fail(format!(
                            "{path}: decoded, but this test found no frame header in it"
                        )),
                    }
                }
                Err(error) => {
                    if is_not_an_image(path, &bytes) {
                        not_images.push(format!("{path}: {error}\n      {}", preview(&bytes)));
                    } else {
                        jpeg_totals.fail(format!("{path}: {error}\n      {}", preview(&bytes)));
                    }
                }
            }
        } else {
            match tga::decode(&bytes) {
                Ok(image) => {
                    tga_totals.ok += 1;
                    if let Ok(info) = tga::info(&bytes) {
                        tga_totals.note(match info.bits_per_pixel {
                            32 => "32-bit",
                            24 => "24-bit",
                            16 => "16-bit",
                            8 => "8-bit",
                            _ => "other depth",
                        });
                    }
                    let expected = u64::from(image.width()) * u64::from(image.height()) * 4;
                    if image.pixels().len() as u64 == expected {
                        tga_pixels_agreed += 1;
                    } else {
                        tga_totals.fail(format!(
                            "{path}: {}x{} decoded to {} bytes, not {expected}",
                            image.width(),
                            image.height(),
                            image.pixels().len()
                        ));
                    }
                }
                Err(error) => {
                    if is_not_an_image(path, &bytes) {
                        not_images.push(format!("{path}: {error}\n      {}", preview(&bytes)));
                    } else {
                        tga_totals.fail(format!("{path}: {error}\n      {}", preview(&bytes)));
                    }
                }
            }
        }
    }
    let elapsed = started.elapsed().as_secs_f64().max(1e-9);

    let megabytes = (dds.bytes + tga_totals.bytes + jpeg_totals.bytes) as f64 / (1024.0 * 1024.0);
    println!("== {} ==", describe(&roots));
    println!(
        "  {} of {} DDS decoded, {} of {} TGA decoded, {} of {} JPEG decoded",
        dds.ok, dds.seen, tga_totals.ok, tga_totals.seen, jpeg_totals.ok, jpeg_totals.seen
    );
    println!(
        "  {:.1} MiB in {:.2}s ({:.0} MiB/s)",
        megabytes,
        elapsed,
        megabytes / elapsed
    );
    println!("  DDS by format: {:?}", dds.by_kind);
    println!("  TGA by depth:  {:?}", tga_totals.by_kind);
    println!("  JPEG by size:  {:?}", jpeg_totals.by_kind);
    let jpeg_seconds = jpeg_elapsed.as_secs_f64().max(1e-9);
    println!(
        "  JPEG decode alone: {:.2}s for {:.1} MiB in and {:.1} Mpixel out ({:.0} MiB/s, {:.1} Mpixel/s)",
        jpeg_seconds,
        jpeg_totals.bytes as f64 / (1024.0 * 1024.0),
        jpeg_pixels as f64 / 1.0e6,
        (jpeg_totals.bytes as f64 / (1024.0 * 1024.0)) / jpeg_seconds,
        (jpeg_pixels as f64 / 1.0e6) / jpeg_seconds
    );
    println!("\n  cross-checks (fields the decoders never read while parsing):");
    println!(
        "    declared linear size == computed:  {linear_size_agreed} of {}",
        dds.ok
    );
    println!(
        "    mip chain fits the file:           {chain_fits} of {}",
        dds.ok
    );
    println!(
        "    decoded pixels == declared size:   {tga_pixels_agreed} of {}",
        tga_totals.ok
    );
    println!(
        "    JPEG frame header == decoded size: {jpeg_frame_agreed} of {}",
        jpeg_totals.ok
    );

    if !not_images.is_empty() {
        println!(
            "\n  {} file(s) with an image extension that are not images, refused by \
             name rather than decoded as garbage:",
            not_images.len()
        );
        for line in &not_images {
            println!("    {line}");
        }
    }
    for failure in dds
        .failures
        .iter()
        .chain(tga_totals.failures.iter())
        .chain(jpeg_totals.failures.iter())
    {
        println!("    FAILED {failure}");
    }

    assert_eq!(
        chain_fits, dds.ok,
        "a mip chain claiming more bytes than the file holds means the level \
         layout is misunderstood"
    );
    assert_eq!(
        tga_pixels_agreed, tga_totals.ok,
        "a decoded TGA whose buffer is not width * height * 4 means the row \
         walk is wrong"
    );
    assert_eq!(
        jpeg_frame_agreed, jpeg_totals.ok,
        "a JPEG whose frame header disagrees with the size the decoder \
         produced means the wrong frame was read, or that it was scaled"
    );
    assert!(
        dds.failures.is_empty()
            && tga_totals.failures.is_empty()
            && jpeg_totals.failures.is_empty(),
        "{} texture(s) that really are images failed to decode; \
         the first are listed above",
        dds.failures.len() + tga_totals.failures.len() + jpeg_totals.failures.len()
    );
}

#[test]
#[ignore = "needs a retail installation; set MINT_CORPUS"]
fn a_malformed_jpeg_returns_an_error_rather_than_taking_the_process_down() {
    let roots = corpus();
    if roots.is_empty() {
        println!("SKIPPED: MINT_CORPUS is not set, so nothing was validated");
        return;
    }
    let vfs = mount_all(&roots);

    let paths: Vec<String> = vfs
        .paths()
        .filter(|path| is_jpeg_path(path.as_str()))
        .map(|path| path.as_str().to_owned())
        .collect();
    let mut sampled: Vec<&String> = paths.iter().step_by(20).collect();
    sampled.sort();
    assert!(!sampled.is_empty(), "no JPEGs under {}", describe(&roots));

    let started = Instant::now();
    let mut cases = 0usize;
    let mut still_decoded = 0usize;
    for path in &sampled {
        let Ok(bytes) = vfs.read(path) else {
            continue;
        };
        for step in 0..64usize {
            let cut = bytes.len() * step / 64;
            if let Some(prefix) = bytes.get(..cut) {
                cases += 1;
                if jpeg::decode(prefix).is_ok() {
                    still_decoded += 1;
                }
            }
        }
        for step in 0..256usize {
            let index = bytes.len() * step / 256;
            let mut corrupt = bytes.clone();
            if let Some(byte) = corrupt.get_mut(index) {
                *byte ^= 0x80;
                cases += 1;
                if jpeg::decode(&corrupt).is_ok() {
                    still_decoded += 1;
                }
            }
        }
    }

    println!("== {} ==", describe(&roots));
    println!(
        "  {cases} malformed JPEGs built from {} of {} shipped files, in {:.1}s",
        sampled.len(),
        paths.len(),
        started.elapsed().as_secs_f64()
    );
    println!(
        "  {still_decoded} still decoded to something, {} were refused; \
         no case panicked, which is the whole assertion",
        cases - still_decoded
    );
}

fn describe(roots: &[PathBuf]) -> String {
    roots
        .iter()
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>()
        .join(" + ")
}

#[test]
#[ignore = "needs a retail installation; set MINT_CORPUS"]
fn every_model_converts_to_geometry_a_gpu_can_be_handed() {
    let roots = corpus();
    if roots.is_empty() {
        println!("SKIPPED: MINT_CORPUS is not set, so nothing was validated");
        return;
    }
    let vfs = mount_all(&roots);
    let resolver = Resolver::new();

    let models: Vec<String> = vfs
        .paths()
        .filter(|path| path.extension() == Some("w3d"))
        .map(|path| path.as_str().to_owned())
        .collect();

    let mut parsed = 0usize;
    let mut converted = 0usize;
    let mut parts = 0usize;
    let mut vertices = 0usize;
    let mut triangles = 0usize;
    let mut dropped_triangles = 0usize;
    let mut models_with_dropped = 0usize;
    let mut texture_names = 0usize;
    let mut texture_names_resolved = 0usize;
    let mut pick_proxies = 0usize;
    let mut normal_maps = 0usize;
    let mut normal_maps_resolved = 0usize;
    let mut parts_with_tangents = 0usize;
    let mut non_finite_uv_vertices = 0usize;
    let mut non_finite_uv: Vec<String> = Vec::new();
    let mut unresolved: Vec<String> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    let mut external_hierarchy_models = 0usize;
    let mut external_hierarchy_resolved = 0usize;
    let mut external_hierarchy_unresolved: Vec<String> = Vec::new();
    let mut no_geometry = 0usize;

    let started = Instant::now();
    for path in &models {
        let Ok(bytes) = vfs.read(path) else {
            continue;
        };
        let model = match Model::parse(&bytes) {
            Ok(model) => model,
            Err(error) => {
                if failures.len() < 20 {
                    failures.push(format!("{path}: {error}"));
                }
                continue;
            }
        };
        parsed += 1;

        let wants_external_hierarchy = model.external_hierarchy().map(str::to_owned);

        let logical_name = path.rsplit('/').next().unwrap_or(path.as_str());
        let loaded = match load_model(&vfs, &resolver, logical_name) {
            Ok(loaded) => loaded,
            Err(ModelError::NoGeometry { .. }) => {
                no_geometry += 1;
                continue;
            }
            Err(error) => {
                if failures.len() < 20 {
                    failures.push(format!("{path}: {error}"));
                }
                continue;
            }
        };
        converted += 1;

        if let Some(hierarchy_name) = &wants_external_hierarchy {
            external_hierarchy_models += 1;
            if loaded.skeleton.is_some() {
                external_hierarchy_resolved += 1;
            } else if external_hierarchy_unresolved.len() < 20 {
                external_hierarchy_unresolved.push(format!("{path} wants {hierarchy_name}"));
            }
        }

        let result = &loaded.model;

        for part in &result.parts {
            parts += 1;
            vertices += part.mesh.vertices.len();
            triangles += part.mesh.triangle_count();
            dropped_triangles += part.dropped_triangles;
            if part.pick_proxy {
                pick_proxies += 1;
            }
            if let Some(name) = &part.normal_map {
                normal_maps += 1;
                if resolver.resolve(&vfs, name).is_ok() {
                    normal_maps_resolved += 1;
                }
            }
            if part
                .mesh
                .vertices
                .iter()
                .any(|vertex| vertex.tangent[..3] != [0.0; 3])
            {
                parts_with_tangents += 1;
            }
            let bad = part
                .mesh
                .vertices
                .iter()
                .filter(|vertex| !vertex.uv.iter().all(|value| value.is_finite()))
                .count();
            if bad > 0 {
                non_finite_uv_vertices += bad;
                if non_finite_uv.len() < 10 {
                    non_finite_uv.push(format!("{path} :: {} ({bad} vertices)", part.name));
                }
            }

            assert_eq!(
                part.mesh.validate(),
                None,
                "{path} :: {} produced an index outside its own vertex array",
                part.name
            );

            if let Some(name) = &part.texture {
                texture_names += 1;
                if resolver.resolve(&vfs, name).is_ok() {
                    texture_names_resolved += 1;
                } else if unresolved.len() < 20 {
                    unresolved.push(format!("{path} :: {} wants {name}", part.name));
                }
            }
        }
        if result.parts.iter().any(|part| part.dropped_triangles > 0) {
            models_with_dropped += 1;
        }
    }
    let elapsed = started.elapsed().as_secs_f64().max(1e-9);

    println!("== {} ==", describe(&roots));
    println!(
        "  {parsed} of {} models parsed, {converted} converted ({no_geometry} of the parsed \
         files are animation/skeleton-only and have no geometry to convert)",
        models.len()
    );
    println!("  {parts} parts, {vertices} vertices, {triangles} triangles");
    println!("  {:.2}s, {:.0} models/s", elapsed, parsed as f64 / elapsed);
    println!("\n  cross-checks:");
    println!(
        "    texture names that resolve to a file: {texture_names_resolved} of {texture_names}"
    );
    println!(
        "    models with an HLOD naming an external hierarchy: {external_hierarchy_resolved} of \
         {external_hierarchy_models} resolved it"
    );
    for line in external_hierarchy_unresolved.iter().take(20) {
        println!("      UNRESOLVED SKELETON {line}");
    }
    println!("    picking proxies, converted but not drawn: {pick_proxies}");
    println!(
        "    parts naming a normal map: {normal_maps}, of which {normal_maps_resolved} resolve"
    );
    println!("    parts carrying a tangent basis: {parts_with_tangents}");
    println!(
        "    vertices whose texture coordinate is not finite in the file: {non_finite_uv_vertices}"
    );
    for line in non_finite_uv.iter().take(10) {
        println!("      NON-FINITE UV {line}");
    }
    println!(
        "    models with a triangle dropped for a bad index: {models_with_dropped} of {converted}"
    );
    println!("    triangles dropped in total: {dropped_triangles}");
    for line in unresolved.iter().take(20) {
        println!("    UNRESOLVED {line}");
    }
    for line in failures.iter().take(20) {
        println!("    PARSE FAILED {line}");
    }

    assert!(converted > 0, "no models converted at all");

    assert_eq!(
        dropped_triangles, 0,
        "{dropped_triangles} triangle(s) across {converted} models had an index \
         outside their own mesh's vertex array; that is either a parser defect \
         or genuinely corrupt geometry, and it needs to be told apart"
    );
}

#[test]
#[ignore = "needs a retail installation; set MINT_CORPUS"]
fn every_animation_binds_to_a_hierarchy_and_samples() {
    let roots = corpus();
    if roots.is_empty() {
        println!("SKIPPED: MINT_CORPUS is not set, so nothing was validated");
        return;
    }
    let vfs = mount_all(&roots);
    let resolver = Resolver::new();

    let paths: Vec<String> = vfs
        .paths()
        .filter(|path| path.extension() == Some("w3d"))
        .map(|path| path.as_str().to_owned())
        .collect();

    let mut parsed = 0usize;
    let mut animation_files = 0usize;
    let mut bound = 0usize;
    let mut posed = 0usize;
    let mut total_out_of_range = 0usize;
    let mut total_root_ignored = 0usize;
    let mut unbound: Vec<String> = Vec::new();
    let mut pose_mismatches: Vec<String> = Vec::new();

    let started = Instant::now();
    for path in &paths {
        let Ok(bytes) = vfs.read(path) else { continue };
        let Ok(model) = Model::parse(&bytes) else {
            continue;
        };
        parsed += 1;
        if model.animations.is_empty() {
            continue;
        }
        animation_files += 1;

        let name = path.rsplit('/').next().unwrap_or(path);
        match load_animation(&vfs, &resolver, name) {
            Ok(loaded) => {
                bound += 1;
                let sample = sample_pose(&loaded.hierarchy, &loaded.animation, 0.0);
                if sample.world.len() == loaded.hierarchy.len() {
                    posed += 1;
                } else if pose_mismatches.len() < 20 {
                    pose_mismatches.push(format!(
                        "{path}: sampled {} world transform(s) for a {}-pivot hierarchy",
                        sample.world.len(),
                        loaded.hierarchy.len()
                    ));
                }
                total_out_of_range += sample.out_of_range_channels;
                total_root_ignored += sample.root_channels_ignored;
            }
            Err(error) => {
                if unbound.len() < 20 {
                    unbound.push(format!("{path}: {error}"));
                }
            }
        }
    }
    let elapsed = started.elapsed().as_secs_f64().max(1e-9);

    println!("== {} ==", describe(&roots));
    println!("  {parsed} of {} .w3d files parsed", paths.len());
    println!("  {animation_files} contain at least one animation");
    println!("  {bound} of {animation_files} bound to a hierarchy through load_animation");
    println!("  {posed} of {bound} sampled exactly one world transform per pivot");
    println!(
        "  {total_out_of_range} channel(s) across every bound animation named an out-of-range \
         pivot and were dropped"
    );
    println!("  {total_root_ignored} channel(s) named pivot 0 (the object root) and were ignored");
    println!(
        "  {elapsed:.2}s, {:.0} animations/s",
        f64::from(u32::try_from(animation_files).unwrap_or(u32::MAX)) / elapsed
    );

    if !unbound.is_empty() {
        println!("\n  could not bind to a hierarchy (first 20):");
        for line in &unbound {
            println!("    {line}");
        }
    }
    for line in &pose_mismatches {
        println!("    POSE MISMATCH {line}");
    }

    assert!(animation_files > 0, "no animations were found at all");
    assert!(
        pose_mismatches.is_empty(),
        "a sampled pose did not cover every pivot of its own hierarchy - see above"
    );
}

fn direction(v: [f32; 3]) -> Option<[f32; 3]> {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if !length.is_finite() || length <= 1e-12 {
        return None;
    }
    Some([v[0] / length, v[1] / length, v[2] / length])
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[test]
#[ignore = "needs a retail installation; set MINT_CORPUS"]
fn the_shipped_tangent_arrays_are_the_basis_a_normal_map_needs() {
    let roots = corpus();
    if roots.is_empty() {
        println!("SKIPPED: MINT_CORPUS is not set, so nothing was validated");
        return;
    }
    let vfs = mount_all(&roots);
    let models: Vec<String> = vfs
        .paths()
        .filter(|path| path.extension() == Some("w3d"))
        .map(|path| path.as_str().to_owned())
        .collect();

    let mut meshes = 0usize;
    let mut with_tangents = 0usize;
    let mut with_normal_map = 0usize;
    let mut normal_map_without_tangents = 0usize;
    let mut pick_proxies = 0usize;
    let mut compared = 0usize;
    let mut tangent_agrees = 0usize;
    let mut bitangent_agrees = 0usize;
    let mut naive_agrees = 0usize;
    let mut unit_length = 0usize;
    let mut left_handed = 0usize;

    let five_degrees = 0.996_194_7f32;

    let started = Instant::now();
    for path in &models {
        let Ok(bytes) = vfs.read(path) else { continue };
        let Ok(model) = Model::parse(&bytes) else {
            continue;
        };
        for mesh in &model.meshes {
            meshes += 1;
            if mesh.is_pick_proxy() {
                pick_proxies += 1;
            }
            let has_map = mesh.normal_map_name().is_some();
            if has_map {
                with_normal_map += 1;
            }
            let tangents = mesh.uv_tangents();
            let bitangents = mesh.uv_bitangents();
            if tangents.is_empty() {
                if has_map {
                    normal_map_without_tangents += 1;
                }
                continue;
            }
            with_tangents += 1;

            let (Ok(positions), Ok(normals), Ok(tangents), Ok(bitangents)) = (
                mesh.positions.to_vec(),
                mesh.normals.to_vec(),
                tangents.to_vec(),
                bitangents.to_vec(),
            ) else {
                continue;
            };
            let Ok(texcoords) = mesh.texcoords().to_vec() else {
                continue;
            };
            if texcoords.len() != positions.len() || tangents.len() != positions.len() {
                continue;
            }

            let mut du = vec![[0.0f32; 3]; positions.len()];
            let mut dv = vec![[0.0f32; 3]; positions.len()];
            for triangle in mesh.triangles.iter() {
                let Ok(triangle) = triangle else { continue };
                let [a, b, c] = triangle.indices.map(|i| i as usize);
                let (Some(pa), Some(pb), Some(pc)) =
                    (positions.get(a), positions.get(b), positions.get(c))
                else {
                    continue;
                };
                let (Some(ta), Some(tb), Some(tc)) =
                    (texcoords.get(a), texcoords.get(b), texcoords.get(c))
                else {
                    continue;
                };
                let e1 = [pb.x - pa.x, pb.y - pa.y, pb.z - pa.z];
                let e2 = [pc.x - pa.x, pc.y - pa.y, pc.z - pa.z];
                let (du1, dv1) = (tb.u - ta.u, tb.v - ta.v);
                let (du2, dv2) = (tc.u - ta.u, tc.v - ta.v);
                let determinant = du1 * dv2 - du2 * dv1;
                if !determinant.is_finite() || determinant.abs() < 1e-12 {
                    continue;
                }
                let scale = 1.0 / determinant;
                for index in [a, b, c] {
                    let (Some(u_slot), Some(v_slot)) = (du.get_mut(index), dv.get_mut(index))
                    else {
                        continue;
                    };
                    for axis in 0..3 {
                        u_slot[axis] += (e1[axis] * dv2 - e2[axis] * dv1) * scale;
                        v_slot[axis] += (e2[axis] * du1 - e1[axis] * du2) * scale;
                    }
                }
            }

            for index in 0..positions.len() {
                let (Some(shipped_t), Some(shipped_b)) =
                    (tangents.get(index), bitangents.get(index))
                else {
                    continue;
                };
                let shipped_t = [shipped_t.x, shipped_t.y, shipped_t.z];
                let shipped_b = [shipped_b.x, shipped_b.y, shipped_b.z];
                let (Some(derived_u), Some(derived_v)) = (
                    du.get(index).copied().and_then(direction),
                    dv.get(index).copied().and_then(direction),
                ) else {
                    continue;
                };
                let (Some(unit_t), Some(unit_b)) = (direction(shipped_t), direction(shipped_b))
                else {
                    continue;
                };
                compared += 1;
                if (dot3(shipped_t, shipped_t).sqrt() - 1.0).abs() < 1e-3
                    && (dot3(shipped_b, shipped_b).sqrt() - 1.0).abs() < 1e-3
                {
                    unit_length += 1;
                }

                if dot3(derived_u, unit_t) > five_degrees {
                    tangent_agrees += 1;
                }
                if -dot3(derived_v, unit_b) > five_degrees {
                    bitangent_agrees += 1;
                }
                if dot3(derived_u, unit_b) > five_degrees {
                    naive_agrees += 1;
                }

                if let Some(normal) = normals.get(index).and_then(|n| direction([n.x, n.y, n.z])) {
                    let cross = [
                        normal[1] * unit_t[2] - normal[2] * unit_t[1],
                        normal[2] * unit_t[0] - normal[0] * unit_t[2],
                        normal[0] * unit_t[1] - normal[1] * unit_t[0],
                    ];
                    if dot3(cross, unit_b) < 0.0 {
                        left_handed += 1;
                    }
                }
            }
        }
    }
    let elapsed = started.elapsed().as_secs_f64().max(1e-9);
    let percent = |n: usize| 100.0 * n as f64 / compared.max(1) as f64;

    println!("== {} ==", describe(&roots));
    println!("  {meshes} meshes, {with_tangents} carrying a tangent basis");
    println!("  {with_normal_map} declare a NormalMap, {normal_map_without_tangents} of those ship no tangents");
    println!("  {pick_proxies} picking proxies");
    println!("  {compared} vertices compared in {elapsed:.2}s");
    println!("\n  cross-checks, against a basis derived from the geometry:");
    println!(
        "    uv_tangents (chunk BITANGENTS) == dP/du within 5 deg:    {tangent_agrees} ({:.3}%)",
        percent(tangent_agrees)
    );
    println!(
        "    uv_bitangents (chunk TANGENTS) == -dP/dv within 5 deg:   {bitangent_agrees} ({:.3}%)",
        percent(bitangent_agrees)
    );
    println!(
        "    trusting the chunk names instead, TANGENTS == dP/du:     {naive_agrees} ({:.3}%)",
        percent(naive_agrees)
    );
    println!(
        "    both arrays unit length:           {unit_length} ({:.3}%)",
        percent(unit_length)
    );
    println!(
        "    left-handed vertices:              {left_handed} ({:.3}%)",
        percent(left_handed)
    );

    assert!(compared > 0, "no tangent arrays were compared at all");

    assert!(
        percent(tangent_agrees) > 95.0,
        "what uv_tangents returns stopped looking like dP/du: {:.3}%",
        percent(tangent_agrees)
    );
    assert!(
        percent(bitangent_agrees) > 95.0,
        "what uv_bitangents returns stopped looking like -dP/dv: {:.3}%",
        percent(bitangent_agrees)
    );
    assert!(
        percent(naive_agrees) < 10.0,
        "reading the chunks at face value should not work: {:.3}%",
        percent(naive_agrees)
    );
    assert!(
        percent(left_handed) > 10.0 && percent(left_handed) < 90.0,
        "handedness is not mixed after all: {:.3}%",
        percent(left_handed)
    );
}

#[test]
fn the_corpus_harness_is_wired_up_even_when_no_corpus_is_present() {
    let vfs = Vfs::new();
    assert!(mint_sage_model::find_models(&vfs, "anything").is_empty());

    let model = Model::parse(&[]).expect("no chunks is an empty model");
    let converted = convert::convert_model(&model, None).expect("an empty model converts");
    assert!(converted.parts.is_empty());
    assert!(converted.bounds.is_empty());

    if corpus().is_empty() {
        println!(
            "note: MINT_CORPUS is not set, so the two corpus tests in this file \
             were not run and nothing was validated against real data"
        );
    }
}

#[test]
fn a_missing_installation_is_an_error_rather_than_an_empty_success() {
    let mut vfs = Vfs::new();
    assert!(mount_install(&mut vfs, Path::new("definitely-not-an-install")).is_err());
}
