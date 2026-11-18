mod alliance;
mod ambient;
mod ambient_stream;
mod announcer;
mod apt_overlay;
mod army;
mod audio;
mod base_install;
mod button_flash;
mod cah_edit;
mod cah_examine;
mod cah_picture;
mod cah_scene;
mod cah_screen;
mod cah_view;
mod camera_area;
mod camera_limits;
mod campaign;
mod campaign_dev;
mod campaign_flow;
mod campaign_save;
mod campaign_still;
mod capture;
mod castle;
mod cinematic;
mod claims;
mod combat;
mod combat_facts;
mod command_bar;
mod construction;
mod crash;
mod create_a_hero;
mod cursors;
mod death_phase;
mod defeat;
mod diag;
mod drive;
mod economy;
mod end_game;
mod experience;
mod floating_text;
mod forest;
mod fx;
mod fx_present;
mod game;
mod game_data;
mod game_mode;
mod ghosts;
mod goal_cells;
mod handicap;
mod help_box;
mod heroes;
mod horde;
mod horde_trail;
mod hud;
mod in_game_menu;
mod intro;
mod letterbox;
mod level_up;
mod list_sort;
mod load_screen;
mod loading_ring;
mod lobby;
mod looks;
mod map_keys;
mod map_players;
mod mapcache;
mod match_loader;
mod match_start;
mod match_stats;
mod melee;
mod military_caption;
mod mode;
mod movie_cmd;
mod movie_fade;
mod music_script;
mod named_timer;
mod notification_box;
mod notification_host;
mod objectives;
mod objectives_host;
mod obstacles;
mod options;
mod perches;
mod perf;
mod pick;
mod pointer;
mod portals;
mod post_match;
mod powers;
mod radar_events;
mod radar_jump;
mod radar_pings;
mod radial;
mod reflection;
mod render_perf;
mod render_state;
mod replace;
mod replay_run;
mod report;
mod rules;
mod save_load;
mod scene;
mod sciences;
mod script_camera;
mod script_pose;
mod selection_flash;
mod setup_grid;
mod setup_heroes;
mod setup_rules;
mod shell;
mod shell_game;
mod shell_load_screen;
mod shell_map;
mod shroud;
mod skirmish;
mod skirmish_ai;
mod snapshot;
mod spawn;
mod spell_book;
mod spell_state;
mod stress;
mod summons;
mod tdfx;
mod templates;
mod test_controls;
pub mod title;
mod transition;
mod tutorial;
mod ui_audio;
mod ui_resources;
mod unit;
mod user_data;
mod user_options;
mod validate;
mod view3d;
mod visuals;
mod voice;
mod voice_chain;
mod wall_layer;
mod wall_meshes;
mod weapon_use;

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use mint_app::{App, InputEvent, RtsCamera, WindowConfig};
use mint_render::{Camera, Renderer, Scene, ScreenDraw, Vec3};
use mint_sage_big::BigArchive;
use mint_vfs::Vfs;

use crate::game::Game;
use crate::scene::load_match_scene;

struct Options {
    primary: PathBuf,
    extra_installs: Vec<PathBuf>,
    map: Option<String>,
    faction: Option<String>,
    opponent: Option<String>,
    more_opponents: Vec<String>,
    opponent_ai: OpponentAi,
    player_ai: bool,
    run_match: Option<f64>,
    stress: Option<u32>,
    focus_player: Option<u8>,
    screenshot: Option<PathBuf>,
    shroud: Option<String>,
    shroud_viewer: Option<u8>,
    queue: u32,
    ticks: u32,
    hash_every: Option<u32>,
    order: Option<(f32, f32)>,
    look_at: Option<(f32, f32)>,
    particle_flipbook: Option<mint_sage_scene::FlipbookMode>,
    preview: Option<(String, f32, f32)>,
    construct: Option<(String, f32, f32, f32)>,
    search: Option<String>,
    smoothing: Option<String>,
    avoidance: Option<String>,
    fight: bool,
    fight_nearest: bool,
    view: Option<f32>,
    blend: Option<f32>,
    trace: bool,
    check_passability: bool,
    check_stuck: bool,
    frames: u32,
    tick_per_frame: bool,
    reflection: Option<String>,
    water_detail: Option<String>,
    sun: Option<String>,
    shadow_filter: Option<String>,
    shadow_cascades: Option<bool>,
    contact_shadows: Option<bool>,
    model_lights: Option<String>,
    pca_gate: Option<String>,
    pca_rate: Option<String>,
    image_modes: Option<String>,
    pitch: Option<f32>,
    waves: Option<String>,
    passability: Option<String>,
    obstacles: bool,
    select_menu: Option<String>,
    research: Vec<String>,
    research_at: u32,
    ui_frames: u32,
    ui_deselect_after: Option<u32>,
    radial_press: Option<usize>,
    apt_overlay: bool,
    shell_cli: ShellCli,
    scale_mode: mint_sage_apt_viewer::ScaleMode,
    text_anchor: mint_sage_apt_viewer::TextAnchor,
    size: Option<(u32, u32)>,
    shell_screenshot: Option<(String, PathBuf)>,
    army: Option<u32>,
    spawn: Option<String>,
    spawn_buildings: Option<String>,
    cah_hero: Option<String>,
    dummies: Option<String>,
    dummies_attack: bool,
    money: Option<i64>,
    control: Option<String>,
    drive: Option<String>,
    drive_dt: std::time::Duration,
    campaign_dev: Option<crate::campaign_dev::CampaignDev>,
    tutorial_dev: Option<crate::tutorial::TutorialKind>,
    ini_census: Option<PathBuf>,
    replay: Option<PathBuf>,
}

const DEMO_ORDER_DISTANCE: f32 = 300.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpponentAi {
    On,
    Off,
    Hold,
}

impl OpponentAi {
    const fn is_ai(self) -> bool {
        matches!(self, Self::On)
    }
}

const MAX_TEXT_SETTLE_PASSES: u32 = 16;

const UI_FRAME: std::time::Duration = std::time::Duration::from_nanos(33_333_334);

const DEFAULT_UI_FRAMES: u32 = 90;

fn parse_args() -> Result<Options, String> {
    let mut args = std::env::args().skip(1).peekable();
    let mut primary = None;
    let mut extra_installs = Vec::new();
    let mut map = None;
    let mut faction = None;
    let mut opponent = None;
    let mut more_opponents = Vec::new();
    let mut opponent_ai = None;
    let mut player_ai = false;
    let mut army = None;
    let mut spawn = None;
    let mut spawn_buildings = None;
    let mut cah_hero = None;
    let mut dummies = None;
    let mut dummies_attack = false;
    let mut run_match = None;
    let mut stress = None;
    let mut focus_player = None;
    let mut screenshot = None;
    let mut shroud = None;
    let mut shroud_viewer = None;
    let mut queue = 0u32;
    let mut ticks = 0u32;
    let mut hash_every = None;
    let mut order = None;
    let mut look_at = None;
    let mut particle_flipbook = None;
    let mut preview = None;
    let mut construct = None;
    let mut search = None;
    let mut smoothing = None;
    let mut avoidance = None;
    let mut reflection = None;
    let mut water_detail = None;
    let mut sun = None;
    let mut shadow_filter = None;
    let mut shadow_cascades = None;
    let mut contact_shadows = None;
    let mut model_lights = None;
    let mut pca_gate = None;
    let mut pca_rate = None;
    let mut image_modes = None;
    let mut pitch = None;
    let mut waves = None;
    let mut passability = None;
    let mut frames = 1u32;
    let mut tick_per_frame = false;
    let mut fight = false;
    let mut fight_nearest = false;
    let mut obstacles = false;
    let mut trace = false;
    let mut check_passability = false;
    let mut check_stuck = false;
    let mut view = None;
    let mut blend = None;
    let mut select_menu = None;
    let mut research: Vec<String> = Vec::new();
    let mut research_at = 0u32;
    let mut ui_frames = DEFAULT_UI_FRAMES;
    let mut ui_deselect_after = None;
    let mut radial_press = None;
    let mut apt_overlay = true;
    let mut shell_cli = ShellCli::default();
    let mut scale_mode = mint_sage_apt_viewer::ScaleMode::default();
    let mut text_anchor = mint_sage_apt_viewer::TextAnchor::default();
    let mut size = None;
    let mut shell_screenshot = None;
    let mut control = None;
    let mut drive = None;
    let mut drive_dt = std::time::Duration::from_micros(16_667);
    let mut money = None;
    let mut ini_census: Option<PathBuf> = None;
    let mut replay = None;
    let (mut campaign_name, mut campaign_mission, mut campaign_difficulty) = (None, None, None);
    let mut tutorial_flag = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--campaign" => {
                campaign_name = Some(
                    args.next()
                        .ok_or("--campaign wants a LinearCampaign name")?,
                );
            }
            "--tutorial" => {
                tutorial_flag = Some(args.next().ok_or("--tutorial wants basic or advanced")?);
            }
            "--mission" => {
                campaign_mission = Some(args.next().ok_or("--mission wants a name or number")?);
            }
            "--difficulty" => {
                campaign_difficulty = Some(args.next().ok_or("--difficulty wants a level")?);
            }
            "--install" => {
                extra_installs.push(PathBuf::from(args.next().ok_or("--install wants a path")?));
            }
            "--map" => {
                map = Some(args.next().ok_or("--map wants a name")?);
            }
            "--faction" => {
                faction = Some(args.next().ok_or("--faction wants a PlayerTemplate name")?)
            }
            "--opponent" => {
                let name = args
                    .next()
                    .ok_or("--opponent wants a PlayerTemplate name")?;
                if opponent.is_none() {
                    opponent = Some(name);
                } else {
                    more_opponents.push(name);
                }
            }
            "--opponent-ai" => {
                let value = args.next().ok_or("--opponent-ai wants on, off or hold")?;
                opponent_ai = Some(match value.as_str() {
                    "on" => OpponentAi::On,
                    "off" => OpponentAi::Off,
                    "hold" => OpponentAi::Hold,
                    other => {
                        return Err(format!("--opponent-ai wants on, off or hold, got {other}"))
                    }
                });
            }
            "--player-ai" => {
                let value = args.next().ok_or("--player-ai wants on or off")?;
                player_ai = match value.as_str() {
                    "on" => true,
                    "off" => false,
                    other => return Err(format!("--player-ai wants on or off, got {other}")),
                };
            }
            "--army" => {
                let count = match args.peek() {
                    Some(next) if next.parse::<u32>().is_ok() => {
                        args.next().and_then(|value| value.parse().ok())
                    }
                    _ => None,
                };
                army = Some(count.unwrap_or(crate::army::DEFAULT_ARMY_PER_SIDE));
            }
            "--spawn" => {
                spawn = Some(args.next().ok_or("--spawn wants a spec, e.g. \"2 Foo\"")?);
            }
            "--cah-hero" => {
                cah_hero = Some(args.next().ok_or("--cah-hero wants a system hero id")?);
            }
            "--spawn-buildings" => {
                spawn_buildings = Some(
                    args.next()
                        .ok_or("--spawn-buildings wants a spec, e.g. \"2 Foo\"")?,
                );
            }
            "--dummies" => {
                let spec = match args.peek() {
                    Some(next) if !next.starts_with("--") => args.next(),
                    _ => None,
                };
                dummies = Some(spec.unwrap_or_default());
            }
            "--dummiesAttack" => dummies_attack = true,
            "--money" => {
                money = Some(parse_money(
                    &args.next().ok_or("--money wants an amount, e.g. 10000")?,
                )?);
            }
            "--run-match" => {
                run_match = Some(
                    args.next()
                        .ok_or("--run-match wants a number of minutes")?
                        .parse::<f64>()
                        .map_err(|_| "--run-match wants a number of minutes")?,
                );
            }
            "--stress" => {
                stress = Some(
                    args.next()
                        .ok_or("--stress wants a total unit count")?
                        .parse::<u32>()
                        .map_err(|_| "--stress wants a total unit count")?,
                );
            }
            "--focus-player" => {
                focus_player = Some(
                    args.next()
                        .ok_or("--focus-player wants a player number")?
                        .parse()
                        .map_err(|_| "--focus-player wants a player number")?,
                );
            }
            "--shroud" => shroud = Some(args.next().ok_or("--shroud wants off or on")?),
            "--shroud-viewer" => {
                shroud_viewer = Some(
                    args.next()
                        .ok_or("--shroud-viewer wants a player number")?
                        .parse()
                        .map_err(|_| "--shroud-viewer wants a player number")?,
                );
            }
            "--queue" => {
                queue = args
                    .next()
                    .ok_or("--queue wants a count")?
                    .parse()
                    .map_err(|_| "--queue wants a count")?;
            }
            "--screenshot" => {
                screenshot = Some(PathBuf::from(
                    args.next().ok_or("--screenshot wants a path")?,
                ));
            }
            "--shell-screenshot" => {
                let value = args
                    .next()
                    .ok_or("--shell-screenshot wants <screen>,<file>")?;
                let (screen, file) = value
                    .split_once(',')
                    .ok_or("--shell-screenshot wants <screen>,<file>")?;
                shell_screenshot = Some((screen.to_owned(), PathBuf::from(file)));
            }
            "--ticks" => {
                let value = args.next().ok_or("--ticks wants a number")?;
                ticks = value
                    .parse()
                    .map_err(|_| format!("--ticks wants a number, got {value}"))?;
            }
            "--hash-every" => {
                let value = args.next().ok_or("--hash-every wants a number")?;
                hash_every = Some(
                    value
                        .parse::<u32>()
                        .ok()
                        .filter(|n| *n > 0)
                        .ok_or_else(|| {
                            format!("--hash-every wants a positive number, got {value}")
                        })?,
                );
            }
            "--order" => {
                let value = args.next().ok_or("--order wants x,y")?;
                let (x, y) = value
                    .split_once(',')
                    .ok_or_else(|| format!("--order wants x,y, got {value}"))?;
                let parse = |part: &str| -> Result<f32, String> {
                    part.trim()
                        .parse()
                        .map_err(|_| format!("--order wants numbers, got {value}"))
                };
                order = Some((parse(x)?, parse(y)?));
            }
            "--look-at" => {
                let value = args.next().ok_or("--look-at wants x,y")?;
                let (x, y) = value
                    .split_once(',')
                    .ok_or_else(|| format!("--look-at wants x,y, got {value}"))?;
                let parse = |part: &str| -> Result<f32, String> {
                    part.trim()
                        .parse()
                        .map_err(|_| format!("--look-at wants numbers, got {value}"))
                };
                look_at = Some((parse(x)?, parse(y)?));
            }
            "--preview" => {
                let value = args.next().ok_or("--preview wants template,x,y")?;
                let parts: Vec<&str> = value.split(',').collect();
                let [name, x, y] = parts.as_slice() else {
                    return Err(format!("--preview wants template,x,y, got {value}"));
                };
                let parse = |part: &str| -> Result<f32, String> {
                    part.trim()
                        .parse()
                        .map_err(|_| format!("--preview wants numbers, got {value}"))
                };
                preview = Some(((*name).to_owned(), parse(x)?, parse(y)?));
            }
            "--particle-flipbook" => {
                let value = args.next().ok_or("--particle-flipbook wants a rung")?;
                particle_flipbook = Some(mint_sage_scene::FlipbookMode::parse(&value).ok_or_else(
                    || format!("--particle-flipbook wants off|life|life-blend|frame|blend|retail, got {value}"),
                )?);
            }
            "--construct" => {
                let value = args
                    .next()
                    .ok_or("--construct wants template,x,y[,angle]")?;
                let parts: Vec<&str> = value.split(',').collect();
                let [name, x, y, rest @ ..] = parts.as_slice() else {
                    return Err(format!(
                        "--construct wants template,x,y[,angle], got {value}"
                    ));
                };
                let parse = |part: &str| -> Result<f32, String> {
                    part.trim()
                        .parse()
                        .map_err(|_| format!("--construct wants numbers, got {value}"))
                };
                let angle = match rest {
                    [angle] => parse(angle)?,
                    [] => 0.0,
                    _ => {
                        return Err(format!(
                            "--construct wants template,x,y[,angle], got {value}"
                        ))
                    }
                };
                construct = Some(((*name).to_owned(), parse(x)?, parse(y)?, angle));
            }
            "--select-menu" => {
                let value = args
                    .next()
                    .ok_or("--select-menu wants \"porter\" or \"producer\"")?;
                if value != "porter" && value != "producer" {
                    return Err(format!(
                        "--select-menu wants \"porter\" or \"producer\", got {value}"
                    ));
                }
                select_menu = Some(value);
            }
            "--research" => {
                let value = args
                    .next()
                    .ok_or("--research wants <Upgrade>[,<Upgrade>...]")?;
                research.extend(
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                        .map(str::to_owned),
                );
            }
            "--research-at" => {
                research_at = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("--research-at wants a tick count")?;
            }
            "--ui-deselect-after" => {
                ui_deselect_after = Some(
                    args.next()
                        .and_then(|v| v.parse().ok())
                        .ok_or("--ui-deselect-after wants a whole number")?,
                );
            }
            "--radial-press" => {
                radial_press = Some(
                    args.next()
                        .and_then(|v| v.parse().ok())
                        .ok_or("--radial-press wants a ring position")?,
                );
            }
            "--ui-frames" => {
                ui_frames = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("--ui-frames wants a whole number")?;
            }
            "--no-shell-map" => shell_cli.off = true,
            "--shell-map" => {
                shell_cli.name = Some(args.next().ok_or("--shell-map wants a map path")?);
            }
            "--shell-map-on" => shell_cli.force_on = true,
            "--intro" => shell_cli.intro = Some(true),
            "--no-intro" => shell_cli.intro = Some(false),
            "--apt-overlay" => {
                let value = args.next().ok_or("--apt-overlay wants on or off")?;
                apt_overlay = match value.as_str() {
                    "on" => true,
                    "off" => false,
                    other => return Err(format!("--apt-overlay wants on or off, got {other}")),
                };
            }
            "--ui-scale" => {
                let value = args.next().ok_or("--ui-scale wants stretch or anchored")?;
                scale_mode = mint_sage_apt_viewer::ScaleMode::parse(&value)
                    .ok_or_else(|| format!("--ui-scale wants stretch or anchored, got {value}"))?;
            }
            "--text-anchor" => {
                text_anchor = match args.next().as_deref() {
                    Some("centred" | "centered") => mint_sage_apt_viewer::TextAnchor::Centred,
                    Some("top") => mint_sage_apt_viewer::TextAnchor::Top,
                    Some("field") => mint_sage_apt_viewer::TextAnchor::Field,
                    _ => return Err("--text-anchor wants field, centred or top".to_owned()),
                };
            }
            "--size" => {
                let text = args.next().ok_or("--size wants WxH")?;
                let (w, h) = text
                    .split_once(['x', 'X'])
                    .ok_or_else(|| format!("--size wants WxH, got {text}"))?;
                size = Some((
                    w.parse().map_err(|_| format!("bad width: {w}"))?,
                    h.parse().map_err(|_| format!("bad height: {h}"))?,
                ));
            }
            "--fight" => fight = true,
            "--fight-nearest" => fight_nearest = true,
            "--trace" => trace = true,
            "--check-passability" => check_passability = true,
            "--check-stuck" => check_stuck = true,
            "--blend" => {
                let value = args.next().ok_or("--blend wants a number from 0 to 1")?;
                blend = Some(
                    value
                        .parse()
                        .map_err(|_| format!("--blend wants a number, got {value}"))?,
                );
            }
            "--view" => {
                let value = args.next().ok_or("--view wants a distance")?;
                view = Some(
                    value
                        .parse()
                        .map_err(|_| format!("--view wants a number, got {value}"))?,
                );
            }
            "--search" => {
                search = Some(args.next().ok_or("--search wants a name")?);
            }
            "--smoothing" => {
                smoothing = Some(args.next().ok_or("--smoothing wants a name")?);
            }
            "--avoidance" => {
                avoidance = Some(args.next().ok_or("--avoidance wants off or separation")?);
            }
            "--reflection" => {
                reflection = Some(args.next().ok_or("--reflection wants env or planar")?);
            }
            "--pitch" => {
                let value = args.next().ok_or("--pitch wants degrees")?;
                pitch = Some(
                    value
                        .parse::<f32>()
                        .map_err(|_| format!("--pitch wants degrees, got {value}"))?,
                );
            }
            "--pca-gate" => pca_gate = Some(args.next().ok_or("--pca-gate wants line|band|open")?),
            "--pca-rate" => {
                pca_rate = Some(
                    args.next()
                        .ok_or("--pca-rate wants per-logic-frame|per-second")?,
                )
            }
            "--image-modes" => {
                image_modes = Some(
                    args.next()
                        .ok_or("--image-modes wants matte|unset-solid|alpha-opaque|radial-disc")?,
                )
            }
            "--sun" => sun = Some(args.next().ok_or("--sun wants stored|mirrored-azimuth")?),
            "--model-lights" => {
                model_lights = Some(args.next().ok_or("--model-lights wants retail|terrain")?);
            }
            "--shadow-filter" => {
                shadow_filter = Some(
                    args.next()
                        .ok_or("--shadow-filter wants tent|retail|contact|wide")?,
                );
            }
            "--shadow-cascades" | "--contact-shadows" => {
                let on = match args.next().as_deref() {
                    Some("on") => true,
                    Some("off") => false,
                    _ => return Err(format!("{arg} wants on|off")),
                };
                if arg == "--shadow-cascades" {
                    shadow_cascades = Some(on);
                } else {
                    contact_shadows = Some(on);
                }
            }
            "--water-detail" => {
                water_detail = Some(
                    args.next()
                        .ok_or("--water-detail wants ultra|high|medium|low")?,
                );
            }
            "--waves" => {
                waves = Some(args.next().ok_or("--waves wants a rung")?);
            }
            "--passability" => {
                passability = Some(args.next().ok_or("--passability wants a rung")?);
            }
            "--obstacles" => obstacles = true,
            "--control" => {
                let value = args
                    .next()
                    .ok_or("--control wants \"all\" or a comma list")?;
                crate::game::ControlSet::parse(&value)?;
                control = Some(value);
            }
            "--drive" => {
                let script = args.next().ok_or("--drive wants a script")?;
                drive = Some(match script.strip_prefix('@') {
                    Some(path) => std::fs::read_to_string(path)
                        .map_err(|e| format!("--drive @{path}: {e}"))?,
                    None => script,
                });
            }
            "--drive-dt" => {
                let ms: f64 = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .filter(|ms: &f64| *ms > 0.0 && *ms < 1000.0)
                    .ok_or("--drive-dt wants milliseconds")?;
                drive_dt = std::time::Duration::from_secs_f64(ms / 1000.0);
            }
            "--ini-census" => {
                ini_census = Some(PathBuf::from(
                    args.next().ok_or("--ini-census wants an output path")?,
                ));
            }
            "--replay" => replay = Some(PathBuf::from(args.next().ok_or("--replay wants a path")?)),
            "--tick-per-frame" => tick_per_frame = true,
            "--frames" => {
                let value = args.next().ok_or("--frames wants a count")?;
                frames = value
                    .parse()
                    .map_err(|_| format!("--frames wants a count, got {value}"))?;
            }
            other if primary.is_none() && !other.starts_with("--") => {
                primary = Some(PathBuf::from(other));
            }
            other => return Err(format!("unknown option {other}")),
        }
    }
    let primary = primary.ok_or_else(|| {
        format!(
            "usage: {} <install dir> [--install <dir>] [--map <name>] \
             [--faction <PlayerTemplate> --opponent <PlayerTemplate>] \
             [--screenshot <file>] [--focus-player <n>] [--shroud off|on] \
             [--shroud-viewer <n>] [--queue <n>]",
            crate::title::current().binary
        )
    })?;
    let extra_installs = crate::base_install::with_base_game(
        &primary,
        &extra_installs,
        crate::title::current().base_game.as_ref(),
    );
    let tutorial_dev =
        crate::campaign_dev::parse_tutorial(tutorial_flag.as_deref(), campaign_name.is_some())?;
    let campaign_dev = crate::campaign_dev::CampaignDev::from_flags(
        campaign_name,
        campaign_mission,
        campaign_difficulty,
    )?;
    Ok(Options {
        campaign_dev,
        tutorial_dev,
        primary,
        extra_installs,
        map,
        faction,
        opponent,
        more_opponents,
        opponent_ai: opponent_ai.unwrap_or(if dummies.is_some() {
            OpponentAi::Hold
        } else {
            OpponentAi::On
        }),
        player_ai,
        run_match,
        stress,
        focus_player,
        screenshot,
        shroud,
        shroud_viewer,
        queue,
        ticks,
        hash_every,
        order,
        look_at,
        particle_flipbook,
        preview,
        construct,
        search,
        smoothing,
        avoidance,
        reflection,
        water_detail,
        sun,
        shadow_filter,
        shadow_cascades,
        contact_shadows,
        model_lights,
        pca_gate,
        pca_rate,
        image_modes,
        pitch,
        waves,
        passability,
        frames,
        tick_per_frame,
        fight,
        fight_nearest,
        obstacles,
        trace,
        check_passability,
        check_stuck,
        view,
        blend,
        select_menu,
        research,
        research_at,
        ui_frames,
        ui_deselect_after,
        radial_press,
        apt_overlay,
        shell_cli,
        scale_mode,
        text_anchor,
        size,
        shell_screenshot,
        army,
        spawn,
        spawn_buildings,
        cah_hero,
        dummies,
        dummies_attack,
        money,
        control,
        drive,
        drive_dt,
        ini_census,
        replay,
    })
}

