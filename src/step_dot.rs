use vizia::prelude::*;
use vizia::vg;

#[derive(Debug, Clone, Copy, PartialEq, Data)]
pub enum StepDotState {
    Off,
    HalfDim,
    Dim,
    On,
}
pub struct StepDot {
    state: StepDotState,
}

impl StepDot {
    pub fn new(cx: &mut Context, state: StepDotState) -> Handle<'_, Self> {
        Self { state }.build(cx, |_cx| {})
    }
}

// FIXME! - common helper later
#[inline]
fn argb(a: u8, r: u8, g: u8, b: u8) -> vg::Color {
    vg::Color::from_argb(a, r, g, b)
}

impl View for StepDot {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let size = bounds.w.min(bounds.h);
        let cx_f = bounds.x + bounds.w * 0.5;
        let cy_f = bounds.y + bounds.h * 0.5;
        let radius = size * 0.5;

        // (pit_depth, led_alpha, glow_alpha, glow_factor)
        let (pit_depth, led_alpha, glow_alpha, glow_factor): (f32, f32, f32, f32) = match self.state
        {
            StepDotState::Off => (1.00, 0.00, 0.00, 0.0),
            StepDotState::HalfDim => (0.85, 0.52, 0.06, 1.3),
            StepDotState::Dim => (0.70, 0.76, 0.18, 1.6),
            StepDotState::On => (0.50, 1.00, 0.35, 2.0),
        };

        // flags argument for radial_gradient
        let no_flags = vg::gradient_shader::Flags::empty();

        // outer halo
        if glow_alpha > 0.0 {
            let halo_r = radius * glow_factor;
            let colors = [
                argb((glow_alpha * 180.0) as u8, 220, 30, 10),
                argb((glow_alpha * 80.0) as u8, 180, 20, 5),
                argb(0, 180, 20, 5),
            ];
            let pos: [f32; 3] = [0.0, 0.55, 1.0];

            if let Some(shader) = vg::shader::Shader::radial_gradient(
                (cx_f, cy_f),
                halo_r,
                colors.as_ref(),
                Some(pos.as_ref()),
                vg::TileMode::Clamp,
                no_flags,
                None,
            ) {
                let mut paint = vg::Paint::default();
                paint.set_shader(shader);
                paint.set_anti_alias(true);
                canvas.draw_circle((cx_f, cy_f), halo_r, &paint);
            }
        }

        // bezel rim
        {
            let colors = [
                argb(255, 210, 208, 175),
                argb(255, 175, 172, 140),
                argb(255, 130, 128, 100),
            ];
            let pos: [f32; 3] = [0.0, 0.40, 1.0];

            // linear_gradient: (pt0, pt1), colors, pos, tile_mode, local_matrix
            if let Some(shader) = vg::shader::Shader::linear_gradient(
                ((cx_f, cy_f - radius), (cx_f, cy_f + radius)),
                colors.as_ref(),
                Some(pos.as_ref()),
                vg::TileMode::Clamp,
                no_flags,
                None,
            ) {
                let mut paint = vg::Paint::default();
                paint.set_shader(shader);
                paint.set_anti_alias(true);
                canvas.draw_circle((cx_f, cy_f), radius, &paint);
            }
        }

        // pit interior
        {
            let pit_r = radius
                * match self.state {
                    StepDotState::Off => 0.82,
                    StepDotState::HalfDim => 0.88,
                    StepDotState::Dim => 0.86,
                    StepDotState::On => 0.82,
                };
            let dark = (100.0 * pit_depth) as u8;
            let mid = (140.0 * pit_depth) as u8;
            let colors = [
                argb(255, dark, dark, dark / 2),
                argb(255, mid, mid, mid / 2),
                argb(255, 180, 175, 110),
            ];
            let pos: [f32; 3] = [0.0, 0.7, 1.0];

            if let Some(shader) = vg::shader::Shader::radial_gradient(
                (cx_f, cy_f + pit_r * 0.15),
                pit_r,
                colors.as_ref(),
                Some(pos.as_ref()),
                vg::TileMode::Clamp,
                no_flags,
                None,
            ) {
                let mut paint = vg::Paint::default();
                paint.set_shader(shader);
                paint.set_anti_alias(true);
                canvas.draw_circle((cx_f, cy_f), pit_r, &paint);
            }
        }

        // led fill
        let led_r = radius
            * match self.state {
                StepDotState::Off => 0.80,
                StepDotState::HalfDim => 0.86, // matches pit_r
                StepDotState::Dim => 0.84,
                StepDotState::On => 0.80,
            };

        if led_alpha > 0.0 {
            let r = (40.0 + 215.0 * led_alpha).min(255.0) as u8;
            let g = (5.0 * led_alpha).min(255.0) as u8;
            let b = (3.0 * led_alpha).min(255.0) as u8;
            let g_bright = ((g as u16) + 60).min(255) as u8;
            let center = (255.0 * led_alpha).min(255.0) as u8;

            let colors = [
                argb(255, center, g_bright, b),
                argb(255, r, g, b),
                argb(255, (r / 3).max(15), 3, 3),
            ];
            let pos: [f32; 3] = [0.0, 0.5, 1.0];

            if let Some(shader) = vg::shader::Shader::radial_gradient(
                (cx_f - led_r * 0.20, cy_f + led_r * 0.25),
                led_r * 2.1,
                colors.as_ref(),
                Some(pos.as_ref()),
                vg::TileMode::Clamp,
                no_flags,
                None,
            ) {
                let mut paint = vg::Paint::default();
                paint.set_shader(shader);
                paint.set_anti_alias(true);
                canvas.draw_circle((cx_f, cy_f), led_r, &paint);
            }

            // hot-spot on Dim and On
            if led_alpha > 0.5 {
                let spec_r = led_r * 0.22;
                let spec_cx = cx_f - led_r * 0.22;
                let spec_cy = cy_f + led_r * 0.22;
                let spec_a = ((led_alpha - 0.5) / 0.5 * 180.0) as u8;

                let mut paint = vg::Paint::default();
                paint.set_anti_alias(true);
                paint.set_color(argb(spec_a, 255, 200, 180));
                canvas.draw_circle((spec_cx, spec_cy), spec_r, &paint);
            }
        } else {
            // Off
            let mut paint = vg::Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(argb(255, 18, 8, 8));
            canvas.draw_circle((cx_f, cy_f), led_r, &paint);
        }

        // rim crescent highlight
        {
            let mut paint = vg::Paint::default();
            paint.set_anti_alias(true);
            paint.set_style(vg::PaintStyle::Stroke);
            paint.set_stroke_width(radius * 0.06);
            paint.set_color(argb(60, 200, 200, 200));

            let mut path = vg::Path::new();
            let arc_rect = vg::Rect::from_xywh(
                cx_f - radius * 0.78,
                cy_f - radius * 0.78,
                radius * 1.56,
                radius * 1.56,
            );
            path.arc_to(arc_rect, 210.0, 120.0, false);
            canvas.draw_path(&path, &paint);
        }
    }
}
