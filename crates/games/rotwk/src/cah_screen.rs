use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use mint_sage_apt_player::{Exposure, ImageKey, Outward, SyncHost, Value};
use mint_sage_asset::{HeroSource, HeroStore};
use mint_sage_str::StringTable;

use crate::cah_edit::{
    confirm_delete, name_is_valid, request_delete, save_new, save_over, AutoChange, CahSystem,
    CellState, ExamineApply, HeroEdit, PendingDelete, PowerSelect, SlotKind, WizardLine,
    WizardSuffix,
};
use crate::cah_examine::ExamineParams;
use crate::cah_picture::{picture_stem, PictureRequest};
use crate::cah_view::{CameraMode, EditView, Held, ViewSet};
use crate::hud::{CellSpec, ImageSource, ListSpec};
use crate::list_sort::{name_order, ColumnSort, SortMode};
use crate::shell::message_box::{BoxButton, BoxKind, BoxRequest};

const MAX_ATTRIBUTE: u32 = 20;
const FALLBACK_POWER_ICON: &str = "CircleRed_42x42";
const COLUMNS: usize = 4;
const MAX_PENDING_APPLIES: usize = 64;
const STATE_LABELS: [&str; 7] = [
    "_unused",
    "_avail",
    "_needPrereq",
    "_levelLow",
    "_selected",
    "_palantirFull",
    "_palantirFull",
];
const PICKS: usize = 10;
const SORT_ICON: usize = 0;
const SORT_NAME: usize = 1;
const SORT_TYPE: usize = 2;
const UNSORTED_HEROES: ColumnSort = ColumnSort::new(
    SortMode::ascending(SORT_ICON),
    SortMode::ascending(SORT_ICON),
);

#[derive(PartialEq, Eq)]
enum HeroSortKey {
    System(bool),
    Name(String),
    Type(u64),
}

impl PartialOrd for HeroSortKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HeroSortKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Self::System(a), Self::System(b)) => a.cmp(b),
            (Self::Name(a), Self::Name(b)) => name_order(a, b),
            (Self::Type(a), Self::Type(b)) => a.cmp(b),
            _ => std::cmp::Ordering::Equal,
        }
    }
}
const HERO_LIST_COLUMNS: [(u8, bool); 3] = [(9, false), (46, false), (45, false)];
const STATS_LIST_COLUMNS: [(u8, bool); 2] = [(80, false), (20, false)];
const OWN_HERO_MARKER: &str = "AptUserMapMaxConquered";
const OWN_HERO_MARKER_PERCENT: u8 = 80;
pub(crate) const HERO_LIST: (&str, &str) = ("CahManager::InitGadgets", "SavedHeroList");
pub(crate) const STATS_LIST: (&str, &str) = ("CahManager::InitGadgets", "SavedHeroStats");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Page {
    None,
    NewFeatures,
    Manager,
    Class,
    Appearance,
    Powers,
}

