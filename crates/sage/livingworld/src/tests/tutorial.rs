use mint_sage_ini::living_world::{load_files, LivingWorldData};
use mint_sage_ini::MemorySources;

use super::{catalog, regions, TEMPLATES};
use crate::model::{
    Battle, BattleMode, Command, Event, Phase, PlayerId, PlayerKind, RegionId, Rejected,
};
use crate::stand_in::DefenderWins;
use crate::state::LivingWorldState;
use crate::tutorial::{
    autoplay, next_action, scripted_setup, Action, Blocked, SessionAudio, Silent, Tutorial,
    TutorialError, TutorialEvent, UiAction, COMPLETED_TEXT, COMPLETED_TITLE,
};

const GOOD: PlayerId = PlayerId(0);
const EVIL: PlayerId = PlayerId(1);
const A: RegionId = RegionId(0);
const B: RegionId = RegionId(1);
const C: RegionId = RegionId(2);
const D: RegionId = RegionId(3);

const OWNERSHIP: &str =
    "  OwnershipSet\n   Regions = A B C\n   StartRegion = A\n   SpawnBuildings\n    \
                         Region = A\n    Buildings = Barracks\n   End\n  End\n  OwnershipSet\n   \
                         Regions = D E\n   StartRegion = E\n  End\n";

const ACT: &str = " Act One
  SpawnArmy
   ScriptingName = ElfHero
   SpawnForTemplates = Elves
   PlayerArmy = Champ
   HeroTemplateName = hero_a
   InitialRegion = A
  End
  SpawnArmy
   ScriptingName = ElfGarrison
   SpawnForTemplates = Elves
   PlayerArmy = Levy
   InitialRegion = A
  End
  SpawnArmy
   ScriptingName = OrcHero
   SpawnForTemplates = Orcs
   PlayerArmy = Champ
   HeroTemplateName = hero_b
   InitialRegion = E
  End
 End
";

const VARIANTS: &str = "
LivingWorldBuilding OrcFarm
 Type = Resource
 AvailableTo = Orcs
End
LivingWorldBuilding ElfFarm
 Type = Resource
 AvailableTo = Elves
End
";

fn capped_regions() -> String {
    regions().replace(
        "  BuildingSpot = X:2 Y:2\n",
        "  BuildingSpot = X:2 Y:2\n  RestrictBuildings\n   Buildings = Barracks Resource\n   NumberAllowed = 1\n  End\n",
    )
}

fn session(task: &str, params: &str) -> String {
    format!(
        "   Session\n    TaskAfterAudio\n     Task = {task}\n     Params = {params}\n    End\n   End\n"
    )
}

fn phase(name: &str, body: &str) -> String {
    format!("  Phase {name}\n{body}  End\n")
}

fn turn(n: u32, body: &str) -> String {
    format!(" Turn {n}\n{body} End\n")
}

fn data(tutorial_turns: &str, ownership: &str) -> LivingWorldData {
    data_in(TEMPLATES, tutorial_turns, ownership)
}

fn data_in(templates: &str, tutorial_turns: &str, ownership: &str) -> LivingWorldData {
    let campaign = format!(
        "LivingWorldCampaign Tut\n IsScriptedCampaign = Yes\n AddPlayer Good\n  PlayerTemplate = Elves\n  \
         BaseRegion = A\n  TeamNumber = 0\n End\n AddPlayer Evil\n  PlayerTemplate = Orcs\n  IsDumb = Yes\n  \
         BaseRegion = E\n  TeamNumber = 1\n End\n LocalPlayer = Good\n Scenario\n  RegionCampaign = World\n\
         {ownership} End\n Tutorial\n{tutorial_turns} End\n{ACT}End\n"
    );
    let mut sources = MemorySources::new();
    sources.insert(
        "data/ini/t.ini",
        format!("{templates}{VARIANTS}{}{campaign}", capped_regions()).into_bytes(),
    );
    let load = load_files(&sources, &["data/ini/t.ini".to_owned()]);
    assert!(load.errors.is_empty(), "{:?}", load.errors);
    load.data
}

fn build_with(templates: &str, turns: &str) -> (LivingWorldData, LivingWorldState, Tutorial) {
    let data = data_in(templates, turns, OWNERSHIP);
    let setup = scripted_setup(&data, "Tut", 1).unwrap();
    let state = LivingWorldState::start(&data, &catalog(), &setup).unwrap();
    let tutorial = Tutorial::new(&data, &state).unwrap();
    (data, state, tutorial)
}

