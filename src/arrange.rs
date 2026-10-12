//! Harmony is shared by every pitched supporting track.
use crate::music::{BAR_UNITS, BARS, Interval, LOOP_UNITS, MelodyInfo, STEP_UNITS, beat_weight};
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
    // Derive quality from the actual thirds and fifths in each mode.
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

struct ScoreWeights {
    fit: i32,
    conflict: i32,
    decoration_divisor: i32,
    change: i32,
    movement: i32,
    cadence: i32,
    repeat_divisor: i32,
    diminished: i32,
}

const WEIGHTS: ScoreWeights = ScoreWeights {
    fit: 1,
    conflict: 2,
    decoration_divisor: 4,
    change: 60,
    movement: 8,
    cadence: 120,
    repeat_divisor: 8,
    diminished: 80,
};
pub const PATH_MARGIN: i32 = 120;
const STATE_PATHS: usize = 4;
const COMPLETE_PATHS: usize = 32;
const DECORATION_UNITS: u16 = 60;
const RESOLUTION_UNITS: u16 = 120;

fn neighbors(info: &MelodyInfo, attack: usize) -> Option<(Interval, Interval, Interval)> {
    // Exclude split carry fragments; compare absolute pitches across bar boundaries.
    let notes: Vec<_> = info
        .intervals
        .iter()
        .copied()
        .filter(|i| i.start == i.attack as u16 * STEP_UNITS)
        .collect();
    if notes.len() < 3 {
        return None;
    }
    let index = notes.iter().position(|i| i.attack == attack)?;
    Some((
        notes[(index + notes.len() - 1) % notes.len()],
        notes[index],
        notes[(index + 1) % notes.len()],
    ))
}

fn resolves(info: &MelodyInfo, interval: &Interval, bar: usize, chord: Chord) -> bool {
    let classes = chord.classes();
    let Some((before, note, after)) = neighbors(info, interval.attack) else {
        return false;
    };
    let to_next = (after.start + LOOP_UNITS - note.start) % LOOP_UNITS;
    let from_previous = (note.start + LOOP_UNITS - before.start) % LOOP_UNITS;
    let delta = i16::from(note.pitch) - i16::from(before.pitch);
    let next_delta = i16::from(after.pitch) - i16::from(note.pitch);
    let stepwise = (1..=2).contains(&delta.abs()) && (1..=2).contains(&next_delta.abs());
    let preparation = classes.contains(&(before.pitch % 12)) && before.end == note.start;
    let resolution = classes.contains(&(after.pitch % 12)) && to_next <= RESOLUTION_UNITS;
    let decoration = preparation
        && resolution
        && stepwise
        && (delta.signum() == next_delta.signum() || before.pitch == after.pitch)
        && to_next <= DECORATION_UNITS
        && from_previous <= RESOLUTION_UNITS
        && note.end - note.start <= DECORATION_UNITS;

    // A prepared carry may resolve by step shortly after a chord change.
    let bar_start = bar as u16 * BAR_UNITS;
    let carried = interval.start < bar_start || (interval.start == 0 && interval.attack > 0);
    let suspension = carried
        && classes.contains(&(after.pitch % 12))
        && (1..=2).contains(&next_delta.abs())
        && (after.start + LOOP_UNITS - bar_start) % LOOP_UNITS <= RESOLUTION_UNITS;
    decoration || suspension
}

pub fn melody_score(info: &MelodyInfo, bar: usize, chord: Chord) -> i32 {
    let classes = chord.classes();
    let start = bar as u16 * BAR_UNITS;
    let mut score = 0;
    for interval in &info.intervals {
        let compatible = classes.contains(&(interval.pitch % 12));
        let decoration = !compatible && resolves(info, interval, bar, chord);
        for step in 0..8 {
            let left = start + step * STEP_UNITS;
            let overlap = interval
                .end
                .min(left + STEP_UNITS)
                .saturating_sub(interval.start.max(left));
            let weight = i32::from(overlap) * beat_weight(step as usize);
            score += if compatible {
                weight * WEIGHTS.fit
            } else if decoration {
                -weight * WEIGHTS.conflict / WEIGHTS.decoration_divisor
            } else {
                -weight * WEIGHTS.conflict
            };
        }
    }
    let summary = info.bars[bar];
    if summary.ending {
        let last = info
            .intervals
            .iter()
            .filter(|i| i.start < start + BAR_UNITS && i.end > start)
            .max_by_key(|i| i.end.min(start + BAR_UNITS));
        if last
            .and_then(|i| info.sounding(i.end.min(start + BAR_UNITS) - 1))
            .is_some_and(|pitch| pitch % 12 == chord.root)
        {
            score += WEIGHTS.cadence;
        }
    }
    if chord.quality == Quality::Diminished {
        score -= WEIGHTS.diminished;
    }
    score
}

