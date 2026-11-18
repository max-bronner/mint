use super::binary::{Binary, Record, Table, TableKey};
use super::corpus::{Corpus, Node};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum How {
    RegistrationProc,
    RegistrationCallee,
    ModuleFactory,
    UserData,
    FieldProc,
    FieldProcCallee,
    Vocabulary,
    Vote,
    Counterpart,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Bound {
    pub(crate) tables: Vec<(usize, How)>,
}

#[derive(Debug, Clone)]
pub(crate) enum Target {
    Fixed(String),
    Typed(BTreeMap<String, String>),
    Module,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Context {
    pub(crate) bound: Bound,
    pub(crate) children: BTreeMap<String, Target>,
}

pub(crate) struct TitleBinding {
    pub(crate) contexts: BTreeMap<String, Context>,
    pub(crate) file_scope: BTreeMap<String, String>,
    pub(crate) modules: BTreeMap<String, String>,
    pub(crate) unbound: Vec<(String, u64)>,
}

type Counts = BTreeMap<String, u64>;

struct Binder<'a> {
    bin: &'a mut Binary,
    corpus: &'a Corpus,
    index: BTreeMap<String, Vec<usize>>,
    edges: BTreeMap<(usize, String), EdgeBinding>,
    modules: BTreeMap<String, Option<Bound>>,
    module_counts: BTreeMap<String, Counts>,
    module_openers: BTreeMap<String, BTreeSet<String>>,
    out: TitleBinding,
}

#[derive(Clone)]
enum EdgeBinding {
    Fixed(Bound),
    Typed(BTreeMap<String, Bound>),
    Module,
    None,
}

const MODULE_SLOT_SHARE: f64 = 0.9;
const ACCEPT: f64 = 0.5;
const MAX_VOTED_TABLES: usize = 4;

pub(crate) fn bind(bin: &mut Binary, corpus: &Corpus) -> TitleBinding {
    let mut index: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, table) in bin.tables.iter().enumerate() {
        for token in table.tokens() {
            let slot = index.entry(token.to_owned()).or_default();
            if !slot.contains(&i) {
                slot.push(i);
            }
        }
    }
    let mut module_counts: BTreeMap<String, Counts> = BTreeMap::new();
    let mut module_openers: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for node in &corpus.nodes {
        if let Some(arg) = &node.arg {
            if bin.modules.contains_key(arg) {
                add_counts(
                    module_counts.entry(arg.clone()).or_default(),
                    &node.statements,
                );
                let openers = module_openers.entry(arg.clone()).or_default();
                openers.extend(node.children.keys().map(|(k, _)| k.clone()));
            }
        }
    }
    let mut binder = Binder {
        bin,
        corpus,
        index,
        edges: BTreeMap::new(),
        modules: BTreeMap::new(),
        module_counts,
        module_openers,
        out: TitleBinding {
            contexts: BTreeMap::new(),
            file_scope: BTreeMap::new(),
            modules: BTreeMap::new(),
            unbound: Vec::new(),
        },
    };
    binder.run();
    binder.out
}

fn add_counts(into: &mut Counts, from: &Counts) {
    for (token, count) in from {
        *into.entry(token.clone()).or_default() += count;
    }
}