fn build(
    turns: &str,
    ownership: &str,
) -> Result<(LivingWorldData, LivingWorldState, Tutorial), TutorialError> {
    let data = data(turns, ownership);
    let setup = scripted_setup(&data, "Tut", 1).unwrap();
    let state = LivingWorldState::start(&data, &catalog(), &setup).unwrap();
    let tutorial = Tutorial::new(&data, &state)?;
    Ok((data, state, tutorial))
}

fn one_task(task: &str, params: &str) -> (LivingWorldState, Tutorial) {
    one_task_in("Planning", task, params, OWNERSHIP)
}

fn one_task_in(
    phase_name: &str,
    task: &str,
    params: &str,
    ownership: &str,
) -> (LivingWorldState, Tutorial) {
    let script = turn(1, &phase(phase_name, &session(task, params)));
    let (_, state, tutorial) = build(&script, ownership).unwrap();
    (state, tutorial)
}

fn tick(t: &mut Tutorial, s: &mut LivingWorldState) -> Vec<TutorialEvent> {
    t.tick(s, None, &mut Silent).tutorial
}

fn begin(t: &mut Tutorial, s: &mut LivingWorldState) {
    tick(t, s);
}

struct Sound {
    frames: u32,
    fails: bool,
}

impl Sound {
    fn update(&mut self) {
        self.frames = self.frames.saturating_sub(1);
    }
}

impl SessionAudio for Sound {
    fn play(&mut self, _: &str) -> Option<u32> {
        (!self.fails).then_some(7)
    }

    fn playing(&self, _: u32) -> bool {
        self.frames > 0
    }
}

fn voiced(task: &str, params: &str) -> (LivingWorldState, Tutorial) {
    let body = format!(
        "   Session\n    Audio = Line\n    TaskAfterAudio\n     Task = {task}\n     Params = {params}\n    End\n   End\n"
    );
    let (_, state, tutorial) = build(&turn(1, &phase("Planning", &body)), OWNERSHIP).unwrap();
    (state, tutorial)
}

fn hero(s: &LivingWorldState) -> crate::model::ArmyId {
    s.scripted_army("ElfHero").unwrap()
}

fn garrison(s: &LivingWorldState) -> crate::model::ArmyId {
    s.scripted_army("ElfGarrison").unwrap()
}

fn done(events: &[TutorialEvent]) -> bool {
    events
        .iter()
        .any(|e| matches!(e, TutorialEvent::SessionDone { .. }))
}

fn go(t: &mut Tutorial, s: &mut LivingWorldState, c: Command) -> Result<Vec<Event>, Blocked> {
    t.apply(s, Some(&mut DefenderWins), GOOD, c)
}

fn battle_at(s: &mut LivingWorldState, id: u32, region: RegionId, players: &[PlayerId]) {
    s.battles.push(Battle {
        id,
        region,
        players: players.to_vec(),
        choices: Vec::new(),
        withdrawn: Vec::new(),
        snapshot: Vec::new(),
    });
}

#[test]
fn the_start_is_the_add_player_records_with_the_local_player_human_and_the_dumb_one_inert() {
    let (_, s, _) = build(
        &turn(1, &phase("Planning", &session("ScriptedDelay", "1.0"))),
        OWNERSHIP,
    )
    .unwrap();
    assert_eq!(s.kind(GOOD), Some(PlayerKind::Human));
    assert_eq!(s.kind(EVIL), Some(PlayerKind::Inert));
    assert!(s.scripted.is_some());
    assert_eq!(s.armies_of(EVIL).count(), 1);
}

#[test]
fn spawn_buildings_places_only_the_variants_available_to_the_owner() {
    let ownership = "  OwnershipSet\n   Regions = A B C\n   StartRegion = A\n   SpawnBuildings\n    \
                     Region = A\n    Buildings = OrcFarm ElfFarm\n   End\n  End\n  OwnershipSet\n   \
                     Regions = D E\n   StartRegion = E\n  End\n";
    let (_, s, _) = build(
        &turn(1, &phase("Planning", &session("ScriptedDelay", "1.0"))),
        ownership,
    )
    .unwrap();
    let placed: Vec<_> = s.region(A).unwrap().plots.iter().flatten().collect();
    assert_eq!(placed.len(), 1);
    assert_eq!(placed[0].building, s.rules.building_id("ElfFarm").unwrap());
}

#[test]
fn a_scripted_army_move_orders_the_named_army_at_once_and_its_session_ends() {
    let (mut s, mut t) = one_task("ScriptedArmyMove", "OrcHero D");
    let events = tick(&mut t, &mut s);
    let orc = s.scripted_army("OrcHero").unwrap();
    assert_eq!(s.armies[&orc].order, Some(D));
    assert!(events
        .iter()
        .any(|e| matches!(e, TutorialEvent::ScriptedMove { result: Ok(()), .. })));
    assert!(done(&events));
}

