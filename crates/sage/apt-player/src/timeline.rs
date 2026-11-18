use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use mint_sage_apt::{records_for_state, Block, Character, FrameItem, PlaceObject};

use crate::actions::{clip_event, ActionQueue, ActionSource, QueuedAction};
use crate::arena::Arena;
use crate::avm::heap::Heap;
use crate::avm::object::Native;
use crate::avm::{natives, Outward, ScriptStats, VmLimits};
use crate::geom::{ColorTransform, Matrix};
use crate::instance::{flags, ButtonVisual, ClipState, Instance, InstanceId, InstanceKind};
use crate::library::{Library, MovieId, TimelineId};
use crate::player::{Counters, EventKind, Limits, TimelineEvent};
use crate::Error;

pub const AVM_DEPTH_BIAS: i32 = 16384;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PlaceAction {
    Place(u32),
    Replace(u32),
    Modify,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Placement<'a> {
    pub(crate) action: PlaceAction,
    pub(crate) depth: i32,
    pub(crate) matrix: Option<Matrix>,
    pub(crate) color: Option<ColorTransform>,
    pub(crate) ratio: Option<f32>,
    pub(crate) name: Option<&'a str>,
    pub(crate) clip_depth: Option<i32>,
    pub(crate) clip_actions: Option<u32>,
}

pub(crate) fn placement(po: &PlaceObject) -> Option<Placement<'_>> {
    let action = match (po.flags.move_existing(), po.character) {
        (false, Some(id)) => PlaceAction::Place(id),
        (true, Some(id)) => PlaceAction::Replace(id),
        (true, None) => PlaceAction::Modify,
        (false, None) => return None,
    };
    Some(Placement {
        action,
        depth: po.depth,
        matrix: po.matrix.as_ref().map(Matrix::from_placement),
        color: po
            .color_transform
            .map(|mult| ColorTransform::from_apt(mult, po.color_add.unwrap_or(0))),
        ratio: po.ratio,
        name: po.name.as_deref(),
        clip_depth: po.clip_depth,
        clip_actions: po.clip_actions_offset,
    })
}

#[derive(Debug, Clone)]
pub(crate) struct GotoCmd<'a> {
    frame: u16,
    p: Placement<'a>,
    index: usize,
}

impl<'a> GotoCmd<'a> {
    fn new(frame: u16, mut p: Placement<'a>, is_rewind: bool, index: usize) -> Self {
        if is_rewind && matches!(p.action, PlaceAction::Place(_)) {
            p.matrix.get_or_insert(Matrix::IDENTITY);
            p.color.get_or_insert(ColorTransform::IDENTITY);
            p.ratio.get_or_insert(0.0);
        }
        Self { frame, p, index }
    }

    fn merge(&mut self, next: Self) {
        if !matches!(next.p.action, PlaceAction::Modify) {
            self.p.action = next.p.action;
            self.frame = next.frame;
        }
        if next.p.matrix.is_some() {
            self.p.matrix = next.p.matrix;
        }
        if next.p.color.is_some() {
            self.p.color = next.p.color;
        }
        if next.p.ratio.is_some() {
            self.p.ratio = next.p.ratio;
        }
    }
}

pub(crate) struct Stage {
    pub(crate) arena: Arena<Instance>,
    pub(crate) exec: VecDeque<InstanceId>,
    pub(crate) scratch: Vec<InstanceId>,
    pub(crate) actions: ActionQueue,
    pub(crate) limits: Limits,
    pub(crate) instance_counter: u32,
    pub(crate) background: Option<u32>,
    pub(crate) counters: Counters,
    pub(crate) events: Option<Vec<TimelineEvent>>,
    pub(crate) first_error: Option<Error>,
    pub(crate) init_run: BTreeSet<TimelineId>,
    pub(crate) scripts_enabled: bool,
    pub(crate) root: Option<InstanceId>,
    pub(crate) heap: Heap,
    pub(crate) vm_limits: VmLimits,
    pub(crate) blocks: BTreeMap<(MovieId, u32), Option<Arc<Block>>>,
    pub(crate) outward: Vec<Outward>,
    pub(crate) sync_host: Option<Box<dyn crate::avm::SyncHost>>,
    pub(crate) script_stats: ScriptStats,
    pub(crate) budget: u64,
    pub(crate) record_calls: bool,
    pub(crate) traces: Option<Vec<String>>,
    pub(crate) rng: u64,
    pub(crate) pointer: [f32; 2],
    pub(crate) pending_removal: Vec<InstanceId>,
    pub(crate) writing_bound_text: bool,
    pub(crate) timers: Vec<crate::avm::host::Timer>,
    pub(crate) next_timer: i32,
    pub(crate) mouse: crate::mouse::MouseData,
    pub(crate) empty_pool: Arc<[crate::avm::Value]>,
    pub(crate) externs: std::collections::HashMap<Arc<str>, crate::avm::Value>,
    pub(crate) extern_object: Option<crate::avm::ObjectId>,
    pub(crate) native_externs: std::collections::BTreeSet<Arc<str>>,
    pub(crate) native_extern_writes: Vec<(Arc<str>, crate::avm::Value)>,
    pub(crate) pending_loads: Vec<crate::load::LoadRequest>,
    pub(crate) loaded: Vec<(InstanceId, MovieId)>,
}

