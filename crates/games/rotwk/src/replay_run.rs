use std::path::Path;
use std::sync::Arc;

use mint_sim::{CommandKey, Entity, Frame, PlayerId};
use mint_vfs::Vfs;

use crate::rules::{DebugCommand, DebugTarget, RotwkCommand, SpawnRole};

struct Recording {
    seed: u64,
    initial_hash: u64,
    commands: Vec<(CommandKey, RotwkCommand)>,
    hashes: Vec<u64>,
}

pub(crate) fn run(
    vfs: &Arc<Vfs>,
    map_bytes: &[u8],
    map_label: String,
    config: &crate::game::MatchConfig,
    path: &Path,
) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let recording = parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    println!(
        "replay: {} command(s) over {} frame(s), seed {:#018x}",
        recording.commands.len(),
        recording.hashes.len(),
        recording.seed
    );

    let window = mint_app::WindowConfig::default();
    let mut renderer =
        mint_render::Renderer::offscreen(window.width, window.height).map_err(|e| e.to_string())?;
    let (mut data, line) = crate::load_game_data_or_exit(vfs);
    println!("{line}");
    crate::game_data::compose_for_match(&mut data, vfs, &map_label)?;
    let object_index = data.object_index.take();
    let (skirmish, allow_ring_heroes) = config.scene_filter();
    let scene = crate::scene::load_match_scene(
        &mut renderer,
        vfs,
        map_bytes,
        map_label,
        &data.templates,
        skirmish,
        allow_ring_heroes,
        object_index,
        Some(crate::scene::game_tuning_file()),
    )?;
    let setup = crate::game::build_match(
        &scene,
        &data,
        vfs,
        config,
        crate::game::PassabilitySources::default(),
    )?;
    let mut simulation = setup.simulation;
    if simulation.seed() != recording.seed {
        return Err(format!(
            "the built match has seed {:#018x}, the recording {:#018x}",
            simulation.seed(),
            recording.seed
        ));
    }
    if simulation.state_hash() != recording.initial_hash {
        return Err(format!(
            "the initial state differs: recorded {:#018x}, got {:#018x}",
            recording.initial_hash,
            simulation.state_hash()
        ));
    }

    let mut commands = recording.commands.into_iter().peekable();
    let started = std::time::Instant::now();
    let mut diverged = None;
    for (index, expected) in recording.hashes.iter().enumerate() {
        let frame = Frame::new(u64::try_from(index).map_err(|e| e.to_string())?);
        while let Some((key, _)) = commands.peek() {
            if key.frame > frame {
                break;
            }
            let Some((key, command)) = commands.next() else {
                break;
            };
            simulation
                .submit_with_key(key, command)
                .map_err(|e| format!("frame {index}: {e}"))?;
        }
        let report = simulation
            .tick()
            .map_err(|e| format!("frame {index} failed: {}", chain(&e)))?;
        if report.hash != *expected && diverged.is_none() {
            println!(
                "replay: diverged at frame {index}: recorded {expected:#018x}, got {:#018x}; continuing",
                report.hash
            );
            diverged = Some(index);
        }
    }
    match diverged {
        None => println!(
            "replay: all {} frame(s) matched in {:.1} s",
            recording.hashes.len(),
            started.elapsed().as_secs_f64()
        ),
        Some(index) => println!(
            "replay: ran {} frame(s) in {:.1} s, matching up to frame {index}",
            recording.hashes.len(),
            started.elapsed().as_secs_f64()
        ),
    }
    for (key, command) in commands {
        simulation
            .submit_with_key(key, command)
            .map_err(|e| e.to_string())?;
    }
    match simulation.tick() {
        Ok(report) => {
            println!(
                "replay: the next frame {} ticks cleanly",
                report.frame.get()
            );
            Ok(())
        }
        Err(error) => Err(format!(
            "frame {} failed: {}",
            recording.hashes.len(),
            chain(&error)
        )),
    }
}

pub(crate) fn chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(&format!("\n  caused by: {cause}"));
        source = cause.source();
    }
    text
}

