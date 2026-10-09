use ym2149::{Ym2149, Ym2149Backend};

use crate::sequencer::{Drum, MelodyStyle};

const MASTER_CLOCK: u32 = 1_789_773;
const PLAYER_HZ: u32 = 60;
const MSX_PORT_MODE: u8 = 0x80;
const MELODY: usize = 0;
const ARPEGGIO: usize = 1;
const BASS: usize = 2;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Voice {
    #[default]
    Off,
    Pluck,
    Sustain,
    Melody2,
    Arpeggio,
    Bass,
    Kick,
    Snare,
    ClosedHat,
    OpenHat,
}

#[derive(Clone, Copy, Default)]
struct Note {
    voice: Voice,
    age: u32,
    length: u32,
    freq: f32,
}

impl Note {
    fn start(&mut self, voice: Voice, length: u32, freq: f32) {
        *self = Self {
            voice,
            age: 0,
            length,
            freq,
        };
    }

    fn active(&self) -> bool {
        self.age < self.length
    }

    fn level(&self) -> u8 {
        let left = self.length - self.age;
        let fade = |volume: u32| (volume * left / self.length) as u8;
        let release = |volume: u32| (volume * left.min(7) / 7) as u8;

        match self.voice {
            Voice::Off => 0,
            Voice::Pluck => fade(11),
            Voice::Sustain => fade(11),
            Voice::Melody2 => ((8 * (self.age + 1).min(3) / 3) as u8).min(release(8)),
            Voice::Arpeggio => fade(9),
            Voice::Bass => release(9),
            Voice::Kick => fade(10),
            Voice::Snare => fade(8),
            Voice::ClosedHat | Voice::OpenHat => fade(10),
        }
    }
}

pub struct PsgEngine {
    chip: Ym2149,
    mixer: u8,
    notes: [Note; 3],
    drum: Option<Drum>,
    sample_rate: u32,
    tick_phase: u32,
    tick_pending: bool,
    arp_notes: [f32; 3],
}

impl PsgEngine {
    pub fn new(sample_rate: u32) -> Self {
        let mut engine = Self {
            chip: Ym2149::with_clocks(MASTER_CLOCK, sample_rate),
            mixer: MSX_PORT_MODE,
            notes: [Note::default(); 3],
            drum: None,
            sample_rate,
            tick_phase: 0,
            tick_pending: true,
            arp_notes: [0.0; 3],
        };
        engine.configure();
        engine
    }

    pub fn reset(&mut self) {
        self.chip.reset();
        self.notes = [Note::default(); 3];
        self.drum = None;
        self.tick_phase = 0;
        self.tick_pending = true;
        self.arp_notes = [0.0; 3];
        self.configure();
    }

    fn write_reg(&mut self, register: u8, value: u8) {
        if self.chip.read_register(register) != value {
            self.chip.write_register(register, value);
        }
    }

    fn set_mixer(&mut self, sound: u8) {
        // Keep MSX port A as input and port B as output.
        self.mixer = MSX_PORT_MODE | (sound & 0x3f);
        self.write_reg(7, sound & 0x3f);
    }

    fn configure(&mut self) {
        // Start with tones on and noise off; silence every channel.
        self.set_mixer(0x38);
        for channel in 8..=10 {
            self.write_reg(channel, 0);
        }
    }

    fn start(&mut self, channel: usize, voice: Voice, ticks: u32, freq: f32) {
        self.notes[channel].start(voice, ticks, freq);
        self.tick_pending = true;
    }

    fn drum(&mut self, kind: Drum) {
        // Drums can interrupt lower-priority hits, but never the reverse.
        let priority = |drum| match drum {
            Drum::Kick => 3,
            Drum::Snare => 2,
            Drum::OpenHat | Drum::ClosedHat => 1,
        };
        if self.notes[BASS].active() && self.drum.is_some_and(|d| priority(d) > priority(kind)) {
            return;
        }

        self.drum = Some(kind);
        let (voice, ticks, noise, mixer) = match kind {
            Drum::Kick => (Voice::Kick, 21, 8, 0x38),
            Drum::Snare => (Voice::Snare, 15, 8, 0x18),
            Drum::ClosedHat => (Voice::ClosedHat, 5, 3, 0x1c),
            Drum::OpenHat => (Voice::OpenHat, 21, 3, 0x1c),
        };
        self.start(BASS, voice, ticks, 0.0);
        self.write_reg(6, noise);
        self.set_mixer(mixer);
    }

