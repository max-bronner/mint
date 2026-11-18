use super::*;

const DATA: &str = "\
Weapon ZTorch\n  AttackRange = 40\n  DelayBetweenShots = 100\n  DeathType = BURNED\n  \
  DamageNugget\n    Damage = 1000\n    DamageType = FLAME\n  End\nEnd\n\
Object ZBurner\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZTorch\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZFader\n  Side = Men\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Fade\n    DeathTypes = ALL\n    \
    FadeDelay = 500\n    DestructionDelay = 2000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZPlain\n  Side = Men\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Plain\n    DeathTypes = ALL\n    \
    DestructionDelay = 2000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn scene() -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZBurner", "ZFader", "ZPlain"];
    let (kinds, missing) = KindTable::closure(
        &data,
        &roots
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    );
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [
        ("ZFader", Vec2::new(300.0, 300.0), PLAYER),
        ("ZPlain", Vec2::new(300.0, 320.0), PLAYER),
        ("ZBurner", Vec2::new(320.0, 310.0), OTHER),
    ]
    .into_iter()
    .map(|(name, at, player)| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                at,
                Facing::EAST,
                player,
            )
            .unwrap()
    })
    .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_teams(vec![None, None]);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let bodies = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let fader = bodies[0];
    let plain = bodies[1];
    (simulation, ids, fader, plain)
}

#[test]
fn a_fading_corpse_holds_opaque_then_fades_linearly_to_its_removal_frame() {
    let (mut simulation, ids, fader, _plain) = scene();
    let opacity_at = |simulation: &Simulation<RotwkRules>| {
        simulation
            .rules()
            .corpse_fade_opacity(simulation.state(), fader, simulation.frame())
    };
    let run_to = |simulation: &mut Simulation<RotwkRules>, frame: u64| {
        while simulation.frame().get() < frame {
            simulation.tick().unwrap();
        }
    };

    let mut death_tick = None;
    for t in 0..300u64 {
        simulation.tick().unwrap();
        if simulation.state().get(ids.doomed, fader).is_some() {
            death_tick = Some(t);
            break;
        }
    }
    let death_tick = death_tick.expect("the fader must die within the test window");

    let start = death_tick + 15;
    let end = death_tick + 60;

    run_to(&mut simulation, start);
    assert_eq!(
        opacity_at(&simulation),
        1.0,
        "opaque up to and including FadeDelay's own frame"
    );

    let quarter = (end - start) / 4;
    run_to(&mut simulation, start + quarter);
    let at_quarter = opacity_at(&simulation);
    assert!(
        (at_quarter - 0.75).abs() < 0.1,
        "about three-quarters opaque a quarter through the fade, got {at_quarter}"
    );

    run_to(&mut simulation, start + (end - start) / 2);
    let halfway = opacity_at(&simulation);
    assert!(
        (halfway - 0.5).abs() < 0.05,
        "about half-faded halfway through, got {halfway}"
    );

    run_to(&mut simulation, end + 1);
    assert!(
        simulation.state().get(ids.doomed, fader).is_none(),
        "the corpse must be removed by its own DestructionDelay, {end}"
    );
}

#[test]
fn a_death_with_no_fade_delay_never_schedules_a_fade() {
    let (mut simulation, ids, _fader, plain) = scene();
    let mut died = false;
    for _ in 0..40u64 {
        simulation.tick().unwrap();
        if simulation.state().get(ids.doomed, plain).is_some() {
            died = true;
            break;
        }
    }
    assert!(died, "ZPlain must have died within the test window");
    for _ in 0..20 {
        simulation.tick().unwrap();
        assert_eq!(
            simulation
                .rules()
                .corpse_fade_opacity(simulation.state(), plain, simulation.frame()),
            1.0,
            "ZPlain declares no FadeDelay and must never fade"
        );
    }
}
