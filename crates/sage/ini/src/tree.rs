use std::sync::Arc;

use crate::table::{BlockOpener, Scope};
use crate::{reader, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub keyword: String,
    pub args: Vec<String>,
    pub file: Arc<str>,
    pub line: u32,
}

impl Field {
    #[must_use]
    pub fn value(&self) -> Option<&str> {
        self.args.first().map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    pub body: String,
    pub file: Arc<str>,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Field(Field),
    Block(Block),
    Script(Script),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub keyword: String,
    pub args: Vec<String>,
    pub children: Vec<Node>,
    pub file: Arc<str>,
    pub line: u32,
}

impl Block {
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.args.first().map(String::as_str)
    }

    #[must_use]
    pub fn selector(&self) -> Option<&str> {
        self.args.first().map(String::as_str)
    }

    #[must_use]
    pub fn scope(&self) -> Scope {
        match self.args.first() {
            Some(selector) => Scope::slot(self.keyword.as_str(), selector.as_str()),
            None => Scope::new(self.keyword.as_str()),
        }
    }

    pub fn fields(&self) -> impl Iterator<Item = &Field> {
        self.children.iter().filter_map(|n| match n {
            Node::Field(f) => Some(f),
            _ => None,
        })
    }

    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.children.iter().filter_map(|n| match n {
            Node::Block(b) => Some(b),
            _ => None,
        })
    }

    pub fn scripts(&self) -> impl Iterator<Item = &Script> {
        self.children.iter().filter_map(|n| match n {
            Node::Script(s) => Some(s),
            _ => None,
        })
    }

    #[must_use]
    pub fn field(&self, keyword: &str) -> Option<&Field> {
        #[cfg(feature = "coverage")]
        crate::coverage::record(&self.keyword, self.selector(), keyword);
        self.fields().find(|f| f.keyword == keyword)
    }

    #[must_use]
    pub fn overlaid(&self, over: &Block) -> Block {
        let rewritten = |keyword: &str| over.fields().any(|field| field.keyword == keyword);
        let mut children: Vec<Node> = self
            .children
            .iter()
            .filter(|node| !matches!(node, Node::Field(field) if rewritten(&field.keyword)))
            .cloned()
            .collect();
        children.extend(over.children.iter().cloned());
        Block {
            keyword: self.keyword.clone(),
            args: self.args.clone(),
            children,
            file: Arc::clone(&over.file),
            line: over.line,
        }
    }

    pub fn fields_named<'a>(&'a self, keyword: &'a str) -> impl Iterator<Item = &'a Field> + 'a {
        #[cfg(feature = "coverage")]
        crate::coverage::record(&self.keyword, self.selector(), keyword);
        self.fields().filter(move |f| f.keyword == keyword)
    }

    pub fn blocks_named<'a>(&'a self, keyword: &'a str) -> impl Iterator<Item = &'a Block> + 'a {
        self.blocks().filter(move |b| b.keyword == keyword)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Define {
    pub name: String,
    pub body: String,
    pub file: Arc<str>,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Diagnostic {
    #[error("{file}:{line}: include not found: {target}")]
    IncludeMissing {
        file: Arc<str>,
        line: u32,
        target: String,
    },

    #[error("{file}:{line}: unknown directive #{raw}")]
    UnknownDirective {
        file: Arc<str>,
        line: u32,
        raw: String,
    },

    #[error("{file}:{line}: `End` with no block open")]
    StrayEnd { file: Arc<str>, line: u32 },

    #[error("{file}:{line}: block `{keyword}` was never closed")]
    UnclosedBlock {
        file: Arc<str>,
        line: u32,
        keyword: String,
    },

    #[error("{file}:{line}: field `{keyword}` outside any block")]
    FieldAtFileScope {
        file: Arc<str>,
        line: u32,
        keyword: String,
    },

    #[error("{file}:{line}: block `{keyword}` opened directly inside a block of the same keyword")]
    SelfNested {
        file: Arc<str>,
        line: u32,
        keyword: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub(crate) name: Arc<str>,
    pub(crate) roots: Vec<Block>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) files: Vec<Arc<str>>,
    pub(crate) statements: usize,
    pub(crate) defines: Vec<Define>,
}

impl Document {
    pub fn parse(name: &str, text: &str, opener: &dyn BlockOpener) -> Result<Self> {
        reader::parse_text(name, text, opener, &crate::Limits::default())
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn roots(&self) -> &[Block] {
        &self.roots
    }

    pub fn roots_named<'a>(&'a self, keyword: &'a str) -> impl Iterator<Item = &'a Block> + 'a {
        self.roots.iter().filter(move |b| b.keyword == keyword)
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub fn files(&self) -> &[Arc<str>] {
        &self.files
    }

    #[must_use]
    pub fn statements(&self) -> usize {
        self.statements
    }

    #[must_use]
    pub fn defines(&self) -> &[Define] {
        &self.defines
    }

    #[must_use]
    pub fn is_balanced(&self) -> bool {
        !self.diagnostics.iter().any(|d| {
            matches!(
                d,
                Diagnostic::StrayEnd { .. } | Diagnostic::UnclosedBlock { .. }
            )
        })
    }

    #[must_use]
    pub fn has_self_nesting(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| matches!(d, Diagnostic::SelfNested { .. }))
    }

    pub fn expand_token_list_macros(&mut self, macros: &crate::value::MacroTable) {
        for block in &mut self.roots {
            expand_block_macros(block, macros);
        }
    }
}

fn expand_block_macros(block: &mut Block, macros: &crate::value::MacroTable) {
    crate::value::expand_token_list_macros(&mut block.args, macros);
    for child in &mut block.children {
        match child {
            Node::Field(field) => crate::value::expand_token_list_macros(&mut field.args, macros),
            Node::Block(inner) => expand_block_macros(inner, macros),
            Node::Script(_) => {}
        }
    }
}
