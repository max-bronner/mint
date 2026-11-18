use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::title::BaseGame;

pub(crate) fn with_base_game(
    primary: &Path,
    extras: &[PathBuf],
    base: Option<&BaseGame>,
) -> Vec<PathBuf> {
    if !extras.is_empty() {
        return extras.to_vec();
    }
    let Some(base) = base else {
        return Vec::new();
    };
    static FOUND: OnceLock<Option<PathBuf>> = OnceLock::new();
    let found = FOUND.get_or_init(|| {
        let key = base.app_paths_key;
        let found = registry_path(key).filter(|dir| is_base_install(dir, base.marker_files));
        match &found {
            Some(dir) => println!(
                "base game: {} from {key} - mounted beneath the primary (--install overrides)",
                dir.display()
            ),
            None => println!(
                "error: no base game install found ({key} `Path`, holding {}) - retail stops                  here (BaseGameFilesNotFound); mint goes on with the primary alone, so the                  script libraries, AI bases and castles only the base game ships are missing.                  Pass --install <base game install>.",
                base.marker_files.join(", ")
            ),
        }
        found
    });
    found
        .iter()
        .filter(|dir| *dir != primary)
        .cloned()
        .collect()
}

fn registry_path(key: &str) -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let output = std::process::Command::new("reg")
        .args(["query", key, "/v", "Path", "/reg:32"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    reg_query_value(&String::from_utf8_lossy(&output.stdout), "Path").map(PathBuf::from)
}

fn reg_query_value(stdout: &str, name: &str) -> Option<String> {
    stdout.lines().find_map(|line| {
        let rest = line.trim_start().strip_prefix(name)?.trim_start();
        let value = rest
            .strip_prefix("REG_SZ")
            .or_else(|| rest.strip_prefix("REG_EXPAND_SZ"))?;
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

fn is_base_install(dir: &Path, marker_files: &[&str]) -> bool {
    marker_files.iter().all(|file| dir.join(file).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reg_query_value_keeps_spaces_and_skips_other_values() {
        let stdout = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Some Key\r\n    PathExtra    REG_SZ    \
                      X:\\wrong\r\n    Path    REG_SZ    X:\\Some Games\\Base Game 2\r\n\r\n";
        assert_eq!(
            reg_query_value(stdout, "Path").as_deref(),
            Some("X:\\Some Games\\Base Game 2")
        );
        assert_eq!(
            reg_query_value("    Path    REG_DWORD    0x1\r\n", "Path"),
            None
        );
    }

    #[test]
    fn a_base_install_needs_every_marker_file() {
        let markers = crate::title::ROTWK.base_game.unwrap().marker_files;
        let dir = std::env::temp_dir().join(format!("mint-base-install-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for file in markers.iter().skip(1) {
            std::fs::write(dir.join(file), b"").unwrap();
        }
        assert!(!is_base_install(&dir, markers));
        std::fs::write(dir.join(markers[0]), b"").unwrap();
        assert!(is_base_install(&dir, markers));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn named_installs_are_kept_as_given() {
        let named = vec![PathBuf::from("named")];
        assert_eq!(
            with_base_game(
                Path::new("primary"),
                &named,
                crate::title::ROTWK.base_game.as_ref()
            ),
            named
        );
    }
}
