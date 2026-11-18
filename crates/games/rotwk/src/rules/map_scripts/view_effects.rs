use mint_sage_map::Parameter;
use mint_sim::StateHasher;

use super::cinematic::{simple, Parsed};
use super::{parameter_int, MapScriptState, ParseCtx};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ViewEffects {
    pub(crate) border_clear: bool,
    pub(crate) shake_seq: u32,
    pub(crate) shake_kind: i32,
}

impl ViewEffects {
    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        if *self == Self::default() {
            return;
        }
        hasher.write_u8(1);
        hasher.write_bool(self.border_clear);
        hasher.write_u32(self.shake_seq);
        hasher.write_i32(self.shake_kind);
    }
}

pub(super) fn parse_disable_border_shroud(_: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    simple(|_, _, _, state: &mut MapScriptState| state.effects.border_clear = true)
}

pub(super) fn parse_enable_border_shroud(_: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    simple(|_, _, _, state: &mut MapScriptState| state.effects.border_clear = false)
}

pub(super) fn parse_screen_shake(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let kind = parameter_int(params, 0)?;
    simple(move |_, _, _, state: &mut MapScriptState| {
        state.effects.shake_seq = state.effects.shake_seq.wrapping_add(1);
        state.effects.shake_kind = kind;
    })
}
