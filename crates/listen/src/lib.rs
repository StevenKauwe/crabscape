//! Speech to text on the player's own device, for spoken bug reports: the
//! audio never leaves it.
//!
//! The model is Moonshine tiny (Useful Sensors, MIT licence, 27 million
//! numbers): it reads the raw sound, 16 000 samples a second, and its work
//! grows with the length of the clip (Whisper always works through 30
//! seconds), so a few spoken sentences take a moment even on a phone. The
//! layers follow Hugging Face's `modeling_moonshine.py`; the weights and the
//! word list are its `model.safetensors` and `tokenizer.json`.
//!
//! On the web this crate is its own small WebAssembly file run in a
//! background worker (see `web`), downloaded with the weights only when
//! someone first speaks a report, so the game's own download doesn't grow.

use candle_core::{D, DType, Device, Module, Result, Tensor};
use candle_nn::{
    Conv1d, Conv1dConfig, Embedding, GroupNorm, LayerNorm, Linear, VarBuilder, conv1d,
    conv1d_no_bias, embedding, group_norm, linear, linear_no_bias,
};

#[cfg(target_arch = "wasm32")]
mod web;

/// Samples a second of the sound the model hears.
pub const RATE: u32 = 16_000;

const HIDDEN: usize = 288;
const HEADS: usize = 8;
const HEAD: usize = HIDDEN / HEADS;
/// Positions turn only the first 90% of each head (32 of 36 numbers).
const ROTARY: usize = 32;
const LAYERS: usize = 6;
const INNER: usize = 1152;
const VOCAB: usize = 32768;
const START: u32 = 1;
const END: u32 = 2;
/// The longest answer the decoder was trained for.
const MAX_TOKENS: usize = 194;
const EPS: f64 = 1e-5;

fn norm(vb: VarBuilder) -> Result<LayerNorm> {
    Ok(LayerNorm::new_no_bias(vb.get(HIDDEN, "weight")?, EPS))
}

/// Cosines and sines that turn each position, for positions `from..from+len`:
/// both `(len, ROTARY / 2)`.
fn turns(from: usize, len: usize, device: &Device) -> Result<(Tensor, Tensor)> {
    let inv: Vec<f32> = (0..ROTARY / 2)
        .map(|i| 1.0 / 10_000f32.powf(2.0 * i as f32 / ROTARY as f32))
        .collect();
    let mut angles = Vec::with_capacity(len * inv.len());
    for p in from..from + len {
        angles.extend(inv.iter().map(|f| p as f32 * f));
    }
    let angles = Tensor::from_vec(angles, (len, ROTARY / 2), device)?;
    Ok((angles.cos()?, angles.sin()?))
}

/// Turn the first `ROTARY` numbers of each head by its position, in pairs
/// side by side (GLM's way); the rest pass unturned. `x` is `(1, heads, t, HEAD)`.
fn turn(x: &Tensor, cos: &Tensor, sin: &Tensor) -> Result<Tensor> {
    let turned =
        candle_nn::rotary_emb::rope_i(&x.narrow(D::Minus1, 0, ROTARY)?.contiguous()?, cos, sin)?;
    Tensor::cat(
        &[turned, x.narrow(D::Minus1, ROTARY, HEAD - ROTARY)?],
        D::Minus1,
    )
}

struct Attention {
    q: Linear,
    k: Linear,
    v: Linear,
    o: Linear,
}

impl Attention {
    fn new(vb: VarBuilder) -> Result<Self> {
        Ok(Self {
            q: linear_no_bias(HIDDEN, HIDDEN, vb.pp("q_proj"))?,
            k: linear_no_bias(HIDDEN, HIDDEN, vb.pp("k_proj"))?,
            v: linear_no_bias(HIDDEN, HIDDEN, vb.pp("v_proj"))?,
            o: linear_no_bias(HIDDEN, HIDDEN, vb.pp("o_proj"))?,
        })
    }

    /// `(1, t, HIDDEN)` into `(1, HEADS, t, HEAD)`.
    fn heads(x: &Tensor) -> Result<Tensor> {
        let t = x.dim(1)?;
        x.reshape((1, t, HEADS, HEAD))?
            .transpose(1, 2)?
            .contiguous()
    }

