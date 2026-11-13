//! Warm dark-mode skeuomorphic Vizia GUI for `J-SAT`.

use std::f32::consts::PI;
use std::sync::Arc;
use std::time::Duration;

use j_sat_dsp::{SCOPE_LEN, ScopeRing};
use nih_plug::prelude::{Editor, Param};
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::vizia::vg;
use nih_plug_vizia::widgets::param_base::ParamWidgetBase;
use nih_plug_vizia::widgets::util::ModifiersExt;
use nih_plug_vizia::widgets::RawParamEvent;
use nih_plug_vizia::{assets, create_vizia_editor, ViziaState, ViziaTheming};

use crate::JSatPluginParams;

const BASE_WIDTH: u32 = 660;
const BASE_HEIGHT: u32 = 435;
const DEFAULT_SCALE: f64 = 1.25;

/// Vertical pixels needed to sweep a rotary knob or numeric box from 0% to 100%.
const KNOB_DRAG_PIXELS: f32 = 160.0;
/// Fine drag multiplier when holding Shift.
const KNOB_FINE_MULTIPLIER: f32 = 0.12;

#[derive(Lens)]
pub(crate) struct UiData {
    pub params: Arc<JSatPluginParams>,
}

impl Model for UiData {}

pub(crate) fn default_state() -> Arc<ViziaState> {
    ViziaState::new_with_default_scale_factor(|| (BASE_WIDTH, BASE_HEIGHT), DEFAULT_SCALE)
}

#[derive(Debug, Clone, Copy)]
struct DragState {
    start_y: f32,
    start_val: f32,
}

/// Skeuomorphic 3D rotary knob widget rendered via `femtovg` (`vizia::vg`).
pub struct RotaryKnob {
    param_base: ParamWidgetBase,
    drag_active: bool,
    drag_state: Option<DragState>,
    scrolled_lines: f32,
    bipolar: bool,
    accent_rgb: (u8, u8, u8),
}

impl RotaryKnob {
    pub fn new<'a, L, Params, P, FMap>(
        cx: &'a mut Context,
        params: L,
        params_to_param: FMap,
        bipolar: bool,
        accent_rgb: (u8, u8, u8),
    ) -> Handle<'a, Self>
    where
        L: Lens<Target = Params> + Clone,
        Params: 'static,
        P: Param + 'static,
        FMap: Fn(&Params) -> &P + Copy + 'static,
    {
        Self {
            param_base: ParamWidgetBase::new(cx, params.clone(), params_to_param),
            drag_active: false,
            drag_state: None,
            scrolled_lines: 0.0,
            bipolar,
            accent_rgb,
        }
        .build(
            cx,
            ParamWidgetBase::build_view(params, params_to_param, move |cx, param_data| {
                let norm_lens =
                    param_data.make_lens(|param| param.modulated_normalized_value());
                Binding::new(cx, norm_lens, |cx, _| {
                    cx.needs_redraw();
                    Element::new(cx).hoverable(false);
                });
            }),
        )
    }

    fn reset_to_default(&mut self, cx: &mut EventContext) {
        if self.drag_active {
            self.drag_active = false;
            self.drag_state = None;
            cx.release();
            cx.set_active(false);
            self.param_base.end_set_parameter(cx);
        }
        let target = self.param_base.default_normalized_value();
        self.param_base.begin_set_parameter(cx);
        self.param_base.set_normalized_value(cx, target);
        self.param_base.end_set_parameter(cx);
        cx.needs_redraw();
    }
}

