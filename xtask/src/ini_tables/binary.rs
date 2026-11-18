use super::pe::Image;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TableKey {
    Static(u32),
    Stack(u32, i32),
    Vocabulary(u32),
}

#[derive(Debug, Clone)]
pub(crate) struct Record {
    pub(crate) token: Option<String>,
    pub(crate) proc_va: u32,
    pub(crate) user_data: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct Table {
    pub(crate) key: TableKey,
    pub(crate) records: Vec<Record>,
}

impl Table {
    pub(crate) fn is_wildcard(&self) -> bool {
        self.records.first().is_some_and(|r| r.token.is_none())
    }

    pub(crate) fn tokens(&self) -> impl Iterator<Item = &str> {
        self.records.iter().filter_map(|r| r.token.as_deref())
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Refs {
    pub(crate) tables: Vec<usize>,
    pub(crate) calls: Vec<u32>,
    pub(crate) pushes: Vec<u32>,
    pub(crate) after_table: Vec<u32>,
}

pub(crate) struct Binary {
    pub(crate) image: Image,
    pub(crate) tables: Vec<Table>,
    by_address: BTreeMap<u32, usize>,
    stack_tables: BTreeMap<u32, Vec<usize>>,
    functions: Vec<u32>,
    text: (u32, u32),
    refs: BTreeMap<u32, Refs>,
    pub(crate) registrations: BTreeMap<String, u32>,
    pub(crate) modules: BTreeMap<String, u32>,
    pub(crate) vocabularies: BTreeMap<u32, Vec<String>>,
    add: Option<Option<u32>>,
    pub(crate) strings: BTreeSet<String>,
}

const MAX_FUNCTION_BYTES: usize = 8192;
const MAX_CHAIN_DEPTH: usize = 12;

fn is_token(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'%' | b'-' | b'.' | b':'))
}

fn is_identifier(text: &str) -> bool {
    is_token(text) && text.bytes().next().is_some_and(|b| b.is_ascii_alphabetic())
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    bytes
        .get(at..end)
        .and_then(|s| <[u8; 4]>::try_from(s).ok())
        .map(u32::from_le_bytes)
}

fn i32_at(bytes: &[u8], at: usize) -> Option<i32> {
    u32_at(bytes, at).map(|v| i32::from_le_bytes(v.to_le_bytes()))
}

impl Binary {
    pub(crate) fn recover(image: Image) -> Result<Self, String> {
        let text_section = image.section(".text").ok_or("no .text section")?;
        let text_va = image
            .image_base
            .saturating_add(text_section.virtual_address);
        let text = (text_va, text_va.saturating_add(text_section.virtual_size));
        let mut binary = Self {
            image,
            tables: Vec::new(),
            by_address: BTreeMap::new(),
            stack_tables: BTreeMap::new(),
            functions: Vec::new(),
            text,
            refs: BTreeMap::new(),
            registrations: BTreeMap::new(),
            modules: BTreeMap::new(),
            vocabularies: BTreeMap::new(),
            add: None,
            strings: BTreeSet::new(),
        };
        binary.find_functions();
        let stores = binary.absolute_stores();
        let nodes = binary.registration_nodes(&stores);
        binary.find_tables(&stores, &nodes);
        binary.find_stack_tables();
        binary.find_registrations(&stores, &nodes);
        binary.find_modules();
        binary.find_vocabularies();
        binary.find_strings();
        Ok(binary)
    }

    fn text_bytes(&self) -> (&[u8], u32) {
        match self.image.section(".text") {
            Some(s) => (
                self.image.raw_bytes(s),
                self.image.image_base.saturating_add(s.virtual_address),
            ),
            None => (&[], 0),
        }
    }

    pub(crate) fn in_text(&self, va: u32) -> bool {
        va >= self.text.0 && va < self.text.1
    }

    pub(crate) fn token_at(&self, va: u32) -> Option<String> {
        let rva = self.image.va_to_rva(va)?;
        let off = self.image.rva_to_file_offset(rva)?;
        let bytes = self.image.data.get(off..)?;
        let end = bytes.iter().take(65).position(|&b| b == 0)?;
        let text = std::str::from_utf8(bytes.get(..end)?).ok()?;
        is_token(text).then(|| text.to_owned())
    }

