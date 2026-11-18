use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Title {
    Bfme2,
    Rotwk,
}

#[derive(Debug, Default)]
pub(crate) struct Node {
    pub(crate) keyword: String,
    pub(crate) arg: Option<String>,
    pub(crate) statements: BTreeMap<String, u64>,
    pub(crate) children: BTreeMap<(String, Option<String>), usize>,
}

pub(crate) struct Corpus {
    pub(crate) title: Title,
    pub(crate) nodes: Vec<Node>,
    pub(crate) roots: BTreeMap<String, usize>,
}

impl Corpus {
    pub(crate) fn load(root: &Path, install: &Path) -> Result<Self, String> {
        let output = std::process::Command::new("cargo")
            .args([
                "run",
                "--release",
                "-q",
                "-p",
                "mint-sage-ini",
                "--example",
                "schema_coverage",
                "--",
                "--contexts",
            ])
            .arg(install)
            .current_dir(root)
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }
        Self::parse(&String::from_utf8_lossy(&output.stdout))
    }

    fn parse(text: &str) -> Result<Self, String> {
        let mut corpus = Self {
            title: Title::Bfme2,
            nodes: Vec::new(),
            roots: BTreeMap::new(),
        };
        let mut title = None;
        for line in text.lines() {
            let cols: Vec<&str> = line.split('\t').collect();
            match cols.as_slice() {
                ["T", _, "rotwk"] => title = Some(Title::Rotwk),
                ["T", _, "bfme2"] => title = Some(Title::Bfme2),
                ["S", _, path, token, _kind, count] => {
                    let count: u64 = count.parse().map_err(|_| format!("bad count: {line}"))?;
                    let node = corpus.node(path);
                    if let Some(n) = corpus.nodes.get_mut(node) {
                        *n.statements.entry((*token).to_owned()).or_default() += count;
                    }
                }
                _ => {}
            }
        }
        corpus.title = title.ok_or("the corpus dump named no title")?;
        Ok(corpus)
    }

    fn node(&mut self, path: &str) -> usize {
        let mut parts = path.split('/');
        let root_keyword = parts.next().unwrap_or_default().to_owned();
        let mut current = match self.roots.get(&root_keyword) {
            Some(&id) => id,
            None => {
                let id = self.nodes.len();
                self.nodes.push(Node {
                    keyword: root_keyword.clone(),
                    ..Node::default()
                });
                self.roots.insert(root_keyword, id);
                id
            }
        };
        for part in parts {
            let (keyword, arg) = match part.split_once('=') {
                Some((k, a)) => (k.to_owned(), Some(a.to_owned())),
                None => (part.to_owned(), None),
            };
            let key = (keyword.clone(), arg.clone());
            let existing = self
                .nodes
                .get(current)
                .and_then(|n| n.children.get(&key))
                .copied();
            current = match existing {
                Some(id) => id,
                None => {
                    let id = self.nodes.len();
                    self.nodes.push(Node {
                        keyword,
                        arg,
                        ..Node::default()
                    });
                    if let Some(parent) = self.nodes.get_mut(current) {
                        parent.children.insert(key, id);
                    }
                    id
                }
            };
        }
        current
    }

    pub(crate) fn subtree_statements(&self, node: usize) -> u64 {
        let Some(n) = self.nodes.get(node) else {
            return 0;
        };
        n.statements.values().sum::<u64>()
            + n.children
                .values()
                .map(|&c| self.subtree_statements(c))
                .sum::<u64>()
    }
}
