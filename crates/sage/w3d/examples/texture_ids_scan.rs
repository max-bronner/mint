use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use mint_sage_big::BigArchive;
use mint_sage_w3d::{Array, IndexTable, Mesh, Model, Shader};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct BlendState {
    opaque_replace: bool,
    additive: bool,
    alpha_test: bool,
}

fn blend_state(shader: &Shader) -> BlendState {
    BlendState {
        opaque_replace: shader.dest_blend == 0
            || (shader.dest_blend != 1 && shader.alpha_test != 0 && shader.depth_mask != 0),
        additive: shader.dest_blend == 1,
        alpha_test: shader.alpha_test != 0,
    }
}

fn draw_groups(mesh: &Mesh<'_>, with_shader: bool) -> usize {
    let Some(pass) = mesh.passes.first() else {
        return 1;
    };
    fn per_face<'a, 'b>(ids: Option<&'b IndexTable<'a>>) -> Option<&'b Array<'a, u32>> {
        match ids {
            Some(IndexTable::PerElement(ids)) if ids.len() > 1 => Some(ids),
            _ => None,
        }
    }
    let stage0 = per_face(
        pass.texture_stages
            .first()
            .and_then(|stage| stage.texture_ids.as_ref()),
    );
    let stage1 = per_face(
        pass.texture_stages
            .get(1)
            .and_then(|stage| stage.texture_ids.as_ref()),
    );
    let shader = if with_shader {
        per_face(pass.shader_ids.as_ref())
    } else {
        None
    };
    if stage0.is_none() && stage1.is_none() && shader.is_none() {
        return 1;
    }
    let id_at = |ids: Option<&Array<'_, u32>>, face: usize| {
        ids?.get(face)?.ok().filter(|id| *id != u32::MAX)
    };
    let mut groups: BTreeSet<(Option<u32>, Option<u32>, Option<u32>)> = BTreeSet::new();
    for face in 0..mesh.triangles.len() {
        groups.insert((
            id_at(stage0, face),
            id_at(stage1, face),
            id_at(shader, face),
        ));
    }
    groups.len().max(1)
}

