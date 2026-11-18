use super::binary::{Binary, Table};
use super::bind::{bind, How, Target, TitleBinding};
use super::corpus::{Corpus, Title};
use super::pe::Image;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const COUNTERPART_OVERLAP: f64 = 0.8;

const OUTPUT: &str = "crates/sage/ini/src/tables/generated.rs";

struct Bound {
    title: Title,
    binary: Binary,
    binding: TitleBinding,
}

fn mask(title: Title) -> u8 {
    match title {
        Title::Bfme2 => 1,
        Title::Rotwk => 2,
    }
}

fn mask_name(mask: u8) -> &'static str {
    match mask {
        1 => "B2",
        2 => "RK",
        3 => "BOTH",
        _ => "NONE",
    }
}

fn how_name(how: Option<How>) -> &'static str {
    match how {
        Some(How::RegistrationProc) => "Bound::RegistrationProc",
        Some(How::RegistrationCallee) => "Bound::RegistrationCallee",
        Some(How::ModuleFactory) => "Bound::ModuleFactory",
        Some(How::UserData) => "Bound::UserData",
        Some(How::FieldProc) => "Bound::FieldProc",
        Some(How::FieldProcCallee) => "Bound::FieldProcCallee",
        Some(How::Vocabulary) => "Bound::Vocabulary",
        Some(How::Vote) => "Bound::Vote",
        Some(How::Counterpart) => "Bound::Counterpart",
        None => "Bound::Absent",
    }
}

#[derive(Default, Clone)]
struct MergedTable {
    fields: Vec<(String, u8)>,
    wildcard: u8,
    titles: u8,
    how: [Option<How>; 2],
}

#[derive(Clone)]
enum MergedTarget {
    Fixed(String),
    Typed(BTreeMap<String, String>),
    Module,
}

#[derive(Default, Clone)]
struct MergedContext {
    tables: Vec<MergedTable>,
    children: BTreeMap<String, (u8, MergedTarget)>,
}

fn table_tokens(table: &Table) -> Vec<String> {
    let mut seen = BTreeSet::new();
    table
        .tokens()
        .filter(|t| seen.insert(*t))
        .map(str::to_owned)
        .collect()
}

fn overlap(a: &[String], b: &[String]) -> f64 {
    let a: BTreeSet<&String> = a.iter().collect();
    let b: BTreeSet<&String> = b.iter().collect();
    let union = a.union(&b).count();
    if union == 0 {
        return 1.0;
    }
    a.intersection(&b).count() as f64 / union as f64
}

type Merged = (
    BTreeMap<String, MergedContext>,
    BTreeMap<String, String>,
    BTreeMap<String, String>,
);

fn merge(bound: &[Bound]) -> Merged {
    let mut contexts: BTreeMap<String, MergedContext> = BTreeMap::new();
    let mut file_scope = BTreeMap::new();
    let mut modules = BTreeMap::new();
    for b in bound {
        let m = mask(b.title);
        let slot = usize::from(m == 2);
        file_scope.extend(b.binding.file_scope.clone());
        modules.extend(b.binding.modules.clone());
        for (key, ctx) in &b.binding.contexts {
            let merged = contexts.entry(key.clone()).or_default();
            for &(t, how) in &ctx.bound.tables {
                let Some(table) = b.binary.tables.get(t) else {
                    continue;
                };
                let tokens = table_tokens(table);
                let wildcard = table.is_wildcard();
                let partner = merged
                    .tables
                    .iter_mut()
                    .filter(|mt| mt.titles & m == 0)
                    .map(|mt| {
                        let theirs: Vec<String> =
                            mt.fields.iter().map(|(f, _)| f.clone()).collect();
                        (overlap(&theirs, &tokens), mt)
                    })
                    .filter(|(score, _)| *score >= 0.5)
                    .max_by(|x, y| x.0.total_cmp(&y.0))
                    .map(|(_, mt)| mt);
                match partner {
                    Some(mt) => {
                        for token in &tokens {
                            match mt.fields.iter_mut().find(|(f, _)| f == token) {
                                Some((_, fm)) => *fm |= m,
                                None => mt.fields.push((token.clone(), m)),
                            }
                        }
                        mt.titles |= m;
                        if wildcard {
                            mt.wildcard |= m;
                        }
                        if let Some(h) = mt.how.get_mut(slot) {
                            *h = Some(how);
                        }
                    }
                    None => {
                        let mut mt = MergedTable {
                            fields: tokens.into_iter().map(|f| (f, m)).collect(),
                            wildcard: if wildcard { m } else { 0 },
                            titles: m,
                            how: [None, None],
                        };
                        if let Some(h) = mt.how.get_mut(slot) {
                            *h = Some(how);
                        }
                        merged.tables.push(mt);
                    }
                }
            }
            for (keyword, target) in &ctx.children {
                let target = match target {
                    Target::Fixed(k) => MergedTarget::Fixed(k.clone()),
                    Target::Typed(map) => MergedTarget::Typed(map.clone()),
                    Target::Module => MergedTarget::Module,
                };
                match merged.children.get_mut(keyword) {
                    Some((cm, existing)) => {
                        *cm |= m;
                        if let (MergedTarget::Typed(a), MergedTarget::Typed(b)) =
                            (existing, &target)
                        {
                            a.extend(b.clone());
                        }
                    }
                    None => {
                        merged.children.insert(keyword.clone(), (m, target));
                    }
                }
            }
        }
    }
    counterparts(&mut contexts, bound);
    (contexts, file_scope, modules)
}

