use crate::container::{read_chunks, NameTable, RawChunk};
use crate::reader::Reader;
use crate::{Error, Result};

const MAX_SCRIPT_DEPTH: usize = 64;

const COORD3D_PARAM_TYPE: i32 = 16;

const NAME_KEY_ASCII_STRING_TAG: u32 = 3;

#[derive(Debug, Clone, PartialEq)]
pub enum Parameter {
    Coord3D {
        x: f32,
        y: f32,
        z: f32,
    },
    Value {
        param_type: i32,
        int_value: i32,
        real_value: f32,
        string_value: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    pub condition_type: i32,
    pub name: String,
    pub params: Vec<Parameter>,
    pub enabled: bool,
    pub inverted: bool,
}

impl Condition {
    pub fn negates(&self) -> bool {
        self.inverted
            && !matches!(
                self.name.as_str(),
                "CONDITION_FALSE" | "COUNTER" | "FLAG" | "CONDITION_TRUE" | "TIMER_EXPIRED"
            )
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct OrCondition {
    pub conditions: Vec<Condition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScriptAction {
    pub action_type: i32,
    pub name: String,
    pub params: Vec<Parameter>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    pub name: String,
    pub comment: String,
    pub condition_comment: String,
    pub action_comment: String,
    pub active: bool,
    pub one_shot: bool,
    pub easy: bool,
    pub normal: bool,
    pub hard: bool,
    pub subroutine: bool,
    pub delay_evaluation_seconds: i32,
    pub or_conditions: Vec<OrCondition>,
    pub actions: Vec<ScriptAction>,
    pub actions_false: Vec<ScriptAction>,
    pub sequence: ScriptSequence,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptSequence {
    pub run_sequentially: bool,
    pub loop_flag: bool,
    pub loop_count: i32,
    pub target_is_unit: bool,
    pub target: String,
    pub phases: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScriptNode {
    Script(Script),
    Group(ScriptGroup),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScriptGroup {
    pub name: String,
    pub active: bool,
    pub is_subroutine: bool,
    pub children: Vec<ScriptNode>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScriptTree {
    pub player_slots: Vec<Vec<ScriptNode>>,
}

fn resolve_name_key(names: &NameTable, packed: u32, offset: usize) -> Result<String> {
    let tag = packed & 0xff;
    if tag != NAME_KEY_ASCII_STRING_TAG {
        return Err(Error::Malformed {
            name: "NameKey",
            reason: "type tag is not DICT_ASCIISTRING (3)",
        });
    }
    let index = packed >> 8;
    names.resolve(index, offset)
}

fn read_parameter(r: &mut Reader<'_>) -> Result<Parameter> {
    let param_type = r.i32()?;
    if param_type == COORD3D_PARAM_TYPE {
        let x = r.f32()?;
        let y = r.f32()?;
        let z = r.f32()?;
        Ok(Parameter::Coord3D { x, y, z })
    } else {
        let int_value = r.i32()?;
        let real_value = r.f32()?;
        let string_value = r.ascii_string()?;
        Ok(Parameter::Value {
            param_type,
            int_value,
            real_value,
            string_value,
        })
    }
}

fn read_parameters(r: &mut Reader<'_>, chunk_name: &'static str) -> Result<Vec<Parameter>> {
    let offset = r.position();
    let raw_count = r.i32()?;
    let count = u32::try_from(raw_count).map_err(|_| Error::Malformed {
        name: chunk_name,
        reason: "numParms is negative",
    })?;
    let available = r.remaining();
    let min_bytes = usize::try_from(count).ok().and_then(|c| c.checked_mul(4));
    if min_bytes.is_none_or(|needed| needed > available) {
        return Err(Error::CountExceedsPayload {
            chunk: chunk_name,
            offset,
            count,
            item_size: 4,
            available,
        });
    }
    let mut params = Vec::new();
    for _ in 0..count {
        params.push(read_parameter(r)?);
    }
    Ok(params)
}

fn known_trailer(table: &[(u16, usize)], chunk: &'static str, version: u16) -> Result<usize> {
    table
        .iter()
        .find(|(v, _)| *v == version)
        .map(|(_, trailer)| *trailer)
        .ok_or(Error::UnknownScriptVersion { chunk, version })
}

const CONDITION_VERSIONS: &[(u16, usize)] = &[(4, 0), (5, 8), (6, 8)];
const SCRIPT_ACTION_VERSIONS: &[(u16, usize)] = &[(2, 0), (3, 4)];
const OR_CONDITION_VERSIONS: &[u16] = &[1, 2];
const SCRIPT_VERSIONS: &[u16] = &[2, 3, 4];
const SCRIPT_GROUP_VERSIONS: &[u16] = &[2, 3];
const SCRIPT_LIST_VERSIONS: &[u16] = &[1];
const PLAYER_SCRIPTS_LIST_VERSIONS: &[u16] = &[1, 5, 6];

fn read_condition(body: &[u8], chunk: &RawChunk, names: &NameTable) -> Result<Condition> {
    let trailer = known_trailer(CONDITION_VERSIONS, "Condition", chunk.version)?;
    let payload = chunk.payload(body)?;
    let mut r = Reader::new_at(payload, chunk.body);
    let condition_type = r.i32()?;
    let key_offset = r.position();
    let packed = r.u32()?;
    let name = resolve_name_key(names, packed, key_offset)?;
    let params = read_parameters(&mut r, "Condition")?;
    let remaining = r.remaining();
    if remaining != trailer {
        return Err(Error::TrailingBytes {
            name: "Condition",
            offset: r.position(),
            remaining,
        });
    }
    let (enabled, inverted) = if trailer == 0 {
        (true, false)
    } else {
        let enabled = r.u32()? != 0;
        (enabled, r.u32()? != 0)
    };
    Ok(Condition {
        condition_type,
        name,
        params,
        enabled,
        inverted,
    })
}

fn read_or_condition(body: &[u8], chunk: &RawChunk, names: &NameTable) -> Result<OrCondition> {
    if !OR_CONDITION_VERSIONS.contains(&chunk.version) {
        return Err(Error::UnknownScriptVersion {
            chunk: "OrCondition",
            version: chunk.version,
        });
    }
    let end = chunk.end()?;
    let children = read_chunks(body, chunk.body, end, names)?;
    let mut conditions = Vec::with_capacity(children.len());
    for child in &children {
        if child.name != "Condition" {
            return Err(Error::UnexpectedChild {
                parent: "OrCondition",
                found: child.name.clone(),
            });
        }
        conditions.push(read_condition(body, child, names)?);
    }
    Ok(OrCondition { conditions })
}

fn read_script_action(
    body: &[u8],
    chunk: &RawChunk,
    names: &NameTable,
    label: &'static str,
) -> Result<ScriptAction> {
    let trailer = known_trailer(SCRIPT_ACTION_VERSIONS, label, chunk.version)?;
    let payload = chunk.payload(body)?;
    let mut r = Reader::new_at(payload, chunk.body);
    let action_type = r.i32()?;
    let key_offset = r.position();
    let packed = r.u32()?;
    let name = resolve_name_key(names, packed, key_offset)?;
    let params = read_parameters(&mut r, label)?;
    let remaining = r.remaining();
    if remaining != trailer {
        return Err(Error::TrailingBytes {
            name: label,
            offset: r.position(),
            remaining,
        });
    }
    let enabled = trailer == 0 || r.u32()? != 0;
    Ok(ScriptAction {
        action_type,
        name,
        params,
        enabled,
    })
}

fn read_script_children(
    body: &[u8],
    start: usize,
    end: usize,
    names: &NameTable,
) -> Result<(Vec<OrCondition>, Vec<ScriptAction>, Vec<ScriptAction>)> {
    let children = read_chunks(body, start, end, names)?;
    let mut or_conditions = Vec::new();
    let mut actions = Vec::new();
    let mut actions_false = Vec::new();
    for child in &children {
        match child.name.as_str() {
            "OrCondition" => or_conditions.push(read_or_condition(body, child, names)?),
            "ScriptAction" => actions.push(read_script_action(body, child, names, "ScriptAction")?),
            "ScriptActionFalse" => {
                actions_false.push(read_script_action(body, child, names, "ScriptActionFalse")?);
            }
            other => {
                return Err(Error::UnexpectedChild {
                    parent: "Script",
                    found: other.to_owned(),
                })
            }
        }
    }
    Ok((or_conditions, actions, actions_false))
}

fn read_script(body: &[u8], chunk: &RawChunk, names: &NameTable, depth: usize) -> Result<Script> {
    if depth > MAX_SCRIPT_DEPTH {
        return Err(Error::Malformed {
            name: "Script",
            reason: "nesting deeper than this reader's depth cap",
        });
    }
    if !SCRIPT_VERSIONS.contains(&chunk.version) {
        return Err(Error::UnknownScriptVersion {
            chunk: "Script",
            version: chunk.version,
        });
    }
    let payload = chunk.payload(body)?;
    let mut r = Reader::new_at(payload, chunk.body);
    let name = r.ascii_string()?;
    let comment = r.ascii_string()?;
    let condition_comment = r.ascii_string()?;
    let action_comment = r.ascii_string()?;
    let active = r.u8()? != 0;
    let one_shot = r.u8()? != 0;
    let easy = r.u8()? != 0;
    let normal = r.u8()? != 0;
    let hard = r.u8()? != 0;
    let subroutine = r.u8()? != 0;
    let delay_evaluation_seconds = r.i32()?;
    let mut sequence = ScriptSequence::default();
    if chunk.version >= 3 {
        sequence.run_sequentially = r.u8()? != 0;
        sequence.loop_flag = r.u8()? != 0;
        sequence.loop_count = r.i32()?;
        sequence.target_is_unit = r.u8()? != 0;
        sequence.target = r.ascii_string()?;
        if chunk.version >= 4 {
            sequence.phases = r.ascii_string()?;
        }
    }
    let children_start = r.position();
    let end = chunk.end()?;
    let (or_conditions, actions, actions_false) =
        read_script_children(body, children_start, end, names)?;
    Ok(Script {
        name,
        comment,
        condition_comment,
        action_comment,
        active,
        one_shot,
        easy,
        normal,
        hard,
        subroutine,
        delay_evaluation_seconds,
        or_conditions,
        actions,
        actions_false,
        sequence,
    })
}

fn read_script_group(
    body: &[u8],
    chunk: &RawChunk,
    names: &NameTable,
    depth: usize,
) -> Result<ScriptGroup> {
    if depth > MAX_SCRIPT_DEPTH {
        return Err(Error::Malformed {
            name: "ScriptGroup",
            reason: "nesting deeper than this reader's depth cap",
        });
    }
    if !SCRIPT_GROUP_VERSIONS.contains(&chunk.version) {
        return Err(Error::UnknownScriptVersion {
            chunk: "ScriptGroup",
            version: chunk.version,
        });
    }
    let payload = chunk.payload(body)?;
    let mut r = Reader::new_at(payload, chunk.body);
    let name = r.ascii_string()?;
    let active = r.u8()? != 0;
    let is_subroutine = r.u8()? != 0;
    let children_start = r.position();
    let end = chunk.end()?;
    let children = read_chunks(body, children_start, end, names)?;
    let mut nodes = Vec::with_capacity(children.len());
    for child in &children {
        match child.name.as_str() {
            "Script" => nodes.push(ScriptNode::Script(read_script(
                body,
                child,
                names,
                depth.saturating_add(1),
            )?)),
            "ScriptGroup" => nodes.push(ScriptNode::Group(read_script_group(
                body,
                child,
                names,
                depth.saturating_add(1),
            )?)),
            other => {
                return Err(Error::UnexpectedChild {
                    parent: "ScriptGroup",
                    found: other.to_owned(),
                })
            }
        }
    }
    Ok(ScriptGroup {
        name,
        active,
        is_subroutine,
        children: nodes,
    })
}

fn read_script_list(
    body: &[u8],
    chunk: &RawChunk,
    names: &NameTable,
    depth: usize,
) -> Result<Vec<ScriptNode>> {
    if !SCRIPT_LIST_VERSIONS.contains(&chunk.version) {
        return Err(Error::UnknownScriptVersion {
            chunk: "ScriptList",
            version: chunk.version,
        });
    }
    let end = chunk.end()?;
    let children = read_chunks(body, chunk.body, end, names)?;
    let mut nodes = Vec::with_capacity(children.len());
    for child in &children {
        match child.name.as_str() {
            "Script" => nodes.push(ScriptNode::Script(read_script(
                body,
                child,
                names,
                depth.saturating_add(1),
            )?)),
            "ScriptGroup" => nodes.push(ScriptNode::Group(read_script_group(
                body,
                child,
                names,
                depth.saturating_add(1),
            )?)),
            other => {
                return Err(Error::UnexpectedChild {
                    parent: "ScriptList",
                    found: other.to_owned(),
                })
            }
        }
    }
    Ok(nodes)
}

pub(crate) fn read_player_scripts_list(
    body: &[u8],
    chunk: &RawChunk,
    names: &NameTable,
) -> Result<ScriptTree> {
    if !PLAYER_SCRIPTS_LIST_VERSIONS.contains(&chunk.version) {
        return Err(Error::UnknownScriptVersion {
            chunk: "PlayerScriptsList",
            version: chunk.version,
        });
    }
    let end = chunk.end()?;
    let lists = read_chunks(body, chunk.body, end, names)?;
    let mut player_slots = Vec::with_capacity(lists.len());
    for list in &lists {
        if list.name != "ScriptList" {
            return Err(Error::UnexpectedChild {
                parent: "PlayerScriptsList",
                found: list.name.clone(),
            });
        }
        player_slots.push(read_script_list(body, list, names, 1)?);
    }
    Ok(ScriptTree { player_slots })
}

fn collect_initial_values(
    nodes: &[ScriptNode],
    action_names: &[&str],
    out: &mut std::collections::BTreeMap<String, i32>,
) {
    for node in nodes {
        match node {
            ScriptNode::Group(group) => collect_initial_values(&group.children, action_names, out),
            ScriptNode::Script(script) => {
                for action in &script.actions {
                    if !action_names.iter().any(|name| *name == action.name) {
                        continue;
                    }
                    let Some(Parameter::Value { string_value, .. }) = action.params.first() else {
                        continue;
                    };
                    let Some(Parameter::Value { int_value, .. }) = action.params.get(1) else {
                        continue;
                    };
                    if string_value.is_empty() {
                        continue;
                    }
                    out.insert(string_value.clone(), *int_value);
                }
            }
        }
    }
}

#[must_use]
pub fn initial_int_values(
    tree: &ScriptTree,
    action_names: &[&str],
) -> std::collections::BTreeMap<String, i32> {
    let mut out = std::collections::BTreeMap::new();
    for slot in &tree.player_slots {
        collect_initial_values(slot, action_names, &mut out);
    }
    out
}

#[cfg(test)]
mod tests;