fn mount<'a>(installs: impl IntoIterator<Item = &'a Path>) -> Vfs {
    let mut vfs = Vfs::new();
    let mut archives = 0usize;
    for install in installs {
        let mut here = 0usize;
        for path in mint_sage_big::archives_under(install) {
            if let Ok(archive) = BigArchive::open(&path) {
                vfs.mount(Arc::new(archive));
                here = here.saturating_add(1);
            }
        }
        println!("{here} archive(s) mounted from {}", install.display());
        archives = archives.saturating_add(here);
    }
    println!("{archives} archive(s) mounted in all");
    vfs
}

pub(crate) fn load_game_data_or_exit(vfs: &Vfs) -> (crate::game_data::GameData, String) {
    match crate::game_data::load_game_data(vfs) {
        Ok(loaded) => loaded,
        Err(refusal) => {
            refusal.report();
            std::process::exit(2);
        }
    }
}

fn mount_primary_last(primary: &Path, extras: &[PathBuf]) -> Vfs {
    crate::user_data::note_install(primary);
    let extras = crate::base_install::with_base_game(
        primary,
        extras,
        crate::title::current().base_game.as_ref(),
    );
    let order: Vec<&Path> = extras
        .iter()
        .rev()
        .map(PathBuf::as_path)
        .chain(std::iter::once(primary))
        .collect();
    mount(order)
}

pub(crate) fn curated_maps(vfs: &Vfs, needle: Option<&str>) -> Vec<String> {
    let needle = needle.map(str::to_ascii_lowercase);
    let mut names: Vec<String> = vfs
        .mounts()
        .flat_map(|mount| mount.entries())
        .filter(|path| path.extension().is_some_and(|ext| ext == "map"))
        .filter(|path| {
            needle
                .as_ref()
                .is_none_or(|needle| path.as_str().contains(needle.as_str()))
        })
        .map(|path| path.as_str().to_owned())
        .collect();
    names.sort_by(|a, b| {
        let rank = |name: &str| u8::from(!name.starts_with("maps/"));
        (rank(a), a).cmp(&(rank(b), b))
    });
    names.dedup();
    names
}

fn check_kind_command(args: &[String]) -> ExitCode {
    let mut installs = Vec::new();
    let mut names = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--install" {
            match iter.next() {
                Some(path) => installs.push(PathBuf::from(path)),
                None => {
                    eprintln!("error: --install wants a path");
                    return ExitCode::FAILURE;
                }
            }
        } else if installs.is_empty() && names.is_empty() {
            installs.push(PathBuf::from(arg));
        } else {
            names.push(arg.clone());
        }
    }
    if installs.is_empty() || names.is_empty() {
        eprintln!(
            "usage: mint-rotwk --check-kind <install dir> [--install <dir>] <TemplateName> [<TemplateName> ...]"
        );
        return ExitCode::FAILURE;
    }
    let Some((primary, extras)) = installs.split_first() else {
        eprintln!("error: no install given");
        return ExitCode::FAILURE;
    };
    let vfs = mount_primary_last(primary, extras);
    let (data, line) = crate::load_game_data_or_exit(&vfs);
    println!("{line}");
    for name in &names {
        match data.templates.get(name) {
            Some(record) => println!(
                "{name}: SELECTABLE={}, KindOf={:?}",
                record.has_kind("SELECTABLE"),
                record.kind_of
            ),
            None => println!("{name}: no such template"),
        }
    }
    ExitCode::SUCCESS
}

fn check_weapon_fire_command(args: &[String]) -> ExitCode {
    let mut installs = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--install" {
            match iter.next() {
                Some(path) => installs.push(PathBuf::from(path)),
                None => {
                    eprintln!("error: --install wants a path");
                    return ExitCode::FAILURE;
                }
            }
        } else {
            installs.push(PathBuf::from(arg));
        }
    }
    if installs.is_empty() {
        eprintln!("usage: mint-rotwk --check-weapon-fire <install dir> [--install <dir>]");
        return ExitCode::FAILURE;
    }
    let Some((primary, extras)) = installs.split_first() else {
        eprintln!("error: no install given");
        return ExitCode::FAILURE;
    };
    let vfs = mount_primary_last(primary, extras);
    let (mut data, line) = crate::load_game_data_or_exit(&vfs);
    println!("{line}");
    let objects = data.object_index.take().unwrap_or_default();

    let mut total = 0usize;
    let mut object_names = BTreeSet::new();
    let mut power_ok = 0usize;
    let mut weapon_ok = 0usize;
    let mut anim_ok = 0usize;
    let mut rows: Vec<String> = Vec::new();

    for record in data.templates.iter() {
        for module in &record.declared_modules {
            if !module
                .name
                .eq_ignore_ascii_case("WeaponFireSpecialAbilityUpdate")
            {
                continue;
            }
            total += 1;
            object_names.insert(record.name.clone());

            let power = module
                .field("SpecialPowerTemplate")
                .map(|field| field.args.join(" "));
            let power_resolves = power
                .as_deref()
                .is_some_and(|name| data.special_powers.get(name).is_some());
            if power_resolves {
                power_ok = power_ok.saturating_add(1);
            } else {
                rows.push(format!(
                    "{}: SpecialPowerTemplate {power:?} does not resolve",
                    record.name
                ));
            }

            let weapon = module
                .field("SpecialWeapon")
                .map(|field| field.args.join(" "));
            let weapon_resolves = weapon
                .as_deref()
                .is_some_and(|name| data.combat.weapons.id(name).is_some());
            if weapon_resolves {
                weapon_ok = weapon_ok.saturating_add(1);
            } else {
                rows.push(format!(
                    "{}: SpecialWeapon {weapon:?} does not resolve",
                    record.name
                ));
            }

            let which = module.field("WhichSpecialWeapon").and_then(|f| f.numeric);
            match mint_sage_modules::special_weapon_condition(which) {
                Some(condition) => {
                    let has_state = objects.animation_states(&record.name).iter().any(|state| {
                        state
                            .conditions
                            .iter()
                            .any(|c| c.eq_ignore_ascii_case(&condition))
                    });
                    if has_state {
                        anim_ok = anim_ok.saturating_add(1);
                    } else {
                        rows.push(format!(
                            "{}: WhichSpecialWeapon {which:?} -> {condition}, no matching AnimationState",
                            record.name
                        ));
                    }
                }
                None => {
                    rows.push(format!(
                        "{}: WhichSpecialWeapon {which:?} names no SPECIAL_WEAPON_<1-5> state",
                        record.name
                    ));
                }
            }
        }
    }

    println!(
        "weapon-fire gate: {total} declaration(s) on {} object(s) - \
         SpecialPowerTemplate resolves {power_ok}/{total}, SpecialWeapon resolves \
         {weapon_ok}/{total}, WhichSpecialWeapon resolves an AnimationState {anim_ok}/{total}",
        object_names.len()
    );
    for row in &rows {
        println!("  {row}");
    }
    ExitCode::SUCCESS
}

fn check_toggle_mounted_command(args: &[String]) -> ExitCode {
    let mut installs = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--install" {
            match iter.next() {
                Some(path) => installs.push(PathBuf::from(path)),
                None => {
                    eprintln!("error: --install wants a path");
                    return ExitCode::FAILURE;
                }
            }
        } else {
            installs.push(PathBuf::from(arg));
        }
    }
    if installs.is_empty() {
        eprintln!("usage: mint-rotwk --check-toggle-mounted <install dir> [--install <dir>]");
        return ExitCode::FAILURE;
    }
    let Some((primary, extras)) = installs.split_first() else {
        eprintln!("error: no install given");
        return ExitCode::FAILURE;
    };
    let vfs = mount_primary_last(primary, extras);
    let (mut data, line) = crate::load_game_data_or_exit(&vfs);
    println!("{line}");
    let objects = data.object_index.take().unwrap_or_default();

    const MOUNTED: &str = "MOUNTED";

    let mut total = 0usize;
    let mut object_names = BTreeSet::new();
    let mut power_ok = 0usize;
    let mut state_ok = 0usize;
    let mut set_ok = 0usize;
    let mut template_ok = 0usize;
    let mut template_total = 0usize;
    let mut rows: Vec<String> = Vec::new();

    for record in data.templates.iter() {
        for module in &record.declared_modules {
            if !module
                .name
                .eq_ignore_ascii_case("ToggleMountedSpecialAbilityUpdate")
            {
                continue;
            }
            total += 1;
            object_names.insert(record.name.clone());

            let power = module
                .field("SpecialPowerTemplate")
                .map(|field| field.args.join(" "));
            let power_resolves = power
                .as_deref()
                .is_some_and(|name| data.special_powers.get(name).is_some());
            if power_resolves {
                power_ok = power_ok.saturating_add(1);
            } else {
                rows.push(format!(
                    "{}: SpecialPowerTemplate {power:?} does not resolve",
                    record.name
                ));
            }

            let has_state = objects
                .animation_states(&record.name)
                .iter()
                .any(|state| state.conditions.iter().any(|c| c == MOUNTED))
                || objects
                    .model_states(&record.name)
                    .iter()
                    .any(|state| state.conditions.iter().any(|c| c == MOUNTED));
            if has_state {
                state_ok = state_ok.saturating_add(1);
            } else {
                rows.push(format!(
                    "{}: no MOUNTED AnimationState or ModelConditionState",
                    record.name
                ));
            }

            let has_set = record
                .locomotor_sets
                .iter()
                .any(|set| set.condition == MOUNTED)
                || record
                    .weapon_sets
                    .iter()
                    .any(|set| set.conditions.iter().any(|c| c == MOUNTED))
                || record
                    .armor_sets
                    .iter()
                    .any(|set| set.conditions.iter().any(|c| c == MOUNTED));
            if has_set {
                set_ok = set_ok.saturating_add(1);
            } else {
                rows.push(format!(
                    "{}: no MOUNTED-conditioned LocomotorSet, WeaponSet or ArmorSet",
                    record.name
                ));
            }

            if let Some(target) = module.field("MountedTemplate").map(|f| f.args.join(" ")) {
                template_total += 1;
                if data.templates.get(&target).is_some() {
                    template_ok = template_ok.saturating_add(1);
                } else {
                    rows.push(format!(
                        "{}: MountedTemplate {target:?} does not resolve",
                        record.name
                    ));
                }
            }
        }
    }

    println!(
        "toggle-mounted gate: {total} declaration(s) on {} object(s) - \
         SpecialPowerTemplate resolves {power_ok}/{total}, a MOUNTED state exists \
         {state_ok}/{total}, a MOUNTED-conditioned set exists {set_ok}/{total}, \
         MountedTemplate resolves {template_ok}/{template_total}",
        object_names.len()
    );
    for row in &rows {
        println!("  {row}");
    }
    ExitCode::SUCCESS
}

fn check_grants_command(args: &[String]) -> ExitCode {
    let mut installs = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--install" {
            match iter.next() {
                Some(path) => installs.push(PathBuf::from(path)),
                None => {
                    eprintln!("error: --install wants a path");
                    return ExitCode::FAILURE;
                }
            }
        } else {
            installs.push(PathBuf::from(arg));
        }
    }
    if installs.is_empty() {
        eprintln!("usage: mint-rotwk --check-grants <install dir> [--install <dir>]");
        return ExitCode::FAILURE;
    }
    let Some((primary, extras)) = installs.split_first() else {
        eprintln!("error: no install given");
        return ExitCode::FAILURE;
    };
    let vfs = mount_primary_last(primary, extras);
    let (data, line) = crate::load_game_data_or_exit(&vfs);
    println!("{line}");

    let mut grant_total = 0usize;
    let mut grant_ok = 0usize;
    let mut grant_rows: Vec<String> = Vec::new();
    for record in data.templates.iter() {
        for module in &record.declared_modules {
            if !module.name.eq_ignore_ascii_case("GrantUpgradeCreate") {
                continue;
            }
            grant_total += 1;
            let Some(name) = module.field("UpgradeToGrant").map(|f| f.args.join(" ")) else {
                grant_rows.push(format!("{}: no UpgradeToGrant field", record.name));
                continue;
            };
            match data.upgrade_bits.bit(&name) {
                Some(bit) => {
                    grant_ok = grant_ok.saturating_add(1);
                    let kind = if data.upgrade_bits.is_player(bit) {
                        "PLAYER"
                    } else {
                        "OBJECT"
                    };
                    grant_rows.push(format!(
                        "{}: UpgradeToGrant {name} -> resolves, Type {kind}",
                        record.name
                    ));
                }
                None => {
                    grant_rows.push(format!(
                        "{}: UpgradeToGrant {name:?} does not resolve",
                        record.name
                    ));
                }
            }
        }
    }

    let mut initial_total = 0usize;
    let mut initial_ok = 0usize;
    let mut initial_rows: Vec<String> = Vec::new();
    for faction in data.players.iter() {
        for name in &faction.initial_upgrades {
            initial_total += 1;
            match data.upgrade_bits.bit(name) {
                Some(bit) => {
                    initial_ok = initial_ok.saturating_add(1);
                    let kind = if data.upgrade_bits.is_player(bit) {
                        "PLAYER"
                    } else {
                        "OBJECT"
                    };
                    initial_rows.push(format!(
                        "{}: InitialUpgrades {name} -> resolves, Type {kind}",
                        faction.name
                    ));
                }
                None => {
                    initial_rows.push(format!(
                        "{}: InitialUpgrades {name:?} does not resolve",
                        faction.name
                    ));
                }
            }
        }
    }

    println!(
        "grants gate: GrantUpgradeCreate UpgradeToGrant resolves {grant_ok}/{grant_total}, \
         InitialUpgrades resolves {initial_ok}/{initial_total}"
    );
    for row in &grant_rows {
        println!("  {row}");
    }
    for row in &initial_rows {
        println!("  {row}");
    }
    ExitCode::SUCCESS
}

fn check_cancel_fx_command(args: &[String]) -> ExitCode {
    let mut installs = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--install" {
            let Some(path) = iter.next() else {
                eprintln!("error: --install wants a path");
                return ExitCode::FAILURE;
            };
            installs.push(PathBuf::from(path));
        } else {
            installs.push(PathBuf::from(arg));
        }
    }
    let Some((primary, extras)) = installs.split_first() else {
        eprintln!("usage: mint-rotwk --check-cancel-fx <install dir> [--install <dir>]");
        return ExitCode::FAILURE;
    };
    let vfs = mount_primary_last(primary, extras);
    let (data, line) = crate::load_game_data_or_exit(&vfs);
    println!("{line}");

    let mut buildables: BTreeSet<String> = BTreeSet::new();
    for record in data.templates.iter() {
        for set in record
            .command_set
            .iter()
            .chain(record.upgraded_command_sets.iter())
        {
            for build in
                mint_sage_modules::dozer_constructs(set, &data.command_sets, &data.command_buttons)
            {
                buildables.insert(build.object.to_ascii_lowercase());
            }
        }
    }
    let initial = |module: &mint_sage_modules::CapturedModule, keyword: &str| -> Vec<String> {
        module
            .fields
            .iter()
            .filter(|field| field.keyword.eq_ignore_ascii_case(keyword))
            .filter(|field| {
                field
                    .args
                    .first()
                    .is_some_and(|phase| phase.eq_ignore_ascii_case("INITIAL"))
            })
            .flat_map(|field| field.args.iter().skip(1).cloned())
            .collect()
    };
    let mut by_fx: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut other_die: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    let (mut resolved, mut with_fx, mut none, mut muxed) = (0usize, 0usize, 0usize, 0usize);
    let mut rows = Vec::new();
    for name in &buildables {
        let Some(record) = data.templates.get(name) else {
            rows.push(format!("{name}: not a template"));
            continue;
        };
        resolved += 1;
        let mut fired: Vec<String> = Vec::new();
        for module in &record.declared_modules {
            match module.name.as_str() {
                "StructureCollapseUpdate" => {
                    fired.extend(initial(module, "FXList"));
                    fired.extend(
                        initial(module, "OCL")
                            .into_iter()
                            .map(|n| format!("OCL {n}")),
                    );
                    if ["DeathTypes", "RequiredStatus", "ExemptStatus"]
                        .iter()
                        .any(|keyword| module.field(keyword).is_some())
                    {
                        muxed += 1;
                    }
                }
                "FXListDie" => {
                    if let Some(field) = module.field("DeathFX") {
                        fired.extend(field.args.iter().cloned());
                    }
                }
                "SlowDeathBehavior" => {
                    fired.extend(initial(module, "FX"));
                    fired.extend(
                        initial(module, "OCL")
                            .into_iter()
                            .map(|n| format!("OCL {n}")),
                    );
                }
                other if other.ends_with("Die") => {
                    *other_die.entry(other.to_owned()).or_default() += 1;
                }
                _ => {}
            }
        }
        if fired.is_empty() {
            none += 1;
        } else {
            with_fx += 1;
        }
        let key = if fired.is_empty() {
            String::from("(none)")
        } else {
            fired.join(" + ")
        };
        *by_fx.entry(key.clone()).or_default() += 1;
        rows.push(format!("{}: {key}", record.name));
    }
    println!(
        "cancel fx gate: {} DOZER_CONSTRUCT target(s), {resolved} resolve; {with_fx} fire an \
         INITIAL list on cancel, {none} fire none; {muxed} collapse module(s) write a die-mux field",
        buildables.len()
    );
    for (fx, count) in &by_fx {
        println!("  {count:>4} x {fx}");
    }
    for (module, count) in &other_die {
        println!("  other die module, not run on cancel: {module} x {count}");
    }
    for row in &rows {
        println!("  {row}");
    }
    ExitCode::SUCCESS
}