fn parse(text: &str) -> Result<Recording, String> {
    let mut format_version = None;
    let mut expected_frames = None;
    let mut expected_commands = None;
    let mut seed = None;
    let mut initial_hash = None;
    let mut commands = Vec::new();
    let mut hashes = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let at = |e: String| format!("line {}: {e}", number + 1);
        if let Some(rest) = line.strip_prefix("replay-format ") {
            format_version = Some(
                rest.trim()
                    .parse::<u32>()
                    .map_err(|_| at(format!("{rest:?} is not a version number")))?,
            );
        } else if let Some(rest) = line.strip_prefix("frames ") {
            expected_frames = Some(
                rest.trim()
                    .parse::<usize>()
                    .map_err(|_| at(format!("{rest:?} is not a frame count")))?,
            );
        } else if let Some(rest) = line.strip_prefix("commands ") {
            expected_commands = Some(
                rest.trim()
                    .parse::<usize>()
                    .map_err(|_| at(format!("{rest:?} is not a command count")))?,
            );
        } else if let Some(rest) = line.strip_prefix("seed ") {
            seed = Some(hex(rest).map_err(at)?);
        } else if let Some(rest) = line.strip_prefix("initial-hash ") {
            initial_hash = Some(hex(rest).map_err(at)?);
        } else if let Some(rest) = line.strip_prefix("h ") {
            let (frame, hash) = rest
                .split_once(' ')
                .ok_or_else(|| at("h wants two fields".into()))?;
            if frame.parse::<usize>().ok() != Some(hashes.len()) {
                return Err(at(format!("frame {frame} is out of order")));
            }
            hashes.push(hex(hash).map_err(at)?);
        } else if let Some(rest) = line.strip_prefix("c ") {
            let mut parts = rest.splitn(4, ' ');
            let mut next = |what: &str| parts.next().ok_or_else(|| at(format!("c wants {what}")));
            let frame: u64 = next("a frame")?
                .parse()
                .map_err(|_| at("bad frame".into()))?;
            let player: u8 = next("a player")?
                .parse()
                .map_err(|_| at("bad player".into()))?;
            let sequence: u32 = next("a sequence")?
                .parse()
                .map_err(|_| at("bad sequence".into()))?;
            let value = Parser::new(next("a command")?).value_to_end().map_err(at)?;
            let command = command(&value).map_err(at)?;
            commands.push((
                CommandKey::new(Frame::new(frame), PlayerId::new(player), sequence),
                command,
            ));
        }
    }
    let format_version = format_version.ok_or("no replay-format line")?;
    if format_version != mint_sim::REPLAY_FORMAT_VERSION {
        return Err(format!(
            "replay-format {format_version} is not a version this build knows \
             ({} is)",
            mint_sim::REPLAY_FORMAT_VERSION
        ));
    }
    let expected_frames = expected_frames.ok_or("no frames line")?;
    if expected_frames != hashes.len() {
        return Err(format!(
            "frames {expected_frames} but {} were read - the file is truncated",
            hashes.len()
        ));
    }
    let expected_commands = expected_commands.ok_or("no commands line")?;
    if expected_commands != commands.len() {
        return Err(format!(
            "commands {expected_commands} but {} were read - the file is truncated",
            commands.len()
        ));
    }
    Ok(Recording {
        seed: seed.ok_or("no seed line")?,
        initial_hash: initial_hash.ok_or("no initial-hash line")?,
        commands,
        hashes,
    })
}

fn hex(text: &str) -> Result<u64, String> {
    let digits = text.trim().trim_start_matches("0x");
    u64::from_str_radix(digits, 16).map_err(|_| format!("{text:?} is not a hex number"))
}

#[derive(Debug)]
enum Value {
    Word(String),
    Text(String),
    List(Vec<Value>),
    Tuple(Option<String>, Vec<Value>),
    Struct(String, Vec<(String, Value)>),
}

struct Parser<'a> {
    rest: &'a str,
    depth: usize,
}

const MAX_DEPTH: usize = 16;

impl<'a> Parser<'a> {
    const fn new(text: &'a str) -> Self {
        Self {
            rest: text,
            depth: 0,
        }
    }

    fn value_to_end(mut self) -> Result<Value, String> {
        let value = self.value()?;
        if self.rest.trim().is_empty() {
            Ok(value)
        } else {
            Err(format!("trailing text {:?}", self.rest))
        }
    }

