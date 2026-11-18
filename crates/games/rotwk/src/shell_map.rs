#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShellMapData {
    pub(crate) name: String,
    pub(crate) on: bool,
    pub(crate) off_by_command_argument: bool,
    pub(crate) play_intro: bool,
}

impl Default for ShellMapData {
    fn default() -> Self {
        Self {
            name: String::from(DEFAULT_SHELL_MAP),
            on: true,
            off_by_command_argument: false,
            play_intro: true,
        }
    }
}

const DEFAULT_SHELL_MAP: &str = "Maps\\ShellMap1\\ShellMap1.map";

pub(crate) const EFFECTS_LOD_ULTRA_HIGH: u8 = 4;

impl ShellMapData {
    pub(crate) fn from_block(block: &mint_sage_ini::Block) -> Self {
        let defaults = Self::default();
        let flag = |name: &str, default: bool| {
            block
                .field(name)
                .and_then(mint_sage_ini::Field::value)
                .and_then(mint_sage_ini::value::parse_bool)
                .unwrap_or(default)
        };
        Self {
            name: block
                .field("ShellMapName")
                .and_then(mint_sage_ini::Field::value)
                .map_or(defaults.name, str::to_owned),
            on: flag("ShellMapOn", defaults.on),
            off_by_command_argument: flag(
                "ShellMapOffByCommandArgument",
                defaults.off_by_command_argument,
            ),
            play_intro: flag("PlayIntro", defaults.play_intro),
        }
    }

    pub(crate) fn switch_off_by_command_argument(&mut self) {
        self.on = false;
        self.off_by_command_argument = true;
    }
}

pub(crate) fn shell_map_on(
    fields: &ShellMapData,
    map_exists: bool,
    effects_lod: u8,
    latched_off: &mut bool,
    headless: bool,
) -> bool {
    let on = !*latched_off
        && fields.on
        && !headless
        && map_exists
        && effects_lod > 1
        && !fields.off_by_command_argument;
    if !on {
        *latched_off = true;
    }
    on
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(text: &str) -> ShellMapData {
        let source = format!("GameData\n{text}End\n");
        let document = mint_sage_ini::Document::parse(
            "data/ini/gamedata.ini",
            &source,
            &mint_sage_ini::BlockTable::corpus_path(),
        )
        .unwrap();
        ShellMapData::from_block(&mint_sage_ini::game_data::merged_game_data(&[document]))
    }

    fn on(fields: &ShellMapData, map_exists: bool, effects_lod: u8, latch: &mut bool) -> bool {
        shell_map_on(fields, map_exists, effects_lod, latch, false)
    }

    #[test]
    fn the_four_fields_override_their_defaults_and_absent_ones_keep_them() {
        let none = fields("");
        assert_eq!(none, ShellMapData::default());
        let written = fields(
            "  ShellMapName = Maps\\Test\\Test.map\n  ShellMapOn = No\n  \
             ShellMapOffByCommandArgument = Yes\n  PlayIntro = No\n",
        );
        assert_eq!(
            written,
            ShellMapData {
                name: String::from("Maps\\Test\\Test.map"),
                on: false,
                off_by_command_argument: true,
                play_intro: false,
            }
        );
    }

    #[test]
    fn the_switch_follows_the_data_the_map_and_the_detail_level() {
        let mut latch = false;
        assert!(on(&fields(""), true, 3, &mut latch));
        let mut latch = false;
        assert!(!on(&fields("  ShellMapOn = No\n"), true, 3, &mut latch));
        let mut latch = false;
        assert!(!on(&fields(""), false, 3, &mut latch));
        let mut latch = false;
        assert!(on(&fields(""), true, 2, &mut latch));
        let mut latch = false;
        assert!(!on(&fields(""), true, 1, &mut latch));
        let mut latch = false;
        assert!(!shell_map_on(&fields(""), true, 3, &mut latch, true));
    }

    #[test]
    fn the_command_line_switch_stays_off() {
        let mut data = fields("");
        data.switch_off_by_command_argument();
        let mut latch = false;
        assert!(!on(&data, true, 4, &mut latch));
    }

    #[test]
    fn a_refused_session_is_not_given_the_map_back() {
        let data = fields("");
        let mut latch = false;
        assert!(!on(&data, true, 1, &mut latch));
        assert!(!on(&data, true, 3, &mut latch));
    }
}
