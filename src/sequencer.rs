use std::sync::{Arc, Mutex};

use crate::arrange::{HarmonyPlan, legacy_root, render_arpeggio};
use crate::music::MelodyInfo;
pub use crate::music::ToneStep;
use crate::music::{ARP_GATE, BASS_GATE, PLUCK_GATE, SHORT_GATE, SUSTAIN_GATE, freq_pitch};

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
    pub pitch: u8,
    pub voice: MelodyVoice,
    pub duration: u16,
    pub bend: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drum {
    Kick,
    Snare,
    ClosedHat,
    OpenHat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hat {
    Closed,
    Open,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrumStep {
    pub kick: bool,
    pub snare: bool,
    pub hat: Option<Hat>,
}

impl DrumStep {
    pub fn primary(self) -> Option<Drum> {
        if self.kick {
            Some(Drum::Kick)
        } else if self.snare {
            Some(Drum::Snare)
        } else {
            self.hat.map(|hat| match hat {
                Hat::Closed => Drum::ClosedHat,
                Hat::Open => Drum::OpenHat,
            })
        }
    }
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
    // Full-pattern generation is currently an offline entry point.
    #[cfg_attr(not(test), expect(dead_code))]
    Minor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArpShape {
    Classic,
    Up,
    Down,
    Repeat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seeds {
    pub melody: u64,
    pub harmony: u64,
    pub arpeggio: u64,
    pub groove: u64,
    pub bass: u64,
}

impl Seeds {
    fn from_rng(rng: &mut fastrand::Rng) -> Self {
        // Keep this order fixed so each stage has an independent stream.
        Self {
            melody: rng.u64(..),
            harmony: rng.u64(..),
            arpeggio: rng.u64(..),
            groove: rng.u64(..),
            bass: rng.u64(..),
        }
    }
}

#[derive(Clone)]
pub struct Pattern {
    pub seeds: Seeds,
    pub tonic: u8,
    pub mode: Mode,
    pub melody: [Option<MelodyStep>; STEPS],
    pub arpeggio: [Option<ToneStep>; STEPS],
    pub harmony: HarmonyPlan,
    pub(crate) arpeggio_shape: ArpShape,
    pub(crate) arpeggio_sevenths: [bool; STEPS / PAGE_STEPS],
    pub bass: [Option<ToneStep>; STEPS],
    pub drums: [DrumStep; STEPS],
    pub melody_style: MelodyStyle,
}

// Ready cue (0:12–0:17) followed by the opening of the next theme.
// MIDI notes; zero marks a held note or a rest.
const MELODY: [u8; STEPS] = [
    84, 0, 0, 83, 0, 82, 83, 0, 0, 0, 0, 81, 0, 0, 0, 79, 0, 78, 0, 79, 0, 0, 0, 81, 0, 78, 0, 79,
    0, 78, 79, 0, 78, 79, 0, 74, 77, 0, 79, 81, 83, 84, 0, 0, 0, 0, 0, 65, 0, 0, 62, 0, 57, 0, 62,
    64, 65, 0, 67, 0, 62, 65, 60, 64,
];
// C, Bb, G, Eb, Eb, C, Dm, G: follow the cue, then the next theme.

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

fn seventh_at(tonic: u8, mode: Mode, root: u8) -> f32 {
    let degree = root as usize + 6;
    scale(tonic, mode)[degree % 7] * if degree >= 7 { 2.0 } else { 1.0 }
}

fn harmony_roots(pattern: &Pattern, rng: &mut fastrand::Rng) -> HarmonyPlan {
    crate::arrange::choose_harmony(
        &MelodyInfo::new(&pattern.melody),
        pattern.tonic,
        pattern.mode,
        rng,
    )
}

fn page_chord(pattern: &Pattern, page: usize) -> [f32; 4] {
    match legacy_root(pattern, page) {
        // Borrow Bb and Eb major to follow the cue's chromatic notes.
        7 | 8 => {
            let note = if legacy_root(pattern, page) == 7 {
                70
            } else {
                63
            };
            let root = 440.0 * 2.0_f32.powf((note as f32 + pattern.tonic as f32 - 69.0) / 12.0);
            [
                root,
                root * 2.0_f32.powf(4.0 / 12.0),
                root * 2.0_f32.powf(7.0 / 12.0),
                root * 2.0_f32.powf(11.0 / 12.0),
            ]
        }
        root => {
            let triad = chord_at(pattern.tonic, pattern.mode, root);
            [
                triad[0],
                triad[1],
                triad[2],
                seventh_at(pattern.tonic, pattern.mode, root),
            ]
        }
    }
}

fn arp_notes(chord: [f32; 4], shape: ArpShape, seventh: bool) -> [f32; 6] {
    let positions = match (shape, seventh) {
        (ArpShape::Up, true) => [(0, 0), (1, 0), (2, 0), (3, 0), (0, 1), (1, 1)],
        (ArpShape::Down, true) => [(3, 0), (2, 0), (1, 0), (0, 0), (3, -1), (2, -1)],
        (ArpShape::Up, false) => [(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1)],
        (ArpShape::Down, false) => [(2, 0), (1, 0), (0, 0), (2, -1), (1, -1), (0, -1)],
        (ArpShape::Repeat, false) => [(0, 0), (2, 0), (1, 0), (2, 0), (0, 0), (2, 0)],
        (ArpShape::Classic, false) => [(0, 0), (1, 0), (2, 0), (2, 0), (1, 0), (0, 0)],
        _ => unreachable!("seventh chords require an ascending or descending shape"),
    };
    positions.map(|(note, octave)| chord[note] * 2.0_f32.powi(octave))
}

#[cfg(test)]
fn random_arp_shape(pattern: &mut Pattern, rng: &mut fastrand::Rng) {
    pattern.arpeggio_shape = match rng.usize(0..4) {
        0 => ArpShape::Classic,
        1 => ArpShape::Up,
        2 => ArpShape::Down,
        _ => ArpShape::Repeat,
    };

    // Generated harmony scores triads only; do not add unscored sevenths.
    pattern.arpeggio_sevenths.fill(false);
}

fn set_arpeggios(pattern: &mut Pattern) {
    for page in 0..STEPS / PAGE_STEPS {
        let notes = arp_notes(
            page_chord(pattern, page),
            pattern.arpeggio_shape,
            pattern.arpeggio_sevenths[page],
        );
        let start = page * PAGE_STEPS;
        for (step, freq) in notes.into_iter().enumerate() {
            pattern.arpeggio[start + step] = Some(ToneStep::new(freq_pitch(freq), ARP_GATE));
        }
        pattern.arpeggio[start + 6..start + PAGE_STEPS].fill(None);
    }
}

#[derive(Clone, Copy)]
enum Groove {
    Backbeat,
    FourFloor,
    HalfTime,
    Syncopated,
}

fn groove_steps(groove: Groove) -> [DrumStep; PAGE_STEPS] {
    std::array::from_fn(|step| {
        let kick = match groove {
            Groove::Backbeat => matches!(step, 0 | 4),
            Groove::FourFloor => step % 2 == 0,
            Groove::HalfTime => matches!(step, 0 | 5),
            Groove::Syncopated => matches!(step, 0 | 3 | 4),
        };
        let snare = match groove {
            Groove::HalfTime => step == 4,
            _ => matches!(step, 2 | 6),
        };
        let hat = if matches!(groove, Groove::FourFloor) && step % 2 == 0 {
            None
        } else if step == 7 && matches!(groove, Groove::HalfTime | Groove::Syncopated) {
            Some(Hat::Open)
        } else {
            Some(Hat::Closed)
        };
        DrumStep { kick, snare, hat }
    })
}

fn set_drums(pattern: &mut Pattern, rng: &mut fastrand::Rng) {
    // Pick one groove and repeat its anchors across all eight bars.
    let groove = match rng.usize(0..4) {
        0 => Groove::Backbeat,
        1 => Groove::FourFloor,
        2 => Groove::HalfTime,
        _ => Groove::Syncopated,
    };
    let bar = groove_steps(groove);
    pattern.drums = std::array::from_fn(|step| bar[step % PAGE_STEPS]);
}

fn set_bass(pattern: &mut Pattern) {
    for page in 0..STEPS / PAGE_STEPS {
        let root = Some(ToneStep::new(
            freq_pitch(page_chord(pattern, page)[0] / 4.0),
            BASS_GATE,
        ));
        for step in 0..PAGE_STEPS {
            let index = page * PAGE_STEPS + step;
            // Preserve the chosen rhythm while following this page's chord root.
            pattern.bass[index] = match step {
                0..=3 if pattern.bass[index].is_some() => root,
                5 if pattern.bass[index].is_some() => {
                    root.map(|note| ToneStep::new(note.pitch + 12, note.duration))
                }
                _ => None,
            };
        }
    }
}

impl Default for Pattern {
    fn default() -> Self {
        let bar = groove_steps(Groove::Backbeat);
        let mut pattern = Self {
            seeds: Seeds::from_rng(&mut fastrand::Rng::with_seed(0)),
            tonic: 0,
            mode: Mode::Major,
            melody: MELODY.map(|note| {
                (note != 0).then_some(MelodyStep {
                    pitch: note,
                    voice: MelodyVoice::M1,
                    duration: SUSTAIN_GATE,
                    bend: false,
                })
            }),
            arpeggio: [None; STEPS],
            harmony: HarmonyPlan::preset(),
            arpeggio_shape: ArpShape::Classic,
            arpeggio_sevenths: [false; STEPS / PAGE_STEPS],
            bass: std::array::from_fn(|i| {
                BASS_PATTERNS[2][i % PAGE_STEPS].then_some(ToneStep::new(0, BASS_GATE))
            }),
            drums: std::array::from_fn(|i| bar[i % PAGE_STEPS]),
            melody_style: MelodyStyle::Sustain,
        };
        set_arpeggios(&mut pattern);
        set_bass(&mut pattern);
        pattern
    }
}

fn random_melody(
    tonic: u8,
    mode: Mode,
    style: MelodyStyle,
    rng: &mut fastrand::Rng,
) -> [Option<MelodyStep>; STEPS] {
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

    let mut short: [bool; PAGE_STEPS] = std::array::from_fn(|_| rng.bool());
    short[0] = false;
    short[PAGE_STEPS - 1] = true;
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
                pitch: freq_pitch(notes[degree]),
                voice,
                duration: if short[i] {
                    SHORT_GATE
                } else if voice == MelodyVoice::M1 && style == MelodyStyle::Pluck {
                    PLUCK_GATE
                } else {
                    SUSTAIN_GATE
                },
                bend: page == 7 && i == PAGE_STEPS - 1,
            });
        }
    }
    melody
}