    fn read_u32(&self, va: u32) -> Option<u32> {
        self.image.read_u32(self.image.va_to_rva(va)?)
    }

    fn find_functions(&mut self) {
        let (bytes, base) = self.text_bytes();
        let mut starts = Vec::new();
        for (i, window) in bytes.windows(3).enumerate() {
            if window == [0x55, 0x8B, 0xEC] {
                starts.push(base.saturating_add(u32::try_from(i).unwrap_or(u32::MAX)));
            }
        }
        self.functions = starts;
    }

    fn function_of(&self, va: u32) -> Option<u32> {
        let at = self.functions.partition_point(|&f| f <= va);
        self.functions.get(at.checked_sub(1)?).copied()
    }

    fn function_end(&self, start: u32) -> u32 {
        let at = self.functions.partition_point(|&f| f <= start);
        self.functions.get(at).copied().unwrap_or(self.text.1)
    }

    fn absolute_stores(&self) -> Vec<(u32, u32)> {
        let (bytes, _) = self.text_bytes();
        let mut out = Vec::new();
        let mut i = 0usize;
        while let Some(pos) = bytes
            .get(i..)
            .and_then(|rest| rest.windows(2).position(|w| w == [0xC7, 0x05]))
        {
            let at = i.saturating_add(pos);
            if let (Some(target), Some(imm)) = (u32_at(bytes, at + 2), u32_at(bytes, at + 6)) {
                out.push((target, imm));
            }
            i = at.saturating_add(1);
        }
        out
    }

    fn registration_nodes(&self, stores: &[(u32, u32)]) -> BTreeSet<u32> {
        let first: BTreeMap<u32, u32> = stores.iter().rev().copied().collect();
        let mut per_head: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        for &(target, node) in stores {
            let (Some(name), Some(proc_va)) = (
                node.checked_add(4).and_then(|a| first.get(&a)),
                node.checked_add(8).and_then(|a| first.get(&a)),
            ) else {
                continue;
            };
            if self.token_at(*name).is_some() && self.in_text(*proc_va) {
                per_head.entry(target).or_default().insert(node);
            }
        }
        per_head
            .into_values()
            .max_by_key(BTreeSet::len)
            .unwrap_or_default()
    }

    fn find_tables(&mut self, stores: &[(u32, u32)], nodes: &BTreeSet<u32>) {
        let mut records: BTreeMap<u32, Record> = BTreeMap::new();
        let first: BTreeMap<u32, u32> = stores.iter().rev().copied().collect();
        for name in [".rdata", ".data"] {
            let Some(section) = self.image.section(name) else {
                continue;
            };
            let base = self
                .image
                .image_base
                .saturating_add(section.virtual_address);
            let bytes = self.image.raw_bytes(section);
            let mut at = 0usize;
            while at.saturating_add(16) <= bytes.len() {
                if let (Some(token), Some(proc_va), Some(user_data)) = (
                    u32_at(bytes, at),
                    u32_at(bytes, at + 4),
                    u32_at(bytes, at + 8),
                ) {
                    let va = base.saturating_add(u32::try_from(at).unwrap_or(u32::MAX));
                    let proc_va = match proc_va {
                        0 if token != 0 => va
                            .checked_add(4)
                            .and_then(|slot| first.get(&slot))
                            .copied()
                            .unwrap_or(0),
                        _ => proc_va,
                    };
                    if self.in_text(proc_va) {
                        let text = if token == 0 {
                            None
                        } else {
                            self.token_at(token)
                        };
                        if token == 0 || text.is_some() {
                            records.insert(
                                va,
                                Record {
                                    token: text,
                                    proc_va,
                                    user_data,
                                },
                            );
                        }
                    }
                }
                at = at.saturating_add(4);
            }
        }
        for (&target, &imm) in &first {
            if records.contains_key(&target)
                || target.checked_sub(4).is_some_and(|n| nodes.contains(&n))
            {
                continue;
            }
            let Some(token) = self.token_at(imm) else {
                continue;
            };
            let Some(&proc_va) = target.checked_add(4).and_then(|a| first.get(&a)) else {
                continue;
            };
            if !self.in_text(proc_va) {
                continue;
            }
            let user_data = target
                .checked_add(8)
                .and_then(|a| first.get(&a))
                .copied()
                .unwrap_or(0);
            records.insert(
                target,
                Record {
                    token: Some(token),
                    proc_va,
                    user_data,
                },
            );
        }
        let starts: Vec<u32> = records
            .iter()
            .filter(|(va, _)| {
                va.checked_sub(16)
                    .and_then(|prev| records.get(&prev))
                    .is_none_or(|prev| prev.token.is_none())
            })
            .map(|(va, _)| *va)
            .collect();
        for start in starts {
            let mut run: Vec<Record> = Vec::new();
            let mut va = start;
            while let Some(record) = records.get(&va) {
                if record.token.is_none() && !run.is_empty() {
                    break;
                }
                run.push(record.clone());
                va = va.saturating_add(16);
            }
            let terminated = self.read_u32(va) == Some(0);
            let named = run.iter().filter(|r| r.token.is_some()).count();
            let wildcard = run.first().is_some_and(|r| r.token.is_none());
            if named >= 1 || (terminated && wildcard) {
                self.by_address.insert(start, self.tables.len());
                self.tables.push(Table {
                    key: TableKey::Static(start),
                    records: run,
                });
            }
        }
    }

