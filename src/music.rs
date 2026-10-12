//! Musical time is independent of the 60 Hz sound player.

pub const STEP_UNITS: u16 = 60;
pub const LOOP_UNITS: u16 = 3840;
pub const SHORT_GATE: u16 = 32;
pub const PLUCK_GATE: u16 = 48;
pub const SUSTAIN_GATE: u16 = 144;
pub const ARP_GATE: u16 = 48;
pub const BASS_GATE: u16 = 32;

pub fn pitch_freq(pitch: u8) -> f32 {
    440.0 * 2.0_f32.powf((pitch as f32 - 69.0) / 12.0)
}

pub fn freq_pitch(freq: f32) -> u8 {
    (69.0 + 12.0 * (freq / 440.0).log2())
        .round()
        .clamp(0.0, 127.0) as u8
}

pub fn duration(units: u16) -> u16 {
    units.clamp(1, LOOP_UNITS)
}

pub fn player_ticks(units: u16, bpm: f64) -> u32 {
    // One eighth note is 30/BPM seconds; round to the nearest player tick.
    ((duration(units) as f64 * 30.0 / (STEP_UNITS as f64 * bpm) * 60.0).round() as u32).max(1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToneStep {
    pub pitch: u8,
    pub duration: u16,
}

impl ToneStep {
    pub fn new(pitch: u8, units: u16) -> Self {
        Self {
            pitch: pitch.min(127),
            duration: duration(units),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitch_and_gate_conversion() {
        assert_eq!(pitch_freq(69), 440.0);
        assert!((pitch_freq(60) - 261.63).abs() < 0.01);
        for pitch in 0..128 {
            assert_eq!(freq_pitch(pitch_freq(pitch)), pitch);
        }
        for (units, ticks) in [(32, 8), (48, 12), (144, 36)] {
            assert_eq!(player_ticks(units, 120.0), ticks);
            assert_eq!(player_ticks(units, 60.0), ticks * 2);
            assert_eq!(player_ticks(units, 180.0), ticks * 2 / 3);
        }
    }

    #[test]
    fn invalid_duration_is_clamped() {
        assert_eq!(
            ToneStep::new(255, 0),
            ToneStep {
                pitch: 127,
                duration: 1
            }
        );
        assert_eq!(duration(u16::MAX), LOOP_UNITS);
        assert_eq!(player_ticks(0, 180.0), 1);
        assert_eq!(player_ticks(u16::MAX, 120.0), 960);
    }
}
