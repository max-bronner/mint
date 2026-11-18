use std::collections::{BTreeMap, BTreeSet};

use mint_sage_audio::{ClientRng, MusicRequest};
use mint_sage_map::{Condition, Parameter, Script, ScriptAction, ScriptNode, ScriptTree};

use crate::diag::diag;

const FRAMES_PER_SECOND: u32 = mint_sim::SIMULATION_HZ;

const MAX_CALL_DEPTH: u8 = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MusicCommand {
    Replace(MusicRequest),
    Push(MusicRequest),
    Pop { fade_out: bool, fade_in: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Who {
    Local,
    Everyone,
    LocalEnemies,
}

impl Who {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "<Local Player>" => Some(Self::Local),
            "<All Players>" => Some(Self::Everyone),
            "<Local Player's Enemies>" => Some(Self::LocalEnemies),
            _ => None,
        }
    }
}

pub(crate) trait MusicWorld {
    fn local_side(&self) -> Option<&str>;
    fn home_base(&self, who: Who) -> Option<[f32; 2]>;
    fn units_beyond(&self, who: Who, at: [f32; 2], distance: f32) -> u32;
    fn has_science(&self, who: Who, science: &str) -> bool;
    fn objects_with_condition(&self, who: Who, condition: &str) -> Option<u32>;
    fn units_near(&self, who: Who, at: [f32; 2], distance: f32) -> u32;
    fn eva_last_played(&self, event: &str) -> Option<Option<(u32, [f32; 2])>>;
    fn reached_level_cap(&self, who: Who) -> bool;
}

#[derive(Debug, Clone, Copy, Default)]
struct Counter {
    value: i32,
    countdown: bool,
}

#[derive(Debug)]
struct Entry {
    script: Script,
    active: bool,
    next_frame: u64,
    was_true: bool,
}

#[derive(Debug)]
enum Node {
    Script(usize),
    Group {
        name: String,
        active: bool,
        subroutine: bool,
        children: Vec<Node>,
    },
}

pub(crate) struct MusicScripts {
    entries: Vec<Entry>,
    roots: Vec<Node>,
    flags: BTreeMap<String, bool>,
    counters: BTreeMap<String, Counter>,
    places: BTreeMap<String, [f32; 2]>,
    frame: u64,
    rng: ClientRng,
    commands: Vec<MusicCommand>,
    named_once: BTreeSet<String>,
}

