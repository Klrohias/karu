use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping, SwashCache, Wrap};
use karu::{
    AppBackend, AppConfig, AppRoot, Brush, Color, Composition, Constraints, GradientStop, KeyCode,
    KeyEvent, KeyModifiers, PointerEvent, PointerPhase, Rect, RenderCommand, TextInputEvent,
    TextLayout, TextLayoutEngine, TextWrap,
};
use macroquad::input::{
    KeyCode as QuadKeyCode, MouseButton, TouchPhase, get_char_pressed, is_key_down, is_key_pressed,
    is_mouse_button_pressed, is_mouse_button_released, mouse_position, mouse_wheel, touches,
};
use macroquad::prelude::{
    Color as QuadColor, Conf, clear_background, draw_rectangle, draw_rectangle_lines, next_frame,
    screen_height, screen_width,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct Quad;

impl Quad {
    pub fn new() -> Self {
        Self
    }

    #[cfg(feature = "font-kit")]
    pub fn default_system_font(self) -> ConfiguredQuad {
        ConfiguredQuad::default().default_system_font()
    }

    #[cfg(feature = "font-kit")]
    pub fn system_font(self, family: impl Into<String>) -> ConfiguredQuad {
        ConfiguredQuad::default().system_font(family)
    }
}

#[derive(Clone, Debug, Default)]
pub struct ConfiguredQuad {
    #[cfg(feature = "font-kit")]
    font: Option<FontConfig>,
}

impl ConfiguredQuad {
    fn font_family(&self) -> Option<String> {
        #[cfg(feature = "font-kit")]
        {
            match self.font.as_ref()? {
                FontConfig::DefaultSystem => None,
                FontConfig::SystemFamily(family) => Some(family.clone()),
            }
        }
        #[cfg(not(feature = "font-kit"))]
        {
            None
        }
    }

    #[cfg(feature = "font-kit")]
    pub fn default_system_font(mut self) -> Self {
        self.font = Some(FontConfig::DefaultSystem);
        self
    }

    #[cfg(feature = "font-kit")]
    pub fn system_font(mut self, family: impl Into<String>) -> Self {
        self.font = Some(FontConfig::SystemFamily(family.into()));
        self
    }
}

#[cfg(feature = "font-kit")]
#[derive(Clone, Debug, PartialEq, Eq)]
enum FontConfig {
    DefaultSystem,
    SystemFamily(String),
}

impl AppBackend for Quad {
    fn run(self, root: AppRoot, config: AppConfig) {
        ConfiguredQuad::default().run(root, config);
    }
}

impl AppBackend for ConfiguredQuad {
    fn run(self, root: AppRoot, config: AppConfig) {
        let window = Conf {
            window_title: config.title.clone(),
            window_width: config.width as i32,
            window_height: config.height as i32,
            window_resizable: config.resizable,
            ..Default::default()
        };

        macroquad::Window::from_config(window, run_quad(root, config, self));
    }
}

async fn run_quad(root: AppRoot, config: AppConfig, quad: ConfiguredQuad) {
    let mut composition = Composition::new(root);
    let mut text_renderer = CosmicTextRenderer::new(quad.font_family());
    macroquad::input::simulate_mouse_with_touch(false);
    let mut previous_mouse_position = None;

    loop {
        clear_background(to_quad_color(config.background));

        composition.set_constraints(Constraints::loose(screen_width(), screen_height()));
        let mouse = mouse_position();
        let mouse_position = karu::Offset::new(mouse.0, mouse.1);
        let wheel = mouse_wheel();
        if wheel.0 != 0.0 || wheel.1 != 0.0 {
            composition.dispatch_scroll_event(karu::ScrollEvent {
                position: mouse_position,
                delta: karu::Offset::new(wheel.0, -wheel.1 * 24.0),
            });
        }
        let modifiers = KeyModifiers {
            shift: is_key_down(QuadKeyCode::LeftShift) || is_key_down(QuadKeyCode::RightShift),
            ctrl: is_key_down(QuadKeyCode::LeftControl) || is_key_down(QuadKeyCode::RightControl),
            alt: is_key_down(QuadKeyCode::LeftAlt) || is_key_down(QuadKeyCode::RightAlt),
            logo: is_key_down(QuadKeyCode::LeftSuper) || is_key_down(QuadKeyCode::RightSuper),
        };
        let mut suppress_character_input = modifiers.command() || modifiers.alt;
        for (quad_key, key) in keyboard_keys() {
            if !is_key_pressed(quad_key) {
                continue;
            }
            let event = KeyEvent {
                code: key,
                modifiers,
                repeat: false,
            };
            if key == KeyCode::V && modifiers.command() {
                suppress_character_input = true;
                if let Some(text) = macroquad::miniquad::window::clipboard_get() {
                    composition.dispatch_text_input_event(TextInputEvent::Paste {
                        position: mouse_position,
                        text,
                    });
                }
            } else {
                let result = composition.dispatch_key_event_with_result(mouse_position, event);
                if let Some(text) = result.clipboard {
                    macroquad::miniquad::window::clipboard_set(&text);
                }
            }
        }
        if !suppress_character_input {
            while let Some(character) = get_char_pressed() {
                if !character.is_control() {
                    composition.dispatch_text_input_event(TextInputEvent::Insert {
                        position: mouse_position,
                        text: character.to_string(),
                    });
                }
            }
        } else {
            while get_char_pressed().is_some() {}
        }
        if is_mouse_button_pressed(MouseButton::Left) {
            composition.dispatch_pointer_event(PointerEvent {
                kind: karu::PointerKind::Mouse,
                phase: PointerPhase::Down,
                position: mouse_position,
                primary: true,
            });
        } else if is_mouse_button_released(MouseButton::Left) {
            composition.dispatch_pointer_event(PointerEvent {
                kind: karu::PointerKind::Mouse,
                phase: PointerPhase::Up,
                position: mouse_position,
                primary: true,
            });
        } else if previous_mouse_position != Some(mouse_position) {
            composition.dispatch_pointer_event(PointerEvent {
                kind: karu::PointerKind::Mouse,
                phase: PointerPhase::Move,
                position: mouse_position,
                primary: false,
            });
        }
        previous_mouse_position = Some(mouse_position);

        for touch in touches() {
            let phase = match touch.phase {
                TouchPhase::Started => PointerPhase::Down,
                TouchPhase::Stationary | TouchPhase::Moved => PointerPhase::Move,
                TouchPhase::Ended => PointerPhase::Up,
                TouchPhase::Cancelled => PointerPhase::Cancel,
            };
            composition.dispatch_pointer_event(PointerEvent {
                kind: karu::PointerKind::Touch { id: touch.id },
                phase,
                position: karu::Offset::new(touch.position.x, touch.position.y),
                primary: true,
            });
        }

        let result = if composition.last_result().is_none() || composition.is_dirty() {
            composition.compose()
        } else {
            composition
                .last_result()
                .expect("composition result exists")
                .clone()
        };

        let commands = karu::commands_for_tree_with_engine(
            &result.render_tree.root,
            &text_renderer.layout_engine,
        );
        update_ime(&commands);

        let mut clips = Vec::new();
        for command in &commands {
            draw_command(command, &mut text_renderer, &mut clips);
        }

        next_frame().await;
    }
}

fn draw_command(
    command: &RenderCommand,
    text_renderer: &mut CosmicTextRenderer,
    clips: &mut Vec<Rect>,
) {
    match command {
        RenderCommand::FillRect { rect, color, .. } => draw_rect(*rect, *color),
        RenderCommand::FillBrush { rect, brush, .. } => draw_brush(*rect, brush),
        RenderCommand::StrokeRect {
            rect, brush, width, ..
        } => draw_stroke(*rect, brush, *width),
        RenderCommand::DrawText {
            rect,
            text,
            style,
            layout,
            offset,
            ..
        } => text_renderer.draw_text(rect, text, style.font_size, style.color, layout, *offset),
        RenderCommand::DrawSelection { rect, color, .. }
        | RenderCommand::DrawCursor { rect, color, .. }
        | RenderCommand::DrawComposition { rect, color, .. } => draw_rect(*rect, *color),
        RenderCommand::PushClip(rect) => {
            let rect = snap_rect_to_physical_pixels(*rect);
            let rect = clips
                .last()
                .copied()
                .map(|parent| intersect_rect(parent, rect))
                .unwrap_or(rect);
            clips.push(rect);
            set_scissor(rect);
        }
        RenderCommand::PopClip => {
            clips.pop();
            if let Some(rect) = clips.last().copied() {
                set_scissor(rect);
            } else {
                unsafe { macroquad::window::get_internal_gl() }
                    .quad_gl
                    .scissor(None);
            }
        }
        RenderCommand::DrawImage { .. } => {}
    }
}

fn set_scissor(rect: Rect) {
    unsafe { macroquad::window::get_internal_gl() }
        .quad_gl
        .scissor(Some((
            rect.origin.x as i32,
            (screen_height() - rect.origin.y - rect.size.height) as i32,
            rect.size.width.max(0.0) as i32,
            rect.size.height.max(0.0) as i32,
        )));
}

fn intersect_rect(left: Rect, right: Rect) -> Rect {
    let x = left.origin.x.max(right.origin.x);
    let y = left.origin.y.max(right.origin.y);
    let right_edge = (left.origin.x + left.size.width).min(right.origin.x + right.size.width);
    let bottom = (left.origin.y + left.size.height).min(right.origin.y + right.size.height);
    Rect::new(x, y, (right_edge - x).max(0.0), (bottom - y).max(0.0))
}

struct CosmicTextRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    family: Option<String>,
    layout_engine: CosmicTextLayoutEngine,
}