    /// Queries against keys and values already split into heads.
    fn attend(&self, q: &Tensor, k: &Tensor, v: &Tensor, mask: Option<&Tensor>) -> Result<Tensor> {
        let t = q.dim(2)?;
        let mut w = (q.matmul(&k.t()?)? * (HEAD as f64).powf(-0.5))?;
        if let Some(mask) = mask {
            w = w.broadcast_add(mask)?;
        }
        let w = candle_nn::ops::softmax_last_dim(&w)?;
        let out = w.matmul(v)?.transpose(1, 2)?.reshape((1, t, HIDDEN))?;
        self.o.forward(&out)
    }

    /// Keys and values for `x` (the encoder's output, for cross-attention).
    fn keys(&self, x: &Tensor) -> Result<(Tensor, Tensor)> {
        Ok((
            Self::heads(&self.k.forward(x)?)?,
            Self::heads(&self.v.forward(x)?)?,
        ))
    }
}

struct EncoderLayer {
    attn: Attention,
    norm1: LayerNorm,
    norm2: LayerNorm,
    fc1: Linear,
    fc2: Linear,
}

impl EncoderLayer {
    fn new(vb: VarBuilder) -> Result<Self> {
        Ok(Self {
            attn: Attention::new(vb.pp("self_attn"))?,
            norm1: norm(vb.pp("input_layernorm"))?,
            norm2: norm(vb.pp("post_attention_layernorm"))?,
            fc1: linear(HIDDEN, INNER, vb.pp("mlp.fc1"))?,
            fc2: linear(INNER, HIDDEN, vb.pp("mlp.fc2"))?,
        })
    }

    fn forward(&self, x: &Tensor, cos: &Tensor, sin: &Tensor) -> Result<Tensor> {
        let h = self.norm1.forward(x)?;
        let a = &self.attn;
        let q = turn(&Attention::heads(&a.q.forward(&h)?)?, cos, sin)?;
        let k = turn(&Attention::heads(&a.k.forward(&h)?)?, cos, sin)?;
        let v = Attention::heads(&a.v.forward(&h)?)?;
        let x = (x + a.attend(&q, &k, &v, None)?)?;
        let h = self.fc1.forward(&self.norm2.forward(&x)?)?.gelu_erf()?;
        x + self.fc2.forward(&h)?
    }
}

struct DecoderLayer {
    attn: Attention,
    cross: Attention,
    norm1: LayerNorm,
    norm2: LayerNorm,
    norm3: LayerNorm,
    fc1: Linear,
    fc2: Linear,
}

/// What a decoder layer keeps between words: its own keys and values so far,
/// and the encoder's (worked out once).
struct Memory {
    k: Option<Tensor>,
    v: Option<Tensor>,
    heard: (Tensor, Tensor),
}

impl DecoderLayer {
    fn new(vb: VarBuilder) -> Result<Self> {
        Ok(Self {
            attn: Attention::new(vb.pp("self_attn"))?,
            cross: Attention::new(vb.pp("encoder_attn"))?,
            norm1: norm(vb.pp("input_layernorm"))?,
            norm2: norm(vb.pp("post_attention_layernorm"))?,
            norm3: norm(vb.pp("final_layernorm"))?,
            fc1: linear(HIDDEN, 2 * INNER, vb.pp("mlp.fc1"))?,
            fc2: linear(INNER, HIDDEN, vb.pp("mlp.fc2"))?,
        })
    }

    /// One new word (`x` is `(1, 1, HIDDEN)`), attending to every word so far.
    fn step(&self, x: &Tensor, cos: &Tensor, sin: &Tensor, mem: &mut Memory) -> Result<Tensor> {
        let h = self.norm1.forward(x)?;
        let a = &self.attn;
        let q = turn(&Attention::heads(&a.q.forward(&h)?)?, cos, sin)?;
        let k = turn(&Attention::heads(&a.k.forward(&h)?)?, cos, sin)?;
        let v = Attention::heads(&a.v.forward(&h)?)?;
        let k = match &mem.k {
            Some(old) => Tensor::cat(&[old, &k], 2)?,
            None => k,
        };
        let v = match &mem.v {
            Some(old) => Tensor::cat(&[old, &v], 2)?,
            None => v,
        };
        let x = (x + a.attend(&q, &k, &v, None)?)?;
        mem.k = Some(k);
        mem.v = Some(v);
        let h = self.norm2.forward(&x)?;
        let q = Attention::heads(&self.cross.q.forward(&h)?)?;
        let x = (&x + self.cross.attend(&q, &mem.heard.0, &mem.heard.1, None)?)?;
        let h = self.fc1.forward(&self.norm3.forward(&x)?)?;
        let (h, gate) = (
            h.narrow(D::Minus1, 0, INNER)?,
            h.narrow(D::Minus1, INNER, INNER)?,
        );
        x + self.fc2.forward(&(gate.silu()? * h)?)?
    }
}

