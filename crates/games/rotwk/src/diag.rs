use std::fmt;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::{Once, PoisonError, RwLock};

pub(crate) const CATEGORIES: &[&str] = &[
    "ability",
    "allies",
    "ambient",
    "amphibious",
    "approach",
    "apt",
    "attach",
    "bird",
    "busy",
    "button sound",
    "cah",
    "capture",
    "carryover",
    "castle",
    "chase",
    "claims",
    "climb",
    "clips",
    "command",
    "construct",
    "construction",
    "contact",
    "contain",
    "crate",
    "crush",
    "crush notice",
    "damage-creation",
    "damagefx",
    "death",
    "debris",
    "debug",
    "deploy",
    "dest",
    "detect",
    "disguise",
    "door",
    "emotion",
    "eva",
    "fade",
    "fire",
    "flank",
    "forest",
    "formation",
    "foundation",
    "gadget",
    "gate",
    "group bonus",
    "guard",
    "height",
    "hero",
    "hit reaction",
    "hold",
    "input",
    "knockback",
    "lua",
    "melee",
    "members",
    "misc",
    "music",
    "nugget",
    "object-creation",
    "obstacles",
    "order",
    "path",
    "portal",
    "power",
    "production",
    "projectile",
    "rampage",
    "rank",
    "rebuild",
    "repair",
    "replace",
    "replenish",
    "report",
    "script",
    "script trace",
    "sell",
    "shot",
    "shroud",
    "siege",
    "skirmish-ai",
    "sound field",
    "spawn",
    "spawn-buildings",
    "spell",
    "spell book",
    "stance",
    "stats",
    "stealth",
    "swing",
    "templates",
    "tint",
    "turn",
    "ui sound",
    "units",
    "upgrade",
    "vision",
    "voice vote",
    "wall",
    "weapon",
    "weather",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Filter {
    all: bool,
    include: Vec<String>,
    exclude: Vec<String>,
}

impl Filter {
    pub(crate) fn parse(spec: &str) -> Self {
        let mut filter = Self::default();
        for item in spec
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            let item = item.to_ascii_lowercase();
            match item.as_str() {
                "1" | "all" => filter.all = true,
                "0" | "off" => {}
                _ => match item.strip_prefix('-') {
                    Some(name) => filter.exclude.push(name.trim().to_owned()),
                    None => filter.include.push(item),
                },
            }
        }
        filter
    }

    pub(crate) fn admits(&self, category: &str) -> bool {
        if self.exclude.iter().any(|name| name == category) {
            return false;
        }
        self.all || self.include.iter().any(|name| name == category)
    }

    pub(crate) fn unknown(&self) -> Vec<&str> {
        self.include
            .iter()
            .chain(&self.exclude)
            .map(String::as_str)
            .filter(|name| !CATEGORIES.contains(name))
            .collect()
    }

    fn mode(&self) -> u8 {
        if self.all && self.exclude.is_empty() {
            ALL
        } else if self.all || !self.include.is_empty() {
            SOME
        } else {
            OFF
        }
    }
}

impl std::fmt::Display for Filter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut items: Vec<String> = Vec::new();
        if self.all {
            items.push("all".to_owned());
        }
        items.extend(self.include.iter().cloned());
        items.extend(self.exclude.iter().map(|name| format!("-{name}")));
        if items.is_empty() {
            f.write_str("off")
        } else {
            f.write_str(&items.join(","))
        }
    }
}

pub(crate) const BLACK_BOX: &[&str] = &[
    "capture",
    "command",
    "construct",
    "construction",
    "death",
    "deploy",
    "hero",
    "object-creation",
    "order",
    "production",
    "rebuild",
    "replace",
    "sell",
    "skirmish-ai",
    "spawn",
    "spawn-buildings",
    "spell",
    "spell book",
    "upgrade",
];

const OFF: u8 = 0;
const ALL: u8 = 1;
const SOME: u8 = 2;

static INIT: Once = Once::new();
static GENERATION: AtomicU32 = AtomicU32::new(0);
static MODE: AtomicU8 = AtomicU8::new(OFF);
static FILTER: RwLock<Filter> = RwLock::new(Filter {
    all: false,
    include: Vec::new(),
    exclude: Vec::new(),
});

fn mode() -> u8 {
    INIT.call_once(|| {
        let spec = std::env::var("MINT_ROTWK_DIAG").unwrap_or_default();
        if let Err(message) = install(&spec) {
            println!("{message}");
        }
    });
    MODE.load(Ordering::Acquire)
}

