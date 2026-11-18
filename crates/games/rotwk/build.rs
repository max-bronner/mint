use std::path::{Path, PathBuf};
use std::process::Command;

fn git(arguments: &[&str]) -> Option<String> {
    let output = Command::new("git").args(arguments).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn profile() -> String {
    std::env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .and_then(|out| {
            out.ancestors()
                .nth(3)
                .and_then(Path::file_name)
                .map(|name| name.to_string_lossy().into_owned())
        })
        .or_else(|| std::env::var("PROFILE").ok())
        .unwrap_or_else(|| String::from("unknown"))
}

fn main() {
    let commit = git(&["rev-parse", "HEAD"]).filter(|hash| !hash.is_empty());
    let tree = match git(&["status", "--porcelain", "--untracked-files=no"]) {
        Some(changes) if changes.is_empty() => "clean",
        Some(_) => "had uncommitted changes",
        None => "unknown",
    };
    println!(
        "cargo:rustc-env=MINT_BUILD_COMMIT={}",
        commit.as_deref().unwrap_or("unknown")
    );
    println!("cargo:rustc-env=MINT_BUILD_TREE={tree}");
    println!("cargo:rustc-env=MINT_BUILD_PROFILE={}", profile());

    if let Some(log) = git(&["rev-parse", "--git-path", "logs/HEAD"]) {
        println!("cargo:rerun-if-changed={log}");
    }
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=build.rs");
}
