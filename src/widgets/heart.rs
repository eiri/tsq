use vizia::prelude::*;
use vizia::vg;

use super::colors::{
    HEART_BORDER_BOT, HEART_BORDER_SIDE, HEART_BORDER_TOP, HEART_OFF_0, HEART_OFF_1, HEART_OFF_2,
    HEART_ON_B, HEART_ON_G, HEART_ON_G_BRIGHT, HEART_ON_R, HEART_RIM_0, HEART_RIM_1, argb,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HeartState {
    Off,
    On,
}

pub struct Heart {
    state: HeartState,
}

impl Heart {
    pub fn new(cx: &mut Context, state: HeartState) -> Handle<'_, Self> {
        Self { state }.build(cx, |_cx| {})
    }
}

impl View for Heart {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let w = bounds.w;
        let h = bounds.h;
        let x = bounds.x;
        let y = bounds.y;

        let ideal_w_from_h = h * (3.0_f32.sqrt() / 2.0);
        let (base, tri_h) = if ideal_w_from_h <= w {
            (h, ideal_w_from_h)
        } else {
            let b = w / (3.0_f32.sqrt() / 2.0);
            (b, w)
        };

        let top_y = y + (h - base) * 0.5;
        let bot_y = top_y + base;
        let left = x + (w - tri_h) * 0.5;
        let right = left + tri_h;

        // tl - top-left (left edge, top end)
        // bl - bottom-left (left edge, bottom end)
        // rp - right apex (pointing right)
        let tl = (left, top_y);
        let bl = (left, bot_y);
        let rp = (right, top_y + base * 0.5);

        let no_flags = vg::gradient_shader::Flags::empty();

        let face = {
            let mut pb = vg::PathBuilder::new();
            pb.move_to(tl);
            pb.line_to(bl);
            pb.line_to(rp);
            pb.close();
            pb.snapshot()
        };

        let cx_f = left + tri_h / 3.0;
        let cy_f = top_y + base * 0.5;
        let grad_r = base.max(tri_h) * 0.9;

        match self.state {
            HeartState::Off => {
                let colors = [
                    argb(HEART_OFF_0.0, HEART_OFF_0.1, HEART_OFF_0.2, HEART_OFF_0.3),
                    argb(HEART_OFF_1.0, HEART_OFF_1.1, HEART_OFF_1.2, HEART_OFF_1.3),
                    argb(HEART_OFF_2.0, HEART_OFF_2.1, HEART_OFF_2.2, HEART_OFF_2.3),
                ];
                let pos: [f32; 3] = [0.0, 0.5, 1.0];

                if let Some(shader) = vg::gradient_shader::linear(
                    ((left, cy_f), (rp.0, cy_f)),
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

            HeartState::On => {
                let focal_x = cx_f - grad_r * 0.18;
                let focal_y = cy_f - grad_r * 0.20;

                let colors = [
                    argb(255, HEART_ON_R, HEART_ON_G_BRIGHT, HEART_ON_B),
                    argb(255, HEART_ON_R, HEART_ON_G, HEART_ON_B),
                    argb(
                        255,
                        (HEART_ON_R / 3).max(15),
                        (HEART_ON_G / 4).max(10),
                        HEART_ON_B,
                    ),
                ];
                let pos: [f32; 3] = [0.0, 0.5, 1.0];

                if let Some(shader) = vg::gradient_shader::radial(
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

                let rim_colors = [
                    argb(HEART_RIM_0.0, HEART_RIM_0.1, HEART_RIM_0.2, HEART_RIM_0.3),
                    argb(HEART_RIM_1.0, HEART_RIM_1.1, HEART_RIM_1.2, HEART_RIM_1.3),
                ];
                let rim_pos: [f32; 2] = [0.65, 1.0];

                if let Some(rim_shader) = vg::gradient_shader::radial(
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

        {
            let mut top_paint = vg::Paint::default();
            top_paint.set_anti_alias(true);
            top_paint.set_style(vg::PaintStyle::Stroke);
            top_paint.set_stroke_width(1.2);
            top_paint.set_color(argb(
                HEART_BORDER_TOP.0,
                HEART_BORDER_TOP.1,
                HEART_BORDER_TOP.2,
                HEART_BORDER_TOP.3,
            ));

            let top_edge = {
                let mut pb = vg::PathBuilder::new();
                pb.move_to(tl);
                pb.line_to(bl);
                pb.snapshot()
            };
            canvas.draw_path(&top_edge, &top_paint);
        }

        {
            let mut bot_paint = vg::Paint::default();
            bot_paint.set_anti_alias(true);
            bot_paint.set_style(vg::PaintStyle::Stroke);
            bot_paint.set_stroke_width(1.0);
            bot_paint.set_color(argb(
                HEART_BORDER_BOT.0,
                HEART_BORDER_BOT.1,
                HEART_BORDER_BOT.2,
                HEART_BORDER_BOT.3,
            ));

            let bot_edges = {
                let mut pb = vg::PathBuilder::new();
                pb.move_to(tl);
                pb.line_to(rp);
                pb.snapshot()
            };
            canvas.draw_path(&bot_edges, &bot_paint);
        }

        {
            let mut side_paint = vg::Paint::default();
            side_paint.set_anti_alias(true);
            side_paint.set_style(vg::PaintStyle::Stroke);
            side_paint.set_stroke_width(0.8);
            side_paint.set_color(argb(
                HEART_BORDER_SIDE.0,
                HEART_BORDER_SIDE.1,
                HEART_BORDER_SIDE.2,
                HEART_BORDER_SIDE.3,
            ));

            let right_edge = {
                let mut pb = vg::PathBuilder::new();
                pb.move_to(bl);
                pb.line_to(rp);
                pb.snapshot()
            };
            canvas.draw_path(&right_edge, &side_paint);
        }
    }
}
