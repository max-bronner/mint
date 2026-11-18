use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_sage_ini::tables::field_table::{self, Context, Target, Titles};
use mint_sage_ini::{
    included_paths, Block, BlockOpener, BlockTable, Diagnostic, Document, MemorySources, Node,
    Reader, Scope,
};

const INI_ROOT: &str = "data/ini/";

const MAX_LISTED: usize = 80;

struct FallbackCounter<'a> {
    inner: &'a BlockTable,
    total: Cell<u64>,
    fallback: Cell<u64>,
    fallback_pairs: RefCell<BTreeSet<(String, String)>>,
}

impl<'a> FallbackCounter<'a> {
    fn new(inner: &'a BlockTable) -> Self {
        Self {
            inner,
            total: Cell::new(0),
            fallback: Cell::new(0),
            fallback_pairs: RefCell::new(BTreeSet::new()),
        }
    }
}

impl BlockOpener for FallbackCounter<'_> {
    fn opens_block(&self, context: &[Scope], keyword: &str, args: &[String]) -> Option<bool> {
        self.total.set(self.total.get().saturating_add(1));
        let answer = self.inner.opens_block(context, keyword, args);
        if answer.is_none() {
            self.fallback.set(self.fallback.get().saturating_add(1));
            let parent = context
                .last()
                .map(|scope| scope.keyword.to_ascii_lowercase())
                .unwrap_or_default();
            self.fallback_pairs
                .borrow_mut()
                .insert((parent, keyword.to_ascii_lowercase()));
        }
        answer
    }
}

