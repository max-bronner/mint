use crate::tree::{Block, Node};

use super::grammar::{
    fields, index_enum, label, walk, Coord2, Coord3, Cx, Defined, Millis, TeamList, TokenList,
    Tokens,
};
use super::players::SpawnArmyDef;

fields! {
    pub struct AddPlayerDef {
        player_template: Option<String> = "PlayerTemplate",
        ai_template: Option<String> = "AITemplate",
        base_region: Option<String> = "BaseRegion",
        mp_slot_color_index: Option<i32> = "MP_SlotColorIndex",
        team_number: Option<i32> = "TeamNumber",
        lw_handicap: Option<i32> = "LWHandicap",
        is_dumb: Option<bool> = "IsDumb",
    }
}

fields! {
    pub struct PlayerDefeatCondition {
        teams: TeamList = "Teams",
        lose_if_capital_lost: Option<bool> = "LoseIfCapitalLost",
        num_controlled_regions_less_or_equal_to: Option<i32> = "NumControlledRegionsLessOrEqualTo",
    }
}

fields! {
    pub struct TeamDefeatCondition {
        teams: TeamList = "Teams",
        num_controlled_regions_less_or_equal_to: Option<i32> = "NumControlledRegionsLessOrEqualTo",
    }
}

fields! {
    pub struct TeamVictoryCondition {
        teams: TeamList = "Teams",
        num_controlled_regions_greater_or_equal_to: Option<i32> = "NumControlledRegionsGreaterOrEqualTo",
        controlled_regions: Tokens = "ControlledRegions",
        controlled_regions_held_for_turns: Option<i32> = "ControlledRegionsHeldForTurns",
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Conditions {
    pub player_defeat: Vec<PlayerDefeatCondition>,
    pub team_defeat: Vec<TeamDefeatCondition>,
    pub team_victory: Vec<TeamVictoryCondition>,
}

impl Conditions {
    fn take(&mut self, block: &Block, cx: &Cx) -> bool {
        match block.keyword.as_str() {
            "PlayerDefeatCondition" => self
                .player_defeat
                .push(PlayerDefeatCondition::read_plain_block(block, cx)),
            "TeamDefeatCondition" => self
                .team_defeat
                .push(TeamDefeatCondition::read_plain_block(block, cx)),
            "TeamVictoryCondition" => self
                .team_victory
                .push(TeamVictoryCondition::read_plain_block(block, cx)),
            _ => return false,
        }
        true
    }
}

fields! {
    pub struct VictoryTypeDef {
        display_objectives: Option<String> = "DisplayObjectives",
        display_game_type: Option<String> = "DisplayGameType",
        display_victorious_text: Option<String> = "DisplayVictoriousText",
        display_defeated_text: Option<String> = "DisplayDefeatedText",
    }
    with {
        pub conditions: Conditions,
    }
}

impl VictoryTypeDef {
    fn read(block: &Block, cx: &Cx) -> Self {
        let ctx = label(block);
        let mut out = Self {
            name: block.name().unwrap_or_default().to_owned(),
            ..Self::default()
        };
        walk(block, cx, &ctx, |node| match node {
            Node::Field(field) => out.read_field(field, cx, &ctx),
            Node::Block(child) => out.conditions.take(child, cx),
            Node::Script(_) => false,
        });
        out
    }
}

fields! {
    pub struct SpawnBuildingsDef {
        buildings: Tokens = "Buildings",
        region: Option<String> = "Region",
    }
}

fields! {
    pub struct SpawnArmiesDef {
        armies: Tokens = "Armies",
        region: Option<String> = "Region",
    }
}

fields! {
    pub struct OwnershipSetDef {
        regions: Tokens = "Regions",
        start_region: Option<String> = "StartRegion",
    }
    with {
        pub spawn_buildings: Vec<SpawnBuildingsDef>,
        pub spawn_armies: Vec<SpawnArmiesDef>,
    }
}

impl OwnershipSetDef {
    fn read(block: &Block, cx: &Cx) -> Self {
        let ctx = label(block);
        let mut out = Self {
            name: block.name().unwrap_or_default().to_owned(),
            ..Self::default()
        };
        walk(block, cx, &ctx, |node| match node {
            Node::Field(field) => out.read_field(field, cx, &ctx),
            Node::Block(child) if child.keyword == "SpawnBuildings" => {
                out.spawn_buildings
                    .push(SpawnBuildingsDef::read_plain_block(child, cx));
                true
            }
            Node::Block(child) if child.keyword == "SpawnArmies" => {
                out.spawn_armies
                    .push(SpawnArmiesDef::read_plain_block(child, cx));
                true
            }
            _ => false,
        });
        out
    }
}

fields! {
    pub struct StartingRestrictionDef {
        regions: Tokens = "Regions",
        factions: Tokens = "Factions",
        teams: TeamList = "Teams",
    }
}

fields! {
    pub struct ScenarioDef {
        display_name: Option<String> = "DisplayName",
        display_description: Option<String> = "DisplayDescription",
        display_fiction: Option<String> = "DisplayFiction",
        display_objectives: Option<String> = "DisplayObjectives",
        display_game_type: Option<String> = "DisplayGameType",
        display_victorious_text: Option<String> = "DisplayVictoriousText",
        display_defeated_text: Option<String> = "DisplayDefeatedText",
        region_campaign: Option<String> = "RegionCampaign",
        disable_regions: Tokens = "DisableRegions",
        disallow_start_in_regions: Tokens = "DisallowStartInRegions",
        default_start_spots: Tokens = "DefaultStartSpots",
        disabled_factions: Tokens = "DisabledFactions",
        max_players: Option<i32> = "MaxPlayers",
        min_players: Option<i32> = "MinPlayers",
        historical_scenario: Option<bool> = "HistoricalScenario",
        use_mp_rules_victory_condition: Option<bool> = "UseMpRulesVictoryCondition",
    }
    with {
        pub conditions: Conditions,
        pub ownership_sets: Vec<OwnershipSetDef>,
        pub starting_restrictions: Vec<StartingRestrictionDef>,
    }
}

impl ScenarioDef {
    fn read(block: &Block, cx: &Cx) -> Self {
        let ctx = label(block);
        let mut out = Self {
            name: block.name().unwrap_or_default().to_owned(),
            ..Self::default()
        };
        walk(block, cx, &ctx, |node| match node {
            Node::Field(field) => out.read_field(field, cx, &ctx),
            Node::Block(child) if child.keyword == "OwnershipSet" => {
                out.ownership_sets.push(OwnershipSetDef::read(child, cx));
                true
            }
            Node::Block(child) if child.keyword == "StartingRestriction" => {
                out.starting_restrictions
                    .push(StartingRestrictionDef::read_plain_block(child, cx));
                true
            }
            Node::Block(child) => out.conditions.take(child, cx),
            Node::Script(_) => false,
        });
        out
    }
}

fields! {
    pub struct EnableRegionNugget {
        region: Option<String> = "Region",
    }
}

fields! {
    pub struct ForceBattleNugget {
        region: Option<String> = "Region",
        event_name: Option<String> = "EventName",
        use_army: Option<String> = "UseArmy",
        movie: Option<String> = "Movie",
        position: Option<Coord2> = "Position",
        army_attack_direction: Option<Coord2> = "ArmyAttackDirection",
        play_next_act_after_movie: Option<bool> = "PlayNextActAfterMovie",
        remove_event: Option<bool> = "RemoveEvent",
    }
}

fields! {
    pub struct MoveArmyNugget {
        army_scripting_name: Option<String> = "ArmyScriptingName",
        target_region_name: Option<String> = "TargetRegionName",
    }
}

fields! {
    pub struct SpawnBuildingNugget {
        player: Option<String> = "Player",
        building: Option<String> = "Building",
        region: Option<String> = "Region",
        instant_create: Option<bool> = "InstantCreate",
    }
}

fields! {
    pub struct MoveCameraNugget {
        delay_from_act_start: Option<Millis> = "DelayFromActStart",
        scroll_time: Option<Millis> = "ScrollTime",
        position: Option<Coord3> = "Position",
        view_angle: Option<f32> = "ViewAngle",
        summary_event: Option<bool> = "SummaryEvent",
    }
}

fields! {
    pub struct ControlPointDef {
        position: Option<Coord3> = "Position",
        angle: Option<f32> = "Angle",
        ease_in: Option<f32> = "EaseIn",
        ease_out: Option<f32> = "EaseOut",
        time: Option<Millis> = "Time",
    }
}

fields! {
    pub struct SplineCameraNugget {
        delay_from_act_start: Option<Millis> = "DelayFromActStart",
        summary_event: Option<bool> = "SummaryEvent",
    }
    with {
        pub control_points: Vec<ControlPointDef>,
    }
}

fields! {
    pub struct WorldTextNugget {
        delay_from_act_start: Option<Millis> = "DelayFromActStart",
        duration: Option<Millis> = "Duration",
        string_tag: Option<String> = "StringTag",
        summary_event: Option<bool> = "SummaryEvent",
    }
}

fields! {
    pub struct AudioEventNugget {
        delay_from_act_start: Option<Millis> = "DelayFromActStart",
        sound: Option<String> = "Sound",
        summary_event: Option<bool> = "SummaryEvent",
    }
}

fields! {
    pub struct UpdateAnimObjectNugget {
        anim_object_name: Option<String> = "AnimObjectName",
        world_pos: Option<Coord3> = "WorldPos",
    }
}

fields! {
    pub struct EyeTowerPointsNugget {
        look_point: Vec<Coord2> = "LookPoint",
    }
}

fields! {
    pub struct SetPlayerControlOfArmyNugget {
        army_scripting_name: Option<String> = "ArmyScriptingName",
        is_controllable_by_owner: Option<bool> = "IsControllableByOwner",
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActNugget {
    EnableRegion(EnableRegionNugget),
    ForceBattle(ForceBattleNugget),
    SpawnArmy(Box<SpawnArmyDef>),
    MoveArmy(MoveArmyNugget),
    SpawnBuilding(SpawnBuildingNugget),
    MoveCamera(MoveCameraNugget),
    SplineCamera(SplineCameraNugget),
    WorldText(WorldTextNugget),
    AudioEvent(AudioEventNugget),
    UpdateAnimObject(UpdateAnimObjectNugget),
    EyeTowerPoints(EyeTowerPointsNugget),
    SetPlayerControlOfArmy(SetPlayerControlOfArmyNugget),
}

fields! {
    pub struct ActDef {
        call_act_subroutine: TokenList = "CallActSubroutine",
        jump_to_act: Option<String> = "JumpToAct",
        end_act: Option<bool> = "EndAct",
    }
    with {
        pub nuggets: Vec<ActNugget>,
    }
}

impl ActDef {
    fn read(block: &Block, cx: &Cx) -> Self {
        let ctx = label(block);
        let mut out = Self {
            name: block.name().unwrap_or_default().to_owned(),
            ..Self::default()
        };
        walk(block, cx, &ctx, |node| match node {
            Node::Field(field) => out.read_field(field, cx, &ctx),
            Node::Block(child) => {
                let nugget = match child.keyword.as_str() {
                    "EnableRegion" => {
                        ActNugget::EnableRegion(EnableRegionNugget::read_plain_block(child, cx))
                    }
                    "ForceBattle" => {
                        ActNugget::ForceBattle(ForceBattleNugget::read_plain_block(child, cx))
                    }
                    "SpawnArmy" => ActNugget::SpawnArmy(Box::new(SpawnArmyDef::read(child, cx))),
                    "MoveArmy" => ActNugget::MoveArmy(MoveArmyNugget::read_plain_block(child, cx)),
                    "SpawnBuilding" => {
                        ActNugget::SpawnBuilding(SpawnBuildingNugget::read_plain_block(child, cx))
                    }
                    "MoveCamera" => {
                        ActNugget::MoveCamera(MoveCameraNugget::read_plain_block(child, cx))
                    }
                    "SplineCamera" => ActNugget::SplineCamera(read_spline_camera(child, cx)),
                    "WorldText" => {
                        ActNugget::WorldText(WorldTextNugget::read_plain_block(child, cx))
                    }
                    "AudioEvent" => {
                        ActNugget::AudioEvent(AudioEventNugget::read_plain_block(child, cx))
                    }
                    "UpdateAnimObject" => ActNugget::UpdateAnimObject(
                        UpdateAnimObjectNugget::read_plain_block(child, cx),
                    ),
                    "EyeTowerPoints" => {
                        ActNugget::EyeTowerPoints(EyeTowerPointsNugget::read_plain_block(child, cx))
                    }
                    "SetPlayerControlOfArmy" => ActNugget::SetPlayerControlOfArmy(
                        SetPlayerControlOfArmyNugget::read_plain_block(child, cx),
                    ),
                    _ => return false,
                };
                out.nuggets.push(nugget);
                true
            }
            Node::Script(_) => false,
        });
        out
    }
}

fn read_spline_camera(block: &Block, cx: &Cx) -> SplineCameraNugget {
    let ctx = label(block);
    let mut out = SplineCameraNugget {
        name: block.name().unwrap_or_default().to_owned(),
        ..SplineCameraNugget::default()
    };
    walk(block, cx, &ctx, |node| match node {
        Node::Field(field) => out.read_field(field, cx, &ctx),
        Node::Block(child) if child.keyword == "ControlPoint" => {
            out.control_points
                .push(ControlPointDef::read_plain_block(child, cx));
            true
        }
        _ => false,
    });
    out
}

index_enum! {
    pub enum TutorialTask ("tutorial task") {
        EndPhase = "EndPhase",
        ZoomScroll = "ZoomScroll",
        BuildBarracks = "BuildBarracks",
        TrainUnit = "TrainUnit",
        MergeGarrison = "MergeGarrison",
        MoveHeroArmyToRegion = "MoveHeroArmyToRegion",
        OpenDetailsPanel = "OpenDetailsPanel",
        SelectBattle = "SelectBattle",
        EnableAutoResolveButtonAndWaitForCompletion = "EnableAutoResolveButtonAndWaitForCompletion",
        RetreatArmies = "RetreatArmies",
        ScriptedArmyMove = "ScriptedArmyMove",
        ScriptedDelay = "ScriptedDelay",
        EndTutorial = "EndTutorial",
        Invalid = "INVALID",
    }
}

fields! {
    pub struct TaskAfterAudioDef {
        task: Option<TutorialTask> = "Task",
        params: Tokens = "Params",
        text: Option<String> = "Text",
    }
}

fields! {
    pub struct SessionDef {
        audio: Option<String> = "Audio",
        text: Option<String> = "Text",
        clear_previous_text: Option<bool> = "ClearPreviousText",
    }
    with {
        pub tasks: Vec<TaskAfterAudioDef>,
    }
}

fields! {
    pub struct PhaseDef {}
    with {
        pub sessions: Vec<SessionDef>,
    }
}

fields! {
    pub struct TurnDef {}
    with {
        pub phases: Vec<PhaseDef>,
    }
}

fields! {
    pub struct TutorialDef {}
    with {
        pub turns: Vec<TurnDef>,
    }
}

fn read_children<T>(
    block: &Block,
    cx: &Cx,
    child_keyword: &str,
    mut child: impl FnMut(&Block, &Cx) -> T,
) -> Vec<T> {
    let ctx = label(block);
    let mut out = Vec::new();
    walk(block, cx, &ctx, |node| match node {
        Node::Block(b) if b.keyword == child_keyword => {
            out.push(child(b, cx));
            true
        }
        _ => false,
    });
    out
}

fn read_tutorial(block: &Block, cx: &Cx) -> TutorialDef {
    let turns = read_children(block, cx, "Turn", |turn, cx| {
        let phases = read_children(turn, cx, "Phase", |phase, cx| {
            let sessions = read_children(phase, cx, "Session", read_session);
            PhaseDef {
                name: phase.name().unwrap_or_default().to_owned(),
                sessions,
            }
        });
        TurnDef {
            name: turn.name().unwrap_or_default().to_owned(),
            phases,
        }
    });
    TutorialDef {
        name: block.name().unwrap_or_default().to_owned(),
        turns,
    }
}

fn read_session(block: &Block, cx: &Cx) -> SessionDef {
    let ctx = label(block);
    let mut out = SessionDef {
        name: block.name().unwrap_or_default().to_owned(),
        ..SessionDef::default()
    };
    walk(block, cx, &ctx, |node| match node {
        Node::Field(field) => out.read_field(field, cx, &ctx),
        Node::Block(child) if child.keyword == "TaskAfterAudio" => {
            out.tasks
                .push(TaskAfterAudioDef::read_plain_block(child, cx));
            true
        }
        _ => false,
    });
    out
}

fields! {
    pub struct LivingWorldCampaignDef {
        local_player: Option<String> = "LocalPlayer",
        script_holder: Option<String> = "ScriptHolder",
        seconds_per_reinforcement: Option<i32> = "SecondsPerReinforcement",
        starting_cash_rts: Option<u32> = "StartingCashRTS",
        starting_cash_rts_with_fort: Option<u32> = "StartingCashRTSWithFort",
        initial_revival_cost_multiplier: Option<f32> = "InitialRevivalCostMultiplier",
        initial_revival_time_multiplier: Option<f32> = "InitialRevivalTimeMultiplier",
        is_scripted_campaign: Option<bool> = "IsScriptedCampaign",
        is_evil_campaign: Option<bool> = "IsEvilCampaign",
        force_advance_turn_phase: Option<bool> = "ForceAdvanceTurnPhase",
    }
    with {
        pub add_players: Vec<AddPlayerDef>,
        pub acts: Vec<ActDef>,
        pub victory_types: Vec<VictoryTypeDef>,
        pub scenarios: Vec<ScenarioDef>,
        pub tutorials: Vec<TutorialDef>,
    }
}

impl Defined for LivingWorldCampaignDef {
    fn def_name(&self) -> &str {
        &self.name
    }
}

impl LivingWorldCampaignDef {
    pub(crate) fn read(block: &Block, cx: &Cx) -> Self {
        let ctx = label(block);
        let mut out = Self {
            name: block.name().unwrap_or_default().to_owned(),
            ..Self::default()
        };
        if block.name().is_none() {
            cx.missing_name(block);
        }
        walk(block, cx, &ctx, |node| match node {
            Node::Field(field) => out.read_field(field, cx, &ctx),
            Node::Block(child) => {
                match child.keyword.as_str() {
                    "AddPlayer" => out
                        .add_players
                        .push(AddPlayerDef::read_plain_block(child, cx)),
                    "Act" => out.acts.push(ActDef::read(child, cx)),
                    "LivingWorldVictoryType" => {
                        out.victory_types.push(VictoryTypeDef::read(child, cx));
                    }
                    "Scenario" => out.scenarios.push(ScenarioDef::read(child, cx)),
                    "Tutorial" => out.tutorials.push(read_tutorial(child, cx)),
                    _ => return false,
                }
                true
            }
            Node::Script(_) => false,
        });
        out
    }

    pub fn scripting_names(&self) -> impl Iterator<Item = &str> {
        self.acts
            .iter()
            .flat_map(|act| act.nuggets.iter())
            .filter_map(|nugget| match nugget {
                ActNugget::SpawnArmy(army) => army.scripting_name.as_deref(),
                _ => None,
            })
    }
}
