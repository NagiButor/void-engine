use crate::{settings::Settings, weapon::WeaponHud};
use egui::{
    Align2, Color32, FontId, Id, LayerId, Margin, Order, Pos2, Rect, Rounding, Sense, Shape,
    Stroke, Ui, Vec2 as EguiVec2,
};

// V.O.I.D. Obsidian-Glass & Solar-Amber Palette (sRGB premultiplied-safe via const helpers)
pub const BG_DEEP: Color32 = Color32::from_rgba_premultiplied(9, 12, 16, 232);
pub const BG_CARD: Color32 = Color32::from_rgba_premultiplied(14, 18, 25, 222);
pub const BG_PILL: Color32 = Color32::from_rgba_premultiplied(10, 14, 20, 200);
pub const BG_INPUT: Color32 = Color32::from_rgba_premultiplied(19, 25, 34, 230);
pub const BORDER_SUBTLE: Color32 = Color32::from_rgba_premultiplied(28, 34, 44, 34);
pub const BORDER_MID: Color32 = Color32::from_rgba_premultiplied(48, 56, 70, 58);
pub const AMBER: Color32 = Color32::from_rgb(238, 174, 82);
pub const AMBER_BRIGHT: Color32 = Color32::from_rgb(252, 206, 134);
pub const CYAN: Color32 = Color32::from_rgb(104, 212, 228);
pub const MINT: Color32 = Color32::from_rgb(106, 224, 168);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(238, 242, 247);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(150, 164, 180);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(98, 112, 128);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScreenState {
    Menu,
    Playing,
    Paused,
    Settings,
    Sun,
}

#[derive(Clone, Copy)]
pub struct SunState {
    pub auto_cycle: bool,
    pub cycle_speed: f32,
}

impl Default for SunState {
    fn default() -> Self {
        Self {
            auto_cycle: false,
            cycle_speed: 0.8,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Graphics,
    Camera,
    Audio,
    Gamepad,
}

pub struct HudSnapshot<'a> {
    pub state: ScreenState,
    pub fps: f32,
    pub recent_ms: &'a [f32],
    pub adapter: &'a str,
    pub audio_voices: usize,
    pub audio_gain: f32,
    pub slow: bool,
    pub time: f32,
    pub light_hovered: bool,
    pub light_held: bool,
    pub prop_hovered: bool,
    pub prop_held: bool,
    pub light_count: usize,
    pub light_max: usize,
    pub light_preset: usize,
    pub prop_count: usize,
    pub show_controls: bool,
    pub gamepad_name: Option<&'a str>,
    pub gamepad_active: bool,
    pub weapon: WeaponHud,
}

#[derive(Default)]
pub struct UiActions {
    pub start: bool,
    pub resume: bool,
    pub reset_scene: bool,
    pub open_settings: bool,
    pub close_settings: bool,
    pub open_sun: bool,
    pub close_sun: bool,
    pub reset_sun: bool,
    pub to_menu: bool,
    pub reset_settings: bool,
    pub exit: bool,
    pub click_sfx: bool,
}

#[derive(Clone, Copy, Default)]
pub struct MenuInput {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub axis_x: f32,
    pub confirm: bool,
    pub tab_prev: bool,
    pub tab_next: bool,
    pub dt: f32,
}

#[derive(Default)]
pub struct MenuNav {
    pub index: usize,
    cursor: usize,
    active: bool,
    input: MenuInput,
    hold_y: i8,
    hold_y_timer: f32,
    hold_x: i8,
    hold_x_timer: f32,
    last_state: Option<ScreenState>,
    last_tab: Option<SettingsTab>,
}

impl MenuNav {
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        state: ScreenState,
        tab: SettingsTab,
        gamepad_active: bool,
        up_held: bool,
        down_held: bool,
        left_held: bool,
        right_held: bool,
        axis_x: f32,
        confirm: bool,
        tab_prev: bool,
        tab_next: bool,
        dt: f32,
    ) {
        if self.last_state != Some(state)
            || (state == ScreenState::Settings && self.last_tab != Some(tab))
        {
            self.index = 0;
            self.last_state = Some(state);
            self.last_tab = Some(tab);
        }
        self.active = gamepad_active;
        let dir_y = if down_held && !up_held {
            1
        } else if up_held && !down_held {
            -1
        } else {
            0
        };
        let (up, down) = Self::repeat_dir(dir_y, &mut self.hold_y, &mut self.hold_y_timer, dt);

        let dir_x = if right_held && !left_held {
            1
        } else if left_held && !right_held {
            -1
        } else {
            0
        };
        let (left, right) = Self::repeat_dir(dir_x, &mut self.hold_x, &mut self.hold_x_timer, dt);

        self.input = MenuInput {
            up: up == -1,
            down: down == 1,
            left: left == -1,
            right: right == 1,
            axis_x,
            confirm,
            tab_prev,
            tab_next,
            dt,
        };
    }

    fn repeat_dir(dir: i8, prev: &mut i8, timer: &mut f32, dt: f32) -> (i8, i8) {
        if dir == 0 {
            *prev = 0;
            *timer = 0.0;
            return (0, 0);
        }
        if *prev != dir {
            *prev = dir;
            *timer = 0.36;
            return (dir, dir);
        }
        *timer -= dt;
        if *timer <= 0.0 {
            *timer = 0.14;
            return (dir, dir);
        }
        (0, 0)
    }

    fn next(&mut self, ui: &Ui, rect: Rect) -> bool {
        let idx = self.cursor;
        self.cursor += 1;
        let focused = self.active && idx == self.index;
        if focused {
            ui.scroll_to_rect(rect.expand(8.0), None);
            ui.painter().rect_stroke(
                rect.expand(2.0),
                Rounding::same(10.0),
                stroke(2.0, AMBER_BRIGHT),
            );
        }
        focused
    }

    fn finish(&mut self) {
        let total = self.cursor.max(1);
        if self.index >= total {
            self.index = total - 1;
        }
        if self.input.down {
            self.index = (self.index + 1) % total;
        } else if self.input.up {
            self.index = (self.index + total - 1) % total;
        }
        self.cursor = 0;
    }
}

fn rgba(c: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha)
}

fn stroke(width: f32, color: Color32) -> Stroke {
    Stroke::new(width, color)
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        mix(a.r(), b.r()),
        mix(a.g(), b.g()),
        mix(a.b(), b.b()),
        mix(a.a(), b.a()),
    )
}

pub fn apply_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = BG_DEEP;
    visuals.panel_fill = BG_DEEP;
    visuals.window_rounding = Rounding::same(14.0);
    visuals.window_stroke = stroke(1.0, BORDER_SUBTLE);
    visuals.selection.bg_fill = rgba(AMBER, 160);
    visuals.selection.stroke = stroke(1.0, AMBER_BRIGHT);
    visuals.widgets.noninteractive.fg_stroke = stroke(1.0, TEXT_PRIMARY);
    visuals.widgets.inactive.bg_fill = BG_INPUT;
    visuals.widgets.inactive.rounding = Rounding::same(8.0);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(32, 40, 52);
    visuals.widgets.hovered.rounding = Rounding::same(8.0);
    visuals.widgets.active.bg_fill = rgba(AMBER, 55);
    visuals.widgets.active.rounding = Rounding::same(8.0);
    style.visuals = visuals;
    style.spacing.item_spacing = EguiVec2::new(10.0, 8.0);
    style.spacing.window_margin = Margin::same(22.0);
    ctx.set_style(style);
}

/// Draws the signature animated V.O.I.D. geometric aperture emblem.
fn draw_void_emblem(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    time: f32,
    accent: Color32,
) {
    // Soft ambient core glow
    painter.circle_filled(center, radius * 0.92, rgba(accent, 16));
    painter.circle_filled(center, radius * 0.56, rgba(accent, 24));

    // Outer segmented orbital ring (6 architectural arcs)
    let rot_outer = time * 0.22;
    let segments = 6;
    let arc_span = std::f32::consts::TAU / segments as f32;
    for i in 0..segments {
        let a0 = rot_outer + i as f32 * arc_span + 0.14;
        let a1 = rot_outer + (i + 1) as f32 * arc_span - 0.14;
        let steps = 5;
        let pts: Vec<Pos2> = (0..=steps)
            .map(|s| {
                let a = a0 + (a1 - a0) * (s as f32 / steps as f32);
                center + EguiVec2::new(a.cos() * radius, a.sin() * radius)
            })
            .collect();
        let stroke_color = if i % 2 == 0 {
            accent
        } else {
            rgba(TEXT_PRIMARY, 110)
        };
        painter.add(Shape::line(pts, stroke(1.5, stroke_color)));
    }

    // Counter-rotating inner aperture diamond
    let rot_inner = -time * 0.38 + std::f32::consts::FRAC_PI_4;
    let r_in = radius * 0.54;
    let diamond: Vec<Pos2> = (0..=4)
        .map(|i| {
            let a = rot_inner + i as f32 * std::f32::consts::FRAC_PI_2;
            center + EguiVec2::new(a.cos() * r_in, a.sin() * r_in)
        })
        .collect();
    painter.add(Shape::line(diamond, stroke(1.2, rgba(AMBER_BRIGHT, 195))));

    // Breathing luminous core
    let pulse = 1.0 + (time * 2.4).sin() * 0.16;
    painter.circle_filled(center, (radius * 0.16 * pulse).max(1.8), accent);
}

fn draw_corner_brackets(painter: &egui::Painter, rect: Rect, len: f32, color: Color32, width: f32) {
    let s = stroke(width, color);
    let min = rect.min;
    let max = rect.max;
    // Top-left
    painter.line_segment([min, min + EguiVec2::new(len, 0.0)], s);
    painter.line_segment([min, min + EguiVec2::new(0.0, len)], s);
    // Top-right
    let tr = Pos2::new(max.x, min.y);
    painter.line_segment([tr, tr + EguiVec2::new(-len, 0.0)], s);
    painter.line_segment([tr, tr + EguiVec2::new(0.0, len)], s);
    // Bottom-left
    let bl = Pos2::new(min.x, max.y);
    painter.line_segment([bl, bl + EguiVec2::new(len, 0.0)], s);
    painter.line_segment([bl, bl + EguiVec2::new(0.0, -len)], s);
    // Bottom-right
    painter.line_segment([max, max + EguiVec2::new(-len, 0.0)], s);
    painter.line_segment([max, max + EguiVec2::new(0.0, -len)], s);
}