    fn skip_space(&mut self) {
        self.rest = self.rest.trim_start();
    }

    fn eat(&mut self, token: char) -> bool {
        self.skip_space();
        match self.rest.strip_prefix(token) {
            Some(rest) => {
                self.rest = rest;
                true
            }
            None => false,
        }
    }

    fn expect(&mut self, token: char) -> Result<(), String> {
        if self.eat(token) {
            Ok(())
        } else {
            Err(format!("wanted {token:?} at {:?}", self.rest))
        }
    }

    fn word(&mut self) -> String {
        self.skip_space();
        let end = self
            .rest
            .find(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '.' | '-' | '+')))
            .unwrap_or(self.rest.len());
        let (word, rest) = self.rest.split_at(end);
        self.rest = rest;
        word.to_owned()
    }

    fn value(&mut self) -> Result<Value, String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err("nested too deep".into());
        }
        let value = self.value_inner();
        self.depth -= 1;
        value
    }

    fn value_inner(&mut self) -> Result<Value, String> {
        if self.eat('[') {
            return Ok(Value::List(self.items(']')?));
        }
        if self.eat('(') {
            return Ok(Value::Tuple(None, self.items(')')?));
        }
        if self.eat('"') {
            return self.text();
        }
        let word = self.word();
        if word.is_empty() {
            return Err(format!("no value at {:?}", self.rest));
        }
        if self.eat('(') {
            return Ok(Value::Tuple(Some(word), self.items(')')?));
        }
        if self.eat('{') {
            let mut fields = Vec::new();
            while !self.eat('}') {
                let name = self.word();
                self.expect(':')?;
                fields.push((name, self.value()?));
                if !self.eat(',') {
                    self.expect('}')?;
                    break;
                }
            }
            return Ok(Value::Struct(word, fields));
        }
        Ok(Value::Word(word))
    }

    fn items(&mut self, close: char) -> Result<Vec<Value>, String> {
        let mut items = Vec::new();
        while !self.eat(close) {
            items.push(self.value()?);
            if !self.eat(',') {
                self.expect(close)?;
                break;
            }
        }
        Ok(items)
    }

    fn text(&mut self) -> Result<Value, String> {
        let mut out = String::new();
        let mut chars = self.rest.char_indices();
        while let Some((at, c)) = chars.next() {
            match c {
                '"' => {
                    self.rest = self.rest.get(at + 1..).unwrap_or_default();
                    return Ok(Value::Text(out));
                }
                '\\' => match chars.next().map(|(_, c)| c) {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some(other) => out.push(other),
                    None => break,
                },
                other => out.push(other),
            }
        }
        Err("unterminated string".into())
    }
}

fn fields<'v>(value: &'v Value, name: &str) -> Result<&'v [(String, Value)], String> {
    match value {
        Value::Struct(found, fields) if found == name => Ok(fields),
        other => Err(format!("wanted {name} {{..}}, got {other:?}")),
    }
}

fn field<'v>(fields: &'v [(String, Value)], name: &str) -> Result<&'v Value, String> {
    fields
        .iter()
        .find(|(found, _)| found == name)
        .map(|(_, value)| value)
        .ok_or_else(|| format!("no field {name}"))
}

fn number<T: std::str::FromStr>(fields: &[(String, Value)], name: &str) -> Result<T, String> {
    match field(fields, name)? {
        Value::Word(word) => word
            .parse()
            .map_err(|_| format!("{name}: bad number {word:?}")),
        other => Err(format!("{name}: wanted a number, got {other:?}")),
    }
}

fn bool_field(fields: &[(String, Value)], name: &str) -> bool {
    match fields.iter().find(|(found, _)| found == name) {
        Some((_, Value::Word(word))) => word == "true",
        _ => false,
    }
}

fn entity(value: &Value) -> Result<Entity, String> {
    let parts = fields(value, "Entity")?;
    Ok(Entity::from_parts(
        number(parts, "index")?,
        number(parts, "generation")?,
    ))
}

