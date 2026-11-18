use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub keyword: String,
    pub selector: Option<String>,
    context: String,
    slot: Option<String>,
}

impl Scope {
    #[must_use]
    pub fn new(keyword: impl Into<String>) -> Self {
        let keyword = keyword.into();
        let context = keyword.to_ascii_lowercase();
        Self {
            keyword,
            selector: None,
            context,
            slot: None,
        }
    }

    #[must_use]
    pub fn slot(keyword: impl Into<String>, selector: impl Into<String>) -> Self {
        let keyword = keyword.into();
        let selector = selector.into();
        let context = keyword.to_ascii_lowercase();
        let mut slot = context.clone();
        slot.push(':');
        slot.push_str(&selector.to_ascii_lowercase());
        Self {
            keyword,
            selector: Some(selector),
            context,
            slot: Some(slot),
        }
    }
}

pub trait BlockOpener {
    fn opens_block(&self, context: &[Scope], keyword: &str, args: &[String]) -> Option<bool>;
}

pub const OBJECT_MODEL_RULES: &[(&str, &str, bool)] = &[
    ("", "Object", true),
    ("", "ChildObject", true),
    ("", "ObjectReskin", true),
    ("", "Road", true),
    ("", "Terrain", true),
    ("object", "Draw", true),
    ("object", "Behavior", true),
    ("object", "Body", true),
    ("object", "ClientUpdate", true),
    ("object", "ClientBehavior", true),
    ("object", "ArmorSet", true),
    ("object", "WeaponSet", true),
    ("object", "Prerequisites", true),
    ("object", "UnitSpecificSounds", true),
    ("object", "UnitSpecificFX", true),
    ("object", "AddModule", true),
    ("object", "ReplaceModule", true),
    ("object", "InheritableModule", true),
    ("object", "LocomotorSet", true),
    ("object", "ThreatBreakdown", true),
    ("object", "AutoResolveWeapon", true),
    ("object", "AutoResolveArmor", true),
    ("object", "Flammability", true),
    ("behavior", "ObjectStatusOfContained", false),
    ("draw", "DefaultModelConditionState", true),
    ("draw", "ModelConditionState", true),
    ("draw", "AnimationState", true),
    ("draw", "IdleAnimationState", true),
    ("draw", "TransitionState", true),
    ("draw", "ModelName", false),
    ("draw", "LodOptions", true),
    ("defaultmodelconditionstate", "Model", false),
    ("defaultmodelconditionstate", "Animation", true),
    ("animationstate", "Animation", true),
];

pub const PARTICLE_RULES: &[(&str, &str, bool)] = &[
    ("", "FXParticleSystem", true),
    ("", "ParticleSystem", true),
    ("", "FXList", true),
    ("fxparticlesystem", "System", true),
    ("fxparticlesystem", "Color", true),
    ("fxparticlesystem", "Alpha", true),
    ("fxparticlesystem", "Update", true),
    ("fxparticlesystem", "Physics", true),
    ("fxparticlesystem", "EmissionVelocity", true),
    ("fxparticlesystem", "EmissionVolume", true),
    ("fxparticlesystem", "Draw", true),
    ("fxparticlesystem", "Wind", true),
    ("fxparticlesystem", "Event", true),
    ("fxparticlesystem", "Emitter", true),
    ("fxlist", "ParticleSystem", true),
    ("fxlist", "Sound", true),
    ("fxlist", "DynamicDecal", true),
    ("fxlist", "EvaEvent", true),
    ("fxlist", "TerrainScorch", true),
    ("fxlist", "CameraShakerVolume", true),
    ("fxlist", "FXListAtBonePos", true),
    ("fxlist", "ViewShake", true),
    ("fxlist", "BuffNugget", true),
    ("fxlist", "TintDrawable", true),
    ("fxlist", "LightPulse", true),
    ("fxlist", "AttachedModel", true),
    ("fxlist", "Tracer", true),
    ("fxlist", "RayEffect", true),
    ("fxlist", "Laser", true),
    ("fxlist", "FXParticleSysBoneNugget", true),
    ("fxlist", "ParticleSysBone", false),
    ("fxlist", "PlayEvenIfShrouded", false),
    ("fxlist", "CullingInfo", false),
];

