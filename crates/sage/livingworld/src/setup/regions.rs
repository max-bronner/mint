use mint_sage_ini::living_world::campaign::{ScenarioDef, StartingRestrictionDef};
use mint_sage_ini::living_world::LivingWorldData;

use super::{listed, PlayerSetup, Setup};
use crate::rules::{Catalog, Rules, SetupError};

fn faction_of(data: &LivingWorldData, template: &str) -> String {
    data.player_templates
        .get(template)
        .and_then(|t| t.faction.clone())
        .unwrap_or_else(|| template.to_owned())
}

fn historical(scenario: &ScenarioDef) -> bool {
    scenario.historical_scenario == Some(true)
}

fn team_of(restriction: &StartingRestrictionDef) -> Option<i32> {
    restriction
        .teams
        .0
        .first()
        .and_then(|t| i32::try_from(*t).ok())
}

pub(super) fn apply_historical_start(
    data: &LivingWorldData,
    scenario: &ScenarioDef,
    seat: &mut PlayerSetup,
) {
    let faction = faction_of(data, &seat.template);
    let Some(restriction) = scenario
        .starting_restrictions
        .iter()
        .find(|r| listed(&r.factions.0, &faction))
    else {
        return;
    };
    if let Some(region) = restriction.regions.0.first() {
        seat.start_region = Some(region.clone());
    }
    if let Some(team) = team_of(restriction) {
        seat.team = team;
    }
}

fn apply_restricted_region(
    data: &LivingWorldData,
    catalog: &Catalog,
    scenario: &ScenarioDef,
    seat: &mut PlayerSetup,
    region: &str,
) {
    let Some(restriction) = scenario
        .starting_restrictions
        .iter()
        .find(|r| listed(&r.regions.0, region))
    else {
        return;
    };
    let faction = data.player_templates.iter().find(|t| {
        t.faction.as_deref().is_some_and(|f| {
            listed(&restriction.factions.0, f)
                && catalog.playable_factions.contains(&f.to_ascii_lowercase())
        })
    });
    if let Some(template) = faction {
        seat.template.clone_from(&template.name);
    }
    if let Some(team) = team_of(restriction) {
        seat.team = team;
    }
}

struct Context<'a> {
    data: &'a LivingWorldData,
    catalog: &'a Catalog,
    rules: Rules,
    scenario: &'a ScenarioDef,
}

fn context<'a>(
    data: &'a LivingWorldData,
    catalog: &'a Catalog,
    setup: &Setup,
) -> Result<Context<'a>, SetupError> {
    let rules = Rules::build(
        data,
        catalog,
        &setup.campaign,
        setup.scenario,
        setup.victory_type,
    )?;
    let scenario = data
        .campaigns
        .get(&setup.campaign)
        .ok_or_else(|| SetupError::UnknownCampaign(setup.campaign.clone()))?
        .scenarios
        .get(setup.scenario)
        .ok_or(SetupError::UnknownScenario(setup.scenario))?;
    Ok(Context {
        data,
        catalog,
        rules,
        scenario,
    })
}

impl Context<'_> {
    fn write_start(&self, seat: &mut PlayerSetup, region: &str) -> Result<(), SetupError> {
        let id = self.rules.region(region)?;
        let name = self
            .rules
            .regions
            .get(id.index())
            .map_or_else(|| region.to_owned(), |r| r.name.clone());
        if !historical(self.scenario) {
            apply_restricted_region(self.data, self.catalog, self.scenario, seat, &name);
        }
        seat.start_region = Some(name);
        Ok(())
    }
}

pub fn reset_start_regions(setup: &mut Setup) {
    for seat in &mut setup.players {
        seat.start_region = None;
    }
}

pub fn default_start_regions(
    data: &LivingWorldData,
    catalog: &Catalog,
    setup: &mut Setup,
) -> Result<(), SetupError> {
    let cx = context(data, catalog, setup)?;
    let spots = cx.scenario.default_start_spots.0.iter().filter(|name| {
        cx.rules.region(name).is_ok() && !listed(&cx.scenario.disallow_start_in_regions.0, name)
    });
    for (seat, name) in setup.players.iter_mut().zip(spots) {
        cx.write_start(seat, name)?;
    }
    Ok(())
}

pub fn assign_start_regions(
    data: &LivingWorldData,
    catalog: &Catalog,
    setup: &mut Setup,
) -> Result<(), SetupError> {
    reset_start_regions(setup);
    default_start_regions(data, catalog, setup)
}

pub fn set_start_region(
    data: &LivingWorldData,
    catalog: &Catalog,
    setup: &mut Setup,
    seat: usize,
    region: &str,
) -> Result<(), SetupError> {
    let cx = context(data, catalog, setup)?;
    let id = cx.rules.region(region)?;
    let held = setup.players.iter().enumerate().any(|(i, other)| {
        i != seat
            && other
                .start_region
                .as_deref()
                .and_then(|n| cx.rules.region(n).ok())
                == Some(id)
    });
    if held {
        return Err(SetupError::StartRegionShared(region.to_owned()));
    }
    let target = setup
        .players
        .get_mut(seat)
        .ok_or(SetupError::UnknownSeat(seat))?;
    cx.write_start(target, region)
}

pub fn clear_start_region(setup: &mut Setup, seat: usize) -> Result<(), SetupError> {
    setup
        .players
        .get_mut(seat)
        .ok_or(SetupError::UnknownSeat(seat))?
        .start_region = None;
    Ok(())
}

pub fn set_seat_faction(
    data: &LivingWorldData,
    catalog: &Catalog,
    setup: &mut Setup,
    seat: usize,
    template: &str,
) -> Result<(), SetupError> {
    let cx = context(data, catalog, setup)?;
    let target = setup
        .players
        .get_mut(seat)
        .ok_or(SetupError::UnknownSeat(seat))?;
    template.clone_into(&mut target.template);
    if historical(cx.scenario) {
        apply_historical_start(data, cx.scenario, target);
    }
    Ok(())
}
