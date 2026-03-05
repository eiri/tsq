mod sequencer;
mod voices;

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal;

use sequencer::{AudioClock, STEPS, SharedState, ToneVoice, new_shared_state, random_pattern};
use voices::{VoicePool, hihat_closed, hihat_open, kick, square_tone, tone};

// C major scale from middle C (C4) to C5, one note per step
const TONE_FREQS: [f64; STEPS] = [
    261.63, 293.66, 329.63, 349.23, 392.00, 440.00, 493.88, 523.25,
];

// C4 -> 60
// fn note_to_freq(midi: u8) -> f64 {
//     440.0 * 2.0_f64.powf((midi as f64 - 69.0) / 12.0)
// }

fn build_audio_stream(shared: SharedState) -> Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("no output device"))?;
    let config = device.default_output_config()?;
    let sr = config.sample_rate() as f64;
    let channels = config.channels() as usize;

    let mut clock = AudioClock::new(sr);
    let mut pools: [VoicePool; 4] = [
        VoicePool::new(),
        VoicePool::new(),
        VoicePool::new(),
        VoicePool::new(),
    ];

    let stream = device.build_output_stream(
        &config.into(),
        move |data: &mut [f32], _| {
            {
                let mut s = shared.lock().unwrap();
                if s.reset {
                    pools.iter_mut().for_each(|p| p.reset());
                    s.reset = false;
                }
            }
            let (bpm, pattern, playing) = {
                let s = shared.lock().unwrap();
                (s.bpm, s.pattern.clone(), s.playing)
            };

            for frame in data.chunks_mut(channels) {
                if let Some(step) = clock.advance(bpm) {
                    if playing {
                        if pattern.kick[step] {
                            pools[0].trigger(kick(1.0), 0.4);
                        }
                        if pattern.hihat_closed[step] {
                            pools[1].trigger(hihat_closed(1.0), 0.3);
                        }
                        if pattern.hihat_open[step] {
                            pools[2].trigger(hihat_open(1.0), 0.8);
                        }
                        if pattern.tone[step] {
                            let (voice, ttl) = match pattern.tone_voice {
                                ToneVoice::Sine => (tone(TONE_FREQS[step], 1.0), 2.0),
                                ToneVoice::Square => (square_tone(TONE_FREQS[step], 1.0), 0.5),
                            };
                            pools[3].trigger(voice, ttl);
                        }
                    }
                    let mut s = shared.lock().unwrap();
                    s.current_step = step;
                }

                let sample = (pools[0].render(sr)
                    + pools[1].render(sr)
                    + pools[2].render(sr)
                    + pools[3].render(sr)) as f32
                    * 0.5;

                for ch in frame.iter_mut() {
                    *ch = sample;
                }
            }
        },
        |err| eprintln!("audio error: {err}"),
        None,
    )?;

    stream.play()?;
    Ok(stream)
}

fn print_state(step: usize, bpm: f64) {
    let indicator: String = (0..STEPS)
        .map(|i| if i == step { "[x]" } else { "[ ]" })
        .collect::<Vec<_>>()
        .join(" ");
    print!("\r\x1B[2K  {indicator}   {bpm} BPM  (q to quit)");
    let _ = std::io::Write::flush(&mut std::io::stdout());
}

fn main() -> Result<()> {
    let shared = new_shared_state();
    let _stream = build_audio_stream(shared.clone())?;

    println!("tsq running — pattern:");
    println!(
        "  kick:      {}",
        shared
            .lock()
            .unwrap()
            .pattern
            .kick
            .map(|b| if b { "[x]" } else { "[ ]" })
            .join(" ")
    );
    println!(
        "  hh closed: {}",
        shared
            .lock()
            .unwrap()
            .pattern
            .hihat_closed
            .map(|b| if b { "[x]" } else { "[ ]" })
            .join(" ")
    );
    println!(
        "  hh open:   {}",
        shared
            .lock()
            .unwrap()
            .pattern
            .hihat_open
            .map(|b| if b { "[x]" } else { "[ ]" })
            .join(" ")
    );
    println!(
        "  tone:      {}",
        shared
            .lock()
            .unwrap()
            .pattern
            .tone
            .map(|b| if b { "[x]" } else { "[ ]" })
            .join(" ")
    );
    println!();

    terminal::enable_raw_mode()?;
    loop {
        let (step, bpm) = {
            let s = shared.lock().unwrap();
            (s.current_step, s.bpm)
        };
        print_state(step, bpm);

        if event::poll(std::time::Duration::from_millis(16))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('r') => {
                    let mut s = shared.lock().unwrap();
                    s.pattern = random_pattern();
                    s.reset = true;
                    terminal::disable_raw_mode()?;
                    print!("\x1B[5A\r");
                    println!(
                        "  kick:      {}",
                        s.pattern
                            .kick
                            .map(|b| if b { "[x]" } else { "[ ]" })
                            .join(" ")
                    );
                    println!(
                        "  hh closed: {}",
                        s.pattern
                            .hihat_closed
                            .map(|b| if b { "[x]" } else { "[ ]" })
                            .join(" ")
                    );
                    println!(
                        "  hh open:   {}",
                        s.pattern
                            .hihat_open
                            .map(|b| if b { "[x]" } else { "[ ]" })
                            .join(" ")
                    );
                    println!(
                        "  tone:      {}",
                        s.pattern
                            .tone
                            .map(|b| if b { "[x]" } else { "[ ]" })
                            .join(" ")
                    );
                    println!();
                    terminal::enable_raw_mode()?;
                }
                _ => {}
            }
        }
    }
    terminal::disable_raw_mode()?;
    println!();
    Ok(())
}
