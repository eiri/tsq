use vizia::prelude::*;
use vizia::vg;

#[derive(Debug, Clone, Copy, PartialEq, Data)]
pub enum PipState {
    Off,
    On,
}

pub struct Pip {
    state: PipState,
}

impl Pip {
    pub fn new(cx: &mut Context, state: PipState) -> Handle<'_, Self> {
        Self { state }.build(cx, |_cx| {})
    }
}

// FIXME! - common helper later
#[inline]
fn argb(a: u8, r: u8, g: u8, b: u8) -> vg::Color {
    vg::Color::from_argb(a, r, g, b)
}

impl View for Pip {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let w = bounds.w;
        let h = bounds.h;
        let x = bounds.x;
        let y = bounds.y;

        // Skew offset — how much the top edge shifts right relative to bottom
        let skew = w * 0.18;

        // The four corners of the skewed quad from top-left
        let tl = (x + skew, y);
        let tr = (x + w, y);
        let br = (x + w - skew, y + h);
        let bl = (x, y + h);

        let no_flags = vg::gradient_shader::Flags::empty();

        // face path
        let mut face = vg::Path::new();
        face.move_to(tl);
        face.line_to(tr);
        face.line_to(br);
        face.line_to(bl);
        face.close();

        // centre of the face for gradient origins
        let cx_f = x + w * 0.5;
        let cy_f = y + h * 0.5;
        let grad_r = w.max(h) * 0.9;

        match self.state {
            PipState::Off => {
                let colors = [
                    argb(255, 80, 78, 55),
                    argb(255, 45, 44, 30),
                    argb(255, 25, 24, 15),
                ];
                let pos: [f32; 3] = [0.0, 0.5, 1.0];

                if let Some(shader) = vg::shader::Shader::linear_gradient(
                    ((cx_f, y), (cx_f, y + h)),
                    colors.as_ref(),
                    Some(pos.as_ref()),
                    vg::TileMode::Clamp,
                    no_flags,
                    None,
                ) {
                    let mut paint = vg::Paint::default();
                    paint.set_shader(shader);
                    paint.set_anti_alias(true);
                    canvas.save();
                    canvas.clip_path(&face, None, true);
                    canvas.draw_paint(&paint);
                    canvas.restore();
                }
            }

            PipState::On => {
                // Base light fill — offset focal point toward bottom-left
                // so shadow falls at top-right
                let focal_x = cx_f - grad_r * 0.20;
                let focal_y = cy_f + grad_r * 0.25;
                let center = 255u8;
                let r = 255u8;
                let g = (5.0_f32).min(255.0) as u8;
                let b = (3.0_f32).min(255.0) as u8;
                let g_bright = ((g as u16) + 60).min(255) as u8;

                let colors = [
                    argb(255, center, g_bright, b),
                    argb(255, r, g, b),
                    argb(255, (r / 3).max(15), 3, 3),
                ];
                let pos: [f32; 3] = [0.0, 0.5, 1.0];

                if let Some(shader) = vg::shader::Shader::radial_gradient(
                    (focal_x, focal_y),
                    grad_r * 1.8,
                    colors.as_ref(),
                    Some(pos.as_ref()),
                    vg::TileMode::Clamp,
                    no_flags,
                    None,
                ) {
                    let mut paint = vg::Paint::default();
                    paint.set_shader(shader);
                    paint.set_anti_alias(true);
                    canvas.save();
                    canvas.clip_path(&face, None, true);
                    canvas.draw_paint(&paint);
                    canvas.restore();
                }

                // Rim glow
                let rim_colors = [argb(0, 200, 10, 200), argb(120, 220, 20, 220)];
                let rim_pos: [f32; 2] = [0.7, 1.0];

                if let Some(rim_shader) = vg::shader::Shader::radial_gradient(
                    (cx_f, cy_f),
                    grad_r,
                    rim_colors.as_ref(),
                    Some(rim_pos.as_ref()),
                    vg::TileMode::Clamp,
                    no_flags,
                    None,
                ) {
                    let mut rim_paint = vg::Paint::default();
                    rim_paint.set_shader(rim_shader);
                    rim_paint.set_anti_alias(true);
                    rim_paint.set_blend_mode(vg::BlendMode::Screen);
                    canvas.save();
                    canvas.clip_path(&face, None, true);
                    canvas.draw_paint(&rim_paint);
                    canvas.restore();
                }
            }
        }

        // outer border, top edge lighter, bottom - darkr
        {
            let mut top_paint = vg::Paint::default();
            top_paint.set_anti_alias(true);
            top_paint.set_style(vg::PaintStyle::Stroke);
            top_paint.set_stroke_width(1.2);
            top_paint.set_color(argb(180, 210, 205, 160));

            let mut top = vg::Path::new();
            top.move_to(tl);
            top.line_to(tr);
            canvas.draw_path(&top, &top_paint);

            let mut bot_paint = vg::Paint::default();
            bot_paint.set_anti_alias(true);
            bot_paint.set_style(vg::PaintStyle::Stroke);
            bot_paint.set_stroke_width(1.0);
            bot_paint.set_color(argb(120, 30, 28, 18));

            let mut bot = vg::Path::new();
            bot.move_to(bl);
            bot.line_to(br);
            canvas.draw_path(&bot, &bot_paint);

            // Side edges - medium
            let mut side_paint = vg::Paint::default();
            side_paint.set_anti_alias(true);
            side_paint.set_style(vg::PaintStyle::Stroke);
            side_paint.set_stroke_width(0.8);
            side_paint.set_color(argb(100, 130, 128, 90));

            let mut sides = vg::Path::new();
            sides.move_to(tl);
            sides.line_to(bl);
            sides.move_to(tr);
            sides.line_to(br);
            canvas.draw_path(&sides, &side_paint);
        }
    }
}
