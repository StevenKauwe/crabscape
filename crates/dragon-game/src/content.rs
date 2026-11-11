//! Original Valmora story and interface content, extracted from the JS source.
//! Text is not rewritten. This module does not narrate, capture audio or access storage.
use serde_json::Value;
use std::sync::OnceLock;

pub fn catalog() -> &'static Value {
    static CONTENT: OnceLock<Value> = OnceLock::new();
    CONTENT.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/content.json"))
            .expect("checked source content")
    })
}
pub fn story_page(id: &str) -> Option<&'static Value> {
    catalog()["story"].get(id)
}
pub fn mission_goal(id: &str) -> Option<&'static str> {
    catalog()["mission_goals"]
        .get(id.strip_suffix('c').unwrap_or(id))?
        .as_str()
}
pub fn briefing(id: &str) -> Option<&'static str> {
    catalog()["briefs"].get(id)?.as_str()
}
pub fn story_happened(profile: &Value, id: &str) -> bool {
    id == "intro" || cleared(profile, if id == "part2" { "m12" } else { id })
}
pub fn journal(profile: &Value) -> Vec<&'static str> {
    catalog()["story_order"]
        .as_array()
        .expect("source story order")
        .iter()
        .filter_map(Value::as_str)
        .filter(|id| story_happened(profile, id))
        .collect()
}
pub fn quest_line(profile: &Value) -> String {
    catalog()["quests"]
        .as_array()
        .expect("source quests")
        .iter()
        .find(|q| !cleared(profile, q[0].as_str().expect("quest mission")))
        .map(|q| format!("Your quest: {}", q[1].as_str().expect("quest text")))
        .unwrap_or_else(|| "Every star is home. Keep Valmora safe, Star Guard!".into())
}
/// Base and Champion mission unlock rules. Party launch may explicitly bypass this
/// check, as in the source's `fromParty` path; ordinary selection must call it.
pub fn mission_unlocked(profile: &Value, others: &[Value], id: &str) -> bool {
    let champion = id.ends_with('c');
    let base = if champion {
        id.strip_suffix('c').unwrap_or(id)
    } else {
        id
    };
    let Some(index) = crate::missions::missions()
        .iter()
        .position(|m| m.id == base)
    else {
        return false;
    };
    let required = if champion {
        format!("m{}", (index / 4 + 1) * 4)
    } else if index == 0 {
        return true;
    } else {
        format!("m{index}")
    };
    cleared(profile, &required)
        || others.iter().any(|p| cleared(p, &required))
        || (!champion && cleared(profile, base))
}
fn cleared(profile: &Value, id: &str) -> bool {
    profile["cleared"]
        .as_array()
        .is_some_and(|a| a.iter().any(|v| v == id))
}

/// Authored source ROUTES labels, with the source fallback for longer missions.
pub fn route_stop(id: &str, wave: usize) -> String {
    static ROUTES: OnceLock<Value> = OnceLock::new();
    let routes = ROUTES.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/routes.json")).expect("source routes")
    });
    routes[id][wave][0]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("stop {}", wave + 1))
}
