use mint_sim::PlayerId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Stance {
    Allied,
    Neutral,
    Hostile,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub(crate) struct MapRelations {
    pub(crate) allies: Vec<PlayerId>,
    pub(crate) enemies: Vec<PlayerId>,
    pub(crate) universal_hostile: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub(crate) struct Alliances {
    teams: Vec<Option<u8>>,
    first_map_player: Option<u8>,
    map_players: Vec<MapRelations>,
    contestant_sides: Vec<(PlayerId, MapRelations)>,
}

impl Alliances {
    pub(crate) const fn new(teams: Vec<Option<u8>>) -> Self {
        Self {
            teams,
            first_map_player: None,
            map_players: Vec::new(),
            contestant_sides: Vec::new(),
        }
    }

    pub(crate) fn with_map_players(mut self, first: u8, relations: Vec<MapRelations>) -> Self {
        self.first_map_player = Some(first);
        self.map_players = relations;
        self
    }

    pub(crate) fn with_contestant_sides(mut self, sides: Vec<(PlayerId, MapRelations)>) -> Self {
        self.contestant_sides = sides;
        self
    }

    fn team(&self, player: PlayerId) -> Option<u8> {
        self.teams.get(usize::from(player.get())).copied().flatten()
    }

    fn map_player(&self, player: PlayerId) -> Option<&MapRelations> {
        let offset = player.get().checked_sub(self.first_map_player?)?;
        self.map_players.get(usize::from(offset))
    }

    pub(crate) fn relation_from(&self, a: PlayerId, b: PlayerId) -> Stance {
        if a == b {
            return Stance::Allied;
        }
        if let Some((_, own)) = self.contestant_sides.iter().find(|(p, _)| *p == a) {
            return if own.allies.contains(&b) {
                Stance::Allied
            } else if own.enemies.contains(&b) {
                Stance::Hostile
            } else {
                Stance::Neutral
            };
        }
        let (of_a, of_b) = (self.map_player(a), self.map_player(b));
        let universal = |of: Option<&MapRelations>| of.is_some_and(|r| r.universal_hostile);
        if universal(of_a) {
            return if of_b.is_none() {
                Stance::Hostile
            } else {
                Stance::Neutral
            };
        }
        if universal(of_b) && of_a.is_none() {
            return Stance::Hostile;
        }
        let (lists, target) = match (of_a, of_b) {
            (None, None) => {
                return if self.team(a).is_some_and(|t| self.team(b) == Some(t)) {
                    Stance::Allied
                } else {
                    Stance::Hostile
                };
            }
            (Some(lists), _) => (lists, b),
            (None, Some(lists)) => (lists, a),
        };
        if lists.allies.contains(&target) {
            Stance::Allied
        } else if lists.enemies.contains(&target) {
            Stance::Hostile
        } else {
            Stance::Neutral
        }
    }

    pub(crate) fn stance(&self, a: PlayerId, b: PlayerId) -> Stance {
        self.relation_from(a, b)
    }

    pub(crate) fn allied(&self, a: PlayerId, b: PlayerId) -> bool {
        self.stance(a, b) == Stance::Allied
    }

    pub(crate) fn is_map_player(&self, player: PlayerId) -> bool {
        self.map_player(player).is_some()
    }

    pub(crate) fn is_creep_player(&self, player: PlayerId) -> bool {
        self.map_player(player).is_some_and(|r| r.universal_hostile)
    }

    pub(crate) fn hostile(&self, a: PlayerId, b: PlayerId) -> bool {
        self.stance(a, b) == Stance::Hostile
    }

    pub(crate) fn relation(&self, a: PlayerId, b: PlayerId) -> mint_sage_combat::victim::Relation {
        match self.stance(a, b) {
            Stance::Allied => mint_sage_combat::victim::Relation::Allies,
            Stance::Hostile => mint_sage_combat::victim::Relation::Enemies,
            Stance::Neutral => mint_sage_combat::victim::Relation::Neutral,
        }
    }

    pub(crate) fn one_side(&self, players: &[PlayerId]) -> bool {
        players
            .iter()
            .all(|a| players.iter().all(|b| self.allied(*a, *b)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: PlayerId = PlayerId::new(0);
    const B: PlayerId = PlayerId::new(1);
    const C: PlayerId = PlayerId::new(2);

    #[test]
    fn a_team_makes_allies_and_no_team_fights_everyone() {
        let teams = Alliances::new(vec![Some(1), Some(1), None]);
        assert!(teams.allied(A, B) && teams.allied(B, A));
        assert!(!teams.hostile(A, B), "allies do not fight");
        assert!(teams.hostile(A, C) && teams.hostile(C, B));
        assert!(teams.allied(C, C), "a player is its own side");
        let apart = Alliances::new(vec![Some(0), Some(1)]);
        assert!(apart.hostile(A, B));
        let none = Alliances::default();
        assert!(none.hostile(A, B) && none.allied(A, A));
        let unknown = Alliances::new(vec![None, None]);
        assert!(unknown.hostile(A, B), "two players on no team");
    }

    #[test]
    fn a_map_player_is_neutral_unless_a_list_names_the_other() {
        let d = PlayerId::new(3);
        let alliances = Alliances::new(vec![None, None]).with_map_players(
            2,
            vec![
                MapRelations::default(),
                MapRelations {
                    allies: Vec::new(),
                    enemies: vec![C],
                    universal_hostile: false,
                },
            ],
        );
        assert_eq!(alliances.stance(A, C), Stance::Neutral);
        assert_eq!(alliances.stance(C, B), Stance::Neutral);
        assert_eq!(alliances.stance(A, B), Stance::Hostile, "two contestants");
        assert_eq!(alliances.stance(d, C), Stance::Hostile);
        assert_eq!(
            alliances.stance(C, d),
            Stance::Neutral,
            "the other side's list is not read"
        );
        assert!(!alliances.hostile(A, d) && !alliances.allied(A, d));
    }

    #[test]
    fn a_universal_hostile_map_player_fights_everyone_regardless_of_its_lists() {
        let alliances = Alliances::new(vec![None, None]).with_map_players(
            2,
            vec![
                MapRelations {
                    allies: Vec::new(),
                    enemies: Vec::new(),
                    universal_hostile: true,
                },
                MapRelations::default(),
            ],
        );
        let (creeps, inn) = (PlayerId::new(2), PlayerId::new(3));
        assert_eq!(alliances.stance(A, creeps), Stance::Hostile);
        assert_eq!(alliances.stance(creeps, B), Stance::Hostile);
        assert_eq!(alliances.stance(creeps, inn), Stance::Neutral);
    }

    #[test]
    fn a_side_in_both_lists_is_an_ally() {
        let alliances = Alliances::new(vec![None, None]).with_map_players(
            2,
            vec![
                MapRelations {
                    allies: vec![PlayerId::new(3)],
                    enemies: vec![PlayerId::new(3)],
                    universal_hostile: false,
                },
                MapRelations::default(),
            ],
        );
        assert_eq!(
            alliances.relation_from(PlayerId::new(2), PlayerId::new(3)),
            Stance::Allied
        );
    }

    #[test]
    fn one_side_is_every_pair_allied() {
        let teams = Alliances::new(vec![Some(2), Some(2), Some(0)]);
        assert!(teams.one_side(&[A, B]));
        assert!(!teams.one_side(&[A, B, C]));
        assert!(teams.one_side(&[C]));
        assert!(!Alliances::default().one_side(&[A, B]));
    }

    #[test]
    fn a_campaign_contestant_reads_its_own_sides_lists() {
        let w = PlayerId::new(2);
        let alliances = Alliances::new(vec![None, None])
            .with_map_players(
                2,
                vec![MapRelations {
                    allies: vec![A],
                    enemies: Vec::new(),
                    universal_hostile: false,
                }],
            )
            .with_contestant_sides(vec![(
                A,
                MapRelations {
                    allies: Vec::new(),
                    enemies: vec![w],
                    universal_hostile: false,
                },
            )]);
        assert_eq!(
            [alliances.stance(A, w), alliances.stance(w, A)],
            [Stance::Hostile, Stance::Allied]
        );
    }
}
