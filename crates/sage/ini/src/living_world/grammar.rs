use std::collections::BTreeMap;

use crate::diagnostic::ValueErrors;
use crate::tree::{Block, Field, Node};
use crate::value::{self, MacroTable};

pub(crate) struct Cx<'a> {
    pub macros: &'a MacroTable,
    pub errors: &'a ValueErrors,
}

impl Cx<'_> {
    pub(crate) fn unknown_field(&self, ctx: &str, field: &Field) {
        self.errors.field(ctx, field, "a field of this block");
    }

    pub(crate) fn unknown_node(&self, ctx: &str, node: &Node) {
        match node {
            Node::Field(field) => self.unknown_field(ctx, field),
            Node::Block(block) => self.block_error(ctx, block, "a block this block holds"),
            Node::Script(script) => self.errors.at(
                &script.file,
                script.line,
                ctx,
                "BeginScript",
                "",
                "allowed in this block",
            ),
        }
    }

    pub(crate) fn block_error(&self, ctx: &str, block: &Block, expected: &'static str) {
        self.errors.at(
            &block.file,
            block.line,
            ctx,
            &block.keyword,
            &block.args.join(" "),
            expected,
        );
    }

    pub(crate) fn missing_name(&self, block: &Block) {
        self.block_error(&block.keyword, block, "named");
    }
}

pub(crate) fn label(block: &Block) -> String {
    if block.args.is_empty() {
        block.keyword.clone()
    } else {
        format!("{} {}", block.keyword, block.args.join(" "))
    }
}

pub(crate) fn walk(block: &Block, cx: &Cx, ctx: &str, mut handle: impl FnMut(&Node) -> bool) {
    for node in &block.children {
        if !handle(node) {
            cx.unknown_node(ctx, node);
        }
    }
}

pub(crate) trait Prim: Sized {
    const EXPECTED: &'static str;
    fn parse(args: &[String], macros: &MacroTable) -> Option<Self>;
}

pub(crate) trait Slot {
    fn read(&mut self, field: &Field, cx: &Cx, ctx: &str);
}

impl<T: Prim> Slot for Option<T> {
    fn read(&mut self, field: &Field, cx: &Cx, ctx: &str) {
        match T::parse(&field.args, cx.macros) {
            Some(value) => *self = Some(value),
            None => cx.errors.field(ctx, field, T::EXPECTED),
        }
    }
}

