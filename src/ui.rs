use vizia::prelude::*;

use crate::sequencer::{
    Audition, Drum, MelodyVoice, PAGE_STEPS, STEPS, SharedState, TRACKS, random_track,
};
use crate::widgets::{EllipseButton, Heart, HeartState, Pip, PipState, StepDot, StepDotState};

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
    melody: Signal<Vec<Option<MelodyVoice>>>,
    arpeggio: Signal<Vec<bool>>,
    bass: Signal<Vec<bool>>,
    drums: Signal<Vec<Option<Drum>>>,
    playing: Signal<bool>,
    muted: Signal<[bool; TRACKS]>,
    shared: SharedState,
}

impl AppState {
    fn sync_from_shared(&self) {
        // Release the audio-state lock before notifying UI subscribers.
        let s = self.shared.lock().unwrap().clone();

        // Unchanged timer ticks must not rebuild the sequencer views.
        self.current_step.set_if_changed(s.current_step);
        self.melody.set_if_changed(
            s.pattern
                .melody
                .iter()
                .map(|note| note.map(|n| n.voice))
                .collect(),
        );
        self.arpeggio
            .set_if_changed(s.pattern.arpeggio.iter().map(Option::is_some).collect());
        self.bass
            .set_if_changed(s.pattern.bass.iter().map(Option::is_some).collect());
        self.drums.set_if_changed(s.pattern.drums.to_vec());
        self.playing.set_if_changed(s.playing);
        self.muted.set_if_changed(s.muted);
    }

    fn toggle_mute(&self) {
        let track = self.selected_track.get();
        let mut state = self.shared.lock().unwrap();
        state.muted[track] = !state.muted[track];
        drop(state);
        self.sync_from_shared();
    }
}

#[derive(Debug)]
enum AppEvent {
    Tick,
    Randomize,
    NextTrack,
    ToggleMute,
    TogglePlay,
    Audition(Audition),
}

#[derive(Debug, PartialEq, Copy, Clone)]
enum KeymapAction {
    OnP,
    OnT,
    OnM,
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
                    random_track(&mut s.pattern, self.selected_track.get());
                }
                self.sync_from_shared();
            }
            AppEvent::NextTrack => {
                self.selected_track.update(|t| *t = (*t + 1) % TRACKS);
            }
            AppEvent::ToggleMute => self.toggle_mute(),
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
            AppEvent::Audition(request) => {
                self.shared.lock().unwrap().audition = Some(*request);
            }
        });
    }
}

fn track_state(selected: usize, muted: bool, row: usize) -> HeartState {
    match (muted, selected == row) {
        (false, true) => HeartState::On,
        (false, false) => HeartState::Dim,
        (true, true) => HeartState::HalfDim,
        (true, false) => HeartState::Off,
    }
}

fn step_color_bool(active: bool, is_current: bool) -> StepDotState {
    match (active, is_current) {
        (_, true) => StepDotState::On,
        (true, false) => StepDotState::Dim,
        (false, false) => StepDotState::Off,
    }
}

fn page_step(current: usize) -> (usize, usize, usize) {
    let page = current / PAGE_STEPS;
    (page, page * PAGE_STEPS, current % PAGE_STEPS)
}

fn melody_step_row(cx: &mut Context, steps: &[Option<MelodyVoice>], current: usize) {
    HStack::new(cx, |cx| {
        for (i, voice) in steps.iter().enumerate() {
            let state = match (i == current, voice) {
                (true, _) => StepDotState::On,
                (false, Some(MelodyVoice::M1)) => StepDotState::Dim,
                (false, Some(MelodyVoice::M2)) => StepDotState::HalfDim,
                (false, None) => StepDotState::Off,
            };
            StepDot::new(cx, state)
                .width(Pixels(18.0))
                .height(Pixels(18.0));
        }
    })
    .alignment(Alignment::Center)
    .height(Pixels(54.0))
    .horizontal_gap(Pixels(36.0));
}

fn step_color_drum(step: &Option<Drum>, is_current: bool) -> StepDotState {
    match (step, is_current) {
        (_, true) => StepDotState::On,
        (Some(Drum::ClosedHat), false) => StepDotState::HalfDim,
        (Some(_), false) => StepDotState::Dim,
        (None, false) => StepDotState::Off,
    }
}