pub const PARTICLE_CLOSED: &[&str] = &[
    "fxparticlesystem",
    "particlesystem",
    "fxlist",
    "system",
    "color",
    "alpha",
    "update",
    "physics",
    "emissionvelocity",
    "emissionvolume",
    "draw",
    "wind",
    "event",
    "emitter",
    "sound",
    "dynamicdecal",
    "evaevent",
    "terrainscorch",
    "camerashakervolume",
    "fxlistatbonepos",
    "viewshake",
    "buffnugget",
    "tintdrawable",
    "lightpulse",
    "attachedmodel",
    "tracer",
    "rayeffect",
    "laser",
    "fxparticlesysbonenugget",
];

pub const OBJECT_MODEL_ALIASES: &[(&str, &str)] = &[
    ("childobject", "object"),
    ("objectreskin", "object"),
    ("addmodule", "object"),
    ("replacemodule", "object"),
    ("inheritablemodule", "object"),
    ("idleanimationstate", "animationstate"),
    ("transitionstate", "animationstate"),
    ("modelconditionstate", "defaultmodelconditionstate"),
];

pub const OBJECT_MODEL_CLOSED: &[&str] = &[
    "object",
    "draw",
    "defaultmodelconditionstate",
    "animationstate",
    "animation",
];

pub const PRIORITY_BLOCK_RULES: &[(&str, &str, bool)] = &[
    ("", "Weapon", true),
    ("", "Armor", true),
    ("", "Locomotor", true),
    ("", "ObjectCreationList", true),
    ("", "FXList", true),
    ("", "DamageFX", true),
    ("", "CommandButton", true),
    ("", "CommandSet", true),
    ("", "Upgrade", true),
    ("", "ButtonSet", true),
    ("weapon", "DamageNugget", true),
    ("weapon", "ProjectileNugget", true),
    ("weapon", "MetaImpactNugget", true),
    ("weapon", "FireLogicNugget", true),
    ("weapon", "HordeAttackNugget", true),
    ("weapon", "DOTNugget", true),
    ("weapon", "AttributeModifierNugget", true),
    ("weapon", "WeaponOCLNugget", true),
    ("weapon", "ParalyzeNugget", true),
    ("weapon", "SlaveAttackNugget", true),
    ("weapon", "LuaEventNugget", true),
    ("weapon", "GrabNugget", true),
    ("weapon", "SpecialModelConditionNugget", true),
    ("weapon", "EmotionWeaponNugget", true),
    ("weapon", "OpenGateNugget", true),
    ("weapon", "StealMoneyNugget", true),
    ("weapon", "SpawnAndFadeNugget", true),
    ("weapon", "DamageContainedNugget", true),
    ("weapon", "DamageFieldNugget", true),
    ("objectcreationlist", "CreateObject", true),
    ("objectcreationlist", "CreateDebris", true),
];

pub const PRIORITY_BLOCK_CLOSED: &[&str] = &[
    "weapon",
    "armor",
    "locomotor",
    "objectcreationlist",
    "fxlist",
    "damagefx",
    "commandbutton",
    "commandset",
    "upgrade",
    "buttonset",
];

pub const AUDIO_RULES: &[(&str, &str, bool)] = &[
    ("", "AudioEvent", true),
    ("", "DialogEvent", true),
    ("", "MusicTrack", true),
    ("", "AmbientStream", true),
    ("", "StreamedSound", true),
    ("", "AudioSettings", true),
    ("", "MiscAudio", true),
    ("", "Multisound", true),
    ("", "CrowdResponse", true),
    ("crowdresponse", "Threshold", true),
    ("threshold", "UnitSpecificSounds", true),
];

pub const AUDIO_CLOSED: &[&str] = &[
    "audioevent",
    "dialogevent",
    "musictrack",
    "ambientstream",
    "streamedsound",
    "audiosettings",
    "miscaudio",
    "multisound",
    "threshold",
];

pub const HORDE_CONTAIN_RULES: &[(&str, &str, bool)] = &[
    ("behavior:hordecontain", "MeleeBehavior", true),
    ("behavior:horsehordecontain", "MeleeBehavior", true),
    ("behavior:aodhordecontain", "MeleeBehavior", true),
];

pub const HORDE_CONTAIN_CLOSED: &[&str] = &[
    "behavior:hordecontain",
    "behavior:horsehordecontain",
    "behavior:aodhordecontain",
];

