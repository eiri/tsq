use fundsp::prelude64::*;

pub type Voice = Box<dyn AudioUnit + Send>;

pub fn kick(amplitude: f32) -> Voice {
    let pitch_env = envelope(|t: f64| {
        if t < 0.001 {
            150.0 + (1.0 - t / 0.001) * 50.0
        } else if t < 0.06 {
            150.0 - (t - 0.001) / 0.059 * 148.0
        } else {
            2.0
        }
    });

    let amp_env = envelope(|t: f64| {
        if t < 0.002 {
            t / 0.002
        } else {
            (-((t - 0.002) * 14.0)).exp()
        }
    });

    Box::new((pitch_env >> sine()) * amp_env * 1.2 * amplitude)
}

pub fn hihat_closed(amplitude: f32) -> Voice {
    hihat(60.0, amplitude)
}

pub fn hihat_open(amplitude: f32) -> Voice {
    hihat(18.0, amplitude)
}

fn hihat(decay: f64, amplitude: f32) -> Voice {
    let amp_env = envelope(move |t: f64| (-t * decay).exp());
    Box::new(noise() >> (highpass_hz(7000.0, 0.8) * amp_env * 0.35 * amplitude))
}

#[allow(dead_code)]
pub fn snare(amplitude: f32) -> Voice {
    let amp_env = envelope(|t: f64| {
        if t < 0.002 {
            t / 0.002
        } else {
            (-(t - 0.002) * 22.0).exp()
        }
    });
    let body = sine_hz(180.0);
    let crack = noise() >> highpass_hz(2000.0, 0.7);
    Box::new((body + crack) * amp_env * 0.5 * amplitude)
}

pub fn tone(freq: f32, amplitude: f32) -> Voice {
    let amp_env = envelope(move |t: f64| {
        let attack = 0.01;
        let decay = 0.08;
        let sustain = 0.45;
        let sustain_end = attack + decay + 0.09;

        if t < attack {
            t / attack
        } else if t < attack + decay {
            1.0 - (1.0 - sustain) * ((t - attack) / decay)
        } else if t < sustain_end {
            sustain
        } else {
            (sustain * (-((t - sustain_end) * 10.0)).exp()).max(0.0)
        }
    });

    Box::new(sine_hz(freq) * amp_env * 0.4 * amplitude)
}

