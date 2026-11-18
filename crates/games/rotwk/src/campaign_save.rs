use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use mint_sage_save::{
    between_missions_defaults, has_retail_extension, is_scratch_map_name, mint_extension,
    AsciiString, CampaignBlock, CampaignState, CurrentCampaign, GameStateBlock, SaveError,
    SaveFile, CHUNK_AUDIO, CHUNK_CAMPAIGN, CHUNK_GAME_LOGIC, CHUNK_GAME_STATE,
    CHUNK_GAME_STATE_MAP, CHUNK_LIVING_WORLD_LOGIC,
};
use mint_vfs::Vfs;

use crate::campaign::CampaignRun;

const DESCRIPTION: &str = "__AUTO#SAVE__";
const BETWEEN_MISSIONS: u32 = 1;
const NAME_LABEL: &str = "GUI:AutoSaveName";
const NAME_MISSING: &str = "00000000";

pub(crate) struct Autosave {
    pub(crate) campaign: String,
    pub(crate) difficulty: u32,
    pub(crate) state: CampaignState,
    pub(crate) map_label: String,
    pub(crate) date: [u16; 8],
    pub(crate) description: String,
    pub(crate) is_auto_save: u32,
}

pub(crate) fn encode(autosave: &Autosave) -> Result<Vec<u8>, SaveError> {
    let (living_world, audio, map, logic) = between_missions_defaults();
    let info = GameStateBlock {
        save_file_type: BETWEEN_MISSIONS,
        date: autosave.date,
        description: autosave.description.as_str().into(),
        map_label: autosave.map_label.as_str().into(),
        is_auto_save: autosave.is_auto_save,
        ..GameStateBlock::default()
    };
    let campaign = CampaignBlock {
        campaign: Some(CurrentCampaign {
            name: AsciiString::from(autosave.campaign.as_str()),
            difficulty: autosave.difficulty,
            state: autosave.state.clone(),
        }),
    };
    let mut file = SaveFile::default();
    file.push_with(CHUNK_LIVING_WORLD_LOGIC, |_| living_world.encode())?;
    file.push_with(CHUNK_AUDIO, |_| audio.encode())?;
    file.push_with(CHUNK_GAME_STATE, |_| info.encode())?;
    file.push_with(CHUNK_CAMPAIGN, |offset| campaign.encode(offset))?;
    file.push_with(CHUNK_GAME_STATE_MAP, |_| map.encode())?;
    file.push_with(CHUNK_GAME_LOGIC, |_| logic.encode())?;
    Ok(file.to_bytes())
}

pub(crate) fn file_name(format: Option<&str>, map_name: &str) -> String {
    let stem = match format {
        Some(format) => format
            .replace("%ls", map_name)
            .replace("%s", map_name)
            .replace("%S", map_name),
        None => NAME_MISSING.to_owned(),
    };
    format!("{}{}", sanitised(&stem), mint_extension(BETWEEN_MISSIONS))
}

pub(crate) fn sanitised(stem: &str) -> String {
    stem.chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect()
}

fn normalised(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

pub(crate) fn map_display_name(
    vfs: &Vfs,
    strings: &mint_sage_str::StringTable,
    label: &str,
) -> String {
    let wanted = normalised(label);
    let key = vfs.read("maps/mapcache.ini").ok().and_then(|bytes| {
        crate::mapcache::parse_map_cache(&String::from_utf8_lossy(&bytes))
            .into_iter()
            .find(|entry| normalised(&entry.map_path) == wanted)
            .and_then(|entry| entry.display_name_key)
    });
    key.map(|key| mint_sage_apt_viewer::localise(&key, strings))
        .filter(|text| !text.starts_with("MISSING:"))
        .unwrap_or_else(|| label.to_owned())
}

fn date_of(now: SystemTime) -> [u16; 8] {
    let since = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = i64::try_from(since.as_secs()).unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    let weekday = (days + 4).rem_euclid(7);
    let field = |value: i64| u16::try_from(value).unwrap_or(0);
    [
        field(year),
        field(month),
        field(day),
        field(weekday),
        field(rest / 3_600),
        field(rest % 3_600 / 60),
        field(rest % 60),
        u16::try_from(since.subsec_millis()).unwrap_or(0),
    ]
}

fn save_folder() -> std::io::Result<PathBuf> {
    crate::user_data::dir()
        .map(|dir| dir.join("Save"))
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no user data folder"))
}

fn snapshot(
    install_roots: &[PathBuf],
    run: &CampaignRun,
    description: &str,
    is_auto_save: u32,
) -> std::io::Result<(Autosave, mint_sage_str::StringTable)> {
    let other = |message: String| std::io::Error::other(message);
    let next = run
        .mission(run.state.menu_mission_index())
        .and_then(|mission| mission.map.as_deref())
        .ok_or_else(|| other("the next mission has no map".to_owned()))?;
    let map_label = format!("maps\\{next}\\{next}.map");
    let (strings, _, _) = mint_sage_apt_viewer::load_string_table_from_roots(install_roots);
    let autosave = Autosave {
        campaign: run.def.name.clone(),
        difficulty: u32::from(run.state.difficulty()),
        state: run.state.to_save(&run.carry),
        map_label,
        date: date_of(SystemTime::now()),
        description: description.to_owned(),
        is_auto_save,
    };
    Ok((autosave, strings))
}

pub(crate) fn write_autosave(
    vfs: &Vfs,
    install_roots: &[PathBuf],
    run: &CampaignRun,
) -> std::io::Result<PathBuf> {
    let folder = save_folder()?;
    let (autosave, strings) = snapshot(install_roots, run, DESCRIPTION, 1)?;
    let name = file_name(
        strings.get(NAME_LABEL),
        &map_display_name(vfs, &strings, &autosave.map_label),
    );
    let bytes = encode(&autosave).map_err(|error| std::io::Error::other(error.to_string()))?;
    write_into(&folder, &name, &bytes)
}

pub(crate) fn write_user_save(
    install_roots: &[PathBuf],
    run: &CampaignRun,
    stem: &str,
    description: &str,
) -> std::io::Result<PathBuf> {
    let folder = save_folder()?;
    let (autosave, _) = snapshot(install_roots, run, description, 0)?;
    let name = format!("{}{}", sanitised(stem), mint_extension(BETWEEN_MISSIONS));
    let bytes = encode(&autosave).map_err(|error| std::io::Error::other(error.to_string()))?;
    write_into(&folder, &name, &bytes)
}

fn write_into(folder: &Path, name: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    if has_retail_extension(name) || is_scratch_map_name(name) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "mint does not write retail save extensions or names retail deletes",
        ));
    }
    std::fs::create_dir_all(folder)?;
    let path = folder.join(name);
    std::fs::write(&path, bytes)?;
    Ok(path)
}

#[cfg(test)]
mod tests;
