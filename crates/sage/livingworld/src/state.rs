#![allow(missing_docs)]

use std::collections::BTreeMap;

use mint_math::rng::Pcg32;

use crate::model::{
    Army, ArmyId, AutoResolveDialog, Battle, Capture, Decision, Phase, Player, PlayerId,
    PlayerKind, RegionId, RegionState,
};
use crate::rules::Rules;

#[derive(Debug, Clone)]
pub struct LivingWorldState {
    pub rules: Rules,
    pub turn: u32,
    pub phase: Phase,
    pub rng: Pcg32,
    pub players: Vec<Player>,
    pub regions: Vec<RegionState>,
    pub armies: BTreeMap<ArmyId, Army>,
    pub next_army_id: u32,
    pub next_member_id: u32,
    pub next_battle_id: u32,
    pub battles: Vec<Battle>,
    pub captures: Vec<Capture>,
    pub decisions: BTreeMap<RegionId, Decision>,
    pub retreat_entries: Vec<ArmyId>,
    pub destroy_pending: Vec<ArmyId>,
    pub awaiting: Option<u32>,
    pub ended: Option<Option<i32>>,
    pub seat_colours: Vec<u32>,
    pub dialog: Option<AutoResolveDialog>,
    pub scripted: Option<Scripted>,
    pub battle_type: crate::screen_queries::BattleType,
    pub palantir_markers: Vec<PalantirMarker>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PalantirMarker {
    pub event: String,
    pub movie: String,
    pub position: [f32; 2],
    pub play_next_act_after_movie: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Scripted {
    pub armies: Vec<(String, ArmyId)>,
}

impl LivingWorldState {
    #[must_use]
    pub fn scripted_army(&self, name: &str) -> Option<ArmyId> {
        self.scripted
            .as_ref()?
            .armies
            .iter()
            .find(|(n, id)| n.eq_ignore_ascii_case(name) && self.armies.contains_key(id))
            .map(|(_, id)| *id)
    }

    #[must_use]
    pub fn player(&self, id: PlayerId) -> Option<&Player> {
        self.players.get(id.index())
    }

    pub(crate) fn player_mut(&mut self, id: PlayerId) -> Option<&mut Player> {
        self.players.get_mut(id.index())
    }

    #[must_use]
    pub fn region(&self, id: RegionId) -> Option<&RegionState> {
        self.regions.get(id.index())
    }

    pub(crate) fn region_mut(&mut self, id: RegionId) -> Option<&mut RegionState> {
        self.regions.get_mut(id.index())
    }

    #[must_use]
    pub fn owner(&self, region: RegionId) -> Option<PlayerId> {
        self.region(region).and_then(|r| r.owner)
    }

    #[must_use]
    pub fn allied(&self, a: PlayerId, b: PlayerId) -> bool {
        if a == b {
            return true;
        }
        match (self.player(a), self.player(b)) {
            (Some(x), Some(y)) => x.team == y.team && x.team != -1,
            _ => false,
        }
    }

    #[must_use]
    pub fn allied_owner(&self, owner: Option<PlayerId>, p: PlayerId) -> bool {
        owner.is_some_and(|o| self.allied(o, p))
    }

    #[must_use]
    pub fn regions_owned(&self, p: PlayerId) -> i32 {
        let n = self.regions.iter().filter(|r| r.owner == Some(p)).count();
        i32::try_from(n).unwrap_or(i32::MAX)
    }

    pub fn armies_of(&self, p: PlayerId) -> impl Iterator<Item = &Army> {
        self.armies.values().filter(move |a| a.player == p)
    }

    pub fn live_players(&self) -> impl Iterator<Item = &Player> {
        self.players.iter().filter(|p| !p.defeated)
    }

    #[must_use]
    pub fn is_defended(&self, region: RegionId) -> bool {
        let Some(state) = self.region(region) else {
            return false;
        };
        if state.owner.is_none() {
            return false;
        }
        let fort = self
            .rules
            .region_rules(region)
            .is_some_and(|r| r.create_auto_fort);
        fort || state.plots.iter().flatten().any(|plot| {
            !plot.under_construction
                && self
                    .rules
                    .building(plot.building)
                    .is_some_and(|b| b.can_defend)
        })
    }

    #[must_use]
    pub fn defended_against(&self, region: RegionId, p: PlayerId) -> bool {
        let owner = self.owner(region);
        owner.is_some() && !self.allied_owner(owner, p) && self.is_defended(region)
    }

    #[must_use]
    pub fn kind(&self, p: PlayerId) -> Option<PlayerKind> {
        self.player(p).map(|x| x.kind)
    }

    pub(crate) fn fresh_army_id(&mut self) -> ArmyId {
        let id = ArmyId(self.next_army_id);
        self.next_army_id = self.next_army_id.saturating_add(1);
        id
    }

    pub(crate) fn fresh_member_id(&mut self) -> u32 {
        let id = self.next_member_id;
        self.next_member_id = self.next_member_id.saturating_add(1);
        id
    }

    pub(crate) fn queue_destroy(&mut self, army: ArmyId) {
        if let Some(a) = self.armies.get_mut(&army) {
            if !a.destroy_queued {
                a.destroy_queued = true;
                self.destroy_pending.push(army);
            }
        }
    }

    pub(crate) fn flush_destroy(&mut self) -> Vec<ArmyId> {
        let pending = std::mem::take(&mut self.destroy_pending);
        for id in &pending {
            self.delete_army(*id);
        }
        pending
    }

    pub(crate) fn delete_army(&mut self, id: ArmyId) {
        self.armies.remove(&id);
        self.retreat_entries.retain(|a| *a != id);
        self.destroy_pending.retain(|a| *a != id);
        for region in &mut self.regions {
            region.present.retain(|a| *a != id);
        }
    }
}
