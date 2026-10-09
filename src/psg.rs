use ym2149::{Ym2149, Ym2149Backend};

use crate::sequencer::{Drum, MelodyStyle};

const MASTER_CLOCK: u32 = 1_789_773;
const MSX_PORT_MODE: u8 = 0x80;
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
    mixer: u8,
    notes: [Note; 3],
    drum: Option<Drum>,
    melody2: bool,
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
            mixer: MSX_PORT_MODE,
            notes: std::array::from_fn(|_| Note::default()),
            drum: None,
            melody2: false,
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
        self.melody2 = false;
        self.arp_age = 0;
        self.arp_phase = 0.0;
        self.bass_age = 0;
        self.bass_freq = 0.0;
        self.arp_notes = [0.0; 3];
        self.configure();
    }

    fn set_mixer(&mut self, sound: u8) {
        // Keep MSX port A as input and port B as output.
        self.mixer = MSX_PORT_MODE | (sound & 0x3f);
        self.chip.write_register(7, self.mixer);
    }

    fn configure(&mut self) {
        // All tone channels are enabled; noise starts disabled.
        self.set_mixer(0x38);
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
        self.set_mixer(match kind {
            Drum::Kick => 0x38,
            Drum::Snare => 0x18, // Snare combines tone and noise on C.
            _ => 0x1c,           // Hats use noise only on C.
        });
    }

    pub fn kick(&mut self) {
        self.drum(Drum::Kick, 0.35, 10);
    }

    pub fn snare(&mut self) {
        self.drum(Drum::Snare, 0.25, 8);
        if self.drum == Some(Drum::Snare) {
            self.set_pitch(BASS, 180.0);
        }
    }

    pub fn hihat(&mut self, open: bool) {
        let kind = if open { Drum::OpenHat } else { Drum::ClosedHat };
        self.drum(kind, if open { 0.35 } else { 0.08 }, 10);
    }

    pub fn melody(&mut self, freq: f32, style: MelodyStyle) {
        self.melody2 = false;
        self.set_pitch(MELODY, freq);
        let duration = match style {
            MelodyStyle::Pluck => 0.2,
            MelodyStyle::Sustain => 0.6,
        };
        self.notes[MELODY].start(self.samples(duration), 11);
    }

    pub fn melody2(&mut self, freq: f32) {
        // A soft attack and release give channel A a sustained variation.
        self.melody2 = true;
        self.set_pitch(MELODY, freq);
        self.notes[MELODY].start(self.samples(0.6), 8);
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
        // Disable tone C and use its stepped volume as a soft bass waveform.
        self.set_mixer(0x3c);
        self.bass_freq = freq;
        self.bass_age = 0;
        self.notes[BASS].start(self.samples(0.45), 9);
        self.set_pitch(BASS, freq);
    }

    pub fn mute(&mut self, track: usize) {
        // Stop only the selected voice; bass and drums borrow the same channel.
        let channel = match track {
            0 => Some(MELODY),
            1 => Some(ARPEGGIO),
            2 if self.drum.is_none() => Some(BASS),
            3 if self.drum.is_some() => {
                self.drum = None;
                self.set_mixer(0x38);
                Some(BASS)
            }
            _ => None,
        };
        if let Some(channel) = channel {
            self.notes[channel].left = 0;
            self.chip.write_register(8 + channel as u8, 0);
        }
    }

    pub fn active(&self) -> bool {
        self.notes.iter().any(|note| note.left > 0)
    }

    pub fn next_sample(&mut self) -> f32 {
        // Release the borrowed channel when the drum ends.
        if self.notes[BASS].left == 0 && self.drum.take().is_some() {
            self.set_mixer(0x38);
        }

        let mut levels = std::array::from_fn::<_, 3, _>(|i| self.notes[i].level());

        // M2 holds its level between a gentle attack and release.
        let melody = &self.notes[MELODY];
        if self.melody2 && melody.left > 0 {
            let age = melody.length - melody.left;
            let attack = age as f32 / self.samples(0.04).max(1) as f32;
            let release = melody.left as f32 / self.samples(0.12).max(1) as f32;
            levels[MELODY] = (melody.volume as f32 * attack.min(release).min(1.0)).round() as u8;
        }

        // Fast chord cycling suggests a chord on a single square-wave channel.
        if self.notes[ARPEGGIO].left > 0 {
            let slice = (self.sample_rate / 24).max(1);
            let index = (self.arp_age / slice) as usize % 3;
            if self.arp_age.is_multiple_of(slice) {
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

        // Hold the rounded bass at full level, then fade to avoid a click.
        if self.drum.is_none() && self.notes[BASS].left > 0 {
            let phase = (self.bass_age as f32 * self.bass_freq / self.sample_rate as f32).fract();
            let triangle = 1.0 - (2.0 * phase - 1.0).abs();
            let rounded = triangle * triangle * (3.0 - 2.0 * triangle);
            let release =
                (self.notes[BASS].left as f32 / self.samples(0.12).max(1) as f32).min(1.0);
            levels[BASS] = (self.notes[BASS].volume as f32 * rounded * release).round() as u8;
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
        assert_eq!(engine.mixer, 0xb8);
    }

    #[test]
    fn noise_uses_only_channel_c() {
        let mut engine = PsgEngine::new(48_000);
        engine.snare();
        assert_eq!(engine.chip.read_register(7), 0x18);
        assert_eq!(engine.mixer, 0x98);
        engine.kick();
        assert_eq!(engine.chip.read_register(7), 0x38);
        engine.hihat(false); // Lower-priority drum cannot interrupt the kick.
        assert_eq!(engine.drum, Some(Drum::Kick));
        let mut hat = PsgEngine::new(48_000);
        hat.hihat(false);
        assert_eq!(hat.chip.read_register(7), 0x1c);
        assert_eq!(hat.mixer, 0x9c);
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
        for _ in 1..100 {
            engine.next_sample();
        }
        assert!(engine.chip.read_register(10) > 0);
        for _ in 100..2001 {
            engine.next_sample();
        }
        assert_eq!(engine.chip.read_register(2), 83); // 329.63 Hz: period 339.
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
        for _ in 300..12_000 {
            engine.next_sample();
        }
        let sustained_peak = (0..400)
            .map(|_| {
                engine.next_sample();
                engine.chip.read_register(10)
            })
            .max()
            .unwrap();
        assert_eq!(sustained_peak, 9);
        assert_eq!(engine.chip.read_register(7), 0x3c);
    }

    #[test]
    fn drum_levels_are_lower() {
        let mut engine = PsgEngine::new(48_000);
        engine.kick();
        engine.next_sample();
        assert_eq!(engine.chip.read_register(10), 10);
        engine.snare(); // A lower-priority hit cannot interrupt the kick.
        assert_eq!(engine.drum, Some(Drum::Kick));
        engine.reset();
        engine.snare();
        engine.next_sample();
        assert_eq!(engine.chip.read_register(10), 8);
    }

    #[test]
    fn melody2_sustains_softly() {
        let mut engine = PsgEngine::new(48_000);
        engine.melody(440.0, MelodyStyle::Sustain);
        engine.melody2(440.0);
        engine.next_sample();
        assert_eq!(engine.chip.read_register(8), 0);
        assert_eq!(engine.chip.read_register(0), 254); // MSX clock: 440 Hz.
        assert_eq!(engine.chip.read_register(1), 0);

        for _ in 0..48_000 / 4 {
            engine.next_sample();
        }
        assert_eq!(engine.chip.read_register(8), 8);

        for _ in 0..48_000 / 3 {
            engine.next_sample();
        }
        assert!(engine.chip.read_register(8) < 8);

        engine.melody(440.0, MelodyStyle::Pluck);
        engine.next_sample();
        assert_eq!(engine.chip.read_register(8), 11);
    }

    #[test]
    fn mute_keeps_other_channels() {
        let mut engine = PsgEngine::new(48_000);
        engine.melody(440.0, MelodyStyle::Sustain);
        engine.arpeggio([261.63, 329.63, 392.0]);
        engine.bass(130.81);
        engine.mute(0);
        assert_eq!(engine.notes[MELODY].left, 0);
        assert!(engine.notes[ARPEGGIO].left > 0);
        assert!(engine.notes[BASS].left > 0);

        engine.kick();
        engine.mute(2); // Muting bass must not stop a drum.
        assert_eq!(engine.drum, Some(Drum::Kick));
        engine.mute(3);
        assert_eq!(engine.drum, None);
        assert_eq!(engine.chip.read_register(10), 0);
        assert!(engine.notes[ARPEGGIO].left > 0);

        engine.bass(130.81);
        engine.mute(3); // Muting drums must not stop a bass note.
        assert!(engine.notes[BASS].left > 0);
        engine.mute(2);
        assert_eq!(engine.notes[BASS].left, 0);
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