#[test]
fn a_scripted_delay_counts_truncated_seconds_times_five_in_lwl_updates() {
    for (seconds, updates) in [("0.5", 2), ("1.0", 5), ("1.5", 7), ("0.1", 0)] {
        let (mut s, mut t) = one_task("ScriptedDelay", seconds);
        begin(&mut t, &mut s);
        for _ in 1..updates {
            assert!(!done(&tick(&mut t, &mut s)), "{seconds} s ended early");
        }
        if updates > 0 {
            assert!(done(&tick(&mut t, &mut s)), "{seconds} s did not end");
        }
    }
}

#[test]
fn the_next_session_starts_in_the_update_its_predecessor_is_done_and_updates_in_the_next() {
    let body = format!(
        "{}{}",
        session("ScriptedArmyMove", "OrcHero D"),
        session("ScriptedDelay", "5.0")
    );
    let (_, mut s, mut t) = build(&turn(1, &phase("Planning", &body)), OWNERSHIP).unwrap();
    let count = |events: &[TutorialEvent], pick: fn(&TutorialEvent) -> bool| {
        events.iter().filter(|e| pick(e)).count()
    };
    let started = |e: &TutorialEvent| matches!(e, TutorialEvent::SessionStarted { .. });
    let began = |e: &TutorialEvent| matches!(e, TutorialEvent::TaskBegan { .. });
    let first = tick(&mut t, &mut s);
    assert_eq!((count(&first, started), count(&first, began)), (2, 1));
    assert_eq!(count(&tick(&mut t, &mut s), began), 1);
}

#[test]
fn the_task_starts_only_once_the_audio_has_stopped_and_in_the_update_that_sees_it_stopped() {
    let (mut s, mut t) = voiced("ScriptedArmyMove", "OrcHero D");
    let orc = s.scripted_army("OrcHero").unwrap();
    let mut sound = Sound {
        frames: 3,
        fails: false,
    };
    for _ in 0..3 {
        t.tick(&mut s, None, &mut sound);
        sound.update();
        assert_eq!(s.armies[&orc].order, None);
    }
    t.tick(&mut s, None, &mut sound);
    assert_eq!(s.armies[&orc].order, Some(D));
}

#[test]
fn a_session_starts_and_updates_in_the_same_update_when_it_has_no_sound_or_the_sound_fails() {
    let (mut s, mut t) = one_task("ScriptedArmyMove", "OrcHero D");
    let orc = s.scripted_army("OrcHero").unwrap();
    let events = tick(&mut t, &mut s);
    assert!(events
        .iter()
        .any(|e| matches!(e, TutorialEvent::SessionStarted { .. })));
    assert_eq!(s.armies[&orc].order, Some(D));

    let (mut s, mut t) = voiced("ScriptedArmyMove", "OrcHero D");
    let orc = s.scripted_army("OrcHero").unwrap();
    let mut failed = Sound {
        frames: 99,
        fails: true,
    };
    t.tick(&mut s, None, &mut failed);
    assert_eq!(s.armies[&orc].order, Some(D));
}

#[test]
fn build_barracks_completes_on_a_barracks_in_its_region_and_refuses_the_rest() {
    let (mut s, mut t) = one_task_in("Planning", "BuildBarracks", "A", "  OwnershipSet\n   Regions = A B C\n   StartRegion = A\n  End\n  OwnershipSet\n   Regions = D E\n   StartRegion = E\n  End\n");
    begin(&mut t, &mut s);
    let barracks = s.rules.building_id("Barracks").unwrap();
    let farm = s.rules.building_id("Farm").unwrap();
    let elsewhere = Command::Build {
        region: B,
        plot: 0,
        building: barracks,
    };
    assert_eq!(
        go(&mut t, &mut s, elsewhere).unwrap_err(),
        Blocked::ByTutorial
    );
    go(
        &mut t,
        &mut s,
        Command::Build {
            region: A,
            plot: 0,
            building: farm,
        },
    )
    .unwrap();
    assert!(t.active_task(&s).is_some());
    go(
        &mut t,
        &mut s,
        Command::Build {
            region: A,
            plot: 1,
            building: barracks,
        },
    )
    .unwrap();
    assert!(t.active_task(&s).is_none());
}

