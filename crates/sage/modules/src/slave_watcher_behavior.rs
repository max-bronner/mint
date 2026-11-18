use std::sync::Arc;

use mint_sim::module::{
    Capability, CapabilityMask, Sleep, UpdateContext, UpgradeContext, UpgradeMask,
};
use mint_sim::{Component, Entity, FieldError, Frame, ModuleFieldSource, ModuleKind, StateHasher};

use crate::effects::{Effect, EffectLog, Name};
use crate::world_reads::MasterSlaves;

const POLL_RATE: mint_sim::LogicFrames = mint_sim::LogicFrames::from_frames(7);

#[derive(Debug, PartialEq)]
struct Data {
    remove_upgrade: UpgradeMask,
    grant_upgrade: Option<Name>,
    share_upgrades: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SlaveWatcherBehavior {
    data: Arc<Data>,
    watching: Option<Entity>,
    share_due: bool,
}

impl Component for SlaveWatcherBehavior {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.watching.is_some());
        if let Some(slave) = self.watching {
            slave.hash_into(hasher);
        }
        hasher.write_bool(self.share_due);
    }
}

impl ModuleKind for SlaveWatcherBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update)
        .union(CapabilityMask::single(Capability::Upgrade));

    fn on_attach(&mut self, _now: Frame) {
        self.watching = None;
        self.share_due = false;
    }

    fn attempt_upgrade(&mut self, _active: &UpgradeMask, _ctx: &mut UpgradeContext<'_>) {
        self.share_due |= self.data.share_upgrades;
    }

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        let living: Vec<Entity> = ctx
            .reads
            .get::<MasterSlaves>()
            .map(|slaves| slaves.slaves_of(ctx.entity).collect())
            .unwrap_or_default();
        match self.watching {
            Some(watched) if !living.contains(&watched) => {
                let mut out = Vec::new();
                if let Some(name) = &self.data.grant_upgrade {
                    out.push(Effect::GrantUpgrade(Name::clone(name)));
                }
                if !self.data.remove_upgrade.is_empty() {
                    out.push(Effect::RemoveUpgrades(
                        self.data.remove_upgrade.clone(),
                        false,
                    ));
                }
                if let Some(log) = ctx.effects.get::<EffectLog>() {
                    for effect in out {
                        log.push(ctx.entity, effect);
                    }
                }
                self.watching = None;
                self.share_due = false;
            }
            Some(_) => {}
            None => {
                self.watching = living.first().copied();
                self.share_due |= self.data.share_upgrades && self.watching.is_some();
            }
        }
        if self.share_due && self.watching.is_some() {
            if let Some(log) = ctx.effects.get::<EffectLog>() {
                log.push(ctx.entity, Effect::ShareUpgradesWithSlaves);
            }
            self.share_due = false;
        }
        Sleep::Frames(POLL_RATE)
    }
}

