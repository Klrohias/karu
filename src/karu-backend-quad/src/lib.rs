use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping, SwashCache, Wrap};
use karu::{
    AppBackend, AppConfig, AppRoot, Brush, CaretAffinity, CaretPosition, Clipboard, ClipboardError,
    Color, Composition, Constraints, GradientStop, KeyCode, KeyEvent, KeyModifiers, Offset,
    PointerEvent, PointerPhase, Recomposer, Rect, RenderBackend, RenderCommand, Size,
    TextInputCommand, TextInputEvent, TextLayoutEngine, TextWrap,
};
use macroquad::input::{
    KeyCode as QuadKeyCode, MouseButton, TouchPhase, get_char_pressed, is_key_down, is_key_pressed,
    is_mouse_button_pressed, is_mouse_button_released, mouse_position, mouse_wheel, touches,
};
use macroquad::prelude::{
    Color as QuadColor, Conf, clear_background, draw_rectangle, draw_rectangle_lines, next_frame,
    screen_height, screen_width,
};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use unicode_segmentation::UnicodeSegmentation;

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
    let family = quad.font_family();
    let mut text_layout = CosmicTextLayout::new(family.clone());
    let mut backend = QuadBackend::new(family);
    let mut composition = Composition::new(root);
    let mut recomposer = Recomposer::new();
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
            let result = composition.dispatch_key_event_with_result_with(
                &mut text_layout,
                mouse_position,
                event,
            );
            suppress_character_input |= result.handled;
            for command in result.commands {
                match command {
                    TextInputCommand::Copy(text) | TextInputCommand::Cut(text) => {
                        let _ = backend.clipboard().set_text(&text);
                    }
                    TextInputCommand::PasteRequest => {
                        if let Ok(Some(text)) = backend.clipboard().get_text() {
                            composition.dispatch_text_input_event_with(
                                &mut text_layout,
                                TextInputEvent::Paste {
                                    position: mouse_position,
                                    text,
                                },
                            );
                        }
                    }
                }
            }
        }
        if !suppress_character_input {
            while let Some(character) = get_char_pressed() {
                if !character.is_control() {
                    composition.dispatch_text_input_event_with(
                        &mut text_layout,
                        TextInputEvent::Insert {
                            position: mouse_position,
                            text: character.to_string(),
                        },
                    );
                }
            }
        } else {
            while get_char_pressed().is_some() {}
        }
        if is_mouse_button_pressed(MouseButton::Left) {
            composition.dispatch_pointer_event_with(
                &mut text_layout,
                PointerEvent {
                    kind: karu::PointerKind::Mouse,
                    phase: PointerPhase::Down,
                    position: mouse_position,
                    primary: true,
                },
            );
        } else if is_mouse_button_released(MouseButton::Left) {
            composition.dispatch_pointer_event_with(
                &mut text_layout,
                PointerEvent {
                    kind: karu::PointerKind::Mouse,
                    phase: PointerPhase::Up,
                    position: mouse_position,
                    primary: true,
                },
            );
        } else if previous_mouse_position != Some(mouse_position) {
            composition.dispatch_pointer_event_with(
                &mut text_layout,
                PointerEvent {
                    kind: karu::PointerKind::Mouse,
                    phase: PointerPhase::Move,
                    position: mouse_position,
                    primary: false,
                },
            );
        }
        previous_mouse_position = Some(mouse_position);

        for touch in touches() {
            let phase = match touch.phase {
                TouchPhase::Started => PointerPhase::Down,
                TouchPhase::Stationary | TouchPhase::Moved => PointerPhase::Move,
                TouchPhase::Ended => PointerPhase::Up,
                TouchPhase::Cancelled => PointerPhase::Cancel,
            };
            composition.dispatch_pointer_event_with(
                &mut text_layout,
                PointerEvent {
                    kind: karu::PointerKind::Touch { id: touch.id },
                    phase,
                    position: karu::Offset::new(touch.position.x, touch.position.y),
                    primary: true,
                },
            );
        }

        let result = recomposer
            .recompose_with(&mut composition, &mut text_layout)
            .or_else(|| composition.last_result().cloned())
            .expect("composition result exists");

        backend
            .render(&result.render_tree, &result.commands)
            .expect("quad rendering succeeds");
        update_ime(&result.commands);

        next_frame().await;
    }
}