fn draw_left_scrim(painter: &egui::Painter, screen: Rect) {
    let max_width = (screen.width() * 0.56).clamp(460.0, 860.0);
    let mid_x = screen.left() + max_width * 0.48;
    let end_x = screen.left() + max_width;
    let mut mesh = egui::Mesh::default();
    let c0 = Color32::from_rgba_unmultiplied(6, 9, 13, 195);
    let c1 = Color32::from_rgba_unmultiplied(6, 9, 13, 95);
    let c2 = Color32::from_rgba_unmultiplied(6, 9, 13, 0);
    let v = |x, y, color| egui::epaint::Vertex {
        pos: Pos2::new(x, y),
        uv: egui::epaint::WHITE_UV,
        color,
    };
    mesh.vertices.extend([
        v(screen.left(), screen.top(), c0),
        v(mid_x, screen.top(), c1),
        v(end_x, screen.top(), c2),
        v(screen.left(), screen.bottom(), c0),
        v(mid_x, screen.bottom(), c1),
        v(end_x, screen.bottom(), c2),
    ]);
    mesh.indices.extend([0, 1, 4, 0, 4, 3, 1, 2, 5, 1, 5, 4]);
    painter.add(Shape::mesh(mesh));
}

#[allow(clippy::too_many_arguments)]
fn void_button(
    ui: &mut Ui,
    nav: &mut MenuNav,
    id_salt: &str,
    label: &str,
    tag: &str,
    primary: bool,
    width: f32,
    height: f32,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(EguiVec2::new(width, height), Sense::click());
    let focused = nav.next(ui, rect);
    let hover = ui.ctx().animate_bool(
        ui.make_persistent_id(id_salt),
        response.hovered() || focused,
    );
    let pressed = if response.is_pointer_button_down_on() {
        1.0
    } else {
        0.0
    };

    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let rounding = Rounding::same(10.0);

        let bg = if primary {
            let base = Color32::from_rgba_unmultiplied(238, 174, 82, 42);
            let hov = Color32::from_rgba_unmultiplied(238, 174, 82, 82);
            let prs = Color32::from_rgba_unmultiplied(238, 174, 82, 110);
            lerp_color(lerp_color(base, hov, hover), prs, pressed)
        } else {
            let base = Color32::from_rgba_unmultiplied(20, 26, 36, 175);
            let hov = Color32::from_rgba_unmultiplied(30, 39, 53, 215);
            let prs = Color32::from_rgba_unmultiplied(40, 50, 66, 235);
            lerp_color(lerp_color(base, hov, hover), prs, pressed)
        };

        let border = if primary {
            lerp_color(rgba(AMBER, 150), AMBER_BRIGHT, hover)
        } else {
            lerp_color(BORDER_SUBTLE, rgba(AMBER, 130), hover)
        };

        p.rect_filled(rect, rounding, bg);
        p.rect_stroke(rect, rounding, stroke(1.0 + hover * 0.4, border));

        // Vertical luminous accent bar on the left
        let bar_h = rect.height() * (if primary { 0.56 } else { 0.28 + hover * 0.34 });
        let bar_rect = Rect::from_center_size(
            Pos2::new(rect.left() + 8.0, rect.center().y),
            EguiVec2::new(3.0, bar_h),
        );
        let bar_color = if primary {
            AMBER_BRIGHT
        } else {
            lerp_color(rgba(TEXT_MUTED, 110), AMBER, hover)
        };
        p.rect_filled(bar_rect, Rounding::same(2.0), bar_color);

        // Label with subtle glide on hover
        let text_color = if primary {
            lerp_color(AMBER_BRIGHT, Color32::WHITE, hover * 0.65)
        } else {
            lerp_color(TEXT_PRIMARY, Color32::WHITE, hover * 0.5)
        };
        p.text(
            Pos2::new(rect.left() + 22.0 + hover * 4.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(if primary { 14.0 } else { 13.0 }),
            text_color,
        );

        // Right-hand chevron (primary) or index tag (secondary)
        if primary {
            let cx = rect.right() - 20.0 + hover * 4.0;
            let cy = rect.center().y;
            let s = stroke(1.8, text_color);
            p.line_segment([Pos2::new(cx - 4.0, cy - 4.5), Pos2::new(cx + 1.0, cy)], s);
            p.line_segment([Pos2::new(cx + 1.0, cy), Pos2::new(cx - 4.0, cy + 4.5)], s);
        } else if !tag.is_empty() {
            p.text(
                Pos2::new(rect.right() - 14.0, rect.center().y),
                Align2::RIGHT_CENTER,
                tag,
                FontId::monospace(10.5),
                lerp_color(TEXT_MUTED, rgba(AMBER, 200), hover),
            );
        }
    }

    response.clicked() || (focused && nav.input.confirm)
}

fn void_toggle(
    ui: &mut Ui,
    nav: &mut MenuNav,
    id_salt: &str,
    label: &str,
    hint: &str,
    value: &mut bool,
) -> bool {
    let width = ui.available_width();
    let height = if hint.is_empty() { 34.0 } else { 44.0 };
    let (rect, response) = ui.allocate_exact_size(EguiVec2::new(width, height), Sense::click());
    let focused = nav.next(ui, rect);
    let clicked =
        response.clicked() || (focused && (nav.input.confirm || nav.input.left || nav.input.right));
    if clicked {
        *value = !*value;
    }

    let on_t = ui
        .ctx()
        .animate_bool(ui.make_persistent_id((id_salt, "on")), *value);
    let hov_t = ui.ctx().animate_bool(
        ui.make_persistent_id((id_salt, "hov")),
        response.hovered() || focused,
    );

    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let row_bg = lerp_color(
            Color32::from_rgba_unmultiplied(255, 255, 255, 0),
            Color32::from_rgba_unmultiplied(255, 255, 255, 10),
            hov_t,
        );
        p.rect_filled(rect, Rounding::same(8.0), row_bg);

        if hint.is_empty() {
            p.text(
                Pos2::new(rect.left() + 8.0, rect.center().y),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(13.5),
                TEXT_PRIMARY,
            );
        } else {
            p.text(
                Pos2::new(rect.left() + 8.0, rect.top() + 14.0),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(13.5),
                TEXT_PRIMARY,
            );
            p.text(
                Pos2::new(rect.left() + 8.0, rect.top() + 31.0),
                Align2::LEFT_CENTER,
                hint,
                FontId::proportional(11.0),
                TEXT_MUTED,
            );
        }

        // Pill switch
        let sw_w = 42.0;
        let sw_h = 22.0;
        let sw_rect = Rect::from_center_size(
            Pos2::new(rect.right() - 8.0 - sw_w * 0.5, rect.center().y),
            EguiVec2::new(sw_w, sw_h),
        );
        let track_fill = lerp_color(
            Color32::from_rgba_unmultiplied(36, 44, 58, 220),
            rgba(AMBER, 210),
            on_t,
        );
        let track_stroke = lerp_color(BORDER_MID, AMBER_BRIGHT, on_t);
        p.rect_filled(sw_rect, Rounding::same(sw_h * 0.5), track_fill);
        p.rect_stroke(
            sw_rect,
            Rounding::same(sw_h * 0.5),
            stroke(1.0, track_stroke),
        );

        let knob_x = sw_rect.left() + 11.0 + on_t * (sw_w - 22.0);
        let knob_center = Pos2::new(knob_x, sw_rect.center().y);
        if on_t > 0.05 {
            p.circle_filled(knob_center, 11.0, rgba(AMBER, (on_t * 45.0) as u8));
        }
        p.circle_filled(
            knob_center,
            7.5,
            lerp_color(
                Color32::from_rgb(180, 192, 206),
                Color32::from_rgb(18, 20, 24),
                on_t,
            ),
        );
    }
    clicked
}

fn void_slider(
    ui: &mut Ui,
    nav: &mut MenuNav,
    id_salt: &str,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    fmt: impl Fn(f32) -> String,
) -> bool {
    let width = ui.available_width();
    let height = 46.0;
    let (rect, response) =
        ui.allocate_exact_size(EguiVec2::new(width, height), Sense::click_and_drag());
    let focused = nav.next(ui, rect);

    let min = *range.start();
    let max = *range.end();
    let track_rect = Rect::from_min_max(
        Pos2::new(rect.left() + 8.0, rect.top() + 30.0),
        Pos2::new(rect.right() - 8.0, rect.top() + 36.0),
    );

    let mut changed = false;
    if (response.dragged() || response.clicked()) && response.interact_pointer_pos().is_some() {
        if let Some(pos) = response.interact_pointer_pos() {
            let norm = ((pos.x - track_rect.left()) / track_rect.width().max(1.0)).clamp(0.0, 1.0);
            let next = min + norm * (max - min);
            if (*value - next).abs() > f32::EPSILON {
                *value = next;
                changed = true;
            }
        }
    }
    if focused {
        let mut step = 0.0f32;
        if nav.input.axis_x.abs() > 0.05 {
            step = nav.input.axis_x * 0.55 * nav.input.dt;
        } else if nav.input.right {
            step = 0.05;
        } else if nav.input.left {
            step = -0.05;
        }
        if step != 0.0 {
            let next = (*value + step * (max - min)).clamp(min, max);
            if (*value - next).abs() > f32::EPSILON {
                *value = next;
                changed = true;
            }
        }
    }

    let hov = ui.ctx().animate_bool(
        ui.make_persistent_id((id_salt, "slider_hov")),
        response.hovered() || response.dragged() || focused,
    );

    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        p.text(
            Pos2::new(rect.left() + 8.0, rect.top() + 12.0),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(13.0),
            TEXT_PRIMARY,
        );

        // Tabular value badge on the right
        let badge_rect = Rect::from_min_max(
            Pos2::new(rect.right() - 82.0, rect.top() + 2.0),
            Pos2::new(rect.right() - 8.0, rect.top() + 21.0),
        );
        p.rect_filled(
            badge_rect,
            Rounding::same(5.0),
            lerp_color(
                Color32::from_rgba_unmultiplied(24, 31, 42, 210),
                rgba(AMBER, 42),
                hov,
            ),
        );
        p.text(
            badge_rect.center(),
            Align2::CENTER_CENTER,
            fmt(*value),
            FontId::monospace(11.5),
            lerp_color(AMBER, AMBER_BRIGHT, hov),
        );

        // Track background
        p.rect_filled(
            track_rect,
            Rounding::same(3.0),
            Color32::from_rgba_unmultiplied(36, 44, 58, 220),
        );

        // Filled active portion
        let norm = ((*value - min) / (max - min).max(0.0001)).clamp(0.0, 1.0);
        let fill_x = track_rect.left() + norm * track_rect.width();
        let fill_rect = Rect::from_min_max(track_rect.min, Pos2::new(fill_x, track_rect.max.y));
        p.rect_filled(
            fill_rect,
            Rounding::same(3.0),
            lerp_color(AMBER, AMBER_BRIGHT, hov * 0.5),
        );

        // Thumb with hover glow
        let thumb = Pos2::new(fill_x, track_rect.center().y);
        if hov > 0.02 {
            p.circle_filled(thumb, 11.0 + hov * 2.0, rgba(AMBER, (hov * 52.0) as u8));
        }
        p.circle_filled(thumb, 6.2 + hov * 1.3, AMBER_BRIGHT);
        p.circle_stroke(
            thumb,
            6.2 + hov * 1.3,
            stroke(1.5, Color32::from_rgb(16, 20, 28)),
        );
    }

    changed
}

