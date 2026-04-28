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
                .class("ellipse-button");
            Label::new(cx, label).font_size(12.0);
        })
        .width(self.width)
        .height(self.height)
        .alignment(Alignment::BottomCenter)
        .gap(Pixels(6.0));
    }
}
