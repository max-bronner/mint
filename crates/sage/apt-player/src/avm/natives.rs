use std::sync::Arc;

use crate::geom::ColorTransform;
use crate::instance::{InstanceId, InstanceKind};
use crate::library::{Library, MovieId};
use crate::timeline::{Stage, AVM_DEPTH_BIAS};

use super::interp::Frame;
use super::object::{Native, Object};
use super::value::{format_number, ObjectId, Value};
use super::{Halt, Outward};

pub(crate) type NativeFn =
    fn(&mut Stage, &Library, &Frame, Value, Vec<Value>) -> Result<Value, Halt>;

type R = Result<Value, Halt>;

const AVM_MAX_DEPTH: i32 = 2_130_706_428;
const AVM_MAX_REMOVE_DEPTH: i32 = 2_130_706_416;

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Undefined)
}

fn this_clip(stage: &Stage, this: &Value) -> Option<InstanceId> {
    let inst = stage.heap.objects.get(this.as_object()?)?.display()?;
    stage.arena.contains(inst).then_some(inst)
}

fn method(stage: &mut Stage, object: ObjectId, name: &str, f: NativeFn) {
    let fp = stage.heap.protos.function;
    if let Ok(id) = stage.alloc(Some(fp), Native::Builtin(f)) {
        if let Some(o) = stage.heap.objects.get_mut(object) {
            o.props.define(name, Value::Object(id), true);
        }
    }
}

fn constructor(stage: &mut Stage, name: &str, proto: ObjectId, f: NativeFn) -> Option<ObjectId> {
    let fp = stage.heap.protos.function;
    let id = stage.alloc(Some(fp), Native::Builtin(f)).ok()?;
    let global = stage.heap.global;
    if let Some(o) = stage.heap.objects.get_mut(id) {
        o.props.define("prototype", Value::Object(proto), true);
    }
    if let Some(o) = stage.heap.objects.get_mut(proto) {
        o.props.define("constructor", Value::Object(id), true);
    }
    if let Some(o) = stage.heap.objects.get_mut(global) {
        o.props.define(name, Value::Object(id), true);
    }
    Some(id)
}

pub(crate) fn install(stage: &mut Stage) {
    let p = stage.heap.protos;
    let global = stage.heap.global;

    constructor(stage, "Object", p.object, object_ctor);
    constructor(stage, "Function", p.function, noop);
    constructor(stage, "Array", p.array, array_ctor);
    constructor(stage, "String", p.string, string_ctor);
    constructor(stage, "Number", p.number, number_ctor);
    constructor(stage, "Boolean", p.boolean, boolean_ctor);
    constructor(stage, "MovieClip", p.movie_clip, noop);
    constructor(stage, "Button", p.button, noop);
    constructor(stage, "TextField", p.text_field, noop);
    constructor(stage, "Color", p.color, color_ctor);

    method(stage, p.object, "toString", object_to_string);
    method(stage, p.object, "valueOf", identity);
    method(stage, p.object, "hasOwnProperty", has_own_property);
    method(stage, p.function, "call", function_call);
    method(stage, p.function, "apply", function_apply);

    for (name, f) in [
        ("gotoAndPlay", mc_goto_and_play as NativeFn),
        ("gotoAndStop", mc_goto_and_stop),
        ("play", mc_play),
        ("stop", mc_stop),
        ("nextFrame", mc_next_frame),
        ("prevFrame", mc_prev_frame),
        ("getDepth", get_depth),
        ("swapDepths", mc_swap_depths),
        ("attachMovie", mc_attach_movie),
        ("duplicateMovieClip", mc_duplicate),
        ("removeMovieClip", mc_remove),
        ("getBytesLoaded", mc_bytes),
        ("getBytesTotal", mc_bytes),
        ("getInstanceAtDepth", mc_instance_at_depth),
        ("getNextHighestDepth", mc_next_highest_depth),
        ("localToGlobal", mc_local_to_global),
        ("globalToLocal", mc_global_to_local),
        ("hitTest", false_fn),
        ("createEmptyMovieClip", mc_create_empty),
        ("startDrag", noop),
        ("stopDrag", noop),
        ("loadMovie", mc_load_movie),
        ("unloadMovie", mc_unload_movie),
        ("getURL", mc_get_url),
    ] {
        method(stage, p.movie_clip, name, f);
    }
    method(stage, p.button, "getDepth", get_depth);
    method(stage, p.text_field, "getDepth", get_depth);

    for (name, f) in [
        ("substring", str_substring as NativeFn),
        ("substr", str_substr),
        ("slice", str_slice),
        ("indexOf", str_index_of),
        ("lastIndexOf", str_last_index_of),
        ("charAt", str_char_at),
        ("charCodeAt", str_char_code_at),
        ("toUpperCase", str_upper),
        ("toLowerCase", str_lower),
        ("split", str_split),
        ("concat", str_concat),
        ("toString", value_string),
        ("valueOf", value_string),
    ] {
        method(stage, p.string, name, f);
    }
    for (name, f) in [
        ("push", arr_push as NativeFn),
        ("pop", arr_pop),
        ("shift", arr_shift),
        ("unshift", arr_unshift),
        ("join", arr_join),
        ("toString", arr_to_string),
        ("concat", arr_concat),
        ("slice", arr_slice),
        ("splice", arr_splice),
        ("reverse", arr_reverse),
        ("sort", arr_sort),
    ] {
        method(stage, p.array, name, f);
    }
    method(stage, p.number, "toString", value_string);
    method(stage, p.boolean, "toString", value_string);
    for (name, f) in [
        ("setRGB", color_set_rgb as NativeFn),
        ("getRGB", color_get_rgb),
        ("setTransform", color_set_transform),
        ("getTransform", color_get_transform),
    ] {
        method(stage, p.color, name, f);
    }

    for (name, f) in [
        ("parseInt", parse_int as NativeFn),
        ("parseFloat", parse_float),
        ("isNaN", is_nan),
        ("isFinite", is_finite),
        ("escape", identity_arg),
        ("unescape", identity_arg),
        ("setInterval", set_interval),
        ("clearInterval", clear_interval),
    ] {
        method(stage, global, name, f);
    }

    if let Ok(object) = stage.new_object() {
        if let Some(o) = stage.heap.objects.get_mut(global) {
            o.props.define("extern", Value::Object(object), true);
        }
        stage.extern_object = Some(object);
    }

    let Ok(math) = stage.new_object() else {
        return;
    };
    if let Some(o) = stage.heap.objects.get_mut(global) {
        o.props.define("Math", Value::Object(math), true);
    }
    if let Some(o) = stage.heap.objects.get_mut(math) {
        o.props
            .define("PI", Value::Number(std::f64::consts::PI), true);
        o.props
            .define("E", Value::Number(std::f64::consts::E), true);
    }
    for (name, f) in [
        ("abs", math_abs as NativeFn),
        ("floor", math_floor),
        ("ceil", math_ceil),
        ("round", math_round),
        ("min", math_min),
        ("max", math_max),
        ("random", math_random),
        ("sqrt", math_sqrt),
        ("pow", math_pow),
        ("sin", math_sin),
        ("cos", math_cos),
        ("atan2", math_atan2),
    ] {
        method(stage, math, name, f);
    }
    if let Ok(from_char) = stage.alloc(
        Some(stage.heap.protos.function),
        Native::Builtin(str_from_char_code),
    ) {
        let string_ctor = stage
            .heap
            .objects
            .get(global)
            .and_then(|g| g.props.get("String"))
            .and_then(Value::as_object);
        if let Some(o) = string_ctor.and_then(|s| stage.heap.objects.get_mut(s)) {
            o.props
                .define("fromCharCode", Value::Object(from_char), true);
        }
    }
}