pub const CORPUS_FILE_SCOPE_RULES: &[(&str, &str, bool)] = &[
    ("", "MappedImage", true),
    ("", "ExperienceLevel", true),
    ("", "Multisound", true),
    ("", "ModifierList", true),
    ("", "HouseColor", true),
    ("", "SpecialPower", true),
    ("", "NewEvaEvent", true),
    ("", "Video", true),
    ("", "LivingWorldPlayerArmy", true),
    ("", "MeshNameMatches", true),
    ("", "Rank", true),
    ("", "AutoResolveWeapon", true),
    ("", "DebugCommandMap", true),
    ("", "AutoResolveArmor", true),
    ("", "LoadSubsystem", true),
    ("", "AIBase", true),
    ("", "AutoResolveBody", true),
    ("", "LargeGroupAudioMap", true),
    ("", "ControlBarResizer", true),
    ("", "Science", true),
    ("", "WindowTransition", true),
    ("", "MouseCursor", true),
    ("", "LivingWorldAnimObject", true),
    ("", "LivingWorldArmyIcon", true),
    ("", "LivingWorldBuilding", true),
    ("", "LivingWorldBuildingIcon", true),
    ("", "AutoResolveHandicapLevel", true),
    ("", "LivingWorldSound", true),
    ("", "PredefinedEvaEvent", true),
    ("", "EmotionNugget", true),
    ("", "PlayerAIType", true),
    ("", "FactionVictoryData", true),
    ("", "AutoResolveLeadership", true),
    ("", "CrowdResponse", true),
    ("", "PlayerTemplate", true),
    ("", "WaterTextureList", true),
    ("", "SkyboxTextureSet", true),
    ("", "MultiplayerColor", true),
    ("", "AutoResolveCombatChain", true),
    ("", "CrateData", true),
    ("", "AIDozerAssignment", true),
    ("", "ArmyDefinition", true),
    ("", "LivingWorldPlayerTemplate", true),
    ("", "BannerType", true),
    ("", "ControlBarScheme", true),
    ("", "LivingWorldBuildPlotIcon", true),
    ("", "LivingWorldObject", true),
    ("", "StanceTemplate", true),
    ("", "WebpageURL", true),
    ("", "DynamicGameLOD", true),
    ("", "LinearCampaign", true),
    ("", "StaticGameLOD", true),
    ("", "WeatherData", true),
    ("", "WaterSet", true),
    ("", "ExperienceScalarTable", true),
    ("", "FontDefaultSettings", true),
    ("", "GameData", true),
    ("", "AIData", true),
    ("", "AudioLOD", true),
    ("", "AutoResolveReinforcementSchedule", true),
    ("", "Bridge", true),
    ("", "CommandMap", true),
    ("", "MultiplayerSettings", true),
    ("", "OptionGroup", true),
    ("", "ScoredKillEvaAnnouncer", true),
    ("", "VictorySystemData", true),
    ("", "AnimationSoundClientBehaviorGlobalSetting", true),
    ("", "AptButtonTooltipMap", true),
    ("", "ArmySummaryDescription", true),
    ("", "AwardSystem", true),
    ("", "CloudEffect", true),
    ("", "DrawGroupInfo", true),
    ("", "Fire", true),
    ("", "FireEffect", true),
    ("", "FireLogicSystem", true),
    ("", "FontSubstitution", true),
    ("", "FormationAssistant", true),
    ("", "GlowEffect", true),
    ("", "InGameNotificationBox", true),
    ("", "LargeGroupAudioUnusedKnownKeys", true),
    ("", "LightPointLevel", true),
    ("", "LivingWorldAITemplate", true),
    ("", "LivingWorldAutoResolveResourceBonus", true),
    ("", "LivingWorldAutoResolveSciencePurchasePointBonus", true),
    ("", "LivingWorldMapInfo", true),
    ("", "LivingWorldRegionEffects", true),
    ("", "MiscEvaData", true),
    ("", "Mouse", true),
    ("", "OnlineChatColors", true),
    ("", "Pathfinder", true),
    ("", "RingEffect", true),
    ("", "ShadowMap", true),
    ("", "ShellMenuScheme", true),
    ("", "SkirmishAIData", true),
    ("", "StrategicHUD", true),
    ("", "StreamedSound", true),
    ("", "WaterTransparency", true),
    ("", "Weather", true),
    ("", "MissionObjectiveList", true),
    ("", "CreateAHeroSystem", true),
    ("", "LivingWorldCampaign", true),
    ("", "LivingWorldRegionCampaign", true),
    ("", "Animation", true),
    ("", "InGameUI", true),
    ("", "Text", false),
    ("", "Style", false),
    ("", "Blank", false),
    ("", "ScrollRate", false),
    ("", "ScrollRateEveryFrames", false),
    ("", "ScrollDown", false),
    ("", "TitleColor", false),
    ("", "NormalColor", false),
    ("", "MinorTitleColor", false),
    ("", "LODPreset", false),
    ("", "BenchProfile", false),
    ("", "ReallyLowMHz", false),
    ("", "AudioLowMHz", false),
    ("", "EvaEventForwardReference", false),
    ("", "RegionCampain", true),
    ("", "HeaderTemplate", true),
    ("", "Language", true),
    ("", "ScriptAction", true),
    ("", "ScriptCondition", true),
];

