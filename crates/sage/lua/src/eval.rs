use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::ast::{BinOp, Call, Expr, Function, Stmt, UnOp};
use crate::error::{EvalError, EvalErrorKind, HostError};
use crate::limits::{MAX_CALL_DEPTH, MAX_STRING_BYTES};
use crate::value::{Env, Value};

pub trait Host {
    fn call(&mut self, name: &str, args: &[Value]) -> Result<Value, HostError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    remaining: u32,
}

impl Budget {
    #[must_use]
    pub fn new(steps: u32) -> Self {
        Self { remaining: steps }
    }

    #[must_use]
    pub fn remaining(self) -> u32 {
        self.remaining
    }

    fn spend(&mut self, line: u32) -> Result<(), EvalError> {
        match self.remaining.checked_sub(1) {
            Some(left) => {
                self.remaining = left;
                Ok(())
            }
            None => Err(EvalError {
                line,
                kind: EvalErrorKind::BudgetExhausted,
            }),
        }
    }
}

impl Default for Budget {
    fn default() -> Self {
        Self::new(crate::limits::DEFAULT_BUDGET)
    }
}

enum Flow {
    Fell,
    Returned(Value),
}

pub(crate) struct Run<'a, H: Host + ?Sized> {
    pub(crate) functions: &'a [Function],
    pub(crate) host: &'a mut H,
    pub(crate) budget: &'a mut Budget,
    depth: u32,
}

impl<'a, H: Host + ?Sized> Run<'a, H> {
    pub(crate) fn new(functions: &'a [Function], host: &'a mut H, budget: &'a mut Budget) -> Self {
        Self {
            functions,
            host,
            budget,
            depth: 0,
        }
    }

    pub(crate) fn body(&mut self, body: &[Stmt], env: &mut Env) -> Result<Value, EvalError> {
        match self.block(body, env)? {
            Flow::Returned(value) => Ok(value),
            Flow::Fell => Ok(Value::Nil),
        }
    }

    pub(crate) fn function(
        &mut self,
        function: &Function,
        args: &[Value],
        line: u32,
    ) -> Result<Value, EvalError> {
        if args.len() != function.params.len() {
            return Err(EvalError {
                line,
                kind: EvalErrorKind::WrongArity {
                    name: function.name.to_string(),
                    expected: function.params.len(),
                    found: args.len(),
                },
            });
        }
        let mut env = Env::new();
        for (index, param) in function.params.iter().enumerate() {
            let value = args.get(index).cloned().unwrap_or(Value::Nil);
            env.set_at(param, value, function.line)?;
        }
        self.body(&function.body, &mut env)
    }

    fn block(&mut self, body: &[Stmt], env: &mut Env) -> Result<Flow, EvalError> {
        for stmt in body {
            match self.statement(stmt, env)? {
                Flow::Fell => {}
                returned => return Ok(returned),
            }
        }
        Ok(Flow::Fell)
    }

    fn statement(&mut self, stmt: &Stmt, env: &mut Env) -> Result<Flow, EvalError> {
        match stmt {
            Stmt::Assign { name, value, line } => {
                self.budget.spend(*line)?;
                let value = self.expression(value, env)?;
                env.set_at(name, value, *line)?;
                Ok(Flow::Fell)
            }
            Stmt::Call { call, line } => {
                self.budget.spend(*line)?;
                self.call(call, env, *line)?;
                Ok(Flow::Fell)
            }
            Stmt::If { arms, otherwise } => {
                for (condition, body) in arms {
                    if self.expression(condition, env)?.is_truthy() {
                        return self.block(body, env);
                    }
                }
                match otherwise {
                    Some(body) => self.block(body, env),
                    None => Ok(Flow::Fell),
                }
            }
            Stmt::Return { value, line } => {
                self.budget.spend(*line)?;
                let value = match value {
                    Some(expr) => self.expression(expr, env)?,
                    None => Value::Nil,
                };
                Ok(Flow::Returned(value))
            }
        }
    }

    fn expression(&mut self, expr: &Expr, env: &mut Env) -> Result<Value, EvalError> {
        match expr {
            Expr::Literal(value) => Ok(value.clone()),
            Expr::Var { name, line } => {
                self.budget.spend(*line)?;
                match env.get(name) {
                    Some(value) => Ok(value.clone()),
                    None => Err(EvalError {
                        line: *line,
                        kind: EvalErrorKind::UnknownVariable(name.to_string()),
                    }),
                }
            }
            Expr::Call { call, line } => {
                self.budget.spend(*line)?;
                self.call(call, env, *line)
            }
            Expr::Unary { op, operand, line } => {
                self.budget.spend(*line)?;
                let value = self.expression(operand, env)?;
                match op {
                    UnOp::Not => Ok(Value::Bool(!value.is_truthy())),
                    UnOp::Neg => match value {
                        Value::Number(number) => Ok(Value::Number(-number)),
                        other => Err(EvalError {
                            line: *line,
                            kind: EvalErrorKind::TypeMismatch {
                                op: "-",
                                found: other.type_name(),
                            },
                        }),
                    },
                }
            }
            Expr::And { left, right } => {
                let left = self.expression(left, env)?;
                if left.is_truthy() {
                    self.expression(right, env)
                } else {
                    Ok(left)
                }
            }
            Expr::Or { left, right } => {
                let left = self.expression(left, env)?;
                if left.is_truthy() {
                    Ok(left)
                } else {
                    self.expression(right, env)
                }
            }
            Expr::Binary {
                op,
                left,
                right,
                line,
            } => {
                self.budget.spend(*line)?;
                let left = self.expression(left, env)?;
                let right = self.expression(right, env)?;
                binary(*op, &left, &right, *line)
            }
        }
    }

