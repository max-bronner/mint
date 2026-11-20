pub mod color_picker;
pub mod gadget;
pub mod host;
pub mod scene;
pub mod scene_surface;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mint_sage_apt::{
    parse_const, AptMovie, Character, ConstItem, DatEntry, Import, MovieSet, ResolvedImport,
};
use mint_sage_apt_player::{InstanceId, Library, Player};
use mint_sage_big::BigArchive;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("{path}: {source}")]
    Archive {
        path: PathBuf,
        #[source]
        source: mint_sage_big::Error,
    },
    #[error("{path}: missing a .apt or .const entry")]
    IncompleteMovie { path: PathBuf },
    #[error("{path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: mint_sage_apt::Error,
    },
    #[error("{path}: {source}")]
    Dat {
        path: PathBuf,
        #[source]
        source: mint_sage_apt::Error,
    },
    #[error("no movie named {name:?} under {install_root}/apt")]
    NotFound { name: String, install_root: PathBuf },
}

#[derive(Debug, Default)]
pub struct MovieAssets {
    pub apt: Vec<u8>,
    pub consts: Vec<ConstItem>,
    pub ru: HashMap<String, String>,
    pub dat_entries: Vec<DatEntry>,
    pub tga: HashMap<String, Vec<u8>>,
}

#[derive(Debug)]
pub struct LoadedMovie {
    pub movie: AptMovie,
    pub assets: MovieAssets,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub attempted: usize,
    pub resolved: usize,
    pub failures: Vec<(String, String, String)>,
}

pub struct MovieLibrary {
    install_roots: Vec<PathBuf>,
    loaded: HashMap<String, LoadedMovie>,
}

fn normalize(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    lower.strip_suffix(".apt").unwrap_or(&lower).to_owned()
}

impl MovieLibrary {
    #[must_use]
    pub fn new(install_root: impl Into<PathBuf>) -> Self {
        Self::new_with_roots([install_root.into()])
    }

    #[must_use]
    pub fn new_with_roots(install_roots: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            install_roots: install_roots.into_iter().collect(),
            loaded: HashMap::new(),
        }
    }

    #[must_use]
    pub fn list_available(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .install_roots
            .iter()
            .flat_map(|root| {
                let dir = root.join("apt");
                std::fs::read_dir(&dir)
                    .into_iter()
                    .flatten()
                    .filter_map(std::result::Result::ok)
                    .map(|entry| entry.path())
                    .collect::<Vec<_>>()
            })
            .filter(|path| {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("big"))
            })
            .filter_map(|path| {
                path.file_stem()
                    .map(|stem| stem.to_string_lossy().to_ascii_lowercase())
            })
            .collect();
        names.sort();
        names.dedup();
        names
    }

    pub fn ensure_loaded(&mut self, name: &str) -> Result<()> {
        let key = normalize(name);
        if self.loaded.contains_key(&key) {
            return Ok(());
        }
        let mut last_error = None;
        for root in self.install_roots.clone() {
            let path = root.join("apt").join(format!("{key}.big"));
            match self.load_from(path, &key) {
                Ok(()) => return Ok(()),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(|| Error::NotFound {
            name: name.to_owned(),
            install_root: self
                .install_roots
                .first()
                .cloned()
                .unwrap_or_else(|| PathBuf::from(".")),
        }))
    }

    fn load_from(&mut self, path: PathBuf, key: &str) -> Result<()> {
        let archive = BigArchive::open(&path).map_err(|source| Error::Archive {
            path: path.clone(),
            source,
        })?;
        let mut apt_bytes = None;
        let mut const_bytes = None;
        let mut ru = HashMap::new();
        let mut dat_text = String::new();
        let mut tga = HashMap::new();
        for entry in archive.entries() {
            let full = entry.path.as_str();
            let lower = full.to_ascii_lowercase();
            let base = lower
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(&lower)
                .to_owned();
            if lower.ends_with(".apt") {
                apt_bytes = Some(archive.read_entry(entry).map_err(|source| Error::Archive {
                    path: path.clone(),
                    source,
                })?);
            } else if lower.ends_with(".const") {
                const_bytes = Some(archive.read_entry(entry).map_err(|source| Error::Archive {
                    path: path.clone(),
                    source,
                })?);
            } else if lower.ends_with(".dat") {
                let bytes = archive.read_entry(entry).map_err(|source| Error::Archive {
                    path: path.clone(),
                    source,
                })?;
                dat_text = String::from_utf8_lossy(&bytes).into_owned();
            } else if lower.ends_with(".ru") {
                let bytes = archive.read_entry(entry).map_err(|source| Error::Archive {
                    path: path.clone(),
                    source,
                })?;
                ru.insert(base, String::from_utf8_lossy(&bytes).into_owned());
            } else if lower.ends_with(".tga") {
                let bytes = archive.read_entry(entry).map_err(|source| Error::Archive {
                    path: path.clone(),
                    source,
                })?;
                tga.insert(base, bytes);
            }
        }
        let (Some(apt_bytes), Some(const_bytes)) = (apt_bytes, const_bytes) else {
            return Err(Error::IncompleteMovie { path });
        };
        let dat_entries = mint_sage_apt::parse_dat(&dat_text).map_err(|source| Error::Dat {
            path: path.clone(),
            source,
        })?;
        let consts = parse_const(&const_bytes)
            .map_err(|source| Error::Parse {
                path: path.clone(),
                source,
            })?
            .items;
        let movie =
            AptMovie::load(key, &const_bytes, &apt_bytes).map_err(|source| Error::Parse {
                path: path.clone(),
                source,
            })?;
        self.loaded.insert(
            key.to_owned(),
            LoadedMovie {
                movie,
                assets: MovieAssets {
                    apt: apt_bytes,
                    consts,
                    ru,
                    dat_entries,
                    tga,
                },
            },
        );
        Ok(())
    }

    pub fn load_imports_transitively(
        &mut self,
        root_name: &str,
        max_depth: u32,
    ) -> Result<ImportReport> {
        self.ensure_loaded(root_name)?;
        let mut report = ImportReport::default();
        let mut queue = vec![(normalize(root_name), 0u32)];
        let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
        while let Some((name, depth)) = queue.pop() {
            if !visited.insert(name.clone()) {
                continue;
            }
            let Some(loaded) = self.loaded.get(&name) else {
                continue;
            };
            let imports: Vec<Import> = loaded.movie.imports.clone();
            for import in &imports {
                report.attempted = report.attempted.saturating_add(1);
                let target_key = normalize(&import.movie);
                if depth < max_depth && self.ensure_loaded(&target_key).is_err() {
                    report
                        .failures
                        .push((name.clone(), import.movie.clone(), import.name.clone()));
                    continue;
                }
                if depth < max_depth {
                    queue.push((target_key.clone(), depth.saturating_add(1)));
                }
            }
        }
        let movie_set = self.movie_set();
        report.resolved = 0;
        report.failures.clear();
        for name in self.loaded.keys().cloned().collect::<Vec<_>>() {
            let Some(loaded) = self.loaded.get(&name) else {
                continue;
            };
            for import in &loaded.movie.imports {
                match mint_sage_apt::resolve_import(&movie_set, import) {
                    Ok(_) => report.resolved = report.resolved.saturating_add(1),
                    Err(_) => report.failures.push((
                        name.clone(),
                        import.movie.clone(),
                        import.name.clone(),
                    )),
                }
            }
        }
        report.attempted = report.resolved.saturating_add(report.failures.len());
        Ok(report)
    }

    #[must_use]
    pub fn movie_set(&self) -> MovieSet {
        let mut set = MovieSet::new();
        for loaded in self.loaded.values() {
            set.insert(loaded.movie.clone());
        }
        set
    }

    #[must_use]
    pub fn movie(&self, name: &str) -> Option<&AptMovie> {
        self.loaded.get(&normalize(name)).map(|l| &l.movie)
    }

    #[must_use]
    pub fn assets(&self, name: &str) -> Option<&MovieAssets> {
        self.loaded.get(&normalize(name)).map(|l| &l.assets)
    }
}