    fn find_stack_tables(&mut self) {
        let (bytes, base) = self.text_bytes();
        let mut per_function: BTreeMap<u32, BTreeMap<i32, u32>> = BTreeMap::new();
        let mut i = 0usize;
        while i.saturating_add(11) <= bytes.len() {
            let (disp, imm) = match (bytes.get(i), bytes.get(i + 1)) {
                (Some(0xC7), Some(0x45)) => (
                    bytes.get(i + 2).map(|&b| i32::from(i8::from_le_bytes([b]))),
                    u32_at(bytes, i + 3),
                ),
                (Some(0xC7), Some(0x85)) => (i32_at(bytes, i + 2), u32_at(bytes, i + 6)),
                _ => {
                    i += 1;
                    continue;
                }
            };
            if let (Some(disp), Some(imm)) = (disp, imm) {
                let va = base.saturating_add(u32::try_from(i).unwrap_or(u32::MAX));
                if let Some(function) = self.function_of(va) {
                    per_function
                        .entry(function)
                        .or_default()
                        .entry(disp)
                        .or_insert(imm);
                }
            }
            i += 1;
        }
        for (function, slots) in per_function {
            let mut records: BTreeMap<i32, Record> = BTreeMap::new();
            for (&disp, &imm) in &slots {
                let Some(token) = self.token_at(imm) else {
                    continue;
                };
                let Some(&proc_va) = disp.checked_add(4).and_then(|d| slots.get(&d)) else {
                    continue;
                };
                if !self.in_text(proc_va) {
                    continue;
                }
                let user_data = disp
                    .checked_add(8)
                    .and_then(|d| slots.get(&d))
                    .copied()
                    .unwrap_or(0);
                records.insert(
                    disp,
                    Record {
                        token: Some(token),
                        proc_va,
                        user_data,
                    },
                );
            }
            let starts: Vec<i32> = records
                .keys()
                .filter(|d| d.checked_sub(16).is_none_or(|p| !records.contains_key(&p)))
                .copied()
                .collect();
            for start in starts {
                let mut run = Vec::new();
                let mut disp = start;
                while let Some(record) = records.get(&disp) {
                    run.push(record.clone());
                    let Some(next) = disp.checked_add(16) else {
                        break;
                    };
                    disp = next;
                }
                if run.len() >= 2 {
                    self.stack_tables
                        .entry(function)
                        .or_default()
                        .push(self.tables.len());
                    self.tables.push(Table {
                        key: TableKey::Stack(function, start),
                        records: run,
                    });
                }
            }
        }
    }