fn void_segmented<T: Copy + PartialEq>(
    ui: &mut Ui,
    nav: &mut MenuNav,
    id_salt: &str,
    label: &str,
    current: &mut T,
    options: &[(T, &str)],
) -> bool {
    let mut changed = false;
    ui.add_space(2.0);
    ui.label(egui::RichText::new(label).size(13.0).color(TEXT_PRIMARY));
    ui.add_space(4.0);

    let width = ui.available_width();
    let height = 34.0;
    let (outer, _) = ui.allocate_exact_size(EguiVec2::new(width, height), Sense::hover());
    let focused = nav.next(ui, outer);
    if focused && !options.is_empty() {
        let cur_idx = options
            .iter()
            .position(|(v, _)| *v == *current)
            .unwrap_or(0);
        if nav.input.right {
            *current = options[(cur_idx + 1) % options.len()].0;
            changed = true;
        } else if nav.input.left {
            *current = options[(cur_idx + options.len() - 1) % options.len()].0;
            changed = true;
        }
    }
    let p = ui.painter();
    p.rect_filled(
        outer,
        Rounding::same(9.0),
        Color32::from_rgba_unmultiplied(18, 23, 32, 225),
    );
    p.rect_stroke(outer, Rounding::same(9.0), stroke(1.0, BORDER_SUBTLE));

    let pad = 3.0;
    let seg_w = (outer.width() - pad * 2.0) / options.len().max(1) as f32;
    for (i, (val, text)) in options.iter().enumerate() {
        let seg_rect = Rect::from_min_max(
            Pos2::new(outer.left() + pad + i as f32 * seg_w, outer.top() + pad),
            Pos2::new(
                outer.left() + pad + (i + 1) as f32 * seg_w,
                outer.bottom() - pad,
            ),
        );
        let resp = ui.interact(
            seg_rect,
            ui.make_persistent_id((id_salt, i)),
            Sense::click(),
        );
        let active = *current == *val;
        if resp.clicked() && !active {
            *current = *val;
            changed = true;
        }
        let hov = ui
            .ctx()
            .animate_bool(ui.make_persistent_id((id_salt, "h", i)), resp.hovered());
        if active {
            p.rect_filled(seg_rect, Rounding::same(7.0), rgba(AMBER, 52));
            p.rect_stroke(seg_rect, Rounding::same(7.0), stroke(1.0, rgba(AMBER, 185)));
        } else if hov > 0.01 {
            p.rect_filled(
                seg_rect,
                Rounding::same(7.0),
                Color32::from_rgba_unmultiplied(255, 255, 255, (hov * 14.0) as u8),
            );
        }
        p.text(
            seg_rect.center(),
            Align2::CENTER_CENTER,
            *text,
            FontId::proportional(12.5),
            if active {
                AMBER_BRIGHT
            } else {
                lerp_color(TEXT_SECONDARY, TEXT_PRIMARY, hov)
            },
        );
    }
    changed
}

fn draw_tab_icon(painter: &egui::Painter, center: Pos2, tab: SettingsTab, color: Color32) {
    let s = stroke(1.4, color);
    match tab {
        SettingsTab::Graphics => {
            // Minimal monitor / frame icon
            let r =
                Rect::from_center_size(center - EguiVec2::new(0.0, 1.0), EguiVec2::new(13.0, 9.0));
            painter.rect_stroke(r, Rounding::same(1.5), s);
            painter.line_segment(
                [
                    center + EguiVec2::new(-3.5, 6.0),
                    center + EguiVec2::new(3.5, 6.0),
                ],
                s,
            );
        }
        SettingsTab::Camera => {
            // Minimal optical aperture ring
            painter.circle_stroke(center, 5.5, s);
            painter.circle_filled(center, 1.8, color);
        }
        SettingsTab::Audio => {
            // Minimal 3-bar audio wave
            for (dx, h) in [(-4.0, 3.0), (0.0, 6.0), (4.0, 4.0)] {
                painter.line_segment(
                    [
                        center + EguiVec2::new(dx, -h),
                        center + EguiVec2::new(dx, h),
                    ],
                    s,
                );
            }
        }
        SettingsTab::Gamepad => {
            // Minimal controller body + sticks
            let r = Rect::from_center_size(center, EguiVec2::new(13.0, 8.0));
            painter.rect_stroke(r, Rounding::same(3.5), s);
            painter.circle_filled(center + EguiVec2::new(-3.2, 0.0), 1.3, color);
            painter.circle_filled(center + EguiVec2::new(3.2, 0.0), 1.3, color);
        }
    }
}

fn void_tab_bar(ui: &mut Ui, current: &mut SettingsTab) -> bool {
    let mut switched = false;
    let tabs = [
        (SettingsTab::Graphics, "ГРАФИКА"),
        (SettingsTab::Camera, "КАМЕРА"),
        (SettingsTab::Audio, "ЗВУК"),
        (SettingsTab::Gamepad, "ГЕЙМПАД"),
    ];
    let width = ui.available_width();
    let height = 38.0;
    let (outer, _) = ui.allocate_exact_size(EguiVec2::new(width, height), Sense::hover());
    let p = ui.painter();
    p.rect_filled(
        outer,
        Rounding::same(10.0),
        Color32::from_rgba_unmultiplied(15, 20, 28, 230),
    );
    p.rect_stroke(outer, Rounding::same(10.0), stroke(1.0, BORDER_SUBTLE));

    let pad = 4.0;
    let w = (outer.width() - pad * 2.0) / tabs.len() as f32;
    for (i, (tab, title)) in tabs.iter().enumerate() {
        let r = Rect::from_min_max(
            Pos2::new(outer.left() + pad + i as f32 * w, outer.top() + pad),
            Pos2::new(
                outer.left() + pad + (i + 1) as f32 * w,
                outer.bottom() - pad,
            ),
        );
        let resp = ui.interact(
            r,
            ui.make_persistent_id(("settings_tab", i)),
            Sense::click(),
        );
        let active = *current == *tab;
        if resp.clicked() && !active {
            *current = *tab;
            switched = true;
        }
        let hov = ui
            .ctx()
            .animate_bool(ui.make_persistent_id(("settings_tab_h", i)), resp.hovered());
        if active {
            p.rect_filled(r, Rounding::same(7.0), rgba(AMBER, 44));
            p.rect_stroke(r, Rounding::same(7.0), stroke(1.0, rgba(AMBER, 175)));
            // Subtle bottom accent dash
            p.line_segment(
                [
                    Pos2::new(r.center().x - 18.0, r.bottom() - 1.5),
                    Pos2::new(r.center().x + 18.0, r.bottom() - 1.5),
                ],
                stroke(2.0, AMBER_BRIGHT),
            );
        } else if hov > 0.01 {
            p.rect_filled(
                r,
                Rounding::same(7.0),
                Color32::from_rgba_unmultiplied(255, 255, 255, (hov * 12.0) as u8),
            );
        }
        let col = if active {
            AMBER_BRIGHT
        } else {
            lerp_color(TEXT_SECONDARY, TEXT_PRIMARY, hov)
        };
        draw_tab_icon(p, Pos2::new(r.center().x - 42.0, r.center().y), *tab, col);
        p.text(
            Pos2::new(r.center().x + 10.0, r.center().y),
            Align2::CENTER_CENTER,
            *title,
            FontId::proportional(12.0),
            col,
        );
    }
    switched
}

fn draw_sparkline(painter: &egui::Painter, rect: Rect, samples_ms: &[f32]) {
    painter.rect_filled(
        rect,
        Rounding::same(4.0),
        Color32::from_rgba_unmultiplied(8, 11, 16, 170),
    );
    if samples_ms.len() < 2 {
        return;
    }
    let max_ms = samples_ms
        .iter()
        .copied()
        .fold(20.0f32, f32::max)
        .clamp(16.6, 50.0);
    let n = samples_ms.len();
    let pts: Vec<Pos2> = samples_ms
        .iter()
        .enumerate()
        .map(|(i, &ms)| {
            let x = rect.left() + 2.0 + (i as f32 / (n - 1) as f32) * (rect.width() - 4.0);
            let norm = (ms / max_ms).clamp(0.0, 1.0);
            let y = rect.bottom() - 2.0 - norm * (rect.height() - 4.0);
            Pos2::new(x, y)
        })
        .collect();

    // 16.6 ms (60 FPS) subtle reference line
    let ref_y = rect.bottom() - 2.0 - (16.67 / max_ms).clamp(0.0, 1.0) * (rect.height() - 4.0);
    painter.line_segment(
        [
            Pos2::new(rect.left() + 2.0, ref_y),
            Pos2::new(rect.right() - 2.0, ref_y),
        ],
        stroke(0.7, Color32::from_rgba_unmultiplied(255, 255, 255, 22)),
    );

    // Subtle vertical bars under samples for depth
    for pt in &pts {
        painter.line_segment(
            [*pt, Pos2::new(pt.x, rect.bottom() - 2.0)],
            stroke(1.0, rgba(CYAN, 26)),
        );
    }
    painter.add(Shape::line(pts, stroke(1.4, CYAN)));
}

