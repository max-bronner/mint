use mint_sage_apt_player::Value;
use mint_sage_livingworld::{ArmyId, Command, LivingWorldState, Phase, PlayerId, RegionId};
use mint_sage_worldmap::WorldMap;

use crate::wotr_dev::Session;
use crate::wotr_movies::Kind;
use crate::wotr_screens::{Hud, Strategic};

fn region_name(session: &Session, region: RegionId) -> String {
    session
        .state
        .rules
        .region_rules(region)
        .map_or_else(|| format!("#{}", region.0), |r| r.name.clone())
}

fn local_battle(state: &LivingWorldState, local: PlayerId) -> Option<u32> {
    (state.phase == Phase::ResolveBattles)
        .then(|| state.battles.iter().find(|b| b.players.contains(&local)))
        .flatten()
        .map(|b| b.id)
}

fn attack(session: &mut Session, local: PlayerId, turns: u32) -> Result<String, String> {
    for _ in 0..turns {
        if let Some(battle) = local_battle(&session.state, local) {
            return Ok(format!("battle {battle} on turn {}", session.state.turn));
        }
        let state = &session.state;
        let order = state
            .armies_of(local)
            .find(|a| a.is_hero_army())
            .and_then(|army| {
                let neighbours = &state.rules.region_rules(army.region)?.neighbours;
                let dest = neighbours.iter().copied().find(|region| {
                    state
                        .armies
                        .values()
                        .any(|a| a.region == *region && a.player != local)
                })?;
                Some(Command::MoveArmy {
                    army: army.id,
                    dest,
                })
            });
        if let Some(order) = order.filter(|_| state.phase == Phase::Planning) {
            let _ = session.submit(local, order);
        }
        let _ = session.submit(local, Command::EndPhase);
    }
    local_battle(&session.state, local)
        .map(|battle| format!("battle {battle} on turn {}", session.state.turn))
        .ok_or_else(|| format!("no battle in {turns} turn(s)"))
}

fn standing(state: &LivingWorldState) -> String {
    let mut out = format!(
        "turn {} {:?} ended {:?} battles {}",
        state.turn,
        state.phase,
        state.ended,
        state.battles.len()
    );
    for player in &state.players {
        out.push_str(&format!(
            "\n  player {} team {} {:?} defeated {} regions {} armies {} resources {}",
            player.id.0,
            player.team,
            player.kind,
            player.defeated,
            state.regions_owned(player.id),
            state.armies_of(player.id).count(),
            player.resources
        ));
    }
    let strength = |region: RegionId| -> (usize, usize) {
        let there = state.armies.values().filter(|a| a.region == region);
        (
            there.clone().count(),
            there
                .map(|a| a.roster.iter().map(|m| m.count as usize).sum::<usize>())
                .sum(),
        )
    };
    for player in state
        .players
        .iter()
        .filter(|p| p.kind != mint_sage_livingworld::PlayerKind::Computer)
    {
        for index in 0..state.regions.len() {
            let Ok(raw) = u16::try_from(index) else { break };
            let region = RegionId(raw);
            if state.owner(region) != Some(player.id) {
                continue;
            }
            let (armies, members) = strength(region);
            out.push_str(&format!(
                "\n  held region {} armies {armies} members {members}",
                state
                    .rules
                    .region_rules(region)
                    .map_or("?", |r| r.name.as_str())
            ));
            for n in state
                .rules
                .region_rules(region)
                .map(|r| r.neighbours.clone())
                .unwrap_or_default()
            {
                let (a, m) = strength(n);
                let ordered = state
                    .armies
                    .values()
                    .filter(|x| x.order == Some(region))
                    .count();
                out.push_str(&format!(
                    "\n    neighbour {} owner {:?} armies {a} members {m} ordered-here {ordered}",
                    state.rules.region_rules(n).map_or("?", |r| r.name.as_str()),
                    state.owner(n).map(|o| o.0)
                ));
            }
        }
    }
    out
}

