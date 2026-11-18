use std::io;

use mint_sage_asset::{HeroSource, HeroStore};

use super::edit::HeroEdit;

#[must_use]
pub(crate) fn name_is_valid(text: &str, store: &HeroStore) -> bool {
    let name = text.trim();
    !name.is_empty()
        && !store
            .heroes()
            .iter()
            .any(|h| h.record.name_string().to_lowercase() == name.to_lowercase())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingDelete {
    id: String,
}

#[must_use]
pub(crate) fn request_delete(store: &HeroStore, index: usize) -> Option<PendingDelete> {
    let hero = store.heroes().get(index)?;
    if hero.source == HeroSource::System {
        return None;
    }
    Some(PendingDelete {
        id: hero.record.id_str()?,
    })
}

pub(crate) fn confirm_delete(store: &mut HeroStore, pending: PendingDelete) -> Option<String> {
    store.forget(&pending.id).then_some(pending.id)
}

pub(crate) fn save_new(store: &mut HeroStore, edit: &HeroEdit, id: &str) -> io::Result<()> {
    let mut record = edit.record().clone();
    record.id = Some(id.as_bytes().to_vec());
    store.save(record)
}

pub(crate) fn save_over(store: &mut HeroStore, edit: &HeroEdit, index: usize) -> io::Result<bool> {
    let Some(hero) = store.heroes().get(index) else {
        return Ok(false);
    };
    if hero.source == HeroSource::System {
        return Ok(false);
    }
    let mut record = edit.record().clone();
    record.id = hero.record.id.clone();
    store.save(record)?;
    Ok(true)
}