impl View for RotaryKnob {
    fn element(&self) -> Option<&'static str> {
        Some("rotary-knob")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|raw_param_event, _| {
            if let RawParamEvent::ParametersChanged = raw_param_event {
                cx.needs_redraw();
            }
        });

        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left)
            | WindowEvent::MouseTripleClick(MouseButton::Left) => {
                if cx.modifiers().command() || cx.modifiers().contains(Modifiers::CTRL) {
                    self.reset_to_default(cx);
                } else {
                    self.drag_active = true;
                    cx.capture();
                    cx.focus();
                    cx.set_active(true);
                    self.param_base.begin_set_parameter(cx);
                    self.drag_state = Some(DragState {
                        start_y: cx.mouse().cursory,
                        start_val: self.param_base.unmodulated_normalized_value(),
                    });
                    cx.needs_redraw();
                }
                meta.consume();
            }
            WindowEvent::MouseDoubleClick(MouseButton::Left)
            | WindowEvent::MouseDown(MouseButton::Right)
            | WindowEvent::MouseDoubleClick(MouseButton::Right)
            | WindowEvent::MouseTripleClick(MouseButton::Right) => {
                self.reset_to_default(cx);
                meta.consume();
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if self.drag_active {
                    self.drag_active = false;
                    self.drag_state = None;
                    cx.release();
                    cx.set_active(false);
                    self.param_base.end_set_parameter(cx);
                    cx.needs_redraw();
                    meta.consume();
                }
            }
            WindowEvent::MouseMove(_x, y) => {
                if self.drag_active {
                    let state = self.drag_state.get_or_insert(DragState {
                        start_y: *y,
                        start_val: self.param_base.unmodulated_normalized_value(),
                    });

                    let sensitivity = if cx.modifiers().shift() {
                        KNOB_FINE_MULTIPLIER
                    } else {
                        1.0
                    };

                    let dy = (state.start_y - *y) / (KNOB_DRAG_PIXELS * cx.scale_factor());
                    let new_val = (state.start_val + dy * sensitivity).clamp(0.0, 1.0);
                    self.param_base.set_normalized_value(cx, new_val);

                    state.start_y = *y;
                    state.start_val = new_val;

                    cx.needs_redraw();
                    meta.consume();
                }
            }
            WindowEvent::MouseScroll(_scroll_x, scroll_y) => {
                self.scrolled_lines += scroll_y;
                if self.scrolled_lines.abs() >= 0.5 {
                    let finer = cx.modifiers().shift();
                    if !self.drag_active {
                        self.param_base.begin_set_parameter(cx);
                    }

                    let mut current = self.param_base.unmodulated_normalized_value();
                    let step = if finer { 0.004 } else { 0.02 };
                    let delta = self.scrolled_lines * step;
                    current = (current + delta).clamp(0.0, 1.0);
                    self.param_base.set_normalized_value(cx, current);
                    self.scrolled_lines = 0.0;

                    if !self.drag_active {
                        self.param_base.end_set_parameter(cx);
                    }
                    cx.needs_redraw();
                }
                meta.consume();
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let bounds = cx.bounds();
        if bounds.w <= 0.0 || bounds.h <= 0.0 {
            return;
        }

        let dpx = cx.scale_factor();
        let center_x = bounds.x + bounds.w * 0.5;
        let center_y = bounds.y + bounds.h * 0.52;
        let max_r = (bounds.w.min(bounds.h) * 0.5) - 2.0 * dpx;

        let arc_r = max_r * 0.90;
        let body_r = max_r * 0.70;
        let cap_r = max_r * 0.57;

        let norm = self.param_base.modulated_normalized_value().clamp(0.0, 1.0);

        let start_angle = 0.75 * PI;
        let sweep_angle = 1.50 * PI;
        let end_angle = start_angle + sweep_angle;
        let val_angle = start_angle + norm * sweep_angle;

        let num_ticks = 11;
        for i in 0..num_ticks {
            let t = i as f32 / (num_ticks - 1) as f32;
            let a = start_angle + t * sweep_angle;
            let r_in = max_r * 0.94;
            let r_out = max_r * 1.02;
            let mut tick = vg::Path::new();
            tick.move_to(center_x + a.cos() * r_in, center_y + a.sin() * r_in);
            tick.line_to(center_x + a.cos() * r_out, center_y + a.sin() * r_out);
            let is_major = i == 0 || i == num_ticks - 1 || i == num_ticks / 2;
            let mut paint = if is_major {
                vg::Paint::color(vg::Color::rgba(218, 198, 175, 215))
            } else {
                vg::Paint::color(vg::Color::rgba(155, 138, 120, 155))
            };
            paint.set_line_width((if is_major { 1.4 } else { 1.0 }) * dpx);
            canvas.stroke_path(&tick, &paint);
        }

        let mut track_path = vg::Path::new();
        track_path.arc(center_x, center_y, arc_r, start_angle, end_angle, vg::Solidity::Hole);
        let mut track_paint = vg::Paint::color(vg::Color::rgba(30, 25, 21, 245));
        track_paint.set_line_width(3.4 * dpx);
        track_paint.set_line_cap(vg::LineCap::Round);
        canvas.stroke_path(&track_path, &track_paint);

        let (ar, ag, ab) = self.accent_rgb;
        let (arc_a0, arc_a1) = if self.bipolar {
            let mid_angle = start_angle + 0.5 * sweep_angle;
            if val_angle >= mid_angle {
                (mid_angle, val_angle)
            } else {
                (val_angle, mid_angle)
            }
        } else {
            (start_angle, val_angle)
        };

        if (arc_a1 - arc_a0).abs() > 0.015 {
            let mut glow_path = vg::Path::new();
            glow_path.arc(center_x, center_y, arc_r, arc_a0, arc_a1, vg::Solidity::Hole);
            let mut glow_paint = vg::Paint::color(vg::Color::rgba(ar, ag, ab, 80));
            glow_paint.set_line_width(6.5 * dpx);
            glow_paint.set_line_cap(vg::LineCap::Round);
            canvas.stroke_path(&glow_path, &glow_paint);

            let mut val_path = vg::Path::new();
            val_path.arc(center_x, center_y, arc_r, arc_a0, arc_a1, vg::Solidity::Hole);
            let mut val_paint = vg::Paint::color(vg::Color::rgba(ar, ag, ab, 250));
            val_paint.set_line_width(2.8 * dpx);
            val_paint.set_line_cap(vg::LineCap::Round);
            canvas.stroke_path(&val_path, &val_paint);
        }

        let mut shadow_path = vg::Path::new();
        shadow_path.circle(center_x, center_y + 3.2 * dpx, body_r * 1.03);
        let shadow_paint = vg::Paint::radial_gradient(
            center_x,
            center_y + 3.2 * dpx,
            body_r * 0.5,
            body_r * 1.12,
            vg::Color::rgba(14, 11, 8, 200),
            vg::Color::rgba(14, 11, 8, 0),
        );
        canvas.fill_path(&shadow_path, &shadow_paint);

        let mut body_path = vg::Path::new();
        body_path.circle(center_x, center_y, body_r);
        let body_paint = vg::Paint::linear_gradient(
            center_x,
            center_y - body_r,
            center_x,
            center_y + body_r,
            vg::Color::rgb(112, 98, 86),
            vg::Color::rgb(50, 42, 36),
        );
        canvas.fill_path(&body_path, &body_paint);

        let mut rim_stroke = vg::Paint::color(vg::Color::rgba(24, 20, 17, 235));
        rim_stroke.set_line_width(1.3 * dpx);
        canvas.stroke_path(&body_path, &rim_stroke);

        let serrations = 24;
        for i in 0..serrations {
            let a = (i as f32 / serrations as f32) * 2.0 * PI + norm * 1.2;
            let r0 = cap_r * 1.02;
            let r1 = body_r * 0.96;
            let mut groove = vg::Path::new();
            groove.move_to(center_x + a.cos() * r0, center_y + a.sin() * r0);
            groove.line_to(center_x + a.cos() * r1, center_y + a.sin() * r1);
            let mut gpaint = vg::Paint::color(vg::Color::rgba(32, 26, 22, 135));
            gpaint.set_line_width(1.0 * dpx);
            canvas.stroke_path(&groove, &gpaint);
        }

        let mut cap_path = vg::Path::new();
        cap_path.circle(center_x, center_y, cap_r);
        let cap_paint = vg::Paint::radial_gradient(
            center_x - cap_r * 0.3,
            center_y - cap_r * 0.35,
            cap_r * 0.08,
            cap_r * 1.1,
            vg::Color::rgb(138, 122, 108),
            vg::Color::rgb(62, 53, 46),
        );
        canvas.fill_path(&cap_path, &cap_paint);

        let mut cap_ring = vg::Paint::linear_gradient(
            center_x,
            center_y - cap_r,
            center_x,
            center_y + cap_r,
            vg::Color::rgba(225, 204, 182, 140),
            vg::Color::rgba(28, 23, 19, 200),
        );
        cap_ring.set_line_width(1.1 * dpx);
        canvas.stroke_path(&cap_path, &cap_ring);

        let p_in = cap_r * 0.24;
        let p_out = cap_r * 0.88;
        let px0 = center_x + val_angle.cos() * p_in;
        let py0 = center_y + val_angle.sin() * p_in;
        let px1 = center_x + val_angle.cos() * p_out;
        let py1 = center_y + val_angle.sin() * p_out;

        let mut ptr_shadow = vg::Path::new();
        ptr_shadow.move_to(px0, py0 + 1.0 * dpx);
        ptr_shadow.line_to(px1, py1 + 1.0 * dpx);
        let mut ps_paint = vg::Paint::color(vg::Color::rgba(0, 0, 0, 195));
        ps_paint.set_line_width(3.0 * dpx);
        ps_paint.set_line_cap(vg::LineCap::Round);
        canvas.stroke_path(&ptr_shadow, &ps_paint);

        let mut ptr_path = vg::Path::new();
        ptr_path.move_to(px0, py0);
        ptr_path.line_to(px1, py1);
        let mut ptr_paint = vg::Paint::color(vg::Color::rgb(255, 244, 228));
        ptr_paint.set_line_width(2.1 * dpx);
        ptr_paint.set_line_cap(vg::LineCap::Round);
        canvas.stroke_path(&ptr_path, &ptr_paint);
    }
}