impl CosmicTextRenderer {
    fn new(family: Option<String>) -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            layout_engine: CosmicTextLayoutEngine::new(family.clone()),
            family,
        }
    }

    fn draw_text(
        &mut self,
        rect: &Rect,
        text: &str,
        font_size: f32,
        color: Color,
        layout: &TextLayout,
        offset: karu::Offset,
    ) {
        let line_height = layout.line_height.max(font_size);
        let mut buffer = Buffer::new(
            &mut self.font_system,
            Metrics::new(font_size.max(1.0), line_height),
        );
        buffer.set_size(
            Some(rect.size.width.max(1.0)),
            Some(rect.size.height.max(1.0)),
        );
        buffer.set_wrap(if layout.lines.len() > 1 {
            Wrap::WordOrGlyph
        } else {
            Wrap::None
        });
        let attrs = self
            .family
            .as_deref()
            .map(|family| Attrs::new().family(cosmic_text::Family::Name(family)))
            .unwrap_or_else(Attrs::new);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);
        let base_x = snap_to_physical_pixel(offset.x);
        let base_y = snap_to_physical_pixel(offset.y);
        let quad_color = cosmic_color(color);
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            quad_color,
            |x, y, width, height, pixel: cosmic_text::Color| {
                let pixel = QuadColor::new(
                    f32::from(pixel.r()) / 255.0,
                    f32::from(pixel.g()) / 255.0,
                    f32::from(pixel.b()) / 255.0,
                    f32::from(pixel.a()) / 255.0,
                );
                draw_rectangle(
                    base_x + x as f32,
                    base_y + y as f32,
                    width as f32,
                    height as f32,
                    pixel,
                );
            },
        );
    }
}