/// Words: token number to text, the way Moonshine's tokenizer writes them
/// (`▁` is a space, `<0xNN>` a raw byte).
struct Words(Vec<String>);

impl Words {
    fn new(tokenizer_json: &[u8]) -> std::result::Result<Self, String> {
        let json: serde_json::Value =
            serde_json::from_slice(tokenizer_json).map_err(|e| e.to_string())?;
        let vocab = json["model"]["vocab"]
            .as_object()
            .ok_or("tokenizer has no vocab")?;
        let mut words = vec![String::new(); VOCAB];
        for (word, id) in vocab {
            if let Some(slot) = id.as_u64().and_then(|i| words.get_mut(i as usize)) {
                *slot = word.clone();
            }
        }
        Ok(Self(words))
    }

    fn write(&self, tokens: &[u32]) -> String {
        let mut bytes = Vec::new();
        for &t in tokens {
            let Some(word) = self.0.get(t as usize) else {
                continue;
            };
            if let Some(hex) = word.strip_prefix("<0x").and_then(|w| w.strip_suffix('>')) {
                if let Ok(b) = u8::from_str_radix(hex, 16) {
                    bytes.push(b);
                }
            } else if !(word.starts_with('<') && word.ends_with('>')) {
                bytes.extend(word.replace('\u{2581}', " ").bytes());
            }
        }
        String::from_utf8_lossy(&bytes).trim().to_string()
    }
}

/// The model, ready to listen.
pub struct Listener {
    conv1: Conv1d,
    groupnorm: GroupNorm,
    conv2: Conv1d,
    conv3: Conv1d,
    encoder: Vec<EncoderLayer>,
    encoder_norm: LayerNorm,
    embed: Embedding,
    decoder: Vec<DecoderLayer>,
    decoder_norm: LayerNorm,
    words: Words,
    device: Device,
}

impl Listener {
    /// From the bytes of `model.safetensors` and `tokenizer.json`.
    pub fn new(weights: Vec<u8>, tokenizer_json: &[u8]) -> std::result::Result<Self, String> {
        let words = Words::new(tokenizer_json)?;
        Self::load(weights, words).map_err(|e| e.to_string())
    }

    fn load(weights: Vec<u8>, words: Words) -> Result<Self> {
        let device = Device::Cpu;
        let vb = VarBuilder::from_buffered_safetensors(weights, DType::F32, &device)?;
        let (enc, dec) = (vb.pp("model.encoder"), vb.pp("model.decoder"));
        let stride = |stride| Conv1dConfig {
            stride,
            ..Default::default()
        };
        Ok(Self {
            conv1: conv1d_no_bias(1, HIDDEN, 127, stride(64), enc.pp("conv1"))?,
            groupnorm: group_norm(1, HIDDEN, EPS, enc.pp("groupnorm"))?,
            conv2: conv1d(HIDDEN, 2 * HIDDEN, 7, stride(3), enc.pp("conv2"))?,
            conv3: conv1d(2 * HIDDEN, HIDDEN, 3, stride(2), enc.pp("conv3"))?,
            encoder: (0..LAYERS)
                .map(|i| EncoderLayer::new(enc.pp(format!("layers.{i}"))))
                .collect::<Result<_>>()?,
            encoder_norm: norm(enc.pp("layer_norm"))?,
            embed: embedding(VOCAB, HIDDEN, dec.pp("embed_tokens"))?,
            decoder: (0..LAYERS)
                .map(|i| DecoderLayer::new(dec.pp(format!("layers.{i}"))))
                .collect::<Result<_>>()?,
            decoder_norm: norm(dec.pp("norm"))?,
            words,
            device,
        })
    }