impl Stage {
    pub(crate) fn new(limits: Limits, scripts_enabled: bool) -> Result<Self, Error> {
        let vm_limits = limits.vm;
        let heap = Heap::new(vm_limits.max_objects, vm_limits.max_scopes).map_err(|full| {
            Error::Limit {
                what: full.what,
                limit: full.cap,
            }
        })?;
        let mut stage = Self {
            arena: Arena::with_cap(limits.max_instances),
            exec: VecDeque::new(),
            scratch: Vec::new(),
            actions: ActionQueue::with_cap(limits.max_queued_actions),
            limits,
            instance_counter: 1,
            background: None,
            counters: Counters::default(),
            events: None,
            first_error: None,
            init_run: BTreeSet::new(),
            scripts_enabled,
            root: None,
            heap,
            vm_limits,
            blocks: BTreeMap::new(),
            outward: Vec::new(),
            sync_host: None,
            script_stats: ScriptStats::default(),
            budget: 0,
            record_calls: false,
            traces: None,
            rng: 0x9E37_79B9_7F4A_7C15,
            pointer: [0.0; 2],
            pending_removal: Vec::new(),
            writing_bound_text: false,
            timers: Vec::new(),
            next_timer: 1,
            mouse: crate::mouse::MouseData::default(),
            empty_pool: Arc::from(Vec::new()),
            externs: std::collections::HashMap::new(),
            extern_object: None,
            native_externs: std::collections::BTreeSet::new(),
            native_extern_writes: Vec::new(),
            pending_loads: Vec::new(),
            loaded: Vec::new(),
        };
        if scripts_enabled {
            natives::install(&mut stage);
        }
        Ok(stage)
    }

    pub(crate) fn fail(&mut self, error: Error) {
        self.counters.limit_refusals = self.counters.limit_refusals.saturating_add(1);
        if self.first_error.is_none() {
            self.first_error = Some(error);
        }
    }

    fn event(&mut self, parent: InstanceId, depth: i32, kind: EventKind) {
        if let Some(events) = &mut self.events {
            events.push(TimelineEvent {
                parent,
                depth,
                kind,
            });
        }
    }

    pub(crate) fn push_action(&mut self, action: QueuedAction) {
        if self.actions.push(action) {
            self.counters.actions_queued = self.counters.actions_queued.saturating_add(1);
        } else {
            self.fail(Error::Limit {
                what: "queued actions",
                limit: self.limits.max_queued_actions,
            });
        }
    }

    fn child_at(&self, parent: InstanceId, depth: i32) -> Option<InstanceId> {
        self.arena.get(parent)?.child_at(depth)
    }

    fn current_frame(&self, id: InstanceId) -> u16 {
        self.arena
            .get(id)
            .and_then(Instance::clip)
            .map_or(0, |c| c.current_frame)
    }

    fn set_current_frame(&mut self, id: InstanceId, frame: u16) {
        if let Some(clip) = self.arena.get_mut(id).and_then(Instance::clip_mut) {
            clip.current_frame = frame;
        }
    }

    pub(crate) fn run_frame_avm1(&mut self, lib: &Library, id: InstanceId) {
        let Some(clip) = self.arena.get_mut(id).and_then(Instance::clip_mut) else {
            return;
        };
        let event = if clip.initialized {
            clip_event::ENTER_FRAME
        } else {
            clip.initialized = true;
            clip_event::LOAD
        };
        let playing = clip.playing;
        self.queue_clip_event(lib, id, event);
        if event == clip_event::ENTER_FRAME {
            self.queue_method(id, "onEnterFrame");
        }
        if playing {
            self.run_frame_internal(lib, id, true);
        }
    }