    fn find_registrations(&mut self, stores: &[(u32, u32)], nodes: &BTreeSet<u32>) {
        let first: BTreeMap<u32, u32> = stores.iter().rev().copied().collect();
        for &node in nodes {
            let (Some(&name), Some(&proc_va)) = (
                node.checked_add(4).and_then(|a| first.get(&a)),
                node.checked_add(8).and_then(|a| first.get(&a)),
            ) else {
                continue;
            };
            if let Some(name) = self.token_at(name) {
                self.registrations.insert(name, proc_va);
            }
        }
    }

    fn find_modules(&mut self) {
        let (bytes, base) = self.text_bytes();
        let mut by_target: BTreeMap<u32, Vec<(Option<String>, u32)>> = BTreeMap::new();
        let mut i = 0usize;
        while i.saturating_add(24) <= bytes.len() {
            if bytes.get(i) != Some(&0x68) || bytes.get(i + 5) != Some(&0x68) {
                i += 1;
                continue;
            }
            let (Some(data_proc), Some(new_proc)) = (u32_at(bytes, i + 1), u32_at(bytes, i + 6))
            else {
                i += 1;
                continue;
            };
            if !self.in_text(data_proc) || !self.in_text(new_proc) {
                i += 1;
                continue;
            }
            let mut j = i + 10;
            match (bytes.get(j), bytes.get(j + 1)) {
                (Some(0x8B), Some(0x8D)) => j += 6,
                (Some(0x8B), Some(0x4D)) => j += 3,
                _ => {}
            }
            if bytes.get(j) == Some(&0xE8) {
                if let Some(rel) = i32_at(bytes, j + 1) {
                    let call_va = base.saturating_add(u32::try_from(j).unwrap_or(u32::MAX));
                    let target = call_va.wrapping_add(5).wrapping_add_signed(rel);
                    let name = (1..90usize)
                        .filter_map(|back| i.checked_sub(back))
                        .filter(|&k| bytes.get(k) == Some(&0x68))
                        .find_map(|k| {
                            u32_at(bytes, k + 1)
                                .and_then(|p| self.token_at(p))
                                .filter(|s| is_identifier(s))
                        });
                    by_target.entry(target).or_default().push((name, data_proc));
                }
            }
            i += 1;
        }
        let Some(registrations) = by_target.into_values().max_by_key(Vec::len) else {
            return;
        };
        for (name, data_proc) in registrations {
            if let Some(name) = name {
                self.modules.entry(name).or_insert(data_proc);
            }
        }
    }

    fn find_vocabularies(&mut self) {
        let Some(section) = self.image.section(".rdata") else {
            return;
        };
        let base = self
            .image
            .image_base
            .saturating_add(section.virtual_address);
        let bytes = self.image.raw_bytes(section).to_vec();
        let mut at = 0usize;
        while at.saturating_add(4) <= bytes.len() {
            let mut names = Vec::new();
            let mut pointers = 0usize;
            let mut k = at;
            while let Some(text) = u32_at(&bytes, k)
                .and_then(|p| self.image.va_to_rva(p))
                .and_then(|rva| self.image.read_cstr(rva, 128))
            {
                pointers += 1;
                if is_identifier(&text) {
                    names.push(text);
                }
                k = k.saturating_add(4);
                if pointers >= 256 {
                    break;
                }
            }
            if pointers >= 3 && names.len() >= 3 {
                let va = base.saturating_add(u32::try_from(at).unwrap_or(u32::MAX));
                self.vocabularies.insert(va, names);
                at = k;
            } else {
                at = at.saturating_add(4);
            }
        }
    }

    fn find_strings(&mut self) {
        for name in [".rdata", ".data"] {
            let Some(section) = self.image.section(name) else {
                continue;
            };
            for run in self.image.raw_bytes(section).split(|&b| b == 0) {
                if let Ok(text) = std::str::from_utf8(run) {
                    if is_token(text) {
                        self.strings.insert(text.to_owned());
                    }
                }
            }
        }
    }

    pub(crate) fn table_at(&self, va: u32) -> Option<usize> {
        self.by_address.get(&va).copied()
    }