fn main() -> ExitCode {
    let mut contexts = false;
    let roots: Vec<PathBuf> = std::env::args_os()
        .skip(1)
        .filter(|arg| {
            let flag = arg == "--contexts";
            contexts |= flag;
            !flag
        })
        .map(PathBuf::from)
        .collect();
    if contexts {
        for root in &roots {
            dump_contexts(root);
        }
        return ExitCode::SUCCESS;
    }
    if roots.is_empty() {
        eprintln!("usage: schema_coverage <install dir> [<install dir> ...]");
        eprintln!("       mount both installs, ROTWK last - see the module docs");
        return ExitCode::FAILURE;
    }

    let started = Instant::now();
    let mut failed = false;

    for root in &roots {
        let label = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string());
        failed |= run(&label, std::slice::from_ref(root));
    }
    if roots.len() > 1 {
        failed |= run("pooled, later installs shadowing earlier", &roots);
    }

    println!("\n== throughput ==");
    println!("{:.2} s total", started.elapsed().as_secs_f64());

    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn run(label: &str, roots: &[PathBuf]) -> bool {
    println!("\n== {label} ==");
    let sources = ini_sources(roots);
    let included = included_paths(&sources);
    let tops = loaded_documents(&sources, &included);
    println!(
        "{} distinct entries under {INI_ROOT}, {} loaded documents, {} unreached fragment(s) not read",
        sources.len(),
        tops.len(),
        sources
            .paths()
            .filter(|p| !p.ends_with(".ini") && !included.contains(*p))
            .count(),
    );

    let before = measure(&sources, &tops, &BlockTable::object_model_path());
    let after = measure(&sources, &tops, &BlockTable::corpus_path());

    println!(
        "                              {:>22} {:>22}",
        "object_model_path", "corpus_path"
    );
    row("documents read", before.read, after.read);
    row("documents that balance", before.balanced, after.balanced);
    row(
        "documents free of self-nesting",
        before.no_self_nesting,
        after.no_self_nesting,
    );
    row("documents structurally clean", before.clean, after.clean);
    row("definitions found", before.definitions, after.definitions);
    row(
        "definitions parsing completely",
        before.complete,
        after.complete,
    );
    println!(
        "  {:<30} {:>21.2}% {:>21.2}%",
        "  ...as a share of those found",
        share(before.complete, before.definitions),
        share(after.complete, after.definitions)
    );
    row("stray End", before.stray_end, after.stray_end);
    row("blocks left open", before.unclosed, after.unclosed);
    row(
        "fields at file scope",
        before.at_file_scope,
        after.at_file_scope,
    );
    row("self-nested blocks", before.self_nested, after.self_nested);
    row(
        "declared file-scope leaves",
        before.declared_leaves,
        after.declared_leaves,
    );
    row(
        "object-family definitions whole",
        before.object_complete_at.len() as u64,
        after.object_complete_at.len() as u64,
    );
    row(
        "distinct file-scope keywords",
        before.root_keywords.len() as u64,
        after.root_keywords.len() as u64,
    );
    row(
        "block/field questions asked",
        before.fallback_questions,
        after.fallback_questions,
    );
    row(
        "  ...no table opinion (reads as field)",
        before.fallback_answered,
        after.fallback_answered,
    );
    println!(
        "  {:<30} {:>21.2}% {:>21.2}%",
        "  ...as a share of those asked",
        share(before.fallback_answered, before.fallback_questions),
        share(after.fallback_answered, after.fallback_questions)
    );
    row(
        "  ...distinct (parent, keyword) pairs with no opinion",
        before.fallback_pairs,
        after.fallback_pairs,
    );
    println!(
        "  {:<30} {:>21.2}s {:>21.2}s",
        "wall clock", before.seconds, after.seconds
    );

    let mut failed = false;

    println!(
        "  cross-check: 0 fallback decisions (the mechanism was deleted); {} question(s) read as \
         a field with no explicit row, across {} pair(s) - expected and unbounded by design \
         ({} of them named in `fallback_pair_names` for a debugger, not printed here)",
        after.fallback_answered,
        after.fallback_pairs,
        after.fallback_pair_names.len()
    );

    let lost: Vec<&(String, u32)> = before
        .object_complete_at
        .difference(&after.object_complete_at)
        .collect();
    if lost.is_empty() {
        println!(
            "  cross-check: 0 of {} object-family definitions complete under object_model_path lost",
            before.object_complete_at.len()
        );
    } else {
        failed = true;
        println!(
            "  cross-check FAILED: {} object-family definitions complete under              object_model_path are no longer",
            lost.len()
        );
        for (file, line) in lost.iter().take(MAX_LISTED) {
            println!("    regressed: {file}:{line}");
        }
    }

    let leaked: Vec<&String> = after
        .root_keywords
        .difference(&before.root_keywords)
        .filter(|keyword| INTERIOR_ONLY.contains(&keyword.as_str()))
        .collect();
    if leaked.is_empty() {
        println!("  cross-check: no interior keyword reaches file scope under corpus_path");
    } else {
        failed = true;
        println!("  cross-check FAILED: interior keywords at file scope: {leaked:?}");
    }

    println!(
        "  {} document(s) still carry a structural diagnostic under corpus_path:",
        after.damaged.len()
    );
    for (path, first, count) in after.damaged.iter().take(MAX_LISTED) {
        println!("    {path}: {count} diagnostic(s), first: {first}");
    }
    if after.damaged.len() > MAX_LISTED {
        println!("    ... and {} more", after.damaged.len() - MAX_LISTED);
    }

    field_coverage(&sources, &tops, title_of(&sources));
    failed
}

#[derive(Default)]
struct FieldTally {
    total: [u64; 4],
    covered: [u64; 4],
    uncovered: BTreeMap<(String, String), u64>,
    unresolved: BTreeMap<String, u64>,
}

const KINDS: [&str; 4] = [
    "field statements",
    "nested-block openers",
    "script regions",
    "file-scope blocks",
];

