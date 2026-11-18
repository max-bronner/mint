use std::path::PathBuf;
use std::sync::Arc;

use mint_math::rng::Pcg32;
use mint_sage_asset::{HeroSource, HeroStore};
use mint_sage_cah::HeroRecord;

use crate::lobby::{Catalogue, HeroPick, ResolvedPlayer};

const SIDE_NAMES: [&str; 9] = [
    "Men", "Elves", "Dwarves", "Isengard", "Mordor", "Wild", "Angmar", "Arnor", "Neutral",
];
pub(crate) const NO_SIDE: u8 = 9;

pub(crate) fn faction_bit(side: &str) -> u8 {
    let side = if side.eq_ignore_ascii_case("Goblins") {
        "Wild"
    } else {
        side
    };
    SIDE_NAMES
        .iter()
        .position(|name| name.eq_ignore_ascii_case(side))
        .and_then(|i| u8::try_from(i).ok())
        .unwrap_or(NO_SIDE)
}

fn mask_of(sides: impl IntoIterator<Item = impl AsRef<str>>) -> u16 {
    sides.into_iter().fold(0, |mask, side| {
        let bit = faction_bit(side.as_ref());
        mask | 1u16.checked_shl(u32::from(bit)).unwrap_or(0)
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SubclassFactions {
    pub(crate) usable: u16,
    pub(crate) default_faction: Option<u8>,
}

pub(crate) fn read_subclasses(
    documents: &[mint_sage_ini::Document],
    macros: &mint_sage_ini::MacroTable,
) -> Vec<Vec<SubclassFactions>> {
    let errors = mint_sage_ini::ValueErrors::new();
    documents
        .iter()
        .flat_map(|d| d.roots_named(mint_sage_ini::CREATE_A_HERO_SYSTEM_KEYWORD))
        .flat_map(|system| {
            mint_sage_ini::read_create_a_hero_system(system, &errors, macros).classes
        })
        .map(|class| {
            class
                .subclasses
                .iter()
                .map(|sub| SubclassFactions {
                    usable: mask_of(&sub.usable_factions),
                    default_faction: sub.default_faction.as_deref().map(faction_bit),
                })
                .collect()
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HeroEntry {
    pub(crate) name: String,
    pub(crate) system: bool,
    pub(crate) factions: SubclassFactions,
    pub(crate) record: Arc<HeroRecord>,
}

impl HeroEntry {
    pub(crate) fn usable_by(&self, faction: Option<u8>) -> bool {
        faction.is_none_or(|bit| {
            1u16.checked_shl(u32::from(bit))
                .is_some_and(|mask| self.factions.usable & mask != 0)
        })
    }
}

pub(crate) fn entries(store: &HeroStore, subclasses: &[Vec<SubclassFactions>]) -> Vec<HeroEntry> {
    store
        .heroes()
        .iter()
        .map(|stored| {
            let factions = usize::try_from(stored.record.class_index)
                .ok()
                .and_then(|c| subclasses.get(c))
                .zip(usize::try_from(stored.record.subclass_index).ok())
                .and_then(|(subs, s)| subs.get(s))
                .copied()
                .unwrap_or_default();
            HeroEntry {
                name: stored.record.name_string(),
                system: stored.source == HeroSource::System,
                factions,
                record: Arc::new(stored.record.clone()),
            }
        })
        .collect()
}

pub(crate) fn own_dir() -> PathBuf {
    crate::user_data::dir()
        .unwrap_or_else(|| std::env::temp_dir().join("mint"))
        .join(mint_sage_asset::SAVE_FOLDER)
}

pub(crate) fn load_entries(
    vfs: &mint_vfs::Vfs,
    subclasses: &[Vec<SubclassFactions>],
) -> Vec<HeroEntry> {
    match HeroStore::load(vfs, own_dir(), &[]) {
        Ok(store) => {
            for skipped in store.skipped() {
                println!("heroes: skipped {skipped}");
            }
            entries(&store, subclasses)
        }
        Err(error) => {
            println!("heroes: no hero list: {error}");
            Vec::new()
        }
    }
}

pub(crate) fn follow_army(hero: HeroPick, faction: Option<u8>, heroes: &[HeroEntry]) -> HeroPick {
    let current = match hero {
        HeroPick::Stored(i) => heroes.get(i),
        HeroPick::None | HeroPick::Random => None,
    };
    let replace = match current {
        None => true,
        Some(h) if h.system => true,
        Some(h) => faction.is_some() && !h.usable_by(faction),
    };
    if !replace {
        return hero;
    }
    match faction {
        None => HeroPick::Random,
        Some(bit) => heroes
            .iter()
            .position(|h| h.system && h.factions.default_faction == Some(bit))
            .map_or_else(
                || match current {
                    Some(h) if !h.usable_by(faction) => HeroPick::Random,
                    _ => hero,
                },
                HeroPick::Stored,
            ),
    }
}

const HERO_STREAM: u64 = 0x4845_524F;

pub(crate) fn pick_random_heroes(players: &mut [ResolvedPlayer], catalogue: &Catalogue, seed: u64) {
    let mut rng = Pcg32::with_stream(seed, HERO_STREAM);
    for player in players.iter_mut() {
        if player.hero_pick != HeroPick::Random {
            continue;
        }
        let faction = catalogue.faction_bit(&player.faction);
        let candidates: Vec<usize> = catalogue
            .heroes
            .iter()
            .enumerate()
            .filter(|(_, h)| h.usable_by(faction))
            .map(|(i, _)| i)
            .collect();
        let n = u32::try_from(candidates.len()).unwrap_or(u32::MAX);
        player.hero_pick = if n == 0 {
            HeroPick::None
        } else {
            usize::try_from(rng.below(n))
                .ok()
                .and_then(|at| candidates.get(at))
                .map_or(HeroPick::None, |&i| HeroPick::Stored(i))
        };
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