impl Binder<'_> {
    fn names(&self, table: usize) -> BTreeSet<&str> {
        self.bin
            .tables
            .get(table)
            .map(|t| t.tokens().collect())
            .unwrap_or_default()
    }

    fn wildcard(&self, table: usize) -> bool {
        self.bin.tables.get(table).is_some_and(Table::is_wildcard)
    }

    fn score(&self, tables: &[usize], counts: &Counts) -> f64 {
        let total: u64 = counts.values().sum();
        if total == 0 || tables.iter().any(|&t| self.wildcard(t)) {
            return 1.0;
        }
        let names: BTreeSet<&str> = tables.iter().flat_map(|&t| self.names(t)).collect();
        let covered: u64 = counts
            .iter()
            .filter(|(k, _)| names.contains(k.as_str()))
            .map(|(_, c)| c)
            .sum();
        covered as f64 / total as f64
    }

    fn vote(&self, counts: &Counts, already: &[usize], exclude: &[usize]) -> Vec<usize> {
        let mut chosen: Vec<usize> = Vec::new();
        let mut remaining = counts.clone();
        for &t in already {
            let names = self.names(t);
            remaining.retain(|k, _| !names.contains(k.as_str()));
        }
        while chosen.len() < MAX_VOTED_TABLES && !remaining.is_empty() {
            let rest: u64 = remaining.values().sum();
            let mut score: BTreeMap<usize, (u64, usize)> = BTreeMap::new();
            for (token, count) in &remaining {
                for &t in self.index.get(token).into_iter().flatten() {
                    if chosen.contains(&t) || already.contains(&t) || exclude.contains(&t) {
                        continue;
                    }
                    let entry = score.entry(t).or_default();
                    entry.0 += count;
                    entry.1 += 1;
                }
            }
            let free = chosen.is_empty() && already.is_empty();
            let unique = |t: usize| {
                remaining.keys().any(|k| {
                    self.index
                        .get(k)
                        .is_some_and(|ts| ts.len() == 1 && ts.contains(&t))
                })
            };
            let best = score
                .into_iter()
                .filter(|&(t, (covered, distinct))| {
                    free || (covered as f64 >= 0.5 * rest as f64 && (distinct >= 2 || unique(t)))
                })
                .max_by_key(|&(t, (covered, _))| {
                    let size = self.bin.tables.get(t).map_or(0, |x| x.records.len());
                    (covered, std::cmp::Reverse(size), std::cmp::Reverse(t))
                });
            let Some((t, _)) = best else { break };
            let names = self.names(t);
            remaining.retain(|k, _| !names.contains(k.as_str()));
            chosen.push(t);
        }
        chosen
    }

    fn vocabulary(&mut self, tables: &[usize], openers: &BTreeSet<String>) -> Option<usize> {
        let names: BTreeSet<&str> = tables.iter().flat_map(|&t| self.names(t)).collect();
        let missing: BTreeSet<&str> = openers
            .iter()
            .map(String::as_str)
            .filter(|k| !names.contains(k))
            .collect();
        if missing.len() < 2 {
            return None;
        }
        let (hit, _, va) = self
            .bin
            .vocabularies
            .iter()
            .map(|(va, words)| {
                let distinct: BTreeSet<&str> = words.iter().map(String::as_str).collect();
                let hit = missing.intersection(&distinct).count();
                (hit, std::cmp::Reverse(distinct.len()), *va)
            })
            .max()?;
        if hit * 5 < missing.len() * 4 {
            return None;
        }
        if let Some(existing) = self
            .bin
            .tables
            .iter()
            .position(|t| t.key == TableKey::Vocabulary(va))
        {
            return Some(existing);
        }
        let words = self.bin.vocabularies.get(&va).cloned().unwrap_or_default();
        let mut seen = BTreeSet::new();
        let records = words
            .into_iter()
            .filter(|w| seen.insert(w.clone()))
            .map(|w| Record {
                token: Some(w),
                proc_va: 0,
                user_data: 0,
            })
            .collect();
        let id = self.bin.tables.len();
        self.bin.tables.push(Table {
            key: TableKey::Vocabulary(va),
            records,
        });
        Some(id)
    }

    fn settle(
        &mut self,
        direct: Vec<(usize, How)>,
        callee: Vec<(usize, How)>,
        counts: &Counts,
        openers: &BTreeSet<String>,
        exclude: &[usize],
    ) -> Option<Bound> {
        let mut chosen = direct;
        let mut covered: BTreeSet<String> = chosen
            .iter()
            .flat_map(|&(t, _)| self.names(t))
            .map(str::to_owned)
            .collect();
        for (t, how) in callee {
            let names = self.names(t);
            if counts
                .keys()
                .any(|k| names.contains(k.as_str()) && !covered.contains(k))
            {
                covered.extend(names.into_iter().map(str::to_owned));
                chosen.push((t, how));
            }
        }
        let tables: Vec<usize> = chosen.iter().map(|&(t, _)| t).collect();
        if !counts.is_empty() && self.score(&tables, counts) == 0.0 {
            chosen.clear();
        }
        let from_binary = !chosen.is_empty();
        let tables: Vec<usize> = chosen.iter().map(|&(t, _)| t).collect();
        if let Some(v) = self.vocabulary(&tables, openers) {
            chosen.push((v, How::Vocabulary));
        }
        let tables: Vec<usize> = chosen.iter().map(|&(t, _)| t).collect();
        for t in self.vote(counts, &tables, exclude) {
            chosen.push((t, How::Vote));
        }
        let tables: Vec<usize> = chosen.iter().map(|&(t, _)| t).collect();
        if chosen.is_empty() || (!from_binary && self.score(&tables, counts) < ACCEPT) {
            return None;
        }
        Some(Bound { tables: chosen })
    }

    fn module(&mut self, name: &str) -> Option<Bound> {
        if let Some(cached) = self.modules.get(name) {
            return cached.clone();
        }
        let counts = self.module_counts.get(name).cloned().unwrap_or_default();
        let openers = self.module_openers.get(name).cloned().unwrap_or_default();
        let bound = match self.bin.modules.get(name).copied() {
            Some(data_proc) => {
                let tables = self.bin.builder_tables(data_proc).unwrap_or_default();
                let direct = tables
                    .into_iter()
                    .map(|t| (t, How::ModuleFactory))
                    .collect();
                Some(
                    self.settle(direct, Vec::new(), &counts, &openers, &[])
                        .unwrap_or_default(),
                )
            }
            None => self.settle(Vec::new(), Vec::new(), &counts, &openers, &[]),
        };
        self.modules.insert(name.to_owned(), bound.clone());
        bound
    }

    fn file_scope(
        &mut self,
        name: &str,
        counts: &Counts,
        openers: &BTreeSet<String>,
    ) -> Option<Bound> {
        let (direct, callee) = match self.bin.registrations.get(name).copied() {
            Some(proc_va) => self.bin.reach(proc_va),
            None => (Vec::new(), Vec::new()),
        };
        let direct = direct
            .into_iter()
            .map(|t| (t, How::RegistrationProc))
            .collect();
        let callee = callee
            .into_iter()
            .map(|t| (t, How::RegistrationCallee))
            .collect();
        self.settle(direct, callee, counts, openers, &[])
    }

    fn edge(
        &mut self,
        owner: usize,
        keyword: &str,
        children: &[usize],
        parents: &[usize],
    ) -> EdgeBinding {
        let key = (owner, keyword.to_owned());
        if let Some(cached) = self.edges.get(&key) {
            return cached.clone();
        }
        let binding = self.bind_edge(owner, keyword, children, parents);
        self.edges.insert(key, binding.clone());
        binding
    }

    fn bind_edge(
        &mut self,
        owner: usize,
        keyword: &str,
        children: &[usize],
        parents: &[usize],
    ) -> EdgeBinding {
        let nodes: Vec<&Node> = children
            .iter()
            .filter_map(|&c| self.corpus.nodes.get(c))
            .collect();
        let modules = nodes
            .iter()
            .filter(|n| {
                n.arg
                    .as_ref()
                    .is_some_and(|a| self.bin.modules.contains_key(a))
            })
            .count();
        if !nodes.is_empty() && modules as f64 >= MODULE_SLOT_SHARE * nodes.len() as f64 {
            return EdgeBinding::Module;
        }
        let mut pooled = Counts::new();
        let mut openers = BTreeSet::new();
        let mut per_arg: BTreeMap<String, (Counts, BTreeSet<String>)> = BTreeMap::new();
        for n in &nodes {
            add_counts(&mut pooled, &n.statements);
            let slot = per_arg
                .entry(n.arg.clone().unwrap_or_default())
                .or_default();
            add_counts(&mut slot.0, &n.statements);
            for (k, _) in n.children.keys() {
                openers.insert(k.clone());
                slot.1.insert(k.clone());
            }
        }

        let record = self
            .bin
            .tables
            .get(owner)
            .and_then(|t| {
                t.records
                    .iter()
                    .find(|r| r.token.as_deref() == Some(keyword))
            })
            .cloned();
        let (mut direct, mut callee) = (Vec::new(), Vec::new());
        if let Some(record) = record {
            if let Some(t) = self.bin.table_at(record.user_data) {
                direct.push((t, How::UserData));
            }
            if self.bin.in_text(record.proc_va) {
                let (own, called) = self.bin.reach(record.proc_va);
                let built = self.bin.builder_tables(record.proc_va).unwrap_or_default();
                for t in own.into_iter().chain(built) {
                    if !direct.iter().any(|&(d, _)| d == t) {
                        direct.push((t, How::FieldProc));
                    }
                }
                callee = called
                    .into_iter()
                    .map(|t| (t, How::FieldProcCallee))
                    .collect();
            }
        }
        if !direct.is_empty() || !callee.is_empty() {
            let accepted = self.settle(direct.clone(), callee.clone(), &pooled, &openers, parents);
            if let Some(bound) = accepted {
                if bound.tables.iter().any(|&(_, how)| how != How::Vote) {
                    return EdgeBinding::Fixed(bound);
                }
            }
        }

        let total: u64 = pooled.values().sum();
        let pooled_vote = self.settle(Vec::new(), Vec::new(), &pooled, &openers, parents);
        let pooled_tables: Vec<usize> = pooled_vote
            .as_ref()
            .map(|b| b.tables.iter().map(|&(t, _)| t).collect())
            .unwrap_or_default();
        let pooled_covered = self.score(&pooled_tables, &pooled) * total as f64;
        let mut typed: BTreeMap<String, Bound> = BTreeMap::new();
        let mut typed_covered = 0.0;
        let mut winners: BTreeSet<Vec<usize>> = BTreeSet::new();
        for (arg, (counts, arg_openers)) in &per_arg {
            if let Some(bound) = self.settle(Vec::new(), Vec::new(), counts, arg_openers, parents) {
                let tables: Vec<usize> = bound.tables.iter().map(|&(t, _)| t).collect();
                typed_covered += self.score(&tables, counts) * counts.values().sum::<u64>() as f64;
                winners.insert(tables);
                typed.insert(arg.clone(), bound);
            }
        }
        let typed_args = nodes
            .iter()
            .filter(|n| n.arg.as_ref().is_some_and(|a| self.bin.strings.contains(a)))
            .count();
        let has_types = typed_args as f64 >= MODULE_SLOT_SHARE * nodes.len() as f64;
        if has_types && winners.len() > 1 && typed_covered >= pooled_covered {
            return EdgeBinding::Typed(typed);
        }
        match pooled_vote {
            Some(bound) => EdgeBinding::Fixed(bound),
            None => EdgeBinding::None,
        }
    }

    fn register(&mut self, key: &str, bound: Bound) {
        self.out.contexts.entry(key.to_owned()).or_insert(Context {
            bound,
            children: BTreeMap::new(),
        });
    }

    fn run(&mut self) {
        let mut level: Vec<(usize, String, Bound)> = Vec::new();
        let roots: Vec<(String, usize)> = self
            .corpus
            .roots
            .iter()
            .map(|(k, &v)| (k.clone(), v))
            .collect();
        for (name, node) in roots {
            let (counts, openers) = self
                .corpus
                .nodes
                .get(node)
                .map(|n| {
                    let openers = n.children.keys().map(|(k, _)| k.clone()).collect();
                    (n.statements.clone(), openers)
                })
                .unwrap_or_default();
            match self.file_scope(&name, &counts, &openers) {
                Some(bound) => {
                    let key = format!("F:{name}");
                    self.register(&key, bound.clone());
                    self.out.file_scope.insert(name, key.clone());
                    level.push((node, key, bound));
                }
                None => {
                    let below = self.corpus.subtree_statements(node);
                    self.out.unbound.push((name, below));
                }
            }
        }
        let registered: Vec<String> = self.bin.registrations.keys().cloned().collect();
        for name in registered {
            if self.out.file_scope.contains_key(&name) {
                continue;
            }
            if let Some(bound) = self.file_scope(&name, &Counts::new(), &BTreeSet::new()) {
                let key = format!("F:{name}");
                self.register(&key, bound);
                self.out.file_scope.insert(name, key);
            }
        }
        let names: Vec<String> = self.bin.modules.keys().cloned().collect();
        for name in names {
            if let Some(bound) = self.module(&name) {
                let key = format!("M:{name}");
                self.register(&key, bound);
                self.out.modules.insert(name, key);
            }
        }

        let mut depth = 0;
        while !level.is_empty() && depth < 16 {
            depth += 1;
            let mut groups: BTreeMap<(usize, String), Vec<(usize, String)>> = BTreeMap::new();
            let mut enclosing: BTreeMap<(usize, String), BTreeSet<usize>> = BTreeMap::new();
            let mut orphans: Vec<(usize, String)> = Vec::new();
            for (node, key, bound) in &level {
                let Some(n) = self.corpus.nodes.get(*node) else {
                    continue;
                };
                for ((keyword, _), &child) in &n.children {
                    let owner = bound
                        .tables
                        .iter()
                        .map(|&(t, _)| t)
                        .find(|&t| self.names(t).contains(keyword.as_str()));
                    match owner {
                        Some(owner) => {
                            let group = (owner, keyword.clone());
                            enclosing
                                .entry(group.clone())
                                .or_default()
                                .extend(bound.tables.iter().map(|&(t, _)| t));
                            groups.entry(group).or_default().push((child, key.clone()));
                        }
                        None => orphans.push((child, key.clone())),
                    }
                }
            }
            for (child, parent) in orphans {
                let below = self.corpus.subtree_statements(child);
                let label = self.child_label(&parent, child);
                self.out.unbound.push((label, below));
            }
            let mut next = Vec::new();
            for ((owner, keyword), members) in groups {
                let children: Vec<usize> = members.iter().map(|&(c, _)| c).collect();
                let parents: Vec<usize> = enclosing
                    .get(&(owner, keyword.clone()))
                    .map(|s| s.iter().copied().collect())
                    .unwrap_or_default();
                let binding = self.edge(owner, &keyword, &children, &parents);
                for (child, parent) in members {
                    let arg = self
                        .corpus
                        .nodes
                        .get(child)
                        .and_then(|n| n.arg.clone())
                        .unwrap_or_default();
                    let resolved: Option<(String, Bound, Target)> = match &binding {
                        EdgeBinding::Module => {
                            let key = format!("M:{arg}");
                            self.module(&arg).map(|b| (key, b, Target::Module))
                        }
                        EdgeBinding::Fixed(bound) => {
                            let key = format!("{parent}/{keyword}");
                            Some((key.clone(), bound.clone(), Target::Fixed(key)))
                        }
                        EdgeBinding::Typed(map) => map.get(&arg).map(|bound| {
                            let key = format!("{parent}/{keyword}={arg}");
                            let mut targets = BTreeMap::new();
                            targets.insert(arg.clone(), key.clone());
                            (key, bound.clone(), Target::Typed(targets))
                        }),
                        EdgeBinding::None => None,
                    };
                    let Some((key, bound, target)) = resolved else {
                        let below = self.corpus.subtree_statements(child);
                        let label = self.child_label(&parent, child);
                        self.out.unbound.push((label, below));
                        continue;
                    };
                    self.register(&key, bound.clone());
                    if let Some(ctx) = self.out.contexts.get_mut(&parent) {
                        match (ctx.children.get_mut(&keyword), target) {
                            (Some(Target::Typed(existing)), Target::Typed(add)) => {
                                existing.extend(add);
                            }
                            (None, target) => {
                                ctx.children.insert(keyword.clone(), target);
                            }
                            _ => {}
                        }
                    }
                    next.push((child, key, bound));
                }
            }
            level = next;
        }
    }

    fn child_label(&self, parent: &str, child: usize) -> String {
        match self.corpus.nodes.get(child) {
            Some(n) => match &n.arg {
                Some(arg) => format!("{parent}/{}={arg}", n.keyword),
                None => format!("{parent}/{}", n.keyword),
            },
            None => parent.to_owned(),
        }
    }
}
