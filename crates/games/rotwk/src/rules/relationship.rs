use mint_sim::{Entity, PlayerId, World};

use super::map_scripts::{MapScriptState, Relation, Relations, TeamId};
use super::{Ids, RotwkRules};

const DISSIDENT: &str = "EMOTION_DISSIDENT";

pub(crate) struct RelationFrom<'a> {
    rules: &'a RotwkRules,
    world: &'a World,
    ids: Option<Ids>,
    me: Entity,
    mine: Option<PlayerId>,
    state: Option<&'a MapScriptState>,
    dissident: bool,
    my_team: Option<(Option<TeamId>, PlayerId)>,
}

impl RelationFrom<'_> {
    pub(crate) fn to(&self, that: Entity) -> Relation {
        let Some(ids) = self.ids else {
            return Relation::Neutral;
        };
        self.to_owned_by(that, self.world.get(ids.owner, that).map(|owner| owner.0))
    }

    pub(crate) fn to_owned_by(&self, that: Entity, theirs: Option<PlayerId>) -> Relation {
        if self.ids.is_none() {
            return Relation::Neutral;
        }
        let (Some(mine), Some(theirs)) = (self.mine, theirs) else {
            return Relation::Neutral;
        };
        let Some(state) = self.state else {
            return self.rules.alliances.stance(mine, theirs).into();
        };
        if self.me != that && self.dissident {
            return Relation::Enemies;
        }
        let base = self.rules.alliances.stance(mine, theirs).into();
        match self.my_team {
            Some(my_team) if !state.rt.relations.is_empty() => state.rt.relations.object_relation(
                my_team,
                self.rules.team_view(state, that, theirs),
                base,
            ),
            _ => base,
        }
    }
}

impl RotwkRules {
    pub(crate) fn relation_from<'a>(&'a self, world: &'a World, me: Entity) -> RelationFrom<'a> {
        let ids = self.ids();
        let mine = ids.and_then(|ids| world.get(ids.owner, me).map(|owner| owner.0));
        let state = ids.and_then(|ids| self.script_state(world, ids));
        let dissident = state.is_some() && mine.is_some() && self.is_dissident(world, me);
        let my_team = state
            .filter(|state| !state.rt.relations.is_empty())
            .zip(mine)
            .map(|(state, mine)| self.team_view(state, me, mine));
        RelationFrom {
            rules: self,
            world,
            ids,
            me,
            mine,
            state,
            dissident,
            my_team,
        }
    }

    pub(crate) fn player_view(
        &self,
        state: &MapScriptState,
        viewer: PlayerId,
        that: Entity,
        that_owner: PlayerId,
    ) -> Relation {
        let base = self.alliances.stance(viewer, that_owner).into();
        if state.rt.relations.is_empty() {
            return base;
        }
        let team = state.rt.teams.team_of(that);
        let player = team
            .and_then(|team| self.team_player(state, team))
            .unwrap_or(that_owner);
        state.rt.relations.player_view(viewer, team, player, base)
    }

    pub(crate) fn object_relation(&self, world: &World, me: Entity, that: Entity) -> Relation {
        self.relation_from(world, me).to(that)
    }

    pub(crate) fn view_relation(&self, world: &World, viewer: PlayerId, that: Entity) -> Relation {
        let Some(ids) = self.ids() else {
            return Relation::Neutral;
        };
        let Some(owner) = world.get(ids.owner, that).map(|owner| owner.0) else {
            return Relation::Neutral;
        };
        match self.script_state(world, ids) {
            Some(state) => self.player_view(state, viewer, that, owner),
            None => self.alliances.stance(viewer, owner).into(),
        }
    }

    pub(crate) fn views_are_alliance_stances(&self, world: &World, ids: Ids) -> bool {
        self.script_state(world, ids)
            .is_none_or(|state| state.rt.relations.is_empty())
    }

    pub(crate) fn alliance_view(&self, viewer: PlayerId, owner: PlayerId) -> Relation {
        self.alliances.stance(viewer, owner).into()
    }

    pub(crate) fn player_overrides(&self, world: &World, ids: Ids) -> Relations {
        self.script_state(world, ids)
            .map(|state| state.rt.relations.clone())
            .unwrap_or_default()
    }

    pub(crate) fn script_allied(&self, world: &World, ids: Ids, a: PlayerId, b: PlayerId) -> bool {
        let base = self.alliances.stance(a, b).into();
        self.script_state(world, ids).map_or(base, |state| {
            state.rt.relations.player_to_player(a, b, base)
        }) == Relation::Allies
    }

    pub(crate) fn object_hostile(&self, world: &World, me: Entity, that: Entity) -> bool {
        self.object_relation(world, me, that) == Relation::Enemies
    }

    pub(crate) fn object_allied(&self, world: &World, me: Entity, that: Entity) -> bool {
        self.object_relation(world, me, that) == Relation::Allies
    }

    pub(crate) fn object_victim_relation(
        &self,
        world: &World,
        me: Entity,
        that: Entity,
    ) -> mint_sage_combat::victim::Relation {
        self.object_relation(world, me, that).victim()
    }

    pub(crate) fn script_state<'a>(
        &self,
        world: &'a World,
        ids: Ids,
    ) -> Option<&'a MapScriptState> {
        world
            .store(ids.map_script_state)?
            .iter()
            .next()
            .map(|(_, state)| state)
    }

    fn is_dissident(&self, world: &World, entity: Entity) -> bool {
        self.modules.as_ref().is_some_and(|wiring| {
            world
                .get(wiring.state, entity)
                .is_some_and(|held| held.has_model_condition(DISSIDENT))
        })
    }

    fn team_view(
        &self,
        state: &MapScriptState,
        entity: Entity,
        owner: PlayerId,
    ) -> (Option<TeamId>, PlayerId) {
        let team = state.rt.teams.team_of(entity);
        let player = team
            .and_then(|team| self.team_player(state, team))
            .unwrap_or(owner);
        (team, player)
    }

    fn team_player(&self, state: &MapScriptState, team: TeamId) -> Option<PlayerId> {
        let proto = state.rt.teams.get(team)?.proto;
        if let Some(controller) = state.rt.teams.controller(proto) {
            return Some(controller);
        }
        let owner = &self
            .map_scripts
            .as_ref()?
            .team_prototypes()
            .get(proto)?
            .owner;
        self.resolve_side_player(owner)
    }
}