fn set_melody_gates(melody: &mut [Option<MelodyStep>; STEPS], style: MelodyStyle) {
    // Preserve the legacy pluck gate without tying it to the playback tempo.
    if style == MelodyStyle::Pluck {
        for note in melody.iter_mut().flatten() {
            if note.voice == MelodyVoice::M1 && note.duration == SUSTAIN_GATE {
                note.duration = PLUCK_GATE;
            }
        }
    }
}

fn random_style(rng: &mut fastrand::Rng) -> MelodyStyle {
    if rng.bool() {
        MelodyStyle::Pluck
    } else {
        MelodyStyle::Sustain
    }
}

fn random_bass(pattern: &mut Pattern, rng: &mut fastrand::Rng) {
    // Choose only the bass rhythm; pitches come from the current harmony.
    let rhythm = rng.usize(0..BASS_PATTERNS.len());
    let high = rng.bool();
    pattern.bass = std::array::from_fn(|i| {
        let step = i % PAGE_STEPS;
        (BASS_PATTERNS[rhythm][step] || (step == 5 && high)).then_some(ToneStep::new(0, BASS_GATE))
    });
    set_bass(pattern);
}

// Retain full-pattern entry points for tests and offline rendering.
#[cfg_attr(not(test), expect(dead_code))]
pub fn random_pattern() -> Pattern {
    seeded_pattern(fastrand::u64(..))
}