/// Skeuomorphic illuminated hardware push-button / LED switch.
pub struct LedToggleSwitch {
    param_base: ParamWidgetBase,
    accent_rgb: (u8, u8, u8),
}

impl LedToggleSwitch {
    pub fn new<'a, L, Params, P, FMap>(
        cx: &'a mut Context,
        params: L,
        params_to_param: FMap,
        off_label: &'static str,
        on_label: &'static str,
        accent_rgb: (u8, u8, u8),
    ) -> Handle<'a, Self>
    where
        L: Lens<Target = Params> + Clone,
        Params: 'static,
        P: Param + 'static,
        FMap: Fn(&Params) -> &P + Copy + 'static,
    {
        Self {
            param_base: ParamWidgetBase::new(cx, params.clone(), params_to_param),
            accent_rgb,
        }
        .build(
            cx,
            ParamWidgetBase::build_view(params.clone(), params_to_param, move |cx, param_data| {
                let is_on_lens =
                    param_data.make_lens(|param| param.modulated_normalized_value() >= 0.5);
                Binding::new(cx, is_on_lens, move |cx, is_on| {
                    cx.needs_redraw();
                    let label_text = if is_on.get(cx) { on_label } else { off_label };
                    HStack::new(cx, |cx| {
                        Element::new(cx)
                            .width(Pixels(16.0))
                            .height(Stretch(1.0))
                            .hoverable(false);
                        Label::new(cx, label_text)
                            .class("switch-label")
                            .font_family(vec![FamilyOwned::Name(String::from(assets::NOTO_SANS))])
                            .font_weight(FontWeightKeyword::Bold)
                            .hoverable(false);
                    })
                    .child_top(Stretch(1.0))
                    .child_bottom(Stretch(1.0))
                    .child_left(Pixels(7.0))
                    .child_right(Pixels(9.0))
                    .col_between(Pixels(4.0))
                    .hoverable(false);
                });
            }),
        )
        .checked(ParamWidgetBase::make_lens(
            params,
            params_to_param,
            |param| param.modulated_normalized_value() >= 0.5,
        ))
    }

    fn toggle(&self, cx: &mut EventContext) {
        let current = self.param_base.unmodulated_normalized_value();
        let next = if current >= 0.5 { 0.0 } else { 1.0 };
        self.param_base.begin_set_parameter(cx);
        self.param_base.set_normalized_value(cx, next);
        self.param_base.end_set_parameter(cx);
        cx.needs_redraw();
    }
}

