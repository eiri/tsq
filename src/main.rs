#[allow(dead_code)] // The PSG engine replaces the current audio path in the next steps.
mod psg;
mod sequencer;
mod ui;
mod voices;
mod widgets;

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use sequencer::{AudioClock, HihatVoice, STEPS, SharedState, ToneVoice, new_shared_state};
use voices::{Voice, hihat_closed, hihat_open, kick, snare, square_tone, tone};

// C major scale from middle C (C4) to C5, one note per step
const TONE_FREQS: [f32; STEPS] = [
    261.63, 293.66, 329.63, 349.23, 392.00, 440.00, 493.88, 523.25,
];

// C4 -> 60
// fn note_to_freq(midi: u8) -> f64 {
//     440.0 * 2.0_f64.powf((midi as f64 - 69.0) / 12.0)
// }

fn add_voice(track: &mut Vec<(Voice, f64)>, mut voice: Voice, ttl: f64, sr: f64) {
    // Match the device rate to preserve pitch and envelope timing.
    voice.set_sample_rate(sr);
    track.push((voice, ttl));
}

fn render_track(track: &mut Vec<(Voice, f64)>, sr: f64) -> f32 {
    let dt = 1.0 / sr;
    let mut out = 0.0f32;
    track.retain_mut(|(voice, ttl)| {
        let mut buf = [0.0f32; 1];
        voice.tick(&[], &mut buf);
        out += buf[0];
        *ttl -= dt;
        *ttl > 0.0
    });
    out
}

fn write_frame<T: SizedSample + FromSample<f32>>(frame: &mut [T], sample: f32) {
    // Integer conversion requires values below +1, including 24-bit samples.
    let value = T::from_sample(sample.clamp(-1.0, 1.0 - f32::EPSILON));
    frame.fill(value);
}

fn build_audio_stream(shared: SharedState) -> Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("no output device"))?;
    let config = device.default_output_config()?;

    // Build with the device's sample type; float output is not always supported.
    match config.sample_format() {
        SampleFormat::I8 => output_stream::<i8>(&device, config.into(), shared),
        SampleFormat::I16 => output_stream::<i16>(&device, config.into(), shared),
        SampleFormat::I24 => output_stream::<cpal::I24>(&device, config.into(), shared),
        SampleFormat::I32 => output_stream::<i32>(&device, config.into(), shared),
        SampleFormat::I64 => output_stream::<i64>(&device, config.into(), shared),
        SampleFormat::U8 => output_stream::<u8>(&device, config.into(), shared),
        SampleFormat::U16 => output_stream::<u16>(&device, config.into(), shared),
        SampleFormat::U24 => output_stream::<cpal::U24>(&device, config.into(), shared),
        SampleFormat::U32 => output_stream::<u32>(&device, config.into(), shared),
        SampleFormat::U64 => output_stream::<u64>(&device, config.into(), shared),
        SampleFormat::F32 => output_stream::<f32>(&device, config.into(), shared),
        SampleFormat::F64 => output_stream::<f64>(&device, config.into(), shared),
        format => anyhow::bail!("unsupported audio sample format: {format}"),
    }
}

fn output_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    shared: SharedState,
) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32>,
{
    let sr = config.sample_rate as f64;
    let channels = config.channels as usize;

    let mut clock = AudioClock::new(sr);
    let mut tracks: [Vec<(Voice, f64)>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            {
                let mut s = shared.lock().unwrap();
                if s.reset {
                    tracks.iter_mut().for_each(|t| t.clear());
                    s.reset = false;
                }
            }
            let (bpm, pattern, playing) = {
                let s = shared.lock().unwrap();
                (s.bpm, s.pattern.clone(), s.playing)
            };

            for frame in data.chunks_mut(channels) {
                if playing {
                    if let Some(step) = clock.advance(bpm) {
                        if pattern.kick[step] {
                            add_voice(&mut tracks[0], kick(1.0), 0.4, sr);
                        }
                        if pattern.snare[step] {
                            add_voice(&mut tracks[1], snare(0.4), 0.3, sr);
                        }
                        if let Some(hv) = &pattern.hihat[step] {
                            let voice = match hv {
                                HihatVoice::Open => hihat_open(1.0),
                                HihatVoice::Closed => hihat_closed(1.0),
                            };
                            add_voice(&mut tracks[2], voice, 0.8, sr);
                        }
                        if pattern.tone[step] {
                            let (voice, ttl) = match pattern.tone_voice {
                                ToneVoice::Sine => (tone(TONE_FREQS[step], 1.0), 2.0),
                                ToneVoice::Square => (square_tone(TONE_FREQS[step], 1.0), 0.5),
                            };
                            add_voice(&mut tracks[3], voice, ttl, sr);
                        }
                        let mut s = shared.lock().unwrap();
                        s.current_step = step;
                    }
                } else {
                    // Reset clock position so playback always restarts from step 0.
                    clock.sample_counter = 0;
                    clock.step = 0;
                }

                let sample = (render_track(&mut tracks[0], sr)
                    + render_track(&mut tracks[1], sr)
                    + render_track(&mut tracks[2], sr)
                    + render_track(&mut tracks[3], sr))
                    * 0.5;

                write_frame(frame, sample);
            }
        },
        |err| eprintln!("audio error: {err}"),
        None,
    )?;

    stream.play()?;
    Ok(stream)
}

fn main() -> Result<()> {
    let shared = new_shared_state();
    let _stream = build_audio_stream(shared.clone())?;
    ui::run(shared).map_err(|e| anyhow::anyhow!("{e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_samples<T>()
    where
        T: SizedSample + FromSample<f32> + std::fmt::Debug,
        f32: FromSample<T>,
    {
        let mut frame = [T::EQUILIBRIUM; 2];
        for sample in [-2.0, -0.5, 0.0, 0.5, 2.0] {
            write_frame(&mut frame, sample);
            assert_eq!(frame[0], frame[1]);

            let output = frame[0].to_sample::<f32>();
            assert!((-1.0..1.0).contains(&output));
            assert!((output - sample.clamp(-1.0, 1.0 - f32::EPSILON)).abs() < 0.01);
        }

        write_frame(&mut frame, 0.0);
        assert_eq!(frame, [T::EQUILIBRIUM; 2]);
    }

    #[test]
    fn output_sample_formats() {
        check_samples::<i8>();
        check_samples::<i16>();
        check_samples::<cpal::I24>();
        check_samples::<i32>();
        check_samples::<i64>();
        check_samples::<u8>();
        check_samples::<u16>();
        check_samples::<cpal::U24>();
        check_samples::<u32>();
        check_samples::<u64>();
        check_samples::<f32>();
        check_samples::<f64>();
    }

    #[test]
    fn voice_pitch_at_device_rate() {
        for sr in [44_100.0, 48_000.0, 96_000.0] {
            let mut track = Vec::new();
            add_voice(&mut track, tone(440.0, 1.0), 2.0, sr);

            // A 440 Hz tone completes 44 cycles in 100 ms at any device rate.
            let samples: Vec<_> = (0..(sr * 0.1) as usize)
                .map(|_| render_track(&mut track, sr))
                .collect();
            let cycles = samples
                .windows(2)
                .filter(|s| s[0] < 0.0 && s[1] >= 0.0)
                .count();

            assert!((43..=44).contains(&cycles), "incorrect pitch at {sr} Hz");
        }
    }
}