fn bool_step_row(cx: &mut Context, steps: &[bool], current: usize) {
    HStack::new(cx, move |cx| {
        for (i, active) in steps.iter().enumerate() {
            let step_dot_state = step_color_bool(*active, i == current);
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

fn drum_step_row(cx: &mut Context, steps: &[Option<Drum>], current: usize) {
    HStack::new(cx, move |cx| {
        for (i, drum) in steps.iter().enumerate() {
            let step_dot_state = step_color_drum(drum, i == current);
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

        let (selected_track, current_step, melody, arpeggio, bass, drums, playing, muted) = {
            let s = shared_clone.lock().unwrap();
            (
                Signal::new(0),
                Signal::new(s.current_step),
                Signal::new(
                    s.pattern
                        .melody
                        .iter()
                        .map(|note| note.map(|n| n.voice))
                        .collect(),
                ),
                Signal::new(s.pattern.arpeggio.iter().map(Option::is_some).collect()),
                Signal::new(s.pattern.bass.iter().map(Option::is_some).collect()),
                Signal::new(s.pattern.drums.to_vec()),
                Signal::new(s.playing),
                Signal::new(s.muted),
            )
        };

        AppState {
            selected_track,
            current_step,
            melody,
            arpeggio,
            bass,
            drums,
            playing,
            muted,
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
                KeyChord::new(Modifiers::empty(), Code::KeyM),
                KeymapEntry::new(KeymapAction::OnM, |ex| ex.emit(AppEvent::ToggleMute)),
            ),
            (
                KeyChord::new(Modifiers::empty(), Code::KeyR),
                KeymapEntry::new(KeymapAction::OnR, |ex| ex.emit(AppEvent::Randomize)),
            ),
        ])
        .build(cx);

        HStack::new(cx, |cx| {
            Binding::new(cx, muted, move |cx| {
                let marks = muted.get();
                Binding::new(cx, selected_track, move |cx| {
                    let selected = selected_track.get();

                    // Selection and mute each change the marker's light level.
                    VStack::new(cx, move |cx| {
                        for (i, muted) in marks.into_iter().enumerate() {
                            let heart_state = track_state(selected, muted, i);
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
            });

            VStack::new(cx, |cx| {
                Binding::new(cx, current_step, move |cx| {
                    let current = current_step.get();
                    let (page, start, column) = page_step(current);

                    // sequencer
                    VStack::new(cx, move |cx| {
                        // The top lights mark the playing eight-step page.
                        HStack::new(cx, |cx| {
                            for i in 0..STEPS / PAGE_STEPS {
                                let state = if i == page {
                                    PipState::On
                                } else {
                                    PipState::Off
                                };
                                Pip::new(cx, state).width(Pixels(18.0)).height(Pixels(9.0));
                            }
                        })
                        .alignment(Alignment::Center)
                        .width(Pixels(432.0))
                        .height(Pixels(36.0))
                        .horizontal_gap(Pixels(36.0));
                        // steps
                        Binding::new(cx, melody, move |cx| {
                            let m = melody.get();
                            melody_step_row(cx, &m[start..start + PAGE_STEPS], column);
                        });

                        Binding::new(cx, arpeggio, move |cx| {
                            let s = arpeggio.get();
                            bool_step_row(cx, &s[start..start + PAGE_STEPS], column);
                        });

                        Binding::new(cx, bass, move |cx| {
                            let b = bass.get();
                            bool_step_row(cx, &b[start..start + PAGE_STEPS], column);
                        });

                        Binding::new(cx, drums, move |cx| {
                            let d = drums.get();
                            drum_step_row(cx, &d[start..start + PAGE_STEPS], column);
                        });
                    })
                    .alignment(Alignment::TopLeft)
                    .width(Pixels(432.0))
                    .height(Pixels(264.0))
                    .padding_top(Pixels(12.0));
                });
            })
            .width(Pixels(432.0))
            .height(Pixels(264.0))
            .alignment(Alignment::TopLeft);

            VStack::new(cx, |_cx| {}).width(Pixels(36.0));

            VStack::new(cx, |cx| {
                // Match the four-column preview grid, with one blank row between.
                HStack::new(cx, |cx| {
                    EllipseButton::new("TRACK")
                        .width(Pixels(54.0))
                        .height(Pixels(54.0))
                        .build(cx, |ex| ex.emit(AppEvent::NextTrack));
                    EllipseButton::new("MUTE")
                        .width(Pixels(54.0))
                        .height(Pixels(54.0))
                        .build(cx, |ex| ex.emit(AppEvent::ToggleMute));
                    EllipseButton::new("RAND")
                        .width(Pixels(54.0))
                        .height(Pixels(54.0))
                        .build(cx, |ex| ex.emit(AppEvent::Randomize));
                    EllipseButton::new("PLAY")
                        .width(Pixels(54.0))
                        .height(Pixels(54.0))
                        .build(cx, |ex| ex.emit(AppEvent::TogglePlay));
                })
                .width(Pixels(216.0))
                .height(Pixels(54.0));

                VStack::new(cx, |_cx| {}).height(Pixels(54.0));

                // Eight sounds occupy two rows in a separate stack.
                VStack::new(cx, |cx| {
                    for row in [
                        &[
                            ("M1", Audition::Melody),
                            ("M2", Audition::Melody2),
                            ("A", Audition::Arpeggio),
                            ("B", Audition::Bass),
                        ][..],
                        &[
                            ("K", Audition::Drum(Drum::Kick)),
                            ("S", Audition::Drum(Drum::Snare)),
                            ("CH", Audition::Drum(Drum::ClosedHat)),
                            ("OH", Audition::Drum(Drum::OpenHat)),
                        ][..],
                    ] {
                        HStack::new(cx, |cx| {
                            for &(label, voice) in row {
                                EllipseButton::new(label)
                                    .width(Pixels(54.0))
                                    .height(Pixels(54.0))
                                    .build(cx, move |ex| ex.emit(AppEvent::Audition(voice)));
                            }
                        })
                        .width(Pixels(216.0))
                        .height(Pixels(54.0));
                    }
                })
                .width(Pixels(216.0))
                .height(Pixels(108.0));
            })
            .width(Pixels(216.0))
            .height(Pixels(264.0))
            .padding_top(Pixels(48.0))
            .alignment(Alignment::TopLeft);
        })
        .class("container")
        .padding_bottom(Pixels(18.0))
        .alignment(Alignment::TopCenter);
    })
    .title("tsq")
    .inner_size((792, 360))
    .resizable(true)
    .run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn pages_follow_the_playhead() {
        assert_eq!(page_step(0), (0, 0, 0));
        assert_eq!(page_step(7), (0, 0, 7));
        assert_eq!(page_step(8), (1, 8, 0));
        assert_eq!(page_step(63), (7, 56, 7));

        let pattern = crate::sequencer::Pattern::default();
        let (_, start, column) = page_step(63);
        assert_eq!(
            pattern.drums[start..start + PAGE_STEPS][column],
            pattern.drums[63]
        );
    }

    #[test]
    fn muted_marker_takes_priority() {
        assert_eq!(track_state(1, false, 1), HeartState::On);
        assert_eq!(track_state(1, false, 0), HeartState::Dim);
        assert_eq!(track_state(1, true, 1), HeartState::HalfDim);
        assert_eq!(track_state(1, true, 0), HeartState::Off);
    }

    #[test]
    fn sync_skips_unchanged_values() {
        let shared = crate::sequencer::new_shared_state();
        let state = AppState {
            selected_track: Signal::new(0),
            current_step: Signal::new(0),
            melody: Signal::new(Vec::new()),
            arpeggio: Signal::new(Vec::new()),
            bass: Signal::new(Vec::new()),
            drums: Signal::new(Vec::new()),
            playing: Signal::new(false),
            muted: Signal::new([false; TRACKS]),
            shared: shared.clone(),
        };
        state.sync_from_shared();

        let signals = (
            state.current_step,
            state.melody,
            state.arpeggio,
            state.bass,
            state.drums,
            state.playing,
            state.muted,
        );
        let updates = Rc::new(Cell::new(0));
        let count = updates.clone();
        let audio_state = shared.clone();
        UpdaterEffect::new(
            move || {
                (
                    signals.0.get(),
                    signals.1.get(),
                    signals.2.get(),
                    signals.3.get(),
                    signals.4.get(),
                    signals.5.get(),
                    signals.6.get(),
                )
            },
            move |_| {
                assert!(audio_state.try_lock().is_ok());
                count.set(count.get() + 1);
            },
        );

        state.sync_from_shared();
        state.sync_from_shared();
        assert_eq!(updates.get(), 0);

        {
            let mut s = shared.lock().unwrap();
            s.current_step = 1;
            s.pattern.melody[4] = Some(crate::sequencer::MelodyStep {
                freq: 392.0,
                voice: crate::sequencer::MelodyVoice::M2,
                short: false,
                bend: false,
            });
            s.playing = true;
        }
        state.sync_from_shared();
        assert_eq!(updates.get(), 3);
        assert_eq!(state.current_step.get(), 1);
        assert_eq!(state.melody.get()[4], Some(MelodyVoice::M2));
        assert!(state.playing.get());

        state.sync_from_shared();
        assert_eq!(updates.get(), 3);

        // Mute changes only the chosen row and not its steps.
        let bass = shared.lock().unwrap().pattern.bass;
        state.selected_track.set(2);
        state.toggle_mute();
        assert_eq!(state.muted.get(), [false, false, true, false]);
        assert_eq!(shared.lock().unwrap().pattern.bass, bass);
        assert_eq!(updates.get(), 4);
        state.toggle_mute();
        assert_eq!(state.muted.get(), [false; TRACKS]);
    }
}
