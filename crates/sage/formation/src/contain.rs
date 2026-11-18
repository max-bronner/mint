use mint_sage_ini::value::{parse_bool, parse_float, parse_float_or_macro, parse_int, MacroTable};
use mint_sage_ini::{find_melee_behavior, Block, Field, MeleeBehaviorDef, ValueErrors};

pub const HORDE_CONTAIN_MODULES: [&str; 3] =
    ["HordeContain", "HorseHordeContain", "AODHordeContain"];

pub const MAX_SLOTS: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaderRef {
    pub rank: u32,
    pub index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotPosition {
    pub x: f32,
    pub y: f32,
    pub leader: Option<LeaderRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RankLine {
    pub rank: u32,
    pub unit_type: String,
    pub positions: Vec<SlotPosition>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContainReport {
    pub positions_dropped: usize,
    pub positions_malformed: usize,
    pub slots_unresolved: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AodSway {
    pub frequency_scale: f32,
    pub frequency_randomness: f32,
    pub amplitude_scale: f32,
    pub amplitude_randomness: f32,
    pub still_amplitude: f32,
    pub frequency_scale_z: f32,
    pub frequency_randomness_z: f32,
    pub amplitude_scale_z: f32,
    pub amplitude_randomness_z: f32,
    pub still_amplitude_z: f32,
    pub oath_z_factor: f32,
    pub large_unit_height_factor: f32,
    pub large_unit_min_height: f32,
    pub large_unit_max_height: f32,
    pub large_unit_timeout_ms: f32,
    pub large_unit_tail_off: f32,
    pub scatter_speed_factor: f32,
    pub scatter_randomness: f32,
}

const DEFAULT_LARGE_UNIT_TIMEOUT_MS: f32 = 2.0 * 1000.0 / 30.0;

impl Default for AodSway {
    fn default() -> Self {
        Self {
            frequency_scale: 0.0,
            frequency_randomness: 0.0,
            amplitude_scale: 0.0,
            amplitude_randomness: 0.0,
            still_amplitude: 0.0,
            frequency_scale_z: 0.0,
            frequency_randomness_z: 0.0,
            amplitude_scale_z: 0.0,
            amplitude_randomness_z: 0.0,
            still_amplitude_z: 0.0,
            oath_z_factor: 1.0,
            large_unit_height_factor: 0.45,
            large_unit_min_height: 0.0,
            large_unit_max_height: 999_999.0,
            large_unit_timeout_ms: DEFAULT_LARGE_UNIT_TIMEOUT_MS,
            large_unit_tail_off: 1.0,
            scatter_speed_factor: 0.3,
            scatter_randomness: 0.4,
        }
    }
}

const AOD_SWAY_FIELDS: [(&str, fn(&mut AodSway) -> &mut f32); 18] = [
    ("FrequencyScale", |s| &mut s.frequency_scale),
    ("FrequencyRandomness", |s| &mut s.frequency_randomness),
    ("AmplitudeScale", |s| &mut s.amplitude_scale),
    ("AmplitudeRandomness", |s| &mut s.amplitude_randomness),
    ("StillAmplitude", |s| &mut s.still_amplitude),
    ("FrequencyScaleZ", |s| &mut s.frequency_scale_z),
    ("FrequencyRandomnessZ", |s| &mut s.frequency_randomness_z),
    ("AmplitudeScaleZ", |s| &mut s.amplitude_scale_z),
    ("AmplitudeRandomnessZ", |s| &mut s.amplitude_randomness_z),
    ("StillAmplitudeZ", |s| &mut s.still_amplitude_z),
    ("OathFulfilledZFactor", |s| &mut s.oath_z_factor),
    ("LargeUnitHeightFactor", |s| &mut s.large_unit_height_factor),
    ("LargeUnitMinHeight", |s| &mut s.large_unit_min_height),
    ("LargeUnitMaxHeight", |s| &mut s.large_unit_max_height),
    ("LargeUnitTimeout", |s| &mut s.large_unit_timeout_ms),
    ("LargeUnitTailOff", |s| &mut s.large_unit_tail_off),
    ("ScatterSpeedFactor", |s| &mut s.scatter_speed_factor),
    ("ScatterRandomness", |s| &mut s.scatter_randomness),
];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HordeContainDef {
    pub slots: Option<u32>,
    pub ranks: Vec<RankLine>,
    pub random_offset: (f32, f32),
    pub front_angle: Option<f32>,
    pub melee_attack_leash_distance: Option<f32>,
    pub use_slow_horde_movement: Option<bool>,
    pub minimum_horde_size: Option<u32>,
    pub ranks_to_release: Vec<u32>,
    pub ranks_that_stop_advance: Vec<u32>,
    pub horse: bool,
    pub alternate_formation: Option<String>,
    pub initial_payload: Option<(String, u32)>,
    pub payload_entries: u32,
    pub banner_carrier_position: Option<(f32, f32)>,
    pub banner_carriers_allowed: Option<String>,
    pub banner_carrier_min_level: Option<u32>,
    pub banner_carrier_destroy_horde_on_death: Option<bool>,
    pub leaders_allowed: Vec<String>,
    pub leader_position: Option<(f32, f32)>,
    pub melee_behavior: Option<MeleeBehaviorDef>,
    pub main_formation: Option<bool>,
    pub porcupine: Option<bool>,
    pub eva_event_last_member_death: Option<String>,
    pub aod: Option<AodSway>,
    pub report: ContainReport,
}

impl HordeContainDef {
    #[must_use]
    pub fn position_count(&self) -> usize {
        self.ranks.iter().fold(0usize, |total, rank| {
            total.saturating_add(rank.positions.len())
        })
    }

    #[must_use]
    pub fn slot_count(&self) -> usize {
        let declared = self.slots.map_or(usize::MAX, |slots| {
            usize::try_from(slots).unwrap_or(usize::MAX)
        });
        declared.min(self.position_count()).min(MAX_SLOTS)
    }
}

#[must_use]
pub fn is_horde_contain(block: &Block) -> bool {
    block.keyword.eq_ignore_ascii_case("Behavior")
        && block.selector().is_some_and(|selector| {
            HORDE_CONTAIN_MODULES
                .iter()
                .any(|name| selector.eq_ignore_ascii_case(name))
        })
}

pub const MAX_MODULE_DEPTH: usize = 4;

#[must_use]
pub fn horde_contains(object: &Block) -> Vec<&Block> {
    let mut found = Vec::new();
    collect(object, 0, &mut found);
    found
}

#[must_use]
pub fn find_horde_contain(object: &Block) -> Option<&Block> {
    horde_contains(object).first().copied()
}

fn collect<'a>(block: &'a Block, depth: usize, found: &mut Vec<&'a Block>) {
    if depth >= MAX_MODULE_DEPTH {
        return;
    }
    for child in block.blocks() {
        if is_horde_contain(child) {
            found.push(child);
            continue;
        }
        collect(child, depth.saturating_add(1), found);
    }
}

fn keyed<'a>(token: &'a str, key: &str) -> Option<&'a str> {
    let (found, rest) = token.split_once(':')?;
    found.eq_ignore_ascii_case(key).then_some(rest)
}

fn axis(args: &[String], at: usize, key: &str) -> Option<(f32, usize)> {
    let token = args.get(at)?;
    let rest = keyed(token, key)?;
    if rest.is_empty() {
        let value = parse_float(args.get(at.saturating_add(1))?)?;
        return Some((value, at.saturating_add(2)));
    }
    Some((parse_float(rest)?, at.saturating_add(1)))
}

fn read_rank_line(
    field: &Field,
    report: &mut ContainReport,
    budget: &mut usize,
) -> Option<RankLine> {
    let args = &field.args;
    let mut rank = None;
    let mut unit_type = None;
    let mut positions: Vec<SlotPosition> = Vec::new();
    let mut at = 0usize;
    while at < args.len() {
        let Some(token) = args.get(at) else { break };
        if let Some(rest) = keyed(token, "RankNumber") {
            let (value, next) = if rest.is_empty() {
                (
                    args.get(at.saturating_add(1)).and_then(|t| parse_int(t)),
                    at.saturating_add(2),
                )
            } else {
                (parse_int(rest), at.saturating_add(1))
            };
            rank = value.and_then(|v| u32::try_from(v).ok());
            at = next;
            continue;
        }
        if let Some(rest) = keyed(token, "UnitType") {
            if rest.is_empty() {
                unit_type = args.get(at.saturating_add(1)).cloned();
                at = at.saturating_add(2);
            } else {
                unit_type = Some(rest.to_owned());
                at = at.saturating_add(1);
            }
            continue;
        }
        if let Some(rest) = keyed(token, "Position") {
            let found = if rest.is_empty() {
                axis(args, at.saturating_add(1), "X")
            } else {
                match keyed(rest, "X") {
                    Some(value) if !value.is_empty() => {
                        parse_float(value).map(|v| (v, at.saturating_add(1)))
                    }
                    Some(_) => args
                        .get(at.saturating_add(1))
                        .and_then(|t| parse_float(t))
                        .map(|v| (v, at.saturating_add(2))),
                    None => None,
                }
            };
            let Some((x, mut next)) = found else {
                report.positions_malformed = report.positions_malformed.saturating_add(1);
                at = at.saturating_add(1);
                continue;
            };
            let Some((y, after_y)) = axis(args, next, "Y") else {
                report.positions_malformed = report.positions_malformed.saturating_add(1);
                at = next;
                continue;
            };
            next = after_y;
            let mut leader = None;
            if args
                .get(next)
                .is_some_and(|t| t.eq_ignore_ascii_case("Leader"))
            {
                let rank_of = args
                    .get(next.saturating_add(1))
                    .and_then(|t| parse_int(t))
                    .and_then(|v| u32::try_from(v).ok());
                let index_of = args
                    .get(next.saturating_add(2))
                    .and_then(|t| parse_int(t))
                    .and_then(|v| u32::try_from(v).ok());
                if let (Some(rank), Some(index)) = (rank_of, index_of) {
                    leader = Some(LeaderRef { rank, index });
                }
                next = next.saturating_add(3);
            }
            at = next;
            if !x.is_finite() || !y.is_finite() || *budget == 0 {
                report.positions_dropped = report.positions_dropped.saturating_add(1);
                continue;
            }
            *budget = budget.saturating_sub(1);
            positions.push(SlotPosition { x, y, leader });
            continue;
        }
        at = at.saturating_add(1);
    }
    Some(RankLine {
        rank: rank.unwrap_or(0),
        unit_type: unit_type.unwrap_or_default(),
        positions,
    })
}

#[must_use]
pub fn read_horde_contain(
    block: &Block,
    macros: &MacroTable,
    errors: &ValueErrors,
) -> HordeContainDef {
    let mut def = HordeContainDef {
        horse: block
            .selector()
            .is_some_and(|module| module.eq_ignore_ascii_case("HorseHordeContain")),
        aod: block
            .selector()
            .is_some_and(|module| module.eq_ignore_ascii_case("AODHordeContain"))
            .then(AodSway::default),
        ..HordeContainDef::default()
    };
    let mut budget = MAX_SLOTS;
    for field in block.fields() {
        let keyword = field.keyword.as_str();
        if let Some(sway) = def.aod.as_mut() {
            if let Some((_, slot)) = AOD_SWAY_FIELDS
                .iter()
                .find(|(name, _)| keyword.eq_ignore_ascii_case(name))
            {
                match field
                    .value()
                    .and_then(parse_float)
                    .filter(|v| v.is_finite())
                {
                    Some(value) => *slot(sway) = value,
                    None => errors.field("AODHordeContain", field, "a number"),
                }
                continue;
            }
        }
        if keyword.eq_ignore_ascii_case("RankInfo") {
            if let Some(line) = read_rank_line(field, &mut def.report, &mut budget) {
                def.ranks.push(line);
            }
            continue;
        }
        if keyword.eq_ignore_ascii_case("Slots") {
            match field
                .value()
                .and_then(|v| parse_float_or_macro(v, macros))
                .filter(|v| v.is_finite() && *v >= 0.0)
            {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                Some(value) => def.slots = Some(value.min(f32::from(u16::MAX)) as u32),
                None => def.report.slots_unresolved = true,
            }
            continue;
        }
        if keyword.eq_ignore_ascii_case("RandomOffset") {
            let x = axis(&field.args, 0, "X").map_or(0.0, |(v, _)| v);
            let y = axis(&field.args, 0, "Y")
                .or_else(|| axis(&field.args, 1, "Y"))
                .map_or(0.0, |(v, _)| v);
            def.random_offset = (
                if x.is_finite() { x.abs() } else { 0.0 },
                if y.is_finite() { y.abs() } else { 0.0 },
            );
            continue;
        }
        if keyword.eq_ignore_ascii_case("FrontAngle") {
            def.front_angle = field.value().and_then(parse_float);
            continue;
        }
        if keyword.eq_ignore_ascii_case("MeleeAttackLeashDistance") {
            def.melee_attack_leash_distance = field.value().and_then(parse_float);
            continue;
        }
        if keyword.eq_ignore_ascii_case("MinimumHordeSize") {
            def.minimum_horde_size = field
                .value()
                .and_then(|v| parse_float_or_macro(v, macros))
                .filter(|v| v.is_finite() && *v >= 0.0)
                .map(|v| {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let value = v.min(f32::from(u16::MAX)) as u32;
                    value
                });
            continue;
        }
        if keyword.eq_ignore_ascii_case("RanksToReleaseWhenAttacking") {
            def.ranks_to_release = rank_list(field);
            continue;
        }
        if keyword.eq_ignore_ascii_case("RanksThatStopAdvance") {
            def.ranks_that_stop_advance = rank_list(field);
            continue;
        }
        if keyword.eq_ignore_ascii_case("UseSlowHordeMovement") {
            def.use_slow_horde_movement = field.value().and_then(parse_bool);
            if def.use_slow_horde_movement.is_none() {
                errors.field("HordeContain", field, "Yes or No");
            }
            continue;
        }
        if keyword.eq_ignore_ascii_case("AlternateFormation") {
            def.alternate_formation = field.value().map(str::to_owned);
            continue;
        }
        if keyword.eq_ignore_ascii_case("EvaEventLastMemberDeath") {
            def.eva_event_last_member_death = field.value().map(str::to_owned);
            continue;
        }
        if keyword.eq_ignore_ascii_case("ThisFormationIsTheMainFormation") {
            def.main_formation = field.value().and_then(parse_bool);
            continue;
        }
        if keyword.eq_ignore_ascii_case("IsPorcupineFormation") {
            def.porcupine = field.value().and_then(parse_bool);
            if def.porcupine.is_none() {
                errors.field("HordeContain", field, "Yes or No");
            }
            continue;
        }
        if keyword.eq_ignore_ascii_case("InitialPayload") {
            def.payload_entries = def.payload_entries.saturating_add(1);
            let template = field.value().map(str::to_owned);
            let count = field
                .args
                .get(1)
                .and_then(|v| parse_float_or_macro(v, macros))
                .filter(|v| v.is_finite() && *v >= 0.0)
                .map(|v| {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let value = v.min(f32::from(u16::MAX)) as u32;
                    value
                });
            if let (Some(template), Some(count)) = (template, count) {
                def.initial_payload = Some((template, count));
            }
            continue;
        }
        if keyword.eq_ignore_ascii_case("BannerCarrierPosition") {
            let mut found = None;
            for at in 0..field.args.len() {
                let Some(token) = field.args.get(at) else {
                    break;
                };
                let Some(rest) = keyed(token, "Pos") else {
                    continue;
                };
                let x = keyed(rest, "X").and_then(parse_float);
                let y = axis(&field.args, at.saturating_add(1), "Y").map(|(v, _)| v);
                if let (Some(x), Some(y)) = (x, y) {
                    found = Some((x, y));
                }
                break;
            }
            def.banner_carrier_position = found;
            continue;
        }
        if keyword.eq_ignore_ascii_case("BannerCarriersAllowed") {
            def.banner_carriers_allowed = field
                .value()
                .filter(|name| !name.eq_ignore_ascii_case("NONE"))
                .map(str::to_owned);
            continue;
        }
        if keyword.eq_ignore_ascii_case("BannerCarrierMinLevel") {
            def.banner_carrier_min_level = field
                .value()
                .and_then(parse_int)
                .and_then(|v| u32::try_from(v).ok());
            continue;
        }
        if keyword.eq_ignore_ascii_case("BannerCarrierDestroyHordeOnDeath") {
            def.banner_carrier_destroy_horde_on_death = field.value().and_then(parse_bool);
            continue;
        }
        if keyword.eq_ignore_ascii_case("LeadersAllowed") {
            def.leaders_allowed = field
                .args
                .iter()
                .filter(|name| !name.eq_ignore_ascii_case("NONE"))
                .map(|name| name.to_owned())
                .collect();
            continue;
        }
        if keyword.eq_ignore_ascii_case("LeaderPosition") {
            let x = axis(&field.args, 0, "X").map(|(v, _)| v);
            let y = axis(&field.args, 0, "Y")
                .or_else(|| axis(&field.args, 1, "Y"))
                .map(|(v, _)| v);
            if let (Some(x), Some(y)) = (x, y) {
                if x.is_finite() && y.is_finite() {
                    def.leader_position = Some((x, y));
                }
            }
            continue;
        }
    }
    def.melee_behavior = find_melee_behavior(block, errors);
    def
}

fn rank_list(field: &Field) -> Vec<u32> {
    field
        .args
        .iter()
        .filter_map(|t| parse_int(t))
        .filter_map(|v| u32::try_from(v).ok())
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use mint_sage_ini::{BlockTable, Document};

    fn contain(text: &str) -> HordeContainDef {
        let table = BlockTable::priority_blocks_path();
        let doc = Document::parse("test.ini", text, &table).unwrap();
        let object = doc.roots().first().expect("one object");
        let module = object
            .blocks()
            .find(|b| is_horde_contain(b))
            .expect("a horde contain module");
        read_horde_contain(module, &MacroTable::new(), &ValueErrors::new())
    }

    const GONDOR: &str = "\
Object GondorFighterHorde
\tBehavior = HordeContain ModuleTag_HordeContain
\t\tFrontAngle = 270
\t\tInitialPayload = GondorFighter 15
\t\tSlots = 15
\t\tShowPips = No
\t\tThisFormationIsTheMainFormation = Yes
\t\tRandomOffset=X:0 Y:0
\t\tMeleeBehavior = Amoeba
\t\tEnd
\t\tBannerCarrierPosition = UnitType:GondorFighter Pos:X:70.0 Y:0.0
\t\tRankInfo = RankNumber:1 UnitType:GondorFighter Position:X:50 Y:0 Position:X:50 Y:20 Position:X:50 Y:-20 Position:X:50 Y:40 Position:X:50 Y:-40
\t\tRankInfo = RankNumber:2 UnitType:GondorFighter Position:X:30 Y:0 Leader 1 0 Position:X:30 Y:20 Leader 1 1 Position:X:30 Y:-20 Leader 1 2 Position:X:30 Y:40 Leader 1 3 Position:X:30 Y:-40 Leader 1 4
\t\tRankInfo = RankNumber:3 UnitType:GondorFighter Position:X:10 Y:0 Leader 2 0 Position:X:10 Y:20 Leader 2 1 Position:X:10 Y:-20 Leader 2 2 Position:X:10 Y:40 Leader 2 3 Position:X:10 Y:-40 Leader 2 4
\t\tRanksToReleaseWhenAttacking = 1
\t\tAlternateFormation = GondorFighterHordeBlock
\t\tMeleeAttackLeashDistance = 1
\tEnd
End
";

    #[test]
    fn the_shipped_line_grammar_reads_every_field_the_research_measured() {
        let def = contain(GONDOR);
        assert_eq!(def.slots, Some(15));
        assert_eq!(def.ranks.len(), 3);
        assert_eq!(def.position_count(), 15);
        assert_eq!(def.slot_count(), 15);
        assert_eq!(def.front_angle, Some(270.0));
        assert_eq!(def.random_offset, (0.0, 0.0));
        assert_eq!(def.ranks_to_release, vec![1]);
        assert_eq!(
            def.alternate_formation.as_deref(),
            Some("GondorFighterHordeBlock")
        );
        assert_eq!(def.melee_attack_leash_distance, Some(1.0));
        assert_eq!(def.initial_payload, Some(("GondorFighter".to_owned(), 15)));
        assert_eq!(def.banner_carrier_position, Some((70.0, 0.0)));
        assert_eq!(def.banner_carriers_allowed, None, "not in this fixture");
        assert_eq!(
            def.melee_behavior.as_ref().map(|m| m.kind),
            Some(mint_sage_ini::MeleeBehaviorKind::Amoeba)
        );
        assert_eq!(def.main_formation, Some(true));
        let banner = contain(&GONDOR.replace(
            "\t\tBannerCarrierPosition",
            "\t\tBannerCarriersAllowed = GondorInfantryBanner\n\t\tBannerCarrierMinLevel = 0;\n\
             \t\tBannerCarrierPosition",
        ));
        assert_eq!(
            banner.banner_carriers_allowed.as_deref(),
            Some("GondorInfantryBanner")
        );
        assert_eq!(banner.banner_carrier_min_level, Some(0), "`0;` reads as 0");

        assert_eq!(
            def.leaders_allowed,
            Vec::<String>::new(),
            "not in this fixture"
        );
        assert_eq!(def.leader_position, None);
        let leader = contain(&GONDOR.replace(
            "\t\tBannerCarrierPosition",
            "\t\tLeadersAllowed = GondorFighterLeader\n\t\tLeaderPosition = X:60 Y:0\n\
             \t\tBannerCarrierPosition",
        ));
        assert_eq!(
            leader.leaders_allowed,
            vec!["GondorFighterLeader".to_owned()]
        );
        assert_eq!(leader.leader_position, Some((60.0, 0.0)));

        let first = &def.ranks[0];
        assert_eq!(first.rank, 1);
        assert_eq!(first.unit_type, "GondorFighter");
        assert_eq!(first.positions.len(), 5);
        assert_eq!(
            first.positions[0],
            SlotPosition {
                x: 50.0,
                y: 0.0,
                leader: None
            }
        );
        assert_eq!(first.positions[2].y, -20.0);
        assert_eq!(def.ranks[2].positions[0].x, 10.0);
        for rank in &def.ranks[1..] {
            for (index, slot) in rank.positions.iter().enumerate() {
                let leader = slot.leader.expect("a leader reference");
                assert_eq!(leader.rank, rank.rank.saturating_sub(1));
                assert_eq!(usize::try_from(leader.index).unwrap(), index);
            }
        }
        assert_eq!(def.report, ContainReport::default());
    }

    #[test]
    fn axis_letters_are_case_insensitive_and_survive_being_split() {
        let def = contain(
            "Object A\n\tBehavior = HordeContain Tag\n\t\tSlots = 2\n\
             \t\tRandomOffset = X:2 y:3\n\
             \t\tRankInfo = RankNumber:1 UnitType:U Position:X: 50 Y: 10 Position:x:5 y:-5\n\
             \tEnd\nEnd\n",
        );
        assert_eq!(def.random_offset, (2.0, 3.0));
        assert_eq!(def.ranks[0].positions.len(), 2);
        assert_eq!(
            def.ranks[0].positions[0],
            SlotPosition {
                x: 50.0,
                y: 10.0,
                leader: None
            }
        );
        assert_eq!(
            def.ranks[0].positions[1],
            SlotPosition {
                x: 5.0,
                y: -5.0,
                leader: None
            }
        );
    }

    #[test]
    fn slot_count_is_capped_by_the_positions_actually_present() {
        let more_positions = contain(
            "Object A\n\tBehavior = HordeContain Tag\n\t\tSlots = 2\n\
             \t\tRankInfo = RankNumber:1 UnitType:U Position:X:1 Y:1 Position:X:2 Y:2 \
             Position:X:3 Y:3\n\tEnd\nEnd\n",
        );
        assert_eq!(more_positions.position_count(), 3);
        assert_eq!(more_positions.slot_count(), 2);

        let lying = contain(
            "Object A\n\tBehavior = HordeContain Tag\n\t\tSlots = 4000000000\n\
             \t\tRankInfo = RankNumber:1 UnitType:U Position:X:1 Y:1\n\tEnd\nEnd\n",
        );
        assert_eq!(
            lying.slot_count(),
            1,
            "a declared count must not size anything"
        );

        let absent = contain(
            "Object A\n\tBehavior = HordeContain Tag\n\
             \t\tRankInfo = RankNumber:1 UnitType:U Position:X:1 Y:1\n\tEnd\nEnd\n",
        );
        assert_eq!(absent.slots, None);
        assert_eq!(absent.slot_count(), 1);
    }

    #[test]
    fn a_hostile_block_is_read_without_panicking_and_without_inventing_a_slot() {
        let hostile = contain(
            "Object A\n\tBehavior = HordeContain Tag\n\
             \t\tSlots = NotANumber\n\
             \t\tRandomOffset = X:NaN Y:-5\n\
             \t\tFrontAngle =\n\
             \t\tRankInfo = RankNumber: UnitType: Position:X:1\n\
             \t\tRankInfo = Position: Position:X: Position:X:1 Y: Leader\n\
             \t\tRankInfo = RankNumber:-3 UnitType:U Position:X:inf Y:2 Position:X:3 Y:4 Leader 1\n\
             \t\tRankInfo =\n\
             \tEnd\nEnd\n",
        );
        assert!(hostile.report.slots_unresolved);
        assert_eq!(hostile.slots, None);
        assert_eq!(hostile.random_offset, (0.0, 5.0));
        assert_eq!(hostile.front_angle, None);
        assert_eq!(hostile.ranks.len(), 4);
        let kept: Vec<SlotPosition> = hostile
            .ranks
            .iter()
            .flat_map(|r| r.positions.iter().copied())
            .collect();
        assert_eq!(kept.len(), 1, "{kept:?}");
        assert_eq!(
            kept[0],
            SlotPosition {
                x: 3.0,
                y: 4.0,
                leader: None
            }
        );
        assert!(hostile.report.positions_malformed >= 3);
        assert_eq!(hostile.report.positions_dropped, 0);
        assert_eq!(hostile.ranks[2].rank, 0);
        assert_eq!(hostile.slot_count(), 1);
    }

    #[test]
    fn a_table_past_the_cap_is_truncated_rather_than_believed() {
        let mut line = String::from(
            "Object A\n\tBehavior = HordeContain Tag\n\t\tSlots = 5000\n\t\tRankInfo = \
             RankNumber:1 UnitType:U",
        );
        for index in 0..(MAX_SLOTS + 50) {
            line.push_str(&format!(" Position:X:{index} Y:0"));
        }
        line.push_str("\n\tEnd\nEnd\n");
        let def = contain(&line);
        assert_eq!(def.position_count(), MAX_SLOTS);
        assert_eq!(def.report.positions_dropped, 50);
        assert_eq!(def.slot_count(), MAX_SLOTS);
    }

    #[test]
    fn the_module_search_stops_at_the_depth_cap() {
        let deep = MAX_MODULE_DEPTH + 3;
        let mut lines: Vec<String> = vec!["Object A".to_owned()];
        for level in 0..deep {
            lines.push(format!(
                "{}Behavior = SomethingElse Tag",
                "\t".repeat(level + 1)
            ));
        }
        lines.push(format!(
            "{}Behavior = HordeContain Tag",
            "\t".repeat(deep + 1)
        ));
        lines.push(format!("{}Slots = 1", "\t".repeat(deep + 2)));
        for level in (0..=deep).rev() {
            lines.push(format!("{}End", "\t".repeat(level + 1)));
        }
        lines.push("End".to_owned());
        lines.push(String::new());
        let text = lines.join("\n");
        let table = BlockTable::priority_blocks_path();
        let doc = Document::parse("t.ini", &text, &table).unwrap();
        let object = doc.roots().first().unwrap();
        assert!(horde_contains(object).is_empty());
    }

    #[test]
    fn an_aod_contain_reads_its_sway_and_rejects_a_word_for_a_number() {
        let read = |module: &str, body: &str| {
            let text = format!("Object A\n\tBehavior = {module} Tag\n{body}\tEnd\nEnd\n");
            let table = BlockTable::priority_blocks_path();
            let doc = Document::parse("t.ini", &text, &table).unwrap();
            let block = doc.roots()[0].blocks().next().unwrap();
            let errors = ValueErrors::new();
            let def = read_horde_contain(block, &MacroTable::new(), &errors);
            (def, errors.len())
        };
        let (def, errors) = read(
            "AODHordeContain",
            "\t\tSlots = 1\n\t\tAmplitudeScale = 33// side to side\n\t\tStillAmplitudeZ = 0.22\n",
        );
        assert_eq!(errors, 0);
        let sway = def.aod.expect("an AODHordeContain carries its sway");
        assert_eq!(sway.amplitude_scale, 33.0);
        assert_eq!(sway.still_amplitude_z, 0.22);
        assert_eq!(sway.frequency_scale, 0.0);
        let (_, errors) = read("AODHordeContain", "\t\tFrequencyScale = fast\n");
        assert_eq!(errors, 1);
        let (plain, _) = read("HordeContain", "\t\tSlots = 1\n");
        assert_eq!(plain.aod, None);

        let (oath, errors) = read(
            "AODHordeContain",
            "\t\tSlots = 1\n\
             \t\tLargeUnitHeightFactor = 0.6\n\t\tLargeUnitMinHeight = 12.5\n\
             \t\tLargeUnitMaxHeight = 180.0\n\t\tLargeUnitTimeout = 9000\n\
             \t\tLargeUnitTailOff = 1.5f\n\
             \t\tScatterSpeedFactor = 0.22\n\t\tScatterRandomness = 0.05\n",
        );
        assert_eq!(errors, 0);
        let sway = oath.aod.expect("an AODHordeContain carries its sway");
        assert_eq!(sway.large_unit_height_factor, 0.6);
        assert_eq!(sway.large_unit_min_height, 12.5);
        assert_eq!(sway.large_unit_max_height, 180.0);
        assert_eq!(sway.large_unit_timeout_ms, 9000.0);
        assert_eq!(sway.large_unit_tail_off, 1.5);
        assert_eq!(sway.scatter_speed_factor, 0.22);
        assert_eq!(sway.scatter_randomness, 0.05);
        assert_eq!(
            sway.oath_z_factor, 1.0,
            "the constructor value when the block omits it"
        );
    }

    #[test]
    fn only_the_three_rank_carrying_module_names_are_recognised() {
        for name in ["HordeContain", "HorseHordeContain", "AODHordeContain"] {
            let text = format!("Object A\n\tBehavior = {name} Tag\n\t\tSlots = 1\n\tEnd\nEnd\n");
            let table = BlockTable::priority_blocks_path();
            let doc = Document::parse("t.ini", &text, &table).unwrap();
            let module = doc.roots()[0].blocks().next().unwrap();
            assert!(is_horde_contain(module), "{name}");
        }
        for name in [
            "HordeGarrisonContain",
            "HordeTransportContain",
            "OpenContain",
        ] {
            let text = format!("Object A\n\tBehavior = {name} Tag\n\t\tSlots = 1\n\tEnd\nEnd\n");
            let table = BlockTable::priority_blocks_path();
            let doc = Document::parse("t.ini", &text, &table).unwrap();
            let module = doc.roots()[0].blocks().next().unwrap();
            assert!(!is_horde_contain(module), "{name}");
        }
    }
}
