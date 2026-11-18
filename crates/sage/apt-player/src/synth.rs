#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::unwrap_used,
    clippy::expect_used,
    missing_docs,
    unreachable_pub,
    dead_code
)]

const SIG: u32 = 0x0987_6543;
const MAGIC: &[u8] = b"Apt constant file\x1a\0\0";

#[derive(Clone, Debug, Default)]
pub struct P {
    pub(crate) move_: bool,
    pub(crate) depth: i32,
    pub(crate) character: Option<u32>,
    pub(crate) matrix: Option<[f32; 6]>,
    pub(crate) cx: Option<(u32, u32)>,
    pub(crate) ratio: Option<f32>,
    pub(crate) name: Option<String>,
    pub(crate) clip_depth: Option<i32>,
    pub(crate) clip_actions: Option<Vec<(u32, u32)>>,
}

impl P {
    pub fn at(mut self, tx: f32, ty: f32) -> Self {
        self.matrix = Some([1.0, 0.0, 0.0, 1.0, tx, ty]);
        self
    }
    pub(crate) fn matrix(mut self, m: [f32; 6]) -> Self {
        self.matrix = Some(m);
        self
    }
    pub(crate) fn cx(mut self, mult: u32, add: u32) -> Self {
        self.cx = Some((mult, add));
        self
    }
    pub(crate) fn ratio(mut self, r: f32) -> Self {
        self.ratio = Some(r);
        self
    }
    pub fn name(mut self, n: &str) -> Self {
        self.name = Some(n.to_owned());
        self
    }
    pub(crate) fn clip_depth(mut self, d: i32) -> Self {
        self.clip_depth = Some(d);
        self
    }
    pub(crate) fn events(mut self, handlers: Vec<(u32, u32)>) -> Self {
        self.clip_actions = Some(handlers);
        self
    }
}

pub fn place(depth: i32, character: u32) -> P {
    P {
        depth,
        character: Some(character),
        ..P::default()
    }
}
pub(crate) fn replace(depth: i32, character: u32) -> P {
    P {
        move_: true,
        depth,
        character: Some(character),
        ..P::default()
    }
}
pub fn modify(depth: i32) -> P {
    P {
        move_: true,
        depth,
        ..P::default()
    }
}

