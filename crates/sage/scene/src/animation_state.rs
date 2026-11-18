use mint_sage_ini::{AnimationClip, AnimationStateDef, AnimationStateKind};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Conditions([u128; 2]);

#[allow(clippy::cast_possible_truncation)]
const EXTRA_FIRST: u32 = Conditions::NAMES.len() as u32;

const CONDITION_BITS: usize = 256;

pub const EXTRA_CAPACITY: usize = CONDITION_BITS - Conditions::NAMES.len();

const WEAPON_SLOTS: usize = 5;

const WEAPON_FIRST: u32 = 23;

const WEAPON_TOKENS: u32 = 7;

#[derive(Clone, Copy)]
enum WeaponToken {
    Firing,
    BetweenFiringShots,
    Reloading,
    PreAttack,
    UsingWeapon,
    FiringOrPreAttack,
    FiringOrReloading,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponPhase {
    PreAttack,
    Firing,
    BetweenFiringShots,
    Reloading,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtraTokens {
    names: Vec<String>,
    refused: usize,
}

impl ExtraTokens {
    pub fn intern(&mut self, token: &str) -> Option<Conditions> {
        if let Some(found) = self.lookup(token) {
            return Some(found);
        }
        if self.names.len() >= EXTRA_CAPACITY {
            self.refused = self.refused.saturating_add(1);
            return None;
        }
        self.names.push(token.to_ascii_uppercase());
        Self::bit(self.names.len().saturating_sub(1))
    }

    #[must_use]
    pub fn lookup(&self, token: &str) -> Option<Conditions> {
        let at = self
            .names
            .iter()
            .position(|name| name.eq_ignore_ascii_case(token))?;
        Self::bit(at)
    }

    #[must_use]
    pub fn bits_of<'n>(&self, names: impl IntoIterator<Item = &'n str>) -> Conditions {
        names
            .into_iter()
            .filter_map(|name| self.lookup(name))
            .fold(Conditions::NONE, Conditions::union)
    }

    #[must_use]
    pub fn resolve<'n>(&self, tokens: impl IntoIterator<Item = &'n str>) -> Conditions {
        tokens.into_iter().fold(Conditions::NONE, |acc, token| {
            acc.union(
                Conditions::from_token(token)
                    .or_else(|| self.lookup(token))
                    .unwrap_or(Conditions::NONE),
            )
        })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    #[must_use]
    pub const fn refused(&self) -> usize {
        self.refused
    }

    fn bit(at: usize) -> Option<Conditions> {
        let shift = u32::try_from(at).ok()?.checked_add(EXTRA_FIRST)?;
        match shift.checked_sub(u128::BITS) {
            None => Some(Conditions([1 << shift, 0])),
            Some(high) => 1u128.checked_shl(high).map(|bit| Conditions([0, bit])),
        }
    }
}

impl Conditions {
    const fn low(bits: u128) -> Self {
        Self([bits, 0])
    }