    pub fn kick(&mut self) {
        self.drum(Drum::Kick);
    }

    pub fn snare(&mut self) {
        self.drum(Drum::Snare);
    }

    pub fn hihat(&mut self, open: bool) {
        self.drum(if open { Drum::OpenHat } else { Drum::ClosedHat });
    }

    pub fn melody(&mut self, freq: f32, style: MelodyStyle) {
        let (voice, ticks) = match style {
            MelodyStyle::Pluck => (Voice::Pluck, 12),
            MelodyStyle::Sustain => (Voice::Sustain, 36),
        };
        self.start(MELODY, voice, ticks, freq);
    }

    pub fn melody2(&mut self, freq: f32) {
        self.start(MELODY, Voice::Melody2, 36, freq);
    }

    pub fn arpeggio(&mut self, notes: [f32; 3]) {
        self.arp_notes = notes;
        self.start(ARPEGGIO, Voice::Arpeggio, 30, notes[0]);
    }

    pub fn bass(&mut self, freq: f32) {
        // A drum owns channel C until it finishes.
        if self.drum.is_some() && self.notes[BASS].active() {
            return;
        }
        self.drum = None;
        self.set_mixer(0x38);
        self.start(BASS, Voice::Bass, 27, freq);
    }

    pub fn mute(&mut self, track: usize) {
        // Bass and drums borrow the same channel; mute only its current owner.
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
            self.notes[channel] = Note::default();
            self.write_reg(8 + channel as u8, 0);
        }
    }

    pub fn active(&self) -> bool {
        self.notes.iter().any(Note::active)
    }

    fn tick(&mut self) {
        // Advance instrument sequences; the chip keeps sounding between ticks.
        for channel in 0..3 {
            let note = self.notes[channel];
            if !note.active() {
                if channel == BASS && self.drum.take().is_some() {
                    self.set_mixer(0x38);
                }
                self.write_reg(8 + channel as u8, 0);
                continue;
            }

            let pitch = match note.voice {
                Voice::Arpeggio => self.arp_notes[(note.age / 3) as usize % 3],
                Voice::Kick => 55.0 + 100.0 * (5 - note.age.min(5)) as f32 / 5.0,
                Voice::Snare => 180.0,
                Voice::ClosedHat | Voice::OpenHat => 0.0,
                _ => note.freq,
            };
            if pitch > 0.0 {
                self.set_pitch(channel, pitch);
            }
            self.write_reg(8 + channel as u8, note.level());
            self.notes[channel].age += 1;

            // Silence expired notes before the audio callback goes idle.
            if !self.notes[channel].active() {
                self.write_reg(8 + channel as u8, 0);
                if channel == BASS && self.drum.take().is_some() {
                    self.set_mixer(0x38);
                }
            }
        }
    }

    pub fn next_sample(&mut self) -> f32 {
        if self.tick_pending {
            self.tick();
            self.tick_pending = false;
        }

        self.chip.clock();
        let sample = self.chip.get_sample();

        // A fractional sample clock keeps the player at 60 Hz at any audio rate.
        self.tick_phase += PLAYER_HZ;
        if self.tick_phase >= self.sample_rate {
            self.tick_phase -= self.sample_rate;
            self.tick_pending = true;
        }
        sample
    }

    fn set_pitch(&mut self, channel: usize, freq: f32) {
        let period = (MASTER_CLOCK as f32 / (16.0 * freq)).round() as u16;
        let period = period.clamp(1, 0x0fff);
        self.write_reg((channel * 2) as u8, period as u8);
        self.write_reg((channel * 2 + 1) as u8, (period >> 8) as u8);
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
        assert_eq!(engine.mixer, 0xb8);
    }

    #[test]
    fn noise_uses_only_channel_c() {
        let mut engine = PsgEngine::new(48_000);
        engine.snare();
        assert_eq!(engine.mixer, 0x98);
        engine.kick();
        assert_eq!(engine.mixer, 0xb8);
        engine.hihat(false);
        assert_eq!(engine.drum, Some(Drum::Kick));
        let mut hat = PsgEngine::new(48_000);
        hat.hihat(false);
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
        assert!(engine.chip.read_register(10) > 0);
        for _ in 1..2401 {
            engine.next_sample();
        }
        assert_eq!(engine.chip.read_register(2), 83);
        assert_eq!(engine.chip.read_register(3), 1);
        engine.kick();
        engine.bass(196.0);
        assert_eq!(engine.drum, Some(Drum::Kick));
    }

    #[test]
    fn bass_is_a_tone() {
        let mut engine = PsgEngine::new(48_000);
        engine.bass(130.81);
        engine.next_sample();
        assert_eq!(engine.mixer, 0xb8);
        assert_eq!(engine.chip.read_register(10), 9);
        for _ in 1..21_600 {
            engine.next_sample();
        }
        assert!(!engine.active());
        assert_eq!(engine.chip.read_register(10), 0);
    }

    #[test]
    fn drum_levels_are_lower() {
        let mut engine = PsgEngine::new(48_000);
        engine.kick();
        engine.next_sample();
        assert_eq!(engine.chip.read_register(10), 10);
        engine.snare();
        assert_eq!(engine.drum, Some(Drum::Kick));
        engine.reset();
        engine.snare();
        engine.next_sample();
        assert_eq!(engine.chip.read_register(10), 8);
    }

    #[test]
    fn melody2_sustains_softly() {
        let mut engine = PsgEngine::new(48_000);
        engine.melody2(440.0);
        engine.next_sample();
        assert_eq!(engine.chip.read_register(8), 2);
        assert_eq!(engine.chip.read_register(0), 254);
        for _ in 1..12_000 {
            engine.next_sample();
        }
        assert_eq!(engine.chip.read_register(8), 8);
        for _ in 12_000..28_000 {
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
        assert!(!engine.notes[MELODY].active());
        assert!(engine.notes[ARPEGGIO].active());
        assert!(engine.notes[BASS].active());

        engine.kick();
        engine.mute(2);
        assert_eq!(engine.drum, Some(Drum::Kick));
        engine.mute(3);
        assert_eq!(engine.drum, None);
        assert_eq!(engine.chip.read_register(10), 0);
        assert!(engine.notes[ARPEGGIO].active());

        engine.bass(130.81);
        engine.mute(3);
        assert!(engine.notes[BASS].active());
        engine.mute(2);
        assert!(!engine.notes[BASS].active());
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
        assert_eq!(engine.mixer, 0xb8);
    }

    #[test]
    fn ticks_keep_time_at_any_audio_rate() {
        for rate in [44_100, 44_117, 48_000, 96_000] {
            let mut engine = PsgEngine::new(rate);
            engine.melody(440.0, MelodyStyle::Sustain);
            engine.next_sample();
            assert_eq!(engine.notes[MELODY].age, 1);

            let mut sample = 0;
            for tick in 1..20 {
                let boundary = (tick * rate + PLAYER_HZ - 1) / PLAYER_HZ;
                while sample + 1 < boundary {
                    engine.next_sample();
                    sample += 1;
                }
                assert_eq!(engine.notes[MELODY].age, tick);

                engine.next_sample();
                sample += 1;
                assert_eq!(engine.notes[MELODY].age, tick + 1);
            }
        }
    }

    #[test]
    fn registers_hold_between_ticks() {
        let mut engine = PsgEngine::new(48_000);
        engine.arpeggio([261.63, 329.63, 392.0]);
        engine.next_sample();
        let initial_pitch = engine.chip.read_register(2);
        let initial_level = engine.chip.read_register(9);

        for _ in 1..800 {
            engine.next_sample();
        }
        assert_eq!(engine.chip.read_register(2), initial_pitch);
        assert_eq!(engine.chip.read_register(9), initial_level);

        engine.next_sample();
        assert!(engine.chip.read_register(9) < initial_level);
        for _ in 801..2400 {
            engine.next_sample();
        }
        assert_eq!(engine.chip.read_register(2), initial_pitch);
        engine.next_sample();
        assert_eq!(engine.chip.read_register(2), 83);
    }

    #[test]
    fn drum_releases_channel_c() {
        let mut engine = PsgEngine::new(48_000);
        engine.snare();
        engine.next_sample();
        assert_eq!(engine.mixer, 0x98);
        for _ in 1..12_000 {
            engine.next_sample();
        }
        assert_eq!(engine.mixer, 0xb8);
        assert_eq!(engine.chip.read_register(10), 0);
        engine.bass(130.81);
        engine.next_sample();
        assert_eq!(engine.drum, None);
        assert_eq!(engine.chip.read_register(10), 9);
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