impl<T: Prim> Slot for Vec<T> {
    fn read(&mut self, field: &Field, cx: &Cx, ctx: &str) {
        match T::parse(&field.args, cx.macros) {
            Some(value) => self.push(value),
            None => cx.errors.field(ctx, field, T::EXPECTED),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tokens(pub Vec<String>);

impl Slot for Tokens {
    fn read(&mut self, field: &Field, _cx: &Cx, _ctx: &str) {
        self.0 = field.args.clone();
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenList(pub Vec<String>);

impl Slot for TokenList {
    fn read(&mut self, field: &Field, _cx: &Cx, _ctx: &str) {
        self.0.extend(field.args.iter().cloned());
    }
}

pub(crate) trait Named: Copy + PartialEq {
    const WHAT: &'static str;
    fn from_name(token: &str) -> Option<Self>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumList<E>(pub Vec<E>);

impl<E> Default for EnumList<E> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<E: Named> Slot for EnumList<E> {
    fn read(&mut self, field: &Field, cx: &Cx, ctx: &str) {
        for token in &field.args {
            match E::from_name(token) {
                Some(member) => self.0.push(member),
                None => cx.errors.value(ctx, field, token, E::WHAT),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bits<E>(pub Vec<E>);

impl<E> Default for Bits<E> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<E: Named> Slot for Bits<E> {
    fn read(&mut self, field: &Field, cx: &Cx, ctx: &str) {
        let Ok(edits) = value::bit_string_edits(&field.args) else {
            cx.errors.field(ctx, field, value::BIT_STRING_EXPECTED);
            return;
        };
        for edit in edits {
            match edit {
                value::BitEdit::Clear => self.0.clear(),
                value::BitEdit::Set(name) | value::BitEdit::Unset(name) => {
                    let Some(member) = E::from_name(name) else {
                        cx.errors.value(ctx, field, name, E::WHAT);
                        continue;
                    };
                    if matches!(edit, value::BitEdit::Set(_)) {
                        if !self.0.contains(&member) {
                            self.0.push(member);
                        }
                    } else {
                        self.0.retain(|held| *held != member);
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShadowBits(pub Vec<String>);

impl Slot for ShadowBits {
    fn read(&mut self, field: &Field, cx: &Cx, ctx: &str) {
        let Ok(edits) = value::bit_string_edits(&field.args) else {
            cx.errors.field(ctx, field, value::BIT_STRING_EXPECTED);
            return;
        };
        for edit in edits {
            match edit {
                value::BitEdit::Clear => self.0.clear(),
                value::BitEdit::Set(name) => {
                    if name.starts_with("SHADOW_") {
                        if !self.0.iter().any(|held| held == name) {
                            self.0.push(name.to_owned());
                        }
                    } else {
                        cx.errors.value(ctx, field, name, "a SHADOW_ name");
                    }
                }
                value::BitEdit::Unset(name) => self.0.retain(|held| held != name),
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TeamList(pub Vec<u32>);

impl Slot for TeamList {
    fn read(&mut self, field: &Field, cx: &Cx, ctx: &str) {
        let mut teams = Vec::with_capacity(field.args.len());
        for token in &field.args {
            let team = scan_int(std::slice::from_ref(token), cx.macros)
                .and_then(|n| u32::try_from(n).ok())
                .and_then(|n| n.checked_sub(1));
            match team {
                Some(team) => teams.push(team),
                None => {
                    cx.errors.value(ctx, field, token, "a team number from 1");
                    return;
                }
            }
        }
        self.0 = teams;
    }
}

#[allow(clippy::cast_possible_truncation)]
fn truncated(value: f32) -> Option<i64> {
    (value.is_finite() && value.abs() < 2.0e9).then(|| value.trunc() as i64)
}

fn scan_int(args: &[String], macros: &MacroTable) -> Option<i64> {
    let first = args.first()?;
    if first.starts_with('#') {
        return value::evaluate_expression(args, macros).and_then(truncated);
    }
    if macros.get(first).is_some() || macros.tokens(first).is_some() {
        return value::evaluate_expression(std::slice::from_ref(first), macros).and_then(truncated);
    }
    value::parse_int(first)
}

impl Prim for String {
    const EXPECTED: &'static str = "a name";
    fn parse(args: &[String], _macros: &MacroTable) -> Option<Self> {
        args.first().cloned()
    }
}

impl Prim for i32 {
    const EXPECTED: &'static str = "an integer";
    fn parse(args: &[String], macros: &MacroTable) -> Option<Self> {
        i32::try_from(scan_int(args, macros)?).ok()
    }
}

impl Prim for u32 {
    const EXPECTED: &'static str = "a non-negative integer";
    fn parse(args: &[String], macros: &MacroTable) -> Option<Self> {
        u32::try_from(scan_int(args, macros)?).ok()
    }
}

impl Prim for f32 {
    const EXPECTED: &'static str = "a number";
    fn parse(args: &[String], macros: &MacroTable) -> Option<Self> {
        let first = args.first()?;
        if first.starts_with('#') {
            value::evaluate_expression(args, macros)
        } else {
            value::evaluate_expression(std::slice::from_ref(first), macros)
        }
    }
}

impl Prim for bool {
    const EXPECTED: &'static str = "Yes or No";
    fn parse(args: &[String], _macros: &MacroTable) -> Option<Self> {
        value::parse_bool(args.first()?)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Percent(pub f32);

impl Prim for Percent {
    const EXPECTED: &'static str = "a percentage";
    fn parse(args: &[String], macros: &MacroTable) -> Option<Self> {
        value::resolve_percent(args, macros).map(Percent)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Millis(pub u32);

impl Prim for Millis {
    const EXPECTED: &'static str = "a duration in seconds";
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn parse(args: &[String], macros: &MacroTable) -> Option<Self> {
        let seconds = <f32 as Prim>::parse(args, macros)?;
        let ms = (f64::from(seconds) * 1000.0).trunc();
        (ms >= 0.0 && ms <= f64::from(u32::MAX)).then_some(Millis(ms as u32))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Coord2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Coord3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ColorInt {
    pub r: i32,
    pub g: i32,
    pub b: i32,
    pub a: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FontSpec {
    pub name: String,
    pub size: i32,
    pub bold: bool,
}

pub(crate) fn labelled(args: &[String]) -> Option<Vec<(&str, &str)>> {
    let mut pairs = Vec::new();
    let mut tokens = args.iter().map(String::as_str);
    while let Some(token) = tokens.next() {
        let (label, rest) = token.split_once(':')?;
        let found = if rest.is_empty() {
            tokens.next()?
        } else {
            rest
        };
        pairs.push((label, found));
    }
    Some(pairs)
}

fn ordered(args: &[String], macros: &MacroTable, labels: &[&str]) -> Option<Vec<f32>> {
    let pairs = labelled(args)?;
    if pairs.len() != labels.len() {
        return None;
    }
    pairs
        .iter()
        .zip(labels)
        .map(|((label, found), want)| {
            if !label.eq_ignore_ascii_case(want) {
                return None;
            }
            value::parse_float_or_macro(found, macros)
        })
        .collect()
}

impl Prim for Coord2 {
    const EXPECTED: &'static str = "X:<real> Y:<real>";
    fn parse(args: &[String], macros: &MacroTable) -> Option<Self> {
        match ordered(args, macros, &["X", "Y"])?.as_slice() {
            [x, y] => Some(Coord2 { x: *x, y: *y }),
            _ => None,
        }
    }
}

impl Prim for Coord3 {
    const EXPECTED: &'static str = "X:<real> Y:<real> Z:<real>";
    fn parse(args: &[String], macros: &MacroTable) -> Option<Self> {
        match ordered(args, macros, &["X", "Y", "Z"])?.as_slice() {
            [x, y, z] => Some(Coord3 {
                x: *x,
                y: *y,
                z: *z,
            }),
            _ => None,
        }
    }
}

impl Prim for Rgb {
    const EXPECTED: &'static str = "R:<n> G:<n> B:<n>";
    fn parse(args: &[String], macros: &MacroTable) -> Option<Self> {
        match ordered(args, macros, &["R", "G", "B"])?.as_slice() {
            [r, g, b] => Some(Rgb {
                r: *r / 255.0,
                g: *g / 255.0,
                b: *b / 255.0,
            }),
            _ => None,
        }
    }
}

impl Prim for ColorInt {
    const EXPECTED: &'static str = "R:<n> G:<n> B:<n> [A:<n>]";
    fn parse(args: &[String], macros: &MacroTable) -> Option<Self> {
        let pairs = labelled(args)?;
        let channels = ["R", "G", "B", "A"];
        if pairs.len() < 3 || pairs.len() > 4 {
            return None;
        }
        let mut found = [0i32, 0, 0, 255];
        for ((slot, want), (label, text)) in found.iter_mut().zip(channels).zip(&pairs) {
            if !label.eq_ignore_ascii_case(want) {
                return None;
            }
            *slot = i32::try_from(scan_int(&[(*text).to_owned()], macros)?).ok()?;
        }
        let [r, g, b, a] = found;
        Some(ColorInt { r, g, b, a })
    }
}

impl Prim for FontSpec {
    const EXPECTED: &'static str = "<face> <size> <bold>";
    fn parse(args: &[String], macros: &MacroTable) -> Option<Self> {
        let [name, size, bold] = args else {
            return None;
        };
        Some(FontSpec {
            name: name.clone(),
            size: i32::try_from(scan_int(std::slice::from_ref(size), macros)?).ok()?,
            bold: value::parse_bool(bold)?,
        })
    }
}

pub(crate) fn labels_in_any_order<'a>(
    pairs: &[(&str, &'a str)],
    allowed: &[&str],
) -> Option<Vec<Option<&'a str>>> {
    let mut out: Vec<Option<&'a str>> = vec![None; allowed.len()];
    for (label, found) in pairs {
        let at = allowed
            .iter()
            .position(|want| label.eq_ignore_ascii_case(want))?;
        let slot = out.get_mut(at)?;
        if slot.is_some() {
            return None;
        }
        *slot = Some(*found);
    }
    Some(out)
}

pub(crate) fn number(token: &str, macros: &MacroTable) -> Option<f32> {
    value::evaluate_expression(&[token.to_owned()], macros)
}

pub(crate) fn integer(token: &str, macros: &MacroTable) -> Option<i32> {
    i32::try_from(scan_int(&[token.to_owned()], macros)?).ok()
}

pub(crate) fn percent(token: &str, macros: &MacroTable) -> Option<f32> {
    value::resolve_percent(&[token.to_owned()], macros)
}

pub trait Defined {
    fn def_name(&self) -> &str;
}

#[derive(Debug, Clone)]
pub struct Store<T> {
    items: Vec<T>,
    by_name: BTreeMap<String, usize>,
    defined: usize,
}

impl<T> Default for Store<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            by_name: BTreeMap::new(),
            defined: 0,
        }
    }
}

impl<T: Defined> Store<T> {
    pub(crate) fn insert(&mut self, item: T) {
        self.defined = self.defined.saturating_add(1);
        let key = item.def_name().to_ascii_lowercase();
        match self.by_name.get(&key).copied() {
            Some(slot) => {
                if let Some(held) = self.items.get_mut(slot) {
                    *held = item;
                }
            }
            None => {
                self.by_name.insert(key, self.items.len());
                self.items.push(item);
            }
        }
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&T> {
        let slot = self.by_name.get(&name.to_ascii_lowercase())?;
        self.items.get(*slot)
    }

    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.by_name.contains_key(&name.to_ascii_lowercase())
    }

    #[must_use]
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.by_name.get(&name.to_ascii_lowercase()).copied()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.items.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub fn defined(&self) -> usize {
        self.defined
    }
}

macro_rules! fields {
    (
        $(#[$meta:meta])*
        pub struct $name:ident {
            $( $(#[$fmeta:meta])* $field:ident : $ty:ty = $kw:literal ),* $(,)?
        }
        $( with {
            $( $(#[$emeta:meta])* pub $extra:ident : $ety:ty ),* $(,)?
        } )?
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Default, PartialEq)]
        pub struct $name {
            pub name: String,
            $(
                #[doc = concat!("`", $kw, "`.")]
                $(#[$fmeta])*
                pub $field: $ty,
            )*
            $( $(
                $(#[$emeta])*
                pub $extra: $ety,
            )* )?
        }

        impl $name {
            pub const KEYWORDS: &'static [&'static str] = &[ $($kw),* ];

            #[allow(dead_code)]
            pub(crate) fn read_plain_block(
                block: &$crate::tree::Block,
                cx: &$crate::living_world::grammar::Cx,
            ) -> Self {
                let ctx = $crate::living_world::grammar::label(block);
                let mut out = Self {
                    name: block.name().unwrap_or_default().to_owned(),
                    ..Self::default()
                };
                $crate::living_world::grammar::walk(block, cx, &ctx, |node| match node {
                    $crate::tree::Node::Field(field) => out.read_field(field, cx, &ctx),
                    _ => false,
                });
                out
            }

            #[allow(dead_code, unused_variables)]
            pub(crate) fn read_field(
                &mut self,
                field: &$crate::tree::Field,
                cx: &$crate::living_world::grammar::Cx,
                ctx: &str,
            ) -> bool {
                match field.keyword.as_str() {
                    $( $kw => {
                        $crate::living_world::grammar::Slot::read(&mut self.$field, field, cx, ctx);
                        true
                    } )*
                    _ => false,
                }
            }
        }

        impl $crate::living_world::grammar::Table for $name {
            fn read_table(
                &mut self,
                field: &$crate::tree::Field,
                cx: &$crate::living_world::grammar::Cx,
                ctx: &str,
            ) -> bool {
                self.read_field(field, cx, ctx)
            }
        }
    };
}
pub(crate) use fields;

pub(crate) trait Table {
    fn read_table(&mut self, field: &Field, cx: &Cx, ctx: &str) -> bool;
}

macro_rules! index_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident ($what:literal) {
            $( $var:ident = $text:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {
            $( #[doc = concat!("`", $text, "`.")] $var, )+
        }

        impl $name {
            pub const NAMES: &'static [&'static str] = &[ $($text),+ ];

            #[must_use]
            pub fn from_name(token: &str) -> Option<Self> {
                $( if token.eq_ignore_ascii_case($text) { return Some(Self::$var); } )+
                None
            }

            #[must_use]
            pub fn name(self) -> &'static str {
                match self { $( Self::$var => $text, )+ }
            }
        }

        impl $crate::living_world::grammar::Named for $name {
            const WHAT: &'static str = concat!("a ", $what);
            fn from_name(token: &str) -> Option<Self> { Self::from_name(token) }
        }

        impl $crate::living_world::grammar::Prim for $name {
            const EXPECTED: &'static str = concat!("a ", $what);
            fn parse(
                args: &[String],
                _macros: &$crate::value::MacroTable,
            ) -> Option<Self> {
                Self::from_name(args.first()?)
            }
        }
    };
}
pub(crate) use index_enum;
