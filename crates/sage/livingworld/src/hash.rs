use mint_ecs::StateHasher;

use crate::model::{Bonus, Phase};
use crate::state::LivingWorldState;

fn write_len(h: &mut StateHasher, len: usize) {
    h.write_u64(u64::try_from(len).unwrap_or(u64::MAX));
}

fn write_bonus(h: &mut StateHasher, b: &Bonus) {
    for v in b.0 {
        h.write_i32(v);
    }
}

fn write_names(h: &mut StateHasher, names: &[String]) {
    write_len(h, names.len());
    for name in names {
        write_len(h, name.len());
        for byte in name.bytes() {
            h.write_u8(byte);
        }
    }
}

fn write_opt_region(h: &mut StateHasher, r: Option<crate::model::RegionId>) {
    h.write_u32(r.map_or(u32::MAX, |r| u32::from(r.0)));
}

impl LivingWorldState {
    #[must_use]
    pub fn state_hash(&self) -> u64 {
        let mut h = StateHasher::new();
        h.write_u32(self.turn);
        h.write_u8(self.phase.ordinal());
        let (state, stream) = self.rng.to_parts();
        h.write_u64(state);
        h.write_u64(stream);
        h.write_u32(self.next_army_id);
        h.write_u32(self.next_member_id);
        h.write_u32(self.next_battle_id);
        h.write_u32(self.awaiting.unwrap_or(0));
        h.write_u8(u8::from(self.awaiting.is_some()));
        h.write_i32(self.ended.map_or(-2, |t| t.unwrap_or(-1)));

        write_len(&mut h, self.rules.regions.len());
        write_len(&mut h, self.rules.units.len());
        write_len(&mut h, self.rules.buildings.len());

        write_len(&mut h, self.players.len());
        for p in &self.players {
            h.write_u8(p.id.0);
            h.write_u8(p.template.0);
            h.write_u8(p.kind as u8);
            h.write_i32(p.team);
            write_opt_region(&mut h, p.base_region);
            h.write_i32(p.resources);
            h.write_i32(p.reserved_cp);
            h.write_bool(p.ready);
            h.write_bool(p.defeated);
            write_bonus(&mut h, &p.bonus_a);
            write_len(&mut h, p.territories.len());
            for t in &p.territories {
                h.write_u16(*t);
            }
            write_len(&mut h, p.fallen_heroes.len());
            for f in &p.fallen_heroes {
                h.write_u16(f.unit.0);
                h.write_u32(f.level);
                h.write_u32(f.experience);
            }
            h.write_u8(p.skill.difficulty());
            h.write_u32(p.skill_points.to_bits());
            h.write_i32(p.rank_points);
            write_names(&mut h, &p.sciences);
            write_names(&mut h, &p.upgrades);
        }

        write_len(&mut h, self.regions.len());
        for r in &self.regions {
            h.write_u8(r.owner.map_or(u8::MAX, |o| o.0));
            h.write_bool(r.enabled);
            h.write_u32(r.changed_turn);
            write_len(&mut h, r.present.len());
            for a in &r.present {
                h.write_u32(a.0);
            }
            write_len(&mut h, r.plots.len());
            for plot in &r.plots {
                match plot {
                    None => h.write_u8(0),
                    Some(b) => {
                        h.write_u8(1);
                        h.write_u16(b.building.0);
                        h.write_i32(b.turns_left);
                        h.write_bool(b.under_construction);
                        h.write_u32(b.queue_turns);
                        h.write_i32(b.upgrades_used);
                        write_len(&mut h, b.queue.len());
                        for entry in &b.queue {
                            h.write_u8(*entry);
                        }
                    }
                }
            }
        }

        write_len(&mut h, self.armies.len());
        for a in self.armies.values() {
            h.write_u32(a.id.0);
            h.write_u8(a.player.0);
            h.write_u32(a.hero.map_or(u32::MAX, |u| u32::from(u.0)));
            write_opt_region(&mut h, Some(a.region));
            write_opt_region(&mut h, a.prev_region);
            h.write_u32(a.turns_here);
            write_opt_region(&mut h, a.order);
            h.write_bool(a.destroy_queued);
            write_roster(&mut h, &a.roster);
        }

        write_len(&mut h, self.battles.len());
        for b in &self.battles {
            h.write_u32(b.id);
            write_opt_region(&mut h, Some(b.region));
            write_len(&mut h, b.players.len());
            for p in &b.players {
                h.write_u8(p.0);
            }
            write_len(&mut h, b.choices.len());
            for (p, method) in &b.choices {
                h.write_u8(p.0);
                h.write_u8(*method);
            }
            write_len(&mut h, b.withdrawn.len());
            for p in &b.withdrawn {
                h.write_u8(p.0);
            }
            write_len(&mut h, b.snapshot.len());
            for (army, roster) in &b.snapshot {
                h.write_u32(army.0);
                write_roster(&mut h, roster);
            }
        }
        write_len(&mut h, self.captures.len());
        for c in &self.captures {
            h.write_u8(c.player.0);
            write_opt_region(&mut h, Some(c.region));
            h.write_u8(c.flags);
        }
        write_len(&mut h, self.decisions.len());
        for d in self.decisions.values() {
            write_opt_region(&mut h, Some(d.region));
            h.write_u8(d.flags);
            h.write_bool(d.resolved);
            write_len(&mut h, d.players.len());
            for p in &d.players {
                h.write_u8(p.0);
            }
        }
        write_len(&mut h, self.retreat_entries.len());
        for a in &self.retreat_entries {
            h.write_u32(a.0);
        }
        write_len(&mut h, self.destroy_pending.len());
        for a in &self.destroy_pending {
            h.write_u32(a.0);
        }
        match &self.scripted {
            None => h.write_u8(0),
            Some(scripted) => {
                h.write_u8(1);
                write_len(&mut h, scripted.armies.len());
                for (name, army) in &scripted.armies {
                    write_names(&mut h, std::slice::from_ref(name));
                    h.write_u32(army.0);
                }
            }
        }
        h.write_u8(self.battle_type.ordinal());
        if !self.palantir_markers.is_empty() {
            write_len(&mut h, self.palantir_markers.len());
            for marker in &self.palantir_markers {
                write_names(&mut h, &[marker.event.clone(), marker.movie.clone()]);
                h.write_f32(marker.position[0]);
                h.write_f32(marker.position[1]);
                h.write_bool(marker.play_next_act_after_movie);
            }
        }
        h.finish()
    }
}

impl Phase {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Planning => "Planning",
            Self::MoveArmies => "MoveArmies",
            Self::ResolveBattles => "ResolveBattles",
            Self::FakePhaseOne => "FakePhaseOne",
            Self::PlanRetreats => "PlanRetreats",
            Self::RetreatArmies => "RetreatArmies",
            Self::Complete => "Complete",
        }
    }
}

fn write_roster(h: &mut StateHasher, roster: &[crate::model::Member]) {
    write_len(h, roster.len());
    for m in roster {
        h.write_u32(m.id);
        h.write_u16(m.unit.0);
        h.write_u32(m.level);
        h.write_u32(m.count);
        h.write_u32(m.experience);
        h.write_u32(m.battles);
        h.write_u32(m.kills);
        h.write_u32(m.kills_battle);
        match m.upgraded_at {
            None => h.write_u8(0),
            Some((region, plot)) => {
                h.write_u8(1);
                h.write_u16(region.0);
                h.write_u8(plot);
            }
        }
    }
}
