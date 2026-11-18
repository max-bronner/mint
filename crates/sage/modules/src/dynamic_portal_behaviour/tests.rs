use super::*;
use mint_sim::module::UpgradeBit;
use mint_sim::{Effects, World};

struct TestFields(Vec<(&'static str, Vec<&'static str>)>);

impl TestFields {
    fn new(fields: &[(&'static str, &[&'static str])]) -> Self {
        Self(fields.iter().map(|(k, v)| (*k, v.to_vec())).collect())
    }
}

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

    fn field_args_each(&self, keyword: &str) -> Vec<Vec<&str>> {
        self.0
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case(keyword))
            .map(|(_, v)| v.clone())
            .collect()
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

fn shipped_postern() -> TestFields {
    TestFields::new(&[
        ("TriggeredBy", &["1"]),
        ("ConflictsWith", &["2"]),
        (
            "CustomAnimAndDuration",
            &["AnimState:UPGRADE_POSTERN_GATE", "AnimTime:0"],
        ),
        ("ActivationDelaySeconds", &["7.3"]),
        (
            "ObjectFilter",
            &["ANY", "+INFANTRY", "-MONSTER", "-MACHINE", "-CAVALRY"],
        ),
        ("BonePrefix", &["Post"]),
        ("NumberOfBones", &["4"]),
        ("WayPoint", &["Index:0", "Type:Walk"]),
        ("WayPoint", &["Index:1", "Type:Walk"]),
        ("WayPoint", &["Index:2", "Type:Walk"]),
        ("WayPoint", &["Index:3", "Type:Walk"]),
        ("WayPoint", &["Index:2", "Type:Walk"]),
        ("WayPoint", &["Index:1", "Type:Walk"]),
        ("Link", &["From:0", "Via:4", "Via:5", "To:3"]),
        ("Link", &["From:3", "Via:1", "Via:2", "To:0"]),
    ])
}

#[test]
fn the_shipped_postern_gate_shape_reads_its_admission_and_climb_graph() {
    let module = dynamic_portal_behaviour_from_fields(&shipped_postern(), Frame::ZERO).unwrap();
    assert!(!module.allow_enemies());
    assert_eq!(module.bone_prefix(), Some("Post"));
    assert_eq!(module.number_of_bones(), 4);
    assert_eq!(module.waypoints().len(), 6);
    assert_eq!(module.waypoints()[0].kind, WayPointType::Walk);
    assert_eq!(module.links().len(), 2);
    assert_eq!(module.links()[0].from, 0);
    assert_eq!(module.links()[0].via, vec![4, 5]);
    assert_eq!(module.links()[0].to, 3);
    assert!(module
        .filter()
        .passes(&[String::from("INFANTRY")], "Soldier"));
    assert!(!module.filter().passes(&[String::from("CAVALRY")], "Knight"));
    assert!(
        !module.is_active(Frame::ZERO),
        "not GenerateNow and not yet triggered"
    );
}

#[test]
fn firing_the_mixin_activates_the_passage_only_after_its_own_delay() {
    let mut module = dynamic_portal_behaviour_from_fields(&shipped_postern(), Frame::ZERO).unwrap();
    let mut active = UpgradeMask::EMPTY;
    active.set(UpgradeBit::new(1));
    let mut log = EffectLog::new();
    let mut world = World::new();
    let mut ctx = UpgradeContext {
        entity: world.spawn().unwrap(),
        now: Frame::new(100),
        effects: Effects::into_sink(&mut log),
    };
    module.attempt_upgrade(&active, &mut ctx);

    assert!(!module.is_active(Frame::new(100)));
    assert!(!module.is_active(Frame::new(315)));
    assert!(module.is_active(Frame::new(316)));
}

#[test]
fn generate_now_is_active_immediately_with_no_trigger_at_all() {
    let fields = TestFields::new(&[("GenerateNow", &["Yes"])]);
    let module = dynamic_portal_behaviour_from_fields(&fields, Frame::ZERO).unwrap();
    assert!(module.is_active(Frame::ZERO));
    assert!(module.waypoints().is_empty());
    assert!(module.links().is_empty());
}

#[test]
fn a_malformed_way_point_or_link_is_refused() {
    for bad in [
        vec![("WayPoint", vec!["Index:0"])],
        vec![("WayPoint", vec!["Index:0", "Type:Sideways"])],
        vec![("WayPoint", vec!["Index:many", "Type:Walk"])],
        vec![("Link", vec!["From:0", "Via:1"])],
        vec![("Link", vec!["From:0", "To:soon"])],
    ] {
        let fields = TestFields(bad.into_iter().collect());
        assert!(
            dynamic_portal_behaviour_from_fields(&fields, Frame::ZERO).is_err(),
            "must refuse"
        );
    }
}

#[test]
fn a_wrong_typed_flag_or_number_is_refused() {
    for (key, value) in [
        ("AllowEnemies", "Sideways"),
        ("ActivationDelaySeconds", "soon"),
        ("NumberOfBones", "many"),
        ("TopAttackRadius", "wide"),
    ] {
        let fields = TestFields::new(&[(key, &[value])]);
        assert!(dynamic_portal_behaviour_from_fields(&fields, Frame::ZERO).is_err());
    }
}

#[test]
fn top_attack_pos_reads_the_shipped_siege_shape() {
    let fields = TestFields::new(&[
        ("TopAttackPos", &["X:30", "Y:0", "Z:52"]),
        ("TopAttackRadius", &["30"]),
        ("AboveWall", &["3"]),
    ]);
    let module = dynamic_portal_behaviour_from_fields(&fields, Frame::ZERO).unwrap();
    assert_eq!(module.top_attack_pos(), Some((30.0, 0.0, 52.0)));
    assert_eq!(module.top_attack_radius(), Some(30.0));
    assert_eq!(module.above_wall(), Some(3));
}