    pub(crate) fn queue_clip_event(&mut self, lib: &Library, id: InstanceId, event: u32) {
        let Some((movie, offset)) = self
            .arena
            .get(id)
            .and_then(Instance::clip)
            .and_then(|c| c.clip_actions)
        else {
            return;
        };
        for handler in lib.clip_actions(movie, offset) {
            if handler.events & event != 0 {
                self.push_action(QueuedAction {
                    clip: id,
                    movie,
                    source: ActionSource::ClipEvent {
                        event,
                        block: handler.block_offset,
                    },
                    is_unload: event == clip_event::UNLOAD,
                });
            }
        }
    }

    pub(crate) fn run_frame_internal(&mut self, lib: &Library, id: InstanceId, run_display: bool) {
        let Some(clip) = self.arena.get(id).and_then(Instance::clip).copied() else {
            return;
        };
        let Some(timeline) = lib.timeline(clip.timeline) else {
            return;
        };
        let count = timeline.frame_count();
        if clip.current_frame < count {
            self.set_current_frame(id, clip.current_frame.saturating_add(1));
        } else if count <= 1 {
            if let Some(c) = self.arena.get_mut(id).and_then(Instance::clip_mut) {
                c.playing = false;
            }
            return;
        } else {
            self.run_goto(lib, id, 1, true);
            return;
        }
        let number = clip.current_frame.saturating_add(1);
        let Some(frame) = timeline.frame(number) else {
            return;
        };
        self.counters.frames_run = self.counters.frames_run.saturating_add(1);
        let movie = timeline.movie;
        for item in &frame.items {
            match item {
                FrameItem::Action(action) => self.push_action(QueuedAction {
                    clip: id,
                    movie,
                    source: ActionSource::Frame {
                        block: action.block_offset,
                    },
                    is_unload: false,
                }),
                FrameItem::PlaceObject(po) if run_display => self.place_object(lib, id, movie, po),
                FrameItem::RemoveObject(ro) if run_display => self.remove_depth(lib, id, ro.depth),
                FrameItem::BackgroundColor(bg) => {
                    self.background.get_or_insert(bg.color);
                }
                _ => {}
            }
            if !self.arena.contains(id) {
                return;
            }
        }
    }

    fn place_object(
        &mut self,
        lib: &Library,
        parent: InstanceId,
        movie: MovieId,
        po: &PlaceObject,
    ) {
        let Some(p) = placement(po) else {
            self.counters.invalid_records = self.counters.invalid_records.saturating_add(1);
            return;
        };
        match p.action {
            PlaceAction::Place(id) => {
                self.instantiate_child(lib, parent, movie, id, &p);
            }
            PlaceAction::Replace(id) => {
                let Some(child) = self.child_at(parent, p.depth) else {
                    self.counters.replace_on_empty =
                        self.counters.replace_on_empty.saturating_add(1);
                    return;
                };
                let swapped = self.replace_with(lib, child, movie, id);
                let frame = self.current_frame(parent);
                if let Some(inst) = self.arena.get_mut(child) {
                    apply_place_object(inst, &p);
                    inst.place_frame = frame;
                }
                self.event(parent, p.depth, EventKind::Replaced { swapped });
            }
            PlaceAction::Modify => {
                let Some(child) = self.child_at(parent, p.depth) else {
                    self.counters.modify_on_empty = self.counters.modify_on_empty.saturating_add(1);
                    return;
                };
                if let Some(inst) = self.arena.get_mut(child) {
                    apply_place_object(inst, &p);
                }
                self.event(parent, p.depth, EventKind::Modified);
            }
        }
    }

    fn replace_with(&mut self, lib: &Library, child: InstanceId, movie: MovieId, id: u32) -> bool {
        let Some(new) = lib.resolve(movie, id) else {
            return false;
        };
        let new_kind = lib.character(new);
        let Some(inst) = self.arena.get_mut(child) else {
            return false;
        };
        let compatible = matches!(
            (&inst.kind, new_kind),
            (InstanceKind::Shape, Some(Character::Shape { .. }))
                | (InstanceKind::Morph, Some(Character::Morph { .. }))
        );
        if compatible {
            inst.character = new;
            inst.placed_id = id;
            self.counters.replace_swapped = self.counters.replace_swapped.saturating_add(1);
        } else {
            self.counters.replace_kept = self.counters.replace_kept.saturating_add(1);
        }
        compatible
    }

