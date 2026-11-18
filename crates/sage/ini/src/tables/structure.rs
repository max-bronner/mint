use crate::table::{
    AUDIO_CLOSED, AUDIO_RULES, CORPUS_CLOSED, CORPUS_FILE_SCOPE_RULES, CORPUS_INTERIOR_RULES,
    HORDE_CONTAIN_CLOSED, HORDE_CONTAIN_RULES, LIVING_WORLD_RULES, OBJECT_MODEL_ALIASES,
    OBJECT_MODEL_CLOSED, OBJECT_MODEL_RULES, PARTICLE_CLOSED, PARTICLE_RULES,
    PRIORITY_BLOCK_CLOSED, PRIORITY_BLOCK_RULES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verified {
    FileScopeRegistry,
    Document(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct OpenerSpec {
    pub context: &'static str,
    pub keyword: &'static str,
    pub opens: bool,
    pub verified: Verified,
}

fn opener_rows(
    rules: &'static [(&'static str, &'static str, bool)],
    group: &'static str,
) -> Vec<OpenerSpec> {
    rules
        .iter()
        .map(|&(context, keyword, opens)| OpenerSpec {
            context,
            keyword,
            opens,
            verified: if context.is_empty() {
                Verified::FileScopeRegistry
            } else {
                Verified::Document(group)
            },
        })
        .collect()
}

#[must_use]
pub fn openers() -> Vec<OpenerSpec> {
    let mut all = Vec::new();
    all.extend(opener_rows(OBJECT_MODEL_RULES, "table::OBJECT_MODEL_RULES"));
    all.extend(opener_rows(PARTICLE_RULES, "table::PARTICLE_RULES"));
    all.extend(opener_rows(
        PRIORITY_BLOCK_RULES,
        "table::PRIORITY_BLOCK_RULES",
    ));
    all.extend(opener_rows(AUDIO_RULES, "table::AUDIO_RULES"));
    all.extend(opener_rows(
        HORDE_CONTAIN_RULES,
        "table::HORDE_CONTAIN_RULES",
    ));
    all.extend(opener_rows(
        CORPUS_FILE_SCOPE_RULES,
        "table::CORPUS_FILE_SCOPE_RULES",
    ));
    all.extend(opener_rows(
        CORPUS_INTERIOR_RULES,
        "table::CORPUS_INTERIOR_RULES",
    ));
    all.extend(opener_rows(LIVING_WORLD_RULES, "table::LIVING_WORLD_RULES"));
    all
}

#[derive(Debug, Clone, Copy)]
pub struct ClosedContext {
    pub context: &'static str,
    pub group: &'static str,
}

fn closed_rows(contexts: &'static [&'static str], group: &'static str) -> Vec<ClosedContext> {
    contexts
        .iter()
        .map(|&context| ClosedContext { context, group })
        .collect()
}

#[must_use]
pub fn closed_contexts() -> Vec<ClosedContext> {
    let mut all = Vec::new();
    all.extend(closed_rows(
        OBJECT_MODEL_CLOSED,
        "table::OBJECT_MODEL_CLOSED",
    ));
    all.extend(closed_rows(PARTICLE_CLOSED, "table::PARTICLE_CLOSED"));
    all.extend(closed_rows(
        PRIORITY_BLOCK_CLOSED,
        "table::PRIORITY_BLOCK_CLOSED",
    ));
    all.extend(closed_rows(AUDIO_CLOSED, "table::AUDIO_CLOSED"));
    all.extend(closed_rows(
        HORDE_CONTAIN_CLOSED,
        "table::HORDE_CONTAIN_CLOSED",
    ));
    all.extend(closed_rows(CORPUS_CLOSED, "table::CORPUS_CLOSED"));
    all
}

#[derive(Debug, Clone, Copy)]
pub struct AliasSpec {
    pub from: &'static str,
    pub to: &'static str,
}

#[must_use]
pub fn aliases() -> Vec<AliasSpec> {
    OBJECT_MODEL_ALIASES
        .iter()
        .map(|&(from, to)| AliasSpec { from, to })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_scope_row_is_registry_checkable_and_a_nested_row_is_not() {
        for row in openers() {
            match row.verified {
                Verified::FileScopeRegistry => assert!(
                    row.context.is_empty(),
                    "{}.{} tagged registry-checkable but is nested",
                    row.context,
                    row.keyword
                ),
                Verified::Document(group) => {
                    assert!(
                        !row.context.is_empty(),
                        "{}.{} is file scope but tagged documentary",
                        row.context,
                        row.keyword
                    );
                    assert!(!group.is_empty());
                }
            }
        }
    }

    #[test]
    fn no_closed_context_is_file_scope() {
        for c in closed_contexts() {
            assert!(!c.context.is_empty(), "{} would close file scope", c.group);
        }
    }

    #[test]
    fn the_totals_match_table_rs_own_tally() {
        assert_eq!(openers().len(), 366);
        assert_eq!(closed_contexts().len(), 172);
        assert_eq!(aliases().len(), OBJECT_MODEL_ALIASES.len());
    }

    #[test]
    fn every_file_scope_row_that_the_committed_object_model_table_names_is_present() {
        let file_scope_true: Vec<&str> = openers()
            .iter()
            .filter(|r| {
                r.context.is_empty() && r.opens && r.verified == Verified::FileScopeRegistry
            })
            .map(|r| r.keyword)
            .collect();
        for name in ["Object", "ChildObject", "ObjectReskin", "Road", "Terrain"] {
            assert!(
                file_scope_true.contains(&name),
                "{name} missing from the file-scope openers"
            );
        }
    }
}
