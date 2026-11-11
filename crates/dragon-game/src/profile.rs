//! Pure browser profile projection; never writes or mutates a source save.
//! `normalize_profile` covers the standard schema with explicit errors for the
//! unsupported coercion/surrogate cases documented on that function.
//! `normalize_progression` is also available independently as a field subset.
//! Unknown fields and original bytes remain owned by `BrowserSave`.
use serde_json::{Value, json};

/// JavaScript's ToPrimitive can throw for JSON objects shadowing `toString`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoercionError;

pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}
fn js_space(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}
fn array_string(v: &Value) -> Result<String, CoercionError> {
    Ok(match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .map(array_string)
            .collect::<Result<Vec<_>, _>>()?
            .join(","),
        Value::Object(o) => {
            if o.contains_key("toString") {
                return Err(CoercionError);
            }
            "[object Object]".into()
        }
        _ => v.to_string(),
    })
}
/// Unary `+` for JSON values. `None` represents missing/undefined, not null.
pub fn number(v: Option<&Value>) -> Result<f64, CoercionError> {
    let Some(v) = v else {
        return Ok(f64::NAN);
    };
    let s = match v {
        Value::Null => return Ok(0.0),
        Value::Bool(b) => return Ok(if *b { 1.0 } else { 0.0 }),
        Value::Number(n) => return Ok(n.as_f64().unwrap_or(f64::NAN)),
        Value::String(s) => s.clone(),
        Value::Array(_) => array_string(v)?,
        Value::Object(o) => {
            if o.contains_key("toString") {
                return Err(CoercionError);
            }
            return Ok(f64::NAN);
        }
    };
    let s = s.trim_matches(js_space);
    if s.is_empty() {
        return Ok(0.0);
    }
    if matches!(s, "Infinity" | "+Infinity" | "-Infinity") {
        return Ok(if s.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        });
    }
    for (prefixes, radix) in [(["0x", "0X"], 16), (["0b", "0B"], 2), (["0o", "0O"], 8)] {
        if prefixes.iter().any(|p| s.starts_with(p)) {
            let mut n = 0.0;
            if s.len() == 2 {
                return Ok(f64::NAN);
            }
            for c in s[2..].chars() {
                let Some(d) = c.to_digit(radix) else {
                    return Ok(f64::NAN);
                };
                n = n * f64::from(radix) + f64::from(d);
            }
            return Ok(n);
        }
    }
    if !s
        .bytes()
        .all(|c| c.is_ascii_digit() || b"+-.eE".contains(&c))
    {
        return Ok(f64::NAN);
    }
    Ok(s.parse().unwrap_or(f64::NAN))
}
pub fn num(v: Option<&Value>, max: f64) -> Result<f64, CoercionError> {
    let n = number(v)?;
    Ok(if n.is_finite() {
        n.clamp(0.0, max)
    } else {
        0.0
    })
}
/// The rider key is supplied explicitly; there are no clock, random or storage reads.
pub fn new_profile(rider_key: &str) -> Value {
    json!({"v":3,"rider":rider_key,"build":null,"earned":0,"spent":0,"hatched":0,"released":0,"treatsUsed":0,
        "party":[{"id":"bond","name":"Dragon","kind":"bond","color":"#7f88aa","xp":0,"bonded":true}],"main":"bond",
        "kindsSeen":[],"craft":{"treatsMade":0,"tea":0,"tonic":0,"saddles":false,"armor":0,"bellows":0},"last":null,
        "cleared":[],"eggs":0,"eggAwake":false,"replays":0,"lastRealm":"hollow","wild":[],"seenMs":[],"goldenGiven":false,"goldenPending":false,
        "xpv":2,"seenIntro":false,"stars":0,"skills":{"breath":0,"rider":0,"magic":0},"crowns":{},"seenPart2":false,"briefed":[],"seenGrow":[],"built":[]})
}
fn filtered(v: &Value, valid: impl Fn(&str) -> bool, unique: bool) -> Value {
    let mut result = Vec::new();
    if let Some(a) = v.as_array() {
        for v in a {
            if v.as_str().is_some_and(&valid) && (!unique || !result.contains(v)) {
                result.push(v.clone());
            }
        }
    }
    Value::Array(result)
}
fn mission(s: &str) -> bool {
    (1..=24).any(|i| s == format!("m{i}"))
}
/// XP migration uses JS strict numeric equality for xpv ("2" is legacy).
pub fn normalized_xp(raw: &Value, xp: Option<&Value>) -> Result<f64, CoercionError> {
    let xp = num(xp, 1e9)?;
    Ok(if raw.get("xpv").and_then(Value::as_f64) == Some(2.0) {
        xp
    } else {
        crate::convert_legacy_xp(xp)
    })
}
/// Project progression from an object/array profile and already-migrated bonded XP.
/// Nonobjects return the same subset of `newProfile`, respecting its early return.
/// No build/party normalization or serialization back to browser storage is implied.
pub fn normalize_progression(raw: &Value, bonded_xp: f64) -> Result<Value, CoercionError> {
    let keys = [
        "earned",
        "spent",
        "hatched",
        "released",
        "treatsUsed",
        "replays",
        "kindsSeen",
        "craft",
        "cleared",
        "eggs",
        "eggAwake",
        "lastRealm",
        "seenMs",
        "goldenGiven",
        "goldenPending",
        "xpv",
        "seenIntro",
        "skills",
        "stars",
        "crowns",
        "seenPart2",
        "seenGrow",
        "built",
    ];
    let defaults = new_profile("");
    let mut p = json!({});
    for k in keys {
        p[k] = defaults[k].clone();
    }
    if !raw.is_object() && !raw.is_array() {
        return Ok(p);
    }
    for k in [
        "earned",
        "spent",
        "hatched",
        "released",
        "treatsUsed",
        "replays",
    ] {
        p[k] = json!(num(raw.get(k), 1e9)?);
    }
    p["kindsSeen"] = filtered(
        &raw["kindsSeen"],
        |s| ["flame", "spark", "boulder", "frost"].contains(&s),
        false,
    );
    let c = &raw["craft"];
    p["craft"]["treatsMade"] = json!(num(c.get("treatsMade"), 1e9)?);
    for k in ["tea", "tonic", "armor", "bellows"] {
        p["craft"][k] = json!(num(c.get(k), 3.0)?.floor());
    }
    p["craft"]["saddles"] = json!(truthy(&c["saddles"]));
    p["cleared"] = filtered(&raw["cleared"], mission, true);
    p["eggs"] = json!(num(raw.get("eggs"), 10.0)?.floor());
    for k in ["eggAwake", "goldenGiven", "goldenPending", "seenPart2"] {
        p[k] = json!(truthy(&raw[k]));
    }
    if raw["lastRealm"]
        .as_str()
        .is_some_and(|s| ["hollow", "wood", "frost", "sea", "ember", "stars"].contains(&s))
    {
        p["lastRealm"] = raw["lastRealm"].clone();
    }
    p["seenMs"] = filtered(
        &raw["seenMs"],
        |s| {
            [
                "first", "song", "gifts", "wing", "golden", "festival", "skies",
            ]
            .contains(&s)
        },
        false,
    );
    p["seenIntro"] = json!(
        raw.get("seenIntro")
            .map_or(!p["cleared"].as_array().expect("array").is_empty(), truthy)
    );
    let sk = &raw["skills"];
    for k in ["breath", "rider", "magic"] {
        p["skills"][k] = json!(num(sk.get(k), 5.0)?.floor());
    }
    p["stars"] = if sk.is_object() || sk.is_array() {
        json!(num(raw.get("stars"), 99.0)?.floor())
    } else {
        let bosses = p["cleared"]
            .as_array()
            .expect("array")
            .iter()
            .filter(|v| {
                v.as_str()
                    .is_some_and(|s| ["m4", "m8", "m12", "m16", "m20", "m24"].contains(&s))
            })
            .count();
        json!(usize::from((crate::level_of(bonded_xp) - 1).min(5)) + bosses)
    };
    for i in 1..=24 {
        let k = format!("m{i}");
        let n = num(raw["crowns"].get(&k), 7.0)?.floor();
        if n != 0.0 {
            p["crowns"][k] = json!(n);
        }
    }
    p["seenGrow"] = filtered(
        &raw["seenGrow"],
        |s| ["glider", "grown", "ride", "mighty"].contains(&s),
        false,
    );
    p["built"] = filtered(
        &raw["built"],
        |s| {
            [
                "lanterns",
                "bell",
                "barns",
                "oaks",
                "stream",
                "meadow",
                "bridge",
                "hall",
                "beacon",
                "lighthouse",
                "harbor",
                "pools",
                "garden",
                "spring",
                "path",
                "bridges",
                "flowers",
                "star",
            ]
            .contains(&s)
        },
        true,
    );
    p["charms"] = json!({});
    for k in ["heart", "pouch", "start"] {
        p["charms"][k] = json!(num(raw["charms"].get(k), 1e9)?.min(9.0).floor());
    }
    let d = &raw["daily"];
    let day = number(d.get("day"))?;
    p["daily"] = if (d.is_object() || d.is_array()) && day.is_finite() {
        json!({"day":day,"done":truthy(&d["done"])})
    } else {
        Value::Null
    };
    Ok(p)
}