#[must_use]
pub fn localise(text: &str, table: &mint_sage_str::StringTable) -> String {
    let text = split_text_style(text).0;
    let Some(key) = text.strip_prefix('$') else {
        return text.to_owned();
    };
    if key.is_empty() {
        return text.to_owned();
    }
    let namespaced = format!("APT:{key}");
    if let Some(found) = table.get(&namespaced) {
        return found.to_owned();
    }
    if let Some(found) = table.get(key) {
        return found.to_owned();
    }
    format!("MISSING: '{namespaced}'")
}

#[must_use]
pub fn localise_with(
    text: &str,
    dynamic: &HashMap<String, String>,
    table: &mint_sage_str::StringTable,
) -> String {
    if let Some(found) = split_text_style(text)
        .0
        .strip_prefix('$')
        .and_then(|key| dynamic.get(key))
    {
        return found.clone();
    }
    localise(text, table)
}

#[must_use]
pub fn localises(text: &str, table: &mint_sage_str::StringTable) -> bool {
    match split_text_style(text).0.strip_prefix('$') {
        Some(key) if !key.is_empty() => {
            table.get(&format!("APT:{key}")).is_some() || table.get(key).is_some()
        }
        _ => false,
    }
}

const DROP_SHADOW_MARKER: &str = "&dropshadow";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextStyle {
    pub drop_shadow: bool,
}

#[must_use]
pub fn split_text_style(text: &str) -> (&str, TextStyle) {
    let cut = text.len().saturating_sub(DROP_SHADOW_MARKER.len());
    match (text.get(..cut), text.get(cut..)) {
        (Some(body), Some(marker)) if marker.eq_ignore_ascii_case(DROP_SHADOW_MARKER) => {
            (body, TextStyle { drop_shadow: true })
        }
        _ => (text, TextStyle::default()),
    }
}

#[must_use]
pub fn text_style(text: &str) -> TextStyle {
    split_text_style(text).1
}

#[must_use]
pub fn load_string_table(install_root: &Path) -> (mint_sage_str::StringTable, usize, usize) {
    load_string_table_from_roots(std::slice::from_ref(&install_root.to_path_buf()))
}

