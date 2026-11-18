use super::*;

const DATA: &str = "\
Upgrade Upgrade_TestRedFaction\n  Type = PLAYER\nEnd\n\
Upgrade Upgrade_TestBlueFaction\n  Type = PLAYER\nEnd\n\
SpecialPower SpecialAbilityTestSeize\n  Enum = SPECIAL_INFANTRY_CAPTURE_BUILDING\n  ReloadTime = 0\nEnd\n\
Object TestBanner\n  Side = Neutral\n  KindOf = IMMOBILE CAPTURABLE STRUCTURE SELECTABLE UNATTACKABLE CAPTUREFLAG\n  \
  Body = HighlanderBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Draw = W3DScriptedModelDraw Tag_Draw\n    \
    AnimationState = START_CAPTURE\n      StateName = Raising\n      LuaEvent = OnStateEnter\n    End\n    \
    AnimationState = CAPTURED\n      StateName = Raised\n      LuaEvent = OnStateEnter\n    End\n    \
    AnimationState = CANCEL_CAPTURE\n      StateName = Lowering\n      LuaEvent = OnStateEnter\n    End\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AILuaEventsList = TestBannerFunctions\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 20\nEnd\n\
Object TestTavern\n  Side = Neutral\n  KindOf = STRUCTURE SELECTABLE IMMOBILE LINKED_TO_FLAG\n  \
  CommandSet = TavernSet\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 5\n  End\n  \
  Behavior = AutoDepositUpdate Tag_Deposit\n    DepositTiming = 500\n    DepositAmount = 0\n    \
    InitialCaptureBonus = 300\n  End\n  \
  Behavior = CommandSetUpgrade Tag_Red\n    TriggeredBy = Upgrade_TestRedFaction\n    \
    CommandSet = TavernRedSet\n  End\n  \
  Behavior = CommandSetUpgrade Tag_Blue\n    TriggeredBy = Upgrade_TestBlueFaction\n    \
    CommandSet = TavernBlueSet\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  GeometryHeight = 30\nEnd\n\
Object Thug\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = ThugSet\n  \
  BuildCost = 200\n  BuildTime = 60\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = SpecialPowerModule Tag_Seize\n    SpecialPowerTemplate = SpecialAbilityTestSeize\n    \
    UpdateModuleStartsAttack = Yes\n  End\n  \
  Behavior = SpecialAbilityUpdate Tag_SeizeUpdate\n    SpecialPowerTemplate = SpecialAbilityTestSeize\n    \
    StartAbilityRange = 15.0\n    UnpackTime = 1\n    PreparationTime = 1000\n    PackTime = 1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet ThugSet\n  1 = Command_TestSeize\nEnd\n\
CommandButton Command_TestSeize\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityTestSeize\n  \
  Options = NEED_TARGET_ENEMY_OBJECT NEED_TARGET_NEUTRAL_OBJECT\nEnd\n\
CommandSet TavernSet\n  1 = Build_Thug\nEnd\n\
CommandSet TavernRedSet\n  1 = Build_Thug\nEnd\n\
CommandSet TavernBlueSet\n  1 = Build_Thug\nEnd\n\
CommandButton Build_Thug\n  Command = UNIT_BUILD\n  Object = Thug\nEnd\n";

const EVENTS: &str = "<SageLuaScriptSection>\n\
                      <Events><InternalEvent Name=\"OnGenericEvent\" /></Events>\n\
                      <EventList Name=\"TestBannerFunctions\">\n\
                      <EventHandler EventName=\"OnGenericEvent\" ScriptFunctionName=\"BannerChanged\"/>\n\
                      </EventList>\n\
                      </SageLuaScriptSection>";

const HANDLERS: &str = "function BannerChanged(self, data)\n\
                        \x20 local who = ObjectCapturingObjectPlayerSide(self)\n\
                        \x20 if who == nil then\n\
                        \x20   who = ObjectPlayerSide(self)\n\
                        \x20 end\n\
                        \x20 ObjectHideSubObjectPermanently(self, \"PART_RED\", true)\n\
                        \x20 ObjectHideSubObjectPermanently(self, \"PART_BLUE\", true)\n\
                        \x20 if who == \"Reds\" then\n\
                        \x20   ObjectHideSubObjectPermanently(self, \"PART_RED\", false)\n\
                        \x20 elseif who == \"Blues\" then\n\
                        \x20   ObjectHideSubObjectPermanently(self, \"PART_BLUE\", false)\n\
                        \x20 else\n\
                        \x20   ObjectHideSubObjectPermanently(self, \"PART_NONE\", false)\n\
                        \x20 end\n\
                        end\n";