impl MusicScripts {
    pub(crate) fn new(tree: ScriptTree, rng: ClientRng) -> Self {
        let mut entries = Vec::new();
        let mut roots = Vec::new();
        for slot in tree.player_slots {
            for node in slot {
                roots.push(compile(node, &mut entries));
            }
        }
        Self {
            entries,
            roots,
            flags: BTreeMap::new(),
            counters: BTreeMap::new(),
            places: BTreeMap::new(),
            frame: 0,
            rng,
            commands: Vec::new(),
            named_once: BTreeSet::new(),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn set_flag(&mut self, name: &str, value: bool) {
        self.flags.insert(name.to_string(), value);
    }

    pub(crate) fn update(&mut self, world: &dyn MusicWorld) -> Vec<MusicCommand> {
        for counter in self.counters.values_mut() {
            if counter.countdown && counter.value >= 0 {
                counter.value = counter.value.saturating_sub(1);
            }
        }
        let roots = std::mem::take(&mut self.roots);
        for node in &roots {
            self.run_node(node, world, 0);
        }
        self.roots = roots;
        self.frame = self.frame.saturating_add(1);
        std::mem::take(&mut self.commands)
    }

    fn run_node(&mut self, node: &Node, world: &dyn MusicWorld, depth: u8) {
        match node {
            Node::Script(index) => {
                let subroutine = self
                    .entries
                    .get(*index)
                    .is_some_and(|e| e.script.subroutine);
                if !subroutine {
                    self.execute(*index, world, depth);
                }
            }
            Node::Group {
                active,
                subroutine,
                children,
                ..
            } => {
                if *active && !*subroutine {
                    for child in children {
                        self.run_node(child, world, depth);
                    }
                }
            }
        }
    }

    fn execute(&mut self, index: usize, world: &dyn MusicWorld, depth: u8) {
        let frame = self.frame;
        let Some(entry) = self.entries.get_mut(index) else {
            return;
        };
        if !entry.active || !entry.script.normal || frame < entry.next_frame {
            return;
        }
        if let Ok(delay) = u64::try_from(entry.script.delay_evaluation_seconds) {
            if delay > 0 {
                entry.next_frame = frame.saturating_add(delay * u64::from(FRAMES_PER_SECOND));
            }
        }
        let script = entry.script.clone();
        let truth = self.evaluate(&script, world);
        if let Some(entry) = self.entries.get_mut(index) {
            if truth && !entry.was_true {
                diag!("music: script {} became true", script.name);
            }
            entry.was_true = truth;
        }
        let actions = if truth {
            &script.actions
        } else {
            &script.actions_false
        };
        let ran = truth || !script.actions_false.is_empty();
        for action in actions {
            self.act(action, world, depth);
        }
        if ran && script.one_shot {
            if let Some(entry) = self.entries.get_mut(index) {
                entry.active = false;
            }
        }
    }

    fn evaluate(&mut self, script: &Script, world: &dyn MusicWorld) -> bool {
        script.or_conditions.iter().any(|or| {
            !or.conditions.is_empty()
                && or
                    .conditions
                    .iter()
                    .all(|c| !c.enabled || self.condition(c, world))
        })
    }

    fn condition(&mut self, condition: &Condition, world: &dyn MusicWorld) -> bool {
        let p = &condition.params;
        match condition.name.as_str() {
            "CONDITION_TRUE" => true,
            "CONDITION_FALSE" => false,
            "FLAG" => self.flag(text(p, 0)) == (int(p, 1) != 0),
            "COUNTER" => compare(self.counter(text(p, 0)).value, int(p, 1), int(p, 2)),
            "TIMER_EXPIRED" => {
                let counter = self.counter(text(p, 0));
                counter.countdown && counter.value < 1
            }
            "SKIRMISH_PLAYER_FACTION" => match Who::parse(text(p, 0)) {
                Some(Who::Local) => world.local_side() == Some(text(p, 1)),
                _ => self.cannot(&condition.name, text(p, 0)),
            },
            "PLAYER_HAS_NUMBER_UNITS_DISTANCE_FROM_OBJECT" => {
                let Some(who) = Who::parse(text(p, 0)) else {
                    return self.cannot(&condition.name, text(p, 0));
                };
                let Some(at) = self.places.get(text(p, 4)).copied() else {
                    return false;
                };
                let count = world.units_beyond(who, at, real(p, 3));
                compare(
                    i32::try_from(count).unwrap_or(i32::MAX),
                    int(p, 1),
                    int(p, 2),
                )
            }
            "PLAYER_ACQUIRED_SCIENCE" => match Who::parse(text(p, 0)) {
                Some(who) => world.has_science(who, text(p, 1)),
                None => self.cannot(&condition.name, text(p, 0)),
            },
            "PLAYER_HAS_NUMBER_OBJECTS_WITH_MODELCONDITION" => {
                let answer = Who::parse(text(p, 0))
                    .and_then(|who| world.objects_with_condition(who, text(p, 1)));
                match answer {
                    Some(count) => compare(
                        i32::try_from(count).unwrap_or(i32::MAX),
                        int(p, 2),
                        int(p, 3),
                    ),
                    None => self.cannot(&condition.name, text(p, 1)),
                }
            }
            "PLAYER_HAS_REACHED_LEVEL_CAP" => match Who::parse(text(p, 0)) {
                Some(who) => world.reached_level_cap(who),
                None => self.cannot(&condition.name, text(p, 0)),
            },
            "HAS_EVA_EVENT_PLAYED_IN_LAST_N_SECONDS" => match world.eva_last_played(text(p, 0)) {
                Some(played) => played.is_some_and(|(ago, _)| ago <= eva_frames(real(p, 1))),
                None => self.cannot(&condition.name, text(p, 0)),
            },
            "IS_NUM_OF_UNITS_BELONGING_TO_PLAYER_NEAR_EVA_EVENT_LAST_PLAYED_LOCATION_COMPARISON_INT" => {
                let Some(played) = world.eva_last_played(text(p, 2)) else {
                    return self.cannot(&condition.name, text(p, 2));
                };
                let count = match (played, Who::parse(text(p, 0))) {
                    (Some((_, at)), Some(who)) => world.units_near(who, at, real(p, 1)),
                    _ => 0,
                };
                compare(
                    i32::try_from(count).unwrap_or(i32::MAX),
                    int(p, 3),
                    int(p, 4),
                )
            }
            other => {
                let other = other.to_string();
                self.cannot(&other, "")
            }
        }
    }

    fn act(&mut self, action: &ScriptAction, world: &dyn MusicWorld, depth: u8) {
        let p = &action.params;
        match action.name.as_str() {
            "SET_FLAG" => self.set_flag(text(p, 0), int(p, 1) != 0),
            "SET_COUNTER" => self.counter_mut(text(p, 0)).value = int(p, 1),
            "INCREMENT_COUNTER" => {
                let by = int(p, 0);
                let counter = self.counter_mut(text(p, 1));
                counter.value = counter.value.saturating_add(by);
            }
            "SET_MILLISECOND_TIMER" => self.start_timer(text(p, 0), real(p, 1)),
            "SET_RANDOM_MSEC_TIMER" => {
                let seconds = self.rng.next_range(real(p, 1), real(p, 2));
                self.start_timer(text(p, 0), seconds);
            }
            "SET_COUNTER_TO_CLIENT_RANDOM_VALUE" => {
                let (low, high) = (int(p, 1), int(p, 2));
                let span = u32::try_from(high.saturating_sub(low)).unwrap_or(0);
                let roll = self
                    .rng
                    .next_index(usize::try_from(span).unwrap_or(0).saturating_add(1))
                    .and_then(|r| i32::try_from(r).ok())
                    .unwrap_or(0);
                self.counter_mut(text(p, 0)).value = low.saturating_add(roll);
            }
            "ENABLE_SCRIPT" => self.set_active(text(p, 0), true),
            "DISABLE_SCRIPT" => self.set_active(text(p, 0), false),
            "CALL_SUBROUTINE" => self.call(text(p, 0), world, depth),
            "MUSIC_SCRIPT_SET_TRACK" => {
                let request = MusicRequest {
                    fade_out: int(p, 1) != 0,
                    fade_in: int(p, 2) != 0,
                    ..MusicRequest::forever(text(p, 0))
                };
                self.commands.push(MusicCommand::Replace(request));
            }
            "MUSIC_SCRIPT_PLAY_TRACK_FINITE_TIMES_AND_NOTIFY"
            | "MUSIC_SCRIPT_PLAY_TRACK_FINITE_TIMES" => {
                self.commands.push(MusicCommand::Replace(finite(p)));
            }
            "MUSIC_SCRIPT_PUSH_TRACK_FINITE_TIMES_AND_NOTIFY"
            | "MUSIC_SCRIPT_PUSH_TRACK_FINITE_TIMES" => {
                self.commands.push(MusicCommand::Push(finite(p)));
            }
            "MUSIC_SCRIPT_POP_MUSIC" => self.commands.push(MusicCommand::Pop {
                fade_out: int(p, 0) != 0,
                fade_in: int(p, 1) != 0,
            }),
            "FIND_HOME_BASE_OF_PLAYER" => {
                let place = Who::parse(text(p, 0)).and_then(|who| world.home_base(who));
                match place {
                    Some(at) => {
                        self.places.insert(text(p, 1).to_string(), at);
                    }
                    None => {
                        self.places.remove(text(p, 1));
                    }
                }
            }
            "NO_OP" => {}
            other => {
                let other = other.to_string();
                self.cannot(&other, "");
            }
        }
    }

    fn call(&mut self, name: &str, world: &dyn MusicWorld, depth: u8) {
        if depth >= MAX_CALL_DEPTH {
            self.cannot("CALL_SUBROUTINE nesting", name);
            return;
        }
        let depth = depth.saturating_add(1);
        let roots = std::mem::take(&mut self.roots);
        let group = find_group(&roots, name).map(|node| match node {
            Node::Group {
                active,
                subroutine,
                children,
                ..
            } => (*active && *subroutine, script_indices(children)),
            Node::Script(_) => (false, Vec::new()),
        });
        self.roots = roots;
        if let Some((runs, scripts)) = group {
            if runs {
                for index in scripts {
                    let subroutine = self.entries.get(index).is_some_and(|e| e.script.subroutine);
                    if !subroutine {
                        self.execute(index, world, depth);
                    }
                }
            }
            return;
        }
        if let Some(index) = self.script_index(name) {
            if self.entries.get(index).is_some_and(|e| e.script.subroutine) {
                self.execute(index, world, depth);
            }
        }
    }

    fn set_active(&mut self, name: &str, active: bool) {
        if let Some(index) = self.script_index(name) {
            if let Some(entry) = self.entries.get_mut(index) {
                entry.active = active;
            }
            return;
        }
        set_group_active(&mut self.roots, name, active);
    }

    fn script_index(&self, name: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.script.name == name)
    }

    fn flag(&self, name: &str) -> bool {
        self.flags.get(name).copied().unwrap_or(false)
    }

    fn counter(&self, name: &str) -> Counter {
        self.counters.get(name).copied().unwrap_or_default()
    }

    fn counter_mut(&mut self, name: &str) -> &mut Counter {
        self.counters.entry(name.to_string()).or_default()
    }

    fn start_timer(&mut self, name: &str, seconds: f32) {
        #[allow(clippy::cast_possible_truncation)]
        let frames = (seconds.max(0.0) * FRAMES_PER_SECOND as f32).ceil() as i32;
        let counter = self.counter_mut(name);
        counter.value = frames;
        counter.countdown = true;
    }

    fn cannot(&mut self, what: &str, detail: &str) -> bool {
        let key = format!("{what} {detail}");
        if self.named_once.insert(key) {
            diag!("music: script {what} {detail} - not answered by mint yet, read as false");
        }
        false
    }
}

fn compile(node: ScriptNode, entries: &mut Vec<Entry>) -> Node {
    match node {
        ScriptNode::Script(script) => {
            let index = entries.len();
            entries.push(Entry {
                active: script.active,
                script,
                next_frame: 0,
                was_true: false,
            });
            Node::Script(index)
        }
        ScriptNode::Group(group) => Node::Group {
            name: group.name,
            active: group.active,
            subroutine: group.is_subroutine,
            children: group
                .children
                .into_iter()
                .map(|child| compile(child, entries))
                .collect(),
        },
    }
}

fn find_group<'a>(nodes: &'a [Node], name: &str) -> Option<&'a Node> {
    nodes.iter().find_map(|node| match node {
        Node::Group {
            name: own,
            children,
            ..
        } => {
            if own == name {
                Some(node)
            } else {
                find_group(children, name)
            }
        }
        Node::Script(_) => None,
    })
}

