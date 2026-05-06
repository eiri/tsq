use vizia::prelude::*;

use crate::sequencer::{HihatVoice, STEPS, SharedState, random_pattern};
use crate::widgets::{EllipseButton, Heart, HeartState, Pip, PipState, StepDot, StepDotState};

const NUM_TRACKS: usize = 4;

const STYLE: &str = r#"
    .container {
        border: 12px solid Maroon;

        corner-bottom-right-shape: bevel;
        corner-bottom-right-radius: 10%;
        /* h-shadow v-shadow blur spread color inset */
        shadow: 0px 16px 8px -8px #ccc;
        background-color: Ivory;
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

#[derive(Debug, PartialEq, Copy, Clone)]
enum KeymapAction {
    OnP,
    OnT,
    OnR,
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
    .alignment(Alignment::Center)
    // .width(Pixels(450.0))
    .height(Pixels(54.0))
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
    .alignment(Alignment::Center)
    // .width(Pixels(450.0))
    .height(Pixels(54.0))
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

        Keymap::from(vec![
            (
                KeyChord::new(Modifiers::empty(), Code::KeyP),
                KeymapEntry::new(KeymapAction::OnP, |ex| ex.emit(AppEvent::TogglePlay)),
            ),
            (
                KeyChord::new(Modifiers::empty(), Code::KeyT),
                KeymapEntry::new(KeymapAction::OnT, |ex| ex.emit(AppEvent::NextTrack)),
            ),
            (
                KeyChord::new(Modifiers::empty(), Code::KeyR),
                KeymapEntry::new(KeymapAction::OnR, |ex| ex.emit(AppEvent::Randomize)),
            ),
        ])
        .build(cx);

        HStack::new(cx, |cx| {
            Binding::new(cx, selected_track, move |cx| {
                let selected = selected_track.get();

                // tracks
                VStack::new(cx, move |cx| {
                    for i in 0..NUM_TRACKS {
                        let heart_state = if selected == i {
                            HeartState::On
                        } else {
                            HeartState::Off
                        };
                        Heart::new(cx, heart_state)
                            .width(Pixels(18.0))
                            .height(Pixels(18.0));
                    }
                })
                .alignment(Alignment::TopCenter)
                .width(Pixels(36.0))
                .padding_top(Pixels(66.0))
                .vertical_gap(Pixels(36.0));
            });

            Binding::new(cx, current_step, move |cx| {
                let current = current_step.get();

                // sequencer
                VStack::new(cx, move |cx| {
                    // page stack
                    HStack::new(cx, |cx| {
                        for i in 0..NUM_TRACKS {
                            let state = if i == current / 2 {
                                PipState::On
                            } else {
                                PipState::Off
                            };
                            Pip::new(cx, state).width(Pixels(18.0)).height(Pixels(9.0));
                        }
                    })
                    .alignment(Alignment::Center)
                    .width(Pixels(216.0))
                    .height(Pixels(36.0))
                    .horizontal_gap(Pixels(36.0));
                    // steps
                    Binding::new(cx, kick, move |cx| {
                        let k = kick.get();
                        bool_step_row(cx, &k, current, 0..STEPS);
                    });

                    Binding::new(cx, snare, move |cx| {
                        let s = snare.get();
                        bool_step_row(cx, &s, current, 0..STEPS);
                    });

                    Binding::new(cx, hihat, move |cx| {
                        let h = hihat.get();
                        hihat_step_row(cx, &h, current, 0..STEPS);
                    });

                    Binding::new(cx, tone, move |cx| {
                        let t = tone.get();
                        bool_step_row(cx, &t, current, 0..STEPS);
                    });
                })
                .alignment(Alignment::TopLeft)
                .width(Pixels(432.0))
                .padding_top(Pixels(12.0));
            });

            VStack::new(cx, |_cx| {}).width(Pixels(36.0));

            // controls
            VStack::new(cx, |cx| {
                HStack::new(cx, |cx| {
                    EllipseButton::new("TRACK")
                        .width(Pixels(54.0))
                        .height(Pixels(54.0))
                        .build(cx, |ex| ex.emit(AppEvent::NextTrack));

                    EllipseButton::new("PLAY")
                        .width(Pixels(54.0))
                        .height(Pixels(54.0))
                        .build(cx, |ex| ex.emit(AppEvent::TogglePlay));
                })
                .alignment(Alignment::Left)
                .width(Pixels(135.0))
                .height(Pixels(54.0))
                .horizontal_gap(Pixels(9.0));

                HStack::new(cx, |cx| {
                    EllipseButton::new("RAND")
                        .width(Pixels(54.0))
                        .height(Pixels(54.0))
                        .build(cx, |ex| ex.emit(AppEvent::Randomize));
                })
                .alignment(Alignment::Left)
                .width(Pixels(135.0))
                .height(Pixels(54.0))
                .horizontal_gap(Pixels(9.0));
            })
            .width(Pixels(135.0))
            .padding_top(Pixels(48.0))
            .alignment(Alignment::TopRight);
        })
        .class("container")
        .alignment(Alignment::BottomCenter);
    })
    .title("tsq")
    .inner_size((792, 312))
    .resizable(true)
    .run()
}
