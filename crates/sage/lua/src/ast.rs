use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Stmt {
    Assign {
        name: Arc<str>,
        value: Expr,
        line: u32,
    },
    Call {
        call: Call,
        line: u32,
    },
    If {
        arms: Vec<(Expr, Vec<Stmt>)>,
        otherwise: Option<Vec<Stmt>>,
    },
    Return {
        value: Option<Expr>,
        line: u32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Call {
    pub(crate) name: Arc<str>,
    pub(crate) args: Vec<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnOp {
    Not,
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Concat,
    Add,
    Sub,
    Mul,
    Div,
}

impl BinOp {
    pub(crate) fn text(self) -> &'static str {
        match self {
            BinOp::Eq => "==",
            BinOp::Ne => "~=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Concat => "..",
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Expr {
    Literal(Value),
    Var {
        name: Arc<str>,
        line: u32,
    },
    Call {
        call: Box<Call>,
        line: u32,
    },
    Unary {
        op: UnOp,
        operand: Box<Expr>,
        line: u32,
    },
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
        line: u32,
    },
    And {
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Or {
        left: Box<Expr>,
        right: Box<Expr>,
    },
}

pub(crate) fn collect_calls<'a>(body: &'a [Stmt], out: &mut Vec<&'a str>) {
    for stmt in body {
        match stmt {
            Stmt::Assign { value, .. } => calls_in(value, out),
            Stmt::Call { call, .. } => {
                out.push(&call.name);
                for arg in &call.args {
                    calls_in(arg, out);
                }
            }
            Stmt::If { arms, otherwise } => {
                for (condition, body) in arms {
                    calls_in(condition, out);
                    collect_calls(body, out);
                }
                if let Some(body) = otherwise {
                    collect_calls(body, out);
                }
            }
            Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    calls_in(value, out);
                }
            }
        }
    }
}

fn calls_in<'a>(expr: &'a Expr, out: &mut Vec<&'a str>) {
    match expr {
        Expr::Literal(_) | Expr::Var { .. } => {}
        Expr::Call { call, .. } => {
            out.push(&call.name);
            for arg in &call.args {
                calls_in(arg, out);
            }
        }
        Expr::Unary { operand, .. } => calls_in(operand, out),
        Expr::Binary { left, right, .. } | Expr::And { left, right } | Expr::Or { left, right } => {
            calls_in(left, out);
            calls_in(right, out);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub(crate) name: Arc<str>,
    pub(crate) params: Vec<Arc<str>>,
    pub(crate) body: Vec<Stmt>,
    pub(crate) line: u32,
}

impl Function {
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn arity(&self) -> usize {
        self.params.len()
    }

    #[must_use]
    pub fn call_names(&self) -> Vec<&str> {
        let mut out = Vec::new();
        collect_calls(&self.body, &mut out);
        out
    }
}