pub const CORPUS_INTERIOR_RULES: &[(&str, &str, bool)] = &[
    ("behavior", "MeleeBehavior", true),
    ("behavior", "FireWeaponNugget", true),
    ("behavior", "InvisibilityNugget", true),
    ("behavior", "FloodMember", true),
    ("behavior", "AddEmotion", false),
    ("behavior", "ProductionModifier", true),
    ("behavior", "Turret", true),
    ("behavior", "GridDecalTemplate", true),
    ("behavior", "ReplaceObject", true),
    ("behavior:parttheheavensupdate", "Radius", true),
    ("behavior:parttheheavensupdate", "Opacity", true),
    ("behavior:parttheheavensupdate", "Angle", true),
    ("clientbehavior", "SoundState", true),
    ("clientbehavior", "SoundUpgrade", true),
    ("newevaevent", "SideSound", true),
    ("predefinedevaevent", "SideSound", true),
    ("largegroupaudiomap", "Sound", true),
    ("livingworldarmyicon", "Object", true),
    ("livingworldbuildingicon", "Object", true),
    ("livingworldbuildploticon", "Object", true),
    ("livingworldbuilding", "BuildingNugget", true),
    ("buildingnugget", "ArmyToSpawn", true),
    ("crowdresponse", "Threshold", true),
    ("threshold", "UnitSpecificSounds", true),
    ("autoresolveleadership", "BonusForLevel", true),
    ("armydefinition", "ArmyMemberDefinition", true),
    ("armydefinition", "AIEconomyAssigment", true),
    ("armydefinition", "AIWallNodeAssignment", true),
    ("controlbarscheme", "ImagePart", true),
    ("stancetemplate", "Stance", true),
    ("stance", "MeleeBehavior", true),
    ("linearcampaign", "Mission", true),
    ("aidata", "AttackPriority", true),
    ("aidata", "SideInfo", true),
    ("aidata", "SkirmishBuildList", true),
    ("skirmishbuildlist", "Structure", true),
    ("firelogicsystem", "TerrainCellType", true),
    ("ingamenotificationbox", "NotificationType", true),
    ("armysummarydescription", "UnitCategory", true),
    ("awardsystem", "ObjectAward", true),
    ("awardsystem", "ThingStat", true),
    ("awardsystem", "ThingTemplateNames", true),
    ("objectaward", "Trigger", true),
    ("experiencelevel", "SelectionDecal", true),
    ("object", "FormationPreviewDecal", true),
    ("object", "FormationPreviewItemDecal", true),
    ("ingameui", "RadiusCursorTemplate", true),
    ("livingworldcampaign", "Scenario", true),
    ("livingworldcampaign", "Act", true),
    ("act", "SpawnArmies", true),
    ("scenario", "SpawnArmies", true),
    ("regioncampain", "Region", true),
    ("livingworldregioncampaign", "Region", true),
    ("region", "Connection", true),
    ("connection", "Region", false),
    ("createaherosystem", "CreateAHeroClass", true),
    ("createaherosystem", "CreateAHeroBling", true),
    ("createaheroclass", "SubClass", true),
    ("draw", "DecalTemplate", true),
    ("windowtransition", "Window", true),
    ("window", "Transition", true),
    ("formationassistant", "FormationSelection", true),
    ("formationassistant", "UnitDefinition", true),
    ("formationassistant", "FormationTemplate", true),
    ("formationtemplate", "Rows", true),
    ("soundupgrade", "UnitSpecificSounds", true),
    ("soundstate", "UnitSpecificSounds", true),
];