#[derive(Clone, Debug)]
pub enum Item {
    Place(P),
    Remove(i32),
    Action(u32),
    Label(&'static str),
    Background(u32),
    Init(u32),
}

pub type Frames = Vec<Vec<Item>>;

enum Def {
    Shape,
    ShapeBox([f32; 4]),
    Sprite(Frames),
    Button {
        records: Vec<(u32, u32, i32, f32, f32)>,
        actions: Vec<(u32, u32)>,
        mesh: Vec<[f32; 2]>,
    },
    EditText(String, Option<String>),
    Morph,
    Import,
}

#[derive(Default)]
pub struct Synth {
    defs: Vec<Def>,
    imports: Vec<(String, String, u32)>,
    exports: Vec<(String, u32)>,
    blocks: Vec<Asm>,
    consts: Vec<Konst>,
}

#[derive(Clone, Debug)]
enum Konst {
    Text(String),
    Number(u32),
}

#[derive(Clone, Debug)]
enum Data {
    Str(String),
    Words(Vec<u32>),
    Args(Vec<(u32, String)>, bool),
}

#[derive(Clone, Debug, Default)]
pub struct Asm {
    code: Vec<u8>,
    relocs: Vec<(usize, Data)>,
    labels: Vec<Option<usize>>,
    fixups: Vec<(usize, usize)>,
}

impl Asm {
    pub fn new() -> Self {
        Self::default()
    }
    fn byte(&mut self, b: u8) -> &mut Self {
        self.code.push(b);
        self
    }
    fn pad(&mut self) {
        while self.code.len() % 4 != 0 {
            self.code.push(0);
        }
    }
    fn word(&mut self, v: u32) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }
    fn reloc(&mut self, data: Data) {
        let at = self.code.len();
        self.relocs.push((at, data));
        self.word(0);
    }
    pub fn op(&mut self, op: u8) -> &mut Self {
        self.byte(op)
    }
    fn aligned_str(&mut self, op: u8, s: &str) -> &mut Self {
        self.byte(op);
        self.pad();
        self.reloc(Data::Str(s.to_owned()));
        self
    }
    pub fn push_str(&mut self, s: &str) -> &mut Self {
        self.aligned_str(0xA1, s)
    }
    pub fn get_var(&mut self, s: &str) -> &mut Self {
        self.aligned_str(0xA4, s)
    }
    pub(crate) fn set_target(&mut self, s: &str) -> &mut Self {
        self.aligned_str(0x8B, s)
    }
    fn byte_op(&mut self, op: u8, v: u8) -> &mut Self {
        self.byte(op);
        self.byte(v)
    }
    pub fn push_byte(&mut self, v: u8) -> &mut Self {
        self.byte_op(0xB5, v)
    }
    pub(crate) fn push_reg(&mut self, r: u8) -> &mut Self {
        self.byte_op(0xB9, r)
    }
    pub(crate) fn call_named_fn(&mut self, i: u8) -> &mut Self {
        self.byte_op(0xB1, i)
    }
    pub fn push_float(&mut self, v: f32) -> &mut Self {
        self.byte(0xB4);
        self.word(v.to_bits());
        self
    }
    fn aligned_word(&mut self, op: u8, v: u32) -> &mut Self {
        self.byte(op);
        self.pad();
        self.word(v);
        self
    }
    pub(crate) fn goto_frame(&mut self, zero_based: i32) -> &mut Self {
        self.aligned_word(0x81, zero_based as u32)
    }
    pub(crate) fn set_register(&mut self, r: u32) -> &mut Self {
        self.aligned_word(0x87, r)
    }
    pub(crate) fn constant_pool(&mut self, items: &[u32]) -> &mut Self {
        self.byte(0x88);
        self.pad();
        self.word(items.len() as u32);
        self.reloc(Data::Words(items.to_vec()));
        self
    }
    pub(crate) fn push_data(&mut self, items: &[u32]) -> &mut Self {
        self.byte(0x96);
        self.pad();
        self.word(items.len() as u32);
        self.reloc(Data::Words(items.to_vec()));
        self
    }
    pub fn get_url(&mut self, url: &str, target: &str) -> &mut Self {
        self.byte(0x83);
        self.pad();
        self.reloc(Data::Str(url.to_owned()));
        self.reloc(Data::Str(target.to_owned()));
        self
    }
    pub fn label(&mut self) -> usize {
        self.labels.push(None);
        self.labels.len() - 1
    }
    pub fn bind(&mut self, label: usize) -> &mut Self {
        self.labels[label] = Some(self.code.len());
        self
    }
    pub fn branch(&mut self, op: u8, label: usize) -> &mut Self {
        self.byte(op);
        self.pad();
        let at = self.code.len();
        self.word(0);
        self.fixups.push((at, label));
        self
    }
    pub fn function(
        &mut self,
        name: &str,
        params: &[&str],
        body: impl FnOnce(&mut Self),
    ) -> &mut Self {
        self.byte(0x9B);
        self.pad();
        self.reloc(Data::Str(name.to_owned()));
        self.word(params.len() as u32);
        self.reloc(Data::Args(
            params.iter().map(|p| (0, (*p).to_owned())).collect(),
            false,
        ));
        self.body(body)
    }
    pub fn function2(
        &mut self,
        name: &str,
        flags: u32,
        params: &[(u32, &str)],
        body: impl FnOnce(&mut Self),
    ) -> &mut Self {
        self.byte(0x8E);
        self.pad();
        self.reloc(Data::Str(name.to_owned()));
        self.word(params.len() as u32);
        self.word(flags);
        self.reloc(Data::Args(
            params.iter().map(|(r, p)| (*r, (*p).to_owned())).collect(),
            true,
        ));
        self.body(body)
    }
    fn body(&mut self, body: impl FnOnce(&mut Self)) -> &mut Self {
        let size_at = self.code.len();
        self.word(0);
        self.word(0x9876_5432);
        self.word(0x1234_5678);
        let start = self.code.len();
        body(self);
        let size = (self.code.len() - start) as u32;
        self.code[size_at..size_at + 4].copy_from_slice(&size.to_le_bytes());
        self
    }
    pub fn end(&mut self) -> &mut Self {
        self.byte(0x00)
    }
    fn finish(mut self) -> Self {
        for (at, label) in std::mem::take(&mut self.fixups) {
            let target = self.labels[label].expect("bound label") as i64;
            let delta = (target - (at as i64 + 4)) as i32;
            self.code[at..at + 4].copy_from_slice(&delta.to_le_bytes());
        }
        self
    }
}