fn noop(_: &mut Stage, _: &Library, _: &Frame, _: Value, _: Vec<Value>) -> R {
    Ok(Value::Undefined)
}

fn false_fn(_: &mut Stage, _: &Library, _: &Frame, _: Value, _: Vec<Value>) -> R {
    Ok(Value::Bool(false))
}

fn identity(_: &mut Stage, _: &Library, _: &Frame, this: Value, _: Vec<Value>) -> R {
    Ok(this)
}

fn identity_arg(_: &mut Stage, _: &Library, _: &Frame, _: Value, args: Vec<Value>) -> R {
    Ok(arg(&args, 0))
}

fn object_ctor(stage: &mut Stage, _: &Library, _: &Frame, this: Value, args: Vec<Value>) -> R {
    if let Some(Value::Object(o)) = args.first() {
        return Ok(Value::Object(*o));
    }
    match this {
        Value::Object(_) => Ok(this),
        _ => Ok(Value::Object(stage.new_object()?)),
    }
}

fn object_to_string(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, _: Vec<Value>) -> R {
    let is_display = this
        .as_object()
        .and_then(|o| stage.heap.objects.get(o))
        .and_then(Object::display)
        .is_some();
    if is_display {
        return Ok(Value::String(stage.string_of(lib, &this)));
    }
    Ok(Value::str("[object Object]"))
}

fn has_own_property(
    stage: &mut Stage,
    lib: &Library,
    _: &Frame,
    this: Value,
    args: Vec<Value>,
) -> R {
    let name = stage.string_of(lib, &arg(&args, 0));
    Ok(Value::Bool(
        this.as_object()
            .and_then(|o| stage.heap.objects.get(o))
            .is_some_and(|o| o.props.contains(&name)),
    ))
}

fn function_call(
    stage: &mut Stage,
    lib: &Library,
    caller: &Frame,
    this: Value,
    args: Vec<Value>,
) -> R {
    let mut rest = args.into_iter();
    let new_this = rest.next().unwrap_or(Value::Undefined);
    stage.call_value(lib, caller, &this, new_this, rest.collect(), "call")
}

fn function_apply(
    stage: &mut Stage,
    lib: &Library,
    caller: &Frame,
    this: Value,
    args: Vec<Value>,
) -> R {
    let new_this = arg(&args, 0);
    let list = match arg(&args, 1) {
        Value::Object(a) => {
            let len = stage.array_len(a);
            (0..len)
                .map(|i| stage.get_member(lib, &Value::Object(a), &i.to_string()))
                .collect()
        }
        _ => Vec::new(),
    };
    stage.call_value(lib, caller, &this, new_this, list, "apply")
}

fn string_ctor(stage: &mut Stage, lib: &Library, _: &Frame, _: Value, args: Vec<Value>) -> R {
    let v = args
        .first()
        .map_or_else(|| Arc::from(""), |a| stage.string_of(lib, a));
    Ok(Value::String(v))
}

fn number_ctor(_: &mut Stage, _: &Library, _: &Frame, _: Value, args: Vec<Value>) -> R {
    Ok(Value::Number(args.first().map_or(0.0, Value::to_number)))
}

fn boolean_ctor(_: &mut Stage, _: &Library, _: &Frame, _: Value, args: Vec<Value>) -> R {
    Ok(Value::Bool(args.first().is_some_and(Value::to_bool)))
}

fn value_string(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, args: Vec<Value>) -> R {
    if let (Value::Number(n), Some(radix)) = (&this, args.first()) {
        let radix = radix.to_i32();
        if (2..=36).contains(&radix) && radix != 10 && n.fract() == 0.0 && n.is_finite() {
            return Ok(Value::str(&to_radix(*n, radix.unsigned_abs())));
        }
    }
    Ok(Value::String(stage.string_of(lib, &this)))
}

fn to_radix(n: f64, radix: u32) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let mut v = n.abs() as u64;
    let mut digits = Vec::new();
    loop {
        let d = u32::try_from(v.checked_rem(u64::from(radix)).unwrap_or(0)).unwrap_or(0);
        digits.push(char::from_digit(d, radix).unwrap_or('0'));
        v = v.checked_div(u64::from(radix)).unwrap_or(0);
        if v == 0 {
            break;
        }
    }
    if n < 0.0 {
        digits.push('-');
    }
    digits.into_iter().rev().collect()
}

fn parse_int(stage: &mut Stage, lib: &Library, _: &Frame, _: Value, args: Vec<Value>) -> R {
    let s = stage.string_of(lib, &arg(&args, 0));
    let t = s.trim();
    let radix = args.get(1).map_or(10, Value::to_i32);
    let (neg, digits) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let (radix, digits) = match digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        Some(r) if radix == 10 || radix == 16 => (16, r),
        _ => (radix, digits),
    };
    let radix = u32::try_from(radix).unwrap_or(10).clamp(2, 36);
    let valid: String = digits.chars().take_while(|c| c.is_digit(radix)).collect();
    if valid.is_empty() {
        return Ok(Value::Number(f64::NAN));
    }
    let v = i64::from_str_radix(&valid, radix).map_or(f64::NAN, |v| {
        #[allow(clippy::cast_precision_loss)]
        let f = v as f64;
        f
    });
    Ok(Value::Number(if neg { -v } else { v }))
}

