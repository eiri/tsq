use ym2149::{Ym2149, Ym2149Backend};

use crate::sequencer::{Drum, MelodyStyle};

const MASTER_CLOCK: u32 = 2_000_000;
const MELODY: usize = 0;
const ARPEGGIO: usize = 1;
const BASS: usize = 2;

#[derive(Default)]
struct Note {
    left: u32,
    length: u32,
    volume: u8,
}

impl Note {
    fn start(&mut self, length: u32, volume: u8) {
        self.left = length.max(1);
        self.length = self.left;
        self.volume = volume;
    }

    fn level(&mut self) -> u8 {
        let level = self.volume as u32 * self.left / self.length.max(1);
        self.left = self.left.saturating_sub(1);
        level as u8
    }
}

pub struct PsgEngine {
    chip: Ym2149,
    notes: [Note; 3],
    drum: Option<Drum>,
    sample_rate: u32,
}

impl PsgEngine {
    pub fn new(sample_rate: u32) -> Self {
        let mut engine = Self {
            chip: Ym2149::with_clocks(MASTER_CLOCK, sample_rate),
            notes: std::array::from_fn(|_| Note::default()),
            drum: None,
            sample_rate,
        };
        engine.configure();
        engine
    }

    pub fn reset(&mut self) {
        self.chip.reset();
        self.notes = std::array::from_fn(|_| Note::default());
        self.drum = None;
        self.configure();
    }

    fn configure(&mut self) {
        // All tone channels are enabled; noise starts disabled.
        self.chip.write_register(7, 0x38);
        for channel in 0..3 {
            self.chip.write_register(8 + channel, 0);
        }
    }

    fn drum(&mut self, kind: Drum, duration: f32, volume: u8) {
        // A busy drum channel accepts only an equal or higher-priority hit.
        let priority = |drum| match drum {
            Drum::Kick => 3,
            Drum::Snare => 2,
            Drum::OpenHat | Drum::ClosedHat => 1,
        };
        if self.notes[BASS].left > 0 && self.drum.is_some_and(|d| priority(d) > priority(kind)) {
            return;
        }

        self.drum = Some(kind);
        self.notes[BASS].start(self.samples(duration), volume);
        self.chip.write_register(
            6,
            match kind {
                Drum::Kick => 8,
                Drum::Snare => 8,
                Drum::ClosedHat | Drum::OpenHat => 3,
            },
        );
        self.chip.write_register(
            7,
            match kind {
                Drum::Kick => 0x38,
                _ => 0x18, // Noise C enabled; other channels retain tone only.
            },
        );
    }

    pub fn kick(&mut self) {
        self.drum(Drum::Kick, 0.35, 15);
    }

    pub fn snare(&mut self) {
        self.drum(Drum::Snare, 0.25, 12);
        if self.drum == Some(Drum::Snare) {
            self.set_pitch(BASS, 180.0);
        }
    }

    pub fn hihat(&mut self, open: bool) {
        let kind = if open { Drum::OpenHat } else { Drum::ClosedHat };
        self.drum(kind, if open { 0.35 } else { 0.08 }, 10);
    }

    pub fn melody(&mut self, freq: f32, style: MelodyStyle) {
        self.set_pitch(MELODY, freq);
        let duration = match style {
            MelodyStyle::Pluck => 0.2,
            MelodyStyle::Sustain => 0.6,
        };
        self.notes[MELODY].start(self.samples(duration), 11);
    }

    pub fn next_sample(&mut self) -> f32 {
        // Release the borrowed channel when the drum ends.
        if self.notes[BASS].left == 0 && self.drum.take().is_some() {
            self.chip.write_register(7, 0x38);
        }

        for (channel, note) in self.notes.iter_mut().enumerate() {
            self.chip.write_register(8 + channel as u8, note.level());
        }

        // Drop the kick pitch over its first 80 ms.
        let kick = &self.notes[BASS];
        if self.drum == Some(Drum::Kick) && kick.left > 0 {
            let age = (kick.length - kick.left) as f32 / self.sample_rate as f32;
            self.set_pitch(BASS, 55.0 + 100.0 * (1.0 - age / 0.08).max(0.0));
        }

        self.chip.clock();
        self.chip.get_sample()
    }

    fn samples(&self, seconds: f32) -> u32 {
        (seconds * self.sample_rate as f32) as u32
    }

    fn set_pitch(&mut self, channel: usize, freq: f32) {
        let period = (MASTER_CLOCK as f32 / (16.0 * freq)).round() as u16;
        let period = period.clamp(1, 0x0fff);
        self.chip.write_register((channel * 2) as u8, period as u8);
        self.chip
            .write_register((channel * 2 + 1) as u8, (period >> 8) as u8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_chip_is_silent() {
        for rate in [44_100, 48_000, 96_000] {
            let mut engine = PsgEngine::new(rate);
            assert!((0..1024).all(|_| engine.next_sample().abs() < 0.01));
        }
    }

    #[test]
    fn channels_share_one_chip() {
        let mut engine = PsgEngine::new(48_000);
        engine.melody(440.0, MelodyStyle::Sustain);
        engine.kick();
        engine.next_sample();
        assert!(engine.chip.read_register(8) > 0);
        assert!(engine.chip.read_register(10) > 0);
        assert_eq!(engine.chip.read_register(9), 0);
        assert_eq!(engine.chip.read_register(7), 0x38);
    }

    #[test]
    fn noise_uses_only_channel_c() {
        let mut engine = PsgEngine::new(48_000);
        engine.snare();
        assert_eq!(engine.chip.read_register(7), 0x18);
        engine.kick();
        assert_eq!(engine.chip.read_register(7), 0x38);
        engine.hihat(false); // Lower-priority drum cannot interrupt the kick.
        assert_eq!(engine.drum, Some(Drum::Kick));
    }

    #[test]
    fn instruments_produce_audio() {
        for trigger in [
            PsgEngine::kick as fn(&mut PsgEngine),
            PsgEngine::snare,
            |engine| engine.hihat(false),
            |engine| engine.melody(440.0, MelodyStyle::Pluck),
        ] {
            let mut engine = PsgEngine::new(48_000);
            trigger(&mut engine);
            let peak = (0..4096)
                .map(|_| engine.next_sample().abs())
                .fold(0.0_f32, f32::max);
            assert!(peak > 0.01);
        }
    }

    #[test]
    fn reset_silences_channels() {
        let mut engine = PsgEngine::new(48_000);
        engine.melody(440.0, MelodyStyle::Sustain);
        engine.snare();
        engine.next_sample();
        engine.reset();
        engine.next_sample();
        for channel in 8..=10 {
            assert_eq!(engine.chip.read_register(channel), 0);
        }
        assert_eq!(engine.chip.read_register(7), 0x38);
    }

    #[test]
    fn retrigger_restores_volume() {
        let mut engine = PsgEngine::new(48_000);
        engine.melody(440.0, MelodyStyle::Pluck);
        for _ in 0..48_000 / 4 {
            engine.next_sample();
        }
        assert_eq!(engine.chip.read_register(8), 0);
        engine.melody(523.25, MelodyStyle::Pluck);
        engine.next_sample();
        assert!(engine.chip.read_register(8) > 0);
    }
}
