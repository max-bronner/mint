use super::generated::{CONTEXTS, FILE_SCOPE, MODULES};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Titles(u8);

impl Titles {
    pub const NONE: Self = Self(0);
    pub const BFME2: Self = Self(1);
    pub const ROTWK: Self = Self(2);
    pub const BOTH: Self = Self(3);

    #[must_use]
    pub const fn overlaps(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bound {
    RegistrationProc,
    RegistrationCallee,
    ModuleFactory,
    UserData,
    FieldProc,
    FieldProcCallee,
    Vocabulary,
    Vote,
    Counterpart,
    Absent,
}

#[derive(Debug, Clone, Copy)]
pub struct Field(pub &'static str, pub Titles);

#[derive(Debug, Clone, Copy)]
pub struct Table {
    pub fields: &'static [Field],
    pub titles: Titles,
    pub wildcard: Titles,
    pub bound: [Bound; 2],
}

#[derive(Debug, Clone, Copy)]
pub enum Target {
    Context(u16),
    Typed(&'static [(&'static str, u16)]),
    Module,
}

#[derive(Debug, Clone, Copy)]
pub struct Child {
    pub keyword: &'static str,
    pub titles: Titles,
    pub target: Target,
}

#[derive(Debug, Clone, Copy)]
pub struct Context {
    pub tables: &'static [Table],
    pub children: &'static [Child],
}

impl Context {
    #[must_use]
    pub fn accepts(&self, token: &str, title: Titles) -> bool {
        self.tables.iter().any(|table| {
            table.titles.overlaps(title)
                && (table.wildcard.overlaps(title)
                    || table
                        .fields
                        .iter()
                        .any(|f| f.0 == token && f.1.overlaps(title)))
        })
    }

    #[must_use]
    pub fn child(
        &self,
        keyword: &str,
        arg: Option<&str>,
        title: Titles,
    ) -> Option<&'static Context> {
        let child = self
            .children
            .iter()
            .find(|c| c.keyword == keyword && c.titles.overlaps(title))?;
        match child.target {
            Target::Context(id) => CONTEXTS.get(usize::from(id)),
            Target::Typed(map) => {
                let arg = arg?;
                map.iter()
                    .find(|(name, _)| *name == arg)
                    .and_then(|&(_, id)| CONTEXTS.get(usize::from(id)))
            }
            Target::Module => module(arg?),
        }
    }
}

fn lookup(map: &'static [(&'static str, u16)], name: &str) -> Option<&'static Context> {
    let at = map.binary_search_by(|(k, _)| (*k).cmp(name)).ok()?;
    let &(_, id) = map.get(at)?;
    CONTEXTS.get(usize::from(id))
}

#[must_use]
pub fn file_scope(name: &str) -> Option<&'static Context> {
    lookup(FILE_SCOPE, name)
}

#[must_use]
pub fn module(name: &str) -> Option<&'static Context> {
    lookup(MODULES, name)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