fn parse_float(stage: &mut Stage, lib: &Library, _: &Frame, _: Value, args: Vec<Value>) -> R {
    let s = stage.string_of(lib, &arg(&args, 0));
    let t = s.trim();
    let mut end = 0;
    for (i, c) in t.char_indices() {
        if c.is_ascii_digit()
            || c == '.'
            || ((c == '-' || c == '+') && i == 0)
            || c == 'e'
            || c == 'E'
        {
            end = i.saturating_add(c.len_utf8());
        } else {
            break;
        }
    }
    let mut prefix = t.get(..end).unwrap_or("");
    while !prefix.is_empty() && prefix.parse::<f64>().is_err() {
        prefix = prefix.get(..prefix.len().saturating_sub(1)).unwrap_or("");
    }
    Ok(Value::Number(prefix.parse::<f64>().unwrap_or(f64::NAN)))
}

fn is_nan(_: &mut Stage, _: &Library, _: &Frame, _: Value, args: Vec<Value>) -> R {
    Ok(Value::Bool(arg(&args, 0).to_number().is_nan()))
}

fn is_finite(_: &mut Stage, _: &Library, _: &Frame, _: Value, args: Vec<Value>) -> R {
    Ok(Value::Bool(arg(&args, 0).to_number().is_finite()))
}

pub(crate) fn goto_value(
    stage: &mut Stage,
    lib: &Library,
    frame: &Frame,
    clip: InstanceId,
    target: &Value,
    stop: bool,
) {
    match target {
        Value::Number(n) if n.fract() == 0.0 => {
            let f = super::value::to_i32(*n);
            if f > 0 {
                stage.goto_frame(lib, clip, u16::try_from(f).unwrap_or(u16::MAX), stop);
            }
        }
        other => {
            let text = stage.string_of(lib, other);
            let (clip, label) = match text.rfind([':', '.']) {
                Some(sep) => {
                    let path = text.get(..sep).unwrap_or("");
                    let label = text.get(sep.saturating_add(1)..).unwrap_or("");
                    let Some(start) = stage.script_object(clip) else {
                        return;
                    };
                    let Some(obj) = stage.resolve_target_path(lib, frame, start, path, true, true)
                    else {
                        return;
                    };
                    let Some(inst) = stage.heap.objects.get(obj).and_then(Object::display) else {
                        return;
                    };
                    (inst, label.to_owned())
                }
                None => (clip, text.to_string()),
            };
            if let Ok(n) = label.parse::<f64>() {
                let f = super::value::to_i32(n);
                if f > 0 {
                    stage.goto_frame(lib, clip, u16::try_from(f).unwrap_or(u16::MAX), stop);
                }
            } else {
                stage.goto_label(lib, clip, &label, stop);
            }
        }
    }
}

fn mc_goto(
    stage: &mut Stage,
    lib: &Library,
    caller: &Frame,
    this: &Value,
    args: &[Value],
    stop: bool,
) -> R {
    if let Some(clip) = this_clip(stage, this) {
        goto_value(stage, lib, caller, clip, &arg(args, 0), stop);
    }
    Ok(Value::Undefined)
}

fn mc_goto_and_play(
    stage: &mut Stage,
    lib: &Library,
    caller: &Frame,
    this: Value,
    args: Vec<Value>,
) -> R {
    mc_goto(stage, lib, caller, &this, &args, false)
}

fn mc_goto_and_stop(
    stage: &mut Stage,
    lib: &Library,
    caller: &Frame,
    this: Value,
    args: Vec<Value>,
) -> R {
    mc_goto(stage, lib, caller, &this, &args, true)
}

fn mc_play(stage: &mut Stage, _: &Library, _: &Frame, this: Value, _: Vec<Value>) -> R {
    if let Some(clip) = this_clip(stage, &this) {
        stage.set_playing(clip, true);
    }
    Ok(Value::Undefined)
}

fn mc_stop(stage: &mut Stage, _: &Library, _: &Frame, this: Value, _: Vec<Value>) -> R {
    if let Some(clip) = this_clip(stage, &this) {
        stage.set_playing(clip, false);
    }
    Ok(Value::Undefined)
}

fn mc_next_frame(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, _: Vec<Value>) -> R {
    if let Some(clip) = this_clip(stage, &this) {
        stage.step_frame(lib, clip, true);
    }
    Ok(Value::Undefined)
}

fn mc_prev_frame(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, _: Vec<Value>) -> R {
    if let Some(clip) = this_clip(stage, &this) {
        stage.step_frame(lib, clip, false);
    }
    Ok(Value::Undefined)
}

fn get_depth(stage: &mut Stage, _: &Library, _: &Frame, this: Value, _: Vec<Value>) -> R {
    Ok(this_clip(stage, &this)
        .and_then(|c| stage.arena.get(c))
        .map_or(Value::Undefined, |i| {
            Value::Number(f64::from(i.depth.wrapping_sub(AVM_DEPTH_BIAS)))
        }))
}

fn mc_swap_depths(
    stage: &mut Stage,
    lib: &Library,
    frame: &Frame,
    this: Value,
    args: Vec<Value>,
) -> R {
    let Some(clip) = this_clip(stage, &this) else {
        return Ok(Value::Undefined);
    };
    let Some(parent) = stage.arena.get(clip).and_then(|i| i.parent) else {
        return Ok(Value::Undefined);
    };
    let my_depth = stage.arena.get(clip).map_or(0, |i| i.depth);
    let target = arg(&args, 0);
    let other_depth = match &target {
        Value::Number(n) => super::value::to_i32(*n).wrapping_add(AVM_DEPTH_BIAS),
        other => match stage.resolve_target_display(lib, frame, parent, other, false) {
            Some(o) if stage.arena.get(o).and_then(|i| i.parent) == Some(parent) => {
                stage.arena.get(o).map_or(my_depth, |i| i.depth)
            }
            _ => return Ok(Value::Undefined),
        },
    };
    if other_depth == my_depth || !(0..=AVM_MAX_DEPTH).contains(&other_depth) {
        return Ok(Value::Undefined);
    }
    let other = stage
        .arena
        .get(parent)
        .and_then(|p| p.child_at(other_depth));
    if let Some(p) = stage.arena.get_mut(parent) {
        p.children.remove(&my_depth);
        if let Some(o) = other {
            p.children.insert(my_depth, o);
        }
        p.children.insert(other_depth, clip);
    }
    if let Some(o) = other.and_then(|o| stage.arena.get_mut(o)) {
        o.depth = my_depth;
        o.flags |= crate::instance::flags::TRANSFORMED_BY_SCRIPT;
    }
    if let Some(c) = stage.arena.get_mut(clip) {
        c.depth = other_depth;
        c.flags |= crate::instance::flags::TRANSFORMED_BY_SCRIPT;
    }
    Ok(Value::Undefined)
}

