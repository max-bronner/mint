use mint_sage_scene::units_present::{TintEnvelope, TintSlot};
use mint_sim::Entity;

use crate::game::{report_second, MatchState};
use crate::render_state::UnitView;
use crate::report::SecondReport;

const EA_FRAME: f32 = 1.0 / 30.0;

const FLASH_INTERVAL: f32 = 15.0 * EA_FRAME;

#[derive(Debug, Default)]
pub(crate) struct SelectionFlash {
    saturation: f32,
    house_color: bool,
    selected: Vec<Entity>,
    pub(crate) started: u64,
    captures: Vec<(u32, [f32; 3])>,
    pub(crate) captured: u64,
    script_flashes: Vec<(u32, [f32; 3], u32, Option<f32>)>,
}

impl SelectionFlash {
    pub(crate) fn new(saturation: f32, house_color: bool) -> Self {
        Self {
            saturation,
            house_color,
            ..Self::default()
        }
    }

    pub(crate) fn envelope(&self, house: [f32; 3]) -> TintEnvelope {
        let base = if self.house_color { house } else { [1.0; 3] };
        let half = self.saturation * 0.5;
        TintEnvelope {
            color: base.map(|channel| channel * self.saturation - half),
            attack: EA_FRAME,
            sustain: EA_FRAME,
            decay: 4.0 * EA_FRAME,
        }
    }

    pub(crate) fn capture_envelope(&self, color: [f32; 3]) -> TintEnvelope {
        let half = self.saturation * 0.5;
        TintEnvelope {
            color: color.map(|channel| channel * self.saturation - half),
            attack: EA_FRAME,
            sustain: EA_FRAME,
            decay: 4.0 * EA_FRAME,
        }
    }

    pub(crate) fn queue_capture(&mut self, id: u32, color: [f32; 3]) {
        self.captures.push((id, color));
    }

    pub(crate) fn queue_script_flash(&mut self, id: u32, color: [f32; 3], count: u32) {
        self.script_flashes.retain(|flash| flash.0 != id);
        if count > 0 {
            self.script_flashes.push((id, color, count, None));
        }
    }

    pub(crate) fn take_captures(&mut self) -> Vec<(u32, [f32; 3])> {
        std::mem::take(&mut self.captures)
    }

    pub(crate) fn newly_selected(
        &mut self,
        selected: impl IntoIterator<Item = Entity>,
    ) -> Vec<Entity> {
        let mut now: Vec<Entity> = selected.into_iter().collect();
        now.sort_unstable_by_key(|entity| entity.index());
        let gained = now
            .iter()
            .filter(|entity| {
                self.selected
                    .binary_search_by_key(&entity.index(), |seen| seen.index())
                    .map_or(true, |at| self.selected.get(at) != Some(entity))
            })
            .copied()
            .collect();
        self.selected = now;
        gained
    }
}

const TINT_TRACE_LIMIT: usize = 4;

pub(crate) fn play_tints(
    state: &mut MatchState,
    units: &[UnitView],
    now: f32,
    report: &mut SecondReport,
) {
    let gained = state.selection_flash.newly_selected(
        units
            .iter()
            .filter(|unit| unit.selected)
            .map(|unit| unit.entity),
    );
    for entity in gained {
        let Some(unit) = units.iter().find(|unit| unit.entity == entity) else {
            continue;
        };
        let [r, g, b, _] = state.player_colors.of(unit.owner);
        let envelope = state.selection_flash.envelope([r, g, b]);
        let id = entity.index();
        if state
            .units
            .play_tint(id, TintSlot::Selection, envelope, now)
            > 0
        {
            let flash = &mut state.selection_flash;
            flash.started = flash.started.saturating_add(1);
            crate::diag::diag!(
                "tint: selection flash on drawable {id} at {now:.3}s, peak {:?}",
                envelope.color
            );
        }
    }
    let mut due = Vec::new();
    for flash in &mut state.selection_flash.script_flashes {
        if flash.3.is_none_or(|at| now >= at) {
            due.push((flash.0, flash.1));
            flash.2 = flash.2.saturating_sub(1);
            flash.3 = Some(now + FLASH_INTERVAL);
        }
    }
    state
        .selection_flash
        .script_flashes
        .retain(|flash| flash.2 > 0);
    state.selection_flash.captures.extend(due);
    for (id, color) in state.selection_flash.take_captures() {
        let envelope = state.selection_flash.capture_envelope(color);
        if state.units.play_tint(id, TintSlot::Effect, envelope, now) > 0 {
            let flash = &mut state.selection_flash;
            flash.captured = flash.captured.saturating_add(1);
            crate::diag::diag!(
                "tint: capture flash on drawable {id} at {now:.3}s, peak {:?}",
                envelope.color
            );
        }
    }
    for (id, tint) in state.level_ups.take_tints() {
        let envelope = TintEnvelope {
            color: tint.color,
            attack: tint.attack,
            sustain: tint.sustain,
            decay: tint.decay,
        };
        if state.units.play_tint(id, TintSlot::Effect, envelope, now) > 0 {
            let watch = &mut state.level_ups;
            watch.tinted = watch.tinted.saturating_add(1);
            crate::diag::diag!(
                "tint: level-up tint on drawable {id} at {now:.3}s, peak {:?}, {:.2}/{:.2}/{:.2}s",
                envelope.color,
                envelope.attack,
                envelope.sustain,
                envelope.decay
            );
        }
    }
    if crate::diag::wants("tint") {
        let second = report_second(now);
        for unit in units
            .iter()
            .filter(|unit| unit.selected)
            .take(TINT_TRACE_LIMIT)
        {
            let id = unit.entity.index();
            if let Some(add) = state
                .units
                .tint_at(id, now)
                .filter(|add| second || *add != [0.0; 3])
            {
                crate::diag::diag!("tint: drawable {id} light_add {add:.3?} at {now:.3}s");
            }
        }
    }
    if report_second(now)
        && (state.selection_flash.started > 0
            || state.selection_flash.captured > 0
            || state.level_ups.tinted > 0)
    {
        report.line(format!(
            "tint: {} selection flash(es), {} capture flash(es), {} level-up tint(s) started",
            state.selection_flash.started, state.selection_flash.captured, state.level_ups.tinted
        ));
    }
}