#[cfg_attr(not(test), expect(dead_code))]
pub fn seeded_pattern(seed: u64) -> Pattern {
    let mut root = fastrand::Rng::with_seed(seed);
    let seeds = Seeds::from_rng(&mut root);
    let tonic = root.u8(0..12);
    let mode = if root.bool() {
        Mode::Major
    } else {
        Mode::Minor
    };
    let mut melody_rng = fastrand::Rng::with_seed(seeds.melody);
    let mut melody = random_melody(tonic, mode, MelodyStyle::Sustain, &mut melody_rng);
    let style = random_style(&mut melody_rng);
    set_melody_gates(&mut melody, style);

    // Isolate stage draws so changes in accompaniment cannot change the lead.
    let mut pattern = Pattern {
        seeds,
        tonic,
        mode,
        melody,
        arpeggio: [None; STEPS],
        harmony: HarmonyPlan::from_degrees(tonic, mode, [0; STEPS / PAGE_STEPS]),
        arpeggio_shape: ArpShape::Classic,
        arpeggio_sevenths: [false; STEPS / PAGE_STEPS],
        bass: [None; STEPS],
        drums: [DrumStep::default(); STEPS],
        melody_style: style,
    };
    pattern.harmony = harmony_roots(&pattern, &mut fastrand::Rng::with_seed(seeds.harmony));
    render_arpeggio(
        &mut pattern,
        &MelodyInfo::new(&melody),
        &mut fastrand::Rng::with_seed(seeds.arpeggio),
    );
    random_bass(&mut pattern, &mut fastrand::Rng::with_seed(seeds.bass));
    set_drums(&mut pattern, &mut fastrand::Rng::with_seed(seeds.groove));
    pattern
}

pub fn random_track(pattern: &mut Pattern, track: usize) {
    seeded_track(pattern, track, fastrand::u64(..));
}