#[test]
fn train_unit_completes_on_queueing_in_its_region_and_refuses_it_between_tasks() {
    let (mut s, mut t) = one_task("TrainUnit", "A");
    let queue = Command::QueueArmy {
        region: A,
        plot: 0,
        entry: 0,
    };
    assert_eq!(
        go(&mut t, &mut s, queue.clone()).unwrap_err(),
        Blocked::ByTutorial
    );
    begin(&mut t, &mut s);
    assert!(t.active_task(&s).is_some());
    go(&mut t, &mut s, queue).unwrap();
    assert!(t.active_task(&s).is_none());
}

#[test]
fn train_unit_names_the_first_barracks_plot_and_refuses_the_others() {
    let two = "  OwnershipSet\n   Regions = A B C\n   StartRegion = A\n   SpawnBuildings\n    \
               Region = A\n    Buildings = Barracks Barracks\n   End\n  End\n  OwnershipSet\n   \
               Regions = D E\n   StartRegion = E\n  End\n";
    let (mut s, mut t) = one_task_in("Planning", "TrainUnit", "A", two);
    begin(&mut t, &mut s);
    let queue = |plot| Command::QueueArmy {
        region: A,
        plot,
        entry: 0,
    };
    assert_eq!(
        go(&mut t, &mut s, queue(1)).unwrap_err(),
        Blocked::ByTutorial
    );
    assert!(t.active_task(&s).is_some());
    go(&mut t, &mut s, queue(0)).unwrap();
    assert!(t.active_task(&s).is_none());
}

#[test]
fn production_clicks_follow_the_named_plot_and_the_list_is_offered_only_to_train_unit() {
    let two = "  OwnershipSet\n   Regions = A B C\n   StartRegion = A\n   SpawnBuildings\n    \
               Region = A\n    Buildings = Barracks Barracks\n   End\n  End\n  OwnershipSet\n   \
               Regions = D E\n   StartRegion = E\n  End\n";
    let (mut s, mut t) = one_task_in("Planning", "TrainUnit", "A", two);
    let entry = |plot, entry| UiAction::ProductionEntry {
        region: A,
        plot,
        entry,
    };
    assert!(!t.ui(&s, entry(0, 1)));
    begin(&mut t, &mut s);
    assert!(t.ui(&s, entry(0, 1)));
    assert!(
        !t.ui(&s, entry(1, 1)),
        "the other barracks is not the task's"
    );
    assert!(t.ui(&s, UiAction::ClickBuilding { region: A, plot: 0 }));
    assert!(!t.ui(&s, UiAction::ClickBuilding { region: A, plot: 1 }));
}

#[test]
fn train_unit_offers_only_the_entries_that_take_one_turn() {
    let (mut s, mut t) = one_task("TrainUnit", "A");
    begin(&mut t, &mut s);
    let entry = |entry| UiAction::ProductionEntry {
        region: A,
        plot: 0,
        entry,
    };
    assert!(!t.ui(&s, entry(0)));
    assert!(t.ui(&s, entry(1)));
    assert!(!t.ui(&s, entry(2)), "no such entry");
}

#[test]
fn the_scripted_human_queues_the_first_entry_the_list_offers() {
    let (mut s, mut t) = one_task("TrainUnit", "A");
    begin(&mut t, &mut s);
    assert!(matches!(
        next_action(&t, &s),
        Some(Action::Command(Command::QueueArmy { entry: 1, .. }))
    ));
}

#[test]
fn an_army_to_spawn_without_a_build_time_takes_one_turn() {
    let bare = TEMPLATES.replace("   BuildTime = 1\n", "");
    assert_ne!(bare, TEMPLATES);
    let (_, s, _) = build_with(
        &bare,
        &turn(1, &phase("Planning", &session("TrainUnit", "A"))),
    );
    let barracks = s.region(A).unwrap().plots[0].as_ref().unwrap().building;
    let queue = s.rules.building(barracks).unwrap().queue.as_ref().unwrap();
    assert_eq!(
        queue
            .entries
            .iter()
            .map(|e| e.build_time)
            .collect::<Vec<_>>(),
        [2, 1]
    );
}

#[test]
fn a_barracks_still_under_construction_cannot_finish_train_unit() {
    let (mut s, mut t) = one_task("TrainUnit", "A");
    s.region_mut(A).unwrap().plots[0]
        .as_mut()
        .unwrap()
        .under_construction = true;
    begin(&mut t, &mut s);
    let queue = Command::QueueArmy {
        region: A,
        plot: 0,
        entry: 1,
    };
    assert_eq!(
        go(&mut t, &mut s, queue).unwrap_err(),
        Blocked::Rejected(Rejected::NoQueue)
    );
    assert!(t.active_task(&s).is_some());
}

