use dragon_game::{
    save::{BrowserSave, StorageArchive},
    save_transfer::*,
};
use serde_json::{Value, json};
fn fixture() -> (StorageArchive, BrowserSave, Value) {
    let source = r#" { "v":3, "xpv":2, "rider":"alexa", "rid":"rider-identity", "build":null,
      "party":[{"id":"bond","name":"Dragon","kind":"bond","xp":360,"bonded":true,"futureDragon":{"keep":[1,2]}},
               {"id":"extra1","name":"Frosty","kind":"frost","xp":40,"bonded":false,"futureDragon":"extra one","horn":"obsolete canonical horn"},
               {"id":"extra2","name":"Sparky","kind":"spark","xp":80,"bonded":false,"futureDragon":"extra two"}],
      "earned":12, "crowns":{"m1":7}, "future":{"nested":[true,null,"opaque"]}, "updatedAt":123
    } "#;
    let save = BrowserSave::parse(source).unwrap();
    let archive_source = format!(
        " {{ \"vdf-save-alexa\":{}, \"vdf-save-tavian\":\"unrelated exact bytes\", \"vdf-frozen\":\"{{ arbitrary }}\", \"future-key\":\"opaque \\n\" }} ",
        serde_json::to_string(source).unwrap()
    );
    let archive = StorageArchive::parse(&archive_source).unwrap();
    let profile = save
        .normalized_profile("alexa", 0, || panic!("no fallback IDs"))
        .unwrap();
    (archive, save, profile)
}
#[test]
fn changed_canonical_fields_win_and_unknowns_survive_without_resurrecting_removed_data() {
    let (archive, save, mut profile) = fixture();
    profile["earned"] = json!(99);
    profile["crowns"] = json!({});
    profile["party"][0]["xp"] = json!(1960);
    // Drop extra2; do not resurrect its old gameplay state or extensions.
    profile["party"].as_array_mut().unwrap().remove(2);
    profile["party"].as_array_mut().unwrap().push(json!({"id":"newExtra","name":"Newbie","kind":"flame","color":"#ff6a3d","xp":0,"bonded":false}));
    let plan = plan_replacement(&archive, RiderSlot::Alexa, &save, &profile, 456.0).unwrap();
    assert_eq!(plan.slot(), RiderSlot::Alexa);
    let exported = plan.apply(&archive).unwrap();
    let rider = exported.rider("vdf-save-alexa").unwrap().unwrap();
    let raw = rider.raw();
    assert_eq!(raw["earned"], 99.0);
    assert_eq!(raw["party"][0]["xp"], 1960.0);
    assert_eq!(raw["crowns"], json!({}));
    assert_eq!(raw["future"], save.raw()["future"]);
    assert_eq!(raw["party"][0]["futureDragon"], json!({"keep":[1,2]}));
    assert_eq!(raw["party"][1]["futureDragon"], "extra one");
    assert!(raw["party"][1].get("horn").is_none());
    assert!(raw["party"][2].get("futureDragon").is_none());
    assert_eq!(raw["updatedAt"], 456.0);
    for (k, v) in archive.entries() {
        if k != "vdf-save-alexa" {
            assert_eq!(&exported.entries()[k], v);
        }
    }
    assert_eq!(plan.backup_archive(), archive.original());
    assert_eq!(plan.backup_rider(), save.original());
    assert_eq!(
        archive.rider("vdf-save-alexa").unwrap().unwrap().original(),
        save.original(),
        "planning/apply never mutate inputs"
    );
}
#[test]
fn exact_recovery_roundtrip_and_repeatable_export() {
    let (archive, save, profile) = fixture();
    let plan = plan_replacement(&archive, RiderSlot::Alexa, &save, &profile, 456.0).unwrap();
    let output = plan.apply(&archive).unwrap();
    assert_eq!(output.original(), plan.replacement_archive());
    assert_eq!(output.entries()["vdf-save-alexa"], plan.replacement_rider());
    let recovered = plan.recover(&output).unwrap();
    assert_eq!(
        recovered.original().as_bytes(),
        archive.original().as_bytes()
    );
    assert_eq!(
        recovered.entries()["vdf-save-alexa"].as_bytes(),
        save.original().as_bytes()
    );
    assert_eq!(
        plan.apply(&recovered).unwrap().original(),
        output.original()
    );
    // A subsequent transfer may normalize the exported copy and preserve its extensions again.
    let saved = output.rider("vdf-save-alexa").unwrap().unwrap();
    let normalized = saved.normalized_profile("alexa", 0, || panic!()).unwrap();
    let second = plan_replacement(&output, RiderSlot::Alexa, &saved, &normalized, 789.0).unwrap();
    assert_eq!(second.backup_archive(), output.original());
}
#[test]
fn stale_sources_cross_slot_and_recovery_conflicts_are_non_destructive() {
    let (archive, save, profile) = fixture();
    let plan = plan_replacement(&archive, RiderSlot::Alexa, &save, &profile, 456.0).unwrap();
    let mut entries = archive.entries().clone();
    entries.insert("future-key".into(), "concurrent change".into());
    let changed = StorageArchive::parse(&serde_json::to_string(&entries).unwrap()).unwrap();
    assert_eq!(
        plan.apply(&changed).unwrap_err(),
        TransferError::StaleOriginal
    );
    assert_eq!(
        plan.recover(&archive).unwrap_err(),
        TransferError::RecoveryConflict
    );
    let stale = BrowserSave::parse(&format!("{} ", save.original())).unwrap();
    assert_eq!(
        plan_replacement(&archive, RiderSlot::Alexa, &stale, &profile, 456.0).unwrap_err(),
        TransferError::StaleOriginal
    );
    assert_eq!(
        plan_replacement(&archive, RiderSlot::Rider6, &save, &profile, 456.0).unwrap_err(),
        TransferError::MissingSlot
    );
    let mut wrong = profile.clone();
    wrong["rider"] = json!("tavian");
    assert_eq!(
        plan_replacement(&archive, RiderSlot::Alexa, &save, &wrong, 456.0).unwrap_err(),
        TransferError::WrongRider
    );
    wrong = profile.clone();
    wrong["rid"] = json!("other-rider");
    assert_eq!(
        plan_replacement(&archive, RiderSlot::Alexa, &save, &wrong, 456.0).unwrap_err(),
        TransferError::ChangedIdentity
    );
    assert_eq!(changed.entries()["future-key"], "concurrent change");
}
#[test]
fn rejects_unsupported_malformed_and_noncanonical_profiles_without_repairs() {
    let (archive, save, profile) = fixture();
    for bad in [f64::NAN, f64::INFINITY, -1.0] {
        assert_eq!(
            plan_replacement(&archive, RiderSlot::Alexa, &save, &profile, bad).unwrap_err(),
            TransferError::InvalidTimestamp
        );
    }
    for (field, value, error) in [
        ("v", json!(4), TransferError::UnsupportedVersion),
        ("xpv", json!(3), TransferError::UnsupportedVersion),
        ("party", json!(null), TransferError::MalformedProfile),
        ("earned", json!(-1), TransferError::NotNormalized),
        ("unexpected", json!(true), TransferError::NotNormalized),
        ("skills", json!({"magic":999}), TransferError::NotNormalized),
    ] {
        let mut bad = profile.clone();
        bad[field] = value;
        assert_eq!(
            plan_replacement(&archive, RiderSlot::Alexa, &save, &bad, 456.0).unwrap_err(),
            error
        );
    }
    let mut duplicate = profile.clone();
    duplicate["party"][1]["id"] = json!("bond");
    assert_eq!(
        plan_replacement(&archive, RiderSlot::Alexa, &save, &duplicate, 456.0).unwrap_err(),
        TransferError::AmbiguousDragonIdentity
    );
    assert!(BrowserSave::parse(r#"{"v":4,"party":[]}"#).is_err());
    assert!(StorageArchive::parse(r#"{"vdf-save-alexa":{}}"#).is_err());
    assert_eq!(
        archive.rider("vdf-save-alexa").unwrap().unwrap().original(),
        save.original()
    );
}
#[test]
fn original_ambiguous_dragons_and_cross_slot_identity_are_rejected() {
    for (field, value, error) in [
        ("rider", json!("tavian"), TransferError::WrongRider),
        ("party", json!([null]), TransferError::MalformedProfile),
        (
            "party",
            json!([{"id":"a","kind":"frost"},{"id":"a","kind":"spark"}]),
            TransferError::AmbiguousDragonIdentity,
        ),
        (
            "party",
            json!([{"kind":"frost"}]),
            TransferError::AmbiguousDragonIdentity,
        ),
    ] {
        let (_, save, profile) = fixture();
        let mut raw = save.raw().clone();
        raw[field] = value;
        let save = BrowserSave::parse(&raw.to_string()).unwrap();
        let archive =
            StorageArchive::parse(&json!({"vdf-save-alexa":save.original()}).to_string()).unwrap();
        assert_eq!(
            plan_replacement(&archive, RiderSlot::Alexa, &save, &profile, 1.0).unwrap_err(),
            error
        );
    }
}

#[test]
fn stable_ids_survive_reordering_and_legacy_xp_export_is_explicit() {
    let (archive, save, mut profile) = fixture();
    profile["party"].as_array_mut().unwrap().swap(1, 2);
    let plan = plan_replacement(&archive, RiderSlot::Alexa, &save, &profile, 1.0).unwrap();
    let raw: Value = serde_json::from_str(plan.replacement_rider()).unwrap();
    assert_eq!(raw["party"][1]["id"], "extra2");
    assert_eq!(raw["party"][1]["futureDragon"], "extra two");
    assert_eq!(raw["party"][2]["id"], "extra1");
    assert_eq!(raw["party"][2]["futureDragon"], "extra one");
    let old =
        r#" {"v":3,"rider":"tavian","party":[{"id":"bond","xp":37.5,"bonded":true}],"future":17} "#;
    let saved = BrowserSave::parse(old).unwrap();
    let archive = StorageArchive::parse(&json!({"vdf-save-tavian":old}).to_string()).unwrap();
    let profile = saved.normalized_profile("tavian", 1, || panic!()).unwrap();
    assert_eq!(profile["party"][0]["xp"], 100.0);
    let plan = plan_replacement(&archive, RiderSlot::Tavian, &saved, &profile, 1.0).unwrap();
    let output = plan.apply(&archive).unwrap();
    let saved = output.rider("vdf-save-tavian").unwrap().unwrap();
    assert_eq!(saved.raw()["xpv"], 2);
    assert_eq!(saved.raw()["party"][0]["xp"], 100.0);
    assert_eq!(plan.backup_rider(), old);
    assert_eq!(saved.raw()["future"], 17);
}

#[test]
fn missing_original_rid_uses_source_derived_identity_not_a_new_identity() {
    let (_, save, _) = fixture();
    let mut raw = save.raw().clone();
    raw.as_object_mut().unwrap().remove("rid");
    raw["build"] = json!({"name":"Alex","dname":"Bright"});
    let save = BrowserSave::parse(&raw.to_string()).unwrap();
    let archive =
        StorageArchive::parse(&json!({"vdf-save-alexa":save.original()}).to_string()).unwrap();
    let mut profile = save.normalized_profile("alexa", 0, || panic!()).unwrap();
    assert!(profile["rid"].as_str().unwrap().starts_with('o'));
    let identity = profile["rid"].clone();
    profile["build"]["name"] = json!("New Name");
    let plan = plan_replacement(&archive, RiderSlot::Alexa, &save, &profile, 1.0).unwrap();
    let exported: Value = serde_json::from_str(plan.replacement_rider()).unwrap();
    assert_eq!(exported["rid"], identity);
    assert_eq!(exported["build"]["name"], "New Name");
    assert!(save.raw().get("rid").is_none());
}

#[test]
fn live_builder_and_completion_preserve_real_timestamps_and_optional_absence() {
    use dragon_game::{profile, roost};
    let now = 1_790_000_000_123_u64;
    // The actual builder starts with new_profile, not normalize_profile, and
    // copies these four cleaned build fields into the bonded dragon.
    let mut runtime = profile::new_profile("alexa");
    let build =
        profile::clean_build(&json!({"name":"Nova","dname":"Ember","path":"forge","dkind":"fire"}))
            .unwrap();
    runtime["rid"] = json!("synthetic-new-rider");
    for (to, from) in [
        ("name", "dname"),
        ("color", "dcolor"),
        ("horn", "horn"),
        ("kind", "dkind"),
    ] {
        runtime["party"][0][to] = build[from].clone();
    }
    runtime["build"] = build;
    runtime["last"] = json!({"text":"hatched Ember","at":now});
    assert!(runtime.get("loadout").is_none());
    let original_text = serde_json::to_string(&runtime).unwrap();
    let original = BrowserSave::parse(&original_text).unwrap();
    let archive = StorageArchive::parse(
        &serde_json::to_string(&std::collections::BTreeMap::from([(
            "vdf-save-alexa",
            &original_text,
        )]))
        .unwrap(),
    )
    .unwrap();
    let initial =
        plan_replacement(&archive, RiderSlot::Alexa, &original, &runtime, now as f64).unwrap();
    let initial_value: Value = serde_json::from_str(initial.replacement_rider()).unwrap();
    assert_eq!(initial_value["last"]["at"], now);
    assert!(initial_value.get("loadout").is_none());
    roost::complete_mission(
        &mut runtime,
        &[],
        false,
        &roost::Completion {
            mission_id: "m1".into(),
            champion: false,
            ok: true,
            carry: 7.,
            banked: 0.,
            bonks: 0,
            elapsed: 30.,
            par: 90.,
            had_objective: false,
            day: 20_740,
            now_ms: now + 1000,
        },
    )
    .unwrap();
    let completed = plan_replacement(
        &archive,
        RiderSlot::Alexa,
        &original,
        &runtime,
        (now + 1000) as f64,
    )
    .unwrap();
    let value: Value = serde_json::from_str(completed.replacement_rider()).unwrap();
    assert_eq!(value["last"], runtime["last"]);
    assert_eq!(value["earned"], runtime["earned"]);
    assert_eq!(completed.backup_rider(), original_text);
    let mut bad = runtime.clone();
    bad["last"]["at"] = json!("1790000000123");
    assert!(plan_replacement(&archive, RiderSlot::Alexa, &original, &bad, 0.).is_err());
    bad["last"]["at"] = json!(9_007_199_254_740_992_u64);
    assert!(plan_replacement(&archive, RiderSlot::Alexa, &original, &bad, 0.).is_err());
    bad["last"]["at"] = json!(-1);
    assert!(plan_replacement(&archive, RiderSlot::Alexa, &original, &bad, 0.).is_err());
}

#[test]
fn omitted_loadout_cannot_resurrect_an_original_loadout() {
    let (archive, save, mut profile) = fixture();
    // Even an empty explicit original loadout must not be silently dropped.
    let mut raw = save.raw().clone();
    raw["loadout"] = json!(["naina"]);
    let text = serde_json::to_string(&raw).unwrap();
    let save = BrowserSave::parse(&text).unwrap();
    let mut entries = archive.entries().clone();
    entries.insert("vdf-save-alexa".into(), text);
    let archive = StorageArchive::parse(&serde_json::to_string(&entries).unwrap()).unwrap();
    profile.as_object_mut().unwrap().remove("loadout");
    assert_eq!(
        plan_replacement(&archive, RiderSlot::Alexa, &save, &profile, 0.).unwrap_err(),
        TransferError::NotNormalized
    );
}
