use vizia::prelude::*;

use crate::pip::{Pip, PipState};
use crate::round_button::RoundButton;
use crate::sequencer::{HihatVoice, STEPS, SharedState, random_pattern};
use crate::step_dot::{StepDot, StepDotState};

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

#[derive(Lens)]
struct AppState {
    selected_track: usize,
    current_step: usize,
    kick: Vec<bool>,
    snare: Vec<bool>,
    hihat: Vec<Option<HihatVoice>>,
    tone: Vec<bool>,
    shared: SharedState,
}

impl AppState {
    fn sync_from_shared(&mut self) {
        let s = self.shared.lock().unwrap();
        self.current_step = s.current_step;
        self.kick = s.pattern.kick.to_vec();
        self.snare = s.pattern.snare.to_vec();
        self.hihat = s.pattern.hihat.to_vec();
        self.tone = s.pattern.tone.to_vec();
    }
}

#[derive(Debug)]
enum AppEvent {
    Tick,
    Randomize,
    NextTrack,
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
                self.selected_track = (self.selected_track + 1) % NUM_TRACKS;
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

        let initial_state = {
            let s = shared_clone.lock().unwrap();
            AppState {
                selected_track: 0,
                current_step: s.current_step,
                kick: s.pattern.kick.to_vec(),
                snare: s.pattern.snare.to_vec(),
                hihat: s.pattern.hihat.to_vec(),
                tone: s.pattern.tone.to_vec(),
                shared: shared_clone.clone(),
            }
        };

        initial_state.build(cx);

        let timer = cx.add_timer(std::time::Duration::from_millis(16), None, |cx, _| {
            cx.emit(AppEvent::Tick);
        });
        cx.start_timer(timer);

        cx.add_stylesheet(include_style!("")).ok();

        HStack::new(cx, |cx| {
            AppState::selected_track.get(cx);

            VStack::new(cx, |cx| {
                Binding::new(cx, AppState::selected_track, |cx, selected_lens| {
                    let selected = selected_lens.get(cx);
                    HStack::new(cx, |cx| {
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

                RoundButton::build(cx, "TRACK", Code::KeyT, |ex| ex.emit(AppEvent::NextTrack));
            })
            .alignment(Alignment::BottomCenter)
            .padding_bottom(Pixels(32.0));

            VStack::new(cx, |cx| {
                Binding::new(cx, AppState::current_step, move |cx, _| {
                    let current = AppState::current_step.get(cx);
                    let selected = AppState::selected_track.get(cx);
                    match selected {
                        0 => {
                            let kick = AppState::kick.get(cx);
                            bool_step_row(cx, &kick, current, 0..HALF);
                            bool_step_row(cx, &kick, current, HALF..STEPS);
                        }
                        1 => {
                            let snare = AppState::snare.get(cx);
                            bool_step_row(cx, &snare, current, 0..HALF);
                            bool_step_row(cx, &snare, current, HALF..STEPS);
                        }
                        2 => {
                            let hihat = AppState::hihat.get(cx);
                            hihat_step_row(cx, &hihat, current, 0..HALF);
                            hihat_step_row(cx, &hihat, current, HALF..STEPS);
                        }
                        3 => {
                            let tone = AppState::tone.get(cx);
                            bool_step_row(cx, &tone, current, 0..HALF);
                            bool_step_row(cx, &tone, current, HALF..STEPS);
                        }
                        _ => unreachable!(),
                    }
                });
            })
            .width(Percentage(50.0))
            .height(Percentage(70.0))
            .alignment(Alignment::Center)
            .vertical_gap(Pixels(18.0))
            .class("seq");

            VStack::new(cx, |cx| {
                RoundButton::build(cx, "RAND", Code::KeyR, |ex| ex.emit(AppEvent::Randomize));
            })
            .alignment(Alignment::BottomCenter)
            .padding_bottom(Pixels(32.0));
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
