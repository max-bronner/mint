use crate::tree::{Block, Node};

use super::grammar::{
    fields, index_enum, label, walk, Bits, ColorInt, Coord2, Coord3, Cx, Defined, Rgb, ShadowBits,
    Table, TokenList, Tokens,
};

fields! {
    pub struct EyeTowerDef {
        pupil_anim_object: Option<String> = "PupilAnimObject",
        pupil_beam_anim_object: Option<String> = "PupilBeamAnimObject",
        eye_decal_anim_object: Option<String> = "EyeDecalAnimObject",
        eye_decal_beam_anim_object: Option<String> = "EyeDecalBeamAnimObject",
    }
}

fields! {
    pub struct MapInfoDef {
        map_object: Option<String> = "MapObject",
        cloud_border_sub_object: Option<String> = "CloudBorderSubObject",
        text_layer_sub_object: Option<String> = "TextLayerSubObject",
        embers_part_sys: Option<String> = "EmbersPartSys",
        men_banner: Option<String> = "MenBanner",
        elves_banner: Option<String> = "ElvesBanner",
        dwarves_banner: Option<String> = "DwarvesBanner",
        isengard_banner: Option<String> = "IsengardBanner",
        mordor_banner: Option<String> = "MordorBanner",
        wild_banner: Option<String> = "WildBanner",
        neutral_banner: Option<String> = "NeutralBanner",
        angmar_banner: Option<String> = "AngmarBanner",
        men_ants: Option<String> = "MenAnts",
        elves_ants: Option<String> = "ElvesAnts",
        dwarves_ants: Option<String> = "DwarvesAnts",
        isengard_ants: Option<String> = "IsengardAnts",
        mordor_ants: Option<String> = "MordorAnts",
        wild_ants: Option<String> = "WildAnts",
        neutral_ants: Option<String> = "NeutralAnts",
        angmar_ants: Option<String> = "AngmarAnts",
        battle_marker: Option<String> = "BattleMarker",
        palantir_marker: Option<String> = "PalantirMarker",
        region_award_dispute_marker: Option<String> = "RegionAwardDisputeMarker",
        army_line_texture_name: Option<String> = "ArmyLineTextureName",
        anim_rays: Option<String> = "AnimRays",
        anim_rays_part_sys: Option<String> = "AnimRaysPartSys",
        anim_cloud: Option<String> = "AnimCloud",
        anim_cloud_part_sys: Option<String> = "AnimCloudPartSys",
        mordor_cloud: Option<String> = "MordorCloud",
        add_shadow_sub_object: TokenList = "AddShadowSubObject",
        num_world_tiles: Option<i32> = "NumWorldTiles",
        anim_rays_effect_shells: Option<i32> = "AnimRaysEffectShells",
        anim_rays_effect_diameter: Option<i32> = "AnimRaysEffectDiameter",
        anim_rays_effect_lifetime: Option<i32> = "AnimRaysEffectLifetime",
        num_anim_clouds: Option<i32> = "NumAnimClouds",
        anim_cloud_lifetime: Option<i32> = "AnimCloudLifetime",
        cloud_growth_rate: Option<i32> = "CloudGrowthRate",
        click_scroll_threshold: Option<i32> = "ClickScrollThreshold",
        army_selected_fade_in_start: Option<i32> = "ArmySelectedFadeInStart",
        army_selected_fade_in_end: Option<i32> = "ArmySelectedFadeInEnd",
        army_selected_fade_out_start: Option<i32> = "ArmySelectedFadeOutStart",
        army_selected_fade_out_end: Option<i32> = "ArmySelectedFadeOutEnd",
        army_hilighted_fade_in_time: Option<i32> = "ArmyHilightedFadeInTime",
        army_hilighted_fade_out_time: Option<i32> = "ArmyHilightedFadeOutTime",
        num_points_per_army_line: Option<u32> = "NumPointsPerArmyLine",
        apt_zoom: Option<f32> = "AptZoom",
        apt_pitch: Option<f32> = "AptPitch",
        banner_scale_speed: Option<f32> = "BannerScaleSpeed",
        banner_max_scale: Option<f32> = "BannerMaxScale",
        banner_tilt_angle: Option<f32> = "BannerTiltAngle",
        banner_height: Option<f32> = "BannerHeight",
        army_height: Option<f32> = "ArmyHeight",
        beacon_height: Option<f32> = "BeaconHeight",
        default_army_move_speed: Option<f32> = "DefaultArmyMoveSpeed",
        hero_army_icon_diameter: Option<f32> = "HeroArmyIconDiameter",
        army_line_height_bias: Option<f32> = "ArmyLineHeightBias",
        army_line_width: Option<f32> = "ArmyLineWidth",
        anim_rays_color_scale: Option<f32> = "AnimRaysColorScale",
        cloud_initial_size: Option<f32> = "CloudInitialSize",
        cloud_growth_size: Option<f32> = "CloudGrowthSize",
        cloud_initial_opacity: Option<f32> = "CloudInitialOpacity",
        camera_bound_x: Option<f32> = "CameraBoundX",
        camera_bound_y: Option<f32> = "CameraBoundY",
        mouse_wheel_zoom_per_tick: Option<f32> = "MouseWheelZoomPerTick",
        mouse_wheel_zoom_dampen_factor: Option<f32> = "MouseWheelZoomDampenFactor",
        auto_scroll_speed: Option<f32> = "AutoScrollSpeed",
        max_auto_scroll_time: Option<f32> = "MaxAutoScrollTime",
        center: Option<Coord2> = "Center",
        extent: Option<Coord2> = "Extent",
        apt_center: Option<Coord2> = "AptCenter",
        sun_dir: Option<Coord2> = "SunDir",
        accent1_dir: Option<Coord2> = "Accent1Dir",
        accent2_dir: Option<Coord2> = "Accent2Dir",
        cloud_pos: Option<Coord3> = "CloudPos",
        anim_rays_part_sys_offset: Option<Coord3> = "AnimRaysPartSysOffset",
        anim_cloud_region_min: Option<Coord3> = "AnimCloudRegionMin",
        anim_cloud_region_max: Option<Coord3> = "AnimCloudRegionMax",
        cloud_growth_pos: Option<Coord3> = "CloudGrowthPos",
        ambient: Option<Rgb> = "Ambient",
        sun_rgb: Option<Rgb> = "SunRGB",
        accent1_rgb: Option<Rgb> = "Accent1RGB",
        accent2_rgb: Option<Rgb> = "Accent2RGB",
        army_line_color_attacking: Option<Rgb> = "ArmyLineColorAttacking",
        army_line_color_neutral: Option<Rgb> = "ArmyLineColorNeutral",
        army_line_color_allied: Option<Rgb> = "ArmyLineColorAllied",
        anim_rays_color: Option<Rgb> = "AnimRaysColor",
        shadow_color: Option<ColorInt> = "ShadowColor",
        enable_map_shadows: Option<bool> = "EnableMapShadows",
        battle_marker_created_sound: Option<String> = "BattleMarkerCreatedSound",
        enter_map_sound: Option<String> = "EnterMapSound",
        anim_rays_create_sound: Option<String> = "AnimRaysCreateSound",
    }
    with {
        pub eye_towers: Vec<EyeTowerDef>,
    }
}

