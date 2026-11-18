mod corpus;

use std::path::PathBuf;
use std::process::ExitCode;

use mint_sage_ini_coverage::{abilities, report};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(kind) = args.first().cloned() else {
        usage();
        return ExitCode::FAILURE;
    };

    let mut rest = args.iter().skip(1);
    let abilities_out = (kind == "--abilities")
        .then(|| rest.next())
        .flatten()
        .map(PathBuf::from);
    if kind == "--abilities" && abilities_out.is_none() {
        eprintln!("error: --abilities wants an output path before the install dirs");
        return ExitCode::FAILURE;
    }

    let mut roots: Vec<PathBuf> = rest.map(PathBuf::from).collect();
    if roots.is_empty() {
        if let Some(corpus) = std::env::var_os("MINT_CORPUS") {
            roots.push(PathBuf::from(corpus));
        }
    }
    if roots.is_empty() {
        usage();
        return ExitCode::FAILURE;
    }

    let Some(repo_root) = find_repo_root() else {
        eprintln!("error: could not locate the workspace root above this binary");
        return ExitCode::FAILURE;
    };

    let documents = match corpus::load(&roots) {
        Ok(documents) => documents,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    };
    println!("{} documents loaded", documents.len());

    let run = report::Run::build(&documents);

    if let Some(out_path) = abilities_out {
        let mut world = mint_sim::World::new();
        let mut registry = mint_sim::module::ModuleRegistry::new(&mut world);
        mint_sage_modules::register_every_family(&mut registry, &mut world);
        let is_registered = |name: &str| registry.is_registered(name);
        let report = abilities::render(&documents, &run, &repo_root, &is_registered);
        if let Err(error) = std::fs::write(&out_path, report) {
            eprintln!("error: writing {}: {error}", out_path.display());
            return ExitCode::FAILURE;
        }
        println!("abilities report written to {}", out_path.display());
    } else if kind == "--summary" {
        report::summary(&run, &repo_root);
    } else {
        report::one(&run, &repo_root, &kind);
    }

    ExitCode::SUCCESS
}

fn usage() {
    eprintln!(
        "usage: ini-coverage <BlockType|--summary|--abilities <out.md>> <install dir> [<install dir> ...]  (or set MINT_CORPUS)\n\
         \n\
         BlockType is one of: Locomotor, Weapon, SpecialPower, Object"
    );
}

fn find_repo_root() -> Option<PathBuf> {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join("Cargo.toml").is_file() && dir.join("deny.toml").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}
