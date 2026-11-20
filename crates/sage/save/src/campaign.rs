use crate::xfer::{AsciiString, Reader, WideString, Writer};
use crate::Result;

pub const CHUNK_CAMPAIGN: &[u8] = b"CHUNK_Campaign";

const MANAGER_VERSION: u8 = 1;
const STATE_VERSION: u8 = 6;
const RECORD_VERSION: u8 = 1;
const SUB_VERSION: u8 = 5;
const LIST_VERSION: u8 = 1;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpgradeList(pub Vec<AsciiString>);

impl UpgradeList {
    pub(crate) fn read(r: &mut Reader<'_>) -> Result<Self> {
        r.version("UpgradeMask", LIST_VERSION, LIST_VERSION)?;
        let n = r.u16()?;
        let mut names = Vec::new();
        for _ in 0..n {
            names.push(r.ascii()?);
        }
        Ok(Self(names))
    }

    pub(crate) fn write(&self, w: &mut Writer) {
        w.u8(LIST_VERSION);
        w.u16(u16::try_from(self.0.len()).unwrap_or(u16::MAX));
        for name in &self.0 {
            w.ascii(name);
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CarriedSub {
    pub missions: i32,
    pub kills_total: i32,
    pub kills_mission: i32,
    pub text: WideString,
    pub label: AsciiString,
    pub flag: bool,
}

impl CarriedSub {
    pub(crate) fn read(r: &mut Reader<'_>) -> Result<Self> {
        let version = r.version("CarriedSub", 1, SUB_VERSION)?;
        let missions = r.i32()?;
        let kills_total = r.i32()?;
        let kills_mission = r.i32()?;
        if version < 5 {
            r.i32()?;
        }
        let text = if version < 3 {
            WideString(r.ascii()?.0.into_iter().map(u16::from).collect())
        } else {
            r.wide()?
        };
        let label = if version >= 2 {
            r.ascii()?
        } else {
            AsciiString::default()
        };
        let flag = if version >= 4 { r.bool()? } else { false };
        Ok(Self {
            missions,
            kills_total,
            kills_mission,
            text,
            label,
            flag,
        })
    }

    pub(crate) fn write(&self, w: &mut Writer) {
        w.u8(SUB_VERSION);
        w.i32(self.missions);
        w.i32(self.kills_total);
        w.i32(self.kills_mission);
        w.wide(&self.text);
        w.ascii(&self.label);
        w.bool(self.flag);
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CarryoverUnit {
    pub template_name: AsciiString,
    pub count: i32,
    pub experience: f32,
    pub carried: CarriedSub,
    pub upgrades: UpgradeList,
    pub level: i32,
}

impl CarryoverUnit {
    pub(crate) fn read(r: &mut Reader<'_>) -> Result<Self> {
        r.version("CarryoverUnit", RECORD_VERSION, RECORD_VERSION)?;
        Ok(Self {
            template_name: r.ascii()?,
            count: r.i32()?,
            experience: r.f32()?,
            carried: CarriedSub::read(r)?,
            upgrades: UpgradeList::read(r)?,
            level: r.i32()?,
        })
    }

    pub(crate) fn write(&self, w: &mut Writer) {
        w.u8(RECORD_VERSION);
        w.ascii(&self.template_name);
        w.i32(self.count);
        w.f32(self.experience);
        self.carried.write(w);
        self.upgrades.write(w);
        w.i32(self.level);
    }
}

fn read_vec<T>(
    r: &mut Reader<'_>,
    what: &'static str,
    mut item: impl FnMut(&mut Reader<'_>) -> Result<T>,
) -> Result<Vec<T>> {
    r.version(what, LIST_VERSION, LIST_VERSION)?;
    let n = r.u32()?;
    let mut out = Vec::new();
    for _ in 0..n {
        out.push(item(r)?);
    }
    Ok(out)
}

fn write_vec<T>(w: &mut Writer, items: &[T], mut item: impl FnMut(&mut Writer, &T)) {
    w.u8(LIST_VERSION);
    w.u32(u32::try_from(items.len()).unwrap_or(u32::MAX));
    for it in items {
        item(w, it);
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CampaignState {
    pub mission_index: i32,
    pub index_advanced: bool,
    pub available: Vec<CarryoverUnit>,
    pub mission_won: bool,
    pub mission_lost: bool,
    pub mission_ended: bool,
    pub hero_record_count: i32,
    pub used: Vec<CarryoverUnit>,
    pub skill_points: f32,
    pub sciences: Vec<AsciiString>,
    pub player_upgrades: UpgradeList,
    pub cash_bounty: f32,
}

impl CampaignState {
    pub(crate) fn read(r: &mut Reader<'_>) -> Result<Self> {
        let version = r.version("CampaignState", 1, STATE_VERSION)?;
        let mut s = Self {
            mission_index: r.i32()?,
            ..Self::default()
        };
        if version >= 6 {
            s.index_advanced = r.bool()?;
        }
        s.available = read_vec(r, "CarryoverUnit list", CarryoverUnit::read)?;
        s.mission_won = r.bool()?;
        s.mission_lost = r.bool()?;
        s.mission_ended = r.bool()?;
        if version >= 2 {
            s.hero_record_count = r.i32()?;
            s.used = read_vec(r, "CarryoverUnit list", CarryoverUnit::read)?;
        }
        if version >= 3 {
            s.skill_points = r.f32()?;
            s.sciences = read_vec(r, "science list", |r| r.ascii())?;
        }
        if version >= 4 {
            s.player_upgrades = UpgradeList::read(r)?;
        }
        if version >= 5 {
            s.cash_bounty = r.f32()?;
        }
        Ok(s)
    }

    pub(crate) fn write(&self, w: &mut Writer) {
        w.u8(STATE_VERSION);
        w.i32(self.mission_index);
        w.bool(self.index_advanced);
        write_vec(w, &self.available, |w, u| u.write(w));
        w.bool(self.mission_won);
        w.bool(self.mission_lost);
        w.bool(self.mission_ended);
        w.i32(self.hero_record_count);
        write_vec(w, &self.used, |w, u| u.write(w));
        w.f32(self.skill_points);
        write_vec(w, &self.sciences, |w, s| w.ascii(s));
        self.player_upgrades.write(w);
        w.f32(self.cash_bounty);
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CurrentCampaign {
    pub name: AsciiString,
    pub difficulty: u32,
    pub state: CampaignState,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CampaignBlock {
    pub campaign: Option<CurrentCampaign>,
}

impl CampaignBlock {
    pub fn decode(payload: &[u8], payload_offset: usize) -> Result<Self> {
        let mut r = Reader::new(payload, payload_offset);
        r.version("LinearCampaignManager", MANAGER_VERSION, MANAGER_VERSION)?;
        let campaign = if r.bool()? {
            let name = r.ascii()?;
            let difficulty = r.u32()?;
            let mut nested = r.nested()?;
            let state = CampaignState::read(&mut nested)?;
            nested.finish("CampaignState")?;
            Some(CurrentCampaign {
                name,
                difficulty,
                state,
            })
        } else {
            None
        };
        r.finish("CHUNK_Campaign")?;
        Ok(Self { campaign })
    }

    #[must_use]
    pub fn encode(&self, payload_offset: usize) -> Vec<u8> {
        let mut w = Writer::new(payload_offset);
        w.u8(MANAGER_VERSION);
        w.bool(self.campaign.is_some());
        if let Some(c) = &self.campaign {
            w.ascii(&c.name);
            w.u32(c.difficulty);
            w.nested(|inner| c.state.write(inner));
        }
        w.into_bytes()
    }
}
