//! Source WebAudio scheduling, with entity-scoped cancellation and no PCM batches.
use super::{Note, Wave};
use std::{cell::RefCell, collections::HashMap};
use wasm_bindgen::{JsCast, JsValue};
fn get(o: &JsValue, k: &str) -> Result<JsValue, JsValue> {
    js_sys::Reflect::get(o, &k.into())
}
fn call(o: &JsValue, k: &str, args: &[JsValue]) -> Result<JsValue, JsValue> {
    let array = js_sys::Array::new();
    for a in args {
        array.push(a);
    }
    get(o, k)?.dyn_into::<js_sys::Function>()?.apply(o, &array)
}
fn set(o: &JsValue, k: &str, v: JsValue) -> Result<(), JsValue> {
    js_sys::Reflect::set(o, &k.into(), &v).map(|_| ())
}
fn number(v: f64) -> JsValue {
    JsValue::from_f64(v)
}
struct PendingNodes(Vec<JsValue>);
impl Drop for PendingNodes {
    fn drop(&mut self) {
        for node in &self.0 {
            let _ = call(node, "stop", &[]);
            let _ = call(node, "disconnect", &[]);
        }
    }
}
struct Context {
    context: web_sys::AudioContext,
    noise: web_sys::AudioBuffer,
    next: u64,
    groups: HashMap<u64, Vec<JsValue>>,
}
impl Context {
    fn new() -> Result<Self, JsValue> {
        let context = web_sys::AudioContext::new()?;
        let rate = context.sample_rate();
        let noise = context.create_buffer(1, (rate * 0.8) as u32, rate)?;
        let data: Vec<f32> = (0..(rate * 0.8) as usize)
            .map(|_| (js_sys::Math::random() * 2. - 1.) as f32)
            .collect();
        noise.copy_to_channel(&data, 0)?;
        Ok(Self {
            context,
            noise,
            next: 0,
            groups: HashMap::new(),
        })
    }
    fn schedule(&mut self, notes: &[Note]) -> Result<u64, JsValue> {
        let _ = self.context.resume();
        let context: JsValue = self.context.clone().into();
        let destination: JsValue = self.context.destination().into();
        let now = self.context.current_time();
        let mut nodes = PendingNodes(Vec::with_capacity(notes.len().min(64) * 4));
        for note in notes.iter().take(64) {
            let noise = matches!(note.wave, Wave::Lowpass | Wave::Highpass | Wave::Bandpass);
            let oscillator = call(
                &context,
                if noise {
                    "createBufferSource"
                } else {
                    "createOscillator"
                },
                &[],
            )?;
            nodes.0.push(oscillator.clone());
            let gain = call(&context, "createGain", &[])?;
            nodes.0.push(gain.clone());
            let gain_param = get(&gain, "gain")?;
            let start = now + note.delay;
            if noise {
                set(&oscillator, "buffer", self.noise.clone().into())?;
                let filter = call(&context, "createBiquadFilter", &[])?;
                nodes.0.push(filter.clone());
                set(
                    &filter,
                    "type",
                    match note.wave {
                        Wave::Lowpass => "lowpass",
                        Wave::Highpass => "highpass",
                        _ => "bandpass",
                    }
                    .into(),
                )?;
                set(&get(&filter, "frequency")?, "value", number(note.frequency))?;
                call(&oscillator, "connect", std::slice::from_ref(&filter))?;
                call(&filter, "connect", std::slice::from_ref(&gain))?;
            } else {
                set(
                    &oscillator,
                    "type",
                    match note.wave {
                        Wave::Triangle => "triangle",
                        Wave::Square => "square",
                        Wave::Sawtooth => "sawtooth",
                        _ => "sine",
                    }
                    .into(),
                )?;
                let frequency = get(&oscillator, "frequency")?;
                call(
                    &frequency,
                    "setValueAtTime",
                    &[number(note.frequency), number(start)],
                )?;
                if let Some(f) = note.end_frequency {
                    call(
                        &frequency,
                        "exponentialRampToValueAtTime",
                        &[number(f), number(start + note.duration)],
                    )?;
                }
                call(&oscillator, "connect", std::slice::from_ref(&gain))?;
            }
            call(
                &gain_param,
                "setValueAtTime",
                &[
                    number(if noise { note.volume } else { 0.0001 }),
                    number(start),
                ],
            )?;
            if !noise {
                call(
                    &gain_param,
                    "exponentialRampToValueAtTime",
                    &[
                        number(note.volume),
                        number(start + if note.music { 0.02 } else { 0.012 }),
                    ],
                )?;
            }
            call(
                &gain_param,
                "exponentialRampToValueAtTime",
                &[number(0.0001), number(start + note.duration)],
            )?;
            let bus = call(&context, "createGain", &[])?;
            nodes.0.push(bus.clone());
            set(
                &get(&bus, "gain")?,
                "value",
                number(0.45 * if note.music { 0.22 } else { 1. }),
            )?;
            call(&gain, "connect", std::slice::from_ref(&bus))?;
            call(&bus, "connect", std::slice::from_ref(&destination))?;
            call(&oscillator, "start", &[number(start)])?;
            call(
                &oscillator,
                "stop",
                &[number(
                    start + note.duration + if noise || note.music { 0.05 } else { 0.03 },
                )],
            )?;
        }
        self.next = self.next.wrapping_add(1);
        let id = self.next;
        self.groups.insert(id, std::mem::take(&mut nodes.0));
        Ok(id)
    }
    fn stop(&mut self, id: u64) {
        if let Some(nodes) = self.groups.remove(&id) {
            for node in nodes {
                let _ = call(&node, "stop", &[]);
                let _ = call(&node, "disconnect", &[]);
            }
        }
    }
}
thread_local! {static CONTEXT:RefCell<Option<Context>>=const{RefCell::new(None)};}
#[derive(bevy::prelude::Component)]
pub struct Group {
    id: u64,
}
impl Drop for Group {
    fn drop(&mut self) {
        CONTEXT.with(|c| {
            if let Some(c) = c.borrow_mut().as_mut() {
                c.stop(self.id);
            }
        });
    }
}
#[derive(bevy::prelude::Component)]
pub struct Lifetime(pub bevy::time::Timer);
pub fn play(notes: &[Note], commands: &mut bevy::prelude::Commands) -> bool {
    let id = CONTEXT.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            *c = Context::new().ok();
        }
        c.as_mut()?.schedule(notes).ok()
    });
    let Some(id) = id else {
        return false;
    };
    let duration = notes
        .iter()
        .map(|n| n.delay + n.duration + 0.05)
        .fold(0_f64, f64::max);
    let mut entity = commands.spawn((
        Group { id },
        Lifetime(bevy::time::Timer::from_seconds(
            duration as f32,
            bevy::time::TimerMode::Once,
        )),
        super::ValmoraSound,
    ));
    if notes.iter().all(|note| note.music) {
        entity.insert(super::ValmoraMusic);
    }
    true
}
/// Register once in the Valmora plugin, to release completed browser node groups.
pub fn reap(
    time: bevy::prelude::Res<bevy::prelude::Time<bevy::time::Real>>,
    mut groups: bevy::prelude::Query<(bevy::prelude::Entity, &mut Lifetime)>,
    mut commands: bevy::prelude::Commands,
) {
    for (entity, mut lifetime) in &mut groups {
        lifetime.0.tick(time.delta());
        if lifetime.0.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}