fn draw_dynamic_reticle(
    painter: &egui::Painter,
    center: Pos2,
    weapon: &WeaponHud,
    slow: bool,
    time: f32,
) {
    let accent = if slow { CYAN } else { AMBER };
    let vis = (1.0 - weapon.ads * 0.92).clamp(0.08, 1.0);
    let alpha_main = (205.0 * vis) as u8;
    let alpha_dot = (235.0 * vis) as u8;
    let breathe = (time * 2.2).sin() * 0.35;
    let gap = (6.0 + weapon.spread_px + breathe) * (1.0 - weapon.ads * 0.45);
    let tick = 4.4;

    if vis > 0.15 {
        // Minimal central luminous dot with soft contrast halo
        painter.circle_filled(
            center,
            2.8,
            Color32::from_rgba_unmultiplied(8, 11, 16, (120.0 * vis) as u8),
        );
        painter.circle_filled(center, 1.7, rgba(Color32::WHITE, alpha_dot));
        painter.circle_stroke(center, 2.7, stroke(0.8, rgba(accent, (110.0 * vis) as u8)));

        // Four fine architectural ticks with subtle dark backing for contrast
        let shadow = stroke(
            2.4,
            Color32::from_rgba_unmultiplied(8, 11, 16, (115.0 * vis) as u8),
        );
        let s = stroke(1.2, rgba(Color32::WHITE, alpha_main));
        let ticks = [
            [
                center + EguiVec2::new(-gap - tick, 0.0),
                center + EguiVec2::new(-gap, 0.0),
            ],
            [
                center + EguiVec2::new(gap, 0.0),
                center + EguiVec2::new(gap + tick, 0.0),
            ],
            [
                center + EguiVec2::new(0.0, -gap - tick),
                center + EguiVec2::new(0.0, -gap),
            ],
            [
                center + EguiVec2::new(0.0, gap),
                center + EguiVec2::new(0.0, gap + tick),
            ],
        ];
        for seg in ticks {
            painter.line_segment(seg, shadow);
            painter.line_segment(seg, s);
        }
    }

    // Crisp FPS hitmarker cross when bullet strikes a dynamic target
    if weapon.hitmarker > 0.0 {
        let hm = (weapon.hitmarker / 0.16).clamp(0.0, 1.0);
        let hm_col = rgba(AMBER_BRIGHT, (hm * 240.0) as u8);
        let hm_s = stroke(1.8, hm_col);
        let inner = 5.0 + (1.0 - hm) * 3.0;
        let outer = inner + 5.5;
        for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            painter.line_segment(
                [
                    center + EguiVec2::new(sx * inner, sy * inner),
                    center + EguiVec2::new(sx * outer, sy * outer),
                ],
                hm_s,
            );
        }
    }

    // Subtle outer slow-mo ring
    if slow {
        painter.circle_stroke(center, gap + 7.0, stroke(1.0, rgba(CYAN, 110)));
    }
}

fn draw_keycap(ui: &mut Ui, key: &str, action: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        egui::Frame::none()
            .fill(Color32::from_rgba_unmultiplied(24, 31, 43, 215))
            .stroke(stroke(
                1.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, 36),
            ))
            .rounding(Rounding::same(5.0))
            .inner_margin(Margin::symmetric(6.0, 2.0))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(key)
                        .font(FontId::monospace(10.5))
                        .color(AMBER_BRIGHT),
                );
            });
        ui.label(
            egui::RichText::new(action)
                .size(11.0)
                .color(Color32::from_rgb(244, 158, 66)),
        );
    });
}

fn draw_playing_hud(ctx: &egui::Context, snap: &HudSnapshot<'_>, muted: bool) {
    let rect = ctx.screen_rect();
    let fg = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("hud_fg")));

    // Subtle cryo-cyan edge highlight when slow-mo is active
    if snap.slow {
        let pulse = 0.75 + (snap.time * 3.5).sin() * 0.25;
        fg.rect_stroke(
            rect.shrink(3.0),
            Rounding::same(4.0),
            stroke(2.0, rgba(CYAN, (pulse * 95.0) as u8)),
        );
    }

    // Center dynamic reticle
    draw_dynamic_reticle(&fg, rect.center(), &snap.weapon, snap.slow, snap.time);
    let prompt = if snap.gamepad_active {
        if snap.light_held {
            Some("[Y / RT] Поставить свет  ·  [D-Pad ↓] Убрать  ·  D-Pad ↑/↓: дистанция")
        } else if snap.light_hovered {
            Some("[Y] Переместить свет  ·  [D-Pad ↓] Убрать")
        } else if snap.prop_held {
            Some("[RT] Бросить  ·  [Y] Отпустить  ·  [D-Pad ↓] Удалить")
        } else if snap.prop_hovered {
            Some("[Y] Взять бочку  ·  [D-Pad ↓] Удалить")
        } else {
            None
        }
    } else if snap.light_held {
        Some("[E / ЛКМ] Поставить свет  ·  [X] Убрать  ·  Колесо: дистанция")
    } else if snap.light_hovered {
        Some("[E] Переместить свет  ·  [X] Убрать")
    } else if snap.prop_held {
        Some("[ЛКМ] Бросить  ·  [E] Отпустить  ·  [X] Удалить  ·  Колесо: дистанция")
    } else if snap.prop_hovered {
        Some("[E] Взять бочку  ·  [X] Удалить")
    } else {
        None
    };
    if let Some(label) = prompt {
        fg.text(
            rect.center() + EguiVec2::new(0.0, 26.0),
            Align2::CENTER_TOP,
            label,
            FontId::monospace(11.0),
            AMBER_BRIGHT,
        );
    }

    // Top-right telemetry + frame-time sparkline pill
    egui::Area::new(Id::new("hud_telemetry"))
        .anchor(Align2::RIGHT_TOP, EguiVec2::new(-20.0, 18.0))
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(BG_PILL)
                .stroke(stroke(1.0, BORDER_SUBTLE))
                .rounding(Rounding::same(11.0))
                .inner_margin(Margin::symmetric(12.0, 8.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        let (spark_rect, _) =
                            ui.allocate_exact_size(EguiVec2::new(78.0, 26.0), Sense::hover());
                        draw_sparkline(ui.painter(), spark_rect, snap.recent_ms);

                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 5.0;
                                ui.label(
                                    egui::RichText::new(format!("{:.0}", snap.fps))
                                        .font(FontId::monospace(15.0))
                                        .color(TEXT_PRIMARY)
                                        .strong(),
                                );
                                ui.label(
                                    egui::RichText::new("FPS")
                                        .font(FontId::monospace(10.0))
                                        .color(CYAN),
                                );
                                if muted {
                                    ui.label(
                                        egui::RichText::new("· MUTE")
                                            .font(FontId::monospace(10.0))
                                            .color(AMBER),
                                    );
                                }
                            });
                            let ms = 1000.0 / snap.fps.max(1.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "{ms:.1} ms · {} ch",
                                    snap.audio_voices
                                ))
                                .font(FontId::monospace(10.0))
                                .color(TEXT_MUTED),
                            );
                        });
                    });
                });
        });

    // Bottom-left compact keycap legend (toggleable with H)
    if snap.show_controls {
        egui::Area::new(Id::new("hud_controls"))
            .anchor(Align2::LEFT_BOTTOM, EguiVec2::new(20.0, -20.0))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(BG_PILL)
                    .stroke(stroke(
                        1.0,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 24),
                    ))
                    .rounding(Rounding::same(10.0))
                    .inner_margin(Margin::symmetric(12.0, 8.0))
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 5.0;
                            if snap.gamepad_active {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 12.0;
                                    draw_keycap(ui, "LS", "ходьба");
                                    draw_keycap(ui, "L3", "бег");
                                    draw_keycap(ui, "A", "прыжок");
                                    draw_keycap(ui, "RT", "огонь / бросок");
                                    draw_keycap(ui, "LT", "прицел");
                                    draw_keycap(ui, "X", "перезарядка");
                                });
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 12.0;
                                    draw_keycap(ui, "Y", "взять");
                                    draw_keycap(ui, "RB", "бочка");
                                    draw_keycap(ui, "D-UP", "+ свет");
                                    draw_keycap(ui, "B", "оружие");
                                    draw_keycap(ui, "R3", "осмотр");
                                    draw_keycap(ui, "LB", "время");
                                    draw_keycap(ui, "VIEW", "скрыть");
                                });
                            } else {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 12.0;
                                    draw_keycap(ui, "WASD", "ходьба");
                                    draw_keycap(ui, "SHIFT", "бег");
                                    draw_keycap(ui, "SPACE", "прыжок");
                                    draw_keycap(ui, "ЛКМ", "огонь / бросок");
                                    draw_keycap(ui, "ПКМ", "прицел");
                                    draw_keycap(ui, "R", "перезарядка");
                                });
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 12.0;
                                    draw_keycap(ui, "E", "взять");
                                    draw_keycap(ui, "F", "бочка");
                                    draw_keycap(ui, "G", "+ свет");
                                    draw_keycap(ui, "V", "оружие");
                                    draw_keycap(ui, "T", "осмотр");
                                    draw_keycap(ui, "Q", "время");
                                    draw_keycap(ui, "F5", "сброс");
                                    draw_keycap(ui, "H", "скрыть");
                                });
                            }
                        });
                    });
            });

        // Bottom-right dynamic lights & props palette pill
        let presets = &void_engine::scene::LIGHT_PRESETS;
        let active_preset = &presets[snap.light_preset % presets.len()];
        egui::Area::new(Id::new("hud_sandbox"))
            .anchor(Align2::RIGHT_BOTTOM, EguiVec2::new(-20.0, -20.0))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(BG_PILL)
                    .stroke(stroke(
                        1.0,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 24),
                    ))
                    .rounding(Rounding::same(10.0))
                    .inner_margin(Margin::symmetric(12.0, 8.0))
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 4.0;
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 7.0;
                                ui.label(
                                    egui::RichText::new(format!(
                                        "СВЕТ {}/{} · {}",
                                        snap.light_count, snap.light_max, active_preset.name
                                    ))
                                    .font(FontId::monospace(10.5))
                                    .color(TEXT_PRIMARY),
                                );
                                for (i, p) in presets.iter().enumerate() {
                                    let col = Color32::from_rgb(
                                        (p.color[0].clamp(0.0, 1.0) * 255.0) as u8,
                                        (p.color[1].clamp(0.0, 1.0) * 255.0) as u8,
                                        (p.color[2].clamp(0.0, 1.0) * 255.0) as u8,
                                    );
                                    let (dot_r, _) = ui
                                        .allocate_exact_size(EguiVec2::splat(11.0), Sense::hover());
                                    let selected = i == snap.light_preset % presets.len();
                                    ui.painter().circle_filled(
                                        dot_r.center(),
                                        if selected { 4.8 } else { 3.3 },
                                        col,
                                    );
                                    if selected {
                                        ui.painter().circle_stroke(
                                            dot_r.center(),
                                            6.0,
                                            stroke(1.2, Color32::WHITE),
                                        );
                                    }
                                }
                            });
                            ui.label(
                                egui::RichText::new(format!(
                                    "БОЧКИ: {}  ·  F3 настройки",
                                    snap.prop_count
                                ))
                                .font(FontId::monospace(10.0))
                                .color(TEXT_SECONDARY),
                            );
                        });
                    });
            });

        // Weapon ammo & acoustics HUD pill (bottom-right above sandbox pill)
        if snap.weapon.equipped {
            let acoustic_label = if snap.weapon.indoor_mix >= 0.60 {
                ("ПОМЕЩЕНИЕ", AMBER)
            } else if snap.weapon.indoor_mix >= 0.30 {
                ("УКРЫТИЕ", AMBER_BRIGHT)
            } else {
                ("УЛИЦА · ЭХО", CYAN)
            };
            egui::Area::new(Id::new("hud_weapon"))
                .anchor(Align2::RIGHT_BOTTOM, EguiVec2::new(-20.0, -90.0))
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(BG_PILL)
                        .stroke(stroke(1.0, BORDER_SUBTLE))
                        .rounding(Rounding::same(10.0))
                        .inner_margin(Margin::symmetric(12.0, 8.0))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 3.0;
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 10.0;
                                    let ammo_color = if snap.weapon.ammo == 0 {
                                        Color32::from_rgb(245, 92, 82)
                                    } else if snap.weapon.ammo <= 4 {
                                        AMBER_BRIGHT
                                    } else {
                                        TEXT_PRIMARY
                                    };
                                    let ammo_text = if snap.weapon.chambered && snap.weapon.ammo > 1
                                    {
                                        format!("{:02}+1 / ∞", snap.weapon.ammo - 1)
                                    } else {
                                        format!("{:02} / ∞", snap.weapon.ammo)
                                    };
                                    ui.label(
                                        egui::RichText::new(ammo_text)
                                            .font(FontId::monospace(18.0))
                                            .color(ammo_color)
                                            .strong(),
                                    );
                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = 1.0;
                                        ui.label(
                                            egui::RichText::new("XD-9  9×19 MM")
                                                .font(FontId::monospace(10.0))
                                                .color(TEXT_SECONDARY),
                                        );
                                        ui.label(
                                            egui::RichText::new(acoustic_label.0)
                                                .font(FontId::monospace(9.5))
                                                .color(acoustic_label.1),
                                        );
                                    });
                                });
                                if let Some(progress) = snap.weapon.reloading_progress {
                                    let (bar_r, _) = ui.allocate_exact_size(
                                        EguiVec2::new(150.0, 4.0),
                                        Sense::hover(),
                                    );
                                    ui.painter().rect_filled(
                                        bar_r,
                                        Rounding::same(2.0),
                                        Color32::from_rgba_unmultiplied(36, 44, 58, 220),
                                    );
                                    let fill = Rect::from_min_max(
                                        bar_r.min,
                                        Pos2::new(
                                            bar_r.left() + bar_r.width() * progress,
                                            bar_r.max.y,
                                        ),
                                    );
                                    ui.painter().rect_filled(
                                        fill,
                                        Rounding::same(2.0),
                                        AMBER_BRIGHT,
                                    );
                                } else if snap.weapon.ammo == 0 {
                                    ui.label(
                                        egui::RichText::new("[R] ПЕРЕЗАРЯДКА")
                                            .font(FontId::monospace(9.5))
                                            .color(Color32::from_rgb(245, 110, 92)),
                                    );
                                }
                            });
                        });
                });
        }
    }
}