#[must_use]
pub fn load_string_table_from_roots(
    install_roots: &[PathBuf],
) -> (mint_sage_str::StringTable, usize, usize) {
    let mut table = mint_sage_str::StringTable::new();
    let mut files = 0usize;
    let mut failures = 0usize;
    let archives: Vec<PathBuf> = install_roots
        .iter()
        .flat_map(|root| mint_sage_big::archives_under(root))
        .collect();
    for main_pass in [true, false] {
        for path in &archives {
            let Ok(archive) = BigArchive::open(path) else {
                continue;
            };
            for entry in archive.entries() {
                let lower = entry.name.to_ascii_lowercase();
                if !lower.ends_with(".str") {
                    continue;
                }
                if lower.contains("lotr.str") != main_pass {
                    continue;
                }
                let Ok(bytes) = archive.read_entry(entry) else {
                    failures = failures.saturating_add(1);
                    continue;
                };
                match mint_sage_str::parse_str(&bytes) {
                    Ok(records) => {
                        files = files.saturating_add(1);
                        table.insert(records);
                    }
                    Err(_) => failures = failures.saturating_add(1),
                }
            }
        }
    }
    (table, files, failures)
}

const BUNDLED_FONT_FILES: &[&str] = &["albertusmt.otf", "omnialtstd.ttf", "sachwt__.ttf"];

#[must_use]
pub fn load_bundled_fonts(install_roots: &[PathBuf]) -> HashMap<String, mint_ui::Font> {
    let mut remaining: Vec<&str> = BUNDLED_FONT_FILES.to_vec();
    let mut out = HashMap::new();
    for root in install_roots {
        for path in mint_sage_big::archives_under(root) {
            if remaining.is_empty() {
                break;
            }
            let Ok(archive) = BigArchive::open(&path) else {
                continue;
            };
            let mut found_here = Vec::new();
            for (index, name) in remaining.iter().enumerate() {
                if let Some(entry) = archive
                    .entries()
                    .iter()
                    .find(|e| e.path.as_str().to_ascii_lowercase().ends_with(name))
                {
                    if let Ok(bytes) = archive.read_entry(entry) {
                        if let Ok(font) = mint_ui::Font::from_bytes(bytes) {
                            out.insert((*name).to_string(), font);
                            found_here.push(index);
                        }
                    }
                }
            }
            for index in found_here.into_iter().rev() {
                remaining.remove(index);
            }
        }
    }
    load_host_fonts(&mut out);
    out
}

fn load_host_fonts(out: &mut HashMap<String, mint_ui::Font>) {
    for (logical_name, file) in HOST_FONT_FILE_NAMES {
        if out.contains_key(*file) {
            continue;
        }
        if !force_bundled_fonts() {
            if let Some(bytes) = mint_ui::read_os_font_file(file) {
                match mint_ui::Font::from_bytes(bytes) {
                    Ok(font) => {
                        let dir = mint_ui::os_font_dir().unwrap_or_default();
                        eprintln!("font: {logical_name} -> {}", dir.join(file).display());
                        out.insert((*file).to_string(), font);
                        continue;
                    }
                    Err(err) => eprintln!("font: {logical_name} -> {file}: {err}"),
                }
            }
        }
        if !load_bundled_substitute(logical_name, out) {
            eprintln!("font: {logical_name} -> not found on host, falls back to tofu");
        }
    }
}

#[must_use]
fn force_bundled_fonts() -> bool {
    static FORCE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FORCE.get_or_init(|| std::env::var_os("MINT_FORCE_BUNDLED_FONTS").is_some())
}

fn load_bundled_substitute(logical_name: &str, out: &mut HashMap<String, mint_ui::Font>) -> bool {
    let Some(chain) = mint_ui::chain_for(logical_name) else {
        return false;
    };
    for (name, label) in BUNDLED_SUBSTITUTE_NAMES {
        if out.contains_key(*name) {
            return true;
        }
        if !chain.open_substitutes.contains(name) {
            continue;
        }
        let Some(bytes) = mint_ui::bundled_substitute_font(name) else {
            continue;
        };
        return match mint_ui::Font::from_bytes(bytes.to_vec()) {
            Ok(font) => {
                eprintln!("font: {logical_name} -> bundled {label}");
                out.insert((*name).to_string(), font);
                true
            }
            Err(err) => {
                eprintln!("font: {logical_name} -> bundled {label}: {err}");
                false
            }
        };
    }
    false
}

#[must_use]
pub fn ru_base_name(movie_name: &str, geometry_id: u32) -> String {
    let path = mint_sage_apt::ru_path(movie_name, geometry_id);
    path.rsplit('/').next().unwrap_or(&path).to_owned()
}

pub const MAX_IMPORT_CHASE: u32 = 8;

#[must_use]
pub fn resolve_character<'a>(
    movie_set: &'a MovieSet,
    movie_name: &str,
    character_id: u32,
    depth: u32,
) -> Option<(&'a AptMovie, u32, &'a Character)> {
    let movie = movie_set.get(movie_name)?;
    let character = movie.characters.get(character_id as usize)?;
    if !matches!(character, Character::Placeholder) {
        return Some((movie, character_id, character));
    }
    if depth >= MAX_IMPORT_CHASE {
        return None;
    }
    let import = movie
        .imports
        .iter()
        .find(|import| import.local_character == character_id)?;
    let ResolvedImport {
        movie: target,
        character_id: target_id,
        character: target_character,
    } = mint_sage_apt::resolve_import(movie_set, import).ok()?;
    if matches!(target_character, Character::Placeholder) {
        return resolve_character(movie_set, &target.name, target_id, depth.saturating_add(1));
    }
    Some((target, target_id, target_character))
}