fn status(session: &Session, hud: &Hud) -> String {
    let state = &session.state;
    let mut out = format!(
        "turn {} {:?} hash 0x{:016x} local player {}",
        state.turn,
        state.phase,
        state.state_hash(),
        hud.local.0
    );
    for army in state.armies_of(hud.local) {
        let neighbours: Vec<String> = state
            .rules
            .region_rules(army.region)
            .map(|r| r.neighbours.clone())
            .unwrap_or_default()
            .into_iter()
            .map(|n| {
                let owner = state.owner(n).map_or(-1, |o| i32::from(o.0));
                format!("{}:{}(owner {owner})", n.0, region_name(session, n))
            })
            .collect();
        let roster: Vec<String> = army
            .roster
            .iter()
            .map(|m| {
                format!(
                    "{}x{}{}",
                    m.count,
                    state
                        .rules
                        .unit_names
                        .get(usize::from(m.unit.0))
                        .map_or("?", String::as_str),
                    if m.is_upgraded() { "+" } else { "" }
                )
            })
            .collect();
        out.push_str(&format!(
            "\n  army {} at {}:{} order {:?} members {:?} {roster:?}\n    neighbours {neighbours:?}",
            army.id.0,
            army.region.0,
            region_name(session, army.region),
            army.order.map(|r| r.0),
            army.roster.iter().map(|m| m.id).collect::<Vec<_>>(),
        ));
    }
    if let Some(region) = hud.selection.region {
        let plots: Vec<String> = state
            .region(region)
            .map(|r| r.plots.clone())
            .unwrap_or_default()
            .iter()
            .enumerate()
            .map(|(i, plot)| {
                let name = plot
                    .as_ref()
                    .and_then(|p| state.rules.building(p.building))
                    .map_or("empty", |b| b.name.as_str());
                format!("{i}:{name}")
            })
            .collect();
        out.push_str(&format!(
            "
  picked region {} plots {plots:?}",
            region.0
        ));
    }
    let available: Vec<String> = state
        .rules
        .buildings
        .iter()
        .take(8)
        .map(|b| format!("{}({})", b.name, b.cost))
        .collect();
    out.push_str(&format!(
        "
  buildings {available:?}"
    ));
    for battle in &state.battles {
        out.push_str(&format!(
            "\n  battle {} at {} players {:?} choices {:?}",
            battle.id,
            region_name(session, battle.region),
            battle.players.iter().map(|p| p.0).collect::<Vec<_>>(),
            battle
                .choices
                .iter()
                .map(|(p, m)| (p.0, *m))
                .collect::<Vec<_>>()
        ));
    }
    for decision in state.decisions.values() {
        out.push_str(&format!(
            "\n  decision at {} players {:?} resolved {}",
            region_name(session, decision.region),
            decision.players.iter().map(|p| p.0).collect::<Vec<_>>(),
            decision.resolved
        ));
    }
    if let Some(battle) = state.awaiting {
        out.push_str(&format!("\n  awaiting real-time result of battle {battle}"));
    }
    out.push_str(&format!(
        "\n  shown: {:?}",
        Kind::ALL
            .into_iter()
            .filter(|k| hud.movies.is_up(*k))
            .map(Kind::movie)
            .collect::<Vec<_>>()
    ));
    out
}

fn number(text: &str) -> Result<u32, String> {
    text.trim()
        .parse()
        .map_err(|_| format!("wants a number, got {text:?}"))
}

