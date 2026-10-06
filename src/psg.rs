use ym2149::{Ym2149, Ym2149Backend};

const MASTER_CLOCK: u32 = 2_000_000;

pub struct PsgEngine {
    chips: [Ym2149; 2],
}

impl PsgEngine {
    pub fn new(sample_rate: u32) -> Self {
        // Match the output device rate to keep the chip pitch correct.
        let chips = std::array::from_fn(|_| Ym2149::with_clocks(MASTER_CLOCK, sample_rate));
        Self { chips }
    }

    pub fn next_sample(&mut self) -> f32 {
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
        for chip in one.chips[..1].iter_mut().chain(both.chips.iter_mut()) {
            chip.write_register(0, 0x1c);
            chip.write_register(1, 0x01);
            chip.write_register(7, 0x3e);
            chip.write_register(8, 0x0f);
        }

        let peak = |engine: &mut PsgEngine| {
            (0..4096)
                .map(|_| engine.next_sample().abs())
                .fold(0.0_f32, f32::max)
        };
        assert!(peak(&mut both) > peak(&mut one) * 1.5);
    }
}