fn draw_menu_overlay(
    ctx: &egui::Context,
    snap: &HudSnapshot<'_>,
    nav: &mut MenuNav,
    actions: &mut UiActions,
) {
    let screen = ctx.screen_rect();
    let is_menu = snap.state == ScreenState::Menu;

    let bg_painter = ctx.layer_painter(LayerId::new(Order::Middle, Id::new("menu_scrim")));
    draw_left_scrim(&bg_painter, screen);

    let card_w = 376.0;
    egui::Area::new(Id::new("menu_card"))
        .anchor(Align2::LEFT_CENTER, EguiVec2::new(64.0, 0.0))
        .show(ctx, |ui| {
            let resp = egui::Frame::none()
                .fill(BG_DEEP)
                .stroke(stroke(1.0, BORDER_SUBTLE))
                .rounding(Rounding::same(16.0))
                .inner_margin(Margin::same(28.0))
                .show(ui, |ui| {
                    ui.set_width(card_w - 56.0);

                    // Brand emblem + title
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 14.0;
                        let (emblem_rect, _) =
                            ui.allocate_exact_size(EguiVec2::splat(46.0), Sense::hover());
                        draw_void_emblem(
                            ui.painter(),
                            emblem_rect.center(),
                            20.0,
                            snap.time,
                            AMBER,
                        );
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("V.O.I.D.")
                                        .size(26.0)
                                        .color(TEXT_PRIMARY)
                                        .strong(),
                                );
                                egui::Frame::none()
                                    .fill(rgba(AMBER, 30))
                                    .stroke(stroke(1.0, rgba(AMBER, 110)))
                                    .rounding(Rounding::same(6.0))
                                    .inner_margin(Margin::symmetric(6.0, 2.0))
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new("v0.2")
                                                .font(FontId::monospace(10.0))
                                                .color(AMBER_BRIGHT),
                                        );
                                    });
                            });
                            ui.label(
                                egui::RichText::new("VIRTUAL OBJECTS & DYNAMICS")
                                    .font(FontId::monospace(10.0))
                                    .color(AMBER),
                            );
                        });
                    });

                    ui.add_space(16.0);

                    // Luminous architectural divider
                    let (div, _) = ui.allocate_exact_size(
                        EguiVec2::new(ui.available_width(), 6.0),
                        Sense::hover(),
                    );
                    ui.painter().line_segment(
                        [
                            Pos2::new(div.left(), div.center().y),
                            Pos2::new(div.right(), div.center().y),
                        ],
                        stroke(1.0, BORDER_SUBTLE),
                    );
                    ui.painter().line_segment(
                        [
                            Pos2::new(div.left(), div.center().y),
                            Pos2::new(div.left() + 64.0, div.center().y),
                        ],
                        stroke(2.0, AMBER),
                    );

                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(if is_menu {
                            "ТЕСТОВАЯ СЦЕНА"
                        } else {
                            "СИМУЛЯЦИЯ НА ПАУЗЕ"
                        })
                        .size(18.0)
                        .color(TEXT_PRIMARY)
                        .strong(),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(if is_menu {
                            "Dev-текстуры · Освещение и материалы · Rapier 3D"
                        } else {
                            "Выберите действие или нажмите ESC для возврата"
                        })
                        .size(12.0)
                        .color(TEXT_SECONDARY),
                    );

                    ui.add_space(22.0);
                    let bw = ui.available_width();
                    if is_menu {
                        if void_button(
                            ui,
                            nav,
                            "btn_start",
                            "ВОЙТИ В ПРОСТРАНСТВО",
                            "01",
                            true,
                            bw,
                            46.0,
                        ) {
                            actions.start = true;
                            actions.click_sfx = true;
                        }
                        ui.add_space(6.0);
                        if void_button(
                            ui,
                            nav,
                            "btn_sun",
                            "НЕБО, СОЛНЦЕ И ЛУНА",
                            "02",
                            false,
                            bw,
                            40.0,
                        ) {
                            actions.open_sun = true;
                            actions.click_sfx = true;
                        }
                        ui.add_space(6.0);
                        if void_button(ui, nav, "btn_settings", "НАСТРОЙКИ", "03", false, bw, 40.0)
                        {
                            actions.open_settings = true;
                            actions.click_sfx = true;
                        }
                        ui.add_space(6.0);
                        if void_button(ui, nav, "btn_exit", "ВЫХОД", "04", false, bw, 40.0) {
                            actions.exit = true;
                            actions.click_sfx = true;
                        }
                    } else {
                        if void_button(ui, nav, "btn_resume", "ПРОДОЛЖИТЬ", "01", true, bw, 44.0)
                        {
                            actions.resume = true;
                            actions.click_sfx = true;
                        }
                        ui.add_space(6.0);
                        if void_button(
                            ui,
                            nav,
                            "btn_reset",
                            "СБРОСИТЬ ПОЗИЦИЮ",
                            "02",
                            false,
                            bw,
                            38.0,
                        ) {
                            actions.reset_scene = true;
                            actions.click_sfx = true;
                        }
                        ui.add_space(6.0);
                        if void_button(
                            ui,
                            nav,
                            "btn_sun",
                            "НЕБО, СОЛНЦЕ И ЛУНА",
                            "03",
                            false,
                            bw,
                            38.0,
                        ) {
                            actions.open_sun = true;
                            actions.click_sfx = true;
                        }
                        ui.add_space(6.0);
                        if void_button(ui, nav, "btn_settings", "НАСТРОЙКИ", "04", false, bw, 38.0)
                        {
                            actions.open_settings = true;
                            actions.click_sfx = true;
                        }
                        ui.add_space(6.0);
                        if void_button(ui, nav, "btn_menu", "ГЛАВНОЕ МЕНЮ", "05", false, bw, 38.0)
                        {
                            actions.to_menu = true;
                            actions.click_sfx = true;
                        }
                        ui.add_space(6.0);
                        if void_button(ui, nav, "btn_exit", "ВЫХОД", "06", false, bw, 38.0) {
                            actions.exit = true;
                            actions.click_sfx = true;
                        }
                    }

                    ui.add_space(18.0);

                    // Footer status strip
                    egui::Frame::none()
                        .fill(Color32::from_rgba_unmultiplied(15, 20, 28, 210))
                        .stroke(stroke(1.0, BORDER_SUBTLE))
                        .rounding(Rounding::same(9.0))
                        .inner_margin(Margin::symmetric(12.0, 8.0))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 3.0;
                                ui.horizontal(|ui| {
                                    let (dot, _) = ui
                                        .allocate_exact_size(EguiVec2::splat(8.0), Sense::hover());
                                    let pulse = 0.65 + (snap.time * 3.0).sin() * 0.35;
                                    ui.painter().circle_filled(
                                        dot.center(),
                                        4.0,
                                        rgba(MINT, (pulse * 70.0) as u8),
                                    );
                                    ui.painter().circle_filled(dot.center(), 2.4, MINT);
                                    ui.label(
                                        egui::RichText::new("PHYSICS ONLINE · 60 HZ")
                                            .font(FontId::monospace(10.0))
                                            .color(MINT),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                egui::RichText::new(format!("{:.0} FPS", snap.fps))
                                                    .font(FontId::monospace(10.5))
                                                    .color(CYAN),
                                            );
                                        },
                                    );
                                });
                                ui.label(
                                    egui::RichText::new(snap.adapter)
                                        .font(FontId::monospace(9.5))
                                        .color(TEXT_MUTED),
                                );
                            });
                        });
                });

            draw_corner_brackets(
                ui.painter(),
                resp.response.rect.shrink(6.0),
                12.0,
                rgba(AMBER, 130),
                1.4,
            );
        });

    // Bottom-right subtle shortcut bar in menu / pause
    if is_menu || snap.gamepad_active || snap.gamepad_name.is_some() {
        egui::Area::new(Id::new("menu_hints"))
            .anchor(Align2::RIGHT_BOTTOM, EguiVec2::new(-24.0, -20.0))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(BG_PILL)
                    .stroke(stroke(1.0, BORDER_SUBTLE))
                    .rounding(Rounding::same(10.0))
                    .inner_margin(Margin::symmetric(12.0, 7.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 14.0;
                            if snap.gamepad_active || snap.gamepad_name.is_some() {
                                draw_keycap(ui, "D-PAD / LS", "выбор");
                                draw_keycap(ui, "A", "подтвердить");
                                draw_keycap(ui, "B", "назад");
                            } else {
                                draw_keycap(ui, "ENTER", "вход");
                                draw_keycap(ui, "F3", "настройки");
                                draw_keycap(ui, "ESC", "выход");
                            }
                        });
                    });
            });
    }
    nav.finish();
}

