use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub fn field_map(
    repo_root: &Path,
    rel_path: &str,
    struct_name: &str,
    fn_name: &str,
) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(repo_root.join(rel_path)) else {
        return map;
    };

    let struct_fields = struct_field_names(&text, struct_name);
    let Some(body) = function_body(&text, fn_name) else {
        return map;
    };

    let mut current: Option<String> = None;
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("let ") {
            if let Some(ident) = leading_ident(rest) {
                current = Some(ident);
            }
        } else if let Some(colon) = trimmed.find(':') {
            if let Some(head) = trimmed.get(..colon).map(str::trim) {
                if is_plain_ident(head) {
                    current = Some(head.to_owned());
                }
            }
        }

        if let (Some(field), Some(literal)) = (&current, quoted_identifier(trimmed)) {
            if struct_fields.contains(field.as_str()) {
                map.entry(field.clone()).or_insert(literal);
            }
        }
    }
    map
}

pub fn collisions(repo_root: &Path) -> BTreeMap<String, usize> {
    let mut owners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for_each_source_file(repo_root, None, |rel_path, text| {
        for (struct_name, start) in struct_starts(text) {
            let Some(body) = brace_body_at(text, start) else {
                continue;
            };
            for field in struct_fields_in_body(&body) {
                owners
                    .entry(field)
                    .or_default()
                    .insert(format!("{rel_path}::{struct_name}"));
            }
        }
    });
    owners
        .into_iter()
        .map(|(field, owners)| (field, owners.len()))
        .collect()
}

#[must_use]
pub fn ambiguous(collisions: &BTreeMap<String, usize>, struct_field: &str) -> bool {
    collisions.get(struct_field).copied().unwrap_or(1) > 1
}

#[must_use]
pub fn is_applied(repo_root: &Path, struct_field: &str, exclude_rel_path: &str) -> bool {
    !consumers(repo_root, struct_field, exclude_rel_path).is_empty()
}

pub fn consumers(repo_root: &Path, struct_field: &str, exclude_rel_path: &str) -> Vec<String> {
    let needle = format!(".{struct_field}");
    let mut hits = Vec::new();
    for_each_source_file(repo_root, Some(exclude_rel_path), |rel_path, text| {
        for (i, line) in text.lines().enumerate() {
            if line.contains(&needle) && !is_word_suffix_only(line, &needle) {
                hits.push(format!("{rel_path}:{}", i.saturating_add(1)));
            }
        }
    });
    hits
}

fn for_each_source_file(
    repo_root: &Path,
    exclude_rel_path: Option<&str>,
    mut visit: impl FnMut(&str, &str),
) {
    let mut stack = vec![repo_root.join("crates"), repo_root.join("xtask")];
    while let Some(dir) = stack.pop() {
        let Ok(listing) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in listing.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if path.file_name().is_some_and(|n| n == "tests") {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if !kind.is_file() {
                continue;
            }
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            if path.file_name().is_some_and(|n| n == "tests.rs") {
                continue;
            }
            let rel = path
                .strip_prefix(repo_root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if exclude_rel_path.is_some_and(|excluded| excluded == rel) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            visit(&rel, &text);
        }
    }
}

fn is_word_suffix_only(line: &str, needle: &str) -> bool {
    if let Some(pos) = line.find(needle) {
        let after = pos.saturating_add(needle.len());
        if let Some(ch) = line.get(after..).and_then(|s| s.chars().next()) {
            return ch.is_alphanumeric() || ch == '_';
        }
    }
    false
}

fn leading_ident(text: &str) -> Option<String> {
    let ident: String = text
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!ident.is_empty()).then_some(ident)
}

fn is_plain_ident(text: &str) -> bool {
    !text.is_empty()
        && text.chars().all(|c| c.is_alphanumeric() || c == '_')
        && text
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
}

fn quoted_identifier(line: &str) -> Option<String> {
    let start = line.find('"')?;
    let rest = line.get(start.saturating_add(1)..)?;
    let end = rest.find('"')?;
    let candidate = rest.get(..end)?;
    let valid = !candidate.is_empty()
        && candidate.chars().next().is_some_and(|c| c.is_alphabetic())
        && candidate.chars().all(|c| c.is_alphanumeric() || c == '_');
    valid.then(|| candidate.to_owned())
}

fn function_body(text: &str, fn_name: &str) -> Option<String> {
    let needle = format!("fn {fn_name}(");
    let start = text.find(&needle)?;
    let open = text.get(start..)?.find('{')?;
    brace_body_at(text, start.saturating_add(open))
}

fn struct_starts(text: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let mut search_from = 0usize;
    while let Some(rest) = text.get(search_from..) {
        let Some(rel) = rest.find("pub struct ") else {
            break;
        };
        let at = search_from.saturating_add(rel);
        let after = at.saturating_add("pub struct ".len());
        if let Some(tail) = text.get(after..) {
            if let Some(name) = leading_ident(tail) {
                out.push((name, at));
            }
        }
        search_from = after;
    }
    out
}

fn struct_field_names(text: &str, struct_name: &str) -> BTreeSet<String> {
    let needle = format!("pub struct {struct_name}");
    let Some(at) = text.find(&needle) else {
        return BTreeSet::new();
    };
    let Some(body) = brace_body_at(text, at) else {
        return BTreeSet::new();
    };
    struct_fields_in_body(&body)
}

fn struct_fields_in_body(body: &str) -> BTreeSet<String> {
    let mut fields = BTreeSet::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("pub ") {
            if let Some(colon) = rest.find(':') {
                if let Some(ident) = rest.get(..colon).map(str::trim) {
                    if is_plain_ident(ident) {
                        fields.insert(ident.to_owned());
                    }
                }
            }
        }
    }
    fields
}

fn brace_body_at(text: &str, from: usize) -> Option<String> {
    let rel_open = text.get(from..)?.find('{')?;
    let open = from.saturating_add(rel_open);
    let mut depth: i32 = 0;
    let mut end = None;
    for (offset, byte) in text.as_bytes().iter().enumerate().skip(open) {
        match byte {
            b'{' => depth = depth.saturating_add(1),
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    end = Some(offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end?;
    text.get(open.saturating_add(1)..end).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_name_shared_with_an_unrelated_struct_still_counts_applied_when_a_real_consumer_exists(
    ) {
        let root =
            std::env::temp_dir().join(format!("mint-ini-applied-test-{}", std::process::id()));
        let src_dir = root.join("crates/testcrate/src");
        std::fs::create_dir_all(&src_dir).expect("create src dir");
        std::fs::write(
            src_dir.join("reader.rs"),
            "pub struct Reader { pub attack_range: f32 }\n",
        )
        .expect("write reader.rs");
        std::fs::write(
            src_dir.join("unrelated.rs"),
            "pub struct Unrelated { pub attack_range: f32 }\n",
        )
        .expect("write unrelated.rs");
        std::fs::write(
            src_dir.join("consumer.rs"),
            "fn f(r: &Reader) -> f32 { r.attack_range }\n",
        )
        .expect("write consumer.rs");

        let collisions = collisions(&root);
        let reader_rel = "crates/testcrate/src/reader.rs";
        let applied = is_applied(&root, "attack_range", reader_rel);
        let is_ambiguous = ambiguous(&collisions, "attack_range");

        std::fs::remove_dir_all(&root).ok();

        assert!(
            is_ambiguous,
            "two structs share attack_range, so this must be ambiguous"
        );
        assert!(
            applied,
            "a real consumer exists outside the reader, so it must count as applied regardless"
        );
    }
}