impl View for LedToggleSwitch {
    fn element(&self) -> Option<&'static str> {
        Some("led-toggle")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|raw_param_event, _| {
            if let RawParamEvent::ParametersChanged = raw_param_event {
                cx.set_checked(self.param_base.modulated_normalized_value() >= 0.5);
                cx.needs_redraw();
            }
        });

        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left)
            | WindowEvent::MouseDoubleClick(MouseButton::Left)
            | WindowEvent::MouseTripleClick(MouseButton::Left) => {
                if cx.modifiers().command() || cx.modifiers().contains(Modifiers::CTRL) {
                    let def = self.param_base.default_normalized_value();
                    self.param_base.begin_set_parameter(cx);
                    self.param_base.set_normalized_value(cx, def);
                    self.param_base.end_set_parameter(cx);
                    cx.needs_redraw();
                } else {
                    self.toggle(cx);
                }
                meta.consume();
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let bounds = cx.bounds();
        if bounds.w <= 0.0 || bounds.h <= 0.0 {
            return;
        }

        let dpx = cx.scale_factor();
        let is_on = self.param_base.modulated_normalized_value() >= 0.5;
        let (ar, ag, ab) = self.accent_rgb;

        let r = 4.0 * dpx;
        let mut bg = vg::Path::new();
        bg.rounded_rect(bounds.x, bounds.y, bounds.w, bounds.h, r);

        let housing_paint = if is_on {
            vg::Paint::linear_gradient(
                bounds.x,
                bounds.y,
                bounds.x,
                bounds.y + bounds.h,
                vg::Color::rgb(82, 70, 60),
                vg::Color::rgb(52, 44, 38),
            )
        } else {
            vg::Paint::linear_gradient(
                bounds.x,
                bounds.y,
                bounds.x,
                bounds.y + bounds.h,
                vg::Color::rgb(54, 46, 40),
                vg::Color::rgb(38, 32, 28),
            )
        };
        canvas.fill_path(&bg, &housing_paint);

        let mut border = vg::Paint::color(if is_on {
            vg::Color::rgba(ar, ag, ab, 175)
        } else {
            vg::Color::rgba(24, 20, 17, 235)
        });
        border.set_line_width(1.1 * dpx);
        canvas.stroke_path(&bg, &border);

        let led_x = bounds.x + 13.0 * dpx;
        let led_y = bounds.y + bounds.h * 0.5;
        let led_r = 4.0 * dpx;

        if is_on {
            let mut glow = vg::Path::new();
            glow.circle(led_x, led_y, led_r * 2.2);
            let glow_paint = vg::Paint::radial_gradient(
                led_x,
                led_y,
                led_r * 0.2,
                led_r * 2.2,
                vg::Color::rgba(ar, ag, ab, 165),
                vg::Color::rgba(ar, ag, ab, 0),
            );
            canvas.fill_path(&glow, &glow_paint);
        }

        let mut lamp = vg::Path::new();
        lamp.circle(led_x, led_y, led_r);
        let lamp_paint = if is_on {
            vg::Paint::radial_gradient(
                led_x - led_r * 0.25,
                led_y - led_r * 0.25,
                led_r * 0.1,
                led_r,
                vg::Color::rgb(255, 250, 240),
                vg::Color::rgb(ar, ag, ab),
            )
        } else {
            vg::Paint::radial_gradient(
                led_x - led_r * 0.2,
                led_y - led_r * 0.2,
                led_r * 0.1,
                led_r,
                vg::Color::rgb(92, 80, 70),
                vg::Color::rgb(44, 38, 33),
            )
        };
        canvas.fill_path(&lamp, &lamp_paint);

        let mut lamp_rim = vg::Paint::color(vg::Color::rgba(12, 10, 8, 225));
        lamp_rim.set_line_width(1.0 * dpx);
        canvas.stroke_path(&lamp, &lamp_rim);
    }
}

