//! Playback-only narrator. It never accesses recording or microphone APIs.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Progress {
    pub speaking: bool,
    pub word: Option<usize>,
    pub active_range: Option<(usize, usize)>,
    pub done_to: usize,
    pub failed: bool,
}
#[derive(Clone, Debug)]
pub struct Voice {
    pub name: String,
    pub language: String,
    pub default: bool,
    pub local: bool,
}
pub fn voice_score(v: &Voice) -> i32 {
    let lower = v.name.to_lowercase();
    let lang = v.language.to_lowercase().replace('_', "-");
    let favorites = [
        "Samantha",
        "Ava",
        "Allison",
        "Zoe",
        "Nicky",
        "Karen",
        "Moira",
        "Tessa",
        "Daniel",
        "Aria",
        "Jenny",
        "Google US English",
        "Google UK English Female",
    ];
    let odd = [
        "compact",
        "eloquence",
        "albert",
        "bad news",
        "bahh",
        "bells",
        "boing",
        "bubbles",
        "cellos",
        "deranged",
        "good news",
        "hysterical",
        "jester",
        "junior",
        "kathy",
        "organ",
        "ralph",
        "trinoids",
        "whisper",
        "wobble",
        "zarvox",
        "grandma",
        "grandpa",
        "rocko",
        "shelley",
        "sandy",
        "flo",
        "eddy",
        "reed",
        "fred",
        "superstar",
    ];
    let mut score = if ["premium", "enhanced", "natural", "neural", "siri"]
        .iter()
        .any(|s| lower.contains(s))
    {
        30
    } else {
        0
    };
    if let Some(i) = favorites.iter().position(|f| v.name.contains(f)) {
        score += 20 - i as i32;
    }
    if lang.contains("en-us") {
        score += 5;
    } else if ["en-gb", "en-au", "en-ie"].iter().any(|s| lang.contains(s)) {
        score += 3;
    }
    if odd.iter().any(|s| lower.contains(s)) {
        score -= 40;
    }
    score + i32::from(v.default) * 2 + i32::from(v.local)
}
pub fn preferred_voice(voices: &[Voice]) -> Option<usize> {
    let english = voices
        .iter()
        .any(|v| v.language.to_lowercase().starts_with("en"));
    voices
        .iter()
        .enumerate()
        .filter(|(_, v)| !english || v.language.to_lowercase().starts_with("en"))
        .max_by(|(ai, a), (bi, b)| voice_score(a).cmp(&voice_score(b)).then_with(|| bi.cmp(ai)))
        .map(|(i, _)| i)
}
#[cfg(target_arch = "wasm32")]
mod browser {
    use super::*;
    use std::{cell::RefCell, rc::Rc};
    use wasm_bindgen::{JsCast, JsValue, closure::Closure};
    fn get(o: &JsValue, k: &str) -> JsValue {
        js_sys::Reflect::get(o, &JsValue::from_str(k)).unwrap_or(JsValue::UNDEFINED)
    }
    fn set(o: &JsValue, k: &str, v: &JsValue) {
        let _ = js_sys::Reflect::set(o, &JsValue::from_str(k), v);
    }
    fn call(o: &JsValue, k: &str) -> Result<JsValue, JsValue> {
        get(o, k).dyn_into::<js_sys::Function>()?.call0(o)
    }
    struct Utterance {
        value: JsValue,
        _callbacks: Vec<Closure<dyn FnMut(JsValue)>>,
    }
    impl Drop for Utterance {
        fn drop(&mut self) {
            for key in ["onstart", "onboundary", "onend", "onerror"] {
                set(&self.value, key, &JsValue::NULL);
            }
        }
    }
    #[derive(Default)]
    pub struct Narrator {
        utterances: Vec<Utterance>,
        clip: Option<web_sys::HtmlAudioElement>,
        progress: Rc<RefCell<Progress>>,
        deadline: f64,
        total: usize,
    }
    impl Narrator {
        pub fn stop(&mut self) {
            if let Some(a) = self.clip.take() {
                let _ = a.pause();
            }
            if let Some(w) = web_sys::window() {
                let _ = call(&get(&w.into(), "speechSynthesis"), "cancel");
            }
            self.utterances.clear();
            self.deadline = 0.;
            self.progress = Rc::new(RefCell::new(Progress::default()));
        }
        pub fn snapshot(&mut self) -> Progress {
            if let Some(a) = &self.clip {
                let duration = a.duration();
                let mut p = self.progress.borrow_mut();
                if a.ended() {
                    p.speaking = false;
                    p.done_to = self.total;
                    p.word = None;
                } else if duration.is_finite() && duration > 0. {
                    p.word = (self.total > 0).then(|| {
                        ((a.current_time() / duration * self.total as f64) as usize)
                            .min(self.total.saturating_sub(1))
                    });
                    p.done_to = p.word.unwrap_or(0);
                }
            }
            if self.deadline > 0. && js_sys::Date::now() > self.deadline {
                let mut p = self.progress.borrow_mut();
                p.speaking = false;
                p.done_to = self.total;
                p.word = None;
                p.active_range = None;
            }
            // Source finish() invalidates its sequence. Detach completed callbacks
            // too, so a late boundary/start cannot restore stale highlights.
            if !self.progress.borrow().speaking {
                self.utterances.clear();
                self.deadline = 0.;
            }
            self.progress.borrow().clone()
        }
        /// Caller supplies only a verified story clip URL, never a filesystem save path.
        pub fn clip(&mut self, url: &str, words: usize) -> bool {
            self.stop();
            let Ok(a) = web_sys::HtmlAudioElement::new_with_src(url) else {
                return false;
            };
            self.total = words;
            self.progress.borrow_mut().speaking = true;
            // Retained handlers cover load/decode failures without trapping Continue.
            for key in ["onended", "onerror"] {
                let state = self.progress.clone();
                let failed = key == "onerror";
                let cb = Closure::wrap(Box::new(move |_: JsValue| {
                    let mut p = state.borrow_mut();
                    p.speaking = false;
                    p.failed = failed;
                    p.done_to = words;
                    p.word = None;
                }) as Box<dyn FnMut(JsValue)>);
                set(a.as_ref(), key, cb.as_ref());
                self.utterances.push(Utterance {
                    value: a.clone().into(),
                    _callbacks: vec![cb],
                });
            }
            if let Ok(promise) = a.play() {
                let state = self.progress.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    if wasm_bindgen_futures::JsFuture::from(promise).await.is_err() {
                        let mut p = state.borrow_mut();
                        p.speaking = false;
                        p.failed = true;
                    }
                });
            } else {
                self.progress.borrow_mut().speaking = false;
                return false;
            }
            self.clip = Some(a);
            true
        }
        pub fn speak(&mut self, text: &str) -> bool {
            self.stop();
            let Some(w) = web_sys::window() else {
                return false;
            };
            let synth = get(&w.clone().into(), "speechSynthesis");
            let Ok(constructor) =
                get(&w.into(), "SpeechSynthesisUtterance").dyn_into::<js_sys::Function>()
            else {
                return false;
            };
            let Ok(say) = get(&synth, "speak").dyn_into::<js_sys::Function>() else {
                return false;
            };
            let raw = call(&synth, "getVoices").unwrap_or(JsValue::UNDEFINED);
            let voices = if raw.is_object() {
                js_sys::Array::from(&raw)
            } else {
                js_sys::Array::new()
            };
            let descriptors: Vec<Voice> = voices
                .iter()
                .map(|v| Voice {
                    name: get(&v, "name").as_string().unwrap_or_default(),
                    language: get(&v, "lang").as_string().unwrap_or_default(),
                    default: get(&v, "default").as_bool().unwrap_or(false),
                    local: get(&v, "localService").as_bool().unwrap_or(false),
                })
                .collect();
            let voice = preferred_voice(&descriptors).map(|i| voices.get(i as u32));
            let pieces = super::super::narration_pieces(text);
            self.total = pieces
                .iter()
                .map(|p| p.text.split_whitespace().count())
                .sum();
            if pieces.is_empty() {
                return false;
            }
            self.deadline = js_sys::Date::now() + self.total as f64 * 600. + 5000.;
            self.progress.borrow_mut().speaking = true;
            let mut w0 = 0;
            for (index, piece) in pieces.iter().enumerate() {
                let args = js_sys::Array::new();
                args.push(&JsValue::from_str(&piece.text));
                let Ok(u) = js_sys::Reflect::construct(&constructor, &args) else {
                    self.stop();
                    return false;
                };
                let w1 = w0 + piece.text.split_whitespace().count();
                if let Some(v) = &voice {
                    set(&u, "voice", v);
                }
                for (key, val) in [("volume", 1.), ("rate", piece.rate), ("pitch", piece.pitch)] {
                    set(&u, key, &JsValue::from_f64(f64::from(val)));
                }
                let mut callbacks = Vec::new();
                for key in ["onstart", "onboundary", "onend", "onerror"] {
                    let state = self.progress.clone();
                    let text = piece.text.clone();
                    let last = index + 1 == pieces.len();
                    let cb = Closure::wrap(Box::new(move |event: JsValue| {
                        let mut p = state.borrow_mut();
                        if !p.speaking {
                            return;
                        }
                        match key {
                            "onstart" => p.active_range = Some((w0, w1)),
                            "onboundary" => {
                                let name = get(&event, "name").as_string().unwrap_or_default();
                                if name.is_empty() || name == "word" {
                                    let index =
                                        get(&event, "charIndex").as_f64().unwrap_or(0.) as usize;
                                    let prefix = String::from_utf16_lossy(
                                        &text.encode_utf16().take(index).collect::<Vec<_>>(),
                                    );
                                    p.word = Some(w0 + prefix.split_whitespace().count());
                                }
                            }
                            "onend" => {
                                p.done_to = w1;
                                p.active_range = None;
                                if last {
                                    p.speaking = false;
                                    p.word = None;
                                }
                            }
                            _ => {
                                let error = get(&event, "error").as_string().unwrap_or_default();
                                if error != "interrupted" && error != "canceled" && last {
                                    p.speaking = false;
                                    p.failed = true;
                                    p.done_to = w1;
                                    p.word = None;
                                    p.active_range = None;
                                }
                            }
                        }
                    }) as Box<dyn FnMut(JsValue)>);
                    set(&u, key, cb.as_ref());
                    callbacks.push(cb);
                }
                self.utterances.push(Utterance {
                    value: u.clone(),
                    _callbacks: callbacks,
                });
                if say.call1(&synth, &u).is_err() {
                    self.stop();
                    return false;
                }
                w0 = w1;
            }
            true
        }
    }
    impl Drop for Narrator {
        fn drop(&mut self) {
            self.stop();
        }
    }
}
#[cfg(target_arch = "wasm32")]
pub use browser::Narrator;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferred_voice_is_english_enhanced_and_not_novelty() {
        let voices = vec![
            Voice {
                name: "Samantha compact".into(),
                language: "en-US".into(),
                default: true,
                local: true,
            },
            Voice {
                name: "Ava Enhanced".into(),
                language: "en-US".into(),
                default: false,
                local: true,
            },
            Voice {
                name: "Siri Premium".into(),
                language: "fr-FR".into(),
                default: true,
                local: true,
            },
        ];
        assert_eq!(voice_score(&voices[0]), -12);
        assert_eq!(preferred_voice(&voices), Some(1));
    }
}
