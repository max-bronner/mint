use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mint_sage_big::BigArchive;
use mint_sage_w3d::Model;

fn mapper_name(code: u32) -> &'static str {
    match code {
        0x00 => "UV",
        0x01 => "ENVIRONMENT",
        0x02 => "CHEAP_ENVIRONMENT",
        0x03 => "SCREEN",
        0x04 => "LINEAR_OFFSET",
        0x05 => "SILHOUETTE",
        0x06 => "SCALE",
        0x07 => "GRID",
        0x08 => "ROTATE",
        0x09 => "SINE_LINEAR_OFFSET",
        0x0A => "STEP_LINEAR_OFFSET",
        0x0B => "ZIGZAG_LINEAR_OFFSET",
        0x0C => "WS_CLASSIC_ENV",
        0x0D => "WS_ENVIRONMENT",
        0x0E => "GRID_CLASSIC_ENV",
        0x0F => "GRID_ENVIRONMENT",
        0x10 => "RANDOM",
        0x11 => "EDGE",
        0x12 => "BUMPENV",
        0x13 => "GRID_WS_CLASSIC_ENV",
        0x14 => "GRID_WS_ENVIRONMENT",
        _ => "UNKNOWN",
    }
}

fn main() {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: uv_mapper_scan <install dir> [<install dir> ...]");
        return;
    }

    let mut stage0: BTreeMap<u32, usize> = BTreeMap::new();
    let mut stage1: BTreeMap<u32, usize> = BTreeMap::new();
    let mut non_zero_emissive = 0usize;
    let mut non_unit_opacity = 0usize;
    let mut total_infos = 0usize;
    let mut meshes_with_nonuv_mapper = 0usize;
    let mut total_meshes = 0usize;
    let mut non_unit_opacity_by_blend: BTreeMap<(u8, u8), usize> = BTreeMap::new();
    let mut non_white_ambient = 0usize;
    let mut non_white_diffuse = 0usize;
    let mut zero_diffuse = 0usize;
    let mut zero_diffuse_and_emissive = 0usize;
    const ENV_FAMILY: [u32; 4] = [0x01, 0x02, 0x0C, 0x0D];
    let mut env_family_meshes = 0usize;
    let mut env_family_meshes_no_uv = 0usize;
    let mut zero_material_color_with_dcg = 0usize;
    let mut zero_uv_meshes = 0usize;
    let mut zero_uv_mapper_codes: BTreeMap<(u32, u32), usize> = BTreeMap::new();

    for root in &roots {
        for path in files_with_extension(root, "big") {
            let Ok(archive) = BigArchive::open(&path) else {
                continue;
            };
            for entry in archive.entries() {
                if !entry.name.to_ascii_lowercase().ends_with(".w3d") {
                    continue;
                }
                let Ok(bytes) = archive.read_entry(entry) else {
                    continue;
                };
                let Ok(model) = Model::parse(&bytes) else {
                    continue;
                };
                for mesh in &model.meshes {
                    total_meshes += 1;
                    let mut mesh_has_nonuv = false;
                    for vm in &mesh.vertex_materials {
                        let Some(info) = &vm.info else { continue };
                        total_infos += 1;
                        let s0 = (info.attributes & 0x00FF_0000) >> 16;
                        let s1 = (info.attributes & 0x0000_FF00) >> 8;
                        *stage0.entry(s0).or_default() += 1;
                        *stage1.entry(s1).or_default() += 1;
                        if s0 != 0 || s1 != 0 {
                            mesh_has_nonuv = true;
                        }
                        if info.emissive.r != 0 || info.emissive.g != 0 || info.emissive.b != 0 {
                            non_zero_emissive += 1;
                        }
                        let is_white =
                            |c: mint_sage_w3d::Rgb| c.r == 255 && c.g == 255 && c.b == 255;
                        if !is_white(info.ambient) {
                            non_white_ambient += 1;
                        }
                        if !is_white(info.diffuse) {
                            non_white_diffuse += 1;
                        }
                        if info.diffuse.r == 0 && info.diffuse.g == 0 && info.diffuse.b == 0 {
                            zero_diffuse += 1;
                            if info.emissive.r == 0 && info.emissive.g == 0 && info.emissive.b == 0
                            {
                                zero_diffuse_and_emissive += 1;
                                if info.ambient.r == 0
                                    && info.ambient.g == 0
                                    && info.ambient.b == 0
                                    && !mesh.diffuse_colors().is_empty()
                                {
                                    zero_material_color_with_dcg += 1;
                                }
                                if zero_diffuse_and_emissive <= 15 {
                                    println!(
                                        "ZERO-DIFFUSE-ZERO-EMISSIVE: {} :: mesh {:?} material \
                                         {:?} texture={:?} pick_proxy={} shaders={:?}",
                                        entry.name,
                                        mesh.name(),
                                        vm.name,
                                        mesh.texture_name(),
                                        mesh.is_pick_proxy(),
                                        mesh.shaders
                                            .iter()
                                            .flatten()
                                            .map(|s| (s.src_blend, s.dest_blend))
                                            .collect::<Vec<_>>()
                                    );
                                }
                            }
                        }
                        if (info.opacity - 1.0).abs() > 0.001 {
                            non_unit_opacity += 1;
                            let blend = mesh
                                .shaders
                                .iter()
                                .flatten()
                                .next()
                                .map_or((0, 0), |s| (s.src_blend, s.dest_blend));
                            *non_unit_opacity_by_blend.entry(blend).or_default() += 1;
                        }
                        if info.opacity < 0.01
                            && (info.emissive.r != 0
                                || info.emissive.g != 0
                                || info.emissive.b != 0)
                        {
                            println!(
                                "ZERO-OPACITY+EMISSIVE: {} :: mesh {:?} material {:?} \
                                 opacity={} emissive={:?} diffuse={:?} attr=0x{:08X} shaders={:?}",
                                entry.name,
                                mesh.name(),
                                vm.name,
                                info.opacity,
                                info.emissive,
                                info.diffuse,
                                info.attributes,
                                mesh.shaders
                                    .iter()
                                    .flatten()
                                    .map(|s| (s.src_blend, s.dest_blend))
                                    .collect::<Vec<_>>()
                            );
                        }
                    }
                    if mesh_has_nonuv {
                        meshes_with_nonuv_mapper += 1;
                    }
                    let mesh_env_family = mesh.vertex_materials.iter().any(|vm| {
                        vm.info.as_ref().is_some_and(|info| {
                            let s0 = (info.attributes & 0x00FF_0000) >> 16;
                            let s1 = (info.attributes & 0x0000_FF00) >> 8;
                            ENV_FAMILY.contains(&s0) || ENV_FAMILY.contains(&s1)
                        })
                    });
                    if mesh_env_family {
                        env_family_meshes += 1;
                        if mesh.texcoords().is_empty() {
                            env_family_meshes_no_uv += 1;
                        }
                    }
                    if mesh.texcoords().is_empty() {
                        zero_uv_meshes += 1;
                        let (s0, s1) = mesh
                            .vertex_materials
                            .iter()
                            .find_map(|vm| vm.info.as_ref())
                            .map_or((0, 0), |info| {
                                (
                                    (info.attributes & 0x00FF_0000) >> 16,
                                    (info.attributes & 0x0000_FF00) >> 8,
                                )
                            });
                        *zero_uv_mapper_codes.entry((s0, s1)).or_default() += 1;
                    }

                    let file_lower = entry.name.to_ascii_lowercase();
                    let mesh_lower = mesh.name().to_ascii_lowercase();
                    if file_lower.contains("sfe_armydecal")
                        || file_lower.contains("sfe_menucredits")
                        || mesh_lower.contains("green")
                    {
                        for vm in &mesh.vertex_materials {
                            let Some(info) = &vm.info else { continue };
                            let s0 = (info.attributes & 0x00FF_0000) >> 16;
                            let s1 = (info.attributes & 0x0000_FF00) >> 8;
                            println!(
                                "{} :: mesh {:?} material {:?} attr=0x{:08X} stage0={} ({}) stage1={} ({})",
                                entry.name,
                                mesh.name(),
                                vm.name,
                                info.attributes,
                                s0,
                                mapper_name(s0),
                                s1,
                                mapper_name(s1),
                            );
                            if let Some(args0) = &vm.mapper_args0 {
                                println!("    args0: {args0:?}");
                            }
                            if let Some(args1) = &vm.mapper_args1 {
                                println!("    args1: {args1:?}");
                            }
                            println!(
                                "    ambient={:?} diffuse={:?} specular={:?} emissive={:?} opacity={} translucency={}",
                                info.ambient, info.diffuse, info.specular, info.emissive,
                                info.opacity, info.translucency
                            );
                        }
                        for material in &mesh.shader_materials {
                            println!(
                                "    SHADER_MATERIAL {:?} unknown={}",
                                material.shader_name, material.unknown
                            );
                            for property in &material.properties {
                                println!("      {:?} = {:?}", property.name, property.value);
                            }
                        }
                        for (pi, pass) in mesh.passes.iter().enumerate() {
                            println!(
                                "    pass {pi}: dcg entries={} first={:?}",
                                pass.diffuse_colors.len(),
                                pass.diffuse_colors.get(0)
                            );
                        }
                        for (si, shader) in mesh.shaders.iter().enumerate() {
                            if let Ok(shader) = shader {
                                println!(
                                    "    shader {si}: src_blend={} dest_blend={} alpha_test={} depth_mask={} pri_gradient={} sec_gradient={}",
                                    shader.src_blend, shader.dest_blend, shader.alpha_test, shader.depth_mask,
                                    shader.pri_gradient, shader.sec_gradient
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    println!("\n{total_meshes} meshes, {total_infos} VERTEX_MATERIAL_INFO chunks");
    println!(
        "{meshes_with_nonuv_mapper} of {total_meshes} meshes carry a vertex material with a \
         non-UV mapper on either stage"
    );
    println!("{non_zero_emissive} of {total_infos} VERTEX_MATERIAL_INFO have a non-zero emissive");
    println!("{non_white_ambient} of {total_infos} VERTEX_MATERIAL_INFO have a non-white ambient");
    println!("{non_white_diffuse} of {total_infos} VERTEX_MATERIAL_INFO have a non-white diffuse");
    println!("{zero_diffuse} of {total_infos} VERTEX_MATERIAL_INFO have a fully zero diffuse");
    println!(
        "{zero_diffuse_and_emissive} of those {zero_diffuse} ALSO have zero emissive (would go \
         fully black if diffuse multiplied the sampled texture with no other safeguard)"
    );
    println!(
        "{zero_material_color_with_dcg} of those {zero_diffuse_and_emissive} ALSO have zero \
         ambient and a non-empty DCG array - EA's Post_Load_Process substitutes DCG for the \
         missing colour here; mint does not, and renders these black - see this example's \
         module doc"
    );
    println!(
        "{non_unit_opacity} of {total_infos} VERTEX_MATERIAL_INFO have opacity != 1.0 (tolerance 0.001)"
    );
    println!("  by (src_blend, dest_blend) of the mesh's first SHADERS entry:");
    for ((src, dest), count) in &non_unit_opacity_by_blend {
        println!("    {count:>8}  src={src} dest={dest}");
    }
    println!(
        "{env_family_meshes} of {total_meshes} meshes carry an ENVIRONMENT/CHEAP_ENVIRONMENT/\
         WS_CLASSIC_ENV/WS_ENVIRONMENT mapper on either stage - the family mint now generates \
         UVs for"
    );
    println!(
        "{env_family_meshes_no_uv} of those {env_family_meshes} ship zero stored texture \
         coordinates at all - the case the task names as the one that must work"
    );
    println!(
        "\n{zero_uv_meshes} of {total_meshes} meshes have zero stored texture coordinates at \
         all; by (stage0, stage1) mapper code:"
    );
    for ((s0, s1), count) in &zero_uv_mapper_codes {
        println!(
            "  {count:>8}  stage0={s0} ({}) stage1={s1} ({})",
            mapper_name(*s0),
            mapper_name(*s1)
        );
    }

    println!("\nstage0 mapper distribution:");
    for (code, count) in &stage0 {
        println!("  {count:>8}  {code:>3} {}", mapper_name(*code));
    }
    println!("\nstage1 mapper distribution:");
    for (code, count) in &stage1 {
        println!("  {count:>8}  {code:>3} {}", mapper_name(*code));
    }
}

fn files_with_extension(root: &Path, want: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(listing) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in listing.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(kind) if kind.is_file() => {
                    let matches = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case(want));
                    if matches {
                        found.push(path);
                    }
                }
                _ => {}
            }
        }
    }
    found.sort();
    found
}