fn field_coverage(sources: &MemorySources, documents: &[String], title: Title) {
    let titles = match title {
        Title::Bfme2 => Titles::BFME2,
        Title::Rotwk => Titles::ROTWK,
    };
    let reader = Reader::new(sources);
    let table = BlockTable::corpus_path();
    let mut tally = FieldTally::default();
    for path in documents {
        let Ok(document) = reader.read(path, &table) else {
            continue;
        };
        for root in document.roots() {
            let context = field_table::file_scope(&root.keyword);
            tally.count(3, context.is_some(), "", &root.keyword);
            match context {
                Some(context) => walk_fields(root, context, &root.keyword, titles, &mut tally),
                None => tally.unresolve(&root.keyword, root),
            }
        }
    }

    println!(
        "  field table ({}), against the generated table:",
        title.label()
    );
    for (i, kind) in KINDS.iter().enumerate() {
        let total = tally.total.get(i).copied().unwrap_or(0);
        let covered = tally.covered.get(i).copied().unwrap_or(0);
        println!(
            "    {kind:<22} {covered:>8} of {total:>8} accepted ({:.2}%)",
            share(covered, total)
        );
    }
    let mut unresolved: Vec<(&String, &u64)> = tally.unresolved.iter().collect();
    unresolved.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    println!(
        "    {} context(s) with no field set ({} statements at or below them):",
        unresolved.len(),
        unresolved.iter().map(|(_, n)| **n).sum::<u64>()
    );
    for (context, count) in &unresolved {
        println!("      {count:>7}  {context}");
    }
    let mut uncovered: Vec<(&(String, String), &u64)> = tally.uncovered.iter().collect();
    uncovered.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    println!(
        "    {} uncovered (context, token) pair(s) in contexts that have a field set:",
        uncovered.len()
    );
    for ((context, token), count) in &uncovered {
        println!("      {count:>7}  {context} : {token}");
    }
}

impl FieldTally {
    fn count(&mut self, kind: usize, accepted: bool, context: &str, token: &str) {
        if let Some(t) = self.total.get_mut(kind) {
            *t = t.saturating_add(1);
        }
        if accepted {
            if let Some(c) = self.covered.get_mut(kind) {
                *c = c.saturating_add(1);
            }
        } else if kind != 3 {
            let slot = self
                .uncovered
                .entry((context.to_owned(), token.to_owned()))
                .or_default();
            *slot = slot.saturating_add(1);
        }
    }

    fn unresolve(&mut self, label: &str, block: &Block) {
        let mut below = 0u64;
        count_below(block, &mut below, &mut self.total);
        let slot = self.unresolved.entry(label.to_owned()).or_default();
        *slot = slot.saturating_add(below);
    }
}

fn kind_of(node: &Node) -> (usize, &str) {
    match node {
        Node::Field(field) => (0, field.keyword.as_str()),
        Node::Block(block) => (1, block.keyword.as_str()),
        Node::Script(_) => (2, "BeginScript"),
    }
}

fn count_below(block: &Block, below: &mut u64, total: &mut [u64; 4]) {
    for node in &block.children {
        let (kind, _) = kind_of(node);
        if let Some(t) = total.get_mut(kind) {
            *t = t.saturating_add(1);
        }
        *below = below.saturating_add(1);
        if let Node::Block(child) = node {
            count_below(child, below, total);
        }
    }
}

fn walk_fields(
    block: &Block,
    context: &Context,
    label: &str,
    titles: Titles,
    tally: &mut FieldTally,
) {
    for node in &block.children {
        let (kind, token) = kind_of(node);
        tally.count(kind, context.accepts(token, titles), label, token);
        let Node::Block(child) = node else {
            continue;
        };
        let arg = child.args.first().map(String::as_str);
        let selected = context
            .children
            .iter()
            .find(|c| c.keyword == child.keyword)
            .is_some_and(|c| !matches!(c.target, Target::Context(_)));
        let child_label = match (selected, arg) {
            (true, Some(arg)) => format!("{label}/{}={arg}", child.keyword),
            _ => format!("{label}/{}", child.keyword),
        };
        match context.child(&child.keyword, arg, titles) {
            Some(next) => walk_fields(child, next, &child_label, titles, tally),
            None => tally.unresolve(&child_label, child),
        }
    }
}

const INTERIOR_ONLY: &[&str] = &[
    "behavior",
    "body",
    "draw",
    "armorset",
    "weaponset",
    "locomotorset",
    "modelconditionstate",
    "defaultmodelconditionstate",
    "animationstate",
    "idleanimationstate",
    "transitionstate",
    "kindof",
    "geometry",
    "model",
];