#[must_use]
pub fn compose_affine(
    parent: ([[f32; 2]; 2], [f32; 2]),
    child: ([[f32; 2]; 2], [f32; 2]),
) -> ([[f32; 2]; 2], [f32; 2]) {
    let (pa, pt) = parent;
    let (ca, ct) = child;
    let affine = [
        [
            pa[0][0] * ca[0][0] + pa[0][1] * ca[1][0],
            pa[0][0] * ca[0][1] + pa[0][1] * ca[1][1],
        ],
        [
            pa[1][0] * ca[0][0] + pa[1][1] * ca[1][0],
            pa[1][0] * ca[0][1] + pa[1][1] * ca[1][1],
        ],
    ];
    let translate = [
        pa[0][0] * ct[0] + pa[0][1] * ct[1] + pt[0],
        pa[1][0] * ct[0] + pa[1][1] * ct[1] + pt[1],
    ];
    (affine, translate)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScaleMode {
    #[default]
    Stretch,
    Anchored,
}

impl ScaleMode {
    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            Self::Stretch => Self::Anchored,
            Self::Anchored => Self::Stretch,
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        if s.eq_ignore_ascii_case("stretch") {
            Some(Self::Stretch)
        } else if s.eq_ignore_ascii_case("anchored") {
            Some(Self::Anchored)
        } else {
            None
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stretch => "stretch",
            Self::Anchored => "anchored",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Letterbox {
    pub scale: [f32; 2],
    pub offset: [f32; 2],
}

impl Letterbox {
    #[must_use]
    pub fn fit(stage_w: f32, stage_h: f32, window_w: f32, window_h: f32) -> Self {
        if stage_w <= 0.0 || stage_h <= 0.0 || window_w <= 0.0 || window_h <= 0.0 {
            return Self {
                scale: [1.0, 1.0],
                offset: [0.0, 0.0],
            };
        }
        let scale = (window_w / stage_w).min(window_h / stage_h);
        let offset = [
            (window_w - stage_w * scale) / 2.0,
            (window_h - stage_h * scale) / 2.0,
        ];
        Self {
            scale: [scale, scale],
            offset,
        }
    }

    #[must_use]
    pub fn stretch(stage_w: f32, stage_h: f32, window_w: f32, window_h: f32) -> Self {
        if stage_w <= 0.0 || stage_h <= 0.0 || window_w <= 0.0 || window_h <= 0.0 {
            return Self {
                scale: [1.0, 1.0],
                offset: [0.0, 0.0],
            };
        }
        Self {
            scale: [window_w / stage_w, window_h / stage_h],
            offset: [0.0, 0.0],
        }
    }

    #[must_use]
    pub fn after(&self, inner: Letterbox) -> Letterbox {
        Letterbox {
            scale: [
                self.scale[0] * inner.scale[0],
                self.scale[1] * inner.scale[1],
            ],
            offset: [
                inner.offset[0] * self.scale[0] + self.offset[0],
                inner.offset[1] * self.scale[1] + self.offset[1],
            ],
        }
    }

    #[must_use]
    pub fn as_transform(&self) -> ([[f32; 2]; 2], [f32; 2]) {
        ([[self.scale[0], 0.0], [0.0, self.scale[1]]], self.offset)
    }

    #[must_use]
    pub fn to_stage(&self, window_pos: [f32; 2]) -> [f32; 2] {
        let safe = |s: f32| if s.abs() < f32::EPSILON { 1.0 } else { s };
        [
            (window_pos[0] - self.offset[0]) / safe(self.scale[0]),
            (window_pos[1] - self.offset[1]) / safe(self.scale[1]),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Edge {
    Low,
    Centre,
    High,
}

fn edge_of(low: f32, high: f32, stage_extent: f32, slack: f32) -> Edge {
    if low <= slack {
        Edge::Low
    } else if high >= stage_extent - slack {
        Edge::High
    } else {
        Edge::Centre
    }
}

#[must_use]
pub fn anchor_table<Id: Copy + Eq + std::hash::Hash>(
    elements: &[(Id, [f32; 4])],
    stage_w: f32,
    stage_h: f32,
    window_w: f32,
    window_h: f32,
    slack: f32,
) -> std::collections::HashMap<Id, Letterbox> {
    if stage_w <= 0.0 || stage_h <= 0.0 || window_w <= 0.0 || window_h <= 0.0 {
        return std::collections::HashMap::new();
    }
    anchor_groups(elements, stage_w, slack)
        .into_iter()
        .map(|(id, dock)| (id, dock.letterbox(stage_w, stage_h, window_w, window_h)))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Dock {
    At(f32),
    Fill,
}

impl Dock {
    fn letterbox(self, stage_w: f32, stage_h: f32, window_w: f32, window_h: f32) -> Letterbox {
        match self {
            Self::Fill => Letterbox::stretch(stage_w, stage_h, window_w, window_h),
            Self::At(anchor_x) => {
                let scale = window_h / stage_h;
                let stretch_x = window_w / stage_w;
                Letterbox {
                    scale: [scale, scale],
                    offset: [anchor_x * stretch_x - anchor_x * scale, 0.0],
                }
            }
        }
    }
}

fn spans(bbox: [f32; 4], stage_w: f32, slack: f32) -> bool {
    bbox[0] <= slack && bbox[2] >= stage_w - slack
}

fn anchor_groups<Id: Copy + Eq + std::hash::Hash>(
    elements: &[(Id, [f32; 4])],
    stage_w: f32,
    slack: f32,
) -> std::collections::HashMap<Id, Dock> {
    let mut out = std::collections::HashMap::new();
    let (full, elements): (Vec<_>, Vec<_>) = elements
        .iter()
        .copied()
        .partition(|&(_, bbox)| spans(bbox, stage_w, slack));
    for (id, _) in full {
        out.insert(id, Dock::Fill);
    }
    let n = elements.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], start: usize) -> usize {
        let mut x = start;
        loop {
            let Some(&p) = parent.get(x) else { return x };
            if p == x {
                return x;
            }
            let Some(&gp) = parent.get(p) else { return p };
            if let Some(slot) = parent.get_mut(x) {
                *slot = gp;
            }
            x = gp;
        }
    }
    let near = |a: [f32; 4], b: [f32; 4]| -> bool {
        a[0] <= b[2] + slack && b[0] <= a[2] + slack && a[1] <= b[3] + slack && b[1] <= a[3] + slack
    };
    for i in 0..n {
        let Some(&(_, bi)) = elements.get(i) else {
            continue;
        };
        for j in (i.saturating_add(1))..n {
            let Some(&(_, bj)) = elements.get(j) else {
                continue;
            };
            if near(bi, bj) {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    if let Some(slot) = parent.get_mut(ri) {
                        *slot = rj;
                    }
                }
            }
        }
    }

    let mut groups: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(i);
    }

    for members in groups.into_values() {
        let mut union = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for &i in &members {
            let Some(&(_, bbox)) = elements.get(i) else {
                continue;
            };
            union[0] = union[0].min(bbox[0]);
            union[1] = union[1].min(bbox[1]);
            union[2] = union[2].max(bbox[2]);
            union[3] = union[3].max(bbox[3]);
        }
        let h = edge_of(union[0], union[2], stage_w, slack);
        let anchor_x = match h {
            Edge::Low => 0.0,
            Edge::High => stage_w,
            Edge::Centre => (union[0] + union[2]) / 2.0,
        };
        for &i in &members {
            if let Some(&(id, _)) = elements.get(i) {
                out.insert(id, Dock::At(anchor_x));
            }
        }
    }
    out
}

#[must_use]
pub fn top_level_ancestor(player: &Player, id: InstanceId) -> InstanceId {
    let root = player.root();
    let mut at = id;
    loop {
        let Some(inst) = player.instance(at) else {
            return root;
        };
        match inst.parent() {
            Some(parent) if parent == root => return at,
            Some(parent) => at = parent,
            None => return at,
        }
    }
}

fn collect_top_level_bounds(player: &Player) -> Vec<(InstanceId, [f32; 4])> {
    let root = player.root();
    let Some(root_inst) = player.instance(root) else {
        return Vec::new();
    };
    root_inst
        .children()
        .filter_map(|(_, id)| {
            let bounds = player.local_bounds(id)?;
            let world = player.world_matrix(id)?;
            let corners = [
                world.apply(bounds[0], bounds[1]),
                world.apply(bounds[2], bounds[1]),
                world.apply(bounds[0], bounds[3]),
                world.apply(bounds[2], bounds[3]),
            ];
            let (mut l, mut t, mut r, mut b) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for (x, y) in corners {
                l = l.min(x);
                t = t.min(y);
                r = r.max(x);
                b = b.max(y);
            }
            Some((id, [l, t, r, b]))
        })
        .collect()
}

#[must_use]
pub fn player_anchors(
    player: &Player,
    stage_w: f32,
    stage_h: f32,
    window_w: f32,
    window_h: f32,
    slack: f32,
) -> HashMap<InstanceId, Letterbox> {
    let elements = collect_top_level_bounds(player);
    anchor_table(&elements, stage_w, stage_h, window_w, window_h, slack)
}

#[derive(Debug, Clone, Default)]
pub struct AnchorCache {
    entries: HashMap<InstanceId, (Dock, [f32; 4])>,
}

impl AnchorCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    #[must_use]
    pub fn letterboxes(
        &mut self,
        player: &Player,
        stage_w: f32,
        stage_h: f32,
        window_w: f32,
        window_h: f32,
        slack: f32,
    ) -> HashMap<InstanceId, Letterbox> {
        if stage_w <= 0.0 || stage_h <= 0.0 || window_w <= 0.0 || window_h <= 0.0 {
            return HashMap::new();
        }
        let live = collect_top_level_bounds(player);
        let live_ids: Vec<InstanceId> = live.iter().map(|(id, _)| *id).collect();
        self.entries.retain(|id, _| live_ids.contains(id));

        let new_elements: Vec<(InstanceId, [f32; 4])> = live
            .iter()
            .copied()
            .filter(|(id, _)| !self.entries.contains_key(id))
            .collect();
        if !new_elements.is_empty() {
            let mut all: Vec<(InstanceId, [f32; 4])> = self
                .entries
                .iter()
                .map(|(&id, &(_, bbox))| (id, bbox))
                .collect();
            all.extend(new_elements.iter().copied());
            let groups = anchor_groups(&all, stage_w, slack);
            for (id, bbox) in &new_elements {
                if let Some(&dock) = groups.get(id) {
                    self.entries.insert(*id, (dock, *bbox));
                }
            }
        }

        live_ids
            .into_iter()
            .filter_map(|id| {
                let &(dock, _) = self.entries.get(&id)?;
                Some((id, dock.letterbox(stage_w, stage_h, window_w, window_h)))
            })
            .collect()
    }

    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn resolve_point(
        &mut self,
        player: &Player,
        scale_mode: ScaleMode,
        stage_w: f32,
        stage_h: f32,
        window_w: f32,
        window_h: f32,
        slack: f32,
        window_pos: [f32; 2],
    ) -> [f32; 2] {
        if scale_mode != ScaleMode::Anchored {
            return Letterbox::stretch(stage_w, stage_h, window_w, window_h).to_stage(window_pos);
        }
        let anchors = self.letterboxes(player, stage_w, stage_h, window_w, window_h, slack);
        resolve_with_anchors(
            player, stage_w, stage_h, window_w, window_h, &anchors, window_pos,
        )
    }
}

fn resolve_with_anchors(
    player: &Player,
    stage_w: f32,
    stage_h: f32,
    window_w: f32,
    window_h: f32,
    anchors: &HashMap<InstanceId, Letterbox>,
    window_pos: [f32; 2],
) -> [f32; 2] {
    let stretch = Letterbox::stretch(stage_w, stage_h, window_w, window_h);
    resolve_hosted(player, stretch, anchors, window_pos)
}

fn resolve_hosted(
    player: &Player,
    stretch: Letterbox,
    anchors: &HashMap<InstanceId, Letterbox>,
    window_pos: [f32; 2],
) -> [f32; 2] {
    let root = player.root();
    let Some(root_inst) = player.instance(root) else {
        return stretch.to_stage(window_pos);
    };
    let mut candidates: Vec<(i32, InstanceId, [f32; 4])> = root_inst
        .children()
        .filter_map(|(depth, id)| {
            let bounds = player.local_bounds(id)?;
            let world = player.world_matrix(id)?;
            let lb = anchors.get(&id).copied().unwrap_or(stretch);
            let corners = [
                world.apply(bounds[0], bounds[1]),
                world.apply(bounds[2], bounds[1]),
                world.apply(bounds[0], bounds[3]),
                world.apply(bounds[2], bounds[3]),
            ];
            let (mut l, mut t, mut r, mut b) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for (x, y) in corners {
                let (wx, wy) = (
                    x * lb.scale[0] + lb.offset[0],
                    y * lb.scale[1] + lb.offset[1],
                );
                l = l.min(wx);
                t = t.min(wy);
                r = r.max(wx);
                b = b.max(wy);
            }
            Some((depth, id, [l, t, r, b]))
        })
        .collect();
    candidates.sort_by_key(|(depth, _, _)| std::cmp::Reverse(*depth));
    for (_, id, [l, t, r, b]) in candidates {
        if window_pos[0] >= l && window_pos[0] <= r && window_pos[1] >= t && window_pos[1] <= b {
            let lb = anchors.get(&id).copied().unwrap_or(stretch);
            return lb.to_stage(window_pos);
        }
    }
    stretch.to_stage(window_pos)
}

#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn resolve_stage_point(
    player: &Player,
    scale_mode: ScaleMode,
    stage_w: f32,
    stage_h: f32,
    window_w: f32,
    window_h: f32,
    slack: f32,
    window_pos: [f32; 2],
) -> [f32; 2] {
    if scale_mode != ScaleMode::Anchored {
        return Letterbox::stretch(stage_w, stage_h, window_w, window_h).to_stage(window_pos);
    }
    let anchors = player_anchors(player, stage_w, stage_h, window_w, window_h, slack);
    resolve_with_anchors(
        player, stage_w, stage_h, window_w, window_h, &anchors, window_pos,
    )
}

pub const ANCHOR_EDGE_SLACK: f32 = 24.0;

#[must_use]
pub fn unpack_color(packed: u32) -> [f32; 4] {
    let [r, g, b, a] = packed.to_le_bytes();
    [
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        f32::from(a) / 255.0,
    ]
}

#[must_use]
pub fn multiply_color(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2], a[3] * b[3]]
}