pub(crate) fn step(
    session: &mut Session,
    hud: Option<&mut Hud>,
    map: &mut WorldMap,
    viewport: (u32, u32),
    verb: &str,
    argument: &str,
) -> Result<String, String> {
    if verb != "do" {
        return Err(format!("{verb}: not a step of a War of the Ring session"));
    }
    let hud = hud.ok_or("this session has no human seat: start it with --hud")?;
    let (word, rest) = argument.split_once(' ').unwrap_or((argument, ""));
    match word {
        "status" => Ok(status(session, hud)),
        "army" => {
            let id = ArmyId(number(rest)?);
            if !session.state.armies.contains_key(&id) {
                return Err(format!("no army {}", id.0));
            }
            let changed = hud.selection.pick_army_on_map(Some(id));
            if changed {
                hud.selection.pick_every_member(&session.state);
            }
            Ok(format!(
                "army {} picked on the map{}",
                id.0,
                if changed { "" } else { " (already selected)" }
            ))
        }
        "icon" => {
            let id = ArmyId(number(rest)?);
            map.icon_pixel(viewport, id)
                .map(|(x, y)| format!("army {} icon at {x:.1},{y:.1}", id.0))
                .ok_or_else(|| format!("army {} has no icon drawn", id.0))
        }
        "region" => {
            let id = RegionId(u16::try_from(number(rest)?).map_err(|_| "no such region")?);
            if session.state.region(id).is_none() {
                return Err(format!("no region {}", id.0));
            }
            map.select(Some(id));
            hud.region_clicked(id);
            Ok(format!("region {} clicked", id.0))
        }
        "order" => {
            let id = RegionId(u16::try_from(number(rest)?).map_err(|_| "no such region")?);
            if session.state.region(id).is_none() {
                return Err(format!("no region {}", id.0));
            }
            hud.region_ordered(session, id);
            Ok(format!("region {} right-clicked", id.0))
        }
        "order-army" => {
            let id = ArmyId(number(rest)?);
            let taken = hud.army_ordered(session, id);
            Ok(format!(
                "army {} right-clicked{}",
                id.0,
                if taken { "" } else { " - not a merge" }
            ))
        }
        "plot" => {
            hud.selection.pick_plot(Some(
                u8::try_from(number(rest)?).map_err(|_| "no such plot")?,
            ));
            Ok("plot picked".to_owned())
        }
        "members" => {
            hud.selection.members = rest.split(',').map(number).collect::<Result<_, _>>()?;
            Ok(format!("members {:?} picked", hud.selection.members))
        }
        "control" => {
            let (control, arg) = rest.split_once(' ').unwrap_or((rest, ""));
            let before = hud.lines.len();
            hud.control(session, control, arg)?;
            Ok(hud.lines.get(before..).unwrap_or_default().join(" | "))
        }
        "press" => {
            let (movie, path) = rest.split_once(' ').ok_or("press MOVIE CLIP.PATH")?;
            let kind = Kind::lookup(movie).ok_or_else(|| format!("no movie {movie:?}"))?;
            if !hud.movies.is_up(kind) {
                return Err(format!("{} is not up", kind.movie()));
            }
            hud.movies.press(kind, path.trim())
        }
        "tree" => {
            let (movie, needle) = rest.split_once(' ').unwrap_or((rest, ""));
            let kind = Kind::lookup(movie).ok_or_else(|| format!("no movie {movie:?}"))?;
            if !hud.movies.is_up(kind) {
                return Err(format!("{} is not up", kind.movie()));
            }
            Ok(hud.movies.tree(kind, needle.trim()).join("\n  "))
        }
        "texts" => {
            let (movie, needle) = rest.split_once(' ').unwrap_or((rest, ""));
            let kind = Kind::lookup(movie).ok_or_else(|| format!("no movie {movie:?}"))?;
            if !hud.movies.is_up(kind) {
                return Err(format!("{} is not up", kind.movie()));
            }
            Ok(hud.movies.texts(kind, needle.trim()).join("\n  "))
        }
        "call" => {
            let mut parts = rest.split_whitespace();
            let (Some(movie), Some(path), Some(function)) =
                (parts.next(), parts.next(), parts.next())
            else {
                return Err("call MOVIE PATH FUNCTION [ARGS] (PATH - is the root)".to_owned());
            };
            let kind = Kind::lookup(movie).ok_or_else(|| format!("no movie {movie:?}"))?;
            let args: Vec<Value> = parts
                .map(|a| {
                    a.parse::<f64>()
                        .map_or_else(|_| Value::str(a), Value::Number)
                })
                .collect();
            let path = if path == "-" { "" } else { path };
            hud.movies.call(kind, path, function, args)?;
            Ok(format!("{function} called"))
        }
        "tutorial-auto" => {
            session.scripted = rest.trim() != "off";
            Ok(format!(
                "scripted player {}",
                if session.scripted { "on" } else { "off" }
            ))
        }
        "panels" => Ok(hud.panels_report()),
        "events" => Ok(format!(
            "{} territory event(s) playing or queued; message pop-up up: {}",
            map.events_pending(),
            hud.message_pending()
        )),
        "territory-event" => {
            let territory = u16::try_from(number(rest)?).map_err(|_| "no such territory")?;
            map.territory_unified(hud.local, territory);
            Ok(format!(
                "territory {territory} unified for player {} (test-only: the event a bonus refresh raises); {} event(s) queued",
                hud.local.0,
                map.events_pending()
            ))
        }
        "dialog" => Ok(hud.dialog_report()),
        "frames" => Ok(hud.frames_report(&session.state)),
        "names" => Ok(hud.names_report(&session.state)),
        "global" => {
            let up: Vec<Kind> = Kind::ALL
                .into_iter()
                .filter(|k| hud.movies.is_up(*k))
                .collect();
            let lines: Vec<String> = up
                .into_iter()
                .map(|k| {
                    let value = hud.movies.global(k, rest);
                    format!("{}: _global.{rest} = {value:?}", k.movie())
                })
                .collect();
            Ok(lines.join("\n  "))
        }
        "attack" => attack(session, hud.local, number(rest)?),
        "menu" => {
            hud.movies.menus.set_save_enabled(session.saveable());
            hud.movies.menus.toggle();
            Ok(format!("open {}", hud.movies.menus.is_open()))
        }
        "menu-call" => {
            hud.movies.menus.quit_call(rest.trim());
            Ok(rest.trim().to_owned())
        }
        "menu-save" => hud.movies.menus.commit_save().map(|()| "save".to_owned()),
        "veterancy" => Ok(hud.veterancy_report()),
        "veterancy-mark" => {
            let row = rest.trim().parse().map_err(|_| "a row number")?;
            hud.veterancy_mark(row, true);
            Ok(format!("row {row} marked"))
        }
        "veterancy-continue" => hud
            .movies
            .call(
                Kind::Veterancy,
                "",
                "HandleCloseVeterancyScreen",
                Vec::new(),
            )
            .map(|_| "continue".to_owned()),
        "close-dialog" => {
            hud.close_dialog();
            Ok("dialog closed".to_owned())
        }
        "phase" => Ok(format!("{:?}", session.state().phase)),
        "standing" => Ok(standing(&session.state)),
        other => Err(format!("no drive word {other:?}")),
    }
}