fn set_group_active(nodes: &mut [Node], name: &str, value: bool) -> bool {
    nodes.iter_mut().any(|node| match node {
        Node::Group {
            name: own,
            active,
            children,
            ..
        } => {
            if own == name {
                *active = value;
                true
            } else {
                set_group_active(children, name, value)
            }
        }
        Node::Script(_) => false,
    })
}

fn script_indices(nodes: &[Node]) -> Vec<usize> {
    nodes
        .iter()
        .filter_map(|node| match node {
            Node::Script(index) => Some(*index),
            Node::Group { .. } => None,
        })
        .collect()
}

fn finite(p: &[Parameter]) -> MusicRequest {
    let notify = text(p, 4);
    MusicRequest {
        times: Some(u32::try_from(int(p, 1)).unwrap_or(1).max(1)),
        fade_out: int(p, 2) != 0,
        fade_in: int(p, 3) != 0,
        notify: (!notify.is_empty()).then(|| notify.to_string()),
        ..MusicRequest::forever(text(p, 0))
    }
}

fn eva_frames(seconds: f32) -> u32 {
    #[allow(clippy::cast_precision_loss)]
    let hz = crate::game_data::RETAIL_LOGIC_HZ as f32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = (seconds.max(0.0) * hz).ceil() as u32;
    frames
}

