use mint_sage_livingworld::model::bonus;
use mint_sage_livingworld::{Bonus, LivingWorldState, PlayerId};

pub(super) const BONUS_LINES: [(&str, usize); 6] = [
    ("STRATEGICHUD:RegionAwardCommandPoints", bonus::ARMY),
    (
        "STRATEGICHUD:RegionAwardResourceMultiplier",
        bonus::RESOURCE,
    ),
    ("STRATEGICHUD:RegionAwardSpellPoints", bonus::LEGENDARY),
    ("STRATEGICHUD:RegionAwardBonus", bonus::ATTACK),
    ("STRATEGICHUD:RegionAwardBonus", bonus::DEFENSE),
    ("STRATEGICHUD:RegionAwardBonus", bonus::ATTACK),
];

pub(super) fn bonus_line(bonus: &Bonus, i: usize) -> Option<(&'static str, i32)> {
    BONUS_LINES
        .get(i)
        .map(|(label, field)| (*label, bonus.get(*field)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Row {
    pub(super) player: PlayerId,
    pub(super) regions: usize,
    pub(super) units: u32,
}

pub(super) fn rows(state: &LivingWorldState, players: &[PlayerId]) -> Vec<Row> {
    players
        .iter()
        .map(|&player| Row {
            player,
            regions: (0..state.rules.regions.len())
                .filter_map(|i| u16::try_from(i).ok())
                .filter(|i| state.owner(mint_sage_livingworld::RegionId(*i)) == Some(player))
                .count(),
            units: state
                .armies_of(player)
                .flat_map(|a| a.roster.iter())
                .fold(0u32, |sum, m| sum.saturating_add(m.count)),
        })
        .collect()
}

pub(super) fn select(old: Option<usize>, new: usize) -> Vec<(usize, &'static str)> {
    let mut out = Vec::new();
    if let Some(old) = old.filter(|old| *old != new) {
        out.push((old, "_deselect"));
    }
    out.push((new, "_selected"));
    out
}