    pub(crate) fn refs(&mut self, start: u32) -> Refs {
        if let Some(cached) = self.refs.get(&start) {
            return cached.clone();
        }
        let (bytes, base) = self.text_bytes();
        let mut out = Refs::default();
        let from = usize::try_from(start.saturating_sub(base)).unwrap_or(usize::MAX);
        let to = usize::try_from(self.function_end(start).saturating_sub(base))
            .unwrap_or(usize::MAX)
            .min(from.saturating_add(MAX_FUNCTION_BYTES))
            .min(bytes.len());
        let mut j = from;
        let mut pending = false;
        while j < to {
            if let Some(v) = u32_at(bytes, j) {
                if let Some(&table) = self.by_address.get(&v) {
                    pending = true;
                    if !out.tables.contains(&table) {
                        out.tables.push(table);
                    }
                }
                let at = base.saturating_add(u32::try_from(j).unwrap_or(u32::MAX));
                match bytes.get(j) {
                    Some(0xE8) => {
                        if let Some(rel) = i32_at(bytes, j + 1) {
                            let target = at.wrapping_add(5).wrapping_add_signed(rel);
                            if self.in_text(target) {
                                if pending {
                                    out.after_table.push(target);
                                    pending = false;
                                }
                                if !out.calls.contains(&target) {
                                    out.calls.push(target);
                                }
                            }
                        }
                    }
                    Some(0x68) => {
                        let seh = j >= 2
                            && bytes.get(j - 2) == Some(&0x6A)
                            && bytes.get(j - 1) == Some(&0xFF);
                        if let Some(pushed) = u32_at(bytes, j + 1) {
                            if !seh && self.in_text(pushed) {
                                out.pushes.push(pushed);
                            }
                        }
                    }
                    _ => {}
                }
            }
            j += 1;
        }
        for &table in self.stack_tables.get(&start).into_iter().flatten() {
            if !out.tables.contains(&table) {
                out.tables.push(table);
            }
        }
        self.refs.insert(start, out.clone());
        out
    }

    pub(crate) fn reach(&mut self, start: u32) -> (Vec<usize>, Vec<usize>) {
        let own = self.refs(start);
        let mut callee = Vec::new();
        for call in own.calls {
            for t in self.refs(call).tables {
                if !own.tables.contains(&t) && !callee.contains(&t) {
                    callee.push(t);
                }
            }
        }
        (own.tables, callee)
    }

    pub(crate) fn builder_tables(&mut self, proc_va: u32) -> Option<Vec<usize>> {
        let add = self.multi_field_add();
        let builders = self.refs(proc_va).pushes;
        if builders.is_empty() {
            return None;
        }
        let mut out = Vec::new();
        for build in builders {
            let mut seen = BTreeSet::new();
            self.chain(build, add, 0, &mut seen, &mut out);
        }
        Some(out)
    }

    fn multi_field_add(&mut self) -> Option<u32> {
        if let Some(add) = self.add {
            return add;
        }
        let procs: Vec<u32> = self.modules.values().copied().collect();
        let mut votes: BTreeMap<u32, usize> = BTreeMap::new();
        for data_proc in procs {
            for build in self.refs(data_proc).pushes {
                if let Some(&first) = self.refs(build).after_table.first() {
                    *votes.entry(first).or_default() += 1;
                }
            }
        }
        let add = votes
            .into_iter()
            .max_by_key(|&(t, n)| (n, t))
            .map(|(t, _)| t);
        self.add = Some(add);
        add
    }

    fn chain(
        &mut self,
        function: u32,
        add: Option<u32>,
        depth: usize,
        seen: &mut BTreeSet<u32>,
        out: &mut Vec<usize>,
    ) {
        if depth > MAX_CHAIN_DEPTH || !seen.insert(function) {
            return;
        }
        let refs = self.refs(function);
        for call in refs.calls.into_iter().filter(|&c| Some(c) != add) {
            self.chain(call, add, depth + 1, seen, out);
        }
        for t in refs.tables {
            if !out.contains(&t) {
                out.push(t);
            }
        }
    }
}