pub fn square_tone(freq: f32, amplitude: f32) -> Voice {
    let amp_env = envelope(move |t: f64| {
        let attack = 0.005;
        let decay = 0.05;
        let sustain = 0.6;
        let sustain_end = attack + decay + 0.09;

        if t < attack {
            t / attack
        } else if t < attack + decay {
            1.0 - (1.0 - sustain) * ((t - attack) / decay)
        } else if t < sustain_end {
            sustain
        } else {
            (sustain * (-((t - sustain_end) * 8.0)).exp()).max(0.0)
        }
    });

    Box::new((constant(freq) >> poly_square()) * amp_env * 0.1 * amplitude)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_n(voice: &mut Voice, n: usize, sr: f64) -> Vec<f32> {
        (0..n)
            .map(|_| {
                let mut buf = [0.0; 1];
                voice.tick(&[], &mut buf);
                let _ = sr;
                buf[0]
            })
            .collect()
    }

    #[test]
    fn kick_produces_nonzero_samples() {
        let mut v = kick(1.0);
        let samples = render_n(&mut v, 256, 44100.0);
        let has_signal = samples.iter().any(|&s| s.abs() > 1e-6);
        assert!(has_signal, "kick should produce audible signal");
    }

    #[test]
    fn kick_decays_to_near_silence() {
        let mut v = kick(1.0);
        render_n(&mut v, 44100, 44100.0);
        let tail: Vec<f32> = render_n(&mut v, 256, 44100.0);
        let max_amp = tail.iter().cloned().fold(0.0, f32::max);
        assert!(max_amp < 0.01, "kick tail should be near silent after 1 s");
    }

    #[test]
    fn hihat_closed_is_shorter_than_open() {
        let sr = 44100.0;
        let energy = |mut v: Voice, frames: usize| -> f32 {
            (0..frames)
                .map(|_| {
                    let mut buf = [0.0; 1];
                    v.tick(&[], &mut buf);
                    buf[0] * buf[0]
                })
                .sum()
        };

        let closed_energy = energy(hihat_closed(1.0), (sr * 0.15) as usize);
        let open_energy = energy(hihat_open(1.0), (sr * 0.15) as usize);

        assert!(
            open_energy > closed_energy,
            "open hi-hat should have more energy over 150 ms than closed"
        );
    }

    #[test]
    fn tone_output_is_within_unity() {
        let mut v = tone(440.0, 1.0);
        let samples = render_n(&mut v, 4410, 44100.0);
        let peak = samples.iter().cloned().fold(0.0, |a, b| a.max(b.abs()));
        assert!(peak <= 1.0, "tone must not exceed unity gain");
    }

    #[test]
    fn tone_different_freqs_differ() {
        let mut v220 = tone(220.0, 1.0);
        let mut v880 = tone(880.0, 1.0);
        let s220: Vec<f32> = render_n(&mut v220, 512, 44100.0);
        let s880: Vec<f32> = render_n(&mut v880, 512, 44100.0);
        let same = s220.iter().zip(&s880).all(|(a, b)| (a - b).abs() < 1e-9);
        assert!(
            !same,
            "different frequency tones must produce different waveforms"
        );
    }

    #[test]
    fn square_tone_produces_nonzero_samples() {
        let mut v = square_tone(440.0, 1.0);
        let samples = render_n(&mut v, 256, 44100.0);
        assert!(samples.iter().any(|&s| s.abs() > 1e-6));
    }

    #[test]
    fn square_tone_output_is_within_unity() {
        let mut v = square_tone(440.0, 1.0);
        let samples = render_n(&mut v, 4410, 44100.0);
        let peak = samples.iter().cloned().fold(0.0, |a, b| a.max(b.abs()));
        assert!(peak <= 1.0, "square_tone must not exceed unity gain");
    }

    #[test]
    fn square_tone_different_freqs_differ() {
        let mut v220 = square_tone(220.0, 1.0);
        let mut v880 = square_tone(880.0, 1.0);
        let s220 = render_n(&mut v220, 512, 44100.0);
        let s880 = render_n(&mut v880, 512, 44100.0);
        let same = s220.iter().zip(&s880).all(|(a, b)| (a - b).abs() < 1e-9);
        assert!(
            !same,
            "different frequency square tones must produce different waveforms"
        );
    }

    #[test]
    fn square_tone_decays_to_near_silence() {
        let mut v = square_tone(440.0, 1.0);
        render_n(&mut v, 44100, 44100.0);
        let tail = render_n(&mut v, 256, 44100.0);
        let max_amp = tail.iter().cloned().fold(0.0, f32::max);
        assert!(
            max_amp < 0.01,
            "square_tone tail should be near silent after 1 s"
        );
    }

    #[test]
    fn snare_produces_nonzero_samples() {
        let mut v = snare(1.0);
        let samples = render_n(&mut v, 256, 44100.0);
        assert!(samples.iter().any(|&s| s.abs() > 1e-6));
    }

    #[test]
    fn snare_decays_to_near_silence() {
        let mut v = snare(1.0);
        render_n(&mut v, 44100, 44100.0);
        let tail = render_n(&mut v, 256, 44100.0);
        let max_amp = tail.iter().cloned().fold(0.0, f32::max);
        assert!(max_amp < 0.01, "snare tail should be near silent after 1 s");
    }

    #[test]
    fn snare_output_is_within_unity() {
        let mut v = snare(1.0);
        let samples = render_n(&mut v, 4410, 44100.0);
        let peak = samples.iter().cloned().fold(0.0, |a, b| a.max(b.abs()));
        assert!(peak <= 1.0, "snare must not exceed unity gain");
    }
}
