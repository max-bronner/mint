use mint_sim::{Entity, StateHasher};

use super::count_cache::CountStamp;
use super::lists::ObjectLists;
use super::priority::AttackPriorities;
use super::refs::ObjectRefs;
use super::relations::{Attitudes, Relations};
use super::status::ScriptStatuses;
use super::team::{TeamId, Teams};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ScriptRuntime {
    pub(crate) teams: Teams,
    pub(crate) objects: ObjectRefs,
    pub(crate) lists: ObjectLists,
    pub(crate) relations: Relations,
    pub(crate) attitudes: Attitudes,
    pub(crate) priorities: AttackPriorities,
    pub(crate) statuses: ScriptStatuses,
    pub(crate) hunters: super::orders::Hunters,
    pub(crate) stand_ground: super::stand_ground::StandGround,
    pub(crate) attack_areas: super::attack_area::AttackAreas,
    pub(crate) counts: CountStamp,
    pub(crate) area_watch: super::area_events::AreaWatch,
    pub(crate) walks: super::walks::Walks,
    pub(crate) facings: super::face_teleport::Facings,
    pub(crate) standing: super::standing::PlayerStanding,
    pub(crate) seq: super::sequential::Sequential,
    pub(crate) triggered_powers: super::powers::TriggeredPowers,
    pub(crate) topple: super::topple::ToppleDirections,
    pub(crate) builders: super::team_build::Builders,
    pub(crate) wanderers: super::wander::Wanderers,
    pub(crate) buildability: super::buildability::BuildOverrides,
    pub(crate) power_ready: super::skirmish_ready::PowerReadyCache,
    pub(crate) condition_team: Option<TeamId>,
    pub(crate) calling_team: Option<TeamId>,
    pub(crate) completed_upgrades: super::built_upgrades::CompletedUpgrades,
}

impl ScriptRuntime {
    pub(crate) fn retain_live(&mut self, is_live: impl Fn(Entity) -> bool + Copy) {
        self.teams.retain_live(is_live);
        self.objects.retain_live(is_live);
        self.attitudes.retain_live(is_live);
        self.priorities.retain_live(is_live);
        self.statuses.retain_live(is_live);
        self.hunters.retain_live(is_live);
        self.stand_ground.retain_live(is_live);
        self.attack_areas.retain_live(is_live);
        self.walks.retain_live(is_live);
        self.wanderers.retain_live(is_live);
        self.facings.retain_live(is_live);
        self.builders.retain_live(is_live);
        self.standing.retain_live(is_live);
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        self.teams.hash_into(hasher);
        self.objects.hash_into(hasher);
        self.lists.hash_into(hasher);
        self.relations.hash_into(hasher);
        self.attitudes.hash_into(hasher);
        self.priorities.hash_into(hasher);
        self.statuses.hash_into(hasher);
        self.hunters.hash_into(hasher);
        self.stand_ground.hash_into(hasher);
        self.attack_areas.hash_into(hasher);
        self.counts.hash_into(hasher);
        self.area_watch.hash_into(hasher);
        self.walks.hash_into(hasher);
        self.facings.hash_into(hasher);
        self.standing.hash_into(hasher);
        self.seq.hash_into(hasher);
        self.triggered_powers.hash_into(hasher);
        self.topple.hash_into(hasher);
        self.builders.hash_into(hasher);
        self.wanderers.hash_into(hasher);
        self.buildability.hash_into(hasher);
        self.power_ready.hash_into(hasher);
        for team in [self.condition_team, self.calling_team] {
            hasher.write_u32(team.map_or(0, TeamId::get));
        }
        self.completed_upgrades.hash_into(hasher);
    }
}
