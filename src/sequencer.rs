use std::sync::{Arc, Mutex};

pub const STEPS: usize = 8;
pub const DEFAULT_BPM: f64 = 120.0;

#[derive(Clone)]
pub enum ToneVoice {
    Sine,
    Square,
}

#[derive(Clone)]
pub struct Pattern {
    pub kick: [bool; STEPS],
    pub hihat_closed: [bool; STEPS],
    pub hihat_open: [bool; STEPS],
    pub tone: [bool; STEPS],
    pub tone_voice: ToneVoice,
}

impl Default for Pattern {
    fn default() -> Self {
        Self {
            kick: [true, false, false, false, true, false, false, false],
            hihat_closed: [true, true, true, true, true, true, true, true],
            hihat_open: [false, false, false, false, false, false, false, true],
            tone: [true, false, true, false, false, true, false, true],
            tone_voice: ToneVoice::Sine,
        }
    }
}

pub fn random_pattern() -> Pattern {
    Pattern {
        kick: std::array::from_fn(|_| fastrand::bool()),
        hihat_closed: std::array::from_fn(|_| fastrand::bool()),
        hihat_open: std::array::from_fn(|_| fastrand::bool()),
        tone: std::array::from_fn(|_| fastrand::bool()),
        tone_voice: if fastrand::bool() {
            ToneVoice::Sine
        } else {
            ToneVoice::Square
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
}

impl Default for SequencerState {
    fn default() -> Self {
        Self {
            pattern: Pattern::default(),
            bpm: DEFAULT_BPM,
            current_step: 0,
            playing: true,
            reset: false,
        }
    }
}

pub type SharedState = Arc<Mutex<SequencerState>>;

pub fn new_shared_state() -> SharedState {
    Arc::new(Mutex::new(SequencerState::default()))
}

pub struct AudioClock {
    sample_rate: f64,
    sample_counter: usize,
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
        let threshold = self.step_samples(bpm);
        self.sample_counter += 1;
        if self.sample_counter >= threshold {
            self.sample_counter = 0;
            self.step = (self.step + 1) % STEPS;
            Some(self.step)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_pattern_has_eight_steps() {
        let p = Pattern::default();
        assert_eq!(p.kick.len(), 8);
        assert_eq!(p.hihat_closed.len(), 8);
        assert_eq!(p.hihat_open.len(), 8);
        assert_eq!(p.tone.len(), 8);
    }

    #[test]
    fn default_pattern_kick_on_beats_one_and_five() {
        let p = Pattern::default();
        assert!(p.kick[0]);
        assert!(p.kick[4]);
        assert!(!p.kick[1]);
        assert!(!p.kick[2]);
        assert!(!p.kick[3]);
    }

    #[test]
    fn default_pattern_hihat_closed_on_every_step() {
        let p = Pattern::default();
        assert!(p.hihat_closed.iter().all(|&v| v));
    }

    #[test]
    fn clock_advances_step_after_correct_sample_count() {
        let bpm = 120.0;
        let sr = 44100.0;
        let mut clock = AudioClock::new(sr);
        let threshold = clock.step_samples(bpm);

        let mut fired = false;
        for _ in 0..threshold {
            if clock.advance(bpm).is_some() {
                fired = true;
            }
        }
        assert!(fired, "clock must fire exactly once per step window");
        assert_eq!(clock.step, 1);
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
    fn step_samples_scales_with_bpm() {
        let clock_slow = AudioClock::new(44100.0);
        let clock_fast = AudioClock::new(44100.0);
        assert!(clock_slow.step_samples(60.0) > clock_fast.step_samples(120.0));
    }

    #[test]
    fn shared_state_default_is_playing_at_120_bpm() {
        let state = new_shared_state();
        let s = state.lock().unwrap();
        assert!(s.playing);
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
            s.pattern.kick[2] = true;
        }
        let s = state.lock().unwrap();
        assert_eq!(s.bpm, 140.0);
        assert!(s.pattern.kick[2]);
    }

    #[test]
    fn random_pattern_has_eight_steps() {
        let p = random_pattern();
        assert_eq!(p.kick.len(), 8);
        assert_eq!(p.tone.len(), 8);
    }

    #[test]
    fn random_pattern_differs_from_default() {
        let default = Pattern::default();
        let random = random_pattern();
        let same = default.kick == random.kick
            && default.hihat_closed == random.hihat_closed
            && default.hihat_open == random.hihat_open
            && default.tone == random.tone;
        assert!(!same, "random pattern should differ from default");
    }
}
