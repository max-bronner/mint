use std::sync::Arc;
use std::time::Duration;

use mint_render::Renderer;
use mint_sage_apt_player::{ImageKey, InstanceId, Value};
use mint_sage_apt_viewer::gadget::gadget_key;
use mint_sage_apt_viewer::host::{Change, GadgetEvent, SurfaceContent};
use mint_sage_apt_viewer::scene_surface::{target_size, SceneSurfaces};
use mint_sage_ini::living_world::LivingWorldData;
use mint_sage_livingworld::screen_queries::BattleType;
use mint_sage_livingworld::{Catalog, LivingWorldState, RegionId, Setup};
use mint_sage_worldmap::WorldMap;

use crate::setup_grid::Choice;
use crate::setup_rules::{combo_id, INIT};
use crate::wotr_dev::{offered_colours, Loaded, OfferedColour};

use super::message_box::{BoxKind, BoxRequest};
use super::{
    default_lobby, localised, Screen, ShellApp, CURRENT_MAP_NAME, GAME_MAP_TYPE, STATS_LIST,
};
use crate::hud::{CellSpec, ComboItemSpec, ComboSpec, ImageSource, ListSpec};
use crate::lobby::{Catalogue, SkirmishSetup};
use crate::setup_grid::GridLabels;
use crate::wotr_end::EndReport;
use crate::wotr_setup::{
    self, Click, ClickRefusal, Refusal, RegionSpot, Scenario, ScenarioChoice, StartPicks,
};

const SCENARIO_TYPE: (&str, &str) = ("AptMapPreview::MapGadgetInit", "StrategicScenarioType");
const SCENARIO_DESCRIPTION: (&str, &str) = (
    "AptMapPreview::MapGadgetInit",
    "StrategicScenarioDescription",
);
const TERRITORY_DESCRIPTION: (&str, &str) = (
    "AptMapPreview::MapGadgetInit",
    "StrategicTerritoryDescription",
);
const LIVING_WORLD_MAP: (&str, &str) = ("AptMapPreview::MapGadgetInit", "LivingWorld");
const STRATEGIC: &str = "Strategic";
const RANDOM_LIVING_WORLD_COLOR: [f32; 3] = [1.0; 3];

const VICTORY_ROW: usize = 0;
const BATTLE_ROW: usize = 1;
const VICTORY_LABEL: &str = "RULE:VictoryType";
const BATTLE_LABEL: &str = "RULE:BattleType";

pub(crate) struct Start {
    pub(crate) data: Arc<Loaded>,
    pub(crate) setup: Setup,
    pub(crate) colours: Vec<usize>,
}

struct SetupScreen {
    choices: Vec<ScenarioChoice>,
    selected: usize,
    victory_type: usize,
    battle_type: BattleType,
    picks: StartPicks,
    spots: Vec<RegionSpot>,
    colours: Vec<OfferedColour>,
    hovered: Option<String>,
    preview: Option<Preview>,
    shown: Option<String>,
    saved: Box<(SkirmishSetup, Catalogue, GridLabels)>,
}

struct Preview {
    campaign: String,
    map: Box<WorldMap>,
    drawn: Option<(InstanceId, [f32; 4], (u32, u32))>,
}

#[derive(Default)]
pub(super) struct WotrShell {
    data: Option<Arc<Loaded>>,
    setup: Option<SetupScreen>,
    request: Option<Start>,
    end: Option<EndReport>,
    surfaces: SceneSurfaces,
}

impl WotrShell {
    pub(super) fn setup_shown(&self) -> bool {
        self.setup.is_some()
    }

    pub(super) fn ending(&self) -> bool {
        self.end.is_some()
    }

    pub(super) fn end_done(&mut self) {
        self.end = None;
    }
}

impl ShellApp {
    pub(crate) fn take_wotr_request(&mut self) -> Option<Start> {
        self.wotr.request.take()
    }

