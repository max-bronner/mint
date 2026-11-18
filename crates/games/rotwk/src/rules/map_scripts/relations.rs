use std::collections::BTreeMap;

use mint_sim::{Entity, PlayerId, StateHasher};

use super::team::{TeamId, TeamProtos, Teams};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Relation {
    Enemies,
    Neutral,
    Allies,
}

impl Relation {
    pub(crate) fn from_param(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Enemies),
            1 => Some(Self::Neutral),
            2 => Some(Self::Allies),
            _ => None,
        }
    }

    pub(crate) fn victim(self) -> mint_sage_combat::victim::Relation {
        use mint_sage_combat::victim::Relation as Victim;
        match self {
            Self::Enemies => Victim::Enemies,
            Self::Neutral => Victim::Neutral,
            Self::Allies => Victim::Allies,
        }
    }

    fn code(self) -> u8 {
        match self {
            Self::Enemies => 0,
            Self::Neutral => 1,
            Self::Allies => 2,
        }
    }
}

impl From<crate::alliance::Stance> for Relation {
    fn from(stance: crate::alliance::Stance) -> Self {
        match stance {
            crate::alliance::Stance::Hostile => Self::Enemies,
            crate::alliance::Stance::Neutral => Self::Neutral,
            crate::alliance::Stance::Allied => Self::Allies,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Relations {
    player_player: BTreeMap<(PlayerId, PlayerId), Relation>,
    player_team: BTreeMap<(PlayerId, TeamId), Relation>,
    team_team: BTreeMap<(TeamId, TeamId), Relation>,
    team_player: BTreeMap<(TeamId, PlayerId), Relation>,
}

impl Relations {
    pub(crate) fn set_player_player(&mut self, from: PlayerId, to: PlayerId, r: Relation) {
        self.player_player.insert((from, to), r);
    }

    pub(crate) fn set_player_team(&mut self, from: PlayerId, to: TeamId, r: Relation) {
        self.player_team.insert((from, to), r);
    }

    pub(crate) fn set_team_team(&mut self, from: TeamId, to: TeamId, r: Relation) {
        self.team_team.insert((from, to), r);
    }

    pub(crate) fn set_team_player(&mut self, from: TeamId, to: PlayerId, r: Relation) {
        self.team_player.insert((from, to), r);
    }

    pub(crate) fn remove_team_overrides(&mut self, team: TeamId, to: Option<TeamId>) {
        match to {
            Some(to) => {
                self.team_team.remove(&(team, to));
            }
            None => {
                self.team_team.retain(|(from, _), _| *from != team);
                self.team_player.retain(|(from, _), _| *from != team);
            }
        }
    }

    pub(crate) fn forget_team(&mut self, team: TeamId) {
        self.team_team.retain(|(a, b), _| *a != team && *b != team);
        self.team_player.retain(|(a, _), _| *a != team);
        self.player_team.retain(|(_, b), _| *b != team);
    }

    pub(crate) fn player_to_player(
        &self,
        from: PlayerId,
        to: PlayerId,
        base: Relation,
    ) -> Relation {
        self.player_player.get(&(from, to)).copied().unwrap_or(base)
    }

    pub(crate) fn team_to_team(
        &self,
        (a, owner_a): (TeamId, PlayerId),
        (b, owner_b): (TeamId, PlayerId),
        base: Relation,
    ) -> Relation {
        self.team_team
            .get(&(a, b))
            .or_else(|| self.team_player.get(&(a, owner_b)))
            .or_else(|| self.player_team.get(&(owner_a, b)))
            .copied()
            .unwrap_or_else(|| self.player_to_player(owner_a, owner_b, base))
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.player_player.is_empty()
            && self.player_team.is_empty()
            && self.team_team.is_empty()
            && self.team_player.is_empty()
    }

    pub(crate) fn object_relation(
        &self,
        (me_team, me_player): (Option<TeamId>, PlayerId),
        (that_team, that_player): (Option<TeamId>, PlayerId),
        base: Relation,
    ) -> Relation {
        me_team
            .zip(that_team)
            .and_then(|pair| self.team_team.get(&pair))
            .or_else(|| me_team.and_then(|a| self.team_player.get(&(a, that_player))))
            .or_else(|| that_team.and_then(|b| self.player_team.get(&(me_player, b))))
            .copied()
            .unwrap_or_else(|| self.player_to_player(me_player, that_player, base))
    }

    pub(crate) fn player_view(
        &self,
        viewer: PlayerId,
        that_team: Option<TeamId>,
        that_player: PlayerId,
        base: Relation,
    ) -> Relation {
        that_team
            .and_then(|b| self.player_team.get(&(viewer, b)))
            .copied()
            .unwrap_or_else(|| self.player_to_player(viewer, that_player, base))
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.player_player.len());
        for ((a, b), r) in &self.player_player {
            a.hash_into(hasher);
            b.hash_into(hasher);
            hasher.write_u8(r.code());
        }
        hasher.write_usize(self.player_team.len());
        for ((a, b), r) in &self.player_team {
            a.hash_into(hasher);
            hasher.write_u32(b.get());
            hasher.write_u8(r.code());
        }
        hasher.write_usize(self.team_team.len());
        for ((a, b), r) in &self.team_team {
            hasher.write_u32(a.get());
            hasher.write_u32(b.get());
            hasher.write_u8(r.code());
        }
        hasher.write_usize(self.team_player.len());
        for ((a, b), r) in &self.team_player {
            hasher.write_u32(a.get());
            b.hash_into(hasher);
            hasher.write_u8(r.code());
        }
    }
}

pub(crate) const ATTITUDE_PEACEFUL: i8 = -3;
pub(crate) const ATTITUDE_SLEEP: i8 = -2;
pub(crate) const ATTITUDE_PASSIVE: i8 = -1;
pub(crate) const ATTITUDE_NORMAL: i8 = 0;
pub(crate) const ATTITUDE_ALERT: i8 = 1;
pub(crate) const ATTITUDE_AGGRESSIVE: i8 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Attitudes(BTreeMap<Entity, (i8, u64)>);

impl Attitudes {
    pub(crate) fn set(&mut self, entity: Entity, value: i32, stamp: u64) -> bool {
        match i8::try_from(value) {
            Ok(v) if (ATTITUDE_PEACEFUL..=ATTITUDE_AGGRESSIVE).contains(&v) => {
                self.0.insert(entity, (v, stamp));
                true
            }
            _ => false,
        }
    }

    #[cfg(test)]
    pub(crate) fn get(&self, entity: Entity) -> i8 {
        self.0.get(&entity).map_or(ATTITUDE_NORMAL, |(v, _)| *v)
    }

    pub(crate) fn effective(&self, entity: Entity, teams: &Teams, protos: &TeamProtos) -> i8 {
        let joined = teams.joined_at(entity);
        if let Some(&(value, stamp)) = self.0.get(&entity) {
            if stamp >= joined {
                return value;
            }
        }
        teams
            .proto_of(entity)
            .and_then(|proto| protos.get(proto))
            .and_then(|proto| i8::try_from(proto.template.aggressiveness).ok())
            .filter(|v| (ATTITUDE_PEACEFUL..=ATTITUDE_AGGRESSIVE).contains(v))
            .unwrap_or(ATTITUDE_NORMAL)
    }

    pub(crate) fn retain_live(&mut self, is_live: impl Fn(Entity) -> bool) {
        self.0.retain(|entity, _| is_live(*entity));
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for (entity, (value, stamp)) in &self.0 {
            entity.hash_into(hasher);
            hasher.write_i32(i32::from(*value));
            hasher.write_u64(*stamp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: PlayerId = PlayerId::new(0);
    const B: PlayerId = PlayerId::new(1);

    #[test]
    fn a_team_override_leaves_its_players_relation_unchanged() {
        let mut relations = Relations::default();
        let (t0, t1) = (TeamId::new(1), TeamId::new(2));
        relations.set_team_team(t0, t1, Relation::Allies);
        assert_eq!(
            relations.team_to_team((t0, A), (t1, B), Relation::Enemies),
            Relation::Allies
        );
        assert_eq!(
            relations.player_to_player(A, B, Relation::Enemies),
            Relation::Enemies
        );
        assert_eq!(
            relations.team_to_team((t1, B), (t0, A), Relation::Enemies),
            Relation::Enemies
        );
    }

    #[test]
    fn a_player_relation_is_set_one_way() {
        let mut relations = Relations::default();
        relations.set_player_player(A, B, Relation::Allies);
        assert_eq!(
            relations.player_to_player(A, B, Relation::Enemies),
            Relation::Allies
        );
        assert_eq!(
            relations.player_to_player(B, A, Relation::Enemies),
            Relation::Enemies
        );
    }
}