impl Page {
    fn named(name: &str) -> Self {
        match name {
            "NewFeatures" => Self::NewFeatures,
            "Manager" => Self::Manager,
            "Class" => Self::Class,
            "Appearance" => Self::Appearance,
            "Powers" => Self::Powers,
            _ => Self::None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Request {
    MainMenu,
}

struct Fire {
    function: &'static str,
    args: Vec<Value>,
}

pub(crate) struct CahState {
    sys: Arc<CahSystem>,
    strings: Arc<StringTable>,
    store: HeroStore,
    edit: Option<HeroEdit>,
    page: Page,
    previous: Page,
    selected: Option<usize>,
    hero_sort: Option<ColumnSort>,
    name: String,
    focused: bool,
    suppress_promo: bool,
    pending_delete: Option<PendingDelete>,
    awards_dirty: bool,
    boxes: Vec<BoxRequest>,
    rolled_over: Option<usize>,
    award_description: String,
    stats: (Option<String>, BTreeMap<String, i32>),
    id_seed: u64,
    to_remove: Vec<String>,
    power_error: Option<String>,
    fires: Vec<Fire>,
    request: Option<Request>,
    log: Vec<String>,
    seen_externs: std::collections::BTreeSet<String>,
    view: EditView,
    held: Held,
    shift: bool,
    swapped_to: Option<(usize, usize, Option<usize>)>,
    cheer: bool,
    picture: PictureRequest,
    applies: Vec<ExamineApply>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Preview {
    pub(crate) class: usize,
    pub(crate) subclass: usize,
    pub(crate) set: ViewSet,
    pub(crate) view: EditView,
    pub(crate) on_powers: bool,
    pub(crate) upgrades: Vec<String>,
    pub(crate) cheer: bool,
    pub(crate) examine: Option<(Vec<ExamineApply>, ExamineParams)>,
}

pub(crate) type Shared = Rc<RefCell<CahState>>;

pub(crate) struct CahHost(pub(crate) Shared);

impl SyncHost for CahHost {
    fn command(&mut self, outward: &Outward) -> bool {
        let (command, args) = match outward {
            Outward::FsCommand { command, args } => (command.clone(), args.clone()),
            Outward::GameCode { args, .. } => {
                let mut it = args.iter();
                let command = it.next().cloned().unwrap_or_default();
                (command, it.cloned().collect::<Vec<_>>().join(","))
            }
            Outward::GetUrl { .. } => return false,
        };
        if command.rsplit("::").next() == Some(crate::apt_overlay::PLAY_SOUND) {
            return false;
        }
        let Ok(mut state) = self.0.try_borrow_mut() else {
            return false;
        };
        state.handle(&command, &args);
        true
    }

    fn set_extern(&mut self, name: &str, value: &Value) -> bool {
        let Ok(mut state) = self.0.try_borrow_mut() else {
            return false;
        };
        if name.rsplit("::").next() == Some("SuppressCAHPromo") {
            state.suppress_promo = match value {
                Value::String(text) => text.eq_ignore_ascii_case("true") || value.to_bool(),
                other => other.to_bool(),
            };
            let line = format!("cah: SetExtern({name}) = {value:?}");
            state.log.push(line);
            return true;
        }
        false
    }

    fn get_extern(&mut self, name: &str) -> Option<Value> {
        let mut state = self.0.try_borrow_mut().ok()?;
        let value = state.extern_value(name);
        if state.seen_externs.insert(format!("{name}={value:?}")) {
            state
                .log
                .push(format!("cah: GetExtern({name}) = {value:?}"));
        }
        value
    }
}

fn cheer_on_enter(page: Page, previous: Page, cheer: bool) -> bool {
    match page {
        Page::Appearance if previous == Page::Class => true,
        Page::Class => false,
        _ => cheer,
    }
}

fn atoi(text: &str) -> i64 {
    let t = text.trim();
    let end = t
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+'))))
        .map_or(t.len(), |(i, _)| i);
    t.get(..end).and_then(|s| s.parse().ok()).unwrap_or(0)
}

fn atoi_wrapping(text: &str) -> u32 {
    let text = text.trim_start();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let n = digits
        .chars()
        .map_while(|c| c.to_digit(10))
        .fold(0u32, |n, d| n.wrapping_mul(10).wrapping_add(d));
    if negative {
        n.wrapping_neg()
    } else {
        n
    }
}

fn uatoi(text: &str) -> Option<usize> {
    usize::try_from(atoi(text)).ok()
}

fn suffix_index(name: &str, prefix: &str) -> Option<usize> {
    name.strip_prefix(prefix)?.parse().ok()
}

fn str_value(text: impl AsRef<str>) -> Option<Value> {
    Some(Value::str(text.as_ref()))
}

fn number(n: usize) -> Value {
    Value::Number(u32::try_from(n).map_or(f64::from(u32::MAX), f64::from))
}

fn is_native(command: &str) -> bool {
    !command
        .strip_prefix("AptCreateAHero::")
        .unwrap_or(command)
        .contains("::")
}

pub(crate) fn clock_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| {
            u64::try_from(d.as_nanos() & u128::from(u64::MAX)).unwrap_or(1)
        })
}

fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn fresh_id(seed: &mut u64, taken: impl Fn(&str) -> bool) -> String {
    loop {
        let mut guid = [0u8; 16];
        for half in guid.chunks_mut(8) {
            let bytes = splitmix(seed).to_le_bytes();
            half.copy_from_slice(&bytes);
        }
        let id = mint_sage_cah::id_from_guid(guid);
        if !taken(&id) {
            return id;
        }
    }
}

const MAX_NAME_CHARS: usize = 22;

fn state_tooltip(state: CellState) -> &'static str {
    match state {
        CellState::NeedsPrereq => "TOOLTIP:CAH_NEEDS_PREREQ",
        CellState::LevelTooLow => "TOOLTIP:CAH_LEVEL_TOO_LOW",
        CellState::Selected => "TOOLTIP:CAH_IS_SELECTED",
        CellState::PalantirFull => "TOOLTIP:CAH_PALANTIR_FULL",
        CellState::BookFull => "TOOLTIP:CAH_POWER_BOOK_FULL",
        CellState::Available | CellState::NoButton => "TOOLTIP:CAH_IS_AVAILABLE",
    }
}

fn slot_field(
    edit: Option<&HeroEdit>,
    kind: SlotKind,
    index: usize,
    f: impl Fn(&crate::cah_edit::Slot) -> i32,
) -> String {
    edit.and_then(|e| e.slot(kind, index))
        .map_or_else(|| "0".to_owned(), |s| f(s).to_string())
}

impl CahState {
    pub(crate) fn open(
        sys: Arc<CahSystem>,
        strings: Arc<StringTable>,
        store: HeroStore,
        suppress_promo: bool,
    ) -> Shared {
        Rc::new(RefCell::new(Self {
            sys,
            strings,
            store,
            edit: None,
            page: Page::None,
            previous: Page::None,
            selected: None,
            hero_sort: None,
            name: String::new(),
            focused: false,
            suppress_promo,
            pending_delete: None,
            awards_dirty: false,
            boxes: Vec::new(),
            rolled_over: None,
            award_description: String::new(),
            stats: (None, BTreeMap::new()),
            id_seed: clock_seed(),
            to_remove: Vec::new(),
            power_error: None,
            fires: Vec::new(),
            request: None,
            log: Vec::new(),
            seen_externs: std::collections::BTreeSet::new(),
            view: EditView::new(ViewSet::from_def(None).normal_cam),
            held: Held::default(),
            shift: false,
            swapped_to: None,
            cheer: false,
            picture: PictureRequest::default(),
            applies: Vec::new(),
        }))
    }

    pub(crate) fn close(&mut self) {
        for id in std::mem::take(&mut self.to_remove) {
            if let Err(e) = self.store.remove_file(&id) {
                println!("cah: could not remove hero file {id}: {e}");
            }
        }
    }

    pub(crate) fn suppress_promo(&self) -> bool {
        self.suppress_promo
    }

    pub(crate) fn take_request(&mut self) -> Option<Request> {
        self.request.take()
    }

    pub(crate) fn take_boxes(&mut self) -> Vec<BoxRequest> {
        std::mem::take(&mut self.boxes)
    }

    pub(crate) fn answer_box(&mut self, button: BoxButton) {
        match button {
            BoxButton::Yes => {
                self.answer_delete(true);
            }
            BoxButton::No | BoxButton::Cancel => {
                self.answer_delete(false);
            }
            BoxButton::Ok => {}
        }
    }