const BLUE: PlayerId = OTHER;
const MAP_NEUTRAL: PlayerId = PlayerId::new(2);

struct Scenario {
    simulation: Simulation<RotwkRules>,
    ids: Ids,
    flag: Entity,
    tavern: Entity,
    red_thug: Entity,
    blue_thug: Entity,
}

fn start() -> Scenario {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["TestBanner", "TestTavern", "Thug"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let cast_rules = power_names.cast_rules(&data.special_powers);
    let faction = |name: &str| {
        let mut mask = UpgradeMask::EMPTY;
        mask.set(data.upgrade_bits.bit(name).unwrap());
        mask
    };
    let placed = [
        ("TestBanner", 400.0, 400.0, MAP_NEUTRAL),
        ("TestTavern", 460.0, 400.0, MAP_NEUTRAL),
        ("Thug", 388.0, 400.0, PLAYER),
        ("Thug", 388.0, 408.0, BLUE),
    ];
    let spawns: Vec<Spawn> = placed
        .iter()
        .map(|(name, x, y, owner)| {
            let kind = kinds.kind(name).unwrap();
            let at = Vec2::new(*x, *y);
            let mut spawn = kinds
                .spawn(&data.combat, kind, at, Facing::EAST, *owner)
                .unwrap();
            spawn.producer = kinds.producer(&production, kind, at, Facing::EAST);
            spawn
        })
        .collect();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 10_000)
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_initial_upgrades(vec![
            faction("Upgrade_TestRedFaction"),
            faction("Upgrade_TestBlueFaction"),
        ])
        .with_powers(power_names, power_reload)
        .with_power_cast_rules(cast_rules)
        .with_object_scripts(Some(
            mint_sage_modules::object_scripts::ObjectScripts::load(EVENTS, HANDLERS).unwrap(),
        ))
        .with_attack_ai(
            crate::combat::AttackUrgencyValues::default(),
            Vec::new(),
            vec![
                (PLAYER, String::from("Reds")),
                (BLUE, String::from("Blues")),
            ],
        );
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let of = |name: &str| {
        let kind = kinds.kind(name).unwrap();
        let world = simulation.state();
        world
            .store(ids.kind)
            .unwrap()
            .iter()
            .filter(|(_, k)| **k == kind)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>()
    };
    let (thugs, flag, tavern) = (of("Thug"), of("TestBanner")[0], of("TestTavern")[0]);
    Scenario {
        ids,
        flag,
        tavern,
        red_thug: thugs[0],
        blue_thug: thugs[1],
        simulation,
    }
}

impl Scenario {
    fn order(&mut self, player: PlayerId, on: Entity, command: RotwkCommand) {
        for command in [
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![on],
            },
            command,
        ] {
            self.simulation.submit(player, command).unwrap();
        }
    }

    fn run(&mut self, ticks: u32, hashes: &mut Vec<u64>) {
        for _ in 0..ticks {
            self.simulation.tick().unwrap();
            hashes.push(self.simulation.state_hash());
        }
    }

    fn owner(&self, entity: Entity) -> PlayerId {
        self.simulation
            .state()
            .get(self.ids.owner, entity)
            .unwrap()
            .0
    }

    fn command_set(&self, entity: Entity) -> Option<String> {
        let state = self.simulation.rules().upgrade_state_id().unwrap();
        self.simulation
            .state()
            .get(state, entity)
            .and_then(|state| state.command_set.as_deref().map(str::to_owned))
    }

    fn banner(&self) -> Vec<(String, bool)> {
        let state = self.simulation.rules().upgrade_state_id().unwrap();
        let mut parts: Vec<(String, bool)> = self
            .simulation
            .state()
            .get(state, self.flag)
            .map(|state| {
                state
                    .sub_objects
                    .iter()
                    .map(|write| (write.name.to_string(), write.shown))
                    .collect()
            })
            .unwrap_or_default();
        parts.sort();
        parts
    }

    fn purse(&self, player: PlayerId) -> i64 {
        crate::economy::purse_of(
            self.simulation.state(),
            RotwkRules::economy_ids(self.ids),
            player,
        )
        .amount()
    }
}

const CAPTURE_TICKS: u32 = 60;

type Step = (PlayerId, PlayerId, Option<String>, i64, i64);