fn cosmic_color(color: Color) -> cosmic_text::Color {
    cosmic_text::Color::rgba(
        (color.red.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.green.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.blue.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

/// Quad's system-font layout implementation. It uses cosmic-text for font
/// discovery, shaping, fallback, wrapping, and measurement.
pub struct CosmicTextLayoutEngine {
    font_system: std::cell::RefCell<FontSystem>,
    family: Option<String>,
}

impl std::fmt::Debug for CosmicTextLayoutEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CosmicTextLayoutEngine")
    }
}

impl Default for CosmicTextLayoutEngine {
    fn default() -> Self {
        Self::new(None)
    }
}

impl CosmicTextLayoutEngine {
    pub fn new(family: Option<String>) -> Self {
        Self {
            font_system: std::cell::RefCell::new(FontSystem::new()),
            family,
        }
    }
}

impl TextLayoutEngine for CosmicTextLayoutEngine {
    fn layout(&self, text: &str, font_size: f32, max_width: f32, wrap: TextWrap) -> TextLayout {
        let basic = karu::BasicTextLayoutEngine;
        let mut fallback = basic.layout(text, font_size, max_width, wrap);
        let mut font_system = self.font_system.borrow_mut();
        let mut buffer = Buffer::new(
            &mut font_system,
            Metrics::new(font_size.max(1.0), font_size.max(1.0) * (20.0 / 14.0)),
        );
        buffer.set_size(max_width.is_finite().then_some(max_width.max(1.0)), None);
        buffer.set_wrap(match wrap {
            TextWrap::NoWrap => Wrap::None,
            TextWrap::Word => Wrap::WordOrGlyph,
            TextWrap::Character => Wrap::Glyph,
        });
        let attrs = self
            .family
            .as_deref()
            .map(|family| Attrs::new().family(cosmic_text::Family::Name(family)))
            .unwrap_or_else(Attrs::new);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);
        let mut shaped_widths = vec![None; fallback.lines.len()];
        for run in buffer.layout_runs() {
            if let Some(line) = fallback.lines.get(run.line_i) {
                let basic_width = shaped_widths[run.line_i].unwrap_or(line.width);
                let first_run = shaped_widths[run.line_i].is_none();
                if first_run {
                    shaped_widths[run.line_i] = Some(run.line_w);
                    if basic_width > 0.0 {
                        fallback.scale_carets_on_line(run.line_i, run.line_w / basic_width);
                    }
                }
            }
            if let Some(line) = fallback.lines.get_mut(run.line_i) {
                line.width = run.line_w;
                line.origin.y = run.line_top;
                line.height = run.line_height;
            }
        }
        fallback.size.width = fallback
            .lines
            .iter()
            .map(|line| line.width)
            .fold(0.0, f32::max);
        fallback.size.height = fallback.lines.iter().map(|line| line.height).sum::<f32>();
        fallback
    }
}