    /// What was said in `samples` (mono, [`RATE`] samples a second).
    pub fn hear(&self, samples: &[f32]) -> std::result::Result<String, String> {
        // Shorter than the first layer's window: nothing to hear.
        if samples.len() < RATE as usize / 4 {
            return Ok(String::new());
        }
        self.decode(samples).map_err(|e| e.to_string())
    }

    fn decode(&self, samples: &[f32]) -> Result<String> {
        let heard = self.encode(samples)?;
        let mut memory: Vec<Memory> = self
            .decoder
            .iter()
            .map(|l| {
                Ok(Memory {
                    k: None,
                    v: None,
                    heard: l.cross.keys(&heard)?,
                })
            })
            .collect::<Result<_>>()?;
        // About six words a second at most, as Moonshine's own code allows.
        let most = (samples.len() as f32 / RATE as f32 * 6.5) as usize + 8;
        let mut tokens = vec![START];
        for at in 0..most.min(MAX_TOKENS) {
            let last = Tensor::new(&[[*tokens.last().unwrap_or(&START)]], &self.device)?;
            let mut x = self.embed.forward(&last)?;
            let (cos, sin) = turns(at, 1, &self.device)?;
            for (layer, mem) in self.decoder.iter().zip(&mut memory) {
                x = layer.step(&x, &cos, &sin, mem)?;
            }
            // The output layer shares the word table (tied weights).
            let x = self.decoder_norm.forward(&x)?;
            let logits = x.squeeze(0)?.matmul(&self.embed.embeddings().t()?)?;
            let next = logits.squeeze(0)?.argmax(D::Minus1)?.to_scalar::<u32>()?;
            if next == END {
                break;
            }
            tokens.push(next);
        }
        Ok(self.words.write(&tokens[1..]))
    }

    /// The sound into `(1, frames, HIDDEN)`: about 41 frames a second.
    fn encode(&self, samples: &[f32]) -> Result<Tensor> {
        let x = Tensor::from_slice(samples, (1, 1, samples.len()), &self.device)?;
        let x = self.conv1.forward(&x)?.tanh()?;
        let x = self.groupnorm.forward(&x)?;
        let x = self.conv2.forward(&x)?.gelu_erf()?;
        let x = self.conv3.forward(&x)?.gelu_erf()?;
        let mut x = x.transpose(1, 2)?.contiguous()?;
        let (cos, sin) = turns(0, x.dim(1)?, &self.device)?;
        for layer in &self.encoder {
            x = layer.forward(&x, &cos, &sin)?;
        }
        self.encoder_norm.forward(&x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_turn_spaces_and_raw_bytes_back_into_text() {
        let mut list = vec![String::new(); VOCAB];
        for (i, w) in [
            "<s>",
            "\u{2581}the",
            "\u{2581}cr",
            "ab",
            "<0xC3>",
            "<0xA9>",
            "</s>",
        ]
        .iter()
        .enumerate()
        {
            list[i] = (*w).to_string();
        }
        let words = Words(list);
        assert_eq!(words.write(&[0, 1, 2, 3]), "the crab");
        assert_eq!(words.write(&[1, 4, 5, 6]), "theé");
    }

    /// Hears a spoken sentence. Needs the model's files (about 110 MB), so it
    /// runs only when `LISTEN_MODEL` names their folder and `LISTEN_CLIP` a
    /// 16 kHz mono 16-bit WAV file: `cargo test -p listen -- --ignored`.
    #[test]
    #[ignore]
    fn hears_a_spoken_sentence() {
        let dir = std::env::var("LISTEN_MODEL").expect("LISTEN_MODEL");
        let clip = std::fs::read(std::env::var("LISTEN_CLIP").expect("LISTEN_CLIP")).unwrap();
        let weights = std::fs::read(format!("{dir}/model.safetensors")).unwrap();
        let tokenizer = std::fs::read(format!("{dir}/tokenizer.json")).unwrap();
        let listener = Listener::new(weights, &tokenizer).unwrap();
        // A plain WAV: 44 bytes of header, then 16-bit samples.
        let samples: Vec<f32> = clip[44..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes(*b) as f32 / 32768.0)
            .collect();
        let start = std::time::Instant::now();
        let text = listener.hear(&samples).unwrap();
        eprintln!("heard {:?} in {:?}", text, start.elapsed());
        assert!(text.to_lowercase().contains("crab"), "{text}");
    }
}
