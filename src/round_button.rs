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
}

enum RoundButtonEvent {
    Press,
    Release,
}

impl View for RoundButtonState {}

impl Model for RoundButtonState {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|e: &RoundButtonEvent, _| match e {
            RoundButtonEvent::Press => self.pressed = true,
            RoundButtonEvent::Release => self.pressed = false,
        });
    }
}

pub struct RoundButton;

impl RoundButton {
    pub fn build<F>(cx: &mut Context, label: &'static str, shortcut: Code, on_press: F)
    where
        F: 'static + Fn(&mut EventContext) + Send + Sync,
    {
        cx.add_stylesheet(STYLE).ok();

        Model::build(RoundButtonState { pressed: false }, cx);

        let owner = cx.current();
        let on_press = Arc::new(on_press);
        let on_press_key = Arc::clone(&on_press);

        cx.add_global_listener(move |cx, event| {
            event.map(|e: &WindowEvent, _| match e {
                WindowEvent::KeyDown(code, _) if *code == shortcut => {
                    cx.emit_to(owner, RoundButtonEvent::Press);
                    on_press_key(cx);
                }
                WindowEvent::KeyUp(code, _) if *code == shortcut => {
                    cx.emit_to(owner, RoundButtonEvent::Release);
                }
                _ => {}
            });
        });

        VStack::new(cx, move |cx| {
            Button::new(cx, |cx| Label::new(cx, " "))
                .checked(RoundButtonState::pressed)
                .on_press(move |ex| on_press(ex))
                .class("round-button");
            Label::new(cx, label);
        })
        .height(Pixels(76.0))
        .alignment(Alignment::BottomCenter)
        .gap(Pixels(9.0));
    }
}
