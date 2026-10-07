use ym2149::{Ym2149, Ym2149Backend};

use crate::sequencer::MelodyStyle;

const MASTER_CLOCK: u32 = 2_000_000;
const KICK: usize = 0;
const SNARE: usize = 1;
const HAT: usize = 2;
const MELODY: usize = 3;

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
    chips: [Ym2149; 2],
    notes: [Note; 4],
    sample_rate: u32,
}

impl PsgEngine {
    pub fn new(sample_rate: u32) -> Self {
        // Separate the snare and hi-hat so they do not share a noise period.
        let chips = std::array::from_fn(|_| Ym2149::with_clocks(MASTER_CLOCK, sample_rate));
        let mut engine = Self {
            chips,
            notes: std::array::from_fn(|_| Note::default()),
            sample_rate,
        };
        engine.configure();
        engine
    }

    pub fn reset(&mut self) {
        // Clear active notes and restore register routing after a stop or randomize.
        self.chips.iter_mut().for_each(Ym2149::reset);
        self.notes = std::array::from_fn(|_| Note::default());
        self.configure();
    }

    fn configure(&mut self) {
        self.chips[0].write_register(6, 8);
        self.chips[0].write_register(7, 0x2c); // Kick tone A; snare tone and noise B.
        self.chips[1].write_register(6, 3);
        self.chips[1].write_register(7, 0x35); // Hi-hat noise A; melody tone B.
    }

    pub fn kick(&mut self) {
        self.notes[KICK].start(self.samples(0.35), 15);
    }

    pub fn snare(&mut self) {
        // A fixed tone adds body to the noise on the same channel.
        self.set_pitch(0, 1, 180.0);
        self.notes[SNARE].start(self.samples(0.25), 12);
    }

    pub fn hihat(&mut self, open: bool) {
        let duration = if open { 0.35 } else { 0.08 };
        self.notes[HAT].start(self.samples(duration), 10);
    }

    pub fn melody(&mut self, freq: f32, style: MelodyStyle) {
        self.set_pitch(1, 1, freq);
        let duration = match style {
            MelodyStyle::Pluck => 0.2,
            MelodyStyle::Sustain => 0.6,
        };
        self.notes[MELODY].start(self.samples(duration), 11);
    }

    pub fn next_sample(&mut self) -> f32 {
        // Update independent volume fades without using the shared chip envelope.
        for (i, note) in self.notes.iter_mut().enumerate() {
            let (chip, channel) = match i {
                KICK => (0, 0),
                SNARE => (0, 1),
                HAT => (1, 0),
                _ => (1, 1),
            };
            self.chips[chip].write_register(8 + channel, note.level());
        }

        // Drop the kick pitch over its first 80 ms.
        let kick = &self.notes[KICK];
        if kick.left > 0 {
            let age = (kick.length - kick.left) as f32 / self.sample_rate as f32;
            self.set_pitch(0, 0, 55.0 + 100.0 * (1.0 - age / 0.08).max(0.0));
        }

        // Average both chips before sending a mono sample to the output stream.
        self.chips
            .iter_mut()
            .map(|chip| {
                chip.clock();
                chip.get_sample()
            })
            .sum::<f32>()
            * 0.5
    }

    fn samples(&self, seconds: f32) -> u32 {
        (seconds * self.sample_rate as f32) as u32
    }

    fn set_pitch(&mut self, chip: usize, channel: u8, freq: f32) {
        let period = (MASTER_CLOCK as f32 / (16.0 * freq)).round() as u16;
        let period = period.clamp(1, 0x0fff);
        self.chips[chip].write_register(channel * 2, period as u8);
        self.chips[chip].write_register(channel * 2 + 1, (period >> 8) as u8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_chips_are_silent() {
        for rate in [44_100, 48_000, 96_000] {
            let mut engine = PsgEngine::new(rate);
            // The chip's DC filter leaves a small startup offset.
            assert!((0..1024).all(|_| engine.next_sample().abs() < 0.01));
        }
    }

    #[test]
    fn mixes_both_chips() {
        let mut one = PsgEngine::new(48_000);
        let mut both = PsgEngine::new(48_000);
        one.melody(440.0, MelodyStyle::Sustain);
        both.melody(440.0, MelodyStyle::Sustain);
        both.kick();

        // The kick on chip 0 must change the mix from chip 1 alone.
        let difference = (0..4096)
            .map(|_| (both.next_sample() - one.next_sample()).abs())
            .fold(0.0_f32, f32::max);
        assert!(difference > 0.01);
    }

    #[test]
    fn independent_tracks_and_pitch() {
        let mut engine = PsgEngine::new(48_000);
        engine.kick();
        engine.snare();
        engine.hihat(true);
        engine.melody(440.0, MelodyStyle::Sustain);
        engine.next_sample();

        assert!(engine.chips[0].read_register(8) > 0);
        assert!(engine.chips[0].read_register(9) > 0);
        assert!(engine.chips[1].read_register(8) > 0);
        assert!(engine.chips[1].read_register(9) > 0);
        let period = u16::from(engine.chips[1].read_register(2))
            | (u16::from(engine.chips[1].read_register(3)) << 8);
        assert_eq!(period, 284); // 2 MHz / (16 * 440 Hz).
    }

    #[test]
    fn every_instrument_produces_audio() {
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
    fn melody_period_is_device_independent() {
        for rate in [44_100, 48_000, 96_000] {
            let mut engine = PsgEngine::new(rate);
            engine.melody(440.0, MelodyStyle::Pluck);
            assert_eq!(engine.chips[1].read_register(2), 28);
            assert_eq!(engine.chips[1].read_register(3), 1);
        }
    }

    #[test]
    fn hi_hat_open_outlasts_closed() {
        let mut closed = PsgEngine::new(48_000);
        let mut open = PsgEngine::new(48_000);
        closed.hihat(false);
        open.hihat(true);
        for _ in 0..48_000 / 10 {
            closed.next_sample();
            open.next_sample();
        }

        assert_eq!(closed.chips[1].read_register(8), 0);
        assert!(open.chips[1].read_register(8) > 0);
    }

    #[test]
    fn reset_silences_tracks() {
        let mut engine = PsgEngine::new(48_000);
        engine.kick();
        engine.snare();
        engine.hihat(true);
        engine.melody(440.0, MelodyStyle::Sustain);
        engine.next_sample();
        engine.reset();
        engine.next_sample();

        for chip in &engine.chips {
            assert_eq!(chip.read_register(8), 0);
            assert_eq!(chip.read_register(9), 0);
        }
        assert_eq!(engine.chips[0].read_register(7), 0x2c);
        assert_eq!(engine.chips[1].read_register(7), 0x35);
    }

    #[test]
    fn retrigger_restores_volume() {
        let mut engine = PsgEngine::new(48_000);
        engine.melody(440.0, MelodyStyle::Pluck);
        for _ in 0..48_000 / 4 {
            engine.next_sample();
        }
        assert_eq!(engine.chips[1].read_register(9), 0);

        engine.melody(523.25, MelodyStyle::Pluck);
        engine.next_sample();
        assert!(engine.chips[1].read_register(9) > 0);
    }
}
