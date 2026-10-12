//! Harmony is shared by every pitched supporting track.
use crate::music::{ARP_GATE, ToneStep};
use crate::music::{BAR_UNITS, BARS, Interval, LOOP_UNITS, MelodyInfo, STEP_UNITS, beat_weight};
use crate::sequencer::{
    ArpShape, DrumStep, Groove, Hat, MelodyStep, Mode, Pattern, STEPS, groove_steps,
};

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
    #[cfg(test)]
    pub fn from_degrees(tonic: u8, mode: Mode, roots: [u8; BARS]) -> Self {
        Self {
            chords: roots.map(|root| degree_chord(tonic, mode, root)),
        }
    }

    pub fn preset() -> Self {
        let major = |root| Chord {
            root,
            quality: Quality::Major,
        };
        Self {
            chords: [
                major(0),
                major(10),
                major(7),
                major(3),
                major(3),
                major(0),
                Chord {
                    root: 2,
                    quality: Quality::Minor,
                },
                major(7),
            ],
        }
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
    if notes.len() < 2 {
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
    let preparation =
        classes.contains(&(before.pitch % 12)) && before.end % LOOP_UNITS == note.start;
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

pub const ARP_LOW: u8 = 48;
pub const ARP_HIGH: u8 = 79;
const PLACEMENTS: usize = 6;
const PLACEMENT_MARGIN: i32 = 12;

fn arp_line(placement: [u8; 3], shape: ArpShape) -> [u8; 6] {
    let [a, b, c] = placement;
    match shape {
        ArpShape::Classic => [a, b, c, c, b, a],
        ArpShape::Up => [a, b, c, a, b, c],
        ArpShape::Down => [c, b, a, c, b, a],
        ArpShape::Repeat => [a, c, b, c, a, c],
    }
}

fn placements(chord: Chord, info: &MelodyInfo, bar: usize) -> Vec<[u8; 3]> {
    let classes = chord.classes();
    let mut options = Vec::new();
    for low in ARP_LOW..=ARP_HIGH - 7 {
        if !classes.contains(&(low % 12)) {
            continue;
        }
        let upper: Vec<_> = (low + 1..=ARP_HIGH)
            .filter(|p| classes.contains(&(p % 12)))
            .take(2)
            .collect();
        if upper.len() == 2 {
            options.push([low, upper[0], upper[1]]);
        }
    }
    let register = info.bars[bar].low.unwrap_or(72);
    options.sort_by_key(|p| {
        (
            p[2].saturating_sub(register) * 2 + p[0].abs_diff(register.saturating_sub(12)),
            *p,
        )
    });
    options.truncate(PLACEMENTS);
    options.sort();
    options
}

fn arp_mask(info: &MelodyInfo, bar: usize, rng: &mut fastrand::Rng) -> Vec<usize> {
    let summary = info.bars[bar];
    let target = if summary.attacks >= 5 {
        2
    } else if summary.attacks >= 3 {
        rng.usize(3..5)
    } else {
        rng.usize(4..7)
    };
    let mut steps: Vec<_> = (0..8).collect();
    let jitter: [u8; 8] = std::array::from_fn(|_| rng.u8(0..3));
    // Prefer gaps and quiet offbeats, leaving room under busy lead passages.
    steps.sort_by_key(|&s| {
        let sounding = info.sounding((bar * 8 + s) as u16 * STEP_UNITS).is_some();
        let accent = summary.accents & (1 << s) != 0;
        (u8::from(sounding) * 4 + u8::from(accent) * 4 + jitter[s], s)
    });
    steps.truncate(target);
    steps.sort();
    steps
}

fn clash(info: &MelodyInfo, pitch: u8, units: u16) -> bool {
    info.intervals.iter().any(|i| {
        i.end > units
            && i.start < units + ARP_GATE
            && i.pitch.abs_diff(pitch) <= 2
            && i.pitch != pitch
            && i.end - i.start >= STEP_UNITS
    })
}

fn placement_cost(
    info: &MelodyInfo,
    bar: usize,
    pitches: [u8; 3],
    mask: &[usize],
    shape: ArpShape,
) -> i32 {
    let line = arp_line(pitches, shape);
    let summary = info.bars[bar];
    let register = summary.low.unwrap_or(72);
    let top = summary.high.unwrap_or(84);
    let mut cost = i32::from(pitches[2].saturating_sub(register)) * 2;
    // Close conflicts cost more than soft register crossings.
    for (index, &step) in mask.iter().enumerate() {
        let pitch = line[index];
        if clash(info, pitch, (bar * 8 + step) as u16 * STEP_UNITS) {
            cost += 40;
        }
        cost += i32::from(pitch.saturating_sub(top));
    }
    cost
}

fn arp_movement(from: [u8; 3], to: [u8; 3], from_mask: &[usize], shape: ArpShape) -> i32 {
    let placement: i32 = from
        .iter()
        .zip(to)
        .map(|(&a, b)| i32::from(a.abs_diff(b)))
        .sum();
    let a = arp_line(from, shape);
    let b = arp_line(to, shape);
    placement + i32::from(a[from_mask.len() - 1].abs_diff(b[0])) * 2
}

pub fn render_arpeggio(pattern: &mut Pattern, info: &MelodyInfo, rng: &mut fastrand::Rng) {
    let shape = match rng.usize(0..4) {
        0 => ArpShape::Classic,
        1 => ArpShape::Up,
        2 => ArpShape::Down,
        _ => ArpShape::Repeat,
    };
    let options: Vec<_> = (0..BARS)
        .map(|b| placements(pattern.harmony.chords[b], info, b))
        .collect();
    let masks: Vec<_> = (0..BARS).map(|b| arp_mask(info, b, rng)).collect();
    let costs: Vec<Vec<_>> = (0..BARS)
        .map(|b| {
            options[b]
                .iter()
                .map(|&p| placement_cost(info, b, p, &masks[b], shape))
                .collect()
        })
        .collect();
    let mut paths = Vec::new();
    // Find smooth placements across the loop, including the rendered line boundary.
    for first in 0..options[0].len() {
        let mut states: Vec<Option<Path>> = vec![None; options[0].len()];
        states[first] = Some(Path {
            score: -costs[0][first],
            slots: vec![first],
        });
        for bar in 1..BARS {
            let mut next = vec![None; options[bar].len()];
            for (to, state) in next.iter_mut().enumerate() {
                for (from, previous) in states.iter().enumerate() {
                    let Some(previous) = previous else {
                        continue;
                    };
                    let score = previous.score
                        - costs[bar][to]
                        - arp_movement(
                            options[bar - 1][from],
                            options[bar][to],
                            &masks[bar - 1],
                            shape,
                        );
                    if state.as_ref().is_none_or(|p: &Path| score > p.score) {
                        let mut slots = previous.slots.clone();
                        slots.push(to);
                        *state = Some(Path { score, slots });
                    }
                }
            }
            states = next;
        }
        for (last, path) in states.into_iter().enumerate() {
            if let Some(mut path) = path {
                path.score -= arp_movement(
                    options[BARS - 1][last],
                    options[0][first],
                    &masks[BARS - 1],
                    shape,
                );
                paths.push(path);
            }
        }
    }
    retain(&mut paths, COMPLETE_PATHS);
    let best = paths[0].score;
    paths.retain(|p| p.score >= best - PLACEMENT_MARGIN);
    let chosen = &paths[rng.usize(0..paths.len())];
    pattern.arpeggio = [None; STEPS];
    for bar in 0..BARS {
        let line = arp_line(options[bar][chosen.slots[bar]], shape);
        for (i, &step) in masks[bar].iter().enumerate() {
            let units = (bar * 8 + step) as u16 * STEP_UNITS;
            if !clash(info, line[i], units) {
                pattern.arpeggio[bar * 8 + step] = Some(ToneStep::new(line[i], ARP_GATE));
            }
        }
    }
}

pub const BASS_LOW: u8 = 28;
pub const BASS_HIGH: u8 = 55;
const BASS_GATE: u16 = 32;
const APPROACH_GATE: u16 = 30;

struct GroovePlan {
    family: Groove,
    dense: bool,
    empty: bool,
    fills: [bool; BARS],
    variation: [u8; 4],
}

pub fn render_drums(pattern: &mut Pattern, info: &MelodyInfo, rng: &mut fastrand::Rng) {
    let attacks: usize = info.bars.iter().map(|b| b.attacks as usize).sum();
    let accent_count: u32 = info.bars.iter().map(|b| b.accents.count_ones()).sum();
    let choice = (rng.usize(0..4) + usize::from(attacks > 32) + usize::from(accent_count > 16)) % 4;
    let family = if attacks == 0 {
        Groove::Backbeat
    } else {
        match choice {
            0 => Groove::Backbeat,
            1 => Groove::FourFloor,
            2 => Groove::HalfTime,
            _ => Groove::Syncopated,
        }
    };
    let plan = GroovePlan {
        family,
        dense: attacks > 32,
        empty: attacks == 0,
        fills: std::array::from_fn(|b| {
            info.bars[b].ending && info.bars[b].attacks <= 3 && rng.bool()
        }),
        variation: std::array::from_fn(|_| rng.u8(..)),
    };
    let template = groove_steps(plan.family);
    pattern.drums = [DrumStep::default(); STEPS];
    for bar in 0..BARS {
        let summary = info.bars[bar];
        let variation = plan.variation[bar % 4];
        for (step, &anchor) in template.iter().enumerate() {
            let mut hit = anchor;
            // Keep the template's kick/snare anchors; hats and extra kicks leave lead space.
            if plan.empty || plan.dense || summary.coverage > BAR_UNITS - STEP_UNITS {
                if step % 2 == 1 {
                    hit.hat = None;
                }
            } else if step % 2 == 1 && variation & (1 << (step / 2)) == 0 {
                hit.hat = None;
            }
            if !plan.empty
                && summary.accents & (1 << step) != 0
                && step % 2 == 0
                && variation & 16 != 0
                && summary.attacks <= 4
            {
                hit.kick = true;
            }
            if plan.fills[bar] && step >= 6 {
                hit.snare = true;
                hit.hat = Some(if step == 7 { Hat::Open } else { Hat::Closed });
            }
            pattern.drums[bar * 8 + step] = hit;
        }
    }
}

fn bass_roots(harmony: HarmonyPlan, rng: &mut fastrand::Rng) -> [u8; BARS] {
    let options: Vec<Vec<_>> = harmony
        .chords
        .iter()
        .map(|c| {
            (BASS_LOW..=BASS_HIGH)
                .filter(|p| p % 12 == c.root)
                .collect()
        })
        .collect();
    let mut paths = Vec::new();
    for first in 0..options[0].len() {
        let mut states: Vec<Option<Path>> = vec![None; options[0].len()];
        states[first] = Some(Path {
            score: -i32::from(options[0][first].abs_diff(40)),
            slots: vec![first],
        });
        for bar in 1..BARS {
            let mut next = vec![None; options[bar].len()];
            for (to, state) in next.iter_mut().enumerate() {
                for (from, previous) in states.iter().enumerate() {
                    let Some(previous) = previous else {
                        continue;
                    };
                    let score = previous.score
                        - i32::from(options[bar - 1][from].abs_diff(options[bar][to]));
                    if state.as_ref().is_none_or(|p: &Path| score > p.score) {
                        let mut slots = previous.slots.clone();
                        slots.push(to);
                        *state = Some(Path { score, slots });
                    }
                }
            }
            states = next;
        }
        for (last, path) in states.into_iter().enumerate() {
            if let Some(mut path) = path {
                path.score -= i32::from(options[BARS - 1][last].abs_diff(options[0][first]));
                paths.push(path);
            }
        }
    }
    retain(&mut paths, COMPLETE_PATHS);
    let best = paths[0].score;
    paths.retain(|p| p.score >= best - 4);
    let path = &paths[rng.usize(0..paths.len())];
    std::array::from_fn(|b| options[b][path.slots[b]])
}

pub fn render_bass(pattern: &mut Pattern, info: &MelodyInfo, rng: &mut fastrand::Rng) {
    let roots = bass_roots(pattern.harmony, rng);
    pattern.bass = [None; STEPS];
    for bar in 0..BARS {
        let root = roots[bar];
        let chord = pattern.harmony.chords[bar];
        let summary = info.bars[bar];
        for step in 0..8 {
            let kick = pattern.drums[bar * 8 + step].kick;
            let important = step == 0 || (step == 4 && kick);
            let gap = info
                .sounding((bar * 8 + step) as u16 * STEP_UNITS)
                .is_none();
            let extra = kick && summary.attacks <= 4 && rng.bool();
            if important
                || extra
                || (gap && step == 6 && summary.attacks > 0 && rng.usize(0..4) == 0)
            {
                let mut pitch = root;
                if !important && rng.usize(0..3) == 0 {
                    pitch = (BASS_LOW..=BASS_HIGH)
                        .filter(|p| p % 12 == chord.classes()[2])
                        .min_by_key(|p| p.abs_diff(root))
                        .unwrap();
                }
                pattern.bass[bar * 8 + step] = Some(ToneStep::new(pitch, BASS_GATE));
            }
        }
        let next = (bar + 1) % BARS;
        // Short approaches resolve to the actual next downbeat, including the loop boundary.
        if chord != pattern.harmony.chords[next] && summary.attacks <= 3 && rng.usize(0..4) == 0 {
            let target = roots[next];
            let approach = if target > root {
                target.saturating_sub(1)
            } else {
                target + 1
            };
            if (BASS_LOW..=BASS_HIGH).contains(&approach) {
                pattern.bass[bar * 8 + 7] = Some(ToneStep::new(approach, APPROACH_GATE));
            }
        }
    }
}

pub fn arrange(pattern: &mut Pattern) {
    // One analysis feeds all stages; each stage owns its random stream.
    let info = MelodyInfo::new(&pattern.melody);
    let seeds = pattern.seeds;
    pattern.harmony = choose_harmony(
        &info,
        pattern.tonic,
        pattern.mode,
        &mut fastrand::Rng::with_seed(seeds.harmony),
    );
    render_arpeggio(
        pattern,
        &info,
        &mut fastrand::Rng::with_seed(seeds.arpeggio),
    );
    render_drums(pattern, &info, &mut fastrand::Rng::with_seed(seeds.groove));
    render_bass(pattern, &info, &mut fastrand::Rng::with_seed(seeds.bass));
}

#[cfg_attr(not(test), expect(dead_code))]
pub fn with_melody(pattern: &Pattern, melody: [Option<MelodyStep>; STEPS]) -> Pattern {
    // A future editor can publish this complete snapshot without holding an audio lock.
    let mut next = pattern.clone();
    next.melody = melody.map(|note| {
        note.map(|mut note| {
            note.pitch = note.pitch.min(127);
            note.duration = crate::music::duration(note.duration);
            note
        })
    });
    arrange(&mut next);
    next
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
    fn groove_anchors_and_fills() {
        let sparse = info(&[(24, 72, 480), (56, 72, 480)]);
        let dense = info(&[
            (24, 72, 60),
            (26, 72, 60),
            (28, 72, 60),
            (30, 72, 60),
            (31, 72, 480),
            (56, 72, 60),
            (58, 72, 60),
            (60, 72, 60),
            (62, 72, 60),
            (63, 72, 480),
        ]);
        let mut sparse_fills = 0;
        let mut dense_fills = 0;
        for seed in 0..32 {
            let mut a = Pattern::default();
            let mut b = a.clone();
            render_drums(&mut a, &sparse, &mut fastrand::Rng::with_seed(seed));
            render_drums(&mut b, &dense, &mut fastrand::Rng::with_seed(seed));
            for bar in 0..BARS {
                assert!(a.drums[bar * 8].kick && b.drums[bar * 8].kick);
                if !sparse.bars[bar].ending {
                    for step in [0, 2, 4, 6] {
                        assert_eq!(a.drums[step].snare, a.drums[bar * 8 + step].snare);
                    }
                }
            }
            sparse_fills += usize::from(a.drums[31].snare) + usize::from(a.drums[63].snare);
            dense_fills += usize::from(b.drums[31].snare) + usize::from(b.drums[63].snare);
        }
        assert!(sparse_fills > dense_fills);
        let empty = info(&[]);
        let mut fallback = Pattern::default();
        render_drums(&mut fallback, &empty, &mut fastrand::Rng::with_seed(1));
        assert!(fallback.drums.chunks(8).all(|b| b == &fallback.drums[..8]));
        assert!(fallback.drums.iter().filter(|d| d.hat.is_some()).count() <= 32);
    }

    #[test]
    fn accents_affect_groove() {
        let a = info(&[(0, 72, 60), (2, 72, 60)]);
        let b = info(&[(0, 72, 60), (3, 72, 60)]);
        let mut changed = false;
        for seed in 0..32 {
            let mut x = Pattern::default();
            let mut y = x.clone();
            render_drums(&mut x, &a, &mut fastrand::Rng::with_seed(seed));
            render_drums(&mut y, &b, &mut fastrand::Rng::with_seed(seed));
            changed |= x.drums[2] != y.drums[2];
        }
        assert!(changed);
    }

    #[test]
    fn bass_reads_existing_drums() {
        let info = info(&[]);
        let mut pattern = Pattern {
            harmony: HarmonyPlan::from_degrees(0, Mode::Major, [0, 4, 0, 4, 0, 4, 0, 4]),
            drums: [DrumStep::default(); STEPS],
            ..Pattern::default()
        };
        for bar in 0..BARS {
            pattern.drums[bar * 8].kick = true;
        }
        let drums = pattern.drums;
        let mut approaches = 0;
        for seed in 0..32 {
            render_bass(&mut pattern, &info, &mut fastrand::Rng::with_seed(seed));
            assert_eq!(pattern.drums, drums);
            let mut repeated = pattern.clone();
            render_bass(&mut repeated, &info, &mut fastrand::Rng::with_seed(seed));
            assert_eq!(pattern.bass, repeated.bass);
            for bar in 0..BARS {
                assert_eq!(
                    pattern.bass[bar * 8].unwrap().pitch % 12,
                    pattern.harmony.chords[bar].root
                );
                assert!(pattern.bass[bar * 8 + 4].is_none());
                if let Some(approach) = pattern.bass[bar * 8 + 7] {
                    let target = pattern.bass[((bar + 1) % BARS) * 8].unwrap();
                    assert_eq!(approach.duration, APPROACH_GATE);
                    assert_eq!(approach.pitch.abs_diff(target.pitch), 1);
                    approaches += 1;
                }
            }
            assert!(
                pattern
                    .bass
                    .iter()
                    .flatten()
                    .all(|n| (BASS_LOW..=BASS_HIGH).contains(&n.pitch))
            );
        }
        assert!(approaches > 0);
    }

    #[test]
    fn arpeggio_supports_register() {
        for lead in [30, 61, 84, 110] {
            let mut pattern = Pattern {
                melody: [None; STEPS],
                ..Pattern::default()
            };
            pattern.melody[0] = Some(MelodyStep {
                pitch: lead,
                duration: LOOP_UNITS,
                voice: MelodyVoice::M1,
                bend: false,
            });
            let info = MelodyInfo::new(&pattern.melody);
            render_arpeggio(&mut pattern, &info, &mut fastrand::Rng::with_seed(42));
            let mut repeated = pattern.clone();
            render_arpeggio(&mut repeated, &info, &mut fastrand::Rng::with_seed(42));
            assert_eq!(pattern.arpeggio, repeated.arpeggio);
            for (step, note) in pattern
                .arpeggio
                .iter()
                .enumerate()
                .filter_map(|(s, n)| n.map(|n| (s, n)))
            {
                assert!((ARP_LOW..=ARP_HIGH).contains(&note.pitch));
                assert!(
                    pattern.harmony.chords[step / 8]
                        .classes()
                        .contains(&(note.pitch % 12))
                );
                assert!(!clash(&info, note.pitch, step as u16 * STEP_UNITS));
            }
            assert!(pattern.arpeggio.iter().flatten().count() > 0);
        }
    }

    #[test]
    fn dense_lead_gets_less_arpeggio() {
        let mut sparse = Pattern {
            melody: [None; STEPS],
            harmony: HarmonyPlan::from_degrees(0, Mode::Major, [0; BARS]),
            ..Pattern::default()
        };
        let mut dense = sparse.clone();
        dense.melody.fill(Some(MelodyStep {
            pitch: 72,
            duration: 60,
            voice: MelodyVoice::M1,
            bend: false,
        }));
        render_arpeggio(
            &mut sparse,
            &MelodyInfo::new(&[None; STEPS]),
            &mut fastrand::Rng::with_seed(7),
        );
        let info = MelodyInfo::new(&dense.melody);
        render_arpeggio(&mut dense, &info, &mut fastrand::Rng::with_seed(7));
        assert!(dense.arpeggio.iter().flatten().count() < sparse.arpeggio.iter().flatten().count());
        assert!(
            dense
                .arpeggio
                .chunks(8)
                .all(|b| b.iter().flatten().count() <= 2)
        );
        let smooth = arp_movement([48, 52, 55], [48, 52, 55], &[0, 2, 4, 6], ArpShape::Classic);
        let jumpy = arp_movement([48, 52, 55], [72, 76, 79], &[0, 2, 4, 6], ArpShape::Classic);
        assert!(smooth < jumpy);
        assert!(placements(degree_chord(0, Mode::Major, 0), &info, 0).len() <= PLACEMENTS);
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