pub const LIVING_WORLD_RULES: &[(&str, &str, bool)] = &[
    ("livingworldcampaign", "AddPlayer", true),
    ("livingworldcampaign", "LivingWorldVictoryType", true),
    ("livingworldcampaign", "SpawnArmies", true),
    ("livingworldcampaign", "SpawnBuildings", true),
    ("livingworldcampaign", "Tutorial", true),
    ("livingworldvictorytype", "PlayerDefeatCondition", true),
    ("livingworldvictorytype", "TeamDefeatCondition", true),
    ("livingworldvictorytype", "TeamVictoryCondition", true),
    ("scenario", "OwnershipSet", true),
    ("scenario", "PlayerDefeatCondition", true),
    ("scenario", "StartingRestriction", true),
    ("scenario", "TeamDefeatCondition", true),
    ("scenario", "TeamVictoryCondition", true),
    ("ownershipset", "SpawnArmies", true),
    ("ownershipset", "SpawnBuildings", true),
    ("act", "EyeTowerPoints", true),
    ("act", "SpawnArmy", true),
    ("tutorial", "Turn", true),
    ("turn", "Phase", true),
    ("phase", "Session", true),
    ("session", "TaskAfterAudio", true),
    ("livingworldplayerarmy", "ArmyEntry", true),
    ("livingworldregioneffects", "BordersEffect", true),
    ("livingworldregioneffects", "FilledOwnershipEffect", true),
    ("livingworldregioneffects", "HomeRegionHighlight", true),
    ("livingworldregioneffects", "MouseoverEffectFlareup", true),
    ("livingworldregioneffects", "RegionSelectionEffect", true),
    ("livingworldregioneffects", "UnifiedEffect", true),
    ("borderseffect", "ColorIntensityControlPoint", true),
    ("filledownershipeffect", "ColorIntensityControlPoint", true),
    ("homeregionhighlight", "ColorIntensityControlPoint", true),
    ("mouseovereffectflareup", "ColorIntensityControlPoint", true),
    ("regionselectioneffect", "ColorIntensityControlPoint", true),
    ("unifiedeffect", "ColorIntensityControlPoint", true),
    ("region", "ConcurrentRegionBonus", true),
    ("region", "RestrictBuildings", true),
    ("connection", "RestrictBuildings", true),
    ("livingworldregioncampaign", "ConcurrentRegionBonus", true),
    ("livingworldautoresolveresourcebonus", "Bonus", true),
    (
        "livingworldautoresolvesciencepurchasepointbonus",
        "Bonus",
        true,
    ),
    ("skirmishaidata", "BrutalDifficultyCheats", true),
    ("skirmishaidata", "CombatChainDefinition", true),
    ("skirmishaidata", "DifficultyTuning", true),
    ("createaherosystem", "CreateAHeroBlingBinder", true),
    ("subclass", "Attribute", true),
    ("subclass", "ViewInfo", true),
    ("strategichud", "ArmyDetailsPanel", true),
    ("strategichud", "BattleResolver", true),
    ("strategichud", "BuildQueueDetailsPanel", true),
    ("strategichud", "CancelArmyMemberMoveButton", true),
    ("strategichud", "CancelArmyMoveButton", true),
    ("strategichud", "CancelBuildingConstructionButton", true),
    ("strategichud", "Checklist", true),
    ("strategichud", "DestroyBuildingButton", true),
    ("strategichud", "DisbandArmyMemberButton", true),
    ("strategichud", "DynamicAutoResolveDialog", true),
    ("strategichud", "ObjectivesButton", true),
    ("strategichud", "OptionsButton", true),
    ("strategichud", "RegionDetailsPanelStructuresPage", true),
    ("strategichud", "RegionDisplay", true),
    ("strategichud", "StatsDisplay", true),
    ("strategichud", "ToggleSelectionDetailsButton", true),
    ("strategichud", "TypeImages", true),
    ("strategichud", "UpgradeUnitButton", true),
];

