use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::ast::{BinOp, Call, Expr, Function, Stmt, UnOp};
use crate::error::{ParseError, ParseErrorKind};
use crate::lex::{Kind, Kw, Sym, Token};
use crate::limits::{MAX_ARGS, MAX_DEPTH};
use crate::value::Value;

pub(crate) struct Parser<'a> {
    tokens: &'a [Token<'a>],
    at: usize,
    depth: u32,
}

impl<'a> Parser<'a> {
    pub(crate) fn new(tokens: &'a [Token<'a>]) -> Self {
        Self {
            tokens,
            at: 0,
            depth: 0,
        }
    }

    pub(crate) fn parse_script(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let body = self.block()?;
        match self.peek() {
            None => Ok(body),
            Some(token) => Err(self.unexpected(token, "a statement")),
        }
    }

    pub(crate) fn parse_chunk(&mut self) -> Result<Vec<Function>, ParseError> {
        let mut out = Vec::new();
        while let Some(token) = self.peek() {
            match token.kind {
                Kind::Sym(Sym::Semi) => {
                    self.bump();
                }
                Kind::Kw(Kw::Function) => {
                    self.bump();
                    out.push(self.function()?);
                }
                _ => return Err(self.unexpected(token, "`function`")),
            }
        }
        Ok(out)
    }

    fn function(&mut self) -> Result<Function, ParseError> {
        let line = self.peek().map_or(1, |token| token.line);
        let name = self.name("a function name")?;
        self.expect(Sym::LParen, "`(`")?;
        let mut params: Vec<Arc<str>> = Vec::new();
        if !self.eat(Sym::RParen) {
            loop {
                if params.len() >= MAX_ARGS {
                    return Err(self.here(ParseErrorKind::TooManyArguments { limit: MAX_ARGS }));
                }
                params.push(self.name("a parameter name")?);
                if self.eat(Sym::Comma) {
                    continue;
                }
                self.expect(Sym::RParen, "`,` or `)`")?;
                break;
            }
        }
        let body = self.block()?;
        self.expect_kw(Kw::End, "`end`")?;
        Ok(Function {
            name,
            params,
            body,
            line,
        })
    }

    fn block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut out = Vec::new();
        loop {
            let Some(token) = self.peek() else { break };
            match token.kind {
                Kind::Kw(Kw::End | Kw::Else | Kw::Elseif | Kw::Until) => break,
                Kind::Sym(Sym::Semi) => {
                    self.bump();
                    continue;
                }
                _ => {}
            }
            let is_return = matches!(token.kind, Kind::Kw(Kw::Return));
            out.push(self.statement()?);
            if is_return {
                self.skip_semicolons();
                match self.peek() {
                    None => break,
                    Some(next) => match next.kind {
                        Kind::Kw(Kw::End | Kw::Else | Kw::Elseif | Kw::Until) => break,
                        _ => return Err(self.unexpected(next, "`end`, `else` or `elseif`")),
                    },
                }
            }
        }
        Ok(out)
    }

    fn skip_semicolons(&mut self) {
        while self.eat(Sym::Semi) {}
    }

    fn statement(&mut self) -> Result<Stmt, ParseError> {
        let Some(token) = self.peek().cloned() else {
            return Err(self.here(ParseErrorKind::UnexpectedEnd("a statement")));
        };
        match token.kind {
            Kind::Kw(Kw::Local) => {
                self.bump();
                if matches!(self.peek().map(|t| &t.kind), Some(Kind::Kw(Kw::Function))) {
                    return Err(self.here(ParseErrorKind::Unsupported("a local function")));
                }
                let name = self.name("a variable name")?;
                self.expect(Sym::Assign, "`=`")?;
                let value = self.expression()?;
                Ok(Stmt::Assign {
                    name,
                    value,
                    line: token.line,
                })
            }
            Kind::Kw(Kw::If) => {
                self.bump();
                self.if_statement()
            }
            Kind::Kw(Kw::Return) => {
                self.bump();
                let value = if self.starts_expression() {
                    let value = self.expression()?;
                    if self.eat(Sym::Comma) {
                        return Err(
                            self.here(ParseErrorKind::Unsupported("multiple return values"))
                        );
                    }
                    Some(value)
                } else {
                    None
                };
                Ok(Stmt::Return {
                    value,
                    line: token.line,
                })
            }
            Kind::Kw(Kw::While) => Err(self.here(ParseErrorKind::Unsupported("a `while` loop"))),
            Kind::Kw(Kw::For) => Err(self.here(ParseErrorKind::Unsupported("a `for` loop"))),
            Kind::Kw(Kw::Repeat) => Err(self.here(ParseErrorKind::Unsupported("a `repeat` loop"))),
            Kind::Kw(Kw::Do) => Err(self.here(ParseErrorKind::Unsupported("a `do` block"))),
            Kind::Kw(Kw::Break) => Err(self.here(ParseErrorKind::Unsupported("`break`"))),
            Kind::Kw(Kw::Function) => {
                Err(self.here(ParseErrorKind::Unsupported("a nested function declaration")))
            }
            Kind::Name(name) => {
                self.bump();
                self.named_statement(name, token.line)
            }
            _ => Err(self.unexpected(&token, "a statement")),
        }
    }

    fn named_statement(&mut self, name: &str, line: u32) -> Result<Stmt, ParseError> {
        match self.peek().map(|t| t.kind.clone()) {
            Some(Kind::Sym(Sym::LParen)) => {
                self.bump();
                let call = self.call_arguments(name)?;
                Ok(Stmt::Call { call, line })
            }
            Some(Kind::Sym(Sym::Assign)) => {
                self.bump();
                let value = self.expression()?;
                Ok(Stmt::Assign {
                    name: Arc::from(name),
                    value,
                    line,
                })
            }
            Some(Kind::Sym(Sym::Comma)) => {
                Err(self.here(ParseErrorKind::Unsupported("multiple assignment")))
            }
            Some(Kind::Sym(sym)) if sym.unsupported().is_some() => {
                let what = sym.unsupported().unwrap_or("this construct");
                Err(self.here(ParseErrorKind::Unsupported(what)))
            }
            Some(_) => {
                let token = self.peek().cloned();
                match token {
                    Some(token) => Err(self.unexpected(&token, "`=` or `(`")),
                    None => Err(self.here(ParseErrorKind::UnexpectedEnd("a statement"))),
                }
            }
            None => Err(self.here(ParseErrorKind::UnexpectedEnd("a statement"))),
        }
    }

    fn if_statement(&mut self) -> Result<Stmt, ParseError> {
        self.deeper()?;
        let mut arms = Vec::new();
        let mut otherwise = None;
        loop {
            let condition = self.expression()?;
            self.expect_kw(Kw::Then, "`then`")?;
            let body = self.block()?;
            arms.push((condition, body));
            match self.peek().map(|t| t.kind.clone()) {
                Some(Kind::Kw(Kw::Elseif)) => {
                    self.bump();
                }
                Some(Kind::Kw(Kw::Else)) => {
                    self.bump();
                    otherwise = Some(self.block()?);
                    self.expect_kw(Kw::End, "`end`")?;
                    break;
                }
                Some(Kind::Kw(Kw::End)) => {
                    self.bump();
                    break;
                }
                Some(_) => {
                    let token = self.peek().cloned();
                    return match token {
                        Some(token) => Err(self.unexpected(&token, "`elseif`, `else` or `end`")),
                        None => Err(self.here(ParseErrorKind::UnexpectedEnd("an `if`"))),
                    };
                }
                None => return Err(self.here(ParseErrorKind::UnexpectedEnd("an `if`"))),
            }
        }
        self.shallower();
        Ok(Stmt::If { arms, otherwise })
    }

    fn expression(&mut self) -> Result<Expr, ParseError> {
        self.deeper()?;
        let result = self.or_expression();
        self.shallower();
        result
    }

    fn or_expression(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.and_expression()?;
        let mut links = 0u32;
        while matches!(self.peek().map(|t| &t.kind), Some(Kind::Kw(Kw::Or))) {
            self.bump();
            self.deeper()?;
            links = links.saturating_add(1);
            let right = self.and_expression()?;
            left = Expr::Or {
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        self.release(links);
        Ok(left)
    }

    fn and_expression(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.comparison()?;
        let mut links = 0u32;
        while matches!(self.peek().map(|t| &t.kind), Some(Kind::Kw(Kw::And))) {
            self.bump();
            self.deeper()?;
            links = links.saturating_add(1);
            let right = self.comparison()?;
            left = Expr::And {
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        self.release(links);
        Ok(left)
    }

    fn comparison(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.concatenation()?;
        let mut links = 0u32;
        loop {
            let Some(token) = self.peek() else { break };
            let (op, line) = match token.kind {
                Kind::Sym(Sym::Eq) => (BinOp::Eq, token.line),
                Kind::Sym(Sym::Ne) => (BinOp::Ne, token.line),
                Kind::Sym(Sym::Lt) => (BinOp::Lt, token.line),
                Kind::Sym(Sym::Le) => (BinOp::Le, token.line),
                Kind::Sym(Sym::Gt) => (BinOp::Gt, token.line),
                Kind::Sym(Sym::Ge) => (BinOp::Ge, token.line),
                _ => break,
            };
            self.bump();
            self.deeper()?;
            links = links.saturating_add(1);
            let right = self.concatenation()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                line,
            };
        }
        self.release(links);
        Ok(left)
    }

    fn concatenation(&mut self) -> Result<Expr, ParseError> {
        let left = self.additive()?;
        if let Some(token) = self.peek() {
            if matches!(token.kind, Kind::Sym(Sym::Concat)) {
                let line = token.line;
                self.bump();
                self.deeper()?;
                let right = self.concatenation();
                self.shallower();
                return Ok(Expr::Binary {
                    op: BinOp::Concat,
                    left: Box::new(left),
                    right: Box::new(right?),
                    line,
                });
            }
        }
        Ok(left)
    }

    fn additive(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.multiplicative()?;
        let mut links = 0u32;
        loop {
            let Some(token) = self.peek() else { break };
            let (op, line) = match token.kind {
                Kind::Sym(Sym::Plus) => (BinOp::Add, token.line),
                Kind::Sym(Sym::Minus) => (BinOp::Sub, token.line),
                _ => break,
            };
            self.bump();
            self.deeper()?;
            links = links.saturating_add(1);
            let right = self.multiplicative()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                line,
            };
        }
        self.release(links);
        Ok(left)
    }

    fn multiplicative(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.unary()?;
        let mut links = 0u32;
        loop {
            let Some(token) = self.peek() else { break };
            let (op, line) = match token.kind {
                Kind::Sym(Sym::Star) => (BinOp::Mul, token.line),
                Kind::Sym(Sym::Slash) => (BinOp::Div, token.line),
                _ => break,
            };
            self.bump();
            self.deeper()?;
            links = links.saturating_add(1);
            let right = self.unary()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                line,
            };
        }
        self.release(links);
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        let Some(token) = self.peek().cloned() else {
            return Err(self.here(ParseErrorKind::UnexpectedEnd("an expression")));
        };
        let op = match token.kind {
            Kind::Kw(Kw::Not) => UnOp::Not,
            Kind::Sym(Sym::Minus) => UnOp::Neg,
            _ => return self.primary(),
        };
        self.bump();
        self.deeper()?;
        let operand = self.unary();
        self.shallower();
        Ok(Expr::Unary {
            op,
            operand: Box::new(operand?),
            line: token.line,
        })
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        let Some(token) = self.peek().cloned() else {
            return Err(self.here(ParseErrorKind::UnexpectedEnd("an expression")));
        };
        match token.kind {
            Kind::Number(number) => {
                self.bump();
                Ok(Expr::Literal(Value::Number(number)))
            }
            Kind::Str(text) => {
                self.bump();
                Ok(Expr::Literal(Value::Str(text)))
            }
            Kind::Kw(Kw::True) => {
                self.bump();
                Ok(Expr::Literal(Value::Bool(true)))
            }
            Kind::Kw(Kw::False) => {
                self.bump();
                Ok(Expr::Literal(Value::Bool(false)))
            }
            Kind::Kw(Kw::Nil) => {
                self.bump();
                Ok(Expr::Literal(Value::Nil))
            }
            Kind::Kw(Kw::Function) => {
                Err(self.here(ParseErrorKind::Unsupported("a function expression")))
            }
            Kind::Sym(Sym::LParen) => {
                self.bump();
                self.deeper()?;
                let inner = self.or_expression();
                self.shallower();
                let inner = inner?;
                self.expect(Sym::RParen, "`)`")?;
                Ok(inner)
            }
            Kind::Name(name) => {
                self.bump();
                if let Some(Kind::Sym(sym)) = self.peek().map(|t| t.kind.clone()) {
                    if sym == Sym::LParen {
                        self.bump();
                        let call = self.call_arguments(name)?;
                        return Ok(Expr::Call {
                            call: Box::new(call),
                            line: token.line,
                        });
                    }
                    if let Some(what) = sym.unsupported() {
                        return Err(self.here(ParseErrorKind::Unsupported(what)));
                    }
                }
                Ok(Expr::Var {
                    name: Arc::from(name),
                    line: token.line,
                })
            }
            Kind::Sym(sym) => match sym.unsupported() {
                Some(what) => Err(self.here(ParseErrorKind::Unsupported(what))),
                None => Err(self.unexpected(&token, "an expression")),
            },
            Kind::Kw(_) => Err(self.unexpected(&token, "an expression")),
        }
    }

    fn call_arguments(&mut self, name: &str) -> Result<Call, ParseError> {
        let mut args = Vec::new();
        if !self.eat(Sym::RParen) {
            loop {
                if args.len() >= MAX_ARGS {
                    return Err(self.here(ParseErrorKind::TooManyArguments { limit: MAX_ARGS }));
                }
                self.deeper()?;
                let arg = self.or_expression();
                self.shallower();
                args.push(arg?);
                if self.eat(Sym::Comma) {
                    continue;
                }
                self.expect(Sym::RParen, "`,` or `)`")?;
                break;
            }
        }
        Ok(Call {
            name: Arc::from(name),
            args,
        })
    }

    fn starts_expression(&self) -> bool {
        match self.peek().map(|t| &t.kind) {
            Some(Kind::Number(_) | Kind::Str(_) | Kind::Name(_)) => true,
            Some(Kind::Kw(kw)) => matches!(kw, Kw::True | Kw::False | Kw::Nil | Kw::Not),
            Some(Kind::Sym(sym)) => matches!(sym, Sym::LParen | Sym::Minus),
            None => false,
        }
    }

    fn deeper(&mut self) -> Result<(), ParseError> {
        self.depth = self.depth.saturating_add(1);
        if self.depth > MAX_DEPTH {
            return Err(self.here(ParseErrorKind::TooDeep { limit: MAX_DEPTH }));
        }
        Ok(())
    }

    fn shallower(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn release(&mut self, links: u32) {
        self.depth = self.depth.saturating_sub(links);
    }

    fn peek(&self) -> Option<&Token<'a>> {
        self.tokens.get(self.at)
    }

    fn bump(&mut self) {
        self.at = self.at.saturating_add(1);
    }

    fn eat(&mut self, sym: Sym) -> bool {
        if matches!(self.peek().map(|t| &t.kind), Some(Kind::Sym(seen)) if *seen == sym) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, sym: Sym, expected: &'static str) -> Result<(), ParseError> {
        if self.eat(sym) {
            return Ok(());
        }
        match self.peek().cloned() {
            Some(token) => Err(self.unexpected(&token, expected)),
            None => Err(self.here(ParseErrorKind::UnexpectedEnd("a statement"))),
        }
    }

    fn expect_kw(&mut self, kw: Kw, expected: &'static str) -> Result<(), ParseError> {
        if matches!(self.peek().map(|t| &t.kind), Some(Kind::Kw(seen)) if *seen == kw) {
            self.bump();
            return Ok(());
        }
        match self.peek().cloned() {
            Some(token) => Err(self.unexpected(&token, expected)),
            None => Err(self.here(ParseErrorKind::UnexpectedEnd("a statement"))),
        }
    }

    fn name(&mut self, expected: &'static str) -> Result<Arc<str>, ParseError> {
        match self.peek().cloned() {
            Some(token) => match token.kind {
                Kind::Name(name) => {
                    self.bump();
                    Ok(Arc::from(name))
                }
                _ => Err(self.unexpected(&token, expected)),
            },
            None => Err(self.here(ParseErrorKind::UnexpectedEnd("a declaration"))),
        }
    }

    fn unexpected(&self, token: &Token<'a>, expected: &'static str) -> ParseError {
        if let Kind::Sym(sym) = token.kind {
            if let Some(what) = sym.unsupported() {
                return ParseError {
                    line: token.line,
                    kind: ParseErrorKind::Unsupported(what),
                };
            }
        }
        if let Kind::Kw(kw) = token.kind {
            if let Some(what) = unsupported_keyword(kw) {
                return ParseError {
                    line: token.line,
                    kind: ParseErrorKind::Unsupported(what),
                };
            }
        }
        ParseError {
            line: token.line,
            kind: ParseErrorKind::Expected {
                expected,
                found: token.kind.describe(),
            },
        }
    }

    fn here(&self, kind: ParseErrorKind) -> ParseError {
        let line = self
            .peek()
            .or_else(|| self.tokens.last())
            .map_or(1, |token| token.line);
        ParseError { line, kind }
    }
}

fn unsupported_keyword(kw: Kw) -> Option<&'static str> {
    Some(match kw {
        Kw::While => "a `while` loop",
        Kw::For | Kw::In => "a `for` loop",
        Kw::Repeat | Kw::Until => "a `repeat` loop",
        Kw::Do => "a `do` block",
        Kw::Break => "`break`",
        _ => return None,
    })
}

pub(crate) fn check_unique(functions: &[Function]) -> Result<(), ParseError> {
    for (index, function) in functions.iter().enumerate() {
        let earlier = functions.get(..index).unwrap_or(&[]);
        if earlier.iter().any(|other| other.name == function.name) {
            return Err(ParseError {
                line: function.line,
                kind: ParseErrorKind::DuplicateFunction(function.name.to_string()),
            });
        }
    }
    Ok(())
}