impl MapInfoDef {
    pub(crate) fn read(block: &Block, cx: &Cx) -> Self {
        let ctx = label(block);
        let mut out = Self::default();
        walk(block, cx, &ctx, |node| match node {
            Node::Field(field) => out.read_field(field, cx, &ctx),
            Node::Block(child) if child.keyword == "EyeTower" => {
                out.eye_towers
                    .push(EyeTowerDef::read_plain_block(child, cx));
                true
            }
            _ => false,
        });
        out
    }
}

index_enum! {
    pub enum WorldObjectType ("world object type") {
        Army = "ARMY",
        BattleMarker = "BATTLE_MARKER",
        RegionAwardDispute = "REGION_AWARD_DISPUTE",
        Cloud = "CLOUD",
        Building = "BUILDING",
        Default = "DEFAULT",
    }
}

fields! {
    pub struct WorldObjectDef {
        object_type: Option<WorldObjectType> = "ObjectType",
        default_flash_value: Option<f32> = "DefaultFlashValue",
        flash_variation: Option<f32> = "FlashVariation",
    }
}

impl Defined for WorldObjectDef {
    fn def_name(&self) -> &str {
        &self.name
    }
}

index_enum! {
    pub enum SoundFlag ("sound flag") {
        ZoomedIn = "ZOOMED_IN",
        ZoomedOut = "ZOOMED_OUT",
        ZoomingIn = "ZOOMING_IN",
        Visible = "VISIBLE",
        Once = "ONCE",
        NoKill = "NO_KILL",
        FadeIn = "FADE_IN",
    }
}

