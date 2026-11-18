use crate::model::{ArmyId, Event, PlayerId, RegionId, Rejected};
use crate::state::LivingWorldState;

impl LivingWorldState {
    pub(crate) fn retreat_triage(&mut self, events: &mut Vec<Event>) {
        let entries = self.retreat_entries.clone();
        let mut keep: Vec<ArmyId> = Vec::new();
        for id in entries.into_iter().rev() {
            let Some(army) = self.armies.get(&id) else {
                continue;
            };
            let (player, cur) = (army.player, army.region);
            let back = army
                .prev_region
                .filter(|prev| *prev != cur && self.allied_owner(self.owner(*prev), player));
            let target = match back {
                Some(prev) => Some(prev),
                None => {
                    let options: Vec<RegionId> = self
                        .rules
                        .region_rules(cur)
                        .map(|r| r.neighbours.clone())
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|n| self.allied_owner(self.owner(*n), player))
                        .collect();
                    match options.as_slice() {
                        [only] => Some(*only),
                        [] => {
                            events.push(Event::HeroKilled { army: id });
                            self.queue_destroy(id);
                            continue;
                        }
                        _ => {
                            keep.push(id);
                            continue;
                        }
                    }
                }
            };
            if let Some(dest) = target {
                if let Some(a) = self.armies.get_mut(&id) {
                    a.order = Some(dest);
                }
                events.push(Event::RetreatChosen { army: id, dest });
            }
        }
        keep.reverse();
        self.retreat_entries = keep;
    }

    pub(crate) fn order_retreat(
        &mut self,
        player: PlayerId,
        army: ArmyId,
        dest: RegionId,
    ) -> Result<Vec<Event>, Rejected> {
        if !self.retreat_entries.contains(&army) {
            return Err(Rejected::NoOrderPending);
        }
        self.check_order(player, army, dest, true)?;
        if let Some(a) = self.armies.get_mut(&army) {
            a.order = Some(dest);
        }
        Ok(vec![Event::RetreatChosen { army, dest }])
    }

    pub(crate) fn order_disband(
        &mut self,
        player: PlayerId,
        army_id: ArmyId,
        member: Option<u32>,
    ) -> Result<Vec<Event>, Rejected> {
        let army = self.armies.get(&army_id).ok_or(Rejected::NoSuchArmy)?;
        if army.player != player {
            return Err(Rejected::NotYourArmy);
        }
        let removable = |m: &crate::model::Member| !self.rules.unit(m.unit).hero;
        let doomed: Vec<crate::model::Member> = match member {
            Some(id) => {
                let found = army
                    .roster
                    .iter()
                    .find(|m| m.id == id)
                    .ok_or(Rejected::NoSuchMember)?;
                if !removable(found) {
                    return Err(Rejected::MemberIsHero);
                }
                vec![found.clone()]
            }
            None => army
                .roster
                .iter()
                .filter(|m| removable(m))
                .cloned()
                .collect(),
        };
        let mut events = Vec::new();
        for m in &doomed {
            let refund = self.unit_price(player, m.unit);
            self.add_resources(player, refund);
            if let Some((region, plot)) = m.upgraded_at {
                if let Some(p) = self
                    .region_mut(region)
                    .and_then(|r| r.plots.get_mut(usize::from(plot)))
                    .and_then(|p| p.as_mut())
                {
                    p.upgrades_used = p.upgrades_used.saturating_sub(1).max(0);
                }
            }
            events.push(Event::MemberDisbanded {
                army: army_id,
                member: m.id,
                refund,
            });
        }
        if let Some(a) = self.armies.get_mut(&army_id) {
            a.roster.retain(|m| !doomed.iter().any(|d| d.id == m.id));
        }
        Ok(events)
    }

    pub(crate) fn order_swap(
        &mut self,
        player: PlayerId,
        a: ArmyId,
        b: ArmyId,
        a_members: &[u32],
        b_members: &[u32],
    ) -> Result<Vec<Event>, Rejected> {
        self.check_swap(player, a, b, a_members, b_members)?;
        self.move_members(a, b, a_members);
        self.move_members(b, a, b_members);
        Ok(Vec::new())
    }

    pub fn check_swap(
        &self,
        player: PlayerId,
        a: ArmyId,
        b: ArmyId,
        a_members: &[u32],
        b_members: &[u32],
    ) -> Result<(), Rejected> {
        let (Some(first), Some(second)) = (self.armies.get(&a), self.armies.get(&b)) else {
            return Err(Rejected::NoSuchArmy);
        };
        if a == b || first.player != player || second.player != player {
            return Err(Rejected::NotYourArmy);
        }
        let give = |from: &crate::model::Army, to: &crate::model::Army, ids: &[u32]| {
            let found: Vec<&crate::model::Member> = ids
                .iter()
                .filter_map(|id| from.roster.iter().find(|m| m.id == *id))
                .collect();
            if found.is_empty() {
                return Ok(0i32);
            }
            if found.iter().any(|m| Some(m.unit) == from.hero) {
                return Err(Rejected::MemberIsHero);
            }
            self.check_order(player, from.id, to.region, false)?;
            Ok(found.iter().fold(0i32, |sum, m| {
                let each = self.rules.unit(m.unit).command_points;
                sum.saturating_add(each.saturating_mul(i32::try_from(m.count).unwrap_or(i32::MAX)))
            }))
        };
        let (cp_a, cp_b) = (
            give(first, second, a_members)?,
            give(second, first, b_members)?,
        );
        let fits = if cp_a > cp_b {
            self.army_cp(second)
                .saturating_add(cp_a.saturating_sub(cp_b))
                <= self.army_cap(second)
        } else {
            self.army_cp(first)
                .saturating_add(cp_b.saturating_sub(cp_a))
                <= self.army_cap(first)
        };
        if !fits {
            return Err(Rejected::InvalidSwap);
        }
        Ok(())
    }

    fn move_members(&mut self, from: ArmyId, to: ArmyId, ids: &[u32]) {
        for id in ids {
            let Some(source) = self.armies.get_mut(&from) else {
                return;
            };
            let Some(at) = source.roster.iter().position(|m| m.id == *id) else {
                continue;
            };
            let member = source.roster.remove(at);
            if let Some(target) = self.armies.get_mut(&to) {
                target.roster.push(member);
            }
        }
    }
}