fn draw_command(
    command: &RenderCommand,
    text_layout: &mut CosmicTextLayout,
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
            wrap,
            offset,
            ..
        } => text_layout.draw_text(rect, text, style.font_size, style.color, *wrap, *offset),
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
    let scale = macroquad::miniquad::window::dpi_scale();
    let scissor = scissor_rect(rect, scale);
    unsafe { macroquad::window::get_internal_gl() }
        .quad_gl
        .scissor(Some(scissor));
}

fn scissor_rect(rect: Rect, scale: f32) -> (i32, i32, i32, i32) {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    (
        (rect.origin.x * scale).round() as i32,
        (rect.origin.y * scale).round() as i32,
        (rect.size.width.max(0.0) * scale).round() as i32,
        (rect.size.height.max(0.0) * scale).round() as i32,
    )
}

fn intersect_rect(left: Rect, right: Rect) -> Rect {
    let x = left.origin.x.max(right.origin.x);
    let y = left.origin.y.max(right.origin.y);
    let right_edge = (left.origin.x + left.size.width).min(right.origin.x + right.size.width);
    let bottom = (left.origin.y + left.size.height).min(right.origin.y + right.size.height);
    Rect::new(x, y, (right_edge - x).max(0.0), (bottom - y).max(0.0))
}

pub struct CosmicTextLayout {
    context: Rc<RefCell<CosmicTextContext>>,
    swash_cache: SwashCache,
    family: Option<String>,
}

pub(crate) struct CosmicTextContext {
    font_system: FontSystem,
}

impl CosmicTextLayout {
    pub fn new(family: Option<String>) -> Self {
        let context = Rc::new(RefCell::new(CosmicTextContext {
            font_system: FontSystem::new(),
        }));
        Self::with_context(context, family)
    }

    fn with_context(context: Rc<RefCell<CosmicTextContext>>, family: Option<String>) -> Self {
        Self {
            context,
            swash_cache: SwashCache::new(),
            family,
        }
    }

    fn draw_text(
        &mut self,
        rect: &Rect,
        text: &str,
        font_size: f32,
        color: Color,
        wrap: TextWrap,
        offset: karu::Offset,
    ) {
        let line_height = font_size.max(1.0) * (20.0 / 14.0);
        let mut context = self.context.borrow_mut();
        let mut buffer = Buffer::new(
            &mut context.font_system,
            Metrics::new(font_size.max(1.0), line_height),
        );
        buffer.set_size(
            Some(rect.size.width.max(1.0)),
            Some(rect.size.height.max(1.0)),
        );
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
        buffer.shape_until_scroll(&mut context.font_system, false);
        let base_x = snap_to_physical_pixel(offset.x);
        let base_y = snap_to_physical_pixel(offset.y);
        let quad_color = cosmic_color(color);
        buffer.draw(
            &mut context.font_system,
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

impl TextLayoutEngine for CosmicTextLayout {
    fn measure_text(&mut self, text: &str, font_size: f32, max_width: f32, wrap: TextWrap) -> Size {
        self.geometry(text, font_size, max_width, wrap).size
    }

    fn caret_position(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        offset: usize,
    ) -> CaretPosition {
        self.geometry(text, font_size, max_width, wrap)
            .caret(offset)
    }

    fn hit_test_text(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        position: Offset,
    ) -> usize {
        self.geometry(text, font_size, max_width, wrap)
            .hit_test(position)
    }

    fn text_line_range(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        offset: usize,
    ) -> Range<usize> {
        self.geometry(text, font_size, max_width, wrap)
            .line_range(offset)
    }

    fn selection_rects(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        range: Range<usize>,
    ) -> Vec<Rect> {
        self.geometry(text, font_size, max_width, wrap)
            .selection_rects(range)
    }
}

impl std::fmt::Debug for CosmicTextLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CosmicTextLayout")
    }
}

impl Default for CosmicTextLayout {
    fn default() -> Self {
        Self::new(None)
    }
}

pub struct QuadBackend {
    text_layout: CosmicTextLayout,
    clipboard: QuadClipboard,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct QuadClipboard;

impl Clipboard for QuadClipboard {
    fn get_text(&mut self) -> Result<Option<String>, ClipboardError> {
        Ok(macroquad::miniquad::window::clipboard_get())
    }