fn counterparts(contexts: &mut BTreeMap<String, MergedContext>, bound: &[Bound]) {
    for ctx in contexts.values_mut() {
        for table in &mut ctx.tables {
            for b in bound {
                let m = mask(b.title);
                if table.titles & m != 0 {
                    continue;
                }
                let ours: Vec<String> = table.fields.iter().map(|(f, _)| f.clone()).collect();
                let best = b
                    .binary
                    .tables
                    .iter()
                    .map(|t| (overlap(&ours, &table_tokens(t)), t))
                    .filter(|(score, _)| *score >= COUNTERPART_OVERLAP)
                    .max_by(|x, y| x.0.total_cmp(&y.0));
                let Some((_, theirs)) = best else { continue };
                for token in table_tokens(theirs) {
                    match table.fields.iter_mut().find(|(f, _)| *f == token) {
                        Some((_, fm)) => *fm |= m,
                        None => table.fields.push((token, m)),
                    }
                }
                table.titles |= m;
                if theirs.is_wildcard() {
                    table.wildcard |= m;
                }
                if let Some(h) = table.how.get_mut(usize::from(m == 2)) {
                    *h = Some(How::Counterpart);
                }
            }
        }
        let all = ctx.tables.iter().fold(0, |acc, t| acc | t.titles);
        for (m, _) in ctx.children.values_mut() {
            *m |= all;
        }
    }
}

fn render(
    contexts: &BTreeMap<String, MergedContext>,
    file_scope: &BTreeMap<String, String>,
    modules: &BTreeMap<String, String>,
    installs: usize,
) -> String {
    let mut keys: Vec<&String> = contexts.keys().collect();
    keys.sort_by_key(|k| std::cmp::Reverse(k.matches('/').count()));

    let mut lists: BTreeMap<String, usize> = BTreeMap::new();
    let mut list_src: Vec<String> = Vec::new();
    let mut ids: BTreeMap<String, usize> = BTreeMap::new();
    let mut bodies: BTreeMap<String, usize> = BTreeMap::new();
    let mut body_src: Vec<String> = Vec::new();

    for key in keys {
        let Some(ctx) = contexts.get(key) else {
            continue;
        };
        let mut tables = String::new();
        for t in &ctx.tables {
            let mut list = String::new();
            for (field, m) in &t.fields {
                let _ = write!(list, "Field({field:?}, {}),", mask_name(*m));
            }
            let next = list_src.len();
            let id = *lists.entry(list.clone()).or_insert(next);
            if id == next {
                list_src.push(list);
            }
            let _ = write!(
                tables,
                "Table {{ fields: F{id}, titles: {}, wildcard: {}, bound: [{}, {}] }},",
                mask_name(t.titles),
                mask_name(t.wildcard),
                how_name(t.how[0]),
                how_name(t.how[1]),
            );
        }
        let mut children = String::new();
        for (keyword, (m, target)) in &ctx.children {
            let target = match target {
                MergedTarget::Fixed(k) => match ids.get(k) {
                    Some(id) => format!("Target::Context({id})"),
                    None => continue,
                },
                MergedTarget::Typed(map) => {
                    let mut entries = String::new();
                    for (arg, k) in map {
                        if let Some(id) = ids.get(k) {
                            let _ = write!(entries, "({arg:?}, {id}),");
                        }
                    }
                    format!("Target::Typed(&[{entries}])")
                }
                MergedTarget::Module => "Target::Module".to_owned(),
            };
            let _ = write!(
                children,
                "Child {{ keyword: {keyword:?}, titles: {}, target: {target} }},",
                mask_name(*m)
            );
        }
        let body = format!("Context {{ tables: &[{tables}], children: &[{children}] }},");
        let next = body_src.len();
        let id = *bodies.entry(body.clone()).or_insert(next);
        if id == next {
            body_src.push(body);
        }
        ids.insert(key.clone(), id);
    }

    let mut out = String::new();
    out.push_str(
        "// @generated by `cargo xtask ini-tables --emit`; do not edit by hand.\n\
         // Regenerate with both installs, BFME II first:\n\
         //   cargo xtask ini-tables --emit \"<BFME II install>\" \"<ROTWK install>\"\n\
         // Provenance per table is its `bound` pair; see `super::field_table`.\n\n\
         use super::field_table::{Bound, Child, Context, Field, Table, Target, Titles};\n\n\
         const B2: Titles = Titles::BFME2;\n\
         const RK: Titles = Titles::ROTWK;\n\
         const BOTH: Titles = Titles::BOTH;\n\
         const NONE: Titles = Titles::NONE;\n\n",
    );
    let _ = writeln!(out, "// Built from {installs} install(s).\n");
    for (i, list) in list_src.iter().enumerate() {
        let _ = writeln!(out, "const F{i}: &[Field] = &[{list}];");
    }
    out.push_str("\npub(super) static CONTEXTS: &[Context] = &[\n");
    for body in &body_src {
        let _ = writeln!(out, "    {body}");
    }
    out.push_str("];\n\n/// File-scope block name -> context, sorted.\npub(super) static FILE_SCOPE: &[(&str, u16)] = &[\n");
    for (name, key) in file_scope {
        if let Some(id) = ids.get(key) {
            let _ = writeln!(out, "    ({name:?}, {id}),");
        }
    }
    out.push_str("];\n\n/// Module name -> context, sorted.\npub(super) static MODULES: &[(&str, u16)] = &[\n");
    for (name, key) in modules {
        if let Some(id) = ids.get(key) {
            let _ = writeln!(out, "    ({name:?}, {id}),");
        }
    }
    out.push_str("];\n");
    out
}

