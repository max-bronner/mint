use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_sage_w3d::{ChunkType, Model, ShaderMaterialValue};

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: scan <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    }

    let mut all = Totals::default();
    for root in &roots {
        let started = Instant::now();
        let mut totals = Totals::default();
        walk(root, &mut totals);
        totals.report(root, started.elapsed().as_secs_f64());
        all.merge(totals);
        println!();
    }

    if roots.len() > 1 {
        println!("== all installations ==");
        all.summary();
    }

    let cross_checks_held = all.count_disagreements.is_empty()
        && all.bad_parents.is_empty()
        && all.attribute_length_mismatches.is_empty()
        && all.weight_sum_anomalies.is_empty();
    if cross_checks_held {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[derive(Default)]
struct Totals {
    files_seen: usize,
    files_parsed: usize,
    bytes: u64,

    meshes: usize,
    vertices: usize,
    triangles: usize,
    skinned_meshes: usize,
    two_bone_vertices: usize,
    zero_weight_vertices: usize,
    hierarchies: usize,
    pivots: usize,
    hlods: usize,
    lod_arrays: usize,
    sub_objects: usize,
    animations: usize,
    undecoded_channels: u64,
    shader_materials: usize,
    meshes_with_only_shader_materials: usize,
    dest_blend_counts: BTreeMap<u8, usize>,
    src_blend_counts: BTreeMap<u8, usize>,
    meshes_with_non_opaque_shader: usize,
    texture_info_with_more_than_one_frame: usize,
    depth_mask_agrees_with_dest_blend: usize,
    depth_mask_disagrees_with_dest_blend: usize,

    failures: Vec<String>,
    count_disagreements: Vec<String>,
    bad_parents: Vec<String>,
    attribute_length_mismatches: Vec<String>,
    weight_sum_anomalies: Vec<String>,
    unordered_hierarchies: usize,
    unknown_property_tags: BTreeMap<u32, usize>,
    chunk_ids: BTreeMap<u32, u64>,
}

impl Totals {
    fn absorb(&mut self, name: &str, bytes: &[u8]) {
        self.files_seen += 1;
        self.bytes += bytes.len() as u64;

        let model = match Model::parse(bytes) {
            Ok(model) => model,
            Err(error) => {
                self.failures.push(format!("{name}: {error}"));
                return;
            }
        };
        self.files_parsed += 1;

        for (kind, count) in &model.chunk_counts {
            *self.chunk_ids.entry(kind.0).or_default() += u64::from(*count);
        }

        for mesh in &model.meshes {
            self.meshes += 1;
            self.vertices += mesh.vertex_count();
            self.triangles += mesh.triangle_count();
            if mesh.header.is_skin() {
                self.skinned_meshes += 1;
            }
            if !mesh.counts_agree() {
                self.count_disagreements.push(format!(
                    "{name} :: {}: header says {} verts / {} tris, arrays hold {} / {}",
                    mesh.name(),
                    mesh.header.num_vertices,
                    mesh.header.num_tris,
                    mesh.vertex_count(),
                    mesh.triangle_count(),
                ));
            }

            for influence in mesh.influences.iter().flatten() {
                let sum = u32::from(influence.weights[0]) + u32::from(influence.weights[1]);
                match sum {
                    0 => self.zero_weight_vertices += 1,
                    100 => {
                        if influence.weights[1] != 0 {
                            self.two_bone_vertices += 1;
                        }
                    }
                    other => self.weight_sum_anomalies.push(format!(
                        "{name} :: {}: bones {:?} weights {:?} sum to {other}",
                        mesh.name(),
                        influence.bones,
                        influence.weights,
                    )),
                }
            }

            for (label, len) in [
                ("TANGENTS", mesh.tangents.len()),
                ("BITANGENTS", mesh.bitangents.len()),
                ("VERTICES_2", mesh.positions2.len()),
                ("VERTEX_NORMALS_2", mesh.normals2.len()),
            ] {
                if len != 0 && len != mesh.vertex_count() {
                    self.attribute_length_mismatches.push(format!(
                        "{name} :: {}: {label} holds {len} entries for {} vertices",
                        mesh.name(),
                        mesh.vertex_count(),
                    ));
                }
            }

            self.shader_materials += mesh.shader_materials.len();
            if !mesh.shader_materials.is_empty() && mesh.textures.is_empty() {
                self.meshes_with_only_shader_materials += 1;
            }

            let mut mesh_non_opaque = false;
            for shader in mesh.shaders.iter().flatten() {
                *self.dest_blend_counts.entry(shader.dest_blend).or_default() += 1;
                *self.src_blend_counts.entry(shader.src_blend).or_default() += 1;
                if shader.dest_blend != 0 {
                    mesh_non_opaque = true;
                }
                let opaque_by_dest_blend = shader.dest_blend == 0;
                let writes_depth = shader.depth_mask != 0;
                if opaque_by_dest_blend == writes_depth {
                    self.depth_mask_agrees_with_dest_blend += 1;
                } else {
                    self.depth_mask_disagrees_with_dest_blend += 1;
                }
            }
            if mesh_non_opaque {
                self.meshes_with_non_opaque_shader += 1;
            }
            for texture in &mesh.textures {
                if texture.info.is_some_and(|info| info.frame_count > 1) {
                    self.texture_info_with_more_than_one_frame += 1;
                }
            }
            for material in &mesh.shader_materials {
                for property in &material.properties {
                    if let ShaderMaterialValue::Unknown { tag, .. } = property.value {
                        *self.unknown_property_tags.entry(tag).or_default() += 1;
                    }
                }
            }
        }

        if let Some(hierarchy) = &model.hierarchy {
            self.hierarchies += 1;
            self.pivots += hierarchy.len();
            let bad = hierarchy.unusable_parents();
            if !bad.is_empty() {
                self.bad_parents
                    .push(format!("{name}: pivots {bad:?} have unusable parents"));
            }
            if !hierarchy.parents_precede_children() {
                self.unordered_hierarchies += 1;
            }
        }

        for hlod in &model.hlods {
            self.hlods += 1;
            self.lod_arrays += hlod.lods.len();
            self.sub_objects += hlod.lods.iter().map(|l| l.sub_objects.len()).sum::<usize>();
        }

        for animation in &model.animations {
            self.animations += 1;
            self.undecoded_channels += u64::from(animation.channel_chunks);
        }
    }

    fn merge(&mut self, other: Totals) {
        self.files_seen += other.files_seen;
        self.files_parsed += other.files_parsed;
        self.bytes += other.bytes;
        self.meshes += other.meshes;
        self.vertices += other.vertices;
        self.triangles += other.triangles;
        self.skinned_meshes += other.skinned_meshes;
        self.two_bone_vertices += other.two_bone_vertices;
        self.zero_weight_vertices += other.zero_weight_vertices;
        self.hierarchies += other.hierarchies;
        self.pivots += other.pivots;
        self.hlods += other.hlods;
        self.lod_arrays += other.lod_arrays;
        self.sub_objects += other.sub_objects;
        self.animations += other.animations;
        self.undecoded_channels += other.undecoded_channels;
        self.shader_materials += other.shader_materials;
        self.meshes_with_only_shader_materials += other.meshes_with_only_shader_materials;
        self.meshes_with_non_opaque_shader += other.meshes_with_non_opaque_shader;
        self.texture_info_with_more_than_one_frame += other.texture_info_with_more_than_one_frame;
        self.depth_mask_agrees_with_dest_blend += other.depth_mask_agrees_with_dest_blend;
        self.depth_mask_disagrees_with_dest_blend += other.depth_mask_disagrees_with_dest_blend;
        for (value, n) in other.dest_blend_counts {
            *self.dest_blend_counts.entry(value).or_default() += n;
        }
        for (value, n) in other.src_blend_counts {
            *self.src_blend_counts.entry(value).or_default() += n;
        }
        self.unordered_hierarchies += other.unordered_hierarchies;
        self.failures.extend(other.failures);
        self.count_disagreements.extend(other.count_disagreements);
        self.bad_parents.extend(other.bad_parents);
        self.attribute_length_mismatches
            .extend(other.attribute_length_mismatches);
        self.weight_sum_anomalies.extend(other.weight_sum_anomalies);
        for (tag, n) in other.unknown_property_tags {
            *self.unknown_property_tags.entry(tag).or_default() += n;
        }
        for (id, n) in other.chunk_ids {
            *self.chunk_ids.entry(id).or_default() += n;
        }
    }

    fn report(&self, root: &Path, seconds: f64) {
        println!("== {} ==", root.display());
        self.summary();
        println!(
            "  {:.3}s, {:.0} files/s, {:.1} MiB/s",
            seconds,
            self.files_seen as f64 / seconds.max(1e-9),
            self.bytes as f64 / (1024.0 * 1024.0) / seconds.max(1e-9)
        );

        if !self.failures.is_empty() {
            println!("  {} failures:", self.failures.len());
            for failure in &self.failures {
                println!("    {failure}");
            }
        }
        for line in self
            .count_disagreements
            .iter()
            .chain(&self.bad_parents)
            .chain(&self.attribute_length_mismatches)
            .chain(&self.weight_sum_anomalies)
            .take(50)
        {
            println!("    CROSS-CHECK FAILED: {line}");
        }

        println!("  ten most common chunk ids:");
        let mut ranked: Vec<_> = self.chunk_ids.iter().collect();
        ranked.sort_by_key(|(id, n)| (std::cmp::Reverse(**n), **id));
        for (id, n) in ranked.into_iter().take(10) {
            let kind = ChunkType(*id);
            let name = kind.name().unwrap_or("(unnamed)");
            println!("    0x{id:08X} {name:<36} {n:>8}");
        }
    }

    fn summary(&self) {
        println!(
            "  {} of {} .w3d files parsed ({:.1} MiB)",
            self.files_parsed,
            self.files_seen,
            self.bytes as f64 / (1024.0 * 1024.0)
        );
        println!(
            "  {} meshes, {} vertices, {} triangles ({} skinned)",
            self.meshes, self.vertices, self.triangles, self.skinned_meshes
        );
        println!(
            "  {} skeletons ({} pivots), {} HLODs, {} LOD arrays, {} sub-objects",
            self.hierarchies, self.pivots, self.hlods, self.lod_arrays, self.sub_objects
        );
        println!(
            "  {} shader materials ({} meshes have no legacy TEXTURES at all)",
            self.shader_materials, self.meshes_with_only_shader_materials
        );
        println!(
            "  {} animations carrying {} channel chunks - walked past, not decoded",
            self.animations, self.undecoded_channels
        );
        println!("  cross-checks (each would fail if the format were misread):");
        println!(
            "    mesh header counts == array lengths:   {} of {} meshes",
            self.meshes.saturating_sub(self.count_disagreements.len()),
            self.meshes
        );
        println!(
            "    every pivot parent resolves to a root: {} of {} skeletons",
            self.hierarchies.saturating_sub(self.bad_parents.len()),
            self.hierarchies
        );
        println!(
            "    extension arrays are one Vec3/vertex:  {} mismatches",
            self.attribute_length_mismatches.len()
        );
        println!(
            "    influence weights sum to 100 or 0:     {} anomalies",
            self.weight_sum_anomalies.len()
        );
        println!(
            "  skinning: {} vertices use a second bone, {} store both weights as zero",
            self.two_bone_vertices, self.zero_weight_vertices
        );
        println!(
            "  {} meshes have at least one SHADERS entry with dest_blend != ZERO (non-opaque)",
            self.meshes_with_non_opaque_shader
        );
        println!(
            "  SHADERS dest_blend distribution (W3D_SHADER_DEST_BLEND_FUNC): {:?}",
            self.dest_blend_counts
        );
        println!(
            "  SHADERS src_blend distribution (W3D_SHADER_SRC_BLEND_FUNC): {:?}",
            self.src_blend_counts
        );
        println!(
            "  TEXTURE_INFO chunks declaring FrameCount > 1 (flipbook animation): {}",
            self.texture_info_with_more_than_one_frame
        );
        println!(
            "  depth_mask agrees with \"dest_blend == ZERO means opaque\": {} of {}",
            self.depth_mask_agrees_with_dest_blend,
            self.depth_mask_agrees_with_dest_blend + self.depth_mask_disagrees_with_dest_blend
        );
        if !self.unknown_property_tags.is_empty() {
            println!(
                "  unknown shader-material property tags: {:?}",
                self.unknown_property_tags
            );
        }
        if self.unordered_hierarchies != 0 {
            println!(
                "  {} hierarchies are not stored parents-first",
                self.unordered_hierarchies
            );
        }
    }
}

fn walk(root: &Path, totals: &mut Totals) {
    for path in files_with_extension(root, "big") {
        let Ok(archive) = BigArchive::open(&path) else {
            continue;
        };
        let label = file_name(&path);
        for entry in archive.entries() {
            if !entry.name.to_ascii_lowercase().ends_with(".w3d") {
                continue;
            }
            match archive.read_entry(entry) {
                Ok(bytes) => totals.absorb(&format!("{label}::{}", entry.name), &bytes),
                Err(error) => {
                    totals.files_seen += 1;
                    totals
                        .failures
                        .push(format!("{label}::{}: {error}", entry.name));
                }
            }
        }
    }

    for path in files_with_extension(root, "w3d") {
        match std::fs::read(&path) {
            Ok(bytes) => totals.absorb(&path.display().to_string(), &bytes),
            Err(error) => {
                totals.files_seen += 1;
                totals.failures.push(format!("{}: {error}", path.display()));
            }
        }
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

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