pub mod op {
    pub const STOP: u8 = 0x07;
    pub const NOT: u8 = 0x12;
    pub const POP: u8 = 0x17;
    pub const SET_VARIABLE: u8 = 0x1D;
    pub const GET_PROPERTY: u8 = 0x22;
    pub const SET_PROPERTY: u8 = 0x23;
    pub const TRACE: u8 = 0x26;
    pub const CALL_FUNCTION: u8 = 0x3D;
    pub const RETURN: u8 = 0x3E;
    pub const NEW: u8 = 0x40;
    pub const NEW_LESS: u8 = 0x48;
    pub const GET_MEMBER: u8 = 0x4E;
    pub const SET_MEMBER: u8 = 0x4F;
    pub const INCREMENT: u8 = 0x50;
    pub const CALL_METHOD: u8 = 0x52;
    pub const PUSH_THIS: u8 = 0x70;
    pub const PUSH_GLOBAL: u8 = 0x71;
    pub const BRANCH: u8 = 0x99;
    pub const BRANCH_IF_TRUE: u8 = 0x9D;
}

impl Synth {
    pub fn new() -> Self {
        Self::default()
    }
    fn push(&mut self, def: Def) -> u32 {
        self.defs.push(def);
        self.defs.len() as u32
    }
    pub(crate) fn shape(&mut self) -> u32 {
        self.push(Def::Shape)
    }
    pub fn shape_box(&mut self, r: [f32; 4]) -> u32 {
        self.push(Def::ShapeBox(r))
    }
    pub(crate) fn morph(&mut self) -> u32 {
        self.push(Def::Morph)
    }
    pub fn sprite(&mut self, frames: Frames) -> u32 {
        self.push(Def::Sprite(frames))
    }
    pub(crate) fn button(
        &mut self,
        records: Vec<(u32, u32, i32, f32, f32)>,
        actions: Vec<(u32, u32)>,
    ) -> u32 {
        self.push(Def::Button {
            records,
            actions,
            mesh: Vec::new(),
        })
    }
    pub fn button_with_mesh(
        &mut self,
        records: Vec<(u32, u32, i32, f32, f32)>,
        actions: Vec<(u32, u32)>,
        mesh: Vec<[f32; 2]>,
    ) -> u32 {
        self.push(Def::Button {
            records,
            actions,
            mesh,
        })
    }
    pub fn edit_text(&mut self, text: &str) -> u32 {
        self.push(Def::EditText(text.to_owned(), None))
    }
    pub fn bound_text(&mut self, text: &str, variable: &str) -> u32 {
        self.push(Def::EditText(text.to_owned(), Some(variable.to_owned())))
    }
    pub fn text(&mut self, s: &str) -> u32 {
        self.consts.push(Konst::Text(s.to_owned()));
        (self.consts.len() - 1) as u32
    }
    pub fn number(&mut self, n: u32) -> u32 {
        self.consts.push(Konst::Number(n));
        (self.consts.len() - 1) as u32
    }
    pub fn code(&mut self, asm: &mut Asm) -> u32 {
        let asm = std::mem::take(asm).finish();
        let mut at = 16usize;
        for b in &self.blocks {
            at += b.code.len().next_multiple_of(4);
        }
        self.blocks.push(asm);
        at as u32
    }
    pub(crate) fn import(&mut self, movie: &str, name: &str) -> u32 {
        let id = self.push(Def::Import);
        self.imports.push((movie.to_owned(), name.to_owned(), id));
        id
    }
    pub fn export(&mut self, name: &str, id: u32) {
        self.exports.push((name.to_owned(), id));
    }