fn check_capture_command(args: &[String]) -> ExitCode {
    let mut installs = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--install" {
            match iter.next() {
                Some(path) => installs.push(PathBuf::from(path)),
                None => {
                    eprintln!("error: --install wants a path");
                    return ExitCode::FAILURE;
                }
            }
        } else {
            installs.push(PathBuf::from(arg));
        }
    }
    let Some((primary, extras)) = installs.split_first() else {
        eprintln!("usage: mint-rotwk --check-capture <install dir> [--install <dir>]");
        return ExitCode::FAILURE;
    };
    let vfs = mount_primary_last(primary, extras);
    let (data, line) = crate::load_game_data_or_exit(&vfs);
    println!("{line}");
    let has = |record: &mint_sage_modules::ObjectRecord, bit: &str| {
        record.kind_of.iter().any(|b| b.eq_ignore_ascii_case(bit))
    };
    let captures = |power: &str| {
        data.special_powers
            .get(power)
            .and_then(|def| def.power_enum.as_deref())
            .is_some_and(|value| value.eq_ignore_ascii_case(crate::powers::CAPTURE_BUILDING_ENUM))
    };
    let powers_of = |record: &mint_sage_modules::ObjectRecord, module: &str| -> Vec<String> {
        record
            .declared_modules
            .iter()
            .filter(|declared| declared.name.eq_ignore_ascii_case(module))
            .filter_map(|declared| {
                declared
                    .fields
                    .iter()
                    .find(|field| field.keyword.eq_ignore_ascii_case("SpecialPowerTemplate"))
                    .and_then(|field| field.args.first().cloned())
            })
            .filter(|power| captures(power))
            .collect()
    };
    let (mut capturers, mut capturing_hordes, mut unpaired) = (0usize, 0usize, Vec::new());
    let mut others: Vec<String> = Vec::new();
    for record in data.templates.iter() {
        let updates = powers_of(record, "SpecialAbilityUpdate");
        if updates.is_empty() {
            continue;
        }
        let modules = powers_of(record, "SpecialPowerModule");
        if updates
            .iter()
            .any(|power| modules.iter().any(|m| m.eq_ignore_ascii_case(power)))
        {
            capturers = capturers.saturating_add(1);
            capturing_hordes = capturing_hordes.saturating_add(usize::from(has(record, "HORDE")));
            if !["HORDE", "HERO", "INFANTRY", "CAVALRY"]
                .iter()
                .any(|bit| has(record, bit))
            {
                others.push(record.name.clone());
            }
        } else {
            unpaired.push(record.name.clone());
        }
    }
    println!(
        "capture gate: {capturers} object(s) ({capturing_hordes} hordes) resolve a \
         SpecialPowerModule/SpecialAbilityUpdate pair whose power's Enum is {}; {} carry the \
         update without its power module {:?}",
        crate::powers::CAPTURE_BUILDING_ENUM,
        unpaired.len(),
        unpaired
    );
    println!(
        "capture gate: {} capturer(s) neither horde, hero, infantry nor cavalry: {others:?}",
        others.len()
    );
    let (mut paired_maps, mut mp_paired) = (0usize, 0usize);
    let mut maps: Vec<String> = vfs
        .paths_under("maps/")
        .map(|path| path.as_str().to_owned())
        .filter(|path| path.ends_with(".map"))
        .collect();
    maps.sort();
    let (mut mp_total, mut mp_ok, mut all_ok) = (0usize, 0usize, 0usize);
    let mut player_n_total = 0usize;
    let mut player_n_maps: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut exceptions: Vec<String> = Vec::new();
    for path in &maps {
        let map = match vfs
            .read(path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| mint_sage_map::MapFile::parse(&bytes).map_err(|e| e.to_string()))
        {
            Ok(map) => map,
            Err(error) => {
                exceptions.push(format!("{path}: does not parse - {error}"));
                continue;
            }
        };
        let sides = match map.sides() {
            Ok(sides) => sides,
            Err(error) => {
                exceptions.push(format!("{path}: SidesList does not decode - {error}"));
                continue;
            }
        };
        let every_start: Vec<(u8, mint_sim::PlayerId)> = (0..u8::MAX)
            .take_while(|slot| {
                mint_sage_map::named_waypoint(
                    &map.objects,
                    &mint_sage_map::start_waypoint_name(*slot),
                )
                .is_some()
            })
            .map(|slot| (slot, mint_sim::PlayerId::new(slot)))
            .collect();
        let held = crate::map_players::cast(
            &data.templates,
            &sides,
            &map.objects,
            &every_start,
            &[],
            &std::collections::BTreeSet::new(),
        );
        let mut by_start: std::collections::BTreeMap<u8, (usize, Vec<String>)> =
            std::collections::BTreeMap::new();
        for entity in &held.entities {
            let crate::map_players::Relation::Contestant(player) = entity.owner else {
                continue;
            };
            let slot = by_start.entry(player.get()).or_default();
            slot.0 = slot.0.saturating_add(1);
            if data
                .templates
                .get(&entity.template)
                .is_some_and(|record| record.has_kind("COMMANDCENTER"))
            {
                slot.1.push(entity.template.clone());
            }
        }
        for (start, (count, mains)) in &by_start {
            player_n_total = player_n_total.saturating_add(*count);
            player_n_maps.insert(path.clone());
            println!(
                "player_n: {path}: start {} (Player_{}) holds {count} placement(s), main building {mains:?}",
                start.saturating_add(1),
                start.saturating_add(1)
            );
        }
        let mut wanted: std::collections::BTreeMap<(String, String), usize> =
            std::collections::BTreeMap::new();
        let (mut flags, mut linked) = (0usize, 0usize);
        for object in &map.objects {
            let Some(record) = data.templates.get(&object.template_name) else {
                continue;
            };
            if !crate::map_players::places_entity(
                &data.templates,
                object,
                &std::collections::BTreeSet::new(),
            ) {
                continue;
            }
            flags = flags.saturating_add(usize::from(has(record, "CAPTUREFLAG")));
            linked = linked.saturating_add(usize::from(has(record, "LINKED_TO_FLAG")));
            let owner = object.original_player().unwrap_or_default().to_owned();
            *wanted
                .entry((record.name.to_ascii_lowercase(), owner))
                .or_default() += 1;
        }
        let cast = crate::map_players::cast(
            &data.templates,
            &sides,
            &map.objects,
            &[],
            &[],
            &std::collections::BTreeSet::new(),
        );
        let inputs = crate::skirmish::Inputs {
            map_cast: cast,
            ..crate::skirmish::Inputs::default()
        };
        let built = crate::skirmish::build(
            &data,
            data.object_index.as_ref(),
            &inputs,
            &map.blend_tile,
            |obstacles| {
                mint_sage_passability::Passability::builder(&map.height_map, 1.0)
                    .obstacles(obstacles)
                    .build()
                    .map(|passability| (passability, Vec::new()))
            },
            &|_, _| Vec::new(),
            &|_| Vec::new(),
            &|_| None,
            &|_| Vec::new(),
            &|_| false,
            crate::skirmish::SKIRMISH_SEED,
            &[],
        );
        let start = match built {
            Ok(start) => start,
            Err(error) => {
                exceptions.push(format!("{path}: the match does not build - {error}"));
                continue;
            }
        };
        let rules = start.simulation.rules();
        let mut found: std::collections::BTreeMap<(String, String), usize> =
            std::collections::BTreeMap::new();
        let (mut twice, mut alone, mut unbound) = (0usize, 0usize, 0usize);
        if let Some(ids) = rules.ids() {
            let world = start.simulation.state();
            if let Some(store) = world.store(ids.kind) {
                for (entity, kind) in store.iter() {
                    let Some(entry) = start.kinds.get(*kind) else {
                        continue;
                    };
                    let links = world
                        .get(ids.capture_links, entity)
                        .map_or(0, |links| links.0.len());
                    if has(&entry.record, "CAPTUREFLAG") {
                        match links {
                            0 => alone = alone.saturating_add(1),
                            1 => {}
                            _ => twice = twice.saturating_add(1),
                        }
                    } else if has(&entry.record, "LINKED_TO_FLAG") && links == 0 {
                        unbound = unbound.saturating_add(1);
                    }
                }
            }
            if let Some(store) = world.store(ids.kind) {
                for (entity, kind) in store.iter() {
                    let Some(entry) = start.kinds.get(*kind) else {
                        continue;
                    };
                    let Some(owner) = world
                        .get(ids.owner, entity)
                        .and_then(|owner| rules.map_player_name(owner.0))
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    *found
                        .entry((entry.record.name.to_ascii_lowercase(), owner))
                        .or_default() += 1;
                }
            }
        }
        let ok = wanted == found;
        let mp = path.contains("/map mp ");
        let one_to_one = twice == 0 && alone == 0 && unbound == 0;
        if one_to_one {
            paired_maps = paired_maps.saturating_add(1);
            mp_paired = mp_paired.saturating_add(usize::from(mp));
        } else {
            exceptions.push(format!(
                "{path}: {twice} flag(s) bound to more than one structure, {alone} flag(s) and \
                 {unbound} structure(s) unbound"
            ));
        }
        let owners: Vec<String> = found
            .iter()
            .map(|((template, owner), count)| format!("{template}@{owner:?} x{count}"))
            .collect();
        println!(
            "{path}{}: {flags} flag(s), {linked} linked, {} entit(ies) - {}; {}",
            if mp { " [mp]" } else { "" },
            found.values().sum::<usize>(),
            if ok { "OK" } else { "MISMATCH" },
            owners.join(", ")
        );
        if ok {
            all_ok = all_ok.saturating_add(1);
        } else {
            let missing: Vec<String> = wanted
                .iter()
                .filter(|(key, count)| found.get(*key) != Some(*count))
                .map(|((template, owner), count)| {
                    format!(
                        "{template}@{owner:?} placed {count}, entities {}",
                        found
                            .get(&(template.clone(), owner.clone()))
                            .copied()
                            .unwrap_or(0)
                    )
                })
                .collect();
            exceptions.push(format!("{path}: {}", missing.join("; ")));
        }
        if mp {
            mp_total = mp_total.saturating_add(1);
            mp_ok = mp_ok.saturating_add(usize::from(ok));
        }
    }
    println!(
        "capture gate: map mp maps with every flag and linked structure an entity owned by its \
         originalOwner {mp_ok}/{mp_total}; all maps {all_ok}/{}",
        maps.len()
    );
    println!(
        "capture gate: maps where every flag binds exactly one linked structure and every \
         structure a flag: map mp {mp_paired}/{mp_total}; all maps {paired_maps}/{}",
        maps.len()
    );
    println!(
        "player_n: {player_n_total} placement(s) on {} map(s) go to the contestant holding their Player_N start",
        player_n_maps.len()
    );
    for exception in &exceptions {
        println!("  EXCEPTION {exception}");
    }
    ExitCode::SUCCESS
}

#[must_use]
pub fn run(title: &'static title::TitleRegistration) -> ExitCode {
    crate::title::register(title);
    crate::crash::install();
    mint_sage_apt_player::set_trace_sink(Some(Box::new(|name, args, result| {
        if crate::diag::active("apt") {
            crate::diag::diag!("apt: extern {name}({args:?}) -> {result}");
        }
    })));
    let rest: Vec<String> = std::env::args().skip(1).collect();
    if rest.first().map(String::as_str) == Some("--check-capture") {
        return check_capture_command(rest.get(1..).unwrap_or_default());
    }
    if rest.first().map(String::as_str) == Some("--play-movie") {
        return movie_cmd::play_movie_command(rest.get(1..).unwrap_or_default());
    }
    if rest.first().map(String::as_str) == Some("--check-kind") {
        return check_kind_command(rest.get(1..).unwrap_or_default());
    }
    if rest.first().map(String::as_str) == Some("--check-weapon-fire") {
        return check_weapon_fire_command(rest.get(1..).unwrap_or_default());
    }
    if rest.first().map(String::as_str) == Some("--check-toggle-mounted") {
        return check_toggle_mounted_command(rest.get(1..).unwrap_or_default());
    }
    if rest.first().map(String::as_str) == Some("--check-grants") {
        return check_grants_command(rest.get(1..).unwrap_or_default());
    }
    if rest.first().map(String::as_str) == Some("--check-cancel-fx") {
        return check_cancel_fx_command(rest.get(1..).unwrap_or_default());
    }

    let options = match parse_args() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    };

    let vfs = Arc::new(mount_primary_last(
        &options.primary,
        &options.extra_installs,
    ));

    if let Some(dev) = &options.campaign_dev {
        if let Err(message) = dev.validate(&vfs) {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    }

    if let Some((screen, path)) = options.shell_screenshot.clone() {
        let install_roots: Vec<PathBuf> = std::iter::once(options.primary.clone())
            .chain(options.extra_installs.iter().cloned())
            .collect();
        return match render_shell_screenshot(&vfs, &install_roots, &screen, &path, &options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }

    let needle = Some(
        options
            .map
            .clone()
            .unwrap_or_else(|| String::from(crate::skirmish::DEFAULT_MAP)),
    );
    let mut maps = curated_maps(&vfs, needle.as_deref());
    if maps.is_empty() && options.map.is_none() {
        println!(
            "shell: the default map \"{}\" is not in these archives - playing the first one found",
            crate::skirmish::DEFAULT_MAP
        );
        maps = curated_maps(&vfs, None);
    }
    let Some(map_label) = maps.first().cloned() else {
        eprintln!(
            "error: no .map entry found under {}{}",
            options.primary.display(),
            options
                .map
                .as_deref()
                .map_or_else(String::new, |needle| format!(" matching \"{needle}\""))
        );
        return ExitCode::FAILURE;
    };
    println!(
        "shell: {} map(s) found, playing \"{map_label}\"",
        maps.len()
    );

    let map_bytes = match vfs.read(&map_label) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("error: reading {map_label}: {error}");
            return ExitCode::FAILURE;
        }
    };

    if let Some(out_path) = options.ini_census.clone() {
        #[cfg(feature = "ini-census")]
        {
            return ini_census_command(&vfs, &map_bytes, map_label, &options, &out_path);
        }
        #[cfg(not(feature = "ini-census"))]
        {
            let _ = out_path;
            eprintln!("error: --ini-census needs `--features ini-census`");
            return ExitCode::FAILURE;
        }
    }

    if let Some(path) = options.replay.clone() {
        let config = match_config(&options, &map_label);
        return match crate::replay_run::run(&vfs, &map_bytes, map_label, &config, &path) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }

    if let Some(minutes) = options.run_match {
        return match run_headless_match(&vfs, &map_bytes, map_label, minutes, &options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }

    if let Some(total_units) = options.stress {
        return match crate::stress::run_stress(&vfs, &map_bytes, map_label, total_units, &options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }

    if let Some(path) = options.screenshot.clone() {
        return match render_screenshot(&vfs, &map_bytes, map_label, &path, &options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }

    let config = WindowConfig {
        title: format!("{} - {map_label}", title.binary),
        pipeline_cache_dir: crate::user_data::pipeline_cache_dir(),
        ..WindowConfig::default()
    };
    let viewport = (config.width, config.height);
    let sources = match options.passability.as_deref() {
        Some(raw) => match crate::game::PassabilitySources::parse(raw) {
            Ok(sources) => sources,
            Err(message) => {
                eprintln!("error: {message}");
                return ExitCode::FAILURE;
            }
        },
        None => crate::game::PassabilitySources::default(),
    };

    let match_config = match_config(&options, &map_label);
    let install_roots: Vec<PathBuf> = std::iter::once(options.primary.clone())
        .chain(options.extra_installs.iter().cloned())
        .collect();
    let shell_wanted = shell_wanted(
        &options.map,
        &options.faction,
        &options.opponent,
        options.army,
    ) && options.spawn.is_none()
        && options.spawn_buildings.is_none()
        && options.dummies.is_none()
        || options.campaign_dev.is_some()
        || options.tutorial_dev.is_some();
    let scale_mode = options.scale_mode;
    let control = match &options.control {
        Some(spec) => match crate::game::ControlSet::parse(spec) {
            Ok(control) => control,
            Err(message) => {
                eprintln!("error: {message}");
                return ExitCode::FAILURE;
            }
        },
        None if options.dummies.is_some() => crate::game::ControlSet::Listed(vec![
            crate::game::LOCAL_PLAYER,
            crate::game::ENEMY_PLAYER,
        ]),
        None => crate::game::ControlSet::default_local(),
    };
    let start = AppStart {
        vfs,
        install_roots,
        sources,
        scale_mode,
        shell_wanted,
        shell_cli: ShellCli {
            intro: options.shell_cli.intro.or((options.drive.is_some()
                || options.ticks > 0
                || options.screenshot.is_some()
                || options.campaign_dev.is_some()
                || options.tutorial_dev.is_some())
            .then_some(false)),
            ..options.shell_cli.clone()
        },
        campaign_dev: options.campaign_dev.clone(),
        tutorial_dev: options.tutorial_dev,
        skip_campaign_movies: options.shell_cli.intro != Some(true)
            && (options.drive.is_some() || options.ticks > 0 || options.screenshot.is_some()),
        match_config,
        map_label,
        control,
    };
    if options.ticks > 0 && options.drive.is_none() {
        return match run_ticks_headless(start, options.size.unwrap_or(viewport), &options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }
    if let Some(script) = options.drive.as_deref() {
        let (width, height) = options.size.unwrap_or(viewport);
        let run = || -> Result<(), String> {
            let steps = crate::drive::parse(script)?;
            let mut renderer = open_offscreen_renderer((width, height))?;
            let app = start
                .build(&mut renderer, (width, height))
                .inspect_err(|message| crate::crash::report_message("loading", message))?;
            let mut app = crate::crash::Dumping::new(app, crate::crash::root());
            crate::drive::run(
                &mut app,
                &mut renderer,
                (width, height),
                options.drive_dt,
                &steps,
                |app, verb, argument| match &mut app.app_mut().screen {
                    AppScreen::Match(game) if verb == "do" => game.act_for_script(argument),
                    AppScreen::Match(game) => game.select_for_script(argument),
                    AppScreen::Shell(_) => Err(format!("{verb}: not in a match")),
                },
                |app| match &app.app().screen {
                    AppScreen::Match(game) => Some(game.sim_frame()),
                    AppScreen::Shell(_) => None,
                },
            )?;
            if let AppScreen::Match(game) = &app.app_mut().screen {
                println!(
                    "drive: {} tick(s), state hash {:#018x} (MINT_ROTWK_HASH={})",
                    game.sim_frame(),
                    game.state_hash(),
                    crate::skirmish::frame_hash_label(crate::skirmish::frame_hash())
                );
                if let Some(every) = options.hash_every {
                    print_tick_hashes(game.tick_hashes(), every);
                }
                crate::perf::report();
            }
            Ok(())
        };
        return match run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }
    let result = mint_app::run(&config, move |renderer| {
        println!(
            "renderer: pipeline cache {:?}",
            renderer.pipeline_cache_state()
        );
        start
            .build(renderer, viewport)
            .map(|app| crate::crash::Dumping::new(app, crate::crash::root()))
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })
    });

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            if crate::crash::written() == 0 {
                crate::crash::report_error("starting the window", &error);
            }
            let mut cause = std::error::Error::source(&error);
            while let Some(source) = cause {
                eprintln!("  caused by: {source}");
                cause = source.source();
            }
            ExitCode::FAILURE
        }
    }
}

fn match_config(options: &Options, map_label: &str) -> crate::game::MatchConfig {
    crate::game::MatchConfig::Skirmish(
        crate::lobby::SkirmishSetup::two_player(
            options
                .faction
                .clone()
                .unwrap_or_else(|| String::from(crate::skirmish::DEFAULT_FACTION)),
            options
                .opponent
                .clone()
                .unwrap_or_else(|| String::from(crate::skirmish::DEFAULT_OPPONENT)),
            map_label.to_owned(),
            [options.player_ai, options.opponent_ai.is_ai()],
            options.army,
        )
        .with_more_opponents(&options.more_opponents, options.opponent_ai.is_ai())
        .with_spawn_flags(
            options.spawn.clone(),
            options.spawn_buildings.clone(),
            options.dummies.clone(),
        )
        .with_cah_hero(options.cah_hero.clone())
        .with_dummies_attack(options.dummies_attack)
        .with_starting_money(options.money),
    )
}

fn parse_money(raw: &str) -> Result<i64, String> {
    let money: i64 = raw
        .parse()
        .map_err(|_| format!("--money wants a whole amount, not {raw:?}"))?;
    crate::lobby::check_starting_money(money).map_err(|error| format!("--money: {error}"))?;
    Ok(money)
}

#[allow(clippy::too_many_arguments)]
fn build_game(
    renderer: &mut Renderer,
    vfs: &Arc<Vfs>,
    install_roots: &[PathBuf],
    sources: crate::game::PassabilitySources,
    config: crate::game::MatchConfig,
    map_label: String,
    viewport: (u32, u32),
    scale_mode: mint_sage_apt_viewer::ScaleMode,
    control: crate::game::ControlSet,
    data: Option<crate::game_data::GameData>,
    ui: Option<crate::ui_resources::UiResources>,
) -> Result<Box<Game>, String> {
    let mut loader = crate::match_loader::MatchLoader::new(
        Arc::clone(vfs),
        install_roots.to_vec(),
        sources,
        config,
        map_label,
        viewport,
        scale_mode,
        control,
        data,
        ui,
    );
    loader
        .advance(renderer, std::time::Duration::MAX)?
        .ok_or_else(|| "load: the match did not finish".to_owned())
}