fields! {
    pub struct WorldSoundDef {
        position: Option<Coord3> = "Position",
        sound: Option<String> = "Sound",
        flags: Bits<SoundFlag> = "Flags",
        zoom_region_low: Option<Coord2> = "ZoomRegionLow",
        zoom_region_high: Option<Coord2> = "ZoomRegionHigh",
    }
}

impl Defined for WorldSoundDef {
    fn def_name(&self) -> &str {
        &self.name
    }
}

fields! {
    pub struct BannerTypeDef {
        flag_obj: Option<String> = "FlagObj",
        glow_obj: Option<String> = "GlowObj",
        icon: Option<String> = "Icon",
    }
}

impl Defined for BannerTypeDef {
    fn def_name(&self) -> &str {
        &self.name
    }
}

fields! {
    pub struct AnimObjectDef {
        model: Option<String> = "Model",
        pos: Option<Coord3> = "Pos",
        frame: Option<f32> = "Frame",
        orient_angle: Option<f32> = "OrientAngle",
        has_anim: Option<bool> = "HasAnim",
        xfer: Option<bool> = "Xfer",
        shadow: ShadowBits = "Shadow",
    }
}

impl Defined for AnimObjectDef {
    fn def_name(&self) -> &str {
        &self.name
    }
}

index_enum! {
    pub enum AnimMode ("animation mode") {
        Manual = "MANUAL",
        Loop = "LOOP",
        Once = "ONCE",
        LoopPingpong = "LOOP_PINGPONG",
        PlayToFrame = "PLAY_TO_FRAME",
        LoopBackwards = "LOOP_BACKWARDS",
        OnceBackwards = "ONCE_BACKWARDS",
    }
}

index_enum! {
    pub enum FadeMethod ("fade method") {
        Opacity = "Opacity",
        Additive = "Additive",
    }
}

index_enum! {
    pub enum FadeType ("fade type") {
        None = "NONE",
        In = "IN",
        Out = "OUT",
        InOut = "INOUT",
        Stop = "STOP",
    }
}

index_enum! {
    pub enum ArmySize ("army size") {
        HeroOnly = "HERO_ONLY",
        Small = "SMALL",
        Medium = "MEDIUM",
        Large = "LARGE",
    }
}

fields! {
    pub struct IconObjectBase {
        model: Option<String> = "Model",
        pickbox: Option<String> = "Pickbox",
        sub_objects: Tokens = "SubObjects",
        z_offset: Option<f32> = "ZOffset",
        scale: Option<f32> = "Scale",
        orient_angle: Option<f32> = "OrientAngle",
        fade_hold_percent: Option<f32> = "FadeHoldPercent",
        hide_when_hilighted: Option<bool> = "HideWhenHilighted",
        hide_when_unhilighted: Option<bool> = "HideWhenUnhilighted",
        hide_when_selected: Option<bool> = "HideWhenSelected",
        hide_when_unselected: Option<bool> = "HideWhenUnselected",
        use_house_color: Option<bool> = "UseHouseColor",
        show_only_for_local_player: Option<bool> = "ShowOnlyForLocalPlayer",
        show_only_for_allies: Option<bool> = "ShowOnlyForAllies",
        shadow: ShadowBits = "Shadow",
        anim_mode: Option<AnimMode> = "AnimMode",
        fade_method: Option<FadeMethod> = "FadeMethod",
        fade_type_for_selection: Option<FadeType> = "FadeTypeForSelection",
        fade_type_for_deselection: Option<FadeType> = "FadeTypeForDeselection",
        fade_type_for_hilighting: Option<FadeType> = "FadeTypeForHilighting",
        fade_type_for_unhilighting: Option<FadeType> = "FadeTypeForUnhilighting",
        fade_in_time: Option<u32> = "FadeInTime",
        fade_out_time: Option<u32> = "FadeOutTime",
    }
}

