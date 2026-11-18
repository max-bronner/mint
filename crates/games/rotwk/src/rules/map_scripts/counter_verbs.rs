use super::*;

struct ActionSetCounter {
    key: SideKey,
    value: i32,
}
impl ActionVerb for ActionSetCounter {
    fn run(
        &self,
        _: &RotwkRules,
        _: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        state.counters.entry(self.key.clone()).or_default().value = self.value;
        Ok(())
    }
}

struct ActionAdjustCounter {
    key: SideKey,
    delta: i32,
}
impl ActionVerb for ActionAdjustCounter {
    fn run(
        &self,
        _: &RotwkRules,
        _: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let counter = state.counters.entry(self.key.clone()).or_default();
        counter.value = counter.value.saturating_add(self.delta);
        Ok(())
    }
}

struct ActionStopTimer(SideKey);
impl ActionVerb for ActionStopTimer {
    fn run(
        &self,
        _: &RotwkRules,
        _: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        state.counters.entry(self.0.clone()).or_default().countdown = false;
        Ok(())
    }
}

struct ActionRestartTimer(SideKey);
impl ActionVerb for ActionRestartTimer {
    fn run(
        &self,
        _: &RotwkRules,
        _: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let counter = state.counters.entry(self.0.clone()).or_default();
        if counter.value > 0 {
            counter.countdown = true;
        }
        Ok(())
    }
}

struct ActionNoOp;
impl ActionVerb for ActionNoOp {
    fn run(
        &self,
        _: &RotwkRules,
        _: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        _: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        Ok(())
    }
}

struct ConditionCounterSeconds {
    key: SideKey,
    compare: i32,
    frames: i32,
}
impl ConditionVerb for ConditionCounterSeconds {
    fn eval(
        &self,
        _: &RotwkRules,
        _: &Tick<'_>,
        _: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        let value = state.counters.get(&self.key).map_or(0, |c| c.value);
        compare(value, self.compare, self.frames)
    }
}

struct ConditionCounterCounter {
    left: SideKey,
    compare: i32,
    right: SideKey,
}
impl ConditionVerb for ConditionCounterCounter {
    fn eval(
        &self,
        _: &RotwkRules,
        _: &Tick<'_>,
        _: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        let read = |key: &SideKey| state.counters.get(key).map_or(0, |c| c.value);
        compare(read(&self.left), self.compare, read(&self.right))
    }
}

pub(super) fn parse_set_counter(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionSetCounter {
        key: side_key(params, 0, ctx)?,
        value: parameter_int(params, 1)?,
    }))
}

pub(super) fn parse_increment_counter(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionAdjustCounter {
        key: side_key(params, 1, ctx)?,
        delta: parameter_int(params, 0)?,
    }))
}

pub(super) fn parse_decrement_counter(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionAdjustCounter {
        key: side_key(params, 1, ctx)?,
        delta: parameter_int(params, 0)?.checked_neg()?,
    }))
}

pub(super) fn parse_stop_timer(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionStopTimer(side_key(params, 0, ctx)?)))
}

pub(super) fn parse_restart_timer(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionRestartTimer(side_key(params, 0, ctx)?)))
}

pub(super) fn parse_no_op(_: &[Parameter], _: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionNoOp))
}

pub(super) fn parse_counter_seconds(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionCounterSeconds {
        key: side_key(params, 0, ctx)?,
        compare: parameter_int(params, 1)?,
        frames: seconds_to_script_frames(parameter_real(params, 2)?),
    }))
}

pub(super) fn parse_counter_counter(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionCounterCounter {
        left: side_key(params, 0, ctx)?,
        compare: parameter_int(params, 1)?,
        right: side_key(params, 2, ctx)?,
    }))
}
