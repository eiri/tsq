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

pub struct RoundButton {
    label: &'static str,
    width: Units,
    height: Units,
}

impl RoundButton {
    pub fn new(label: &'static str) -> Self {
        Self {
            label,
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

        let pressed = Signal::new(false);

        VStack::new(cx, move |cx| {
            Button::new(cx, |cx| Label::new(cx, " "))
                .checked(pressed)
                .on_press(move |_ex| {
                    pressed.set(false);
                })
                .on_press_down(move |ex| {
                    pressed.set(true);
                    on_press(ex);
                })
                .class("round-button");
            Label::new(cx, label);
        })
        .width(self.width)
        .height(self.height)
        .alignment(Alignment::BottomCenter)
        .gap(Pixels(9.0));
    }
}
