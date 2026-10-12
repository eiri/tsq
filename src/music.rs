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

#[cfg(test)]
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

use crate::sequencer::{MelodyStep, PAGE_STEPS, STEPS};

pub const BARS: usize = STEPS / PAGE_STEPS;
pub const BAR_UNITS: u16 = STEP_UNITS * PAGE_STEPS as u16;

pub fn beat_weight(step: usize) -> i32 {
    match step % PAGE_STEPS {
        0 => 6,
        4 => 4,
        2 | 6 => 3,
        _ => 1,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Interval {
    pub start: u16,
    pub end: u16,
    pub pitch: u8,
    pub attack: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BarInfo {
    pub attacks: u8,
    pub coverage: u16,
    pub low: Option<u8>,
    pub high: Option<u8>,
    pub accents: u8,
    pub ending: bool,
    pub repeat: Option<usize>,
}

pub struct MelodyInfo {
    pub intervals: Vec<Interval>,
    pub bars: [BarInfo; BARS],
}

impl MelodyInfo {
    pub fn new(melody: &[Option<MelodyStep>; STEPS]) -> Self {
        let attacks: Vec<_> = melody
            .iter()
            .enumerate()
            .filter_map(|(i, n)| n.map(|n| (i, n)))
            .collect();
        let mut intervals = Vec::new();

        // Analyze steady-state carry. Cold playback still starts with silence.
        for (index, &(step, note)) in attacks.iter().enumerate() {
            let start = step as u16 * STEP_UNITS;
            let next = attacks[(index + 1) % attacks.len()].0 as u16 * STEP_UNITS;
            let distance = if next > start {
                next - start
            } else {
                LOOP_UNITS - start + next
            };
            let end = start + duration(note.duration).min(distance);
            intervals.push(Interval {
                start,
                end: end.min(LOOP_UNITS),
                pitch: note.pitch,
                attack: step,
            });
            if end > LOOP_UNITS {
                intervals.push(Interval {
                    start: 0,
                    end: end - LOOP_UNITS,
                    pitch: note.pitch,
                    attack: step,
                });
            }
        }
        intervals.sort_by_key(|i| (i.start, i.attack));
        let mut bars = [BarInfo::default(); BARS];
        for (bar, summary) in bars.iter_mut().enumerate() {
            let start = bar as u16 * BAR_UNITS;
            let end = start + BAR_UNITS;
            for interval in &intervals {
                let overlap = interval
                    .end
                    .min(end)
                    .saturating_sub(interval.start.max(start));
                if overlap > 0 {
                    summary.coverage += overlap;
                    summary.low = Some(
                        summary
                            .low
                            .map_or(interval.pitch, |p| p.min(interval.pitch)),
                    );
                    summary.high = Some(
                        summary
                            .high
                            .map_or(interval.pitch, |p| p.max(interval.pitch)),
                    );
                }
            }
            for step in 0..PAGE_STEPS {
                if melody[bar * PAGE_STEPS + step].is_some() {
                    summary.attacks += 1;
                    if step % 2 == 0 {
                        summary.accents |= 1 << step;
                    }
                }
            }
            let last = (0..PAGE_STEPS)
                .rev()
                .find_map(|s| melody[bar * PAGE_STEPS + s].map(|n| (s, n)));
            summary.ending = last.is_some_and(|(s, n)| {
                n.duration >= STEP_UNITS * 2 || (s <= 4 && summary.attacks <= 3)
            }) && (bar % 4 == 3 || summary.attacks <= 2);

            // Compare transposition-independent pitch and attack shapes.
            let shape = |b: usize| {
                let base = melody[b * PAGE_STEPS..(b + 1) * PAGE_STEPS]
                    .iter()
                    .flatten()
                    .next()
                    .map_or(0, |n| n.pitch);
                std::array::from_fn::<_, PAGE_STEPS, _>(|s| {
                    melody[b * PAGE_STEPS + s]
                        .map(|n| (i16::from(n.pitch) - i16::from(base), duration(n.duration)))
                })
            };
            summary.repeat = (0..bar).find(|&b| shape(b) == shape(bar) && summary.attacks > 0);
        }
        Self { intervals, bars }
    }

    pub fn sounding(&self, units: u16) -> Option<u8> {
        self.intervals
            .iter()
            .find(|i| i.start <= units && units < i.end)
            .map(|i| i.pitch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(pitch: u8, units: u16) -> MelodyStep {
        MelodyStep {
            pitch,
            duration: units,
            voice: crate::sequencer::MelodyVoice::M1,
            bend: false,
        }
    }

    #[test]
    fn carry_silence_and_cutoff() {
        let mut melody = [None; STEPS];
        melody[7] = Some(note(60, 180));
        melody[9] = Some(note(64, 30));
        let info = MelodyInfo::new(&melody);
        assert_eq!(info.bars[1].coverage, 90);
        assert_eq!(info.sounding(480), Some(60));
        assert_eq!(info.sounding(540), Some(64));
        assert_eq!(info.sounding(570), None);
        assert_eq!(info.intervals[0].end, 540);
    }

    #[test]
    fn loop_carry_counts_once() {
        let mut melody = [None; STEPS];
        melody[63] = Some(note(30, 180));
        melody[1] = Some(note(100, 30));
        let info = MelodyInfo::new(&melody);
        assert_eq!(info.sounding(0), Some(30));
        assert_eq!(info.sounding(60), Some(100));
        assert_eq!(
            info.bars.iter().map(|b| b.coverage as u32).sum::<u32>(),
            150
        );
        assert_eq!(info.bars[0].low, Some(30));
        assert_eq!(info.bars[0].high, Some(100));
        // Transport does not call analysis to synthesize a cold-start carry.
        assert!(melody[0].is_none());
        assert!(beat_weight(0) > beat_weight(4));
        assert!(beat_weight(4) > beat_weight(7));
    }

    #[test]
    fn empty_and_full_loop_analysis() {
        let mut melody = [None; STEPS];
        assert!(MelodyInfo::new(&melody).intervals.is_empty());
        melody[32] = Some(note(61, u16::MAX));
        let info = MelodyInfo::new(&melody);
        assert_eq!(
            info.bars.iter().map(|b| b.coverage as u32).sum::<u32>(),
            LOOP_UNITS as u32
        );
        assert_eq!(info.sounding(0), Some(61));
    }

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
