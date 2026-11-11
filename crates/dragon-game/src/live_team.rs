//! Bounded, explicit room projection. Never carries save IDs, archives or preferences.
use serde_json::{Value, json};

const KEYS: &[&str] = &[
    "earned",
    "spent",
    "hatched",
    "released",
    "treatsUsed",
    "craft",
    "cleared",
    "built",
    "build",
    "party",
    "wild",
];
/// Derive from an already normalized local profile. Stable save/dragon IDs never leave.
pub fn project(profile: &Value) -> Value {
    let mut out = json!({});
    for key in KEYS {
        out[*key] = profile[*key].clone();
    }
    out["build"] = crate::profile::clean_build(&profile["build"]).unwrap_or(Value::Null);
    out["party"] = Value::Array(profile["party"].as_array().into_iter().flatten().take(5).map(|d|json!({"name":d["name"],"kind":d["kind"],"color":d["color"],"xp":d["xp"],"bonded":d["bonded"]})).collect());
    out["wild"] = Value::Array(profile["wild"].as_array().into_iter().flatten().take(40).map(|d|json!({"name":d["name"],"kind":d["kind"],"color":d["color"],"realm":d["realm"]})).collect());
    out
}
/// Transport admission supplies rider; payload claims never select a slot.
/// Normalize a copy, never a local save. Invalid updates replace no previous value.
pub fn admit(raw: &Value, rider: usize) -> Option<Value> {
    if rider >= 6 || serde_json::to_vec(raw).ok()?.len() > 12 * 1024 {
        return None;
    }
    let object = raw.as_object()?;
    if object.len() != KEYS.len() || object.keys().any(|k| !KEYS.contains(&k.as_str())) {
        return None;
    }
    for k in ["earned", "spent", "hatched", "released", "treatsUsed"] {
        let n = raw[k].as_f64()?;
        if !(0. ..=1e9).contains(&n) {
            return None;
        }
    }
    let craft = raw["craft"].as_object()?;
    if craft.len() != 6 || raw["craft"]["saddles"].as_bool().is_none() {
        return None;
    }
    for (key, max) in [
        ("treatsMade", 1e9),
        ("tea", 3.),
        ("tonic", 3.),
        ("armor", 3.),
        ("bellows", 3.),
    ] {
        if !(0. ..=max).contains(&raw["craft"][key].as_f64()?) {
            return None;
        }
    }
    for (key, max) in [("party", 5), ("wild", 40), ("cleared", 48), ("built", 32)] {
        if raw[key].as_array()?.len() > max {
            return None;
        }
    }
    if raw["party"].as_array()?.is_empty() {
        return None;
    }
    for d in raw["party"].as_array()? {
        if !(0. ..=1e9).contains(&d["xp"].as_f64()?) || d.get("id").is_some() {
            return None;
        }
    }
    let build = crate::profile::clean_build(&raw["build"]).ok()?;
    if !build.is_object() {
        return None;
    }
    let mut value = raw.clone();
    value["xpv"] = json!(2);
    for (i, d) in value["party"].as_array_mut()?.iter_mut().enumerate() {
        d["id"] = json!(format!("live{rider}d{i}"));
    }
    let key = ["alexa", "tavian", "rider3", "rider4", "rider5", "rider6"][rider];
    let normalized = crate::profile::normalize_profile(&value, key, rider, String::new).ok()?;
    // Normalization removes extensions and validates source names, colors and kinds.
    Some(normalized)
}
