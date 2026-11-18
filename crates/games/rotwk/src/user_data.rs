use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static INSTALL_LEAF: OnceLock<Option<String>> = OnceLock::new();
static GAME_DATA_LEAF: OnceLock<Option<String>> = OnceLock::new();

pub(crate) fn note_install(primary: &Path) {
    INSTALL_LEAF.get_or_init(|| {
        let bytes = std::fs::read(primary.join("gi.dat")).ok()?;
        mint_sage_asset::user_data_leaf(Some(&bytes), None)
    });
}

pub(crate) fn note_game_data_leaf(leaf: Option<&str>) {
    GAME_DATA_LEAF.get_or_init(|| mint_sage_asset::user_data_leaf(None, leaf));
}

pub(crate) fn dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("MINT_USER_DIR").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let leaf = INSTALL_LEAF
        .get()
        .cloned()
        .flatten()
        .or_else(|| GAME_DATA_LEAF.get().cloned().flatten())?;
    Some(base().join(leaf))
}

pub(crate) fn pipeline_cache_dir() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    if let Some(dir) = var("MINT_USER_DIR") {
        return Some(PathBuf::from(dir).join("pipeline-cache"));
    }
    var("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| var("XDG_CACHE_HOME").map(PathBuf::from))
        .or_else(|| var("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .map(|base| base.join("mint").join("pipeline-cache"))
}

fn base() -> PathBuf {
    let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    var("APPDATA")
        .map(PathBuf::from)
        .or_else(|| var("XDG_DATA_HOME").map(PathBuf::from))
        .or_else(|| var("HOME").map(|home| PathBuf::from(home).join(".local").join("share")))
        .unwrap_or_else(std::env::temp_dir)
}