    pub const NONE: Self = Self([0, 0]);
    pub const MOVING: Self = Self::low(1 << 0);
    pub const DYING: Self = Self::low(1 << 1);
    pub const ATTACKING: Self = Self::low(1 << 2);
    pub const PREATTACK_A: Self = Self::weapon_token(0, WeaponToken::PreAttack);
    pub const FIRING_OR_PREATTACK_A: Self = Self::weapon_token(0, WeaponToken::FiringOrPreAttack);
    pub const FIRING_OR_RELOADING_A: Self = Self::weapon_token(0, WeaponToken::FiringOrReloading);
    pub const SELECTED: Self = Self::low(1 << 3);
    pub const AWAITING_CONSTRUCTION: Self = Self::low(1 << 4);
    pub const PARTIALLY_CONSTRUCTED: Self = Self::low(1 << 5);
    pub const ACTIVELY_BEING_CONSTRUCTED: Self = Self::low(1 << 6);
    pub const SNOW: Self = Self::low(1 << 7);
    pub const DAMAGED: Self = Self::low(1 << 8);
    pub const REALLYDAMAGED: Self = Self::low(1 << 9);
    pub const RUBBLE: Self = Self::low(1 << 10);
    pub const CONSTRUCTION_COMPLETE: Self = Self::low(1 << 11);
    pub const POST_RUBBLE: Self = Self::low(1 << 12);
    pub const POST_COLLAPSE: Self = Self::low(1 << 13);
    pub const ACCELERATE: Self = Self::low(1 << 14);
    pub const DECELERATE: Self = Self::low(1 << 15);
    pub const TURN_LEFT: Self = Self::low(1 << 16);
    pub const TURN_RIGHT: Self = Self::low(1 << 17);
    pub const TURN_LEFT_HIGH_SPEED: Self = Self::low(1 << 18);
    pub const TURN_RIGHT_HIGH_SPEED: Self = Self::low(1 << 19);
    pub const BACKING_UP: Self = Self::low(1 << 20);
    pub const ACTIVELY_CONSTRUCTING: Self = Self::low(1 << 21);
    pub const DEPLOYED: Self = Self::low(1 << 22);
    pub const BURNINGDEATH: Self = Self::low(1 << 58);
    pub const WALKING: Self = Self::low(1 << 59);
    pub const RUNNING_OFF_MAP: Self = Self::low(1 << 60);
    pub const ENGAGED: Self = Self::low(1 << 61);
    pub const DEATH_1: Self = Self::low(1 << 62);
    pub const DEATH_2: Self = Self::low(1 << 63);
    pub const DEATH_3: Self = Self::low(1 << 64);
    pub const DEATH_4: Self = Self::low(1 << 65);
    pub const DEATH_5: Self = Self::low(1 << 66);

    pub const UNDER_CONSTRUCTION: Self = Self::AWAITING_CONSTRUCTION
        .union(Self::PARTIALLY_CONSTRUCTED)
        .union(Self::ACTIVELY_BEING_CONSTRUCTED);

    #[allow(clippy::cast_possible_truncation)]
    pub const WEAPON: Self =
        Self::low(((1 << (WEAPON_SLOTS as u32 * WEAPON_TOKENS)) - 1) << WEAPON_FIRST);

    #[allow(clippy::cast_possible_truncation)]
    const fn weapon_token(slot: usize, token: WeaponToken) -> Self {
        if slot >= WEAPON_SLOTS {
            return Self::NONE;
        }
        Self::low(1 << (WEAPON_FIRST + slot as u32 * WEAPON_TOKENS + token as u32))
    }

    #[must_use]
    pub const fn weapon(slot: usize, phase: WeaponPhase) -> Self {
        let (own, merged) = match phase {
            WeaponPhase::PreAttack => (
                WeaponToken::PreAttack,
                Self::weapon_token(slot, WeaponToken::FiringOrPreAttack),
            ),
            WeaponPhase::Firing => (
                WeaponToken::Firing,
                Self::weapon_token(slot, WeaponToken::FiringOrPreAttack)
                    .union(Self::weapon_token(slot, WeaponToken::FiringOrReloading)),
            ),
            WeaponPhase::BetweenFiringShots => (WeaponToken::BetweenFiringShots, Self::NONE),
            WeaponPhase::Reloading => (
                WeaponToken::Reloading,
                Self::weapon_token(slot, WeaponToken::FiringOrReloading),
            ),
        };
        Self::weapon_token(slot, own)
            .union(Self::weapon_token(slot, WeaponToken::UsingWeapon))
            .union(merged)
    }

    #[must_use]
    pub fn names_a_shot(self) -> bool {
        (0..WEAPON_SLOTS).any(|slot| self.names_slot_shot(slot))
    }

    #[must_use]
    pub fn names_slot_shot(self, slot: usize) -> bool {
        self.intersects(
            Self::weapon_token(slot, WeaponToken::PreAttack)
                .union(Self::weapon_token(slot, WeaponToken::Firing))
                .union(Self::weapon_token(slot, WeaponToken::FiringOrPreAttack))
                .union(Self::weapon_token(slot, WeaponToken::FiringOrReloading)),
        )
    }

