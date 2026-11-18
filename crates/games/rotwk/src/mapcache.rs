use std::collections::HashMap;

use crate::list_sort::{name_order, ColumnSort, SortMode};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SkirmishMap {
    pub(crate) map_path: String,
    pub(crate) display_name: String,
    pub(crate) players: u32,
    pub(crate) description: String,
    pub(crate) extent: Option<[f32; 4]>,
    pub(crate) starts: Vec<[f32; 2]>,
    pub(crate) official: bool,
}

pub(crate) const SORT_NAME: usize = 0;
pub(crate) const SORT_PLAYERS: usize = 1;
pub(crate) const SORT_HONOURS: usize = 2;
pub(crate) const DEFAULT_SORT: ColumnSort = ColumnSort::new(
    SortMode::ascending(SORT_PLAYERS),
    SortMode::ascending(SORT_NAME),
);

impl SkirmishMap {
    fn honours_rank(&self) -> u32 {
        if self.official {
            0
        } else {
            0x8000
        }
    }

    fn key_order(&self, other: &Self, key: usize) -> std::cmp::Ordering {
        match key {
            SORT_NAME => name_order(&self.display_name, &other.display_name),
            SORT_PLAYERS => self.players.cmp(&other.players),
            _ => self.honours_rank().cmp(&other.honours_rank()),
        }
    }

    pub(crate) fn sort_order(&self, other: &Self, sort: &ColumnSort) -> std::cmp::Ordering {
        sort.compare(|key| self.key_order(other, key))
            .then_with(|| self.key_order(other, SORT_NAME))
            .then_with(|| self.key_order(other, SORT_PLAYERS))
            .then_with(|| self.key_order(other, SORT_HONOURS))
    }
}

pub(crate) fn skirmish_maps(
    vfs: &mint_vfs::Vfs,
    strings: &mint_sage_str::StringTable,
) -> Vec<SkirmishMap> {
    let Ok(bytes) = vfs.read("maps/mapcache.ini") else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&bytes);
    maps_from_entries(parse_map_cache(&text), strings)
}