pub const CORPUS_CLOSED: &[&str] = &[
    "behavior",
    "clientbehavior",
    "body",
    "armorset",
    "weaponset",
    "locomotorset",
    "threatbreakdown",
    "flammability",
    "clientupdate",
    "lodoptions",
    "damagenugget",
    "metaimpactnugget",
    "firelogicnugget",
    "hordeattacknugget",
    "dotnugget",
    "projectilenugget",
    "attributemodifiernugget",
    "weaponoclnugget",
    "paralyzenugget",
    "slaveattacknugget",
    "luaeventnugget",
    "grabnugget",
    "specialmodelconditionnugget",
    "emotionweaponnugget",
    "opengatenugget",
    "stealmoneynugget",
    "spawnandfadenugget",
    "damagecontainednugget",
    "damagefieldnugget",
    "createobject",
    "createdebris",
    "mappedimage",
    "spawnarmy",
    "terrain",
    "multisound",
    "modifierlist",
    "housecolor",
    "specialpower",
    "video",
    "meshnamematches",
    "rank",
    "autoresolveweapon",
    "debugcommandmap",
    "autoresolvearmor",
    "loadsubsystem",
    "aibase",
    "autoresolvebody",
    "controlbarresizer",
    "road",
    "science",
    "mousecursor",
    "livingworldanimobject",
    "autoresolvehandicaplevel",
    "livingworldsound",
    "emotionnugget",
    "playeraitype",
    "factionvictorydata",
    "playertemplate",
    "watertexturelist",
    "skyboxtextureset",
    "multiplayercolor",
    "cratedata",
    "aidozerassignment",
    "livingworldplayertemplate",
    "bannertype",
    "livingworldobject",
    "webpageurl",
    "dynamicgamelod",
    "staticgamelod",
    "weatherdata",
    "waterset",
    "experiencescalartable",
    "fontdefaultsettings",
    "gamedata",
    "audiolod",
    "autoresolvereinforcementschedule",
    "bridge",
    "commandmap",
    "multiplayersettings",
    "optiongroup",
    "scoredkillevaannouncer",
    "victorysystemdata",
    "sidesound",
    "armymemberdefinition",
    "armytospawn",
    "meleebehavior",
    "unitcategory",
    "thingstat",
    "trigger",
    "fireweaponnugget",
    "invisibilitynugget",
    "floodmember",
    "addemotion",
    "mission",
    "productionmodifier",
    "bonusforlevel",
    "notificationtype",
    "attackpriority",
    "aieconomyassigment",
    "structure",
    "imagepart",
    "soundupgrade",
    "aiwallnodeassignment",
    "sideinfo",
    "turret",
    "terraincelltype",
    "thingtemplatenames",
    "decaltemplate",
    "windowtransition",
    "window",
    "transition",
    "formationselection",
    "unitdefinition",
    "formationtemplate",
    "rows",
    "replaceobject",
];

#[derive(Debug, Clone, Default)]
pub struct BlockTable {
    by_context: BTreeMap<String, BTreeMap<String, bool>>,
    aliases: BTreeMap<String, String>,
    closed: BTreeSet<String>,
    wildcard: BTreeSet<String>,
}

const ANY_CONTEXT: &str = "*";

impl BlockTable {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn from_rules(rules: &[(&str, &str, bool)]) -> Self {
        let mut table = Self::new();
        table.extend(rules);
        table
    }

    #[must_use]
    pub fn object_model_path() -> Self {
        let mut table = Self::from_rules(OBJECT_MODEL_RULES);
        for (from, to) in OBJECT_MODEL_ALIASES {
            table.alias(from, to);
        }
        for context in OBJECT_MODEL_CLOSED {
            table.close(context);
        }
        table.declare_wildcard("unitspecificsounds");
        table.declare_wildcard("unitspecificfx");
        table
    }

    #[must_use]
    pub fn particle_path() -> Self {
        let mut table = Self::from_rules(PARTICLE_RULES);
        for context in PARTICLE_CLOSED {
            table.close(context);
        }
        table
    }

    #[must_use]
    pub fn audio_path() -> Self {
        let mut table = Self::from_rules(AUDIO_RULES);
        for context in AUDIO_CLOSED {
            table.close(context);
        }
        table
    }