fn keyboard_keys() -> [(QuadKeyCode, KeyCode); 17] {
    [
        (QuadKeyCode::Left, KeyCode::Left),
        (QuadKeyCode::Right, KeyCode::Right),
        (QuadKeyCode::Up, KeyCode::Up),
        (QuadKeyCode::Down, KeyCode::Down),
        (QuadKeyCode::Home, KeyCode::Home),
        (QuadKeyCode::End, KeyCode::End),
        (QuadKeyCode::Backspace, KeyCode::Backspace),
        (QuadKeyCode::Delete, KeyCode::Delete),
        (QuadKeyCode::Enter, KeyCode::Enter),
        (QuadKeyCode::Tab, KeyCode::Tab),
        (QuadKeyCode::Escape, KeyCode::Escape),
        (QuadKeyCode::A, KeyCode::A),
        (QuadKeyCode::C, KeyCode::C),
        (QuadKeyCode::V, KeyCode::V),
        (QuadKeyCode::X, KeyCode::X),
        (QuadKeyCode::Y, KeyCode::Y),
        (QuadKeyCode::Z, KeyCode::Z),
    ]
}

fn update_ime(commands: &[RenderCommand]) {
    let cursor = commands.iter().find_map(|command| {
        if let RenderCommand::DrawCursor { rect, .. } = command {
            Some(*rect)
        } else {
            None
        }
    });
    macroquad::miniquad::window::set_ime_enabled(cursor.is_some());
    if let Some(rect) = cursor {
        let scale = macroquad::miniquad::window::dpi_scale();
        macroquad::miniquad::window::set_ime_position(
            (rect.origin.x * scale) as i32,
            ((rect.origin.y + rect.size.height) * scale) as i32,
        );
    }
}

fn draw_rect(rect: Rect, color: Color) {
    let rect = snap_rect_to_physical_pixels(rect);

    draw_rectangle(
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
        to_quad_color(color),
    );
}

