mod sequencer;
mod ui;
mod voices;

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use sequencer::{AudioClock, STEPS, SharedState, ToneVoice, new_shared_state};
use voices::{Voice, hihat_closed, hihat_open, kick, square_tone, tone};

// C major scale from middle C (C4) to C5, one note per step
const TONE_FREQS: [f32; STEPS] = [
    261.63, 293.66, 329.63, 349.23, 392.00, 440.00, 493.88, 523.25,
];

// C4 -> 60
// fn note_to_freq(midi: u8) -> f64 {
//     440.0 * 2.0_f64.powf((midi as f64 - 69.0) / 12.0)
// }

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

fn build_audio_stream(shared: SharedState) -> Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("no output device"))?;
    let config = device.default_output_config()?;
    let sr = config.sample_rate() as f64;
    let channels = config.channels() as usize;

    let mut clock = AudioClock::new(sr);
    let mut tracks: [Vec<(Voice, f64)>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];

    let stream = device.build_output_stream(
        &config.into(),
        move |data: &mut [f32], _| {
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
                if let Some(step) = clock.advance(bpm) {
                    if playing {
                        if pattern.kick[step] {
                            tracks[0].push((kick(1.0), 0.4));
                        }
                        if pattern.hihat_closed[step] {
                            tracks[1].push((hihat_closed(1.0), 0.3));
                        }
                        if pattern.hihat_open[step] {
                            tracks[2].push((hihat_open(1.0), 0.8));
                        }
                        if pattern.tone[step] {
                            let (voice, ttl) = match pattern.tone_voice {
                                ToneVoice::Sine => (tone(TONE_FREQS[step], 1.0), 2.0),
                                ToneVoice::Square => (square_tone(TONE_FREQS[step], 1.0), 0.5),
                            };
                            tracks[3].push((voice, ttl));
                        }
                    }
                    let mut s = shared.lock().unwrap();
                    s.current_step = step;
                }

                let sample = (render_track(&mut tracks[0], sr)
                    + render_track(&mut tracks[1], sr)
                    + render_track(&mut tracks[2], sr)
                    + render_track(&mut tracks[3], sr))
                    * 0.5;

                for ch in frame.iter_mut() {
                    *ch = sample;
                }
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
    ui::run(shared)
}
