use mint_render::Vec3;
use mint_sim::{Frame, Simulation};

use crate::death_phase::DeathPhaseEvent;
use crate::fx::{FxEvent, FxSource};
use crate::rules::RotwkRules;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeathList {
    Fx,
    Sound,
}

pub(crate) fn death_phase_pick<'a>(
    lists: &'a [String],
    event: &DeathPhaseEvent,
    frame: Frame,
    list: DeathList,
) -> Option<&'a str> {
    let count = u64::try_from(lists.len()).ok().filter(|count| *count > 0)?;
    let salt = u32::try_from(event.phase.index())
        .unwrap_or(0)
        .wrapping_mul(2)
        .wrapping_add(u32::from(list == DeathList::Sound));
    let low = u32::try_from(frame.get() & u64::from(u32::MAX)).unwrap_or(0);
    let draw = mint_sage_scene::animation_state::variant(event.entity.index(), low, salt)
        .checked_rem(count)?;
    lists.get(usize::try_from(draw).ok()?).map(String::as_str)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum FxShown {
    #[default]
    All,
    Spells,
    Off,
}

impl FxShown {
    pub(crate) const ALL: [Self; 3] = [Self::All, Self::Spells, Self::Off];

    pub(crate) fn next(self) -> Self {
        match self {
            Self::All => Self::Spells,
            Self::Spells => Self::Off,
            Self::Off => Self::All,
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::All => "spells and weapons",
            Self::Spells => "spells only",
            Self::Off => "off",
        }
    }

    pub(crate) fn rung(self) -> usize {
        Self::ALL
            .iter()
            .position(|shown| *shown == self)
            .map_or(0, |at| at.saturating_add(1))
    }

