//! Harmony is shared by every pitched supporting track.
use crate::music::{BAR_UNITS, BARS, MelodyInfo, STEP_UNITS, beat_weight};
use crate::sequencer::{Mode, Pattern};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    Major,
    Minor,
    Diminished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub root: u8,
    pub quality: Quality,
}

impl Chord {
    pub fn classes(self) -> [u8; 3] {
        let intervals = match self.quality {
            Quality::Major => [0, 4, 7],
            Quality::Minor => [0, 3, 7],
            Quality::Diminished => [0, 3, 6],
        };
        intervals.map(|i| (self.root + i) % 12)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HarmonyPlan {
    pub chords: [Chord; BARS],
}

pub fn scale_offsets(mode: Mode) -> [u8; 7] {
    match mode {
        Mode::Major => [0, 2, 4, 5, 7, 9, 11],
        Mode::Minor => [0, 2, 3, 5, 7, 8, 10],
    }
}

pub fn degree_chord(tonic: u8, mode: Mode, degree: u8) -> Chord {
    // Quality follows the actual stacked intervals, never a fixed degree index.
    let scale = scale_offsets(mode);
    let d = degree as usize;
    let third = (scale[(d + 2) % 7] + 12 - scale[d]) % 12;
    let fifth = (scale[(d + 4) % 7] + 12 - scale[d]) % 12;
    let quality = if fifth == 6 {
        Quality::Diminished
    } else if third == 3 {
        Quality::Minor
    } else {
        Quality::Major
    };
    Chord {
        root: (tonic + scale[d]) % 12,
        quality,
    }
}

impl HarmonyPlan {
    pub fn from_degrees(tonic: u8, mode: Mode, roots: [u8; BARS]) -> Self {
        Self {
            chords: roots.map(|root| match root {
                7 => Chord {
                    root: (tonic + 10) % 12,
                    quality: Quality::Major,
                },
                8 => Chord {
                    root: (tonic + 3) % 12,
                    quality: Quality::Major,
                },
                _ => degree_chord(tonic, mode, root),
            }),
        }
    }

    pub fn preset() -> Self {
        Self::from_degrees(0, Mode::Major, [0, 7, 4, 8, 8, 0, 1, 4])
    }
}

pub fn melody_score(info: &MelodyInfo, bar: usize, chord: Chord) -> i32 {
    let classes = chord.classes();
    let start = bar as u16 * BAR_UNITS;
    let mut score = 0;
    // Every sounding unit contributes, including carry and truncated gates.
    for interval in &info.intervals {
        for step in 0..8 {
            let left = start + step * STEP_UNITS;
            let overlap = interval
                .end
                .min(left + STEP_UNITS)
                .saturating_sub(interval.start.max(left));
            let weight = i32::from(overlap) * beat_weight(step as usize);
            score += if classes.contains(&(interval.pitch % 12)) {
                weight
            } else {
                -weight
            };
        }
    }
    let summary = info.bars[bar];
    if summary.ending && summary.high.is_some_and(|p| classes.contains(&(p % 12))) {
        score += 20;
    }
    if summary.repeat.is_some() && summary.low.is_some_and(|p| classes.contains(&(p % 12))) {
        score += 10;
    }
    if summary.coverage > 0
        && summary.accents & 1 != 0
        && info
            .sounding(start)
            .is_some_and(|p| classes.contains(&(p % 12)))
    {
        score += 10;
    }
    score
}

pub fn legacy_root(pattern: &Pattern, bar: usize) -> u8 {
    let chord = pattern.harmony.chords[bar];
    (0..9)
        .find(|&r| {
            HarmonyPlan::from_degrees(pattern.tonic, pattern.mode, [r; BARS]).chords[0] == chord
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_and_minor_quality() {
        assert_eq!(
            HarmonyPlan::preset().chords.map(|c| c.root),
            [0, 10, 7, 3, 3, 0, 2, 7]
        );
        assert_eq!(degree_chord(0, Mode::Minor, 1).quality, Quality::Diminished);
        assert_eq!(degree_chord(0, Mode::Minor, 6).quality, Quality::Major);
        assert_eq!(degree_chord(0, Mode::Major, 6).quality, Quality::Diminished);
    }
}
