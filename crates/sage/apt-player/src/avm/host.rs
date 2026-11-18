use std::sync::Arc;

use mint_sage_apt::Character;

use crate::actions::{ActionSource, QueuedAction};
use crate::instance::{InstanceId, InstanceKind};
use crate::library::{Library, MovieId};
use crate::timeline::Stage;

use super::heap::ScopeClass;
use super::interp::Frame;
use super::value::Value;
use super::Halt;

pub(crate) const TICK_MS: f64 = 1000.0 / 30.0;

const MAX_CATCH_UP: u32 = 8;

#[derive(Debug, Clone)]
pub(crate) struct Timer {
    pub(crate) id: i32,
    pub(crate) callee: Value,
    pub(crate) method: Option<Arc<str>>,
    pub(crate) args: Vec<Value>,
    pub(crate) interval: f64,
    pub(crate) due: f64,
}

impl Stage {
    pub(crate) fn now_ms(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let ticks = self.counters.ticks as f64;
        ticks * TICK_MS
    }

    pub(crate) fn run_timers(&mut self, lib: &Library) {
        let now = self.now_ms();
        let Some(root) = self.root else {
            return;
        };
        let ids: Vec<i32> = self.timers.iter().map(|t| t.id).collect();
        for id in ids {
            let mut runs = 0u32;
            loop {
                let Some(timer) = self.timers.iter_mut().find(|t| t.id == id) else {
                    break;
                };
                if timer.due > now || runs >= MAX_CATCH_UP {
                    break;
                }
                timer.due += timer.interval;
                let timer = timer.clone();
                runs = runs.saturating_add(1);
                let Ok(frame) = self.host_frame(root) else {
                    return;
                };
                self.budget = self.vm_limits.max_instructions;
                let result = match &timer.method {
                    Some(name) => {
                        let f = self.get_member(lib, &timer.callee, name);
                        self.call_value(lib, &frame, &f, timer.callee.clone(), timer.args, name)
                    }
                    None => self.call_value(
                        lib,
                        &frame,
                        &timer.callee,
                        Value::Undefined,
                        timer.args,
                        "[interval]",
                    ),
                };
                if let Err(halt) = result {
                    super::bump(&mut self.script_stats.halts, halt);
                }
            }
        }
    }

    pub(crate) fn timer_roots(&self) -> Vec<super::ObjectId> {
        let mut out = Vec::new();
        for t in &self.timers {
            out.extend(t.callee.as_object());
            out.extend(t.args.iter().filter_map(Value::as_object));
        }
        out
    }

    pub(crate) fn queue_init_actions(&mut self, lib: &Library, movie: MovieId, root: InstanceId) {
        let Some(m) = lib.movie(movie) else {
            return;
        };
        let mut timelines = vec![m.root];
        for (index, character) in m.movie.characters.iter().enumerate() {
            if !matches!(character, Character::Sprite(_)) {
                continue;
            }
            let Ok(id) = u32::try_from(index) else {
                break;
            };
            if let Some(t) = lib.sprite_timeline(crate::library::CharacterRef { movie, id }) {
                timelines.push(t);
            }
        }
        for t in timelines {
            if !self.init_run.insert(t) {
                continue;
            }
            let Some(timeline) = lib.timeline(t) else {
                continue;
            };
            for block in &timeline.init_actions {
                self.push_action(QueuedAction {
                    clip: root,
                    movie: timeline.movie,
                    source: ActionSource::InitAction { block: *block },
                    is_unload: false,
                });
            }
        }
    }

    pub(crate) fn collect_garbage(&mut self) -> usize {
        let mut roots: Vec<_> = self.arena.iter().filter_map(|(_, i)| i.script).collect();
        roots.extend(self.timer_roots());
        self.heap.collect(roots)
    }

    fn host_frame(&mut self, clip: InstanceId) -> Result<Frame, Halt> {
        let clip_obj = self.script_object(clip);
        let values = clip_obj.unwrap_or(self.heap.global);
        let scope = self
            .heap
            .alloc_scope(super::Scope {
                parent: Some(self.heap.global_scope),
                class: ScopeClass::Target,
                values,
            })
            .map_err(|full| Halt::Limit { what: full.what })?;
        Ok(Frame {
            movie: self
                .arena
                .get(clip)
                .map_or(crate::library::MovieId(0), |i| i.character.movie),
            scope,
            pool: Arc::from(Vec::new()),
            this: clip_obj.map_or(Value::Undefined, Value::Object),
            base_clip: Some(clip),
            target: Some(clip),
            registers: None,
            stack: Vec::new(),
            depth: 0,
        })
    }