fn option<T>(
    value: &Value,
    inner: impl Fn(&Value) -> Result<T, String>,
) -> Result<Option<T>, String> {
    match value {
        Value::Word(word) if word == "None" => Ok(None),
        Value::Tuple(Some(name), items) if name == "Some" => match items.as_slice() {
            [item] => inner(item).map(Some),
            _ => Err("Some wants one item".into()),
        },
        other => Err(format!("wanted an Option, got {other:?}")),
    }
}

fn point(value: &Value) -> Result<(f32, f32), String> {
    match value {
        Value::Tuple(None, items) => match items.as_slice() {
            [Value::Word(x), Value::Word(y)] => Ok((
                x.parse().map_err(|_| format!("bad x {x:?}"))?,
                y.parse().map_err(|_| format!("bad y {y:?}"))?,
            )),
            _ => Err("a point wants two numbers".into()),
        },
        other => Err(format!("wanted a point, got {other:?}")),
    }
}

fn entities(value: &Value) -> Result<Vec<Entity>, String> {
    match value {
        Value::List(items) => items.iter().map(entity).collect(),
        other => Err(format!("wanted a list, got {other:?}")),
    }
}

fn command(value: &Value) -> Result<RotwkCommand, String> {
    let (name, f): (&str, &[(String, Value)]) = match value {
        Value::Word(name) => (name.as_str(), &[]),
        Value::Struct(name, fields) => (name.as_str(), fields.as_slice()),
        Value::Tuple(Some(name), items) if name == "Debug" => {
            return match items.as_slice() {
                [inner] => debug_command(inner).map(RotwkCommand::Debug),
                _ => Err("Debug wants one item".into()),
            };
        }
        other => return Err(format!("not a command: {other:?}")),
    };
    let target = || field(f, "target").and_then(entity);
    Ok(match name {
        "SetSelection" => RotwkCommand::SetSelection {
            replace: number(f, "replace")?,
            units: entities(field(f, "units")?)?,
        },
        "ClearSelection" => RotwkCommand::ClearSelection,
        "SetStance" => RotwkCommand::SetStance {
            stance: number(f, "stance")?,
        },
        "ToggleWeaponSet" => match field(f, "button")? {
            Value::Text(button) => RotwkCommand::ToggleWeaponSet {
                button: button.clone(),
                lead: option(field(f, "lead")?, entity)?,
            },
            other => return Err(format!("button: wanted a string, got {other:?}")),
        },
        "Sacrifice" => RotwkCommand::Sacrifice { target: target()? },
        "ToggleWeapon" => {
            let slot = |name: &str| -> Result<u8, String> { number(f, name) };
            RotwkCommand::ToggleWeapon {
                toggles: [slot("first")?, slot("second")?, slot("third")?],
                lead: option(field(f, "lead")?, entity)?,
            }
        }
        "ToggleHoldFire" => RotwkCommand::ToggleHoldFire {
            lead: option(field(f, "lead")?, entity)?,
        },
        "MoveTo" => RotwkCommand::MoveTo {
            x: number(f, "x")?,
            y: number(f, "y")?,
            queue: bool_field(f, "queue"),
        },
        "AttackObject" => RotwkCommand::AttackObject { target: target()? },
        "AttackMove" => RotwkCommand::AttackMove {
            x: number(f, "x")?,
            y: number(f, "y")?,
            queue: bool_field(f, "queue"),
        },
        "ForceAttackObject" => RotwkCommand::ForceAttackObject { target: target()? },
        "Stop" => RotwkCommand::Stop,
        "ToggleHordeFormation" => RotwkCommand::ToggleHordeFormation,
        "AttackGround" => RotwkCommand::AttackGround {
            slot: number(f, "slot")?,
            x: number(f, "x")?,
            y: number(f, "y")?,
        },
        "WakeAutoPickUp" => RotwkCommand::WakeAutoPickUp {
            option: number(f, "option")?,
        },
        "QueueUnit" => RotwkCommand::QueueUnit {
            button: number(f, "button")?,
        },
        "CancelProduction" => RotwkCommand::CancelProduction,
        "CancelBuild" => RotwkCommand::CancelBuild {
            button: number(f, "button")?,
        },
        "CancelHero" => RotwkCommand::CancelHero {
            button: number(f, "button")?,
        },
        "ObjectUpgrade" => RotwkCommand::ObjectUpgrade {
            button: number(f, "button")?,
        },
        "PlayerUpgrade" => RotwkCommand::PlayerUpgrade {
            button: number(f, "button")?,
        },
        "FireSpecialPower" => RotwkCommand::FireSpecialPower {
            button: number(f, "button")?,
            target: option(field(f, "target")?, entity)?,
            at: option(field(f, "at")?, point)?,
        },
        "RecruitHero" => RotwkCommand::RecruitHero {
            button: number(f, "button")?,
        },
        "PurchaseScience" => RotwkCommand::PurchaseScience {
            science: number(f, "science")?,
        },
        "CastSpell" => RotwkCommand::CastSpell {
            button: number(f, "button")?,
            target: option(field(f, "target")?, entity)?,
            at: option(field(f, "at")?, point)?,
        },
        "CaptureObject" => RotwkCommand::CaptureObject { target: target()? },
        "Enter" => RotwkCommand::Enter { target: target()? },
        "Exit" => RotwkCommand::Exit {
            passenger: option(field(f, "passenger")?, entity)?,
        },
        "Sell" => RotwkCommand::Sell,
        "StartSelfRepair" => RotwkCommand::StartSelfRepair,
        "CancelUpgrade" => RotwkCommand::CancelUpgrade {
            button: number(f, "button")?,
            player: number(f, "player")?,
        },
        "DebugSkillPoints" => RotwkCommand::DebugSkillPoints {
            points: number(f, "points")?,
        },
        "DebugSpellPoints" => RotwkCommand::DebugSpellPoints {
            points: number(f, "points")?,
        },
        "DebugExperience" => RotwkCommand::DebugExperience {
            points: number(f, "points")?,
        },
        "DebugMoney" => RotwkCommand::DebugMoney {
            amount: number(f, "amount")?,
        },
        "DebugKill" => RotwkCommand::DebugKill,
        "DebugOath" => RotwkCommand::DebugOath,
        "ResetShroud" => RotwkCommand::ResetShroud {
            use_shroud: number(f, "use_shroud")?,
        },
        "Construct" => RotwkCommand::Construct {
            kind: number(f, "kind")?,
            x: number(f, "x")?,
            y: number(f, "y")?,
            angle: number(f, "angle")?,
        },
        "FoundationConstruct" => RotwkCommand::FoundationConstruct {
            button: number(f, "button")?,
        },
        "WallSpan" => RotwkCommand::WallSpan {
            option: number(f, "option")?,
            x: number(f, "x")?,
            y: number(f, "y")?,
        },
        "CancelConstruction" => RotwkCommand::CancelConstruction,
        "DebugSpawnHorde" => RotwkCommand::DebugSpawnHorde {
            recipe: number(f, "recipe")?,
            x: number(f, "x")?,
            y: number(f, "y")?,
            facing: number(f, "facing")?,
        },
        "DebugSpawnUnit" => RotwkCommand::DebugSpawnUnit {
            buildable: number(f, "buildable")?,
            x: number(f, "x")?,
            y: number(f, "y")?,
            facing: number(f, "facing")?,
        },
        "DebugSpawnStructure" => RotwkCommand::DebugSpawnStructure {
            kind: number(f, "kind")?,
            x: number(f, "x")?,
            y: number(f, "y")?,
            angle: number(f, "angle")?,
        },
        "MoveToFormation" => RotwkCommand::MoveToFormation {
            x: number(f, "x")?,
            y: number(f, "y")?,
            dx: number(f, "dx")?,
            dy: number(f, "dy")?,
            attack: number(f, "attack")?,
        },
        "SetUiPanelOpen" => RotwkCommand::SetUiPanelOpen {
            panel: match field(f, "panel")? {
                Value::Word(word) if word == "SpellStore" => crate::rules::UiPanel::SpellStore,
                Value::Word(word) if word == "Objectives" => crate::rules::UiPanel::Objectives,
                Value::Word(word) if word == "StanceSubMenu" => {
                    crate::rules::UiPanel::StanceSubMenu
                }
                other => return Err(format!("panel: unknown panel {other:?}")),
            },
            open: number(f, "open")?,
        },
        "CameraScrolled" => RotwkCommand::CameraScrolled {
            distance: number(f, "distance")?,
        },
        "CameraReset" => RotwkCommand::CameraReset,
        "ToggleAutoCast" => RotwkCommand::ToggleAutoCast {
            button: number(f, "button")?,
        },
        "SetRallyPoint" => RotwkCommand::SetRallyPoint {
            structures: entities(field(f, "structures")?)?,
            x: number(f, "x")?,
            y: number(f, "y")?,
            global: number(f, "global")?,
            target: option(field(f, "target")?, entity)?,
        },
        other => return Err(format!("the replay runner does not read {other} yet")),
    })
}

