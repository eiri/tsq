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

// Ready cue (0:12–0:17) followed by the opening of the next theme.
// MIDI notes; zero marks a held note or a rest.
const MELODY: [u8; STEPS] = [
    84, 0, 0, 83, 0, 82, 83, 0, 0, 0, 0, 81, 0, 0, 0, 79, 0, 78, 0, 79, 0, 0, 0, 81, 0, 78, 0, 79,
    0, 78, 79, 0, 78, 79, 0, 74, 77, 0, 79, 81, 83, 84, 0, 0, 0, 0, 0, 65, 0, 0, 62, 0, 57, 0, 62,
    64, 65, 0, 67, 0, 62, 65, 60, 64,
];
const ARP_ORDER: [usize; 6] = [0, 1, 2, 2, 1, 0];
// C, Bb, G, Eb, Eb, C, Dm, G: follow the cue, then the next theme.
const DEFAULT_ROOTS: [u8; STEPS / PAGE_STEPS] = [0, 7, 4, 8, 8, 0, 1, 4];
const BASS_PATTERNS: [[bool; PAGE_STEPS]; 4] = [
    [true, true, true, true, false, false, false, false],
    [true, true, false, true, false, false, false, false],
    [true, false, true, true, false, false, false, false],
    [true, false, false, true, false, false, false, false],
];

fn scale(tonic: u8, mode: Mode) -> [f32; PAGE_STEPS] {
    let intervals = match mode {
        Mode::Major => [0, 2, 4, 5, 7, 9, 11, 12],
        Mode::Minor => [0, 2, 3, 5, 7, 8, 10, 12],
    };

    // MIDI note 60 is middle C; keep generated notes above it.
    intervals
        .map(|interval| 440.0 * 2.0_f32.powf((60.0 + tonic as f32 + interval as f32 - 69.0) / 12.0))
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
        let chord = match pattern.arpeggio_roots[page] {
            // Borrow Bb and Eb major to follow the cue's chromatic notes.
            7 | 8 => {
                let note = if pattern.arpeggio_roots[page] == 7 {
                    70
                } else {
                    63
                };
                let root = 440.0 * 2.0_f32.powf((note as f32 + pattern.tonic as f32 - 69.0) / 12.0);
                [
                    root,
                    root * 2.0_f32.powf(4.0 / 12.0),
                    root * 2.0_f32.powf(7.0 / 12.0),
                ]
            }
            root => chord_at(pattern.tonic, pattern.mode, root),
        };
        for step in 0..PAGE_STEPS {
            pattern.arpeggio[page * PAGE_STEPS + step] =
                ARP_ORDER.get(step).map(|&note| chord[note]);
        }
    }
}

fn set_drums(pattern: &mut Pattern) {
    // Choose one drum phrase around bass hits, then repeat its sounds on every page.
    for step in 0..PAGE_STEPS {
        pattern.drums[step] = if pattern.bass[step].is_some() {
            None
        } else {
            match fastrand::u8(0..5) {
                0 => Some(Drum::Kick),
                1 => Some(Drum::Snare),
                2 => Some(Drum::ClosedHat),
                3 => Some(Drum::OpenHat),
                _ => None,
            }
        };
    }

    // Keep the first drum hit a kick, even when every random choice was empty.
    let first = (0..PAGE_STEPS)
        .find(|&step| pattern.drums[step].is_some())
        .or_else(|| (0..PAGE_STEPS).find(|&step| pattern.bass[step].is_none()));
    if let Some(step) = first {
        pattern.drums[step] = Some(Drum::Kick);
    }

    for step in PAGE_STEPS..STEPS {
        pattern.drums[step] = pattern.drums[step % PAGE_STEPS];
    }
}

fn set_bass(pattern: &mut Pattern) {
    for page in 0..STEPS / PAGE_STEPS {
        let root = pattern.arpeggio[page * PAGE_STEPS].map(|freq| freq / 4.0);
        for step in 0..PAGE_STEPS {
            let index = page * PAGE_STEPS + step;
            // Preserve the chosen rhythm while following this page's chord root.
            pattern.bass[index] = match step {
                0..=3 if pattern.bass[index].is_some() => root,
                5 if pattern.bass[index].is_some() => root.map(|freq| freq * 2.0),
                _ => None,
            };
        }
    }
}