#[must_use]
pub fn add_color(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]]
}

#[must_use]
pub fn unpack_color_add(packed: u32) -> [f32; 4] {
    let [b, g, r, _top] = packed.to_le_bytes();
    [
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        0.0,
    ]
}

#[must_use]
pub fn concat_color_add(
    parent_mult: [f32; 4],
    parent_add: [f32; 4],
    child_add: [f32; 4],
) -> [f32; 4] {
    add_color(multiply_color(parent_mult, child_add), parent_add)
}

#[must_use]
pub fn unpack_edittext_color(packed: u32) -> [f32; 4] {
    let [b, g, r, a] = packed.to_le_bytes();
    [
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        f32::from(a) / 255.0,
    ]
}

#[must_use]
pub fn apply_text_color(
    authored: [f32; 4],
    text_color: Option<&mint_sage_apt_player::Value>,
) -> [f32; 4] {
    use mint_sage_apt_player::Value;
    let rgb = match text_color {
        Some(Value::Number(n)) if n.is_finite() && *n >= 0.0 => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let n = *n as u64;
            u32::try_from(n & 0x00ff_ffff).ok()
        }
        Some(Value::String(text)) => {
            let t = text.trim();
            match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                Some(hex) => u32::from_str_radix(hex, 16).ok(),
                None => t.parse::<u32>().ok(),
            }
            .map(|n| n & 0x00ff_ffff)
        }
        _ => None,
    };
    let Some(rgb) = rgb else {
        return authored;
    };
    let [_, r, g, b] = rgb.to_be_bytes();
    [
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        authored[3],
    ]
}

