use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use mint_app::Key;
use mint_sage_apt_player::{ImageKey, MovieEvent, Outward, SyncHost, Value};
use mint_sage_apt_viewer::gadget::gadget_key;
use mint_sage_apt_viewer::host::Change;
use mint_sage_save::{
    default_save_name, read_info, sort_newest_first, tab_accepts, GameStateBlock, SaveDate, SaveTab,
};

use crate::apt_overlay::Hosted;
use crate::hud::{CellSpec, EntrySpec, ImageSource, ListSpec};

pub(crate) const MOVIE: &str = "saveload";
pub(crate) const READY: &str = "OnInitialized";

const INIT: &str = "AptSaveLoad::InitGadgets";
const GAME_LIST: &str = "GameList";
const AUTO_LIST: &str = "AutoSaveList";
const ENTRY: &str = "FileNameTextEntry";
const MAX_NAME: usize = 40;
const COLUMNS: [(u8, bool); 4] = [(28, false), (36, false), (19, false), (17, false)];
const MAX_DEPTH: usize = 8;
const MAX_FILE: u64 = 256 << 20;

const TABS: [(&str, u8, Option<SaveTab>); 5] = [
    ("Campaign", 1, Some(SaveTab::Campaign)),
    ("Skirmish", 2, Some(SaveTab::Skirmish)),
    ("Replay", 4, None),
    ("WOTRSP", 8, Some(SaveTab::WotrSingle)),
    ("WOTRMP", 0x10, Some(SaveTab::WotrMulti)),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Save,
    Load,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Exit {
    Closed,
    Load(PathBuf),
    Save { stem: String, description: String },
}

#[derive(Clone)]
pub(crate) struct Env {
    pub(crate) folder: PathBuf,
    pub(crate) new_save: String,
    pub(crate) default_name: String,
    pub(crate) map_name: Rc<dyn Fn(&str) -> String>,
}

pub(crate) fn env(
    vfs: &Arc<mint_vfs::Vfs>,
    strings: &Arc<mint_sage_str::StringTable>,
) -> Option<Env> {
    let folder = crate::user_data::dir()?.join("Save");
    let text = |label: &str, fallback: &str| {
        strings
            .get(label)
            .map_or_else(|| fallback.to_owned(), str::to_owned)
    };
    let memo = RefCell::new(BTreeMap::<String, String>::new());
    let (vfs, names) = (Arc::clone(vfs), Arc::clone(strings));
    let map_name = move |label: &str| {
        memo.borrow_mut()
            .entry(label.to_owned())
            .or_insert_with(|| crate::campaign_save::map_display_name(&vfs, &names, label))
            .clone()
    };
    Some(Env {
        folder,
        new_save: text("GUI:NewSaveGame", "GUI:NewSaveGame"),
        default_name: text("GUI:DefaultSaveFileName", "%d"),
        map_name: Rc::new(map_name),
    })
}

pub(crate) fn game_types(mask: u8) -> String {
    TABS.iter()
        .filter(|(_, bit, _)| mask & bit != 0)
        .map(|(name, _, _)| *name)
        .collect()
}

#[derive(Debug)]
struct Externs {
    mode: Mode,
    mask: u8,
    tab_bit: u8,
    changed: bool,
}

struct ExternHost(Rc<RefCell<Externs>>);

impl SyncHost for ExternHost {
    fn command(&mut self, _outward: &Outward) -> bool {
        false
    }

    fn get_extern(&mut self, name: &str) -> Option<Value> {
        let externs = self.0.try_borrow().ok()?;
        match name {
            "SaveLoadMode" => Some(Value::str(match externs.mode {
                Mode::Save => "Save",
                Mode::Load => "Load",
            })),
            "GameTypes" => Some(Value::str(&game_types(externs.mask))),
            "CurrentGameType" => Some(Value::str("0")),
            _ => None,
        }
    }

    fn set_extern(&mut self, name: &str, value: &Value) -> bool {
        if name != "CurrentGameType" {
            return false;
        }
        let Value::String(text) = value else {
            return false;
        };
        let Some((_, bit, _)) = TABS
            .iter()
            .find(|(tab, _, _)| tab.eq_ignore_ascii_case(text))
        else {
            return false;
        };
        let Ok(mut externs) = self.0.try_borrow_mut() else {
            return false;
        };
        externs.tab_bit = *bit;
        externs.changed = true;
        true
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub(crate) path: PathBuf,
    pub(crate) stem: String,
    pub(crate) retail: bool,
    pub(crate) info: GameStateBlock,
    map_name: String,
}

#[derive(Debug, Default)]
pub(crate) struct Scan {
    pub(crate) game: Vec<Entry>,
    pub(crate) auto: Vec<Entry>,
    pub(crate) file_names: Vec<String>,
}

fn walk(dir: &Path, depth: usize, tab: SaveTab, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort_by_key(|p| p.to_string_lossy().to_lowercase());
    for path in paths {
        if path.is_dir() {
            if depth < MAX_DEPTH {
                walk(&path, depth + 1, tab, out);
            }
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| tab.lists_file(n))
        {
            out.push(path);
        }
    }
}

pub(crate) fn scan(folder: &Path, tab: SaveTab, map_name: &dyn Fn(&str) -> String) -> Scan {
    let mut paths = Vec::new();
    walk(folder, 0, tab, &mut paths);
    let mut scan = Scan::default();
    let mut entries = Vec::new();
    for path in paths {
        let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else {
            continue;
        };
        scan.file_names.push(name.clone());
        if std::fs::metadata(&path).map_or(true, |m| m.len() > MAX_FILE) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(info) = read_info(&bytes) else {
            continue;
        };
        if !tab_accepts(tab, info.save_file_type) {
            continue;
        }
        let retail = name
            .to_ascii_lowercase()
            .ends_with(&tab.extension().to_ascii_lowercase());
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_owned();
        let map_name = map_name(&info.map_label.to_string_lossy());
        entries.push(Entry {
            path,
            stem,
            retail,
            info,
            map_name,
        });
    }
    sort_newest_first(&mut entries, |e| SaveDate(e.info.date));
    for entry in entries {
        if entry.info.is_auto_save == 1 {
            scan.auto.push(entry);
        } else {
            scan.game.push(entry);
        }
    }
    scan
}

fn date_text([year, month, day, ..]: [u16; 8]) -> String {
    format!("{month}/{day}/{year}")
}

fn time_text([_, _, _, _, hour, minute, ..]: [u16; 8]) -> String {
    let suffix = if hour < 12 { "AM" } else { "PM" };
    let hour12 = match hour % 12 {
        0 => 12,
        h => h,
    };
    format!("{hour12}:{minute:02} {suffix}")
}

fn row(entry: &Entry, tab: SaveTab) -> Vec<CellSpec> {
    let text = |s: &str| CellSpec::Text(s.to_owned());
    let (time, date) = (time_text(entry.info.date), date_text(entry.info.date));
    match tab {
        SaveTab::Campaign | SaveTab::Skirmish => vec![
            text(&entry.map_name),
            text(&entry.stem),
            text(&time),
            text(&date),
        ],
        SaveTab::WotrSingle => vec![text(&entry.stem), text(" "), text(&time), text(&date)],
        SaveTab::WotrMulti => vec![
            text(&entry.stem),
            text(&entry.info.extra_a.to_string_lossy()),
            text(&time),
            text(&date),
        ],
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Selected {
    None,
    New,
    Game(usize),
    Auto(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    None,
    Load,
    Save,
    Delete,
}

#[derive(Debug, Default)]
pub(crate) struct Frame {
    pub(crate) exit: Option<Exit>,
    pub(crate) others: Vec<MovieEvent>,
}

pub(crate) struct SaveLoad {
    env: Env,
    externs: Rc<RefCell<Externs>>,
    confirm: bool,
    tab: Option<SaveTab>,
    scan: Scan,
    selected: Selected,
    entry: String,
    pending: Pending,
    closing: Option<Exit>,
    enables: Option<[bool; 3]>,
    calls: Vec<(&'static str, Vec<Value>)>,
    shift: bool,
}

impl SaveLoad {
    pub(crate) fn open(mode: Mode, mask: u8, confirm: bool, env: Env) -> Self {
        let first = TABS
            .iter()
            .find(|(_, bit, _)| mask & bit != 0)
            .map_or(1, |(_, bit, _)| *bit);
        let externs = Rc::new(RefCell::new(Externs {
            mode,
            mask,
            tab_bit: first,
            changed: false,
        }));
        let mut screen = Self {
            env,
            externs,
            confirm,
            tab: None,
            scan: Scan::default(),
            selected: Selected::None,
            entry: String::new(),
            pending: Pending::None,
            closing: None,
            enables: None,
            calls: Vec::new(),
            shift: false,
        };
        screen.refill();
        screen
    }

    pub(crate) fn sync_host(&self) -> Box<dyn SyncHost> {
        Box::new(ExternHost(Rc::clone(&self.externs)))
    }

    fn mode(&self) -> Mode {
        self.externs.borrow().mode
    }

    fn refill(&mut self) {
        let bit = self.externs.borrow().tab_bit;
        self.tab = TABS
            .iter()
            .find(|(_, b, _)| *b == bit)
            .and_then(|(_, _, tab)| *tab);
        self.scan = match self.tab {
            Some(tab) => scan(&self.env.folder, tab, &*self.env.map_name),
            None => Scan::default(),
        };
        self.pending = Pending::None;
        let first = if self.mode() == Mode::Save {
            Selected::New
        } else if self.scan.game.is_empty() {
            Selected::None
        } else {
            Selected::Game(0)
        };
        self.select(first);
    }

    fn entry_of(&self, selected: Selected) -> Option<&Entry> {
        match selected {
            Selected::Game(i) => self.scan.game.get(i),
            Selected::Auto(i) => self.scan.auto.get(i),
            Selected::None | Selected::New => None,
        }
    }

    fn select(&mut self, selected: Selected) {
        self.selected = selected;
        if self.mode() != Mode::Save {
            return;
        }
        self.entry = match selected {
            Selected::New => self.default_name(),
            Selected::Game(_) | Selected::Auto(_) => self
                .entry_of(selected)
                .map(|e| e.stem.clone())
                .unwrap_or_default(),
            Selected::None => String::new(),
        };
    }

    fn default_name(&self) -> String {
        let Some(tab) = self.tab else {
            return String::new();
        };
        let names: Vec<&str> = self.scan.file_names.iter().map(String::as_str).collect();
        default_save_name(&self.env.default_name, tab.mint_extension(), &names)
    }

    fn save_path(&self, stem: &str) -> Option<PathBuf> {
        let tab = self.tab?;
        Some(self.env.folder.join(format!(
            "{}{}",
            crate::campaign_save::sanitised(stem),
            tab.mint_extension()
        )))
    }

    fn call(&mut self, function: &'static str, args: Vec<Value>) {
        self.calls.push((function, args));
    }

    fn begin_close(&mut self, exit: Exit) {
        self.closing = Some(exit);
        self.call("closeDelayed", vec![Value::str("OnClosed")]);
    }

    fn selected_entry(&self) -> Option<&Entry> {
        self.entry_of(self.selected)
    }

    fn start_save(&mut self) {
        let description = self.entry.clone();
        let stem = description.trim().to_owned();
        let stem = if stem.is_empty() {
            self.default_name()
        } else {
            stem
        };
        let exists = self.save_path(&stem).is_some_and(|p| p.exists());
        if exists && self.pending != Pending::Save {
            self.pending = Pending::Save;
            self.call("showMessageBox", vec![Value::str("Save")]);
            return;
        }
        self.pending = Pending::None;
        self.begin_close(Exit::Save { stem, description });
    }

    pub(crate) fn command(&mut self, name: &str) -> Option<Exit> {
        match name {
            "Load" => {
                if self.pending != Pending::None {
                    return None;
                }
                let path = self.selected_entry()?.path.clone();
                if self.confirm {
                    self.pending = Pending::Load;
                    self.call("showMessageBox", vec![Value::str("Load")]);
                } else {
                    self.begin_close(Exit::Load(path));
                }
            }
            "Save" => {
                if self.pending == Pending::None {
                    self.start_save();
                }
            }
            "Delete" => {
                if self.pending == Pending::None && self.selected_entry().is_some_and(|e| !e.retail)
                {
                    self.pending = Pending::Delete;
                    self.call("showMessageBox", vec![Value::str("Delete")]);
                }
            }
            "ConfirmationOk" => match std::mem::replace(&mut self.pending, Pending::None) {
                Pending::Load => {
                    if let Some(path) = self.selected_entry().map(|e| e.path.clone()) {
                        self.begin_close(Exit::Load(path));
                    }
                }
                Pending::Save => {
                    self.pending = Pending::Save;
                    self.start_save();
                }
                Pending::Delete => {
                    if let Some(entry) = self.selected_entry().filter(|e| !e.retail) {
                        let path = entry.path.clone();
                        if let Err(error) = std::fs::remove_file(&path) {
                            println!("save/load: could not delete {}: {error}", path.display());
                        }
                        self.refill();
                    }
                }
                Pending::None => {}
            },
            "ConfirmationCancel" => self.pending = Pending::None,
            "Cancel" => return Some(Exit::Closed),
            "OnClosed" => return Some(self.closing.take().unwrap_or(Exit::Closed)),
            _ => {}
        }
        None
    }

    pub(crate) fn modal(&self) -> bool {
        self.pending != Pending::None
    }

    pub(crate) fn escape(&mut self) {
        if self.pending == Pending::None && self.closing.is_none() {
            self.call("closeDelayed", vec![Value::str("Cancel")]);
        }
    }

    pub(crate) fn select_game_row(&mut self, row: usize) {
        let offset = usize::from(self.mode() == Mode::Save);
        if row < offset {
            self.select(Selected::New);
        } else if row - offset < self.scan.game.len() {
            self.select(Selected::Game(row - offset));
        }
    }

    pub(crate) fn select_auto_row(&mut self, row: usize) {
        if row < self.scan.auto.len() {
            self.select(Selected::Auto(row));
        }
    }

    pub(crate) fn gadget_changed(&mut self, key: &str, change: Change) {
        if let Change::Row(row) = change {
            if key == gadget_key(INIT, GAME_LIST) {
                self.select_game_row(row);
            } else if key == gadget_key(INIT, AUTO_LIST) {
                self.select_auto_row(row);
            }
        }
    }

    pub(crate) fn key(&mut self, key: Key, pressed: bool) {
        if key == Key::Shift {
            self.shift = pressed;
            return;
        }
        if !pressed {
            return;
        }
        match key {
            Key::Escape => self.escape(),
            Key::Enter if self.mode() == Mode::Save => {
                self.command("Save");
            }
            Key::Backspace if self.mode() == Mode::Save => {
                self.entry.pop();
            }
            Key::Space if self.mode() == Mode::Save => self.type_text(" "),
            Key::Character(c) if self.mode() == Mode::Save => {
                let text = if self.shift {
                    c.to_uppercase().to_string()
                } else {
                    c.to_string()
                };
                self.type_text(&text);
            }
            _ => {}
        }
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            if self.entry.chars().count() < MAX_NAME && !c.is_control() {
                self.entry.push(c);
            }
        }
    }

    fn enables(&self) -> [bool; 3] {
        let mode = self.mode();
        let entry = self.selected_entry();
        [
            mode == Mode::Save && !self.entry.trim().is_empty(),
            mode == Mode::Load && entry.is_some(),
            entry.is_some_and(|e| !e.retail),
        ]
    }

    fn list_spec(&self, auto: bool) -> ListSpec {
        let Some(tab) = self.tab else {
            return ListSpec {
                columns: COLUMNS.to_vec(),
                ..ListSpec::default()
            };
        };
        let save = self.mode() == Mode::Save;
        let entries = if auto {
            &self.scan.auto
        } else {
            &self.scan.game
        };
        let mut rows = Vec::new();
        if save && !auto {
            rows.push(vec![CellSpec::Text(self.env.new_save.clone())]);
        }
        rows.extend(entries.iter().map(|e| row(e, tab)));
        let offset = usize::from(save && !auto);
        let selected = match (self.selected, auto) {
            (Selected::New, false) => Some(0),
            (Selected::Game(i), false) | (Selected::Auto(i), true) => Some(i + offset),
            _ => None,
        };
        ListSpec {
            columns: COLUMNS.to_vec(),
            rows,
            selected,
            ..ListSpec::default()
        }
    }

    pub(crate) fn frame(&mut self, hosted: &mut Hosted, dt: std::time::Duration) -> Frame {
        let changed = std::mem::take(&mut self.externs.borrow_mut().changed);
        if changed {
            self.refill();
        }
        hosted.expose.image(
            ImageKey::Name(gadget_key(INIT, GAME_LIST)),
            Some(ImageSource::List(Arc::new(self.list_spec(false)))),
        );
        hosted.expose.image(
            ImageKey::Name(gadget_key(INIT, AUTO_LIST)),
            Some(ImageSource::List(Arc::new(self.list_spec(true)))),
        );
        let entry = (self.mode() == Mode::Save).then(|| {
            ImageSource::Entry(Arc::new(EntrySpec {
                text: self.entry.clone(),
                focused: true,
            }))
        });
        hosted
            .expose
            .image(ImageKey::Name(gadget_key(INIT, ENTRY)), entry);
        let enables = self.enables();
        if self.enables != Some(enables) {
            for (function, on) in [
                ("SaveButtonEnable", enables[0]),
                ("LoadButtonEnable", enables[1]),
                ("DeleteButtonEnable", enables[2]),
            ] {
                self.call(function, vec![Value::str(if on { "1" } else { "0" })]);
            }
            self.enables = Some(enables);
        }
        for (function, args) in std::mem::take(&mut self.calls) {
            hosted.expose.fire(function, args);
        }
        hosted.frame(dt);
        let mut frame = Frame::default();
        for event in hosted.expose.take_events() {
            match event.command() {
                Some(
                    name @ ("Load" | "Save" | "Delete" | "ConfirmationOk" | "ConfirmationCancel"
                    | "Cancel" | "OnClosed"),
                ) => {
                    println!("save/load: AptSaveLoad::{name}");
                    if let Some(exit) = self.command(name) {
                        frame.exit.get_or_insert(exit);
                    }
                }
                Some("OnInitialized" | "InitGadgets") => {}
                _ => frame.others.push(event),
            }
        }
        frame
    }
}

#[cfg(test)]
mod tests;