/// Real-time scrolling peak/clipper scope rendered via `femtovg` at 60 fps.
pub struct ClipperScope {
    scope: Arc<ScopeRing>,
    params: Arc<JSatPluginParams>,
}

impl ClipperScope {
    pub fn new(
        cx: &mut Context,
        scope: Arc<ScopeRing>,
        params: Arc<JSatPluginParams>,
    ) -> Handle<'_, Self> {
        let timer = cx.add_timer(Duration::from_millis(16), None, |cx, action| {
            if matches!(action, TimerAction::Tick(_)) {
                cx.needs_redraw();
            }
        });
        cx.start_timer(timer);

        Self { scope, params }.build(cx, |_| {})
    }
}

impl View for ClipperScope {
    fn element(&self) -> Option<&'static str> {
        Some("clipper-scope")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let bounds = cx.bounds();
        if bounds.w <= 4.0 || bounds.h <= 4.0 {
            return;
        }

        let dpx = cx.scale_factor();
        let radius = 5.0 * dpx;

        // 1. Dark recessed background (#181512)
        let mut bg = vg::Path::new();
        bg.rounded_rect(bounds.x, bounds.y, bounds.w, bounds.h, radius);
        let bg_paint = vg::Paint::color(vg::Color::rgb(24, 21, 18));
        canvas.fill_path(&bg, &bg_paint);

        canvas.save();
        canvas.scissor(bounds.x + dpx, bounds.y + dpx, bounds.w - 2.0 * dpx, bounds.h - 2.0 * dpx);

        let pad_y = 4.0 * dpx;
        let top = bounds.y + pad_y;
        let inner_h = (bounds.h - 2.0 * pad_y).max(1.0);
        let bottom = top + inner_h;

        let db_min = -24.0_f32;
        let db_max = 6.0_f32;
        let db_span = db_max - db_min;
        let db_to_y = |db: f32| -> f32 {
            let t = ((db - db_min) / db_span).clamp(0.0, 1.0);
            bottom - t * inner_h
        };

        let y_zero = db_to_y(0.0);
        let y_neg3 = db_to_y(-3.0);