impl Default for Pattern {
    fn default() -> Self {
        let mut pattern = Self {
            tonic: 0,
            mode: Mode::Major,
            melody: MELODY.map(|note| {
                (note != 0).then(|| MelodyStep {
                    freq: 440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0),
                    voice: MelodyVoice::M1,
                })
            }),
            arpeggio: [None; STEPS],
            arpeggio_roots: DEFAULT_ROOTS,
            bass: std::array::from_fn(|i| BASS_PATTERNS[2][i % PAGE_STEPS].then_some(0.0)),
            drums: std::array::from_fn(|i| match i % PAGE_STEPS {
                4 => Some(Drum::Kick),
                5 => Some(Drum::Snare),
                6 => Some(Drum::ClosedHat),
                _ => None,
            }),
            melody_style: MelodyStyle::Sustain,
        };
        set_arpeggios(&mut pattern);
        set_bass(&mut pattern);
        pattern
    }
}

fn random_melody(tonic: u8, mode: Mode, rng: &mut fastrand::Rng) -> [Option<MelodyStep>; STEPS] {
    let notes = scale(tonic, mode);
    let voice = if rng.bool() {
        MelodyVoice::M1
    } else {
        MelodyVoice::M2
    };

    // Reuse one rhythm and a nearby-note walk across the eight measures.
    let mut hits = [false; PAGE_STEPS];
    hits[0] = true;
    hits[PAGE_STEPS - 1] = true;
    let count = rng.usize(4..7);
    while hits.iter().filter(|&&hit| hit).count() < count {
        hits[rng.usize(1..PAGE_STEPS - 1)] = true;
    }
    let penultimate = (1..PAGE_STEPS - 1).rev().find(|&i| hits[i]).unwrap();
    let mut degrees = [0usize; PAGE_STEPS];
    let mut pitch = rng.usize(0..3);
    for i in 0..PAGE_STEPS {
        if !hits[i] {
            continue;
        }
        pitch = if i == penultimate {
            pitch.clamp(1, 2)
        } else if i == PAGE_STEPS - 1 {
            2
        } else if i == 0 {
            pitch
        } else {
            let movement = match rng.usize(0..10) {
                0 => 2,
                1..=4 => 1,
                5..=8 => -1,
                _ => -2,
            };
            (pitch as i32 + movement).clamp(0, 3) as usize
        };
        degrees[i] = pitch;
    }

    let mut melody = [None; STEPS];
    for page in 0..STEPS / PAGE_STEPS {
        for i in 0..PAGE_STEPS {
            if !hits[i] {
                continue;
            }
            let mut degree = degrees[i];
            if page % 4 == 2 && i == penultimate {
                degree = if degree == 1 { 2 } else { 1 };
            }
            if i == PAGE_STEPS - 1 {
                degree = match page {
                    3 => 1,
                    7 => 0,
                    _ => degree,
                };
            }
            melody[page * PAGE_STEPS + i] = Some(MelodyStep {
                freq: notes[degree],
                voice,
            });
        }
    }
    melody
}

pub fn random_pattern() -> Pattern {
    let tonic = fastrand::u8(0..12);
    let mode = if fastrand::bool() {
        Mode::Major
    } else {
        Mode::Minor
    };
    let melody = random_melody(tonic, mode, &mut fastrand::Rng::new());
    let rhythm = fastrand::usize(0..BASS_PATTERNS.len());
    let high = fastrand::bool();

    let mut pattern = Pattern {
        tonic,
        mode,
        melody,
        arpeggio: [None; STEPS],
        arpeggio_roots: std::array::from_fn(|_| fastrand::u8(0..7)),
        bass: std::array::from_fn(|i| {
            let step = i % PAGE_STEPS;
            (BASS_PATTERNS[rhythm][step] || (step == 5 && high)).then_some(0.0)
        }),
        drums: [None; STEPS],
        melody_style: if fastrand::bool() {
            MelodyStyle::Pluck
        } else {
            MelodyStyle::Sustain
        },
    };
    set_arpeggios(&mut pattern);
    set_bass(&mut pattern);
    set_drums(&mut pattern);
    pattern
}

