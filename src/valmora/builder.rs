//! Source two-step rider builder on an owned profile copy. Storage/slot admission is external.
use bevy_egui::egui;
use dragon_game::{profile, random::Mulberry32};
use serde_json::{Value, json};

pub struct Builder {
    original: Value,
    draft: Value,
    step: usize,
    pub error: String,
    pub cancel_requested: bool,
}
impl Builder {
    pub fn new(profile: Value, seed: u32) -> Self {
        let mut rng = Mulberry32 { state: seed };
        let mut pick = |values: &[&str]| {
            values[(rng.next_unit() * values.len() as f64).floor() as usize].to_owned()
        };
        let draft = if profile["build"].is_object() {
            profile["build"].clone()
        } else {
            json!({"name":"", "skin":pick(&SKIN),"hair":pick(&HAIR[..6]),"style":pick(&STYLE),"cloak":pick(&CLOAK[..8]),
                "path":"","dname":"","dcolor":pick(&DRAGON),"horn":HORN[0],"breath":0,"dkind":""})
        };
        Self {
            original: profile,
            draft,
            step: 0,
            error: String::new(),
            cancel_requested: false,
        }
    }
    #[cfg(target_arch = "wasm32")]
    pub fn step(&self) -> usize {
        self.step
    }
    fn ready(&self) -> bool {
        let (name, choice, choices): (&str, &str, &[&str]) = if self.step == 0 {
            ("name", "path", &PATH)
        } else {
            ("dname", "dkind", &KIND)
        };
        profile::clean_name(&self.draft[name]).is_ok_and(|n| !n.is_empty())
            && self.draft[choice]
                .as_str()
                .is_some_and(|c| choices.contains(&c))
    }
    fn crowns(&self) -> u32 {
        self.original["crowns"]
            .as_object()
            .into_iter()
            .flat_map(|o| o.values())
            .map(|v| (v.as_f64().unwrap_or(0.) as u64 & 7).count_ones())
            .sum()
    }
    fn allowed(&self, field: &str, value: &Value) -> bool {
        let required = match (field, value) {
            ("horn", v) if v == "#ffcf3f" => 5,
            ("cloak", v) if v == "#d9a520" => 15,
            ("breath", v) if v == 6 => 30,
            ("crown", v) if v == true => 50,
            _ => 0,
        };
        self.crowns() >= required
    }
    fn finish(&self, new_rider_id: &str, now_ms: u64) -> Result<Value, String> {
        if self.step != 1 || !self.ready() {
            return Err("Name your dragon and pick its breath power.".into());
        }
        let build = profile::clean_build(&self.draft).map_err(|e| format!("{e:?}"))?;
        if !build.is_object() {
            return Err("Finish both builder steps first.".into());
        }
        let mut result = self.original.clone();
        let first = result["build"].is_null();
        if first {
            if new_rider_id.is_empty() {
                return Err("A new rider identity is required.".into());
            }
            result["rid"] = json!(new_rider_id);
        }
        let bonded = result["party"]
            .as_array_mut()
            .and_then(|p| p.first_mut())
            .ok_or("Missing bonded dragon")?;
        for (to, from) in [
            ("name", "dname"),
            ("color", "dcolor"),
            ("horn", "horn"),
            ("kind", "dkind"),
        ] {
            bonded[to] = build[from].clone();
        }
        result["last"] = json!({"text":if first {format!("hatched {}",build["dname"].as_str().unwrap_or(""))} else {"changed their look".into()},"at":now_ms});
        result["build"] = build;
        Ok(result)
    }
    fn choices(&mut self, ui: &mut egui::Ui, label: &str, field: &str, values: &[&str]) {
        ui.label(label);
        ui.horizontal_wrapped(|ui| {
            for (index, value) in values.iter().enumerate() {
                let next = json!(value);
                let mut button = egui::Button::new(*value).selected(self.draft[field] == next);
                if let Some(hex) = value.strip_prefix('#')
                    && let Ok(rgb) = u32::from_str_radix(hex, 16)
                {
                    button = egui::Button::new(format!("{}", index + 1))
                        .fill(egui::Color32::from_rgb(
                            (rgb >> 16) as u8,
                            (rgb >> 8) as u8,
                            rgb as u8,
                        ))
                        .selected(self.draft[field] == next);
                }
                if ui
                    .add_enabled(self.allowed(field, &next), button)
                    .on_hover_text(format!("{label} {}", index + 1))
                    .clicked()
                {
                    self.draft[field] = next;
                    if field == "dkind" {
                        self.draft["breath"] = json!(match *value {
                            "flame" => 2,
                            "spark" | "boulder" => 1,
                            _ => 0,
                        });
                    }
                }
            }
        });
    }
    fn breath_colors(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (index, name) in [
                "Blue",
                "Gold",
                "Red",
                "Green",
                "Purple",
                "Pink",
                "Champion Gold",
            ]
            .iter()
            .enumerate()
            {
                if ui
                    .add_enabled(
                        self.allowed("breath", &json!(index)),
                        egui::Button::new(*name).selected(self.draft["breath"] == index),
                    )
                    .clicked()
                {
                    self.draft["breath"] = json!(index);
                }
            }
        });
    }
    pub fn draw_companion(
        &mut self,
        ui: &mut egui::Ui,
        crab_name: &str,
        id: &str,
        now_ms: u64,
    ) -> Option<Value> {
        self.step = 1;
        if self.original["build"].is_null() {
            self.draft["name"] = json!(
                profile::clean_name(&json!(crab_name))
                    .ok()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "Crab".into())
            );
        }
        ui.label(
            "Your crab stays you. Choose the dragon and flight abilities that travel with you.",
        );
        super::art::companion_preview(ui, &self.draft, &self.original);
        ui.label("Companion name");
        let mut name = self.draft["dname"].as_str().unwrap_or("").to_owned();
        if ui.text_edit_singleline(&mut name).changed() {
            self.draft["dname"] = json!(profile::clean_name(&json!(name)).unwrap_or_default());
        }
        self.choices(ui, "Flight discipline", "path", &PATH);
        self.choices(ui, "Dragon breath", "dkind", &KIND);
        self.choices(ui, "Dragon color", "dcolor", &DRAGON);
        self.choices(ui, "Horns", "horn", &HORN);
        self.breath_colors(ui);
        let ready = self.ready()
            && self.draft["path"]
                .as_str()
                .is_some_and(|p| PATH.contains(&p));
        if ui
            .add_enabled(
                ready,
                egui::Button::new(if self.original["build"].is_object() {
                    "Save companion"
                } else {
                    "Bond with this dragon"
                }),
            )
            .clicked()
        {
            match self.finish(id, now_ms) {
                Ok(p) => return Some(p),
                Err(e) => self.error = e,
            }
        }
        if ui.button("Close").clicked() {
            self.cancel_requested = true;
        }
        ui.label(&self.error);
        None
    }
    pub fn draw(&mut self, ui: &mut egui::Ui, id: &str, now_ms: u64) -> Option<Value> {
        ui.heading(if self.step == 0 {
            "Who are you?"
        } else {
            "Your very own dragon"
        });
        ui.label(format!("Step {} of 2", self.step + 1));
        super::art::builder_preview(ui, &self.draft, &self.original, self.step);

        if self.original["build"].is_object() && ui.button("Cancel").clicked() {
            self.cancel_requested = true;
            return None;
        }
        let field = if self.step == 0 { "name" } else { "dname" };
        let mut name = self.draft[field].as_str().unwrap_or("").to_owned();
        if ui.text_edit_singleline(&mut name).changed() {
            let mut units = 0;
            name = name
                .chars()
                .take_while(|ch| {
                    units += ch.len_utf16();
                    units <= 14
                })
                .collect();
            self.draft[field] = json!(name);
        }
        if self.step == 0 {
            self.choices(ui, "Skin", "skin", &SKIN);
            self.choices(ui, "Hair style", "style", &STYLE);
            self.choices(ui, "Hair color", "hair", &HAIR);
            self.choices(ui, "Cloak", "cloak", &CLOAK);
            self.choices(ui, "Your Path and special move", "path", &PATH);
            let mut crown = self.draft["crown"] == true;
            if ui
                .add_enabled(
                    self.allowed("crown", &json!(true)),
                    egui::Checkbox::new(&mut crown, "Wear the golden crown (50 crowns)"),
                )
                .changed()
            {
                self.draft["crown"] = json!(crown);
            }
            if ui
                .add_enabled(self.ready(), egui::Button::new("Next: your dragon"))
                .clicked()
            {
                self.step = 1;
            }
        } else {
            self.choices(ui, "Breath power", "dkind", &KIND);
            self.choices(ui, "Dragon color", "dcolor", &DRAGON);
            self.choices(ui, "Horns", "horn", &HORN);
            self.breath_colors(ui);
            if ui.button("Back").clicked() {
                self.step = 0;
            }
            if ui
                .add_enabled(
                    self.ready(),
                    egui::Button::new(if self.original["build"].is_object() {
                        "Save my changes"
                    } else {
                        "Hatch my dragon!"
                    }),
                )
                .clicked()
            {
                match self.finish(id, now_ms) {
                    Ok(profile) => return Some(profile),
                    Err(e) => self.error = e,
                }
            }
        }
        ui.label(&self.error);
        None
    }
}
const SKIN: [&str; 6] = [
    "#f6d7bc", "#f0c49c", "#dca77c", "#c08456", "#94603c", "#6a4128",
];
const HAIR: [&str; 8] = [
    "#2a1a10", "#5a3418", "#8a4a22", "#c8702e", "#e8c56a", "#ecebe4", "#8a5cff", "#ff6fb5",
];
const STYLE: [&str; 5] = ["short", "long", "braid", "curly", "hood"];
const CLOAK: [&str; 9] = [
    "#2f6f4f", "#2f4f9f", "#7a2f5f", "#8a5a2b", "#9f2f2f", "#2f7f8f", "#5a3f8f", "#3a3f4f",
    "#d9a520",
];
const DRAGON: [&str; 10] = [
    "#3f5ad0", "#2fb6b2", "#d0453f", "#3fa65a", "#8a4fd0", "#e08a2f", "#d04f9a", "#c9a227",
    "#4a4f66", "#e9eef7",
];
const HORN: [&str; 6] = [
    "#ffe7a6", "#f4f4f4", "#2b2b33", "#ff9fcf", "#9fd8ff", "#ffcf3f",
];
const PATH: [&str; 4] = ["shadow", "forge", "storm", "guardian"];
const KIND: [&str; 5] = ["bond", "flame", "spark", "frost", "boulder"];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn creation_requires_both_steps_and_preserves_nonappearance_state() {
        let mut profile = profile::new_profile("rider3");
        profile["earned"] = json!(17);
        let original = profile.clone();
        let mut builder = Builder::new(profile, 42);
        assert!(!builder.ready());
        assert!(builder.finish("new-rider", 100).is_err());
        builder.draft["name"] = json!(" Nova ");
        builder.draft["path"] = json!("forge");
        assert!(builder.ready());
        builder.step = 1;
        assert!(!builder.ready());
        builder.draft["dname"] = json!("Ember");
        builder.draft["dkind"] = json!("flame");
        assert!(builder.finish("", 100).is_err());
        let result = builder.finish("new-rider", 100).unwrap();
        assert_eq!(result["rid"], "new-rider");
        assert_eq!(result["earned"], 17);
        assert_eq!(result["build"]["name"], "Nova");
        assert_eq!(result["party"][0]["kind"], "flame");
        assert_eq!(result["party"][0]["id"], "bond");
        assert_eq!(result["last"], json!({"text":"hatched Ember","at":100}));
        assert_eq!(builder.original, original);
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn finished_builder_round_trips_real_timestamp_without_normalization_loss() {
        use super::super::storage::Store;
        use dragon_game::save_transfer::RiderSlot;
        let mut builder = Builder::new(profile::new_profile("rider3"), 42);
        builder.draft["name"] = json!("Nova");
        builder.draft["path"] = json!("forge");
        builder.draft["dname"] = json!("Ember");
        builder.draft["dkind"] = json!("flame");
        builder.step = 1;
        let profile = builder.finish("fixture-rider", 1_790_000_000_000).unwrap();
        let path = std::env::temp_dir().join(format!(
            "valmora-builder-{}-{}",
            std::process::id(),
            time::OffsetDateTime::now_utc().unix_timestamp_nanos()
        ));
        let store = Store::in_directory(&path);
        let loaded = store.create(RiderSlot::Rider3, profile.clone()).unwrap();
        assert_eq!(loaded.profile(), &profile);
        assert_eq!(
            store.load(RiderSlot::Rider3).unwrap().unwrap().profile(),
            &profile
        );
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn champion_unlocks_count_each_crown_bit_and_edits_preserve_identity() {
        let profile = profile::normalize_profile(&json!({"rid":"existing","build":{"name":"Nova","dname":"Ember","path":"forge"},"crowns":{"m1":7,"m2":3}}),"rider3",2,||"extra".into()).unwrap();
        let mut builder = Builder::new(profile, 42);
        assert!(builder.allowed("horn", &json!("#ffcf3f")));
        assert!(!builder.allowed("cloak", &json!("#d9a520")));
        assert!(!builder.allowed("crown", &json!(true)));
        builder.step = 1;
        let edited = builder.finish("ignored-new-id", 200).unwrap();
        assert_eq!(edited["rid"], "existing");
        assert_eq!(edited["last"]["text"], "changed their look");
    }
}

#[cfg(all(test, target_os = "macos"))]
mod source_tests {
    use super::*;
    #[test]
    #[ignore = "requires macOS JavaScriptCore source oracle"]
    fn original_builder_defaults_and_rng_order_match() {
        let source = include_str!("../../web/dragon/index.html");
        let look_start = source.find("const LOOK = {").unwrap();
        let look_end = source[look_start..]
            .find("// The breath a brand-new dragon")
            .unwrap()
            + look_start;
        let fn_start = source.find("function toBuilder(edit = false)").unwrap();
        let fn_end = source[fn_start..].find("const draftRider").unwrap() + fn_start;
        let mut cases = Vec::new();
        let mut expected = Vec::new();
        for seed in 0..32 {
            let profile = profile::new_profile("rider3");
            let mut rng = Mulberry32 { state: seed };
            let draws: Vec<_> = (0..5).map(|_| rng.next_unit()).collect();
            expected.push(json!({"draft":Builder::new(profile.clone(),seed).draft,"remaining":0}));
            cases.push(json!({"profile":profile,"draws":draws}));
        }
        let script = format!(
            "{} let draft,buildStep,buildEdit,state,me,draws; const renderBuilder=()=>{{}},showOnly=()=>{{}},updateHud=()=>{{}},pick=a=>a[Math.floor(draws.shift()*a.length)]; {} const cases={}; print(JSON.stringify(cases.map(v=>{{me=v.profile;draws=v.draws;toBuilder(false);return {{draft,remaining:draws.length}};}})));",
            &source[look_start..look_end],
            &source[fn_start..fn_end],
            json!(cases)
        );
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        let mut child = Command::new(
            "/System/Library/Frameworks/JavaScriptCore.framework/Versions/A/Helpers/jsc",
        )
        .args(["-e", "eval(JSON.parse(readline()))"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(format!("{}\n", json!(script)).as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let actual: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(expected, actual);
    }
}
