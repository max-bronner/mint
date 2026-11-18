use std::collections::BTreeMap;

use crate::ai::data::{Main, TYPES};
use crate::model::{ArmyId, BuildingId, PlayerId, RegionId};
use crate::save::codec::{Dec, Enc};
use crate::save::LoadError;
use crate::state::LivingWorldState;

pub const FAR: u16 = 9999;

#[must_use]
pub fn composition_weight(members: &[(usize, u32)], unit_type: usize) -> i32 {
    members
        .iter()
        .filter(|(t, _)| *t == unit_type)
        .fold(0i32, |sum, (_, id)| {
            sum.saturating_add(i32::try_from(*id).unwrap_or(i32::MAX))
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArmyRec {
    pub id: ArmyId,
    pub player: PlayerId,
    pub region: RegionId,
    pub hero: bool,
    pub members: Vec<(usize, u32)>,
}

impl ArmyRec {
    #[must_use]
    pub fn weight(&self, unit_type: usize) -> i32 {
        composition_weight(&self.members, unit_type)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Built {
    pub plot: u8,
    pub building: BuildingId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionRec {
    pub id: RegionId,
    pub owner: Option<PlayerId>,
    pub strategic_value: i32,
    pub empty_plots: i32,
    pub under_construction: bool,
    pub strengthen: Vec<Built>,
    pub upgrade: Vec<Built>,
    pub world_cp: Vec<Built>,
    pub spawn_only: Vec<Built>,
    pub spawn_other: Vec<Built>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Summary {
    pub cp_used: i32,
    pub cp_limit: i32,
    pub cp_max: i32,
    pub money: i32,
    pub income: i32,
    pub regions: Vec<RegionId>,
    pub hero_armies: Vec<ArmyId>,
    pub heroless: Vec<ArmyId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub regions: Vec<RegionRec>,
    armies: BTreeMap<ArmyId, ArmyRec>,
    pub summaries: Vec<Summary>,
    dist: Vec<Vec<Option<u16>>>,
    rings: Vec<Vec<Vec<RegionId>>>,
}

fn region_id(index: usize) -> Option<RegionId> {
    u16::try_from(index).ok().map(RegionId)
}

fn hops(state: &LivingWorldState, start: usize) -> (Vec<Option<u16>>, Vec<Vec<RegionId>>) {
    let n = state.regions.len();
    let mut dist: Vec<Option<u16>> = vec![None; n];
    let mut rings: Vec<Vec<RegionId>> = Vec::new();
    let enabled = |i: usize| state.regions.get(i).is_some_and(|r| r.enabled);
    if !enabled(start) {
        return (dist, rings);
    }
    if let Some(slot) = dist.get_mut(start) {
        *slot = Some(0);
    }
    let mut frontier: Vec<usize> = vec![start];
    let mut depth: u16 = 0;
    while !frontier.is_empty() {
        rings.push(frontier.iter().filter_map(|i| region_id(*i)).collect());
        let mut next = Vec::new();
        for index in &frontier {
            let Some(rules) = region_id(*index).and_then(|r| state.rules.region_rules(r)) else {
                continue;
            };
            for nb in &rules.neighbours {
                let at = nb.index();
                if enabled(at) && dist.get(at).is_some_and(Option::is_none) {
                    if let Some(slot) = dist.get_mut(at) {
                        *slot = Some(depth.saturating_add(1));
                    }
                    next.push(at);
                }
            }
        }
        frontier = next;
        depth = depth.saturating_add(1);
    }
    (dist, rings)
}

impl Snapshot {
    #[must_use]
    pub fn take(state: &LivingWorldState) -> Self {
        let mut armies = BTreeMap::new();
        for a in state.armies.values() {
            let members = a
                .roster
                .iter()
                .map(|m| {
                    (
                        state
                            .rules
                            .unit(m.unit)
                            .unit_type
                            .map_or(TYPES - 1, usize::from),
                        m.id,
                    )
                })
                .collect();
            armies.insert(
                a.id,
                ArmyRec {
                    id: a.id,
                    player: a.player,
                    region: a.region,
                    hero: a.is_hero_army(),
                    members,
                },
            );
        }

        let mut regions = Vec::new();
        for (index, region) in state.regions.iter().enumerate() {
            let Some(id) = region_id(index) else { break };
            let mut rec = RegionRec {
                id,
                owner: region.owner,
                strategic_value: state
                    .rules
                    .region_rules(id)
                    .map_or(0, |r| r.strategic_value),
                empty_plots: 0,
                under_construction: false,
                strengthen: Vec::new(),
                upgrade: Vec::new(),
                world_cp: Vec::new(),
                spawn_only: Vec::new(),
                spawn_other: Vec::new(),
            };
            for (slot, plot) in region.plots.iter().enumerate() {
                let Some(plot) = plot else {
                    rec.empty_plots = rec.empty_plots.saturating_add(1);
                    continue;
                };
                if plot.under_construction {
                    rec.under_construction = true;
                    continue;
                }
                let Some(ai) = state.rules.ai.building(plot.building) else {
                    continue;
                };
                let built = Built {
                    plot: u8::try_from(slot).unwrap_or(u8::MAX),
                    building: plot.building,
                };
                match ai.main {
                    Some(Main::WorldCp) => rec.world_cp.push(built),
                    Some(Main::Strengthen) => rec.strengthen.push(built),
                    Some(Main::Upgrade) => rec.upgrade.push(built),
                    None => {}
                }
                if ai.spawn.is_some() {
                    if ai.main.is_none() {
                        rec.spawn_only.push(built);
                    } else {
                        rec.spawn_other.push(built);
                    }
                }
            }
            regions.push(rec);
        }

        let mut summaries = Vec::new();
        for p in &state.players {
            let me = p.id;
            let mut summary = Summary {
                cp_used: state.world_cp_in_use(me),
                cp_limit: state.world_cp_limit(me),
                cp_max: state
                    .rules
                    .templates
                    .get(usize::from(p.template.0))
                    .map_or(0, |t| t.max_world_cp),
                money: p.resources,
                ..Summary::default()
            };
            for rec in regions.iter().filter(|r| r.owner == Some(me)) {
                summary.regions.push(rec.id);
                let fertile = state
                    .rules
                    .region_rules(rec.id)
                    .map_or(0, |r| r.bonus.get(crate::model::bonus::FERTILE_TERRITORY));
                summary.income = summary.income.saturating_add(fertile);
                let treasury = state
                    .region(rec.id)
                    .into_iter()
                    .flat_map(|r| r.plots.iter().flatten())
                    .filter(|b| !b.under_construction)
                    .filter_map(|b| state.rules.building(b.building))
                    .flat_map(|b| b.treasury.iter().copied())
                    .fold(0i32, i32::saturating_add);
                summary.income = summary.income.saturating_add(treasury);
            }
            for a in armies.values().filter(|a| a.player == me) {
                if a.hero {
                    summary.hero_armies.push(a.id);
                } else if !a.members.is_empty() {
                    summary.heroless.push(a.id);
                }
            }
            summaries.push(summary);
        }

        Self::with_paths(state, regions, armies, summaries)
    }

    fn with_paths(
        state: &LivingWorldState,
        regions: Vec<RegionRec>,
        armies: BTreeMap<ArmyId, ArmyRec>,
        summaries: Vec<Summary>,
    ) -> Self {
        let n = state.regions.len();
        let mut dist = Vec::with_capacity(n);
        let mut rings = Vec::with_capacity(n);
        for start in 0..n {
            let (d, r) = hops(state, start);
            dist.push(d);
            rings.push(r);
        }
        Self {
            regions,
            armies,
            summaries,
            dist,
            rings,
        }
    }

    pub(crate) fn write_save(&self, e: &mut Enc) {
        let put_built = |e: &mut Enc, list: &Vec<Built>| {
            e.list(list, |e, b| {
                e.u8(b.plot);
                e.u16(b.building.0);
            });
        };
        e.list(&self.regions, |e, r| {
            e.u16(r.id.0);
            e.opt(r.owner, |e, o| e.u8(o.0));
            e.i32(r.strategic_value);
            e.i32(r.empty_plots);
            e.bool(r.under_construction);
            for list in [
                &r.strengthen,
                &r.upgrade,
                &r.world_cp,
                &r.spawn_only,
                &r.spawn_other,
            ] {
                put_built(e, list);
            }
        });
        e.len(self.armies.len());
        for a in self.armies.values() {
            e.u32(a.id.0);
            e.u8(a.player.0);
            e.u16(a.region.0);
            e.bool(a.hero);
            e.list(&a.members, |e, (kind, id)| {
                e.u32(u32::try_from(*kind).unwrap_or(u32::MAX));
                e.u32(*id);
            });
        }
        e.list(&self.summaries, |e, s| {
            for v in [s.cp_used, s.cp_limit, s.cp_max, s.money, s.income] {
                e.i32(v);
            }
            e.list(&s.regions, |e, r| e.u16(r.0));
            e.list(&s.hero_armies, |e, a| e.u32(a.0));
            e.list(&s.heroless, |e, a| e.u32(a.0));
        });
    }

    pub(crate) fn read_save(state: &LivingWorldState, d: &mut Dec) -> Result<Self, LoadError> {
        let get_built = |d: &mut Dec| {
            d.list(|d| {
                Ok(Built {
                    plot: d.u8()?,
                    building: BuildingId(d.u16()?),
                })
            })
        };
        let regions = d.list(|d| {
            Ok(RegionRec {
                id: RegionId(d.u16()?),
                owner: d.opt(|d| Ok(PlayerId(d.u8()?)))?,
                strategic_value: d.i32()?,
                empty_plots: d.i32()?,
                under_construction: d.bool()?,
                strengthen: get_built(d)?,
                upgrade: get_built(d)?,
                world_cp: get_built(d)?,
                spawn_only: get_built(d)?,
                spawn_other: get_built(d)?,
            })
        })?;
        let mut armies = BTreeMap::new();
        for rec in d.list(|d| {
            Ok(ArmyRec {
                id: ArmyId(d.u32()?),
                player: PlayerId(d.u8()?),
                region: RegionId(d.u16()?),
                hero: d.bool()?,
                members: d
                    .list(|d| Ok((usize::try_from(d.u32()?).unwrap_or(usize::MAX), d.u32()?)))?,
            })
        })? {
            armies.insert(rec.id, rec);
        }
        let summaries = d.list(|d| {
            Ok(Summary {
                cp_used: d.i32()?,
                cp_limit: d.i32()?,
                cp_max: d.i32()?,
                money: d.i32()?,
                income: d.i32()?,
                regions: d.list(|d| Ok(RegionId(d.u16()?)))?,
                hero_armies: d.list(|d| Ok(ArmyId(d.u32()?)))?,
                heroless: d.list(|d| Ok(ArmyId(d.u32()?)))?,
            })
        })?;
        Ok(Self::with_paths(state, regions, armies, summaries))
    }

    #[must_use]
    pub fn region(&self, id: RegionId) -> Option<&RegionRec> {
        self.regions.get(id.index())
    }

    #[must_use]
    pub fn army(&self, id: ArmyId) -> Option<&ArmyRec> {
        self.armies.get(&id)
    }

    pub fn armies(&self) -> impl Iterator<Item = &ArmyRec> {
        self.armies.values()
    }

    #[must_use]
    pub fn summary(&self, p: PlayerId) -> Option<&Summary> {
        self.summaries.get(p.index())
    }

    #[must_use]
    pub fn dist(&self, a: RegionId, b: RegionId) -> Option<u16> {
        self.dist.get(a.index())?.get(b.index()).copied().flatten()
    }

    #[must_use]
    pub fn dist_or_far(&self, a: RegionId, b: RegionId) -> u16 {
        self.dist(a, b).unwrap_or(FAR)
    }

    #[must_use]
    pub fn ring(&self, from: RegionId, hops: u16) -> &[RegionId] {
        self.rings
            .get(from.index())
            .and_then(|r| r.get(usize::from(hops)))
            .map_or(&[], Vec::as_slice)
    }

    pub fn nearest(
        &self,
        from: RegionId,
        keep_last: bool,
        keep: impl Fn(&ArmyRec) -> bool,
    ) -> Option<(&ArmyRec, u16)> {
        let mut best: Option<(&ArmyRec, u16)> = None;
        for a in self.armies.values().filter(|a| keep(a)) {
            let Some(d) = self.dist(from, a.region) else {
                continue;
            };
            let better = match best {
                None => true,
                Some((_, bd)) => d < bd || (keep_last && d == bd),
            };
            if better {
                best = Some((a, d));
            }
        }
        best
    }

    pub fn nearest_region(
        &self,
        from: RegionId,
        keep: impl Fn(&RegionRec) -> bool,
    ) -> Option<RegionId> {
        let rings = self.rings.get(from.index())?;
        rings
            .iter()
            .flatten()
            .find(|r| self.region(**r).is_some_and(&keep))
            .copied()
    }
}
