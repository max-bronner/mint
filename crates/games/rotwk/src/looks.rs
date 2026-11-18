use mint_sage_modules::UpgradeState;
use mint_sage_scene::{LookKey, LookTexture, UnitLook, UnitScene};
use mint_sim::{ComponentId, Entity, World};

pub(crate) fn unit_look(state: &UpgradeState) -> UnitLook {
    UnitLook {
        conditions: state
            .model_condition_names()
            .into_iter()
            .map(Into::into)
            .collect(),
        sub_objects: state
            .sub_objects
            .iter()
            .map(|(name, shown)| (std::sync::Arc::clone(name), *shown))
            .collect(),
        textures: state
            .textures
            .iter()
            .map(|swap| LookTexture {
                from: std::sync::Arc::clone(&swap.from),
                to: std::sync::Arc::clone(&swap.to),
                exclude: swap.exclude.iter().cloned().collect(),
            })
            .collect(),
    }
}

pub(crate) fn look_of(
    world: &World,
    upgrades: Option<ComponentId<UpgradeState>>,
    scene: &mut UnitScene,
    entity: Entity,
) -> LookKey {
    let Some(state) = upgrades.and_then(|id| world.get(id, entity)) else {
        return LookKey::PLAIN;
    };
    if !state.touches_look() {
        return LookKey::PLAIN;
    }
    let stamp = (u64::from(entity.generation()) << 32) | u64::from(state.revision);
    scene.look_for(entity.index(), stamp, || unit_look(state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sage_modules::Effect;

    fn names(items: &[&str]) -> std::sync::Arc<[std::sync::Arc<str>]> {
        items
            .iter()
            .map(|item| std::sync::Arc::from(*item))
            .collect()
    }

    #[test]
    fn a_look_carries_the_upgrade_flags_parts_and_swaps() {
        let mut state = UpgradeState::default();
        state.apply(&Effect::WeaponSetFlag("PLAYER_UPGRADE".into()));
        state.apply(&Effect::SubObjects {
            show: names(&["FireArowTip"]),
            hide: names(&[]),
        });
        state.apply(&Effect::Texture {
            swaps: std::sync::Arc::from([("GUArcher.tga".into(), "GUArcher_HA.tga".into())]),
            exclude: names(&["Bow"]),
        });
        let look = unit_look(&state);
        assert_eq!(look.conditions, vec!["WEAPONSET_PLAYER_UPGRADE".into()]);
        assert_eq!(look.sub_objects, vec![("FireArowTip".into(), true)]);
        assert_eq!(look.textures.len(), 1);
        assert_eq!(&*look.textures[0].to, "GUArcher_HA.tga");
        assert_eq!(look.textures[0].exclude, vec!["Bow".into()]);
    }

    #[test]
    fn an_object_no_upgrade_touched_is_plain_and_a_revision_rebuilds_its_look() {
        let mut world = World::new();
        let id = world.register::<UpgradeState>();
        let archer = world.spawn().unwrap();
        let bare = world.spawn().unwrap();
        world.insert(id, archer, UpgradeState::default()).unwrap();
        let mut scene = UnitScene::new();
        assert_eq!(
            look_of(&world, Some(id), &mut scene, archer),
            LookKey::PLAIN
        );
        assert_eq!(look_of(&world, Some(id), &mut scene, bare), LookKey::PLAIN);
        assert_eq!(look_of(&world, None, &mut scene, archer), LookKey::PLAIN);

        let raise = |world: &mut World, effect: Effect| {
            let mut state = world.get(id, archer).cloned().unwrap();
            state.apply(&effect);
            world.insert(id, archer, state).unwrap();
        };
        raise(
            &mut world,
            Effect::SubObjects {
                show: names(&["Forged_Blade"]),
                hide: names(&[]),
            },
        );
        let blade = look_of(&world, Some(id), &mut scene, archer);
        assert_ne!(blade, LookKey::PLAIN);
        assert_eq!(scene.look(blade).sub_objects.len(), 1);
        raise(&mut world, Effect::WeaponSetFlag("PLAYER_UPGRADE".into()));
        let both = look_of(&world, Some(id), &mut scene, archer);
        assert_ne!(
            both, blade,
            "the new revision was rebuilt, not served stale"
        );
        assert_eq!(
            scene.look(both).conditions,
            vec!["WEAPONSET_PLAYER_UPGRADE".into()]
        );
    }
}