    pub(super) fn setup_clip(&self) -> &'static str {
        if self.wotr.setup_shown() {
            STRATEGIC
        } else {
            GAME_MAP_TYPE.1
        }
    }

    pub(super) fn setup_map_type(&self) -> crate::setup_grid::MapType {
        if self.wotr.setup_shown() {
            crate::setup_grid::MapType::Strategic
        } else {
            crate::setup_grid::MapType::OpenPlay
        }
    }

    pub(super) fn enter_strategic(&mut self) -> bool {
        if !self.prepare_strategic() {
            return false;
        }
        self.enter(Screen::SkirmishSetup);
        self.setup
            .expose
            .set_extern(GAME_MAP_TYPE.0, Value::str(STRATEGIC));
        self.declare_strategic_rows();
        true
    }

    fn declare_strategic_rows(&mut self) {
        for (name, rows) in [
            ("MpGameRules::NumComboBoxes", 2.0),
            ("MpGameRules::NumCheckBoxes", 0.0),
        ] {
            self.setup.expose.set_extern(name, Value::Number(rows));
        }
    }

    fn prepare_strategic(&mut self) -> bool {
        let data = match &self.wotr.data {
            Some(data) => Arc::clone(data),
            None => {
                let loaded = crate::wotr_dev::load_data(&self.vfs);
                let playable_factions = self
                    .catalogue
                    .random_pool
                    .iter()
                    .map(|name| name.to_ascii_lowercase())
                    .collect();
                let catalog = Catalog {
                    playable_factions,
                    ..loaded.catalog
                };
                let data = Arc::new(Loaded { catalog, ..loaded });
                self.wotr.data = Some(Arc::clone(&data));
                data
            }
        };
        let choices = wotr_setup::scenario_choices(&data.living);
        let factions = crate::wotr_dev::playable_templates(&data.living);
        if choices.is_empty() || factions.is_empty() {
            println!("shell: Strategic: no scenario or no playable faction in the data");
            return false;
        }
        let Some(first) = choices.first() else {
            return false;
        };
        let colours = offered_colours(&self.vfs);
        let mut catalogue = self.catalogue.clone();
        catalogue.random_pool.clone_from(&factions);
        catalogue.factions.clone_from(&factions);
        catalogue.starts = max_players(&data.living, first);
        catalogue.colours = u8::try_from(colours.len()).unwrap_or(u8::MAX);
        let mut labels = self.grid_labels.clone();
        labels.factions = factions
            .iter()
            .map(|name| localised(&self.strings, &format!("INI:{name}")))
            .collect();
        labels.colours = colours.iter().map(|c| c.rgb).collect();
        let mut lobby = default_lobby(Some(first.campaign.clone()));
        lobby.fit_to_map(catalogue.starts);
        lobby.rules = self.lobby.rules.clone();
        let saved = Box::new((
            std::mem::replace(&mut self.lobby, lobby),
            std::mem::replace(&mut self.catalogue, catalogue),
            std::mem::replace(&mut self.grid_labels, labels),
        ));
        println!(
            "shell: Strategic setup - {} scenario(s), {} playable faction(s): {factions:?}, \
             {} colour(s): {:?}",
            choices.len(),
            factions.len(),
            colours.len(),
            colours.iter().map(|c| c.name.as_str()).collect::<Vec<_>>()
        );
        self.wotr.setup = Some(SetupScreen {
            choices,
            selected: 0,
            victory_type: 0,
            battle_type: BattleType::default(),
            picks: StartPicks::default(),
            spots: Vec::new(),
            colours,
            hovered: None,
            preview: None,
            shown: None,
            saved,
        });
        self.scenario_chosen();
        self.grid = None;
        true
    }

    pub(super) fn leave_strategic(&mut self) {
        if let Some(screen) = self.wotr.setup.take() {
            let (lobby, catalogue, labels) = *screen.saved;
            self.lobby = lobby;
            self.catalogue = catalogue;
            self.grid_labels = labels;
            self.grid = None;
        }
    }

    fn with_scenario<R>(
        &mut self,
        f: impl FnOnce(&Scenario, &mut StartPicks, &mut SkirmishSetup) -> R,
    ) -> Option<R> {
        let data = self.wotr.data.clone()?;
        let screen = self.wotr.setup.as_mut()?;
        let choice = screen.choices.get(screen.selected)?;
        let cx = Scenario {
            data: &data.living,
            catalog: &data.catalog,
            choice,
            victory_type: screen.victory_type,
        };
        Some(f(&cx, &mut screen.picks, &mut self.lobby))
    }

    fn scenario_chosen(&mut self) {
        if let Some(screen) = &mut self.wotr.setup {
            screen.victory_type = 0;
        }
        self.with_scenario(|cx, picks, lobby| picks.choose_scenario(cx, lobby));
        let spots = self
            .wotr
            .data
            .as_ref()
            .zip(
                self.wotr
                    .setup
                    .as_ref()
                    .and_then(|s| s.choices.get(s.selected)),
            )
            .map(|(data, choice)| wotr_setup::startable_regions(&data.living, choice))
            .unwrap_or_default();
        if let Some(screen) = &mut self.wotr.setup {
            screen.spots = spots;
            screen.hovered = None;
            screen.shown = None;
        }
    }

    pub(super) fn publish_strategic(&mut self) {
        let Some(screen) = &self.wotr.setup else {
            return;
        };
        let strings = &self.strings;
        let text = |key: &str| localised(strings, key);
        let name = |c: &ScenarioChoice| {
            c.display_name
                .as_deref()
                .map_or_else(|| c.campaign.clone(), text)
        };
        let combo = ComboSpec {
            items: screen
                .choices
                .iter()
                .map(|c| ComboItemSpec::text(name(c)))
                .collect(),
            selected: Some(screen.selected),
            enabled: true,
        };
        let chosen = screen.choices.get(screen.selected);
        let described = |text: Option<String>| ListSpec {
            columns: Vec::new(),
            rows: text
                .map(|text| vec![vec![CellSpec::Text(text)]])
                .unwrap_or_default(),
            selected: None,
            scroll_bar: None,
            text_inset_percent: 0,
            scroll_to_top: true,
        };
        let data = self.wotr.data.as_ref();
        let description = described(data.zip(chosen).map(|(data, choice)| {
            wotr_setup::scenario_description(&data.living, choice, screen.victory_type, &text)
        }));
        let territory = described(data.zip(chosen).zip(screen.shown.as_deref()).and_then(
            |((data, choice), region)| {
                wotr_setup::territory_description(&data.living, choice, region, &text)
            },
        ));
        let title = screen.choices.get(screen.selected).map(name);
        for (key, source) in [
            (SCENARIO_TYPE, ImageSource::Combo(Arc::new(combo))),
            (
                SCENARIO_DESCRIPTION,
                ImageSource::List(Arc::new(description)),
            ),
            (
                TERRITORY_DESCRIPTION,
                ImageSource::List(Arc::new(territory)),
            ),
        ] {
            self.setup
                .expose
                .image(ImageKey::Name(gadget_key(key.0, key.1)), Some(source));
        }
        self.setup.expose.string(CURRENT_MAP_NAME, title);
        self.declare_strategic_rules();
    }

    fn declare_strategic_rules(&mut self) {
        let Some(screen) = &self.wotr.setup else {
            return;
        };
        let (Some(data), Some(choice)) = (&self.wotr.data, screen.choices.get(screen.selected))
        else {
            return;
        };
        let strings = &self.strings;
        let victory = ComboSpec {
            items: wotr_setup::victory_options(&data.living, choice)
                .into_iter()
                .map(|o| {
                    ComboItemSpec::text(o.label.map_or(o.name, |key| localised(strings, &key)))
                })
                .collect(),
            selected: Some(screen.victory_type),
            enabled: true,
        };
        let battle = ComboSpec {
            items: wotr_setup::BATTLE_TYPES
                .iter()
                .map(|(_, key)| ComboItemSpec::text(localised(strings, key)))
                .collect(),
            selected: wotr_setup::BATTLE_TYPES
                .iter()
                .position(|(kind, _)| *kind == screen.battle_type),
            enabled: true,
        };
        for (row, label, spec) in [
            (VICTORY_ROW, VICTORY_LABEL, victory),
            (BATTLE_ROW, BATTLE_LABEL, battle),
        ] {
            let id = combo_id(row);
            self.setup
                .expose
                .string(&id, Some(localised(strings, label)));
            self.setup.expose.image(
                ImageKey::Name(gadget_key(INIT, &id)),
                Some(ImageSource::Combo(Arc::new(spec))),
            );
        }
    }

    pub(super) fn strategic_gadget_changed(&mut self, event: &GadgetEvent) -> bool {
        if !self.wotr.setup_shown() {
            return false;
        }
        let type_combo = gadget_key(SCENARIO_TYPE.0, SCENARIO_TYPE.1);
        let victory = gadget_key(INIT, &combo_id(VICTORY_ROW));
        let battle = gadget_key(INIT, &combo_id(BATTLE_ROW));
        match (event.key.as_deref(), event.change) {
            (Some(key), Change::Row(row)) if key == type_combo => {
                self.select_scenario(row);
                true
            }
            (Some(key), Change::Row(row)) if key == victory => {
                let _ = self.set_victory_type(row);
                true
            }
            (Some(key), Change::Row(row)) if key == battle => {
                let _ = self.set_battle_type(row);
                true
            }
            _ => false,
        }
    }

    pub(super) fn select_scenario(&mut self, row: usize) {
        let Some(data) = self.wotr.data.clone() else {
            return;
        };
        let Some(screen) = &mut self.wotr.setup else {
            return;
        };
        let Some(choice) = screen.choices.get(row) else {
            return;
        };
        screen.selected = row;
        self.catalogue.starts = max_players(&data.living, choice);
        self.lobby.map_label.clone_from(&choice.campaign);
        self.lobby.fit_to_map(self.catalogue.starts);
        self.scenario_chosen();
        self.grid = None;
        let (min, max) = self
            .wotr
            .setup
            .as_ref()
            .and_then(|s| s.choices.get(s.selected))
            .map_or((None, None), |c| scenario_limits(&data.living, c));
        let offered: Vec<&str> = self
            .wotr
            .setup
            .as_ref()
            .map(|s| s.spots.iter().map(|r| r.name.as_str()).collect())
            .unwrap_or_default();
        println!(
            "shell: Strategic scenario {row} - {:?} ({min:?}..{max:?} players), starts {:?}, \
             the preview offers {} region(s): {offered:?}",
            self.wotr
                .setup
                .as_ref()
                .and_then(|s| s.choices.get(s.selected))
                .map(|c| c.campaign.as_str()),
            self.start_summary(),
            offered.len()
        );
    }

    fn start_summary(&self) -> Vec<(usize, String)> {
        let Some(screen) = &self.wotr.setup else {
            return Vec::new();
        };
        self.lobby
            .slots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.occupant.occupied())
            .map(|(i, _)| (i, screen.picks.get(i).unwrap_or("-").to_owned()))
            .collect()
    }

    fn set_victory_type(&mut self, index: usize) -> Result<String, String> {
        let data = self.wotr.data.clone().ok_or("no data")?;
        let screen = self
            .wotr
            .setup
            .as_mut()
            .ok_or("not on the strategic setup")?;
        let choice = screen.choices.get(screen.selected).ok_or("no scenario")?;
        let options = wotr_setup::victory_options(&data.living, choice);
        let option = options
            .get(index)
            .ok_or_else(|| format!("victory type {index}: the campaign has {}", options.len()))?;
        screen.victory_type = index;
        let line = format!("victory type {index} ({})", option.name);
        println!("shell: Strategic {line}");
        Ok(line)
    }

    fn set_battle_type(&mut self, index: usize) -> Result<String, String> {
        let kind = wotr_setup::battle_type_at(index)
            .ok_or_else(|| format!("battle type {index}: there are three"))?;
        let screen = self
            .wotr
            .setup
            .as_mut()
            .ok_or("not on the strategic setup")?;
        screen.battle_type = kind;
        let line = format!("battle type {index} ({kind:?})");
        println!("shell: Strategic {line}");
        Ok(line)
    }

    pub(super) fn draw_living_world(&mut self, renderer: &mut Renderer) {
        let movie = self.setup.view.movie_id();
        let drawn = self.draw_preview(renderer);
        let live: Vec<InstanceId> = drawn.map(|(id, _, _)| id).into_iter().collect();
        self.wotr.surfaces.retain(renderer, movie, &live);
        if let Some(preview) = self.wotr.setup.as_mut().and_then(|s| s.preview.as_mut()) {
            preview.drawn = drawn;
        }
    }

    fn draw_preview(
        &mut self,
        renderer: &mut Renderer,
    ) -> Option<(InstanceId, [f32; 4], (u32, u32))> {
        if self.screen != Screen::SkirmishSetup {
            return None;
        }
        let data = self.wotr.data.clone()?;
        let screen = self.wotr.setup.as_mut()?;
        let ids = self.setup.view.surface_instances();
        let id = self
            .setup
            .view
            .surface_with_key(&ids, &gadget_key(LIVING_WORLD_MAP.0, LIVING_WORLD_MAP.1))?;
        let rect = self.setup.view.slot_rect(id)?;
        let choice = screen.choices.get(screen.selected)?;
        if screen
            .preview
            .as_ref()
            .is_none_or(|p| p.campaign != choice.campaign)
        {
            let (map, report) = WorldMap::load_preview(
                renderer,
                Arc::clone(&self.vfs),
                &data.living,
                &choice.campaign,
                0,
            );
            println!(
                "shell: Strategic preview at {rect:?} - {} region(s), {} pick triangle(s), \
                 missing {:?}",
                map.layout.regions.len(),
                report.pick_triangles,
                report.missing
            );
            screen.preview = Some(Preview {
                campaign: choice.campaign.clone(),
                map: Box::new(map),
                drawn: None,
            });
        }
        let cx = Scenario {
            data: &data.living,
            catalog: &data.catalog,
            choice,
            victory_type: screen.victory_type,
        };
        let colours = &screen.colours;
        let lobby = &self.lobby;
        let marks = wotr_setup::preview_marks(
            &cx,
            &screen.picks,
            lobby,
            |slot| match lobby.slots.get(slot)?.colour {
                Some(chosen) => colours.get(usize::from(chosen)).map(|c| c.colors.fill),
                None => Some(RANDOM_LIVING_WORLD_COLOR),
            },
            screen.hovered.as_deref(),
        );
        let preview = screen.preview.as_mut()?;
        let size = target_size(rect);
        #[allow(clippy::cast_precision_loss)]
        let aspect = size.0 as f32 / size.1 as f32;
        let key = (self.setup.view.movie_id(), id);
        let rendered = preview
            .map
            .preview_scene(renderer, &marks, aspect)
            .and_then(|scene| self.wotr.surfaces.render(renderer, key, size, &scene));
        match rendered {
            Ok(icon) => self
                .setup
                .view
                .set_surface(id, Some(SurfaceContent::Icon(icon))),
            Err(error) => {
                println!("shell: Strategic preview did not render ({error})");
                return None;
            }
        }
        Some((id, rect, size))
    }

    pub(super) fn advance_living_world(&mut self, elapsed: Duration) {
        if let Some(preview) = self.wotr.setup.as_mut().and_then(|s| s.preview.as_mut()) {
            preview.map.advance(elapsed.as_secs_f32());
        }
    }

    fn preview_region_at(&self, at: [f32; 2]) -> Option<Option<String>> {
        let data = self.wotr.data.as_ref()?;
        let screen = self.wotr.setup.as_ref()?;
        let preview = screen.preview.as_ref()?;
        let (_, [l, t, r, b], (w, h)) = preview.drawn?;
        if !(at[0] >= l && at[0] < r && at[1] >= t && at[1] < b) {
            return None;
        }
        #[allow(clippy::cast_precision_loss)]
        let (x, y) = (
            (at[0] - l) * w as f32 / (r - l).max(1.0),
            (at[1] - t) * h as f32 / (b - t).max(1.0),
        );
        let choice = screen.choices.get(screen.selected)?;
        let pick = preview.map.pick_region((w, h), x, y);
        Some(wotr_setup::picked_region(&data.living, choice, pick))
    }

    pub(super) fn preview_pointer_moved(&mut self, at: [f32; 2]) -> bool {
        let found = self.preview_region_at(at);
        let over = found.is_some();
        let region = found.flatten();
        let Some(screen) = self.wotr.setup.as_mut() else {
            return false;
        };
        if screen.hovered != region {
            let id = region.as_deref().and_then(|name| {
                let data = self.wotr.data.as_ref()?;
                let choice = screen.choices.get(screen.selected)?;
                mint_sage_worldmap::region_views(&data.living, &choice.campaign, 0)
                    .iter()
                    .position(|r| r.name.eq_ignore_ascii_case(name))
                    .and_then(|i| u16::try_from(i).ok())
                    .map(RegionId)
            });
            if let Some(preview) = screen.preview.as_mut() {
                preview.map.set_hovered(id);
            }
            println!("shell: Strategic preview hover {region:?}");
            screen.shown.clone_from(&region);
            screen.hovered = region;
        }
        over
    }

    pub(super) fn preview_pointer_button(&mut self, at: [f32; 2], pressed: bool) -> bool {
        let Some(found) = self.preview_region_at(at) else {
            return false;
        };
        if let (false, Some(region)) = (pressed, found) {
            let _ = self.click_region(&region);
        }
        true
    }

    fn click_region(&mut self, region: &str) -> Result<String, String> {
        let click = self
            .with_scenario(|cx, picks, lobby| picks.click(cx, lobby, region))
            .ok_or("not on the strategic setup")?;
        if let (Click::Assigned { region, .. } | Click::Released { region, .. }, Some(screen)) =
            (&click, &mut self.wotr.setup)
        {
            screen.shown = Some(region.clone());
        }
        self.grid = None;
        let line = match &click {
            Click::Assigned { slot, region } => format!("region {region} assigned to slot {slot}"),
            Click::Released { slot, region } => format!("region {region} released by slot {slot}"),
            Click::Refused(why) => format!("region {region} refused: {}", why_refused(why)),
        };
        println!(
            "shell: Strategic start {line}; starts {:?}",
            self.start_summary()
        );
        match click {
            Click::Refused(_) => Err(line),
            _ => Ok(line),
        }
    }

    pub(super) fn apply_grid_choice(&mut self, row: usize, choice: &Choice) -> bool {
        if !self.wotr.setup_shown() {
            return crate::setup_grid::apply(&mut self.lobby, &self.catalogue, row, choice);
        }
        let before = wotr_setup::snapshot(&self.lobby);
        let applied = crate::setup_grid::apply(&mut self.lobby, &self.catalogue, row, choice);
        if applied {
            self.with_scenario(|cx, picks, lobby| picks.reconcile(cx, lobby, &before));
        }
        applied
    }

    pub(super) fn strategic_start_clicked(&mut self) {
        let (Some(data), Some(screen)) = (self.wotr.data.clone(), &self.wotr.setup) else {
            return;
        };
        let Some(choice) = screen.choices.get(screen.selected) else {
            return;
        };
        let seed = crate::lobby::fresh_seed(self.lobby.random_seed);
        let cx = Scenario {
            data: &data.living,
            catalog: &data.catalog,
            choice,
            victory_type: screen.victory_type,
        };
        let setup =
            match wotr_setup::prepare(&cx, screen.battle_type, seed, &self.lobby, &screen.picks) {
                Ok(setup) => setup,
                Err(refusal) => {
                    self.refuse_start(refusal);
                    return;
                }
            };
        let colours = wotr_setup::seat_colours(&self.lobby, screen.colours.len());
        let opening = match LivingWorldState::start(&data.living, &data.catalog, &setup) {
            Ok(state) => state.state_hash(),
            Err(error) => {
                println!("shell: Strategic session start refused - {error}");
                self.refuse_start(wotr_setup::refusal_of(&error));
                return;
            }
        };
        println!("shell: Strategic setup {setup:?}");
        println!("shell: Strategic seat colours {colours:?}");
        println!("shell: Strategic opening hash 0x{opening:016x}");
        self.wotr.request = Some(Start {
            data,
            setup,
            colours,
        });
    }

    fn refuse_start(&mut self, refusal: Refusal) {
        match refusal {
            Refusal::Silent => {
                println!("shell: Strategic PLAY refused - no message");
            }
            Refusal::Counted(key, count) => {
                let text = mint_sage_str::format_placeholder(
                    &localised(&self.strings, key),
                    mint_sage_str::Placeholder::Str(&count.to_string()),
                );
                println!("shell: Strategic PLAY refused - {key}: {text}");
                self.show_message_box(&BoxRequest {
                    kind: BoxKind::Ok,
                    title: String::new(),
                    text,
                });
            }
            Refusal::Message(key) => {
                let text = localised(&self.strings, key);
                println!("shell: Strategic PLAY refused - {key}: {text}");
                self.show_message_box(&BoxRequest {
                    kind: BoxKind::Ok,
                    title: String::new(),
                    text,
                });
            }
        }
    }

    pub(super) fn act_wotr(&mut self, action: &str) -> Option<Result<String, String>> {
        let (verb, rest) = action.split_once(' ').unwrap_or((action, ""));
        let number = || {
            rest.trim()
                .parse::<usize>()
                .map_err(|e| format!("{action}: {e}"))
        };
        match verb {
            "skirmish" if self.screen == Screen::MainMenu => {
                self.enter(Screen::SkirmishSetup);
                Some(Ok("skirmish setup".to_owned()))
            }
            "strategic" if self.screen == Screen::SkirmishSetup => {
                let shown = self.enter_strategic();
                Some(Ok(format!("strategic setup: {shown}")))
            }
            "menu" if self.screen == Screen::MainMenu => {
                let (command, args) = rest.trim().split_once(' ').unwrap_or((rest.trim(), ""));
                self.main_menu_command(Some(command), args.trim());
                Some(Ok(format!("{command}({})", args.trim())))
            }
            "scenario" if self.wotr.setup_shown() => Some(number().map(|row| {
                self.select_scenario(row);
                format!("scenario {row}")
            })),
            "victory" if self.wotr.setup_shown() => {
                Some(number().and_then(|i| self.set_victory_type(i)))
            }
            "battle-type" if self.wotr.setup_shown() => {
                Some(number().and_then(|i| self.set_battle_type(i)))
            }
            "region" if self.wotr.setup_shown() => Some(self.click_region(rest.trim())),
            "play" if self.wotr.setup_shown() => {
                self.strategic_start_clicked();
                Some(Ok(format!(
                    "start requested: {}",
                    self.wotr.request.is_some()
                )))
            }
            "slot" if self.wotr.setup_shown() => Some(self.script_slot(rest)),
            "continue" if self.wotr.ending() => {
                self.wotr.end = None;
                self.enter(Screen::MainMenu);
                Some(Ok("back to the main menu".to_owned()))
            }
            _ => None,
        }
    }

    fn script_slot(&mut self, rest: &str) -> Result<String, String> {
        use crate::lobby::{Difficulty, Occupant};
        let words: Vec<&str> = rest.split_whitespace().collect();
        let row = words
            .first()
            .and_then(|w| w.parse::<usize>().ok())
            .ok_or("slot wants a row")?;
        let choice = match words.get(1..) {
            Some(["faction", "random"]) => Choice::Faction(None),
            Some(["faction", name]) => Choice::Faction(Some((*name).to_owned())),
            Some(["team", "none"]) => Choice::Team(None),
            Some(["team", n]) => Choice::Team(Some(n.parse().map_err(|_| "team wants a number")?)),
            Some(["colour", "random"]) => Choice::Colour(None),
            Some(["colour", n]) => {
                Choice::Colour(Some(n.parse().map_err(|_| "colour wants a number")?))
            }
            Some(["closed"]) => Choice::Occupant(Occupant::Closed),
            Some(["ai", level]) => Choice::Occupant(Occupant::Ai(match *level {
                "easy" => Difficulty::Easy,
                "medium" => Difficulty::Medium,
                "hard" => Difficulty::Hard,
                "brutal" => Difficulty::Brutal,
                other => return Err(format!("ai level {other}")),
            })),
            _ => return Err(format!("slot {rest}: not a grid choice")),
        };
        let applied = self.apply_grid_choice(row, &choice);
        self.grid = None;
        Ok(format!(
            "slot {row} {choice:?} {}; starts {:?}",
            if applied { "set" } else { "refused" },
            self.start_summary()
        ))
    }

    pub(crate) fn show_wotr_end(&mut self, report: EndReport) {
        println!(
            "shell: Strategic end - {:?} on turn {}, title {:?}, scenario {:?}, condition {:?}, image {:?}",
            report.outcome,
            report.turn,
            report.title_key,
            report.scenario_key,
            report.condition_key,
            report.image
        );
        for (index, row) in report.rows.iter().enumerate() {
            println!(
                "shell: Strategic stats row {index}: {} team {} regions {} armies {} {}",
                row.template,
                row.team,
                row.regions,
                row.armies,
                if row.defeated { "defeated" } else { "standing" }
            );
        }
        self.wotr.end = Some(report);
        self.enter(Screen::TimeLine);
        self.declare_wotr_end();
    }

    fn declare_wotr_end(&mut self) {
        let Some(report) = &self.wotr.end else {
            return;
        };
        let expose = &mut self.time_line.expose;
        expose.set_extern("TimeLine:ScreenMode", Value::str("OtherSingle"));
        expose.set_extern("TimeLine:ShowSaveReplay", Value::Bool(false));
        expose.set_extern("TimeLine:NumCahAwards", Value::Number(0.0));
        expose.set_extern("TimeLine:LocalPlayerIsObserver", Value::Bool(false));
        expose.set_extern("TimeLine:StrategicEnd", Value::Bool(true));
        #[allow(clippy::cast_precision_loss)]
        expose.set_extern(
            "TimeLine:NumOfPlayers",
            Value::Number(report.rows.len() as f64),
        );
        let status = localised(&self.strings, report.title_key);
        expose.string("StrategicWinLossTitle", Some(status.clone()));
        expose.string(
            "StrategicScenarioName",
            report
                .scenario_key
                .as_deref()
                .map(|key| localised(&self.strings, key)),
        );
        expose.string(
            "StrategicWinLossConditionDescription",
            report
                .condition_key
                .as_deref()
                .map(|key| localised(&self.strings, key)),
        );
        expose.image(
            ImageKey::Name("StrategicWinLossImage".to_owned()),
            Some(ImageSource::MappedImage(report.image.clone())),
        );
        let rows = report
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let name = localised(&self.strings, &format!("INI:{}", row.template));
                expose.string(&format!("TimeLine:PlayerName:{index}"), Some(name.clone()));
                expose.set_extern(
                    &format!("TimeLine:PlayerColor:{index}"),
                    Value::str(&row.colour_extern()),
                );
                expose.string(
                    &format!("TimeLine:PlayerScore:{index}"),
                    Some(String::new()),
                );
                if let Some(icon) = row.faction_icon() {
                    expose.image(
                        ImageKey::Name(format!("TimeLine:PlayerFactionIcon:{index}")),
                        Some(ImageSource::MappedImage(icon)),
                    );
                }
                vec![
                    CellSpec::Text(name),
                    CellSpec::Text(row.regions.to_string()),
                    CellSpec::Text(row.armies.to_string()),
                    CellSpec::Text(if index == report.local {
                        status.clone()
                    } else {
                        String::new()
                    }),
                ]
            })
            .collect();
        for i in 0..=10 {
            expose.string(&format!("Timeline:XAxis:{i}"), Some(String::new()));
            expose.string(&format!("Timeline:YAxis:{i}"), Some(String::new()));
        }
        println!(
            "shell: Strategic score page - {} player(s): {}",
            report.rows.len(),
            report
                .rows
                .iter()
                .map(|row| format!(
                    "{}/{}/{}/{}",
                    row.template,
                    row.side.as_deref().unwrap_or("-"),
                    row.colour_extern(),
                    if row.defeated { "defeated" } else { "standing" }
                ))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let list = ListSpec {
            columns: vec![(40, false), (20, true), (20, true), (20, true)],
            rows,
            selected: None,
            scroll_bar: None,
            text_inset_percent: 0,
            scroll_to_top: false,
        };
        expose.image(
            ImageKey::Name(gadget_key(STATS_LIST.0, STATS_LIST.1)),
            Some(ImageSource::List(Arc::new(list))),
        );
    }
}

fn why_refused(why: &ClickRefusal) -> String {
    match why {
        ClickRefusal::NotStartable => "not an enabled region".to_owned(),
        ClickRefusal::Disallowed => "the scenario disallows starting there".to_owned(),
        ClickRefusal::Historical => "a historical scenario fixes its starts".to_owned(),
        ClickRefusal::NoFreeSeat => "every seat has a region".to_owned(),
        ClickRefusal::Engine(error) => error.to_string(),
    }
}

fn max_players(data: &LivingWorldData, choice: &ScenarioChoice) -> u8 {
    data.campaigns
        .iter()
        .find(|c| c.name == choice.campaign)
        .and_then(|c| c.scenarios.first())
        .and_then(|s| s.max_players)
        .map_or(8, |n| u8::try_from(n.clamp(1, 8)).unwrap_or(8))
}

fn scenario_limits(data: &LivingWorldData, choice: &ScenarioChoice) -> (Option<i32>, Option<i32>) {
    data.campaigns
        .iter()
        .find(|c| c.name == choice.campaign)
        .and_then(|c| c.scenarios.first())
        .map_or((None, None), |s| (s.min_players, s.max_players))
}