fn report(b: &Bound) {
    let label = match b.title {
        Title::Bfme2 => "BFME II",
        Title::Rotwk => "ROTWK",
    };
    let mut by_how: BTreeMap<(&str, How), usize> = BTreeMap::new();
    for (key, ctx) in &b.binding.contexts {
        let kind = if key.contains('/') {
            "nested"
        } else if key.starts_with("M:") {
            "module"
        } else {
            "file-scope"
        };
        for &(_, how) in &ctx.bound.tables {
            *by_how.entry((kind, how)).or_default() += 1;
        }
    }
    println!(
        "\n=== {label}: {} tables recovered, {} registrations, {} modules registered ===",
        b.binary.tables.len(),
        b.binary.registrations.len(),
        b.binary.modules.len()
    );
    println!(
        "bound: {} file-scope blocks, {} modules, {} contexts in all",
        b.binding.file_scope.len(),
        b.binding.modules.len(),
        b.binding.contexts.len()
    );
    for ((kind, how), n) in &by_how {
        println!("  {kind:<10} {how:?}: {n} table binding(s)");
    }
    let unbound_registrations: Vec<&String> = b
        .binary
        .registrations
        .keys()
        .filter(|k| !b.binding.file_scope.contains_key(*k))
        .collect();
    println!("registered blocks left unbound: {unbound_registrations:?}");
    let unbound_modules: Vec<&String> = b
        .binary
        .modules
        .keys()
        .filter(|k| !b.binding.modules.contains_key(*k))
        .collect();
    println!("registered modules left unbound: {unbound_modules:?}");
    if let Ok(filter) = std::env::var("INI_TABLES_DEBUG") {
        for (key, ctx) in &b.binding.contexts {
            let votes = filter == "votes"
                && ctx
                    .bound
                    .tables
                    .iter()
                    .any(|&(_, how)| matches!(how, How::Vote | How::Vocabulary));
            if !votes && !key.contains(filter.as_str()) {
                continue;
            }
            println!(
                "  {key}: children {:?}",
                ctx.children.keys().collect::<Vec<_>>()
            );
            for &(t, how) in &ctx.bound.tables {
                if let Some(table) = b.binary.tables.get(t) {
                    println!("    {how:?} {:?}: {:?}", table.key, table_tokens(table));
                }
            }
        }
    }
    let mut unbound = b.binding.unbound.clone();
    unbound.sort_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.cmp(&y.0)));
    let total: u64 = unbound.iter().map(|(_, n)| n).sum();
    println!(
        "corpus paths no binding reached: {} ({total} statements at or below them)",
        unbound.len()
    );
    for (path, n) in unbound.iter().take(40) {
        println!("  {n:>7}  {path}");
    }
}

pub(crate) fn run(root: &Path, installs: &[PathBuf]) -> bool {
    let mut bound: Vec<Bound> = Vec::new();
    for install in installs {
        let exe = install.join("Worldbuilder.exe");
        let result = Image::load(&exe)
            .and_then(Binary::recover)
            .and_then(|binary| Corpus::load(root, install).map(|corpus| (binary, corpus)));
        match result {
            Ok((mut binary, corpus)) => {
                let binding = bind(&mut binary, &corpus);
                let b = Bound {
                    title: corpus.title,
                    binary,
                    binding,
                };
                report(&b);
                bound.push(b);
            }
            Err(e) => {
                eprintln!("FAILED: {}: {e}", install.display());
                return false;
            }
        }
    }
    let (contexts, file_scope, modules) = merge(&bound);
    let source = render(&contexts, &file_scope, &modules, bound.len());
    let path = root.join(OUTPUT);
    match std::fs::write(&path, source) {
        Ok(()) => {
            println!(
                "\nwrote {} ({} contexts before sharing)",
                OUTPUT,
                contexts.len()
            );
            true
        }
        Err(e) => {
            eprintln!("FAILED: {}: {e}", path.display());
            false
        }
    }
}
