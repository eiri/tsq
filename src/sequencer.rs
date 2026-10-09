use std::sync::{Arc, Mutex};

pub const STEPS: usize = 64;
pub const PAGE_STEPS: usize = 8;
pub const TRACKS: usize = 4;
pub const DEFAULT_BPM: f64 = 120.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MelodyStyle {
    Pluck,
    Sustain,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MelodyVoice {
    M1,
    M2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MelodyStep {
    pub freq: f32,
    pub voice: MelodyVoice,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drum {
    Kick,
    Snare,
    ClosedHat,
    OpenHat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Audition {
    Melody,
    Melody2,
    Arpeggio,
    Bass,
    Drum(Drum),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Major,
    Minor,
}

#[derive(Clone)]
pub struct Pattern {
    pub tonic: u8,
    pub mode: Mode,
    pub melody: [Option<MelodyStep>; STEPS],
    pub arpeggio: [Option<f32>; STEPS],
    pub(crate) arpeggio_roots: [u8; STEPS / PAGE_STEPS],
    pub bass: [Option<f32>; STEPS],
    pub drums: [Option<Drum>; STEPS],
    pub melody_style: MelodyStyle,
}

// Frequencies use equal-tempered C major notes in Hz.
const MELODY: [f32; PAGE_STEPS] = [261.63, 293.66, 329.63, 349.23, 392.0, 440.0, 493.88, 523.25];
const VARIED_STEPS: [usize; 7] = [0, 1, 2, 3, 5, 6, 7];
const ARP_ORDER: [usize; 6] = [0, 1, 2, 2, 1, 0];

fn scale(tonic: u8, mode: Mode) -> [f32; PAGE_STEPS] {
    let intervals = match mode {
        Mode::Major => [0, 2, 4, 5, 7, 9, 11, 12],
        Mode::Minor => [0, 2, 3, 5, 7, 8, 10, 12],
    };

    // MIDI note 60 is middle C; keep generated notes above it.
    intervals
        .map(|interval| 440.0 * 2.0_f32.powf((60.0 + tonic as f32 + interval as f32 - 69.0) / 12.0))
}

fn melody_at(pattern: &Pattern, step: usize) -> f32 {
    (0..STEPS)
        .map(|back| (step + STEPS - back) % STEPS)
        .find_map(|i| pattern.melody[i].map(|note| note.freq))
        .unwrap_or(scale(pattern.tonic, pattern.mode)[0])
}

fn chord_at(tonic: u8, mode: Mode, root: u8) -> [f32; 3] {
    let notes = scale(tonic, mode);

    // Stack thirds in the scale, lifting notes that cross the octave.
    [0, 2, 4].map(|offset| {
        let degree = root as usize + offset;
        let octave = if degree >= 7 { 2.0 } else { 1.0 };
        notes[degree % 7] * octave
    })
}

fn set_arpeggios(pattern: &mut Pattern) {
    for page in 0..STEPS / PAGE_STEPS {
        let chord = chord_at(pattern.tonic, pattern.mode, pattern.arpeggio_roots[page]);
        for step in 0..PAGE_STEPS {
            pattern.arpeggio[page * PAGE_STEPS + step] =
                ARP_ORDER.get(step).map(|&note| chord[note]);
        }
    }
}

fn set_bass(pattern: &mut Pattern) {
    let mut hits = 0;
    for step in 0..STEPS {
        if pattern.bass[step].is_some() {
            // Alternate the melody pitch with its octave on successive bass hits.
            let octave = if hits % 2 == 0 { 1.0 } else { 2.0 };
            pattern.bass[step] = Some(melody_at(pattern, step) * octave);
            hits += 1;
        }
    }
}

impl Default for Pattern {
    fn default() -> Self {
        Self {
            tonic: 0,
            mode: Mode::Major,
            melody: std::array::from_fn(|i| {
                // Change one note per page; keep the first phrase intact.
                let page = i / PAGE_STEPS;
                let i = i % PAGE_STEPS;
                let voice = if [0, 2, 5, 7].contains(&i) {
                    MelodyVoice::M1
                } else if [1, 3, 6].contains(&i) {
                    MelodyVoice::M2
                } else {
                    return None;
                };
                let note = if page > 0 && i == VARIED_STEPS[page - 1] {
                    if i == PAGE_STEPS - 1 { i - 1 } else { i + 1 }
                } else {
                    i
                };
                Some(MelodyStep {
                    freq: MELODY[note],
                    voice,
                })
            }),
            arpeggio: std::array::from_fn(|i| {
                ARP_ORDER
                    .get(i % PAGE_STEPS)
                    .map(|&n| chord_at(0, Mode::Major, 0)[n])
            }),
            arpeggio_roots: [0; STEPS / PAGE_STEPS],
            // Leave channel C free on these steps so the bass can sound.
            bass: std::array::from_fn(|i| [3, 5, 7].contains(&(i % PAGE_STEPS)).then_some(130.81)),
            drums: std::array::from_fn(|i| match i % PAGE_STEPS {
                0 => Some(Drum::Kick),
                2 | 6 => Some(Drum::ClosedHat),
                4 => Some(Drum::Snare),
                _ => None,
            }),
            melody_style: MelodyStyle::Pluck,
        }
    }
}

pub fn random_pattern() -> Pattern {
    let tonic = fastrand::u8(0..12);
    let mode = if fastrand::bool() {
        Mode::Major
    } else {
        Mode::Minor
    };
    let notes = scale(tonic, mode);

    let mut pattern = Pattern {
        tonic,
        mode,
        melody: std::array::from_fn(|i| {
            fastrand::bool().then(|| MelodyStep {
                freq: notes[i % PAGE_STEPS],
                voice: if fastrand::bool() {
                    MelodyVoice::M1
                } else {
                    MelodyVoice::M2
                },
            })
        }),
        arpeggio: [None; STEPS],
        arpeggio_roots: std::array::from_fn(|_| fastrand::u8(0..7)),
        bass: std::array::from_fn(|_| fastrand::bool().then_some(130.81)),
        drums: std::array::from_fn(|_| match fastrand::u8(0..5) {
            0 => Some(Drum::Kick),
            1 => Some(Drum::Snare),
            2 => Some(Drum::ClosedHat),
            3 => Some(Drum::OpenHat),
            _ => None,
        }),
        melody_style: if fastrand::bool() {
            MelodyStyle::Pluck
        } else {
            MelodyStyle::Sustain
        },
    };
    set_arpeggios(&mut pattern);
    set_bass(&mut pattern);
    pattern
}

pub fn random_track(pattern: &mut Pattern, track: usize) {
    // Retry if the selected track happens to match its old pattern.
    loop {
        let next = random_pattern();
        match track {
            0 if pattern.melody != next.melody
                || pattern.melody_style != next.melody_style
                || pattern.tonic != next.tonic
                || pattern.mode != next.mode =>
            {
                pattern.tonic = next.tonic;
                pattern.mode = next.mode;
                pattern.melody = next.melody;
                pattern.melody_style = next.melody_style;
                set_arpeggios(pattern);
                set_bass(pattern);
            }
            1 => {
                let old = pattern.arpeggio;
                pattern.arpeggio_roots = next.arpeggio_roots;
                set_arpeggios(pattern);
                if pattern.arpeggio == old {
                    continue;
                }
            }
            2 => {
                let old = pattern.bass;
                pattern.bass = next.bass;
                set_bass(pattern);
                if pattern.bass == old {
                    continue;
                }
            }
            3 if pattern.drums != next.drums => pattern.drums = next.drums,
            0..=3 => continue,
            _ => return,
        }
        return;
    }
}

#[derive(Clone)]
pub struct SequencerState {
    pub pattern: Pattern,
    pub bpm: f64,
    pub current_step: usize,
    pub playing: bool,
    pub muted: [bool; TRACKS],
    pub reset: bool,
    pub audition: Option<Audition>,
}

impl Default for SequencerState {
    fn default() -> Self {
        Self {
            pattern: Pattern::default(),
            bpm: DEFAULT_BPM,
            current_step: 0,
            playing: false,
            muted: [false; TRACKS],
            reset: false,
            audition: None,
        }
    }
}

pub type SharedState = Arc<Mutex<SequencerState>>;

pub fn new_shared_state() -> SharedState {
    Arc::new(Mutex::new(SequencerState::default()))
}

pub struct AudioClock {
    sample_rate: f64,
    pub sample_counter: usize,
    pub step: usize,
}

impl AudioClock {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            sample_counter: 0,
            step: 0,
        }
    }

    pub fn step_samples(&self, bpm: f64) -> usize {
        ((60.0 / bpm) * self.sample_rate) as usize / 2
    }

    pub fn advance(&mut self, bpm: f64) -> Option<usize> {
        // Trigger at the start of each interval, including the first sample.
        let triggered = (self.sample_counter == 0).then_some(self.step);
        self.sample_counter += 1;

        if self.sample_counter >= self.step_samples(bpm) {
            self.sample_counter = 0;
            self.step = (self.step + 1) % STEPS;
        }

        triggered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_pattern_fills_all_pages() {
        let p = Pattern::default();
        assert_eq!(p.melody.len(), STEPS);
        assert_eq!(p.arpeggio.len(), STEPS);
        assert_eq!(p.bass.len(), STEPS);
        assert_eq!(p.drums.len(), STEPS);

        for page in 1..STEPS / PAGE_STEPS {
            let start = page * PAGE_STEPS;
            let changed = (0..PAGE_STEPS)
                .filter(|&i| p.melody[start + i] != p.melody[i])
                .collect::<Vec<_>>();
            assert_eq!(changed, [VARIED_STEPS[page - 1]]);

            for i in 0..PAGE_STEPS {
                assert_eq!(
                    p.melody[start + i].map(|n| n.voice),
                    p.melody[i].map(|n| n.voice)
                );
                assert_eq!(p.arpeggio[start + i], p.arpeggio[i]);
                assert_eq!(p.bass[start + i], p.bass[i]);
                assert_eq!(p.drums[start + i], p.drums[i]);
            }
        }
    }

    #[test]
    fn melody_steps_choose_one_variation() {
        let pattern = Pattern::default();
        assert_eq!(pattern.melody[0].unwrap().voice, MelodyVoice::M1);
        assert_eq!(pattern.melody[1].unwrap().voice, MelodyVoice::M2);
        assert!(pattern.melody[4].is_none());
    }

    #[test]
    fn default_pattern_has_kick_on_first_step() {
        let p = Pattern::default();
        assert_eq!(p.drums[0], Some(Drum::Kick));
        assert_eq!(p.drums[6], Some(Drum::ClosedHat));
        assert_eq!(p.bass[5], Some(130.81));
    }

    #[test]
    fn default_pattern_snare_on_beat_five() {
        let p = Pattern::default();
        assert_eq!(p.drums[4], Some(Drum::Snare));
    }

    #[test]
    fn clock_advances_step_after_correct_sample_count() {
        let bpm = 120.0;
        let sr = 44100.0;
        let mut clock = AudioClock::new(sr);
        let threshold = clock.step_samples(bpm);

        assert_eq!(clock.advance(bpm), Some(0));
        for _ in 1..threshold {
            assert_eq!(clock.advance(bpm), None);
        }
        assert_eq!(clock.advance(bpm), Some(1));
    }

    #[test]
    fn clock_wraps_around_after_64_steps() {
        let bpm = 240.0;
        let sr = 44100.0;
        let mut clock = AudioClock::new(sr);
        let threshold = clock.step_samples(bpm);

        for _ in 0..(threshold * (STEPS - 1)) {
            clock.advance(bpm);
        }
        assert_eq!(clock.step, STEPS - 1);
        assert_eq!(clock.advance(bpm), Some(STEPS - 1));
        for _ in 1..threshold {
            clock.advance(bpm);
        }
        assert_eq!(clock.step, 0);
        assert_eq!(clock.advance(bpm), Some(0));
    }

    #[test]
    fn clock_restart_triggers_zero() {
        let mut clock = AudioClock::new(48000.0);
        for _ in 0..12001 {
            clock.advance(120.0);
        }

        clock.sample_counter = 0;
        clock.step = 0;
        assert_eq!(clock.advance(120.0), Some(0));
    }

    #[test]
    fn step_samples_scales_with_bpm() {
        let clock_slow = AudioClock::new(44100.0);
        let clock_fast = AudioClock::new(44100.0);
        assert!(clock_slow.step_samples(60.0) > clock_fast.step_samples(120.0));
    }

    #[test]
    fn shared_state_default_is_paused_at_120_bpm() {
        let state = new_shared_state();
        let s = state.lock().unwrap();
        let paused = !s.playing;
        assert!(paused);
        assert_eq!(s.bpm, 120.0);
        assert_eq!(s.current_step, 0);
        assert_eq!(s.muted, [false; TRACKS]);
    }

    #[test]
    fn shared_state_can_be_mutated_across_clone() {
        let state = new_shared_state();
        let clone = Arc::clone(&state);
        {
            let mut s = clone.lock().unwrap();
            s.bpm = 140.0;
            s.pattern.drums[2] = Some(Drum::Kick);
        }
        let s = state.lock().unwrap();
        assert_eq!(s.bpm, 140.0);
        assert_eq!(s.pattern.drums[2], Some(Drum::Kick));
    }

    #[test]
    fn random_track_changes_only_selected_row() {
        for track in 0..4 {
            let mut pattern = Pattern::default();
            let before = pattern.clone();
            random_track(&mut pattern, track);

            assert_eq!(
                pattern.melody == before.melody && pattern.melody_style == before.melody_style,
                track != 0
            );
            if track >= 2 {
                assert_eq!(pattern.arpeggio, before.arpeggio);
            } else if track == 1 {
                assert_ne!(pattern.arpeggio, before.arpeggio);
            }
            if track >= 1 && track != 2 {
                assert_eq!(pattern.bass, before.bass);
            } else if track == 2 {
                assert_ne!(pattern.bass, before.bass);
            }
            assert_eq!(pattern.drums == before.drums, track != 3);
            if track != 0 {
                assert_eq!((pattern.tonic, pattern.mode), (before.tonic, before.mode));
            }
            if track != 1 {
                assert_eq!(pattern.arpeggio_roots, before.arpeggio_roots);
            } else {
                assert_ne!(pattern.arpeggio_roots, before.arpeggio_roots);
            }
            if track <= 2 {
                let mut expected = pattern.clone();
                set_arpeggios(&mut expected);
                set_bass(&mut expected);
                if track == 0 || track == 1 {
                    assert_eq!(pattern.arpeggio, expected.arpeggio);
                }
                if track == 0 || track == 2 {
                    assert_eq!(pattern.bass, expected.bass);
                }
            }
        }
    }

    #[test]
    fn scale_uses_major_and_minor_intervals() {
        let major = scale(0, Mode::Major);
        let minor = scale(0, Mode::Minor);
        assert_eq!(major[0], minor[0]);
        assert!(major[2] > minor[2]);
        assert!(major[5] > minor[5]);
        assert!((major[7] / major[0] - 2.0).abs() < 0.001);
    }

    #[test]
    fn chords_stack_scale_thirds() {
        for mode in [Mode::Major, Mode::Minor] {
            let notes = scale(2, mode);
            assert_eq!(
                chord_at(2, mode, 5),
                [notes[5], notes[0] * 2.0, notes[2] * 2.0]
            );
        }
    }

    #[test]
    fn arpeggios_fill_six_steps_per_page() {
        for mode in [Mode::Major, Mode::Minor] {
            let mut pattern = Pattern {
                tonic: 2,
                mode,
                arpeggio_roots: [0, 1, 2, 3, 4, 5, 6, 0],
                ..Pattern::default()
            };
            set_arpeggios(&mut pattern);
            for page in 0..STEPS / PAGE_STEPS {
                let chord = chord_at(2, mode, pattern.arpeggio_roots[page]);
                let start = page * PAGE_STEPS;
                assert_eq!(
                    pattern.arpeggio[start..start + PAGE_STEPS],
                    [
                        Some(chord[0]),
                        Some(chord[1]),
                        Some(chord[2]),
                        Some(chord[2]),
                        Some(chord[1]),
                        Some(chord[0]),
                        None,
                        None
                    ]
                );
            }
        }
    }

    #[test]
    fn bass_repeats_melody_at_next_octave() {
        let mut pattern = Pattern {
            bass: [None; STEPS],
            ..Pattern::default()
        };
        pattern.bass[0] = Some(0.0);
        pattern.bass[1] = Some(0.0);
        pattern.bass[3] = Some(0.0);
        pattern.melody[1] = None;
        set_bass(&mut pattern);

        assert_eq!(pattern.bass[0], Some(pattern.melody[0].unwrap().freq));
        assert_eq!(pattern.bass[1], Some(pattern.melody[0].unwrap().freq * 2.0));
        assert_eq!(pattern.bass[3], Some(pattern.melody[3].unwrap().freq));
    }

    #[test]
    fn random_pattern_has_64_steps() {
        let p = random_pattern();
        assert_eq!(p.drums.len(), STEPS);
        assert_eq!(p.melody.len(), STEPS);
        assert_eq!(p.arpeggio.len(), STEPS);
        assert_eq!(p.bass.len(), STEPS);

        let notes = scale(p.tonic, p.mode);
        for note in p.melody.iter().flatten() {
            assert!(notes.contains(&note.freq));
        }
        let mut hits = 0;
        for step in 0..STEPS {
            let page = step / PAGE_STEPS;
            let chord = chord_at(p.tonic, p.mode, p.arpeggio_roots[page]);
            assert_eq!(
                p.arpeggio[step],
                ARP_ORDER.get(step % PAGE_STEPS).map(|&n| chord[n])
            );
            if let Some(bass) = p.bass[step] {
                assert_eq!(
                    bass,
                    melody_at(&p, step) * if hits % 2 == 0 { 1.0 } else { 2.0 }
                );
                hits += 1;
            }
        }
    }

    #[test]
    fn random_pattern_differs_from_default() {
        let default = Pattern::default();
        let random = random_pattern();
        let same = default.drums == random.drums
            && default.melody == random.melody
            && default.arpeggio == random.arpeggio
            && default.bass == random.bass;
        assert!(!same, "random pattern should differ from default");
    }
}