pub(crate) fn find_linkage(
    stage: &Stage,
    lib: &Library,
    clip: InstanceId,
    name: &str,
) -> Option<(MovieId, u32)> {
    let mut at = Some(clip);
    let mut first = true;
    while let Some(id) = at {
        let inst = stage.arena.get(id)?;
        let movie = inst.character.movie;
        let m = &lib.movie(movie)?.movie;
        if let Some(e) = m.exports.iter().find(|e| e.name.eq_ignore_ascii_case(name)) {
            return Some((movie, e.character));
        }
        if first {
            if let Some(i) = m.imports.iter().find(|i| i.name.eq_ignore_ascii_case(name)) {
                return Some((movie, i.local_character));
            }
        }
        first = false;
        at = inst.parent;
    }
    None
}

fn mc_attach_movie(
    stage: &mut Stage,
    lib: &Library,
    _: &Frame,
    this: Value,
    args: Vec<Value>,
) -> R {
    let Some(clip) = this_clip(stage, &this) else {
        return Ok(Value::Undefined);
    };
    if args.len() < 3 {
        return Ok(Value::Undefined);
    }
    let export = stage.string_of(lib, &arg(&args, 0));
    let name = stage.string_of(lib, &arg(&args, 1));
    let depth = arg(&args, 2).to_i32().wrapping_add(AVM_DEPTH_BIAS);
    if !(0..=AVM_MAX_DEPTH).contains(&depth) {
        return Ok(Value::Undefined);
    }
    let Some((movie, id)) = find_linkage(stage, lib, clip, &export) else {
        return Ok(Value::Undefined);
    };
    let Some(child) = stage.create_by_script(lib, clip, movie, id, depth, &name, |_| {}) else {
        return Ok(Value::Undefined);
    };
    if let Some(Value::Object(init)) = args.get(3) {
        copy_props(stage, lib, *init, child)?;
    }
    Ok(stage
        .script_object(child)
        .map_or(Value::Undefined, Value::Object))
}