fields! {
    pub struct ArmyObjectExtra {
        visible_army_sizes: Bits<ArmySize> = "VisibleArmySizes",
        clickable: Option<bool> = "Clickable",
        display_at_rally_point: Option<bool> = "DisplayAtRallyPoint",
        show_only_after_move_order: Option<bool> = "ShowOnlyAfterMoveOrder",
    }
}

fields! {
    pub struct BuildingObjectExtra {
        clickable: Option<bool> = "Clickable",
        hide_when_under_construction: Option<bool> = "HideWhenUnderConstruction",
        hide_when_not_under_construction: Option<bool> = "HideWhenNotUnderConstruction",
        hide_when_not_producing: Option<bool> = "HideWhenNotProducing",
        fade_type_for_showing: Option<FadeType> = "FadeTypeForShowing",
        fade_type_for_hiding: Option<FadeType> = "FadeTypeForHiding",
    }
}

fields! {
    pub struct BuildPlotObjectExtra {
        clickable: Option<bool> = "Clickable",
        hide_when_occupied: Option<bool> = "HideWhenOccupied",
        hide_when_not_occupied: Option<bool> = "HideWhenNotOccupied",
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct IconObject<K> {
    pub name: String,
    pub base: IconObjectBase,
    pub extra: K,
}

fn read_icon_object<K: Table + Default>(block: &Block, cx: &Cx) -> IconObject<K> {
    let ctx = label(block);
    let mut out = IconObject::<K> {
        name: block.name().unwrap_or_default().to_owned(),
        ..IconObject::default()
    };
    walk(block, cx, &ctx, |node| match node {
        Node::Field(field) => {
            out.base.read_field(field, cx, &ctx) || out.extra.read_table(field, cx, &ctx)
        }
        _ => false,
    });
    out
}

pub type ArmyObject = IconObject<ArmyObjectExtra>;
pub type BuildingObject = IconObject<BuildingObjectExtra>;
pub type BuildPlotObject = IconObject<BuildPlotObjectExtra>;

fields! {
    pub struct ArmyIconDef {
        on_selected_sound: Option<String> = "OnSelectedSound",
        on_move_planned_sound: Option<String> = "OnMovePlannedSound",
        on_move_started_sound: Option<String> = "OnMoveStartedSound",
        welcome_reinforcements_sound: Option<String> = "WelcomeReinforcementsSound",
        kick_out_reinforcements_sound: Option<String> = "KickOutReinforcementsSound",
        disband_unit_sound: Option<String> = "DisbandUnitSound",
        retreat_teleport_to_home_region_eva_event: Option<String> = "RetreatTeleportToHomeRegionEvaEvent",
        retreat_teleport_to_non_home_region_eva_event: Option<String> = "RetreatTeleportToNonHomeRegionEvaEvent",
    }
    with {
        pub objects: Vec<ArmyObject>,
    }
}

fields! {
    pub struct BuildingIconDef {
        on_selected_sound: Option<String> = "OnSelectedSound",
        on_construction_begun_sound: Option<String> = "OnConstructionBegunSound",
        on_construction_finished_sound: Option<String> = "OnConstructionFinishedSound",
    }
    with {
        pub objects: Vec<BuildingObject>,
    }
}

fields! {
    pub struct BuildPlotIconDef {
        on_selected_sound: Option<String> = "OnSelectedSound",
        on_building_destroyed_sound: Option<String> = "OnBuildingDestroyedSound",
    }
    with {
        pub objects: Vec<BuildPlotObject>,
    }
}

macro_rules! icon_reader {
    ($def:ident, $extra:ident) => {
        impl Defined for $def {
            fn def_name(&self) -> &str {
                &self.name
            }
        }

        impl $def {
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
                    Node::Block(child) if child.keyword == "Object" => {
                        out.objects.push(read_icon_object::<$extra>(child, cx));
                        true
                    }
                    _ => false,
                });
                out
            }
        }
    };
}
icon_reader!(ArmyIconDef, ArmyObjectExtra);
icon_reader!(BuildingIconDef, BuildingObjectExtra);
icon_reader!(BuildPlotIconDef, BuildPlotObjectExtra);