/// Unsupported string coercion/UTF-16 cases are errors, never silently rewritten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileError {
    NumericCoercion,
    UnsupportedString,
    PrototypeKey,
}
impl From<CoercionError> for ProfileError {
    fn from(_: CoercionError) -> Self {
        Self::NumericCoercion
    }
}
fn text(v: &Value) -> Result<&str, ProfileError> {
    match v {
        Value::Null => Ok(""),
        Value::String(s) => Ok(s),
        _ => Err(ProfileError::UnsupportedString),
    }
}
fn utf16_prefix(s: &str, max: usize) -> Result<String, ProfileError> {
    let mut units = 0;
    let mut out = String::new();
    for c in s.chars() {
        if units == max {
            break;
        }
        units += c.len_utf16();
        if units > max {
            return Err(ProfileError::UnsupportedString);
        }
        out.push(c);
    }
    Ok(out)
}
/// Match the browser's Unicode letter/number categories and UTF-16 length cap.
pub fn clean_name(v: &Value) -> Result<String, ProfileError> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"[^\p{L}\p{N} '-]").expect("constant regex"));
    let s = re.replace_all(text(v)?, "");
    utf16_prefix(s.trim_matches(js_space), 14)
}
pub fn clean_text(v: &Value) -> Result<String, ProfileError> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"[^\p{L}\p{N} '!.,-]").expect("constant regex"));
    utf16_prefix(&re.replace_all(text(v)?, ""), 80)
}
pub fn safe_color<'a>(v: &'a Value, fallback: &'a str) -> &'a str {
    v.as_str()
        .filter(|s| {
            s.len() == 7
                && s.starts_with('#')
                && s.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        })
        .unwrap_or(fallback)
}
fn one(v: &Value, list: &[&str]) -> Value {
    json!(v.as_str().filter(|s| list.contains(s)).unwrap_or(list[0]))
}
fn prototype(s: &str) -> bool {
    [
        "__proto__",
        "constructor",
        "toString",
        "toLocaleString",
        "valueOf",
        "hasOwnProperty",
        "isPrototypeOf",
        "propertyIsEnumerable",
        "__defineGetter__",
        "__defineSetter__",
        "__lookupGetter__",
        "__lookupSetter__",
    ]
    .contains(&s)
}
fn checked_key(v: &Value) -> Result<Option<&str>, ProfileError> {
    if let Some(s) = v.as_str() {
        if prototype(s) {
            return Err(ProfileError::PrototypeKey);
        }
        Ok(Some(s))
    } else if v.is_null() {
        Ok(None)
    } else {
        Err(ProfileError::UnsupportedString)
    }
}
pub fn clean_build(raw: &Value) -> Result<Value, ProfileError> {
    if !raw.is_object() && !raw.is_array() {
        return Ok(Value::Null);
    }
    let name = clean_name(&raw["name"])?;
    if name.is_empty() {
        return Ok(Value::Null);
    }
    let dname = clean_name(&raw["dname"])?;
    if name.is_empty() || dname.is_empty() {
        return Ok(Value::Null);
    }
    let path = checked_key(&raw["path"])?;
    let breath = number(raw.get("breath"))?;
    Ok(json!({"name":name,"dname":dname,
        "skin":one(&raw["skin"], &["#f6d7bc","#f0c49c","#dca77c","#c08456","#94603c","#6a4128"]),
        "hair":one(&raw["hair"], &["#2a1a10","#5a3418","#8a4a22","#c8702e","#e8c56a","#ecebe4","#8a5cff","#ff6fb5"]),
        "style":one(&raw["style"], &["short","long","braid","curly","hood"]),
        "cloak":one(&raw["cloak"], &["#2f6f4f","#2f4f9f","#7a2f5f","#8a5a2b","#9f2f2f","#2f7f8f","#5a3f8f","#3a3f4f","#d9a520"]),
        "path":path.filter(|s| ["shadow","forge","storm","guardian"].contains(s)).unwrap_or("shadow"),
        "dcolor":one(&raw["dcolor"], &["#3f5ad0","#2fb6b2","#d0453f","#3fa65a","#8a4fd0","#e08a2f","#d04f9a","#c9a227","#4a4f66","#e9eef7"]),
        "horn":one(&raw["horn"], &["#ffe7a6","#f4f4f4","#2b2b33","#ff9fcf","#9fd8ff","#ffcf3f"]),
        "breath":if breath.is_nan() { 0.0 } else { breath.clamp(0.0,6.0).floor() },
        "dkind":one(&raw["dkind"], &["bond","flame","spark","frost","boulder"]),"crown":raw["crown"].as_bool()==Some(true)}))
}
fn kind_color(k: &str) -> Option<&'static str> {
    match k {
        "bond" => Some("#ff6a3d"),
        "flame" => Some("#ff6a3d"),
        "spark" => Some("#e8b923"),
        "boulder" => Some("#9a7350"),
        "frost" => Some("#6cc6f0"),
        "star" => Some("#e8c25a"),
        _ => None,
    }
}
fn base36(mut n: u32) -> String {
    if n == 0 {
        return "0".into();
    }
    let mut s = Vec::new();
    while n != 0 {
        s.push(char::from_digit(n % 36, 36).expect("base36 digit"));
        n /= 36;
    }
    s.into_iter().rev().collect()
}
/// Complete standard-save field projection. Returns errors for unsupported
/// UTF-16 surrogate splits, non-string text/lookup coercions, and inherited prototype keys.
/// `fallback_id` supplies the entire `d`+random-base36 ID, called once per missing
/// or falsy extra-dragon ID, even for extras later truncated away. No wall clock
/// is needed: normalizeProfile uses deterministic oldRiderId, not newRiderId.
pub fn normalize_profile(
    raw: &Value,
    rider_key: &str,
    rider_index: usize,
    mut fallback_id: impl FnMut() -> String,
) -> Result<Value, ProfileError> {
    let mut p = new_profile(rider_key);
    if !raw.is_object() && !raw.is_array() {
        return Ok(p);
    }
    let build = clean_build(&raw["build"])?;
    let list = raw["party"].as_array().map_or(&[][..], Vec::as_slice);
    let bonded = list.iter().find(|d| truthy(&d["bonded"]));
    let xp = normalized_xp(raw, bonded.and_then(|d| d.get("xp")))?;
    let mut party = vec![
        json!({"id":"bond","name":build.get("dname").cloned().unwrap_or(json!("Dragon")),"kind":build.get("dkind").cloned().unwrap_or(json!("bond")),"color":build.get("dcolor").cloned().unwrap_or(json!("#7f88aa")),"xp":xp,"bonded":true}),
    ];
    if !build.is_null() {
        party[0]["horn"] = build["horn"].clone();
    }
    let mut extras = 0;
    for d in list {
        if (!d.is_object() && !d.is_array()) || truthy(&d["bonded"]) {
            continue;
        }
        let Some(kind) = checked_key(&d["kind"])? else {
            continue;
        };
        let Some(color) = kind_color(kind) else {
            continue;
        };
        if kind == "bond" {
            continue;
        }
        let supplied = if truthy(&d["id"]) {
            text(&d["id"])?.to_owned()
        } else {
            fallback_id()
        };
        let mut id: String = supplied
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(24)
            .collect();
        if id.is_empty() {
            id = format!("d{extras}");
        }
        let name = clean_name(&d["name"])?;
        let extra = json!({"id":id,"name":if name.is_empty() {"Dragon"} else {&name},"kind":kind,"color":safe_color(&d["color"], color),"xp":normalized_xp(raw,d.get("xp"))?,"bonded":false});
        if extras < 4 {
            party.push(extra);
        }
        extras += 1;
    }
    let progression = normalize_progression(raw, xp)?;
    for (k, v) in progression.as_object().expect("projection object") {
        p[k] = v.clone();
    }
    p["main"] = if party.iter().any(|d| d["id"] == raw["main"]) {
        raw["main"].clone()
    } else {
        json!("bond")
    };
    p["party"] = json!(party);
    p["last"] = if raw["last"].is_object() || raw["last"].is_array() {
        json!({"text":clean_text(&raw["last"]["text"])?,"at":num(raw["last"].get("at"),1e9)?})
    } else {
        Value::Null
    };
    let wild = raw["wild"].as_array().map_or(&[][..], Vec::as_slice);
    let mut result = Vec::new();
    for w in wild.iter().skip(wild.len().saturating_sub(40)) {
        if !truthy(w) {
            continue;
        }
        let Some(kind) = checked_key(&w["kind"])? else {
            continue;
        };
        let Some(color) = kind_color(kind) else {
            continue;
        };
        let name = clean_name(&w["name"])?;
        let realm = w["realm"]
            .as_str()
            .filter(|s| ["hollow", "wood", "frost", "sea", "ember", "stars"].contains(s))
            .unwrap_or("hollow");
        result.push(json!({"name":if name.is_empty(){"Wild dragon"}else{&name},"kind":kind,"color":safe_color(&w["color"],color),"realm":realm,"from":rider_index}));
    }
    p["wild"] = json!(result);
    p["rid"] = if raw["rid"].as_str().is_some_and(|s| !s.is_empty()) {
        json!(utf16_prefix(text(&raw["rid"])?, 40)?)
    } else if !build.is_null() {
        let text = format!(
            "{rider_key}|{}|{}",
            build["name"].as_str().expect("name"),
            build["dname"].as_str().expect("dname")
        );
        let hash = text.chars().fold(5381_u32, |h, c| {
            h.wrapping_mul(33)
                .wrapping_add(u32::from(c.encode_utf16(&mut [0; 2])[0]))
        });
        json!(format!("o{}", base36(hash)))
    } else {
        Value::Null
    };
    for field in ["briefed", "loadout"] {
        let mut values = Vec::new();
        if let Some(a) = raw[field].as_array() {
            for v in a {
                let Some(s) = checked_key(v)? else {
                    continue;
                };
                let valid = if field == "briefed" {
                    mission(s)
                } else {
                    [
                        "naina", "thuri", "kaelo", "vara", "sila", "mira", "senn", "faro",
                    ]
                    .contains(&s)
                };
                if valid && (field == "briefed" || !values.contains(v)) {
                    values.push(v.clone());
                }
            }
        }
        if field == "loadout" {
            values.truncate(3);
        }
        p[field] = json!(values);
    }
    p["build"] = build;
    Ok(p)
}
