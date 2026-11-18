use std::sync::Arc;

use crate::lex::{decode_latin1, lex, Directive, Lexed};
use crate::source::{normalise_path, resolve_include, Sources};
use crate::table::{BlockOpener, Scope};
use crate::tree::{Block, Define, Diagnostic, Document, Field, Node, Script};
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    pub include_depth: usize,
    pub include_count: usize,
    pub max_statements: usize,
    pub max_block_depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            include_depth: 16,
            include_count: 4096,
            max_statements: 4_000_000,
            max_block_depth: 64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stmt {
    pub file: Arc<str>,
    pub line: u32,
    pub indent: u16,
    pub keyword: String,
    pub args: Vec<String>,
    pub script: Option<String>,
}

impl Stmt {
    #[must_use]
    pub fn is_end(&self) -> bool {
        self.script.is_none() && self.keyword.eq_ignore_ascii_case("End")
    }
}

#[derive(Debug, Clone)]
pub struct Reader<'a, S: Sources + ?Sized> {
    sources: &'a S,
    limits: Limits,
}

impl<'a, S: Sources + ?Sized> Reader<'a, S> {
    #[must_use]
    pub fn new(sources: &'a S) -> Self {
        Self {
            sources,
            limits: Limits::default(),
        }
    }

    #[must_use]
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    #[must_use]
    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    #[allow(clippy::type_complexity)]
    pub fn statements(
        &self,
        path: &str,
    ) -> Result<(Vec<Stmt>, Vec<Diagnostic>, Vec<Arc<str>>, Vec<Define>)> {
        let path = normalise_path(path);
        let Some(bytes) = self.sources.read(&path) else {
            return Err(Error::NotFound { path });
        };

        let mut flat = Flatten {
            sources: Some(self.sources),
            limits: &self.limits,
            stmts: Vec::new(),
            diagnostics: Vec::new(),
            files: Vec::new(),
            defines: Vec::new(),
            open: Vec::new(),
            includes_used: 0,
        };
        let name: Arc<str> = Arc::from(path.as_str());
        flat.ingest(&name, &decode_latin1(&bytes), 0)?;
        Ok((flat.stmts, flat.diagnostics, flat.files, flat.defines))
    }

    pub fn read(&self, path: &str, opener: &dyn BlockOpener) -> Result<Document> {
        let (stmts, mut diagnostics, files, defines) = self.statements(path)?;
        let name: Arc<str> = Arc::from(normalise_path(path).as_str());
        let statements = stmts.len();
        let opens = classify(&stmts, opener);
        let roots = build(stmts, &opens, &self.limits, &mut diagnostics)?;
        Ok(Document {
            name,
            roots,
            diagnostics,
            files,
            statements,
            defines,
        })
    }
}

pub(crate) fn parse_text(
    name: &str,
    text: &str,
    opener: &dyn BlockOpener,
    limits: &Limits,
) -> Result<Document> {
    let name: Arc<str> = Arc::from(normalise_path(name).as_str());
    let mut flat: Flatten<'_, crate::MemorySources> = Flatten {
        sources: None,
        limits,
        stmts: Vec::new(),
        diagnostics: Vec::new(),
        files: Vec::new(),
        defines: Vec::new(),
        open: Vec::new(),
        includes_used: 0,
    };
    flat.ingest(&name, text, 0)?;

    let statements = flat.stmts.len();
    let opens = classify(&flat.stmts, opener);
    let mut diagnostics = flat.diagnostics;
    let roots = build(flat.stmts, &opens, limits, &mut diagnostics)?;
    Ok(Document {
        name,
        roots,
        diagnostics,
        files: flat.files,
        statements,
        defines: flat.defines,
    })
}

struct Flatten<'a, S: Sources + ?Sized> {
    sources: Option<&'a S>,
    limits: &'a Limits,
    stmts: Vec<Stmt>,
    diagnostics: Vec<Diagnostic>,
    files: Vec<Arc<str>>,
    defines: Vec<Define>,
    open: Vec<String>,
    includes_used: usize,
}