fn capture_three_times() -> (Vec<Step>, Vec<u64>) {
    let mut s = start();
    let mut hashes = Vec::new();
    let mut steps = Vec::new();
    let snapshot = |s: &Scenario| {
        (
            s.owner(s.flag),
            s.owner(s.tavern),
            s.command_set(s.tavern),
            s.purse(PLAYER),
            s.purse(BLUE),
        )
    };
    steps.push(snapshot(&s));

    let (red_thug, flag, tavern, blue_thug) = (s.red_thug, s.flag, s.tavern, s.blue_thug);
    s.order(
        PLAYER,
        red_thug,
        RotwkCommand::CaptureObject { target: flag },
    );
    s.run(CAPTURE_TICKS, &mut hashes);
    steps.push(snapshot(&s));

    s.order(PLAYER, tavern, RotwkCommand::QueueUnit { button: 0 });
    s.run(1, &mut hashes);
    steps.push(snapshot(&s));
    s.order(
        BLUE,
        blue_thug,
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: Some(flag),
            at: None,
        },
    );
    s.run(CAPTURE_TICKS, &mut hashes);
    steps.push(snapshot(&s));

    s.order(
        PLAYER,
        red_thug,
        RotwkCommand::CaptureObject { target: flag },
    );
    s.run(CAPTURE_TICKS, &mut hashes);
    steps.push(snapshot(&s));
    (steps, hashes)
}

#[test]
fn a_made_up_flag_and_its_tavern_change_hands_with_refund_one_bonus_and_each_owners_command_set() {
    let (steps, _) = capture_three_times();
    let red = Some(String::from("TavernRedSet"));
    let blue = Some(String::from("TavernBlueSet"));
    assert_eq!(steps[0], (MAP_NEUTRAL, MAP_NEUTRAL, None, 10_000, 10_000));
    assert_eq!(
        steps[1],
        (PLAYER, PLAYER, red.clone(), 10_300, 10_000),
        "red holds both, its bonus paid once, its command set on"
    );
    assert_eq!(steps[2].3, 10_100, "red paid for the thug in training");
    assert_eq!(
        steps[3],
        (BLUE, BLUE, blue, 10_300, 10_000),
        "blue holds both, red refunded, no second bonus"
    );
    assert_eq!(
        steps[4],
        (PLAYER, PLAYER, red, 10_300, 10_000),
        "red again, its own command set fired again"
    );
}

#[test]
fn only_the_flag_is_a_capture_target_never_its_linked_structure() {
    let s = start();
    let rules = s.simulation.rules();
    let world = s.simulation.state();
    assert!(rules.capture_target(world, PLAYER, s.tavern).is_err());
    assert_eq!(rules.capture_target(world, PLAYER, s.flag), Ok(s.flag));
}

#[test]
fn a_capture_replays_to_the_same_hash_sequence() {
    let (_, first) = capture_three_times();
    let (_, second) = capture_three_times();
    assert!(!first.is_empty());
    assert_eq!(first, second);
}

#[test]
fn the_banner_shown_is_the_capturing_sides_before_the_flag_is_theirs() {
    let mut s = start();
    let mut hashes = Vec::new();
    assert!(s.banner().is_empty(), "no handler has run yet");

    let (red_thug, flag, blue_thug) = (s.red_thug, s.flag, s.blue_thug);
    s.order(
        PLAYER,
        red_thug,
        RotwkCommand::CaptureObject { target: flag },
    );
    let mut raised: Option<(PlayerId, Vec<(String, bool)>)> = None;
    for _ in 0..CAPTURE_TICKS {
        s.run(1, &mut hashes);
        if raised.is_none() && !s.banner().is_empty() {
            raised = Some((s.owner(flag), s.banner()));
        }
    }
    let (owner_at_raise, banner_at_raise) = raised.expect("the flag's handler ran");
    assert_eq!(
        owner_at_raise, MAP_NEUTRAL,
        "the raise begins while the flag is still neutral"
    );
    assert_eq!(
        banner_at_raise,
        vec![
            (String::from("PART_BLUE"), false),
            (String::from("PART_RED"), true),
        ],
        "the capturing side's banner, not the flag's own"
    );

    s.order(
        BLUE,
        blue_thug,
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: Some(flag),
            at: None,
        },
    );
    s.run(CAPTURE_TICKS, &mut hashes);
    assert_eq!(
        s.banner(),
        vec![
            (String::from("PART_BLUE"), true),
            (String::from("PART_RED"), false),
        ]
    );
}
