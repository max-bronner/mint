use std::path::{Path, PathBuf};
use std::time::SystemTime;

use mint_sage_livingworld::save::{self, LoadError, SessionKey};
use mint_sage_livingworld::{LivingWorldState, Rules, StrategicAi};
use mint_sage_save::mint_extension;

use crate::campaign_save::{date_of, save_folder, write_into};

const WOTR_SINGLE: u32 = 4;

pub(crate) fn write_session(
    state: &LivingWorldState,
    ai: Option<&StrategicAi>,
    stem: &str,
    description: &str,
) -> std::io::Result<PathBuf> {
    write_session_in(&save_folder()?, state, ai, stem, description)
}

fn write_session_in(
    folder: &Path,
    state: &LivingWorldState,
    ai: Option<&StrategicAi>,
    stem: &str,
    description: &str,
) -> std::io::Result<PathBuf> {
    let bytes = save::save_file(state, ai, description, date_of(SystemTime::now()))
        .map_err(|refused| std::io::Error::other(refused.to_string()))?;
    write_into(folder, &file_name(stem), &bytes)
}

fn file_name(stem: &str) -> String {
    format!(
        "{}{}",
        crate::campaign_save::sanitised(stem),
        mint_extension(WOTR_SINGLE)
    )
}

pub(crate) fn path_of(stem: &str) -> std::io::Result<PathBuf> {
    Ok(save_folder()?.join(file_name(stem)))
}

pub(crate) fn is_session(path: &Path) -> bool {
    path.to_string_lossy()
        .to_ascii_lowercase()
        .ends_with(&mint_extension(WOTR_SINGLE).to_ascii_lowercase())
}

pub(crate) fn read_key(path: &Path) -> Result<SessionKey, LoadError> {
    save::file_key(&read_bytes(path)?)
}

pub(crate) fn read_session(
    path: &Path,
    rules: Rules,
) -> Result<(LivingWorldState, Option<StrategicAi>), LoadError> {
    save::load_file(rules, &read_bytes(path)?)
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, LoadError> {
    std::fs::read(path).map_err(|_| LoadError::Invalid("the file cannot be read"))
}