        // 2. Soft-knee compression zone (-3 dBFS to 0 dBFS) when SOFT CLIP is active
        let is_soft = self.params.soft_clip.value();
        if is_soft {
            let mut knee_rect = vg::Path::new();
            knee_rect.rect(bounds.x, y_zero, bounds.w, y_neg3 - y_zero);
            let knee_paint = vg::Paint::color(vg::Color::rgba(245, 179, 73, 28));
            canvas.fill_path(&knee_rect, &knee_paint);
        }

        // 3. Subtle horizontal dB grid lines (-6, -12, -18 dBFS)
        for db in [-6.0_f32, -12.0, -18.0] {
            let y = db_to_y(db);
            let mut grid = vg::Path::new();
            grid.move_to(bounds.x, y);
            grid.line_to(bounds.x + bounds.w, y);
            let mut gpaint = vg::Paint::color(vg::Color::rgba(90, 76, 64, 95));
            gpaint.set_line_width(1.0 * dpx);
            canvas.stroke_path(&grid, &gpaint);
        }

        // 4. Snapshot lock-free audio telemetry
        let mut pre = [0.0_f32; SCOPE_LEN];
        let mut post = [0.0_f32; SCOPE_LEN];
        self.scope.snapshot(&mut pre, &mut post);

        let amp_to_y = |amp: f32| -> f32 {
            if amp <= 1e-5 {
                bottom
            } else {
                let db = 20.0 * amp.log10();
                db_to_y(db)
            }
        };

        let step_x = bounds.w / (SCOPE_LEN - 1) as f32;

        // 5. Pre-clip envelope (excess energy above post-clip highlighted in bright orange/amber)
        let mut pre_fill = vg::Path::new();
        let mut pre_line = vg::Path::new();
        pre_fill.move_to(bounds.x, bottom);
        for (i, &p) in pre.iter().enumerate() {
            let x = bounds.x + i as f32 * step_x;
            let y = amp_to_y(p);
            pre_fill.line_to(x, y);
            if i == 0 {
                pre_line.move_to(x, y);
            } else {
                pre_line.line_to(x, y);
            }
        }
        pre_fill.line_to(bounds.x + bounds.w, bottom);
        pre_fill.close();

        let pre_fill_paint = vg::Paint::color(vg::Color::rgba(245, 130, 48, 125));
        canvas.fill_path(&pre_fill, &pre_fill_paint);
        let mut pre_line_paint = vg::Paint::color(vg::Color::rgba(255, 165, 70, 220));
        pre_line_paint.set_line_width(1.25 * dpx);
        canvas.stroke_path(&pre_line, &pre_line_paint);

        // 6. Post-clip envelope silhouette
        let mut post_fill = vg::Path::new();
        let mut post_line = vg::Path::new();
        post_fill.move_to(bounds.x, bottom);
        for (i, &p) in post.iter().enumerate() {
            let x = bounds.x + i as f32 * step_x;
            let y = amp_to_y(p);
            post_fill.line_to(x, y);
            if i == 0 {
                post_line.move_to(x, y);
            } else {
                post_line.line_to(x, y);
            }
        }
        post_fill.line_to(bounds.x + bounds.w, bottom);
        post_fill.close();

        let post_fill_paint = vg::Paint::color(vg::Color::rgba(242, 203, 97, 90));
        canvas.fill_path(&post_fill, &post_fill_paint);
        let mut post_line_paint = vg::Paint::color(vg::Color::rgba(255, 224, 140, 240));
        post_line_paint.set_line_width(1.5 * dpx);
        canvas.stroke_path(&post_line, &post_line_paint);

        // 7. -3 dBFS dashed soft-knee threshold line
        if is_soft {
            let dash = 4.0 * dpx;
            let gap = 4.0 * dpx;
            let mut x = bounds.x;
            let mut knee_line = vg::Path::new();
            while x < bounds.x + bounds.w {
                knee_line.move_to(x, y_neg3);
                knee_line.line_to((x + dash).min(bounds.x + bounds.w), y_neg3);
                x += dash + gap;
            }
            let mut knee_paint = vg::Paint::color(vg::Color::rgba(245, 179, 73, 150));
            knee_paint.set_line_width(1.1 * dpx);
            canvas.stroke_path(&knee_line, &knee_paint);
        }

        // 8. Crisp 0.0 dBFS ceiling line
        let mut ceil_line = vg::Path::new();
        ceil_line.move_to(bounds.x, y_zero);
        ceil_line.line_to(bounds.x + bounds.w, y_zero);
        let mut ceil_paint = vg::Paint::color(vg::Color::rgba(255, 211, 130, 200));
        ceil_paint.set_line_width(1.3 * dpx);
        canvas.stroke_path(&ceil_line, &ceil_paint);

