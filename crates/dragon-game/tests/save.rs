use dragon_game::{convert_legacy_xp, dragon_scale, level_of, save::BrowserSave};
use serde_json::json;

#[test]
fn every_xp_threshold_and_legacy_fraction_is_preserved() {
    for level in 1_u8..=20 {
        let xp = 40.0 * f64::from(level - 1).powi(2);
        let old = 15.0 * f64::from(level - 1).powi(2);
        assert_eq!(level_of(xp), level);
        assert_eq!(convert_legacy_xp(old), xp);
        if level > 1 {
            assert_eq!(level_of(xp - 0.0001), level - 1);
        }
        if level < 20 {
            let next = 40.0 * f64::from(level).powi(2);
            let old_next = 15.0 * f64::from(level).powi(2);
            for fraction in [0.25, 0.5, 0.75] {
                let converted = convert_legacy_xp(old + fraction * (old_next - old));
                assert!((converted - (xp + fraction * (next - xp))).abs() < 1e-9);
            }
        }
    }
    assert_eq!(level_of(-100.0), 1);
    assert_eq!(convert_legacy_xp(-100.0), 0.0);
    assert_eq!(convert_legacy_xp(1e9), 14440.0);
    for (xp, scale) in [(0.0, 0.62), (360.0, 0.8), (1960.0, 1.0), (4840.0, 1.18)] {
        assert_eq!(dragon_scale(xp, false), scale);
        assert_eq!(dragon_scale(xp, true), 1.18);
    }
}

#[test]
fn solo_magic_energy_tea_clamping_and_daddio_size() {
    for (magic, regen, maximum) in [
        (-1.0, false, 100.0),
        (0.9, false, 100.0),
        (1.0, true, 100.0),
        (3.9, true, 100.0),
        (4.0, true, 130.0),
        (99.0, true, 130.0),
    ] {
        let raw = json!({"v":3,"xpv":2,"party":[],"skills":{"magic":magic},"craft":{"tea":3.9},"build":{"name":" DaDdIo "}}).to_string();
        let save = BrowserSave::parse(&raw).unwrap();
        let rules = save.solo_rules();
        assert_eq!(rules.magic, regen);
        assert_eq!(rules.max_energy, maximum);
        assert_eq!(rules.tea, 3.0);
        assert!(!rules.mission_done);
        assert_eq!(save.scale(), 1.18);
        assert_eq!(save.original(), raw);
    }
}

#[test]
fn projection_is_conservative_not_browser_numeric_coercion() {
    // Legacy normalizeProfile uses unary + (accepting numeric strings and booleans).
    // This read-only adapter deliberately projects canonical JSON numbers only.
    for value in [json!("4"), json!(true), json!([4]), json!({}), json!(null)] {
        let raw = json!({"v":3,"xpv":2,"party":[{"bonded":true,"xp":value}],"skills":{"magic":value},"craft":{"tea":value}}).to_string();
        let save = BrowserSave::parse(&raw).unwrap();
        assert_eq!(save.bonded_xp(), 0.0);
        assert!(!save.solo_rules().magic);
        assert_eq!(save.solo_rules().tea, 0.0);
        assert_eq!(save.original(), raw);
    }
    let raw = r#"{"v":3,"xpv":2,"party":[{"bonded":"yes","xp":1960},{"bonded":true,"xp":360}]}"#;
    assert_eq!(BrowserSave::parse(raw).unwrap().bonded_xp(), 360.0);
}