fn draw_brush(rect: Rect, brush: &Brush) {
    match brush {
        Brush::Solid(color) => draw_rect(rect, *color),
        Brush::LinearGradient { stops, .. } => {
            if stops.is_empty() {
                return;
            }
            let segments = 32;
            for index in 0..segments {
                let start = index as f32 / segments as f32;
                let end = (index + 1) as f32 / segments as f32;
                draw_rect(
                    Rect::new(
                        rect.origin.x + rect.size.width * start,
                        rect.origin.y,
                        rect.size.width * (end - start) + 1.0,
                        rect.size.height,
                    ),
                    gradient_color(stops, (start + end) * 0.5),
                );
            }
        }
    }
}

fn draw_stroke(rect: Rect, brush: &Brush, width: f32) {
    let color = match brush {
        Brush::Solid(color) => *color,
        Brush::LinearGradient { stops, .. } => gradient_color(stops, 0.5),
    };
    let rect = snap_rect_to_physical_pixels(rect);
    draw_rectangle_lines(
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
        width,
        to_quad_color(color),
    );
}

fn gradient_color(stops: &[GradientStop], position: f32) -> Color {
    let position = position.clamp(0.0, 1.0);
    if position <= stops[0].position {
        return stops[0].color;
    }
    for pair in stops.windows(2) {
        let from = &pair[0];
        let to = &pair[1];
        if position <= to.position {
            let span = (to.position - from.position).max(f32::EPSILON);
            return from.color.lerp(to.color, (position - from.position) / span);
        }
    }
    stops.last().expect("gradient has at least one stop").color
}

fn to_quad_color(color: Color) -> QuadColor {
    QuadColor::new(color.red, color.green, color.blue, color.alpha)
}

fn snap_rect_to_physical_pixels(rect: Rect) -> Rect {
    let x = snap_to_physical_pixel(rect.origin.x);
    let y = snap_to_physical_pixel(rect.origin.y);
    let right = snap_to_physical_pixel(rect.origin.x + rect.size.width);
    let bottom = snap_to_physical_pixel(rect.origin.y + rect.size.height);

    Rect::new(x, y, (right - x).max(0.0), (bottom - y).max(0.0))
}

fn snap_to_physical_pixel(value: f32) -> f32 {
    snap_value_to_scale(value, macroquad::miniquad::window::dpi_scale())
}

fn snap_value_to_scale(value: f32, scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        (value * scale).round() / scale
    } else {
        value
    }
}

#[allow(dead_code)]
fn font_size(size: f32) -> u16 {
    size.max(1.0).round().min(u16::MAX as f32) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_karu_color_to_quad_color() {
        let color = to_quad_color(Color::rgba(0.1, 0.2, 0.3, 0.4));

        assert_eq!(color.r, 0.1);
        assert_eq!(color.g, 0.2);
        assert_eq!(color.b, 0.3);
        assert_eq!(color.a, 0.4);
    }

    #[test]
    fn clamps_font_size_to_macroquad_range() {
        assert_eq!(font_size(0.0), 1);
        assert_eq!(font_size(14.4), 14);
        assert_eq!(font_size(f32::INFINITY), u16::MAX);
    }

    #[test]
    fn snaps_values_to_physical_pixel_scale() {
        assert_eq!(snap_value_to_scale(10.24, 2.0), 10.0);
        assert_eq!(snap_value_to_scale(10.26, 2.0), 10.5);
        assert_eq!(snap_value_to_scale(10.25, 0.0), 10.25);
    }

    #[cfg(feature = "font-kit")]
    #[test]
    fn configures_system_font_family() {
        let quad = Quad::new().system_font("Arial");
        assert_eq!(
            quad.font,
            Some(FontConfig::SystemFamily("Arial".to_string()))
        );

        let quad = Quad.system_font("PingFang SC");
        assert_eq!(
            quad.font,
            Some(FontConfig::SystemFamily("PingFang SC".to_string()))
        );
    }

    #[cfg(feature = "font-kit")]
    #[test]
    fn configures_default_system_font() {
        let quad = Quad::new().default_system_font();
        assert_eq!(quad.font, Some(FontConfig::DefaultSystem));

        let quad = Quad.default_system_font();
        assert_eq!(quad.font, Some(FontConfig::DefaultSystem));
    }
}
