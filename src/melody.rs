//! Two-bar motifs develop into a question and an answer.
use crate::arrange::scale_offsets;
use crate::music::{LOOP_UNITS, PLUCK_GATE, SHORT_GATE, STEP_UNITS, SUSTAIN_GATE};
use crate::sequencer::{MelodyStep, MelodyStyle, MelodyVoice, Mode, STEPS};

pub const MAX_JUMP: u8 = 7;
const LARGE_JUMP: u8 = 4;
const RHYTHMS: [[usize; 5]; 4] = [
    [0, 2, 3, 4, 6],
    [0, 1, 3, 5, 6],
    [0, 2, 4, 5, 7],
    [0, 3, 4, 6, 7],
];

fn nearest(scale: &[u8; 8], target: u8, previous: u8, recovery: i16, limit: u8) -> u8 {
    // Select a legal pitch rather than clamp an out-of-range random walk.
    *scale
        .iter()
        .filter(|&&p| {
            let delta = i16::from(p) - i16::from(previous);
            p.abs_diff(previous) <= limit
                && (recovery == 0 || ((1..=2).contains(&delta.abs()) && delta.signum() == recovery))
        })
        .min_by_key(|&&p| (p.abs_diff(target), p))
        .unwrap_or(&previous)
}

pub fn generate(
    tonic: u8,
    mode: Mode,
    rng: &mut fastrand::Rng,
) -> ([Option<MelodyStep>; STEPS], MelodyStyle) {
    let base = 60 + tonic;
    let mut pitches = [base; 8];
    pitches[..7].copy_from_slice(&scale_offsets(mode).map(|p| base + p));
    pitches[7] = base + 12;
    let rhythms = [RHYTHMS[rng.usize(0..4)], RHYTHMS[rng.usize(0..4)]];
    let counts = [rng.usize(3..6), rng.usize(3..6)];
    let contour = rng.usize(0..3);
    let mut motif = Vec::new();
    let mut degree = rng.usize(0..3);
    for bar in 0..2 {
        for (index, &step) in rhythms[bar][..counts[bar]].iter().enumerate() {
            let accent = step == 0 || step == 4;
            let target = if accent && rng.usize(0..3) == 0 {
                [0, 2, 4][rng.usize(0..3)]
            } else {
                let motion: i32 = match contour {
                    0 => {
                        if index < 2 {
                            1
                        } else {
                            -1
                        }
                    }
                    1 => {
                        if bar == 0 {
                            1
                        } else {
                            -1
                        }
                    }
                    _ => match rng.usize(0..8) {
                        0 => 3,
                        1 => -3,
                        2 => 0,
                        3..=5 => 1,
                        _ => -1,
                    },
                };
                let choices: Vec<_> = (0..7)
                    .filter(|&d| {
                        (d as i32 - degree as i32).signum() == motion.signum()
                            && (d as i32 - degree as i32).abs() <= motion.abs()
                    })
                    .collect();
                if motion == 0 {
                    degree
                } else {
                    choices.last().copied().unwrap_or(degree)
                }
            };
            degree = target;
            motif.push((bar * 8 + step, pitches[degree]));
        }
    }

    // Close the motif near its start so repetition does not create a large seam jump.
    let tail = motif.len() - 3;
    let mut previous = base;
    for (i, (_, pitch)) in motif.iter_mut().enumerate() {
        let target = if i == 0 {
            base
        } else if i >= tail {
            base + 2
        } else {
            *pitch
        };
        *pitch = nearest(&pitches, target, previous, 0, LARGE_JUMP);
        previous = *pitch;
    }

    let closing_step = [56, 58, 60][rng.usize(0..3)];
    let mut attacks = Vec::new();
    for phrase in 0..4 {
        for &(step, pitch) in &motif {
            let absolute = phrase * 16 + step;
            if (phrase == 1 && absolute >= 28) || (phrase == 3 && absolute >= closing_step) {
                continue;
            }
            let target = if (phrase == 3 && absolute >= 48) || (phrase == 1 && absolute >= 24) {
                base + 2
            } else {
                pitch
            };
            attacks.push((absolute, target));
        }
    }
    // The question stays open; the answer arrives early enough to sustain.
    attacks.push((28, base + 2));
    attacks.push((closing_step, base));
    attacks.sort_by_key(|&(s, _)| s);
    attacks.dedup_by_key(|a| a.0);
    let high = attacks
        .iter()
        .enumerate()
        .filter(|(_, (s, _))| (32..48).contains(s))
        .max_by_key(|(_, (_, p))| p)
        .map(|(i, _)| i)
        .unwrap();
    attacks[high].1 = (attacks[high].1 + 2).min(base + 12);

    let mut previous = base;
    let mut recovery = 0;
    for (index, (step, target)) in attacks.iter_mut().enumerate() {
        let pitch = if index == 0 {
            base
        } else {
            nearest(
                &pitches,
                *target,
                previous,
                recovery,
                if *step >= 48 { LARGE_JUMP } else { MAX_JUMP },
            )
        };
        let delta = i16::from(pitch) - i16::from(previous);
        recovery = if pitch.abs_diff(previous) > LARGE_JUMP {
            -delta.signum()
        } else {
            0
        };
        *target = pitch;
        previous = pitch;
    }
    // Approach the final tonic by step without an unresolved large leap.
    let last = attacks.len() - 1;
    attacks[last - 1].1 = base + 2;
    attacks[last].1 = base;
    // Repair the approach backwards if the prior figure sits too high.
    for i in (1..last).rev() {
        if attacks[i].1.abs_diff(attacks[i - 1].1) > MAX_JUMP {
            attacks[i - 1].1 = nearest(&pitches, attacks[i].1, attacks[i].1, 0, MAX_JUMP);
        }
    }

    // Sound choices use the same seed stream but cannot alter the pitch structure.
    let voice = if rng.bool() {
        MelodyVoice::M1
    } else {
        MelodyVoice::M2
    };
    let style = if rng.bool() {
        MelodyStyle::Pluck
    } else {
        MelodyStyle::Sustain
    };
    let mut melody = [None; STEPS];
    for &(step, pitch) in &attacks {
        let ending = step == 28 || step == closing_step;
        let duration = if step == closing_step {
            LOOP_UNITS - step as u16 * STEP_UNITS
        } else if ending {
            240
        } else if rng.usize(0..4) == 0 {
            SHORT_GATE
        } else if voice == MelodyVoice::M1 && style == MelodyStyle::Pluck {
            PLUCK_GATE
        } else {
            SUSTAIN_GATE
        };
        melody[step] = Some(MelodyStep {
            pitch,
            duration,
            voice,
            bend: ending && rng.usize(0..4) == 0,
        });
    }
    (melody, style)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn seeded_phrase_corpus() {
        let mut degrees = BTreeSet::new();
        let mut rhythms = BTreeSet::new();
        let mut contours = BTreeSet::new();
        for mode in [Mode::Major, Mode::Minor] {
            let mut reached = BTreeSet::new();
            for seed in 0..256 {
                let (melody, style) = generate(0, mode, &mut fastrand::Rng::with_seed(seed));
                let (repeat, other_style) = generate(0, mode, &mut fastrand::Rng::with_seed(seed));
                assert_eq!(melody, repeat, "seed {seed}");
                assert!(style == other_style);
                let scale = scale_offsets(mode);
                let attacks: Vec<_> = melody
                    .iter()
                    .enumerate()
                    .filter_map(|(s, n)| n.map(|n| (s, n)))
                    .collect();
                for &(_, note) in &attacks {
                    assert!((60..=72).contains(&note.pitch));
                    assert!(scale.contains(&(note.pitch % 12)));
                    assert!((1..=LOOP_UNITS).contains(&note.duration));
                    degrees.insert(note.pitch % 12);
                    reached.insert(note.pitch % 12);
                }
                for pair in attacks.windows(2) {
                    assert!(
                        pair[0].1.pitch.abs_diff(pair[1].1.pitch) <= MAX_JUMP,
                        "seed {seed}: {pair:?}"
                    );
                }
                for triple in attacks.windows(3) {
                    let a = i16::from(triple[1].1.pitch) - i16::from(triple[0].1.pitch);
                    let b = i16::from(triple[2].1.pitch) - i16::from(triple[1].1.pitch);
                    if a.abs() > i16::from(LARGE_JUMP) {
                        assert!(
                            (1..=2).contains(&b.abs()) && a.signum() != b.signum(),
                            "seed {seed}: {triple:?}"
                        );
                    }
                }
                let high = attacks.iter().map(|(_, n)| n.pitch).max().unwrap();
                assert_eq!(
                    attacks.iter().filter(|(_, n)| n.pitch == high).count(),
                    1,
                    "seed {seed}: repeated high point {attacks:?}"
                );
                let &(step, closing) = attacks.last().unwrap();
                assert!((56..=60).contains(&step));
                assert_eq!(closing.pitch, 60);
                assert!(closing.duration >= 240);
                assert_ne!(melody[28].unwrap().pitch, closing.pitch);
                let same = (0..16)
                    .filter(|&s| melody[s].map(|n| n.pitch) == melody[s + 32].map(|n| n.pitch))
                    .count();
                assert!(same >= 10, "seed {seed}: motif similarity {same}");
                rhythms.insert(melody[..16].iter().map(Option::is_some).collect::<Vec<_>>());
                contours.insert(
                    melody[..16]
                        .iter()
                        .flatten()
                        .map(|n| n.pitch)
                        .collect::<Vec<_>>(),
                );
            }
            assert_eq!(reached.len(), 7, "{mode:?}: {reached:?}");
        }
        assert!(degrees.len() > 4);
        assert!(rhythms.len() > 4 && contours.len() > 4);
    }
}