fn main() {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: texture_ids_scan <install dir> [<install dir> ...]");
        return;
    }

    let mut jobs: Vec<(String, Vec<u8>)> = Vec::new();
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
                jobs.push((entry.name.to_string(), bytes.to_vec()));
            }
        }
        for path in files_with_extension(root, "w3d") {
            if let Ok(bytes) = std::fs::read(&path) {
                jobs.push((path.display().to_string(), bytes));
            }
        }
    }

    #[derive(Default)]
    struct StageStats {
        with_ids: usize,
        shared: usize,
        per_face: usize,
        distinct: BTreeMap<usize, usize>,
    }

    let mut meshes = 0usize;
    let mut stats: [StageStats; 2] = Default::default();
    let mut files_affected: BTreeSet<String> = BTreeSet::new();
    let mut extra_draws = 0usize;
    let mut examples: Vec<String> = Vec::new();
    let mut per_face_shaders = 0usize;
    let mut per_face_shaders_multi = 0usize;
    let mut per_face_shaders_differ = 0usize;
    let mut shader_examples: Vec<String> = Vec::new();
    let mut meshes_with_extra_shader_draws = 0usize;
    let mut extra_shader_draws = 0usize;
    let mut extra_draw_examples: Vec<String> = Vec::new();

    for (name, bytes) in &jobs {
        let Ok(model) = Model::parse(bytes) else {
            continue;
        };
        for mesh in &model.meshes {
            meshes += 1;
            let Some(pass) = mesh.passes.first() else {
                continue;
            };
            if let Some(IndexTable::PerElement(ids)) = pass.shader_ids.as_ref() {
                if ids.len() > 1 {
                    per_face_shaders += 1;
                    let set: BTreeSet<u32> = ids.iter().filter_map(Result::ok).collect();
                    if set.len() > 1 {
                        per_face_shaders_multi += 1;
                        let states: BTreeSet<BlendState> = set
                            .iter()
                            .filter_map(|id| usize::try_from(*id).ok())
                            .filter_map(|id| mesh.shaders.get(id))
                            .filter_map(Result::ok)
                            .map(|shader| blend_state(&shader))
                            .collect();
                        if states.len() > 1 {
                            per_face_shaders_differ += 1;
                            if shader_examples.len() < 30 {
                                shader_examples.push(format!(
                                    "  {name} :: {} - {} distinct shader ids, {} distinct blend \
                                     states {states:?}",
                                    mesh.name(),
                                    set.len(),
                                    states.len()
                                ));
                            }
                        }
                    }
                }
            }
            let without_shader = draw_groups(mesh, false);
            let with_shader = draw_groups(mesh, true);
            if with_shader > without_shader {
                meshes_with_extra_shader_draws += 1;
                extra_shader_draws += with_shader - without_shader;
                if extra_draw_examples.len() < 30 {
                    extra_draw_examples.push(format!(
                        "  {name} :: {} - {without_shader} part(s) without the shader key, \
                         {with_shader} with it",
                        mesh.name()
                    ));
                }
            }

            let mut mesh_groups = 1usize;
            for (stage, counts) in stats.iter_mut().enumerate() {
                let Some(ids) = pass
                    .texture_stages
                    .get(stage)
                    .and_then(|stage| stage.texture_ids.as_ref())
                else {
                    continue;
                };
                counts.with_ids += 1;
                match ids {
                    IndexTable::Shared(_) => counts.shared += 1,
                    IndexTable::PerElement(array) => {
                        counts.per_face += 1;
                        let set: BTreeSet<u32> = array.iter().filter_map(Result::ok).collect();
                        *counts.distinct.entry(set.len()).or_default() += 1;
                        if set.len() > 1 {
                            mesh_groups = mesh_groups.max(set.len());
                            files_affected.insert(name.to_ascii_lowercase());
                            if examples.len() < 15 {
                                let names: Vec<&str> = set
                                    .iter()
                                    .filter_map(|id| usize::try_from(*id).ok())
                                    .filter_map(|id| mesh.textures.get(id))
                                    .map(|texture| texture.name.as_str())
                                    .collect();
                                examples.push(format!(
                                    "  {name} :: {} stage {stage}: {} faces, {} ids {names:?}",
                                    mesh.name(),
                                    array.len(),
                                    set.len()
                                ));
                            }
                        }
                    }
                }
            }
            extra_draws += mesh_groups - 1;
        }
    }

    println!("{} files, {meshes} meshes", jobs.len());
    for (stage, counts) in stats.iter().enumerate() {
        println!(
            "stage {stage}: {} meshes carry TEXTURE_IDS - {} shared, {} per-face",
            counts.with_ids, counts.shared, counts.per_face
        );
        println!(
            "  per-face meshes by distinct id count: {:?}",
            counts.distinct
        );
    }
    println!(
        "{} files affected, {extra_draws} extra draws if split per distinct id",
        files_affected.len()
    );
    println!(
        "{per_face_shaders} meshes name a shader per triangle, {per_face_shaders_multi} of them \
         naming more than one, {per_face_shaders_differ} of those actually disagreeing on blend/\
         alpha-test state"
    );
    println!("examples:");
    for line in &examples {
        println!("{line}");
    }
    println!("shader examples:");
    for line in &shader_examples {
        println!("{line}");
    }
    println!(
        "{meshes_with_extra_shader_draws} meshes draw one or more extra parts once the shader \
         id joins the split key, {extra_shader_draws} extra draws total"
    );
    println!("extra-draw examples:");
    for line in &extra_draw_examples {
        println!("{line}");
    }
}

fn files_with_extension(root: &Path, extension: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case(extension))
            {
                out.push(path);
            }
        }
    }
    out
}