    pub fn build(&self, root: &Frames) -> (Vec<u8>, Vec<u8>) {
        let mut w = W { b: vec![0; 16] };
        let mut placed = Vec::new();
        for block in &self.blocks {
            let at = w.alloc(block.code.len());
            w.b[at as usize..at as usize + block.code.len()].copy_from_slice(&block.code);
            placed.push(at);
        }
        for (block, base) in self.blocks.iter().zip(&placed) {
            for (pos, data) in &block.relocs {
                let ptr = match data {
                    Data::Str(s) => w.cstr(s),
                    Data::Words(words) => {
                        let at = w.alloc(4 * words.len().max(1));
                        for (i, v) in words.iter().enumerate() {
                            w.u32(at + 4 * i as u32, *v);
                        }
                        at
                    }
                    Data::Args(args, fn2) => {
                        let names: Vec<u32> = args.iter().map(|(_, n)| w.cstr(n)).collect();
                        let stride = if *fn2 { 8 } else { 4 };
                        let at = w.alloc(stride * args.len().max(1));
                        for (i, ((reg, _), name)) in args.iter().zip(&names).enumerate() {
                            let e = at + (stride * i) as u32;
                            if *fn2 {
                                w.u32(e, *reg);
                                w.u32(e + 4, *name);
                            } else {
                                w.u32(e, *name);
                            }
                        }
                        at
                    }
                };
                w.u32(base + *pos as u32, ptr);
            }
        }
        let root_frames = w.frames(root);
        let mut ptrs = Vec::new();
        for def in &self.defs {
            let ptr = match def {
                Def::Shape => {
                    let at = w.alloc(28);
                    w.u32(at, 1);
                    w.u32(at + 4, SIG);
                    w.u32(at + 20, 10);
                    w.u32(at + 24, 0);
                    at
                }
                Def::ShapeBox(r) => {
                    let at = w.alloc(28);
                    w.u32(at, 1);
                    w.u32(at + 4, SIG);
                    for (i, v) in r.iter().enumerate() {
                        w.f32(at + 8 + 4 * i as u32, *v);
                    }
                    at
                }
                Def::Morph => {
                    let at = w.alloc(16);
                    w.u32(at, 8);
                    w.u32(at + 4, SIG);
                    at
                }
                Def::Sprite(frames) => {
                    let offset = w.frames(frames);
                    let at = w.alloc(20);
                    w.u32(at, 5);
                    w.u32(at + 4, SIG);
                    w.u32(at + 8, frames.len() as u32);
                    w.u32(at + 12, offset);
                    at
                }
                Def::Button {
                    records,
                    actions,
                    mesh,
                } => {
                    let verts = w.alloc(8 * mesh.len().max(1));
                    for (i, v) in mesh.iter().enumerate() {
                        w.f32(verts + 8 * i as u32, v[0]);
                        w.f32(verts + 8 * i as u32 + 4, v[1]);
                    }
                    let recs = w.alloc(68 * records.len().max(1));
                    for (i, (flags, ch, depth, tx, ty)) in records.iter().enumerate() {
                        let r = recs + 68 * i as u32;
                        w.u32(r, *flags);
                        w.u32(r + 4, *ch);
                        w.u32(r + 8, *depth as u32);
                        w.f32(r + 12, 1.0);
                        w.f32(r + 24, 1.0);
                        w.f32(r + 28, *tx);
                        w.f32(r + 32, *ty);
                    }
                    let acts = w.alloc(8 * actions.len().max(1));
                    for (i, (cond, block)) in actions.iter().enumerate() {
                        w.u32(acts + 8 * i as u32, *cond);
                        w.u32(acts + 8 * i as u32 + 4, *block);
                    }
                    let at = w.alloc(60);
                    w.u32(at, 4);
                    w.u32(at + 4, SIG);
                    w.u32(at + 32, mesh.len() as u32);
                    w.u32(at + 36, verts);
                    w.u32(at + 44, records.len() as u32);
                    w.u32(at + 48, recs);
                    w.u32(at + 52, actions.len() as u32);
                    w.u32(at + 56, acts);
                    at
                }
                Def::EditText(text, variable) => {
                    let s = w.cstr(text);
                    let v = variable.as_deref().map(|v| w.cstr(v));
                    let at = w.alloc(60);
                    w.u32(at, 2);
                    w.u32(at + 4, SIG);
                    w.u32(at + 52, s);
                    if let Some(v) = v {
                        w.u32(at + 56, v);
                    }
                    at
                }
                Def::Import => 0,
            };
            ptrs.push(ptr);
        }
        let imports = w.alloc(16 * self.imports.len().max(1));
        for (i, (movie, name, id)) in self.imports.iter().enumerate() {
            let m = w.cstr(movie);
            let n = w.cstr(name);
            let at = imports + 16 * i as u32;
            w.u32(at, m);
            w.u32(at + 4, n);
            w.u32(at + 8, *id);
        }
        let exports = w.alloc(8 * self.exports.len().max(1));
        for (i, (name, id)) in self.exports.iter().enumerate() {
            let n = w.cstr(name);
            w.u32(exports + 8 * i as u32, n);
            w.u32(exports + 8 * i as u32 + 4, *id);
        }
        let table = w.alloc(4 * (ptrs.len() + 1));
        for (i, p) in ptrs.iter().enumerate() {
            w.u32(table + 4 * (i as u32 + 1), *p);
        }
        let movie = w.alloc(56);
        w.u32(movie, 9);
        w.u32(movie + 4, SIG);
        w.u32(movie + 8, root.len() as u32);
        w.u32(movie + 12, root_frames);
        w.u32(movie + 20, ptrs.len() as u32 + 1);
        w.u32(movie + 24, table);
        w.u32(movie + 28, 1024);
        w.u32(movie + 32, 768);
        w.u32(movie + 36, 33);
        w.u32(movie + 40, self.imports.len() as u32);
        w.u32(movie + 44, imports);
        w.u32(movie + 48, self.exports.len() as u32);
        w.u32(movie + 52, exports);
        w.u32(table, movie);

        let mut c = MAGIC.to_vec();
        c.extend_from_slice(&movie.to_le_bytes());
        c.extend_from_slice(&(self.consts.len() as u32).to_le_bytes());
        c.extend_from_slice(&0x20u32.to_le_bytes());
        let strings_at = 0x20 + 8 * self.consts.len();
        let mut strings = Vec::new();
        for k in &self.consts {
            match k {
                Konst::Text(s) => {
                    c.extend_from_slice(&1u32.to_le_bytes());
                    c.extend_from_slice(&((strings_at + strings.len()) as u32).to_le_bytes());
                    strings.extend_from_slice(s.as_bytes());
                    strings.push(0);
                }
                Konst::Number(n) => {
                    c.extend_from_slice(&4u32.to_le_bytes());
                    c.extend_from_slice(&n.to_le_bytes());
                }
            }
        }
        c.extend_from_slice(&strings);
        (c, w.b)
    }
}