fn draw_settings_overlay(
    ctx: &egui::Context,
    snap: &HudSnapshot<'_>,
    settings: &mut Settings,
    tab: &mut SettingsTab,
    nav: &mut MenuNav,
    actions: &mut UiActions,
) {
    let screen = ctx.screen_rect();
    let scrim = ctx.layer_painter(LayerId::new(Order::Middle, Id::new("settings_scrim")));
    scrim.rect_filled(
        screen,
        Rounding::ZERO,
        Color32::from_rgba_unmultiplied(6, 9, 14, 165),
    );

    let modal_w = 620.0f32.min(screen.width() - 48.0);
    egui::Area::new(Id::new("settings_modal"))
        .anchor(Align2::CENTER_CENTER, EguiVec2::ZERO)
        .show(ctx, |ui| {
            let resp = egui::Frame::none()
                .fill(BG_DEEP)
                .stroke(stroke(1.0, BORDER_MID))
                .rounding(Rounding::same(16.0))
                .inner_margin(Margin::same(26.0))
                .show(ui, |ui| {
                    ui.set_width(modal_w - 52.0);

                    // Modal header
                    ui.horizontal(|ui| {
                        let (emblem_r, _) =
                            ui.allocate_exact_size(EguiVec2::splat(34.0), Sense::hover());
                        draw_void_emblem(
                            ui.painter(),
                            emblem_r.center(),
                            14.5,
                            snap.time,
                            AMBER,
                        );
                        ui.add_space(4.0);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.label(
                                egui::RichText::new("ПАРАМЕТРЫ СИСТЕМЫ")
                                    .size(18.0)
                                    .color(TEXT_PRIMARY)
                                    .strong(),
                            );
                            ui.label(
                                egui::RichText::new("V.O.I.D. ENGINE · КОНФИГУРАЦИЯ")
                                    .font(FontId::monospace(10.0))
                                    .color(AMBER),
                            );
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (close_r, close_resp) =
                                ui.allocate_exact_size(EguiVec2::splat(30.0), Sense::click());
                            let hov = ui.ctx().animate_bool(
                                Id::new("settings_close_hov"),
                                close_resp.hovered(),
                            );
                            ui.painter().rect_filled(
                                close_r,
                                Rounding::same(8.0),
                                lerp_color(
                                    Color32::from_rgba_unmultiplied(24, 30, 40, 180),
                                    rgba(AMBER, 55),
                                    hov,
                                ),
                            );
                            ui.painter().rect_stroke(
                                close_r,
                                Rounding::same(8.0),
                                stroke(1.0, lerp_color(BORDER_SUBTLE, rgba(AMBER, 160), hov)),
                            );
                            let c = close_r.center();
                            let s = stroke(
                                1.6,
                                lerp_color(TEXT_SECONDARY, AMBER_BRIGHT, hov),
                            );
                            ui.painter().line_segment(
                                [c + EguiVec2::new(-4.5, -4.5), c + EguiVec2::new(4.5, 4.5)],
                                s,
                            );
                            ui.painter().line_segment(
                                [c + EguiVec2::new(4.5, -4.5), c + EguiVec2::new(-4.5, 4.5)],
                                s,
                            );
                            if close_resp.clicked() {
                                actions.close_settings = true;
                                actions.click_sfx = true;
                            }
                        });
                    });

                    ui.add_space(16.0);
                    let tabs = [
                        SettingsTab::Graphics,
                        SettingsTab::Camera,
                        SettingsTab::Audio,
                        SettingsTab::Gamepad,
                    ];
                    if nav.input.tab_next {
                        let i = tabs.iter().position(|t| t == tab).unwrap_or(0);
                        *tab = tabs[(i + 1) % tabs.len()];
                        actions.click_sfx = true;
                    } else if nav.input.tab_prev {
                        let i = tabs.iter().position(|t| t == tab).unwrap_or(0);
                        *tab = tabs[(i + tabs.len() - 1) % tabs.len()];
                        actions.click_sfx = true;
                    }
                    if void_tab_bar(ui, tab) {
                        actions.click_sfx = true;
                    }
                    ui.add_space(14.0);

                    // Tab body inside an elevated inner surface
                    egui::Frame::none()
                        .fill(BG_CARD)
                        .stroke(stroke(1.0, BORDER_SUBTLE))
                        .rounding(Rounding::same(12.0))
                        .inner_margin(Margin::same(18.0))
                        .show(ui, |ui| {
                            ui.set_min_height(310.0);
                            egui::ScrollArea::vertical()
                                .max_height((screen.height() - 320.0).max(220.0))
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.y = 8.0;
                                    match *tab {
                                        SettingsTab::Graphics => {
                                            let mut res = (settings.width, settings.height);
                                            if void_segmented(
                                                ui,
                                                nav,
                                                "res_seg",
                                                "Разрешение экрана",
                                                &mut res,
                                                &[
                                                    ((1280, 720), "1280 × 720"),
                                                    ((1600, 900), "1600 × 900"),
                                                    ((1920, 1080), "1920 × 1080"),
                                                    ((2560, 1440), "2560 × 1440"),
                                                ],
                                            ) {
                                                settings.width = res.0;
                                                settings.height = res.1;
                                                actions.click_sfx = true;
                                            }
                                            ui.add_space(4.0);
                                            if void_segmented(
                                                ui,
                                                nav,
                                                "shadow_seg",
                                                "Качество теней",
                                                &mut settings.shadows,
                                                &[
                                                    (0, "Выкл"),
                                                    (1, "Жёсткие"),
                                                    (2, "Мягкие PCF"),
                                                ],
                                            ) {
                                                actions.click_sfx = true;
                                            }
                                            ui.add_space(6.0);
                                            if void_toggle(
                                                ui,
                                                nav,
                                                "t_vsync",
                                                "Вертикальная синхронизация",
                                                "Ограничение частоты кадров по монитору",
                                                &mut settings.vsync,
                                            ) {
                                                actions.click_sfx = true;
                                            }
                                            if void_toggle(
                                                ui,
                                                nav,
                                                "t_ao",
                                                "Контактное затенение (SSAO)",
                                                "Углубляет тени в стыках и углах геометрии",
                                                &mut settings.ao,
                                            ) {
                                                actions.click_sfx = true;
                                            }
                                            if void_toggle(
                                                ui,
                                                nav,
                                                "t_bloom",
                                                "Свечение (Bloom)",
                                                "Мягкий ореол вокруг ярких источников света",
                                                &mut settings.bloom,
                                            ) {
                                                actions.click_sfx = true;
                                            }
                                            if void_toggle(
                                                ui,
                                                nav,
                                                "t_gi",
                                                "Global Illumination (Visibility Bitmask SSGI)",
                                                "Отражённый непрямой свет и цветоперенос между поверхностями",
                                                &mut settings.gi,
                                            ) {
                                                actions.click_sfx = true;
                                            }
                                            if void_toggle(
                                                ui,
                                                nav,
                                                "t_clouds",
                                                "Процедурные облака",
                                                "Объёмная облачность с физическим рассеянием света",
                                                &mut settings.clouds,
                                            ) {
                                                actions.click_sfx = true;
                                            }
                                            if void_toggle(
                                                ui,
                                                nav,
                                                "t_volumetric",
                                                "Волуметрический свет",
                                                "Объёмные лучи от солнца и луны через тени",
                                                &mut settings.volumetric,
                                            ) {
                                                actions.click_sfx = true;
                                            }
                                            ui.add_space(4.0);
                                            void_slider(
                                                ui,
                                                nav,
                                                "s_exposure",
                                                "Экспозиция",
                                                &mut settings.exposure,
                                                0.5..=1.6,
                                                |v| format!("{v:.2}"),
                                            );
                                            void_slider(
                                                ui,
                                                nav,
                                                "s_fog",
                                                "Атмосферный туман",
                                                &mut settings.fog,
                                                0.0..=2.0,
                                                |v| format!("{v:.2}"),
                                            );
                                            if settings.gi {
                                                void_slider(
                                                    ui,
                                                    nav,
                                                    "s_gi_int",
                                                    "Интенсивность GI",
                                                    &mut settings.gi_intensity,
                                                    0.0..=2.0,
                                                    |v| format!("{v:.2}"),
                                                );
                                            }
                                            if settings.clouds {
                                                void_slider(
                                                    ui,
                                                    nav,
                                                    "s_clouds_cov",
                                                    "Плотность облаков",
                                                    &mut settings.cloud_coverage,
                                                    0.0..=1.0,
                                                    |v| format!("{:.0}%", v * 100.0),
                                                );
                                            }
                                            if settings.volumetric {
                                                void_slider(
                                                    ui,
                                                    nav,
                                                    "s_vol_int",
                                                    "Интенсивность лучей",
                                                    &mut settings.volumetric_intensity,
                                                    0.0..=2.0,
                                                    |v| format!("{v:.2}"),
                                                );
                                            }
                                        }
                                        SettingsTab::Camera => {
                                            void_slider(
                                                ui,
                                                nav,
                                                "s_fov",
                                                "Поле зрения (Hor+ 16:9)",
                                                &mut settings.fov,
                                                void_engine::units::camera::HFOV_MIN_DEG
                                                    ..=void_engine::units::camera::HFOV_MAX_DEG,
                                                |v| format!("{v:.0}°"),
                                            );
                                            void_slider(
                                                ui,
                                                nav,
                                                "s_eye",
                                                "Высота глаз (POV от пола)",
                                                &mut settings.eye_height,
                                                void_engine::units::human::EYE_HEIGHT_MIN
                                                    ..=void_engine::units::human::EYE_HEIGHT_MAX,
                                                |v| format!("{v:.2} м"),
                                            );
                                            void_slider(
                                                ui,
                                                nav,
                                                "s_side",
                                                "Боковое смещение камеры",
                                                &mut settings.camera_side,
                                                -void_engine::units::camera::SIDE_OFFSET_MAX_M
                                                    ..=void_engine::units::camera::SIDE_OFFSET_MAX_M,
                                                |v| format!("{v:+.2} м"),
                                            );
                                            void_slider(
                                                ui,
                                                nav,
                                                "s_sens",
                                                "Чувствительность мыши",
                                                &mut settings.sensitivity,
                                                void_engine::units::camera::SENSITIVITY_MIN
                                                    ..=void_engine::units::camera::SENSITIVITY_MAX,
                                                |v| format!("{v:.2}×"),
                                            );
                                            ui.add_space(4.0);
                                            if void_toggle(
                                                ui,
                                                nav,
                                                "t_inv_y",
                                                "Инверсия оси Y",
                                                "Отразить вертикальное движение мыши",
                                                &mut settings.invert_y,
                                            ) {
                                                actions.click_sfx = true;
                                            }
                                            if void_toggle(
                                                ui,
                                                nav,
                                                "t_bob",
                                                "Покачивание при ходьбе",
                                                "Лёгкое вертикальное движение камеры в такт шагам",
                                                &mut settings.head_bob,
                                            ) {
                                                actions.click_sfx = true;
                                            }
                                        }
                                        SettingsTab::Audio => {
                                            void_slider(
                                                ui,
                                                nav,
                                                "s_vol",
                                                "Общая громкость",
                                                &mut settings.volume,
                                                0.0..=1.0,
                                                |v| format!("{:.0}%", v * 100.0),
                                            );
                                            void_slider(
                                                ui,
                                                nav,
                                                "s_hdr",
                                                "Динамика HDR-аудио",
                                                &mut settings.hdr_audio,
                                                0.0..=1.0,
                                                |v| format!("{:.0}%", v * 100.0),
                                            );
                                            ui.add_space(4.0);
                                            if void_toggle(
                                                ui,
                                                nav,
                                                "t_mute",
                                                "Без звука",
                                                "Быстрое переключение клавишей M в игре",
                                                &mut settings.muted,
                                            ) {
                                                actions.click_sfx = true;
                                            }
                                            ui.add_space(12.0);
                                            egui::Frame::none()
                                                .fill(Color32::from_rgba_unmultiplied(12, 16, 23, 210))
                                                .stroke(stroke(1.0, BORDER_SUBTLE))
                                                .rounding(Rounding::same(9.0))
                                                .inner_margin(Margin::symmetric(14.0, 10.0))
                                                .show(ui, |ui| {
                                                    ui.horizontal(|ui| {
                                                        ui.label(
                                                            egui::RichText::new("АУДИО-МИКШЕР")
                                                                .font(FontId::monospace(10.5))
                                                                .color(CYAN),
                                                        );
                                                        ui.with_layout(
                                                            egui::Layout::right_to_left(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                ui.label(
                                                                    egui::RichText::new(format!(
                                                                        "HDR {:+.1} dB  ·  {} голосов",
                                                                        snap.audio_gain,
                                                                        snap.audio_voices
                                                                    ))
                                                                    .font(FontId::monospace(11.0))
                                                                    .color(TEXT_SECONDARY),
                                                                );
                                                            },
                                                        );
                                                    });
                                                });
                                        }
                                        SettingsTab::Gamepad => {
                                            egui::Frame::none()
                                                .fill(Color32::from_rgba_unmultiplied(
                                                    12, 16, 23, 210,
                                                ))
                                                .stroke(stroke(1.0, BORDER_SUBTLE))
                                                .rounding(Rounding::same(9.0))
                                                .inner_margin(Margin::symmetric(14.0, 10.0))
                                                .show(ui, |ui| {
                                                    ui.horizontal(|ui| {
                                                        let (dot, _) = ui.allocate_exact_size(
                                                            EguiVec2::splat(8.0),
                                                            Sense::hover(),
                                                        );
                                                        let col = if snap.gamepad_name.is_some() {
                                                            MINT
                                                        } else {
                                                            TEXT_MUTED
                                                        };
                                                        ui.painter().circle_filled(
                                                            dot.center(),
                                                            3.0,
                                                            col,
                                                        );
                                                        let text = match snap.gamepad_name {
                                                            Some(name) => {
                                                                format!("ПОДКЛЮЧЁН: {name}")
                                                            }
                                                            None => "ГЕЙМПАД НЕ ОБНАРУЖЕН · Подключите XInput / DualSense".to_owned(),
                                                        };
                                                        ui.label(
                                                            egui::RichText::new(text)
                                                                .font(FontId::monospace(10.5))
                                                                .color(col),
                                                        );
                                                    });
                                                });
                                            ui.add_space(4.0);
                                            void_slider(
                                                ui,
                                                nav,
                                                "s_gp_sens",
                                                "Чувствительность стиков",
                                                &mut settings.gamepad_sensitivity,
                                                0.25..=3.0,
                                                |v| format!("{v:.2}×"),
                                            );
                                            void_slider(
                                                ui,
                                                nav,
                                                "s_gp_dz",
                                                "Мёртвая зона стиков",
                                                &mut settings.gamepad_deadzone,
                                                0.04..=0.30,
                                                |v| format!("{:.0}%", v * 100.0),
                                            );
                                            if void_toggle(
                                                ui,
                                                nav,
                                                "t_gp_vib",
                                                "Вибрация (Rumble)",
                                                "Тактильная отдача при выстрелах, приземлении и захвате",
                                                &mut settings.gamepad_vibration,
                                            ) {
                                                actions.click_sfx = true;
                                            }
                                            ui.add_space(6.0);
                                            ui.label(
                                                egui::RichText::new(
                                                    "LS движение · RS обзор · LT прицел · RT огонь · A прыжок · X перезарядка · Y взять · B оружие · L3 бег · R3 осмотр · LB время · RB бочка · D-Pad свет",
                                                )
                                                .size(11.0)
                                                .color(TEXT_MUTED),
                                            );
                                        }
                                    }
                                });
                        });

                    ui.add_space(16.0);

                    // Footer buttons
                    let total_w = ui.available_width();
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        let reset_w = 180.0;
                        let save_w = (total_w - reset_w - 12.0).max(160.0);
                        if void_button(ui, nav, "s_reset", "СБРОСИТЬ", "", false, reset_w, 42.0) {
                            actions.reset_settings = true;
                            actions.click_sfx = true;
                        }
                        if void_button(
                            ui,
                            nav,
                            "s_save",
                            "ПРИМЕНИТЬ И НАЗАД",
                            "",
                            true,
                            save_w,
                            42.0,
                        ) {
                            actions.close_settings = true;
                            actions.click_sfx = true;
                        }
                    });
                });

            draw_corner_brackets(
                ui.painter(),
                resp.response.rect.shrink(6.0),
                12.0,
                rgba(AMBER, 130),
                1.4,
            );
        });
    if snap.gamepad_active || snap.gamepad_name.is_some() {
        egui::Area::new(Id::new("settings_gp_hints"))
            .anchor(Align2::RIGHT_BOTTOM, EguiVec2::new(-24.0, -20.0))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(BG_PILL)
                    .stroke(stroke(1.0, BORDER_SUBTLE))
                    .rounding(Rounding::same(10.0))
                    .inner_margin(Margin::symmetric(12.0, 7.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 12.0;
                            draw_keycap(ui, "LB / RB", "вкладки");
                            draw_keycap(ui, "D-PAD / LS", "навигация");
                            draw_keycap(ui, "A", "выбрать");
                            draw_keycap(ui, "B", "назад");
                        });
                    });
            });
    }
    nav.finish();
}

