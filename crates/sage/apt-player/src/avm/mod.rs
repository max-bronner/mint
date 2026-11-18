pub(crate) mod display;
pub(crate) mod heap;
pub(crate) mod host;
pub(crate) mod interp;
pub(crate) mod natives;
pub(crate) mod object;
pub(crate) mod value;

use std::collections::BTreeMap;

pub use heap::Scope;
pub use object::Object;
pub use value::{ObjectId, Value};

pub(crate) fn bump<K: Ord>(map: &mut BTreeMap<K, u64>, key: K) {
    let n = map.entry(key).or_insert(0);
    *n = n.saturating_add(1);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmLimits {
    pub max_instructions: u64,
    pub max_stack: usize,
    pub max_call_depth: u32,
    pub max_string_len: usize,
    pub max_objects: usize,
    pub max_scopes: usize,
    pub max_actions_per_drain: usize,
    pub max_outward: usize,
}

impl Default for VmLimits {
    fn default() -> Self {
        Self {
            max_instructions: 100_000,
            max_stack: 1024,
            max_call_depth: 64,
            max_string_len: 65_536,
            max_objects: 262_144,
            max_scopes: 262_144,
            max_actions_per_drain: 65_536,
            max_outward: 4096,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Halt {
    Unimplemented { offset: u32, opcode: u8 },
    Budget { offset: u32 },
    StackUnderflow { offset: u32 },
    BadJump { offset: u32 },
    BadIndex { offset: u32 },
    Limit { what: &'static str },
    Decode,
}

pub trait SyncHost {
    fn command(&mut self, outward: &Outward) -> bool;

    fn get_extern(&mut self, name: &str) -> Option<Value>;

    fn set_extern(&mut self, _name: &str, _value: &Value) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outward {
    FsCommand { command: String, args: String },
    GetUrl { url: String, target: String },
    GameCode { name: String, args: Vec<String> },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptStats {
    pub blocks_run: u64,
    pub blocks_completed: u64,
    pub halts: BTreeMap<Halt, u64>,
    pub instructions: u64,
    pub this_undefined: u64,
    pub this_reads: u64,
    pub unresolved_calls: BTreeMap<String, u64>,
    pub bytecode_calls: BTreeMap<String, u64>,
    pub native_calls: BTreeMap<String, u64>,
    pub traces: u64,
    pub residue_blocks: u64,
    pub discarded: u64,
}
