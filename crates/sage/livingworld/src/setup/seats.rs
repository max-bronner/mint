use mint_math::rng::Pcg32;
use mint_sage_ini::living_world::campaign::ScenarioDef;
use mint_sage_ini::living_world::players::PlayerTemplateDef;
use mint_sage_ini::living_world::LivingWorldData;

use super::regions::apply_historical_start;
use super::PlayerSetup;
use crate::rules::{Catalog, SetupError};

const MAX_DRAWS: u32 = 100_000;

fn hero_allows(_seat: &PlayerSetup, _template: &PlayerTemplateDef) -> bool {
    true
}

pub(super) fn resolve_random_seats(
    data: &LivingWorldData,
    catalog: &Catalog,
    scenario: &ScenarioDef,
    seats: &mut [PlayerSetup],
    rng: &mut Pcg32,
) -> Result<(), SetupError> {
    let historical = scenario.historical_scenario == Some(true);
    for (slot, seat) in seats.iter_mut().enumerate() {
        if !seat.template.is_empty() {
            continue;
        }
        if !scenario.disabled_factions.0.is_empty() {
            return Err(SetupError::RandomFaction(slot));
        }
        let count = u32::try_from(data.player_templates.len()).unwrap_or(u32::MAX);
        let mut drawn = None;
        for _ in 0..MAX_DRAWS {
            let index = usize::try_from(rng.below(count)).unwrap_or(usize::MAX);
            let Some(template) = data.player_templates.iter().nth(index) else {
                break;
            };
            let playable = template
                .faction
                .as_deref()
                .is_some_and(|f| catalog.playable_factions.contains(&f.to_ascii_lowercase()));
            if playable && hero_allows(seat, template) {
                drawn = Some(template);
                break;
            }
        }
        let template = drawn.ok_or(SetupError::NoPlayableFaction(slot))?;
        seat.template.clone_from(&template.name);
        if historical {
            apply_historical_start(data, scenario, seat);
        }
    }
    Ok(())
}