impl<S: Sources + ?Sized> Flatten<'_, S> {
    fn ingest(&mut self, file: &Arc<str>, source: &str, depth: usize) -> Result<()> {
        self.files.push(Arc::clone(file));
        self.open.push(file.to_string());

        for item in lex(source) {
            if self.stmts.len() >= self.limits.max_statements {
                return Err(Error::TooManyStatements {
                    limit: self.limits.max_statements,
                });
            }
            match item {
                Lexed::Line(line) => self.stmts.push(Stmt {
                    file: Arc::clone(file),
                    line: line.number,
                    indent: line.indent,
                    keyword: line.keyword.to_owned(),
                    args: line.args.iter().map(|a| (*a).to_owned()).collect(),
                    script: None,
                }),
                Lexed::Script {
                    number,
                    indent,
                    body,
                } => self.stmts.push(Stmt {
                    file: Arc::clone(file),
                    line: number,
                    indent,
                    keyword: "BeginScript".to_owned(),
                    args: Vec::new(),
                    script: Some(body),
                }),
                Lexed::Directive { number, directive } => {
                    self.directive(file, number, &directive, depth)?;
                }
            }
        }

        self.open.pop();
        Ok(())
    }

    fn directive(
        &mut self,
        file: &Arc<str>,
        line: u32,
        directive: &Directive<'_>,
        depth: usize,
    ) -> Result<()> {
        match *directive {
            Directive::Define { name, body } => {
                self.defines.push(Define {
                    name: name.to_owned(),
                    body: body.to_owned(),
                    file: Arc::clone(file),
                    line,
                });
                Ok(())
            }
            Directive::Other(raw) => {
                self.diagnostics.push(Diagnostic::UnknownDirective {
                    file: Arc::clone(file),
                    line,
                    raw: raw.to_owned(),
                });
                Ok(())
            }
            Directive::Include(target) => self.include(file, line, target, depth),
        }
    }

    fn include(&mut self, file: &Arc<str>, line: u32, target: &str, depth: usize) -> Result<()> {
        let resolved = resolve_include(file, target);

        let Some(next_depth) = depth
            .checked_add(1)
            .filter(|d| *d <= self.limits.include_depth)
        else {
            return Err(Error::IncludeDepth {
                file: file.to_string(),
                line,
                limit: self.limits.include_depth,
            });
        };
        if self.open.contains(&resolved) {
            return Err(Error::IncludeCycle {
                file: file.to_string(),
                line,
                path: resolved,
            });
        }
        self.includes_used = self.includes_used.saturating_add(1);
        if self.includes_used > self.limits.include_count {
            return Err(Error::TooManyIncludes {
                limit: self.limits.include_count,
            });
        }

        let bytes = self.sources.and_then(|s| s.read(&resolved));
        let Some(bytes) = bytes else {
            self.diagnostics.push(Diagnostic::IncludeMissing {
                file: Arc::clone(file),
                line,
                target: resolved,
            });
            return Ok(());
        };

        let name: Arc<str> = Arc::from(resolved.as_str());
        self.ingest(&name, &decode_latin1(&bytes), next_depth)
    }
}

fn classify(stmts: &[Stmt], opener: &dyn BlockOpener) -> Vec<bool> {
    let mut opens = vec![false; stmts.len()];
    let mut context: Vec<Scope> = Vec::new();

    for (i, stmt) in stmts.iter().enumerate() {
        if stmt.is_end() {
            context.pop();
            continue;
        }
        if stmt.script.is_some() {
            continue;
        }

        let decided = opener
            .opens_block(&context, &stmt.keyword, &stmt.args)
            .unwrap_or(false);

        if let Some(slot) = opens.get_mut(i) {
            *slot = decided;
        }
        if decided {
            context.push(match stmt.args.first() {
                Some(selector) => Scope::slot(stmt.keyword.as_str(), selector.as_str()),
                None => Scope::new(stmt.keyword.as_str()),
            });
        }
    }

    opens
}

fn build(
    stmts: Vec<Stmt>,
    opens: &[bool],
    limits: &Limits,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<Block>> {
    let mut roots: Vec<Block> = Vec::new();
    let mut stack: Vec<Block> = Vec::new();

    for (i, stmt) in stmts.into_iter().enumerate() {
        let is_end = stmt.is_end();
        let Stmt {
            file,
            line,
            keyword,
            args,
            script,
            ..
        } = stmt;

        if is_end {
            match stack.pop() {
                Some(done) => close(&mut stack, &mut roots, done),
                None => diagnostics.push(Diagnostic::StrayEnd { file, line }),
            }
            continue;
        }

        if let Some(body) = script {
            let node = Node::Script(Script {
                body,
                file: Arc::clone(&file),
                line,
            });
            match stack.last_mut() {
                Some(parent) => parent.children.push(node),
                None => diagnostics.push(Diagnostic::FieldAtFileScope {
                    file,
                    line,
                    keyword,
                }),
            }
            continue;
        }

        if opens.get(i).copied().unwrap_or(false) {
            if stack.len() >= limits.max_block_depth {
                return Err(Error::TooDeep {
                    file: file.to_string(),
                    line,
                    limit: limits.max_block_depth,
                });
            }
            if stack
                .last()
                .is_some_and(|parent| parent.keyword.eq(&keyword))
            {
                diagnostics.push(Diagnostic::SelfNested {
                    file: Arc::clone(&file),
                    line,
                    keyword: keyword.clone(),
                });
            }
            stack.push(Block {
                keyword,
                args,
                children: Vec::new(),
                file,
                line,
            });
        } else {
            match stack.last_mut() {
                Some(parent) => parent.children.push(Node::Field(Field {
                    keyword,
                    args,
                    file,
                    line,
                })),
                None => diagnostics.push(Diagnostic::FieldAtFileScope {
                    file,
                    line,
                    keyword,
                }),
            }
        }
    }

    while let Some(open) = stack.pop() {
        diagnostics.push(Diagnostic::UnclosedBlock {
            file: Arc::clone(&open.file),
            line: open.line,
            keyword: open.keyword.clone(),
        });
        close(&mut stack, &mut roots, open);
    }

    Ok(roots)
}

fn close(stack: &mut [Block], roots: &mut Vec<Block>, done: Block) {
    match stack.last_mut() {
        Some(parent) => parent.children.push(Node::Block(done)),
        None => roots.push(done),
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests;
