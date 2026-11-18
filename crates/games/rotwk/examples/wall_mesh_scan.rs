use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use mint_sage_asset::Resolver;
use mint_sage_big::BigArchive;
use mint_sage_w3d::Model;
use mint_vfs::{DirMount, Vfs};

const FIELDS: &[(
    &str,
    for<'a> fn(&'a mint_sage_ini::DrawModuleView<'a>) -> Option<&'a str>,
)] = &[
    ("WallBoundsMesh", |m| m.wall_bounds_mesh),
    ("RampMesh1", |m| m.ramp_mesh_1),
    ("RampMesh2", |m| m.ramp_mesh_2),
];

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: wall_mesh_scan <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    }
    let mut vfs = Vfs::new();
    for root in &roots {
        mount_install(&mut vfs, root);
    }
    println!("{} visible paths after shadowing", vfs.len());

    let (index, report) = mint_sage_model::load_object_index(&vfs);
    println!(
        "object index: {} entries, {} top-level, {} parsed, {} failure(s)",
        report.entries,
        report.top_level,
        report.parsed,
        report.failures.len()
    );
    for failure in report.failures.iter().take(10) {
        println!("  parse failure: {failure}");
    }

    let resolver = Resolver::new();
    let mut hits = 0usize;
    let mut resolved = 0usize;
    let names: Vec<String> = index.names().map(str::to_owned).collect();
    for name in &names {
        for module in index.draw_modules(name) {
            for (field_name, getter) in FIELDS {
                let Some(mesh_name) = getter(&module) else {
                    continue;
                };
                hits += 1;
                let outcome = resolve(&vfs, &resolver, &index, name, mesh_name);
                if outcome.starts_with("ok") {
                    resolved += 1;
                }
                println!("{name} / {field_name} = {mesh_name}: {outcome}");
            }
        }
    }
    println!("{resolved} of {hits} named mesh reference(s) resolved to real geometry");
    ExitCode::SUCCESS
}

fn resolve(
    vfs: &Vfs,
    resolver: &Resolver,
    index: &mint_sage_model::ObjectIndex,
    template: &str,
    mesh_name: &str,
) -> String {
    let drawn = index.lookup(template);
    let Some(model_name) = drawn.model() else {
        return "FAIL: template has no model".to_owned();
    };
    let loaded = match mint_sage_model::load_model(vfs, resolver, model_name) {
        Ok(loaded) => loaded,
        Err(error) => return format!("FAIL: load_model({model_name}): {error}"),
    };
    let bytes = match vfs.read_path(&loaded.path) {
        Ok(bytes) => bytes,
        Err(error) => return format!("FAIL: reading {}: {error}", loaded.path),
    };
    let raw = match Model::parse(&bytes) {
        Ok(raw) => raw,
        Err(error) => return format!("FAIL: Model::parse({}): {error}", loaded.path),
    };
    let found = raw
        .meshes
        .iter()
        .find(|mesh| mesh.name().eq_ignore_ascii_case(mesh_name));
    match found {
        Some(mesh) => {
            let positions = mesh.positions.to_vec().map(|v| v.len());
            let triangles = mesh.triangles.to_vec().map(|v| v.len());
            format!(
                "ok: model {model_name}, mesh found, {:?} position(s), {:?} triangle(s)",
                positions, triangles
            )
        }
        None => {
            let names: Vec<&str> = raw.meshes.iter().map(Mesh::name).collect();
            format!(
                "FAIL: model {model_name} has {} mesh(es), none named {mesh_name:?}: {names:?}",
                raw.meshes.len()
            )
        }
    }
}

use mint_sage_w3d::Mesh;

fn mount_install(vfs: &mut Vfs, root: &Path) {
    for path in archives(root) {
        let Ok(archive) = BigArchive::open(&path) else {
            continue;
        };
        vfs.mount(Arc::new(archive));
    }
    let data = root.join("data");
    if data.is_dir() {
        if let Ok(mount) = DirMount::with_prefix(&data, "data") {
            vfs.mount(Arc::new(mount));
        }
    }
}

fn archives(root: &Path) -> Vec<PathBuf> {
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
                    let is_big = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("big"));
                    if is_big {
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
