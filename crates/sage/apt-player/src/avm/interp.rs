use std::sync::Arc;

use mint_sage_apt::{decode_block, Block, DecodeLimits, Function, Instruction, Op};

use crate::actions::{ActionSource, QueuedAction};
use crate::instance::{Instance, InstanceId, InstanceKind};
use crate::library::{Library, MovieId};
use crate::timeline::Stage;

use super::heap::{Scope, ScopeClass, ScopeId};
use super::object::{BytecodeFn, Native, Object};
use super::value::{format_number, loose_equals, ObjectId, Value};
use super::{Halt, Outward};

type R<T> = Result<T, Halt>;

#[derive(Debug, Clone)]
pub(crate) struct Frame {
    pub(crate) movie: MovieId,
    pub(crate) scope: ScopeId,
    pub(crate) pool: Arc<[Value]>,
    pub(crate) this: Value,
    pub(crate) base_clip: Option<InstanceId>,
    pub(crate) target: Option<InstanceId>,
    pub(crate) registers: Option<Vec<Value>>,
    pub(crate) stack: Vec<Value>,
    pub(crate) depth: u32,
}

pub(crate) enum Flow {
    Ended,
    Returned(Value),
}

enum Step {
    Next,
    End,
    Return(Value),
}

mod fflags {
    use mint_sage_apt::Function;

    pub(super) fn swf(function: &Function) -> u32 {
        function.swf_flags()
    }

    pub(super) fn register_count(function: &Function) -> usize {
        function.register_count() as usize
    }

    pub(super) const PRELOAD_THIS: u32 = 0x01;
    pub(super) const SUPPRESS_THIS: u32 = 0x02;
    pub(super) const PRELOAD_ARGUMENTS: u32 = 0x04;
    pub(super) const SUPPRESS_ARGUMENTS: u32 = 0x08;
    pub(super) const PRELOAD_SUPER: u32 = 0x10;
    pub(super) const PRELOAD_ROOT: u32 = 0x40;
    pub(super) const PRELOAD_PARENT: u32 = 0x80;
    pub(super) const PRELOAD_GLOBAL: u32 = 0x100;
}

impl Stage {
    pub(crate) fn block(
        &mut self,
        lib: &Library,
        movie: MovieId,
        offset: u32,
    ) -> Option<Arc<Block>> {
        if let Some(decoded) = lib.block(movie, offset) {
            return decoded;
        }
        if let Some(cached) = self.blocks.get(&(movie, offset)) {
            return cached.clone();
        }
        let decoded = lib
            .movie(movie)
            .and_then(|m| decode_block(&m.apt, offset, &DecodeLimits::default()).ok())
            .map(Arc::new);
        self.blocks.insert((movie, offset), decoded.clone());
        decoded
    }

    pub(crate) fn drain_actions(&mut self, lib: &Library) -> usize {
        let mut ran = 0usize;
        while let Some(action) = self.actions.pop() {
            let runnable =
                action.is_unload || self.arena.get(action.clip).is_some_and(|i| !i.removed);
            if !runnable {
                self.counters.actions_dropped_removed =
                    self.counters.actions_dropped_removed.saturating_add(1);
                continue;
            }
            if !self.scripts_enabled {
                self.script_stats.discarded = self.script_stats.discarded.saturating_add(1);
                continue;
            }
            if ran >= self.vm_limits.max_actions_per_drain {
                self.fail(crate::Error::Limit {
                    what: "actions per drain",
                    limit: self.vm_limits.max_actions_per_drain,
                });
                self.actions.clear();
                break;
            }
            ran = ran.saturating_add(1);
            self.run_queued(lib, &action);
        }
        ran
    }

    fn run_queued(&mut self, lib: &Library, action: &QueuedAction) {
        match &action.source {
            ActionSource::Frame { block }
            | ActionSource::ClipEvent { block, .. }
            | ActionSource::Initialize { block }
            | ActionSource::Button { block, .. }
            | ActionSource::InitAction { block } => {
                let _halt = self.run_block(lib, action.movie, *block, action.clip);
            }
            ActionSource::Method { name } => {
                let _halt = self.run_method(lib, action.clip, name);
            }
            ActionSource::Construct { blocks } => {
                for block in blocks {
                    let _halt = self.run_block(lib, action.movie, *block, action.clip);
                }
            }
        }
    }

    pub(crate) fn run_block(
        &mut self,
        lib: &Library,
        movie: MovieId,
        offset: u32,
        clip: InstanceId,
    ) -> Result<(), Halt> {
        self.script_stats.blocks_run = self.script_stats.blocks_run.saturating_add(1);
        let result = self.run_block_inner(lib, movie, offset, clip);
        match &result {
            Ok(()) => {
                self.script_stats.blocks_completed =
                    self.script_stats.blocks_completed.saturating_add(1);
            }
            Err(halt) => {
                super::bump(&mut self.script_stats.halts, halt.clone());
            }
        }
        result
    }

    fn run_block_inner(
        &mut self,
        lib: &Library,
        movie: MovieId,
        offset: u32,
        clip: InstanceId,
    ) -> R<()> {
        let block = self.block(lib, movie, offset).ok_or(Halt::Decode)?;
        let clip_obj = self.script_object(clip);
        let values = clip_obj.unwrap_or(self.heap.global);
        let scope = self.new_scope(Some(self.heap.global_scope), ScopeClass::Target, values)?;
        let mut frame = Frame {
            movie,
            scope,
            pool: Arc::from(Vec::new()),
            this: clip_obj.map_or(Value::Undefined, Value::Object),
            base_clip: Some(clip),
            target: Some(clip),
            registers: None,
            stack: Vec::new(),
            depth: 0,
        };
        self.budget = self.vm_limits.max_instructions;
        let start = self.vm_limits.max_instructions;
        let result = self.exec(lib, &block, &block.code, block.end, &mut frame);
        if matches!(result, Ok(Flow::Ended)) && !frame.stack.is_empty() {
            self.script_stats.residue_blocks = self.script_stats.residue_blocks.saturating_add(1);
        }
        self.script_stats.instructions = self
            .script_stats
            .instructions
            .saturating_add(start.saturating_sub(self.budget));
        result.map(|_| ())
    }

    fn new_scope(
        &mut self,
        parent: Option<ScopeId>,
        class: ScopeClass,
        values: ObjectId,
    ) -> R<ScopeId> {
        self.heap
            .alloc_scope(Scope {
                parent,
                class,
                values,
            })
            .map_err(|full| Halt::Limit { what: full.what })
    }

    pub(crate) fn script_object(&self, id: InstanceId) -> Option<ObjectId> {
        self.arena.get(id).and_then(|i| i.script)
    }

    pub(crate) fn alloc(&mut self, proto: Option<ObjectId>, native: Native) -> R<ObjectId> {
        self.heap
            .alloc(Object::new(proto, native))
            .map_err(|full| Halt::Limit { what: full.what })
    }

    pub(crate) fn new_array(&mut self, values: Vec<Value>) -> R<ObjectId> {
        let id = self.alloc(Some(self.heap.protos.array), Native::Array)?;
        if let Some(obj) = self.heap.objects.get_mut(id) {
            #[allow(clippy::cast_precision_loss)]
            let len = values.len() as f64;
            for (i, v) in values.into_iter().enumerate() {
                obj.props.set(&i.to_string(), v);
            }
            obj.props.define("length", Value::Number(len), true);
        }
        Ok(id)
    }

    pub(crate) fn new_object(&mut self) -> R<ObjectId> {
        self.alloc(Some(self.heap.protos.object), Native::Plain)
    }

    fn proto_of(&self, value: &Value) -> Option<ObjectId> {
        match value {
            Value::Object(id) => self.heap.objects.get(*id).and_then(|o| o.proto),
            Value::String(_) => Some(self.heap.protos.string),
            Value::Number(_) => Some(self.heap.protos.number),
            Value::Bool(_) => Some(self.heap.protos.boolean),
            Value::Undefined | Value::Null => None,
        }
    }

    pub(crate) fn get_member(&mut self, lib: &Library, object: &Value, name: &str) -> Value {
        if let Value::String(s) = object {
            if name.eq_ignore_ascii_case("length") {
                #[allow(clippy::cast_precision_loss)]
                return Value::Number(s.chars().count() as f64);
            }
        }
        if let Value::Object(id) = object {
            if Some(*id) == self.extern_object {
                if let Some(answer) = self.extern_query(name) {
                    return answer;
                }
                if let Some(live) = self.read_extern_live(name) {
                    return live;
                }
            }
        }
        let mut current = match object {
            Value::Object(id) => Some(*id),
            other => self.proto_of(other),
        };
        let mut hops = 0u32;
        while let Some(id) = current {
            hops = hops.saturating_add(1);
            if hops > 256 {
                break;
            }
            let Some(obj) = self.heap.objects.get(id) else {
                break;
            };
            if let Some(v) = obj.props.get(name) {
                return v.clone();
            }
            let display = obj.display();
            let next = obj.proto;
            if let Some(inst) = display {
                if let Some(v) = self.display_lookup(lib, inst, name) {
                    return v;
                }
            }
            current = next;
        }
        Value::Undefined
    }

