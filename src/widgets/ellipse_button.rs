use std::sync::Arc;
use vizia::prelude::*;

const STYLE: &str = r#"
    .ellipse-button {
        width: 42px;
        height: 29px;
        corner-radius: 50%;
        border: 6px solid #900;
        background-image: linear-gradient(to top, #ccc 30%, #eee 80%, #fff);
    }

    .ellipse-button:active, .ellipse-button:checked {
        background-image: linear-gradient(to top, #bbb 80%, #ddd 90%, #eee);
    }
"#;

#[derive(Lens)]
struct EllipseButtonState {
    pressed: bool,
    on_press: Arc<dyn Fn(&mut EventContext) + Send + Sync>,
}

enum EllipseButtonEvent {
    Press,
    Release,
}

impl View for EllipseButtonState {}

impl Model for EllipseButtonState {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|e: &EllipseButtonEvent, _| match e {
            EllipseButtonEvent::Press => {
                self.pressed = true;
                (self.on_press)(cx);
            }
            EllipseButtonEvent::Release => {
                self.pressed = false;
            }
        });
    }
}

pub struct EllipseButton {
    label: &'static str,
    shortcut: Code,
    width: Units,
    height: Units,
}

impl EllipseButton {
    pub fn new(label: &'static str, shortcut: Code) -> Self {
        Self {
            label,
            shortcut,
            width: Units::Auto,
            height: Pixels(64.0),
        }
    }

    #[expect(dead_code)]
    pub fn width(mut self, width: Units) -> Self {
        self.width = width;
        self
    }

    #[expect(dead_code)]
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
                EllipseButtonState {
                    pressed: false,
                    on_press: Arc::new(on_press),
                },
                cx,
            );
            Button::new(cx, |cx| Label::new(cx, " "))
                .checked(EllipseButtonState::pressed)
                .on_press(|ex| ex.emit(EllipseButtonEvent::Release))
                .on_press_down(|ex| ex.emit(EllipseButtonEvent::Press))
                .class("ellipse-button");
            Label::new(cx, label).font_size(12.0);
        })
        .width(self.width)
        .height(self.height)
        .alignment(Alignment::BottomCenter)
        .gap(Pixels(6.0));

        let owner = vstack.entity();

        cx.add_global_listener(move |cx, event| {
            event.map(|e: &WindowEvent, _| match e {
                WindowEvent::KeyDown(code, _) if *code == shortcut => {
                    cx.emit_to(owner, EllipseButtonEvent::Press);
                }
                WindowEvent::KeyUp(code, _) if *code == shortcut => {
                    cx.emit_to(owner, EllipseButtonEvent::Release);
                }
                _ => {}
            });
        });
    }
}
