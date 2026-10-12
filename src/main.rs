mod arrange;
mod melody;
mod music;
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
    bpm: f64,
) {
    if !muted[0]
        && let Some(note) = pattern.melody[step]
    {
        engine.melody_step(note, pattern.melody_style, bpm);
    }
    if !muted[1]
        && let Some(note) = pattern.arpeggio[step]
    {
        engine.tone_step(1, note, bpm);
    }
    if !muted[2]
        && let Some(note) = pattern.bass[step]
    {
        engine.tone_step(2, note, bpm);
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

fn mix_sample(engine: &mut PsgEngine, drums: &mut DrumSampler) -> f32 {
    // Leave headroom for simultaneous bass, kit, and melodic voices.
    (engine.next_sample() + drums.next_sample()) * 0.75
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
                        trigger_step(&mut engine, &mut drums, pattern, step, muted, bpm);
                        pending_step = Some(step);
                    }
                } else {
                    // Reset clock position so playback always restarts from step 0.
                    clock.sample_counter = 0;
                    clock.step = 0;
                }

                let sample = if playing || engine.active() || drums.active() {
                    mix_sample(&mut engine, &mut drums)
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
                        pitch: 69,
                        voice: sequencer::MelodyVoice::M1,
                        duration: 144,
                        bend: false,
                    })
                }
                1 => {
                    pattern.melody[0] = Some(sequencer::MelodyStep {
                        pitch: 69,
                        voice: sequencer::MelodyVoice::M2,
                        duration: 144,
                        bend: false,
                    })
                }
                2 => pattern.arpeggio[0] = Some(sequencer::ToneStep::new(60, 48)),
                3 => pattern.bass[0] = Some(sequencer::ToneStep::new(48, 32)),
                _ => pattern.drums[0].snare = true,
            }
            let mut drums = DrumSampler::new(48_000);
            trigger_step(&mut engine, &mut drums, &pattern, 0, [false; TRACKS], 120.0);
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
            trigger_step(&mut silent, &mut silent_drums, &pattern, 0, muted, 120.0);
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
            120.0,
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
        pattern.bass[0] = Some(sequencer::ToneStep::new(48, 32));
        pattern.drums[0].snare = true;
        pattern.drums[0].hat = Some(Hat::Closed);

        let mut engine = PsgEngine::new(48_000);
        let mut drums = DrumSampler::new(48_000);
        trigger_step(&mut engine, &mut drums, &pattern, 0, [false; TRACKS], 120.0);
        assert!(engine.active() && drums.active());

        for track in [2, 3] {
            let mut muted = [false; TRACKS];
            muted[track] = true;
            let mut engine = PsgEngine::new(48_000);
            let mut drums = DrumSampler::new(48_000);
            trigger_step(&mut engine, &mut drums, &pattern, 0, muted, 120.0);
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

    fn render_loops(pattern: &Pattern, bpm: f64, rate: u32, muted: [bool; TRACKS]) -> Vec<f32> {
        let mut clock = AudioClock::new(rate as f64);
        let mut engine = PsgEngine::new(rate);
        let mut drums = DrumSampler::new(rate);
        let frames = clock.step_samples(bpm) * sequencer::STEPS * 2;
        (0..frames)
            .map(|_| {
                if let Some(step) = clock.advance(bpm) {
                    trigger_step(&mut engine, &mut drums, pattern, step, muted, bpm);
                }
                mix_sample(&mut engine, &mut drums)
            })
            .collect()
    }

    fn fixtures() -> Vec<(
        &'static str,
        [Option<sequencer::MelodyStep>; sequencer::STEPS],
    )> {
        let note = |pitch, duration| sequencer::MelodyStep {
            pitch,
            duration,
            voice: sequencer::MelodyVoice::M1,
            bend: false,
        };
        let mut sparse = [None; sequencer::STEPS];
        for step in (0..64).step_by(8) {
            sparse[step] = Some(note(72, 120));
        }
        let dense =
            std::array::from_fn(|s| Some(note([72, 74, 76, 77, 79, 77, 76, 74][s % 8], 32)));
        let mut held = [None; sequencer::STEPS];
        held[7] = Some(note(76, 1500));
        held[39] = Some(note(74, 1500));
        let mut chromatic = sparse;
        for step in (0..64).step_by(8) {
            chromatic[step] = Some(note([70, 63, 66, 61][step / 8 % 4], 420));
        }
        let low = sparse.map(|n| {
            n.map(|mut n| {
                n.pitch = 36;
                n
            })
        });
        let high = sparse.map(|n| {
            n.map(|mut n| {
                n.pitch = 108;
                n
            })
        });
        vec![
            ("sparse", sparse),
            ("dense", dense),
            ("held", held),
            ("chromatic", chromatic),
            ("low", low),
            ("high", high),
            ("empty", [None; sequencer::STEPS]),
        ]
    }

    fn fixture_pattern(melody: [Option<sequencer::MelodyStep>; sequencer::STEPS]) -> Pattern {
        let base = Pattern {
            tonic: 0,
            mode: sequencer::Mode::Major,
            melody_style: sequencer::MelodyStyle::Sustain,
            ..sequencer::seeded_pattern(42)
        };
        arrange::with_melody(&base, melody)
    }

    fn diagnostics(pattern: &Pattern) -> (u32, u32, usize, usize, usize) {
        let info = music::MelodyInfo::new(&pattern.melody);
        let conflict = (0..music::BARS)
            .map(|b| arrange::unresolved_units(&info, b, pattern.harmony.chords[b]))
            .sum();
        let arp: Vec<_> = pattern.arpeggio.iter().flatten().map(|n| n.pitch).collect();
        let movement = if arp.is_empty() {
            0
        } else {
            arp.iter()
                .zip(arp.iter().cycle().skip(1))
                .map(|(&a, &b)| u32::from(a.abs_diff(b)))
                .sum()
        };
        let crossings = pattern
            .arpeggio
            .iter()
            .enumerate()
            .filter(|(s, n)| {
                n.is_some_and(|n| {
                    info.sounding(*s as u16 * music::STEP_UNITS)
                        .is_some_and(|p| n.pitch > p)
                })
            })
            .count();
        let changes = pattern
            .harmony
            .chords
            .iter()
            .zip(pattern.harmony.chords.iter().cycle().skip(1))
            .filter(|(a, b)| a != b)
            .count();
        (conflict, movement, crossings, changes, arp.len())
    }

    #[test]
    fn seeded_arrangement_evaluation() {
        let mut totals = (0u64, 0u64, 0usize, 0usize, 0usize);
        let mut peak = 0.0_f32;
        for tonic in 0..12 {
            for mode in [sequencer::Mode::Major, sequencer::Mode::Minor] {
                for seed in [0, 42, 1023] {
                    let mut pattern = sequencer::seeded_pattern(seed);
                    pattern.tonic = tonic;
                    pattern.mode = mode;
                    (pattern.melody, pattern.melody_style) = crate::melody::generate(
                        tonic,
                        mode,
                        &mut fastrand::Rng::with_seed(pattern.seeds.melody),
                    );
                    let melody = pattern.melody;
                    arrange::arrange(&mut pattern);
                    assert_eq!(pattern.melody, melody, "{tonic} {mode:?} seed {seed}");
                    let mut repeated = pattern.clone();
                    arrange::arrange(&mut repeated);
                    assert_eq!(pattern, repeated, "{tonic} {mode:?} seed {seed}");
                    for bar in 0..music::BARS {
                        assert_eq!(
                            pattern.bass[bar * 8].unwrap().pitch % 12,
                            pattern.harmony.chords[bar].root,
                            "{tonic} {mode:?} seed {seed}, bar {bar}"
                        );
                    }
                    for (step, note) in pattern
                        .arpeggio
                        .iter()
                        .enumerate()
                        .filter_map(|(s, n)| n.map(|n| (s, n)))
                    {
                        assert!(
                            (arrange::ARP_LOW..=arrange::ARP_HIGH).contains(&note.pitch),
                            "{tonic} {mode:?} seed {seed}, step {step}"
                        );
                        assert!(
                            pattern.harmony.chords[step / 8]
                                .classes()
                                .contains(&(note.pitch % 12)),
                            "{tonic} {mode:?} seed {seed}, step {step}"
                        );
                    }
                    let d = diagnostics(&pattern);
                    totals.0 += d.0 as u64;
                    totals.1 += d.1 as u64;
                    totals.2 += d.2;
                    totals.3 += d.3;
                    totals.4 += d.4;
                    for sample in render_loops(&pattern, 120.0, 8000, [false; TRACKS]) {
                        assert!(
                            sample.is_finite() && sample.abs() < 1.0,
                            "{tonic} {mode:?} seed {seed}: sample {sample}"
                        );
                        peak = peak.max(sample.abs());
                    }
                }
            }
        }
        // Diagnostics describe music, not pass/fail quality targets. Decorations are allowed.
        println!(
            "72 arrangements: weighted unresolved units={}, arp motion={}, crossings={}, chord changes={}, arp attacks={}, peak={peak:.3}",
            totals.0, totals.1, totals.2, totals.3, totals.4
        );
    }

    #[test]
    fn authored_fixture_evaluation() {
        for (name, melody) in fixtures() {
            let pattern = fixture_pattern(melody);
            assert_eq!(pattern.melody, melody, "{name}");
            for bpm in [60.0, 120.0, 180.0] {
                for muted in [[false; TRACKS], [false, true, true, true]] {
                    let samples = render_loops(&pattern, bpm, 8000, muted);
                    assert!(
                        samples.iter().all(|s| s.is_finite() && s.abs() < 1.0),
                        "{name}, BPM {bpm}"
                    );
                }
            }
            println!("{name}: {:?}", diagnostics(&pattern));
        }
    }

    #[test]
    fn cold_start_and_loop_carry() {
        let mut melody = [None; sequencer::STEPS];
        melody[63] = Some(sequencer::MelodyStep {
            pitch: 69,
            duration: 180,
            voice: sequencer::MelodyVoice::M1,
            bend: false,
        });
        let pattern = arrange::with_melody(&Pattern::default(), melody);
        let samples = render_loops(&pattern, 120.0, 8000, [false, true, true, true]);
        let step_frames = AudioClock::new(8000.0).step_samples(120.0);
        assert!(samples[..step_frames * 8].iter().all(|s| s.abs() < 0.01));
        let loop_start = step_frames * sequencer::STEPS;
        assert!(
            samples[loop_start..loop_start + step_frames]
                .iter()
                .any(|s| s.abs() > 0.01)
        );
        assert!(
            render_loops(&pattern, 120.0, 8000, [true; TRACKS])
                .iter()
                .all(|s| s.abs() < 0.01)
        );
    }

    fn write_wav(path: &std::path::Path, samples: &[f32], rate: u32) -> std::io::Result<()> {
        use std::io::Write;
        // Buffer PCM writes; per-sample file syscalls would dominate offline rendering.
        let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
        let bytes = samples.len() as u32 * 2;
        file.write_all(b"RIFF")?;
        file.write_all(&(36 + bytes).to_le_bytes())?;
        file.write_all(b"WAVEfmt ")?;
        file.write_all(&16u32.to_le_bytes())?;
        file.write_all(&1u16.to_le_bytes())?;
        file.write_all(&1u16.to_le_bytes())?;
        file.write_all(&rate.to_le_bytes())?;
        file.write_all(&(rate * 2).to_le_bytes())?;
        file.write_all(&2u16.to_le_bytes())?;
        file.write_all(&16u16.to_le_bytes())?;
        file.write_all(b"data")?;
        file.write_all(&bytes.to_le_bytes())?;
        for sample in samples {
            file.write_all(
                &((sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16).to_le_bytes(),
            )?;
        }
        file.flush()
    }

    fn write_events(path: &std::path::Path, pattern: &Pattern, seed: u64) -> std::io::Result<()> {
        use std::io::Write;
        let mut file = std::fs::File::create(path)?;
        writeln!(
            file,
            "meta,{},{},{},{}",
            pattern.tonic,
            u8::from(pattern.mode == sequencer::Mode::Minor),
            u8::from(pattern.melody_style == sequencer::MelodyStyle::Sustain),
            seed
        )?;
        for step in 0..sequencer::STEPS {
            if let Some(n) = pattern.melody[step] {
                writeln!(
                    file,
                    "melody,{step},{},{},{},{}",
                    n.pitch,
                    n.duration,
                    u8::from(n.voice == sequencer::MelodyVoice::M2),
                    u8::from(n.bend)
                )?;
            }
            for (name, note) in [
                ("arp", pattern.arpeggio[step]),
                ("bass", pattern.bass[step]),
            ] {
                if let Some(n) = note {
                    writeln!(file, "{name},{step},{},{}", n.pitch, n.duration)?;
                }
            }
            let drum = pattern.drums[step];
            let hat = match drum.hat {
                None => 0,
                Some(Hat::Closed) => 1,
                Some(Hat::Open) => 2,
            };
            writeln!(
                file,
                "drum,{step},{},{},{hat}",
                u8::from(drum.kick),
                u8::from(drum.snare)
            )?;
        }
        for (bar, chord) in pattern.harmony.chords.iter().enumerate() {
            let quality = match chord.quality {
                arrange::Quality::Major => 0,
                arrange::Quality::Minor => 1,
                arrange::Quality::Diminished => 2,
            };
            writeln!(file, "chord,{bar},{},{quality}", chord.root)?;
        }
        Ok(())
    }

    fn read_events(path: &std::path::Path) -> Pattern {
        let mut pattern = Pattern {
            melody: [None; sequencer::STEPS],
            arpeggio: [None; sequencer::STEPS],
            bass: [None; sequencer::STEPS],
            drums: [DrumStep::default(); sequencer::STEPS],
            ..Pattern::default()
        };
        for line in std::fs::read_to_string(path).unwrap().lines() {
            let cells: Vec<_> = line.split(',').collect();
            let number = |i: usize| cells[i].parse::<u16>().unwrap();
            let step = number(1) as usize;
            match cells[0] {
                "meta" => {
                    pattern.tonic = number(1) as u8;
                    pattern.mode = if number(2) == 0 {
                        sequencer::Mode::Major
                    } else {
                        sequencer::Mode::Minor
                    };
                    pattern.melody_style = if number(3) == 0 {
                        sequencer::MelodyStyle::Pluck
                    } else {
                        sequencer::MelodyStyle::Sustain
                    };
                    pattern.seeds = sequencer::seeded_pattern(cells[4].parse().unwrap()).seeds;
                }
                "melody" => {
                    pattern.melody[step] = Some(sequencer::MelodyStep {
                        pitch: number(2) as u8,
                        duration: number(3),
                        voice: if number(4) == 0 {
                            sequencer::MelodyVoice::M1
                        } else {
                            sequencer::MelodyVoice::M2
                        },
                        bend: number(5) != 0,
                    })
                }
                "arp" => {
                    pattern.arpeggio[step] =
                        Some(sequencer::ToneStep::new(number(2) as u8, number(3)))
                }
                "bass" => {
                    pattern.bass[step] = Some(sequencer::ToneStep::new(number(2) as u8, number(3)))
                }
                "drum" => {
                    pattern.drums[step] = DrumStep {
                        kick: number(2) != 0,
                        snare: number(3) != 0,
                        hat: match number(4) {
                            0 => None,
                            1 => Some(Hat::Closed),
                            _ => Some(Hat::Open),
                        },
                    }
                }
                "chord" => {
                    pattern.harmony.chords[step] = arrange::Chord {
                        root: number(2) as u8,
                        quality: match number(3) {
                            0 => arrange::Quality::Major,
                            1 => arrange::Quality::Minor,
                            _ => arrange::Quality::Diminished,
                        },
                    }
                }
                _ => panic!("unknown event in {}: {line}", path.display()),
            }
        }
        pattern
    }

    #[test]
    #[ignore = "renders locally adapted baseline events from the step-1 worktree"]
    fn render_baseline() {
        let directory = std::path::Path::new("target/listening");
        let source = directory.join("baseline-events");
        assert!(
            source.exists(),
            "export baseline events from worktree 9b6b2bf first"
        );
        let output = directory.join("baseline");
        std::fs::create_dir_all(&output).unwrap();
        for entry in std::fs::read_dir(source).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "csv") {
                continue;
            }
            let pattern = read_events(&path);
            let name = path.file_stem().unwrap().to_str().unwrap();
            if let Some(name) = name.strip_prefix("arranger-") {
                let after = read_events(&directory.join("events").join(format!("{name}.csv")));
                assert_eq!(pattern.melody, after.melody, "matched melody: {name}");
                assert_eq!(
                    pattern.melody_style, after.melody_style,
                    "matched style: {name}"
                );
            }
            for bpm in [60.0, 120.0, 180.0] {
                for (part, muted) in [
                    ("full", [false; TRACKS]),
                    ("lead", [false, true, true, true]),
                ] {
                    let samples = render_loops(&pattern, bpm, 22050, muted);
                    assert!(
                        samples.iter().all(|s| s.is_finite() && s.abs() < 1.0),
                        "{name} {bpm} {part}"
                    );
                    let path = output.join(format!("{name}-{bpm}-{part}.wav"));
                    write_wav(&path, &samples, 22050).unwrap();
                    println!("{}", path.display());
                }
            }
        }
    }

    #[test]
    #[ignore = "writes two-loop WAV examples under target/listening"]
    fn render_listening() {
        let directory = std::path::Path::new("target/listening");
        std::fs::create_dir_all(directory.join("events")).unwrap();
        let mut examples = Vec::new();
        for tonic in [0, 5, 11] {
            for mode in [sequencer::Mode::Major, sequencer::Mode::Minor] {
                for seed in [0, 42] {
                    let mut pattern = sequencer::seeded_pattern(seed);
                    pattern.tonic = tonic;
                    pattern.mode = mode;
                    (pattern.melody, pattern.melody_style) = crate::melody::generate(
                        tonic,
                        mode,
                        &mut fastrand::Rng::with_seed(pattern.seeds.melody),
                    );
                    arrange::arrange(&mut pattern);
                    examples.push((format!("key{tonic}-{mode:?}-seed{seed}"), seed, pattern));
                }
            }
        }
        for (name, melody) in fixtures() {
            examples.push((name.to_owned(), 42, fixture_pattern(melody)));
        }
        for (name, seed, pattern) in examples {
            write_events(
                &directory.join("events").join(format!("{name}.csv")),
                &pattern,
                seed,
            )
            .unwrap();
            for bpm in [60.0, 120.0, 180.0] {
                for (part, muted) in [
                    ("full", [false; TRACKS]),
                    ("lead", [false, true, true, true]),
                ] {
                    let samples = render_loops(&pattern, bpm, 22050, muted);
                    assert!(
                        samples.iter().all(|s| s.is_finite() && s.abs() < 1.0),
                        "{name} {bpm} {part}"
                    );
                    let path = directory.join(format!("{name}-{bpm}-{part}.wav"));
                    write_wav(&path, &samples, 22050).unwrap();
                    println!("{}", path.display());
                }
            }
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
