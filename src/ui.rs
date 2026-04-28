use vizia::prelude::*;

use crate::sequencer::{HihatVoice, STEPS, SharedState, random_pattern};
use crate::widgets::{EllipseButton, Pip, PipState, RoundButton, StepDot, StepDotState};

const NUM_TRACKS: usize = 4;
const HALF: usize = STEPS / 2;

const STYLE: &str = r#"
    .seq {
        background-color: #ffffe0;
        border: 10px solid #900;
        outline: 6px #ffffe0;
        corner-radius: 0px;
        shadow:
            0px 0px 6px 8px #eeeed0,
            0px 0px 6px 11px #ddddc0,
            0px 0px 6px 14px #ccccb0,
            0px 0px 6px 17px #bbbba1,
            0px 0px 6px 20px #abab92;
    }
"#;

struct AppState {
    selected_track: Signal<usize>,
    current_step: Signal<usize>,
    kick: Signal<Vec<bool>>,
    snare: Signal<Vec<bool>>,
    hihat: Signal<Vec<Option<HihatVoice>>>,
    tone: Signal<Vec<bool>>,
    playing: Signal<bool>,
    shared: SharedState,
}

impl AppState {
    fn sync_from_shared(&self) {
        let s = self.shared.lock().unwrap();
        self.current_step.set(s.current_step);
        self.kick.set(s.pattern.kick.to_vec());
        self.snare.set(s.pattern.snare.to_vec());
        self.hihat.set(s.pattern.hihat.to_vec());
        self.tone.set(s.pattern.tone.to_vec());
        self.playing.set(s.playing);
    }
}

#[derive(Debug)]
enum AppEvent {
    Tick,
    Randomize,
    NextTrack,
    TogglePlay,
}

impl Model for AppState {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|e: &AppEvent, _| match e {
            AppEvent::Tick => {
                self.sync_from_shared();
            }
            AppEvent::Randomize => {
                {
                    let mut s = self.shared.lock().unwrap();
                    s.pattern = random_pattern();
                    s.reset = true;
                }
                self.sync_from_shared();
            }
            AppEvent::NextTrack => {
                self.selected_track.update(|t| *t = (*t + 1) % NUM_TRACKS);
            }
            AppEvent::TogglePlay => {
                {
                    let mut s = self.shared.lock().unwrap();
                    s.playing = !s.playing;
                    if !s.playing {
                        s.reset = true;
                    }
                }
                self.sync_from_shared();
            }
        });
    }
}

fn step_color_bool(active: bool, is_current: bool) -> StepDotState {
    match (active, is_current) {
        (_, true) => StepDotState::On,
        (true, false) => StepDotState::Dim,
        (false, false) => StepDotState::Off,
    }
}

fn step_color_hihat(step: &Option<HihatVoice>, is_current: bool) -> StepDotState {
    match (step, is_current) {
        (_, true) => StepDotState::On,
        (Some(HihatVoice::Open), false) => StepDotState::Dim,
        (Some(HihatVoice::Closed), false) => StepDotState::HalfDim,
        (None, false) => StepDotState::Off,
    }
}