const OBJECT_FAMILY: &[&str] = &["Object", "ChildObject", "ObjectReskin", "Road", "Terrain"];

fn share(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 * 100.0 / whole as f64
    }
}

fn row(what: &str, before: u64, after: u64) {
    let delta = i128::from(after) - i128::from(before);
    let arrow = if delta == 0 {
        String::new()
    } else {
        format!("  ({delta:+})")
    };
    println!("  {what:<30} {before:>22} {after:>22}{arrow}");
}

#[derive(Default)]
struct Coverage {
    read: u64,
    balanced: u64,
    no_self_nesting: u64,
    clean: u64,
    definitions: u64,
    complete: u64,
    stray_end: u64,
    unclosed: u64,
    at_file_scope: u64,
    self_nested: u64,
    declared_leaves: u64,
    seconds: f64,
    fallback_questions: u64,
    fallback_answered: u64,
    fallback_pairs: u64,
    fallback_pair_names: BTreeSet<(String, String)>,
    complete_at: BTreeSet<(String, u32)>,
    object_complete_at: BTreeSet<(String, u32)>,
    root_keywords: BTreeSet<String>,
    damaged: Vec<(String, String, usize)>,
}

fn measure(sources: &MemorySources, tops: &[String], table: &BlockTable) -> Coverage {
    let started = Instant::now();
    let reader = Reader::new(sources);
    let counter = FallbackCounter::new(table);
    let mut out = Coverage::default();
    for path in tops {
        let Ok(document) = reader.read(path, &counter) else {
            continue;
        };
        out.read = out.read.saturating_add(1);
        tally(&document, table, &mut out);
    }
    out.seconds = started.elapsed().as_secs_f64();
    out.fallback_questions = counter.total.get();
    out.fallback_answered = counter.fallback.get();
    out.fallback_pairs = counter.fallback_pairs.borrow().len() as u64;
    out.fallback_pair_names = counter.fallback_pairs.borrow().clone();
    out
}

fn tally(document: &Document, table: &BlockTable, out: &mut Coverage) {
    if document.is_balanced() {
        out.balanced = out.balanced.saturating_add(1);
    }
    if !document.has_self_nesting() {
        out.no_self_nesting = out.no_self_nesting.saturating_add(1);
    }

    let mut by_file: BTreeMap<&str, Vec<&Block>> = BTreeMap::new();
    for root in document.roots() {
        by_file.entry(&root.file).or_default().push(root);
        out.definitions = out.definitions.saturating_add(1);
        if !document.name().ends_with(".inc") {
            out.root_keywords.insert(root.keyword.to_ascii_lowercase());
        }
    }
    for roots in by_file.values_mut() {
        roots.sort_by_key(|root| root.line);
    }

    let mut structural = 0usize;
    let mut first: Option<String> = None;
    let mut damaged_spans: BTreeSet<(String, u32)> = BTreeSet::new();
    for diagnostic in document.diagnostics() {
        let (file, line) = match diagnostic {
            Diagnostic::StrayEnd { file, line } => {
                out.stray_end = out.stray_end.saturating_add(1);
                (file, *line)
            }
            Diagnostic::UnclosedBlock { file, line, .. } => {
                out.unclosed = out.unclosed.saturating_add(1);
                (file, *line)
            }
            Diagnostic::FieldAtFileScope {
                file,
                line,
                keyword,
            } => {
                if table.opens_block(&[], keyword, &[]) == Some(false) {
                    out.declared_leaves = out.declared_leaves.saturating_add(1);
                    continue;
                }
                out.at_file_scope = out.at_file_scope.saturating_add(1);
                (file, *line)
            }
            Diagnostic::SelfNested { file, line, .. } => {
                out.self_nested = out.self_nested.saturating_add(1);
                (file, *line)
            }
            Diagnostic::IncludeMissing { .. } | Diagnostic::UnknownDirective { .. } => continue,
        };
        structural = structural.saturating_add(1);
        if first.is_none() {
            first = Some(diagnostic.to_string());
        }
        if let Some(roots) = by_file.get(file.as_ref()) {
            if let Some(owner) = roots.iter().rev().find(|root| root.line <= line) {
                damaged_spans.insert((owner.file.to_string(), owner.line));
            }
        }
    }

    for (file, roots) in &by_file {
        for root in roots {
            let key = ((*file).to_owned(), root.line);
            if !damaged_spans.contains(&key) {
                out.complete = out.complete.saturating_add(1);
                if structural == 0
                    && OBJECT_FAMILY
                        .iter()
                        .any(|k| root.keyword.eq_ignore_ascii_case(k))
                {
                    out.object_complete_at.insert(key.clone());
                }
                out.complete_at.insert(key);
            }
        }
    }

    if structural == 0 {
        out.clean = out.clean.saturating_add(1);
    } else {
        out.damaged.push((
            document.name().to_owned(),
            first.unwrap_or_default(),
            structural,
        ));
    }
}

