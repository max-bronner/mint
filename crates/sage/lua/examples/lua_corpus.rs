use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_lua::{Chunk, Script};

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: lua_corpus <dir> [<dir> ...]");
        return ExitCode::FAILURE;
    }

    let mut totals = Totals::default();
    for root in &roots {
        walk(root, &mut totals);
    }
    totals.report();

    if totals.failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[derive(Default)]
struct Totals {
    files: usize,
    blocks: usize,
    block_lines: usize,
    chunks: usize,
    functions: usize,
    calls: BTreeMap<String, usize>,
    assigned: BTreeMap<String, usize>,
    failures: Vec<String>,
}

impl Totals {
    fn report(&self) {
        let parsed = self.blocks.saturating_sub(self.failures.len());
        println!(
            "files with scripts: {}\nblocks: {} parsed, {} failed ({} non-blank lines)",
            self.files,
            parsed,
            self.failures.len(),
            self.block_lines
        );
        println!("chunks: {} with {} functions", self.chunks, self.functions);
        println!("distinct call names: {}", self.calls.len());
        let mut by_count: Vec<(&String, &usize)> = self.calls.iter().collect();
        by_count.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        for (name, count) in by_count {
            println!("  {count:6}  {name}");
        }
        println!("assigned variable names: {}", self.assigned.len());
        let mut vars: Vec<(&String, &usize)> = self.assigned.iter().collect();
        vars.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        for (name, count) in vars {
            println!("  {count:6}  {name}");
        }
        for failure in &self.failures {
            println!("FAIL {failure}");
        }
    }
}

fn walk(path: &Path, totals: &mut Totals) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            walk(&path, totals);
            continue;
        }
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match extension.as_str() {
            "ini" | "inc" => read_blocks(&path, totals),
            "lua" => read_chunk(&path, totals),
            _ => {}
        }
    }
}

fn read_text(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    Some(bytes.iter().map(|byte| char::from(*byte)).collect())
}

fn read_chunk(path: &Path, totals: &mut Totals) {
    let Some(text) = read_text(path) else { return };
    match Chunk::parse(&text) {
        Ok(chunk) => {
            totals.chunks = totals.chunks.saturating_add(1);
            for function in chunk.functions() {
                totals.functions = totals.functions.saturating_add(1);
                for name in function.call_names() {
                    *totals.calls.entry(name.to_string()).or_default() += 1;
                }
            }
        }
        Err(error) => totals
            .failures
            .push(format!("{}:{} {error}", path.display(), error.line)),
    }
}

fn code(line: &str) -> &str {
    let end = [";", "//"]
        .iter()
        .filter_map(|marker| line.find(marker))
        .min()
        .unwrap_or(line.len());
    line.get(..end).unwrap_or("").trim()
}

fn read_blocks(path: &Path, totals: &mut Totals) {
    let Some(text) = read_text(path) else { return };
    let lines: Vec<&str> = text.split('\n').collect();
    let mut index = 0usize;
    let mut found = false;
    while index < lines.len() {
        let line = lines.get(index).copied().unwrap_or_default();
        if !code(line).eq_ignore_ascii_case("BeginScript") {
            index = index.saturating_add(1);
            continue;
        }
        let start = index.saturating_add(1);
        let mut end = start;
        while end < lines.len()
            && !code(lines.get(end).copied().unwrap_or_default()).eq_ignore_ascii_case("EndScript")
        {
            end = end.saturating_add(1);
        }
        let body = lines.get(start..end).unwrap_or_default().join("\n");
        found = true;
        totals.blocks = totals.blocks.saturating_add(1);
        totals.block_lines = totals
            .block_lines
            .saturating_add(body.lines().filter(|l| !l.trim().is_empty()).count());
        match Script::parse(&body) {
            Ok(script) => {
                for name in script.call_names() {
                    *totals.calls.entry(name.to_string()).or_default() += 1;
                }
                for name in assigned_names(&body) {
                    *totals.assigned.entry(name).or_default() += 1;
                }
            }
            Err(error) => totals.failures.push(format!(
                "{}:{} {error}",
                path.display(),
                start.saturating_add(error.line as usize)
            )),
        }
        index = end;
    }
    if found {
        totals.files = totals.files.saturating_add(1);
    }
}

fn assigned_names(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in body.lines() {
        let line = code(line);
        let Some((left, _)) = line.split_once('=') else {
            continue;
        };
        let left = left.trim();
        if left.ends_with(['=', '~', '<', '>']) {
            continue;
        }
        let name = left.strip_prefix("local ").unwrap_or(left).trim();
        if !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !name.starts_with(|c: char| c.is_ascii_digit())
        {
            out.push(name.to_string());
        }
    }
    out
}