        canvas.restore();

        // Outer recessed border
        let mut border = vg::Path::new();
        border.rounded_rect(bounds.x, bounds.y, bounds.w, bounds.h, radius);
        let mut border_paint = vg::Paint::color(vg::Color::rgb(73, 62, 54));
        border_paint.set_line_width(1.0 * dpx);
        canvas.stroke_path(&border, &border_paint);
    }
}

fn build_knob_cell<L, Params, P, FMap>(
    cx: &mut Context,
    params: L,
    params_to_param: FMap,
    title: &'static str,
    bipolar: bool,
    accent_rgb: (u8, u8, u8),
) where
    L: Lens<Target = Params> + Clone,
    Params: 'static,
    P: Param + 'static,
    FMap: Fn(&Params) -> &P + Copy + 'static,
{
    let cell_w = 96.0;
    let knob_sz = 68.0;

    VStack::new(cx, move |cx| {
        Label::new(cx, title)
            .class("knob-title")
            .font_family(vec![FamilyOwned::Name(String::from(assets::NOTO_SANS))])
            .font_weight(FontWeightKeyword::Bold)
            .width(Stretch(1.0))
            .height(Pixels(18.0))
            .child_space(Stretch(1.0));

        RotaryKnob::new(cx, params.clone(), params_to_param, bipolar, accent_rgb)
            .width(Pixels(knob_sz))
            .height(Pixels(knob_sz))
            .left(Stretch(1.0))
            .right(Stretch(1.0));

        HStack::new(
            cx,
            ParamWidgetBase::build_view(params, params_to_param, move |cx, param_data| {
                let readout_lens = param_data.make_lens(|param| {
                    param.normalized_value_to_string(param.modulated_normalized_value(), true)
                });
                Label::new(cx, readout_lens)
                    .class("knob-readout-text")
                    .font_family(vec![FamilyOwned::Name(String::from(assets::NOTO_SANS))])
                    .font_weight(FontWeightKeyword::Bold)
                    .width(Stretch(1.0))
                    .height(Stretch(1.0))
                    .child_space(Stretch(1.0));
            }),
        )
        .class("knob-readout-box")
        .width(Pixels(cell_w - 8.0))
        .height(Pixels(22.0))
        .left(Stretch(1.0))
        .right(Stretch(1.0));
    })
    .class("knob-cell")
    .width(Pixels(cell_w))
    .height(Auto)
    .row_between(Pixels(4.0));
}

fn build_rack_screw(cx: &mut Context) {
    Element::new(cx).class("rack-screw");
}