#[test]
fn the_details_tray_is_allowed_from_open_details_panel_and_after_auto_resolve() {
    let (mut s, mut t) = one_task("MergeGarrison", "A ElfHero");
    assert!(!t.details_tray_allowed());
    begin(&mut t, &mut s);
    assert!(!t.details_tray_allowed(), "no other task opens it");

    let (mut s, mut t) = one_task("OpenDetailsPanel", "A");
    assert!(!t.details_tray_allowed());
    begin(&mut t, &mut s);
    assert!(t.details_tray_allowed());

    let (mut s, mut t) = one_task_in(
        "ResolveBattles",
        "EnableAutoResolveButtonAndWaitForCompletion",
        "B",
        OWNERSHIP,
    );
    s.phase = Phase::ResolveBattles;
    let (h, o) = (hero(&s), scripted(&s, "OrcHero"));
    s.armies.get_mut(&h).unwrap().region = B;
    s.armies.get_mut(&o).unwrap().region = B;
    s.region_mut(B).unwrap().present = vec![h, o];
    battle_at(&mut s, 0, B, &[GOOD, EVIL]);
    begin(&mut t, &mut s);
    assert!(!t.details_tray_allowed());
    let auto = Command::ChooseBattleMode {
        battle: 0,
        method: 1,
    };
    go(&mut t, &mut s, auto).unwrap();
    tick(&mut t, &mut s);
    assert!(t.details_tray_allowed());
}

#[test]
fn a_plot_click_to_build_is_allowed_only_in_the_barracks_region() {
    let (mut s, mut t) = one_task("BuildBarracks", "A");
    begin(&mut t, &mut s);
    assert!(t.ui(&s, UiAction::ClickPlot { region: A, plot: 1 }));
    assert!(!t.ui(&s, UiAction::ClickPlot { region: B, plot: 0 }));
}

#[test]
fn train_unit_in_another_region_does_not_complete_it() {
    let (mut s, mut t) = one_task("TrainUnit", "B");
    begin(&mut t, &mut s);
    let _ = go(
        &mut t,
        &mut s,
        Command::QueueArmy {
            region: A,
            plot: 0,
            entry: 0,
        },
    );
    assert!(t.active_task(&s).is_some());
}

#[test]
fn merge_garrison_completes_only_on_the_swap_of_the_region_garrison_and_the_named_army() {
    let (mut s, mut t) = one_task("MergeGarrison", "A ElfHero");
    begin(&mut t, &mut s);
    let (hero, garrison) = (hero(&s), garrison(&s));
    let members: Vec<u32> = s.armies[&garrison].roster.iter().map(|m| m.id).collect();
    let wrong = Command::SwapArmyMembers {
        a: hero,
        b: scripted(&s, "OrcHero"),
        a_members: Vec::new(),
        b_members: Vec::new(),
    };
    assert_eq!(go(&mut t, &mut s, wrong).unwrap_err(), Blocked::ByTutorial);
    let reversed = Command::SwapArmyMembers {
        a: hero,
        b: garrison,
        a_members: Vec::new(),
        b_members: members.clone(),
    };
    assert_eq!(
        go(&mut t, &mut s, reversed).unwrap_err(),
        Blocked::ByTutorial
    );
    let nothing_moves = Command::SwapArmyMembers {
        a: garrison,
        b: hero,
        a_members: Vec::new(),
        b_members: Vec::new(),
    };
    go(&mut t, &mut s, nothing_moves).unwrap();
    assert!(t.active_task(&s).is_some());
    let right = Command::SwapArmyMembers {
        a: garrison,
        b: hero,
        a_members: members,
        b_members: Vec::new(),
    };
    go(&mut t, &mut s, right).unwrap();
    assert!(t.active_task(&s).is_none());
}

fn scripted(s: &LivingWorldState, name: &str) -> crate::model::ArmyId {
    s.scripted_army(name).unwrap()
}

#[test]
fn move_hero_army_completes_when_the_armys_order_ends_in_the_region() {
    let (mut s, mut t) = one_task("MoveHeroArmyToRegion", "ElfHero B");
    let army = hero(&s);
    crate::engine::apply(&mut s, GOOD, Command::MoveArmy { army, dest: C }).unwrap();
    begin(&mut t, &mut s);
    assert!(t.active_task(&s).is_some());
    let off = Command::MoveArmy { army, dest: C };
    assert_eq!(go(&mut t, &mut s, off).unwrap_err(), Blocked::ByTutorial);
    assert!(t.active_task(&s).is_some());
    go(&mut t, &mut s, Command::MoveArmy { army, dest: B }).unwrap();
    assert!(t.active_task(&s).is_none());
}

