use std::sync::Arc;

use crate::actions::{ActionSource, QueuedAction};
use crate::avm::{Halt, ObjectId, Outward, ScriptStats, Value, VmLimits};
use crate::instance::{ButtonVisual, ClipState, Instance, InstanceId, InstanceKind};
use crate::library::{CharacterRef, Library, MovieId};
use crate::timeline::Stage;
use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_instances: usize,
    pub max_tree_depth: u16,
    pub max_queued_actions: usize,
    pub vm: VmLimits,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_instances: 65_536,
            max_tree_depth: 64,
            max_queued_actions: 65_536,
            vm: VmLimits::default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct Counters {
    pub ticks: u64,
    pub frames_run: u64,
    pub instantiated: u64,
    pub reclaimed: u64,
    pub place_refused_occupied: u64,
    pub replace_swapped: u64,
    pub replace_kept: u64,
    pub replace_on_empty: u64,
    pub modify_on_empty: u64,
    pub remove_on_empty: u64,
    pub invalid_records: u64,
    pub unresolved: u64,
    pub not_placeable: u64,
    pub gotos: u64,
    pub rewinds: u64,
    pub loops: u64,
    pub rewind_kept: u64,
    pub rewind_removed: u64,
    pub actions_queued: u64,
    pub actions_dropped_removed: u64,
    pub limit_refusals: u64,
    pub movies_loaded: u64,
    pub loads_refused: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimelineEvent {
    pub parent: InstanceId,
    pub depth: i32,
    pub kind: EventKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Placed,
    PlaceRefusedOccupied,
    Replaced { swapped: bool },
    Modified,
    Removed,
    RewindKept,
    RewindRemoved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HeapStats {
    pub objects: usize,
    pub scopes: usize,
    pub object_slots: usize,
    pub collections: u64,
    pub reclaimed: u64,
}

pub struct Player {
    pub(crate) library: Arc<Library>,
    pub(crate) stage: Stage,
    pub(crate) root: InstanceId,
}

impl Player {
    pub fn new(library: Arc<Library>, root: MovieId, limits: Limits) -> Result<Self, Error> {
        Self::start(library, root, limits, true)
    }

    pub fn new_without_scripts(
        library: Arc<Library>,
        root: MovieId,
        limits: Limits,
    ) -> Result<Self, Error> {
        Self::start(library, root, limits, false)
    }

    fn start(
        library: Arc<Library>,
        root: MovieId,
        limits: Limits,
        scripts: bool,
    ) -> Result<Self, Error> {
        if !library.is_linked() {
            return Err(Error::Unlinked);
        }
        let movie = library
            .movie(root)
            .ok_or_else(|| Error::UnknownMovie(format!("#{}", root.0)))?;
        let timeline = movie.root;
        let mut stage = Stage::new(limits, scripts)?;
        let instance = Instance::new(
            None,
            0,
            0,
            0,
            CharacterRef { movie: root, id: 0 },
            InstanceKind::Clip(ClipState {
                timeline,
                current_frame: 0,
                playing: true,
                initialized: false,
                clip_actions: None,
            }),
        );
        let id = stage.arena.insert(instance).map_err(|full| Error::Limit {
            what: "live instances",
            limit: full.cap,
        })?;
        stage.root = Some(id);
        stage.post_instantiation(&library, id);
        if scripts {
            stage.queue_init_actions(&library, root, id);
            stage.drain_actions(&library);
        }
        let mut player = Self {
            library,
            stage,
            root: id,
        };
        player.result()?;
        Ok(player)
    }

    fn result(&mut self) -> Result<(), Error> {
        self.stage.first_error.take().map_or(Ok(()), Err)
    }

    #[must_use]
    pub fn library(&self) -> &Library {
        &self.library
    }

    #[must_use]
    pub fn root(&self) -> InstanceId {
        self.root
    }

    #[must_use]
    pub fn scripts_enabled(&self) -> bool {
        self.stage.scripts_enabled
    }

    #[must_use]
    pub fn instance(&self, id: InstanceId) -> Option<&Instance> {
        self.stage.arena.get(id)
    }

    pub fn instance_mut(&mut self, id: InstanceId) -> Option<&mut Instance> {
        self.stage.arena.get_mut(id)
    }

    #[must_use]
    pub fn live_instances(&self) -> usize {
        self.stage.arena.len()
    }

    #[must_use]
    pub fn instance_slots(&self) -> usize {
        self.stage.arena.slots()
    }

    #[must_use]
    pub fn child_by_name(&self, parent: InstanceId, name: &str) -> Option<InstanceId> {
        self.stage.child_by_name(parent, name)
    }

    #[must_use]
    pub fn path(&self, id: InstanceId) -> Option<Vec<i32>> {
        let mut path = Vec::new();
        let mut at = self.instance(id)?;
        while let Some(parent) = at.parent() {
            path.push(at.depth());
            at = self.instance(parent)?;
        }
        path.reverse();
        Some(path)
    }

    #[must_use]
    pub fn world_matrix(&self, id: InstanceId) -> Option<crate::geom::Matrix> {
        let mut chain = Vec::new();
        let mut at = id;
        loop {
            let inst = self.instance(at)?;
            chain.push(inst.matrix());
            match inst.parent() {
                Some(parent) => at = parent,
                None => break,
            }
        }
        let mut world = crate::geom::Matrix::IDENTITY;
        for matrix in chain.iter().rev() {
            world = world.concat(matrix);
        }
        Some(world)
    }

    #[must_use]
    pub fn world_color(&self, id: InstanceId) -> Option<crate::geom::ColorTransform> {
        let mut chain = Vec::new();
        let mut at = id;
        loop {
            let inst = self.instance(at)?;
            chain.push(inst.color());
            match inst.parent() {
                Some(parent) => at = parent,
                None => break,
            }
        }
        let mut world = crate::geom::ColorTransform::IDENTITY;
        for color in chain.iter().rev() {
            world = world.concat(color);
        }
        Some(world)
    }

    #[must_use]
    pub fn target_path(&self, id: InstanceId) -> String {
        self.stage.target_path_dot(id)
    }

    #[must_use]
    pub fn background(&self) -> Option<u32> {
        self.stage.background
    }

    pub fn tick(&mut self) -> Result<(), Error> {
        self.stage.run_loads(&self.library);
        self.advance();
        if self.stage.scripts_enabled {
            self.stage.drain_actions(&self.library);
            self.run_timers();
            self.idle_pointer();
            self.collect_if_due();
        }
        self.result()
    }

    pub fn idle_pointer(&mut self) {
        if self.stage.scripts_enabled && self.stage.mouse.position.is_some() {
            self.stage.update_mouse(&self.library, false, None);
            self.stage.drain_actions(&self.library);
        }
    }

    pub fn pointer_move(&mut self, x: f32, y: f32) -> Result<(), Error> {
        self.stage.mouse.position = Some([x, y]);
        if self.stage.scripts_enabled {
            self.stage.update_mouse(&self.library, true, None);
            self.stage.drain_actions(&self.library);
        }
        self.result()
    }

    pub fn pointer_button(&mut self, down: bool) -> Result<(), Error> {
        if self.stage.mouse.down == down {
            return Ok(());
        }
        self.stage.mouse.down = down;
        if self.stage.scripts_enabled {
            self.stage.update_mouse(&self.library, false, Some(down));
            self.stage.drain_actions(&self.library);
        }
        self.result()
    }

    #[must_use]
    pub fn hovered(&self) -> Option<InstanceId> {
        self.stage.mouse.hovered
    }

    pub fn set_hit_mode(&mut self, mode: crate::mouse::HitMode) {
        self.stage.mouse.hit_mode = mode;
    }

    pub fn hit_areas(&self, out: &mut Vec<crate::mouse::HitArea>) {
        self.stage.hit_areas(&self.library, out);
    }

    pub fn run_timers(&mut self) {
        if self.stage.scripts_enabled && !self.stage.timers.is_empty() {
            self.stage.run_timers(&self.library);
            self.stage.drain_actions(&self.library);
        }
    }

    pub fn advance(&mut self) {
        let stage = &mut self.stage;
        stage.remove_pending(&self.library);
        let arena = &stage.arena;
        stage.exec.retain(|h| arena.contains(*h));
        let mut order = std::mem::take(&mut stage.scratch);
        order.clear();
        order.extend(stage.exec.iter().copied());
        for id in &order {
            if stage.arena.get(*id).is_some_and(|i| !i.removed) {
                stage.run_frame_avm1(&self.library, *id);
            }
        }
        stage.scratch = order;
        stage.counters.ticks = stage.counters.ticks.saturating_add(1);
    }

    pub fn collect_if_due(&mut self) -> usize {
        if self.stage.heap.wants_collection() {
            self.stage.collect_garbage()
        } else {
            0
        }
    }

    pub fn run_actions(&mut self) -> Result<usize, Error> {
        if !self.stage.scripts_enabled {
            return Ok(0);
        }
        let ran = self.stage.drain_actions(&self.library);
        self.result().map(|()| ran)
    }

    pub fn goto_frame(&mut self, clip: InstanceId, frame: u16, stop: bool) -> Result<(), Error> {
        self.stage.goto_frame(&self.library, clip, frame, stop);
        self.result()
    }

    pub fn goto_label(&mut self, clip: InstanceId, label: &str, stop: bool) -> Result<bool, Error> {
        let found = self.stage.goto_label(&self.library, clip, label, stop);
        self.result().map(|()| found)
    }

    pub fn play(&mut self, clip: InstanceId) {
        self.stage.set_playing(clip, true);
    }

    pub fn stop(&mut self, clip: InstanceId) {
        self.stage.set_playing(clip, false);
    }

    pub fn next_frame(&mut self, clip: InstanceId) -> Result<(), Error> {
        self.stage.step_frame(&self.library, clip, true);
        self.result()
    }

    pub fn prev_frame(&mut self, clip: InstanceId) -> Result<(), Error> {
        self.stage.step_frame(&self.library, clip, false);
        self.result()
    }

    pub fn set_button_state(
        &mut self,
        button: InstanceId,
        visual: ButtonVisual,
    ) -> Result<(), Error> {
        let current = match self.instance(button).map(Instance::kind) {
            Some(InstanceKind::Button(v)) => *v,
            _ => return Ok(()),
        };
        if current != visual {
            self.stage.set_button_state(&self.library, button, visual);
        }
        self.result()
    }

    pub fn queue_button_actions(&mut self, button: InstanceId, fired: u32) -> Result<usize, Error> {
        let Some(inst) = self.instance(button) else {
            return Ok(0);
        };
        let (Some(parent), character) = (inst.parent(), inst.character()) else {
            return Ok(0);
        };
        let mut queued = 0usize;
        for action in self.library.button_actions(character) {
            if action.condition & fired == 0 {
                continue;
            }
            self.stage.push_action(QueuedAction {
                clip: parent,
                movie: character.movie,
                source: ActionSource::Button {
                    condition: action.condition,
                    block: action.block_offset,
                },
                is_unload: false,
            });
            queued = queued.saturating_add(1);
        }
        self.result().map(|()| queued)
    }

    pub fn dispatch_clip_event(&mut self, clip: InstanceId, event: u32) -> Result<(), Error> {
        self.stage.queue_clip_event(&self.library, clip, event);
        self.result()
    }

    pub fn pop_action(&mut self) -> Option<QueuedAction> {
        loop {
            let action = self.stage.actions.pop()?;
            if action.is_unload || self.stage.arena.contains(action.clip) {
                return Some(action);
            }
            self.stage.counters.actions_dropped_removed = self
                .stage
                .counters
                .actions_dropped_removed
                .saturating_add(1);
        }
    }

    #[must_use]
    pub fn queued_actions(&self) -> usize {
        self.stage.actions.len()
    }

    pub fn clear_actions(&mut self) {
        self.stage.actions.clear();
    }

    pub fn run_block(&mut self, clip: InstanceId, movie: MovieId, offset: u32) -> Result<(), Halt> {
        if !self.stage.scripts_enabled {
            return Ok(());
        }
        self.stage.run_block(&self.library, movie, offset, clip)
    }

    pub fn set_sync_host(&mut self, host: Option<Box<dyn crate::avm::SyncHost>>) {
        self.stage.sync_host = host;
    }

    pub fn take_outward(&mut self) -> Vec<Outward> {
        std::mem::take(&mut self.stage.outward)
    }

    pub fn set_extern(&mut self, name: &str, value: bool) {
        self.set_extern_value(name, Value::Bool(value));
    }

    pub fn set_extern_value(&mut self, name: &str, value: Value) {
        if let Some(object) = self
            .stage
            .extern_object
            .and_then(|o| self.stage.heap.objects.get_mut(o))
        {
            object.props.define(name, value.clone(), false);
        }
        self.stage.externs.insert(Arc::from(name), value);
    }

    pub fn register_native_extern(&mut self, name: &str) {
        self.stage.native_externs.insert(Arc::from(name));
    }

    pub fn take_extern_writes(&mut self) -> Vec<(Arc<str>, Value)> {
        std::mem::take(&mut self.stage.native_extern_writes)
    }

    #[must_use]
    pub fn extern_member(&self, name: &str) -> Option<Value> {
        let object = self.stage.extern_object?;
        self.stage
            .heap
            .objects
            .get(object)?
            .props
            .get(name)
            .cloned()
    }

    #[must_use]
    pub fn loaded_movies(&self) -> Vec<(InstanceId, String)> {
        self.stage
            .loaded
            .iter()
            .filter(|(clip, _)| self.is_live(*clip))
            .filter_map(|(clip, movie)| {
                Some((*clip, self.library.movie(*movie)?.movie.name.clone()))
            })
            .collect()
    }

    pub fn member(&mut self, id: InstanceId, name: &str) -> Value {
        match self.stage.script_object(id) {
            Some(object) => self
                .stage
                .get_member(&self.library, &Value::Object(object), name),
            None => Value::Undefined,
        }
    }

    #[must_use]
    pub fn script_stats(&self) -> &ScriptStats {
        &self.stage.script_stats
    }

    pub fn take_script_stats(&mut self) -> ScriptStats {
        std::mem::take(&mut self.stage.script_stats)
    }

    pub fn set_record_calls(&mut self, on: bool) {
        self.stage.record_calls = on;
    }

    pub fn set_trace_log(&mut self, on: bool) {
        self.stage.traces = on.then(Vec::new);
    }

    pub fn take_traces(&mut self) -> Vec<String> {
        self.stage
            .traces
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }

    pub fn call(&mut self, clip: InstanceId, path: &str, args: Vec<Value>) -> Result<Value, Halt> {
        self.stage
            .call_named_from_host(&self.library, clip, path, args)
    }

    pub fn call_defined(
        &mut self,
        clip: InstanceId,
        path: &str,
        args: Vec<Value>,
    ) -> Result<Option<Value>, Halt> {
        self.stage
            .call_defined_from_host(&self.library, clip, path, args)
    }

    #[must_use]
    pub fn own_value(&self, id: InstanceId, name: &str) -> Option<Value> {
        let stage = &self.stage;
        stage
            .arena
            .get(id)
            .and_then(|i| i.script)
            .and_then(|o| stage.heap.objects.get(o))
            .and_then(|o| o.props.get(name))
            .cloned()
    }

    #[must_use]
    pub fn own_target_path(&self, id: InstanceId, name: &str) -> Option<String> {
        match self.own_value(id, name)? {
            Value::String(s) => Some(s.to_string()),
            Value::Object(object) => {
                let clip = self.stage.heap.objects.get(object)?.display()?;
                Some(self.stage.target_path_dot(clip))
            }
            _ => None,
        }
    }

    #[must_use]
    pub fn is_live(&self, id: InstanceId) -> bool {
        self.stage.arena.get(id).is_some_and(|i| !i.removed)
    }

    pub fn variable(&mut self, clip: InstanceId, path: &str) -> Value {
        self.stage.eval_variable(&self.library, clip, path)
    }

    pub fn set_variable(&mut self, clip: InstanceId, path: &str, value: Value) -> Result<(), Halt> {
        self.stage.assign_variable(&self.library, clip, path, value)
    }

    pub fn display_string(&mut self, value: &Value) -> String {
        self.stage.string_of(&self.library, value).to_string()
    }

    pub fn text(&mut self, id: InstanceId) -> Option<String> {
        self.stage.edit_text(&self.library, id)
    }

    pub fn pin(&mut self, object: ObjectId) {
        let n = self.stage.heap.pins.entry(object).or_insert(0);
        *n = n.saturating_add(1);
    }

    pub fn unpin(&mut self, object: ObjectId) {
        if let Some(n) = self.stage.heap.pins.get_mut(&object) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                self.stage.heap.pins.remove(&object);
            }
        }
    }

    pub fn collect_garbage(&mut self) -> usize {
        self.stage.collect_garbage()
    }

    #[must_use]
    pub fn heap_stats(&self) -> HeapStats {
        let heap = &self.stage.heap;
        HeapStats {
            objects: heap.objects.len(),
            scopes: heap.scopes.len(),
            object_slots: heap.objects.slots(),
            collections: heap.collections,
            reclaimed: heap.reclaimed,
        }
    }

    #[must_use]
    pub fn local_bounds(&self, id: InstanceId) -> Option<[f32; 4]> {
        self.stage.local_bounds(&self.library, id, 0)
    }

    pub fn set_pointer(&mut self, x: f32, y: f32) {
        self.stage.pointer = [x, y];
    }

    #[must_use]
    pub fn counters(&self) -> &Counters {
        &self.stage.counters
    }

    pub fn set_diagnostics(&mut self, on: bool) {
        self.stage.events = on.then(Vec::new);
    }

    pub fn take_events(&mut self) -> Vec<TimelineEvent> {
        self.stage
            .events
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }
}
