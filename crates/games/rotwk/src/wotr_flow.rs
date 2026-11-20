use std::path::Path;

use mint_app::{App, InputEvent};
use mint_render::Renderer;
use mint_sage_livingworld::{BattleResult, BattleSpec, Rules};

use crate::wotr_dev::{Session, WotrApp};
use crate::{AppScreen, AppState};

const TURN_CAP: u32 = 400;

pub(crate) struct BattleHop {
    run: Box<WotrApp>,
    spec: BattleSpec,
    left_at: u64,
}

const TUTORIAL_SEED: u64 = 1;

impl AppState {
    pub(crate) fn wotr_step(&mut self, renderer: &mut Renderer) {
        if let Some(run) = &mut self.wotr {
            if run.session().state.ended.is_some() || run.wants_exit() {
                self.end_wotr();
            } else if let Some(spec) = run.battle_ready() {
                self.hop_to_battle(spec);
            }
            return;
        }
        if let Some(path) = self.wotr_load.take() {
            self.load_wotr_session(renderer, &path);
            return;
        }
        let AppScreen::Shell(shell) = &mut self.screen else {
            return;
        };
        let Some(start) = shell.take_wotr_request() else {
            return;
        };
        self.release_retired();
        let opened = Session::from_setup(&start.data, &start.setup).and_then(|session| {
            WotrApp::open(
                renderer,
                &self.vfs,
                &self.install_roots,
                session.with_colours(start.colours),
                false,
            )
        });
        match opened {
            Ok(app) => self.wotr = Some(Box::new(app)),
            Err(message) => println!("wotr: {message}"),
        }
    }

    fn load_wotr_session(&mut self, renderer: &mut Renderer, path: &Path) {
        let file = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        self.release_retired();
        let opened = self.read_wotr_session(path).and_then(|session| {
            WotrApp::open(renderer, &self.vfs, &self.install_roots, session, false)
        });
        match opened {
            Ok(app) => {
                let state = &app.session().state;
                println!(
                    "wotr: loaded {file}: turn {} {:?} hash 0x{:016x}",
                    state.turn,
                    state.phase,
                    state.state_hash()
                );
                self.wotr = Some(Box::new(app));
            }
            Err(error) => {
                println!("wotr: could not load {file}: {error}");
                if let AppScreen::Shell(shell) = &mut self.screen {
                    shell.show_load_error(&file);
                }
            }
        }
    }

    fn read_wotr_session(&self, path: &Path) -> Result<Session, String> {
        let loaded = crate::wotr_dev::load_data(&self.vfs);
        let key = crate::wotr_save::read_key(path).map_err(|e| format!("{e:?}"))?;
        let rules = Rules::build(
            &loaded.living,
            &loaded.catalog,
            &key.campaign,
            key.scenario,
            key.victory_type,
        )
        .map_err(|e| e.to_string())?;
        let (state, ai) =
            crate::wotr_save::read_session(path, rules).map_err(|e| format!("{e:?}"))?;
        Ok(Session::resume(&loaded, state, ai))
    }

    fn hop_to_battle(&mut self, spec: BattleSpec) {
        let Some(run) = self.wotr.take() else {
            return;
        };
        let left_at = run.session().state.state_hash();
        let map = spec.map.clone().unwrap_or_default();
        println!(
            "wotr: battle {} at {} goes to map {map:?}, strategic hash 0x{left_at:016x}, \
             seed 0x{:016x}",
            spec.battle, spec.region_name, spec.seed
        );
        let setup = crate::wotr_battle::battle_setup(&spec, &map);
        let readable = self.vfs.contains(&setup.map_label);
        self.wotr_battle = Some(BattleHop { run, spec, left_at });
        match &mut self.screen {
            AppScreen::Shell(shell) if readable => shell.start_wotr_battle(setup),
            _ => {
                println!("wotr: {} is not in the install", setup.map_label);
                self.wotr_battle_failed();
            }
        }
    }

    pub(crate) fn wotr_battle_failed(&mut self) {
        if let Some(hop) = self.wotr_battle.take() {
            self.back_to_session(hop, BattleResult::default());
        }
    }

    pub(crate) fn wotr_battle_ended(&mut self, renderer: &mut Renderer) -> bool {
        let AppScreen::Match(game) = &self.screen else {
            return false;
        };
        let (Some(hop), true) = (&self.wotr_battle, game.wants_shell()) else {
            return false;
        };
        let viewport = game.viewport();
        let result = game.wotr_battle_result(&hop.spec, &hop.run.session().state.rules);
        println!(
            "wotr: match ended at tick {}, hash {:#018x}, winning side {:?}, {} army roster(s)",
            game.sim_frame(),
            game.state_hash(),
            result.winner,
            result.rosters.len()
        );
        if let Err(error) = self.enter_shell(renderer, viewport) {
            eprintln!("wotr: could not leave the match ({error})");
            return false;
        }
        if let Some(mut hop) = self.wotr_battle.take() {
            let (width, height) = renderer.size();
            hop.run.input(&InputEvent::Resized { width, height });
            self.back_to_session(hop, result);
        }
        true
    }