#[test]
fn a_move_order_may_be_cancelled_only_while_the_move_task_is_incomplete() {
    let (mut s, mut t) = one_task("MoveHeroArmyToRegion", "ElfHero B");
    begin(&mut t, &mut s);
    let army = hero(&s);
    assert!(go(&mut t, &mut s, Command::CancelMove { army }).is_ok());
    let other = garrison(&s);
    assert!(!matches!(
        go(&mut t, &mut s, Command::CancelMove { army: other }),
        Err(Blocked::ByTutorial)
    ));
    go(&mut t, &mut s, Command::MoveArmy { army, dest: B }).unwrap();
    assert_eq!(
        go(&mut t, &mut s, Command::CancelMove { army }).unwrap_err(),
        Blocked::ByTutorial
    );
}

#[test]
fn open_details_panel_needs_the_garrison_army_opened_and_the_hud_signal() {
    let (mut s, mut t) = one_task("OpenDetailsPanel", "A");
    assert!(!t.ui(&s, UiAction::SelectArmy(garrison(&s))));
    begin(&mut t, &mut s);
    assert!(!t.ui(&s, UiAction::SelectArmy(hero(&s))));
    assert!(t.ui(&s, UiAction::SelectArmy(garrison(&s))));
    assert!(t.active_task(&s).is_some());
    assert!(t.ui(&s, UiAction::HudDetailsSignal));
    assert!(t.active_task(&s).is_none());
}

#[test]
fn open_details_panel_clears_each_signal_when_the_tray_closes_or_the_garrison_is_deselected() {
    let (mut s, mut t) = one_task("OpenDetailsPanel", "A");
    begin(&mut t, &mut s);
    let g = garrison(&s);
    assert!(t.ui(&s, UiAction::HudDetailsSignal));
    assert!(t.ui(&s, UiAction::HudDetailsClosed));
    assert!(t.ui(&s, UiAction::SelectArmy(g)));
    assert!(
        t.active_task(&s).is_some(),
        "the tray closed after it opened"
    );
    assert!(t.ui(&s, UiAction::DeselectArmy(g)));
    assert!(t.ui(&s, UiAction::HudDetailsSignal));
    assert!(t.active_task(&s).is_some(), "the garrison was deselected");
    assert!(t.ui(&s, UiAction::DeselectArmy(hero(&s))));
    assert!(t.ui(&s, UiAction::SelectArmy(g)));
    assert!(t.active_task(&s).is_none());
    assert!(t.ui(&s, UiAction::HudDetailsClosed), "done stays done");
    assert!(t.active_task(&s).is_none());
}

#[test]
fn only_the_armies_a_task_names_may_be_selected() {
    let (mut s, mut t) = one_task("MoveHeroArmyToRegion", "ElfHero B");
    begin(&mut t, &mut s);
    assert!(t.ui(&s, UiAction::SelectArmy(hero(&s))));
    assert!(!t.ui(&s, UiAction::SelectArmy(garrison(&s))));

    let (mut s, mut t) = one_task("MergeGarrison", "A ElfHero");
    begin(&mut t, &mut s);
    assert!(t.ui(&s, UiAction::SelectArmy(garrison(&s))));
    assert!(!t.ui(&s, UiAction::SelectArmy(hero(&s))));

    let (mut s, mut t) = one_task("ScriptedDelay", "5.0");
    begin(&mut t, &mut s);
    assert!(!t.ui(&s, UiAction::SelectArmy(hero(&s))));
}

#[test]
fn demolishing_is_refused_throughout_the_tutorial() {
    let (mut s, mut t) = one_task("BuildBarracks", "A");
    begin(&mut t, &mut s);
    let demolish = Command::Demolish { region: A, plot: 0 };
    assert_eq!(
        go(&mut t, &mut s, demolish).unwrap_err(),
        Blocked::ByTutorial
    );
}

#[test]
fn select_battle_completes_on_selecting_its_battle_only() {
    let (mut s, mut t) = one_task_in("ResolveBattles", "SelectBattle", "B", OWNERSHIP);
    s.phase = Phase::ResolveBattles;
    battle_at(&mut s, 0, B, &[GOOD, EVIL]);
    battle_at(&mut s, 1, D, &[EVIL]);
    begin(&mut t, &mut s);
    assert!(!t.ui(&s, UiAction::SelectBattle(D)));
    assert!(t.active_task(&s).is_some());
    assert!(t.ui(&s, UiAction::SelectBattle(B)));
    assert!(t.active_task(&s).is_none());
}