fn boolean(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<bool, FieldError> {
    match fields.field(keyword) {
        None => Ok(false),
        Some(raw) => mint_sage_ini::value::parse_bool(raw)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No"))),
    }
}

pub fn slave_watcher_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<SlaveWatcherBehavior, FieldError> {
    let data = Data {
        remove_upgrade: fields.field_upgrade_mask("RemoveUpgrade"),
        grant_upgrade: fields.field("GrantUpgrade").map(Name::from),
        share_upgrades: boolean(fields, "ShareUpgrades")?,
    };
    Ok(SlaveWatcherBehavior {
        data: Arc::new(data),
        watching: None,
        share_due: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sim::module::{DestroyRequests, ModuleRegistry, Reads, UpgradeBit};
    use mint_sim::{Effects, Pcg32, UpgradeMask, World};
    use std::collections::BTreeMap;

    struct TestFields(BTreeMap<&'static str, Vec<&'static str>>);
    impl ModuleFieldSource for TestFields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .and_then(|(_, v)| v.first())
                .copied()
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        }
        fn field_upgrade_mask(&self, keyword: &str) -> UpgradeMask {
            let mut mask = UpgradeMask::EMPTY;
            for token in self.field_args(keyword) {
                if let Ok(n) = token.parse::<u32>() {
                    mask.set(UpgradeBit::new(n));
                }
            }
            mask
        }
    }

    fn fields(pairs: &[(&'static str, &[&'static str])]) -> TestFields {
        TestFields(pairs.iter().map(|(k, v)| (*k, v.to_vec())).collect())
    }

    fn tick(
        registry: &ModuleRegistry,
        world: &mut World,
        now: Frame,
        slaves: &MasterSlaves,
    ) -> Vec<Effect> {
        let mut rng = Pcg32::new(1);
        let mut log = EffectLog::new();
        registry.update_with_reads(
            world,
            now,
            &mut rng,
            &mut DestroyRequests::new(),
            Effects::into_sink(&mut log),
            Reads::from_snapshots(&[slaves]),
        );
        log.drain().into_iter().map(|(_, e)| e).collect()
    }

    #[test]
    fn share_upgrades_hands_over_at_enslavement_and_again_when_the_host_gains_one() {
        let module = slave_watcher_behavior_from_fields(
            &fields(&[("ShareUpgrades", &["Yes"])]),
            Frame::ZERO,
        )
        .unwrap();
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<SlaveWatcherBehavior>(&mut world, "SlaveWatcherBehavior");
        let master = world.spawn().unwrap();
        let slave = world.spawn().unwrap();
        registry
            .attach(&mut world, master, Frame::ZERO, module)
            .unwrap();

        let none = MasterSlaves::default();
        assert!(
            tick(&registry, &mut world, Frame::new(0), &none).is_empty(),
            "nothing to share with before a slave exists"
        );
        let with_slave = MasterSlaves::from_pairs([(master, slave)]);
        assert_eq!(
            tick(&registry, &mut world, Frame::new(7), &with_slave),
            vec![Effect::ShareUpgradesWithSlaves]
        );
        assert!(
            tick(&registry, &mut world, Frame::new(14), &with_slave).is_empty(),
            "no re-share while nothing changes"
        );

        let mut gained = UpgradeMask::EMPTY;
        gained.set(UpgradeBit::new(4));
        let mut log = EffectLog::new();
        registry.broadcast_upgrade_into(
            &mut world,
            master,
            &gained,
            Frame::new(20),
            Effects::into_sink(&mut log),
        );
        assert_eq!(
            tick(&registry, &mut world, Frame::new(21), &with_slave),
            vec![Effect::ShareUpgradesWithSlaves]
        );
    }

    #[test]
    fn fires_only_once_its_watched_slave_stops_being_a_living_slave() {
        let module = slave_watcher_behavior_from_fields(
            &fields(&[
                ("RemoveUpgrade", &["1"]),
                ("GrantUpgrade", &["Upgrade_Rebuy"]),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<SlaveWatcherBehavior>(&mut world, "SlaveWatcherBehavior");
        let master = world.spawn().unwrap();
        let slave = world.spawn().unwrap();
        registry
            .attach(&mut world, master, Frame::ZERO, module)
            .unwrap();

        let with_slave = MasterSlaves::from_pairs([(master, slave)]);
        assert!(tick(&registry, &mut world, Frame::new(0), &with_slave).is_empty());
        assert!(
            tick(&registry, &mut world, Frame::new(7), &with_slave).is_empty(),
            "still watching the same live slave"
        );

        let without_slave = MasterSlaves::default();
        let effects = tick(&registry, &mut world, Frame::new(14), &without_slave);
        assert_eq!(effects.len(), 2);
        assert!(matches!(effects[0], Effect::GrantUpgrade(_)));
        assert!(matches!(effects[1], Effect::RemoveUpgrades(..)));

        let respawned = MasterSlaves::from_pairs([(master, world.spawn().unwrap())]);
        assert!(tick(&registry, &mut world, Frame::new(21), &respawned).is_empty());
        let effects_again = tick(&registry, &mut world, Frame::new(28), &without_slave);
        assert_eq!(effects_again.len(), 2, "watches and reacts a second time");
    }
}