struct W {
    b: Vec<u8>,
}

impl W {
    fn alloc(&mut self, n: usize) -> u32 {
        while self.b.len() % 4 != 0 {
            self.b.push(0);
        }
        let at = self.b.len() as u32;
        self.b.resize(self.b.len() + n, 0);
        at
    }
    fn u32(&mut self, at: u32, v: u32) {
        let at = at as usize;
        self.b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, at: u32, v: f32) {
        self.u32(at, v.to_bits());
    }
    fn cstr(&mut self, s: &str) -> u32 {
        let at = self.alloc(s.len() + 1);
        self.b[at as usize..at as usize + s.len()].copy_from_slice(s.as_bytes());
        at
    }
    fn item(&mut self, item: &Item) -> u32 {
        match item {
            Item::Action(block) => {
                let at = self.alloc(8);
                self.u32(at, 1);
                self.u32(at + 4, *block);
                at
            }
            Item::Label(label) => {
                let s = self.cstr(label);
                let at = self.alloc(16);
                self.u32(at, 2);
                self.u32(at + 4, s);
                self.u32(at + 8, 0x7_0000);
                at
            }
            Item::Remove(depth) => {
                let at = self.alloc(8);
                self.u32(at, 4);
                self.u32(at + 4, *depth as u32);
                at
            }
            Item::Background(c) => {
                let at = self.alloc(8);
                self.u32(at, 5);
                self.u32(at + 4, *c);
                at
            }
            Item::Init(block) => {
                let at = self.alloc(12);
                self.u32(at, 8);
                self.u32(at + 8, *block);
                at
            }
            Item::Place(p) => {
                let name = p.name.as_deref().map(|n| self.cstr(n));
                let actions = p.clip_actions.as_ref().map(|handlers| {
                    let array = self.alloc(12 * handlers.len().max(1));
                    for (i, (events, block)) in handlers.iter().enumerate() {
                        self.u32(array + 12 * i as u32, *events);
                        self.u32(array + 12 * i as u32 + 8, *block);
                    }
                    let rec = self.alloc(8);
                    self.u32(rec, handlers.len() as u32);
                    self.u32(rec + 4, array);
                    rec
                });
                let at = self.alloc(64);
                let mut flags = 0u32;
                self.u32(at, 3);
                self.u32(at + 8, p.depth as u32);
                if p.move_ {
                    flags |= 0x01;
                }
                if let Some(ch) = p.character {
                    flags |= 0x02;
                    self.u32(at + 12, ch);
                }
                if let Some(m) = p.matrix {
                    flags |= 0x04;
                    for (i, v) in m.iter().enumerate() {
                        self.f32(at + 16 + 4 * i as u32, *v);
                    }
                }
                if let Some((mult, add)) = p.cx {
                    flags |= 0x08;
                    self.u32(at + 40, mult);
                    self.u32(at + 44, add);
                }
                if let Some(r) = p.ratio {
                    flags |= 0x10;
                    self.f32(at + 48, r);
                }
                if let Some(n) = name {
                    flags |= 0x20;
                    self.u32(at + 52, n);
                }
                if let Some(d) = p.clip_depth {
                    flags |= 0x40;
                    self.u32(at + 56, d as u32);
                }
                if let Some(rec) = actions {
                    flags |= 0x80;
                    self.u32(at + 60, rec);
                }
                self.u32(at + 4, flags);
                at
            }
        }
    }
    fn frames(&mut self, frames: &Frames) -> u32 {
        let mut lists = Vec::new();
        for frame in frames {
            let ptrs: Vec<u32> = frame.iter().map(|i| self.item(i)).collect();
            let arr = self.alloc(4 * ptrs.len().max(1));
            for (i, p) in ptrs.iter().enumerate() {
                self.u32(arr + 4 * i as u32, *p);
            }
            lists.push((ptrs.len() as u32, arr));
        }
        let at = self.alloc(8 * frames.len().max(1));
        for (i, (n, arr)) in lists.iter().enumerate() {
            self.u32(at + 8 * i as u32, *n);
            self.u32(at + 8 * i as u32 + 4, *arr);
        }
        at
    }
}