#[test]
fn only_the_battle_the_active_task_names_may_be_selected() {
    let (mut s, mut t) = one_task_in("ResolveBattles", "SelectBattle", "B", OWNERSHIP);
    s.phase = Phase::ResolveBattles;
    battle_at(&mut s, 0, B, &[GOOD, EVIL]);
    battle_at(&mut s, 1, D, &[EVIL]);
    assert!(!t.battle_selectable(&s, B), "no task has begun");
    begin(&mut t, &mut s);
    assert!(t.battle_selectable(&s, B));
    assert!(!t.battle_selectable(&s, D));
}

#[test]
fn auto_resolve_waits_for_its_battle_to_finish_and_allows_only_the_auto_choice() {
    let (mut s, mut t) = one_task_in(
        "ResolveBattles",
        "EnableAutoResolveButtonAndWaitForCompletion",
        "B",
        OWNERSHIP,
    );
    s.phase = Phase::ResolveBattles;
    let (h, o) = (hero(&s), scripted(&s, "OrcHero"));
    s.armies.get_mut(&h).unwrap().region = B;
    s.armies.get_mut(&o).unwrap().region = B;
    s.region_mut(B).unwrap().present = vec![h, o];
    battle_at(&mut s, 0, B, &[GOOD, EVIL]);
    battle_at(&mut s, 1, D, &[EVIL]);
    begin(&mut t, &mut s);
    assert!(t.active_task(&s).is_some());
    let real_time = Command::ChooseBattleMode {
        battle: 0,
        method: 3,
    };
    assert_eq!(
        go(&mut t, &mut s, real_time).unwrap_err(),
        Blocked::ByTutorial
    );
    let force = Command::ForceRealTime { battle: 0 };
    assert_eq!(go(&mut t, &mut s, force).unwrap_err(), Blocked::ByTutorial);
    let by_region = Command::ResolveBattle {
        region: B,
        mode: BattleMode::RealTime,
    };
    assert_eq!(
        go(&mut t, &mut s, by_region).unwrap_err(),
        Blocked::ByTutorial
    );
    assert!(t.active_task(&s).is_some());
    assert_eq!(t.battle_methods(&s), 2);
    let other_battle = Command::ChooseBattleMode {
        battle: 1,
        method: 1,
    };
    assert!(!matches!(
        go(&mut t, &mut s, other_battle),
        Err(Blocked::ByTutorial)
    ));
    assert!(t.active_task(&s).is_some());
    go(
        &mut t,
        &mut s,
        Command::ChooseBattleMode {
            battle: 0,
            method: 1,
        },
    )
    .unwrap();
    assert!(!s.battles.iter().any(|b| b.region == B));
    assert!(t.active_task(&s).is_none());
    assert_eq!(t.battle_methods(&s), 0);
}

#[test]
fn auto_resolve_is_keyed_on_the_battle_found_at_begin() {
    let (mut s, mut t) = one_task_in(
        "ResolveBattles",
        "EnableAutoResolveButtonAndWaitForCompletion",
        "B",
        OWNERSHIP,
    );
    s.phase = Phase::ResolveBattles;
    begin(&mut t, &mut s);
    assert!(t.active_task(&s).is_some());
}

#[test]
fn retreat_armies_completes_once_every_pending_retreat_has_a_destination() {
    let (mut s, mut t) = one_task_in("PlanRetreats", "RetreatArmies", "", OWNERSHIP);
    s.phase = Phase::PlanRetreats;
    let (h, o) = (hero(&s), scripted(&s, "OrcHero"));
    s.retreat_entries = vec![h, o];
    begin(&mut t, &mut s);
    assert!(t.active_task(&s).is_some());
    assert_eq!(
        go(&mut t, &mut s, Command::MoveArmy { army: h, dest: B }).unwrap_err(),
        Blocked::ByTutorial
    );
    go(&mut t, &mut s, Command::ChooseRetreat { army: h, dest: B }).unwrap();
    assert!(t.active_task(&s).is_none());
}

#[test]
fn end_tutorial_shows_the_popup_refuses_everything_gated_until_it_is_closed_and_then_ends() {
    let (mut s, mut t) = one_task("EndTutorial", "");
    let events = tick(&mut t, &mut s);
    assert!(events.contains(&TutorialEvent::CompletionPopup {
        title: COMPLETED_TITLE.to_owned(),
        text: COMPLETED_TEXT.to_owned(),
    }));
    assert!(t.popup_open() && !t.finished());
    assert!(!done(&tick(&mut t, &mut s)));
    let army = hero(&s);
    assert_eq!(
        go(&mut t, &mut s, Command::MoveArmy { army, dest: B }).unwrap_err(),
        Blocked::ByTutorial
    );
    assert!(t.popup_closed().is_some());
    assert!(t.finished() && !t.popup_open());
    assert!(t.popup_closed().is_none());
}