    const fn admits(self, source: FxSource) -> bool {
        match self {
            Self::All => true,
            Self::Spells => matches!(source, FxSource::Power),
            Self::Off => false,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct FxPresenter {
    shown: FxShown,
    pending: Vec<FxEvent>,
    last: Option<Frame>,
    pub(crate) fired: u64,
    pub(crate) systems: u64,
    pub(crate) unknown: u64,
    pub(crate) at_bone_pos: u64,
    pub(crate) bone_found: u64,
    pub(crate) bone_missing: u64,
    pub(crate) nested_list_missing: u64,
    pub(crate) camera_shakes: u64,
    pub(crate) anim_fired: u64,
    pub(crate) anim_systems: u64,
    pub(crate) anim_unknown: u64,
    pub(crate) shrouded: u64,
}

impl FxPresenter {
    pub(crate) const fn shown(&self) -> FxShown {
        self.shown
    }

    pub(crate) fn cycle(&mut self) -> FxShown {
        self.shown = self.shown.next();
        self.shown
    }

    pub(crate) fn collect(&mut self, simulation: &Simulation<RotwkRules>) -> &[FxEvent] {
        let before = self.pending.len();
        let ran = simulation.frame().get().checked_sub(1).map(Frame::new);
        if let Some(frame) = ran.filter(|frame| self.last != Some(*frame)) {
            self.last = Some(frame);
            let shown = self.shown;
            let rules = simulation.rules();
            let events = rules.fx_events(simulation.state(), frame);
            self.pending
                .extend(events.iter().filter(|event| shown.admits(event.source)));
            for event in rules.death_phase_events(simulation.state(), frame) {
                let (lists, _) = rules.death_phase_lists(event);
                let Some(name) = death_phase_pick(lists, event, frame, DeathList::Fx) else {
                    continue;
                };
                let Some(list) = rules.fx_id(name) else {
                    continue;
                };
                crate::diag::diag!(
                    "death: entity {} {} plays fx {name}",
                    event.entity.index(),
                    event.phase.label()
                );
                let fx = FxEvent {
                    list,
                    x: event.at.x,
                    y: event.at.y,
                    z: event.at.z,
                    source: FxSource::Weapon,
                    caller: event.entity,
                    target: None,
                    victim: None,
                    on_object: true,
                };
                if shown.admits(fx.source) {
                    self.pending.push(fx);
                }
            }
        }
        self.pending.get(before..).unwrap_or_default()
    }

    pub(crate) fn take(&mut self) -> impl Iterator<Item = FxEvent> + '_ {
        self.pending.drain(..)
    }
}

pub(crate) fn census_line(
    particles: &mint_sage_scene::ParticleScene,
    rules: &RotwkRules,
    shown: FxShown,
) -> String {
    let names = rules.fx_list_names().count();
    let (found, with_particles, with_sounds) = particles.list_census(rules.fx_list_names());
    format!(
        "effects: {found} of {names} FXList name(s) the match can play are declared, \
         {with_particles} with a ParticleSystem nugget and {with_sounds} with a Sound; other \
         nugget kinds are not played [{}, {} of {}; `,` cycles]",
        shown.label(),
        shown.rung(),
        FxShown::ALL.len()
    )
}

pub(crate) fn place_of(event: &FxEvent, ground: impl Fn(f32, f32) -> f32) -> Vec3 {
    let z = event.z.unwrap_or_else(|| ground(event.x, event.y));
    Vec3::new(event.x, event.y, z)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Facts<'a> {
    pub(crate) template: &'a str,
    pub(crate) kind_of: &'a [String],
    pub(crate) model_conditions: &'a [String],
    pub(crate) invisible_to_local: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ObjectFacts {
    pub(crate) template: String,
    pub(crate) kind_of: Vec<String>,
    pub(crate) model_conditions: Vec<String>,
    pub(crate) invisible_to_local: bool,
}

impl ObjectFacts {
    pub(crate) fn as_facts(&self) -> Facts<'_> {
        Facts {
            template: &self.template,
            kind_of: &self.kind_of,
            model_conditions: &self.model_conditions,
            invisible_to_local: self.invisible_to_local,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct NuggetMask(Vec<bool>);

impl NuggetMask {
    pub(crate) fn admits(&self, index: usize) -> bool {
        self.0.get(index).copied().unwrap_or(false)
    }
}

pub(crate) fn admit(
    nuggets: &[mint_sage_ini::FxNugget],
    primary: Option<&Facts<'_>>,
    secondary: Option<&Facts<'_>>,
    weather: mint_sage_ini::NuggetWeather,
    equivalent: &dyn Fn(&str, &str) -> bool,
) -> NuggetMask {
    let mut mask = vec![false; nuggets.len()];
    for (index, nugget) in nuggets.iter().enumerate() {
        if !precondition(&nugget.conditions, primary, secondary, weather, equivalent) {
            continue;
        }
        if let Some(admitted) = mask.get_mut(index) {
            *admitted = true;
        }
        if nugget.conditions.stop_if_played {
            break;
        }
    }
    NuggetMask(mask)
}

fn precondition(
    conditions: &mint_sage_ini::NuggetConditions,
    primary: Option<&Facts<'_>>,
    secondary: Option<&Facts<'_>>,
    weather: mint_sage_ini::NuggetWeather,
    equivalent: &dyn Fn(&str, &str) -> bool,
) -> bool {
    if let Some(primary) = primary {
        if !passes_filter(conditions.source_filter.as_deref(), primary, equivalent)
            || !holds_model_conditions(
                primary.model_conditions,
                &conditions.required_source,
                &conditions.excluded_source,
            )
            || primary.invisible_to_local
        {
            return false;
        }
    }
    if let Some(secondary) = secondary {
        if !passes_filter(conditions.object_filter.as_deref(), secondary, equivalent)
            || !holds_model_conditions(
                secondary.model_conditions,
                &conditions.required_secondary,
                &conditions.excluded_secondary,
            )
        {
            return false;
        }
    }
    mint_sage_ini::weather_gate_admits(conditions.weather, weather)
}

fn passes_filter(
    filter: Option<&[String]>,
    facts: &Facts<'_>,
    equivalent: &dyn Fn(&str, &str) -> bool,
) -> bool {
    filter.is_none_or(|tokens| {
        mint_sage_modules::ObjectFilter::parse(tokens).passes_equiv(
            facts.kind_of,
            facts.template,
            equivalent,
        )
    })
}

fn holds_model_conditions(held: &[String], required: &[String], excluded: &[String]) -> bool {
    let has = |name: &str| held.iter().any(|flag| flag.eq_ignore_ascii_case(name));
    required.iter().all(|flag| has(flag)) && !excluded.iter().any(|flag| has(flag))
}

pub(crate) fn eva_event_name(
    nugget: &mint_sage_ini::FxEvaEvent,
    on_object: bool,
    play_even_if_shrouded: Option<bool>,
    visible: bool,
    owner: Option<mint_sim::PlayerId>,
    local: mint_sim::PlayerId,
    allied: impl FnOnce(mint_sim::PlayerId, mint_sim::PlayerId) -> bool,
) -> Option<&str> {
    if !on_object || (play_even_if_shrouded != Some(true) && !visible) {
        return None;
    }
    match owner {
        Some(owner) if owner == local => nugget.owner.as_deref(),
        Some(owner) if allied(local, owner) => nugget.ally.as_deref(),
        _ => nugget.enemy.as_deref(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod eva_event_tests {
    use super::eva_event_name;
    use mint_sage_ini::FxEvaEvent;
    use mint_sim::PlayerId;

    fn nugget() -> FxEvaEvent {
        FxEvaEvent {
            owner: Some("EVA_Owner".to_owned()),
            ally: Some("EVA_Ally".to_owned()),
            enemy: Some("EVA_Enemy".to_owned()),
        }
    }

    const LOCAL: PlayerId = PlayerId::new(0);
    const OTHER: PlayerId = PlayerId::new(1);

    #[test]
    fn the_variant_is_picked_by_the_owners_relation_to_local() {
        assert_eq!(
            eva_event_name(&nugget(), true, None, true, Some(LOCAL), LOCAL, |_, _| {
                false
            }),
            Some("EVA_Owner")
        );
        assert_eq!(
            eva_event_name(&nugget(), true, None, true, Some(OTHER), LOCAL, |_, _| true),
            Some("EVA_Ally")
        );
        assert_eq!(
            eva_event_name(&nugget(), true, None, true, Some(OTHER), LOCAL, |_, _| {
                false
            }),
            Some("EVA_Enemy")
        );
    }

    #[test]
    fn invisible_skips_unless_the_list_overrides_it() {
        assert_eq!(
            eva_event_name(&nugget(), true, None, false, Some(LOCAL), LOCAL, |_, _| {
                false
            }),
            None
        );
        assert_eq!(
            eva_event_name(
                &nugget(),
                true,
                Some(true),
                false,
                Some(LOCAL),
                LOCAL,
                |_, _| false
            ),
            Some("EVA_Owner")
        );
    }

    #[test]
    fn a_position_play_reports_nothing() {
        assert_eq!(
            eva_event_name(&nugget(), false, None, true, Some(LOCAL), LOCAL, |_, _| {
                false
            }),
            None
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod admit_tests {
    use super::{admit, Facts};
    use mint_sage_ini::{FxNugget, NuggetConditions, NuggetWeather};

    fn nugget(conditions: NuggetConditions) -> FxNugget {
        FxNugget {
            keyword: "Sound".to_owned(),
            name: None,
            conditions,
            particle: None,
            at_bone_pos: None,
            camera_shaker: None,
            eva_event: None,
        }
    }

    fn facts<'a>(template: &'a str, kind_of: &'a [String]) -> Facts<'a> {
        Facts {
            template,
            kind_of,
            model_conditions: &[],
            invisible_to_local: false,
        }
    }

    #[test]
    fn required_source_model_condition_gates_the_primary() {
        let list = [nugget(NuggetConditions {
            required_source: vec!["DISGUISED".to_owned()],
            ..Default::default()
        })];
        let held = [String::from("DISGUISED")];
        let primary = Facts {
            model_conditions: &held,
            ..facts("Eowyn", &[])
        };
        assert!(admit(
            &list,
            Some(&primary),
            None,
            NuggetWeather::Normal,
            &|_, _| false
        )
        .admits(0));
        let bare = facts("Eowyn", &[]);
        assert!(
            !admit(&list, Some(&bare), None, NuggetWeather::Normal, &|_, _| {
                false
            })
            .admits(0)
        );
    }

    #[test]
    fn excluded_source_model_condition_gates_the_primary() {
        let list = [nugget(NuggetConditions {
            excluded_source: vec!["DEAD".to_owned()],
            ..Default::default()
        })];
        let held = [String::from("DEAD")];
        let primary = Facts {
            model_conditions: &held,
            ..facts("Eowyn", &[])
        };
        assert!(!admit(
            &list,
            Some(&primary),
            None,
            NuggetWeather::Normal,
            &|_, _| false
        )
        .admits(0));
    }

    #[test]
    fn source_object_filter_gates_the_primary_by_kind_of() {
        let list = [nugget(NuggetConditions {
            source_filter: Some(vec!["NONE".to_owned(), "+INFANTRY".to_owned()]),
            ..Default::default()
        })];
        let infantry = [String::from("INFANTRY")];
        assert!(admit(
            &list,
            Some(&facts("Soldier", &infantry)),
            None,
            NuggetWeather::Normal,
            &|_, _| false,
        )
        .admits(0));
        assert!(!admit(
            &list,
            Some(&facts("Farm", &[])),
            None,
            NuggetWeather::Normal,
            &|_, _| false,
        )
        .admits(0));
    }

    #[test]
    fn a_loose_source_filter_name_widens_through_the_equivalent_lookup() {
        let list = [nugget(NuggetConditions {
            source_filter: Some(vec!["NONE".to_owned(), "+GoblinFighter".to_owned()]),
            ..Default::default()
        })];
        let variant = facts("GoblinFighterVariant", &[]);
        let equivalent = |a: &str, b: &str| {
            a.eq_ignore_ascii_case("GoblinFighter")
                && b.eq_ignore_ascii_case("GoblinFighterVariant")
        };
        assert!(admit(
            &list,
            Some(&variant),
            None,
            NuggetWeather::Normal,
            &equivalent
        )
        .admits(0));
        assert!(
            !admit(
                &list,
                Some(&variant),
                None,
                NuggetWeather::Normal,
                &|_, _| false
            )
            .admits(0),
            "with no equivalence lookup the name must match exactly"
        );
    }

    #[test]
    fn object_filter_gates_the_secondary_and_a_null_one_skips_it() {
        let list = [nugget(NuggetConditions {
            object_filter: Some(vec!["NONE".to_owned(), "+HERO".to_owned()]),
            ..Default::default()
        })];
        let hero = [String::from("HERO")];
        assert!(admit(
            &list,
            None,
            Some(&facts("Aragorn", &hero)),
            NuggetWeather::Normal,
            &|_, _| false,
        )
        .admits(0));
        assert!(!admit(
            &list,
            None,
            Some(&facts("Soldier", &[])),
            NuggetWeather::Normal,
            &|_, _| false,
        )
        .admits(0));
        assert!(
            admit(&list, None, None, NuggetWeather::Normal, &|_, _| false).admits(0),
            "no secondary: ObjectFilter is not evaluated"
        );
    }

    #[test]
    fn a_position_play_admits_a_conditioned_nugget_but_still_reads_weather() {
        let conditioned = [nugget(NuggetConditions {
            source_filter: Some(vec!["NONE".to_owned()]),
            required_source: vec!["ANYTHING".to_owned()],
            ..Default::default()
        })];
        assert!(
            admit(&conditioned, None, None, NuggetWeather::Normal, &|_, _| {
                false
            })
            .admits(0)
        );
        let weathered = [nugget(NuggetConditions {
            weather: Some(NuggetWeather::Snowy),
            ..Default::default()
        })];
        assert!(!admit(&weathered, None, None, NuggetWeather::Normal, &|_, _| false).admits(0));
        assert!(admit(&weathered, None, None, NuggetWeather::Snowy, &|_, _| false).admits(0));
    }

    #[test]
    fn stop_if_nugget_played_stops_after_an_admitted_nugget_not_a_rejected_one() {
        let list = [
            nugget(NuggetConditions {
                stop_if_played: true,
                ..Default::default()
            }),
            nugget(NuggetConditions::default()),
        ];
        let mask = admit(&list, None, None, NuggetWeather::Normal, &|_, _| false);
        assert!(mask.admits(0));
        assert!(!mask.admits(1), "the first nugget played and stopped it");

        let rejecting = [
            nugget(NuggetConditions {
                weather: Some(NuggetWeather::Snowy),
                stop_if_played: true,
                ..Default::default()
            }),
            nugget(NuggetConditions::default()),
        ];
        let mask = admit(&rejecting, None, None, NuggetWeather::Normal, &|_, _| false);
        assert!(!mask.admits(0), "Weather rejected it");
        assert!(mask.admits(1), "a rejected nugget must not stop the walk");
    }

    #[test]
    fn a_stealth_invisible_primary_admits_nothing() {
        let list = [nugget(NuggetConditions::default())];
        let invisible = Facts {
            invisible_to_local: true,
            ..facts("Spy", &[])
        };
        assert!(!admit(
            &list,
            Some(&invisible),
            None,
            NuggetWeather::Normal,
            &|_, _| false
        )
        .admits(0));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_rungs_cycle_and_filter_as_labelled() {
        let mut shown = FxShown::default();
        assert_eq!(shown, FxShown::All);
        assert!(shown.admits(FxSource::Weapon));
        shown = shown.next();
        assert_eq!((shown.label(), shown.rung()), ("spells only", 2));
        assert!(shown.admits(FxSource::Power));
        assert!(!shown.admits(FxSource::Weapon));
        shown = shown.next();
        assert!(!shown.admits(FxSource::Power));
        assert_eq!(shown.next(), FxShown::All);
    }

    #[test]
    fn an_event_plays_at_its_height_else_on_the_ground() {
        let mut world = mint_sim::World::new();
        let caller = world.spawn().unwrap();
        let at = crate::fx::FxPoint::from(mint_sage_combat::Point3::new(10.0, 20.0, 309.6));
        let mut event = FxEvent {
            list: crate::fx::FxId(0),
            x: at.x,
            y: at.y,
            z: at.z,
            source: FxSource::Weapon,
            caller,
            target: None,
            victim: None,
            on_object: false,
        };
        let ground = |_: f32, _: f32| 300.0;
        assert_eq!(place_of(&event, ground), Vec3::new(10.0, 20.0, 309.6));
        event.z = crate::fx::FxPoint::from(mint_movement::Vec2::new(10.0, 20.0)).z;
        assert_eq!(place_of(&event, ground), Vec3::new(10.0, 20.0, 300.0));
    }
}