    fn message(&mut self, kind: BoxKind, title: &str, text: &str) {
        let request = BoxRequest {
            kind,
            title: self.loc(title),
            text: self.loc(text),
        };
        self.boxes.push(request);
    }

    pub(crate) fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }

    pub(crate) fn select_row(&mut self, row: usize) {
        if let Some(&index) = self.hero_order().get(row) {
            self.selected = Some(index);
            self.load_selected();
            self.selection_applied();
            self.slots_rebuilt();
        }
    }

    pub(crate) fn type_text(&mut self, text: &str) {
        if self.page == Page::Appearance {
            for c in text.chars() {
                if self.name.chars().count() < MAX_NAME_CHARS {
                    self.name.push(c);
                }
            }
            self.focused = true;
        }
    }

    pub(crate) fn backspace(&mut self) {
        if self.page == Page::Appearance {
            self.name.pop();
            self.focused = true;
        }
    }

    pub(crate) fn answer_delete(&mut self, yes: bool) -> bool {
        let Some(pending) = self.pending_delete.take() else {
            return false;
        };
        if yes {
            if let Some(id) = confirm_delete(&mut self.store, pending) {
                self.log.push(format!("cah: deleted hero {id}"));
                self.to_remove.push(id);
            }
            self.selected = None;
            self.fix_selection();
            self.awards_dirty = true;
        }
        true
    }

    fn view_set(&self) -> ViewSet {
        let (class, subclass) = self.edit.as_ref().map_or((0, 0), |e| e.class_subclass());
        ViewSet::from_def(
            self.sys
                .subclass(class, subclass)
                .and_then(|sub| sub.view_info.as_ref()),
        )
    }

    pub(crate) fn set_shift(&mut self, down: bool) {
        self.shift = down;
    }

    pub(crate) fn set_arrow(&mut self, key: mint_app::Key, down: bool) {
        match key {
            mint_app::Key::ArrowLeft => self.held.rotate_left = down,
            mint_app::Key::ArrowRight => self.held.rotate_right = down,
            mint_app::Key::ArrowUp => self.held.zoom_in = down,
            mint_app::Key::ArrowDown => self.held.zoom_out = down,
            _ => {}
        }
    }

    pub(crate) fn tick_view(&mut self, update_due: bool) {
        let key = self
            .edit
            .as_ref()
            .map(|e| e.class_subclass())
            .map(|(c, s)| (c, s, self.selected));
        if key != self.swapped_to {
            self.swapped_to = key;
            self.view.reset(self.view_set().normal_cam);
        }
        if update_due {
            self.view.step(self.held, self.shift);
        }
    }

    pub(crate) fn preview(&mut self) -> Option<Preview> {
        let applies = std::mem::take(&mut self.applies);
        let edit = self.edit.as_ref()?;
        let (class, subclass) = edit.class_subclass();
        Some(Preview {
            class,
            subclass,
            set: self.view_set(),
            view: self.view,
            on_powers: self.page == Page::Powers,
            upgrades: edit.dress_upgrades(),
            cheer: self.cheer,
            examine: (!applies.is_empty())
                .then(|| (applies, ExamineParams::from_def(self.sys.def()))),
        })
    }

    pub(crate) fn guard_drawn(&mut self) {
        self.picture.guard_drawn();
    }

    pub(crate) fn take_picture_request(&mut self, unix_seconds: u64) -> Option<String> {
        if !self.picture.take() {
            return None;
        }
        let name = self.edit.as_ref().map(HeroEdit::name).unwrap_or_default();
        Some(picture_stem(&name, unix_seconds))
    }

    pub(crate) fn picture_taken(&mut self, path: &str) {
        self.fire("OnGameTookPicture", Vec::new());
        let text = self.loc("GUI:OnPictureTaken").replace("%s", path);
        self.log.push(format!("cah: {text}"));
    }

    fn loc(&self, key: &str) -> String {
        crate::shell::localised(&self.strings, key)
    }

    fn loc_opt(&self, key: Option<&str>) -> String {
        key.map(|k| self.loc(k)).unwrap_or_default()
    }

    fn fix_selection(&mut self) {
        let count = self.store.heroes().len();
        self.selected = match self.selected {
            Some(i) if i < count => Some(i),
            _ if count > 0 => Some(0),
            _ => None,
        };
        self.load_selected();
        self.selection_applied();
    }

    fn selection_applied(&mut self) {
        if self.selected.is_none() {
            return;
        }
        self.awards_dirty = true;
        let enable = !self.selected_is_system();
        self.fire("EnableCustomizeButtons", vec![Value::Bool(enable)]);
    }

    fn load_selected(&mut self) {
        self.edit = self
            .selected
            .and_then(|i| self.store.heroes().get(i))
            .map(|h| HeroEdit::from_record(Arc::clone(&self.sys), h.record.clone()));
    }

    fn selected_is_system(&self) -> bool {
        self.selected
            .and_then(|i| self.store.heroes().get(i))
            .is_some_and(|h| h.source == HeroSource::System)
    }

    fn came_from_manager(&self) -> bool {
        self.previous == Page::Manager
    }

    fn fire(&mut self, function: &'static str, args: Vec<Value>) {
        self.fires.push(Fire { function, args });
    }

    fn cost(&self) -> u32 {
        self.edit
            .as_ref()
            .map_or(0, |e| e.build_cost(self.sys.base_cost()))
    }

    fn refresh_preview(&mut self) {
        if let Some(edit) = self.edit.as_mut() {
            if self.applies.len() < MAX_PENDING_APPLIES {
                self.applies.push(edit.take_examine());
            }
        }
    }