    fn display_lookup(&mut self, lib: &Library, inst: InstanceId, name: &str) -> Option<Value> {
        self.arena.get(inst)?;
        let magic = name.starts_with('_');
        if magic {
            if let Some(v) = self.path_property(inst, name) {
                return Some(v);
            }
        }
        if let Some(child) = self.child_by_name(inst, name) {
            let obj = self.script_object(child).or_else(|| {
                self.arena
                    .get(child)
                    .and_then(|c| c.parent)
                    .and_then(|p| self.script_object(p))
            });
            return Some(obj.map_or(Value::Undefined, Value::Object));
        }
        if magic {
            if let Some(index) = super::display::property_index(name) {
                return Some(self.get_display_property(lib, inst, index));
            }
        }
        if name.eq_ignore_ascii_case("text") || name.eq_ignore_ascii_case("htmlText") {
            if let Some(InstanceKind::EditText { text }) = self.arena.get(inst).map(Instance::kind)
            {
                return Some(Value::str(text));
            }
        }
        None
    }

    fn set_edit_text(&mut self, lib: &Library, inst: InstanceId, value: &Value) -> R<()> {
        let text = self.string_of(lib, value).to_string();
        let Some(i) = self.arena.get_mut(inst) else {
            return Ok(());
        };
        if let InstanceKind::EditText { text: t } = &mut i.kind {
            t.clone_from(&text);
        }
        let (parent, character) = (i.parent, i.character);
        let variable = match lib.character(character) {
            Some(mint_sage_apt::Character::EditText {
                variable: Some(v), ..
            }) if !v.is_empty() => v.clone(),
            _ => return Ok(()),
        };
        if self.writing_bound_text {
            return Ok(());
        }
        let Some(parent) = parent else {
            return Ok(());
        };
        self.writing_bound_text = true;
        let result = self.assign_variable(lib, parent, &variable, Value::str(&text));
        self.writing_bound_text = false;
        result
    }

    pub(crate) fn has_property(&self, object: ObjectId, name: &str) -> bool {
        let mut current = Some(object);
        let mut hops = 0u32;
        while let Some(id) = current {
            hops = hops.saturating_add(1);
            if hops > 256 {
                return false;
            }
            let Some(obj) = self.heap.objects.get(id) else {
                return false;
            };
            if obj.props.contains(name) {
                return true;
            }
            if let Some(inst) = obj.display() {
                if self.arena.contains(inst)
                    && ((name.starts_with('_')
                        && (self.path_property(inst, name).is_some()
                            || super::display::property_index(name).is_some()))
                        || self.child_by_name(inst, name).is_some())
                {
                    return true;
                }
            }
            current = obj.proto;
        }
        false
    }

    pub(crate) fn set_member(
        &mut self,
        lib: &Library,
        object: &Value,
        name: &str,
        value: Value,
    ) -> R<()> {
        let Value::Object(id) = object else {
            return Ok(());
        };
        if name.is_empty() {
            return Ok(());
        }
        if Some(*id) == self.extern_object {
            self.write_extern_live(name, &value);
        }
        let Some(obj) = self.heap.objects.get(*id) else {
            return Ok(());
        };
        if let Some(inst) = obj.display() {
            if !obj.props.contains(name) {
                if let Some(index) = super::display::property_index(name) {
                    if self.arena.contains(inst) {
                        self.set_display_property(lib, inst, index, &value);
                    }
                    return Ok(());
                }
                let is_text = matches!(
                    self.arena.get(inst).map(Instance::kind),
                    Some(InstanceKind::EditText { .. })
                );
                if is_text
                    && (name.eq_ignore_ascii_case("text") || name.eq_ignore_ascii_case("htmlText"))
                {
                    return self.set_edit_text(lib, inst, &value);
                }
            }
        }
        let is_array = matches!(obj.native, Native::Array);
        let Some(obj) = self.heap.objects.get_mut(*id) else {
            return Ok(());
        };
        if is_array {
            if name.eq_ignore_ascii_case("length") {
                let new_len = value.to_number();
                let old_len = obj.props.get("length").map_or(0.0, Value::to_number);
                let mut i = new_len.max(0.0);
                while i < old_len {
                    obj.props.remove(&format_number(i));
                    i += 1.0;
                }
                obj.props
                    .define("length", Value::Number(new_len.max(0.0).trunc()), true);
                return Ok(());
            }
            if let Ok(index) = name.parse::<u32>() {
                let len = obj.props.get("length").map_or(0.0, Value::to_number);
                if f64::from(index) >= len {
                    obj.props
                        .define("length", Value::Number(f64::from(index) + 1.0), true);
                }
            }
        }
        obj.props.set(name, value);
        Ok(())
    }

    pub(crate) fn string_of(&mut self, lib: &Library, value: &Value) -> Arc<str> {
        if let Some(s) = value.primitive_string() {
            return s;
        }
        let Value::Object(id) = value else {
            return Arc::from("");
        };
        let Some(obj) = self.heap.objects.get(*id) else {
            return Arc::from("undefined");
        };
        match &obj.native {
            Native::Display(inst) => Arc::from(self.target_path_dot(*inst).as_str()),
            Native::Function(_) | Native::Builtin(_) => Arc::from("[type Function]"),
            Native::Array => {
                let joined = self.join_array(lib, *id, ",");
                Arc::from(joined.as_str())
            }
            Native::Plain => Arc::from("[object Object]"),
        }
    }

    pub(crate) fn join_array(&mut self, lib: &Library, id: ObjectId, sep: &str) -> String {
        let len = self.array_len(id);
        let mut out = String::new();
        for i in 0..len {
            if i > 0 {
                out.push_str(sep);
            }
            let v = self.get_member(lib, &Value::Object(id), &i.to_string());
            if !matches!(v, Value::Undefined | Value::Null) {
                if Some(id) == v.as_object() {
                    continue;
                }
                out.push_str(&self.string_of(lib, &v));
            }
            if out.len() > self.vm_limits.max_string_len {
                break;
            }
        }
        out
    }