    fn call(&mut self, call: &Call, env: &mut Env, line: u32) -> Result<Value, EvalError> {
        let mut args: Vec<Value> = Vec::with_capacity(call.args.len());
        for arg in &call.args {
            args.push(self.expression(arg, env)?);
        }
        let declared = self
            .functions
            .iter()
            .find(|function| *function.name == *call.name);
        match declared {
            Some(function) => {
                self.depth = self.depth.saturating_add(1);
                if self.depth > MAX_CALL_DEPTH {
                    return Err(EvalError {
                        line,
                        kind: EvalErrorKind::CallTooDeep {
                            limit: MAX_CALL_DEPTH,
                        },
                    });
                }
                let result = self.function(function, &args, line);
                self.depth = self.depth.saturating_sub(1);
                result
            }
            None => match base_library(&call.name, &args) {
                Some(value) => Ok(value),
                None => self
                    .host
                    .call(&call.name, &args)
                    .map_err(|error| EvalError {
                        line,
                        kind: EvalErrorKind::Host {
                            name: call.name.to_string(),
                            error,
                        },
                    }),
            },
        }
    }
}

pub const BASE_LIBRARY: [&str; 2] = ["tostring", "tonumber"];

fn base_library(name: &str, args: &[Value]) -> Option<Value> {
    match name {
        "tostring" => Some(Value::string(
            args.first().unwrap_or(&Value::Nil).to_string(),
        )),
        "tonumber" => Some(match args.first() {
            Some(Value::Number(number)) => Value::Number(*number),
            Some(Value::Str(text)) => text.trim().parse::<f32>().map_or(Value::Nil, Value::Number),
            _ => Value::Nil,
        }),
        _ => None,
    }
}

fn binary(op: BinOp, left: &Value, right: &Value, line: u32) -> Result<Value, EvalError> {
    match op {
        BinOp::Eq => Ok(Value::Bool(left == right)),
        BinOp::Ne => Ok(Value::Bool(left != right)),
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => order(op, left, right, line),
        BinOp::Concat => concat(left, right, line),
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => arithmetic(op, left, right, line),
    }
}

fn order(op: BinOp, left: &Value, right: &Value, line: u32) -> Result<Value, EvalError> {
    let ordering = match (left, right) {
        (Value::Number(a), Value::Number(b)) => {
            if a < b {
                Some(core::cmp::Ordering::Less)
            } else if a > b {
                Some(core::cmp::Ordering::Greater)
            } else if a == b {
                Some(core::cmp::Ordering::Equal)
            } else {
                None
            }
        }
        (Value::Str(a), Value::Str(b)) => Some((**a).cmp(b)),
        _ => {
            return Err(EvalError {
                line,
                kind: EvalErrorKind::IncomparableTypes {
                    left: left.type_name(),
                    right: right.type_name(),
                },
            })
        }
    };
    let Some(ordering) = ordering else {
        return Ok(Value::Bool(false));
    };
    let truth = match op {
        BinOp::Lt => ordering.is_lt(),
        BinOp::Le => ordering.is_le(),
        BinOp::Gt => ordering.is_gt(),
        _ => ordering.is_ge(),
    };
    Ok(Value::Bool(truth))
}

fn concat(left: &Value, right: &Value, line: u32) -> Result<Value, EvalError> {
    let (Some(left), Some(right)) = (left.as_str(), right.as_str()) else {
        let found = if left.as_str().is_none() {
            left.type_name()
        } else {
            right.type_name()
        };
        return Err(EvalError {
            line,
            kind: EvalErrorKind::TypeMismatch { op: "..", found },
        });
    };
    let length = left.len().saturating_add(right.len());
    if length > MAX_STRING_BYTES {
        return Err(EvalError {
            line,
            kind: EvalErrorKind::StringTooLong {
                limit: MAX_STRING_BYTES,
            },
        });
    }
    let mut joined = String::with_capacity(length);
    joined.push_str(left);
    joined.push_str(right);
    Ok(Value::Str(Arc::from(joined.as_str())))
}

fn arithmetic(op: BinOp, left: &Value, right: &Value, line: u32) -> Result<Value, EvalError> {
    let (Some(a), Some(b)) = (left.as_number(), right.as_number()) else {
        let found = if left.as_number().is_none() {
            left.type_name()
        } else {
            right.type_name()
        };
        return Err(EvalError {
            line,
            kind: EvalErrorKind::TypeMismatch {
                op: op.text(),
                found,
            },
        });
    };
    let result = match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        _ => a / b,
    };
    Ok(Value::Number(result))
}