    #[must_use]
    pub fn in_follow_through(self, showing: Self) -> bool {
        self.contains(Self::ATTACKING)
            && (0..WEAPON_SLOTS).any(|slot| {
                let shot = Self::weapon_token(slot, WeaponToken::PreAttack)
                    .union(Self::weapon_token(slot, WeaponToken::Firing))
                    .union(Self::weapon_token(slot, WeaponToken::FiringOrPreAttack))
                    .union(Self::weapon_token(slot, WeaponToken::FiringOrReloading));
                showing.intersects(shot)
                    && self.contains(Self::weapon_token(slot, WeaponToken::Firing))
            })
    }

    pub const NAMES: [(&'static str, Self); 67] = [
        ("MOVING", Self::MOVING),
        ("DYING", Self::DYING),
        ("ATTACKING", Self::ATTACKING),
        ("SELECTED", Self::SELECTED),
        ("AWAITING_CONSTRUCTION", Self::AWAITING_CONSTRUCTION),
        ("PARTIALLY_CONSTRUCTED", Self::PARTIALLY_CONSTRUCTED),
        (
            "ACTIVELY_BEING_CONSTRUCTED",
            Self::ACTIVELY_BEING_CONSTRUCTED,
        ),
        ("SNOW", Self::SNOW),
        ("DAMAGED", Self::DAMAGED),
        ("REALLYDAMAGED", Self::REALLYDAMAGED),
        ("RUBBLE", Self::RUBBLE),
        ("CONSTRUCTION_COMPLETE", Self::CONSTRUCTION_COMPLETE),
        ("POST_RUBBLE", Self::POST_RUBBLE),
        ("POST_COLLAPSE", Self::POST_COLLAPSE),
        ("ACCELERATE", Self::ACCELERATE),
        ("DECELERATE", Self::DECELERATE),
        ("TURN_LEFT", Self::TURN_LEFT),
        ("TURN_RIGHT", Self::TURN_RIGHT),
        ("TURN_LEFT_HIGH_SPEED", Self::TURN_LEFT_HIGH_SPEED),
        ("TURN_RIGHT_HIGH_SPEED", Self::TURN_RIGHT_HIGH_SPEED),
        ("BACKING_UP", Self::BACKING_UP),
        ("ACTIVELY_CONSTRUCTING", Self::ACTIVELY_CONSTRUCTING),
        ("DEPLOYED", Self::DEPLOYED),
        ("FIRING_A", Self::weapon_token(0, WeaponToken::Firing)),
        (
            "BETWEEN_FIRING_SHOTS_A",
            Self::weapon_token(0, WeaponToken::BetweenFiringShots),
        ),
        ("RELOADING_A", Self::weapon_token(0, WeaponToken::Reloading)),
        ("PREATTACK_A", Self::weapon_token(0, WeaponToken::PreAttack)),
        (
            "USING_WEAPON_A",
            Self::weapon_token(0, WeaponToken::UsingWeapon),
        ),
        (
            "FIRING_OR_PREATTACK_A",
            Self::weapon_token(0, WeaponToken::FiringOrPreAttack),
        ),
        (
            "FIRING_OR_RELOADING_A",
            Self::weapon_token(0, WeaponToken::FiringOrReloading),
        ),
        ("FIRING_B", Self::weapon_token(1, WeaponToken::Firing)),
        (
            "BETWEEN_FIRING_SHOTS_B",
            Self::weapon_token(1, WeaponToken::BetweenFiringShots),
        ),
        ("RELOADING_B", Self::weapon_token(1, WeaponToken::Reloading)),
        ("PREATTACK_B", Self::weapon_token(1, WeaponToken::PreAttack)),
        (
            "USING_WEAPON_B",
            Self::weapon_token(1, WeaponToken::UsingWeapon),
        ),
        (
            "FIRING_OR_PREATTACK_B",
            Self::weapon_token(1, WeaponToken::FiringOrPreAttack),
        ),
        (
            "FIRING_OR_RELOADING_B",
            Self::weapon_token(1, WeaponToken::FiringOrReloading),
        ),
        ("FIRING_C", Self::weapon_token(2, WeaponToken::Firing)),
        (
            "BETWEEN_FIRING_SHOTS_C",
            Self::weapon_token(2, WeaponToken::BetweenFiringShots),
        ),
        ("RELOADING_C", Self::weapon_token(2, WeaponToken::Reloading)),
        ("PREATTACK_C", Self::weapon_token(2, WeaponToken::PreAttack)),
        (
            "USING_WEAPON_C",
            Self::weapon_token(2, WeaponToken::UsingWeapon),
        ),
        (
            "FIRING_OR_PREATTACK_C",
            Self::weapon_token(2, WeaponToken::FiringOrPreAttack),
        ),
        (
            "FIRING_OR_RELOADING_C",
            Self::weapon_token(2, WeaponToken::FiringOrReloading),
        ),
        ("FIRING_D", Self::weapon_token(3, WeaponToken::Firing)),
        (
            "BETWEEN_FIRING_SHOTS_D",
            Self::weapon_token(3, WeaponToken::BetweenFiringShots),
        ),
        ("RELOADING_D", Self::weapon_token(3, WeaponToken::Reloading)),
        ("PREATTACK_D", Self::weapon_token(3, WeaponToken::PreAttack)),
        (
            "USING_WEAPON_D",
            Self::weapon_token(3, WeaponToken::UsingWeapon),
        ),
        (
            "FIRING_OR_PREATTACK_D",
            Self::weapon_token(3, WeaponToken::FiringOrPreAttack),
        ),
        (
            "FIRING_OR_RELOADING_D",
            Self::weapon_token(3, WeaponToken::FiringOrReloading),
        ),
        ("FIRING_E", Self::weapon_token(4, WeaponToken::Firing)),
        (
            "BETWEEN_FIRING_SHOTS_E",
            Self::weapon_token(4, WeaponToken::BetweenFiringShots),
        ),
        ("RELOADING_E", Self::weapon_token(4, WeaponToken::Reloading)),
        ("PREATTACK_E", Self::weapon_token(4, WeaponToken::PreAttack)),
        (
            "USING_WEAPON_E",
            Self::weapon_token(4, WeaponToken::UsingWeapon),
        ),
        (
            "FIRING_OR_PREATTACK_E",
            Self::weapon_token(4, WeaponToken::FiringOrPreAttack),
        ),
        (
            "FIRING_OR_RELOADING_E",
            Self::weapon_token(4, WeaponToken::FiringOrReloading),
        ),
        ("BURNINGDEATH", Self::BURNINGDEATH),
        ("WALKING", Self::WALKING),
        ("RUNNING_OFF_MAP", Self::RUNNING_OFF_MAP),
        ("ENGAGED", Self::ENGAGED),
        ("DEATH_1", Self::DEATH_1),
        ("DEATH_2", Self::DEATH_2),
        ("DEATH_3", Self::DEATH_3),
        ("DEATH_4", Self::DEATH_4),
        ("DEATH_5", Self::DEATH_5),
    ];

    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(token))
            .map(|(_, bit)| *bit)
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        let ([a, b], [c, d]) = (self.0, other.0);
        Self([a | c, b | d])
    }

    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        let ([a, b], [c, d]) = (self.0, other.0);
        Self([a & !c, b & !d])
    }

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        let ([a, b], [c, d]) = (self.0, other.0);
        a & c == c && b & d == d
    }

    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        let ([a, b], [c, d]) = (self.0, other.0);
        a & c != 0 || b & d != 0
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        let [a, b] = self.0;
        a == 0 && b == 0
    }

    #[must_use]
    pub const fn bits(self) -> [u128; 2] {
        self.0
    }

    #[must_use]
    pub const fn on_death(self) -> Self {
        let cleared = Self::MOVING.union(Self::ATTACKING).union(Self::WEAPON);
        self.without(cleared).union(Self::DYING)
    }

    #[must_use]
    pub const fn on_structure_death(self) -> Self {
        let cleared = Self::MOVING
            .union(Self::ATTACKING)
            .union(Self::WEAPON)
            .union(Self::DAMAGED)
            .union(Self::REALLYDAMAGED);
        self.without(cleared).union(Self::RUBBLE)
    }

    #[must_use]
    pub const fn on_collapse_done(self) -> Self {
        self.without(Self::RUBBLE).union(Self::POST_COLLAPSE)
    }

    #[must_use]
    pub fn label(self) -> String {
        let mut out = String::new();
        for (name, bit) in Self::NAMES {
            if self.contains(bit) {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(name);
            }
        }
        if out.is_empty() {
            out.push_str("NONE");
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompiledState {
    pub source: usize,
    pub required: Conditions,
    pub reachable: bool,
    pub selectable: bool,
}

fn compile_header<'s>(
    tokens: &'s [String],
    mut extras: Option<&mut ExtraTokens>,
    unmodelled: &mut Vec<&'s str>,
) -> (Conditions, bool, bool) {
    let mut required = Conditions::NONE;
    let (mut reachable, mut selectable) = (true, true);
    for token in tokens {
        if let Some(bit) = Conditions::from_token(token) {
            required = required.union(bit);
            continue;
        }
        reachable = false;
        match extras
            .as_deref_mut()
            .and_then(|extras| extras.intern(token))
        {
            Some(bit) => required = required.union(bit),
            None => selectable = false,
        }
        if !unmodelled
            .iter()
            .any(|seen| seen.eq_ignore_ascii_case(token))
        {
            unmodelled.push(token);
        }
    }
    (required, reachable, selectable)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MachineReport {
    pub states: usize,
    pub unreachable: usize,
    pub transitions: usize,
    pub unmodelled_tokens: usize,
    pub has_fallback: bool,
    pub scripted: usize,
    pub clipless: usize,
}

#[derive(Debug, Clone, Default)]
pub struct StateMachine {
    states: Vec<CompiledState>,
    fallback: Option<usize>,
    report: MachineReport,
}

impl StateMachine {
    #[must_use]
    pub fn compile(states: &[AnimationStateDef]) -> Self {
        Self::compile_with(states, None)
    }

    #[must_use]
    pub fn compile_with(
        states: &[AnimationStateDef],
        mut extras: Option<&mut ExtraTokens>,
    ) -> Self {
        let mut compiled = Vec::with_capacity(states.len());
        let mut fallback = None;
        let mut idle_fallback = None;
        let mut report = MachineReport {
            states: states.len(),
            ..MachineReport::default()
        };
        let mut unmodelled: Vec<&str> = Vec::new();

        for (index, state) in states.iter().enumerate() {
            if state.script.is_some() {
                report.scripted = report.scripted.saturating_add(1);
            }
            if state.clips.is_empty() {
                report.clipless = report.clipless.saturating_add(1);
            }
            let (required, reachable, selectable) = if state.kind == AnimationStateKind::Transition
            {
                report.transitions = report.transitions.saturating_add(1);
                (Conditions::NONE, false, false)
            } else {
                compile_header(&state.conditions, extras.as_deref_mut(), &mut unmodelled)
            };
            if !reachable && state.kind != AnimationStateKind::Transition {
                report.unreachable = report.unreachable.saturating_add(1);
            }
            if reachable && required.is_empty() && !state.clips.is_empty() {
                if state.kind == AnimationStateKind::Idle {
                    idle_fallback.get_or_insert(index);
                } else {
                    fallback = Some(index);
                }
            }
            compiled.push(CompiledState {
                source: index,
                required,
                reachable,
                selectable,
            });
        }
        let fallback = idle_fallback.or(fallback);
        report.unmodelled_tokens = unmodelled.len();
        report.has_fallback = fallback.is_some();
        Self {
            states: compiled,
            fallback,
            report,
        }
    }

    #[must_use]
    pub fn states(&self) -> &[CompiledState] {
        &self.states
    }

    #[must_use]
    pub const fn report(&self) -> MachineReport {
        self.report
    }

    #[must_use]
    pub fn select(&self, live: Conditions) -> Option<usize> {
        self.select_playable(live, |_| true)
    }

    #[must_use]
    pub fn select_playable(
        &self,
        live: Conditions,
        plays: impl Fn(usize) -> bool,
    ) -> Option<usize> {
        self.states
            .iter()
            .filter(|state| {
                state.selectable && !state.required.is_empty() && live.contains(state.required)
            })
            .map(|state| state.source)
            .find(|&state| plays(state))
            .or_else(|| self.fallback.filter(|&state| plays(state)))
    }

    #[must_use]
    pub const fn fallback(&self) -> Option<usize> {
        self.fallback
    }

    #[must_use]
    pub fn required(&self, index: usize) -> Conditions {
        self.states
            .iter()
            .find(|state| state.source == index)
            .map_or(Conditions::NONE, |state| state.required)
    }

    #[must_use]
    pub fn fallthrough_count(&self, probe: &[Conditions]) -> usize {
        probe
            .iter()
            .filter(|live| self.select(**live) == self.fallback && !self.states.is_empty())
            .count()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModelStateMachine {
    states: Vec<CompiledState>,
    fallback: Option<usize>,
    report: MachineReport,
}

impl ModelStateMachine {
    #[must_use]
    pub fn compile(states: &[mint_sage_ini::ModelStateDef]) -> Self {
        Self::compile_with(states, None)
    }

    #[must_use]
    pub fn compile_with(
        states: &[mint_sage_ini::ModelStateDef],
        mut extras: Option<&mut ExtraTokens>,
    ) -> Self {
        let mut compiled = Vec::with_capacity(states.len());
        let mut fallback = None;
        let mut report = MachineReport {
            states: states.len(),
            ..MachineReport::default()
        };
        let mut unmodelled: Vec<&str> = Vec::new();
        for (index, state) in states.iter().enumerate() {
            let (required, reachable, selectable) =
                compile_header(&state.conditions, extras.as_deref_mut(), &mut unmodelled);
            if !reachable {
                report.unreachable = report.unreachable.saturating_add(1);
            }
            if reachable && required.is_empty() {
                fallback.get_or_insert(index);
            }
            compiled.push(CompiledState {
                source: index,
                required,
                reachable,
                selectable,
            });
        }
        report.unmodelled_tokens = unmodelled.len();
        report.has_fallback = fallback.is_some();
        Self {
            states: compiled,
            fallback,
            report,
        }
    }

    #[must_use]
    pub const fn report(&self) -> MachineReport {
        self.report
    }

    #[must_use]
    pub fn states(&self) -> &[CompiledState] {
        &self.states
    }

    #[must_use]
    pub fn required(&self, index: usize) -> Conditions {
        self.states
            .iter()
            .find(|state| state.source == index)
            .map_or(Conditions::NONE, |state| state.required)
    }

    #[must_use]
    pub fn select(&self, live: Conditions) -> Option<usize> {
        self.states
            .iter()
            .find(|state| {
                state.selectable && !state.required.is_empty() && live.contains(state.required)
            })
            .map(|state| state.source)
            .or(self.fallback)
    }

    #[must_use]
    pub const fn fallback(&self) -> Option<usize> {
        self.fallback
    }
}

#[must_use]
pub fn reachable_condition_sets() -> Vec<Conditions> {
    let mut out = vec![
        Conditions::DYING,
        Conditions::DYING.union(Conditions::SELECTED),
    ];
    let phases = [
        WeaponPhase::PreAttack,
        WeaponPhase::Firing,
        WeaponPhase::BetweenFiringShots,
        WeaponPhase::Reloading,
    ];
    let weapon: Vec<Conditions> = std::iter::once(Conditions::NONE)
        .chain(
            (0..WEAPON_SLOTS).flat_map(|slot| phases.map(|phase| Conditions::weapon(slot, phase))),
        )
        .collect();
    for moving in [Conditions::NONE, Conditions::MOVING] {
        for attacking in [Conditions::NONE, Conditions::ATTACKING] {
            for &shot in &weapon {
                for selected in [Conditions::NONE, Conditions::SELECTED] {
                    out.push(moving.union(attacking).union(shot).union(selected));
                }
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[must_use]
pub fn variant(id: u32, state: u32, salt: u32) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in id
        .to_le_bytes()
        .into_iter()
        .chain(state.to_le_bytes())
        .chain(salt.to_le_bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash ^= hash >> 30;
    hash = hash.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    hash ^= hash >> 27;
    hash = hash.wrapping_mul(0x94d0_49bb_1331_11eb);
    hash ^ (hash >> 31)
}

#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn variant_fraction(id: u32, state: u32, salt: u32) -> f32 {
    let bits = (variant(id, state, salt) >> 40) as u32;
    bits as f32 / 16_777_216.0
}

#[must_use]
pub fn pick_clip(
    priorities: &[Option<i32>],
    id: u32,
    state: usize,
    entry: u32,
    avoid: Option<usize>,
) -> Option<usize> {
    let clips = priorities.len();
    if clips <= 1 {
        return (clips == 1).then_some(0);
    }
    let state = u32::try_from(state).unwrap_or(u32::MAX);
    let salt = 0x01_u32.wrapping_add(entry.wrapping_shl(8));
    let draw = variant(id, state, salt);
    let weight = |index: usize, priority: &Option<i32>| -> i64 {
        let weight = i64::from(priority.unwrap_or(1).clamp(0, MAX_ANIMATION_PRIORITY));
        if avoid == Some(index) {
            (weight - 1).max(0)
        } else {
            weight
        }
    };
    let total: i64 = priorities
        .iter()
        .enumerate()
        .map(|(index, priority)| weight(index, priority))
        .sum();
    let Ok(total) = u64::try_from(total) else {
        return usize::try_from(draw % u64::try_from(clips).unwrap_or(1)).ok();
    };
    if total == 0 {
        return usize::try_from(draw % u64::try_from(clips).unwrap_or(1)).ok();
    }
    let mut left = i64::try_from(draw % total).unwrap_or(0);
    for (index, priority) in priorities.iter().enumerate() {
        left -= weight(index, priority);
        if left < 0 {
            return Some(index);
        }
    }
    Some(clips - 1)
}

pub const MAX_ANIMATION_PRIORITY: i32 = 100;

#[must_use]
pub fn pick_body(bodies: usize, id: u32) -> usize {
    if bodies <= 1 {
        return 0;
    }
    let bodies_u64 = u64::try_from(bodies).unwrap_or(1);
    usize::try_from(variant(id, 0, 0x04) % bodies_u64.max(1)).unwrap_or(0)
}

#[must_use]
pub fn rate_scale(
    clip: &AnimationClip,
    speed: f32,
    natural_seconds: f32,
    id: u32,
    weapon_cycle: f32,
) -> f32 {
    let mut scale = speed_factor(clip, id);
    if clip.use_weapon_timing && weapon_cycle > 0.0 && natural_seconds > 0.0 {
        scale = natural_seconds / weapon_cycle;
    }
    if let Some(distance) = clip.distance {
        if distance > 0.0 && speed > 0.0 && natural_seconds > 0.0 {
            let desired = distance / speed;
            if desired.is_finite() && desired > 0.0 {
                scale = natural_seconds / desired;
            }
        }
    }
    let (low, high) = RATE_SCALE_LIMITS;
    if scale.is_finite() {
        scale.clamp(low, high)
    } else {
        1.0
    }
}

#[must_use]
pub fn speed_factor(clip: &AnimationClip, id: u32) -> f32 {
    let (min, max) = clip.speed_range.unwrap_or((1.0, 1.0));
    let (min, max) = if min.is_finite() && max.is_finite() && max >= min {
        (min, max)
    } else {
        (1.0, 1.0)
    };
    let factor = min + (max - min) * variant_fraction(id, 0, 0x02);
    if factor.is_finite() && factor > 0.0 {
        factor
    } else {
        1.0
    }
}

pub const RATE_SCALE_LIMITS: (f32, f32) = (0.1, 8.0);

#[must_use]
pub fn phase_offset(repeats: bool, natural_seconds: f32, id: u32, state: usize) -> f32 {
    if !repeats || !natural_seconds.is_finite() || natural_seconds <= 0.0 {
        return 0.0;
    }
    let state = u32::try_from(state).unwrap_or(u32::MAX);
    natural_seconds * variant_fraction(id, state, 0x03)
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::unwrap_used)]
mod tests;