    pub(crate) fn array_len(&self, id: ObjectId) -> usize {
        let n = self
            .heap
            .objects
            .get(id)
            .and_then(|o| o.props.get("length"))
            .map_or(0.0, Value::to_number);
        if n.is_finite() && n > 0.0 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let len = n.min(1_000_000.0) as usize;
            len
        } else {
            0
        }
    }

    fn type_of(&self, value: &Value) -> &'static str {
        match value {
            Value::Undefined => "undefined",
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Object(id) => match self.heap.objects.get(*id).map(|o| &o.native) {
                Some(Native::Function(_) | Native::Builtin(_)) => "function",
                Some(Native::Display(inst)) => match self.arena.get(*inst).map(Instance::kind) {
                    Some(InstanceKind::EditText { .. }) => "object",
                    _ => "movieclip",
                },
                _ => "object",
            },
        }
    }

    fn resolve(&mut self, lib: &Library, frame: &Frame, name: &str) -> (Value, Option<ObjectId>) {
        if name.eq_ignore_ascii_case("this") {
            return (frame.this.clone(), None);
        }
        let mut scope = Some(frame.scope);
        let mut hops = 0u32;
        while let Some(id) = scope {
            hops = hops.saturating_add(1);
            if hops > 256 {
                break;
            }
            let Some(s) = self.heap.scopes.get(id) else {
                break;
            };
            let (values, parent) = (s.values, s.parent);
            if self.has_property(values, name) {
                let v = self.get_member(lib, &Value::Object(values), name);
                return (v, Some(values));
            }
            scope = parent;
        }
        (Value::Undefined, None)
    }

    fn set_simple(&mut self, lib: &Library, frame: &Frame, name: &str, value: Value) -> R<()> {
        let mut scope = Some(frame.scope);
        let mut last = self.heap.global;
        let mut hops = 0u32;
        while let Some(id) = scope {
            hops = hops.saturating_add(1);
            if hops > 256 {
                break;
            }
            let Some(s) = self.heap.scopes.get(id) else {
                break;
            };
            let (values, parent, class) = (s.values, s.parent, s.class);
            let removed = self
                .heap
                .objects
                .get(values)
                .and_then(Object::display)
                .is_some_and(|inst| self.arena.get(inst).is_none_or(|i| i.removed));
            if !removed && (class == ScopeClass::Target || self.has_property(values, name)) {
                return self.set_member(lib, &Value::Object(values), name, value);
            }
            last = values;
            scope = parent;
        }
        self.set_member(lib, &Value::Object(last), name, value)
    }

    fn define_local(&mut self, lib: &Library, frame: &Frame, name: &str, value: Value) -> R<()> {
        if !self.in_local_scope(frame) && name.contains(['.', ':']) {
            return self.set_variable(lib, frame, name, value);
        }
        let values = self
            .heap
            .scopes
            .get(frame.scope)
            .map_or(self.heap.global, |s| s.values);
        self.set_member(lib, &Value::Object(values), name, value)
    }

    fn in_local_scope(&self, frame: &Frame) -> bool {
        self.heap
            .scopes
            .get(frame.scope)
            .is_some_and(|s| s.class == ScopeClass::Local)
    }

    fn scope_values(&self, frame: &Frame) -> Vec<ObjectId> {
        let mut out = Vec::new();
        let mut scope = Some(frame.scope);
        while let Some(id) = scope {
            if out.len() > 256 {
                break;
            }
            let Some(s) = self.heap.scopes.get(id) else {
                break;
            };
            out.push(s.values);
            scope = s.parent;
        }
        out
    }

    pub(crate) fn get_variable(
        &mut self,
        lib: &Library,
        frame: &Frame,
        path: &str,
    ) -> (Value, Option<ObjectId>) {
        if let Some(sep) = path.rfind([':', '.']) {
            let (target, var) = (
                path.get(..sep).unwrap_or(""),
                path.get(sep.saturating_add(1)..).unwrap_or(""),
            );
            if target.is_empty() {
                return (Value::Undefined, None);
            }
            for values in self.scope_values(frame) {
                if let Some(obj) = self.resolve_target_path(lib, frame, values, target, true, true)
                {
                    if self.has_property(obj, var) {
                        let v = self.get_member(lib, &Value::Object(obj), var);
                        return (v, Some(obj));
                    }
                }
            }
            return (Value::Undefined, None);
        }
        if path.contains('/') {
            for values in self.scope_values(frame) {
                if let Some(obj) = self.resolve_target_path(lib, frame, values, path, false, false)
                {
                    return (Value::Object(obj), None);
                }
            }
        }
        self.resolve(lib, frame, path)
    }

    pub(crate) fn set_variable(
        &mut self,
        lib: &Library,
        frame: &Frame,
        path: &str,
        value: Value,
    ) -> R<()> {
        if path.is_empty() {
            return Ok(());
        }
        if let Some(sep) = path.rfind([':', '.']) {
            let (target, var) = (
                path.get(..sep).unwrap_or(""),
                path.get(sep.saturating_add(1)..).unwrap_or(""),
            );
            for values in self.scope_values(frame) {
                if let Some(obj) = self.resolve_target_path(lib, frame, values, target, true, true)
                {
                    return self.set_member(lib, &Value::Object(obj), var, value);
                }
            }
            return Ok(());
        }
        self.set_simple(lib, frame, path, value)
    }

    pub(crate) fn root_object(&self) -> Option<ObjectId> {
        self.root.and_then(|r| self.script_object(r))
    }

    pub(crate) fn resolve_target_path(
        &mut self,
        lib: &Library,
        frame: &Frame,
        start: ObjectId,
        path: &str,
        mut first: bool,
        handle_this: bool,
    ) -> Option<ObjectId> {
        if path.is_empty() {
            return Some(start);
        }
        let (mut object, mut rest, mut slash) = if let Some(r) = path.strip_prefix('/') {
            first = false;
            (self.root_object()?, r, true)
        } else {
            (start, path, false)
        };
        let mut hops = 0u32;
        while !rest.is_empty() {
            hops = hops.saturating_add(1);
            if hops > 256 {
                return None;
            }
            rest = rest.trim_start_matches(':');
            if rest.starts_with("..") {
                let after = rest.get(2..).unwrap_or("");
                if after.starts_with('/') {
                    slash = true;
                }
                rest = after.get(1..).unwrap_or("");
                let inst = self.heap.objects.get(object).and_then(Object::display)?;
                let parent = self.arena.get(inst).and_then(|i| i.parent)?;
                object = self.script_object(parent)?;
                first = false;
                continue;
            }
            let mut end = rest.len();
            for (i, c) in rest.char_indices() {
                if c == ':' || (c == '.' && !slash) {
                    end = i;
                    break;
                }
                if c == '/' {
                    slash = true;
                    end = i;
                    break;
                }
            }
            let name = rest.get(..end).unwrap_or("");
            rest = rest.get(end.saturating_add(1)..).unwrap_or("");
            let value = if handle_this && name.eq_ignore_ascii_case("this") {
                frame.this.clone()
            } else if first
                && (name.eq_ignore_ascii_case("_root")
                    || super::display::level_number(name).is_some())
            {
                self.root_object().map_or(Value::Undefined, Value::Object)
            } else {
                let child = self
                    .heap
                    .objects
                    .get(object)
                    .and_then(Object::display)
                    .and_then(|inst| self.child_by_name(inst, name));
                match child {
                    Some(c) => self
                        .script_object(c)
                        .or_else(|| {
                            self.arena
                                .get(c)
                                .and_then(|i| i.parent)
                                .and_then(|p| self.script_object(p))
                        })
                        .map_or(Value::Undefined, Value::Object),
                    None => self.get_member(lib, &Value::Object(object), name),
                }
            };
            first = false;
            object = value.as_object()?;
        }
        Some(object)
    }

    pub(crate) fn resolve_target_display(
        &mut self,
        lib: &Library,
        frame: &Frame,
        start: InstanceId,
        target: &Value,
        allow_empty: bool,
    ) -> Option<InstanceId> {
        if let Value::Object(id) = target {
            if let Some(inst) = self.heap.objects.get(*id).and_then(Object::display) {
                return Some(inst);
            }
        }
        let path = self.string_of(lib, target);
        if !allow_empty && path.is_empty() {
            return None;
        }
        let start_obj = self.script_object(start)?;
        let obj = self.resolve_target_path(lib, frame, start_obj, &path, true, false)?;
        self.heap.objects.get(obj).and_then(Object::display)
    }

    fn target_or_root(&self, frame: &Frame) -> Option<InstanceId> {
        frame.target.or(self.root)
    }

    pub(crate) fn call_value(
        &mut self,
        lib: &Library,
        caller: &Frame,
        callee: &Value,
        this: Value,
        args: Vec<Value>,
        name: &str,
    ) -> R<Value> {
        let native = callee
            .as_object()
            .and_then(|id| self.heap.objects.get(id).map(|o| (id, o.native.clone())));
        match native {
            Some((id, Native::Function(f))) => {
                if self.record_calls {
                    super::bump(&mut self.script_stats.bytecode_calls, name.to_owned());
                }
                self.call_bytecode(lib, caller, &f, id, this, args)
            }
            Some((_, Native::Builtin(nf))) => {
                if self.record_calls {
                    super::bump(&mut self.script_stats.native_calls, name.to_owned());
                }
                if crate::trace::enabled() {
                    let arg_strings: Vec<String> = args
                        .iter()
                        .map(|a| self.string_of(lib, a).to_string())
                        .collect();
                    let result = nf(self, lib, caller, this, args);
                    let result_text = match &result {
                        Ok(value) => format!("{value:?}"),
                        Err(halt) => format!("error: {halt:?}"),
                    };
                    crate::trace::fire(name, &arg_strings, &result_text);
                    result
                } else {
                    nf(self, lib, caller, this, args)
                }
            }
            _ => {
                super::bump(&mut self.script_stats.unresolved_calls, name.to_owned());
                if name.eq_ignore_ascii_case("GameCode")
                    || name.eq_ignore_ascii_case("GameCodeNoPrefix")
                {
                    let args: Vec<String> = args
                        .iter()
                        .map(|a| self.string_of(lib, a).to_string())
                        .collect();
                    if crate::trace::enabled() {
                        crate::trace::fire(name, &args, "Undefined (outward)");
                    }
                    self.dispatch_outward(Outward::GameCode {
                        name: name.to_owned(),
                        args,
                    });
                    return Ok(Value::Undefined);
                }
                if name.eq_ignore_ascii_case("GetExtern") {
                    let key = args.first().map(|a| self.string_of(lib, a));
                    let key_args: Vec<String> = key.iter().map(|k| k.to_string()).collect();
                    let result = key
                        .and_then(|k| self.read_extern(k.as_ref()))
                        .unwrap_or(Value::Undefined);
                    if crate::trace::enabled() {
                        crate::trace::fire(name, &key_args, &format!("{result:?}"));
                    }
                    return Ok(result);
                }
                if name.eq_ignore_ascii_case("ToggleExtern") {
                    if let Some(key) = args.first().map(|a| self.string_of(lib, a)) {
                        let current = self
                            .externs
                            .get(key.as_ref())
                            .map(Value::to_bool)
                            .unwrap_or(false);
                        self.externs
                            .insert(Arc::from(key.as_ref()), Value::Bool(!current));
                        self.write_extern_live(key.as_ref(), &Value::Bool(!current));
                        if crate::trace::enabled() {
                            crate::trace::fire(
                                name,
                                &[key.to_string()],
                                &format!("{:?}", Value::Bool(!current)),
                            );
                        }
                    }
                    return Ok(Value::Undefined);
                }
                Ok(Value::Undefined)
            }
        }
    }

    pub(crate) fn dispatch_outward(&mut self, value: Outward) {
        if let Some(mut host) = self.sync_host.take() {
            let handled = host.command(&value);
            self.sync_host = Some(host);
            if handled {
                return;
            }
        }
        self.push_outward(value);
    }

    fn read_extern_live(&mut self, name: &str) -> Option<Value> {
        let mut host = self.sync_host.take()?;
        let live = host.get_extern(name);
        self.sync_host = Some(host);
        live
    }

    fn write_extern_live(&mut self, name: &str, value: &Value) {
        if let Some(mut host) = self.sync_host.take() {
            host.set_extern(name, value);
            self.sync_host = Some(host);
        }
        if self.native_externs.contains(name)
            && self.native_extern_writes.len() < self.vm_limits.max_outward
        {
            self.native_extern_writes
                .push((Arc::from(name), value.clone()));
        }
    }

    fn extern_query(&self, name: &str) -> Option<Value> {
        let target = name.strip_prefix('?')?;
        let known = self.native_externs.contains(target) || self.externs.contains_key(target);
        Some(Value::str(if known { "extern" } else { "0" }))
    }

    fn read_extern(&mut self, name: &str) -> Option<Value> {
        if let Some(answer) = self.extern_query(name) {
            return Some(answer);
        }
        if let Some(mut host) = self.sync_host.take() {
            let live = host.get_extern(name);
            self.sync_host = Some(host);
            if live.is_some() {
                return live;
            }
        }
        self.externs.get(name).cloned()
    }

    pub(crate) fn push_outward(&mut self, value: Outward) {
        if self.outward.len() < self.vm_limits.max_outward {
            self.outward.push(value);
        } else {
            self.fail(crate::Error::Limit {
                what: "outward values",
                limit: self.vm_limits.max_outward,
            });
        }
    }

    fn call_bytecode(
        &mut self,
        lib: &Library,
        caller: &Frame,
        f: &BytecodeFn,
        callee: ObjectId,
        this: Value,
        args: Vec<Value>,
    ) -> R<Value> {
        if caller.depth >= self.vm_limits.max_call_depth {
            return Err(Halt::Limit { what: "call depth" });
        }
        let block = Arc::clone(&f.block);
        let Some(function) = block.functions.get(f.index) else {
            return Err(Halt::BadIndex { offset: 0 });
        };
        let this_inst = this
            .as_object()
            .and_then(|o| self.heap.objects.get(o))
            .and_then(Object::display);
        let base_clip = f
            .base_clip
            .filter(|b| self.arena.contains(*b))
            .or(this_inst)
            .or(caller.target);
        let locals = self.alloc(None, Native::Plain)?;
        let scope = self.new_scope(Some(f.scope), ScopeClass::Local, locals)?;
        let inherits_this =
            fflags::swf(function) & (fflags::PRELOAD_THIS | fflags::SUPPRESS_THIS) != 0;
        let local_this = if inherits_this && function.is_function2 {
            caller.this.clone()
        } else {
            this.clone()
        };
        let mut frame = Frame {
            movie: f.movie,
            scope,
            pool: Arc::clone(&f.pool),
            this: local_this,
            base_clip,
            target: base_clip,
            registers: None,
            stack: Vec::new(),
            depth: caller.depth.saturating_add(1),
        };
        self.bind_arguments(lib, function, &mut frame, locals, callee, &this, &args)?;
        let end = function.body_start.saturating_add(function.body_size);
        let flow = self.exec(lib, &block, &function.body, end, &mut frame)?;
        Ok(match flow {
            Flow::Returned(v) => v,
            Flow::Ended => Value::Undefined,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn bind_arguments(
        &mut self,
        lib: &Library,
        function: &Function,
        frame: &mut Frame,
        locals: ObjectId,
        callee: ObjectId,
        this: &Value,
        args: &[Value],
    ) -> R<()> {
        let arguments = self.new_array(args.to_vec())?;
        if let Some(o) = self.heap.objects.get_mut(arguments) {
            o.props.define("callee", Value::Object(callee), true);
        }
        let locals_v = Value::Object(locals);
        if function.is_function2 {
            let flags = fflags::swf(function);
            let mut regs: Vec<Value> = vec![Value::Undefined];
            let preload = |value: Value, regs: &mut Vec<Value>| regs.push(value);
            if flags & fflags::PRELOAD_THIS != 0 {
                let v = if flags & fflags::SUPPRESS_THIS != 0 {
                    Value::Undefined
                } else {
                    this.clone()
                };
                preload(v, &mut regs);
            }
            if flags & fflags::PRELOAD_ARGUMENTS != 0 {
                preload(Value::Object(arguments), &mut regs);
            } else if flags & fflags::SUPPRESS_ARGUMENTS == 0 {
                self.set_member(lib, &locals_v, "arguments", Value::Object(arguments))?;
            }
            if flags & fflags::PRELOAD_SUPER != 0 {
                preload(Value::Undefined, &mut regs);
            }
            if flags & fflags::PRELOAD_ROOT != 0 {
                preload(
                    self.root_object().map_or(Value::Undefined, Value::Object),
                    &mut regs,
                );
            }
            if flags & fflags::PRELOAD_PARENT != 0 {
                if let Some(parent) = frame
                    .base_clip
                    .and_then(|b| self.arena.get(b))
                    .and_then(|i| i.parent)
                {
                    preload(
                        self.script_object(parent)
                            .map_or(Value::Undefined, Value::Object),
                        &mut regs,
                    );
                }
            }
            if flags & fflags::PRELOAD_GLOBAL != 0 {
                preload(Value::Object(self.heap.global), &mut regs);
            }
            let count = fflags::register_count(function);
            if regs.len() < count {
                regs.resize(count, Value::Undefined);
            }
            for (i, param) in function.params.iter().enumerate() {
                let value = args.get(i).cloned().unwrap_or(Value::Undefined);
                if param.register > 0 && param.register < 256 {
                    let r = param.register as usize;
                    if regs.len() <= r {
                        regs.resize(r.saturating_add(1), Value::Undefined);
                    }
                    if let Some(slot) = regs.get_mut(r) {
                        *slot = value;
                    }
                } else {
                    self.set_member(lib, &locals_v, &param.name, value)?;
                }
            }
            frame.registers = Some(regs);
        } else {
            self.set_member(lib, &locals_v, "arguments", Value::Object(arguments))?;
            for (i, param) in function.params.iter().enumerate() {
                let value = args.get(i).cloned().unwrap_or(Value::Undefined);
                self.set_member(lib, &locals_v, &param.name, value)?;
            }
        }
        Ok(())
    }

    pub(crate) fn construct(
        &mut self,
        lib: &Library,
        caller: &Frame,
        ctor: &Value,
        args: Vec<Value>,
        name: &str,
    ) -> R<Value> {
        let proto = match self.get_member(lib, ctor, "prototype") {
            Value::Object(p) => Some(p),
            _ => Some(self.heap.protos.object),
        };
        let this = self.alloc(proto, Native::Plain)?;
        if let Some(o) = self.heap.objects.get_mut(this) {
            o.props.define("constructor", ctor.clone(), true);
        }
        let result = self.call_value(lib, caller, ctor, Value::Object(this), args, name)?;
        Ok(match result {
            Value::Object(o) => Value::Object(o),
            _ => Value::Object(this),
        })
    }

    fn exec(
        &mut self,
        lib: &Library,
        block: &Arc<Block>,
        code: &[Instruction],
        region_end: u32,
        frame: &mut Frame,
    ) -> R<Flow> {
        let mut pc = 0usize;
        loop {
            let Some(instruction) = code.get(pc) else {
                return Ok(Flow::Ended);
            };
            if self.budget == 0 {
                return Err(Halt::Budget {
                    offset: instruction.offset,
                });
            }
            self.budget = self.budget.saturating_sub(1);
            pc = pc.saturating_add(1);
            match self.step(lib, block, code, region_end, frame, instruction, &mut pc)? {
                Step::Next => {}
                Step::End => return Ok(Flow::Ended),
                Step::Return(v) => return Ok(Flow::Returned(v)),
            }
            if frame.base_clip.is_some_and(|b| !self.arena.contains(b)) {
                return Ok(Flow::Ended);
            }
        }
    }

    fn push(&self, frame: &mut Frame, value: Value) -> R<()> {
        if frame.stack.len() >= self.vm_limits.max_stack {
            return Err(Halt::Limit {
                what: "operand stack depth",
            });
        }
        frame.stack.push(value);
        Ok(())
    }

    fn pop(frame: &mut Frame, at: u32) -> R<Value> {
        frame.stack.pop().ok_or(Halt::StackUnderflow { offset: at })
    }

    fn pop_count(frame: &mut Frame, at: u32) -> R<usize> {
        let n = Self::pop(frame, at)?.to_number();
        if n.is_nan() || !(0.0..=65_536.0).contains(&n) {
            return Err(Halt::Limit {
                what: "argument or element count",
            });
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Ok(n as usize)
    }

    fn pop_args(frame: &mut Frame, at: u32) -> R<Vec<Value>> {
        let count = Self::pop_count(frame, at)?;
        let mut args = Vec::with_capacity(count.min(frame.stack.len()));
        for _ in 0..count {
            args.push(Self::pop(frame, at)?);
        }
        Ok(args)
    }

    fn pool_get(frame: &Frame, index: usize, at: u32) -> R<Value> {
        frame
            .pool
            .get(index)
            .cloned()
            .ok_or(Halt::BadIndex { offset: at })
    }

    fn const_item(lib: &Library, movie: MovieId, index: usize, at: u32) -> R<Value> {
        lib.movie(movie)
            .and_then(|m| m.consts.get(index))
            .cloned()
            .ok_or(Halt::BadIndex { offset: at })
    }

    fn read_register(&self, frame: &Frame, index: usize) -> Value {
        let file = frame.registers.as_ref().unwrap_or(&self.heap.registers);
        file.get(index).cloned().unwrap_or(Value::Undefined)
    }

    fn write_register(&mut self, frame: &mut Frame, index: usize, value: Value) {
        let file = match frame.registers.as_mut() {
            Some(local) => local,
            None => &mut self.heap.registers,
        };
        if file.len() <= index {
            file.resize(index.saturating_add(1), Value::Undefined);
        }
        if let Some(slot) = file.get_mut(index) {
            *slot = value;
        }
    }

    fn concat(&self, a: &str, b: &str) -> R<Value> {
        if a.len().saturating_add(b.len()) > self.vm_limits.max_string_len {
            return Err(Halt::Limit {
                what: "string length",
            });
        }
        let mut s = String::with_capacity(a.len().saturating_add(b.len()));
        s.push_str(a);
        s.push_str(b);
        Ok(Value::String(Arc::from(s.as_str())))
    }

    fn jump(
        code: &[Instruction],
        region_end: u32,
        instruction: &Instruction,
        delta: i32,
        at: u32,
    ) -> R<usize> {
        let after = instruction
            .offset
            .checked_add(1)
            .map(|p| p.saturating_add(3) & !3u32)
            .and_then(|aligned| aligned.checked_add(4))
            .ok_or(Halt::BadJump { offset: at })?;
        let target = if delta >= 0 {
            after.checked_add(delta.unsigned_abs())
        } else {
            after.checked_sub(delta.unsigned_abs())
        }
        .ok_or(Halt::BadJump { offset: at })?;
        match code.binary_search_by_key(&target, |i| i.offset) {
            Ok(i) => Ok(i),
            Err(_) if target == region_end => Ok(code.len()),
            Err(_) => Err(Halt::BadJump { offset: at }),
        }
    }

    pub(crate) fn next_random(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn goto_expression(&mut self, lib: &Library, frame: &Frame, target: Value, play: bool) {
        let Some(clip) = self.target_or_root(frame) else {
            return;
        };
        super::natives::goto_value(self, lib, frame, clip, &target, !play);
    }

    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    fn step(
        &mut self,
        lib: &Library,
        block: &Arc<Block>,
        code: &[Instruction],
        region_end: u32,
        frame: &mut Frame,
        instruction: &Instruction,
        pc: &mut usize,
    ) -> R<Step> {
        let at = instruction.offset;
        match &instruction.op {
            Op::End => return Ok(Step::End),
            Op::Return => {
                let v = Self::pop(frame, at)?;
                return Ok(Step::Return(v));
            }

            Op::EaPushZero => self.push(frame, Value::Number(0.0))?,
            Op::EaPushOne => self.push(frame, Value::Number(1.0))?,
            Op::EaPushTrue => self.push(frame, Value::Bool(true))?,
            Op::EaPushFalse => self.push(frame, Value::Bool(false))?,
            Op::EaPushNull => self.push(frame, Value::Null)?,
            Op::EaPushUndefined => self.push(frame, Value::Undefined)?,
            Op::EaPushByte(v) => self.push(frame, Value::Number(f64::from(*v)))?,
            Op::EaPushShort(v) => self.push(frame, Value::Number(f64::from(*v)))?,
            Op::EaPushLong(v) => self.push(frame, Value::Number(f64::from(*v)))?,
            Op::EaPushFloat(v) => self.push(frame, Value::Number(f64::from(*v)))?,
            Op::EaPushString(s) => self.push(frame, Value::str(s))?,
            Op::EaPushThis => {
                self.script_stats.this_reads = self.script_stats.this_reads.saturating_add(1);
                if frame.this == Value::Undefined {
                    self.script_stats.this_undefined =
                        self.script_stats.this_undefined.saturating_add(1);
                }
                let this = frame.this.clone();
                self.push(frame, this)?;
            }
            Op::EaPushGlobal => self.push(frame, Value::Object(self.heap.global))?,
            Op::EaPushConstant(i) => {
                let v = Self::pool_get(frame, usize::from(*i), at)?;
                self.push(frame, v)?;
            }
            Op::EaPushWordConstant(i) => {
                let v = Self::pool_get(frame, usize::from(*i), at)?;
                self.push(frame, v)?;
            }
            Op::EaPushRegister(i) => {
                let v = self.read_register(frame, usize::from(*i));
                self.push(frame, v)?;
            }
            Op::PushData(indices) => {
                for index in indices {
                    let v = match Self::const_item(lib, frame.movie, *index as usize, at)? {
                        Value::Number(n) if n >= 0.0 => {
                            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                            let r = n as usize;
                            self.read_register(frame, r)
                        }
                        other => other,
                    };
                    self.push(frame, v)?;
                }
            }
            Op::ConstantPool(indices) => {
                let mut pool = Vec::with_capacity(indices.len().min(65_536));
                for index in indices {
                    pool.push(Self::const_item(lib, frame.movie, *index as usize, at)?);
                }
                frame.pool = Arc::from(pool);
            }
            Op::Pop => {
                Self::pop(frame, at)?;
            }
            Op::Dup => {
                let v = frame
                    .stack
                    .last()
                    .cloned()
                    .ok_or(Halt::StackUnderflow { offset: at })?;
                self.push(frame, v)?;
            }
            Op::Swap => {
                let a = Self::pop(frame, at)?;
                let b = Self::pop(frame, at)?;
                self.push(frame, a)?;
                self.push(frame, b)?;
            }
            Op::SetRegister(index) => {
                let v = frame
                    .stack
                    .last()
                    .cloned()
                    .ok_or(Halt::StackUnderflow { offset: at })?;
                let index = *index as usize;
                if index >= 256 {
                    return Err(Halt::Limit {
                        what: "register index",
                    });
                }
                self.write_register(frame, index, v);
            }

            Op::GetVariable => {
                let name = Self::pop(frame, at)?;
                let name = self.string_of(lib, &name);
                let (v, _) = self.get_variable(lib, frame, &name);
                self.push(frame, v)?;
            }
            Op::SetVariable => {
                let v = Self::pop(frame, at)?;
                let name = Self::pop(frame, at)?;
                let name = self.string_of(lib, &name);
                if name.eq_ignore_ascii_case("this") {
                    frame.this = v;
                } else {
                    self.set_variable(lib, frame, &name, v)?;
                }
            }
            Op::EaPushValueOfVar(i) => {
                let name = Self::pool_get(frame, usize::from(*i), at)?;
                let name = self.string_of(lib, &name);
                let (v, _) = self.get_variable(lib, frame, &name);
                self.push(frame, v)?;
            }
            Op::EaGetStringVar(name) => {
                let (v, _) = self.get_variable(lib, frame, name);
                self.push(frame, v)?;
            }
            Op::EaSetStringVar(value) => {
                let name = Self::pop(frame, at)?;
                let name = self.string_of(lib, &name);
                self.set_variable(lib, frame, &name, Value::str(value))?;
            }
            Op::EaZeroVariable => {
                let name = Self::pop(frame, at)?;
                let name = self.string_of(lib, &name);
                self.set_variable(lib, frame, &name, Value::Number(0.0))?;
            }
            Op::DefineLocal => {
                let v = Self::pop(frame, at)?;
                let name = Self::pop(frame, at)?;
                let name = self.string_of(lib, &name);
                self.define_local(lib, frame, &name, v)?;
            }
            Op::Var => {
                let name = Self::pop(frame, at)?;
                let name = self.string_of(lib, &name);
                let values = self.heap.scopes.get(frame.scope).map(|s| s.values);
                let exists = values.is_some_and(|v| {
                    self.heap
                        .objects
                        .get(v)
                        .is_some_and(|o| o.props.contains(&name))
                });
                if !exists {
                    self.define_local(lib, frame, &name, Value::Undefined)?;
                }
            }
            Op::Delete => {
                let name = Self::pop(frame, at)?;
                let object = Self::pop(frame, at)?;
                let name = self.string_of(lib, &name);
                let removed = object
                    .as_object()
                    .and_then(|o| self.heap.objects.get_mut(o))
                    .is_some_and(|o| o.props.remove(&name));
                self.push(frame, Value::Bool(removed))?;
            }
            Op::Delete2 => {
                let name = Self::pop(frame, at)?;
                let name = self.string_of(lib, &name);
                let mut removed = false;
                for values in self.scope_values(frame) {
                    if let Some(o) = self.heap.objects.get_mut(values) {
                        if o.props.remove(&name) {
                            removed = true;
                            break;
                        }
                    }
                }
                self.push(frame, Value::Bool(removed))?;
            }

            Op::GetMember => {
                let name = Self::pop(frame, at)?;
                let object = Self::pop(frame, at)?;
                let name = self.string_of(lib, &name);
                let v = self.get_member(lib, &object, &name);
                self.push(frame, v)?;
            }
            Op::SetMember => {
                let v = Self::pop(frame, at)?;
                let name = Self::pop(frame, at)?;
                let object = Self::pop(frame, at)?;
                let name = self.string_of(lib, &name);
                self.set_member(lib, &object, &name, v)?;
            }
            Op::EaGetNamedMember(i) => {
                let name = Self::pool_get(frame, usize::from(*i), at)?;
                let name = self.string_of(lib, &name);
                let object = Self::pop(frame, at)?;
                let v = self.get_member(lib, &object, &name);
                self.push(frame, v)?;
            }
            Op::EaGetStringMember(name) => {
                let object = Self::pop(frame, at)?;
                let v = self.get_member(lib, &object, name);
                self.push(frame, v)?;
            }
            Op::EaSetStringMember(value) => {
                let name = Self::pop(frame, at)?;
                let object = Self::pop(frame, at)?;
                let name = self.string_of(lib, &name);
                self.set_member(lib, &object, &name, Value::str(value))?;
            }
            Op::InitObject => {
                let count = Self::pop_count(frame, at)?;
                let id = self.new_object()?;
                for _ in 0..count {
                    let v = Self::pop(frame, at)?;
                    let name = Self::pop(frame, at)?;
                    let name = self.string_of(lib, &name);
                    self.set_member(lib, &Value::Object(id), &name, v)?;
                }
                self.push(frame, Value::Object(id))?;
            }
            Op::InitArray => {
                let count = Self::pop_count(frame, at)?;
                let mut values = Vec::with_capacity(count.min(frame.stack.len()));
                for _ in 0..count {
                    values.push(Self::pop(frame, at)?);
                }
                let id = self.new_array(values)?;
                self.push(frame, Value::Object(id))?;
            }
            Op::Enumerate | Op::Enumerate2 => {
                let value = if matches!(instruction.op, Op::Enumerate) {
                    let name = Self::pop(frame, at)?;
                    let name = self.string_of(lib, &name);
                    self.get_variable(lib, frame, &name).0
                } else {
                    Self::pop(frame, at)?
                };
                self.push(frame, Value::Undefined)?;
                if let Value::Object(id) = value {
                    let keys = self.enumerate_keys(id);
                    for key in keys.into_iter().rev() {
                        self.push(frame, Value::String(key))?;
                    }
                }
            }

            Op::Add | Op::Subtract | Op::Multiply | Op::Divide | Op::Modulo => {
                let b = Self::pop(frame, at)?.to_number();
                let a = Self::pop(frame, at)?.to_number();
                let v = match instruction.op {
                    Op::Add => a + b,
                    Op::Subtract => a - b,
                    Op::Multiply => a * b,
                    Op::Divide => a / b,
                    _ => a % b,
                };
                self.push(frame, Value::Number(v))?;
            }
            Op::NewAdd => {
                let b = Self::pop(frame, at)?;
                let a = Self::pop(frame, at)?;
                let v = if matches!(a, Value::String(_)) || matches!(b, Value::String(_)) {
                    let (x, y) = (self.string_of(lib, &a), self.string_of(lib, &b));
                    self.concat(&x, &y)?
                } else {
                    Value::Number(a.to_number() + b.to_number())
                };
                self.push(frame, v)?;
            }
            Op::StringConcat => {
                let b = Self::pop(frame, at)?;
                let a = Self::pop(frame, at)?;
                let (x, y) = (self.string_of(lib, &a), self.string_of(lib, &b));
                let v = self.concat(&x, &y)?;
                self.push(frame, v)?;
            }
            Op::Increment | Op::Decrement => {
                let n = Self::pop(frame, at)?.to_number();
                let v = if matches!(instruction.op, Op::Increment) {
                    n + 1.0
                } else {
                    n - 1.0
                };
                self.push(frame, Value::Number(v))?;
            }
            Op::Equals | Op::LessThan => {
                let b = Self::pop(frame, at)?.to_number();
                let a = Self::pop(frame, at)?.to_number();
                let r = if matches!(instruction.op, Op::Equals) {
                    a == b
                } else {
                    a < b
                };
                self.push(frame, Value::Bool(r))?;
            }
            Op::NewEquals => {
                let b = Self::pop(frame, at)?;
                let a = Self::pop(frame, at)?;
                self.push(frame, Value::Bool(loose_equals(&a, &b)))?;
            }
            Op::StrictEquals => {
                let b = Self::pop(frame, at)?;
                let a = Self::pop(frame, at)?;
                self.push(frame, Value::Bool(a == b))?;
            }
            Op::NewLessThan | Op::Greater => {
                let b = Self::pop(frame, at)?;
                let a = Self::pop(frame, at)?;
                let greater = matches!(instruction.op, Op::Greater);
                let r = match (&a, &b) {
                    (Value::String(x), Value::String(y)) => {
                        if greater {
                            x > y
                        } else {
                            x < y
                        }
                    }
                    _ => {
                        let (x, y) = (a.to_number(), b.to_number());
                        if greater {
                            x > y
                        } else {
                            x < y
                        }
                    }
                };
                self.push(frame, Value::Bool(r))?;
            }
            Op::StringEquals | Op::StringGreater | Op::StringCompare => {
                let b = Self::pop(frame, at)?;
                let a = Self::pop(frame, at)?;
                let (x, y) = (self.string_of(lib, &a), self.string_of(lib, &b));
                let r = match instruction.op {
                    Op::StringEquals => x == y,
                    Op::StringGreater => x > y,
                    _ => x < y,
                };
                self.push(frame, Value::Bool(r))?;
            }
            Op::StringLength | Op::MbLength => {
                let a = Self::pop(frame, at)?;
                #[allow(clippy::cast_precision_loss)]
                let n = self.string_of(lib, &a).chars().count() as f64;
                self.push(frame, Value::Number(n))?;
            }
            Op::Substring | Op::MbSubstring => {
                let count = Self::pop(frame, at)?.to_i32();
                let index = Self::pop(frame, at)?.to_i32();
                let s = Self::pop(frame, at)?;
                let s = self.string_of(lib, &s);
                let start = usize::try_from(index.saturating_sub(1).max(0)).unwrap_or(0);
                let take = usize::try_from(count.max(0)).unwrap_or(0);
                let out: String = s.chars().skip(start).take(take).collect();
                self.push(frame, Value::str(&out))?;
            }
            Op::Ord | Op::MbOrd => {
                let s = Self::pop(frame, at)?;
                let s = self.string_of(lib, &s);
                let c = s.chars().next().map_or(0, u32::from);
                self.push(frame, Value::Number(f64::from(c)))?;
            }
            Op::Chr | Op::MbChr => {
                let n = Self::pop(frame, at)?.to_i32();
                let c = u32::try_from(n)
                    .ok()
                    .and_then(char::from_u32)
                    .unwrap_or('\0');
                let s = if c == '\0' {
                    String::new()
                } else {
                    c.to_string()
                };
                self.push(frame, Value::str(&s))?;
            }
            Op::LogicalNot => {
                let a = Self::pop(frame, at)?;
                self.push(frame, Value::Bool(!a.to_bool()))?;
            }
            Op::LogicalAnd | Op::LogicalOr => {
                let b = Self::pop(frame, at)?.to_bool();
                let a = Self::pop(frame, at)?.to_bool();
                let r = if matches!(instruction.op, Op::LogicalAnd) {
                    a && b
                } else {
                    a || b
                };
                self.push(frame, Value::Bool(r))?;
            }
            Op::BitwiseAnd
            | Op::BitwiseOr
            | Op::BitwiseXor
            | Op::ShiftLeft
            | Op::ShiftRight
            | Op::ShiftRight2 => {
                let y = Self::pop(frame, at)?.to_i32();
                let x = Self::pop(frame, at)?.to_i32();
                let xu = u32::from_ne_bytes(x.to_ne_bytes());
                let shift = u32::from_ne_bytes(y.to_ne_bytes()) & 31;
                let v = match instruction.op {
                    Op::BitwiseAnd => f64::from(x & y),
                    Op::BitwiseOr => f64::from(x | y),
                    Op::BitwiseXor => f64::from(x ^ y),
                    Op::ShiftLeft => f64::from(x.wrapping_shl(shift)),
                    Op::ShiftRight => f64::from(x.wrapping_shr(shift)),
                    _ => f64::from(xu.wrapping_shr(shift)),
                };
                self.push(frame, Value::Number(v))?;
            }
            Op::ToNumber => {
                let a = Self::pop(frame, at)?;
                self.push(frame, Value::Number(a.to_number()))?;
            }
            Op::ToInteger => {
                let n = Self::pop(frame, at)?.to_number();
                let v = if n.is_nan() { 0.0 } else { n.trunc() };
                self.push(frame, Value::Number(v))?;
            }
            Op::ToString => {
                let a = Self::pop(frame, at)?;
                let s = self.string_of(lib, &a);
                self.push(frame, Value::String(s))?;
            }
            Op::TypeOf => {
                let a = Self::pop(frame, at)?;
                let t = self.type_of(&a);
                self.push(frame, Value::str(t))?;
            }
            Op::Random => {
                let max = Self::pop(frame, at)?.to_i32();
                let v = if max > 0 {
                    let m = u64::from(max.unsigned_abs());
                    let r = self.next_random().checked_rem(m).unwrap_or(0);
                    #[allow(clippy::cast_precision_loss)]
                    let r = r as f64;
                    r
                } else {
                    0.0
                };
                self.push(frame, Value::Number(v))?;
            }
            Op::GetTimer => {
                let ms = self.now_ms();
                self.push(frame, Value::Number(ms.trunc()))?;
            }
            Op::Trace => {
                let a = Self::pop(frame, at)?;
                let s = self.string_of(lib, &a);
                self.script_stats.traces = self.script_stats.traces.saturating_add(1);
                if let Some(log) = &mut self.traces {
                    if log.len() < 4096 {
                        log.push(s.to_string());
                    }
                }
            }
            Op::ToggleQuality | Op::StopSounds => {}

            Op::BranchAlways(delta) => *pc = Self::jump(code, region_end, instruction, *delta, at)?,
            Op::BranchIfTrue(delta) => {
                if Self::pop(frame, at)?.to_bool() {
                    *pc = Self::jump(code, region_end, instruction, *delta, at)?;
                }
            }
            Op::EaBranchIfFalse(delta) => {
                if !Self::pop(frame, at)?.to_bool() {
                    *pc = Self::jump(code, region_end, instruction, *delta, at)?;
                }
            }

            Op::Stop | Op::Play | Op::NextFrame | Op::PrevFrame => {
                if let Some(clip) = frame.target {
                    match instruction.op {
                        Op::Stop => self.set_playing(clip, false),
                        Op::Play => self.set_playing(clip, true),
                        Op::NextFrame => self.step_frame(lib, clip, true),
                        _ => self.step_frame(lib, clip, false),
                    }
                }
            }
            Op::GotoFrame(frame_index) => {
                if let Some(clip) = frame.target {
                    let n = frame_index.saturating_add(1).clamp(1, i32::from(u16::MAX));
                    self.goto_frame(lib, clip, u16::try_from(n).unwrap_or(1), true);
                }
            }
            Op::GotoLabel(label) => {
                if let Some(clip) = frame.target {
                    self.goto_label(lib, clip, label, true);
                }
            }
            Op::GotoExpression(flags) => {
                let target = Self::pop(frame, at)?;
                self.goto_expression(lib, frame, target, flags & 1 != 0);
            }
            Op::SetTarget(path) => {
                let target = Value::str(path);
                self.set_target(lib, frame, &target)?;
            }
            Op::SetTargetExpression => {
                let target = Self::pop(frame, at)?;
                self.set_target(lib, frame, &target)?;
            }
            Op::GetUrl { url, target } => self.get_url(url, target),
            Op::GetUrl2 => {
                let target = Self::pop(frame, at)?;
                let url = Self::pop(frame, at)?;
                let (url, target) = (self.string_of(lib, &url), self.string_of(lib, &target));
                self.get_url(&url, &target);
            }
            Op::TargetPath => {
                let v = Self::pop(frame, at)?;
                let path = v
                    .as_object()
                    .and_then(|o| self.heap.objects.get(o))
                    .and_then(Object::display)
                    .map_or(Value::Undefined, |inst| {
                        Value::str(&self.target_path_dot(inst))
                    });
                self.push(frame, path)?;
            }
            Op::GetProperty => {
                let index = Self::pop(frame, at)?.to_number();
                let path = Self::pop(frame, at)?;
                let start = self.target_or_root(frame);
                let clip =
                    start.and_then(|s| self.resolve_target_display(lib, frame, s, &path, true));
                let v = match (clip, super::display::index_from_number(index)) {
                    (Some(clip), Some(i)) => self.get_display_property(lib, clip, i),
                    _ => Value::Undefined,
                };
                self.push(frame, v)?;
            }
            Op::SetProperty => {
                let value = Self::pop(frame, at)?;
                let index = Self::pop(frame, at)?.to_number();
                let path = Self::pop(frame, at)?;
                let start = self.target_or_root(frame);
                let clip =
                    start.and_then(|s| self.resolve_target_display(lib, frame, s, &path, true));
                if let (Some(clip), Some(i)) = (clip, super::display::index_from_number(index)) {
                    self.set_display_property(lib, clip, i, &value);
                }
            }
            Op::DuplicateClip => {
                let depth = Self::pop(frame, at)?.to_i32();
                let name = Self::pop(frame, at)?;
                let target = Self::pop(frame, at)?;
                let name = self.string_of(lib, &name);
                let start = self.target_or_root(frame);
                if let Some(clip) =
                    start.and_then(|s| self.resolve_target_display(lib, frame, s, &target, false))
                {
                    super::natives::clone_sprite(
                        self,
                        lib,
                        clip,
                        &name,
                        depth.wrapping_add(crate::AVM_DEPTH_BIAS),
                    );
                }
            }
            Op::RemoveClip => {
                let target = Self::pop(frame, at)?;
                let start = self.target_or_root(frame);
                if let Some(clip) =
                    start.and_then(|s| self.resolve_target_display(lib, frame, s, &target, true))
                {
                    super::natives::remove_display_object(self, lib, clip);
                }
            }

            Op::DefineFunction(index) | Op::DefineFunction2(index) => {
                let Some(function) = block.functions.get(*index) else {
                    return Err(Halt::BadIndex { offset: at });
                };
                let name = function.name.clone();
                let proto = self.new_object()?;
                let f = self.alloc(
                    Some(self.heap.protos.function),
                    Native::Function(Box::new(BytecodeFn {
                        block: Arc::clone(block),
                        index: *index,
                        movie: frame.movie,
                        scope: frame.scope,
                        pool: Arc::clone(&frame.pool),
                        base_clip: frame.base_clip,
                    })),
                )?;
                if let Some(o) = self.heap.objects.get_mut(f) {
                    o.props.define("prototype", Value::Object(proto), true);
                }
                if let Some(o) = self.heap.objects.get_mut(proto) {
                    o.props.define("constructor", Value::Object(f), true);
                }
                if name.is_empty() {
                    self.push(frame, Value::Object(f))?;
                } else {
                    self.define_local(lib, frame, &name, Value::Object(f))?;
                }
            }
            Op::CallFunction | Op::EaCallFunction | Op::EaCallFunctionPop => {
                let name = Self::pop(frame, at)?;
                let name = self.string_of(lib, &name);
                let args = Self::pop_args(frame, at)?;
                let v = self.call_named(lib, frame, &name, args)?;
                self.deliver(lib, frame, &instruction.op, v, at)?;
            }
            Op::EaCallNamedFunction(i) | Op::EaCallNamedFunctionPop(i) => {
                let name = Self::pool_get(frame, usize::from(*i), at)?;
                let name = self.string_of(lib, &name);
                let args = Self::pop_args(frame, at)?;
                let v = self.call_named(lib, frame, &name, args)?;
                self.deliver(lib, frame, &instruction.op, v, at)?;
            }
            Op::CallMethod | Op::EaCallMethod | Op::EaCallMethodPop => {
                let name = Self::pop(frame, at)?;
                let object = Self::pop(frame, at)?;
                let args = Self::pop_args(frame, at)?;
                let v = self.call_method(lib, frame, &object, &name, args)?;
                self.deliver(lib, frame, &instruction.op, v, at)?;
            }
            Op::EaCallNamedMethod(i) | Op::EaCallNamedMethodPop(i) => {
                let name = Self::pool_get(frame, usize::from(*i), at)?;
                let object = Self::pop(frame, at)?;
                let args = Self::pop_args(frame, at)?;
                let v = self.call_method(lib, frame, &object, &name, args)?;
                self.deliver(lib, frame, &instruction.op, v, at)?;
            }
            Op::New => {
                let name = Self::pop(frame, at)?;
                let name = self.string_of(lib, &name);
                let args = Self::pop_args(frame, at)?;
                let (ctor, _) = self.resolve(lib, frame, &name);
                let v = self.construct(lib, frame, &ctor, args, &name)?;
                self.push(frame, v)?;
            }
            Op::NewMethod => {
                let name = Self::pop(frame, at)?;
                let object = Self::pop(frame, at)?;
                let args = Self::pop_args(frame, at)?;
                let v = if matches!(object, Value::Undefined | Value::Null) {
                    Value::Undefined
                } else {
                    let name = if name == Value::Undefined {
                        Arc::from("")
                    } else {
                        self.string_of(lib, &name)
                    };
                    let ctor = if name.is_empty() {
                        object.clone()
                    } else {
                        self.get_member(lib, &object, &name)
                    };
                    self.construct(lib, frame, &ctor, args, &name)?
                };
                self.push(frame, v)?;
            }
            Op::InstanceOf => {
                let ctor = Self::pop(frame, at)?;
                let object = Self::pop(frame, at)?;
                let proto = self.get_member(lib, &ctor, "prototype").as_object();
                let mut current = self.proto_of(&object);
                let mut found = false;
                let mut hops = 0u32;
                while let Some(p) = current {
                    hops = hops.saturating_add(1);
                    if hops > 256 {
                        break;
                    }
                    if Some(p) == proto {
                        found = true;
                        break;
                    }
                    current = self.heap.objects.get(p).and_then(|o| o.proto);
                }
                self.push(frame, Value::Bool(found))?;
            }

            other => {
                return Err(Halt::Unimplemented {
                    offset: at,
                    opcode: other.opcode(),
                })
            }
        }
        Ok(Step::Next)
    }

    fn deliver(&mut self, lib: &Library, frame: &mut Frame, op: &Op, v: Value, at: u32) -> R<()> {
        match op {
            Op::EaCallFunctionPop
            | Op::EaCallMethodPop
            | Op::EaCallNamedFunctionPop(_)
            | Op::EaCallNamedMethodPop(_) => Ok(()),
            Op::EaCallFunction
            | Op::EaCallMethod
            | Op::EaCallNamedFunction(_)
            | Op::EaCallNamedMethod(_) => {
                let name = Self::pop(frame, at)?;
                let name = self.string_of(lib, &name);
                self.set_variable(lib, frame, &name, v)
            }
            _ => self.push(frame, v),
        }
    }

    pub(crate) fn call_named(
        &mut self,
        lib: &Library,
        frame: &Frame,
        name: &str,
        args: Vec<Value>,
    ) -> R<Value> {
        let (callee, found_on) = self.get_variable(lib, frame, name);
        let this = match found_on {
            Some(o) => Value::Object(o),
            None => self
                .target_or_root(frame)
                .and_then(|t| self.script_object(t))
                .map_or(Value::Undefined, Value::Object),
        };
        self.call_value(lib, frame, &callee, this, args, name)
    }

    fn call_method(
        &mut self,
        lib: &Library,
        frame: &Frame,
        object: &Value,
        name: &Value,
        args: Vec<Value>,
    ) -> R<Value> {
        if matches!(object, Value::Undefined | Value::Null) {
            return Ok(Value::Undefined);
        }
        let name = if *name == Value::Undefined {
            Arc::from("")
        } else {
            self.string_of(lib, name)
        };
        if name.is_empty() {
            return self.call_value(lib, frame, object, Value::Undefined, args, "<blank>");
        }
        let callee = self.get_member(lib, object, &name);
        self.call_value(lib, frame, &callee, object.clone(), args, &name)
    }

    fn set_target(&mut self, lib: &Library, frame: &mut Frame, target: &Value) -> R<()> {
        let base = frame.base_clip;
        let empty = match target {
            Value::Undefined => true,
            Value::String(s) => s.is_empty(),
            _ => false,
        };
        frame.target = if empty {
            base
        } else {
            base.and_then(|b| self.resolve_target_display(lib, frame, b, target, false))
        };
        let clip_obj = self
            .target_or_root(frame)
            .and_then(|t| self.script_object(t))
            .unwrap_or(self.heap.global);
        frame.scope = self.replace_target_scope(frame.scope, clip_obj, 0)?;
        Ok(())
    }

    fn replace_target_scope(&mut self, scope: ScopeId, clip: ObjectId, depth: u32) -> R<ScopeId> {
        let Some(s) = self.heap.scopes.get(scope).cloned() else {
            return Ok(scope);
        };
        if depth > 256 {
            return Ok(scope);
        }
        match s.class {
            ScopeClass::Target => self.new_scope(s.parent, ScopeClass::Target, clip),
            ScopeClass::Global => Ok(scope),
            ScopeClass::Local => {
                let parent = match s.parent {
                    Some(p) => Some(self.replace_target_scope(p, clip, depth.saturating_add(1))?),
                    None => None,
                };
                self.new_scope(parent, ScopeClass::Local, s.values)
            }
        }
    }

    fn get_url(&mut self, url: &str, target: &str) {
        let is_fs = url
            .get(..10)
            .is_some_and(|p| p.eq_ignore_ascii_case("FSCommand:"));
        if is_fs {
            self.dispatch_outward(Outward::FsCommand {
                command: url.get(10..).unwrap_or("").to_owned(),
                args: target.to_owned(),
            });
        } else {
            self.request_load(target, Some(url));
            self.push_outward(Outward::GetUrl {
                url: url.to_owned(),
                target: target.to_owned(),
            });
        }
    }

    fn enumerate_keys(&self, id: ObjectId) -> Vec<Arc<str>> {
        let mut keys: Vec<Arc<str>> = Vec::new();
        let mut chain = Vec::new();
        let mut current = Some(id);
        while let Some(c) = current {
            if chain.len() > 256 {
                break;
            }
            chain.push(c);
            current = self.heap.objects.get(c).and_then(|o| o.proto);
        }
        for c in chain.iter().rev() {
            if let Some(o) = self.heap.objects.get(*c) {
                for name in o.props.enumerable_names() {
                    if !keys.iter().any(|k| k.eq_ignore_ascii_case(name)) {
                        keys.push(Arc::clone(name));
                    }
                }
            }
        }
        if let Some(inst) = self.heap.objects.get(id).and_then(Object::display) {
            if let Some(i) = self.arena.get(inst) {
                let children: Vec<InstanceId> = i.children.values().rev().copied().collect();
                for c in children {
                    if let Some(name) = self.arena.get(c).and_then(|c| c.name.clone()) {
                        keys.push(Arc::from(name.as_str()));
                    }
                }
            }
        }
        keys
    }
}
