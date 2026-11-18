use super::*;
use crate::rules::DebugTarget;

const SCRIPT_VIEW_DISTANCE: f32 = 330.0;

fn world_point(text: &str) -> Result<(f32, f32), String> {
    let bad = || format!("{text:?} is not X,Y");
    let (x, y) = text.trim().split_once(',').ok_or_else(bad)?;
    let number = |s: &str| {
        s.trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(bad)
    };
    Ok((number(x)?, number(y)?))
}

impl Game {
    fn place_for_script(&self, text: &str) -> Result<(f32, f32), String> {
        let text = text.trim();
        if text == "here" {
            let focus = self.state.camera.focus();
            return Ok((focus.x, focus.y));
        }
        let Some(name) = text.strip_prefix("kind ") else {
            return world_point(text);
        };
        let name = name.trim();
        let first = self
            .objects_of_kind(name, None)?
            .first()
            .copied()
            .ok_or_else(|| format!("no live {name:?}"))?;
        let ids = self.state.simulation.rules().ids().ok_or("no match")?;
        let body = self
            .state
            .simulation
            .state()
            .get(ids.body, first)
            .ok_or_else(|| format!("{name:?} has no body"))?;
        println!(
            "drive: kind {name} entity {} at ({:.0}, {:.0})",
            first.index(),
            body.0.position.x,
            body.0.position.y
        );
        Ok((body.0.position.x, body.0.position.y))
    }

    fn stance_report_for_script(&self) -> String {
        let rules = self.state.simulation.rules();
        let Some(ids) = rules.ids() else {
            return String::from("no match");
        };
        let lines = rules.stance_report(self.state.simulation.state(), ids, self.acting_player);
        if lines.is_empty() {
            return String::from("drive: no selected unit has a stance");
        }
        for line in &lines {
            println!("drive: stance {line}");
        }
        lines.join("; ")
    }

    pub(crate) fn select_for_script(&mut self, which: &str) -> Result<String, String> {
        self.ensure_snapshot();
        let units = match which {
            "porter" => porters_of(&self.state.simulation, LOCAL_PLAYER),
            "producer" => producing_structures(&self.state.simulation, LOCAL_PLAYER),
            "army" => hordes_of(&self.state.simulation, LOCAL_PLAYER),
            "all" => self.mobile_units_for_script(),
            "none" => Vec::new(),
            other if other.starts_with("named ") => {
                vec![self.object_named(other.trim_start_matches("named ").trim())?]
            }
            other if other.starts_with("kind ") => {
                let name = other.trim_start_matches("kind ").trim();
                self.objects_of_kind(name, Some(LOCAL_PLAYER))?
            }
            other if other.starts_with("near ") => {
                let (name, at) = other
                    .trim_start_matches("near ")
                    .rsplit_once(" at ")
                    .ok_or_else(|| format!("select: {other:?} is not near NAME at X,Y"))?;
                let point = at
                    .split_once(',')
                    .and_then(|(x, y)| {
                        Some((x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?))
                    })
                    .ok_or_else(|| format!("select: {at:?} is not X,Y"))?;
                self.objects_near(name.trim(), point)?
            }
            other if other.starts_with("one ") => {
                let name = other.trim_start_matches("one ").trim();
                let mut all = self.objects_of_kind(name, Some(LOCAL_PLAYER))?;
                all.truncate(1);
                all
            }
            other if other.starts_with("any ") => {
                let name = other.trim_start_matches("any ").trim();
                let mut all = self.objects_of_kind(name, None)?;
                all.truncate(1);
                all
            }
            other => {
                let range = other
                    .strip_prefix("hordes ")
                    .and_then(|r| r.split_once(".."))
                    .and_then(|(a, b)| Some((a.trim().parse().ok()?, b.trim().parse().ok()?)));
                let Some((from, to)) = range else {
                    return Err(format!(
                        "select: {other:?} is not porter, producer, army, all, hordes A..B, kind NAME, \
                         near NAME at X,Y, one NAME, any NAME, named NAME or none"
                    ));
                };
                self.horde_members_for_script(from, to)
            }
        };
        let count = units.len();
        if let Some(first) = units.first().copied() {
            if let Some(unit) = self.units().iter().find(|u| u.entity == first) {
                let (x, y) = (unit.position.x, unit.position.y);
                let z = ground_at(&self.state.scene, x, y);
                self.state.camera.set_focus(Vec3::new(x, y, z));
            }
        }
        let first = units
            .first()
            .map_or(String::new(), |e| format!(", first entity {}", e.index()));
        self.submit(RotwkCommand::SetSelection {
            replace: true,
            units,
        });
        Ok(format!("{count} unit(s){first}"))
    }

