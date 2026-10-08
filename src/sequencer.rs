use std::sync::{Arc, Mutex};

pub const STEPS: usize = 8;
pub const DEFAULT_BPM: f64 = 120.0;

#[derive(Clone, Copy)]
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

#[derive(Clone)]
pub struct Pattern {
    pub melody: [Option<MelodyStep>; STEPS],
    pub arpeggio: [Option<[f32; 3]>; STEPS],
    pub bass: [Option<f32>; STEPS],
    pub drums: [Option<Drum>; STEPS],
    pub melody_style: MelodyStyle,
}

// Frequencies use equal-tempered C major notes in Hz.
const MELODY: [f32; STEPS] = [261.63, 293.66, 329.63, 349.23, 392.0, 440.0, 493.88, 523.25];
const CHORD: [f32; 3] = [261.63, 329.63, 392.0];

impl Default for Pattern {
    fn default() -> Self {
        Self {
            melody: std::array::from_fn(|i| {
                let voice = if [0, 2, 5, 7].contains(&i) {
                    MelodyVoice::M1
                } else if [1, 3, 6].contains(&i) {
                    MelodyVoice::M2
                } else {
                    return None;
                };
                Some(MelodyStep {
                    freq: MELODY[i],
                    voice,
                })
            }),
            arpeggio: std::array::from_fn(|i| (i % 2 == 0).then_some(CHORD)),
            // Leave channel C free on these steps so the bass can sound.
            bass: std::array::from_fn(|i| [3, 5, 7].contains(&i).then_some(130.81)),
            drums: [
                Some(Drum::Kick),
                None,
                Some(Drum::ClosedHat),
                None,
                Some(Drum::Snare),
                None,
                Some(Drum::ClosedHat),
                None,
            ],
            melody_style: MelodyStyle::Pluck,
        }
    }
}

pub fn random_pattern() -> Pattern {
    Pattern {
        melody: std::array::from_fn(|i| {
            fastrand::bool().then(|| MelodyStep {
                freq: MELODY[i],
                voice: if fastrand::bool() {
                    MelodyVoice::M1
                } else {
                    MelodyVoice::M2
                },
            })
        }),
        arpeggio: std::array::from_fn(|_| fastrand::bool().then_some(CHORD)),
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
    }
}

#[derive(Clone)]
pub struct SequencerState {
    pub pattern: Pattern,
    pub bpm: f64,
    pub current_step: usize,
    pub playing: bool,
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
    fn default_pattern_has_eight_steps() {
        let p = Pattern::default();
        assert_eq!(p.melody.len(), 8);
        assert_eq!(p.arpeggio.len(), 8);
        assert_eq!(p.bass.len(), 8);
        assert_eq!(p.drums.len(), 8);
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
    fn clock_wraps_around_after_eight_steps() {
        let bpm = 240.0;
        let sr = 44100.0;
        let mut clock = AudioClock::new(sr);
        let threshold = clock.step_samples(bpm);

        for _ in 0..(threshold * STEPS) {
            clock.advance(bpm);
        }
        assert_eq!(clock.step, 0);
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
    fn random_pattern_has_eight_steps() {
        let p = random_pattern();
        assert_eq!(p.drums.len(), 8);
        assert_eq!(p.melody.len(), 8);
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