fn shell_wanted(
    map: &Option<String>,
    faction: &Option<String>,
    opponent: &Option<String>,
    army: Option<u32>,
) -> bool {
    map.is_none() && faction.is_none() && opponent.is_none() && army.is_none()
}

enum AppScreen {
    Shell(Box<crate::shell::ShellApp>),
    Match(Box<Game>),
}

struct AppStart {
    vfs: Arc<Vfs>,
    install_roots: Vec<PathBuf>,
    sources: crate::game::PassabilitySources,
    scale_mode: mint_sage_apt_viewer::ScaleMode,
    shell_wanted: bool,
    shell_cli: ShellCli,
    campaign_dev: Option<crate::campaign_dev::CampaignDev>,
    tutorial_dev: Option<crate::tutorial::TutorialKind>,
    skip_campaign_movies: bool,
    match_config: crate::game::MatchConfig,
    map_label: String,
    control: crate::game::ControlSet,
}

#[derive(Debug, Clone, Default)]
struct ShellCli {
    off: bool,
    name: Option<String>,
    force_on: bool,
    intro: Option<bool>,
}

impl ShellCli {
    fn apply(&self, fields: &mut crate::shell_map::ShellMapData) {
        if let Some(name) = &self.name {
            fields.name.clone_from(name);
        }
        if self.force_on {
            fields.on = true;
        }
        if self.off {
            fields.switch_off_by_command_argument();
        }
    }
}

impl AppStart {
    fn build(self, renderer: &mut Renderer, viewport: (u32, u32)) -> Result<AppState, String> {
        if self.shell_wanted {
            println!("shell: booting into the APT main menu (pass --map/--demo/--faction/--opponent to skip it)");
            let mut state = AppState::new_shell(
                renderer,
                self.vfs,
                self.install_roots,
                self.sources,
                self.scale_mode,
                viewport,
            )?;
            state.boot_shell(viewport, &self.shell_cli);
            state
                .campaign_flow
                .set_skip_movies(self.skip_campaign_movies);
            if let Some(dev) = self.campaign_dev {
                state.campaign_flow.set_dev_entry(dev);
            }
            if let Some(kind) = self.tutorial_dev {
                state.campaign_flow.set_dev_tutorial(kind);
            }
            return Ok(state);
        }
        let start = MatchStart {
            sources: self.sources,
            config: self.match_config,
            map_label: self.map_label,
            control: self.control,
        };
        let game = build_game(
            renderer,
            &self.vfs,
            &self.install_roots,
            start.sources,
            start.config.clone(),
            start.map_label.clone(),
            viewport,
            self.scale_mode,
            start.control.clone(),
            None,
            None,
        )?;
        let mut state = AppState::new_match(
            game,
            self.vfs,
            self.install_roots,
            self.sources,
            self.scale_mode,
        )?;
        state.last_match = Some(start);
        Ok(state)
    }
}

const CREATE_A_HERO_MAP: &str = r"Maps\CreateAHero\CreateAHero.map";

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn save_picture(renderer: &mut Renderer, stem: &str) -> Result<String, String> {
    let (width, height, rgba) = renderer
        .take_capture()
        .ok_or("the frame was not captured")?;
    let dir = crate::user_data::dir().ok_or("no user data folder is known")?;
    let path = crate::cah_picture::save(&dir, stem, width, height, &rgba)?;
    Ok(path.display().to_string())
}

struct AppState {
    vfs: Arc<Vfs>,
    install_roots: Vec<PathBuf>,
    sources: crate::game::PassabilitySources,
    scale_mode: mint_sage_apt_viewer::ScaleMode,
    screen: AppScreen,
    cursors: crate::cursors::Cursors,
    last_match: Option<MatchStart>,
    loading: Option<Box<Loading>>,
    shell_game: Option<crate::shell_game::ShellGame>,
    shell_loading: Option<Box<crate::match_loader::MatchLoader>>,
    shell_loading_kind: crate::shell_game::ShellGameKind,
    flow: crate::shell_game::ShellFlow,
    menu_held: bool,
    shell_map_name: String,
    campaign_flow: crate::campaign_flow::CampaignFlow,
    intro: Option<crate::intro::Intro>,
    pending_boot: Option<bool>,
    shell_hold: Option<ShellLoadHold>,
    shell_preloading: bool,
    preload_warm: u32,
    loading_ring: Option<crate::loading_ring::LoadingRing>,
    frame_dt: f32,
}

struct ShellLoadHold {
    age: f32,
    loaded: bool,
    warm_frames: u32,
}

const SHELL_WARM_FRAMES: u32 = 30;

const PRELOAD_GAP_BUDGET: std::time::Duration = std::time::Duration::from_millis(250);

const FADE_WHOLE_SCREEN: &str = "FadeWholeScreen";

struct Loading {
    loader: crate::match_loader::MatchLoader,
    start: MatchStart,
    budget: std::time::Duration,
}

struct MatchStart {
    sources: crate::game::PassabilitySources,
    config: crate::game::MatchConfig,
    map_label: String,
    control: crate::game::ControlSet,
}

impl AppState {
    fn new_shell(
        renderer: &mut Renderer,
        vfs: Arc<Vfs>,
        install_roots: Vec<PathBuf>,
        sources: crate::game::PassabilitySources,
        scale_mode: mint_sage_apt_viewer::ScaleMode,
        viewport: (u32, u32),
    ) -> Result<Self, String> {
        #[allow(clippy::cast_precision_loss)]
        let window_size = (viewport.0 as f32, viewport.1 as f32);
        let shell = crate::shell::ShellApp::load(
            Arc::clone(&vfs),
            install_roots.clone(),
            window_size,
            scale_mode,
            renderer,
        )?;
        let cursors = crate::cursors::Cursors::load(&vfs, &install_roots)?;
        Ok(Self {
            vfs,
            install_roots,
            sources,
            scale_mode,
            screen: AppScreen::Shell(Box::new(shell)),
            cursors,
            last_match: None,
            loading: None,
            shell_game: None,
            shell_loading: None,
            shell_loading_kind: crate::shell_game::ShellGameKind::ShellMap,
            flow: crate::shell_game::ShellFlow::default(),
            menu_held: false,
            shell_map_name: String::new(),
            campaign_flow: crate::campaign_flow::CampaignFlow::default(),
            intro: None,
            pending_boot: None,
            shell_hold: None,
            shell_preloading: false,
            preload_warm: 0,
            loading_ring: None,
            frame_dt: 0.0,
        })
    }

    fn new_match(
        game: Box<Game>,
        vfs: Arc<Vfs>,
        install_roots: Vec<PathBuf>,
        sources: crate::game::PassabilitySources,
        scale_mode: mint_sage_apt_viewer::ScaleMode,
    ) -> Result<Self, String> {
        let cursors = crate::cursors::Cursors::load(&vfs, &install_roots)?;
        Ok(Self {
            vfs,
            install_roots,
            sources,
            scale_mode,
            screen: AppScreen::Match(game),
            cursors,
            last_match: None,
            loading: None,
            shell_game: None,
            shell_loading: None,
            shell_loading_kind: crate::shell_game::ShellGameKind::ShellMap,
            flow: crate::shell_game::ShellFlow::default(),
            menu_held: false,
            shell_map_name: String::new(),
            campaign_flow: crate::campaign_flow::CampaignFlow::default(),
            intro: None,
            pending_boot: None,
            shell_hold: None,
            shell_preloading: false,
            preload_warm: 0,
            loading_ring: None,
            frame_dt: 0.0,
        })
    }

    fn enter_shell(&mut self, renderer: &mut Renderer, viewport: (u32, u32)) -> Result<(), String> {
        let fresh = Self::new_shell(
            renderer,
            Arc::clone(&self.vfs),
            self.install_roots.clone(),
            self.sources,
            self.scale_mode,
            viewport,
        )?;
        self.screen = fresh.screen;
        self.last_match = None;
        let actions = self.flow.on(crate::shell_game::FlowEvent::ReturnToShell);
        self.apply_flow(viewport, actions);
        Ok(())
    }

    fn boot_shell(&mut self, viewport: (u32, u32), cli: &ShellCli) {
        let AppScreen::Shell(shell) = &self.screen else {
            return;
        };
        let mut fields = shell.shell_map_fields();
        cli.apply(&mut fields);
        let map_exists = self.vfs.contains(&fields.name);
        let effects = crate::scene::static_lod_effects_index(&self.vfs);
        let headless = std::env::var_os("_EA_RTS_HEADLESS").is_some();
        let mut latched_off = false;
        let gate_on = crate::shell_map::shell_map_on(
            &fields,
            map_exists,
            effects,
            &mut latched_off,
            headless,
        );
        println!(
            "shell-map: ShellMapName={} ShellMapOn={} off-by-argument={} map-in-cache={map_exists} \
             effects-lod={effects} headless={headless} -> {}",
            fields.name,
            fields.on,
            fields.off_by_command_argument,
            if gate_on { "on" } else { "off" }
        );
        self.shell_map_name = fields.name;
        if cli.intro.unwrap_or(true) {
            println!(
                "intro: PlayIntro={} - the movies, then the title screen",
                fields.play_intro
            );
            self.intro = Some(crate::intro::Intro::new(fields.play_intro, gate_on));
            self.pending_boot = Some(gate_on);
            self.menu_held = true;
            if gate_on {
                println!("shell: preloading the shell map under the intro");
                self.shell_preloading = true;
                self.preload_warm = 0;
                self.start_shell_game(viewport);
            }
            return;
        }
        self.boot_flow(viewport, gate_on);
    }

    fn boot_flow(&mut self, viewport: (u32, u32), gate_on: bool) {
        self.menu_held = gate_on;
        let actions = self.flow.on(crate::shell_game::FlowEvent::Boot { gate_on });
        self.apply_flow(viewport, actions);
    }

    fn intro_step(&mut self, renderer: &mut Renderer) {
        use crate::campaign_flow::FADE_IN_GAME_MOVIE;
        use crate::intro::IntroAction;
        let Some(intro) = self.intro.as_mut() else {
            return;
        };
        let vfs = Arc::clone(&self.vfs);
        let playing = self.campaign_flow.movie_playing();
        let fade_done = self.campaign_flow.handler(&vfs).is_finished();
        let title_still = intro.title_shown() && fade_done;
        match intro.step(playing, fade_done, self.frame_dt) {
            IntroAction::None => {
                if title_still {
                    self.preload_step(renderer, crate::match_loader::FRAME_BUDGET);
                }
            }
            IntroAction::PlayMovie { name, faded } => {
                self.preload_step(renderer, PRELOAD_GAP_BUDGET);
                if faded {
                    self.campaign_flow.handler(&vfs).reverse(FADE_IN_GAME_MOVIE);
                }
                if !self.play_movie(renderer, name, true, faded) {
                    self.campaign_flow.handler(&vfs).reset();
                }
            }
            IntroAction::ShowTitle => {
                println!("intro: title screen {}", crate::intro::TITLE_IMAGE);
                self.campaign_flow.handler(&vfs).reverse(FADE_IN_GAME_MOVIE);
            }
            IntroAction::FadeOutTitle => {
                let handler = self.campaign_flow.handler(&vfs);
                handler.set_group(FADE_IN_GAME_MOVIE);
                handler.set_hold(true);
            }
            IntroAction::Finished => {
                println!("intro: finished");
                self.intro = None;
                let gate_on = self.pending_boot.take().unwrap_or(false);
                let ready = self.shell_preload_ready();
                let handler = self.campaign_flow.handler(&vfs);
                handler.reset();
                if gate_on && !ready {
                    handler.reverse(FADE_WHOLE_SCREEN);
                }
                let viewport = self.viewport();
                self.boot_flow(viewport, gate_on);
            }
        }
    }

    fn preload_step(&mut self, renderer: &mut Renderer, budget: std::time::Duration) {
        if !self.shell_preloading {
            return;
        }
        if self.shell_loading.is_some() {
            self.advance_shell_loading(renderer, budget);
            return;
        }
        #[allow(clippy::cast_precision_loss)]
        let size = {
            let (w, h) = self.viewport();
            (w as f32, h as f32)
        };
        let Some(shell_game) = self.shell_game.as_mut() else {
            return;
        };
        let began = std::time::Instant::now();
        while self.preload_warm < SHELL_WARM_FRAMES {
            let _ = shell_game.render_to_rect(renderer, 0.0, size, size.0);
            self.preload_warm = self.preload_warm.saturating_add(1);
            if self.preload_warm == SHELL_WARM_FRAMES {
                println!("shell: preloaded shell map warm ({SHELL_WARM_FRAMES} off-screen frames)");
            }
            if began.elapsed() >= budget {
                break;
            }
        }
    }

    fn shell_preload_ready(&self) -> bool {
        self.shell_game.is_some()
            && self.shell_loading.is_none()
            && self.preload_warm >= SHELL_WARM_FRAMES
    }

    fn hold_for_shell_load(&mut self) {
        self.menu_held = true;
        println!("shell: load screen up");
        self.shell_hold = Some(ShellLoadHold {
            age: 0.0,
            loaded: false,
            warm_frames: 0,
        });
    }

    fn shell_hold_step(&mut self) {
        let Some(hold) = self.shell_hold.as_mut() else {
            return;
        };
        hold.age += self.frame_dt;
        if !(hold.loaded && hold.warm_frames >= SHELL_WARM_FRAMES) {
            return;
        }
        println!(
            "shell: load screen down after {:.2} s, {} warm frame(s)",
            hold.age, hold.warm_frames
        );
        self.shell_hold = None;
        self.menu_held = false;
        let viewport = self.viewport();
        let actions = self.flow.on(crate::shell_game::FlowEvent::ShellGameLoaded);
        self.apply_flow(viewport, actions);
    }

    fn apply_flow(&mut self, viewport: (u32, u32), actions: Vec<crate::shell_game::FlowAction>) {
        use crate::shell_game::FlowAction;
        for action in actions {
            match action {
                FlowAction::StartShellGame if self.shell_preload_ready() => {
                    println!("shell: preloaded shell map runs - no load screen");
                    self.release_preloaded_shell_game();
                    let actions = self.flow.on(crate::shell_game::FlowEvent::ShellGameLoaded);
                    self.apply_flow(viewport, actions);
                }
                FlowAction::StartShellGame => {
                    let preloaded = self.shell_game.is_some();
                    self.release_preloaded_shell_game();
                    self.hold_for_shell_load();
                    if let Some(hold) = self.shell_hold.as_mut().filter(|_| preloaded) {
                        hold.loaded = true;
                        hold.warm_frames = self.preload_warm;
                    }
                    self.start_shell_game(viewport);
                }
                FlowAction::RestartShellGame => {
                    self.shell_game = None;
                    self.hold_for_shell_load();
                    self.start_shell_game(viewport);
                }
                FlowAction::DropShellGame => {
                    self.shell_game = None;
                    self.shell_loading = None;
                }
                FlowAction::PushMainMenu => {
                    self.menu_held = false;
                    println!("shell: main menu pushed");
                }
                FlowAction::HoldLoadScreen => self.hold_for_shell_load(),
                FlowAction::DropLoadScreen => {
                    if self.shell_hold.take().is_some() {
                        println!("shell: load screen down, nothing left to load");
                    }
                    self.menu_held = false;
                }
                FlowAction::ShowLowLodImage => {
                    if let AppScreen::Shell(shell) = &mut self.screen {
                        shell.set_shell_map_running(false);
                    }
                }
            }
        }
    }

    fn release_preloaded_shell_game(&mut self) {
        self.shell_preloading = false;
        if let (Some(_), AppScreen::Shell(shell)) = (&self.shell_game, &mut self.screen) {
            shell.set_shell_map_running(true);
        }
    }

    fn start_shell_game(&mut self, viewport: (u32, u32)) {
        let kind = crate::shell_game::ShellGameKind::ShellMap;
        let building = self.shell_loading.is_some() && self.shell_loading_kind == kind;
        if building
            || !crate::shell_game::start_needed(
                self.shell_game.as_ref().map(|game| game.kind()),
                kind,
            )
        {
            return;
        }
        let (data, ui) = match &self.screen {
            AppScreen::Shell(shell) => (shell.game_data_clone(), Some(shell.ui_resources())),
            AppScreen::Match(_) => (None, None),
        };
        self.shell_loading_kind = kind;
        self.shell_loading = Some(Box::new(crate::match_loader::MatchLoader::new(
            Arc::clone(&self.vfs),
            self.install_roots.clone(),
            self.sources,
            kind.config(),
            self.shell_map_name.clone(),
            viewport,
            self.scale_mode,
            crate::game::ControlSet::default_local(),
            data,
            ui,
        )));
    }

    fn sync_cah_scene(&mut self) {
        use crate::shell_game::{FlowEvent, ShellGameKind};
        let wanted = matches!(&self.screen, AppScreen::Shell(shell) if shell.cah_scene_wanted());
        let running = self.shell_game.as_ref().map(|game| game.kind());
        let loading = self.shell_loading.as_ref().map(|_| self.shell_loading_kind);
        let viewport = self.viewport();
        match (wanted, running, loading) {
            (true, Some(ShellGameKind::CreateAHero), _)
            | (true, _, Some(ShellGameKind::CreateAHero)) => {}
            (true, _, _) => {
                let (data, ui) = match &self.screen {
                    AppScreen::Shell(shell) => {
                        (shell.game_data_clone(), Some(shell.ui_resources()))
                    }
                    AppScreen::Match(_) => (None, None),
                };
                let actions = self.flow.on(FlowEvent::CreateAHeroStart);
                self.apply_flow(viewport, actions);
                if let AppScreen::Shell(shell) = &mut self.screen {
                    shell.set_shell_map_running(false);
                }
                self.shell_loading_kind = ShellGameKind::CreateAHero;
                self.shell_loading = Some(Box::new(crate::match_loader::MatchLoader::new(
                    Arc::clone(&self.vfs),
                    self.install_roots.clone(),
                    self.sources,
                    ShellGameKind::CreateAHero.config(),
                    CREATE_A_HERO_MAP.to_owned(),
                    viewport,
                    self.scale_mode,
                    crate::game::ControlSet::default_local(),
                    data,
                    ui,
                )));
            }
            (false, Some(ShellGameKind::CreateAHero), _)
            | (false, _, Some(ShellGameKind::CreateAHero)) => {
                self.shell_game = None;
                self.shell_loading = None;
                let actions = self.flow.on(FlowEvent::ReturnToShell);
                self.apply_flow(viewport, actions);
            }
            (false, _, _) => {}
        }
    }

    fn advance_shell_loading(&mut self, renderer: &mut Renderer, budget: std::time::Duration) {
        let Some(loader) = self.shell_loading.as_mut() else {
            return;
        };
        let result = loader.advance(renderer, budget);
        match result {
            Ok(None) => {}
            Ok(Some(mut game)) => {
                fit_to_window(&mut game, renderer);
                self.shell_loading = None;
                let kind = self.shell_loading_kind;
                let map = match kind {
                    crate::shell_game::ShellGameKind::ShellMap => self.shell_map_name.as_str(),
                    crate::shell_game::ShellGameKind::CreateAHero => CREATE_A_HERO_MAP,
                };
                self.shell_game = Some(crate::shell_game::ShellGame::new(kind, game, map));
                if self.shell_preloading {
                    println!("shell: shell map preloaded, held at its first frame");
                    return;
                }
                if let AppScreen::Shell(shell) = &mut self.screen {
                    shell.set_shell_map_running(true);
                }
                if let Some(hold) = self.shell_hold.as_mut() {
                    hold.loaded = true;
                    println!("shell: {map} loaded after {:.2} s", hold.age);
                    return;
                }
                if kind == crate::shell_game::ShellGameKind::CreateAHero {
                    return;
                }
                let viewport = self.viewport();
                let actions = self.flow.on(crate::shell_game::FlowEvent::ShellGameLoaded);
                self.apply_flow(viewport, actions);
            }
            Err(error) => {
                eprintln!(
                    "shell-map: could not load {} ({error}) - the still image stands in",
                    match self.shell_loading_kind {
                        crate::shell_game::ShellGameKind::ShellMap => self.shell_map_name.as_str(),
                        crate::shell_game::ShellGameKind::CreateAHero => CREATE_A_HERO_MAP,
                    }
                );
                self.shell_loading = None;
                self.shell_hold = None;
                self.menu_held = false;
                if let AppScreen::Shell(shell) = &mut self.screen {
                    shell.set_shell_map_running(false);
                }
            }
        }
    }

    fn viewport(&self) -> (u32, u32) {
        match &self.screen {
            AppScreen::Shell(shell) => {
                let (w, h) = shell.window_size();
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let size = (w.max(0.0) as u32, h.max(0.0) as u32);
                size
            }
            AppScreen::Match(game) => game.viewport(),
        }
    }

    fn advance_loading(&mut self, renderer: &mut Renderer) {
        let Some(loading) = self.loading.as_mut() else {
            return;
        };
        let result = loading.loader.advance(renderer, loading.budget);
        if let (Some((percent, phase)), AppScreen::Shell(shell)) =
            (loading.loader.progress_change(), &mut self.screen)
        {
            println!("load: {percent}% {phase}");
            shell.load_progress(percent);
        }
        match result {
            Ok(None) => {}
            Ok(Some(mut game)) => {
                fit_to_window(&mut game, renderer);
                println!("load: 100% done");
                let mut config = None;
                if let Some(loading) = self.loading.take() {
                    config = Some(loading.start.config.clone());
                    self.last_match = Some(loading.start);
                }
                let game = match config {
                    Some(config) => match self.begin_start_tail(&config, game) {
                        Ok(()) => return,
                        Err(game) => game,
                    },
                    None => game,
                };
                self.screen = AppScreen::Match(game);
            }
            Err(error) => {
                eprintln!("shell: could not start the match ({error}) - back to the menu");
                self.loading = None;
            }
        }
    }