    fn after_edit(&mut self) {
        self.refresh_preview();
        let cost = f64::from(self.cost());
        let zero = || Value::Number(0.0);
        let args = vec![Value::Number(cost), zero(), zero(), zero(), zero()];
        self.fire("UpdateHeroBuildCost", args);
    }

    fn slots_rebuilt(&mut self) {
        self.fire("UpdateHeroBaseAttributes", Vec::new());
        self.after_edit();
    }

    fn picked(&mut self) {
        let count = self.edit.as_ref().map_or(0, HeroEdit::power_count);
        self.fire("SetPowersSelectedNum", vec![number(count)]);
        self.after_edit();
        self.wizard_changed();
    }

    fn wizard_changed(&mut self) {
        if self
            .edit
            .as_ref()
            .is_some_and(HeroEdit::any_power_available)
        {
            self.fire("OnNewWizardMessage", Vec::new());
        }
    }

    fn handle(&mut self, command: &str, args: &str) {
        let name = command.rsplit("::").next().unwrap_or(command);
        self.log.push(format!("cah: {command}({args})"));
        match name {
            "OnShowScreen" => self.show_screen(Page::named(args.trim())),
            "OnSortIcon" => self.sort_heroes(SORT_ICON),
            "OnSortName" => self.sort_heroes(SORT_NAME),
            "OnSortType" => self.sort_heroes(SORT_TYPE),
            "SetClassAndType" => self.set_class_and_type(args),
            "Exit" => self.request = Some(Request::MainMenu),
            "NextAppearance" | "PrevAppearance" => {
                let delta = if name == "NextAppearance" { 1 } else { -1 };
                if let (Some(e), Some(i)) = (self.edit.as_mut(), uatoi(args)) {
                    e.step_appearance(i, delta);
                }
                self.after_edit();
            }
            "IncreaseAttribute" | "DecreaseAttribute" => {
                let delta = if name == "IncreaseAttribute" { 1 } else { -1 };
                if let (Some(e), Some(i)) = (self.edit.as_mut(), uatoi(args)) {
                    e.step_attribute(i, delta);
                }
                self.after_edit();
            }
            "AutoChangeBttn" => self.auto_change(args.trim()),
            "NamePrompt" => {
                let text = if self.name.trim().is_empty() {
                    "APT:EnterNameError"
                } else {
                    "LAN:ErrorDuplicateName"
                };
                self.message(BoxKind::Ok, "APT:EnterNameErrorTitle", text);
            }
            "OnComplete" => self.complete(true),
            "OnPowerSelectionComplete" => self.complete(false),
            "OnDeleteHero" => {
                self.pending_delete = self.selected.and_then(|i| request_delete(&self.store, i));
                if self.pending_delete.is_some() {
                    self.message(BoxKind::YesNo, "GUI:DeleteFile", "GUI:AreYouSureDelete");
                }
            }
            "OnPowerSelect" => self.power_select(args),
            "OnNoPowerSelect" => {
                if let Some(e) = self.edit.as_mut() {
                    if matches!(e.select_no_power(), PowerSelect::Committed { .. }) {
                        self.picked();
                    }
                }
            }
            "OnMyPowerSelect" | "OnResetBttn" => {
                if let Some(e) = self.edit.as_mut() {
                    if name == "OnResetBttn" {
                        e.reset_powers();
                    } else if let Some(n) = uatoi(args) {
                        e.undo_power(n);
                    }
                    self.picked();
                }
            }
            "OnSelectAward" => {
                let award = u32::try_from(atoi(args))
                    .ok()
                    .and_then(|i| usize::try_from(i).ok())
                    .and_then(|i| self.subclass_award(i));
                self.award_description =
                    self.loc_opt(award.and_then(|a| a.description_tag.as_deref()));
            }
            "OnMyPowerRollOver" => self.rolled_over = uatoi(args).and_then(|n| n.checked_sub(1)),
            "OnMyPowerRollOut" => {
                if uatoi(args).and_then(|n| n.checked_sub(1)) == self.rolled_over {
                    self.rolled_over = None;
                }
            }
            "RotateLeft" | "RotateRight" | "ZoomIn" | "ZoomOut" if is_native(command) => {
                let down = args.trim_start().starts_with('t');
                match name {
                    "RotateLeft" => self.held.rotate_left = down,
                    "RotateRight" => self.held.rotate_right = down,
                    "ZoomIn" => self.held.zoom_in = down,
                    _ => self.held.zoom_out = down,
                }
            }
            "PrepareToTakePicture" => self.picture.prepare(),
            "OnTakePicture" => self.picture.ask(),
            "OnHairColor" | "OnSkinColor" | "OnPaintColor" => {
                let index = match name {
                    "OnHairColor" => 0,
                    "OnSkinColor" => 1,
                    _ => 2,
                };
                if let Some(e) = self.edit.as_mut() {
                    e.set_colour(index, atoi_wrapping(args));
                }
                self.refresh_preview();
            }
            _ => {}
        }
    }

    fn set_class_and_type(&mut self, args: &str) {
        let mut it = args.split_whitespace();
        let class = it.next().map_or(0, atoi);
        let sub = it.next().map_or(0, atoi);
        let (Ok(c), Ok(s)) = (u32::try_from(class), u32::try_from(sub)) else {
            return;
        };
        match self.edit.as_mut() {
            Some(e) => {
                e.set_class_and_type(c, s);
            }
            None => self.edit = HeroEdit::new_hero(Arc::clone(&self.sys), c, s),
        }
        self.slots_rebuilt();
    }