    fn set_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        macroquad::miniquad::window::clipboard_set(text);
        Ok(())
    }
}

impl QuadBackend {
    pub fn new(family: Option<String>) -> Self {
        Self {
            text_layout: CosmicTextLayout::new(family),
            clipboard: QuadClipboard,
        }
    }
}

impl RenderBackend for QuadBackend {
    type Output = ();
    type Error = std::convert::Infallible;
    type Clipboard = QuadClipboard;

    fn render(
        &mut self,
        _tree: &karu::RenderTree,
        commands: &[RenderCommand],
    ) -> Result<Self::Output, Self::Error> {
        let mut clips = Vec::new();
        for command in commands {
            draw_command(command, &mut self.text_layout, &mut clips);
        }
        Ok(())
    }

    fn clipboard(&mut self) -> &mut Self::Clipboard {
        &mut self.clipboard
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

#[derive(Clone, Debug)]
struct CosmicGeometry {
    size: Size,
    line_height: f32,
    lines: Vec<CosmicLine>,
    carets: Vec<CaretPosition>,
}

#[derive(Clone, Debug)]
struct CosmicLine {
    range: Range<usize>,
    origin: Offset,
    width: f32,
    height: f32,
}

impl CosmicTextLayout {
    fn geometry(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
    ) -> CosmicGeometry {
        let mut context = self.context.borrow_mut();
        let mut buffer = Buffer::new(
            &mut context.font_system,
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
        buffer.shape_until_scroll(&mut context.font_system, false);
        let starts = paragraph_starts(text);
        let mut lines = Vec::new();
        let mut carets = Vec::new();
        for run in buffer.layout_runs() {
            let base = starts.get(run.line_i).copied().unwrap_or(0);
            let start = run
                .glyphs
                .iter()
                .map(|glyph| glyph.start)
                .min()
                .unwrap_or(0);
            let end = run
                .glyphs
                .iter()
                .map(|glyph| glyph.end)
                .max()
                .unwrap_or(start);
            let line_index = lines.len();
            let line = CosmicLine {
                range: base + start..base + end,
                origin: Offset::new(0.0, run.line_top),
                width: run.line_w,
                height: run.line_height,
            };
            lines.push(line.clone());
            carets.push(CaretPosition {
                offset: line.range.start,
                position: karu::Offset::new(0.0, run.line_top),
                line: line_index,
                height: run.line_height,
                affinity: karu::CaretAffinity::After,
            });
            for glyph in run.glyphs {
                let cluster = &run.text[glyph.start..glyph.end];
                let graphemes = cluster.grapheme_indices(true).collect::<Vec<_>>();
                let width = glyph.w / graphemes.len().max(1) as f32;
                for (index, grapheme) in graphemes {
                    let x = glyph.x + index as f32 * width;
                    let start_x = if glyph.level.is_rtl() { x + width } else { x };
                    let end_x = if glyph.level.is_rtl() { x } else { x + width };
                    let absolute = base + glyph.start + index;
                    carets.push(CaretPosition {
                        offset: absolute,
                        position: karu::Offset::new(start_x, run.line_top),
                        line: line_index,
                        height: run.line_height,
                        affinity: karu::CaretAffinity::After,
                    });
                    carets.push(CaretPosition {
                        offset: absolute + grapheme.len(),
                        position: karu::Offset::new(end_x, run.line_top),
                        line: line_index,
                        height: run.line_height,
                        affinity: karu::CaretAffinity::Before,
                    });
                }
            }
        }
        if lines.is_empty() {
            let line_height = font_size.max(1.0) * (20.0 / 14.0);
            lines.push(CosmicLine {
                range: 0..text.len(),
                origin: karu::Offset::ZERO,
                width: 0.0,
                height: line_height,
            });
            carets.push(CaretPosition {
                offset: 0,
                position: karu::Offset::ZERO,
                line: 0,
                height: line_height,
                affinity: karu::CaretAffinity::After,
            });
        }
        let width = lines.iter().map(|line| line.width).fold(0.0, f32::max);
        let height = lines
            .iter()
            .map(|line| line.origin.y + line.height)
            .fold(0.0, f32::max);
        CosmicGeometry {
            size: Size::new(width, height),
            line_height: font_size.max(1.0) * (20.0 / 14.0),
            lines,
            carets,
        }
    }
}

impl CosmicGeometry {
    fn caret(&self, offset: usize) -> CaretPosition {
        let offset = offset.min(self.carets.last().map_or(0, |caret| caret.offset));
        self.carets
            .iter()
            .find(|caret| caret.offset == offset && caret.affinity == CaretAffinity::After)
            .or_else(|| {
                self.carets
                    .iter()
                    .min_by_key(|caret| caret.offset.abs_diff(offset))
            })
            .copied()
            .unwrap_or(CaretPosition {
                offset: 0,
                position: Offset::ZERO,
                line: 0,
                height: self.line_height,
                affinity: CaretAffinity::After,
            })
    }

    fn hit_test(&self, position: Offset) -> usize {
        let line_index = self
            .lines
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| {
                distance_to_cosmic_line(left, position.y)
                    .partial_cmp(&distance_to_cosmic_line(right, position.y))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(index, _)| index)
            .unwrap_or(0);
        let mut carets = self
            .carets
            .iter()
            .filter(|caret| caret.line == line_index)
            .copied()
            .collect::<Vec<_>>();
        carets.sort_by(|left, right| {
            left.position
                .x
                .partial_cmp(&right.position.x)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.offset.cmp(&right.offset))
        });
        let Some(first) = carets.first() else {
            return self
                .lines
                .get(line_index)
                .map_or(0, |line| line.range.start);
        };
        if position.x <= first.position.x {
            return first.offset;
        }
        for pair in carets.windows(2) {
            if position.x < (pair[0].position.x + pair[1].position.x) * 0.5 {
                return pair[0].offset;
            }
        }
        carets.last().map_or(first.offset, |caret| caret.offset)
    }

    fn line_range(&self, offset: usize) -> Range<usize> {
        self.lines
            .iter()
            .find(|line| offset >= line.range.start && offset <= line.range.end)
            .map(|line| line.range.clone())
            .unwrap_or_else(|| self.lines.last().map_or(0..0, |line| line.range.clone()))
    }

    fn selection_rects(&self, range: Range<usize>) -> Vec<Rect> {
        if range.start == range.end {
            return Vec::new();
        }
        let start = range.start.min(range.end);
        let end = range.start.max(range.end);
        let mut rects = Vec::new();
        for (index, line) in self.lines.iter().enumerate() {
            let overlaps = if line.range.start == line.range.end {
                start <= line.range.start && end > line.range.start
            } else {
                start < line.range.end && end > line.range.start
            };
            if !overlaps {
                continue;
            }
            let line_start = if start <= line.range.start {
                line.origin.x
            } else {
                self.caret_on_line(index, start, true).position.x
            };
            let line_end = if end >= line.range.end {
                line.origin.x + line.width
            } else {
                self.caret_on_line(index, end, false).position.x
            };
            if line_end > line_start {
                rects.push(Rect::new(
                    line_start,
                    line.origin.y,
                    line_end - line_start,
                    line.height,
                ));
            }
        }
        rects
    }

    fn caret_on_line(&self, line: usize, offset: usize, prefer_after: bool) -> CaretPosition {
        self.carets
            .iter()
            .filter(|caret| caret.line == line && caret.offset == offset)
            .find(|caret| {
                (prefer_after && caret.affinity == CaretAffinity::After)
                    || (!prefer_after && caret.affinity == CaretAffinity::Before)
            })
            .or_else(|| {
                self.carets
                    .iter()
                    .filter(|caret| caret.line == line)
                    .min_by_key(|caret| caret.offset.abs_diff(offset))
            })
            .copied()
            .unwrap_or(CaretPosition {
                offset,
                position: self
                    .lines
                    .get(line)
                    .map_or(Offset::ZERO, |line| line.origin),
                line,
                height: self.line_height,
                affinity: CaretAffinity::After,
            })
    }
}

fn distance_to_cosmic_line(line: &CosmicLine, y: f32) -> f32 {
    if y < line.origin.y {
        line.origin.y - y
    } else if y > line.origin.y + line.height {
        y - line.origin.y - line.height
    } else {
        0.0
    }
}

fn paragraph_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (offset, _) in text.match_indices('\n') {
        starts.push(offset + 1);
    }
    starts
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
#[karu::composable]
fn cosmic_text_content() {
    karu::__private::Text("Karu Foundation Playground", karu::TextOptions::default());
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

    #[test]
    fn converts_logical_clip_to_framebuffer_pixels() {
        assert_eq!(
            scissor_rect(Rect::new(10.0, 20.0, 100.0, 40.0), 1.0),
            (10, 20, 100, 40)
        );
        assert_eq!(
            scissor_rect(Rect::new(10.0, 20.0, 100.0, 40.0), 2.0),
            (20, 40, 200, 80)
        );
    }

    #[test]
    fn scissor_conversion_handles_invalid_scale_and_empty_size() {
        assert_eq!(
            scissor_rect(Rect::new(10.5, 20.5, -4.0, 0.0), 0.0),
            (11, 21, 0, 0)
        );
        assert_eq!(
            scissor_rect(Rect::new(10.0, 20.0, 100.0, 40.0), f32::INFINITY),
            (10, 20, 100, 40)
        );
    }

    #[test]
    fn cosmic_renderer_builds_carets_from_shaped_clusters() {
        let mut renderer = CosmicTextLayout::new(None);
        let text = "a你😀e\u{301}";
        let caret =
            renderer.caret_position(text, 14.0, f32::INFINITY, TextWrap::NoWrap, text.len());
        let size =
            renderer.measure_text("Karu Foundation Playground", 24.0, 600.0, TextWrap::NoWrap);

        assert_eq!(caret.offset, text.len());
        assert!(caret.position.x >= 0.0);
        assert!(size.width > 0.0 && size.height > 0.0);
    }

    #[test]
    fn cosmic_composition_text_commands_have_visible_viewports() {
        let mut composition = karu::Composition::new(cosmic_text_content)
            .with_constraints(karu::Constraints::loose(800.0, 600.0));
        let mut renderer = CosmicTextLayout::new(None);
        composition.compose_with(&mut renderer);
        let result = composition
            .last_result()
            .expect("composition result exists");
        let command = result
            .commands
            .iter()
            .find_map(|command| match command {
                karu::RenderCommand::DrawText { rect, .. } => Some(*rect),
                _ => None,
            })
            .expect("text command exists");
        assert!(command.size.width > 0.0 && command.size.height > 0.0);
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