#[test]
fn a_task_with_no_handler_never_ends_its_session() {
    let (mut s, mut t) = one_task("EndPhase", "");
    for _ in 0..200 {
        assert!(!done(&tick(&mut t, &mut s)));
    }
}

#[test]
fn disbanding_and_dequeueing_are_refused_even_while_a_task_is_active() {
    let (mut s, mut t) = one_task("TrainUnit", "A");
    begin(&mut t, &mut s);
    let member = s.armies[&garrison(&s)].roster[0].id;
    let disband = Command::DisbandMember {
        army: garrison(&s),
        member,
    };
    assert_eq!(
        go(&mut t, &mut s, disband).unwrap_err(),
        Blocked::ByTutorial
    );
    let dequeue = Command::CancelQueuedArmy {
        region: A,
        plot: 0,
        index: 0,
    };
    assert_eq!(
        go(&mut t, &mut s, dequeue).unwrap_err(),
        Blocked::ByTutorial
    );
    assert_eq!(go(&mut t, &mut s, Command::EndPhase).map(|_| ()), Ok(()));
}

#[test]
fn a_task_with_the_wrong_params_or_unknown_names_makes_the_data_an_error() {
    for (task, params) in [
        ("BuildBarracks", ""),
        ("BuildBarracks", "A B"),
        ("MergeGarrison", "A"),
        ("BuildBarracks", "Nowhere"),
        ("ScriptedArmyMove", "Nobody A"),
        ("ScriptedDelay", "soon"),
    ] {
        let script = turn(1, &phase("Planning", &session(task, params)));
        assert!(
            build(&script, OWNERSHIP).is_err(),
            "{task} {params} should not load"
        );
    }
}

fn mini_tutorial() -> String {
    let say = |audio: &str, task: &str| {
        format!("   Session\n    Audio = {audio}\n    Text = Label:{audio}\n{task}   End\n")
    };
    let task = |name: &str, params: &str| {
        format!("    TaskAfterAudio\n     Task = {name}\n     Params = {params}\n    End\n")
    };
    let t1 = format!(
        "{}{}",
        phase(
            "Planning",
            &format!(
                "{}{}{}",
                session("ScriptedArmyMove", "OrcHero D"),
                say("One", &task("MoveHeroArmyToRegion", "ElfHero B")),
                say("Two", &task("OpenDetailsPanel", "A")),
            )
        ),
        phase("PlanRetreats", &say("Retreat", ""))
    );
    let t2 = phase("Planning", &say("Last", &task("EndTutorial", "")));
    format!("{}{}", turn(1, &t1), turn(2, &t2))
}

#[test]
fn the_scripted_human_reaches_end_tutorial_with_the_same_hashes_every_run() {
    let run = || {
        let (_, mut s, mut t) = build(&mini_tutorial(), OWNERSHIP).unwrap();
        autoplay(&mut t, &mut s, Some(&mut DefenderWins), &mut |_| 3, 10_000)
    };
    let (first, second) = (run(), run());
    assert!(first.finished, "refused {:?}", first.refused);
    assert!(first.refused.is_empty(), "{:?}", first.refused);
    assert!(!first.hashes.is_empty());
    assert_eq!(first.hashes, second.hashes);
    assert_eq!(first.frames, second.frames);
}

#[test]
fn a_phase_that_passes_inside_one_command_still_starts_its_first_session() {
    let (_, mut s, mut t) = build(&mini_tutorial(), OWNERSHIP).unwrap();
    let run = autoplay(&mut t, &mut s, Some(&mut DefenderWins), &mut |_| 3, 10_000);
    assert!(run.tutorial.iter().any(|e| matches!(
        e,
        TutorialEvent::SessionStarted { turn: 1, phase: Phase::PlanRetreats, audio: Some(a), .. } if a == "Retreat"
    )));
}

#[test]
fn a_blocked_command_never_reaches_the_engine() {
    let (_, mut s, mut t) = build(&mini_tutorial(), OWNERSHIP).unwrap();
    let before = s.state_hash();
    let army = hero(&s);
    let r = go(&mut t, &mut s, Command::DisbandAll { army });
    assert_eq!(r.unwrap_err(), Blocked::ByTutorial);
    assert_eq!(s.state_hash(), before);
}