fn bool_step_row(cx: &mut Context, steps: &[bool], current: usize, range: std::ops::Range<usize>) {
    HStack::new(cx, move |cx| {
        for i in range {
            let step_dot_state = step_color_bool(steps[i], i == current);
            StepDot::new(cx, step_dot_state)
                .width(Pixels(18.0))
                .height(Pixels(18.0));
        }
    })
    .height(Pixels(36.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(36.0));
}

fn hihat_step_row(
    cx: &mut Context,
    steps: &[Option<HihatVoice>],
    current: usize,
    range: std::ops::Range<usize>,
) {
    HStack::new(cx, move |cx| {
        for i in range {
            let step_dot_state = step_color_hihat(&steps[i], i == current);
            StepDot::new(cx, step_dot_state)
                .width(Pixels(18.0))
                .height(Pixels(18.0));
        }
    })
    .height(Pixels(36.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(36.0));
}

pub fn run(shared: SharedState) -> Result<(), ApplicationError> {
    let shared_clone = shared.clone();

    Application::new(move |cx| {
        cx.add_stylesheet(STYLE).expect("loads the style");

        let (selected_track, current_step, kick, snare, hihat, tone, playing) = {
            let s = shared_clone.lock().unwrap();
            (
                Signal::new(0),
                Signal::new(s.current_step),
                Signal::new(s.pattern.kick.to_vec()),
                Signal::new(s.pattern.snare.to_vec()),
                Signal::new(s.pattern.hihat.to_vec()),
                Signal::new(s.pattern.tone.to_vec()),
                Signal::new(s.playing),
            )
        };

        AppState {
            selected_track,
            current_step,
            kick,
            snare,
            hihat,
            tone,
            playing,
            shared: shared_clone.clone(),
        }
        .build(cx);

        let timer = cx.add_timer(std::time::Duration::from_millis(16), None, |cx, _| {
            cx.emit(AppEvent::Tick);
        });
        cx.start_timer(timer);

        cx.add_stylesheet(include_style!("")).ok();

        HStack::new(cx, |cx| {
            VStack::new(cx, |cx| {
                VStack::new(cx, move |cx| {
                    Binding::new(cx, selected_track, move |cx| {
                        let selected = selected_track.get();
                        HStack::new(cx, move |cx| {
                            for i in 0..NUM_TRACKS {
                                let state = if i == selected {
                                    PipState::On
                                } else {
                                    PipState::Off
                                };
                                Pip::new(cx, state).width(Pixels(18.0)).height(Pixels(9.0));
                            }
                        })
                        .height(Pixels(64.0))
                        .alignment(Alignment::Center)
                        .horizontal_gap(Pixels(9.0));
                    });
                })
                .padding_top(Pixels(7.0))
                .alignment(Alignment::TopCenter)
                .height(Pixels(200.0));

                RoundButton::new("TRACK")
                    .height(Pixels(100.0))
                    .build(cx, |ex| ex.emit(AppEvent::NextTrack));
            })
            .alignment(Alignment::Center);

            VStack::new(cx, |cx| {
                Binding::new(cx, current_step, move |cx| {
                    Binding::new(cx, selected_track, move |cx| {
                        let current = current_step.get();
                        let selected = selected_track.get();
                        match selected {
                            0 => {
                                Binding::new(cx, kick, move |cx| {
                                    let k = kick.get();
                                    bool_step_row(cx, &k, current, 0..HALF);
                                    bool_step_row(cx, &k, current, HALF..STEPS);
                                });
                            }
                            1 => {
                                Binding::new(cx, snare, move |cx| {
                                    let s = snare.get();
                                    bool_step_row(cx, &s, current, 0..HALF);
                                    bool_step_row(cx, &s, current, HALF..STEPS);
                                });
                            }
                            2 => {
                                Binding::new(cx, hihat, move |cx| {
                                    let h = hihat.get();
                                    hihat_step_row(cx, &h, current, 0..HALF);
                                    hihat_step_row(cx, &h, current, HALF..STEPS);
                                });
                            }
                            3 => {
                                Binding::new(cx, tone, move |cx| {
                                    let t = tone.get();
                                    bool_step_row(cx, &t, current, 0..HALF);
                                    bool_step_row(cx, &t, current, HALF..STEPS);
                                });
                            }
                            _ => unreachable!(),
                        }
                    });
                });
            })
            .width(Percentage(50.0))
            .height(Percentage(70.0))
            .alignment(Alignment::Center)
            .vertical_gap(Pixels(18.0))
            .class("seq");

            VStack::new(cx, |cx| {
                VStack::new(cx, move |cx| {
                    EllipseButton::new("PLAY").build(cx, |ex| ex.emit(AppEvent::TogglePlay));
                })
                .padding_top(Pixels(7.0))
                .alignment(Alignment::TopCenter)
                .height(Pixels(200.0));
                RoundButton::new("RAND")
                    .height(Pixels(100.0))
                    .build(cx, |ex| ex.emit(AppEvent::Randomize));
            })
            .alignment(Alignment::Center);
        })
        .alignment(Alignment::Center)
        .background_color(Color::lightyellow())
        .border_color(Color::darkred())
        .border_width(Pixels(10.0))
        .corner_radius(Pixels(6.0));
    })
    .title("tsq")
    .inner_size((720, 360))
    .resizable(false)
    .run()
}