fn debug_command(value: &Value) -> Result<DebugCommand, String> {
    let (name, f): (&str, &[(String, Value)]) = match value {
        Value::Word(name) => (name.as_str(), &[]),
        Value::Struct(name, fields) => (name.as_str(), fields.as_slice()),
        other => return Err(format!("not a debug command: {other:?}")),
    };
    Ok(match name {
        "Spawn" => DebugCommand::Spawn {
            kind: number(f, "kind")?,
            x: number(f, "x")?,
            y: number(f, "y")?,
            facing: number(f, "facing")?,
            role: match field(f, "role")? {
                Value::Word(role) if role == "Normal" => SpawnRole::Normal,
                Value::Word(role) if role == "Dummy" => SpawnRole::Dummy,
                Value::Word(role) if role == "FightingDummy" => SpawnRole::FightingDummy,
                other => return Err(format!("role: {other:?}")),
            },
        },
        "LevelUp" => DebugCommand::LevelUp {
            quarters: number(f, "quarters")?,
        },
        "ResetPowers" => DebugCommand::ResetPowers,
        "Damage" => DebugCommand::Damage {
            percent: number(f, "percent")?,
        },
        "DummiesInvincible" => DebugCommand::DummiesInvincible {
            on: number(f, "on")?,
        },
        "Kill" => DebugCommand::Kill {
            target: debug_target(field(f, "target")?)?,
        },
        "Indestructible" => DebugCommand::Indestructible {
            target: debug_target(field(f, "target")?)?,
            on: number(f, "on")?,
        },
        "Complete" => DebugCommand::Complete {
            target: debug_target(field(f, "target")?)?,
        },
        "Capture" => DebugCommand::Capture {
            target: debug_target(field(f, "target")?)?,
        },
        "Upgrade" => DebugCommand::Upgrade {
            target: debug_target(field(f, "target")?)?,
            upgrade: match field(f, "upgrade")? {
                Value::Text(text) => text.clone(),
                other => return Err(format!("upgrade: {other:?}")),
            },
        },
        "Teleport" => DebugCommand::Teleport {
            target: debug_target(field(f, "target")?)?,
            x: number(f, "x")?,
            y: number(f, "y")?,
        },
        "KillBy" => DebugCommand::KillBy {
            target: debug_target(field(f, "target")?)?,
            killer: debug_target(field(f, "killer")?)?,
        },
        other => {
            return Err(format!(
                "the replay runner does not read Debug({other}) yet"
            ))
        }
    })
}

fn debug_target(value: &Value) -> Result<DebugTarget, String> {
    Ok(match value {
        Value::Word(word) if word == "All" => DebugTarget::All,
        Value::Tuple(Some(name), items) => match (name.as_str(), items.as_slice()) {
            ("Named", [Value::Text(text)]) => DebugTarget::Named(text.clone()),
            ("Team", [Value::Text(text)]) => DebugTarget::Team(text.clone()),
            ("Player", [Value::Text(text)]) => DebugTarget::Player(text.clone()),
            _ => return Err(format!("not a debug target: {value:?}")),
        },
        Value::Struct(name, f) if name == "Near" => DebugTarget::Near {
            kind: number(f, "kind")?,
            x: number(f, "x")?,
            y: number(f, "y")?,
        },
        Value::Struct(name, f) if name == "Kind" => DebugTarget::Kind {
            kind: number(f, "kind")?,
            side: option(field(f, "side")?, |value| match value {
                Value::Text(text) => Ok(text.clone()),
                other => Err(format!("side: {other:?}")),
            })?,
        },
        other => return Err(format!("not a debug target: {other:?}")),
    })
}

#[cfg(test)]
mod tests;