fn copy_props(
    stage: &mut Stage,
    lib: &Library,
    from: ObjectId,
    to: InstanceId,
) -> Result<(), Halt> {
    let Some(target) = stage.script_object(to) else {
        return Ok(());
    };
    let pairs: Vec<(Arc<str>, Value)> = stage
        .heap
        .objects
        .get(from)
        .map(|o| {
            o.props
                .enumerable_names()
                .map(|n| {
                    (
                        Arc::clone(n),
                        o.props.get(n).cloned().unwrap_or(Value::Undefined),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    for (name, value) in pairs.into_iter().rev() {
        stage.set_member(lib, &Value::Object(target), &name, value)?;
    }
    Ok(())
}

pub(crate) fn clone_sprite(
    stage: &mut Stage,
    lib: &Library,
    clip: InstanceId,
    name: &str,
    depth: i32,
) -> Option<InstanceId> {
    let (parent, character, placed_id, matrix, color, actions) = {
        let i = stage.arena.get(clip)?;
        if !matches!(i.kind, InstanceKind::Clip(_)) {
            return None;
        }
        (
            i.parent?,
            i.character,
            i.placed_id,
            i.matrix,
            i.color,
            i.clip().and_then(|c| c.clip_actions),
        )
    };
    if !(0..=AVM_MAX_DEPTH).contains(&depth) {
        return None;
    }
    stage.create_by_script(
        lib,
        parent,
        character.movie,
        character.id,
        depth,
        name,
        |inst| {
            inst.placed_id = placed_id;
            inst.set_matrix(matrix);
            inst.color = color;
            if let (Some(a), Some(c)) = (actions, inst.clip_mut()) {
                c.clip_actions = Some(a);
            }
        },
    )
}

fn mc_duplicate(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, args: Vec<Value>) -> R {
    let Some(clip) = this_clip(stage, &this) else {
        return Ok(Value::Undefined);
    };
    let name = stage.string_of(lib, &arg(&args, 0));
    let depth = args
        .get(1)
        .map_or(0, Value::to_i32)
        .wrapping_add(AVM_DEPTH_BIAS);
    let new = clone_sprite(stage, lib, clip, &name, depth);
    if let (Some(n), Some(Value::Object(init))) = (new, args.get(2)) {
        copy_props(stage, lib, *init, n)?;
    }
    Ok(new
        .and_then(|n| stage.script_object(n))
        .map_or(Value::Undefined, Value::Object))
}

pub(crate) fn remove_display_object(stage: &mut Stage, lib: &Library, clip: InstanceId) {
    let Some(i) = stage.arena.get(clip) else {
        return;
    };
    if (AVM_DEPTH_BIAS..AVM_MAX_REMOVE_DEPTH).contains(&i.depth) && !i.removed && i.parent.is_some()
    {
        stage.remove_child(lib, clip);
    }
}

fn mc_remove(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, _: Vec<Value>) -> R {
    if let Some(clip) = this_clip(stage, &this) {
        remove_display_object(stage, lib, clip);
    }
    Ok(Value::Undefined)
}

fn mc_create_empty(
    stage: &mut Stage,
    lib: &Library,
    _: &Frame,
    this: Value,
    args: Vec<Value>,
) -> R {
    let Some(clip) = this_clip(stage, &this) else {
        return Ok(Value::Undefined);
    };
    if args.len() < 2 {
        return Ok(Value::Undefined);
    }
    let name = stage.string_of(lib, &arg(&args, 0));
    let depth = arg(&args, 1).to_i32().wrapping_add(AVM_DEPTH_BIAS);
    Ok(stage
        .create_empty_clip(lib, clip, depth, &name)
        .and_then(|c| stage.script_object(c))
        .map_or(Value::Undefined, Value::Object))
}

fn set_interval(stage: &mut Stage, lib: &Library, _: &Frame, _: Value, args: Vec<Value>) -> R {
    let callable = |stage: &Stage, v: &Value| {
        v.as_object()
            .and_then(|o| stage.heap.objects.get(o))
            .is_some_and(|o| matches!(o.native, Native::Function(_) | Native::Builtin(_)))
    };
    let (callee, method, rest) = match args.as_slice() {
        [f, rest @ ..] if callable(stage, f) => (f.clone(), None, rest),
        [Value::Object(o), name, rest @ ..] => {
            let name = stage.string_of(lib, name);
            (Value::Object(*o), Some(name), rest)
        }
        _ => return Ok(Value::Undefined),
    };
    let Some((interval, extra)) = rest.split_first() else {
        return Ok(Value::Undefined);
    };
    if stage.timers.len() >= 4096 {
        return Err(Halt::Limit { what: "timers" });
    }
    let interval = interval.to_number();
    let interval = if interval.is_finite() {
        interval.max(10.0)
    } else {
        10.0
    };
    let id = stage.next_timer;
    stage.next_timer = stage.next_timer.wrapping_add(1);
    let due = stage.now_ms() + interval;
    stage.timers.push(super::host::Timer {
        id,
        callee,
        method,
        args: extra.to_vec(),
        interval,
        due,
    });
    Ok(Value::Number(f64::from(id)))
}

fn clear_interval(stage: &mut Stage, _: &Library, _: &Frame, _: Value, args: Vec<Value>) -> R {
    let id = arg(&args, 0).to_i32();
    stage.timers.retain(|t| t.id != id);
    Ok(Value::Undefined)
}

fn mc_bytes(_: &mut Stage, _: &Library, _: &Frame, _: Value, _: Vec<Value>) -> R {
    Ok(Value::Number(1.0))
}

fn mc_instance_at_depth(
    stage: &mut Stage,
    _: &Library,
    _: &Frame,
    this: Value,
    args: Vec<Value>,
) -> R {
    let Some(clip) = this_clip(stage, &this) else {
        return Ok(Value::Undefined);
    };
    let depth = arg(&args, 0).to_i32().wrapping_add(AVM_DEPTH_BIAS);
    Ok(stage
        .arena
        .get(clip)
        .and_then(|i| i.child_at(depth))
        .and_then(|c| stage.script_object(c))
        .map_or(Value::Undefined, Value::Object))
}

fn mc_next_highest_depth(
    stage: &mut Stage,
    _: &Library,
    _: &Frame,
    this: Value,
    _: Vec<Value>,
) -> R {
    let Some(clip) = this_clip(stage, &this) else {
        return Ok(Value::Undefined);
    };
    let top = stage
        .arena
        .get(clip)
        .and_then(|i| i.children.keys().next_back().copied())
        .map_or(0, |d| {
            d.saturating_sub(AVM_DEPTH_BIAS).saturating_add(1).max(0)
        });
    Ok(Value::Number(f64::from(top)))
}

fn point_arg(stage: &mut Stage, lib: &Library, args: &[Value]) -> Option<(ObjectId, f32, f32)> {
    let obj = arg(args, 0).as_object()?;
    let x = stage.get_member(lib, &Value::Object(obj), "x").to_number();
    let y = stage.get_member(lib, &Value::Object(obj), "y").to_number();
    #[allow(clippy::cast_possible_truncation)]
    Some((obj, x as f32, y as f32))
}

fn world_matrix(stage: &Stage, clip: InstanceId) -> crate::geom::Matrix {
    let mut chain = Vec::new();
    let mut at = Some(clip);
    while let Some(id) = at {
        let Some(i) = stage.arena.get(id) else {
            break;
        };
        if chain.len() > 64 {
            break;
        }
        chain.push(i.matrix);
        at = i.parent;
    }
    chain
        .iter()
        .rev()
        .fold(crate::geom::Matrix::IDENTITY, |acc, m| acc.concat(m))
}

fn write_point(
    stage: &mut Stage,
    lib: &Library,
    obj: ObjectId,
    x: f32,
    y: f32,
) -> Result<(), Halt> {
    stage.set_member(lib, &Value::Object(obj), "x", Value::Number(f64::from(x)))?;
    stage.set_member(lib, &Value::Object(obj), "y", Value::Number(f64::from(y)))
}

fn mc_local_to_global(
    stage: &mut Stage,
    lib: &Library,
    _: &Frame,
    this: Value,
    args: Vec<Value>,
) -> R {
    if let (Some(clip), Some((obj, x, y))) = (this_clip(stage, &this), point_arg(stage, lib, &args))
    {
        let (gx, gy) = world_matrix(stage, clip).apply(x, y);
        write_point(stage, lib, obj, gx, gy)?;
    }
    Ok(Value::Undefined)
}

fn mc_global_to_local(
    stage: &mut Stage,
    lib: &Library,
    _: &Frame,
    this: Value,
    args: Vec<Value>,
) -> R {
    if let (Some(clip), Some((obj, x, y))) = (this_clip(stage, &this), point_arg(stage, lib, &args))
    {
        let m = world_matrix(stage, clip);
        let det = m.a * m.d - m.b * m.c;
        if det.abs() > f32::EPSILON {
            let (px, py) = (x - m.tx, y - m.ty);
            let lx = (m.d * px - m.c * py) / det;
            let ly = (-m.b * px + m.a * py) / det;
            write_point(stage, lib, obj, lx, ly)?;
        }
    }
    Ok(Value::Undefined)
}

fn mc_load_movie(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, args: Vec<Value>) -> R {
    let url = stage.string_of(lib, &arg(&args, 0)).to_string();
    let Some(clip) = this_clip(stage, &this) else {
        return Ok(Value::Undefined);
    };
    let target = stage.target_path_dot(clip);
    stage.request_load(&target, Some(&url));
    stage.push_outward(Outward::GetUrl { url, target });
    Ok(Value::Undefined)
}

fn mc_unload_movie(stage: &mut Stage, _: &Library, _: &Frame, this: Value, _: Vec<Value>) -> R {
    if let Some(clip) = this_clip(stage, &this) {
        let target = stage.target_path_dot(clip);
        stage.request_load(&target, None);
    }
    Ok(Value::Undefined)
}

fn mc_get_url(stage: &mut Stage, lib: &Library, _: &Frame, _: Value, args: Vec<Value>) -> R {
    let url = stage.string_of(lib, &arg(&args, 0)).to_string();
    let target = stage.string_of(lib, &arg(&args, 1)).to_string();
    let is_fs = url
        .get(..10)
        .is_some_and(|p| p.eq_ignore_ascii_case("FSCommand:"));
    if is_fs {
        stage.dispatch_outward(Outward::FsCommand {
            command: url.get(10..).unwrap_or("").to_owned(),
            args: target,
        });
    } else {
        stage.push_outward(Outward::GetUrl { url, target });
    }
    Ok(Value::Undefined)
}

fn color_ctor(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, args: Vec<Value>) -> R {
    if let Value::Object(_) = this {
        stage.set_member(lib, &this, "target", arg(&args, 0))?;
    }
    Ok(this)
}

fn color_target(
    stage: &mut Stage,
    lib: &Library,
    caller: &Frame,
    this: &Value,
) -> Option<InstanceId> {
    let target = stage.get_member(lib, this, "target");
    if target == Value::Undefined {
        return None;
    }
    let start = caller.target.or(stage.root)?;
    stage.resolve_target_display(lib, caller, start, &target, false)
}

fn color_set_rgb(
    stage: &mut Stage,
    lib: &Library,
    caller: &Frame,
    this: Value,
    args: Vec<Value>,
) -> R {
    if let Some(clip) = color_target(stage, lib, caller, &this) {
        let rgb = arg(&args, 0).to_i32();
        let [b, g, r, _] = rgb.to_le_bytes();
        if let Some(i) = stage.arena.get_mut(clip) {
            let mut c = i.color;
            c.mult[0] = 0.0;
            c.mult[1] = 0.0;
            c.mult[2] = 0.0;
            c.add[0] = f32::from(r) / 255.0;
            c.add[1] = f32::from(g) / 255.0;
            c.add[2] = f32::from(b) / 255.0;
            i.set_color(c);
        }
    }
    Ok(Value::Undefined)
}

fn byte(v: f32) -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let b = (v * 255.0).round() as i32;
    b.clamp(-255, 255)
}

fn color_get_rgb(
    stage: &mut Stage,
    lib: &Library,
    caller: &Frame,
    this: Value,
    _: Vec<Value>,
) -> R {
    let Some(clip) = color_target(stage, lib, caller, &this) else {
        return Ok(Value::Undefined);
    };
    let c = stage
        .arena
        .get(clip)
        .map_or(ColorTransform::IDENTITY, |i| i.color);
    let rgb = (byte(c.add[0]) << 16) | (byte(c.add[1]) << 8) | byte(c.add[2]);
    Ok(Value::Number(f64::from(rgb)))
}

fn color_set_transform(
    stage: &mut Stage,
    lib: &Library,
    caller: &Frame,
    this: Value,
    args: Vec<Value>,
) -> R {
    let Some(clip) = color_target(stage, lib, caller, &this) else {
        return Ok(Value::Undefined);
    };
    let Some(Value::Object(t)) = args.first() else {
        return Ok(Value::Undefined);
    };
    let mut c = stage
        .arena
        .get(clip)
        .map_or(ColorTransform::IDENTITY, |i| i.color);
    for (i, (m, a)) in [("ra", "rb"), ("ga", "gb"), ("ba", "bb"), ("aa", "ab")]
        .into_iter()
        .enumerate()
    {
        let mv = stage.get_member(lib, &Value::Object(*t), m);
        if mv != Value::Undefined {
            if let Some(slot) = c.mult.get_mut(i) {
                #[allow(clippy::cast_possible_truncation)]
                let f = (mv.to_number() / 100.0) as f32;
                *slot = f;
            }
        }
        let av = stage.get_member(lib, &Value::Object(*t), a);
        if av != Value::Undefined {
            if let Some(slot) = c.add.get_mut(i) {
                #[allow(clippy::cast_possible_truncation)]
                let f = (av.to_number() / 255.0) as f32;
                *slot = f;
            }
        }
    }
    if let Some(i) = stage.arena.get_mut(clip) {
        i.set_color(c);
    }
    Ok(Value::Undefined)
}

fn color_get_transform(
    stage: &mut Stage,
    lib: &Library,
    caller: &Frame,
    this: Value,
    _: Vec<Value>,
) -> R {
    let Some(clip) = color_target(stage, lib, caller, &this) else {
        return Ok(Value::Undefined);
    };
    let c = stage
        .arena
        .get(clip)
        .map_or(ColorTransform::IDENTITY, |i| i.color);
    let o = stage.new_object()?;
    for (i, (m, a)) in [("ra", "rb"), ("ga", "gb"), ("ba", "bb"), ("aa", "ab")]
        .into_iter()
        .enumerate()
    {
        let mv = c.mult.get(i).copied().unwrap_or(1.0);
        let av = c.add.get(i).copied().unwrap_or(0.0);
        stage.set_member(
            lib,
            &Value::Object(o),
            m,
            Value::Number(f64::from(mv) * 100.0),
        )?;
        stage.set_member(
            lib,
            &Value::Object(o),
            a,
            Value::Number(f64::from(av) * 255.0),
        )?;
    }
    Ok(Value::Object(o))
}

fn this_chars(stage: &mut Stage, lib: &Library, this: &Value) -> Vec<char> {
    stage.string_of(lib, this).chars().collect()
}

fn clamp_index(v: f64, len: usize) -> usize {
    if v.is_nan() || v <= 0.0 {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let i = v.min(1e9) as usize;
    i.min(len)
}

fn str_of(chars: &[char]) -> Value {
    Value::str(&chars.iter().collect::<String>())
}

fn str_substring(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, args: Vec<Value>) -> R {
    let chars = this_chars(stage, lib, &this);
    let a = clamp_index(arg(&args, 0).to_number(), chars.len());
    let b = match args.get(1) {
        None | Some(Value::Undefined) => chars.len(),
        Some(v) => clamp_index(v.to_number(), chars.len()),
    };
    let (lo, hi) = (a.min(b), a.max(b));
    Ok(str_of(chars.get(lo..hi).unwrap_or(&[])))
}

fn str_substr(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, args: Vec<Value>) -> R {
    let chars = this_chars(stage, lib, &this);
    let len = chars.len();
    let start = arg(&args, 0).to_number();
    #[allow(clippy::cast_precision_loss)]
    let start = if start < 0.0 {
        (len as f64 + start).max(0.0)
    } else {
        start
    };
    let s = clamp_index(start, len);
    let count = match args.get(1) {
        None | Some(Value::Undefined) => len,
        Some(v) => clamp_index(v.to_number(), len),
    };
    let e = s.saturating_add(count).min(len);
    Ok(str_of(chars.get(s..e).unwrap_or(&[])))
}

fn rel_index(v: &Value, len: usize, default: usize) -> usize {
    match v {
        Value::Undefined => default,
        v => {
            let n = v.to_number();
            #[allow(clippy::cast_precision_loss)]
            let n = if n < 0.0 {
                (len as f64 + n).max(0.0)
            } else {
                n
            };
            clamp_index(n, len)
        }
    }
}

fn str_slice(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, args: Vec<Value>) -> R {
    let chars = this_chars(stage, lib, &this);
    let s = rel_index(&arg(&args, 0), chars.len(), 0);
    let e = rel_index(&arg(&args, 1), chars.len(), chars.len());
    Ok(str_of(chars.get(s..e.max(s)).unwrap_or(&[])))
}

fn find(hay: &[char], needle: &[char], from: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(from.min(hay.len()));
    }
    (from..hay.len()).find(|&i| hay.get(i..i.saturating_add(needle.len())) == Some(needle))
}

fn str_index_of(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, args: Vec<Value>) -> R {
    let hay = this_chars(stage, lib, &this);
    let needle: Vec<char> = stage.string_of(lib, &arg(&args, 0)).chars().collect();
    let from = clamp_index(arg(&args, 1).to_number(), hay.len());
    #[allow(clippy::cast_precision_loss)]
    Ok(Value::Number(
        find(&hay, &needle, from).map_or(-1.0, |i| i as f64),
    ))
}

fn str_last_index_of(
    stage: &mut Stage,
    lib: &Library,
    _: &Frame,
    this: Value,
    args: Vec<Value>,
) -> R {
    let hay = this_chars(stage, lib, &this);
    let needle: Vec<char> = stage.string_of(lib, &arg(&args, 0)).chars().collect();
    let mut last = None;
    let mut from = 0usize;
    while let Some(i) = find(&hay, &needle, from) {
        last = Some(i);
        from = i.saturating_add(1);
        if from > hay.len() {
            break;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    Ok(Value::Number(last.map_or(-1.0, |i| i as f64)))
}

fn str_char_at(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, args: Vec<Value>) -> R {
    let chars = this_chars(stage, lib, &this);
    let n = arg(&args, 0).to_number();
    let c = if n >= 0.0 {
        chars.get(clamp_index(n, usize::MAX))
    } else {
        None
    };
    Ok(Value::str(&c.map(char::to_string).unwrap_or_default()))
}

fn str_char_code_at(
    stage: &mut Stage,
    lib: &Library,
    _: &Frame,
    this: Value,
    args: Vec<Value>,
) -> R {
    let chars = this_chars(stage, lib, &this);
    let n = arg(&args, 0).to_number();
    let c = if n >= 0.0 {
        chars.get(clamp_index(n, usize::MAX))
    } else {
        None
    };
    Ok(Value::Number(
        c.map_or(f64::NAN, |c| f64::from(u32::from(*c))),
    ))
}

fn str_upper(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, _: Vec<Value>) -> R {
    Ok(Value::str(&stage.string_of(lib, &this).to_uppercase()))
}

fn str_lower(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, _: Vec<Value>) -> R {
    Ok(Value::str(&stage.string_of(lib, &this).to_lowercase()))
}

fn str_split(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, args: Vec<Value>) -> R {
    let s = stage.string_of(lib, &this);
    let parts: Vec<Value> = match args.first() {
        None | Some(Value::Undefined) => vec![Value::String(s)],
        Some(sep) => {
            let sep = stage.string_of(lib, sep);
            if sep.is_empty() {
                s.chars().map(|c| Value::str(&c.to_string())).collect()
            } else {
                s.split(&*sep).map(Value::str).collect()
            }
        }
    };
    Ok(Value::Object(stage.new_array(parts)?))
}

fn str_concat(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, args: Vec<Value>) -> R {
    let mut s = stage.string_of(lib, &this).to_string();
    for a in &args {
        s.push_str(&stage.string_of(lib, a));
        if s.len() > stage.vm_limits.max_string_len {
            return Err(Halt::Limit {
                what: "string length",
            });
        }
    }
    Ok(Value::str(&s))
}

fn str_from_char_code(_: &mut Stage, _: &Library, _: &Frame, _: Value, args: Vec<Value>) -> R {
    let s: String = args
        .iter()
        .filter_map(|a| u32::try_from(a.to_i32()).ok().and_then(char::from_u32))
        .collect();
    Ok(Value::str(&s))
}

fn array_values(stage: &mut Stage, lib: &Library, id: ObjectId) -> Vec<Value> {
    let len = stage.array_len(id);
    (0..len)
        .map(|i| stage.get_member(lib, &Value::Object(id), &i.to_string()))
        .collect()
}

fn set_array(stage: &mut Stage, id: ObjectId, values: Vec<Value>) {
    if let Some(o) = stage.heap.objects.get_mut(id) {
        let old = o.props.get("length").map_or(0.0, Value::to_number);
        let mut i = 0.0;
        while i < old {
            o.props.remove(&format_number(i));
            i += 1.0;
        }
        #[allow(clippy::cast_precision_loss)]
        let len = values.len() as f64;
        for (i, v) in values.into_iter().enumerate() {
            o.props.set(&i.to_string(), v);
        }
        o.props.define("length", Value::Number(len), true);
    }
}

fn arr_push(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, args: Vec<Value>) -> R {
    let Some(id) = this.as_object() else {
        return Ok(Value::Undefined);
    };
    let mut values = array_values(stage, lib, id);
    values.extend(args);
    #[allow(clippy::cast_precision_loss)]
    let len = values.len() as f64;
    set_array(stage, id, values);
    Ok(Value::Number(len))
}

fn arr_pop(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, _: Vec<Value>) -> R {
    let Some(id) = this.as_object() else {
        return Ok(Value::Undefined);
    };
    let mut values = array_values(stage, lib, id);
    let last = values.pop().unwrap_or(Value::Undefined);
    set_array(stage, id, values);
    Ok(last)
}

fn arr_shift(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, _: Vec<Value>) -> R {
    let Some(id) = this.as_object() else {
        return Ok(Value::Undefined);
    };
    let mut values = array_values(stage, lib, id);
    let first = if values.is_empty() {
        Value::Undefined
    } else {
        values.remove(0)
    };
    set_array(stage, id, values);
    Ok(first)
}

fn arr_unshift(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, args: Vec<Value>) -> R {
    let Some(id) = this.as_object() else {
        return Ok(Value::Undefined);
    };
    let mut values = args;
    values.extend(array_values(stage, lib, id));
    #[allow(clippy::cast_precision_loss)]
    let len = values.len() as f64;
    set_array(stage, id, values);
    Ok(Value::Number(len))
}

fn arr_join(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, args: Vec<Value>) -> R {
    let Some(id) = this.as_object() else {
        return Ok(Value::str(""));
    };
    let sep = match args.first() {
        None | Some(Value::Undefined) => Arc::from(","),
        Some(s) => stage.string_of(lib, s),
    };
    Ok(Value::str(&stage.join_array(lib, id, &sep)))
}

fn arr_to_string(stage: &mut Stage, lib: &Library, frame: &Frame, this: Value, _: Vec<Value>) -> R {
    arr_join(stage, lib, frame, this, Vec::new())
}

fn arr_concat(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, args: Vec<Value>) -> R {
    let mut values = this
        .as_object()
        .map(|id| array_values(stage, lib, id))
        .unwrap_or_default();
    for a in args {
        let is_array = a
            .as_object()
            .and_then(|o| stage.heap.objects.get(o))
            .is_some_and(|o| matches!(o.native, Native::Array));
        match (is_array, a.as_object()) {
            (true, Some(o)) => values.extend(array_values(stage, lib, o)),
            _ => values.push(a),
        }
    }
    Ok(Value::Object(stage.new_array(values)?))
}

fn arr_slice(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, args: Vec<Value>) -> R {
    let values = this
        .as_object()
        .map(|id| array_values(stage, lib, id))
        .unwrap_or_default();
    let s = rel_index(&arg(&args, 0), values.len(), 0);
    let e = rel_index(&arg(&args, 1), values.len(), values.len());
    let out = values
        .get(s..e.max(s))
        .map(<[Value]>::to_vec)
        .unwrap_or_default();
    Ok(Value::Object(stage.new_array(out)?))
}

fn arr_splice(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, args: Vec<Value>) -> R {
    let Some(id) = this.as_object() else {
        return Ok(Value::Undefined);
    };
    let mut values = array_values(stage, lib, id);
    let s = rel_index(&arg(&args, 0), values.len(), 0);
    let count = match args.get(1) {
        None => values.len().saturating_sub(s),
        Some(v) => clamp_index(v.to_number(), values.len().saturating_sub(s)),
    };
    let removed: Vec<Value> = values
        .splice(s..s.saturating_add(count), args.into_iter().skip(2))
        .collect();
    set_array(stage, id, values);
    Ok(Value::Object(stage.new_array(removed)?))
}

fn arr_reverse(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, _: Vec<Value>) -> R {
    if let Some(id) = this.as_object() {
        let mut values = array_values(stage, lib, id);
        values.reverse();
        set_array(stage, id, values);
    }
    Ok(this)
}

fn arr_sort(stage: &mut Stage, lib: &Library, _: &Frame, this: Value, _: Vec<Value>) -> R {
    if let Some(id) = this.as_object() {
        let values = array_values(stage, lib, id);
        let mut keyed: Vec<(Arc<str>, Value)> = values
            .into_iter()
            .map(|v| (stage.string_of(lib, &v), v))
            .collect();
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        set_array(stage, id, keyed.into_iter().map(|(_, v)| v).collect());
    }
    Ok(this)
}

fn array_ctor(stage: &mut Stage, _: &Library, _: &Frame, this: Value, args: Vec<Value>) -> R {
    let values = match args.as_slice() {
        [Value::Number(n)] => {
            let len = clamp_index(*n, 65_536);
            vec![Value::Undefined; len]
        }
        _ => args,
    };
    match this.as_object() {
        Some(id)
            if stage
                .heap
                .objects
                .get(id)
                .is_some_and(|o| matches!(o.native, Native::Plain)) =>
        {
            if let Some(o) = stage.heap.objects.get_mut(id) {
                o.native = Native::Array;
                o.proto = Some(stage.heap.protos.array);
            }
            set_array(stage, id, values);
            Ok(Value::Object(id))
        }
        _ => Ok(Value::Object(stage.new_array(values)?)),
    }
}

fn num(args: &[Value], i: usize) -> f64 {
    arg(args, i).to_number()
}

fn math_abs(_: &mut Stage, _: &Library, _: &Frame, _: Value, a: Vec<Value>) -> R {
    Ok(Value::Number(num(&a, 0).abs()))
}
fn math_floor(_: &mut Stage, _: &Library, _: &Frame, _: Value, a: Vec<Value>) -> R {
    Ok(Value::Number(num(&a, 0).floor()))
}
fn math_ceil(_: &mut Stage, _: &Library, _: &Frame, _: Value, a: Vec<Value>) -> R {
    Ok(Value::Number(num(&a, 0).ceil()))
}
fn math_round(_: &mut Stage, _: &Library, _: &Frame, _: Value, a: Vec<Value>) -> R {
    Ok(Value::Number((num(&a, 0) + 0.5).floor()))
}
fn math_min(_: &mut Stage, _: &Library, _: &Frame, _: Value, a: Vec<Value>) -> R {
    Ok(Value::Number(
        a.iter().map(Value::to_number).fold(f64::INFINITY, f64::min),
    ))
}
fn math_max(_: &mut Stage, _: &Library, _: &Frame, _: Value, a: Vec<Value>) -> R {
    Ok(Value::Number(
        a.iter()
            .map(Value::to_number)
            .fold(f64::NEG_INFINITY, f64::max),
    ))
}
fn math_random(stage: &mut Stage, _: &Library, _: &Frame, _: Value, _: Vec<Value>) -> R {
    let r = stage.next_random();
    #[allow(clippy::cast_precision_loss)]
    let f = (r >> 11) as f64 / (1u64 << 53) as f64;
    Ok(Value::Number(f))
}
fn math_sqrt(_: &mut Stage, _: &Library, _: &Frame, _: Value, a: Vec<Value>) -> R {
    Ok(Value::Number(num(&a, 0).sqrt()))
}
fn math_pow(_: &mut Stage, _: &Library, _: &Frame, _: Value, a: Vec<Value>) -> R {
    Ok(Value::Number(mint_math::f64::powf(num(&a, 0), num(&a, 1))))
}
fn math_sin(_: &mut Stage, _: &Library, _: &Frame, _: Value, a: Vec<Value>) -> R {
    Ok(Value::Number(mint_math::f64::sin(num(&a, 0))))
}
fn math_cos(_: &mut Stage, _: &Library, _: &Frame, _: Value, a: Vec<Value>) -> R {
    Ok(Value::Number(mint_math::f64::cos(num(&a, 0))))
}
fn math_atan2(_: &mut Stage, _: &Library, _: &Frame, _: Value, a: Vec<Value>) -> R {
    Ok(Value::Number(mint_math::f64::atan2(num(&a, 0), num(&a, 1))))
}