fn compare(value: i32, comparison: i32, against: i32) -> bool {
    match comparison {
        0 => value < against,
        1 => value <= against,
        2 => value == against,
        3 => value >= against,
        4 => value > against,
        5 => value != against,
        _ => false,
    }
}

fn text(p: &[Parameter], at: usize) -> &str {
    match p.get(at) {
        Some(Parameter::Value { string_value, .. }) => string_value,
        _ => "",
    }
}

fn int(p: &[Parameter], at: usize) -> i32 {
    match p.get(at) {
        Some(Parameter::Value { int_value, .. }) => *int_value,
        _ => 0,
    }
}

fn real(p: &[Parameter], at: usize) -> f32 {
    match p.get(at) {
        Some(Parameter::Value { real_value, .. }) => *real_value,
        _ => 0.0,
    }
}

pub(crate) struct MatchFacts<'a> {
    pub(crate) local: mint_sim::PlayerId,
    pub(crate) units: &'a [crate::render_state::UnitView],
    pub(crate) kinds: &'a crate::templates::KindTable,
    pub(crate) rules: &'a crate::rules::RotwkRules,
    pub(crate) world: &'a mint_sim::World,
}

const HOME_BASE_KIND: &str = "COMMANDCENTER";

impl MatchFacts<'_> {
    fn players(&self, who: Who) -> impl Iterator<Item = mint_sim::PlayerId> + '_ {
        self.rules
            .player_sides()
            .iter()
            .map(|(player, _)| *player)
            .filter(move |player| match who {
                Who::Local => *player == self.local,
                Who::Everyone => true,
                Who::LocalEnemies => self.rules.hostile(self.local, *player),
            })
    }

    fn owned_by(&self, who: Who, owner: mint_sim::PlayerId) -> bool {
        self.players(who).any(|player| player == owner)
    }

    fn has_kind(&self, unit: &crate::render_state::UnitView, name: &str) -> bool {
        unit.kind
            .and_then(|kind| self.kinds.get(crate::unit::Kind(kind)))
            .is_some_and(|entry| entry.record.has_kind(name))
    }
}

