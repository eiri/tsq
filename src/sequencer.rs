use std::sync::{Arc, Mutex};

use crate::arrange::{
    HarmonyPlan, arrange, degree_chord, render_arpeggio, render_bass, render_drums,
};
pub use crate::music::ToneStep;
use crate::music::{ARP_GATE, BARS, BASS_GATE, MelodyInfo, SUSTAIN_GATE};

pub const STEPS: usize = 64;
pub const PAGE_STEPS: usize = 8;
pub const TRACKS: usize = 4;
pub const DEFAULT_BPM: f64 = 120.0;
pub const REROLL_ATTEMPTS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MelodyStyle {
    Pluck,
    Sustain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MelodyVoice {
    M1,
    M2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    pub seeds: Seeds,
    pub tonic: u8,
    pub mode: Mode,
    pub melody: [Option<MelodyStep>; STEPS],
    pub arpeggio: [Option<ToneStep>; STEPS],
    pub harmony: HarmonyPlan,
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

#[derive(Clone, Copy)]
pub(crate) enum Groove {
    Backbeat,
    FourFloor,
    HalfTime,
    Syncopated,
}

pub(crate) fn groove_steps(groove: Groove) -> [DrumStep; PAGE_STEPS] {
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

impl Default for Pattern {
    fn default() -> Self {
        let bar = groove_steps(Groove::Backbeat);
        let harmony = HarmonyPlan::preset();
        // Preserve authored octave placements, which exceed generated register bounds.
        let roots = [60, 70, 67, 63, 63, 60, 62, 67];
        let arpeggio = std::array::from_fn(|i| {
            let chord = harmony.chords[i / PAGE_STEPS];
            let root = roots[i / PAGE_STEPS];
            let classes = chord.classes();
            let offsets = classes.map(|p| (p + 12 - chord.root) % 12);
            [0, 1, 2, 2, 1, 0]
                .get(i % PAGE_STEPS)
                .map(|&index| ToneStep::new(root + offsets[index], ARP_GATE))
        });
        let bass = std::array::from_fn(|i| {
            matches!(i % PAGE_STEPS, 0 | 2 | 3)
                .then_some(ToneStep::new(roots[i / PAGE_STEPS] - 24, BASS_GATE))
        });
        Self {
            seeds: Seeds::from_rng(&mut fastrand::Rng::with_seed(0)),
            tonic: 0,
            mode: Mode::Major,
            melody: MELODY.map(|pitch| {
                (pitch != 0).then_some(MelodyStep {
                    pitch,
                    voice: MelodyVoice::M1,
                    duration: SUSTAIN_GATE,
                    bend: false,
                })
            }),
            arpeggio,
            harmony,
            bass,
            drums: std::array::from_fn(|i| bar[i % PAGE_STEPS]),
            melody_style: MelodyStyle::Sustain,
        }
    }
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
    let (melody, melody_style) =
        crate::melody::generate(tonic, mode, &mut fastrand::Rng::with_seed(seeds.melody));
    let mut pattern = Pattern {
        seeds,
        tonic,
        mode,
        melody,
        melody_style,
        harmony: HarmonyPlan {
            chords: [degree_chord(tonic, mode, 0); BARS],
        },
        arpeggio: [None; STEPS],
        bass: [None; STEPS],
        drums: [DrumStep::default(); STEPS],
    };
    arrange(&mut pattern);
    pattern
}

pub fn random_track(pattern: &mut Pattern, track: usize) {
    seeded_track(pattern, track, fastrand::u64(..));
}

pub fn seeded_track(pattern: &mut Pattern, track: usize, seed: u64) {
    if track >= TRACKS {
        return;
    }
    let mut root = fastrand::Rng::with_seed(seed);
    // Publish only a distinct, complete candidate. Exhaustion preserves the original seeds.
    for _ in 0..REROLL_ATTEMPTS {
        let mut candidate = pattern.clone();
        let seed = root.u64(..);
        match track {
            0 => {
                candidate.seeds.melody = seed;
                (candidate.melody, candidate.melody_style) = crate::melody::generate(
                    candidate.tonic,
                    candidate.mode,
                    &mut fastrand::Rng::with_seed(seed),
                );
                arrange(&mut candidate);
            }
            1 => {
                candidate.seeds.harmony = seed;
                candidate.seeds.arpeggio = root.u64(..);
                let info = MelodyInfo::new(&candidate.melody);
                candidate.harmony = crate::arrange::choose_harmony(
                    &info,
                    candidate.tonic,
                    candidate.mode,
                    &mut fastrand::Rng::with_seed(seed),
                );
                let seeds = candidate.seeds;
                render_arpeggio(
                    &mut candidate,
                    &info,
                    &mut fastrand::Rng::with_seed(seeds.arpeggio),
                );
                render_bass(
                    &mut candidate,
                    &info,
                    &mut fastrand::Rng::with_seed(seeds.bass),
                );
            }
            2 => {
                candidate.seeds.bass = seed;
                let info = MelodyInfo::new(&candidate.melody);
                render_bass(&mut candidate, &info, &mut fastrand::Rng::with_seed(seed));
            }
            3 => {
                candidate.seeds.groove = seed;
                let info = MelodyInfo::new(&candidate.melody);
                let bass_seed = candidate.seeds.bass;
                render_drums(&mut candidate, &info, &mut fastrand::Rng::with_seed(seed));
                render_bass(
                    &mut candidate,
                    &info,
                    &mut fastrand::Rng::with_seed(bass_seed),
                );
            }
            _ => unreachable!(),
        }
        let changed = match track {
            0 => {
                candidate.melody != pattern.melody || candidate.melody_style != pattern.melody_style
            }
            1 => candidate.arpeggio != pattern.arpeggio,
            2 => candidate.bass != pattern.bass,
            _ => candidate.drums != pattern.drums,
        };
        if changed {
            *pattern = candidate;
            return;
        }
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
    use crate::arrange::with_melody;

    #[test]
    fn default_melody_plays_ready_then_theme() {
        let pattern = Pattern::default();
        assert_eq!(pattern.melody.map(|n| n.map_or(0, |n| n.pitch)), MELODY);
        assert!(
            pattern
                .melody
                .iter()
                .flatten()
                .all(|n| n.duration == 144 && n.voice == MelodyVoice::M1 && !n.bend)
        );
        assert_eq!(pattern.melody_style, MelodyStyle::Sustain);
    }

    #[test]
    fn default_chords_follow_melody() {
        let pattern = Pattern::default();
        let roots = [60, 70, 67, 63, 63, 60, 62, 67];
        assert_eq!(pattern.harmony, HarmonyPlan::preset());
        for (bar, root) in roots.into_iter().enumerate() {
            assert_eq!(pattern.arpeggio[bar * 8].unwrap().pitch, root);
            assert_eq!(pattern.bass[bar * 8].unwrap().pitch, root - 24);
            let classes = pattern.harmony.chords[bar].classes();
            let expected = [0, 1, 2, 2, 1, 0].map(|i| root + (classes[i] + 12 - classes[0]) % 12);
            for (step, pitch) in expected.into_iter().enumerate() {
                assert_eq!(
                    pattern.arpeggio[bar * 8 + step],
                    Some(ToneStep::new(pitch, ARP_GATE))
                );
            }
            assert_eq!(&pattern.arpeggio[bar * 8 + 6..bar * 8 + 8], &[None, None]);
            for step in 0..8 {
                assert_eq!(
                    pattern.bass[bar * 8 + step].is_some(),
                    matches!(step, 0 | 2 | 3)
                );
            }
        }
        assert_eq!(pattern.arpeggio[10].unwrap().pitch, 77);
        assert_eq!(pattern.arpeggio[26].unwrap().pitch, 70);
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
    fn seeded_generation_repeats() {
        for seed in 0..16 {
            assert_eq!(seeded_pattern(seed), seeded_pattern(seed));
        }
        assert_ne!(random_pattern(), Pattern::default());
    }

    #[test]
    fn stage_streams_are_independent() {
        let original = seeded_pattern(42);
        let mut altered = original.clone();
        let info = MelodyInfo::new(&original.melody);
        let mut arp = fastrand::Rng::with_seed(original.seeds.arpeggio);
        for _ in 0..100 {
            arp.u64(..);
        }
        render_arpeggio(&mut altered, &info, &mut arp);
        render_drums(
            &mut altered,
            &info,
            &mut fastrand::Rng::with_seed(original.seeds.groove),
        );
        let (melody, style) = crate::melody::generate(
            original.tonic,
            original.mode,
            &mut fastrand::Rng::with_seed(original.seeds.melody),
        );
        assert_eq!(melody, original.melody);
        assert_eq!(style, original.melody_style);
        assert_eq!(altered.drums, original.drums);
    }

    #[test]
    fn supplied_gates_are_bounded() {
        let mut melody = [None; STEPS];
        melody[0] = Some(MelodyStep {
            pitch: 255,
            duration: 0,
            voice: MelodyVoice::M2,
            bend: true,
        });
        melody[8] = Some(MelodyStep {
            pitch: 61,
            duration: u16::MAX,
            voice: MelodyVoice::M1,
            bend: false,
        });
        let pattern = with_melody(&Pattern::default(), melody);
        assert_eq!(pattern.melody[0].unwrap().pitch, 127);
        assert_eq!(pattern.melody[0].unwrap().duration, 1);
        assert_eq!(
            pattern.melody[8].unwrap().duration,
            crate::music::LOOP_UNITS
        );
        let before = pattern.melody;
        let mut repeat = pattern;
        arrange(&mut repeat);
        assert_eq!(repeat.melody, before);
    }

    #[test]
    fn seeded_rerolls_repeat() {
        for track in 0..TRACKS {
            let mut a = seeded_pattern(42);
            let mut b = a.clone();
            seeded_track(&mut a, track, 123);
            seeded_track(&mut b, track, 123);
            assert_eq!(a, b);
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
    fn rerolls_follow_dependencies() {
        for track in 0..TRACKS {
            let mut pattern = seeded_pattern(42);
            let before = pattern.clone();
            seeded_track(&mut pattern, track, 123);
            assert_eq!((pattern.tonic, pattern.mode), (before.tonic, before.mode));
            if track != 0 {
                assert_eq!(pattern.melody, before.melody);
            }
            if track >= 2 {
                assert_eq!(pattern.arpeggio, before.arpeggio);
                assert_eq!(pattern.harmony, before.harmony);
            }
            if track == 1 || track == 2 {
                assert_eq!(pattern.drums, before.drums);
            }
            let info = MelodyInfo::new(&pattern.melody);
            let mut expected = pattern.clone();
            let seeds = pattern.seeds;
            render_bass(
                &mut expected,
                &info,
                &mut fastrand::Rng::with_seed(seeds.bass),
            );
            assert_eq!(pattern.bass, expected.bass);
            if track == 0 || track == 3 {
                render_drums(
                    &mut expected,
                    &info,
                    &mut fastrand::Rng::with_seed(seeds.groove),
                );
                assert_eq!(pattern.drums, expected.drums);
            }
            if track <= 1 {
                render_arpeggio(
                    &mut expected,
                    &info,
                    &mut fastrand::Rng::with_seed(seeds.arpeggio),
                );
                assert_eq!(pattern.arpeggio, expected.arpeggio);
            }
        }
    }

    #[test]
    fn melody_update_is_pure() {
        let original = seeded_pattern(42);
        let mut melody = original.melody;
        melody[0] = Some(MelodyStep {
            pitch: 61,
            duration: 480,
            voice: MelodyVoice::M2,
            bend: true,
        });
        let a = with_melody(&original, melody);
        assert_eq!(a, with_melody(&original, melody));
        assert_eq!(a.melody, melody);
        assert_eq!(a.seeds, original.seeds);
        assert_ne!(a.harmony, original.harmony);
        assert_ne!(a.arpeggio, original.arpeggio);
        assert_ne!(a.bass, original.bass);
        assert_ne!(a.drums, original.drums);
        assert_eq!(original, seeded_pattern(42));
    }

    #[test]
    fn degenerate_rerolls_terminate() {
        let original = with_melody(&Pattern::default(), [None; STEPS]);
        let mut pattern = original.clone();
        seeded_track(&mut pattern, 3, 1);
        assert_eq!(pattern, original);
        seeded_track(&mut pattern, TRACKS, 1);
        assert_eq!(pattern, original);
        for track in 0..TRACKS {
            seeded_track(&mut pattern, track, 1);
        }
        assert_eq!(REROLL_ATTEMPTS, 16);
    }

    #[test]
    fn clock_advances_step_after_correct_sample_count() {
        let mut clock = AudioClock::new(44100.0);
        let threshold = clock.step_samples(120.0);
        assert_eq!(clock.advance(120.0), Some(0));
        for _ in 1..threshold {
            assert_eq!(clock.advance(120.0), None);
        }
        assert_eq!(clock.advance(120.0), Some(1));
    }

    #[test]
    fn clock_wraps_around_after_64_steps() {
        let bpm = 240.0;
        let mut clock = AudioClock::new(44100.0);
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
        let clock = AudioClock::new(44100.0);
        assert!(clock.step_samples(60.0) > clock.step_samples(120.0));
    }

    #[test]
    fn shared_state_default_is_paused_at_120_bpm() {
        let state = new_shared_state();
        let s = state.lock().unwrap();
        assert!(!s.playing);
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
}
