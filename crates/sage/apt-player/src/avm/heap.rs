use std::collections::BTreeMap;

use crate::arena::{Arena, Handle};

use super::object::{Native, Object};
use super::value::{ObjectId, Value};

pub(crate) type ScopeId = Handle<Scope>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScopeClass {
    Global,
    Target,
    Local,
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub(crate) parent: Option<ScopeId>,
    pub(crate) class: ScopeClass,
    pub(crate) values: ObjectId,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Protos {
    pub(crate) object: ObjectId,
    pub(crate) function: ObjectId,
    pub(crate) array: ObjectId,
    pub(crate) string: ObjectId,
    pub(crate) number: ObjectId,
    pub(crate) boolean: ObjectId,
    pub(crate) movie_clip: ObjectId,
    pub(crate) button: ObjectId,
    pub(crate) text_field: ObjectId,
    pub(crate) color: ObjectId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HeapFull {
    pub(crate) what: &'static str,
    pub(crate) cap: usize,
}

pub(crate) struct Heap {
    pub(crate) objects: Arena<Object>,
    pub(crate) scopes: Arena<Scope>,
    pub(crate) global: ObjectId,
    pub(crate) global_scope: ScopeId,
    pub(crate) protos: Protos,
    pub(crate) registers: Vec<Value>,
    pub(crate) pins: BTreeMap<ObjectId, u32>,
    pub(crate) debt: usize,
    pub(crate) collections: u64,
    pub(crate) reclaimed: u64,
}

impl Heap {
    pub(crate) fn new(max_objects: usize, max_scopes: usize) -> Result<Self, HeapFull> {
        let mut objects = Arena::with_cap(max_objects);
        let full = |_| HeapFull {
            what: "script objects",
            cap: max_objects,
        };
        let object = objects
            .insert(Object::new(None, Native::Plain))
            .map_err(full)?;
        let proto = |objects: &mut Arena<Object>| {
            objects
                .insert(Object::new(Some(object), Native::Plain))
                .map_err(full)
        };
        let function = proto(&mut objects)?;
        let array = proto(&mut objects)?;
        let string = proto(&mut objects)?;
        let number = proto(&mut objects)?;
        let boolean = proto(&mut objects)?;
        let movie_clip = proto(&mut objects)?;
        let button = proto(&mut objects)?;
        let text_field = proto(&mut objects)?;
        let color = proto(&mut objects)?;
        let global = proto(&mut objects)?;
        let mut scopes = Arena::with_cap(max_scopes);
        let global_scope = scopes
            .insert(Scope {
                parent: None,
                class: ScopeClass::Global,
                values: global,
            })
            .map_err(|_| HeapFull {
                what: "scopes",
                cap: max_scopes,
            })?;
        Ok(Self {
            objects,
            scopes,
            global,
            global_scope,
            protos: Protos {
                object,
                function,
                array,
                string,
                number,
                boolean,
                movie_clip,
                button,
                text_field,
                color,
            },
            registers: Vec::new(),
            pins: BTreeMap::new(),
            debt: 0,
            collections: 0,
            reclaimed: 0,
        })
    }

    pub(crate) fn alloc(&mut self, object: Object) -> Result<ObjectId, HeapFull> {
        let cap = self.objects.len();
        let id = self.objects.insert(object).map_err(|full| HeapFull {
            what: "script objects",
            cap: full.cap.max(cap),
        })?;
        self.debt = self.debt.saturating_add(1);
        Ok(id)
    }

    pub(crate) fn alloc_scope(&mut self, scope: Scope) -> Result<ScopeId, HeapFull> {
        let id = self.scopes.insert(scope).map_err(|full| HeapFull {
            what: "scopes",
            cap: full.cap,
        })?;
        self.debt = self.debt.saturating_add(1);
        Ok(id)
    }

    pub(crate) fn wants_collection(&self) -> bool {
        let live = self.objects.len().saturating_add(self.scopes.len());
        self.debt >= (live / 2).max(1024)
    }

    pub(crate) fn collect(&mut self, roots: impl IntoIterator<Item = ObjectId>) -> usize {
        let mut objects_marked = vec![false; self.objects.slots()];
        let mut scopes_marked = vec![false; self.scopes.slots()];
        let mut objects_todo: Vec<ObjectId> = Vec::new();
        let mut scopes_todo: Vec<ScopeId> = vec![self.global_scope];
        objects_todo.push(self.global);
        let p = self.protos;
        objects_todo.extend([
            p.object,
            p.function,
            p.array,
            p.string,
            p.number,
            p.boolean,
            p.movie_clip,
            p.button,
            p.text_field,
            p.color,
        ]);
        objects_todo.extend(self.pins.keys().copied());
        objects_todo.extend(self.registers.iter().filter_map(Value::as_object));
        objects_todo.extend(roots);

        loop {
            if let Some(id) = objects_todo.pop() {
                let Some(slot) = objects_marked.get_mut(id.index() as usize) else {
                    continue;
                };
                let Some(object) = self.objects.get(id) else {
                    continue;
                };
                if *slot {
                    continue;
                }
                *slot = true;
                objects_todo.extend(object.props.values().filter_map(Value::as_object));
                objects_todo.extend(object.proto);
                if let Native::Function(f) = &object.native {
                    scopes_todo.push(f.scope);
                    objects_todo.extend(f.pool.iter().filter_map(Value::as_object));
                }
                continue;
            }
            if let Some(id) = scopes_todo.pop() {
                let Some(slot) = scopes_marked.get_mut(id.index() as usize) else {
                    continue;
                };
                let Some(scope) = self.scopes.get(id) else {
                    continue;
                };
                if *slot {
                    continue;
                }
                *slot = true;
                objects_todo.push(scope.values);
                scopes_todo.extend(scope.parent);
                continue;
            }
            break;
        }

        let dead_objects: Vec<ObjectId> = self
            .objects
            .iter()
            .filter(|(h, _)| {
                !objects_marked
                    .get(h.index() as usize)
                    .copied()
                    .unwrap_or(false)
            })
            .map(|(h, _)| h)
            .collect();
        let dead_scopes: Vec<ScopeId> = self
            .scopes
            .iter()
            .filter(|(h, _)| {
                !scopes_marked
                    .get(h.index() as usize)
                    .copied()
                    .unwrap_or(false)
            })
            .map(|(h, _)| h)
            .collect();
        let freed = dead_objects.len().saturating_add(dead_scopes.len());
        for h in dead_objects {
            self.objects.remove(h);
        }
        for h in dead_scopes {
            self.scopes.remove(h);
        }
        self.debt = 0;
        self.collections = self.collections.saturating_add(1);
        self.reclaimed = self.reclaimed.saturating_add(freed as u64);
        freed
    }
}