#[must_use]
pub fn bundled_font_file(name: &str) -> Option<&'static str> {
    match name {
        "Albertus MT" => Some("albertusmt.otf"),
        "Omnia LT Std" => Some("omnialtstd.ttf"),
        "SachaWynter" => Some("sachwt__.ttf"),
        _ => None,
    }
}

const HOST_FONT_FILE_NAMES: &[(&str, &str)] = &[
    ("Times New Roman", "times.ttf"),
    ("Arial", "arial.ttf"),
    ("Courier New", "cour.ttf"),
];

const BUNDLED_SUBSTITUTE_NAMES: &[(&str, &str)] = &[("Tinos", "Tinos-Regular")];

#[must_use]
fn host_font_file(name: &str) -> Option<&'static str> {
    static PRESENT: std::sync::OnceLock<Vec<bool>> = std::sync::OnceLock::new();
    let present = PRESENT.get_or_init(|| {
        HOST_FONT_FILE_NAMES
            .iter()
            .map(|(_, file)| mint_ui::os_font_file_exists(file))
            .collect()
    });
    let force_bundled = force_bundled_fonts();
    let chain = mint_ui::chain_for(name)?;
    let resolved = mint_ui::resolve_font_name(chain, |candidate| {
        if let Some((bundled_name, _)) = BUNDLED_SUBSTITUTE_NAMES
            .iter()
            .find(|(known, _)| *known == candidate)
        {
            return mint_ui::bundled_substitute_font(bundled_name).is_some();
        }
        !force_bundled
            && HOST_FONT_FILE_NAMES
                .iter()
                .zip(present)
                .any(|((known, _), &found)| found && *known == candidate)
    })?;
    HOST_FONT_FILE_NAMES
        .iter()
        .find(|(known, _)| *known == resolved)
        .map(|(_, file)| *file)
        .or_else(|| {
            BUNDLED_SUBSTITUTE_NAMES
                .iter()
                .find(|(known, _)| *known == resolved)
                .map(|(name, _)| *name)
        })
}