    pub(crate) fn run_method(
        &mut self,
        lib: &Library,
        clip: InstanceId,
        name: &str,
    ) -> Result<(), Halt> {
        let Some(object) = self.script_object(clip) else {
            return Ok(());
        };
        self.script_stats.blocks_run = self.script_stats.blocks_run.saturating_add(1);
        let frame = self.host_frame(clip)?;
        self.budget = self.vm_limits.max_instructions;
        let callee = self.get_member(lib, &Value::Object(object), name);
        let result = self.call_value(
            lib,
            &frame,
            &callee,
            Value::Object(object),
            Vec::new(),
            name,
        );
        match result {
            Ok(_) => {
                self.script_stats.blocks_completed =
                    self.script_stats.blocks_completed.saturating_add(1);
                Ok(())
            }
            Err(halt) => {
                super::bump(&mut self.script_stats.halts, halt.clone());
                Err(halt)
            }
        }
    }

    pub(crate) fn call_named_from_host(
        &mut self,
        lib: &Library,
        clip: InstanceId,
        path: &str,
        args: Vec<Value>,
    ) -> Result<Value, Halt> {
        if !self.scripts_enabled || !self.arena.contains(clip) {
            return Ok(Value::Undefined);
        }
        self.script_stats.blocks_run = self.script_stats.blocks_run.saturating_add(1);
        let frame = self.host_frame(clip)?;
        self.budget = self.vm_limits.max_instructions;
        let result = self.call_named(lib, &frame, path, args);
        match &result {
            Ok(_) => {
                self.script_stats.blocks_completed =
                    self.script_stats.blocks_completed.saturating_add(1);
            }
            Err(halt) => super::bump(&mut self.script_stats.halts, halt.clone()),
        }
        self.drain_actions(lib);
        result
    }

    pub(crate) fn call_defined_from_host(
        &mut self,
        lib: &Library,
        clip: InstanceId,
        path: &str,
        args: Vec<Value>,
    ) -> Result<Option<Value>, Halt> {
        if !self.scripts_enabled || !self.arena.contains(clip) {
            return Ok(None);
        }
        let frame = self.host_frame(clip)?;
        let (callee, _) = self.get_variable(lib, &frame, path);
        let callable = callee
            .as_object()
            .and_then(|id| self.heap.objects.get(id))
            .is_some_and(|o| {
                matches!(
                    o.native,
                    super::object::Native::Function(_) | super::object::Native::Builtin(_)
                )
            });
        if !callable {
            return Ok(None);
        }
        self.call_named_from_host(lib, clip, path, args).map(Some)
    }

    pub(crate) fn eval_variable(&mut self, lib: &Library, clip: InstanceId, path: &str) -> Value {
        if !self.scripts_enabled || !self.arena.contains(clip) {
            return Value::Undefined;
        }
        match self.host_frame(clip) {
            Ok(frame) => self.get_variable(lib, &frame, path).0,
            Err(_) => Value::Undefined,
        }
    }

    pub(crate) fn assign_variable(
        &mut self,
        lib: &Library,
        clip: InstanceId,
        path: &str,
        value: Value,
    ) -> Result<(), Halt> {
        if !self.scripts_enabled || !self.arena.contains(clip) {
            return Ok(());
        }
        let frame = self.host_frame(clip)?;
        self.set_variable(lib, &frame, path, value)
    }

    pub(crate) fn edit_text(&mut self, lib: &Library, id: InstanceId) -> Option<String> {
        let inst = self.arena.get(id)?;
        let InstanceKind::EditText { text } = &inst.kind else {
            return None;
        };
        let own = text.clone();
        let parent = inst.parent;
        let variable = match lib.character(inst.character) {
            Some(Character::EditText {
                variable: Some(v), ..
            }) if !v.is_empty() => v.clone(),
            _ => return Some(own),
        };
        let Some(parent) = parent else {
            return Some(own);
        };
        match self.read_bound(lib, parent, &variable) {
            Value::Undefined => Some(own),
            v => Some(self.string_of(lib, &v).to_string()),
        }
    }

    pub(crate) fn read_bound(&mut self, lib: &Library, clip: InstanceId, path: &str) -> Value {
        let Some(object) = self.script_object(clip) else {
            return Value::Undefined;
        };
        let global = self.heap.global;
        if let Some(sep) = path.rfind([':', '.']) {
            let (head, tail) = (
                path.get(..sep).unwrap_or(""),
                path.get(sep.saturating_add(1)..).unwrap_or(""),
            );
            if head.is_empty() {
                return Value::Undefined;
            }
            let frame = Frame {
                movie: self
                    .arena
                    .get(clip)
                    .map_or(crate::library::MovieId(0), |i| i.character.movie),
                scope: self.heap.global_scope,
                pool: Arc::clone(&self.empty_pool),
                this: Value::Object(object),
                base_clip: Some(clip),
                target: Some(clip),
                registers: None,
                stack: Vec::new(),
                depth: 0,
            };
            for start in [object, global] {
                if let Some(target) = self.resolve_target_path(lib, &frame, start, head, true, true)
                {
                    if self.has_property(target, tail) {
                        return self.get_member(lib, &Value::Object(target), tail);
                    }
                }
            }
            return Value::Undefined;
        }
        for scope in [object, global] {
            if self.has_property(scope, path) {
                return self.get_member(lib, &Value::Object(scope), path);
            }
        }
        Value::Undefined
    }
}