    fn mobile_units_for_script(&self) -> Vec<mint_sim::Entity> {
        use mint_sage_asset::KindOf;
        let Some(ids) = self.state.simulation.rules().ids() else {
            return Vec::new();
        };
        let world = self.state.simulation.state();
        let skip = [KindOf::STRUCTURE, KindOf::IMMOBILE, KindOf::DOZER];
        world
            .store(ids.shape)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, shape)| {
                        !skip.iter().any(|&bit| shape.kind_of.contains(bit))
                            && !world.contains(ids.doomed, *entity)
                            && world
                                .get(ids.owner, *entity)
                                .is_some_and(|owner| owner.0 == LOCAL_PLAYER)
                    })
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn object_named(&self, name: &str) -> Result<mint_sim::Entity, String> {
        let ids = self.state.simulation.rules().ids().ok_or("no match")?;
        let world = self.state.simulation.state();
        world
            .store(ids.script_name)
            .and_then(|store| {
                store
                    .iter()
                    .find(|(entity, tag)| {
                        tag.0 == name
                            && world.is_live(*entity)
                            && !world.contains(ids.doomed, *entity)
                    })
                    .map(|(entity, _)| entity)
            })
            .ok_or_else(|| format!("no live object named {name:?}"))
    }

    fn script_target(&self, argument: &str) -> Result<mint_sim::Entity, String> {
        match argument.strip_prefix("named ") {
            Some(name) => {
                let named = self.object_named(name.trim())?;
                let ids = self.state.simulation.rules().ids().ok_or("no match")?;
                let world = self.state.simulation.state();
                Ok(world
                    .get(ids.horde, named)
                    .and_then(|horde| {
                        horde
                            .members
                            .iter()
                            .copied()
                            .find(|&m| world.is_live(m) && !world.contains(ids.doomed, m))
                    })
                    .unwrap_or(named))
            }
            None => self
                .objects_of_kind(argument, None)?
                .first()
                .copied()
                .ok_or_else(|| format!("no live {argument}")),
        }
    }

    fn debug_target(&self, text: &str) -> Result<DebugTarget, String> {
        let text = text.trim();
        if text == "all" {
            return Ok(DebugTarget::All);
        }
        if let Some(name) = text.strip_prefix("named ") {
            return Ok(DebugTarget::Named(name.trim().to_owned()));
        }
        if let Some(name) = text.strip_prefix("team ") {
            return Ok(DebugTarget::Team(name.trim().to_owned()));
        }
        if let Some(side) = text.strip_prefix("player ") {
            return Ok(DebugTarget::Player(side.trim().to_owned()));
        }
        if let Some(rest) = text.strip_prefix("kind ") {
            let (name, side) = match rest.rsplit_once(" of ") {
                Some((name, side)) => (name.trim(), Some(side.trim().to_owned())),
                None => (rest.trim(), None),
            };
            let kind = self
                .state
                .kinds
                .kind(name)
                .ok_or_else(|| format!("{name:?} names no template"))?;
            return Ok(DebugTarget::Kind { kind: kind.0, side });
        }
        if let Some(rest) = text.strip_prefix("near ") {
            let (name, at) = rest
                .rsplit_once(" at ")
                .ok_or_else(|| format!("{text:?} is not near TEMPLATE at X,Y"))?;
            let kind = self
                .state
                .kinds
                .kind(name.trim())
                .ok_or_else(|| format!("{name:?} names no template"))?;
            let (x, y) = world_point(at)?;
            return Ok(DebugTarget::Near { kind: kind.0, x, y });
        }
        Err(format!(
            "{text:?} is not named NAME, team NAME, near TEMPLATE at X,Y, kind TEMPLATE [of SIDE], \
             player SIDE or all"
        ))
    }

    fn objects_near(&self, name: &str, point: (f32, f32)) -> Result<Vec<mint_sim::Entity>, String> {
        let mut found = self.objects_of_kind(name, Some(LOCAL_PLAYER))?;
        let Some(ids) = self.state.simulation.rules().ids() else {
            return Ok(found);
        };
        let world = self.state.simulation.state();
        let distance = |entity: &mint_sim::Entity| {
            world.get(ids.body, *entity).map_or(f32::MAX, |body| {
                let (dx, dy) = (body.0.position.x - point.0, body.0.position.y - point.1);
                dx * dx + dy * dy
            })
        };
        found.sort_by(|a, b| distance(a).total_cmp(&distance(b)).then(a.cmp(b)));
        Ok(found)
    }

    fn objects_of_kind(
        &self,
        name: &str,
        owner: Option<PlayerId>,
    ) -> Result<Vec<mint_sim::Entity>, String> {
        let kind = self
            .state
            .kinds
            .kind(name)
            .ok_or_else(|| format!("{name:?} names no kind"))?;
        let Some(ids) = self.state.simulation.rules().ids() else {
            return Ok(Vec::new());
        };
        let world = self.state.simulation.state();
        Ok(world
            .store(ids.kind)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, found)| {
                        **found == kind
                            && !world.contains(ids.doomed, *entity)
                            && owner.is_none_or(|owner| {
                                world.get(ids.owner, *entity).is_some_and(|o| o.0 == owner)
                            })
                    })
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default())
    }

    fn horde_members_for_script(&self, from: usize, to: usize) -> Vec<mint_sim::Entity> {
        let Some(ids) = self.state.simulation.rules().ids() else {
            return Vec::new();
        };
        let world = self.state.simulation.state();
        let owned = hordes_of(&self.state.simulation, LOCAL_PLAYER);
        let hordes = crate::horde::hordes_of(world, RotwkRules::horde_ids(ids), &owned);
        hordes
            .iter()
            .skip(from)
            .take(to.saturating_sub(from))
            .filter_map(|h| world.get(ids.horde, *h))
            .flat_map(|h| h.members.iter().copied())
            .collect()
    }

    fn command_bar_report_for_script(&self) -> String {
        let units = self.units();
        let hud = crate::hud::read_hud(&self.state, &self.data, &units, self.viewport);
        let rules = self.state.simulation.rules();
        let purse = rules.ids().map(RotwkRules::economy_ids).map(|e| {
            crate::economy::purse_of(self.state.simulation.state(), e, LOCAL_PLAYER).amount()
        });
        let mut lines: Vec<String> = vec![format!("purse {purse:?}")];
        for (position, slot) in hud.command_bar.ring.iter().enumerate() {
            if let Some(slot) = slot {
                lines.push(format!(
                    "ring {position}: {} ({}) cost {:?} progress {:?} count {} auto_cast {} \
                     enabled {} availability {:?} frame {} image {:?}",
                    slot.command_button,
                    slot.label,
                    slot.cost,
                    slot.progress,
                    slot.count,
                    slot.auto_cast,
                    slot.enabled,
                    slot.availability,
                    crate::hud::button_state(slot, false),
                    slot.image
                ));
            }
        }
        for slot in &hud.command_bar.bar {
            lines.push(format!(
                "bar: {} ({}) cost {:?} progress {:?} count {} enabled {} availability {:?} \
                 frame {} image {:?}",
                slot.command_button,
                slot.label,
                slot.cost,
                slot.progress,
                slot.count,
                slot.enabled,
                slot.availability,
                crate::hud::button_state(slot, false),
                slot.image
            ));
        }
        for (position, slot) in &hud.command_bar.radial {
            lines.push(format!(
                "radial {position}: {} ({}) cost {:?} progress {:?} count {} enabled {} \
                 availability {:?} frame {} image {:?}",
                slot.command_button,
                slot.label,
                slot.cost,
                slot.progress,
                slot.count,
                slot.enabled,
                slot.availability,
                crate::hud::button_state(slot, false),
                slot.image
            ));
        }
        lines.join("; ")
    }

    fn spell_bar_report_for_script(&self) -> String {
        let units = self.units();
        let hud = crate::hud::read_hud(&self.state, &self.data, &units, self.viewport);
        if hud.spell_bar.is_empty() {
            return "no spell-bar slot held".to_owned();
        }
        let lines: Vec<String> = hud
            .spell_bar
            .iter()
            .enumerate()
            .map(|(row, slot)| {
                let (state, percent) = match slot.readiness {
                    crate::rules::SpellBarReadiness::Ready => ("_up", None),
                    crate::rules::SpellBarReadiness::OnCooldown { percent } => {
                        ("_notReady", Some(percent))
                    }
                    crate::rules::SpellBarReadiness::Static => ("_static", None),
                    crate::rules::SpellBarReadiness::Disabled => ("_disabled", None),
                };
                format!(
                    "row {} (book button {}): {} state {state} percent {percent:?}",
                    row + 1,
                    slot.button,
                    slot.command_button
                )
            })
            .collect();
        lines.join("; ")
    }

    fn rank_ui_report_for_script(&self) -> String {
        let units = self.units();
        let hud = crate::hud::read_hud(&self.state, &self.data, &units, self.viewport);
        let frame = self.state.simulation.frame().get();
        let text = hud.level_text.as_deref().unwrap_or_default();
        let bar = |fill: Option<f32>| {
            fill.map_or_else(
                || "no bar".to_owned(),
                |fill| format!("bar {fill:.3} frame {}", crate::hud::rank_bar_frame(fill)),
            )
        };
        match hud.rank_ui {
            None => format!("rank ui: hidden at frame {frame}"),
            Some(shown @ crate::experience::RankDisplay::Level(level)) => format!(
                "rank ui: level {} {} text {text:?} at frame {frame}",
                level.rank,
                bar(shown.bar())
            ),
            Some(shown @ crate::experience::RankDisplay::TimeRemaining(_)) => format!(
                "rank ui: time remaining {} text {text:?} at frame {frame}",
                bar(shown.bar())
            ),
        }
    }

    fn map_owner_report_for_script(&self) -> String {
        let rules = self.state.simulation.rules();
        if rules.ids().is_none() {
            return String::from("no match");
        }
        let counts = self.object_counts(true);
        let rows: Vec<String> = counts
            .iter()
            .map(|((owner, name), count)| {
                let player = rules
                    .map_player_name(PlayerId::new(*owner))
                    .filter(|name| !name.is_empty())
                    .unwrap_or("(neutral side)");
                format!("player {owner} {player}: {name} x{count}")
            })
            .collect();
        format!(
            "{} map-owned entit(ies): {}",
            counts.values().sum::<usize>(),
            rows.join("; ")
        )
    }

    fn shroud_report_for_script(&self) -> String {
        let rules = self.state.simulation.rules();
        let Some(ids) = rules.ids() else {
            return String::from("no match");
        };
        let world = self.state.simulation.state();
        let shroud_ids = RotwkRules::shroud_ids(ids);
        let viewer = self.state.shroud_viewer;
        let mut rows = Vec::new();
        if let Some(store) = world.store(ids.shape) {
            for (entity, _) in store.iter() {
                let owner = world.get(ids.owner, entity).map(|owner| owner.0);
                let name = world.get(ids.kind, entity).map_or_else(
                    || String::from("(no kind)"),
                    |kind| {
                        self.state
                            .kinds
                            .get(*kind)
                            .map_or_else(|| format!("kind {}", kind.0), |e| e.record.name.clone())
                    },
                );
                let visibility = world
                    .get(shroud_ids.seen, entity)
                    .map_or(mint_sage_shroud::ObjectVisibility::Shrouded, |seen| {
                        seen.visibility(viewer)
                    });
                let owner_text =
                    owner.map_or_else(|| String::from("none"), |o| o.get().to_string());
                rows.push(format!(
                    "entity {} {name} (owner {owner_text}): {visibility:?}",
                    entity.index()
                ));
            }
        }
        format!("shroud for player {}: {}", viewer.get(), rows.join("; "))
    }

    fn obstacle_cells_for_script(&self) -> String {
        use mint_movement::Passability as _;
        let passability = self.state.simulation.rules().passability();
        let extent = passability.grid.extent();
        let mut count = 0usize;
        for y in 0..extent.height {
            for x in 0..extent.width {
                let cell = mint_movement::Cell::new(x, y);
                if !passability
                    .grid
                    .flags(cell)
                    .intersects(mint_sage_passability::OBSTACLE)
                {
                    continue;
                }
                let centre = passability.geometry.centre_of(cell);
                println!("obst {x} {y} {:.1} {:.1}", centre.x, centre.y);
                count += 1;
            }
        }
        format!("{count} obstacle cell(s)")
    }

    fn soldier_positions_for_script(&self) -> String {
        let Some(ids) = self.state.simulation.rules().ids() else {
            return String::from("no match");
        };
        let world = self.state.simulation.state();
        let Some(store) = world.store(ids.member) else {
            return String::from("0 soldiers");
        };
        let mut count = 0usize;
        for (entity, member) in store.iter() {
            if world
                .get(ids.health, entity)
                .is_some_and(|health| health.0.is_dead())
            {
                continue;
            }
            if let Some(body) = world.get(ids.body, entity) {
                let state = if world.contains(ids.member_attack, entity) {
                    'a'
                } else if world.contains(ids.path, entity) {
                    'p'
                } else if world
                    .get(ids.member_melee, entity)
                    .is_some_and(|record| record.goal.minus(body.0.position).length() > 1.0)
                {
                    's'
                } else if world.contains(ids.member_melee, entity) {
                    'm'
                } else {
                    'o'
                };
                let horde_state = if world.contains(ids.horde_melee, member.horde) {
                    'm'
                } else if world.contains(ids.horde_target, member.horde) {
                    't'
                } else if world.contains(ids.path, member.horde) {
                    'p'
                } else {
                    'i'
                };
                let owner = world
                    .get(ids.owner, entity)
                    .map_or(0, |owner| owner.0.get());
                let attack = world.get(ids.member_attack, entity).map_or_else(
                    || String::from("-"),
                    |attack| {
                        attack.spot.map_or_else(
                            || format!("{}:-", attack.victim.index()),
                            |spot| {
                                format!(
                                    "{}:{:.0}:{}",
                                    attack.victim.index(),
                                    spot.minus(body.0.position).length(),
                                    world.get(ids.path, entity).map_or("-", |path| {
                                        match path.kind {
                                            crate::unit::PathKind::Formation => "f",
                                            _ => "r",
                                        }
                                    })
                                )
                            },
                        )
                    },
                );
                let aim =
                    world
                        .get(ids.horde_target, member.horde)
                        .map_or(String::from("-"), |held| {
                            held.target
                                .map_or(String::from("none"), |target| target.index().to_string())
                        });
                println!(
                    "soldier {} {:.1} {:.1} {} {state} {horde_state} {owner} {attack} {aim} {:.2} {:.1}",
                    entity.index(),
                    body.0.position.x,
                    body.0.position.y,
                    member.horde.index(),
                    body.0.facing.radians(),
                    body.0.speed
                );
                count += 1;
            }
        }
        if let Some(hordes) = world.store(ids.horde) {
            for (horde, _) in hordes.iter() {
                if let Some(body) = world.get(ids.body, horde) {
                    let phase = world
                        .get(ids.horde_melee, horde)
                        .map_or(String::from("-"), |melee| format!("{:?}", melee.phase));
                    println!(
                        "hordeobj {} {:.1} {:.1} {:.3} {} {}",
                        horde.index(),
                        body.0.position.x,
                        body.0.position.y,
                        body.0.facing.radians(),
                        if world.contains(ids.path, horde) {
                            "p"
                        } else {
                            "-"
                        },
                        phase.split_whitespace().next().unwrap_or("-")
                    );
                }
            }
        }
        format!("{count} soldiers")
    }

    fn footprint_breach_for_script(&self) -> String {
        let Some(ids) = self.state.simulation.rules().ids() else {
            return String::from("no match");
        };
        let world = self.state.simulation.state();
        let breaching = self
            .state
            .simulation
            .rules()
            .members_inside_a_structure_footprint(world, ids);
        if breaching.is_empty() {
            return String::from("0 member(s) inside a structure footprint");
        }
        for entity in &breaching {
            println!("breach {}", entity.index());
        }
        format!("{} member(s) inside a structure footprint", breaching.len())
    }

    fn entity_report_for_script(&self, argument: &str) -> Result<String, String> {
        let ids = self.state.simulation.rules().ids().ok_or("no match")?;
        let world = self.state.simulation.state();
        let entity = match argument.parse::<u32>() {
            Ok(index) => world
                .store(ids.body)
                .and_then(|store| store.iter().find(|(e, _)| e.index() == index))
                .map(|(e, _)| e)
                .ok_or_else(|| format!("do entity: no entity {index} with a body"))?,
            Err(_) => self.script_target(argument)?,
        };
        let name = world
            .get(ids.kind, entity)
            .and_then(|kind| self.state.kinds.get(*kind))
            .map_or("-", |entry| entry.record.name.as_str());
        let owner = world.get(ids.owner, entity).map(|owner| owner.0);
        let tag = world
            .get(ids.script_name, entity)
            .map_or("-", |tag| tag.0.as_str());
        let horde = world
            .get(ids.member, entity)
            .map_or(String::from("-"), |member| member.horde.index().to_string());
        let position = world
            .get(ids.body, entity)
            .map_or((f32::NAN, f32::NAN), |body| {
                (body.0.position.x, body.0.position.y)
            });
        let goal = world
            .get(ids.path, entity)
            .map_or(String::from("-"), |path| {
                format!("{:.0},{:.0}", path.goal.x, path.goal.y)
            });
        let target = world
            .get(ids.armed, entity)
            .and_then(|armed| armed.target)
            .map_or(String::from("-"), |target| target.index().to_string());
        let health = world
            .get(ids.health, entity)
            .map_or(-1.0, |health| health.0.current());
        Ok(format!(
            "entity {} {name} owner {owner:?} named {tag:?} horde {horde} at {:.1},{:.1} goal {goal} target {target} health {health:.0}",
            entity.index(),
            position.0,
            position.1
        ))
    }

    fn unit_positions_for_script(&self) -> String {
        let Some(ids) = self.state.simulation.rules().ids() else {
            return String::from("no match");
        };
        let world = self.state.simulation.state();
        let Some(store) = world.store(ids.shape) else {
            return String::from("0 units");
        };
        let mut count = 0usize;
        for (entity, shape) in store.iter() {
            if world.contains(ids.member, entity)
                || shape.kind_of.contains(mint_sage_asset::KindOf::IMMOBILE)
            {
                continue;
            }
            let Some(body) = world.get(ids.body, entity) else {
                continue;
            };
            let name = world
                .get(ids.kind, entity)
                .and_then(|kind| self.state.kinds.get(*kind))
                .map_or("-", |entry| entry.record.name.as_str());
            let path = world
                .get(ids.path, entity)
                .map_or(String::from("-"), |path| {
                    format!("{:.0},{:.0}", path.goal.x, path.goal.y)
                });
            let target = world
                .get(ids.armed, entity)
                .and_then(|armed| armed.target)
                .map_or(String::from("-"), |target| target.index().to_string());
            let layer = world
                .get(ids.layer, entity)
                .map_or(crate::wall_layer::Layer::GROUND.0, |layer| layer.0);
            let health = world
                .get(ids.health, entity)
                .map_or(-1.0, |health| health.0.current());
            println!(
                "unit {} {name} {:.1} {:.1} facing {:.2} speed {:.2} path {path} target {target} radius {:.1} layer {layer} health {health:.0}",
                entity.index(),
                body.0.position.x,
                body.0.position.y,
                body.0.facing.radians(),
                body.0.speed,
                shape.radius,
            );
            count += 1;
        }
        format!("{count} units")
    }

    fn horde_report_for_script(&self) -> String {
        use mint_movement::destination::Footprint;
        let Some(ids) = self.state.simulation.rules().ids() else {
            return String::from("no match");
        };
        let world = self.state.simulation.state();
        let horde_ids = RotwkRules::horde_ids(ids);
        let owned = hordes_of(&self.state.simulation, LOCAL_PLAYER);
        let hordes = crate::horde::hordes_of(world, horde_ids, &owned);
        let anchor = self.script_anchor;
        let mut blocks = Vec::new();
        for (index, horde) in hordes.iter().enumerate() {
            let (Some(body), Some(block)) = (
                world.get(ids.body, *horde),
                crate::horde::horde_block(world, horde_ids, *horde),
            ) else {
                continue;
            };
            let forward = body.0.facing.to_vector();
            let left = Vec2::new(-forward.y, forward.x);
            let centre = body
                .0
                .position
                .plus(forward.scale(block.centre.x))
                .plus(left.scale(block.centre.y));
            let footprint = Footprint::new(
                centre,
                body.0.facing,
                block.half_along,
                block.half_across,
                block.member_radius,
            );
            let (max_slot_error, farthest) =
                world.get(ids.horde, *horde).map_or((0.0, 0.0), |table| {
                    let poses = crate::horde::slot_poses(table, &body.0);
                    table
                        .members
                        .iter()
                        .filter_map(|member| {
                            let at = world.get(ids.body, *member)?.0.position;
                            let slot = world
                                .get(ids.member, *member)
                                .and_then(|entry| entry.slot)
                                .and_then(|slot| poses.get(usize::from(slot)).copied());
                            Some((
                                slot.map_or(0.0, |slot| slot.minus(at).length()),
                                at.minus(body.0.position).length(),
                            ))
                        })
                        .fold((0.0f32, 0.0f32), |(e, f), (me, mf)| (e.max(me), f.max(mf)))
                });
            println!(
                "drive: horde {index} entity {} at ({:.0}, {:.0}) block centre {:.0} from anchor, \
                 {:.0}x{:.0}, facing {:.0} deg, moving {}, max slot error {max_slot_error:.1}, \
                 farthest member {farthest:.1}",
                horde.index(),
                body.0.position.x,
                body.0.position.y,
                centre.minus(anchor).length(),
                block.half_across * 2.0,
                block.half_along * 2.0,
                body.0.facing.radians().to_degrees(),
                world.contains(ids.path, *horde)
            );
            blocks.push((*horde, footprint));
        }
        let mut crowded = 0usize;
        let mut worst = 0.0f32;
        for (i, (_, a)) in blocks.iter().enumerate() {
            for (j, (_, b)) in blocks.iter().enumerate().skip(i + 1) {
                let depth = a.penetration(b);
                worst = worst.max(depth);
                if a.crowds(b) {
                    crowded += 1;
                    println!("drive: hordes {i} and {j} overlap by {depth:.1}");
                }
            }
        }
        let mut soldiers = Vec::new();
        for (horde, _) in &blocks {
            let Some(table) = world.get(ids.horde, *horde) else {
                continue;
            };
            for member in &table.members {
                if let (Some(body), Some(shape)) =
                    (world.get(ids.body, *member), world.get(ids.shape, *member))
                {
                    let state = if world.contains(ids.member_attack, *member) {
                        0
                    } else if world.contains(ids.path, *member) {
                        1
                    } else if world
                        .get(ids.member_melee, *member)
                        .is_some_and(|record| record.goal.minus(body.0.position).length() > 1.0)
                    {
                        2
                    } else if world.contains(ids.member_melee, *member) {
                        3
                    } else {
                        4
                    };
                    soldiers.push((horde.index(), body.0.position, shape.radius, state));
                }
            }
        }
        let mut touching = 0usize;
        let mut deepest = 0.0f32;
        let mut kinds = [[0usize; 5]; 5];
        for (i, (ha, pa, ra, sa)) in soldiers.iter().enumerate() {
            for (hb, pb, rb, sb) in soldiers.iter().skip(i + 1) {
                if ha == hb {
                    continue;
                }
                let depth = ra + rb - pa.minus(*pb).length();
                if depth > 0.0 {
                    touching += 1;
                    deepest = deepest.max(depth);
                    if depth > 3.0 {
                        if let Some(row) = kinds.get_mut(*sa.min(sb)) {
                            if let Some(n) = row.get_mut(*sa.max(sb)) {
                                *n = n.saturating_add(1);
                            }
                        }
                    }
                }
            }
        }
        println!("drive: overlap pairs by state (attack, path, step, stand, other): {kinds:?}");
        let farthest = blocks
            .iter()
            .map(|(_, f)| f.centre.minus(anchor).length())
            .fold(0.0f32, f32::max);
        format!(
            "{} hordes; block pairs crowding {crowded}, deepest block overlap {worst:.1}; \
             soldier pairs of different hordes overlapping {touching}, deepest {deepest:.1}; \
             farthest block centre {farthest:.0} from anchor",
            blocks.len()
        )
    }

    pub(crate) fn act_for_script(&mut self, action: &str) -> Result<String, String> {
        self.ensure_snapshot();
        let (verb, argument) = action.split_once(' ').unwrap_or((action, ""));
        let argument = argument.trim();
        match verb {
            "buy" if !argument.is_empty() => {
                self.purchase_science(argument);
                Ok(format!("PurchaseScience {argument}"))
            }
            "cast" => {
                let (number, place) = argument.split_once(' ').unwrap_or((argument, ""));
                let button = number
                    .parse::<u8>()
                    .map_err(|_| format!("do cast: {number:?} is not a button number"))?;
                if place.trim().is_empty() {
                    self.cast_spell(button);
                    return Ok(format!("spell-book button {button}"));
                }
                let (x, y) = if place.trim() == "here" {
                    let focus = self.state.camera.focus();
                    (focus.x, focus.y)
                } else {
                    world_point(place)?
                };
                self.submit(RotwkCommand::CastSpell {
                    button,
                    target: None,
                    at: Some((x, y)),
                });
                Ok(format!("spell-book button {button} at ({x:.0}, {y:.0})"))
            }
            "power" => {
                let (first, rest) = argument.split_once(' ').unwrap_or((argument, ""));
                let Ok(button) = first.parse::<u8>() else {
                    let place = rest.trim();
                    let at = if place.is_empty() {
                        None
                    } else if place == "here" {
                        let focus = self.state.camera.focus();
                        Some((focus.x, focus.y))
                    } else {
                        Some(world_point(place)?)
                    };
                    self.submit(RotwkCommand::Debug(DebugCommand::FireNamedPower {
                        power: first.to_owned(),
                        target: None,
                        at,
                    }));
                    return Ok(format!(
                        "power {first}{}",
                        at.map_or_else(String::new, |(x, y)| format!(" at ({x:.0}, {y:.0})"),)
                    ));
                };
                let rest = rest.trim();
                if let Some(name) = rest.strip_prefix("kind ") {
                    let name = name.trim();
                    let target = self
                        .objects_of_kind(name, None)?
                        .first()
                        .copied()
                        .ok_or_else(|| format!("do power: no live {name:?}"))?;
                    self.issue_targeted(RotwkCommand::FireSpecialPower {
                        button,
                        target: Some(target),
                        at: None,
                    });
                    Ok(format!(
                        "power button {button} at kind {name} entity {}",
                        target.index()
                    ))
                } else if let Some(place) = rest.strip_prefix("at ") {
                    let place = place.trim();
                    let (x, y) = if place == "here" {
                        let focus = self.state.camera.focus();
                        (focus.x, focus.y)
                    } else {
                        world_point(place)?
                    };
                    self.submit(RotwkCommand::FireSpecialPower {
                        button,
                        target: None,
                        at: Some((x, y)),
                    });
                    Ok(format!("power button {button} at ({x:.0}, {y:.0})"))
                } else if rest.is_empty() {
                    self.submit(RotwkCommand::FireSpecialPower {
                        button,
                        target: None,
                        at: None,
                    });
                    Ok(format!("power button {button}"))
                } else {
                    Err(format!(
                        "do power: {rest:?} is not empty, kind NAME or at X,Y/here"
                    ))
                }
            }
            "autocast" => {
                let button = argument
                    .parse::<u8>()
                    .map_err(|_| format!("do autocast: {argument:?} is not a button number"))?;
                self.submit(RotwkCommand::ToggleAutoCast { button });
                Ok(format!("auto-cast button {button} toggled"))
            }
            "target" => {
                let pending = self
                    .power_target
                    .take()
                    .ok_or("do target: no button is waiting for a target")?;
                let (x, y) = self.place_for_script(argument)?;
                let at = (pending.kind != crate::templates::TargetKind::Confirm).then_some((x, y));
                let command = pending
                    .command(None, at)
                    .ok_or("do target: the button made no order")?;
                println!("drive: target ({x:.0}, {y:.0}) orders {command:?}");
                self.submit(command);
                Ok(format!("target ({x:.0}, {y:.0})"))
            }
            "fire" => {
                let (slot, place) = argument
                    .split_once(" at ")
                    .ok_or_else(|| format!("do fire: {argument:?} is not SLOT at PLACE"))?;
                let slot = mint_sage_combat::WeaponSlot::parse(slot.trim())
                    .ok_or_else(|| format!("do fire: {slot:?} is not a weapon slot"))?;
                let (x, y) = self.place_for_script(place)?;
                let slot_index = u8::try_from(slot.index()).map_err(|_| "do fire: slot")?;
                self.submit(RotwkCommand::AttackGround {
                    slot: slot_index,
                    x,
                    y,
                });
                Ok(format!("AttackGround {} at ({x:.0}, {y:.0})", slot.name()))
            }
            "attack" => {
                let target = self
                    .script_target(argument)
                    .map_err(|e| format!("do attack: {e}"))?;
                println!("drive: attack {argument} - entity {}", target.index());
                self.submit(RotwkCommand::AttackObject { target });
                Ok(format!("attack {}", target.index()))
            }
            "forceattack" => {
                let target = self
                    .script_target(argument)
                    .map_err(|e| format!("do forceattack: {e}"))?;
                println!("drive: force-attack {argument} - entity {}", target.index());
                self.submit(RotwkCommand::ForceAttackObject { target });
                Ok(format!("force-attack {}", target.index()))
            }
            "attackmove" => {
                let (dx, dy) = world_point(argument)?;
                let (x, y) = (self.script_anchor.x + dx, self.script_anchor.y + dy);
                self.submit(RotwkCommand::AttackMove {
                    x,
                    y,
                    queue: self.order_mode_waypoint,
                });
                Ok(format!("AttackMove ({x:.0}, {y:.0})"))
            }
            "stop" => {
                self.slot_action(SlotAction::Stop);
                Ok(String::from("Stop"))
            }
            "toggleformation" => {
                self.slot_action(SlotAction::ToggleHordeFormation);
                Ok(String::from("ToggleHordeFormation"))
            }
            "look" => {
                let (x, y) = match world_point(argument) {
                    Ok(point) => point,
                    Err(_) => {
                        let first = self
                            .script_target(argument)
                            .map_err(|e| format!("do look: {e}"))?;
                        let ids = self
                            .state
                            .simulation
                            .rules()
                            .ids()
                            .ok_or("do look: no match")?;
                        let body = self
                            .state
                            .simulation
                            .state()
                            .get(ids.body, first)
                            .ok_or("do look: no body")?;
                        println!("drive: look {argument} - entity {}", first.index());
                        (body.0.position.x, body.0.position.y)
                    }
                };
                let focus = Vec3::new(x, y, ground_at(&self.state.scene, x, y));
                focus_on(&mut self.state.camera, focus, SCRIPT_VIEW_DISTANCE);
                Ok(format!("camera on ({x:.0}, {y:.0})"))
            }
            "cine" => self.dev_cine(argument),
            "flash" => self.dev_flash(argument),
            "anchor" => {
                let focus = self.state.camera.focus();
                self.script_anchor = Vec2::new(focus.x, focus.y);
                Ok(format!("anchor ({:.0}, {:.0})", focus.x, focus.y))
            }
            "camera" => {
                let focus = self.state.camera.focus();
                let inset = self.state.camera.ground_inset();
                Ok(format!(
                    "focus ({:.1}, {:.1}) inset {inset:.1} yaw {:.1} deg pitch {:.1} deg distance {:.1}",
                    focus.x,
                    focus.y,
                    self.state.camera.yaw().to_degrees(),
                    self.state.camera.pitch().to_degrees(),
                    self.state.camera.distance()
                ))
            }
            "move" => {
                let (dx, dy) = world_point(argument)?;
                let (x, y) = (self.script_anchor.x + dx, self.script_anchor.y + dy);
                self.submit(RotwkCommand::MoveTo {
                    x,
                    y,
                    queue: self.order_mode_waypoint,
                });
                Ok(format!("MoveTo ({x:.0}, {y:.0})"))
            }
            "moveto" => {
                let (x, y) = world_point(argument)?;
                self.submit(RotwkCommand::MoveTo {
                    x,
                    y,
                    queue: self.order_mode_waypoint,
                });
                Ok(format!("MoveTo ({x:.0}, {y:.0})"))
            }
            "attackmoveto" => {
                let (x, y) = world_point(argument)?;
                self.submit(RotwkCommand::AttackMove {
                    x,
                    y,
                    queue: self.order_mode_waypoint,
                });
                Ok(format!("AttackMove ({x:.0}, {y:.0})"))
            }
            "diag" => crate::diag::set(argument),
            "hordes" => Ok(self.horde_report_for_script()),
            "soldiers" => Ok(self.soldier_positions_for_script()),
            "obstaclecells" => Ok(self.obstacle_cells_for_script()),
            "footprint-breach" => Ok(self.footprint_breach_for_script()),
            "units" => Ok(self.unit_positions_for_script()),
            "entity" => self.entity_report_for_script(argument),
            "powerready" => {
                let (kind, power_name) = argument
                    .split_once(' ')
                    .ok_or_else(|| format!("do powerready: {argument:?} is not KIND POWERNAME"))?;
                let entity = self
                    .objects_of_kind(kind.trim(), None)?
                    .first()
                    .copied()
                    .ok_or_else(|| format!("do powerready: no live {kind:?}"))?;
                let rules = self.state.simulation.rules();
                let Some(ids) = rules.ids() else {
                    return Err(String::from("do powerready: no match"));
                };
                let now = self.state.simulation.frame();
                Ok(rules.power_ready_report(
                    self.state.simulation.state(),
                    ids,
                    now,
                    entity,
                    power_name.trim(),
                ))
            }
            "bar" => Ok(self.command_bar_report_for_script()),
            "spells" => Ok(self.spell_bar_report_for_script()),
            "rank" => Ok(self.rank_ui_report_for_script()),
            "owners" => Ok(self.map_owner_report_for_script()),
            "crash" => {
                self.forced_crash = Some(super::crash_report::ForcedCrash::parse(argument)?);
                Ok(format!("crash {argument} on the next frame"))
            }
            "shroud" => Ok(self.shroud_report_for_script()),
            "scripts" => {
                let line = self.state.units.report_line();
                println!("{line}");
                Ok(line)
            }
            "report" => {
                let (printed, skipped) = self.report.census();
                Ok(format!("report {printed} printed, {skipped} skipped"))
            }
            "reset-shroud" => {
                let use_shroud = argument == "on";
                self.submit(RotwkCommand::ResetShroud { use_shroud });
                Ok(format!(
                    "reset-shroud {}",
                    if use_shroud { "on" } else { "off" }
                ))
            }
            "sell" => {
                self.slot_action(SlotAction::Sell);
                Ok(String::from("Sell"))
            }
            "enter" => {
                let target = self
                    .script_target(argument)
                    .map_err(|e| format!("do enter: {e}"))?;
                self.submit(RotwkCommand::Enter { target });
                Ok(format!("Enter entity {}", target.index()))
            }
            "exit" => {
                self.submit(RotwkCommand::Exit { passenger: None });
                Ok(String::from("Exit"))
            }
            "contain" => {
                let rules = self.state.simulation.rules();
                let lines = rules.contain_report(self.state.simulation.state());
                if lines.is_empty() {
                    return Ok(String::from("contain: nothing contained"));
                }
                for line in &lines {
                    println!("{line}");
                }
                Ok(lines.join("; "))
            }
            "press" if !argument.is_empty() => {
                let units = self.units();
                let hud = crate::hud::read_hud(&self.state, &self.data, &units, self.viewport);
                let bar = &hud.command_bar;
                let flyout = hud
                    .stance_flyout
                    .map_or(&[][..], |_| bar.stance_flyout.as_slice());
                let slot = bar
                    .ring
                    .iter()
                    .flatten()
                    .chain(&bar.bar)
                    .chain(flyout)
                    .chain(bar.radial.iter().map(|(_, slot)| slot))
                    .find(|slot| slot.command_button.eq_ignore_ascii_case(argument))
                    .ok_or_else(|| format!("do press: no shown button {argument:?}"))?;
                let action = slot.action.clone();
                self.slot_action(action);
                Ok(format!("pressed {argument}"))
            }
            "found" => {
                let button = argument
                    .parse::<u8>()
                    .map_err(|_| format!("do found: {argument:?} is not a button number"))?;
                self.slot_action(SlotAction::FoundationConstruct(button));
                Ok(format!("FoundationConstruct button {button}"))
            }
            "recruit" | "upgrade" => {
                let button = argument
                    .parse::<u8>()
                    .map_err(|_| format!("do {verb}: {argument:?} is not a button number"))?;
                let action = if verb == "recruit" {
                    SlotAction::Revive(button)
                } else {
                    SlotAction::ObjectUpgrade(button)
                };
                self.slot_action(action);
                Ok(format!("{verb} button {button}"))
            }
            "stance" if argument.is_empty() => Ok(self.stance_report_for_script()),
            "stance" => {
                let stance = mint_sage_ini::Stance::from_name(argument)
                    .ok_or_else(|| format!("do stance: {argument:?} is not a stance name"))?;
                self.set_stance(stance);
                Ok(format!("SetStance {}", stance.as_str()))
            }
            "kill" if argument.is_empty() => {
                self.submit(RotwkCommand::DebugKill);
                Ok(String::from("DebugKill"))
            }
            "kill" if argument.contains(" by ") => {
                let (target, killer) = argument
                    .rsplit_once(" by ")
                    .ok_or_else(|| format!("do kill: {argument:?} wants TARGET by TARGET"))?;
                let target = self.debug_target(target)?;
                let killer = self.debug_target(killer)?;
                let said = format!("Kill {target:?} by {killer:?}");
                self.submit(RotwkCommand::Debug(DebugCommand::KillBy { target, killer }));
                Ok(said)
            }
            "kill" => {
                let target = self.debug_target(argument)?;
                let said = format!("Kill {target:?}");
                self.submit(RotwkCommand::Debug(DebugCommand::Kill { target }));
                Ok(said)
            }
            "invulnerable" => {
                let (target, on) = argument
                    .rsplit_once(' ')
                    .and_then(|(target, on)| match on {
                        "on" => Some((target, true)),
                        "off" => Some((target, false)),
                        _ => None,
                    })
                    .ok_or_else(|| format!("do invulnerable: {argument:?} wants TARGET on|off"))?;
                let target = self.debug_target(target)?;
                let said = format!("Indestructible {on} {target:?}");
                self.submit(RotwkCommand::Debug(DebugCommand::Indestructible {
                    target,
                    on,
                }));
                Ok(said)
            }
            "complete" => {
                let target = self.debug_target(argument)?;
                let said = format!("Complete {target:?}");
                self.submit(RotwkCommand::Debug(DebugCommand::Complete { target }));
                Ok(said)
            }
            "capture" => {
                let target = self.debug_target(argument)?;
                let said = format!("Capture {target:?}");
                self.submit(RotwkCommand::Debug(DebugCommand::Capture { target }));
                Ok(said)
            }
            "place" => {
                let (name, at) = argument
                    .rsplit_once(" at ")
                    .ok_or_else(|| format!("do place: {argument:?} wants TEMPLATE at X,Y"))?;
                let (x, y) = world_point(at)?;
                let name = name.trim();
                let kind = self
                    .state
                    .kinds
                    .kind(name)
                    .ok_or_else(|| format!("do place: {name:?} names no template"))?;
                let structure = self
                    .state
                    .kinds
                    .get(kind)
                    .is_some_and(|entry| entry.record.has_kind("STRUCTURE"));
                self.submit(if structure {
                    RotwkCommand::DebugSpawnStructure {
                        kind: kind.0,
                        x,
                        y,
                        angle: 0.0,
                    }
                } else {
                    RotwkCommand::Debug(DebugCommand::Spawn {
                        kind: kind.0,
                        x,
                        y,
                        facing: 0.0,
                        role: crate::rules::SpawnRole::Normal,
                    })
                });
                Ok(format!("place {name} at ({x}, {y})"))
            }
            "teleport" => {
                let (target, at) = argument
                    .rsplit_once(" to ")
                    .ok_or_else(|| format!("do teleport: {argument:?} wants TARGET to X,Y"))?;
                let (x, y) = world_point(at)?;
                let target = self.debug_target(target)?;
                let said = format!("Teleport {target:?} to ({x}, {y})");
                self.submit(RotwkCommand::Debug(DebugCommand::Teleport { target, x, y }));
                Ok(said)
            }
            "oath" => {
                self.submit(RotwkCommand::DebugOath);
                Ok(String::from("DebugOath"))
            }
            "ring" => {
                self.submit(crate::test_controls::TestControls::hold_ring());
                Ok(String::from("HoldRing"))
            }
            "damage" => {
                let percent = argument
                    .parse::<u8>()
                    .ok()
                    .filter(|p| (1..=100).contains(p))
                    .ok_or_else(|| format!("do damage: {argument:?} is not 1 to 100"))?;
                self.submit(crate::test_controls::TestControls::damage(percent));
                Ok(format!("Damage {percent}%"))
            }
            "trace" => {
                let kind = self
                    .state
                    .kinds
                    .kind(argument)
                    .ok_or_else(|| format!("do trace: no template {argument:?}"))?;
                self.state_trace = Some((kind.0, std::collections::BTreeMap::new()));
                Ok(format!("tracing {argument}"))
            }
            "hover"
                if argument == "own" || argument == "enemy" || argument.starts_with("kind ") =>
            {
                let me = self.acting_player;
                let focus = self.state.camera.focus();
                let units = self.units();
                let of_kind = match argument.strip_prefix("kind ") {
                    Some(name) => self.objects_of_kind(name.trim(), None)?,
                    None => Vec::new(),
                };
                let rules = self.state.simulation.rules();
                let unit = units
                    .iter()
                    .filter(|u| u.pick_type == mint_sage_asset::PickType::Selectable)
                    .filter(|u| match argument {
                        "own" => u.owner == me,
                        "enemy" => rules.hostile(u.owner, me),
                        _ => of_kind.contains(&u.entity),
                    })
                    .min_by(|a, b| {
                        let d = |u: &UnitView| {
                            let (dx, dy) = (u.position.x - focus.x, u.position.y - focus.y);
                            dx * dx + dy * dy
                        };
                        d(a).total_cmp(&d(b))
                    })
                    .ok_or_else(|| format!("do hover: no {argument} unit"))?;
                let (entity, x, y, feet, mid) = (
                    unit.entity,
                    unit.position.x,
                    unit.position.y,
                    unit.ground,
                    unit.height * 0.5,
                );
                let ground = ground_at(&self.state.scene, x, y);
                self.state.camera.set_focus(Vec3::new(x, y, ground));
                let (px, py, _) = crate::pick::project(
                    &self.state.camera.camera(),
                    self.viewport,
                    Vec3::new(x, y, feet + mid),
                )
                .ok_or("do hover: the unit is behind the camera")?;
                self.input(&InputEvent::PointerMoved { x: px, y: py });
                Ok(format!(
                    "pointer on {argument} entity {} at pixel ({px:.0}, {py:.0}) cursor {}",
                    entity.index(),
                    self.cursor_want().name()
                ))
            }
            "groundclick" => {
                let (dx, dy) = world_point(argument)?;
                let (x, y) = (self.script_anchor.x + dx, self.script_anchor.y + dy);
                let ground = ground_at(&self.state.scene, x, y);
                let (px, py, _) = crate::pick::project(
                    &self.state.camera.camera(),
                    self.viewport,
                    Vec3::new(x, y, ground),
                )
                .ok_or("do groundclick: point is behind the camera")?;
                self.input(&InputEvent::PointerMoved { x: px, y: py });
                let cursor = self.cursor_want().name().to_owned();
                self.input(&InputEvent::PointerButton {
                    button: mint_app::PointerButton::Secondary,
                    pressed: true,
                });
                self.input(&InputEvent::PointerButton {
                    button: mint_app::PointerButton::Secondary,
                    pressed: false,
                });
                Ok(format!(
                    "groundclick ({x:.0}, {y:.0}) at pixel ({px:.0}, {py:.0}) cursor {cursor}"
                ))
            }
            "build" if !argument.is_empty() => {
                let Some(kind) = self.state.kinds.kind(argument) else {
                    return Err(format!("do build: {argument:?} names no kind"));
                };
                let at = self.state.camera.focus();
                self.submit(RotwkCommand::Construct {
                    kind: kind.0,
                    x: at.x,
                    y: at.y,
                    angle: 0.0,
                });
                Ok(format!(
                    "Construct {argument} at ({:.0}, {:.0})",
                    at.x, at.y
                ))
            }
            "wall" => {
                let (head, tail) = argument.split_once(' ').unwrap_or(("", argument));
                let option = if head.is_empty() {
                    0
                } else {
                    head.trim()
                        .parse::<u8>()
                        .map_err(|_| format!("do wall: {head:?} is not a module option"))?
                        .saturating_sub(1)
                };
                let (dx, dy) = world_point(tail)?;
                let hub = self
                    .units()
                    .into_iter()
                    .find(|unit| unit.selected && unit.owner == self.acting_player)
                    .ok_or("do wall: nothing of the acting player's is selected")?;
                let (x, y) = (hub.position.x + dx, hub.position.y + dy);
                self.submit(RotwkCommand::WallSpan { option, x, y });
                Ok(format!(
                    "WallSpan option {} from entity {} to ({x:.0}, {y:.0})",
                    u16::from(option).saturating_add(1),
                    hub.entity.index()
                ))
            }
            "victory" | "defeat" => {
                self.forced_end = Some(if verb == "victory" {
                    mint_sage_campaign::MissionEnd::Victory
                } else {
                    mint_sage_campaign::MissionEnd::Defeat
                });
                Ok(format!("mission end forced: {verb}"))
            }
            "money" => {
                let amount = argument
                    .parse::<u32>()
                    .map_err(|_| format!("do money: {argument:?} is not a number"))?;
                self.submit(RotwkCommand::DebugMoney { amount });
                Ok(format!("{amount} resource(s)"))
            }
            "points" => {
                let points = argument
                    .parse::<u32>()
                    .map_err(|_| format!("do points: {argument:?} is not a number"))?;
                self.submit(RotwkCommand::DebugSkillPoints { points });
                Ok(format!("{points} skill point(s)"))
            }
            "audio" => Ok(self.audio.readout()),
            "spellpoints" => {
                let points = argument
                    .parse::<u32>()
                    .map_err(|_| format!("do spellpoints: {argument:?} is not a number"))?;
                self.submit(RotwkCommand::DebugSpellPoints { points });
                Ok(format!("{points} power point(s)"))
            }
            "experience" => {
                let points = argument
                    .parse::<u32>()
                    .map_err(|_| format!("do experience: {argument:?} is not a number"))?;
                self.submit(RotwkCommand::DebugExperience { points });
                Ok(format!("{points} experience for the selection"))
            }
            "flag" | "counter" | "timer" => {
                let (value, name) = argument
                    .split_once(' ')
                    .map(|(value, name)| (value.trim(), name.trim().to_owned()))
                    .filter(|(_, name)| !name.is_empty())
                    .ok_or_else(|| format!("do {verb}: wants VALUE NAME"))?;
                let command = if verb == "flag" {
                    let value = match value {
                        "1" | "true" => true,
                        "0" | "false" => false,
                        _ => return Err(format!("do flag: {value:?} is not 1 or 0")),
                    };
                    DebugCommand::SetScriptFlag {
                        name: name.clone(),
                        value,
                    }
                } else {
                    let value = value
                        .parse::<i32>()
                        .map_err(|_| format!("do {verb}: {value:?} is not a number"))?;
                    if verb == "timer" {
                        DebugCommand::SetScriptTimer {
                            name: name.clone(),
                            frames: value,
                        }
                    } else {
                        DebugCommand::SetScriptCounter {
                            name: name.clone(),
                            value,
                        }
                    }
                };
                self.submit(RotwkCommand::Debug(command));
                Ok(format!("script {verb} {name:?} = {value}"))
            }
            "status" if !argument.is_empty() => {
                self.submit(RotwkCommand::Debug(DebugCommand::SetStatus {
                    status: argument.to_owned(),
                }));
                Ok(format!("status {argument} for the selection"))
            }
            "grant" if argument.contains(" to ") => {
                let (upgrade, target) = argument
                    .split_once(" to ")
                    .ok_or_else(|| format!("do grant: {argument:?} wants UPGRADE to TARGET"))?;
                let target = self.debug_target(target)?;
                let upgrade = upgrade.trim().to_owned();
                let said = format!("Upgrade {upgrade} for {target:?}");
                self.submit(RotwkCommand::Debug(DebugCommand::Upgrade {
                    target,
                    upgrade,
                }));
                Ok(said)
            }
            "grant" if !argument.is_empty() => {
                self.submit(RotwkCommand::Debug(DebugCommand::GrantUpgrade {
                    upgrade: argument.to_owned(),
                }));
                Ok(format!("upgrade {argument} for the selection"))
            }
            "spawn" => {
                let name = argument.trim();
                let rules = self.state.simulation.rules();
                let found = rules
                    .buildable_catalogue()
                    .into_iter()
                    .find(|entry| entry.label == name)
                    .map(|entry| (entry.index, entry.is_horde))
                    .or_else(|| rules.hero_buildable_named(name).map(|index| (index, false)));
                let (index, is_horde) = found
                    .ok_or_else(|| format!("do spawn: {name:?} is not buildable in this match"))?;
                let focus = self.state.camera.focus();
                let (x, y) = (focus.x, focus.y);
                self.submit(if is_horde {
                    RotwkCommand::DebugSpawnHorde {
                        recipe: index,
                        x,
                        y,
                        facing: 0.0,
                    }
                } else {
                    RotwkCommand::DebugSpawnUnit {
                        buildable: index,
                        x,
                        y,
                        facing: 0.0,
                    }
                });
                Ok(format!("spawn {name} at ({x:.0}, {y:.0})"))
            }
            "cancel-upgrade" | "cancel-player-upgrade" => {
                let button = argument
                    .parse::<u8>()
                    .map_err(|_| format!("do {verb}: {argument:?} is not a button number"))?;
                let player = verb == "cancel-player-upgrade";
                self.submit(RotwkCommand::CancelUpgrade { button, player });
                Ok(format!("CancelUpgrade button {button}"))
            }
            other => Err(format!(
                "do: {other:?} is not buy SCIENCE, cast N [X,Y|here], look X,Y, sell, kill, \
                 build KIND, spawn NAME, points N or cancel-upgrade N"
            )),
        }
    }
}