    fn create(
        &mut self,
        lib: &Library,
        parent: InstanceId,
        movie: MovieId,
        id: u32,
        depth: i32,
    ) -> Option<InstanceId> {
        let tree_depth = self.arena.get(parent)?.tree_depth.saturating_add(1);
        if tree_depth > self.limits.max_tree_depth {
            self.fail(Error::Limit {
                what: "display tree depth",
                limit: usize::from(self.limits.max_tree_depth),
            });
            return None;
        }
        let Some(character) = lib.resolve(movie, id) else {
            self.counters.unresolved = self.counters.unresolved.saturating_add(1);
            return None;
        };
        let kind = match lib.character(character) {
            Some(Character::Sprite(_)) => InstanceKind::Clip(ClipState {
                timeline: lib.sprite_timeline(character)?,
                current_frame: 0,
                playing: true,
                initialized: false,
                clip_actions: None,
            }),
            Some(Character::Shape { .. }) => InstanceKind::Shape,
            Some(Character::Button(_)) => InstanceKind::Button(ButtonVisual::Up),
            Some(Character::EditText { text, .. }) => InstanceKind::EditText {
                text: text.clone().unwrap_or_default(),
            },
            Some(Character::Text { .. }) => InstanceKind::StaticText,
            Some(Character::Morph { .. }) => InstanceKind::Morph,
            _ => {
                self.counters.not_placeable = self.counters.not_placeable.saturating_add(1);
                return None;
            }
        };
        let instance = Instance::new(Some(parent), depth, tree_depth, id, character, kind);
        self.insert_child(lib, parent, depth, instance)
    }

    fn insert_child(
        &mut self,
        lib: &Library,
        parent: InstanceId,
        depth: i32,
        instance: Instance,
    ) -> Option<InstanceId> {
        if let Some(old) = self.child_at(parent, depth) {
            self.remove_child(lib, old);
        }
        let child = match self.arena.insert(instance) {
            Ok(child) => child,
            Err(full) => {
                self.fail(Error::Limit {
                    what: "live instances",
                    limit: full.cap,
                });
                return None;
            }
        };
        if let Some(p) = self.arena.get_mut(parent) {
            p.children.insert(depth, child);
        }
        self.counters.instantiated = self.counters.instantiated.saturating_add(1);
        Some(child)
    }

    fn instantiate_child(
        &mut self,
        lib: &Library,
        parent: InstanceId,
        movie: MovieId,
        id: u32,
        p: &Placement<'_>,
    ) -> Option<InstanceId> {
        if self.child_at(parent, p.depth).is_some() {
            self.counters.place_refused_occupied =
                self.counters.place_refused_occupied.saturating_add(1);
            self.event(parent, p.depth, EventKind::PlaceRefusedOccupied);
            return None;
        }
        let child = self.create(lib, parent, movie, id, p.depth)?;
        let frame = self.current_frame(parent);
        if let Some(inst) = self.arena.get_mut(child) {
            inst.flags |= flags::INSTANTIATED_BY_TIMELINE;
            inst.place_frame = frame;
            apply_place_object(inst, p);
            if let Some(name) = p.name {
                inst.name = Some(name.to_owned());
            }
            if let Some(clip_depth) = p.clip_depth {
                inst.clip_depth = clip_depth;
            }
            if let (Some(offset), Some(clip)) = (p.clip_actions, inst.clip_mut()) {
                clip.clip_actions = Some((movie, offset));
            }
        }
        self.post_instantiation(lib, child);
        self.run_frame_avm1(lib, child);
        self.event(parent, p.depth, EventKind::Placed);
        Some(child)
    }