pub(crate) fn create(
    params: Arc<JSatPluginParams>,
    editor_state: Arc<ViziaState>,
    scope: Arc<ScopeRing>,
) -> Option<Box<dyn Editor>> {
    create_vizia_editor(editor_state, ViziaTheming::Builtin, move |cx, _| {
        assets::register_noto_sans_bold(cx);
        cx.set_default_font(&[assets::NOTO_SANS]);

        let _ = cx.add_stylesheet(include_style!("src/theme.css"));

        let scope_for_ui = scope.clone();
        let params_for_scope = params.clone();

        UiData {
            params: params.clone(),
        }
        .build(cx);

        let amber = (245, 179, 73);
        let gold = (242, 203, 97);
        let coral = (235, 140, 85);

        VStack::new(cx, move |cx| {
            // =========================================================================
            // TOP HARDWARE HEADER BAR
            // =========================================================================
            HStack::new(cx, |cx| {
                build_rack_screw(cx);

                HStack::new(cx, |cx| {
                    Element::new(cx).class("brand-pill");
                    Label::new(cx, "J-SAT")
                        .class("header-title")
                        .font_family(vec![FamilyOwned::Name(String::from(assets::NOTO_SANS))])
                        .font_weight(FontWeightKeyword::Bold);
                    Label::new(cx, "PHAT RACK")
                        .class("header-subtitle")
                        .font_family(vec![FamilyOwned::Name(String::from(assets::NOTO_SANS))])
                        .font_weight(FontWeightKeyword::Bold);
                })
                .col_between(Pixels(10.0))
                .child_top(Stretch(1.0))
                .child_bottom(Stretch(1.0))
                .width(Auto);

                Element::new(cx).width(Stretch(1.0));

                build_rack_screw(cx);
            })
            .class("header-bar")
            .width(Stretch(1.0))
            .height(Pixels(38.0))
            .child_left(Pixels(12.0))
            .child_right(Pixels(12.0))
            .child_top(Stretch(1.0))
            .child_bottom(Stretch(1.0))
            .col_between(Pixels(12.0));

            // =========================================================================
            // MAIN RACK CHASSIS (TWO-ROW STACKED LAYOUT)
            // =========================================================================
            VStack::new(cx, |cx| {
                VStack::new(cx, |cx| {
                    // -----------------------------------------------------------------
                    // TOP ROW: INPUT & SATURATION + MODE TOGGLES
                    // -----------------------------------------------------------------
                    VStack::new(cx, |cx| {
                        HStack::new(cx, |cx| {
                            Label::new(cx, "INPUT & SATURATION")
                                .class("module-title")
                                .font_family(vec![FamilyOwned::Name(String::from(
                                    assets::NOTO_SANS,
                                ))])
                                .font_weight(FontWeightKeyword::Bold);
                        })
                        .class("module-header")
                        .height(Pixels(26.0))
                        .width(Stretch(1.0));

                        HStack::new(cx, |cx| {
                            build_knob_cell(
                                cx,
                                UiData::params,
                                |p| &p.pre_gain_db,
                                "PRE-GAIN",
                                false,
                                amber,
                            );

                            build_knob_cell(
                                cx,
                                UiData::params,
                                |p| &p.drive,
                                "DRIVE",
                                false,
                                amber,
                            );

                            Element::new(cx).width(Stretch(1.0));

                            VStack::new(cx, |cx| {
                                LedToggleSwitch::new(
                                    cx,
                                    UiData::params,
                                    |p| &p.sub_cut,
                                    "SUB CUT",
                                    "SUB CUT",
                                    amber,
                                )
                                .width(Pixels(128.0))
                                .height(Pixels(32.0));

                                LedToggleSwitch::new(
                                    cx,
                                    UiData::params,
                                    |p| &p.soft_clip,
                                    "SOFT CLIP",
                                    "SOFT CLIP",
                                    amber,
                                )
                                .width(Pixels(128.0))
                                .height(Pixels(32.0));
                            })
                            .width(Auto)
                            .height(Auto)
                            .top(Stretch(1.0))
                            .bottom(Stretch(1.0))
                            .row_between(Pixels(12.0));
                        })
                        .class("knob-row")
                        .width(Stretch(1.0))
                        .height(Stretch(1.0));
                    })
                    .class("hardware-module")
                    .width(Stretch(1.0))
                    .height(Stretch(1.0));

                    // -----------------------------------------------------------------
                    // BOTTOM ROW: TONE & OUTPUT + CLIPPER SCOPE
                    // -----------------------------------------------------------------
                    VStack::new(cx, |cx| {
                        HStack::new(cx, |cx| {
                            Label::new(cx, "TONE & OUTPUT")
                                .class("module-title")
                                .font_family(vec![FamilyOwned::Name(String::from(
                                    assets::NOTO_SANS,
                                ))])
                                .font_weight(FontWeightKeyword::Bold);
                        })
                        .class("module-header")
                        .height(Pixels(26.0))
                        .width(Stretch(1.0));

                        HStack::new(cx, |cx| {
                            build_knob_cell(
                                cx,
                                UiData::params,
                                |p| &p.treble_db,
                                "TREBLE",
                                true,
                                gold,
                            );

                            build_knob_cell(
                                cx,
                                UiData::params,
                                |p| &p.notch_mix,
                                "TAME",
                                false,
                                coral,
                            );

                            build_knob_cell(
                                cx,
                                UiData::params,
                                |p| &p.clip_gain_db,
                                "CLIP GAIN",
                                true,
                                amber,
                            );

                            ClipperScope::new(cx, scope_for_ui.clone(), params_for_scope.clone())
                                .width(Stretch(1.0))
                                .min_width(Pixels(200.0))
                                .height(Pixels(114.0))
                                .top(Stretch(1.0))
                                .bottom(Stretch(1.0));
                        })
                        .class("knob-row")
                        .width(Stretch(1.0))
                        .height(Stretch(1.0));
                    })
                    .class("hardware-module")
                    .width(Stretch(1.0))
                    .height(Stretch(1.0));
                })
                .class("section-panel")
                .row_between(Pixels(10.0))
                .width(Stretch(1.0))
                .height(Stretch(1.0));
            })
            .class("main-chassis")
            .width(Stretch(1.0))
            .height(Stretch(1.0));
        })
        .class("plugin-root")
        .width(Stretch(1.0))
        .height(Stretch(1.0));
    })
}
