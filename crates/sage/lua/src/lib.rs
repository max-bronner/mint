#![no_std]
#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

extern crate alloc;

mod ast;
mod error;
mod eval;
mod lex;
mod parse;
mod value;

#[cfg(test)]
mod tests;

use alloc::vec::Vec;

pub use ast::Function;
pub use error::{EvalError, EvalErrorKind, HostError, ParseError, ParseErrorKind};
pub use eval::{Budget, Host, BASE_LIBRARY};
pub use value::{Env, Value};

pub mod limits {
    pub const MAX_SOURCE_BYTES: usize = 256 * 1024;
    pub const MAX_TOKENS: usize = 32_768;
    pub const MAX_DEPTH: u32 = 24;
    pub const MAX_ARGS: usize = 16;
    pub const MAX_CALL_DEPTH: u32 = 16;
    pub const MAX_STRING_BYTES: usize = 4096;
    pub const MAX_VARIABLES: usize = 64;
    pub const DEFAULT_BUDGET: u32 = 10_000;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    body: Vec<ast::Stmt>,
}

impl Script {
    pub fn parse(source: &str) -> Result<Self, ParseError> {
        let tokens = lex::lex(source, lex::Comments::HostFile)?;
        let body = parse::Parser::new(&tokens).parse_script()?;
        Ok(Self { body })
    }

    pub fn run<H: Host + ?Sized>(
        &self,
        env: &mut Env,
        host: &mut H,
        budget: &mut Budget,
    ) -> Result<Value, EvalError> {
        eval::Run::new(&[], host, budget).body(&self.body, env)
    }

    #[must_use]
    pub fn call_names(&self) -> Vec<&str> {
        let mut out = Vec::new();
        ast::collect_calls(&self.body, &mut out);
        out
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.body.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.body.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    functions: Vec<Function>,
}

impl Chunk {
    pub fn parse(source: &str) -> Result<Self, ParseError> {
        let tokens = lex::lex(source, lex::Comments::LuaFile)?;
        let functions = parse::Parser::new(&tokens).parse_chunk()?;
        parse::check_unique(&functions)?;
        Ok(Self { functions })
    }

    #[must_use]
    pub fn function(&self, name: &str) -> Option<&Function> {
        self.functions
            .iter()
            .find(|function| function.name() == name)
    }

    pub fn functions(&self) -> impl Iterator<Item = &Function> {
        self.functions.iter()
    }

    pub fn call<H: Host + ?Sized>(
        &self,
        name: &str,
        args: &[Value],
        host: &mut H,
        budget: &mut Budget,
    ) -> Result<Value, EvalError> {
        let Some(function) = self.function(name) else {
            return Err(EvalError {
                line: 0,
                kind: EvalErrorKind::UnknownVariable(alloc::string::String::from(name)),
            });
        };
        eval::Run::new(&self.functions, host, budget).function(function, args, function.line)
    }
}