    #[must_use]
    pub fn priority_blocks_path() -> Self {
        let mut table = Self::object_model_path();
        table.extend(PRIORITY_BLOCK_RULES);
        for &(context, keyword, opens) in PARTICLE_RULES {
            if context == "fxlist" {
                table.insert(context, keyword, opens);
            }
        }
        table.extend(HORDE_CONTAIN_RULES);
        for context in PRIORITY_BLOCK_CLOSED {
            table.close(context);
        }
        for context in HORDE_CONTAIN_CLOSED {
            table.close(context);
        }
        table
    }

    #[must_use]
    pub fn corpus_path() -> Self {
        let mut table = Self::priority_blocks_path();
        table.extend(PARTICLE_RULES);
        for context in PARTICLE_CLOSED {
            table.close(context);
        }
        table.extend(AUDIO_RULES);
        for context in AUDIO_CLOSED {
            table.close(context);
        }
        table.extend(CORPUS_FILE_SCOPE_RULES);
        table.extend(CORPUS_INTERIOR_RULES);
        table.extend(LIVING_WORLD_RULES);
        for context in CORPUS_CLOSED {
            table.close(context);
        }
        table
    }

    pub fn alias(&mut self, from: &str, to: &str) {
        self.aliases
            .insert(from.to_ascii_lowercase(), to.to_ascii_lowercase());
    }

    pub fn close(&mut self, context: &str) {
        self.closed.insert(context.to_ascii_lowercase());
    }

    #[must_use]
    pub fn is_closed(&self, context: &str) -> bool {
        self.closed
            .contains(self.resolve(&context.to_ascii_lowercase()))
    }

    pub fn declare_wildcard(&mut self, context: &str) {
        self.wildcard.insert(context.to_ascii_lowercase());
        self.close(context);
    }

    #[must_use]
    pub fn is_wildcard(&self, context: &str) -> bool {
        self.wildcard
            .contains(self.resolve(&context.to_ascii_lowercase()))
    }

    fn resolve<'a>(&'a self, context: &'a str) -> &'a str {
        self.aliases.get(context).map_or(context, String::as_str)
    }

    pub fn extend(&mut self, rules: &[(&str, &str, bool)]) {
        for (context, keyword, opens) in rules {
            self.insert(context, keyword, *opens);
        }
    }

    pub fn insert(&mut self, context: &str, keyword: &str, opens: bool) {
        self.by_context
            .entry(context.to_ascii_lowercase())
            .or_default()
            .insert(keyword.to_owned(), opens);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_context.values().map(BTreeMap::len).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_context.values().all(BTreeMap::is_empty)
    }

    fn get(&self, context: &str, keyword: &str) -> Option<bool> {
        self.by_context.get(context)?.get(keyword).copied()
    }
}

impl BlockOpener for BlockTable {
    fn opens_block(&self, context: &[Scope], keyword: &str, args: &[String]) -> Option<bool> {
        if keyword.eq("AddEmotion")
            && args
                .first()
                .is_some_and(|a| a.eq_ignore_ascii_case("OVERRIDE"))
        {
            return Some(true);
        }

        let Some(parent) = context.last() else {
            return self
                .get("", keyword)
                .or_else(|| self.get(ANY_CONTEXT, keyword));
        };

        let parent_keyword = self.resolve(&parent.context);
        let aliased = parent_keyword != parent.context;
        let rebuilt = parent.slot.as_ref().filter(|_| aliased).map(|slot| {
            let mut key = parent_keyword.to_owned();
            key.push_str(slot.get(parent.context.len()..).unwrap_or(""));
            key
        });
        let slot = rebuilt.as_deref().or(parent.slot.as_deref());
        if let Some(slot) = slot {
            if let Some(opens) = self.get(slot, keyword) {
                return Some(opens);
            }
        }
        if let Some(opens) = self.get(parent_keyword, keyword) {
            return Some(opens);
        }
        if let Some(opens) = self.get(ANY_CONTEXT, keyword) {
            return Some(opens);
        }
        if let Some(slot) = slot {
            if self.closed.contains(slot) {
                return Some(false);
            }
        }
        if self.closed.contains(parent_keyword) {
            return Some(false);
        }
        None
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoOpinion;

impl BlockOpener for NoOpinion {
    fn opens_block(&self, _context: &[Scope], _keyword: &str, _args: &[String]) -> Option<bool> {
        None
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests;
