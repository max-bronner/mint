use mint_sim::{Frame, PlayerId, Simulation, SIMULATION_HZ};

use crate::rules::{DebugCommand, LevelStep, RotwkCommand, RotwkRules};
use crate::templates::KindTable;

struct PendingLevelUp {
    frame: Frame,
    player: PlayerId,
    quarters: u8,
    before: Vec<LevelStep>,
}

const SPEEDS: [f64; 9] = [0.1, 0.25, 0.5, 2.0, 3.0, 5.0, 10.0, 20.0, 60.0];

fn speed_after(current: f64, digit: u8) -> f64 {
    let asked = usize::from(digit)
        .checked_sub(1)
        .and_then(|at| SPEEDS.get(at))
        .copied()
        .unwrap_or(1.0);
    if asked == current {
        1.0
    } else {
        asked
    }
}

struct GameSpeed {
    factor: f64,
    wall: f32,
    from: Frame,
}

impl Default for GameSpeed {
    fn default() -> Self {
        Self {
            factor: 1.0,
            wall: 0.0,
            from: Frame::ZERO,
        }
    }
}

#[derive(Default)]
pub(crate) struct TestControls {
    level_up: Option<PendingLevelUp>,
    speed: GameSpeed,
}

impl TestControls {
    pub(crate) fn set_speed(&mut self, digit: u8, frame: Frame) {
        let factor = speed_after(self.speed.factor, digit);
        self.speed = GameSpeed {
            factor,
            wall: 0.0,
            from: frame,
        };
        println!(
            "speed: {factor}x [Ctrl+Shift+{digit}; the same digit again or Ctrl+Shift+0 = 1x]"
        );
    }

    pub(crate) fn speed(&self) -> f64 {
        self.speed.factor
    }

    pub(crate) fn meter_speed(&mut self, dt: f32, frame: Frame) {
        let meter = &mut self.speed;
        if meter.factor == 1.0 {
            return;
        }
        meter.wall += dt.max(0.0);
        if meter.wall < 1.0 {
            return;
        }
        let ticks = frame.get().saturating_sub(meter.from.get());
        #[allow(clippy::cast_precision_loss)]
        let rate = ticks as f32 / meter.wall;
        #[allow(clippy::cast_precision_loss)]
        let achieved = rate / SIMULATION_HZ as f32;
        println!(
            "speed: {}x asked, {achieved:.1}x achieved ({rate:.0} ticks/s)",
            meter.factor
        );
        meter.wall = 0.0;
        meter.from = frame;
    }

    pub(crate) fn reset_powers(player: PlayerId) -> RotwkCommand {
        println!("powers: every power of player {} ready", player.get());
        RotwkCommand::Debug(DebugCommand::ResetPowers)
    }

    pub(crate) fn damage(percent: u8) -> RotwkCommand {
        println!("damage: the selection loses {percent}% of its maximum health");
        RotwkCommand::Debug(DebugCommand::Damage { percent })
    }

    pub(crate) fn hold_ring() -> RotwkCommand {
        println!("ring: the selection now holds the Ring");
        RotwkCommand::Debug(DebugCommand::SetStatus {
            status: String::from("HOLDING_THE_RING"),
        })
    }

    pub(crate) fn spell_points(
        simulation: &Simulation<RotwkRules>,
        player: PlayerId,
        points: u32,
    ) -> RotwkCommand {
        let current = simulation
            .rules()
            .purchase_points_of(simulation.state(), player);
        println!(
            "spell points: +{points} -> {}",
            current.saturating_add(points)
        );
        RotwkCommand::DebugSpellPoints { points }
    }

    pub(crate) fn toggle_dummies(simulation: &Simulation<RotwkRules>) -> Option<RotwkCommand> {
        let (units, invincible) = simulation.rules().dummy_invincibility(simulation.state());
        if units == 0 {
            println!("dummies: none alive");
            return None;
        }
        let on = !invincible;
        println!(
            "dummies: invincible {} [{units} units]",
            if on { "on" } else { "off" }
        );
        Some(RotwkCommand::Debug(DebugCommand::DummiesInvincible { on }))
    }

    pub(crate) fn level_up(
        &mut self,
        simulation: &Simulation<RotwkRules>,
        player: PlayerId,
        quarters: u8,
    ) -> Option<RotwkCommand> {
        let before = simulation.rules().level_steps(simulation.state(), player);
        if before.is_empty() {
            println!(
                "level-up: player {} has nothing selected with experience levels",
                player.get()
            );
            return None;
        }
        self.level_up = Some(PendingLevelUp {
            frame: simulation.frame(),
            player,
            quarters,
            before,
        });
        Some(RotwkCommand::Debug(DebugCommand::LevelUp { quarters }))
    }

    pub(crate) fn after_tick(&mut self, simulation: &Simulation<RotwkRules>, kinds: &KindTable) {
        if self
            .level_up
            .as_ref()
            .is_none_or(|pending| simulation.frame() <= pending.frame)
        {
            return;
        }
        let Some(pending) = self.level_up.take() else {
            return;
        };
        let Some(ids) = simulation.rules().ids() else {
            return;
        };
        let world = simulation.state();
        let after = simulation.rules().level_steps(world, pending.player);
        println!(
            "level-up: player {}, +{}/4 level, {} experience holder(s), frame {}",
            pending.player.get(),
            pending.quarters,
            pending.before.len(),
            pending.frame.get()
        );
        for old in &pending.before {
            let name = world
                .get(ids.kind, old.holder)
                .and_then(|kind| kinds.get(*kind))
                .map_or("?", |entry| entry.record.name.as_str());
            let label = format!("{name} (entity {})", old.holder.index());
            let Some((next_rank, required)) = old.next else {
                println!(
                    "level-up:   {label}: level {} is its last - unchanged",
                    old.rank
                );
                continue;
            };
            match after.iter().find(|new| new.holder == old.holder) {
                Some(new) => println!(
                    "level-up:   {label}: level {} -> {}, experience {} -> {} (level {next_rank} \
                     needs {required})",
                    old.rank, new.rank, old.points, new.points
                ),
                None => println!(
                    "level-up:   {label}: level {} -> ? - no longer selected or alive",
                    old.rank
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::speed_after;

    #[test]
    fn a_digit_sets_its_speed_and_the_same_digit_again_or_zero_returns_to_one() {
        assert_eq!(speed_after(1.0, 6), 5.0);
        assert_eq!(speed_after(5.0, 9), 60.0);
        assert_eq!(speed_after(60.0, 9), 1.0);
        assert_eq!(speed_after(60.0, 0), 1.0);
        assert_eq!(speed_after(1.0, 0), 1.0);
    }
}
