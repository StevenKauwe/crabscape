use super::{Note, Wave};
const RATE: u32 = 44100;
/// Mono PCM16 RIFF, with the original master (0.45) and music bus (0.22).
/// Noise uses a reproducible 0.8-second buffer, just as each WebAudio context does.
pub fn wav(notes: &[Note]) -> Vec<u8> {
    let duration = notes
        .iter()
        .map(|n| n.delay + n.duration + 0.05)
        .fold(0_f64, f64::max)
        .clamp(0., 30.);
    let mut samples = vec![0_f64; (duration * f64::from(RATE)).ceil() as usize];
    let mut seed = 0x594df123_u32;
    let noise: Vec<f64> = (0..(RATE * 4 / 5))
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            f64::from(seed) / f64::from(u32::MAX) * 2. - 1.
        })
        .collect();
    for n in notes {
        if n.duration <= 0. || n.frequency <= 0. || n.volume <= 0. {
            continue;
        }
        let is_noise = matches!(n.wave, Wave::Lowpass | Wave::Highpass | Wave::Bandpass);
        let omega = std::f64::consts::TAU * n.frequency / f64::from(RATE);
        let alpha = omega.sin() / 2.;
        let cos = omega.cos();
        let a0 = 1. + alpha;
        let (b0, b1, b2) = match n.wave {
            Wave::Lowpass => ((1. - cos) / 2., 1. - cos, (1. - cos) / 2.),
            Wave::Highpass => ((1. + cos) / 2., -1. - cos, (1. + cos) / 2.),
            _ => (alpha, 0., -alpha),
        };
        let (mut x1, mut x2, mut y1, mut y2) = (0., 0., 0., 0.);
        let mut phase = 0.;
        let start = (n.delay.max(0.) * f64::from(RATE)) as usize;
        let tail = if is_noise || n.music { 0.05 } else { 0.03 };
        for (i, sample) in samples
            .iter_mut()
            .skip(start)
            .take(((n.duration + tail) * f64::from(RATE)) as usize)
            .enumerate()
        {
            let time = i as f64 / f64::from(RATE);
            let t = time.min(n.duration);
            let attack = if n.music { 0.02 } else { 0.012 };
            let gain = if is_noise {
                n.volume * (0.0001 / n.volume).powf(t / n.duration)
            } else if t < attack {
                0.0001 * (n.volume / 0.0001).powf(t / attack)
            } else {
                n.volume * (0.0001 / n.volume).powf((t - attack) / (n.duration - attack))
            };
            let value = if is_noise {
                let x = noise.get(i).copied().unwrap_or(0.);
                let y = (b0 * x + b1 * x1 + b2 * x2 + 2. * cos * y1 - (1. - alpha) * y2) / a0;
                x2 = x1;
                x1 = x;
                y2 = y1;
                y1 = y;
                y
            } else {
                let value = match n.wave {
                    Wave::Sine => (phase * std::f64::consts::TAU).sin(),
                    Wave::Square => {
                        if phase < 0.5 {
                            1.
                        } else {
                            -1.
                        }
                    }
                    Wave::Sawtooth => 2. * phase - 1.,
                    Wave::Triangle => 1. - 4. * (phase - 0.5_f64).abs(),
                    _ => 0.,
                };
                let freq = n.end_frequency.map_or(n.frequency, |f| {
                    n.frequency * (f / n.frequency).powf(t / n.duration)
                });
                phase = (phase + freq / f64::from(RATE)).fract();
                value
            };
            *sample += value * gain * 0.45 * if n.music { 0.22 } else { 1. };
        }
    }
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(data_len as usize + 44);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&RATE.to_le_bytes());
    bytes.extend_from_slice(&(RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        bytes.extend_from_slice(&((s.clamp(-1., 1.) * 32767.) as i16).to_le_bytes());
    }
    bytes
}
