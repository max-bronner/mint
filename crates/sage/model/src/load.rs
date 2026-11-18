use mint_render::Aabb;
use mint_sage_asset::Resolver;
use mint_sage_w3d::{Hierarchy, Model};
use mint_vfs::{Vfs, VfsPath};

use crate::{convert, ConvertedModel, Error, Result};

#[derive(Debug, Clone)]
pub struct LoadedModel {
    pub path: VfsPath,
    pub skeleton: Option<VfsPath>,
    pub hierarchy: Option<Hierarchy>,
    pub embedded_animation_name: Option<String>,
    pub model: ConvertedModel,
}

impl LoadedModel {
    #[must_use]
    pub fn bounds(&self) -> Aabb {
        self.model.bounds
    }
}

pub fn load_model(vfs: &Vfs, resolver: &Resolver, name: &str) -> Result<LoadedModel> {
    let wanted = if name.to_ascii_lowercase().ends_with(".w3d") {
        name.to_owned()
    } else {
        format!("{name}.w3d")
    };
    let path = resolver
        .resolve(vfs, &wanted)
        .map_err(|source| Error::Unresolved {
            name: wanted.clone(),
            source,
        })?;

    let bytes = vfs.read_path(&path)?;
    let model = Model::parse(&bytes)?;
    if model.meshes.is_empty() {
        return Err(Error::NoGeometry { name: wanted });
    }
    let embedded_animation_name = model.animations.first().map(|a| a.name.clone());

    let mut skeleton_path = None;
    let mut skeleton_bytes = Vec::new();
    let mut notes = Vec::new();
    if let Some(wanted_hierarchy) = model.external_hierarchy() {
        match find_skeleton(vfs, resolver, wanted_hierarchy) {
            Some((found, bytes)) => {
                skeleton_path = Some(found);
                skeleton_bytes = bytes;
            }
            None => notes.push(format!(
                "the HLOD names hierarchy {wanted_hierarchy}, which no mounted file supplies; \
                 parts are placed at the origin"
            )),
        }
    }

    let skeleton = if skeleton_bytes.is_empty() {
        None
    } else {
        match Model::parse(&skeleton_bytes) {
            Ok(parsed) => parsed.hierarchy,
            Err(error) => {
                notes.push(format!("the skeleton file did not parse: {error}"));
                None
            }
        }
    };
    let hierarchy_ref: Option<&Hierarchy> = model.hierarchy.as_ref().or(skeleton.as_ref());

    let mut converted = convert::convert_model(&model, hierarchy_ref)?;
    converted.notes.splice(0..0, notes);
    let hierarchy = hierarchy_ref.cloned();

    Ok(LoadedModel {
        path,
        skeleton: skeleton_path,
        hierarchy,
        embedded_animation_name,
        model: converted,
    })
}

#[derive(Debug, Clone)]
pub struct LoadedAnimation {
    pub path: VfsPath,
    pub animation: mint_sage_w3d::Animation,
    pub hierarchy_path: VfsPath,
    pub hierarchy: Hierarchy,
}

pub fn load_animation(vfs: &Vfs, resolver: &Resolver, name: &str) -> Result<LoadedAnimation> {
    let wanted = if name.to_ascii_lowercase().ends_with(".w3d") {
        name.to_owned()
    } else {
        format!("{name}.w3d")
    };
    let path = resolver
        .resolve(vfs, &wanted)
        .map_err(|source| Error::Unresolved {
            name: wanted.clone(),
            source,
        })?;
    let bytes = vfs.read_path(&path)?;
    let model = Model::parse(&bytes)?;
    let found = describe_contents(&model);
    let animation = model
        .animations
        .into_iter()
        .next()
        .ok_or_else(|| Error::NoAnimation {
            name: wanted.clone(),
            found,
        })?;

    if let Some(hierarchy) = model.hierarchy {
        if hierarchy
            .header
            .name
            .eq_ignore_ascii_case(&animation.hierarchy_name)
        {
            return Ok(LoadedAnimation {
                path: path.clone(),
                hierarchy_path: path,
                animation,
                hierarchy,
            });
        }
    }

    let tried = skeleton_candidates(&animation.hierarchy_name);
    let not_found = || Error::SkeletonNotFound {
        animation: wanted.clone(),
        hierarchy: animation.hierarchy_name.clone(),
        tried: tried.to_vec(),
    };
    let Some((hierarchy_path, hierarchy_bytes)) =
        find_skeleton(vfs, resolver, &animation.hierarchy_name)
    else {
        return Err(not_found());
    };
    let hierarchy = Model::parse(&hierarchy_bytes)?
        .hierarchy
        .ok_or_else(not_found)?;

    Ok(LoadedAnimation {
        path,
        animation,
        hierarchy_path,
        hierarchy,
    })
}

fn describe_contents(model: &Model<'_>) -> String {
    let mut parts = Vec::new();
    if model.hierarchy.is_some() {
        parts.push("HIERARCHY".to_owned());
    }
    if !model.meshes.is_empty() {
        parts.push(format!("{} mesh(es)", model.meshes.len()));
    }
    if !model.hlods.is_empty() {
        parts.push(format!("{} HLOD(s)", model.hlods.len()));
    }
    if parts.is_empty() {
        "nothing else recognised".to_owned()
    } else {
        parts.join(", ")
    }
}

fn skeleton_candidates(hierarchy_name: &str) -> [String; 2] {
    let stem = hierarchy_name.to_ascii_lowercase();
    [format!("{stem}.w3d"), format!("{stem}_skl.w3d")]
}

fn find_skeleton(vfs: &Vfs, resolver: &Resolver, hierarchy: &str) -> Option<(VfsPath, Vec<u8>)> {
    for candidate in skeleton_candidates(hierarchy) {
        let Ok(path) = resolver.resolve(vfs, &candidate) else {
            continue;
        };
        let Ok(bytes) = vfs.read_path(&path) else {
            continue;
        };
        return Some((path, bytes));
    }
    None
}

#[must_use]
pub fn find_models(vfs: &Vfs, needle: &str) -> Vec<String> {
    let needle = needle.to_ascii_lowercase();
    let mut found: Vec<String> = vfs
        .paths()
        .filter(|path| path.extension() == Some("w3d"))
        .filter_map(|path| path.as_str().rsplit('/').next())
        .filter(|name| needle.is_empty() || name.contains(&needle))
        .map(str::to_owned)
        .collect();
    found.sort();
    found.dedup();
    found
}

#[cfg(test)]
mod tests;