pub fn candidates(info: &MelodyInfo, tonic: u8, mode: Mode) -> Vec<Chord> {
    let mut choices: Vec<_> = (0..7).map(|d| degree_chord(tonic, mode, d)).collect();
    let scale = scale_offsets(mode).map(|p| (p + tonic) % 12);
    let chromatic: Vec<_> = info
        .intervals
        .iter()
        .map(|i| i.pitch % 12)
        .filter(|p| !scale.contains(p))
        .collect();
    for offset in [3, 10] {
        let chord = Chord {
            root: (tonic + offset) % 12,
            quality: Quality::Major,
        };
        if !choices.contains(&chord) && chromatic.iter().any(|p| chord.classes().contains(p)) {
            choices.push(chord);
        }
    }
    choices
}

fn transition(from: Chord, to: Chord) -> i32 {
    if from == to {
        return 0;
    }
    let movement: i32 = from
        .classes()
        .iter()
        .map(|&p| {
            to.classes()
                .iter()
                .map(|&q| {
                    let d = p.abs_diff(q);
                    i32::from(d.min(12 - d))
                })
                .min()
                .unwrap()
        })
        .sum();
    -WEIGHTS.change - movement * WEIGHTS.movement
}

#[derive(Clone, Debug)]
struct Path {
    score: i32,
    slots: Vec<usize>,
}

fn retain(paths: &mut Vec<Path>, count: usize) {
    paths.sort_by(|a, b| b.score.cmp(&a.score).then(a.slots.cmp(&b.slots)));
    paths.dedup_by(|a, b| a.slots == b.slots);
    paths.truncate(count);
}

fn search(scores: &[Vec<i32>], choices: &[Chord]) -> Vec<Path> {
    let mut complete = Vec::new();
    for first in 0..choices.len() {
        let mut states = vec![Vec::<Path>::new(); choices.len()];
        states[first].push(Path {
            score: scores[0][first],
            slots: vec![first],
        });
        for bar_scores in &scores[1..] {
            let mut next = vec![Vec::new(); choices.len()];
            for (to, paths) in next.iter_mut().enumerate() {
                for (from, previous) in states.iter().enumerate() {
                    for path in previous {
                        let mut slots = path.slots.clone();
                        slots.push(to);
                        paths.push(Path {
                            score: path.score
                                + transition(choices[from], choices[to])
                                + bar_scores[to],
                            slots,
                        });
                    }
                }
                retain(paths, STATE_PATHS);
            }
            states = next;
        }
        for (last, paths) in states.into_iter().enumerate() {
            for mut path in paths {
                path.score += transition(choices[last], choices[first]);
                complete.push(path);
            }
        }
    }
    // Apply boundary costs before filtering or sampling complete paths.
    retain(&mut complete, COMPLETE_PATHS);
    let best = complete[0].score;
    complete.retain(|p| p.score >= best - PATH_MARGIN);
    complete
}