impl MusicWorld for MatchFacts<'_> {
    fn local_side(&self) -> Option<&str> {
        self.rules
            .player_sides()
            .iter()
            .find(|(player, _)| *player == self.local)
            .map(|(_, side)| side.as_str())
    }

    fn home_base(&self, who: Who) -> Option<[f32; 2]> {
        self.units
            .iter()
            .filter(|u| !u.dying && self.owned_by(who, u.owner))
            .filter(|u| self.has_kind(u, HOME_BASE_KIND))
            .min_by_key(|u| u.entity.index())
            .map(|u| [u.position.x, u.position.y])
    }

    fn units_beyond(&self, who: Who, at: [f32; 2], distance: f32) -> u32 {
        let far = self
            .units
            .iter()
            .filter(|u| !u.dying && u.health.is_some() && self.owned_by(who, u.owner))
            .filter(|u| !u.kind_of.contains(mint_sage_asset::KindOf::STRUCTURE))
            .filter(|u| {
                let (dx, dy) = (u.position.x - at[0], u.position.y - at[1]);
                (dx * dx + dy * dy).sqrt() >= distance
            })
            .count();
        u32::try_from(far).unwrap_or(u32::MAX)
    }

    fn has_science(&self, who: Who, science: &str) -> bool {
        let Some(ids) = self.rules.ids() else {
            return false;
        };
        self.players(who)
            .any(|player| self.rules.holds_science(self.world, ids, player, science))
    }

    fn objects_with_condition(&self, who: Who, condition: &str) -> Option<u32> {
        let wanted = mint_sage_scene::Conditions::from_token(condition)?;
        let held = self
            .units
            .iter()
            .filter(|u| self.owned_by(who, u.owner) && u.conditions.contains(wanted))
            .count();
        Some(u32::try_from(held).unwrap_or(u32::MAX))
    }

    fn units_near(&self, who: Who, at: [f32; 2], distance: f32) -> u32 {
        let near = self
            .units
            .iter()
            .filter(|u| self.owned_by(who, u.owner))
            .filter(|u| {
                UNIT_KINDS.iter().any(|k| self.has_kind(u, k))
                    && !NOT_UNIT_KINDS.iter().any(|k| self.has_kind(u, k))
            })
            .filter(|u| {
                let (dx, dy) = (u.position.x - at[0], u.position.y - at[1]);
                (dx * dx + dy * dy).sqrt() - u.radius <= distance
            })
            .count();
        u32::try_from(near).unwrap_or(u32::MAX)
    }

    fn eva_last_played(&self, _event: &str) -> Option<Option<(u32, [f32; 2])>> {
        None
    }

    fn reached_level_cap(&self, who: Who) -> bool {
        let Some(ids) = self.rules.ids() else {
            return false;
        };
        self.players(who)
            .any(|player| self.rules.reached_level_cap(self.world, ids, player))
    }
}

const UNIT_KINDS: [&str; 5] = ["INFANTRY", "CAVALRY", "MONSTER", "MACHINE", "AIRCRAFT"];
const NOT_UNIT_KINDS: [&str; 3] = ["HORDE", "STRUCTURE", "INERT"];

#[cfg(test)]
mod tests;