fn maps_from_entries(
    entries: Vec<MapCacheEntry>,
    strings: &mint_sage_str::StringTable,
) -> Vec<SkirmishMap> {
    let found = |key: &str| {
        let label = mint_sage_apt_viewer::localise(key, strings);
        (!label.starts_with("MISSING:")).then_some(label)
    };
    let mut maps: Vec<SkirmishMap> = entries
        .into_iter()
        .filter(MapCacheEntry::skirmish_playable)
        .map(|entry| {
            let key = entry.display_name_key.as_deref();
            SkirmishMap {
                display_name: key
                    .and_then(found)
                    .unwrap_or_else(|| entry.map_path.clone()),
                description: key
                    .and_then(|key| found(&format!("{key}/Desc")))
                    .unwrap_or_default(),
                players: entry.players,
                map_path: entry.map_path,
                extent: entry.extent,
                starts: entry.starts,
                official: entry.official,
            }
        })
        .collect();
    maps.sort_by(|a, b| a.sort_order(b, &DEFAULT_SORT));
    maps
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MapCacheEntry {
    pub(crate) map_path: String,
    pub(crate) display_name_key: Option<String>,
    pub(crate) is_multiplayer: bool,
    pub(crate) has_player_1_start: bool,
    pub(crate) has_player_2_start: bool,
    pub(crate) players: u32,
    pub(crate) extent: Option<[f32; 4]>,
    pub(crate) starts: Vec<[f32; 2]>,
    pub(crate) official: bool,
}

impl MapCacheEntry {
    pub(crate) fn skirmish_playable(&self) -> bool {
        self.has_player_1_start && self.has_player_2_start
    }
}

pub(crate) fn parse_map_cache(text: &str) -> Vec<MapCacheEntry> {
    let mut out = Vec::new();
    let mut current: Option<(String, HashMap<String, String>)> = None;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("MapCache ") {
            current = Some((rest.trim().to_owned(), HashMap::new()));
            continue;
        }
        if line.eq_ignore_ascii_case("END") {
            if let Some((name, fields)) = current.take() {
                out.push(build_entry(&name, &fields));
            }
            continue;
        }
        if let (Some((_, fields)), Some((key, value))) = (current.as_mut(), line.split_once('=')) {
            fields.insert(key.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    out
}

fn build_entry(escaped_name: &str, fields: &HashMap<String, String>) -> MapCacheEntry {
    let map_path = sage_ascii(escaped_name).replace('\\', "/");
    let display_name_key = fields
        .get("displayname")
        .map(|v| sage_wide_string(v))
        .filter(|s| !s.is_empty());
    let yes = |key: &str| {
        fields
            .get(key)
            .is_some_and(|v| v.eq_ignore_ascii_case("yes"))
    };
    let is_multiplayer = yes("ismultiplayer");
    let official = yes("isofficial");
    let starts = (1..=8)
        .take_while(|n| fields.contains_key(&format!("player_{n}_start")))
        .count();
    let players = fields
        .get("numplayers")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or_else(|| u32::try_from(starts).unwrap_or(0));
    let point = |key: &str| fields.get(key).and_then(|v| coordinates(v));
    let extent = point("extentmin")
        .zip(point("extentmax"))
        .map(|([lx, ly], [hx, hy])| [lx, ly, hx, hy]);
    let start_points = (1..=8)
        .map_while(|n| point(&format!("player_{n}_start")))
        .collect();
    MapCacheEntry {
        players,
        extent,
        starts: start_points,
        has_player_1_start: fields.contains_key("player_1_start"),
        has_player_2_start: fields.contains_key("player_2_start"),
        map_path,
        display_name_key,
        is_multiplayer,
        official,
    }
}

fn coordinates(value: &str) -> Option<[f32; 2]> {
    let mut x = None;
    let mut y = None;
    for part in value.split_whitespace() {
        let parsed = |v: &str| v.parse::<f32>().ok().filter(|f| f.is_finite());
        if let Some(v) = part.strip_prefix("X:").or_else(|| part.strip_prefix("x:")) {
            x = parsed(v);
        } else if let Some(v) = part.strip_prefix("Y:").or_else(|| part.strip_prefix("y:")) {
            y = parsed(v);
        }
    }
    Some([x?, y?])
}

fn unescape_sage_bytes(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'_' {
            if let Some(byte) = s
                .get(i.saturating_add(1)..i.saturating_add(3))
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            {
                out.push(byte);
                i = i.saturating_add(3);
                continue;
            }
        }
        out.push(b);
        i = i.saturating_add(1);
    }
    out
}

fn sage_ascii(escaped: &str) -> String {
    String::from_utf8_lossy(&unescape_sage_bytes(escaped)).into_owned()
}

fn sage_wide_string(escaped: &str) -> String {
    let bytes = unescape_sage_bytes(escaped);
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .filter_map(|pair| match pair {
            [lo, hi] => Some(u16::from_le_bytes([*lo, *hi])),
            _ => None,
        })
        .collect();
    String::from_utf16_lossy(&units)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GONDOR: &str = "\
MapCache maps_5Cmap_20wor_20gondor_5Cmap_20wor_20gondor_2Emap
  isOfficial = yes
  isMultiplayer = yes
  numPlayers = 5
  displayName = _24_00M_00a_00p_00_3A_00M_00A_00P_00W_00O_00R_00G_00o_00n_00d_00o_00r_00
  Player_1_Start = X:3444.75 Y:1442.66 Z:0.00
  Player_2_Start = X:2827.01 Y:4096.12 Z:0.00
END
";

    const CREATE_A_HERO: &str = "\
MapCache maps_5Ccreateahero_5Ccreateahero_2Emap
  isMultiplayer = no
  numPlayers = 1
  displayName = _24_00M_00a_00p_00_3A_00C_00r_00e_00a_00t_00e_00A_00H_00e_00r_00o_00
  Player_1_Start = X:0.00 Y:0.00 Z:0.00
END
";

    #[test]
    fn a_real_skirmish_map_decodes_its_path_flags_and_display_key() {
        let entries = parse_map_cache(GONDOR);
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.map_path, "maps/map wor gondor/map wor gondor.map");
        assert_eq!(e.display_name_key.as_deref(), Some("$Map:MAPWORGondor"));
        assert!(e.is_multiplayer);
        assert!(e.skirmish_playable());
    }

    #[test]
    fn the_extent_and_starts_read() {
        let e = &parse_map_cache(&GONDOR.replace(
            "  numPlayers = 5\n",
            "  numPlayers = 5\n  extentMin = X:0.00 Y:-10.5 Z:0.00\n  extentMax = X:4000 Y:4250.00 Z:0\n",
        ))[0];
        assert_eq!(e.extent, Some([0.0, -10.5, 4000.0, 4250.0]));
        assert_eq!(e.starts, vec![[3444.75, 1442.66], [2827.01, 4096.12]]);
        assert_eq!(parse_map_cache(GONDOR)[0].extent, None, "not stated");
        assert_eq!(coordinates("X:1 Z:3"), None);
        assert_eq!(coordinates("X:nan Y:2"), None);
        assert_eq!(coordinates("Y:2 X:1"), Some([1.0, 2.0]));
        let gap = "MapCache a_2Emap\n  Player_1_Start = X:1 Y:2\n  Player_2_Start = X:? Y:2\n  Player_3_Start = X:5 Y:6\nEND\n";
        assert_eq!(parse_map_cache(gap)[0].starts, vec![[1.0, 2.0]]);
    }

    #[test]
    fn a_single_player_scenario_is_not_skirmish_playable() {
        let entries = parse_map_cache(CREATE_A_HERO);
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].skirmish_playable());
        assert!(!entries[0].is_multiplayer);
    }

    #[test]
    fn multiple_blocks_in_one_file_parse_independently() {
        let combined = format!("{GONDOR}\n{CREATE_A_HERO}");
        let entries = parse_map_cache(&combined);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].skirmish_playable());
        assert!(!entries[1].skirmish_playable());
    }

    #[test]
    fn a_missing_or_empty_display_name_is_none() {
        let no_field = "MapCache maps_5Cx_2Emap\n  isMultiplayer = yes\nEND\n";
        assert_eq!(parse_map_cache(no_field)[0].display_name_key, None);

        let empty = "MapCache maps_5Cx_2Emap\n  displayName = \nEND\n";
        assert_eq!(parse_map_cache(empty)[0].display_name_key, None);
    }

    #[test]
    fn unescape_sage_bytes_recovers_every_escaped_byte_and_passes_literals_through() {
        assert_eq!(
            unescape_sage_bytes("_00_ff_41A"),
            vec![0x00, 0xff, b'A', b'A']
        );
        assert_eq!(unescape_sage_bytes("plain"), b"plain".to_vec());
        assert_eq!(unescape_sage_bytes("a_"), vec![b'a', b'_']);
    }

    fn table_with(label: &str, text: &str) -> mint_sage_str::StringTable {
        let mut table = mint_sage_str::StringTable::new();
        table.insert([mint_sage_str::StringRecord {
            label: label.to_owned(),
            text: text.to_owned(),
            speech: None,
        }]);
        table
    }

    #[test]
    fn maps_from_entries_filters_localises_and_sorts() {
        let entries = parse_map_cache(&format!("{GONDOR}\n{CREATE_A_HERO}"));
        let strings = table_with("Map:MAPWORGondor", "War of the Ring: Gondor");
        let maps = maps_from_entries(entries, &strings);
        assert_eq!(
            maps,
            vec![SkirmishMap {
                map_path: "maps/map wor gondor/map wor gondor.map".to_owned(),
                display_name: "War of the Ring: Gondor".to_owned(),
                players: 5,
                description: String::new(),
                extent: None,
                starts: vec![[3444.75, 1442.66], [2827.01, 4096.12]],
                official: true,
            }],
            "the single-player scenario must be dropped, and the survivor's \
             key must resolve through the string table"
        );
    }

    #[test]
    fn maps_are_listed_as_ea_lists_them() {
        let entry = |path: &str, key: &str, extra: &str| {
            let wide: String = key
                .bytes()
                .map(|b| format!("{}_00", char::from(b)))
                .collect();
            format!(
                "MapCache {path}\n  displayName = {wide}\n  Player_1_Start = X:0\n  Player_2_Start = X:0\n{extra}END\n"
            )
        };
        let text = [
            entry("b_2Emap", "$Map:B", "  numPlayers = 4\n"),
            entry("a_2Emap", "$Map:A", "  numPlayers = 4\n"),
            entry("c_2Emap", "$Map:C", "  Player_3_Start = X:0\n"),
        ]
        .concat();
        let mut strings = table_with("Map:B", "beta");
        strings.insert([
            mint_sage_str::StringRecord {
                label: "Map:A".to_owned(),
                text: "Alpha".to_owned(),
                speech: None,
            },
            mint_sage_str::StringRecord {
                label: "Map:C".to_owned(),
                text: "Gamma".to_owned(),
                speech: None,
            },
            mint_sage_str::StringRecord {
                label: "Map:A/Desc".to_owned(),
                text: "The first.".to_owned(),
                speech: None,
            },
        ]);
        let maps = maps_from_entries(parse_map_cache(&text), &strings);
        let order: Vec<(&str, u32)> = maps
            .iter()
            .map(|m| (m.display_name.as_str(), m.players))
            .collect();
        assert_eq!(order, vec![("Gamma", 3), ("Alpha", 4), ("beta", 4)]);
        assert_eq!(maps[1].description, "The first.");
        assert_eq!(maps[0].description, "", "no /Desc: none");
    }

    #[test]
    fn an_unresolved_display_name_key_falls_back_to_the_map_path() {
        let entries = parse_map_cache(GONDOR);
        let empty = mint_sage_str::StringTable::new();
        let maps = maps_from_entries(entries, &empty);
        assert_eq!(maps.len(), 1);
        assert_eq!(maps[0].display_name, maps[0].map_path);
    }
}