fn ini_sources(roots: &[PathBuf]) -> MemorySources {
    let mut sources = MemorySources::new();
    for root in roots {
        for path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&path) else {
                continue;
            };
            for entry in archive.entries() {
                if !entry.path.as_str().starts_with(INI_ROOT) {
                    continue;
                }
                let Ok(bytes) = archive.read_entry(entry) else {
                    continue;
                };
                sources.insert(entry.path.as_str(), bytes);
            }
        }
    }
    sources
}

fn big_archives(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(listing) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in listing.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(kind) if kind.is_file() => {
                    if path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("big"))
                    {
                        found.push(path);
                    }
                }
                _ => {}
            }
        }
    }
    found.sort();
    found
}

fn dump_contexts(root: &PathBuf) {
    let label = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let sources = ini_sources(std::slice::from_ref(root));
    println!("T	{label}	{}", title_of(&sources).label());
    let table = BlockTable::corpus_path();
    let reader = Reader::new(&sources);
    let mut counts: BTreeMap<(String, String, char), u64> = BTreeMap::new();
    for path in loaded_documents(&sources, &included_paths(&sources)) {
        let Ok(document) = reader.read(&path, &table) else {
            continue;
        };
        for block in document.roots() {
            walk_contexts(block, &block.keyword, &mut counts);
        }
    }
    for ((parent, token, kind), count) in counts {
        println!("S	{label}	{parent}	{token}	{kind}	{count}");
    }
}

fn walk_contexts(block: &Block, path: &str, counts: &mut BTreeMap<(String, String, char), u64>) {
    for node in &block.children {
        let (token, kind) = match node {
            Node::Field(field) => (field.keyword.as_str(), 'F'),
            Node::Block(child) => (child.keyword.as_str(), 'B'),
            Node::Script(_) => ("BeginScript", 'S'),
        };
        let slot = counts
            .entry((path.to_owned(), token.to_owned(), kind))
            .or_default();
        *slot = slot.saturating_add(1);
        if let Node::Block(child) = node {
            let child_path = match child.args.first() {
                Some(arg) => format!("{path}/{}={arg}", child.keyword),
                None => format!("{path}/{}", child.keyword),
            };
            walk_contexts(child, &child_path, counts);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Title {
    Bfme2,
    Rotwk,
}

impl Title {
    fn label(self) -> &'static str {
        match self {
            Self::Bfme2 => "bfme2",
            Self::Rotwk => "rotwk",
        }
    }
}

fn title_of(sources: &MemorySources) -> Title {
    if sources
        .paths()
        .any(|path| path == "data/ini/default/subsystemlegendexpansion1.ini")
    {
        Title::Rotwk
    } else {
        Title::Bfme2
    }
}

fn loaded_documents(sources: &MemorySources, included: &BTreeSet<String>) -> Vec<String> {
    sources
        .paths()
        .filter(|path| path.ends_with(".ini") && !included.contains(*path))
        .map(str::to_owned)
        .collect()
}
