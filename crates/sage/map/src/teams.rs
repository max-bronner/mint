use crate::container::{NameTable, RawChunk};
use crate::dictionary::{read_dictionary, Dictionary};
use crate::reader::Reader;
use crate::{Error, Result};

pub const TEAMS_CHUNK: &str = "Teams";

pub const TEAM_UNIT_SLOTS: usize = 7;

pub const TEAM_GENERIC_HOOKS: usize = 32;

const MIN_DICT_BYTES: usize = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct TeamDef {
    pub owner: String,
    pub name: String,
    pub singleton: bool,
    pub properties: Dictionary,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnitSlot {
    pub min: i32,
    pub max: i32,
    pub experience_level: i32,
    pub upgrades: Vec<String>,
    pub template: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TeamTemplate {
    pub units: Vec<UnitSlot>,
    pub home: Option<String>,
    pub on_create_script: Option<String>,
    pub on_idle_script: Option<String>,
    pub enemy_sighted_script: Option<String>,
    pub all_clear_script: Option<String>,
    pub on_destroyed_script: Option<String>,
    pub on_unit_destroyed_script: Option<String>,
    pub production_condition: Option<String>,
    pub events_list: Option<String>,
    pub ai_recruitable: bool,
    pub base_defense: bool,
    pub perimeter_defense: bool,
    pub auto_reinforce: bool,
    pub transports_return: bool,
    pub avoid_threats: bool,
    pub attack_common_target: bool,
    pub executes_actions_on_create: bool,
    pub aggressiveness: i32,
    pub max_instances: i32,
    pub initial_idle_seconds: i32,
    pub destroyed_threshold: f32,
    pub production_priority: i32,
    pub production_priority_success_increase: i32,
    pub production_priority_failure_decrease: i32,
    pub generic_hooks: [Option<String>; TEAM_GENERIC_HOOKS],
    pub transport: Option<String>,
    pub transports_exit: bool,
    pub starts_full: bool,
    pub reinforcement_origin: Option<String>,
    pub veterancy: i32,
    pub team_type: i32,
}

#[must_use]
pub fn split_team_ref<'a>(param: &'a str, default_owner: &'a str) -> (&'a str, &'a str) {
    param.split_once('/').unwrap_or((default_owner, param))
}

fn non_empty(text: Option<&str>) -> Option<String> {
    text.filter(|t| !t.is_empty()).map(str::to_string)
}

impl TeamDef {
    fn from_properties(properties: Dictionary) -> Result<Self> {
        let name = properties
            .get_str(TEAMS_CHUNK, "teamName")?
            .ok_or(Error::Malformed {
                name: TEAMS_CHUNK,
                reason: "a team dictionary without teamName",
            })?
            .to_string();
        let owner = properties
            .get_str(TEAMS_CHUNK, "teamOwner")?
            .unwrap_or_default()
            .to_string();
        let singleton = properties
            .get_bool(TEAMS_CHUNK, "teamIsSingleton")?
            .unwrap_or(false);
        Ok(Self {
            owner,
            name,
            singleton,
            properties,
        })
    }

    pub fn template(&self) -> Result<TeamTemplate> {
        let d = &self.properties;
        let int = |key: &str| d.get_int(TEAMS_CHUNK, key).map(Option::unwrap_or_default);
        let flag = |key: &str| d.get_bool(TEAMS_CHUNK, key).map(Option::unwrap_or_default);
        let text = |key: &str| d.get_str(TEAMS_CHUNK, key).map(non_empty);

        let mut units = Vec::new();
        for slot in 1..=TEAM_UNIT_SLOTS {
            let min = int(&format!("teamUnitMinCount{slot}"))?;
            let max = int(&format!("teamUnitMaxCount{slot}"))?;
            let template = d.get_str(TEAMS_CHUNK, &format!("teamUnitType{slot}"))?;
            if let (true, Some(template)) = (max > 0, template) {
                let upgrades = d
                    .get_str(TEAMS_CHUNK, &format!("teamUnitUpgradeList{slot}"))?
                    .unwrap_or_default()
                    .split_whitespace()
                    .map(str::to_string)
                    .collect();
                units.push(UnitSlot {
                    min,
                    max,
                    experience_level: int(&format!("teamUnitExperienceLevel{slot}"))?,
                    upgrades,
                    template: template.to_string(),
                });
            }
        }

        let mut generic_hooks: [Option<String>; TEAM_GENERIC_HOOKS] = Default::default();
        for (index, hook) in generic_hooks.iter_mut().enumerate() {
            *hook = text(&format!("teamGenericScriptHook{index}"))?;
        }

        Ok(TeamTemplate {
            units,
            home: text("teamHome")?,
            on_create_script: text("teamOnCreateScript")?,
            on_idle_script: text("teamOnIdleScript")?,
            enemy_sighted_script: text("teamEnemySightedScript")?,
            all_clear_script: text("teamAllClearScript")?,
            on_destroyed_script: text("teamOnDestroyedScript")?,
            on_unit_destroyed_script: text("teamOnUnitDestroyedScript")?,
            production_condition: text("teamProductionCondition")?,
            events_list: text("teamEventsList")?,
            ai_recruitable: flag("teamIsAIRecruitable")?,
            base_defense: flag("teamIsBaseDefense")?,
            perimeter_defense: flag("teamIsPerimeterDefense")?,
            auto_reinforce: flag("teamAutoReinforce")?,
            transports_return: flag("teamTransportsReturn")?,
            avoid_threats: flag("teamAvoidThreats")?,
            attack_common_target: flag("teamAttackCommonTarget")?,
            executes_actions_on_create: flag("teamExecutesActionsOnCreate")?,
            aggressiveness: int("teamAggressiveness")?,
            max_instances: int("teamMaxInstances")?,
            initial_idle_seconds: int("teamInitialIdleSeconds")?,
            destroyed_threshold: d
                .get_float(TEAMS_CHUNK, "teamDestroyedThreshold")?
                .unwrap_or_default(),
            production_priority: int("teamProductionPriority")?,
            production_priority_success_increase: int("teamProductionPrioritySuccessIncrease")?,
            production_priority_failure_decrease: int("teamProductionPriorityFailureDecrease")?,
            generic_hooks,
            transport: text("teamTransport")?,
            transports_exit: flag("teamTransportsExit")?,
            starts_full: flag("teamStartsFull")?,
            reinforcement_origin: text("teamReinforcementOrigin")?,
            veterancy: int("teamVeterancy")?,
            team_type: int("teamType")?.max(0),
        })
    }
}

pub(crate) fn read_teams(body: &[u8], chunk: &RawChunk, names: &NameTable) -> Result<Vec<TeamDef>> {
    let payload = chunk.payload(body)?;
    let mut r = Reader::new_at(payload, chunk.body);
    let declared = r.u32()?;
    let count = usize::try_from(declared).unwrap_or(usize::MAX);
    if count.saturating_mul(MIN_DICT_BYTES) > r.remaining() {
        return Err(Error::CountExceedsPayload {
            chunk: TEAMS_CHUNK,
            offset: r.position(),
            count: declared,
            item_size: MIN_DICT_BYTES,
            available: r.remaining(),
        });
    }
    let mut teams = Vec::new();
    for _ in 0..count {
        teams.push(TeamDef::from_properties(read_dictionary(&mut r, names)?)?);
    }
    if r.remaining() != 0 {
        return Err(Error::TrailingBytes {
            name: TEAMS_CHUNK,
            offset: r.position(),
            remaining: r.remaining(),
        });
    }
    Ok(teams)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::testutil::MapBuilder;

    fn read(payload: &[u8], names: &NameTable) -> Result<Vec<TeamDef>> {
        let chunk = RawChunk {
            name: TEAMS_CHUNK.into(),
            version: 1,
            body: 0,
            len: payload.len(),
        };
        read_teams(payload, &chunk, names)
    }

    fn chunk_of(b: &MapBuilder, dicts: &[Vec<Vec<u8>>]) -> Vec<u8> {
        let mut payload = u32::try_from(dicts.len()).unwrap().to_le_bytes().to_vec();
        for entries in dicts {
            payload.extend_from_slice(&b.dict(entries.clone()));
        }
        payload
    }

    fn identity(b: &mut MapBuilder, owner: &str, name: &str) -> Vec<Vec<u8>> {
        vec![
            b.dict_str("teamName", name),
            b.dict_str("teamOwner", owner),
            b.dict_bool("teamIsSingleton", false),
        ]
    }

    fn template_of(b: &mut MapBuilder, mut entries: Vec<Vec<u8>>) -> TeamTemplate {
        entries.extend(identity(b, "Owner", "Squad"));
        let payload = chunk_of(b, &[entries]);
        let teams = read(&payload, &b.name_table()).unwrap();
        teams[0].template().unwrap()
    }

    #[test]
    fn identity_is_the_owner_name_pair() {
        let mut b = MapBuilder::new();
        let dicts = [
            identity(&mut b, "North", "Squad"),
            identity(&mut b, "", "Squad"),
        ];
        let payload = chunk_of(&b, &dicts);
        let teams = read(&payload, &b.name_table()).unwrap();
        assert_eq!(teams.len(), 2);
        assert_eq!(
            (teams[0].owner.as_str(), teams[0].name.as_str()),
            ("North", "Squad")
        );
        assert_eq!(
            (teams[1].owner.as_str(), teams[1].name.as_str()),
            ("", "Squad")
        );
    }

    #[test]
    fn unit_slots_need_a_positive_max_and_a_type_and_close_up() {
        let mut b = MapBuilder::new();
        let mut entries = Vec::new();
        for (slot, max, ty) in [
            (1, 2, Some("Alpha")),
            (2, 0, Some("Beta")),
            (3, 4, Some("Gamma")),
            (4, 3, None),
        ] {
            entries.push(b.dict_int(&format!("teamUnitMinCount{slot}"), 1));
            entries.push(b.dict_int(&format!("teamUnitMaxCount{slot}"), max));
            if let Some(ty) = ty {
                entries.push(b.dict_str(&format!("teamUnitType{slot}"), ty));
            }
        }
        entries.push(b.dict_str("teamUnitUpgradeList3", "UpA  UpB"));
        let t = template_of(&mut b, entries);
        let kept: Vec<_> = t.units.iter().map(|u| u.template.as_str()).collect();
        assert_eq!(kept, ["Alpha", "Gamma"]);
        assert_eq!(t.units[1].upgrades, ["UpA", "UpB"]);
    }

    #[test]
    fn hooks_run_from_zero_to_thirty_one() {
        let mut b = MapBuilder::new();
        let entries = vec![
            b.dict_str("teamGenericScriptHook31", "Last"),
            b.dict_str("teamGenericScriptHook32", "Beyond"),
            b.dict_str("teamGenericScriptHook0", ""),
        ];
        let t = template_of(&mut b, entries);
        assert_eq!(t.generic_hooks[31].as_deref(), Some("Last"));
        assert_eq!(t.generic_hooks[0], None);
        assert_eq!(t.generic_hooks.iter().flatten().count(), 1);
    }

    #[test]
    fn a_negative_team_type_reads_as_zero() {
        let mut b = MapBuilder::new();
        let entries = vec![b.dict_int("teamType", -1)];
        assert_eq!(template_of(&mut b, entries).team_type, 0);
    }

    #[test]
    fn a_wrong_typed_known_key_is_an_error() {
        let mut b = MapBuilder::new();
        let mut entries = identity(&mut b, "Owner", "Squad");
        entries.push(b.dict_str("teamMaxInstances", "three"));
        let payload = chunk_of(&b, &[entries]);
        let teams = read(&payload, &b.name_table()).unwrap();
        assert!(matches!(
            teams[0].template(),
            Err(Error::WrongValueType { .. })
        ));
    }

    #[test]
    fn team_references_split_at_the_first_slash() {
        assert_eq!(split_team_ref("North/Squad", "Me"), ("North", "Squad"));
        assert_eq!(split_team_ref("Squad", "Me"), ("Me", "Squad"));
        assert_eq!(
            split_team_ref("North/Squad/Two", "Me"),
            ("North", "Squad/Two")
        );
    }

    fn sample() -> (Vec<u8>, NameTable) {
        let mut b = MapBuilder::new();
        let mut first = identity(&mut b, "North", "Squad");
        first.push(b.dict_int("teamAggressiveness", 1));
        let second = identity(&mut b, "", "Wolves");
        (chunk_of(&b, &[first, second]), b.name_table())
    }

    #[test]
    fn truncation_errors_rather_than_panicking() {
        let (payload, names) = sample();
        for len in 0..payload.len() {
            assert!(read(&payload[..len], &names).is_err(), "prefix {len}");
        }
    }

    #[test]
    fn corruption_never_panics() {
        let (payload, names) = sample();
        for at in 0..payload.len() {
            for flip in [0x01u8, 0x80, 0xff] {
                let mut bad = payload.clone();
                bad[at] ^= flip;
                let _ = read(&bad, &names).map(|t| t.iter().map(TeamDef::template).count());
            }
        }
    }

    #[test]
    fn a_count_larger_than_the_bytes_present_is_refused_before_allocating() {
        let (mut payload, names) = sample();
        payload[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            read(&payload, &names),
            Err(Error::CountExceedsPayload { .. })
        ));
    }
}
