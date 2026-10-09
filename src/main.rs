mod psg;
mod sequencer;
mod ui;
mod widgets;

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use psg::PsgEngine;
use sequencer::{
    AudioClock, Audition, Drum, MelodyVoice, Pattern, SharedState, TRACKS, new_shared_state,
};

fn trigger_drum(engine: &mut PsgEngine, drum: Drum) {
    match drum {
        Drum::Kick => engine.kick(),
        Drum::Snare => engine.snare(),
        Drum::ClosedHat => engine.hihat(false),
        Drum::OpenHat => engine.hihat(true),
    }
}

fn trigger_step(engine: &mut PsgEngine, pattern: &Pattern, step: usize, muted: [bool; TRACKS]) {
    if !muted[0]
        && let Some(note) = pattern.melody[step]
    {
        match note.voice {
            MelodyVoice::M1 => engine.melody(note.freq, pattern.melody_style),
            MelodyVoice::M2 => engine.melody2(note.freq),
        }
    }
    if !muted[1]
        && let Some(notes) = pattern.arpeggio[step]
    {
        engine.arpeggio(notes);
    }
    // Drums take channel C if bass and drums start on the same step.
    if !muted[2]
        && let Some(freq) = pattern.bass[step]
    {
        engine.bass(freq);
    }
    if !muted[3]
        && let Some(drum) = pattern.drums[step]
    {
        trigger_drum(engine, drum);
    }
}

fn trigger_audition(engine: &mut PsgEngine, request: Audition) {
    match request {
        Audition::Melody => engine.melody(440.0, sequencer::MelodyStyle::Sustain),
        Audition::Melody2 => engine.melody2(440.0),
        Audition::Arpeggio => engine.arpeggio([261.63, 329.63, 392.0]),
        Audition::Bass => engine.bass(130.81),
        Audition::Drum(drum) => trigger_drum(engine, drum),
    }
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
    let mut clock = AudioClock::new(config.sample_rate as f64);
    let mut engine = PsgEngine::new(config.sample_rate);
    let mut last_mutes = [false; TRACKS];
    let mut last_playing = false;
    let channels = config.channels as usize;

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            let (bpm, pattern, playing, reset, audition, muted) = {
                let mut s = shared.lock().unwrap();
                let reset = std::mem::take(&mut s.reset);
                (
                    s.bpm,
                    s.pattern.clone(),
                    s.playing,
                    reset,
                    s.audition.take(),
                    s.muted,
                )
            };
            // Stop playback tails, but let a paused audition start cleanly.
            if reset || (!playing && (last_playing || audition.is_some())) {
                engine.reset();
            }

            // Stop newly muted notes and muted previews when playback starts.
            for track in 0..TRACKS {
                if muted[track] && (!last_mutes[track] || (playing && !last_playing)) {
                    engine.mute(track);
                }
            }
            last_mutes = muted;
            last_playing = playing;

            if let Some(request) = audition {
                trigger_audition(&mut engine, request);
            }

            for frame in data.chunks_mut(channels) {
                if playing {
                    if let Some(step) = clock.advance(bpm) {
                        trigger_step(&mut engine, &pattern, step, muted);
                        let mut s = shared.lock().unwrap();
                        s.current_step = step;
                    }
                } else {
                    // Reset clock position so playback always restarts from step 0.
                    clock.sample_counter = 0;
                    clock.step = 0;
                }

                let sample = if playing || engine.active() {
                    engine.next_sample()
                } else {
                    0.0
                };
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
    fn sequenced_voices_sound() {
        let mut pattern = Pattern {
            melody: [None; sequencer::STEPS],
            arpeggio: [None; sequencer::STEPS],
            bass: [None; sequencer::STEPS],
            drums: [None; sequencer::STEPS],
            ..Pattern::default()
        };

        for voice in 0..5 {
            let mut engine = PsgEngine::new(48_000);
            match voice {
                0 => {
                    pattern.melody[0] = Some(sequencer::MelodyStep {
                        freq: 440.0,
                        voice: MelodyVoice::M1,
                    })
                }
                1 => {
                    pattern.melody[0] = Some(sequencer::MelodyStep {
                        freq: 440.0,
                        voice: MelodyVoice::M2,
                    })
                }
                2 => pattern.arpeggio[0] = Some([261.63, 329.63, 392.0]),
                3 => pattern.bass[0] = Some(130.81),
                _ => pattern.drums[0] = Some(Drum::Snare),
            }
            trigger_step(&mut engine, &pattern, 0, [false; TRACKS]);
            assert!(engine.active());
            let peak = (0..4096)
                .map(|_| engine.next_sample().abs())
                .fold(0.0_f32, f32::max);
            assert!(peak > 0.01, "voice {voice} was silent");

            let mut muted = [false; TRACKS];
            muted[match voice {
                0 | 1 => 0,
                2 => 1,
                3 => 2,
                _ => 3,
            }] = true;
            let mut silent = PsgEngine::new(48_000);
            trigger_step(&mut silent, &pattern, 0, muted);
            assert!(!silent.active(), "muted voice {voice} played");

            pattern.melody[0] = None;
            pattern.arpeggio[0] = None;
            pattern.bass[0] = None;
            pattern.drums[0] = None;
        }
    }

    #[test]
    fn last_step_sounds() {
        let mut pattern = Pattern {
            melody: [None; sequencer::STEPS],
            arpeggio: [None; sequencer::STEPS],
            bass: [None; sequencer::STEPS],
            drums: [None; sequencer::STEPS],
            ..Pattern::default()
        };
        pattern.drums[sequencer::STEPS - 1] = Some(Drum::Snare);

        let mut engine = PsgEngine::new(48_000);
        trigger_step(&mut engine, &pattern, sequencer::STEPS - 1, [false; TRACKS]);
        assert!(engine.active());
    }

    #[test]
    fn shared_channel_mutes_are_independent() {
        let mut pattern = Pattern {
            melody: [None; sequencer::STEPS],
            arpeggio: [None; sequencer::STEPS],
            bass: [None; sequencer::STEPS],
            drums: [None; sequencer::STEPS],
            ..Pattern::default()
        };
        pattern.bass[0] = Some(130.81);
        pattern.drums[0] = Some(Drum::Snare);

        for track in [2, 3] {
            let mut muted = [false; TRACKS];
            muted[track] = true;
            let mut engine = PsgEngine::new(48_000);
            trigger_step(&mut engine, &pattern, 0, muted);
            let peak = (0..4096)
                .map(|_| engine.next_sample().abs())
                .fold(0.0_f32, f32::max);
            assert!(peak > 0.01, "muting track {track} silenced both voices");
        }
    }

    #[test]
    fn audition_finishes_while_paused() {
        for request in [
            Audition::Melody,
            Audition::Melody2,
            Audition::Arpeggio,
            Audition::Bass,
            Audition::Drum(Drum::Kick),
            Audition::Drum(Drum::Snare),
            Audition::Drum(Drum::ClosedHat),
            Audition::Drum(Drum::OpenHat),
        ] {
            let mut engine = PsgEngine::new(48_000);
            trigger_audition(&mut engine, request);
            assert!(engine.active());
            let peak = (0..48_000)
                .map(|_| {
                    if engine.active() {
                        engine.next_sample().abs()
                    } else {
                        0.0
                    }
                })
                .fold(0.0_f32, f32::max);
            assert!(peak > 0.01, "{request:?} was silent");
            assert!(!engine.active());
        }
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
}