#[must_use]
pub fn resolved_font_file(name: &str) -> Option<&'static str> {
    bundled_font_file(name).or_else(|| host_font_file(name))
}

#[must_use]
pub fn widget_id(movie_name: &str, character_id: u32) -> mint_ui::WidgetId {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    movie_name.hash(&mut hasher);
    character_id.hash(&mut hasher);
    mint_ui::WidgetId(hasher.finish())
}

#[must_use]
pub fn instance_widget_id(path: &[i32], movie_name: &str, character_id: u32) -> mint_ui::WidgetId {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    movie_name.hash(&mut hasher);
    character_id.hash(&mut hasher);
    mint_ui::WidgetId(hasher.finish())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextBox {
    pub bounds: mint_sage_apt::Rect,
    pub alignment: u32,
    pub text_width: f32,
    pub ascent: f32,
    pub descent: f32,
    pub win_ascent: f32,
    pub win_descent: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ImageModes {
    #[default]
    Decoded,
    Matte,
    UnsetSolid,
    AlphaOpaque,
    RadialDisc,
}

impl ImageModes {
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Decoded => Self::Matte,
            Self::Matte => Self::UnsetSolid,
            Self::UnsetSolid => Self::AlphaOpaque,
            Self::AlphaOpaque => Self::RadialDisc,
            Self::RadialDisc => Self::Decoded,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Decoded => "decoded (SOLID/SOLID_TEXTURE opaque, ADDITIVE adds, else alpha)",
            Self::Matte => "matte (every texture's alpha kept)",
            Self::UnsetSolid => "unset-solid (unset or SOLID on a RenderImage: opaque)",
            Self::AlphaOpaque => "alpha-opaque (ALPHA or SOLID: opaque)",
            Self::RadialDisc => {
                "radial-disc (on the radial ring, ALPHA or SOLID: opaque inside a circle)"
            }
        }
    }

    #[must_use]
    pub fn opaque(self, surface_type: Option<&str>, mode: Option<&str>) -> bool {
        let mode = mode.map(str::trim).unwrap_or_default();
        let named = |name: &str| mode.eq_ignore_ascii_case(name);
        match self {
            Self::Matte | Self::RadialDisc => false,
            Self::Decoded => {
                !surface_type
                    .is_some_and(|t| t.eq_ignore_ascii_case(crate::host::DISABLED_IMAGE_TYPE))
                    && (named("SOLID") || named("SOLID_GRAYSCALE") || named("SOLID_TEXTURE"))
            }
            Self::UnsetSolid => {
                named("SOLID")
                    || (mode.is_empty()
                        && surface_type.is_some_and(|t| t.eq_ignore_ascii_case("RenderImage")))
            }
            Self::AlphaOpaque => named("SOLID") || named("ALPHA"),
        }
    }

    #[must_use]
    pub fn additive(self, surface_type: Option<&str>, mode: Option<&str>) -> bool {
        self == Self::Decoded
            && !surface_type
                .is_some_and(|t| t.eq_ignore_ascii_case(crate::host::DISABLED_IMAGE_TYPE))
            && mode
                .map(str::trim)
                .unwrap_or_default()
                .eq_ignore_ascii_case("ADDITIVE")
    }

    #[must_use]
    pub fn disc(self, mode: Option<&str>) -> bool {
        match self {
            Self::Decoded => true,
            Self::RadialDisc => {
                let trimmed = mode.map(str::trim).unwrap_or_default();
                trimmed.eq_ignore_ascii_case("SOLID") || trimmed.eq_ignore_ascii_case("ALPHA")
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextAnchor {
    #[default]
    Field,
    Top,
    Centred,
}

impl TextAnchor {
    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            Self::Field => Self::Top,
            Self::Top => Self::Centred,
            Self::Centred => Self::Field,
        }
    }

    #[must_use]
    pub fn for_field(self, multiline: bool, wordwrap: bool) -> Self {
        match self {
            Self::Field if multiline && wordwrap => Self::Top,
            Self::Field => Self::Centred,
            forced => forced,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Field => "per field (top if multiline+wordwrap, else centred)",
            Self::Top => "top (Flash TextField)",
            Self::Centred => "centred in bounds (GDI line box)",
        }
    }
}

pub const TEXT_GUTTER: f32 = 2.0;

#[must_use]
pub fn align_fraction(alignment: u32) -> f32 {
    match alignment {
        1 => 1.0,
        2 => 0.5,
        _ => 0.0,
    }
}

#[must_use]
pub fn text_origin(text_box: TextBox, anchor: TextAnchor) -> [f32; 2] {
    let TextBox {
        bounds,
        alignment,
        text_width,
        ascent,
        win_ascent,
        win_descent,
        ..
    } = text_box;
    let left = bounds.left + TEXT_GUTTER;
    let right = bounds.right - TEXT_GUTTER;
    let fraction = align_fraction(alignment);
    let x = left + (right - left - text_width) * fraction;
    let y = match anchor {
        TextAnchor::Top => bounds.top + TEXT_GUTTER + ascent,
        TextAnchor::Centred | TextAnchor::Field => {
            (bounds.top + bounds.bottom) / 2.0 + (win_ascent - win_descent) / 2.0
        }
    };
    [x, y]
}

#[must_use]
pub fn edit_text_lines(
    text: &str,
    multiline: bool,
    width: f32,
    measure: impl FnMut(&str) -> f32,
) -> Vec<String> {
    let lines: Vec<String> = if multiline {
        crate::gadget::list::wrapped(text, width, measure)
    } else {
        Vec::new()
    };
    if lines.len() > 1 {
        lines
    } else {
        vec![text.to_owned()]
    }
}

#[must_use]
pub fn line_origin(
    text_box: TextBox,
    anchor: TextAnchor,
    line: usize,
    lines: usize,
    line_height: f32,
) -> [f32; 2] {
    let [x, y] = text_origin(text_box, anchor);
    #[allow(clippy::cast_precision_loss)]
    let (line, before) = (line as f32, lines.max(1).saturating_sub(1) as f32);
    let first = match anchor {
        TextAnchor::Top => y,
        TextAnchor::Centred | TextAnchor::Field => y - before * line_height / 2.0,
    };
    [x, first + line * line_height]
}

#[must_use]
pub fn load_player_library(install_roots: &[PathBuf]) -> (Arc<Library>, Vec<String>) {
    let mut library = Library::new();
    let mut names = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for root in install_roots {
        for name in MovieLibrary::new(root).list_available() {
            if !seen.insert(name.clone()) {
                continue;
            }
            let path = root.join("apt").join(format!("{name}.big"));
            let Ok(archive) = BigArchive::open(&path) else {
                continue;
            };
            let mut apt = None;
            let mut constant = None;
            for entry in archive.entries() {
                let lower = entry.path.as_str().to_ascii_lowercase();
                if lower.ends_with(".apt") {
                    apt = archive.read_entry(entry).ok();
                } else if lower.ends_with(".const") {
                    constant = archive.read_entry(entry).ok();
                }
            }
            let (Some(apt), Some(constant)) = (apt, constant) else {
                continue;
            };
            match library.add(&name, &constant, apt) {
                Ok(_) => names.push(name),
                Err(e) => eprintln!("{name}: {e}"),
            }
        }
    }
    library.link();
    names.sort();
    (Arc::new(library), names)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::float_cmp)]
mod tests;