    fn check_transitions(&mut self, renderer: &mut Renderer) {
        self.intro_step(renderer);
        self.shell_hold_step();
        self.campaign_step(renderer);
        if self.loading.is_some() {
            self.advance_loading(renderer);
            return;
        }
        self.sync_cah_scene();
        if !self.shell_preloading {
            self.advance_shell_loading(renderer, crate::match_loader::FRAME_BUDGET);
        }
        if let AppScreen::Match(game) = &mut self.screen {
            if let Some(path) = game.take_load_request() {
                let viewport = game.viewport();
                if let Err(error) = self.enter_shell(renderer, viewport) {
                    eprintln!("load: could not leave the match ({error})");
                    return;
                }
                self.load_campaign_save(&path);
                return;
            }
        }
        match &mut self.screen {
            AppScreen::Match(game) if game.wants_shell() => {
                let viewport = game.viewport();
                let score = game.score_figures();
                let mission_end = game.mission_end();
                let carried = if mission_end == Some(mint_sage_campaign::MissionEnd::Victory) {
                    game.capture_carryover()
                } else {
                    None
                };
                let was_campaign = matches!(
                    self.last_match.as_ref().map(|start| &start.config),
                    Some(crate::game::MatchConfig::Campaign(_))
                );
                let setup = match self.last_match.as_ref().map(|start| &start.config) {
                    Some(crate::game::MatchConfig::Skirmish(setup)) => Some(setup.clone()),
                    _ => None,
                };
                if let Err(error) = self.enter_shell(renderer, viewport) {
                    eprintln!(
                        "shell: could not reload the menu ({error}) - staying in the match (Escape to try again)"
                    );
                    return;
                }
                if was_campaign {
                    if self.campaign_running() {
                        self.campaign_match_ended(mission_end, carried);
                    }
                    if let AppScreen::Shell(shell) = &mut self.screen {
                        shell.show_campaign_score(&score.unwrap_or_default());
                    }
                    return;
                }
                if let (Some(figures), Some(setup), AppScreen::Shell(shell)) =
                    (score, setup, &mut self.screen)
                {
                    shell.show_score(&figures, setup);
                }
            }
            AppScreen::Match(game) if game.wants_restart() => {
                let viewport = game.viewport();
                let Some(start) = self.last_match.take() else {
                    eprintln!("restart: no record of how this match was built - staying");
                    return;
                };
                if let Err(error) = self.enter_shell(renderer, viewport) {
                    eprintln!("restart: could not leave the match ({error})");
                    self.last_match = Some(start);
                    return;
                }
                let actions = self.flow.on(crate::shell_game::FlowEvent::MatchStart);
                self.apply_flow(viewport, actions);
                let (data, ui) = match &mut self.screen {
                    AppScreen::Shell(shell) => (shell.take_game_data(), Some(shell.ui_resources())),
                    AppScreen::Match(_) => (None, None),
                };
                match build_game(
                    renderer,
                    &self.vfs,
                    &self.install_roots,
                    start.sources,
                    start.config.clone(),
                    start.map_label.clone(),
                    viewport,
                    self.scale_mode,
                    start.control.clone(),
                    data,
                    ui,
                ) {
                    Ok(game) => {
                        println!("restart: the same match built again");
                        self.screen = AppScreen::Match(game);
                        self.last_match = Some(start);
                    }
                    Err(error) => {
                        eprintln!("restart: could not rebuild the match ({error}) - in the menu");
                    }
                }
            }
            AppScreen::Shell(shell) if shell.load_screen_shown() => {
                let Some(pending) = shell.take_pending() else {
                    return;
                };
                let actions = self.flow.on(crate::shell_game::FlowEvent::MatchStart);
                let map_label = pending.setup.map_label.clone();
                let config = match pending.campaign {
                    Some(campaign) => crate::game::MatchConfig::Campaign(campaign),
                    None => crate::game::MatchConfig::Skirmish(pending.setup),
                };
                let start = MatchStart {
                    sources: pending.passability_sources,
                    config: config.clone(),
                    map_label: map_label.clone(),
                    control: crate::game::ControlSet::default_local(),
                };
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let viewport = {
                    let (w, h) = shell.window_size();
                    (w.max(0.0) as u32, h.max(0.0) as u32)
                };
                let budget = std::env::var("MINT_LOAD_BUDGET_MS")
                    .ok()
                    .and_then(|ms| ms.parse().ok())
                    .map_or(
                        crate::match_loader::FRAME_BUDGET,
                        std::time::Duration::from_millis,
                    );
                let loader = crate::match_loader::MatchLoader::new(
                    Arc::clone(&self.vfs),
                    self.install_roots.clone(),
                    start.sources,
                    config,
                    map_label,
                    viewport,
                    self.scale_mode,
                    start.control.clone(),
                    shell.take_game_data(),
                    Some(shell.ui_resources()),
                );
                self.loading = Some(Box::new(Loading {
                    loader,
                    start,
                    budget,
                }));
                self.apply_flow(viewport, actions);
            }
            _ => {}
        }
    }
}

fn fit_to_window(game: &mut crate::game::Game, renderer: &Renderer) {
    let (width, height) = renderer.size();
    if game.viewport() != (width, height) {
        game.input(&InputEvent::Resized { width, height });
    }
}

impl crate::crash::Subject for AppState {
    fn running_match(&self) -> Option<&dyn crate::crash::MatchReport> {
        match &self.screen {
            AppScreen::Match(game) => Some(game.as_ref()),
            AppScreen::Shell(_) => None,
        }
    }

    fn take_failure(&mut self) -> Option<crate::crash::Failure> {
        match &mut self.screen {
            AppScreen::Match(game) => game.take_failure(),
            AppScreen::Shell(_) => None,
        }
    }
}

impl App for AppState {
    fn input(&mut self, event: &InputEvent) {
        if matches!(event, InputEvent::Resized { .. }) {
            match &mut self.screen {
                AppScreen::Shell(shell) => shell.input(event),
                AppScreen::Match(game) => game.input(event),
            }
            if let Some(shell_game) = self.shell_game.as_mut() {
                shell_game.resize(event);
            }
            self.campaign_flow.resize(event);
            return;
        }
        if let (
            Some(intro),
            InputEvent::Key {
                key: mint_app::Key::Escape,
                pressed: false,
            },
        ) = (self.intro.as_mut(), event)
        {
            if self.campaign_flow.movie_playing() {
                intro.skip_logos();
            }
        }
        if self.campaign_flow.movie_input(event) {
            return;
        }
        match &mut self.screen {
            AppScreen::Shell(shell) => {
                if !self.menu_held {
                    shell.input(event);
                }
            }
            AppScreen::Match(game) => game.input(event),
        }
    }

    fn update(&mut self, step: u64) {
        let preloading = self.shell_preloading;
        match &mut self.screen {
            AppScreen::Match(_) if self.campaign_flow.blocks_simulation() => {}
            AppScreen::Match(game) => {
                crate::render_perf::time(crate::render_perf::stage!("update"), || {
                    game.update(step);
                });
            }
            AppScreen::Shell(_) => {
                if let Some(shell_game) = self.shell_game.as_mut().filter(|_| !preloading) {
                    shell_game.update(step);
                }
            }
        }
    }

    fn frame(&mut self, dt: f32) {
        self.frame_dt = dt;
        let preloading = self.shell_preloading;
        self.cursors.advance(dt);
        self.campaign_flow.advance(dt);
        self.apply_sound_fade();
        match &mut self.screen {
            AppScreen::Shell(shell) => {
                if !self.menu_held {
                    shell.frame(dt);
                }
                if let Some(shell_game) = self.shell_game.as_mut().filter(|_| !preloading) {
                    if let Some(preview) = shell.cah_preview() {
                        shell_game.preview(&preview);
                    }
                    shell_game.frame(dt);
                }
            }
            AppScreen::Match(game) => {
                crate::render_perf::time(crate::render_perf::stage!("frame"), || game.frame(dt));
            }
        }
    }

