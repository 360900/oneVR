//! The desktop keyboard's window: a text entry that reports each edit as keys. Only used by the
//! `xrizer-keyboard` executable, so it uses nothing else of the crate.

use super::edit::{keys_for_edit, limit_chars};
use super::protocol::{Reply, Request};
use egui::{Color32, CornerRadius, Key, Margin, RichText, Stroke, TextEdit, Vec2};
use egui_miniquad::EguiMq;
use miniquad::{EventHandler, GlContext, PassAction, RenderingBackend, conf::Conf};

/// The window's width, and the height it grows to before the text scrolls, in points (pixels at
/// the window's DPI scale).
const WIDTH: f32 = 440.0;
const MAX_HEIGHT: f32 = 420.0;

/// oneDesign's dark tokens with the blue accent, in sRGB.
const CANVAS: Color32 = Color32::from_rgb(0x18, 0x1a, 0x1c);
const ENTRY: Color32 = Color32::from_rgb(0x28, 0x2a, 0x2d);
const BUTTON: Color32 = Color32::from_rgb(0x30, 0x32, 0x36);
const TEXT: Color32 = Color32::from_rgb(0xef, 0xf2, 0xf6);
const TEXT2: Color32 = Color32::from_rgb(0xae, 0xb1, 0xb5);
const ACCENT: Color32 = Color32::from_rgb(0x5f, 0xad, 0xff);
const ON_ACCENT: Color32 = Color32::from_rgb(0x14, 0x16, 0x19);
const HAIRLINE: Color32 = Color32::from_rgba_premultiplied(0x1a, 0x1a, 0x1a, 0x1a);
/// Padding, control height and radii, on oneDesign's 4 px grid.
const PADDING: i8 = 14;
const CONTROL: f32 = 36.0;
const CONTROL_RADIUS: u8 = 10;

/// Shows the window for `request`, calling `reply` for what's typed, until it's closed.
pub fn run(request: Request, reply: impl FnMut(Reply) + 'static) {
    let title = if request.description.is_empty() {
        "Keyboard".to_string()
    } else {
        request.description.clone()
    };
    miniquad::start(
        Conf {
            window_title: title,
            high_dpi: true,
            window_width: WIDTH as i32,
            // Fitted to the content once it's laid out, see Dialog::draw.
            window_height: 140,
            ..Default::default()
        },
        move || Box::new(Dialog::new(request, Box::new(reply))),
    );
}

struct Form {
    request: Request,
    text: String,
    /// The text the app last heard of.
    reported: String,
    reply: Box<dyn FnMut(Reply)>,
    focused: bool,
}

impl Form {
    fn report(&mut self) {
        if self.text != self.reported {
            let key = keys_for_edit(&self.reported, &self.text);
            self.reported.clone_from(&self.text);
            (self.reply)(Reply::Input {
                key,
                text: self.text.clone(),
            });
        }
    }

    fn done(&mut self) {
        self.report();
        (self.reply)(Reply::Done);
        miniquad::window::order_quit();
    }

    /// Draws the form and returns the height it needs.
    fn ui(&mut self, ctx: &egui::Context) -> f32 {
        let frame = egui::Frame::new()
            .fill(CANVAS)
            .inner_margin(Margin::same(PADDING));
        let mut done = false;
        let mut close = false;
        let mut height = 0.0;
        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            // Laid out unbounded and scrolled, so its height is what it needs.
            egui::ScrollArea::vertical().show(ui, |ui| {
                height = ui
                    .vertical(|ui| {
                        if !self.request.description.is_empty() {
                            ui.label(RichText::new(&self.request.description).color(TEXT2));
                            ui.add_space(4.0);
                        }
                        let edit = if self.request.multiline {
                            TextEdit::multiline(&mut self.text).desired_rows(5)
                        } else {
                            TextEdit::singleline(&mut self.text)
                        }
                        .password(self.request.password)
                        .desired_width(f32::INFINITY)
                        .min_size(Vec2::new(0.0, CONTROL))
                        .margin(Margin::symmetric(12, 9))
                        .vertical_align(if self.request.multiline {
                            egui::Align::Min
                        } else {
                            egui::Align::Center
                        });
                        let edit = ui.add(edit);
                        if !self.focused {
                            edit.request_focus();
                            self.focused = true;
                        }
                        let limited = limit_chars(&self.text, self.request.max_chars).len();
                        self.text.truncate(limited);
                        self.report();

                        // Enter finishes a single line, Ctrl+Enter any text.
                        let enter = ui.input(|i| i.key_pressed(Key::Enter));
                        let ctrl = ui.input(|i| i.modifiers.command);
                        if self.request.done_key && enter && (!self.request.multiline || ctrl) {
                            done = true;
                        }

                        ui.add_space(12.0);
                        let row = Vec2::new(ui.available_width(), CONTROL);
                        ui.allocate_ui_with_layout(
                            row,
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if self.request.done_key && ui.add(button("Done", true)).clicked() {
                                    done = true;
                                }
                                let close_label = if self.request.done_key {
                                    "Cancel"
                                } else {
                                    "Close"
                                };
                                if ui
                                    .add(button(close_label, !self.request.done_key))
                                    .clicked()
                                {
                                    close = true;
                                }
                            },
                        );
                    })
                    .response
                    .rect
                    .height();
            });
        });
        if done {
            self.done();
        } else if close || ctx.input(|i| i.key_pressed(Key::Escape)) {
            miniquad::window::order_quit();
        }
        height + frame.inner_margin.sum().y
    }
}