    pub(crate) fn post_instantiation(&mut self, lib: &Library, id: InstanceId) {
        if self.scripts_enabled {
            self.construct_script_object(id);
        }
        let Some(inst) = self.arena.get_mut(id) else {
            return;
        };
        let names_by_default = matches!(
            inst.kind,
            InstanceKind::Clip(_)
                | InstanceKind::Button(_)
                | InstanceKind::Shape
                | InstanceKind::Morph
        );
        if names_by_default && inst.name.is_none() && inst.parent.is_some() {
            inst.default_name = Some(self.instance_counter);
            self.instance_counter = self.instance_counter.wrapping_add(1);
        }
        match inst.kind.clone() {
            InstanceKind::Clip(clip) => {
                self.exec.push_front(id);
                if let Some((movie, offset)) = clip.clip_actions {
                    let mut construct = Vec::new();
                    for handler in lib.clip_actions(movie, offset) {
                        if handler.events & clip_event::INITIALIZE != 0 {
                            self.push_action(QueuedAction {
                                clip: id,
                                movie,
                                source: ActionSource::Initialize {
                                    block: handler.block_offset,
                                },
                                is_unload: false,
                            });
                        }
                        if handler.events & clip_event::CONSTRUCT != 0 {
                            construct.push(handler.block_offset);
                        }
                    }
                    if !construct.is_empty() {
                        self.push_action(QueuedAction {
                            clip: id,
                            movie,
                            source: ActionSource::Construct { blocks: construct },
                            is_unload: false,
                        });
                    }
                }
                if self.init_run.insert(clip.timeline) {
                    let runs_as = if self.scripts_enabled {
                        self.root.unwrap_or(id)
                    } else {
                        id
                    };
                    if let Some(timeline) = lib.timeline(clip.timeline) {
                        for block in &timeline.init_actions {
                            self.push_action(QueuedAction {
                                clip: runs_as,
                                movie: timeline.movie,
                                source: ActionSource::InitAction { block: *block },
                                is_unload: false,
                            });
                        }
                    }
                }
            }
            InstanceKind::Button(_) => self.set_button_state(lib, id, ButtonVisual::Up),
            _ => {}
        }
    }

    fn construct_script_object(&mut self, id: InstanceId) {
        let p = self.heap.protos;
        let proto = match self.arena.get(id).map(|i| &i.kind) {
            Some(InstanceKind::Clip(_)) => p.movie_clip,
            Some(InstanceKind::Button(_)) => p.button,
            Some(InstanceKind::EditText { .. }) => p.text_field,
            _ => return,
        };
        match self
            .heap
            .alloc(crate::avm::Object::new(Some(proto), Native::Display(id)))
        {
            Ok(object) => {
                if let Some(inst) = self.arena.get_mut(id) {
                    inst.script = Some(object);
                }
            }
            Err(full) => self.fail(Error::Limit {
                what: full.what,
                limit: full.cap,
            }),
        }
    }

    fn remove_depth(&mut self, lib: &Library, parent: InstanceId, depth: i32) {
        match self.child_at(parent, depth) {
            Some(child) => {
                self.remove_child(lib, child);
                self.event(parent, depth, EventKind::Removed);
            }
            None => {
                self.counters.remove_on_empty = self.counters.remove_on_empty.saturating_add(1);
            }
        }
    }

    pub(crate) fn remove_child(&mut self, lib: &Library, id: InstanceId) {
        let Some(inst) = self.arena.get(id) else {
            return;
        };
        if inst.removed {
            return;
        }
        if self.scripts_enabled && self.should_delay_removal(lib, id, 0) {
            self.queue_removal(lib, id);
            return;
        }
        self.remove_child_directly(lib, id);
    }

    fn should_delay_removal(&self, lib: &Library, id: InstanceId, depth: u16) -> bool {
        let Some(inst) = self.arena.get(id) else {
            return false;
        };
        if let Some(clip) = inst.clip() {
            let handler = clip.clip_actions.is_some_and(|(movie, offset)| {
                lib.clip_actions(movie, offset)
                    .iter()
                    .any(|h| h.events & clip_event::UNLOAD != 0)
            });
            let dynamic = inst
                .script
                .is_some_and(|o| self.has_property(o, "onUnload"));
            if handler || dynamic {
                return true;
            }
        }
        depth < self.limits.max_tree_depth
            && inst
                .children
                .values()
                .any(|c| self.should_delay_removal(lib, *c, depth.saturating_add(1)))
    }

    fn queue_removal(&mut self, lib: &Library, id: InstanceId) {
        let Some((parent, depth)) = self.arena.get(id).and_then(|i| Some((i.parent?, i.depth)))
        else {
            return;
        };
        let new_depth = depth.saturating_neg().saturating_sub(1);
        if let Some(occupant) = self.child_at(parent, new_depth) {
            self.remove_child_directly(lib, occupant);
        }
        if let Some(p) = self.arena.get_mut(parent) {
            if p.children.get(&depth) == Some(&id) {
                p.children.remove(&depth);
            }
            p.children.insert(new_depth, id);
        }
        if let Some(inst) = self.arena.get_mut(id) {
            inst.depth = new_depth;
        }
        for h in self.subtree_post_order(id) {
            if let Some(inst) = self.arena.get_mut(h) {
                inst.removed = true;
            }
            self.queue_clip_event(lib, h, clip_event::UNLOAD);
        }
        self.pending_removal.push(id);
    }