pub fn choose_harmony(
    info: &MelodyInfo,
    tonic: u8,
    mode: Mode,
    rng: &mut fastrand::Rng,
) -> HarmonyPlan {
    let choices = candidates(info, tonic, mode);
    // Fit is computed once per bar/candidate. Repeated shapes share supported preferences.
    let fits: Vec<Vec<_>> = (0..BARS)
        .map(|b| choices.iter().map(|&c| melody_score(info, b, c)).collect())
        .collect();
    let scores: Vec<Vec<_>> = (0..BARS)
        .map(|b| {
            choices
                .iter()
                .enumerate()
                .map(|(c, _)| {
                    fits[b][c]
                        + info.bars[b]
                            .repeat
                            .map_or(0, |r| fits[r][c].max(0) / WEIGHTS.repeat_divisor)
                })
                .collect()
        })
        .collect();
    let paths = search(&scores, &choices);
    let selected = &paths[rng.usize(0..paths.len())];
    HarmonyPlan {
        chords: std::array::from_fn(|b| choices[selected.slots[b]]),
    }
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
    use crate::sequencer::{MelodyStep, MelodyVoice, STEPS};

    fn info(notes: &[(usize, u8, u16)]) -> MelodyInfo {
        let mut melody = [None; STEPS];
        for &(step, pitch, duration) in notes {
            melody[step] = Some(MelodyStep {
                pitch,
                duration,
                voice: MelodyVoice::M1,
                bend: false,
            });
        }
        MelodyInfo::new(&melody)
    }

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

    #[test]
    fn decoration_and_suspension() {
        let c = degree_chord(0, Mode::Major, 0);
        for end in [60, 64] {
            let melody = info(&[(0, 60, 120), (2, 62, 60), (3, end, 180)]);
            assert!(resolves(&melody, &melody.intervals[1], 0, c));
            assert!(
                melody_score(&melody, 0, c)
                    > melody_score(&melody, 0, degree_chord(0, Mode::Major, 1))
            );
        }
        let short = info(&[(0, 60, 120), (2, 62, 30), (3, 64, 180)]);
        let long = info(&[(0, 60, 120), (2, 62, 300), (7, 64, 180)]);
        assert!(melody_score(&short, 0, c) > melody_score(&long, 0, c));
        let held = info(&[(0, 67, 120), (6, 65, 180), (9, 64, 180)]);
        let carry = held.intervals.iter().find(|i| i.attack == 6).unwrap();
        assert!(resolves(&held, carry, 1, c));
        let octave = info(&[(0, 60, 120), (2, 74, 60), (3, 64, 180)]);
        assert!(!resolves(&octave, &octave.intervals[1], 0, c));
    }

    #[test]
    fn carried_and_chromatic_fit() {
        let c = degree_chord(0, Mode::Major, 0);
        let held = info(&[(7, 61, 300)]);
        assert!(melody_score(&held, 1, c) < melody_score(&info(&[(7, 61, 30)]), 1, c));
        let chromatic = info(&[(0, 70, 480), (8, 63, 480)]);
        let choices = candidates(&chromatic, 0, Mode::Major);
        assert!(choices.contains(&Chord {
            root: 10,
            quality: Quality::Major
        }));
        assert!(choices.contains(&Chord {
            root: 3,
            quality: Quality::Major
        }));
        let minor = candidates(&chromatic, 0, Mode::Minor);
        for (i, c) in minor.iter().enumerate() {
            assert!(!minor[..i].contains(c));
        }
        let ending = info(&[(56, 65, 480)]);
        let plan = choose_harmony(&ending, 0, Mode::Major, &mut fastrand::Rng::with_seed(1));
        assert!(plan.chords[7].classes().contains(&5));
        assert_ne!(plan.chords[7], c);
    }

    #[test]
    fn tiny_search_matches_exhaustive() {
        let choices = [
            degree_chord(0, Mode::Major, 0),
            degree_chord(0, Mode::Major, 4),
        ];
        let scores = vec![vec![40, 80], vec![200, 10], vec![30, 90]];
        let mut exhaustive = Vec::new();
        for a in 0..2 {
            for b in 0..2 {
                for c in 0..2 {
                    exhaustive.push(Path {
                        score: scores[0][a]
                            + scores[1][b]
                            + scores[2][c]
                            + transition(choices[a], choices[b])
                            + transition(choices[b], choices[c])
                            + transition(choices[c], choices[a]),
                        slots: vec![a, b, c],
                    });
                }
            }
        }
        retain(&mut exhaustive, COMPLETE_PATHS);
        let paths = search(&scores, &choices);
        assert_eq!(paths[0].score, exhaustive[0].score);
        assert_eq!(paths[0].slots, exhaustive[0].slots);
        assert!(
            paths
                .iter()
                .all(|p| p.score >= exhaustive[0].score - PATH_MARGIN)
        );
    }
}