pub fn seeded_track(pattern: &mut Pattern, track: usize, seed: u64) {
    let mut root = fastrand::Rng::with_seed(seed);

    // Retry only the requested stage, preserving existing track dependencies.
    loop {
        let seed = root.u64(..);
        let mut rng = fastrand::Rng::with_seed(seed);
        match track {
            0 => {
                let mut melody =
                    random_melody(pattern.tonic, pattern.mode, MelodyStyle::Sustain, &mut rng);
                let style = random_style(&mut rng);
                set_melody_gates(&mut melody, style);
                if melody == pattern.melody && style == pattern.melody_style {
                    continue;
                }
                pattern.seeds.melody = seed;
                pattern.melody = melody;
                pattern.melody_style = style;
                pattern.harmony = harmony_roots(
                    pattern,
                    &mut fastrand::Rng::with_seed(pattern.seeds.harmony),
                );
                render_arpeggio(
                    pattern,
                    &MelodyInfo::new(&pattern.melody),
                    &mut fastrand::Rng::with_seed(pattern.seeds.arpeggio),
                );
                set_bass(pattern);
            }
            1 => {
                let old = pattern.arpeggio;
                pattern.seeds.harmony = seed;
                pattern.seeds.arpeggio = root.u64(..);
                pattern.harmony = harmony_roots(pattern, &mut rng);
                render_arpeggio(
                    pattern,
                    &MelodyInfo::new(&pattern.melody),
                    &mut fastrand::Rng::with_seed(pattern.seeds.arpeggio),
                );
                if pattern.arpeggio == old {
                    continue;
                }
                set_bass(pattern);
            }
            2 => {
                let old = pattern.bass;
                random_bass(pattern, &mut rng);
                if pattern.bass == old {
                    continue;
                }
                pattern.seeds.bass = seed;
            }
            3 => {
                let old = pattern.drums;
                set_drums(pattern, &mut rng);
                if pattern.drums == old {
                    continue;
                }
                pattern.seeds.groove = seed;
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
    use crate::arrange::{degree_chord, melody_score};
    use crate::music::pitch_freq;

    fn assert_same(a: &Pattern, b: &Pattern) {
        assert_eq!(a.seeds, b.seeds);
        assert_eq!(a.tonic, b.tonic);
        assert_eq!(a.mode, b.mode);
        assert_eq!(a.melody, b.melody);
        assert!(a.melody_style == b.melody_style);
        assert_eq!(a.arpeggio, b.arpeggio);
        assert_eq!(a.harmony, b.harmony);
        assert_eq!(a.arpeggio_shape, b.arpeggio_shape);
        assert_eq!(a.arpeggio_sevenths, b.arpeggio_sevenths);
        assert_eq!(a.bass, b.bass);
        assert_eq!(a.drums, b.drums);
    }

    #[test]
    fn seeded_generation_repeats() {
        for seed in 0..16 {
            assert_same(&seeded_pattern(seed), &seeded_pattern(seed));
        }
    }

    #[test]
    fn seeded_rerolls_repeat() {
        for track in 0..TRACKS {
            let mut a = seeded_pattern(42);
            let mut b = a.clone();
            seeded_track(&mut a, track, 123);
            seeded_track(&mut b, track, 123);
            assert_same(&a, &b);

            // Only seeds belonging to the requested stage may change.
            let old = seeded_pattern(42).seeds;
            if track != 0 {
                assert_eq!(a.seeds.melody, old.melody);
            }
            if track != 1 {
                assert_eq!(a.seeds.harmony, old.harmony);
                assert_eq!(a.seeds.arpeggio, old.arpeggio);
            }
            if track != 2 {
                assert_eq!(a.seeds.bass, old.bass);
            }
            if track != 3 {
                assert_eq!(a.seeds.groove, old.groove);
            }
        }
    }

    #[test]
    fn stage_streams_are_independent() {
        let original = seeded_pattern(42);
        let mut altered = original.clone();
        let mut arp = fastrand::Rng::with_seed(original.seeds.arpeggio);
        for _ in 0..100 {
            arp.u64(..);
        }
        random_arp_shape(&mut altered, &mut arp);
        set_arpeggios(&mut altered);

        // Rebuild unrelated stages after extra arpeggio draws.
        let mut melody = fastrand::Rng::with_seed(original.seeds.melody);
        altered.melody = random_melody(
            altered.tonic,
            altered.mode,
            MelodyStyle::Sustain,
            &mut melody,
        );
        altered.melody_style = random_style(&mut melody);
        set_melody_gates(&mut altered.melody, altered.melody_style);
        set_drums(
            &mut altered,
            &mut fastrand::Rng::with_seed(original.seeds.groove),
        );
        assert_eq!(altered.melody, original.melody);
        assert!(altered.melody_style == original.melody_style);
        assert_eq!(altered.drums, original.drums);
    }

    #[test]
    fn seeded_baseline_fixtures() {
        // Event summaries preserve the seeded baseline for later listening comparisons.
        let fixtures = [
            (
                0,
                11,
                [73, 0, 71, 71, 0, 73, 0, 75],
                0b00001001,
                [0, 4, 0, 1, 4, 4, 0, 0],
                Groove::HalfTime,
            ),
            (
                42,
                3,
                [67, 63, 65, 67, 67, 0, 0, 67],
                0b00001011,
                [0, 0, 0, 0, 0, 0, 0, 0],
                Groove::HalfTime,
            ),
            (
                1023,
                11,
                [75, 0, 73, 0, 71, 0, 73, 75],
                0b00001101,
                [0, 0, 0, 5, 5, 5, 5, 0],
                Groove::Syncopated,
            ),
        ];
        for (seed, tonic, pitches, bass_mask, _baseline_roots, groove) in fixtures {
            let p = seeded_pattern(seed);
            assert_eq!(p.tonic, tonic, "seed {seed}");
            assert_eq!(p.mode, Mode::Major, "seed {seed}");
            assert_eq!(&p.drums[..PAGE_STEPS], &groove_steps(groove), "seed {seed}");
            for (step, pitch) in pitches.into_iter().enumerate() {
                let actual = p.melody[step].map(|n| n.pitch).unwrap_or(0);
                assert_eq!(actual, pitch, "seed {seed}, step {step}");
                assert_eq!(
                    p.bass[step].is_some(),
                    bass_mask & (1 << step) != 0,
                    "seed {seed}, step {step}"
                );
            }
        }
    }

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
        assert_eq!(pattern.melody[0].unwrap().pitch, 84);
        assert_eq!(pattern.melody[3].unwrap().pitch, 83);
        assert_eq!(pattern.melody[5].unwrap().pitch, 82);
        assert_eq!(pattern.melody[41].unwrap().pitch, 84);
        assert!(pattern.melody[42..47].iter().all(Option::is_none));
        assert_eq!(pattern.melody[47].unwrap().pitch, 65);
        assert!(matches!(pattern.melody_style, MelodyStyle::Sustain));
    }

    #[test]
    fn default_chords_follow_melody() {
        let pattern = Pattern::default();
        let roots = [261.63, 466.16, 392.0, 311.13, 311.13, 261.63, 293.66, 392.0];
        for (page, root) in roots.into_iter().enumerate() {
            let start = page * PAGE_STEPS;
            assert!((pitch_freq(pattern.arpeggio[start].unwrap().pitch) - root).abs() < 0.01);
            assert_eq!(
                pattern.bass[start],
                Some(ToneStep::new(
                    pattern.arpeggio[start].unwrap().pitch - 24,
                    BASS_GATE
                ))
            );
        }
        assert_eq!(pattern.arpeggio[PAGE_STEPS + 2].unwrap().pitch, 77);
        assert_eq!(pattern.arpeggio[3 * PAGE_STEPS + 2].unwrap().pitch, 70);
    }

    #[test]
    fn default_pattern_has_backbeat_over_bass() {
        let p = Pattern::default();
        for page in 0..STEPS / PAGE_STEPS {
            let start = page * PAGE_STEPS;
            assert!(p.drums[start].kick && p.drums[start + 4].kick);
            assert!(p.drums[start + 2].snare && p.drums[start + 6].snare);
            assert!(
                p.drums[start..start + PAGE_STEPS]
                    .iter()
                    .all(|step| step.hat == Some(Hat::Closed))
            );
        }
        assert!(p.bass[0].is_some() && p.drums[0].kick);
        assert_eq!(p.drums[0].primary(), Some(Drum::Kick));
        assert_eq!(p.drums[2].primary(), Some(Drum::Snare));
        assert_eq!(p.drums[1].primary(), Some(Drum::ClosedHat));
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
            s.pattern.drums[2].kick = true;
        }
        let s = state.lock().unwrap();
        assert_eq!(s.bpm, 140.0);
        assert!(s.pattern.drums[2].kick);
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
            } else if track == 2 {
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
            assert_eq!(pattern.drums == before.drums, track != 3);
            for step in 0..STEPS {
                assert_eq!(pattern.drums[step], pattern.drums[step % PAGE_STEPS]);
            }
            assert!(pattern.drums[0].kick);
            if track != 0 {
                assert_eq!((pattern.tonic, pattern.mode), (before.tonic, before.mode));
            }
            if track >= 2 {
                assert_eq!(pattern.harmony, before.harmony);
            } else if track == 1 {
                assert_ne!(pattern.arpeggio, before.arpeggio);
            }
            if track >= 2 {
                assert_eq!(pattern.arpeggio_shape, before.arpeggio_shape);
            }
            if track >= 2 {
                assert_eq!(pattern.arpeggio_sevenths, before.arpeggio_sevenths);
            }
            if track <= 2 {
                let mut expected = pattern.clone();
                set_bass(&mut expected);
                if track == 0 || track == 1 {
                    render_arpeggio(
                        &mut expected,
                        &MelodyInfo::new(&pattern.melody),
                        &mut fastrand::Rng::with_seed(pattern.seeds.arpeggio),
                    );
                    assert_eq!(pattern.arpeggio, expected.arpeggio);
                }
                assert_eq!(pattern.bass, expected.bass);
            }
        }
    }

    #[test]
    fn melody_reroll_updates_harmony_and_bass() {
        let mut pattern = Pattern::default();
        let old_melody = pattern.melody;
        let old_drums = pattern.drums;
        let old_rhythm = pattern.bass.map(|note| note.is_some());
        random_track(&mut pattern, 0);

        assert_ne!(pattern.melody, old_melody);
        assert_eq!(pattern.drums, old_drums);
        assert_eq!(pattern.bass.map(|note| note.is_some()), old_rhythm);
        assert!(pattern.harmony.chords.iter().all(|c| c.root < 12));
        for page in 0..STEPS / PAGE_STEPS {
            let start = page * PAGE_STEPS;
            let chord = chord_at(pattern.tonic, pattern.mode, legacy_root(&pattern, page));
            for note in pattern.arpeggio[start..start + PAGE_STEPS].iter().flatten() {
                assert!(
                    pattern.harmony.chords[page]
                        .classes()
                        .contains(&(note.pitch % 12))
                );
            }
            assert_eq!(
                pattern.bass[start],
                Some(ToneStep::new(freq_pitch(chord[0] / 4.0), BASS_GATE))
            );
        }
    }

    #[test]
    fn arpeggio_reroll_keeps_melody_and_changes_chords() {
        let mut pattern = random_pattern();
        let melody = pattern.melody;
        let rhythm = pattern.bass.map(|note| note.is_some());
        let drums = pattern.drums;
        for _ in 0..20 {
            let old = pattern.arpeggio;
            random_track(&mut pattern, 1);
            assert_ne!(pattern.arpeggio, old);
            assert_eq!(pattern.melody, melody);
            assert_eq!(pattern.drums, drums);
            assert_eq!(pattern.bass.map(|note| note.is_some()), rhythm);
            for page in 0..STEPS / PAGE_STEPS {
                let start = page * PAGE_STEPS;
                assert_eq!(
                    pattern.bass[start],
                    Some(ToneStep::new(
                        freq_pitch(page_chord(&pattern, page)[0] / 4.0),
                        BASS_GATE
                    ))
                );
            }
        }
    }

    #[test]
    fn drum_randomizer_uses_repeated_grooves() {
        let mut pattern = Pattern::default();
        let mut seen = [false; 4];
        for _ in 0..100 {
            let old = pattern.drums;
            random_track(&mut pattern, 3);
            assert_ne!(pattern.drums, old);
            let bar = &pattern.drums[..PAGE_STEPS];
            assert!(bar[0].kick);
            assert!([2, 4, 6].iter().any(|&step| bar[step].snare));
            for page in 1..STEPS / PAGE_STEPS {
                assert_eq!(
                    &pattern.drums[page * PAGE_STEPS..(page + 1) * PAGE_STEPS],
                    bar
                );
            }
            for (index, groove) in [
                Groove::Backbeat,
                Groove::FourFloor,
                Groove::HalfTime,
                Groove::Syncopated,
            ]
            .into_iter()
            .enumerate()
            {
                seen[index] |= bar == groove_steps(groove);
            }
        }
        assert!(seen.into_iter().all(|found| found));
    }

    #[test]
    fn bass_randomizer_keeps_drums() {
        let mut pattern = Pattern::default();
        let drums = pattern.drums;
        for _ in 0..20 {
            let bass = pattern.bass;
            random_track(&mut pattern, 2);
            assert_ne!(pattern.bass, bass);
            assert_eq!(pattern.drums, drums);
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
    fn harmony_follows_prominent_notes_and_resolves() {
        let mut pattern = Pattern {
            melody: [None; STEPS],
            ..Pattern::default()
        };
        let notes = scale(0, Mode::Major);
        for (page, degrees) in [(0, [0, 2, 4]), (6, [1, 3, 5]), (7, [0, 2, 4])] {
            for (step, degree) in [0, 3, 7].into_iter().zip(degrees) {
                pattern.melody[page * PAGE_STEPS + step] = Some(MelodyStep {
                    pitch: freq_pitch(notes[degree]),
                    voice: MelodyVoice::M1,
                    duration: SUSTAIN_GATE,
                    bend: false,
                });
            }
        }

        let roots = harmony_roots(&pattern, &mut fastrand::Rng::with_seed(1));
        assert_eq!(roots.chords[0], degree_chord(0, Mode::Major, 0));
        assert_eq!(roots.chords[6], degree_chord(0, Mode::Major, 1));
        assert_eq!(roots.chords[7], degree_chord(0, Mode::Major, 0));
    }

    #[test]
    fn generated_harmony_supports_end() {
        for tonic in 0..12 {
            for mode in [Mode::Major, Mode::Minor] {
                for seed in 0..8 {
                    let mut pattern = Pattern {
                        tonic,
                        mode,
                        melody: random_melody(
                            tonic,
                            mode,
                            MelodyStyle::Sustain,
                            &mut fastrand::Rng::with_seed(seed),
                        ),
                        ..Pattern::default()
                    };
                    pattern.harmony =
                        harmony_roots(&pattern, &mut fastrand::Rng::with_seed(seed + 100));
                    assert!(
                        pattern.harmony.chords[7].root < 12,
                        "{tonic} {mode:?} {seed}"
                    );
                    set_arpeggios(&mut pattern);
                    set_bass(&mut pattern);
                    assert_eq!(
                        pattern.bass[56],
                        pattern.arpeggio[56].map(|n| ToneStep::new(n.pitch - 24, BASS_GATE))
                    );
                }
            }
        }
    }

    #[test]
    fn passing_notes_need_no_new_chord() {
        let mut pattern = Pattern {
            melody: [None; STEPS],
            ..Pattern::default()
        };
        let notes = scale(0, Mode::Major);
        for (step, degree) in [(0, 0), (2, 1), (4, 2)] {
            pattern.melody[step] = Some(MelodyStep {
                pitch: freq_pitch(notes[degree]),
                voice: MelodyVoice::M1,
                duration: if step == 2 { SHORT_GATE } else { SUSTAIN_GATE },
                bend: false,
            });
        }
        let info = MelodyInfo::new(&pattern.melody);
        assert!(
            melody_score(&info, 0, degree_chord(0, Mode::Major, 0))
                > melody_score(&info, 0, degree_chord(0, Mode::Major, 1))
        );
    }

    #[test]
    fn arpeggios_fill_six_steps_per_page() {
        for mode in [Mode::Major, Mode::Minor] {
            let mut pattern = Pattern {
                tonic: 2,
                mode,
                harmony: HarmonyPlan::from_degrees(2, mode, [0, 1, 2, 3, 4, 5, 6, 0]),
                ..Pattern::default()
            };
            set_arpeggios(&mut pattern);
            for page in 0..STEPS / PAGE_STEPS {
                let chord = chord_at(2, mode, legacy_root(&pattern, page));
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
                    .map(|n| n.map(|f| ToneStep::new(freq_pitch(f), ARP_GATE)))
                );
            }
        }
    }

    #[test]
    fn random_arpeggios_include_all_shapes() {
        let mut pattern = Pattern {
            harmony: HarmonyPlan::from_degrees(0, Mode::Major, [0, 4, 0, 1, 4, 0, 3, 0]),
            ..Pattern::default()
        };
        let mut seen = [false; 4];
        for seed in 0..100 {
            random_arp_shape(&mut pattern, &mut fastrand::Rng::with_seed(seed));
            let shape = pattern.arpeggio_shape;
            seen[match shape {
                ArpShape::Classic => 0,
                ArpShape::Up => 1,
                ArpShape::Down => 2,
                ArpShape::Repeat => 3,
            }] = true;
            for page in 0..STEPS / PAGE_STEPS {
                assert!(!pattern.arpeggio_sevenths[page]);
                assert!(
                    !pattern.arpeggio_sevenths[page]
                        || matches!(shape, ArpShape::Up | ArpShape::Down)
                );
            }
            assert!(!pattern.arpeggio_sevenths[7]);
        }
        assert!(seen.into_iter().all(|value| value));
    }

    #[test]
    fn arpeggio_shape_applies_to_every_page() {
        for shape in [
            ArpShape::Classic,
            ArpShape::Up,
            ArpShape::Down,
            ArpShape::Repeat,
        ] {
            let mut pattern = Pattern {
                arpeggio_shape: shape,
                harmony: HarmonyPlan::from_degrees(0, Mode::Major, [0, 1, 2, 3, 4, 5, 6, 0]),
                ..Pattern::default()
            };
            set_arpeggios(&mut pattern);
            for page in 0..STEPS / PAGE_STEPS {
                let start = page * PAGE_STEPS;
                assert_eq!(
                    pattern.arpeggio[start..start + 6],
                    arp_notes(page_chord(&pattern, page), shape, false)
                        .map(|f| Some(ToneStep::new(freq_pitch(f), ARP_GATE)))
                );
                assert_eq!(
                    pattern.arpeggio[start + 6..start + PAGE_STEPS],
                    [None, None]
                );
            }
        }
    }

    #[test]
    fn arpeggio_shapes_and_sevenths() {
        let chord = [261.63, 329.63, 392.0, 493.88];
        assert_eq!(
            arp_notes(chord, ArpShape::Up, false),
            [
                chord[0],
                chord[1],
                chord[2],
                chord[0] * 2.0,
                chord[1] * 2.0,
                chord[2] * 2.0
            ]
        );
        assert_eq!(
            arp_notes(chord, ArpShape::Down, false),
            [
                chord[2],
                chord[1],
                chord[0],
                chord[2] / 2.0,
                chord[1] / 2.0,
                chord[0] / 2.0
            ]
        );
        assert_eq!(
            arp_notes(chord, ArpShape::Repeat, false),
            [chord[0], chord[2], chord[1], chord[2], chord[0], chord[2]]
        );
        assert_eq!(
            arp_notes(chord, ArpShape::Up, true),
            [
                chord[0],
                chord[1],
                chord[2],
                chord[3],
                chord[0] * 2.0,
                chord[1] * 2.0
            ]
        );
        assert_eq!(
            arp_notes(chord, ArpShape::Down, true),
            [
                chord[3],
                chord[2],
                chord[1],
                chord[0],
                chord[3] / 2.0,
                chord[2] / 2.0
            ]
        );
        assert_eq!(
            seventh_at(0, Mode::Major, 4),
            scale(0, Mode::Major)[3] * 2.0
        );
    }

    #[test]
    fn descending_arpeggio_keeps_bass_on_root() {
        let mut pattern = Pattern {
            arpeggio_shape: ArpShape::Down,
            ..Pattern::default()
        };
        pattern.arpeggio_sevenths[0] = true;
        set_arpeggios(&mut pattern);
        set_bass(&mut pattern);
        assert_eq!(
            pattern.arpeggio[0],
            Some(ToneStep::new(
                freq_pitch(page_chord(&pattern, 0)[3]),
                ARP_GATE
            ))
        );
        assert_eq!(
            pattern.bass[0],
            Some(ToneStep::new(
                freq_pitch(page_chord(&pattern, 0)[0] / 4.0),
                BASS_GATE
            ))
        );
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
                    harmony: HarmonyPlan::from_degrees(2, mode, [0, 1, 2, 3, 4, 5, 6, 0]),
                    ..Pattern::default()
                };
                set_arpeggios(&mut pattern);
                for page in 0..STEPS / PAGE_STEPS {
                    for (step, &hit) in rhythm.iter().enumerate() {
                        pattern.bass[page * PAGE_STEPS + step] =
                            (hit || (step == 5 && high)).then_some(ToneStep::new(0, BASS_GATE));
                    }
                }
                set_bass(&mut pattern);

                for page in 0..STEPS / PAGE_STEPS {
                    let start = page * PAGE_STEPS;
                    let arp_root = pattern.arpeggio[start].unwrap().pitch;
                    let root = ToneStep::new(arp_root - 24, BASS_GATE);
                    for (step, &hit) in rhythm.iter().enumerate() {
                        let expected = if step == 5 && high {
                            Some(ToneStep::new(arp_root - 12, BASS_GATE))
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
            let melody = random_melody(
                0,
                Mode::Minor,
                MelodyStyle::Sustain,
                &mut fastrand::Rng::with_seed(seed),
            );
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
            assert_eq!(melody[STEPS - 1].unwrap().pitch, freq_pitch(notes[0]));
            assert_eq!(melody.iter().flatten().filter(|note| note.bend).count(), 1);
            assert!(melody[STEPS - 1].unwrap().bend);
            assert!(bar[0].is_some_and(|note| note.duration == SUSTAIN_GATE));
            assert!(bar[PAGE_STEPS - 1].is_some_and(|note| note.duration == SHORT_GATE));
            let first = notes
                .iter()
                .position(|&n| freq_pitch(n) == bar[0].unwrap().pitch)
                .unwrap();
            assert!(first <= 2);

            for note in melody.iter().flatten() {
                assert!(notes[..4].iter().any(|&f| freq_pitch(f) == note.pitch));
            }
            for page in 0..8 {
                let pitches: Vec<_> = melody[page * PAGE_STEPS..(page + 1) * PAGE_STEPS]
                    .iter()
                    .flatten()
                    .map(|n| {
                        notes
                            .iter()
                            .position(|&freq| freq_pitch(freq) == n.pitch)
                            .unwrap()
                    })
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
        assert!(p.drums[0].kick);
        for step in 0..STEPS {
            assert_eq!(p.drums[step], p.drums[step % PAGE_STEPS]);
        }

        let notes = scale(p.tonic, p.mode);
        for note in p.melody.iter().flatten() {
            assert!(notes.iter().any(|&f| freq_pitch(f) == note.pitch));
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
            let chord = chord_at(p.tonic, p.mode, legacy_root(&p, page));
            if let Some(note) = p.arpeggio[step] {
                assert!((crate::arrange::ARP_LOW..=crate::arrange::ARP_HIGH).contains(&note.pitch));
                assert!(
                    p.harmony.chords[page]
                        .classes()
                        .contains(&(note.pitch % 12))
                );
                assert_eq!(note.duration, ARP_GATE);
            }
            assert!(!p.arpeggio_sevenths[page]);
            let root = ToneStep::new(freq_pitch(page_chord(&p, page)[0] / 4.0), BASS_GATE);
            if step % PAGE_STEPS == 5 {
                assert_eq!(
                    p.bass[step],
                    rhythm[5].then_some(ToneStep::new(freq_pitch(chord[0] / 2.0), BASS_GATE))
                );
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