    fn auto_change(&mut self, which: &str) {
        let Some(e) = self.edit.as_mut() else {
            return;
        };
        match which {
            "AppearanceDefault" => e.auto_change(AutoChange::Default),
            "AppearanceRandom" => {
                let mut seed = clock_seed();
                let mut rand = |lo: i32, hi: i32| {
                    seed = seed
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1_442_695_040_888_963_407);
                    let span = u64::try_from(hi.saturating_sub(lo)).unwrap_or(0) + 1;
                    lo.saturating_add(i32::try_from((seed >> 33) % span).unwrap_or(0))
                };
                e.auto_change(AutoChange::Random(&mut rand));
            }
            "AttribReset" => e.auto_change(AutoChange::Reset),
            "AttribRecommend" => e.auto_change(AutoChange::Recommend),
            _ => {}
        }
        self.after_edit();
    }

    fn power_select(&mut self, args: &str) {
        let mut it = args.split(',');
        let row = it.next().map_or(0, atoi).saturating_sub(1);
        let col = it.next().map_or(0, atoi).saturating_sub(1);
        let (Ok(r), Ok(c)) = (usize::try_from(row), usize::try_from(col)) else {
            return;
        };
        let Some(e) = self.edit.as_mut() else {
            return;
        };
        match e.select_power(r, c) {
            PowerSelect::Committed { flash } => {
                self.power_error = None;
                if let Some(slot) = flash {
                    let slot = usize::try_from(slot).unwrap_or(0);
                    self.fire("FlashPalantir", vec![number(slot)]);
                }
                self.picked();
            }
            PowerSelect::Rejected(state) => {
                self.power_error = Some(self.loc(state_tooltip(state)));
                self.fire("ShowPowerErrorMessage", Vec::new());
            }
            PowerSelect::Ignored => {}
        }
    }

    fn show_screen(&mut self, page: Page) {
        self.rolled_over = None;
        self.previous = self.page;
        self.page = page;
        self.view.mode = match page {
            Page::Class | Page::Appearance => CameraMode::Free,
            _ => CameraMode::Fixed,
        };
        self.cheer = cheer_on_enter(page, self.previous, self.cheer);
        match page {
            Page::Manager => {
                self.fix_selection();
                self.write_my_hero();
                self.awards_dirty = true;
                self.name.clear();
                self.slots_rebuilt();
            }
            Page::Appearance | Page::Powers => {
                if self.edit.is_none() {
                    self.edit = HeroEdit::new_hero(Arc::clone(&self.sys), 0, 0);
                }
                if page == Page::Appearance {
                    self.name = self.edit.as_ref().map(HeroEdit::name).unwrap_or_default();
                    self.focused = false;
                } else {
                    if let Some(e) = self.edit.as_mut() {
                        e.open_powers();
                    }
                    self.power_error = None;
                }
                self.slots_rebuilt();
                if page == Page::Powers {
                    self.wizard_changed();
                }
            }
            Page::Class => {
                if self.previous == Page::Manager {
                    self.set_class_and_type("0 0");
                }
            }
            Page::None | Page::NewFeatures => {}
        }
    }

    fn write_my_hero(&mut self) {
        let Some(hero) = self.selected.and_then(|i| self.store.heroes().get(i)) else {
            return;
        };
        match self.store.write_my_hero(&hero.record) {
            Ok(true) => self.log.push(format!(
                "cah: MyHero.dat written for hero {}",
                hero.record.id_str().unwrap_or_default()
            )),
            Ok(false) => {}
            Err(e) => self.log.push(format!("cah: MyHero.dat not written: {e}")),
        }
    }

    fn new_id(&mut self) -> String {
        let taken: Vec<String> = self
            .store
            .heroes()
            .iter()
            .filter_map(|h| h.record.id_str())
            .collect();
        fresh_id(&mut self.id_seed, |id| taken.iter().any(|t| t == id))
    }

    fn name_valid(&self) -> bool {
        name_is_valid(&self.name, &self.store)
    }

    fn complete(&mut self, from_appearance: bool) {
        if from_appearance {
            if !self.name_valid() {
                return;
            }
            let name = self.name.clone();
            let has_id = self.edit.as_ref().and_then(HeroEdit::id).is_some();
            let id = (!has_id).then(|| self.new_id());
            if let Some(e) = self.edit.as_mut() {
                e.set_name(&name);
                if let Some(id) = id {
                    e.set_id(&id);
                }
            }
        }
        if self.edit.is_none() {
            return;
        }
        let id = (!self.came_from_manager() && !from_appearance).then(|| {
            match self.edit.as_ref().and_then(HeroEdit::id) {
                Some(id) => id,
                None => self.new_id(),
            }
        });
        let Some(edit) = self.edit.as_ref() else {
            return;
        };
        let result = if self.came_from_manager() {
            let Some(i) = self.selected else { return };
            save_over(&mut self.store, edit, i)
                .map(|saved| format!("saved over hero {i} ({saved})"))
        } else if from_appearance {
            return;
        } else {
            let Some(id) = id else { return };
            save_new(&mut self.store, edit, &id).map(|()| format!("saved new hero {id}"))
        };
        let count = self.store.heroes().len();
        match result {
            Ok(line) => {
                self.awards_dirty = true;
                self.log.push(format!("cah: {line}; store now {count}"));
            }
            Err(e) => self.log.push(format!("cah: save failed: {e}")),
        }
    }

    fn extern_value(&self, name: &str) -> Option<Value> {
        let short = name.rsplit("::").next().unwrap_or(name);
        let edit = self.edit.as_ref();
        if let Some(i) = suffix_index(short, "BaseAttrib_") {
            return str_value(slot_field(edit, SlotKind::Attribute, i, |s| {
                s.min.saturating_add(1)
            }));
        }
        if let Some(i) = suffix_index(short, "CurAttrib_") {
            return str_value(slot_field(edit, SlotKind::Attribute, i, |s| {
                s.cur.saturating_add(1)
            }));
        }
        if let Some(i) = suffix_index(short, "MaxAttrib_") {
            return str_value(slot_field(edit, SlotKind::Attribute, i, |s| {
                s.max.saturating_add(1)
            }));
        }
        if let Some(i) = suffix_index(short, "NumAppearance_") {
            let n = edit
                .and_then(|e| e.slot(SlotKind::Appearance, i))
                .map_or(0, crate::cah_edit::Slot::count);
            return str_value(n.to_string());
        }
        if let Some(i) = suffix_index(short, "AwardState_") {
            let state = edit.and_then(|e| e.award_state(i, &self.stats.1));
            return str_value(match state {
                None => "",
                Some(true) => "1",
                Some(false) => "0",
            });
        }
        let flag = |on: bool| str_value(if on { "1" } else { "0" });
        match short {
            "MaxAttribute" => str_value(MAX_ATTRIBUTE.to_string()),
            "MaxAwards" => str_value(self.sys.awards().len().to_string()),
            "IsSystemHero" => flag(self.page != Page::Class && self.selected_is_system()),
            "HeroBuildCost" => str_value(self.cost().to_string()),
            "NumClasses" => str_value(self.sys.def().classes.len().to_string()),
            "NumClassTypes" => str_value(self.class_types().to_string()),
            "SuppressCAHPromo" => flag(self.suppress_promo),
            "HairColor" | "SkinColor" | "PaintColor" => {
                let index = match short {
                    "HairColor" => 0,
                    "SkinColor" => 1,
                    _ => 2,
                };
                edit.and_then(|e| e.record().colors.get(index))
                    .and_then(|c| str_value(c.to_string()))
            }
            "HeroNameSet" => flag(self.name_valid()),
            "ShowNamePrompt" => flag(self.name.is_empty() && !self.focused),
            "NumPowerRows" => str_value(edit.map_or(0, HeroEdit::power_rows).to_string()),
            "NumCurrentPowers" | "CurrentPowerIndex" => {
                str_value(edit.map_or(0, HeroEdit::power_count).to_string())
            }
            _ => None,
        }
    }

    fn class_index(&self) -> usize {
        self.edit
            .as_ref()
            .and_then(|e| usize::try_from(e.record().class_index).ok())
            .unwrap_or(0)
    }

    fn class_types(&self) -> usize {
        self.sys
            .def()
            .classes
            .get(self.class_index())
            .map_or(0, |c| c.subclasses.len())
    }

    fn sort_heroes(&mut self, key: usize) {
        let sort = self.hero_sort.get_or_insert(UNSORTED_HEROES);
        sort.click(key);
        let sort = *sort;
        self.log.push(format!("cah: hero list sorted - {sort:?}"));
    }

    fn hero_order(&self) -> Vec<usize> {
        let heroes = self.store.heroes();
        let mut order: Vec<usize> = (0..heroes.len()).collect();
        if let Some(sort) = self.hero_sort {
            let key = |h: &mint_sage_asset::StoredHero, k: usize| match k {
                SORT_ICON => HeroSortKey::System(h.source == HeroSource::System),
                SORT_NAME => HeroSortKey::Name(h.record.name_string()),
                _ => HeroSortKey::Type(
                    u64::from(h.record.class_index) * 100 + u64::from(h.record.subclass_index),
                ),
            };
            order.sort_by(|&a, &b| match (heroes.get(a), heroes.get(b)) {
                (Some(a), Some(b)) => sort.compare(|k| key(a, k).cmp(&key(b, k))),
                _ => std::cmp::Ordering::Equal,
            });
        }
        order
    }

    pub(crate) fn hero_list(&self) -> ListSpec {
        let order = self.hero_order();
        let heroes = self.store.heroes();
        let rows = order
            .iter()
            .filter_map(|&i| heroes.get(i))
            .map(|h| {
                let sub = self.sys.subclass(
                    usize::try_from(h.record.class_index).unwrap_or(usize::MAX),
                    usize::try_from(h.record.subclass_index).unwrap_or(usize::MAX),
                );
                let marker = if h.source == HeroSource::System {
                    CellSpec::Text(String::new())
                } else {
                    CellSpec::ScaledImage {
                        image: OWN_HERO_MARKER.to_owned(),
                        percent: OWN_HERO_MARKER_PERCENT,
                    }
                };
                vec![
                    marker,
                    CellSpec::Text(h.record.name_string()),
                    CellSpec::Text(self.loc_opt(sub.and_then(|s| s.name_tag.as_deref()))),
                ]
            })
            .collect();
        ListSpec {
            columns: HERO_LIST_COLUMNS.to_vec(),
            rows,
            selected: self
                .selected
                .and_then(|s| order.iter().position(|&i| i == s)),
            scroll_bar: None,
            text_inset_percent: 0,
        }
    }

    pub(crate) fn stats_list(&self) -> ListSpec {
        let sub = self.edit.as_ref().and_then(|e| {
            let record = e.record();
            self.sys.subclass(
                usize::try_from(record.class_index).ok()?,
                usize::try_from(record.subclass_index).ok()?,
            )
        });
        let rows = sub
            .map(|sub| {
                sub.stats
                    .iter()
                    .filter_map(|name| self.sys.thing_stat(name))
                    .map(|stat| {
                        let value = stat
                            .stat_name
                            .as_deref()
                            .and_then(|n| self.stats.1.get(n))
                            .copied()
                            .unwrap_or(0);
                        vec![
                            CellSpec::Text(self.loc_opt(stat.name_tag.as_deref())),
                            CellSpec::Text(value.to_string()),
                        ]
                    })
                    .collect()
            })
            .unwrap_or_default();
        ListSpec {
            columns: STATS_LIST_COLUMNS.to_vec(),
            rows,
            selected: None,
            scroll_bar: None,
            text_inset_percent: 0,
        }
    }

    fn refresh_stats(&mut self) {
        let id = self
            .edit
            .as_ref()
            .and_then(|e| e.record().id_str())
            .filter(|id| !id.is_empty());
        if id != self.stats.0 {
            let counters = id
                .as_deref()
                .map(|id| self.store.stats(id))
                .unwrap_or_default();
            if counters != self.stats.1 {
                self.awards_dirty = true;
            }
            self.stats = (id, counters);
        }
    }

    pub(crate) fn name_text(&self) -> &str {
        &self.name
    }

    pub(crate) fn name_focused(&self) -> bool {
        self.focused
    }

    pub(crate) fn name_hint(&self) -> bool {
        self.page == Page::Appearance
    }

    pub(crate) fn publish(&mut self, expose: &mut Exposure<ImageSource>) {
        self.refresh_stats();
        let sys = Arc::clone(&self.sys);
        let def = sys.def();
        if let Some(e) = &self.edit {
            let record = e.record();
            let class = usize::try_from(record.class_index).unwrap_or(usize::MAX);
            let sub = usize::try_from(record.subclass_index).unwrap_or(usize::MAX);
            let mut put = |key: String, text: String| expose.string(&key, Some(text));
            put("MyHeroName".into(), e.name());
            put("MyHeroSubTitle".into(), e.name());
            put("MyHeroCost".into(), self.cost_label());
            put("MyHeroAttribPoints".into(), e.points().0.to_string());
            if let Some(c) = def.classes.get(class) {
                put("MyHeroClass".into(), self.loc_opt(c.name_tag.as_deref()));
                put(
                    "CahClassDescription".into(),
                    self.loc_opt(c.description_tag.as_deref()),
                );
                if let Some(s) = c.subclasses.get(sub) {
                    put("MyHeroType".into(), self.loc_opt(s.name_tag.as_deref()));
                    let desc = self.loc_opt(s.description_tag.as_deref());
                    put("CahTypeDescription".into(), desc.clone());
                    put("HeroTypeDescription".into(), desc);
                }
            }
            for (kind, label) in [
                (SlotKind::Attribute, "MyHeroAttribute_"),
                (SlotKind::Appearance, "MyHeroAppearance_"),
            ] {
                for i in 0..e.slot_len(kind) {
                    let Some(group) = e.slot_group(kind, i) else {
                        continue;
                    };
                    let binder = def
                        .binders
                        .iter()
                        .find(|b| b.group.as_deref() == Some(group));
                    put(
                        format!("{label}{i}"),
                        self.loc_opt(binder.and_then(|b| b.label_tag.as_deref())),
                    );
                    if kind == SlotKind::Appearance {
                        let choice = e.slot_choice(kind, i);
                        let bling = def.blings.iter().find(|b| b.upgrade.as_deref() == choice);
                        put(
                            format!("MyHeroAppearanceVal_{i}"),
                            self.loc_opt(bling.and_then(|b| b.name_tag.as_deref())),
                        );
                    }
                }
            }
            for n in 0..PICKS {
                let button = record
                    .powers
                    .get(n)
                    .filter(|p| !p.button.is_empty())
                    .map(|p| String::from_utf8_lossy(&p.button).into_owned());
                let label = button
                    .as_deref()
                    .map(|b| self.loc(&format!("CAH:{b}_Name")))
                    .unwrap_or_default();
                expose.string(&format!("MyPowerLevel_{}", n + 1), Some(label));
                let icon = button.as_deref().map(|b| {
                    sys.button(b)
                        .and_then(|p| p.icon.clone())
                        .unwrap_or_else(|| FALLBACK_POWER_ICON.to_owned())
                });
                expose.image(
                    ImageKey::Name(format!("MyPowerIcon_{}", n + 1)),
                    icon.map(ImageSource::MappedImage),
                );
            }
            if self.page == Page::Manager {
                let description = self
                    .rolled_over
                    .and_then(|n| record.powers.get(n))
                    .filter(|p| !p.button.is_empty())
                    .and_then(|p| sys.button(&String::from_utf8_lossy(&p.button)))
                    .map(|b| self.loc_opt(b.descript_label.as_deref()))
                    .unwrap_or_default();
                expose.string("HeroPowersDescription", Some(description));
            }
            if self.page == Page::Powers {
                self.publish_powers(e, expose);
            }
        }
        if matches!(self.page, Page::Manager | Page::Class) {
            for (i, c) in def.classes.iter().enumerate() {
                expose.image(
                    ImageKey::Name(format!("Cah::ClassIcon{i}")),
                    c.icon_image.clone().map(ImageSource::MappedImage),
                );
            }
            if let Some(c) = def.classes.get(self.class_index()) {
                for (i, s) in c.subclasses.iter().enumerate() {
                    expose.image(
                        ImageKey::Name(format!("Cah::TypeIcon{i}")),
                        s.icon_image.clone().map(ImageSource::MappedImage),
                    );
                }
            }
        }
        if self.page == Page::Manager {
            expose.string(
                "Cah:SelectedAwardDesc",
                Some(self.award_description.clone()),
            );
        }
        if self.page == Page::Manager && std::mem::take(&mut self.awards_dirty) {
            self.publish_awards(expose);
            self.fire("UpdateAwards", Vec::new());
        }
        for fire in std::mem::take(&mut self.fires) {
            expose.fire(fire.function, fire.args);
        }
    }

    fn subclass_award(&self, i: usize) -> Option<&mint_sage_ini::AwardDef> {
        let record = self.edit.as_ref()?.record();
        let sub = self.sys.subclass(
            usize::try_from(record.class_index).ok()?,
            usize::try_from(record.subclass_index).ok()?,
        )?;
        let name = sub.awards.get(i)?;
        self.sys.awards().iter().find(|a| {
            a.name
                .as_deref()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
    }

    fn publish_awards(&self, expose: &mut Exposure<ImageSource>) {
        for i in 0..self.sys.awards().len() {
            let award = self.subclass_award(i);
            expose.image(
                ImageKey::Name(format!("CahAward:Image_{i}")),
                award
                    .and_then(|a| a.image.clone())
                    .map(ImageSource::MappedImage),
            );
            expose.string(
                &format!("CahAward:Name_{i}"),
                Some(self.loc_opt(award.and_then(|a| a.name_tag.as_deref()))),
            );
        }
    }

    fn publish_powers(&self, e: &HeroEdit, expose: &mut Exposure<ImageSource>) {
        expose.string(
            "HeroPowersError",
            Some(self.power_error.clone().unwrap_or_default()),
        );
        expose.string("HeroPowersWizard", Some(self.wizard_text(e)));
        self.publish_palantir(e, expose);
        for r in 0..e.power_rows() {
            let mut name = None;
            for c in 0..COLUMNS {
                let key = ImageKey::Name(format!("SelectPower_{}_{}", r + 1, c + 1));
                let Some(button) = e.power_button(r, c) else {
                    expose.image(key, None);
                    continue;
                };
                let state = e.power_state(r, c);
                let has_right = (c + 1..COLUMNS).any(|n| e.power_button(r, n).is_some());
                let chosen = name.is_none()
                    || state == CellState::Available
                    || (state == CellState::Selected && !has_right);
                if chosen {
                    name = Some(self.loc(&format!("CAH:{button}_Name")));
                }
                let icon = self
                    .sys
                    .button(button)
                    .and_then(|p| p.icon.clone())
                    .unwrap_or_else(|| FALLBACK_POWER_ICON.to_owned());
                expose.image(key, Some(ImageSource::MappedImage(icon)));
                expose.call(
                    &format!("power_{r}_{c}"),
                    "UpdateSelectPowerIcon",
                    vec![
                        number(r + 1),
                        number(c + 1),
                        Value::str(
                            STATE_LABELS
                                .get(state as usize)
                                .copied()
                                .unwrap_or("_unused"),
                        ),
                    ],
                );
            }
            expose.string(
                &format!("PowerName_{}", r + 1),
                Some(name.unwrap_or_default()),
            );
        }
    }

    fn palantir_images(&self, e: &HeroEdit) -> Vec<(usize, Option<String>)> {
        let icon = |image: &Option<String>| {
            Some(
                image
                    .clone()
                    .unwrap_or_else(|| FALLBACK_POWER_ICON.to_owned()),
            )
        };
        let mut writes: Vec<(usize, Option<String>)> = self
            .sys
            .template_icons()
            .iter()
            .enumerate()
            .map(|(slot, image)| (slot, icon(image)))
            .collect();
        for entry in e.record().powers.iter().take(e.power_count()) {
            let name = String::from_utf8_lossy(&entry.button);
            if let Some(button) = self.sys.button(&name).filter(|b| b.in_palantir) {
                let slot = usize::try_from(entry.button_index).unwrap_or(usize::MAX);
                writes.push((slot, icon(&button.icon)));
            }
        }
        let leading = self.sys.leading_slots();
        for slot in e.palantir_used()..self.sys.palantir_cap() {
            writes.push((slot.saturating_add(leading), None));
        }
        writes
    }

    fn publish_palantir(&self, e: &HeroEdit, expose: &mut Exposure<ImageSource>) {
        for (slot, image) in self.palantir_images(e) {
            expose.image(
                ImageKey::Name(format!("PalantirBttn_{}", slot.saturating_add(1))),
                image.map(ImageSource::MappedImage),
            );
        }
    }

    fn cost_label(&self) -> String {
        let cost = self.cost().to_string();
        let label = self.loc("CAH:FormatSpecialPowerCost");
        if label.contains("%d") || label.contains("%u") {
            label.replace("%d", &cost).replace("%u", &cost)
        } else {
            cost
        }
    }

    fn wizard_text(&self, e: &HeroEdit) -> String {
        let Some((line, suffix)) = e.wizard() else {
            return String::new();
        };
        let mut text = match line {
            WizardLine::NoAvailablePowers => self.loc("WIZARD:CahPowersNoAvailablePowers"),
            WizardLine::SelectFirstPower => self.loc("WIZARD:CahPowersSelectFirstPower"),
            WizardLine::SelectSecondPower => self.loc("WIZARD:CahPowersSelectSecondPower"),
            WizardLine::SelectionComplete => self.loc("WIZARD:CahPowersSelectionComplete"),
            WizardLine::SelectNextPower { group, level } => self
                .loc(&format!("WIZARD:CahPowersSelectNextPower_{group}"))
                .replace("%d", &level.to_string()),
        };
        let extra = match suffix {
            Some(WizardSuffix::PalantirFull) => Some("WIZARD:CahPowersPalantirFull"),
            Some(WizardSuffix::UpgradeOrNewPower) => Some("WIZARD:CahPowersUpgradeOrNewPower"),
            None => None,
        };
        if let Some(key) = extra {
            text.push(' ');
            text.push_str(&self.loc(key));
        }
        text
    }
}

#[cfg(test)]
mod tests;