pub fn random_track(pattern: &mut Pattern, track: usize) {
    // Retry if the selected track happens to match its old pattern.
    loop {
        let next = random_pattern();
        match track {
            0 => {
                let melody = random_melody(pattern.tonic, pattern.mode, &mut fastrand::Rng::new());
                if melody == pattern.melody && next.melody_style == pattern.melody_style {
                    continue;
                }
                pattern.melody = melody;
                pattern.melody_style = next.melody_style;
            }
            1 => {
                let old = pattern.arpeggio;
                pattern.arpeggio_roots = next.arpeggio_roots;
                set_arpeggios(pattern);
                set_bass(pattern);
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

                let old = pattern.drums;
                while pattern.drums == old {
                    set_drums(pattern);
                }
            }
            3 => {
                if pattern.bass.iter().all(Option::is_some) {
                    return;
                }
                let old = pattern.drums;
                set_drums(pattern);
                if pattern.drums == old {
                    continue;
                }
            }
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

        for (step, note) in p.melody.iter().zip(MELODY) {
            assert_eq!(step.is_some(), note != 0);
            assert!(step.is_none_or(|n| n.voice == MelodyVoice::M1));
        }

        for i in PAGE_STEPS..STEPS {
            assert_eq!(
                p.arpeggio[i].is_some(),
                p.arpeggio[i % PAGE_STEPS].is_some()
            );
            assert_eq!(p.bass[i].is_some(), p.bass[i % PAGE_STEPS].is_some());
            assert_eq!(p.drums[i], p.drums[i % PAGE_STEPS]);
        }
    }

    #[test]
    fn default_melody_plays_ready_then_theme() {
        let pattern = Pattern::default();
        assert!((pattern.melody[0].unwrap().freq - 1046.50).abs() < 0.01); // C6
        assert!((pattern.melody[3].unwrap().freq - 987.77).abs() < 0.01); // B5
        assert!((pattern.melody[5].unwrap().freq - 932.33).abs() < 0.01); // Bb5
        assert!((pattern.melody[41].unwrap().freq - 1046.50).abs() < 0.01); // C6
        assert!(pattern.melody[42..47].iter().all(Option::is_none));
        assert!((pattern.melody[47].unwrap().freq - 349.23).abs() < 0.01); // F4
        assert!(matches!(pattern.melody_style, MelodyStyle::Sustain));
    }

    #[test]
    fn default_chords_follow_melody() {
        let pattern = Pattern::default();
        let roots = [261.63, 466.16, 392.0, 311.13, 311.13, 261.63, 293.66, 392.0];
        for (page, root) in roots.into_iter().enumerate() {
            let start = page * PAGE_STEPS;
            assert!((pattern.arpeggio[start].unwrap() - root).abs() < 0.01);
            assert_eq!(
                pattern.bass[start],
                Some(pattern.arpeggio[start].unwrap() / 4.0)
            );
        }
        assert!((pattern.arpeggio[PAGE_STEPS + 2].unwrap() - 698.46).abs() < 0.01); // F5
        assert!((pattern.arpeggio[3 * PAGE_STEPS + 2].unwrap() - 466.16).abs() < 0.01); // Bb4
    }

    #[test]
    fn default_pattern_keeps_drums_off_bass() {
        let p = Pattern::default();
        assert_eq!(p.drums[0], None);
        assert_eq!(p.drums[4], Some(Drum::Kick));
        assert_eq!(p.drums[6], Some(Drum::ClosedHat));
        assert_eq!(p.bass[0], p.arpeggio[0].map(|freq| freq / 4.0));
        assert_eq!(p.bass[5], None);
        assert_eq!(p.bass[7], None);
    }

    #[test]
    fn default_pattern_snare_after_kick() {
        let p = Pattern::default();
        assert_eq!(p.drums[5], Some(Drum::Snare));
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
            if track == 3 {
                assert_eq!(pattern.bass, before.bass);
            } else if track == 1 || track == 2 {
                assert_ne!(pattern.bass, before.bass);
            }
            if track == 0 || track == 1 {
                for step in 0..STEPS {
                    assert_eq!(pattern.bass[step].is_some(), before.bass[step].is_some());
                }
            } else if track == 2 {
                let rhythm: [bool; PAGE_STEPS] =
                    std::array::from_fn(|step| pattern.bass[step].is_some());
                for page in 1..STEPS / PAGE_STEPS {
                    let start = page * PAGE_STEPS;
                    assert_eq!(
                        std::array::from_fn(|step| pattern.bass[start + step].is_some()),
                        rhythm
                    );
                }
            }
            assert_eq!(pattern.drums == before.drums, track != 2 && track != 3);
            for step in 0..STEPS {
                assert!(pattern.bass[step].is_none() || pattern.drums[step].is_none());
                assert_eq!(pattern.drums[step], pattern.drums[step % PAGE_STEPS]);
            }
            assert_eq!(pattern.drums.iter().flatten().next(), Some(&Drum::Kick));
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
                assert_eq!(pattern.bass, expected.bass);
            }
        }
    }

    #[test]
    fn drum_randomizer_changes_sounds_and_steps() {
        let mut pattern = Pattern::default();
        let mut sounds = [false; 4];
        let mut empty = false;
        let mut filled = false;

        for _ in 0..100 {
            random_track(&mut pattern, 3);
            assert_eq!(pattern.drums.iter().flatten().next(), Some(&Drum::Kick));
            for step in 0..STEPS {
                assert!(pattern.bass[step].is_none() || pattern.drums[step].is_none());
                assert_eq!(pattern.drums[step], pattern.drums[step % PAGE_STEPS]);
                match pattern.drums[step] {
                    Some(Drum::Kick) => sounds[0] = true,
                    Some(Drum::Snare) => sounds[1] = true,
                    Some(Drum::ClosedHat) => sounds[2] = true,
                    Some(Drum::OpenHat) => sounds[3] = true,
                    None if pattern.bass[step].is_none() => empty = true,
                    None => {}
                }
                filled |= pattern.drums[step].is_some();
            }
        }
        assert!(sounds.into_iter().all(|seen| seen));
        assert!(empty && filled);
    }

    #[test]
    fn bass_randomizer_regenerates_drums() {
        let mut pattern = Pattern::default();
        for _ in 0..20 {
            let bass = pattern.bass;
            let drums = pattern.drums;
            random_track(&mut pattern, 2);
            assert_ne!(pattern.bass, bass);
            assert_ne!(pattern.drums, drums);
            assert_eq!(pattern.drums.iter().flatten().next(), Some(&Drum::Kick));
            for step in 0..STEPS {
                assert!(pattern.bass[step].is_none() || pattern.drums[step].is_none());
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
    fn bass_uses_one_rhythm_all_pages() {
        for mode in [Mode::Major, Mode::Minor] {
            for (rhythm, high) in BASS_PATTERNS
                .into_iter()
                .flat_map(|rhythm| [false, true].map(move |high| (rhythm, high)))
            {
                let mut pattern = Pattern {
                    tonic: 2,
                    mode,
                    arpeggio_roots: [0, 1, 2, 3, 4, 5, 6, 0],
                    ..Pattern::default()
                };
                set_arpeggios(&mut pattern);
                for page in 0..STEPS / PAGE_STEPS {
                    for (step, &hit) in rhythm.iter().enumerate() {
                        pattern.bass[page * PAGE_STEPS + step] =
                            (hit || (step == 5 && high)).then_some(0.0);
                    }
                }
                set_bass(&mut pattern);

                for page in 0..STEPS / PAGE_STEPS {
                    let start = page * PAGE_STEPS;
                    let arp_root = pattern.arpeggio[start].unwrap();
                    let root = arp_root / 4.0;
                    for (step, &hit) in rhythm.iter().enumerate() {
                        let expected = if step == 5 && high {
                            Some(arp_root / 2.0)
                        } else {
                            hit.then_some(root)
                        };
                        assert_eq!(pattern.bass[start + step], expected);
                    }
                }
            }
        }
    }

    #[test]
    fn melody_repeats_with_small_changes() {
        for seed in 0..100 {
            let melody = random_melody(0, Mode::Minor, &mut fastrand::Rng::with_seed(seed));
            let notes = scale(0, Mode::Minor);
            let bar = &melody[..PAGE_STEPS];
            assert!((4..=6).contains(&bar.iter().flatten().count()));

            // Every measure keeps the same pauses; only the planned notes change.
            for page in 1..8 {
                let start = page * PAGE_STEPS;
                assert_eq!(
                    std::array::from_fn::<_, PAGE_STEPS, _>(|i| melody[start + i].is_some()),
                    std::array::from_fn(|i| bar[i].is_some())
                );
            }
            assert_eq!(&melody[PAGE_STEPS..2 * PAGE_STEPS], bar);
            assert_eq!(&melody[4 * PAGE_STEPS..5 * PAGE_STEPS], bar);
            assert_eq!(&melody[5 * PAGE_STEPS..6 * PAGE_STEPS], bar);

            for page in [2, 3, 6, 7] {
                let differences = (0..PAGE_STEPS)
                    .filter(|&i| melody[page * PAGE_STEPS + i] != bar[i])
                    .count();
                assert_eq!(differences, 1);
            }
            assert_eq!(melody[STEPS - 1].unwrap().freq, notes[0]);
            let first = notes
                .iter()
                .position(|&n| n == bar[0].unwrap().freq)
                .unwrap();
            assert!(first <= 2);

            for note in melody.iter().flatten() {
                assert!(notes[..4].contains(&note.freq));
            }
            for page in 0..8 {
                let pitches: Vec<_> = melody[page * PAGE_STEPS..(page + 1) * PAGE_STEPS]
                    .iter()
                    .flatten()
                    .map(|n| notes.iter().position(|&freq| freq == n.freq).unwrap())
                    .collect();
                assert!(pitches.windows(2).all(|w| w[0].abs_diff(w[1]) <= 2));
            }
        }
    }

    #[test]
    fn random_pattern_has_64_steps() {
        let p = random_pattern();
        assert_eq!(p.drums.len(), STEPS);
        assert_eq!(p.melody.len(), STEPS);
        assert_eq!(p.arpeggio.len(), STEPS);
        assert_eq!(p.bass.len(), STEPS);
        assert_eq!(p.drums.iter().flatten().next(), Some(&Drum::Kick));
        for step in 0..STEPS {
            assert!(p.bass[step].is_none() || p.drums[step].is_none());
            assert_eq!(p.drums[step], p.drums[step % PAGE_STEPS]);
        }

        let notes = scale(p.tonic, p.mode);
        for note in p.melody.iter().flatten() {
            assert!(notes.contains(&note.freq));
        }
        let rhythm: [bool; PAGE_STEPS] = std::array::from_fn(|step| p.bass[step].is_some());
        let mut base = rhythm;
        base[5] = false;
        assert!(BASS_PATTERNS.contains(&base));
        for page in 1..STEPS / PAGE_STEPS {
            let start = page * PAGE_STEPS;
            assert_eq!(
                std::array::from_fn(|step| p.bass[start + step].is_some()),
                rhythm
            );
        }
        for step in 0..STEPS {
            let page = step / PAGE_STEPS;
            let chord = chord_at(p.tonic, p.mode, p.arpeggio_roots[page]);
            assert_eq!(
                p.arpeggio[step],
                ARP_ORDER.get(step % PAGE_STEPS).map(|&n| chord[n])
            );
            let root = p.arpeggio[page * PAGE_STEPS].unwrap() / 4.0;
            if step % PAGE_STEPS == 5 {
                assert_eq!(p.bass[step], rhythm[5].then_some(chord[0] / 2.0));
            } else {
                assert_eq!(p.bass[step], rhythm[step % PAGE_STEPS].then_some(root));
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
