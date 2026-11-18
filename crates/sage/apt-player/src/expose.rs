use std::collections::BTreeMap;

use crate::avm::{Halt, Outward, Value};
use crate::instance::InstanceId;
use crate::player::Player;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Readiness {
    Immediate,
    Event(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MovieEvent {
    Command {
        name: String,
        qualified: String,
        args: String,
    },
    Load {
        url: String,
        target: String,
    },
}

impl MovieEvent {
    #[must_use]
    pub fn command(&self) -> Option<&str> {
        match self {
            Self::Command { name, .. } => Some(name),
            Self::Load { .. } => None,
        }
    }

    #[must_use]
    pub fn args(&self) -> Option<&str> {
        match self {
            Self::Command { args, .. } => Some(args),
            Self::Load { .. } => None,
        }
    }

    fn decode(outward: Outward) -> Self {
        match outward {
            Outward::FsCommand { command, args } => Self::Command {
                name: after_scope(&command).to_owned(),
                qualified: command,
                args,
            },
            Outward::GameCode { args, .. } => {
                let mut args = args.into_iter();
                let qualified = args.next().unwrap_or_default();
                Self::Command {
                    name: after_scope(&qualified).to_owned(),
                    qualified,
                    args: args.collect::<Vec<_>>().join(","),
                }
            }
            Outward::GetUrl { url, target } => Self::Load { url, target },
        }
    }
}

fn after_scope(command: &str) -> &str {
    command.rsplit("::").next().unwrap_or(command)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ImageKey {
    Name(String),
    Path(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImageChange<I> {
    Name {
        key: String,
        content: Option<I>,
    },
    Instance {
        instance: InstanceId,
        content: Option<I>,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExposureStats {
    pub ready: bool,
    pub pending_calls: usize,
    pub pending_values: usize,
    pub pending_images: usize,
    pub calls_issued: u64,
    pub values_written: u64,
    pub reasserted: u64,
    pub halts: u64,
}

struct CallBinding {
    function: String,
    args: Vec<Value>,
    landed: bool,
    subject: Option<String>,
    landed_on: Option<InstanceId>,
}

struct ValueBinding {
    value: Value,
    target: Option<InstanceId>,
    landed: bool,
}

struct ImageBinding<I> {
    content: Option<I>,
    target: Option<InstanceId>,
    dirty: bool,
}

pub struct Exposure<I> {
    readiness: Readiness,
    ready: bool,
    calls: BTreeMap<String, CallBinding>,
    fires: Vec<(String, Vec<Value>)>,
    values: BTreeMap<(String, String), ValueBinding>,
    externs: BTreeMap<String, Value>,
    externs_dirty: bool,
    globals: BTreeMap<String, Value>,
    strings: BTreeMap<String, String>,
    string_changes: Vec<(String, Option<String>)>,
    images: BTreeMap<ImageKey, ImageBinding<I>>,
    image_changes: Vec<ImageChange<I>>,
    events: Vec<MovieEvent>,
    stats: ExposureStats,
}

impl<I: Clone + PartialEq> Exposure<I> {
    #[must_use]
    pub fn new(readiness: Readiness) -> Self {
        let ready = readiness == Readiness::Immediate;
        Self {
            readiness,
            ready,
            calls: BTreeMap::new(),
            fires: Vec::new(),
            values: BTreeMap::new(),
            externs: BTreeMap::new(),
            externs_dirty: false,
            globals: BTreeMap::new(),
            strings: BTreeMap::new(),
            string_changes: Vec::new(),
            images: BTreeMap::new(),
            image_changes: Vec::new(),
            events: Vec::new(),
            stats: ExposureStats {
                ready,
                ..ExposureStats::default()
            },
        }
    }

    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    #[must_use]
    pub fn landed(&self, key: &str) -> bool {
        self.calls.get(key).is_some_and(|b| b.landed)
    }

    pub fn call(&mut self, key: &str, function: &str, args: Vec<Value>) {
        self.declare_call(key, None, function, args);
    }

    pub fn call_on(&mut self, key: &str, subject: &str, function: &str, args: Vec<Value>) {
        self.declare_call(key, Some(subject), function, args);
    }

    fn declare_call(&mut self, key: &str, subject: Option<&str>, function: &str, args: Vec<Value>) {
        match self.calls.get_mut(key) {
            Some(b)
                if b.function == function && b.args == args && b.subject.as_deref() == subject => {}
            Some(b) => {
                function.clone_into(&mut b.function);
                b.args = args;
                b.subject = subject.map(str::to_owned);
                b.landed = false;
                b.landed_on = None;
            }
            None => {
                self.calls.insert(
                    key.to_owned(),
                    CallBinding {
                        function: function.to_owned(),
                        args,
                        landed: false,
                        subject: subject.map(str::to_owned),
                        landed_on: None,
                    },
                );
            }
        }
    }

    pub fn global(&mut self, name: &str, value: Value) {
        self.globals.insert(name.to_owned(), value);
    }

    pub fn fire(&mut self, function: &str, args: Vec<Value>) {
        self.fires.push((function.to_owned(), args));
    }

    pub fn value(&mut self, clip: &str, member: &str, value: Value) {
        let key = (clip.to_owned(), member.to_owned());
        match self.values.get_mut(&key) {
            Some(b) if b.value == value => {}
            Some(b) => {
                b.value = value;
                b.landed = false;
            }
            None => {
                self.values.insert(
                    key,
                    ValueBinding {
                        value,
                        target: None,
                        landed: false,
                    },
                );
            }
        }
    }

    pub fn set_extern(&mut self, name: &str, value: Value) {
        if self.externs.get(name) != Some(&value) {
            self.externs.insert(name.to_owned(), value);
            self.externs_dirty = true;
        }
    }

    pub fn string(&mut self, key: &str, value: Option<String>) {
        let changed = match &value {
            Some(v) => self.strings.get(key) != Some(v),
            None => self.strings.contains_key(key),
        };
        if !changed {
            return;
        }
        match &value {
            Some(v) => {
                self.strings.insert(key.to_owned(), v.clone());
            }
            None => {
                self.strings.remove(key);
            }
        }
        self.string_changes.push((key.to_owned(), value));
    }

    pub fn image(&mut self, key: ImageKey, content: Option<I>) {
        match self.images.get_mut(&key) {
            Some(b) if b.content == content => {}
            Some(b) => {
                b.content = content;
                b.dirty = true;
            }
            None => {
                self.images.insert(
                    key,
                    ImageBinding {
                        content,
                        target: None,
                        dirty: true,
                    },
                );
            }
        }
    }

    pub fn sync(&mut self, player: &mut Player) {
        self.read_events(player);
        if self.externs_dirty {
            for (name, value) in &self.externs {
                player.set_extern_value(name, value.clone());
            }
            self.externs_dirty = false;
        }
        self.sync_globals(player);
        self.sync_images(player);
        if self.ready {
            self.sync_values(player);
            self.sync_calls(player);
        }
        self.stats.ready = self.ready;
        self.stats.pending_calls = self
            .calls
            .values()
            .filter(|b| !b.landed)
            .count()
            .saturating_add(self.fires.len());
        self.stats.pending_values = self.values.values().filter(|b| !b.landed).count();
        self.stats.pending_images = self
            .images
            .iter()
            .filter(|(k, b)| matches!(k, ImageKey::Path(_)) && b.target.is_none())
            .count();
    }

    pub fn frame(&mut self, player: &mut Player, ticks: u32) {
        self.sync(player);
        for _ in 0..ticks {
            let _ = player.tick();
        }
        self.sync(player);
    }

    pub fn take_events(&mut self) -> Vec<MovieEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn take_string_changes(&mut self) -> Vec<(String, Option<String>)> {
        std::mem::take(&mut self.string_changes)
    }

    pub fn take_image_changes(&mut self) -> Vec<ImageChange<I>> {
        std::mem::take(&mut self.image_changes)
    }

    #[must_use]
    pub fn pending_image_changes(&self) -> &[ImageChange<I>] {
        &self.image_changes
    }

    #[must_use]
    pub fn stats(&self) -> ExposureStats {
        self.stats
    }

    #[must_use]
    pub fn image_target(&self, key: &ImageKey) -> Option<InstanceId> {
        self.images.get(key).and_then(|b| b.target)
    }

    fn read_events(&mut self, player: &mut Player) {
        for outward in player.take_outward() {
            let event = MovieEvent::decode(outward);
            if let (Readiness::Event(expected), Some(name)) = (&self.readiness, event.command()) {
                if name == expected {
                    if self.ready {
                        for b in self.calls.values_mut() {
                            b.landed = false;
                        }
                    }
                    self.ready = true;
                }
            }
            self.events.push(event);
        }
    }

    fn sync_values(&mut self, player: &mut Player) {
        for ((clip, member), b) in &mut self.values {
            let target = resolve_cached(player, clip, b.target);
            b.target = target;
            let Some(target) = target else {
                b.landed = false;
                continue;
            };
            if player.member(target, member) == b.value {
                b.landed = true;
                continue;
            }
            if player.set_variable(target, member, b.value.clone()).is_ok() {
                self.stats.values_written = self.stats.values_written.saturating_add(1);
                if b.landed {
                    self.stats.reasserted = self.stats.reasserted.saturating_add(1);
                }
                b.landed = true;
            }
        }
    }

    fn sync_globals(&mut self, player: &mut Player) {
        let root = player.root();
        for (name, value) in &self.globals {
            let path = format!("_global.{name}");
            if player.variable(root, &path) != *value
                && player.set_variable(root, &path, value.clone()).is_ok()
            {
                self.stats.values_written = self.stats.values_written.saturating_add(1);
            }
        }
    }

    fn sync_calls(&mut self, player: &mut Player) {
        let root = player.root();
        for b in self.calls.values_mut() {
            let subject = match &b.subject {
                Some(path) => match resolve_path(player, path) {
                    Some(id) => Some(id),
                    None => continue,
                },
                None => None,
            };
            if b.landed && (subject.is_none() || b.landed_on == subject) {
                continue;
            }
            match player.call_defined(root, &b.function, b.args.clone()) {
                Ok(None) => {}
                Ok(Some(_)) => {
                    self.stats.calls_issued = self.stats.calls_issued.saturating_add(1);
                    if b.landed {
                        self.stats.reasserted = self.stats.reasserted.saturating_add(1);
                    }
                    b.landed = true;
                    b.landed_on = subject;
                }
                Err(halt) => {
                    self.stats.halts = self.stats.halts.saturating_add(1);
                    let _: Halt = halt;
                    b.landed = true;
                    b.landed_on = subject;
                }
            }
        }
        let mut waiting = Vec::new();
        for (function, args) in std::mem::take(&mut self.fires) {
            match player.call_defined(root, &function, args.clone()) {
                Ok(None) => waiting.push((function, args)),
                Ok(Some(_)) => {
                    self.stats.calls_issued = self.stats.calls_issued.saturating_add(1);
                }
                Err(_) => self.stats.halts = self.stats.halts.saturating_add(1),
            }
        }
        self.fires = waiting;
    }

    fn sync_images(&mut self, player: &mut Player) {
        for (key, b) in &mut self.images {
            match key {
                ImageKey::Name(name) => {
                    if b.dirty {
                        self.image_changes.push(ImageChange::Name {
                            key: name.clone(),
                            content: b.content.clone(),
                        });
                        b.dirty = false;
                    }
                }
                ImageKey::Path(path) => {
                    let target = resolve_cached(player, path, b.target);
                    if target != b.target {
                        if let Some(old) = b.target {
                            self.image_changes.push(ImageChange::Instance {
                                instance: old,
                                content: None,
                            });
                        }
                        b.dirty = true;
                        b.target = target;
                    }
                    if let (true, Some(instance)) = (b.dirty, b.target) {
                        self.image_changes.push(ImageChange::Instance {
                            instance,
                            content: b.content.clone(),
                        });
                        b.dirty = false;
                    }
                }
            }
        }
    }
}

fn resolve_cached(player: &Player, path: &str, cached: Option<InstanceId>) -> Option<InstanceId> {
    match cached {
        Some(id) if player.is_live(id) => Some(id),
        _ => resolve_path(player, path),
    }
}

#[must_use]
pub fn resolve_path(player: &Player, path: &str) -> Option<InstanceId> {
    let mut at = player.root();
    if path.is_empty() {
        return Some(at);
    }
    for segment in path.split('.') {
        at = match segment.strip_prefix('#').map(str::parse::<i32>) {
            Some(Ok(depth)) => player.instance(at)?.child_at(depth)?,
            _ => player.child_by_name(at, segment)?,
        };
        if !player.is_live(at) {
            return None;
        }
    }
    Some(at)
}
