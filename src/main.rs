mod psg;
mod sequencer;
mod ui;
mod widgets;

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use psg::{DrumSampler, PsgEngine};
use sequencer::{
    AudioClock, Audition, Pattern, SequencerState, SharedState, TRACKS, new_shared_state,
};

fn trigger_step(
    engine: &mut PsgEngine,
    drums: &mut DrumSampler,
    pattern: &Pattern,
    step: usize,
    muted: [bool; TRACKS],
) {
    if !muted[0]
        && let Some(note) = pattern.melody[step]
    {
        engine.melody_step(note, pattern.melody_style);
    }
    if !muted[1]
        && let Some(freq) = pattern.arpeggio[step]
    {
        engine.arpeggio(freq);
    }
    if !muted[2]
        && let Some(freq) = pattern.bass[step]
    {
        engine.bass(freq);
    }
    if !muted[3] {
        drums.trigger(pattern.drums[step]);
    }
}

fn trigger_audition(engine: &mut PsgEngine, drums: &mut DrumSampler, request: Audition) {
    match request {
        Audition::Melody => engine.melody(440.0, sequencer::MelodyStyle::Sustain),
        Audition::Melody2 => engine.melody2(440.0),
        Audition::Arpeggio => engine.arpeggio(261.63),
        Audition::Bass => engine.bass(130.81),
        Audition::Drum(drum) => drums.audition(drum),
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

fn poll_state(
    shared: &SharedState,
    snapshot: &mut SequencerState,
    pending_step: &mut Option<usize>,
) -> (bool, Option<Audition>) {
    // Never wait for the UI on the audio thread. Keep the last snapshot if busy.
    let Ok(mut state) = shared.try_lock() else {
        return (false, None);
    };
    if let Some(step) = pending_step.take() {
        state.current_step = step;
    }
    let reset = std::mem::take(&mut state.reset);
    let audition = state.audition.take();
    *snapshot = state.clone();
    (reset, audition)
}

fn publish_step(shared: &SharedState, pending_step: &mut Option<usize>) {
    if let Some(step) = *pending_step
        && let Ok(mut state) = shared.try_lock()
    {
        state.current_step = step;
        *pending_step = None;
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
    let mut drums = DrumSampler::new(config.sample_rate);
    let mut last_mutes = [false; TRACKS];
    let mut last_playing = false;
    let mut snapshot = shared.lock().unwrap().clone();
    let mut pending_step = None;
    let channels = config.channels as usize;

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            let (reset, audition) = poll_state(&shared, &mut snapshot, &mut pending_step);
            let SequencerState {
                bpm,
                ref pattern,
                playing,
                muted,
                ..
            } = snapshot;
            // Stop playback tails, but let a paused audition start cleanly.
            if reset || (!playing && (last_playing || audition.is_some())) {
                engine.reset();
                drums.mute();
            }

            // Stop newly muted notes and muted previews when playback starts.
            for track in 0..TRACKS {
                if muted[track] && (!last_mutes[track] || (playing && !last_playing)) {
                    if track == 3 {
                        drums.mute();
                    } else {
                        engine.mute(track);
                    }
                }
            }
            last_mutes = muted;
            last_playing = playing;

            if let Some(request) = audition {
                trigger_audition(&mut engine, &mut drums, request);
            }

            for frame in data.chunks_mut(channels) {
                if playing {
                    if let Some(step) = clock.advance(bpm) {
                        trigger_step(&mut engine, &mut drums, pattern, step, muted);
                        pending_step = Some(step);
                    }
                } else {
                    // Reset clock position so playback always restarts from step 0.
                    clock.sample_counter = 0;
                    clock.step = 0;
                }

                let sample = if playing || engine.active() || drums.active() {
                    // Leave headroom for simultaneous bass, kit, and melodic voices.
                    (engine.next_sample() + drums.next_sample()) * 0.75
                } else {
                    0.0
                };
                write_frame(frame, sample);
            }
            publish_step(&shared, &mut pending_step);
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
    use crate::sequencer::{Drum, DrumStep, Hat};

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
    fn audio_keeps_snapshot_while_state_is_busy() {
        let shared = new_shared_state();
        let mut snapshot = shared.lock().unwrap().clone();
        let mut pending_step = Some(7);
        let mut state = shared.lock().unwrap();
        state.playing = true;
        state.reset = true;
        state.audition = Some(Audition::Bass);

        assert_eq!(
            poll_state(&shared, &mut snapshot, &mut pending_step),
            (false, None)
        );
        publish_step(&shared, &mut pending_step);
        assert!(!snapshot.playing);
        assert_eq!(pending_step, Some(7));
        assert_eq!(state.current_step, 0);
        drop(state);

        assert_eq!(
            poll_state(&shared, &mut snapshot, &mut pending_step),
            (true, Some(Audition::Bass))
        );
        assert!(snapshot.playing);
        assert_eq!(snapshot.current_step, 7);
        assert_eq!(pending_step, None);
        assert_eq!(
            poll_state(&shared, &mut snapshot, &mut pending_step),
            (false, None)
        );
    }

    #[test]
    fn sequenced_voices_sound() {
        let mut pattern = Pattern {
            melody: [None; sequencer::STEPS],
            arpeggio: [None; sequencer::STEPS],
            bass: [None; sequencer::STEPS],
            drums: [DrumStep::default(); sequencer::STEPS],
            ..Pattern::default()
        };

        for voice in 0..5 {
            let mut engine = PsgEngine::new(48_000);
            match voice {
                0 => {
                    pattern.melody[0] = Some(sequencer::MelodyStep {
                        freq: 440.0,
                        voice: sequencer::MelodyVoice::M1,
                        short: false,
                        bend: false,
                    })
                }
                1 => {
                    pattern.melody[0] = Some(sequencer::MelodyStep {
                        freq: 440.0,
                        voice: sequencer::MelodyVoice::M2,
                        short: false,
                        bend: false,
                    })
                }
                2 => pattern.arpeggio[0] = Some(261.63),
                3 => pattern.bass[0] = Some(130.81),
                _ => pattern.drums[0].snare = true,
            }
            let mut drums = DrumSampler::new(48_000);
            trigger_step(&mut engine, &mut drums, &pattern, 0, [false; TRACKS]);
            assert!(engine.active() || drums.active());
            let peak = (0..4096)
                .map(|_| (engine.next_sample() + drums.next_sample()).abs())
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
            let mut silent_drums = DrumSampler::new(48_000);
            trigger_step(&mut silent, &mut silent_drums, &pattern, 0, muted);
            assert!(
                !silent.active() && !silent_drums.active(),
                "muted voice {voice} played"
            );

            pattern.melody[0] = None;
            pattern.arpeggio[0] = None;
            pattern.bass[0] = None;
            pattern.drums[0] = DrumStep::default();
        }
    }

    #[test]
    fn last_step_sounds() {
        let mut pattern = Pattern {
            melody: [None; sequencer::STEPS],
            arpeggio: [None; sequencer::STEPS],
            bass: [None; sequencer::STEPS],
            drums: [DrumStep::default(); sequencer::STEPS],
            ..Pattern::default()
        };
        pattern.drums[sequencer::STEPS - 1].snare = true;

        let mut engine = PsgEngine::new(48_000);
        let mut drums = DrumSampler::new(48_000);
        trigger_step(
            &mut engine,
            &mut drums,
            &pattern,
            sequencer::STEPS - 1,
            [false; TRACKS],
        );
        assert!(drums.active());
    }

    #[test]
    fn overlapping_bass_and_drums_mute_independently() {
        let mut pattern = Pattern {
            melody: [None; sequencer::STEPS],
            arpeggio: [None; sequencer::STEPS],
            bass: [None; sequencer::STEPS],
            drums: [DrumStep::default(); sequencer::STEPS],
            ..Pattern::default()
        };
        pattern.bass[0] = Some(130.81);
        pattern.drums[0].snare = true;
        pattern.drums[0].hat = Some(Hat::Closed);

        let mut engine = PsgEngine::new(48_000);
        let mut drums = DrumSampler::new(48_000);
        trigger_step(&mut engine, &mut drums, &pattern, 0, [false; TRACKS]);
        assert!(engine.active() && drums.active());

        for track in [2, 3] {
            let mut muted = [false; TRACKS];
            muted[track] = true;
            let mut engine = PsgEngine::new(48_000);
            let mut drums = DrumSampler::new(48_000);
            trigger_step(&mut engine, &mut drums, &pattern, 0, muted);
            assert_eq!(engine.active(), track == 3);
            assert_eq!(drums.active(), track == 2);
            let peak = (0..4096)
                .map(|_| (engine.next_sample() + drums.next_sample()).abs())
                .fold(0.0_f32, f32::max);
            assert!(peak > 0.01, "muting track {track} silenced both voices");
        }
    }

    #[test]
    fn layered_output_keeps_headroom() {
        let mut engine = PsgEngine::new(48_000);
        let mut drums = DrumSampler::new(48_000);
        engine.melody(440.0, sequencer::MelodyStyle::Sustain);
        engine.arpeggio(329.63);
        engine.bass(130.81);
        drums.trigger(DrumStep {
            kick: true,
            snare: true,
            hat: Some(Hat::Closed),
        });

        let peak = (0..48_000)
            .map(|_| ((engine.next_sample() + drums.next_sample()) * 0.75).abs())
            .fold(0.0_f32, f32::max);
        assert!(peak < 1.0, "mixed peak: {peak}");
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
            let mut drums = DrumSampler::new(48_000);
            trigger_audition(&mut engine, &mut drums, request);
            assert!(engine.active() || drums.active());
            let peak = (0..48_000)
                .map(|_| (engine.next_sample() + drums.next_sample()).abs())
                .fold(0.0_f32, f32::max);
            assert!(peak > 0.01, "{request:?} was silent");
            assert!(!engine.active() && !drums.active());
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
