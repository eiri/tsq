use std::sync::Arc;
use vizia::prelude::*;

const STYLE: &str = r#"
    .round-button {
        size: 48px;
        corner-radius: 50%;
        border: 7px solid #900;
        background-image: linear-gradient(to top right, #f00 10%, #c00 60%, #fff);
    }

    .round-button:active, .round-button:checked {
        background-image: linear-gradient(to top right, #d00 10%, #a00 92%, #ddd);
    }
"#;

#[derive(Lens)]
struct RoundButtonState {
    pressed: bool,
    on_press: Arc<dyn Fn(&mut EventContext) + Send + Sync>,
}

enum RoundButtonEvent {
    Press,
    Release,
}

impl View for RoundButtonState {}

impl Model for RoundButtonState {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|e: &RoundButtonEvent, _| match e {
            RoundButtonEvent::Press => {
                self.pressed = true;
                (self.on_press)(cx);
            }
            RoundButtonEvent::Release => {
                self.pressed = false;
            }
        });
    }
}

pub struct RoundButton {
    label: &'static str,
    shortcut: Code,
    width: Units,
    height: Units,
}

impl RoundButton {
    pub fn new(label: &'static str, shortcut: Code) -> Self {
        Self {
            label,
            shortcut,
            width: Units::Auto,
            height: Pixels(76.0),
        }
    }

    #[expect(dead_code)]
    pub fn width(mut self, width: Units) -> Self {
        self.width = width;
        self
    }

    pub fn height(mut self, height: Units) -> Self {
        self.height = height;
        self
    }

    pub fn build<F>(self, cx: &mut Context, on_press: F)
    where
        F: 'static + Fn(&mut EventContext) + Send + Sync,
    {
        cx.add_stylesheet(STYLE).ok();

        let label = self.label;
        let shortcut = self.shortcut;

        let vstack = VStack::new(cx, move |cx| {
            Model::build(
                RoundButtonState {
                    pressed: false,
                    on_press: Arc::new(on_press),
                },
                cx,
            );
            Button::new(cx, |cx| Label::new(cx, " "))
                .checked(RoundButtonState::pressed)
                .on_press(|ex| ex.emit(RoundButtonEvent::Release))
                .on_press_down(|ex| ex.emit(RoundButtonEvent::Press))
                .class("round-button");
            Label::new(cx, label);
        })
        .width(self.width)
        .height(self.height)
        .alignment(Alignment::BottomCenter)
        .gap(Pixels(9.0));

        let owner = vstack.entity();

        cx.add_global_listener(move |cx, event| {
            event.map(|e: &WindowEvent, _| match e {
                WindowEvent::KeyDown(code, _) if *code == shortcut => {
                    cx.emit_to(owner, RoundButtonEvent::Press);
                }
                WindowEvent::KeyUp(code, _) if *code == shortcut => {
                    cx.emit_to(owner, RoundButtonEvent::Release);
                }
                _ => {}
            });
        });
    }
}