fn install(spec: &str) -> Result<(), String> {
    let filter = Filter::parse(spec);
    let unknown = filter.unknown().join(", ");
    let mode = filter.mode();
    *FILTER.write().unwrap_or_else(PoisonError::into_inner) = filter;
    MODE.store(mode, Ordering::Release);
    GENERATION.fetch_add(1, Ordering::Release);
    if unknown.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "diag: no such category: {unknown}; known: {}",
            CATEGORIES.join(", ")
        ))
    }
}

pub(crate) fn set(spec: &str) -> Result<String, String> {
    mode();
    install(spec)?;
    Ok(format!("diag {}", current()))
}

fn current() -> String {
    FILTER
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .to_string()
}

pub(crate) fn wants(category: &str) -> bool {
    match mode() {
        OFF => false,
        ALL => true,
        _ => FILTER
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .admits(category),
    }
}

pub(crate) fn active(category: &str) -> bool {
    wants(category) || crate::capture::is_capturing()
}

pub(crate) fn prefix(text: &str) -> Option<&str> {
    let (head, _) = text.split_once(':')?;
    let mut chars = head.chars();
    let first = chars.next()?;
    (first.is_ascii_lowercase()
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || " -_".contains(c)))
    .then_some(head)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Route {
    Skip,
    Keep,
    Print,
    ByText,
}

const ROUTE_BITS: u32 = 2;

impl Route {
    const fn bits(self) -> u32 {
        match self {
            Self::Skip => 0,
            Self::Keep => 1,
            Self::Print => 2,
            Self::ByText => 3,
        }
    }

    const fn of_bits(bits: u32) -> Self {
        match bits & 3 {
            0 => Self::Skip,
            1 => Self::Keep,
            2 => Self::Print,
            _ => Self::ByText,
        }
    }
}

fn route_of_category(category: &str) -> Route {
    if wants(category) {
        Route::Print
    } else if BLACK_BOX.contains(&category) {
        Route::Keep
    } else {
        Route::Skip
    }
}

pub(crate) fn route_of(format: &'static str) -> Route {
    match (mode(), prefix(format)) {
        (_, Some(category)) => route_of_category(category),
        (OFF, None) => Route::Skip,
        (ALL, None) => Route::Print,
        (_, None) => Route::ByText,
    }
}

pub(crate) struct Site {
    format: &'static str,
    cached: AtomicU32,
}

impl Site {
    pub(crate) const fn new(format: &'static str) -> Self {
        Self {
            format,
            cached: AtomicU32::new(0),
        }
    }

    #[inline]
    pub(crate) fn route(&self) -> Route {
        let generation = GENERATION.load(Ordering::Relaxed);
        let cached = self.cached.load(Ordering::Relaxed);
        if generation != 0 && cached >> ROUTE_BITS == generation & (u32::MAX >> ROUTE_BITS) {
            return Route::of_bits(cached);
        }
        self.recompute()
    }

    #[cold]
    fn recompute(&self) -> Route {
        mode();
        let generation = GENERATION.load(Ordering::Acquire);
        let route = route_of(self.format);
        self.cached
            .store((generation << ROUTE_BITS) | route.bits(), Ordering::Relaxed);
        route
    }
}

pub(crate) fn emit(route: Route, line: fmt::Arguments<'_>) {
    let capturing = crate::capture::is_capturing();
    match route {
        Route::Skip => {
            if capturing {
                crate::capture::write(&line.to_string());
            }
        }
        Route::Keep => {
            crate::crash::log(line);
            if capturing {
                crate::capture::write(&line.to_string());
            }
        }
        Route::Print => {
            crate::crash::log(line);
            println!("{line}");
            if capturing {
                crate::capture::write(&line.to_string());
            }
        }
        Route::ByText => {
            let text = line.to_string();
            match route_of_category(prefix(&text).unwrap_or("misc")) {
                Route::Print => {
                    crate::crash::log(format_args!("{text}"));
                    println!("{text}");
                }
                Route::Keep => crate::crash::log(format_args!("{text}")),
                Route::Skip | Route::ByText => {}
            }
            if capturing {
                crate::capture::write(&text);
            }
        }
    }
}

macro_rules! diag {
    ($format:literal $($arg:tt)*) => {{
        static SITE: $crate::diag::Site = $crate::diag::Site::new($format);
        let route = SITE.route();
        if route != $crate::diag::Route::Skip || $crate::capture::is_capturing() {
            $crate::diag::emit(route, format_args!($format $($arg)*));
        }
    }};
}

pub(crate) use diag;

#[cfg(test)]
mod tests;
