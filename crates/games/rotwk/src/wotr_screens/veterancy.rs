use std::collections::BTreeSet;

use mint_sage_livingworld::{ArmyId, Command, LivingWorldState, PlayerId, RegionId};

pub(crate) type Charge = (ArmyId, u32, i32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Row {
    pub(super) army: ArmyId,
    pub(super) member: u32,
    pub(super) name: String,
    pub(super) level: u32,
    pub(super) hero: bool,
    pub(super) upgraded: bool,
    pub(super) battles: u32,
    pub(super) kills: u32,
    pub(super) kills_battle: u32,
    pub(super) picture: Option<String>,
    pub(super) charge: Option<i32>,
}

pub(super) fn rows(
    state: &LivingWorldState,
    local: PlayerId,
    region: RegionId,
    charges: &[Charge],
) -> Vec<Row> {
    let present = state
        .region(region)
        .map_or(&[][..], |r| r.present.as_slice());
    present
        .iter()
        .filter_map(|id| state.armies.get(id))
        .filter(|army| army.player == local)
        .flat_map(|army| army.roster.iter().map(move |m| (army, m)))
        .map(|(army, m)| Row {
            army: army.id,
            member: m.id,
            name: state.rules.unit_name(m.unit).to_owned(),
            level: m.level,
            hero: state.rules.unit(m.unit).hero || army.hero == Some(m.unit),
            upgraded: m.is_upgraded(),
            battles: m.battles,
            kills: m.kills,
            kills_battle: m.kills_battle,
            picture: state.rules.unit_portrait(m.unit).map(str::to_owned),
            charge: charges
                .iter()
                .find(|(a, id, _)| *a == army.id && *id == m.id)
                .map(|(_, _, price)| *price),
        })
        .collect()
}

pub(super) fn list(rows: &[Row], cell: impl Fn(&Row) -> String) -> String {
    rows.iter().map(|row| format!("{}|", cell(row))).collect()
}

pub(super) fn parse_mark(args: &str) -> Option<(usize, bool)> {
    let (row, on) = args.split_once('|')?;
    Some((row.trim().parse().ok()?, on.trim() == "1"))
}

pub(super) fn disbands(rows: &[Row], marked: &BTreeSet<usize>) -> Vec<Command> {
    marked
        .iter()
        .filter_map(|index| rows.get(*index))
        .filter(|row| row.charge.is_some() && !row.hero)
        .map(|row| Command::DisbandMember {
            army: row.army,
            member: row.member,
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn row(member: u32, hero: bool, charge: Option<i32>) -> Row {
        Row {
            army: ArmyId(7),
            member,
            name: String::new(),
            level: 1,
            hero,
            upgraded: false,
            battles: 0,
            kills: 0,
            kills_battle: 0,
            picture: None,
            charge,
        }
    }

    #[test]
    fn only_a_marked_new_non_hero_row_is_disbanded() {
        let rows = [
            row(1, false, Some(100)),
            row(2, true, Some(100)),
            row(3, false, None),
            row(4, false, Some(50)),
        ];
        let marked = BTreeSet::from([0, 1, 2]);
        assert_eq!(
            disbands(&rows, &marked),
            vec![Command::DisbandMember {
                army: ArmyId(7),
                member: 1
            }]
        );
    }

    #[test]
    fn a_mark_reads_row_and_state_and_refuses_anything_else() {
        assert_eq!(parse_mark("3|1"), Some((3, true)));
        assert_eq!(parse_mark("3|0"), Some((3, false)));
        assert_eq!(parse_mark("x|1"), None);
        assert_eq!(parse_mark("3"), None);
    }
}
