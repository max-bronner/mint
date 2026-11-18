use crate::cah_edit::ExamineApply;

const SELF_CONDITION: &str = "CREATE_A_HERO_EXAMINE_SELF";
const WEAPON_CONDITION: &str = "CREATE_A_HERO_EXAMINE_WEAPON_RIGHT";
const DEFAULT_CHANCE: f32 = 50.0;
const FRAME_SECONDS: f32 = 0.033_333_335;
const RETAIL_FRAMES_PER_SECOND: f32 = 5.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ExamineParams {
    pub(crate) cheer: String,
    pub(crate) weapon: String,
    pub(crate) own: String,
    pub(crate) chance: f32,
    pub(crate) tweak: i32,
}

impl ExamineParams {
    pub(crate) fn from_def(def: &mint_sage_ini::CreateAHeroSystemDef) -> Self {
        let text = |field: &Option<String>| field.clone().unwrap_or_default();
        Self {
            cheer: text(&def.selected_cheer_anim),
            weapon: text(&def.examine_weapon_anim),
            own: text(&def.examine_self_anim),
            chance: def.special_anim_percent_chance.unwrap_or(DEFAULT_CHANCE),
            tweak: def.examine_anim_tweak_value.unwrap_or(0),
        }
    }

    pub(crate) fn is_special(&self, clip: &str) -> bool {
        !clip.is_empty()
            && [&self.cheer, &self.weapon, &self.own]
                .iter()
                .any(|suffix| clip.ends_with(suffix.as_str()))
    }
}

pub(crate) fn armed(apply: &ExamineApply) -> Option<&'static str> {
    match (apply.changed, apply.edited) {
        (1, _) if apply.weapon => Some(WEAPON_CONDITION),
        (1, _) | (0, true) => Some(SELF_CONDITION),
        _ => None,
    }
}

pub(crate) fn examine_choice(
    apply: &ExamineApply,
    skip: &mut bool,
    playing_special: bool,
    chance: f32,
    roll: impl FnOnce() -> f32,
) -> Option<&'static str> {
    *skip |= apply.skip;
    if playing_special || std::mem::take(skip) {
        return None;
    }
    let condition = armed(apply)?;
    (chance >= roll()).then_some(condition)
}

pub(crate) fn examine_frames(frames: u32, tweak: i32) -> u32 {
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    let shortened = (frames as i32).wrapping_sub(tweak) as u32;
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let held = (shortened as f32 * FRAME_SECONDS * RETAIL_FRAMES_PER_SECOND) as i64;
    u32::try_from(held.max(0)).unwrap_or(u32::MAX)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ClientRandom(u64);

impl ClientRandom {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    pub(crate) fn percent(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        let bits = x.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 40;
        #[allow(clippy::cast_precision_loss)]
        let unit = bits as f32 / 16_777_215.0;
        unit * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(changed: usize, weapon: bool, edited: bool) -> ExamineApply {
        ExamineApply {
            skip: false,
            edited,
            changed,
            weapon,
        }
    }

    fn choose(
        apply: &ExamineApply,
        skip: &mut bool,
        playing: bool,
        roll: f32,
    ) -> Option<&'static str> {
        examine_choice(apply, skip, playing, 20.0, || roll)
    }

    #[test]
    fn one_changed_binder_picks_weapon_or_self_and_none_or_many_pick_self_or_nothing() {
        let mut skip = false;
        assert_eq!(
            choose(&apply(1, true, true), &mut skip, false, 0.0),
            Some(WEAPON_CONDITION)
        );
        assert_eq!(
            choose(&apply(1, false, true), &mut skip, false, 0.0),
            Some(SELF_CONDITION)
        );
        assert_eq!(
            choose(&apply(0, false, true), &mut skip, false, 0.0),
            Some(SELF_CONDITION)
        );
        assert_eq!(choose(&apply(2, false, true), &mut skip, false, 0.0), None);
        assert_eq!(choose(&apply(0, false, false), &mut skip, false, 0.0), None);
    }

    #[test]
    fn the_roll_passes_up_to_and_including_the_chance() {
        let mut skip = false;
        let one = apply(1, false, true);
        assert!(choose(&one, &mut skip, false, 20.0).is_some());
        assert!(choose(&one, &mut skip, false, 20.1).is_none());
    }

    #[test]
    fn the_skip_bit_swallows_one_apply_unless_a_special_clip_is_playing() {
        let one = apply(1, false, true);
        let mut skip = true;
        assert_eq!(choose(&one, &mut skip, true, 0.0), None);
        assert!(skip);
        assert_eq!(choose(&one, &mut skip, false, 0.0), None);
        assert!(!skip);
        assert!(choose(&one, &mut skip, false, 0.0).is_some());
        let raised = ExamineApply { skip: true, ..one };
        assert_eq!(choose(&raised, &mut skip, false, 0.0), None);
        assert!(!skip);
    }

    #[test]
    fn a_clip_holds_its_condition_for_its_length_in_retail_frames() {
        assert_eq!(examine_frames(60, 0), 10);
        assert_eq!(examine_frames(60, 30), 5);
        assert_eq!(examine_frames(0, 0), 0);
    }

    #[test]
    fn a_special_clip_is_recognised_by_its_suffix() {
        let params = ExamineParams {
            cheer: "_Cheer".into(),
            weapon: "_Weap".into(),
            own: "_Own".into(),
            chance: 50.0,
            tweak: 0,
        };
        assert!(params.is_special("Hero_Own"));
        assert!(!params.is_special("Hero_Idle"));
        assert!(!params.is_special(""));
    }
}