    fn frame_stages(&self, stage: &mut dyn FnMut(&'static str, f64)) {
        crate::render_perf::last_frame_stages(stage);
    }

    fn draw(&mut self, renderer: &mut Renderer, blend: f32) -> mint_render::Result<()> {
        self.check_transitions(renderer);
        if let Some(result) = self.draw_movie(renderer) {
            return result;
        }
        let cover = self.campaign_flow.cover_alpha();
        match &mut self.screen {
            AppScreen::Shell(shell) => {
                let picture = shell.cah_picture_request(unix_seconds());
                if picture.is_some() {
                    renderer.capture_next_frame();
                }
                if let Some(shell_game) = self
                    .shell_game
                    .as_mut()
                    .filter(|game| game.kind() == crate::shell_game::ShellGameKind::CreateAHero)
                {
                    let window_width = shell.window_size().0;
                    let texture = shell.cah_map_size().and_then(|size| {
                        shell_game.render_to_rect(renderer, blend, size, window_width)
                    });
                    shell.cah_set_scene_texture(texture);
                }
                let (cover_w, cover_h) = shell.window_size();
                let boot = if self
                    .intro
                    .as_ref()
                    .is_some_and(crate::intro::Intro::title_shown)
                {
                    Some(crate::shell::BootScreen::Title)
                } else {
                    self.shell_hold
                        .as_ref()
                        .map(|_| crate::shell::BootScreen::Load)
                };
                let campaign_still = shell.campaign_still_up();
                let ring_rect = if campaign_still {
                    Some(crate::campaign_still::ring_rect(shell.window_size()))
                } else {
                    (boot == Some(crate::shell::BootScreen::Load))
                        .then(|| crate::shell_load_screen::ring_rect(shell.window_size()))
                };
                let (still_quad, foreground) = if campaign_still {
                    (shell.still_quad(renderer), shell.still_foreground(renderer))
                } else {
                    (None, None)
                };
                let ring = match ring_rect {
                    Some(rect) => {
                        if self.loading_ring.is_none() {
                            let library = self.campaign_flow.movie_library(&self.vfs);
                            self.loading_ring = crate::loading_ring::LoadingRing::open(
                                renderer,
                                library,
                                &self.install_roots,
                                crate::campaign_flow::LANGUAGE,
                            );
                        }
                        self.loading_ring
                            .as_mut()
                            .and_then(|ring| ring.quad(renderer, rect))
                    }
                    None => {
                        self.loading_ring = None;
                        None
                    }
                };
                let mut draws = match boot {
                    Some(screen) => shell.boot_draws(renderer, screen),
                    None if campaign_still => still_quad
                        .map(mint_render::ScreenDraw::Quad)
                        .into_iter()
                        .collect(),
                    None if self.menu_held => Vec::new(),
                    None => shell.build_and_draw(renderer),
                };
                draws.extend(ring.map(mint_render::ScreenDraw::Quad));
                draws.extend(foreground.map(mint_render::ScreenDraw::Quad));
                if cover > 0.0 {
                    draws.push(mint_render::ScreenDraw::Quad(
                        mint_render::UiQuad::new(0.0, 0.0, cover_w, cover_h)
                            .with_color_transform([0.0, 0.0, 0.0, cover.min(1.0)], [0.0; 4]),
                    ));
                }
                let held = self.shell_preloading || self.intro.is_some();
                let result = if let Some(shell_game) = self.shell_game.as_mut().filter(|_| !held) {
                    if let Some(hold) = self.shell_hold.as_mut().filter(|hold| hold.loaded) {
                        hold.warm_frames = hold.warm_frames.saturating_add(1);
                    }
                    shell_game.draw(renderer, blend, &draws)
                } else {
                    let mut scene = Scene::new(Camera::default()).with_ordered_screen(&draws);
                    scene.clear_color = crate::shell::SHELL_CLEAR;
                    renderer.render(&scene)
                };
                if let Some(stem) = picture {
                    let path = save_picture(renderer, &stem).unwrap_or_else(|error| {
                        println!("cah: the picture was not saved: {error}");
                        String::new()
                    });
                    shell.cah_picture_taken(&path);
                }
                result
            }
            AppScreen::Match(game) => {
                game.set_cover(cover);
                game.draw(renderer, blend)
            }
        }
    }

    fn cursor(&mut self) -> mint_app::CursorRequest {
        let want = match &self.screen {
            AppScreen::Shell(_) => crate::cursors::Want::Show(crate::cursors::CursorKind::Arrow),
            AppScreen::Match(game) => game.cursor_want(),
        };
        self.cursors.request(want)
    }

    fn speed(&self) -> f64 {
        match &self.screen {
            AppScreen::Shell(_) => 1.0,
            AppScreen::Match(game) => game.speed(),
        }
    }

    fn paused(&self) -> bool {
        let match_paused = match &self.screen {
            AppScreen::Shell(_) => None,
            AppScreen::Match(game) => Some(game.paused()),
        };
        steps_dropped(self.campaign_flow.blocks_simulation(), match_paused)
    }

    fn exit_requested(&self) -> bool {
        match &self.screen {
            AppScreen::Shell(shell) => shell.exit_requested(),
            AppScreen::Match(_) => false,
        }
    }
}

fn print_tick_hashes(hashes: &[u64], every: u32) {
    let every = usize::try_from(every).unwrap_or(usize::MAX).max(1);
    let mut digest: u64 = 0xcbf2_9ce4_8422_2325;
    for (index, hash) in hashes.iter().enumerate() {
        for byte in hash.to_le_bytes() {
            digest = (digest ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
        let tick = index.saturating_add(1);
        if tick % every == 0 {
            println!("hash: tick {tick} {hash:#018x}");
        }
    }
    println!(
        "hash: digest of {} per-tick hash(es) {digest:#018x}",
        hashes.len()
    );
}

fn open_offscreen_renderer((width, height): (u32, u32)) -> Result<Renderer, String> {
    let started = std::time::Instant::now();
    let dir = crate::user_data::pipeline_cache_dir();
    let renderer = Renderer::offscreen_with_pipeline_cache(width, height, dir.as_deref())
        .map_err(|e| e.to_string())?;
    println!(
        "renderer: created in {:.0} ms, pipeline cache {:?}",
        started.elapsed().as_secs_f64() * 1000.0,
        renderer.pipeline_cache_state()
    );
    Ok(renderer)
}

const HEADLESS_STALL_FRAMES: u32 = 18_000;

fn run_ticks_headless(start: AppStart, size: (u32, u32), options: &Options) -> Result<(), String> {
    let mut renderer = open_offscreen_renderer(size)?;
    let app = start
        .build(&mut renderer, size)
        .inspect_err(|message| crate::crash::report_message("loading", message))?;
    let mut app = crate::crash::Dumping::new(app, crate::crash::root());
    app.input(&InputEvent::Resized {
        width: size.0,
        height: size.1,
    });
    let target = u64::from(options.ticks);
    crate::render_perf::keep_run();
    let mut driver = crate::drive::Driver::new(options.drive_dt);
    let mut frames = 0u64;
    let mut idle = 0u32;
    let mut last = None;
    loop {
        let reached = match &app.app_mut().screen {
            AppScreen::Match(game) => Some((game.sim_frame(), game.state_hash())),
            AppScreen::Shell(_) => None,
        };
        if let Some((frame, hash)) = reached.filter(|(frame, _)| *frame >= target) {
            println!(
                "demo: {frame} tick(s) headless after {frames} frame(s), state hash {hash:#018x} (MINT_ROTWK_HASH={})",
                crate::skirmish::frame_hash_label(crate::skirmish::frame_hash())
            );
            if let (Some(every), AppScreen::Match(game)) =
                (options.hash_every, &app.app_mut().screen)
            {
                print_tick_hashes(game.tick_hashes(), every);
            }
            crate::render_perf::report_run();
            crate::perf::report();
            return Ok(());
        }
        let frame = reached.map(|(frame, _)| frame);
        idle = if frame == last {
            idle.saturating_add(1)
        } else {
            0
        };
        last = frame;
        if idle >= HEADLESS_STALL_FRAMES {
            return Err(format!(
                "--ticks {}: no tick in {idle} frame(s), {} at frame {frames}",
                options.ticks,
                frame.map_or_else(
                    || String::from("still in the shell"),
                    |frame| format!("match at tick {frame}")
                ),
            ));
        }
        if app.exit_requested() {
            return Err(format!(
                "--ticks {}: the app asked to exit at frame {frames}",
                options.ticks
            ));
        }
        driver.frame(&mut app, &mut renderer)?;
        frames = frames.saturating_add(1);
    }
}

fn run_headless_match(
    vfs: &Arc<Vfs>,
    map_bytes: &[u8],
    map_label: String,
    minutes: f64,
    options: &Options,
) -> Result<(), String> {
    let mut renderer = Renderer::offscreen(
        WindowConfig::default().width,
        WindowConfig::default().height,
    )
    .map_err(|e| e.to_string())?;
    let config = match_config(options, &map_label);
    let (mut data, line) = crate::load_game_data_or_exit(vfs);
    println!("{line}");
    crate::game_data::compose_for_match(&mut data, vfs, &map_label)?;
    let object_index = data.object_index.take();
    let (skirmish, allow_ring_heroes) = config.scene_filter();
    let scene = load_match_scene(
        &mut renderer,
        vfs,
        map_bytes,
        map_label,
        &data.templates,
        skirmish,
        allow_ring_heroes,
        object_index,
        None,
    )?;
    let sources = match options.passability.as_deref() {
        Some(raw) => crate::game::PassabilitySources::parse(raw)?,
        None => crate::game::PassabilitySources::default(),
    };
    let setup = crate::game::build_match(&scene, &data, vfs, &config, sources)?;
    let mut simulation = setup.simulation;
    let ids = simulation
        .rules()
        .ids()
        .ok_or("the ruleset registered no components")?;
    let players = simulation.rules().player_count();

    let avoidance = parse_avoidance(options.avoidance.as_deref())?;
    println!("match: avoidance={avoidance:?}");
    simulation
        .submit(
            mint_sim::PlayerId::new(0),
            crate::rules::RotwkCommand::SetMovementModes(crate::unit::Modes {
                avoidance,
                ..crate::unit::Modes::default()
            }),
        )
        .map_err(|e| e.to_string())?;

    if options.fight_nearest {
        submit_fight_orders_nearest(&mut simulation)?;
    } else if options.fight {
        submit_fight_orders(&mut simulation)?;
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cap_frames = (minutes * 60.0 * f64::from(mint_app::SIMULATION_HZ)).max(0.0) as u64;
    let sample_every = u64::from(mint_app::SIMULATION_HZ) * 60;
    let mut peaks = vec![(0u32, 0u32); usize::from(players)];
    let started = std::time::Instant::now();
    let mut max_step_ms = 0f64;
    let mut frame = 0u64;
    let mut outcome = None;
    let mut fx_seen = (0u64, 0u64);
    let mut fx_peak = 0usize;
    let mut fx_by_list: std::collections::BTreeMap<crate::fx::FxId, u64> =
        std::collections::BTreeMap::new();
    while frame < cap_frames {
        let step_started = std::time::Instant::now();
        let ticked = simulation.tick().map_err(|e| e.to_string())?;
        crate::perf::note_tick_returned();
        let step_ms = step_started.elapsed().as_secs_f64() * 1000.0;
        max_step_ms = max_step_ms.max(step_ms);
        frame += 1;
        let fired = simulation
            .rules()
            .fx_events(simulation.state(), ticked.frame);
        fx_peak = fx_peak.max(fired.len());
        for event in fired {
            match event.source {
                crate::fx::FxSource::Power => fx_seen.0 = fx_seen.0.saturating_add(1),
                crate::fx::FxSource::Weapon => fx_seen.1 = fx_seen.1.saturating_add(1),
            }
            let count = fx_by_list.entry(event.list).or_insert(0);
            *count = count.saturating_add(1);
        }
        let counts = count_buildings_and_army(simulation.state(), ids, players);
        for (peak, current) in peaks.iter_mut().zip(&counts) {
            peak.0 = peak.0.max(current.0);
            peak.1 = peak.1.max(current.1);
        }
        if frame % sample_every == 0 {
            let minute = frame / u64::from(mint_app::SIMULATION_HZ) / 60;
            println!(
                "match: minute {minute}, buildings/army {}",
                counts
                    .iter()
                    .enumerate()
                    .map(|(player, (buildings, army))| format!("p{player}={buildings}/{army}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            for line in simulation.rules().attack_census(simulation.state(), ids) {
                println!("{line}");
            }
            let economy = crate::rules::RotwkRules::economy_ids(ids);
            for line in crate::economy::census(simulation.state(), economy) {
                println!("{line}");
            }
            for line in simulation.rules().experience_census(simulation.state()) {
                println!("{line}");
            }
            for line in simulation.rules().spell_census(simulation.state()) {
                println!("{line}");
            }
        }
        if let Some(clock) = clock_entity(simulation.state(), ids) {
            if let Some(result) = simulation.state().get(ids.outcome, clock) {
                outcome = Some(*result);
                break;
            }
        }
    }
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mean_step_ms = if frame > 0 {
        elapsed_ms / frame as f64
    } else {
        0.0
    };
    let minutes_reached = frame as f64 / f64::from(mint_app::SIMULATION_HZ) / 60.0;
    match outcome {
        Some(result) => println!(
            "match: player {} wins at frame {frame} ({minutes_reached:.1} minute(s))",
            result.winner.get()
        ),
        None => println!(
            "match: no winner after {frame} frame(s) ({minutes_reached:.1} minute(s)) - the cap"
        ),
    }
    for (player, (buildings, army)) in peaks.iter().enumerate() {
        println!("match: player {player} peaked at {buildings} building(s), {army} army unit(s)");
    }
    let end_tick = u32::try_from(frame).unwrap_or(u32::MAX);
    for line in simulation
        .rules()
        .match_stats_lines(simulation.state(), ids, end_tick)
    {
        println!("{line}");
    }
    for line in simulation.rules().attack_census(simulation.state(), ids) {
        println!("{line} (final)");
    }
    println!(
        "fx: {} power and {} weapon event(s) over the match, at most {fx_peak} in one frame",
        fx_seen.0, fx_seen.1
    );
    let mut commonest: Vec<(crate::fx::FxId, u64)> = fx_by_list.into_iter().collect();
    commonest.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (id, count) in commonest.iter().take(8) {
        println!(
            "fx:   {count:>7} x {}",
            simulation.rules().fx_name(*id).unwrap_or("?")
        );
    }
    println!(
        "match: step cost over the whole match - mean {mean_step_ms:.3} ms, peak {max_step_ms:.3} ms, \
         {frame} step(s) in {elapsed_ms:.0} ms"
    );
    if options.check_passability {
        let report = count_units_on_blocked_cells(&simulation, ids);
        println!(
            "passability check: {} living unit(s), {} on a blocked cell ({} corpse, {} not), \
             {} off the grid - blocked by: {} cliff, {} water, {} deep water, {} obstacle, \
             {} boundary",
            report.total,
            report.blocked,
            report.blocked_corpses,
            report.blocked.saturating_sub(report.blocked_corpses),
            report.off_grid,
            report.cliff,
            report.water,
            report.deep_water,
            report.obstacle,
            report.boundary,
        );
    }
    crate::perf::report();
    Ok(())
}

#[cfg(feature = "ini-census")]
fn ini_census_command(
    vfs: &Arc<Vfs>,
    map_bytes: &[u8],
    map_label: String,
    options: &Options,
    out_path: &Path,
) -> ExitCode {
    let started = std::time::Instant::now();
    if let Err(error) = run_headless_match(vfs, map_bytes, map_label, 0.1, options) {
        eprintln!(
            "warning: the --run-match step failed ({error}); the census covers load-time INI only"
        );
    }
    let documents = match crate::game_data::load_documents_for_census(vfs) {
        Ok(documents) => documents,
        Err(refusal) => {
            refusal.report();
            return ExitCode::FAILURE;
        }
    };
    let document_count = documents.len();
    let accesses = mint_sage_ini::coverage::take();
    let census = mint_sage_ini_coverage::census::Census::build(&documents, &accesses);

    let mut world = mint_sim::World::new();
    let mut registry = mint_sim::module::ModuleRegistry::new(&mut world);
    mint_sage_modules::register_every_family(&mut registry, &mut world);
    let is_registered = |name: &str| registry.is_registered(name);

    let Some(repo_root) = find_repo_root_for_census() else {
        eprintln!("error: could not locate the workspace root above this binary");
        return ExitCode::FAILURE;
    };
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&repo_root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned());
    let unix_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let meta = mint_sage_ini_coverage::census::Meta {
        date: format!("{unix_seconds} unix seconds"),
        commit,
        command: std::env::args().collect::<Vec<_>>().join(" "),
        wall_time: format!("{:.1}s", started.elapsed().as_secs_f64()),
        documents: document_count,
    };
    let report = census.render(&repo_root, &is_registered, &meta);
    match std::fs::write(out_path, report) {
        Ok(()) => {
            println!(
                "census: {document_count} document(s), {} elapsed, written to {}",
                meta.wall_time,
                out_path.display()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: writing {}: {error}", out_path.display());
            ExitCode::FAILURE
        }
    }
}

#[cfg(feature = "ini-census")]
fn find_repo_root_for_census() -> Option<PathBuf> {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join("Cargo.toml").is_file() && dir.join("deny.toml").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

pub(crate) fn clock_entity(
    world: &mint_sim::World,
    ids: crate::rules::Ids,
) -> Option<mint_sim::Entity> {
    world
        .store(ids.clock)
        .and_then(|store| store.entities().first().copied())
}

fn count_buildings_and_army(
    world: &mint_sim::World,
    ids: crate::rules::Ids,
    players: u8,
) -> Vec<(u32, u32)> {
    let mut counts = vec![(0u32, 0u32); usize::from(players)];
    let Some(store) = world.store(ids.owner) else {
        return counts;
    };
    for (entity, owner) in store.iter() {
        if !world.is_live(entity) {
            continue;
        }
        let Some(bucket) = counts.get_mut(usize::from(owner.0.get())) else {
            continue;
        };
        let is_structure = world
            .get(ids.shape, entity)
            .is_some_and(|shape| shape.kind_of.contains(mint_sage_asset::KindOf::STRUCTURE));
        if is_structure {
            bucket.0 = bucket.0.saturating_add(1);
        } else if world.contains(ids.health, entity) {
            bucket.1 = bucket.1.saturating_add(1);
        }
    }
    counts
}

fn render_shell_screenshot(
    vfs: &Arc<Vfs>,
    install_roots: &[PathBuf],
    screen: &str,
    path: &std::path::Path,
    options: &Options,
) -> Result<(), String> {
    let config = WindowConfig::default();
    let (width, height) = options.size.unwrap_or((config.width, config.height));
    let mut renderer = Renderer::offscreen(width, height).map_err(|e| e.to_string())?;
    #[allow(clippy::cast_precision_loss)]
    let window_size = (width as f32, height as f32);
    let mut shell = crate::shell::ShellApp::load(
        Arc::clone(vfs),
        install_roots.to_vec(),
        window_size,
        options.scale_mode,
        &mut renderer,
    )?;
    shell.force_screen(screen)?;
    shell.set_text_anchor(options.text_anchor);
    for _ in 0..options.ui_frames {
        shell.frame(UI_FRAME.as_secs_f32());
    }
    println!("{}", shell.music_line());
    let passes = shell.settle_text(&mut renderer, MAX_TEXT_SETTLE_PASSES);
    if passes >= MAX_TEXT_SETTLE_PASSES {
        eprintln!(
            "shell: text still not settled after {MAX_TEXT_SETTLE_PASSES} pass(es) - the \
             screenshot may show incomplete text"
        );
    }
    let timed = std::time::Instant::now();
    let draws = shell.build_and_draw(&mut renderer);
    let build_ms = timed.elapsed().as_secs_f64() * 1000.0;
    let mut scene = Scene::new(Camera::default()).with_ordered_screen(&draws);
    scene.clear_color = crate::shell::SHELL_CLEAR;
    renderer.render(&scene).map_err(|e| e.to_string())?;
    let pixels = renderer.read_pixels().map_err(|e| e.to_string())?;
    crate::game::write_tga(path, width, height, &pixels).map_err(|e| e.to_string())?;
    let stats = shell.active_stats();
    let p = shell.active_build_profile();
    let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
    println!(
        "wrote {width}x{height} shell screenshot ({screen}) to {} - build_and_draw \
         {build_ms:.3} ms total (settled state, {passes} settle pass(es) beforehand), \
         {} shape(s), {} run(s), {} text field(s), {} mask(s), {} live instance(s)",
        path.display(),
        stats.shapes,
        stats.runs,
        stats.text,
        stats.masks,
        shell.active_live_instances()
    );
    println!(
        "shell: build split - render_list {:.3} ms, meshes {:.3} ms, plan {:.3} ms, \
         shapes {:.3} ms, text_fields {:.3} ms, surfaces {:.3} ms, atlas_upload {:.3} ms, \
         profiled total {:.3} ms ({} surface(s))",
        ms(p.render_list),
        ms(p.meshes),
        ms(p.plan),
        ms(p.shapes),
        ms(p.text_fields),
        ms(p.surfaces),
        ms(p.atlas_upload),
        ms(p.total()),
        stats.surfaces,
    );
    Ok(())
}

fn render_screenshot(
    vfs: &Arc<Vfs>,
    map_bytes: &[u8],
    map_label: String,
    path: &std::path::Path,
    options: &Options,
) -> Result<(), String> {
    let loading = std::time::Instant::now();
    let config = WindowConfig::default();
    let (width, height) = options.size.unwrap_or((config.width, config.height));
    let mut renderer = Renderer::offscreen(width, height).map_err(|e| e.to_string())?;
    let match_setup = match_config(options, &map_label);
    let phase = std::time::Instant::now();
    let (mut data, line) = crate::load_game_data_or_exit(vfs);
    let selection_flash = crate::selection_flash::SelectionFlash::new(
        data.selection_flash_saturation,
        data.selection_flash_house_color,
    );
    let data_ms = phase.elapsed().as_secs_f64() * 1000.0;
    println!("{line}");
    let phase = std::time::Instant::now();
    crate::game_data::compose_for_match(&mut data, vfs, &map_label)?;
    let object_index = data.object_index.take();
    let (skirmish, allow_ring_heroes) = match_setup.scene_filter();
    let mut scene = load_match_scene(
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
    let scene_ms = phase.elapsed().as_secs_f64() * 1000.0;
    let sources = match options.passability.as_deref() {
        Some(raw) => crate::game::PassabilitySources::parse(raw)?,
        None => crate::game::PassabilitySources::default(),
    };
    let phase = std::time::Instant::now();
    let setup = crate::game::build_match(&scene, &data, vfs, &match_setup, sources)?;
    let match_ms = phase.elapsed().as_secs_f64() * 1000.0;
    let mut simulation = setup.simulation;
    let squad = match options.focus_player {
        Some(player) => setup
            .starts
            .get(usize::from(player))
            .copied()
            .ok_or_else(|| {
                format!(
                    "--focus-player {player}: this match has {} start(s)",
                    setup.starts.len()
                )
            })?,
        None => setup.focus,
    };
    let mut shroud_view = crate::game::attach_shroud(&mut renderer, &mut scene, &simulation);
    let visuals = crate::visuals::UnitVisuals::upload(&mut renderer)?;
    let phase = std::time::Instant::now();
    let (mut units, unit_art, player_colors) = crate::game::load_unit_art(
        &mut renderer,
        vfs,
        &scene,
        &setup.kinds.names(),
        &setup.kinds.infantry(),
        &setup.kinds.move_only(),
        &setup.colours,
    );
    crate::game::attach_frame_events(&mut units, &unit_art, simulation.rules());
    let art_ms = phase.elapsed().as_secs_f64() * 1000.0;
    let projectile_art = crate::game::load_projectile_art(
        &mut renderer,
        vfs,
        &scene,
        &mut units,
        &simulation.rules().projectile_templates(),
    );
    println!(
        "load: scene {scene_ms:.0} ms, game data {data_ms:.0} ms, match {match_ms:.0} ms, \
         unit art {art_ms:.0} ms ({} template(s)), {:.0} ms from renderer to first frame's inputs",
        setup.kinds.len(),
        loading.elapsed().as_secs_f64() * 1000.0
    );
    let mut modes =
        crate::game::parse_modes(options.search.as_deref(), options.smoothing.as_deref())?;
    modes.avoidance = parse_avoidance(options.avoidance.as_deref())?;
    let goal = options.order.map_or(
        Vec3::new(squad.x + DEMO_ORDER_DISTANCE, squad.y, squad.z),
        |(x, y)| Vec3::new(x, y, squad.z),
    );

    let mut camera = RtsCamera::new(Vec3::Z);
    crate::game::frame_camera(&mut camera, &scene);
    crate::game::apply_tactical_lens(&mut camera, &data.camera, (width, height));
    let framed = if options.fight || options.fight_nearest || options.ticks == 0 {
        squad
    } else {
        Vec3::new(
            (squad.x + goal.x) * 0.5,
            (squad.y + goal.y) * 0.5,
            (squad.z + goal.z) * 0.5,
        )
    };
    let framed = options.look_at.map_or(framed, |(x, y)| {
        Vec3::new(x, y, crate::game::ground_at(&scene, x, y))
    });
    if let Some(degrees) = options.pitch {
        camera.set_pitch(degrees.to_radians());
    }
    crate::game::focus_on(
        &mut camera,
        framed,
        options
            .view
            .unwrap_or(if options.fight || options.fight_nearest {
                FIGHT_VIEW_DISTANCE
            } else {
                SCREENSHOT_VIEW_DISTANCE
            }),
    );

    for _ in 0..options.queue {
        for player in [crate::game::LOCAL_PLAYER, crate::game::ENEMY_PLAYER] {
            let structures = crate::game::producing_structures(&simulation, player);
            if structures.is_empty() {
                continue;
            }
            simulation
                .submit(
                    player,
                    crate::rules::RotwkCommand::SetSelection {
                        replace: true,
                        units: structures,
                    },
                )
                .map_err(|e| e.to_string())?;
            simulation
                .submit(player, crate::rules::RotwkCommand::QueueUnit { button: 0 })
                .map_err(|e| e.to_string())?;
        }
    }

    if let Some((name, x, y, angle)) = &options.construct {
        let Some(kind) = setup.kinds.kind(name) else {
            return Err(format!(
                "--construct: \"{name}\" is not a template this match knows"
            ));
        };
        let player = crate::game::LOCAL_PLAYER;
        let porters = crate::game::porters_of(&simulation, player);
        if porters.is_empty() {
            println!("shell: --construct found no porter for player {player} - nothing to try");
        } else {
            simulation
                .submit(
                    player,
                    crate::rules::RotwkCommand::SetSelection {
                        replace: true,
                        units: porters,
                    },
                )
                .map_err(|e| e.to_string())?;
            simulation
                .submit(
                    player,
                    crate::rules::RotwkCommand::Construct {
                        kind: kind.0,
                        x: *x,
                        y: *y,
                        angle: *angle,
                    },
                )
                .map_err(|e| e.to_string())?;
            println!("shell: --construct {name} at ({x:.0}, {y:.0}), angle {angle:.2}");
        }
    }

    if let Some(which) = options.select_menu.as_deref() {
        let player = crate::game::LOCAL_PLAYER;
        let units = match which {
            "porter" => crate::game::porters_of(&simulation, player),
            "producer" => crate::game::producing_structures(&simulation, player),
            other => return Err(format!("--select-menu: unreachable value {other:?}")),
        };
        if units.is_empty() {
            println!("shell: --select-menu {which} found nothing to select for player {player}");
        } else {
            simulation
                .submit(
                    player,
                    crate::rules::RotwkCommand::SetSelection {
                        replace: true,
                        units: units.clone(),
                    },
                )
                .map_err(|e| e.to_string())?;
            println!(
                "shell: --select-menu {which} - {} unit(s) selected",
                units.len()
            );
        }
    }

    let research =
        |simulation: &mut mint_sim::Simulation<crate::rules::RotwkRules>| -> Result<(), String> {
            for name in &options.research {
                let commands = simulation.rules().research_commands(
                    simulation.state(),
                    crate::game::LOCAL_PLAYER,
                    name,
                );
                println!(
                    "shell: --research {name} - {} object(s) offer it",
                    commands.len() / 2
                );
                for command in commands {
                    simulation
                        .submit(crate::game::LOCAL_PLAYER, command)
                        .map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        };
    if options.research_at == 0 {
        research(&mut simulation)?;
    }

    let mut goal_marker = None;
    if options.ticks > 0 && (options.fight || options.fight_nearest) {
        run_scripted_fight(
            &mut simulation,
            options.ticks,
            modes,
            options.trace,
            options.fight_nearest,
            options.check_passability,
            options.check_stuck,
        )?;
    } else if options.ticks > 0
        && (options.construct.is_some()
            || options.select_menu.is_some()
            || !options.research.is_empty())
    {
        let stepping = std::time::Instant::now();
        for step in 0..options.ticks {
            if step > 0 && step == options.research_at {
                research(&mut simulation)?;
            }
            simulation.tick().map_err(|e| e.to_string())?;
        }
        print_step_cost(options.ticks, stepping);
        if !options.research.is_empty() {
            for line in crate::game::horde_report(&simulation) {
                println!("{line}");
            }
        }
        println!(
            "demo: {} tick(s) at {}, state hash {:#018x}",
            options.ticks,
            modes.config(),
            simulation.state_hash()
        );
    } else if options.ticks > 0 {
        let ids = simulation
            .rules()
            .ids()
            .ok_or("the ruleset registered no components")?;
        let units: Vec<mint_sim::Entity> = simulation
            .state()
            .store(ids.body)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        let player = crate::game::LOCAL_PLAYER;
        simulation
            .submit(player, crate::rules::RotwkCommand::SetMovementModes(modes))
            .map_err(|e| e.to_string())?;
        simulation
            .submit(
                player,
                crate::rules::RotwkCommand::SetSelection {
                    replace: true,
                    units,
                },
            )
            .map_err(|e| e.to_string())?;
        simulation
            .submit(
                player,
                crate::rules::RotwkCommand::MoveTo {
                    x: goal.x,
                    y: goal.y,
                    queue: false,
                },
            )
            .map_err(|e| e.to_string())?;
        let order_frame = simulation.frame();
        let stuck_snapshot_at = options.ticks.saturating_sub(STUCK_WINDOW_TICKS);
        let mut stuck_before = None;
        let stepping = std::time::Instant::now();
        for step in 0..options.ticks {
            simulation.tick().map_err(|e| e.to_string())?;
            if options.check_stuck && step == stuck_snapshot_at {
                stuck_before = Some(snapshot_positions(&simulation, ids));
            }
        }
        print_step_cost(options.ticks, stepping);
        goal_marker = Some((goal, order_frame));
        for line in crate::game::horde_report(&simulation) {
            println!("{line}");
        }
        println!(
            "demo: {} tick(s) at {}, ordered to ({:.0}, {:.0}), state hash {:#018x}",
            options.ticks,
            modes.config(),
            goal.x,
            goal.y,
            simulation.state_hash()
        );
        if options.check_passability {
            let report = count_units_on_blocked_cells(&simulation, ids);
            println!(
                "passability check: {} living unit(s), {} on a blocked cell ({} corpse, {} not), \
                 {} off the grid - blocked by: {} cliff, {} water, {} deep water, {} obstacle, \
                 {} boundary",
                report.total,
                report.blocked,
                report.blocked_corpses,
                report.blocked.saturating_sub(report.blocked_corpses),
                report.off_grid,
                report.cliff,
                report.water,
                report.deep_water,
                report.obstacle,
                report.boundary,
            );
        }
        if let Some(before) = stuck_before {
            let report = count_stuck_units(&simulation, ids, &before);
            println!(
                "stuck check: {} unit(s) under order, out of weapon range, and unmoved by \
                 more than {STUCK_EPSILON:.1} unit(s) over the last {} tick(s){}",
                report.count,
                options.ticks.min(STUCK_WINDOW_TICKS),
                format_stuck_sample(&report.sample)
            );
        }
    }

    let mut previous = crate::render_state::PoseTrack::new();
    let interpolate = options.blend.is_some();
    if interpolate {
        if let Some(ids) = simulation.rules().ids() {
            crate::render_state::step_remembering(&mut simulation, &mut previous, ids)
                .map_err(|e| e.to_string())?;
        }
    }
    let install_roots: Vec<PathBuf> = std::iter::once(options.primary.clone())
        .chain(options.extra_installs.iter().cloned())
        .collect();
    let ui = crate::ui_resources::UiResources::load(&install_roots);
    println!(
        "strings: {} localisation label(s) from {} .str file(s) ({} failed)",
        ui.strings.len(),
        ui.string_files,
        ui.string_failures,
    );
    let fonts = Arc::clone(&ui.fonts);
    let strings = Arc::clone(&ui.strings);
    let placed_scales = crate::game::placed_scales(&simulation, &scene.objects);
    let mut state = crate::game::MatchState {
        view_box_z: crate::apt_overlay::average_ground_z(
            &scene.height_map,
            scene.terrain.vertical_scale,
        ),
        scene,
        camera,
        simulation,
        visuals,
        units,
        unit_art,
        placed_scales,
        projectile_art,
        player_colors,
        vfs: Arc::clone(vfs),
        goal: goal_marker,
        stance_flyout: None,
        ui_panels_sent: [false; 3],
        scroll_pending: 0.0,
        script_camera_clock: (0, 0.0),
        previous,
        snapshot: crate::snapshot::FrameSnapshot::default(),
        shroud_view: shroud_view.take(),
        shroud_viewer: mint_sim::PlayerId::new(options.shroud_viewer.unwrap_or(0)),
        damage_thresholds: data.damage_thresholds,
        tdfx: crate::tdfx::TdfxPresenter::default(),
        shroud_fade: mint_sage_scene::ShroudFade::default(),
        shroud_clock: None,
        shroud_source: None,
        kinds: Arc::clone(&setup.kinds),
        local_evil: setup.local_evil,
        local_faction_icon: setup.local_faction_icon.clone(),
        local_heroes: setup.local_heroes.clone(),
        local_side: setup.local_side.clone(),
        local_template: setup.local_template.clone(),
        cinematic: crate::cinematic::Cinematic::default(),
        fx: crate::fx_present::FxPresenter::default(),
        bone_fx: crate::game::BoneFxTracker::new(),
        level_ups: crate::level_up::LevelWatch::default(),
        selection_flash,
        floating: crate::floating_text::FloatingTexts::default(),
        caption_points: crate::game_data::SHIPPED_DRAWABLE_CAPTION_POINTS,
        named_timer_style: crate::named_timer::TimerStyle::default(),
        fonts: Arc::clone(&fonts),
        strings: Arc::clone(&strings),
    };
    println!(
        "{}",
        crate::fx_present::census_line(
            &state.scene.particle_scene,
            state.simulation.rules(),
            state.fx.shown()
        )
    );
    if let Some(name) = options.shroud.as_deref() {
        scene_shroud_mode(&mut state, name)?;
    }
    if let Some(name) = options.pca_gate.as_deref() {
        use mint_sage_scene::water_fx::PcaGate;
        let gate = [PcaGate::Line, PcaGate::Band, PcaGate::Open]
            .into_iter()
            .find(|g| g.label() == name)
            .ok_or_else(|| format!("--pca-gate wants line|band|open, got {name}"))?;
        state.scene.fx_water.gate_wanted = gate;
        println!("water: PCA gate {}", gate.label());
    }
    if let Some(name) = options.pca_rate.as_deref() {
        use mint_sage_scene::water_fx::PcaRate;
        let rate = [PcaRate::PerLogicFrame, PcaRate::PerSecond]
            .into_iter()
            .find(|r| r.label() == name)
            .ok_or_else(|| format!("--pca-rate wants per-logic-frame|per-second, got {name}"))?;
        state.scene.fx_water.pca_rate = rate;
        println!("water: PCA basis rate {}", rate.label());
    }
    if let Some(name) = options.sun.as_deref() {
        let wanted = [
            mint_sage_scene::SunConvention::Stored,
            mint_sage_scene::SunConvention::MirroredAzimuth,
        ]
        .into_iter()
        .find(|c| c.label() == name)
        .ok_or_else(|| format!("--sun wants stored|mirrored-azimuth, got {name}"))?;
        if wanted != state.scene.sun {
            state.scene.sun = wanted;
            state.scene.rig.mirror_azimuth();
        }
        println!("render: sun {}", wanted.label());
    }
    if let Some(name) = options.model_lights.as_deref() {
        let wanted = mint_sage_scene::ModelLights::ALL
            .into_iter()
            .find(|m| m.label().split(' ').next() == Some(name))
            .ok_or_else(|| format!("--model-lights wants retail|terrain, got {name}"))?;
        state.scene.model_lights = wanted;
        println!("render: model lights {}", wanted.label());
    }
    if let Some(name) = options.shadow_filter.as_deref() {
        let wanted = mint_sage_scene::ShadowFilter::ALL
            .into_iter()
            .find(|f| f.label().split(' ').next() == Some(name))
            .ok_or_else(|| format!("--shadow-filter wants tent|retail|contact|wide, got {name}"))?;
        if let Some(pass) = state.scene.shadows.as_mut() {
            pass.filter = wanted;
        }
        println!("render: shadow filter {}", wanted.label());
    }
    if let (Some(on), Some(pass)) = (options.shadow_cascades, state.scene.shadows.as_mut()) {
        pass.cascaded = on;
        println!(
            "render: shadow cascades {}",
            if on { "on" } else { "off (one map)" }
        );
    }
    if let (Some(on), Some(pass)) = (options.contact_shadows, state.scene.shadows.as_mut()) {
        pass.contact_shadows = on;
        println!("render: contact shadows {}", if on { "on" } else { "off" });
    }
    if let Some(name) = options.water_detail.as_deref() {
        let detail = mint_sage_scene::water_fx::FxDetail::ALL
            .into_iter()
            .find(|d| d.label().eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("--water-detail wants ultra|high|medium|low, got {name}"))?;
        state.scene.fx_water.detail = detail;
        crate::scene::apply_fx_visibility(&mut state.scene, true);
        println!("water: fx detail {}", detail.label());
    }
    if let Some(name) = options.waves.as_deref() {
        scene_wave_rung(&mut state, name)?;
    }
    if let Some(name) = options.reflection.as_deref() {
        scene_reflection_mode(&mut state, name)?;
    }
    if let Some((name, x, y)) = &options.preview {
        let kind = setup
            .kinds
            .kind(name)
            .ok_or_else(|| format!("--preview: \"{name}\" is not a template this match knows"))?;
        let units = state.simulation.rules().ids().map_or_else(Vec::new, |ids| {
            crate::render_state::RenderState::new(state.simulation.state(), 0.0).units(
                ids,
                crate::game::LOCAL_PLAYER,
                crate::game::LOCAL_PLAYER,
                |weapon, state2, moved, now, conditions| {
                    state
                        .simulation
                        .rules()
                        .weapon_cycle_seconds(weapon, state2, moved, now, conditions)
                },
                |weapon| state.simulation.rules().weapon_firing_duration(weapon),
            )
        });
        let facing = setup
            .kinds
            .get(kind)
            .and_then(|entry| entry.record.placement_view_angle)
            .filter(|degrees| degrees.is_finite())
            .map_or(0.0, f32::to_radians);
        let code = crate::game::placement_is_legal(&state, &units, kind, *x, *y, facing);
        println!(
            "preview: {name} at ({x:.0}, {y:.0}), facing {:.0} degree(s) - {}",
            facing.to_degrees(),
            if code == 0 {
                "legal".to_owned()
            } else {
                format!("illegal (red), code {code}")
            }
        );
        state.visuals.placement_ghost = Some(crate::visuals::PlacementGhost {
            kind: kind.0,
            x: *x,
            y: *y,
            facing,
            code,
            span_code: None,
        });
    }
    if let Some(mode) = options.particle_flipbook {
        state.scene.particle_scene.set_flipbook(mode);
        println!(
            "render: particle flipbook {} [{} of {}]",
            mode.label(),
            mode.rung(),
            mint_sage_scene::FlipbookMode::ALL.len()
        );
    }
    for line in crate::game::match_census(&state.simulation) {
        println!("{line}");
    }
    let mut report = crate::report::SecondReport::default();

    #[allow(clippy::cast_precision_loss)]
    let window_size = {
        let (w, h) = renderer.size();
        (w as f32, h as f32)
    };
    let mut apt_overlay = if options.apt_overlay {
        match crate::apt_overlay::AptOverlay::load(
            &ui,
            &install_roots,
            window_size,
            &mut renderer,
            Arc::clone(vfs),
            Arc::clone(&data.mapped_images),
            Arc::clone(&data.command_buttons),
        ) {
            Ok(mut overlay) => {
                overlay.set_scale_mode(options.scale_mode);
                overlay.set_text_anchor(options.text_anchor);
                if let Some(name) = options.image_modes.as_deref() {
                    let mode = match name {
                        "decoded" => mint_sage_apt_viewer::ImageModes::Decoded,
                        "matte" => mint_sage_apt_viewer::ImageModes::Matte,
                        "unset-solid" => mint_sage_apt_viewer::ImageModes::UnsetSolid,
                        "alpha-opaque" => mint_sage_apt_viewer::ImageModes::AlphaOpaque,
                        "radial-disc" => mint_sage_apt_viewer::ImageModes::RadialDisc,
                        other => {
                            return Err(format!(
                                "--image-modes wants decoded|matte|unset-solid|alpha-opaque|radial-disc, got {other}"
                            ))
                        }
                    };
                    overlay.set_image_modes(mode);
                }
                Some(overlay)
            }
            Err(error) => {
                println!("apt overlay: {error} - screenshot will not show it");
                None
            }
        }
    } else {
        None
    };
    let units = state.simulation.rules().ids().map_or_else(Vec::new, |ids| {
        let render_state = crate::render_state::RenderState::new(state.simulation.state(), 0.0);
        render_state.units(
            ids,
            state.shroud_viewer,
            crate::game::LOCAL_PLAYER,
            |weapon, state2, moved, now, conditions| {
                state
                    .simulation
                    .rules()
                    .weapon_cycle_seconds(weapon, state2, moved, now, conditions)
            },
            |weapon| state.simulation.rules().weapon_firing_duration(weapon),
        )
    });
    let selected_hud = crate::hud::read_hud(&state, &data, &units, renderer.size());
    let deselected_hud = options.ui_deselect_after.map(|_| {
        let cleared: Vec<_> = units
            .iter()
            .cloned()
            .map(|mut unit| {
                unit.selected = false;
                unit
            })
            .collect();
        crate::hud::read_hud(&state, &data, &cleared, renderer.size())
    });
    let hud_at = |frame: u32| match (options.ui_deselect_after, &deselected_hud) {
        (Some(after), Some(hud)) if frame >= after => hud,
        _ => &selected_hud,
    };
    let hud = hud_at(options.ui_frames);
    let dots = crate::hud::minimap_dots(&units, &state);
    let view_box = crate::game::minimap_view_box(&state, renderer.size());
    if let Some(overlay) = apt_overlay.as_mut() {
        for frame in 0..options.ui_frames {
            if let Some(position) = options
                .radial_press
                .filter(|_| frame == options.ui_frames / 2)
            {
                let _ = overlay.press_radial(position);
            }
            overlay.frame(hud_at(frame), UI_FRAME);
        }
        overlay.settle_text(&mut renderer, MAX_TEXT_SETTLE_PASSES);
    }
    let ring: Vec<&str> = hud
        .command_bar
        .ring
        .iter()
        .map(|slot| slot.as_ref().map_or("-", |s| s.label.as_str()))
        .collect();
    let bar: Vec<&str> = hud
        .command_bar
        .bar
        .iter()
        .map(|slot| slot.label.as_str())
        .collect();
    println!(
        "apt overlay: after {} UI frame(s) of {:.1} ms: ring {} of 6 [{}], {} bar slot(s) [{}], {} \
         available builder(s)",
        options.ui_frames,
        UI_FRAME.as_secs_f64() * 1000.0,
        hud.command_bar.ring_count(),
        ring.join(", "),
        bar.len(),
        bar.join(", "),
        hud.builders.len()
    );
    if let Some(overlay) = apt_overlay.as_ref() {
        let shown: Vec<String> = overlay
            .radial_shown()
            .iter()
            .map(|slot| {
                let dim = if slot.enabled { "" } else { " (dimmed)" };
                format!("{}{dim}", slot.label)
            })
            .collect();
        let (cost, frames) = overlay.radial_cost();
        let (palantir, _) = overlay.palantir_cost();
        println!(
            "apt overlay: radial menu [{}] ({} button(s)), {:.3} ms/frame over {frames} frame(s); \
             Palantir {:.3} ms/frame",
            shown.join(", "),
            shown.len(),
            cost.as_secs_f64() * 1000.0,
            palantir.as_secs_f64() * 1000.0
        );
    }

    let outcome = state.simulation.rules().ids().and_then(|ids| {
        let clock = crate::clock_entity(state.simulation.state(), ids)?;
        state
            .simulation
            .state()
            .get(crate::rules::RotwkRules::defeat_ids(ids).outcome, clock)
            .copied()
    });
    if outcome.is_some() {
        println!("match: a real MatchOutcome exists already - drawing the Results banner");
    }
    let repeats = options.frames.max(1);
    let started = std::time::Instant::now();
    let mut overlay_time = std::time::Duration::ZERO;
    let mut ui_frame_time = std::time::Duration::ZERO;
    let mut tick_time = std::time::Duration::ZERO;
    for repeat in 0..repeats {
        if options.tick_per_frame && repeat > 0 {
            let ticked = std::time::Instant::now();
            if let Some(ids) = state.simulation.rules().ids() {
                crate::render_state::step_remembering(
                    &mut state.simulation,
                    &mut state.previous,
                    ids,
                )
                .map_err(|e| e.to_string())?;
            }
            state.fx.collect(&state.simulation);
            tick_time += ticked.elapsed();
        }
        let overlay_started = std::time::Instant::now();
        let overlay_draws: Vec<ScreenDraw<'_>> = if outcome.is_some() {
            match apt_overlay.as_mut() {
                Some(overlay) => overlay.draw_banner(
                    &mut renderer,
                    &crate::game::results_banner_lines(&state.simulation),
                ),
                None => Vec::new(),
            }
        } else {
            match apt_overlay.as_mut() {
                Some(overlay) => {
                    let started = std::time::Instant::now();
                    overlay.frame(hud, UI_FRAME);
                    ui_frame_time += started.elapsed();
                    overlay.build_and_draw(&mut renderer, &dots, view_box, false)
                }
                None => Vec::new(),
            }
        };
        overlay_time += overlay_started.elapsed();
        let mut move_hint_cache = mint_sage_model::ModelCache::new();
        let move_hint_model = (!data.move_hint_name.is_empty())
            .then(|| {
                move_hint_cache.get_or_load(
                    &mut renderer,
                    &state.vfs,
                    &state.scene.resolver,
                    &data.move_hint_name,
                )
            })
            .flatten();
        crate::game::compose_and_render(
            &mut renderer,
            &mut state,
            options.blend.unwrap_or(0.0),
            interpolate,
            options.obstacles,
            None,
            &overlay_draws,
            &mut report,
            move_hint_model.as_ref(),
            crate::game::LOCAL_PLAYER,
            None,
            false,
        )
        .map_err(|e| e.to_string())?;
        state.units.take_sound_events().for_each(drop);
        if crate::perf::enabled() {
            let waited = std::time::Instant::now();
            renderer.wait_idle().map_err(|e| e.to_string())?;
            crate::render_perf::add_to_last(
                crate::render_perf::stage!("gpu wait"),
                waited.elapsed().as_secs_f64() * 1000.0,
            );
        }
    }
    crate::render_perf::report(usize::from(repeats > 4) * 2);
    report.flush();
    if options.frames > 1 {
        println!(
            "apt overlay: {:.3} ms/frame syncing, ticking, building and drawing it ({:.3} ms \
             of it the sync and tick), CPU only (overlay {}), icon cache {} texture(s)",
            overlay_time.as_secs_f64() * 1000.0 / f64::from(repeats),
            ui_frame_time.as_secs_f64() * 1000.0 / f64::from(repeats),
            if options.apt_overlay { "on" } else { "off" },
            apt_overlay
                .as_ref()
                .map_or(0, crate::apt_overlay::AptOverlay::icon_cache_len)
        );
    }
    if options.ticks == 0 || options.ticks % mint_app::SIMULATION_HZ != 0 {
        println!("{}", state.units.house_color_line());
        for line in state.units.state_lines() {
            println!("{line}");
        }
    }
    let (width, height) = renderer.size();
    let pixels = renderer.read_pixels().map_err(|e| e.to_string())?;
    if repeats > 1 {
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        println!(
            "frames: {repeats} draw(s) in {elapsed:.1} ms, {:.3} ms each ({:.1} fps), \
             reflection {}{}",
            elapsed / f64::from(repeats),
            f64::from(repeats) * 1000.0 / elapsed.max(1e-9),
            state.scene.reflection.label(),
            if options.tick_per_frame {
                format!(
                    ", a simulation tick before each redraw ({:.3} ms a tick of that)",
                    tick_time.as_secs_f64() * 1000.0 / f64::from(repeats.saturating_sub(1).max(1))
                )
            } else {
                String::new()
            }
        );
    }
    crate::game::write_tga(path, width, height, &pixels).map_err(|e| e.to_string())?;
    println!("wrote {width}x{height} screenshot to {}", path.display());
    crate::perf::report();
    Ok(())
}

pub(crate) fn parse_avoidance(name: Option<&str>) -> Result<mint_movement::AvoidanceMode, String> {
    match name {
        None => Ok(mint_movement::AvoidanceMode::Off),
        Some("off") => Ok(mint_movement::AvoidanceMode::Off),
        Some("separation") => Ok(mint_movement::AvoidanceMode::Separation),
        Some(other) => Err(format!("--avoidance wants off or separation, got {other}")),
    }
}

fn print_step_cost(ticks: u32, started: std::time::Instant) {
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    println!(
        "step: {ticks} tick(s) in {elapsed:.1} ms, {:.3} ms per step",
        elapsed / f64::from(ticks.max(1))
    );
}

fn submit_fight_orders(
    simulation: &mut mint_sim::Simulation<crate::rules::RotwkRules>,
) -> Result<(), String> {
    use crate::rules::RotwkCommand;
    let ids = simulation
        .rules()
        .ids()
        .ok_or("the ruleset registered no components")?;
    let mut by_player: Vec<(mint_sim::PlayerId, Vec<mint_sim::Entity>)> = Vec::new();
    if let Some(store) = simulation.state().store(ids.health) {
        for (entity, _) in store.iter() {
            let owner = simulation
                .state()
                .get(ids.owner, entity)
                .map_or(mint_sim::PlayerId::new(0), |owner| owner.0);
            match by_player.iter_mut().find(|(player, _)| *player == owner) {
                Some((_, units)) => units.push(entity),
                None => by_player.push((owner, vec![entity])),
            }
        }
    }
    by_player.sort_by_key(|(player, _)| player.get());
    if by_player.len() < 2 {
        return Err(format!(
            "--fight needs two armies with health; found {}",
            by_player.len()
        ));
    }

    for index in 0..by_player.len() {
        let Some((player, units)) = by_player.get(index) else {
            continue;
        };
        let enemy = by_player
            .iter()
            .find(|(other, _)| other != player)
            .and_then(|(_, list)| list.first().copied());
        let Some(target) = enemy else { continue };
        simulation
            .submit(
                *player,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: units.clone(),
                },
            )
            .map_err(|e| e.to_string())?;
        simulation
            .submit(*player, RotwkCommand::AttackObject { target })
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

struct FightUnit {
    owner: mint_sim::PlayerId,
    select: Vec<mint_sim::Entity>,
    target: mint_sim::Entity,
    position: mint_movement::Vec2,
}

fn fight_units(
    simulation: &mint_sim::Simulation<crate::rules::RotwkRules>,
    ids: crate::rules::Ids,
) -> Vec<FightUnit> {
    let mut out = Vec::new();
    if let Some(store) = simulation.state().store(ids.horde) {
        for (horde_entity, horde) in store.iter() {
            let Some(body) = simulation.state().get(ids.body, horde_entity) else {
                continue;
            };
            let mut target_and_owner = None;
            for member in &horde.members {
                if !simulation.state().is_live(*member) {
                    continue;
                }
                let Some(health) = simulation.state().get(ids.health, *member) else {
                    continue;
                };
                if health.0.is_dead() {
                    continue;
                }
                let owner = simulation
                    .state()
                    .get(ids.owner, *member)
                    .map_or(mint_sim::PlayerId::new(0), |owner| owner.0);
                target_and_owner = Some((*member, owner));
                break;
            }
            let Some((target, owner)) = target_and_owner else {
                continue;
            };
            out.push(FightUnit {
                owner,
                select: horde.members.clone(),
                target,
                position: body.0.position,
            });
        }
    }
    if let Some(store) = simulation.state().store(ids.health) {
        for (entity, health) in store.iter() {
            if simulation.state().contains(ids.member, entity) {
                continue;
            }
            if health.0.is_dead() || simulation.state().contains(ids.concealed, entity) {
                continue;
            }
            let Some(body) = simulation.state().get(ids.body, entity) else {
                continue;
            };
            let owner = simulation
                .state()
                .get(ids.owner, entity)
                .map_or(mint_sim::PlayerId::new(0), |owner| owner.0);
            out.push(FightUnit {
                owner,
                select: vec![entity],
                target: entity,
                position: body.0.position,
            });
        }
    }
    out
}

fn submit_fight_orders_nearest(
    simulation: &mut mint_sim::Simulation<crate::rules::RotwkRules>,
) -> Result<(), String> {
    use crate::rules::RotwkCommand;
    let ids = simulation
        .rules()
        .ids()
        .ok_or("the ruleset registered no components")?;
    let units = fight_units(simulation, ids);
    let owners = units
        .iter()
        .map(|unit| unit.owner)
        .collect::<std::collections::BTreeSet<_>>();
    if owners.len() < 2 {
        return Err(format!(
            "--fight-nearest needs two armies with health; found {}",
            owners.len()
        ));
    }
    for (attacker, target) in units.iter().zip(nearest_enemy_indices(&units)) {
        let Some(target) = target.and_then(|at| units.get(at)) else {
            continue;
        };
        simulation
            .submit(
                attacker.owner,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: attacker.select.clone(),
                },
            )
            .map_err(|e| e.to_string())?;
        simulation
            .submit(
                attacker.owner,
                RotwkCommand::AttackObject {
                    target: target.target,
                },
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn nearest_enemy_indices(units: &[FightUnit]) -> Vec<Option<usize>> {
    units
        .iter()
        .map(|attacker| {
            units
                .iter()
                .enumerate()
                .filter(|(_, other)| other.owner != attacker.owner)
                .min_by(|(_, a), (_, b)| {
                    let da = dist2(attacker.position, a.position);
                    let db = dist2(attacker.position, b.position);
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(index, _)| index)
        })
        .collect()
}

fn dist2(a: mint_movement::Vec2, b: mint_movement::Vec2) -> f32 {
    let (dx, dy) = (a.x - b.x, a.y - b.y);
    dx * dx + dy * dy
}

fn run_scripted_fight(
    simulation: &mut mint_sim::Simulation<crate::rules::RotwkRules>,
    ticks: u32,
    modes: crate::unit::Modes,
    trace: bool,
    nearest: bool,
    check_passability: bool,
    check_stuck: bool,
) -> Result<(), String> {
    use crate::rules::RotwkCommand;
    let ids = simulation
        .rules()
        .ids()
        .ok_or("the ruleset registered no components")?;
    simulation
        .submit(
            crate::game::LOCAL_PLAYER,
            RotwkCommand::SetMovementModes(modes),
        )
        .map_err(|e| e.to_string())?;
    if nearest {
        submit_fight_orders_nearest(simulation)?;
    } else {
        submit_fight_orders(simulation)?;
    }
    reset_fight_trace();

    let stuck_snapshot_at = ticks.saturating_sub(STUCK_WINDOW_TICKS);
    let mut stuck_before = None;
    let stepping = std::time::Instant::now();
    for step in 0..ticks {
        simulation.tick().map_err(|e| e.to_string())?;
        if trace {
            println!(
                "TRACE tick {} {}",
                step.saturating_add(1),
                fight_trace_line(simulation, ids)
            );
        }
        if check_stuck && step == stuck_snapshot_at {
            stuck_before = Some(snapshot_positions(simulation, ids));
        }
    }
    if !trace {
        print_step_cost(ticks, stepping);
    }
    let view = crate::render_state::RenderState::new(simulation.state(), 0.0);
    let living = view.living_by_player(ids);
    let survivors: Vec<String> = living
        .iter()
        .map(|(player, count)| format!("player {} x{count}", player.get()))
        .collect();
    let in_flight = simulation
        .state()
        .store(ids.flight)
        .map_or(0, mint_sim::ComponentStore::len);
    let corpses = simulation
        .state()
        .store(ids.doomed)
        .map_or(0, mint_sim::ComponentStore::len);
    println!(
        "fight: {ticks} tick(s) at {}, {} alive ({}), {corpses} corpse(s) still standing,          {in_flight} projectile(s) in flight, state hash {:#018x}",
        modes.config(),
        living.iter().map(|(_, count)| *count).sum::<usize>(),
        survivors.join(", "),
        simulation.state_hash()
    );
    if check_passability {
        let report = count_units_on_blocked_cells(simulation, ids);
        println!(
            "passability check: {} living unit(s), {} on a blocked cell ({} corpse, {} not), \
             {} off the grid - blocked by: {} cliff, {} water, {} deep water, {} obstacle, \
             {} boundary",
            report.total,
            report.blocked,
            report.blocked_corpses,
            report.blocked.saturating_sub(report.blocked_corpses),
            report.off_grid,
            report.cliff,
            report.water,
            report.deep_water,
            report.obstacle,
            report.boundary,
        );
    }
    if let Some(before) = stuck_before {
        let report = count_stuck_units(simulation, ids, &before);
        println!(
            "stuck check: {} unit(s) under order, out of weapon range, and unmoved by more \
             than {STUCK_EPSILON:.1} unit(s) over the last {} tick(s){}",
            report.count,
            ticks.min(STUCK_WINDOW_TICKS),
            format_stuck_sample(&report.sample)
        );
    }
    Ok(())
}

fn format_stuck_sample(sample: &[(u32, mint_movement::Vec2, String)]) -> String {
    if sample.is_empty() {
        return String::new();
    }
    let entries: Vec<String> = sample
        .iter()
        .map(|(index, position, label)| {
            format!(
                "#{index} at ({:.0}, {:.0}) [{label}]",
                position.x, position.y
            )
        })
        .collect();
    format!(" - e.g. {}", entries.join(", "))
}

#[derive(Default)]
struct PassabilityCheck {
    total: u32,
    blocked: u32,
    blocked_corpses: u32,
    off_grid: u32,
    cliff: u32,
    water: u32,
    deep_water: u32,
    obstacle: u32,
    boundary: u32,
}

fn count_units_on_blocked_cells(
    simulation: &mint_sim::Simulation<crate::rules::RotwkRules>,
    ids: crate::rules::Ids,
) -> PassabilityCheck {
    use mint_movement::Passability as _;
    let passability = simulation.rules().passability();
    let mut report = PassabilityCheck::default();
    let Some(store) = simulation.state().store(ids.health) else {
        return report;
    };
    for (entity, _) in store.iter() {
        let Some(body) = simulation.state().get(ids.body, entity) else {
            continue;
        };
        report.total = report.total.saturating_add(1);
        match passability.cell_at(body.0.position) {
            Some(cell) => {
                let flags = passability.grid.flags(cell);
                if !passability
                    .grid
                    .is_passable(cell, mint_sage_passability::GROUND)
                {
                    report.blocked = report.blocked.saturating_add(1);
                    if simulation.state().contains(ids.doomed, entity) {
                        report.blocked_corpses = report.blocked_corpses.saturating_add(1);
                    }
                    if flags.intersects(mint_sage_passability::CLIFF) {
                        report.cliff = report.cliff.saturating_add(1);
                    }
                    if flags.intersects(mint_sage_passability::WATER) {
                        report.water = report.water.saturating_add(1);
                    }
                    if flags.intersects(mint_sage_passability::DEEP_WATER) {
                        report.deep_water = report.deep_water.saturating_add(1);
                    }
                    if flags.intersects(mint_sage_passability::OBSTACLE) {
                        report.obstacle = report.obstacle.saturating_add(1);
                    }
                    if flags.intersects(mint_sage_passability::BOUNDARY) {
                        report.boundary = report.boundary.saturating_add(1);
                    }
                }
            }
            None => report.off_grid = report.off_grid.saturating_add(1),
        }
    }
    report
}

const STUCK_WINDOW_TICKS: u32 = 150;

const STUCK_EPSILON: f32 = 3.0;

fn snapshot_positions(
    simulation: &mint_sim::Simulation<crate::rules::RotwkRules>,
    ids: crate::rules::Ids,
) -> std::collections::BTreeMap<u32, mint_movement::Vec2> {
    let mut out = std::collections::BTreeMap::new();
    let Some(store) = simulation.state().store(ids.health) else {
        return out;
    };
    for (entity, _) in store.iter() {
        if let Some(body) = simulation.state().get(ids.body, entity) {
            out.insert(entity.index(), body.0.position);
        }
    }
    out
}

fn has_active_order(
    simulation: &mint_sim::Simulation<crate::rules::RotwkRules>,
    ids: crate::rules::Ids,
    entity: mint_sim::Entity,
) -> bool {
    let state = simulation.state();
    if state.contains(ids.path, entity) || state.contains(ids.attack_order, entity) {
        return true;
    }
    if let Some(member) = state.get(ids.member, entity) {
        let horde = member.horde;
        if state.contains(ids.path, horde)
            || state.contains(ids.attack_order, horde)
            || state.contains(ids.attack_move_order, horde)
        {
            return true;
        }
    }
    false
}

struct StuckReport {
    count: u32,
    sample: Vec<(u32, mint_movement::Vec2, String)>,
}

fn passability_label(
    simulation: &mint_sim::Simulation<crate::rules::RotwkRules>,
    position: mint_movement::Vec2,
) -> String {
    use mint_movement::Passability as _;
    let passability = simulation.rules().passability();
    let Some(cell) = passability.cell_at(position) else {
        return "off-grid".to_owned();
    };
    let flags = passability.grid.flags(cell);
    if passability
        .grid
        .is_passable(cell, mint_sage_passability::GROUND)
    {
        return "clear".to_owned();
    }
    let mut labels = Vec::new();
    if flags.intersects(mint_sage_passability::CLIFF) {
        labels.push("cliff");
    }
    if flags.intersects(mint_sage_passability::WATER) {
        labels.push("water");
    }
    if flags.intersects(mint_sage_passability::DEEP_WATER) {
        labels.push("deep-water");
    }
    if flags.intersects(mint_sage_passability::OBSTACLE) {
        labels.push("obstacle");
    }
    if flags.intersects(mint_sage_passability::BOUNDARY) {
        labels.push("boundary");
    }
    if flags.intersects(mint_sage_passability::PINCHED) {
        labels.push("pinched");
    }
    if labels.is_empty() {
        "blocked".to_owned()
    } else {
        labels.join("+")
    }
}

const STUCK_SAMPLE_SIZE: usize = 8;

fn count_stuck_units(
    simulation: &mint_sim::Simulation<crate::rules::RotwkRules>,
    ids: crate::rules::Ids,
    before: &std::collections::BTreeMap<u32, mint_movement::Vec2>,
) -> StuckReport {
    let Some(store) = simulation.state().store(ids.health) else {
        return StuckReport {
            count: 0,
            sample: Vec::new(),
        };
    };
    let mut stuck = 0u32;
    let mut sample = Vec::new();
    for (entity, _) in store.iter() {
        if simulation.state().contains(ids.doomed, entity) {
            continue;
        }
        if !has_active_order(simulation, ids, entity) {
            continue;
        }
        if simulation
            .state()
            .get(ids.shape, entity)
            .is_some_and(|shape| shape.kind_of.contains(mint_sage_asset::KindOf::IMMOBILE))
        {
            continue;
        }
        if simulation
            .state()
            .get(ids.armed, entity)
            .is_some_and(|armed| armed.target.is_some())
        {
            continue;
        }
        let Some(before) = before.get(&entity.index()) else {
            continue;
        };
        let Some(body) = simulation.state().get(ids.body, entity) else {
            continue;
        };
        if before.minus(body.0.position).length() <= STUCK_EPSILON {
            stuck = stuck.saturating_add(1);
            if sample.len() < STUCK_SAMPLE_SIZE {
                let mut label = passability_label(simulation, body.0.position);
                let state = simulation.state();
                let own_path = state.contains(ids.path, entity);
                let own_attack = state.contains(ids.attack_order, entity);
                let horde_path = state
                    .get(ids.member, entity)
                    .is_some_and(|m| state.contains(ids.path, m.horde));
                let horde_attack = state
                    .get(ids.member, entity)
                    .is_some_and(|m| state.contains(ids.attack_order, m.horde));
                label.push_str(&format!(
                    " path={own_path} attack={own_attack} horde_path={horde_path} horde_attack={horde_attack}"
                ));
                if let Some(path) = state.get(ids.path, entity) {
                    label.push_str(&format!(
                        " goal=({:.0},{:.0}) [{}] kind={:?} waypoint={}/{} speed={:.1}",
                        path.goal.x,
                        path.goal.y,
                        passability_label(simulation, path.goal),
                        path.kind,
                        body.0.waypoint,
                        path.route.points.len(),
                        body.0.speed,
                    ));
                }
                sample.push((entity.index(), body.0.position, label));
            }
        }
    }
    StuckReport {
        count: stuck,
        sample,
    }
}

thread_local! {
    static HORDES_MOVED: RefCell<BTreeSet<u32>> = const { RefCell::new(BTreeSet::new()) };
}

fn reset_fight_trace() {
    HORDES_MOVED.with(|seen| seen.borrow_mut().clear());
}

#[allow(clippy::cast_precision_loss)]
fn fight_trace_line(
    simulation: &mint_sim::Simulation<crate::rules::RotwkRules>,
    ids: crate::rules::Ids,
) -> String {
    type Row = (u8, u32, f32, f32, u32, u32, u32, u32, u32);
    fn row_for(per: &mut Vec<Row>, owner: u8) -> usize {
        match per.iter().position(|row| row.0 == owner) {
            Some(at) => at,
            None => {
                per.push((owner, 0, 0.0, 0.0, 0, 0, 0, 0, 0));
                per.len() - 1
            }
        }
    }
    let mut per: Vec<Row> = Vec::new();
    if let Some(store) = simulation.state().store(ids.health) {
        for (entity, _) in store.iter() {
            let owner = simulation
                .state()
                .get(ids.owner, entity)
                .map_or(0, |owner| owner.0.get());
            let x = simulation
                .state()
                .get(ids.body, entity)
                .map_or(0.0, |body| body.0.position.x);
            let path = u32::from(simulation.state().contains(ids.path, entity));
            let order = u32::from(simulation.state().contains(ids.attack_order, entity));
            let at = row_for(&mut per, owner);
            let Some(row) = per.get_mut(at) else {
                continue;
            };
            row.1 = row.1.saturating_add(1);
            row.2 += x;
            row.3 = row.3.max(x);
            row.4 = row.4.saturating_add(path);
            row.5 = row.5.saturating_add(order);
        }
    }
    if let Some(store) = simulation.state().store(ids.horde) {
        for (horde_entity, horde) in store.iter() {
            let Some(owner) = horde.members.iter().find_map(|member| {
                simulation
                    .state()
                    .get(ids.owner, *member)
                    .map(|owner| owner.0.get())
            }) else {
                continue;
            };
            let moving = simulation.state().contains(ids.path, horde_entity);
            if moving {
                HORDES_MOVED.with(|seen| seen.borrow_mut().insert(horde_entity.index()));
            }
            let moved = HORDES_MOVED.with(|seen| seen.borrow().contains(&horde_entity.index()));
            let at = row_for(&mut per, owner);
            let Some(row) = per.get_mut(at) else {
                continue;
            };
            row.6 = row.6.saturating_add(1);
            row.7 = row.7.saturating_add(u32::from(moving));
            row.8 = row.8.saturating_add(u32::from(moved));
        }
    }
    per.sort_by_key(|row| row.0);
    per.iter()
        .map(
            |(player, alive, sum_x, max_x, paths, orders, hordes, moving, moved)| {
                let mean = if *alive == 0 {
                    0.0
                } else {
                    sum_x / f32::from(u16::try_from(*alive).unwrap_or(u16::MAX))
                };
                format!(
                    "p{player}: n={alive} meanx={mean:.1} maxx={max_x:.1} paths={paths} \
                 orders={orders} hordes={hordes} moving={moving} moved={moved}"
                )
            },
        )
        .collect::<Vec<_>>()
        .join(" | ")
}

const FIGHT_VIEW_DISTANCE: f32 = 330.0;

fn scene_shroud_mode(state: &mut crate::game::MatchState, name: &str) -> Result<(), String> {
    let mode = match name {
        "off" => mint_sage_terrain::TerrainShroud::Revealed,
        "on" => mint_sage_terrain::TerrainShroud::On,
        other => return Err(format!("--shroud wants off or on, got {other}")),
    };
    state.scene.terrain_shading.modulation.shroud = mode;
    println!("render: shroud {}", mode.label());
    Ok(())
}

fn scene_reflection_mode(state: &mut crate::game::MatchState, name: &str) -> Result<(), String> {
    let rung = crate::reflection::ReflectionRung::parse(name)
        .ok_or_else(|| format!("--reflection wants mirror-retail|mirror-low|mirror, got {name}"))?;
    state.scene.reflection = rung;
    println!(
        "water: reflection {} - {} plane(s)",
        rung.label(),
        state.scene.water_scene.planes.len()
    );
    Ok(())
}

fn scene_wave_rung(state: &mut crate::game::MatchState, name: &str) -> Result<(), String> {
    let rung = mint_sage_scene::WaveRung::parse(name).ok_or_else(|| {
        format!("--waves wants off|breakers|swell|swell+breakers|tall-swell+breakers, got {name}")
    })?;
    state.scene.wave_rung = rung;
    println!("water: waves {}", rung.label());
    Ok(())
}

const SCREENSHOT_VIEW_DISTANCE: f32 = 620.0;

fn steps_dropped(campaign_blocks: bool, match_paused: Option<bool>) -> bool {
    campaign_blocks || match_paused == Some(true)
}

#[cfg(test)]
mod steps_dropped_tests {
    use super::steps_dropped;

    #[test]
    fn the_start_tail_holds_the_clock_behind_the_load_screen_and_over_the_match() {
        assert!(steps_dropped(true, None));
        assert!(steps_dropped(true, Some(false)));
        assert!(steps_dropped(false, Some(true)));
        assert!(!steps_dropped(false, Some(false)));
        assert!(!steps_dropped(false, None));
    }
}

#[cfg(test)]
mod money_flag_tests {
    use super::parse_money;

    #[test]
    fn money_takes_a_whole_amount_and_refuses_a_negative_or_unparsable_one() {
        assert_eq!(parse_money("10000"), Ok(10_000));
        assert_eq!(parse_money("0"), Ok(0));
        assert!(parse_money("-1").is_err());
        assert!(parse_money("lots").is_err());
    }
}

#[cfg(test)]
mod shell_wanted_tests {
    use super::shell_wanted;

    #[test]
    fn no_flags_at_all_wants_the_shell() {
        assert!(shell_wanted(&None, &None, &None, None));
    }

    #[test]
    fn an_explicit_map_skips_the_shell() {
        assert!(!shell_wanted(
            &Some("Anduin".to_owned()),
            &None,
            &None,
            None
        ));
    }

    #[test]
    fn an_explicit_faction_skips_the_shell() {
        assert!(!shell_wanted(
            &None,
            &Some("FactionMen".to_owned()),
            &None,
            None
        ));
    }

    #[test]
    fn an_explicit_opponent_skips_the_shell() {
        assert!(!shell_wanted(
            &None,
            &None,
            &Some("FactionMordor".to_owned()),
            None
        ));
    }

    #[test]
    fn an_explicit_army_skips_the_shell() {
        assert!(!shell_wanted(&None, &None, &None, Some(5_000)));
    }
}

#[cfg(test)]
mod fight_nearest_tests {
    use super::{nearest_enemy_indices, FightUnit};
    use mint_movement::Vec2;
    use mint_sim::{PlayerId, World};

    const PLAYER: PlayerId = PlayerId::new(0);
    const OTHER: PlayerId = PlayerId::new(1);

    fn unit(world: &mut World, owner: PlayerId, x: f32, y: f32) -> FightUnit {
        let entity = world.spawn().unwrap();
        FightUnit {
            owner,
            select: vec![entity],
            target: entity,
            position: Vec2::new(x, y),
        }
    }

    #[test]
    fn each_attacker_picks_the_nearest_enemy_not_a_shared_one() {
        let mut world = World::new();
        let near_attacker = unit(&mut world, PLAYER, 0.0, 0.0);
        let far_attacker = unit(&mut world, PLAYER, 1000.0, 0.0);
        let near_defender = unit(&mut world, OTHER, 10.0, 0.0);
        let far_defender = unit(&mut world, OTHER, 1010.0, 0.0);
        let units = vec![near_attacker, far_attacker, near_defender, far_defender];

        let nearest = nearest_enemy_indices(&units);

        assert_eq!(
            nearest[0],
            Some(2),
            "the near attacker should target the near defender, index 2"
        );
        assert_eq!(
            nearest[1],
            Some(3),
            "the far attacker should target the far defender, index 3, not the near one"
        );
    }

    #[test]
    fn a_unit_with_no_enemy_owner_present_gets_none() {
        let mut world = World::new();
        let lonely = unit(&mut world, PLAYER, 0.0, 0.0);
        let ally = unit(&mut world, PLAYER, 5.0, 0.0);
        let units = vec![lonely, ally];

        let nearest = nearest_enemy_indices(&units);

        assert_eq!(nearest, vec![None, None]);
    }
}

#[cfg(test)]
mod fight_trace_tests {
    use super::{fight_trace_line, reset_fight_trace};
    use crate::horde::HordeSpawn;
    use crate::rules::{RotwkCommand, RotwkRules};
    use crate::unit::{Shape, Spawn};
    use mint_movement::{Facing, Vec2};
    use mint_sage_asset::KindOf;
    use mint_sage_formation::FormationTable;
    use mint_sage_map::HeightMapData;
    use mint_sage_passability::Passability;
    use mint_sim::{PlayerId, Simulation};

    const PLAYER: PlayerId = PlayerId::new(0);

    fn flat_map(width: u32, height: u32) -> HeightMapData {
        HeightMapData {
            width,
            height,
            border_size: 0,
            boundaries: Vec::new(),
            samples: vec![0; (width * height) as usize],
        }
    }

    fn one_horde_sim(origin: Vec2) -> Simulation<RotwkRules> {
        let map = flat_map(96, 96);
        let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
        let formation =
            FormationTable::from_offsets(&[Vec2::new(-10.0, 0.0), Vec2::new(10.0, 0.0)]);
        let member = Spawn {
            health: Some(100.0),
            ..Spawn::inert(
                Vec2::ZERO,
                Facing::EAST,
                Shape {
                    kind_of: KindOf::SELECTABLE,
                    radius: 5.0,
                    height: 18.0,
                    geometry_is_small: false,
                },
                PLAYER,
            )
        };
        let spawn = HordeSpawn {
            formation: 0,
            origin,
            facing: Facing::EAST,
            member,
            count: 2,
            vision: 100.0,
        };
        let rules =
            RotwkRules::new(passability, Vec::new()).with_hordes(vec![formation], vec![spawn]);
        Simulation::new(1, rules).unwrap()
    }

    #[test]
    fn moved_stays_set_after_a_horde_arrives_and_stops_moving() {
        reset_fight_trace();
        let mut simulation = one_horde_sim(Vec2::new(100.0, 100.0));
        let ids = simulation.rules().ids().unwrap();

        let line = fight_trace_line(&simulation, ids);
        assert!(
            line.contains("hordes=1") && line.contains("moving=0") && line.contains("moved=0"),
            "a horde with no order yet should show moving=0 moved=0: {line}"
        );

        let horde = simulation
            .state()
            .store(ids.horde)
            .and_then(|store| store.entities().first().copied())
            .unwrap();
        let member = simulation
            .state()
            .get(ids.horde, horde)
            .and_then(|horde| horde.members.first().copied())
            .unwrap();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![member],
                },
            )
            .unwrap();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::MoveTo {
                    x: 110.0,
                    y: 100.0,
                    queue: false,
                },
            )
            .unwrap();
        simulation.tick().unwrap();
        let line = fight_trace_line(&simulation, ids);
        assert!(
            line.contains("moving=1") && line.contains("moved=1"),
            "a horde walking its own route should show both set: {line}"
        );

        for _ in 0..200 {
            simulation.tick().unwrap();
            if !simulation.state().contains(ids.path, horde) {
                break;
            }
        }
        assert!(
            !simulation.state().contains(ids.path, horde),
            "the fixture's own setup - a 10-unit march should finish well within 200 ticks"
        );
        let line = fight_trace_line(&simulation, ids);
        assert!(
            line.contains("moving=0"),
            "an arrived horde holds no route any more: {line}"
        );
        assert!(
            line.contains("moved=1"),
            "but it must still count as having moved - that is the whole point of a \
             cumulative column: {line}"
        );
    }
}