/// A flat tinted button, or an accent filled one for the suggested action.
fn button(label: &str, suggested: bool) -> egui::Button<'_> {
    let (fill, text) = if suggested {
        (ACCENT, ON_ACCENT)
    } else {
        (BUTTON, TEXT)
    };
    egui::Button::new(RichText::new(label).color(text))
        .fill(fill)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(CONTROL_RADIUS))
        .min_size(Vec2::new(88.0, CONTROL))
}

fn style(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
    ctx.style_mut(|style| {
        let visuals = &mut style.visuals;
        visuals.override_text_color = Some(TEXT);
        visuals.panel_fill = CANVAS;
        visuals.window_fill = CANVAS;
        visuals.extreme_bg_color = ENTRY;
        visuals.selection.bg_fill = ACCENT.linear_multiply(0.4);
        visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
        visuals.text_cursor.stroke = Stroke::new(2.0_f32, ACCENT);
        for widget in [
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
            &mut visuals.widgets.noninteractive,
        ] {
            widget.corner_radius = CornerRadius::same(CONTROL_RADIUS);
            widget.bg_stroke = Stroke::new(1.0_f32, HAIRLINE);
        }
        // The entry's focus ring.
        visuals.widgets.active.bg_stroke = Stroke::new(2.0_f32, ACCENT);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, TEXT2.linear_multiply(0.4));
        style.spacing.item_spacing = Vec2::new(8.0, 4.0);
        style.spacing.button_padding = Vec2::new(14.0, 8.0);
        for font in style.text_styles.values_mut() {
            font.size = font.size.max(14.0);
        }
    });
}

struct Dialog {
    egui_mq: EguiMq,
    mq: GlContext,
    form: Form,
    /// The window height last set, see [Dialog::draw].
    height: f32,
}

impl Dialog {
    fn new(request: Request, reply: Box<dyn FnMut(Reply)>) -> Self {
        let mut mq = GlContext::new();
        let egui_mq = EguiMq::new(&mut mq);
        egui_mq
            .egui_ctx()
            .set_pixels_per_point(miniquad::window::dpi_scale());
        style(egui_mq.egui_ctx());
        let text = limit_chars(&request.text, request.max_chars).to_string();
        Self {
            egui_mq,
            mq,
            form: Form {
                reported: request.text.clone(),
                text,
                request,
                reply,
                focused: false,
            },
            height: 0.0,
        }
    }
}

impl EventHandler for Dialog {
    fn update(&mut self) {}
    fn draw(&mut self) {
        self.mq
            .begin_default_pass(PassAction::clear_color(0.0, 0.0, 0.0, 1.0));
        self.mq.end_render_pass();

        let mut wanted = 0.0;
        self.egui_mq.run(&mut self.mq, |_, ctx| {
            wanted = self.form.ui(ctx);
        });
        self.egui_mq.draw(&mut self.mq);
        self.mq.commit_frame();

        // Fit the window to the content, which grows with a multiline text. Window sizes are in
        // pixels, the content in points.
        let wanted = wanted.ceil().min(MAX_HEIGHT);
        if wanted != self.height {
            self.height = wanted;
            let scale = miniquad::window::dpi_scale();
            miniquad::window::set_window_size(
                (WIDTH * scale).round() as u32,
                (wanted * scale).round() as u32,
            );
        }
    }

    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        self.egui_mq.mouse_motion_event(x, y);
    }
    fn mouse_wheel_event(&mut self, x: f32, y: f32) {
        self.egui_mq.mouse_wheel_event(x, y);
    }
    fn mouse_button_down_event(&mut self, button: miniquad::MouseButton, x: f32, y: f32) {
        self.egui_mq.mouse_button_down_event(button, x, y);
    }
    fn mouse_button_up_event(&mut self, button: miniquad::MouseButton, x: f32, y: f32) {
        self.egui_mq.mouse_button_up_event(button, x, y);
    }
    fn char_event(&mut self, character: char, _: miniquad::KeyMods, _: bool) {
        self.egui_mq.char_event(character);
    }
    fn key_down_event(&mut self, keycode: miniquad::KeyCode, keymods: miniquad::KeyMods, _: bool) {
        self.egui_mq.key_down_event(keycode, keymods);
    }
    fn key_up_event(&mut self, keycode: miniquad::KeyCode, keymods: miniquad::KeyMods) {
        self.egui_mq.key_up_event(keycode, keymods);
    }
}
