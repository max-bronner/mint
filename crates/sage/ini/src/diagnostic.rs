use std::cell::RefCell;
use std::sync::Arc;

use crate::tree::Field;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{file}:{line}: `{context}`: `{field} = {value}` is not {expected}")]
pub struct ValueError {
    pub file: Arc<str>,
    pub line: u32,
    pub context: String,
    pub field: String,
    pub value: String,
    pub expected: &'static str,
}

#[derive(Debug, Clone, Default)]
pub struct ValueErrors(RefCell<Vec<ValueError>>);

impl ValueErrors {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn field(&self, context: &str, field: &Field, expected: &'static str) {
        self.value(context, field, &field.args.join(" "), expected);
    }

    pub fn value(&self, context: &str, field: &Field, value: &str, expected: &'static str) {
        self.0.borrow_mut().push(ValueError {
            file: Arc::clone(&field.file),
            line: field.line,
            context: context.to_owned(),
            field: field.keyword.clone(),
            value: value.to_owned(),
            expected,
        });
    }

    pub fn at(
        &self,
        file: &Arc<str>,
        line: u32,
        context: &str,
        field: &str,
        value: &str,
        expected: &'static str,
    ) {
        self.0.borrow_mut().push(ValueError {
            file: Arc::clone(file),
            line,
            context: context.to_owned(),
            field: field.to_owned(),
            value: value.to_owned(),
            expected,
        });
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.borrow().is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.borrow().len()
    }

    #[must_use]
    pub fn take(&self) -> Vec<ValueError> {
        std::mem::take(&mut self.0.borrow_mut())
    }

    pub fn absorb(&self, other: &Self) {
        self.0.borrow_mut().append(&mut other.0.borrow_mut());
    }
}
