use vizia::prelude::*;

const STYLE: &str = r#"
    .ellipse-recess {
        width: 54px;
        height: 27px;
        corner-radius: 50%;
        shadow:
            3px 3px 12px 0px #999999 inset,
            -1px -1px 8px 0px #000000 inset;
    }

    .ellipse-button {
        width: 36px;
        height: 18px;
        corner-radius: 50%;
        background-image: linear-gradient(to top, #bfbfbf 50%, #7f7f7f 80%, #000000 90%);
        cursor: pointer;
    }

    .ellipse-button:active, .ellipse-button:checked {
        background-image: linear-gradient(to top, #bfbfbf 40%, #7f7f7f 70%, #444444 90%);
    }

    .ellipse-label {
        font-family: "Futura", sans-serif;
        font-size: 11px;
        color: MidnightBlue;
    }
"#;

pub struct EllipseButton {
    label: &'static str,
    width: Units,
    height: Units,
}

impl EllipseButton {
    pub fn new(label: &'static str) -> Self {
        Self {
            label,
            width: Units::Auto,
            height: Pixels(64.0),
        }
    }

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
            HStack::new(cx, |cx| {
                Button::new(cx, |cx| Label::new(cx, " "))
                    .checked(pressed)
                    .on_press(move |_ex| {
                        pressed.set(false);
                    })
                    .on_press_down(move |ex| {
                        pressed.set(true);
                        on_press(ex);
                    })
                    .class("ellipse-button");
            })
            .class("ellipse-recess")
            .alignment(Alignment::Center);

            Label::new(cx, label).class("ellipse-label");
        })
        .alignment(Alignment::BottomCenter)
        .width(self.width)
        .height(self.height);
    }
}