    pub(crate) fn remove_pending(&mut self, lib: &Library) {
        for id in std::mem::take(&mut self.pending_removal) {
            if self.arena.contains(id) {
                self.remove_child_directly(lib, id);
            }
        }
    }

    fn subtree_post_order(&self, id: InstanceId) -> Vec<InstanceId> {
        let mut stack = vec![(id, false)];
        let mut order = Vec::new();
        while let Some((h, expanded)) = stack.pop() {
            if expanded {
                order.push(h);
                continue;
            }
            stack.push((h, true));
            if let Some(inst) = self.arena.get(h) {
                for child in inst.children.values().rev() {
                    stack.push((*child, false));
                }
            }
        }
        order
    }

    pub(crate) fn remove_child_directly(&mut self, lib: &Library, id: InstanceId) {
        let Some(inst) = self.arena.get(id) else {
            return;
        };
        if let Some(parent) = inst.parent {
            let depth = inst.depth;
            if let Some(p) = self.arena.get_mut(parent) {
                if p.children.get(&depth) == Some(&id) {
                    p.children.remove(&depth);
                }
            }
        }
        for h in self.subtree_post_order(id) {
            if self.arena.get(h).is_some_and(|i| !i.removed) {
                self.queue_clip_event(lib, h, clip_event::UNLOAD);
            }
            if self.arena.remove(h).is_some() {
                self.counters.reclaimed = self.counters.reclaimed.saturating_add(1);
            }
        }
    }

    pub(crate) fn set_button_state(&mut self, lib: &Library, id: InstanceId, visual: ButtonVisual) {
        let Some(inst) = self.arena.get(id) else {
            return;
        };
        let character = inst.character;
        let Some(Character::Button(button)) = lib.character(character) else {
            return;
        };
        let mut removed: BTreeSet<i32> = inst.children.keys().copied().collect();
        let mut created = Vec::new();
        for record in records_for_state(button, visual.to_apt()) {
            removed.remove(&record.depth);
            let existing = self.child_at(id, record.depth).filter(|c| {
                self.arena
                    .get(*c)
                    .is_some_and(|i| i.placed_id == record.character)
            });
            let child = match existing {
                Some(child) => child,
                None => {
                    let Some(child) =
                        self.create(lib, id, character.movie, record.character, record.depth)
                    else {
                        continue;
                    };
                    created.push(child);
                    child
                }
            };
            if let Some(c) = self.arena.get_mut(child) {
                c.set_matrix(Matrix {
                    a: record.transform.m00,
                    b: record.transform.m01,
                    c: record.transform.m10,
                    d: record.transform.m11,
                    tx: record.translate.x,
                    ty: record.translate.y,
                });
                c.color = ColorTransform::IDENTITY;
            }
        }
        for depth in removed {
            if let Some(child) = self.child_at(id, depth) {
                self.remove_child(lib, child);
            }
        }
        for child in created {
            self.post_instantiation(lib, child);
            self.run_frame_avm1(lib, child);
        }
        if let Some(InstanceKind::Button(v)) = self.arena.get_mut(id).map(|i| &mut i.kind) {
            *v = visual;
        }
    }