fn format_clock(hours: f32) -> String {
    let total_minutes = (hours.rem_euclid(24.0) * 60.0).round() as u32 % (24 * 60);
    format!("{:02}:{:02}", total_minutes / 60, total_minutes % 60)
}

fn day_phase(elev: f32, time_of_day: f32) -> (&'static str, Color32) {
    if elev < -0.06 {
        ("НОЧЬ", CYAN)
    } else if elev < 0.22 {
        if time_of_day < 12.0 {
            ("РАССВЕТ", AMBER_BRIGHT)
        } else {
            ("ЗАКАТ", Color32::from_rgb(255, 130, 74))
        }
    } else {
        ("ДЕНЬ", MINT)
    }
}

fn format_moon_phase(phase: f32) -> String {
    let p = phase.rem_euclid(1.0);
    let label = if !(0.08..0.92).contains(&p) {
        "Новолуние"
    } else if p < 0.22 {
        "Молодая"
    } else if p < 0.34 {
        "1-я четв."
    } else if p < 0.45 {
        "Прибыв."
    } else if p < 0.56 {
        "Полнолуние"
    } else if p < 0.70 {
        "Убывающая"
    } else {
        "3-я четв."
    };
    format!(
        "{label} ({:.0}%)",
        (1.0 - (p - 0.5).abs() * 2.0).clamp(0.0, 1.0) * 100.0
    )
}