    fn back_to_session(&mut self, hop: BattleHop, result: BattleResult) {
        let BattleHop {
            mut run,
            spec,
            left_at,
        } = hop;
        let held = run.session().state.state_hash();
        if let Err(reason) = run.finish_real_time_battle(&spec, result) {
            println!("wotr: the session refused the battle's result: {reason:?}");
        }
        println!(
            "wotr: back on the world map: strategic hash 0x{left_at:016x} -> 0x{held:016x} across \
             the match, 0x{:016x} after the result, turn {} {:?}",
            run.session().state.state_hash(),
            run.session().state.turn,
            run.session().state.phase
        );
        self.wotr = Some(run);
        if self.retired.is_some() {
            let began = std::time::Instant::now();
            self.finish_teardown();
            println!(
                "teardown: the battle's art and data freed in {:.0} ms",
                began.elapsed().as_secs_f64() * 1000.0
            );
        }
    }

    pub(crate) fn begin_wotr_tutorial(&mut self, renderer: &mut Renderer) {
        if self.wotr.is_some() {
            return;
        }
        self.release_retired();
        let loaded = crate::wotr_dev::load_data_with(&self.vfs, true);
        let opened = Session::tutorial(&loaded, TUTORIAL_SEED).and_then(|session| {
            WotrApp::open(renderer, &self.vfs, &self.install_roots, session, false)
        });
        match opened {
            Ok(app) => {
                println!("wotr-tutorial: OnTutorial(Strategic) opened the tutorial in the HUD");
                self.wotr = Some(Box::new(app));
            }
            Err(message) => println!("wotr-tutorial: {message}"),
        }
    }

    fn end_wotr(&mut self) {
        let Some(run) = self.wotr.take() else {
            return;
        };
        if let AppScreen::Shell(shell) = &mut self.screen {
            if run.session().tutorial_running() {
                println!("wotr-tutorial: the session handed back to the shell, main menu");
                shell.campaign_to_main_menu();
                return;
            }
            let swatches = crate::wotr_dev::seat_swatches(&self.vfs, run.session());
            let report = crate::wotr_end::end_report(
                &run.session().state,
                &run.session().data,
                &|faction| shell.is_evil_template(faction),
                &|faction| shell.template_side(faction),
                &swatches,
            );
            match report {
                Some(report) => shell.show_wotr_end(report),
                None => {
                    println!("wotr: left the session");
                    shell.campaign_to_main_menu();
                }
            }
        }
    }

    fn queue_wotr_load(&mut self, stem: &str) -> Result<String, String> {
        let path = crate::wotr_save::path_of(stem.trim()).map_err(|error| error.to_string())?;
        let shown = path.display().to_string();
        self.wotr_load = Some(path);
        Ok(shown)
    }

    pub(crate) fn wotr_hook(
        &mut self,
        verb: &str,
        argument: &str,
    ) -> Option<Result<String, String>> {
        if let ("do", Some(stem)) = (verb, argument.strip_prefix("wotr-load ")) {
            return Some(self.queue_wotr_load(stem));
        }
        let run = self.wotr.as_mut()?;
        Some(
            match (verb, argument.split_once(' ').unwrap_or((argument, ""))) {
                ("screen", _) => argument
                    .eq_ignore_ascii_case("WotrSession")
                    .then(String::new)
                    .ok_or_else(|| "WotrSession".to_owned()),
                ("do", ("wotr-turn", n)) => {
                    let turns = n.trim().parse::<u32>().unwrap_or(1);
                    for _ in 0..turns {
                        run.play_turn();
                    }
                    Ok(format!("turn {}", run.session().state.turn))
                }
                ("do", ("wotr-end", _)) => {
                    let mut turns = 0;
                    while run.session().state.ended.is_none() && turns < TURN_CAP {
                        let before = run.session().state.state_hash();
                        run.play_turn();
                        if run.session().state.state_hash() == before {
                            break;
                        }
                        turns += 1;
                    }
                    Ok(format!(
                        "{turns} turn(s), turn {}, ended {:?}, hash 0x{:016x}",
                        run.session().state.turn,
                        run.session().state.ended,
                        run.session().state.state_hash()
                    ))
                }
                ("do", ("wotr-save", stem)) => run
                    .session()
                    .save(stem.trim(), stem.trim())
                    .map(|path| {
                        format!(
                            "saved {} at hash 0x{:016x}",
                            path.display(),
                            run.session().state.state_hash()
                        )
                    })
                    .map_err(|error| error.to_string()),
                ("do", ("wotr-leave", _)) => {
                    run.leave();
                    Ok("left".to_owned())
                }
                ("do", _) => {
                    let (session, hud, map, viewport) = run.parts();
                    crate::wotr_drive::step(session, hud, map, viewport, verb, argument)
                }
                _ => Err(format!(
                    "{verb} {argument}: not offered in a War of the Ring session"
                )),
            },
        )
    }
}
