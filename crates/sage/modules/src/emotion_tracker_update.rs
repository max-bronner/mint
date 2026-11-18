use std::sync::Arc;

use mint_sage_ini::{Block, Field, EMOTION_NUGGET_FIELDS};
use mint_sim::module::{CapabilityMask, ModuleKind, UpdateContext, UpgradeMask};
use mint_sim::{Component, Entity, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

use crate::object_filter::{ObjectFilter, Standing};

const DEFAULT_SCAN_MS: u32 = 1000;

#[derive(Debug, PartialEq)]
struct TrackerData {
    cadence: LogicFrames,
    taunt_and_point_distance: f32,
    fear_scan_distance: f32,
    hero_scan_distance: f32,
    afraid_of: ObjectFilter,
    always_afraid_of: ObjectFilter,
    point_at: ObjectFilter,
    taunt_and_point_excluded: ObjectFilter,
    quarrel_probability: f32,
    ignore_veterancy: bool,
    immune_to_fear_level: Option<u32>,
    emotions: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmotionTrackerUpdate {
    data: Arc<TrackerData>,
    next_scan: Frame,
    started: bool,
}

impl EmotionTrackerUpdate {
    #[must_use]
    pub fn cadence(&self) -> LogicFrames {
        self.data.cadence
    }

    #[must_use]
    pub fn taunt_and_point_distance(&self) -> f32 {
        self.data.taunt_and_point_distance
    }

    #[must_use]
    pub fn fear_scan_distance(&self) -> f32 {
        self.data.fear_scan_distance
    }

    #[must_use]
    pub fn hero_scan_distance(&self) -> f32 {
        self.data.hero_scan_distance
    }

    #[must_use]
    pub fn afraid_of(&self) -> &ObjectFilter {
        &self.data.afraid_of
    }

    #[must_use]
    pub fn always_afraid_of(&self) -> &ObjectFilter {
        &self.data.always_afraid_of
    }

    #[must_use]
    pub fn point_at(&self) -> &ObjectFilter {
        &self.data.point_at
    }

    #[must_use]
    pub fn taunt_and_point_excluded(&self) -> &ObjectFilter {
        &self.data.taunt_and_point_excluded
    }

    #[must_use]
    pub fn quarrel_probability(&self) -> f32 {
        self.data.quarrel_probability
    }

    #[must_use]
    pub fn ignore_veterancy(&self) -> bool {
        self.data.ignore_veterancy
    }

    #[must_use]
    pub fn immune_to_fear_level(&self) -> u32 {
        self.data.immune_to_fear_level.unwrap_or(5)
    }

    #[must_use]
    pub fn emotions(&self) -> &[Block] {
        &self.data.emotions
    }

    #[must_use]
    pub const fn started(&self) -> bool {
        self.started
    }

    #[must_use]
    pub fn due(&self, now: Frame) -> bool {
        now >= self.next_scan
    }

    pub fn scanned(&mut self, now: Frame) {
        self.next_scan = now.checked_add(self.data.cadence).unwrap_or(Frame::FOREVER);
    }

    pub fn stagger(&mut self, entity: Entity, now: Frame) {
        let cadence = self.data.cadence.get().max(1);
        let offset = entity.index().checked_rem(cadence).unwrap_or(0);
        self.next_scan = now
            .checked_add(LogicFrames::from_frames(offset))
            .unwrap_or(now);
        self.started = true;
    }
}

impl Component for EmotionTrackerUpdate {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u64(self.next_scan.get());
        hasher.write_bool(self.started);
    }
}

impl ModuleKind for EmotionTrackerUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;

    fn update(&mut self, _ctx: &mut UpdateContext<'_>) -> mint_sim::module::Sleep {
        mint_sim::module::Sleep::Forever
    }

    fn attempt_upgrade(&mut self, _active: &UpgradeMask, _ctx: &mut mint_sim::UpgradeContext<'_>) {}

    fn would_upgrade(&self, _active: &UpgradeMask) -> bool {
        false
    }
}

fn distance(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<f32, FieldError> {
    match fields.field(keyword) {
        None => Ok(0.0),
        Some(raw) => fields
            .field_numeric(keyword)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a distance"))),
    }
}

fn filter(fields: &dyn ModuleFieldSource, keyword: &str) -> ObjectFilter {
    let tokens: Vec<String> = fields
        .field_args(keyword)
        .into_iter()
        .map(str::to_owned)
        .collect();
    if tokens.is_empty() {
        ObjectFilter::parse(&["NONE".to_owned()])
    } else {
        ObjectFilter::parse(&tokens)
    }
}

fn add_emotion_block(header: &[&str], body: Option<&dyn ModuleFieldSource>) -> Block {
    let children = body
        .into_iter()
        .flat_map(|source| {
            EMOTION_NUGGET_FIELDS.into_iter().filter_map(|keyword| {
                let args: Vec<String> = source
                    .field_args(keyword)
                    .into_iter()
                    .map(str::to_owned)
                    .collect();
                (!args.is_empty()).then(|| {
                    mint_sage_ini::Node::Field(Field {
                        keyword: keyword.to_owned(),
                        args,
                        file: Arc::from(mint_sage_ini::ADD_EMOTION_KEYWORD),
                        line: 0,
                    })
                })
            })
        })
        .collect();
    Block {
        keyword: mint_sage_ini::ADD_EMOTION_KEYWORD.to_owned(),
        args: header.iter().map(|token| (*token).to_owned()).collect(),
        children,
        file: Arc::from(mint_sage_ini::ADD_EMOTION_KEYWORD),
        line: 0,
    }
}

pub fn emotion_tracker_update_from_fields(
    fields: &dyn ModuleFieldSource,
    now: Frame,
) -> Result<EmotionTrackerUpdate, FieldError> {
    let cadence_ms = match fields.field("TauntAndPointUpdateDelay") {
        None => DEFAULT_SCAN_MS,
        Some(raw) => {
            let ms = fields
                .field_numeric("TauntAndPointUpdateDelay")
                .filter(|value| value.is_finite() && *value >= 0.0)
                .ok_or_else(|| {
                    FieldError::new(format!(
                        "TauntAndPointUpdateDelay '{raw}' is not a duration"
                    ))
                })?;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let whole = ms.min(86_400_000.0) as u32;
            whole
        }
    };
    let cadence = LogicFrames::from_milliseconds(cadence_ms).max(LogicFrames::ONE);
    let mut emotions: Vec<Block> = fields
        .field_args_each(mint_sage_ini::ADD_EMOTION_KEYWORD)
        .into_iter()
        .map(|header| add_emotion_block(&header, None))
        .collect();
    fields.nested(mint_sage_ini::ADD_EMOTION_KEYWORD, &mut |body| {
        let header = body.header_args();
        emotions.push(add_emotion_block(&header, Some(body)));
    });
    let flag = |keyword: &str| -> Result<bool, FieldError> {
        match fields.field(keyword) {
            None => Ok(false),
            Some(raw) => mint_sage_ini::value::parse_bool(raw)
                .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No"))),
        }
    };
    Ok(EmotionTrackerUpdate {
        data: Arc::new(TrackerData {
            cadence,
            taunt_and_point_distance: distance(fields, "TauntAndPointDistance")?,
            fear_scan_distance: distance(fields, "FearScanDistance")?,
            hero_scan_distance: distance(fields, "HeroScanDistance")?,
            afraid_of: filter(fields, "AfraidOf"),
            always_afraid_of: filter(fields, "AlwaysAfraidOf"),
            point_at: filter(fields, "PointAt"),
            taunt_and_point_excluded: filter(fields, "TauntAndPointExcluded"),
            quarrel_probability: fields
                .field_percent("QuarrelProbability")
                .filter(|value| value.is_finite() && *value >= 0.0)
                .unwrap_or(0.0),
            ignore_veterancy: flag("IgnoreVeterancy")?,
            immune_to_fear_level: fields
                .field_numeric("ImmuneToFearLevel")
                .filter(|value| (0.0..=f32::from(u16::MAX)).contains(value))
                .map(|value| {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let level = value as u32;
                    level
                }),
            emotions,
        }),
        next_scan: now,
        started: false,
    })
}

fn admits(filter: &ObjectFilter, standing: Standing, kind_of: &[String], name: &str) -> bool {
    filter.passes_relation(standing.same_player, standing.allied, standing.hostile)
        && filter.passes(kind_of, name)
}

#[must_use]
pub fn frightens(
    tracker: &EmotionTrackerUpdate,
    standing: Standing,
    scary: bool,
    kind_of: &[String],
    name: &str,
) -> bool {
    if !scary {
        return false;
    }
    let always = admits(tracker.always_afraid_of(), standing, kind_of, name);
    if (standing.allied || standing.same_player) && !always {
        return false;
    }
    always || admits(tracker.afraid_of(), standing, kind_of, name)
}

#[must_use]
pub fn points_at(
    tracker: &EmotionTrackerUpdate,
    standing: Standing,
    kind_of: &[String],
    name: &str,
) -> bool {
    [
        tracker.point_at(),
        tracker.afraid_of(),
        tracker.always_afraid_of(),
    ]
    .into_iter()
    .any(|filter| admits(filter, standing, kind_of, name))
}

const NEVER_TAUNTED: [&str; 5] = [
    "STRUCTURE",
    "HORDE",
    "UNATTACKABLE",
    "IMMOBILE",
    "IGNORED_IN_GUI",
];

#[must_use]
pub fn taunt_excluded(
    tracker: &EmotionTrackerUpdate,
    standing: Standing,
    kind_of: &[String],
    name: &str,
) -> bool {
    kind_of.iter().any(|kind| {
        NEVER_TAUNTED
            .iter()
            .any(|never| kind.eq_ignore_ascii_case(never))
    }) || admits(tracker.taunt_and_point_excluded(), standing, kind_of, name)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct TestFields {
        pairs: Vec<(&'static str, Vec<&'static str>)>,
        nested: Vec<(Vec<&'static str>, BTreeMap<&'static str, &'static str>)>,
    }

    struct NestedFields<'a>(&'a (Vec<&'static str>, BTreeMap<&'static str, &'static str>));

    impl ModuleFieldSource for NestedFields<'_> {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                 .1
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| *v)
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.field(keyword).into_iter().collect()
        }
        fn header_args(&self) -> Vec<&str> {
            self.0 .0.clone()
        }
    }

    impl ModuleFieldSource for TestFields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.field_args(keyword).first().copied()
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.pairs
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        }
        fn field_args_each(&self, keyword: &str) -> Vec<Vec<&str>> {
            self.pairs
                .iter()
                .filter(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| v.clone())
                .collect()
        }
        fn nested(&self, keyword: &str, visit: &mut dyn FnMut(&dyn ModuleFieldSource)) {
            if !keyword.eq_ignore_ascii_case(mint_sage_ini::ADD_EMOTION_KEYWORD) {
                return;
            }
            for entry in &self.nested {
                visit(&NestedFields(entry));
            }
        }
    }

    fn fields(pairs: &[(&'static str, &'static str)]) -> TestFields {
        TestFields {
            pairs: pairs
                .iter()
                .map(|(k, v)| (*k, v.split_whitespace().collect()))
                .collect(),
            nested: Vec::new(),
        }
    }

    #[test]
    fn a_shipped_shape_declaration_reads_its_fields() {
        let tracker = emotion_tracker_update_from_fields(
            &fields(&[
                ("TauntAndPointUpdateDelay", "1000"),
                ("TauntAndPointDistance", "100"),
                ("TauntAndPointExcluded", "NONE"),
                ("FearScanDistance", "100"),
                ("HeroScanDistance", "150"),
                ("AfraidOf", "NONE +MordorMountainTroll"),
                ("AlwaysAfraidOf", "NONE +MordorBalrog"),
                ("QuarrelProbability", "0.0002%"),
                ("AddEmotion", "Terror_Base"),
                ("ImmuneToFearLevel", "2"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(tracker.cadence().get(), 30, "1000 ms is 30 frames");
        assert_eq!(tracker.fear_scan_distance(), 100.0);
        assert_eq!(tracker.hero_scan_distance(), 150.0);
        assert_eq!(tracker.immune_to_fear_level(), 2);
        assert!(tracker.quarrel_probability() > 0.0);
        assert_eq!(tracker.emotions().len(), 1);
        assert_eq!(tracker.emotions()[0].args, ["Terror_Base"]);
    }

    #[test]
    fn a_distance_that_is_not_a_number_is_refused_rather_than_defaulted() {
        assert!(emotion_tracker_update_from_fields(
            &fields(&[("FearScanDistance", "far")]),
            Frame::ZERO
        )
        .is_err());
    }

    #[test]
    fn an_override_declaration_keeps_its_base_name_and_its_body() {
        let mut source = fields(&[("AddEmotion", "Terror_Base")]);
        source.nested.push((
            vec!["OVERRIDE", "UncontrollableFear_Base_Evil"],
            [("Duration", "7000")].into_iter().collect(),
        ));
        let tracker = emotion_tracker_update_from_fields(&source, Frame::ZERO).unwrap();
        assert_eq!(tracker.emotions().len(), 2);
        let inline = &tracker.emotions()[1];
        assert_eq!(inline.args, ["OVERRIDE", "UncontrollableFear_Base_Evil"]);
        assert_eq!(
            inline.field("Duration").and_then(Field::value),
            Some("7000")
        );
    }

    #[test]
    fn an_ally_frightens_only_through_always_afraid_of() {
        let tracker = emotion_tracker_update_from_fields(
            &fields(&[
                ("FearScanDistance", "100"),
                ("AfraidOf", "NONE +ZTroll"),
                ("AlwaysAfraidOf", "NONE +ZBalrog"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let troll = ["MONSTER".to_owned()];
        let ally = Standing {
            same_player: true,
            allied: true,
            hostile: false,
        };
        assert!(frightens(&tracker, Standing::ENEMY, true, &troll, "ZTroll"));
        assert!(!frightens(
            &tracker,
            Standing::ENEMY,
            false,
            &troll,
            "ZTroll"
        ));
        assert!(!frightens(
            &tracker,
            Standing::ENEMY,
            true,
            &troll,
            "ZFighter"
        ));
        assert!(!frightens(&tracker, ally, true, &troll, "ZTroll"));
        assert!(frightens(&tracker, ally, true, &troll, "ZBalrog"));
    }

    #[test]
    fn staggering_spreads_first_scans_over_one_cadence() {
        let tracker = emotion_tracker_update_from_fields(
            &fields(&[("TauntAndPointUpdateDelay", "1000")]),
            Frame::ZERO,
        )
        .unwrap();
        let mut due = std::collections::BTreeSet::new();
        let mut world = mint_sim::World::new();
        for _ in 0..30u32 {
            let entity = world.spawn().unwrap();
            let mut one = tracker.clone();
            one.stagger(entity, Frame::ZERO);
            let mut at = 0u64;
            while !one.due(Frame::new(at)) {
                at = at.saturating_add(1);
            }
            due.insert(at);
        }
        assert_eq!(due.len(), 30, "30 trackers, 30 distinct first-scan frames");
    }
}