fn draw_celestial_diagram(
    ui: &mut Ui,
    settings: &Settings,
    sun_dir: glam::Vec3,
    moon_dir: glam::Vec3,
) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(EguiVec2::new(w, 66.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(
        rect,
        Rounding::same(10.0),
        Color32::from_rgba_unmultiplied(10, 14, 21, 210),
    );
    p.rect_stroke(rect, Rounding::same(10.0), stroke(1.0, BORDER_SUBTLE));

    let pad_x = 12.0;
    let horizon_y = rect.bottom() - 20.0;
    let top_y = rect.top() + 9.0;
    let bot_y = rect.bottom() - 6.0;
    let left_x = rect.left() + pad_x;
    let right_x = rect.right() - pad_x;

    // Horizon line
    p.line_segment(
        [Pos2::new(left_x, horizon_y), Pos2::new(right_x, horizon_y)],
        stroke(1.0, rgba(TEXT_MUTED, 110)),
    );
    p.text(
        Pos2::new(left_x + 2.0, horizon_y - 3.0),
        Align2::LEFT_BOTTOM,
        "ГОРИЗОНТ",
        FontId::monospace(8.5),
        TEXT_MUTED,
    );

    let to_pos = |hour: f32, dir: glam::Vec3| -> Pos2 {
        let t = hour.rem_euclid(24.0) / 24.0;
        let x = left_x + t * (right_x - left_x);
        let y = if dir.y >= 0.0 {
            horizon_y - dir.y * (horizon_y - top_y)
        } else {
            horizon_y - dir.y * (bot_y - horizon_y)
        };
        Pos2::new(x, y)
    };

    let steps = 48;
    let sun_pts: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let h = (i as f32 / steps as f32) * 24.0;
            let d = void_engine::render::sun_direction(h, settings.sun_azimuth, settings.sun_tilt);
            to_pos(h, d)
        })
        .collect();
    let moon_pts: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let h = (i as f32 / steps as f32) * 24.0;
            let d = void_engine::render::moon_direction(
                h,
                settings.sun_azimuth,
                settings.sun_tilt,
                settings.moon_phase,
            );
            to_pos(h, d)
        })
        .collect();

    p.add(Shape::line(moon_pts, stroke(1.4, rgba(CYAN, 145))));
    p.add(Shape::line(sun_pts, stroke(1.8, rgba(AMBER, 195))));

    let cur_sun = to_pos(settings.time_of_day, sun_dir);
    let cur_moon = to_pos(settings.time_of_day, moon_dir);
    p.circle_filled(cur_moon, 6.0, rgba(CYAN, 48));
    p.circle_filled(cur_moon, 3.6, Color32::from_rgb(210, 232, 255));
    p.circle_filled(cur_sun, 7.5, rgba(AMBER, 60));
    p.circle_filled(cur_sun, 4.4, AMBER_BRIGHT);
}

fn draw_sun_overlay(
    ctx: &egui::Context,
    snap: &HudSnapshot<'_>,
    settings: &mut Settings,
    sun: &mut SunState,
    nav: &mut MenuNav,
    actions: &mut UiActions,
) {
    let dir = void_engine::render::sun_direction(
        settings.time_of_day,
        settings.sun_azimuth,
        settings.sun_tilt,
    );
    let moon_dir = void_engine::render::moon_direction(
        settings.time_of_day,
        settings.sun_azimuth,
        settings.sun_tilt,
        settings.moon_phase,
    );
    let elev_deg = dir.y.clamp(-1.0, 1.0).asin().to_degrees();
    let moon_elev_deg = moon_dir.y.clamp(-1.0, 1.0).asin().to_degrees();
    let (phase_label, phase_col) = day_phase(dir.y, settings.time_of_day);

    let card_w = 408.0;
    egui::Area::new(Id::new("sun_card"))
        .anchor(Align2::LEFT_BOTTOM, EguiVec2::new(36.0, -36.0))
        .show(ctx, |ui| {
            let resp = egui::Frame::none()
                .fill(BG_DEEP)
                .stroke(stroke(1.0, BORDER_MID))
                .rounding(Rounding::same(16.0))
                .inner_margin(Margin::same(18.0))
                .show(ui, |ui| {
                    ui.set_width(card_w - 36.0);

                    ui.horizontal(|ui| {
                        let (emblem_r, _) =
                            ui.allocate_exact_size(EguiVec2::splat(34.0), Sense::hover());
                        draw_void_emblem(
                            ui.painter(),
                            emblem_r.center(),
                            14.5,
                            snap.time,
                            phase_col,
                        );
                        ui.add_space(4.0);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.label(
                                egui::RichText::new("НЕБО, СОЛНЦЕ И ЛУНА")
                                    .size(16.5)
                                    .color(TEXT_PRIMARY)
                                    .strong(),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} · ☀ {:+.0}° · ☾ {:+.0}° · {}",
                                    format_clock(settings.time_of_day),
                                    elev_deg,
                                    moon_elev_deg,
                                    phase_label
                                ))
                                .font(FontId::monospace(10.5))
                                .color(phase_col),
                            );
                        });
                    });

                    ui.add_space(8.0);
                    draw_celestial_diagram(ui, settings, dir, moon_dir);
                    ui.add_space(8.0);

                    void_slider(
                        ui,
                        nav,
                        "sun_time",
                        "Время суток",
                        &mut settings.time_of_day,
                        0.0..=24.0,
                        format_clock,
                    );

                    ui.add_space(4.0);
                    let bw = (ui.available_width() - 18.0) / 4.0;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for (id, label, h) in [
                            ("p_dawn", "Рассвет", 6.35f32),
                            ("p_day", "День", 13.5),
                            ("p_sunset", "Закат", 17.65),
                            ("p_night", "Ночь", 23.0),
                        ] {
                            let active = (settings.time_of_day - h).abs() < 0.35;
                            if void_button(ui, nav, id, label, "", active, bw, 32.0) {
                                settings.time_of_day = h;
                                actions.click_sfx = true;
                            }
                        }
                    });

                    ui.add_space(12.0);
                    void_slider(
                        ui,
                        nav,
                        "sun_az",
                        "Азимут орбит",
                        &mut settings.sun_azimuth,
                        -180.0..=180.0,
                        |v| format!("{v:+.0}°"),
                    );
                    void_slider(
                        ui,
                        nav,
                        "sun_tilt",
                        "Высота орбиты",
                        &mut settings.sun_tilt,
                        15.0..=85.0,
                        |v| format!("{v:.0}°"),
                    );
                    void_slider(
                        ui,
                        nav,
                        "moon_phase",
                        "Фаза и орбита луны",
                        &mut settings.moon_phase,
                        0.0..=1.0,
                        format_moon_phase,
                    );
                    void_slider(
                        ui,
                        nav,
                        "sun_clouds",
                        "Плотность облаков",
                        &mut settings.cloud_coverage,
                        0.0..=1.0,
                        |v| format!("{:.0}%", v * 100.0),
                    );

                    ui.add_space(4.0);
                    if void_toggle(
                        ui,
                        nav,
                        "sun_traj",
                        "Траектория светил в небе",
                        "Орбитальные дуги солнца и луны в небе",
                        &mut settings.show_trajectory,
                    ) {
                        actions.click_sfx = true;
                    }
                    if void_toggle(
                        ui,
                        nav,
                        "sun_auto",
                        "Авто-смена дня и ночи",
                        "Плавное движение солнца и луны в реальном времени",
                        &mut sun.auto_cycle,
                    ) {
                        actions.click_sfx = true;
                    }
                    if sun.auto_cycle {
                        ui.add_space(6.0);
                        void_slider(
                            ui,
                            nav,
                            "sun_speed",
                            "Скорость цикла",
                            &mut sun.cycle_speed,
                            0.1..=4.0,
                            |v| format!("{v:.1} ч/с"),
                        );
                    }

                    ui.add_space(16.0);
                    let total_w = ui.available_width();
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let reset_w = 128.0;
                        let back_w = (total_w - reset_w - 10.0).max(120.0);
                        if void_button(ui, nav, "sun_reset", "СБРОС", "", false, reset_w, 40.0)
                        {
                            actions.reset_sun = true;
                            actions.click_sfx = true;
                        }
                        if void_button(ui, nav, "sun_back", "НАЗАД", "", true, back_w, 40.0) {
                            actions.close_sun = true;
                            actions.click_sfx = true;
                        }
                    });
                });

            draw_corner_brackets(
                ui.painter(),
                resp.response.rect.shrink(6.0),
                12.0,
                rgba(AMBER, 130),
                1.4,
            );
        });
    nav.finish();
}

pub fn render(
    ctx: &egui::Context,
    snap: &HudSnapshot<'_>,
    settings: &mut Settings,
    tab: &mut SettingsTab,
    sun: &mut SunState,
    nav: &mut MenuNav,
) -> UiActions {
    let mut actions = UiActions::default();

    match snap.state {
        ScreenState::Playing => draw_playing_hud(ctx, snap, settings.muted),
        ScreenState::Menu | ScreenState::Paused => draw_menu_overlay(ctx, snap, nav, &mut actions),
        ScreenState::Settings => draw_settings_overlay(ctx, snap, settings, tab, nav, &mut actions),
        ScreenState::Sun => draw_sun_overlay(ctx, snap, settings, sun, nav, &mut actions),
    }

    actions
}
