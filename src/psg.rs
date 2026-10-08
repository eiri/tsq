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
    arp_notes: [f32; 3],
    arp_age: u32,
    arp_phase: f32,
    bass_freq: f32,
    bass_age: u32,
}

impl PsgEngine {
    pub fn new(sample_rate: u32) -> Self {
        let mut engine = Self {
            chip: Ym2149::with_clocks(MASTER_CLOCK, sample_rate),
            notes: std::array::from_fn(|_| Note::default()),
            drum: None,
            sample_rate,
            arp_notes: [0.0; 3],
            arp_age: 0,
            arp_phase: 0.0,
            bass_freq: 0.0,
            bass_age: 0,
        };
        engine.configure();
        engine
    }

    pub fn reset(&mut self) {
        self.chip.reset();
        self.notes = std::array::from_fn(|_| Note::default());
        self.drum = None;
        self.arp_age = 0;
        self.arp_phase = 0.0;
        self.bass_age = 0;
        self.bass_freq = 0.0;
        self.arp_notes = [0.0; 3];
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

    pub fn arpeggio(&mut self, notes: [f32; 3]) {
        self.arp_notes = notes;
        self.arp_age = 0;
        self.arp_phase = 0.0;
        self.notes[ARPEGGIO].start(self.samples(0.5), 9);
        self.set_pitch(ARPEGGIO, notes[0]);
    }

    pub fn bass(&mut self, freq: f32) {
        // Bass never cuts short a drum; the next bass note may try again.
        if self.drum.is_some() && self.notes[BASS].left > 0 {
            return;
        }
        self.drum = None;
        self.chip.write_register(7, 0x38);
        self.bass_freq = freq;
        self.bass_age = 0;
        self.notes[BASS].start(self.samples(0.45), 10);
        self.set_pitch(BASS, freq);
    }

    pub fn next_sample(&mut self) -> f32 {
        // Release the borrowed channel when the drum ends.
        if self.notes[BASS].left == 0 && self.drum.take().is_some() {
            self.chip.write_register(7, 0x38);
        }

        let mut levels = std::array::from_fn::<_, 3, _>(|i| self.notes[i].level());

        // Fast chord cycling suggests a chord on a single square-wave channel.
        if self.notes[ARPEGGIO].left > 0 {
            let slice = (self.sample_rate / 24).max(1);
            let index = (self.arp_age / slice) as usize % 3;
            if self.arp_age % slice == 0 {
                self.set_pitch(ARPEGGIO, self.arp_notes[index]);
                self.arp_phase = 0.0;
            }
            // Gate the chip's square tone at a changing duty (not native PWM).
            let sweep = (self.arp_age % (self.sample_rate / 5).max(1)) as f32
                / (self.sample_rate / 5).max(1) as f32;
            let duty = 0.25 + 0.5 * (1.0 - (2.0 * sweep - 1.0).abs());
            if self.arp_phase > duty {
                levels[ARPEGGIO] = 0;
            }
            self.arp_phase =
                (self.arp_phase + self.arp_notes[index] / self.sample_rate as f32).fract();
            self.arp_age += 1;
        }

        // Shape bass with stepped volume while retaining the chip's square tone.
        if self.drum.is_none() && self.notes[BASS].left > 0 {
            let phase = (self.bass_age as f32 * self.bass_freq / self.sample_rate as f32).fract();
            let triangle = 1.0 - (2.0 * phase - 1.0).abs();
            levels[BASS] = (levels[BASS] as f32 * (0.3 + 0.7 * triangle)) as u8;
            self.bass_age += 1;
        }
        for (channel, level) in levels.into_iter().enumerate() {
            self.chip.write_register(8 + channel as u8, level);
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
    fn arp_cycles_and_bass_borrows() {
        let mut engine = PsgEngine::new(48_000);
        engine.arpeggio([261.63, 329.63, 392.0]);
        engine.bass(130.81);
        engine.next_sample();
        assert!(engine.chip.read_register(9) > 0);
        assert!(engine.chip.read_register(10) > 0);
        for _ in 0..2000 {
            engine.next_sample();
        }
        assert_eq!(engine.chip.read_register(2), 123); // 329.63 Hz: period 379.
        assert_eq!(engine.chip.read_register(3), 1);
        engine.kick();
        engine.bass(196.0);
        assert_eq!(engine.drum, Some(Drum::Kick));
    }

    #[test]
    fn bass_volume_is_stepped() {
        let mut engine = PsgEngine::new(48_000);
        engine.bass(130.81);
        let levels: std::collections::HashSet<_> = (0..300)
            .map(|_| {
                engine.next_sample();
                engine.chip.read_register(10)
            })
            .collect();
        assert!(levels.len() > 2);
        assert_eq!(engine.chip.read_register(7), 0x38);
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