    pub(crate) fn run_goto(
        &mut self,
        lib: &Library,
        id: InstanceId,
        frame: u16,
        is_implicit: bool,
    ) {
        let Some(clip) = self.arena.get(id).and_then(Instance::clip).copied() else {
            return;
        };
        let Some(timeline) = lib.timeline(clip.timeline) else {
            return;
        };
        let movie = timeline.movie;
        let is_rewind = frame <= clip.current_frame;
        self.counters.gotos = self.counters.gotos.saturating_add(1);
        if is_rewind {
            self.counters.rewinds = self.counters.rewinds.saturating_add(1);
        }
        if is_implicit {
            self.counters.loops = self.counters.loops.saturating_add(1);
        }
        let mut current = if is_rewind { 0 } else { clip.current_frame };
        let clamped = frame.min(timeline.frame_count());
        let mut cmds: Vec<GotoCmd<'_>> = Vec::new();
        let mut index = 0usize;
        while current < clamped {
            current = current.saturating_add(1);
            let Some(f) = timeline.frame(current) else {
                break;
            };
            for item in &f.items {
                match item {
                    FrameItem::PlaceObject(po) => {
                        let Some(p) = placement(po) else {
                            continue;
                        };
                        index = index.saturating_add(1);
                        let cmd = GotoCmd::new(current, p, is_rewind, index);
                        match cmds.iter_mut().find(|c| c.p.depth == cmd.p.depth) {
                            Some(existing) => existing.merge(cmd),
                            None => cmds.push(cmd),
                        }
                    }
                    FrameItem::RemoveObject(ro) => {
                        if let Some(i) = cmds.iter().position(|c| c.p.depth == ro.depth) {
                            cmds.swap_remove(i);
                        }
                        if !is_rewind {
                            if let Some(child) = self.child_at(id, ro.depth) {
                                self.remove_child(lib, child);
                                self.event(id, ro.depth, EventKind::Removed);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        self.set_current_frame(id, current);
        let hit_target = current == frame;

        if is_rewind {
            let children: Vec<(i32, InstanceId)> = self
                .arena
                .get(id)
                .map(|i| i.children().collect())
                .unwrap_or_default();
            for (depth, child) in children {
                let fin = cmds.iter().find(|c| c.p.depth == depth);
                if self.survives_rewind(child, fin) {
                    self.counters.rewind_kept = self.counters.rewind_kept.saturating_add(1);
                    self.event(id, depth, EventKind::RewindKept);
                } else {
                    self.remove_child(lib, child);
                    self.counters.rewind_removed = self.counters.rewind_removed.saturating_add(1);
                    self.event(id, depth, EventKind::RewindRemoved);
                }
            }
        }

        cmds.sort_by_key(|c| c.index);
        for cmd in cmds.iter().filter(|c| c.frame < frame) {
            self.run_goto_command(lib, id, movie, cmd, is_rewind);
            if !self.arena.contains(id) {
                return;
            }
        }
        if hit_target {
            self.set_current_frame(id, current.saturating_sub(1));
            self.run_frame_internal(lib, id, false);
        } else {
            self.set_current_frame(id, clamped);
        }
        for cmd in cmds.iter().filter(|c| c.frame >= frame) {
            self.run_goto_command(lib, id, movie, cmd, is_rewind);
            if !self.arena.contains(id) {
                return;
            }
        }
    }

    fn run_goto_command(
        &mut self,
        lib: &Library,
        clip: InstanceId,
        movie: MovieId,
        cmd: &GotoCmd<'_>,
        is_rewind: bool,
    ) {
        let child = self.child_at(clip, cmd.p.depth);
        match (cmd.p.action, child, is_rewind) {
            (_, Some(prev), true) | (PlaceAction::Modify, Some(prev), _) => {
                if let Some(inst) = self.arena.get_mut(prev) {
                    apply_place_object(inst, &cmd.p);
                }
            }
            (PlaceAction::Replace(id), Some(prev), _) => {
                let swapped = self.replace_with(lib, prev, movie, id);
                if let Some(inst) = self.arena.get_mut(prev) {
                    apply_place_object(inst, &cmd.p);
                    inst.place_frame = cmd.frame;
                }
                self.event(clip, cmd.p.depth, EventKind::Replaced { swapped });
            }
            (PlaceAction::Place(id) | PlaceAction::Replace(id), _, _) => {
                if let Some(child) = self.instantiate_child(lib, clip, movie, id, &cmd.p) {
                    if let Some(inst) = self.arena.get_mut(child) {
                        inst.place_frame = cmd.frame;
                    }
                }
            }
            (PlaceAction::Modify, None, _) => {
                self.counters.modify_on_empty = self.counters.modify_on_empty.saturating_add(1);
            }
        }
    }

    fn survives_rewind(&self, child: InstanceId, fin: Option<&GotoCmd<'_>>) -> bool {
        let Some(inst) = self.arena.get(child) else {
            return false;
        };
        if inst.depth >= AVM_DEPTH_BIAS && !matches!(inst.kind, InstanceKind::Morph) {
            return true;
        }
        let Some(fin) = fin else {
            return false;
        };
        if inst.flags & flags::PLACED_BY_SCRIPT != 0 && inst.depth < AVM_DEPTH_BIAS {
            return false;
        }
        let p = &fin.p;
        let id_equals = match p.action {
            PlaceAction::Place(id) | PlaceAction::Replace(id) => inst.placed_id == id,
            PlaceAction::Modify => false,
        };
        let ratio_equals = p.ratio.is_none_or(|r| inst.ratio == r);
        let clip_depth_equals = p.clip_depth.is_none_or(|d| inst.clip_depth == d);
        let color_equals = p.color.is_none_or(|c| inst.color == c);
        let matrix_equals = p.matrix.is_none_or(|m| inst.matrix == m);
        match inst.kind {
            InstanceKind::Morph | InstanceKind::Shape | InstanceKind::StaticText => {
                ratio_equals && id_equals && clip_depth_equals && matrix_equals && color_equals
            }
            InstanceKind::Button(_) | InstanceKind::EditText { .. } => {
                ratio_equals && id_equals && clip_depth_equals
            }
            InstanceKind::Clip(_) => ratio_equals,
        }
    }

    pub(crate) fn set_playing(&mut self, clip: InstanceId, playing: bool) {
        if let Some(state) = self.arena.get_mut(clip).and_then(Instance::clip_mut) {
            state.playing = playing;
        }
    }

    pub(crate) fn goto_frame(&mut self, lib: &Library, clip: InstanceId, frame: u16, stop: bool) {
        let Some(state) = self.arena.get_mut(clip).and_then(Instance::clip_mut) else {
            return;
        };
        state.playing = !stop;
        let current = state.current_frame;
        let frame = frame.max(1);
        if frame != current {
            self.run_goto(lib, clip, frame, false);
        }
    }

    pub(crate) fn goto_label(
        &mut self,
        lib: &Library,
        clip: InstanceId,
        label: &str,
        stop: bool,
    ) -> bool {
        let frame = self
            .arena
            .get(clip)
            .and_then(Instance::clip)
            .and_then(|c| lib.timeline(c.timeline))
            .and_then(|t| t.label(label));
        match frame {
            Some(frame) => {
                self.goto_frame(lib, clip, frame, stop);
                true
            }
            None => false,
        }
    }

    pub(crate) fn step_frame(&mut self, lib: &Library, clip: InstanceId, forward: bool) {
        let Some(state) = self.arena.get(clip).and_then(Instance::clip) else {
            return;
        };
        let current = state.current_frame;
        let count = lib.timeline(state.timeline).map_or(0, |t| t.frame_count());
        if forward && current < count {
            self.goto_frame(lib, clip, current.saturating_add(1), true);
        } else if !forward && current > 1 {
            self.goto_frame(lib, clip, current.saturating_sub(1), true);
        }
    }

    pub(crate) fn create_by_script(
        &mut self,
        lib: &Library,
        parent: InstanceId,
        movie: MovieId,
        id: u32,
        depth: i32,
        name: &str,
        setup: impl FnOnce(&mut Instance),
    ) -> Option<InstanceId> {
        let child = self.create(lib, parent, movie, id, depth)?;
        let frame = self.current_frame(parent);
        if let Some(inst) = self.arena.get_mut(child) {
            inst.flags |= flags::PLACED_BY_SCRIPT;
            inst.place_frame = frame;
            inst.name = Some(name.to_owned());
            setup(inst);
        }
        self.post_instantiation(lib, child);
        self.run_frame_avm1(lib, child);
        Some(child)
    }

    pub(crate) fn create_empty_clip(
        &mut self,
        lib: &Library,
        parent: InstanceId,
        depth: i32,
        name: &str,
    ) -> Option<InstanceId> {
        let p = self.arena.get(parent)?;
        let tree_depth = p.tree_depth.saturating_add(1);
        let movie = p.character.movie;
        if tree_depth > self.limits.max_tree_depth {
            self.fail(Error::Limit {
                what: "display tree depth",
                limit: usize::from(self.limits.max_tree_depth),
            });
            return None;
        }
        let mut instance = Instance::new(
            Some(parent),
            depth,
            tree_depth,
            0,
            crate::library::CharacterRef {
                movie,
                id: u32::MAX,
            },
            InstanceKind::Clip(ClipState {
                timeline: NO_TIMELINE,
                current_frame: 0,
                playing: true,
                initialized: false,
                clip_actions: None,
            }),
        );
        instance.flags |= flags::PLACED_BY_SCRIPT;
        instance.place_frame = self.current_frame(parent);
        instance.name = Some(name.to_owned());
        let child = self.insert_child(lib, parent, depth, instance)?;
        self.post_instantiation(lib, child);
        self.run_frame_avm1(lib, child);
        Some(child)
    }
}

pub(crate) const NO_TIMELINE: TimelineId = TimelineId(u32::MAX);

fn apply_place_object(inst: &mut Instance, p: &Placement<'_>) {
    if inst.transformed_by_script() {
        return;
    }
    if let Some(matrix) = p.matrix {
        inst.set_matrix(matrix);
    }
    if let Some(color) = p.color {
        inst.color = color;
    }
    if let Some(ratio) = p.ratio {
        inst.ratio = ratio;
    }
}
