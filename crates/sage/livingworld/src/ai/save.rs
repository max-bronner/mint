use super::construct::{BuildAction, BuildOrder};
use super::recruit::RecruitOrder;
use super::snapshot::Snapshot;
use super::turn::Skipped;
use super::{AiSeat, MoveKind, MoveOrder, Plan, StrategicAi};
use crate::model::{ArmyId, BuildingId, PlayerId, RegionId};
use crate::save::codec::{Dec, Enc};
use crate::save::LoadError;
use crate::state::LivingWorldState;

fn put_plan(e: &mut Enc, plan: &Plan) {
    e.list(&plan.recruits, |e, r| {
        e.u8(r.plot);
        e.u16(r.region.0);
        e.u16(r.building.0);
        e.u32(u32::try_from(r.entry).unwrap_or(u32::MAX));
    });
    e.list(&plan.builds, |e, b| {
        e.u16(b.region.0);
        match b.action {
            BuildAction::Build(building) => {
                e.u8(0);
                e.u16(building.0);
            }
            BuildAction::Demolish { plot } => {
                e.u8(1);
                e.u8(plot);
            }
        }
    });
    e.list(&plan.moves, |e, m| {
        e.u32(m.army.0);
        e.bool(m.heroless);
        match m.kind {
            MoveKind::Move { region } => {
                e.u8(0);
                e.u16(region.0);
            }
            MoveKind::Transfer { to, member } => {
                e.u8(1);
                e.u32(to.0);
                e.u32(member);
            }
        }
    });
}

fn get_plan(d: &mut Dec) -> Result<Plan, LoadError> {
    let recruits = d.list(|d| {
        Ok(RecruitOrder {
            plot: d.u8()?,
            region: RegionId(d.u16()?),
            building: BuildingId(d.u16()?),
            entry: usize::try_from(d.u32()?).unwrap_or(usize::MAX),
        })
    })?;
    let builds = d.list(|d| {
        let region = RegionId(d.u16()?);
        let action = match d.u8()? {
            0 => BuildAction::Build(BuildingId(d.u16()?)),
            1 => BuildAction::Demolish { plot: d.u8()? },
            _ => return Err(d.invalid("build action")),
        };
        Ok(BuildOrder { region, action })
    })?;
    let moves = d.list(|d| {
        let army = ArmyId(d.u32()?);
        let heroless = d.bool()?;
        let kind = match d.u8()? {
            0 => MoveKind::Move {
                region: RegionId(d.u16()?),
            },
            1 => MoveKind::Transfer {
                to: ArmyId(d.u32()?),
                member: d.u32()?,
            },
            _ => return Err(d.invalid("move kind")),
        };
        Ok(MoveOrder {
            army,
            heroless,
            kind,
        })
    })?;
    Ok(Plan {
        recruits,
        builds,
        moves,
    })
}

impl StrategicAi {
    pub(crate) fn write_save(&self, e: &mut Enc) {
        e.list(&self.seats, |e, s| {
            e.u8(s.player.0);
            e.opt(s.template_name.as_deref(), |e, name| e.str(name));
            e.u8(s.difficulty);
            e.u32(s.turn);
            e.u8(s.substep);
            e.i32(s.money);
            put_plan(e, &s.plan);
            e.bool(s.done);
            e.opt(s.retreated, |e, t| e.u32(t));
        });
        e.opt(self.snapshot.as_ref(), |e, (turn, snap)| {
            e.u32(*turn);
            snap.write_save(e);
        });
        for n in [
            self.skipped.recruits,
            self.skipped.unaffordable_builds,
            self.skipped.illegal_moves,
            self.skipped.retreat_fallbacks,
        ] {
            e.u32(u32::try_from(n).unwrap_or(u32::MAX));
        }
    }

    pub(crate) fn read_save(state: &LivingWorldState, d: &mut Dec) -> Result<Self, LoadError> {
        struct Saved {
            seat: AiSeat,
            turn: u32,
            substep: u8,
            money: i32,
            plan: Plan,
            done: bool,
            retreated: Option<u32>,
        }
        let saved = d.list(|d| {
            Ok(Saved {
                seat: AiSeat {
                    player: PlayerId(d.u8()?),
                    template: d.opt(|d| d.str())?,
                    difficulty: d.u8()?,
                },
                turn: d.u32()?,
                substep: d.u8()?,
                money: d.i32()?,
                plan: get_plan(d)?,
                done: d.bool()?,
                retreated: d.opt(|d| d.u32())?,
            })
        })?;
        let snapshot = d.opt(|d| Ok((d.u32()?, Snapshot::read_save(state, d)?)))?;
        let skipped = Skipped {
            recruits: usize::try_from(d.u32()?).unwrap_or(usize::MAX),
            unaffordable_builds: usize::try_from(d.u32()?).unwrap_or(usize::MAX),
            illegal_moves: usize::try_from(d.u32()?).unwrap_or(usize::MAX),
            retreat_fallbacks: usize::try_from(d.u32()?).unwrap_or(usize::MAX),
        };
        let seats: Vec<AiSeat> = saved.iter().map(|s| s.seat.clone()).collect();
        let mut ai = Self::new(state, &seats, false);
        if ai.seats.len() != saved.len() {
            return Err(d.invalid("AI seat for a missing player"));
        }
        for (seat, s) in ai.seats.iter_mut().zip(saved) {
            seat.turn = s.turn;
            seat.substep = s.substep;
            seat.money = s.money;
            seat.plan = s.plan;
            seat.done = s.done;
            seat.retreated = s.retreated;
        }
        ai.snapshot = snapshot;
        ai.skipped = skipped;
        Ok(ai)
    }
}
