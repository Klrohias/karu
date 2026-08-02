use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping, SwashCache, Wrap};
use karu::{
    AppBackend, AppConfig, AppRoot, Brush, CaretAffinity, CaretPosition, Clipboard, ClipboardError,
    Color, Composition, Constraints, GradientStop, KeyCode, KeyEvent, KeyModifiers, Offset,
    PointerEvent, PointerPhase, Recomposer, Rect, RenderBackend, RenderCommand, Size,
    TextEditCommand, TextInputCommand, TextInputEvent, TextInputResult, TextLayoutEngine, TextWrap,
};
use macroquad::conf::{Conf, UpdateTrigger};
use macroquad::input::{
    KeyCode as QuadKeyCode, MouseButton, TouchPhase, get_char_pressed, is_key_down, is_key_pressed,
    is_mouse_button_pressed, is_mouse_button_released, mouse_position, mouse_wheel, touches,
};
use macroquad::prelude::{
    Color as QuadColor, Material, MaterialParams, Mesh, ShaderSource, UniformDesc, UniformType,
    Vertex, clear_background, draw_mesh, draw_rectangle, gl_use_default_material, gl_use_material,
    next_frame, screen_height, screen_width,
};
use macroquad::window::miniquad::{Backend, BlendFactor, BlendState, BlendValue, Equation};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::time::{Duration, Instant};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, Default)]
pub struct Quad;

impl Quad {
    pub fn new() -> Self {
        Self
    }

    pub fn on_demand(self) -> ConfiguredQuad {
        ConfiguredQuad::default().on_demand()
    }

    pub fn continuous(self) -> ConfiguredQuad {
        ConfiguredQuad::default().continuous()
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuadFrameMode {
    #[default]
    OnDemand,
    Continuous,
}

#[derive(Clone, Debug, Default)]
pub struct ConfiguredQuad {
    frame_mode: QuadFrameMode,
    #[cfg(feature = "font-kit")]
    font: Option<FontConfig>,
}

impl ConfiguredQuad {
    pub fn frame_mode(mut self, frame_mode: QuadFrameMode) -> Self {
        self.frame_mode = frame_mode;
        self
    }

    pub fn on_demand(self) -> Self {
        self.frame_mode(QuadFrameMode::OnDemand)
    }

    pub fn continuous(self) -> Self {
        self.frame_mode(QuadFrameMode::Continuous)
    }

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
        let window = window_config(&config, self.frame_mode);

        macroquad::Window::from_config(window, run_quad(root, config, self));
    }
}

fn window_config(config: &AppConfig, frame_mode: QuadFrameMode) -> Conf {
    let mut window = Conf::default();
    window.miniquad_conf.window_title = config.title.clone();
    window.miniquad_conf.window_width = config.width as i32;
    window.miniquad_conf.window_height = config.height as i32;
    window.miniquad_conf.window_resizable = config.resizable;

    if frame_mode == QuadFrameMode::OnDemand {
        window.miniquad_conf.platform.blocking_event_loop = true;
        window.update_on = Some(UpdateTrigger {
            key_down: true,
            mouse_down: true,
            mouse_up: true,
            mouse_motion: true,
            mouse_wheel: true,
            touch: true,
            ..Default::default()
        });
    } else {
        window.update_on = None;
    }

    window
}

async fn run_quad(root: AppRoot, config: AppConfig, quad: ConfiguredQuad) {
    let family = quad.font_family();
    let mut text_layout = CosmicTextLayout::new(family.clone());
    let mut backend = QuadBackend::new(family);
    let mut composition = Composition::new(root);
    let mut recomposer = Recomposer::new();
    let mut stats = QuadFrameStats::new(std::env::var_os("KARU_QUAD_DEBUG").is_some());
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
            suppress_character_input |= handle_text_result(
                result,
                &mut composition,
                &mut text_layout,
                &mut backend,
                mouse_position,
            );
        }
        for quad_key in shortcut_keys() {
            if !is_key_pressed(quad_key) {
                continue;
            }
            let Some(command) = edit_command(quad_key, modifiers) else {
                continue;
            };
            let result = composition.dispatch_text_input_event_with_result_with(
                &mut text_layout,
                TextInputEvent::Command {
                    position: mouse_position,
                    command,
                },
            );
            suppress_character_input |= handle_text_result(
                result,
                &mut composition,
                &mut text_layout,
                &mut backend,
                mouse_position,
            );
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

        let recomposed =
            if let Some(result) = recomposer.recompose_with(&mut composition, &mut text_layout) {
                render_result(&mut backend, &result);
                true
            } else {
                let result = composition
                    .last_result()
                    .expect("composition result exists after the first frame");
                render_result(&mut backend, result);
                false
            };

        stats.record(recomposed, composition.last_result());

        next_frame().await;
    }
}

fn render_result(backend: &mut QuadBackend, result: &karu::CompositionResult) {
    backend
        .render(&result.render_tree, &result.commands)
        .expect("quad rendering succeeds");
    update_ime(&result.commands);
}

struct QuadFrameStats {
    enabled: bool,
    started: Instant,
    frames: u64,
    recompositions: u64,
    commands: usize,
}

const MAX_QUAD_CLIPS: usize = 8;

#[derive(Clone, Copy)]
struct QuadClip {
    rect: Rect,
    radius: f32,
}

impl QuadFrameStats {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            started: Instant::now(),
            frames: 0,
            recompositions: 0,
            commands: 0,
        }
    }

    fn record(&mut self, recomposed: bool, result: Option<&karu::CompositionResult>) {
        if !self.enabled {
            return;
        }

        self.frames += 1;
        self.recompositions += u64::from(recomposed);
        self.commands = result.map_or(0, |result| result.commands.len());

        let elapsed = self.started.elapsed();
        if elapsed >= Duration::from_secs(1) {
            eprintln!(
                "[karu][quad] frames={} recompositions={} commands={} elapsed={:?}",
                self.frames, self.recompositions, self.commands, elapsed,
            );
            self.started = Instant::now();
            self.frames = 0;
            self.recompositions = 0;
        }
    }
}

fn draw_command(
    command: &RenderCommand,
    text_layout: &mut CosmicTextLayout,
    clips: &mut Vec<QuadClip>,
) {
    match command {
        RenderCommand::FillRect {
            rect,
            color,
            radius,
            ..
        } => draw_fill(*rect, &Brush::Solid(*color), *radius),
        RenderCommand::FillBrush {
            rect,
            brush,
            radius,
            ..
        } => draw_fill(*rect, brush, *radius),
        RenderCommand::StrokeRect {
            rect,
            brush,
            width,
            radius,
            ..
        } => draw_stroke(*rect, brush, *width, *radius),
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
        RenderCommand::PushClip { rect, radius, .. } => {
            let rect = snap_rect_to_physical_pixels(*rect);
            let rect = clips
                .last()
                .copied()
                .map(|parent| intersect_rect(parent.rect, rect))
                .unwrap_or(rect);
            clips.push(QuadClip {
                rect,
                radius: *radius,
            });
            set_scissor(rect);
        }
        RenderCommand::PopClip => {
            clips.pop();
            if let Some(clip) = clips.last().copied() {
                set_scissor(clip.rect);
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
    clip_material: Material,
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
            clip_material: create_quad_clip_material(),
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
        gl_use_material(&self.clip_material);
        set_quad_clip_material(&self.clip_material, &clips);
        for command in commands {
            draw_command(command, &mut self.text_layout, &mut clips);
            set_quad_clip_material(&self.clip_material, &clips);
        }
        gl_use_default_material();
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

fn handle_text_result(
    result: TextInputResult,
    composition: &mut Composition,
    text_layout: &mut CosmicTextLayout,
    backend: &mut QuadBackend,
    position: Offset,
) -> bool {
    for command in result.commands {
        match command {
            TextInputCommand::Copy(text) | TextInputCommand::Cut(text) => {
                let _ = backend.clipboard().set_text(&text);
            }
            TextInputCommand::PasteRequest => {
                if let Ok(Some(text)) = backend.clipboard().get_text() {
                    composition.dispatch_text_input_event_with(
                        text_layout,
                        TextInputEvent::Paste { position, text },
                    );
                }
            }
        }
    }
    result.handled
}

fn keyboard_keys() -> [(QuadKeyCode, KeyCode); 11] {
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
    ]
}

fn shortcut_keys() -> [QuadKeyCode; 6] {
    [
        QuadKeyCode::A,
        QuadKeyCode::C,
        QuadKeyCode::V,
        QuadKeyCode::X,
        QuadKeyCode::Y,
        QuadKeyCode::Z,
    ]
}

fn edit_command(key: QuadKeyCode, modifiers: KeyModifiers) -> Option<TextEditCommand> {
    if !modifiers.command() {
        return None;
    }
    Some(match key {
        QuadKeyCode::A => TextEditCommand::SelectAll,
        QuadKeyCode::C => TextEditCommand::Copy,
        QuadKeyCode::V => TextEditCommand::Paste,
        QuadKeyCode::X => TextEditCommand::Cut,
        QuadKeyCode::Z if modifiers.shift => TextEditCommand::Redo,
        QuadKeyCode::Z => TextEditCommand::Undo,
        QuadKeyCode::Y => TextEditCommand::Redo,
        _ => return None,
    })
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

fn set_quad_clip_material(material: &Material, clips: &[QuadClip]) {
    let mut rects = [macroquad::math::Vec4::ZERO; MAX_QUAD_CLIPS];
    let mut radii = [macroquad::math::Vec4::ZERO; 2];
    for (index, clip) in clips.iter().take(MAX_QUAD_CLIPS).enumerate() {
        rects[index] = macroquad::math::vec4(
            clip.rect.origin.x,
            clip.rect.origin.y,
            clip.rect.size.width.max(0.0),
            clip.rect.size.height.max(0.0),
        );
        radii[index / 4][index % 4] = clip.radius.max(0.0);
    }
    material.set_uniform_array("clip_rects", &rects);
    material.set_uniform_array("clip_radii", &radii);
    material.set_uniform("clip_count", clips.len().min(MAX_QUAD_CLIPS) as f32);
}

fn create_quad_clip_material() -> Material {
    let context = unsafe { macroquad::window::get_internal_gl().quad_context };
    let shader = match context.info().backend {
        Backend::OpenGl => ShaderSource::Glsl {
            vertex: QUAD_CLIP_VERTEX,
            fragment: QUAD_CLIP_FRAGMENT,
        },
        Backend::Metal => ShaderSource::Msl {
            program: QUAD_CLIP_METAL,
        },
    };
    let pipeline_params = macroquad::prelude::PipelineParams {
        color_blend: Some(BlendState::new(
            Equation::Add,
            BlendFactor::Value(BlendValue::SourceAlpha),
            BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
        )),
        ..Default::default()
    };
    macroquad::prelude::load_material(
        shader,
        MaterialParams {
            pipeline_params,
            uniforms: vec![
                UniformDesc::array(
                    UniformDesc::new("clip_rects", UniformType::Float4),
                    MAX_QUAD_CLIPS,
                ),
                UniformDesc::array(UniformDesc::new("clip_radii", UniformType::Float4), 2),
                UniformDesc::new("clip_count", UniformType::Float1),
            ],
            ..Default::default()
        },
    )
    .expect("quad rounded clip material compiles")
}

const QUAD_CLIP_VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;

varying lowp vec4 color;
varying highp vec2 world_position;
varying lowp vec2 uv;

uniform mat4 Model;
uniform mat4 Projection;

void main() {
    vec4 world = Model * vec4(position, 1.0);
    gl_Position = Projection * world;
    color = color0 / 255.0;
    world_position = world.xy;
    uv = texcoord;
}"#;

const QUAD_CLIP_FRAGMENT: &str = r#"#version 100
varying lowp vec4 color;
varying highp vec2 world_position;
varying lowp vec2 uv;

uniform sampler2D Texture;
uniform highp vec4 clip_rects[8];
uniform highp vec4 clip_radii[2];
uniform highp float clip_count;

bool inside_rounded_rect(highp vec2 position, highp vec4 rect, highp float radius) {
    if (position.x < rect.x || position.y < rect.y
        || position.x > rect.x + rect.z || position.y > rect.y + rect.w) {
        return false;
    }
    highp float r = min(radius, min(rect.z, rect.w) * 0.5);
    if (r <= 0.0) {
        return true;
    }
    highp vec2 local = position - rect.xy;
    highp float dx = max(r - local.x, max(0.0, local.x - (rect.z - r)));
    highp float dy = max(r - local.y, max(0.0, local.y - (rect.w - r)));
    return dx * dx + dy * dy <= r * r;
}

void main() {
    for (int index = 0; index < 8; index++) {
        if (float(index) >= clip_count) {
            break;
        }
        highp float radius = clip_radii[index / 4][index - (index / 4) * 4];
        if (!inside_rounded_rect(world_position, clip_rects[index], radius)) {
            discard;
        }
    }
    gl_FragColor = color * texture2D(Texture, uv);
}"#;

const QUAD_CLIP_METAL: &str = r#"#include <metal_stdlib>
using namespace metal;

struct Vertex {
    float3 position [[attribute(0)]];
    float2 texcoord [[attribute(1)]];
    float4 color0 [[attribute(2)]];
};

struct Uniforms {
    float4x4 Model;
    float4x4 Projection;
    float4 _Time;
    float4 clip_rects[8];
    float4 clip_radii[2];
    float clip_count;
};

struct RasterizerData {
    float4 position [[position]];
    float4 color [[user(locn0)]];
    float2 world_position [[user(locn1)]];
    float2 uv [[user(locn2)]];
};

vertex RasterizerData vertexShader(Vertex vertex [[stage_in]], constant Uniforms& uniforms [[buffer(0)]]) {
    RasterizerData out;
    float4 world = uniforms.Model * float4(vertex.position, 1.0);
    out.position = uniforms.Projection * world;
    out.color = vertex.color0 / 255.0;
    out.world_position = world.xy;
    out.uv = vertex.texcoord;
    return out;
}

bool inside_rounded_rect(float2 position, float4 rect, float radius) {
    if (position.x < rect.x || position.y < rect.y
        || position.x > rect.x + rect.z || position.y > rect.y + rect.w) {
        return false;
    }
    float r = min(radius, min(rect.z, rect.w) * 0.5);
    if (r <= 0.0) {
        return true;
    }
    float2 local = position - rect.xy;
    float dx = max(r - local.x, max(0.0, local.x - (rect.z - r)));
    float dy = max(r - local.y, max(0.0, local.y - (rect.w - r)));
    return dx * dx + dy * dy <= r * r;
}

fragment float4 fragmentShader(RasterizerData input [[stage_in]], constant Uniforms& uniforms [[buffer(0)]], texture2d<float> texture [[texture(0)]], sampler sampler [[sampler(0)]]) {
    for (int index = 0; index < 8; index++) {
        if (float(index) >= uniforms.clip_count) {
            break;
        }
        float radius = uniforms.clip_radii[index / 4][index - (index / 4) * 4];
        if (!inside_rounded_rect(input.world_position, uniforms.clip_rects[index], radius)) {
            discard_fragment();
        }
    }
    return input.color * texture.sample(sampler, input.uv);
}"#;

fn draw_fill(rect: Rect, brush: &Brush, radius: f32) {
    let rect = snap_rect_to_physical_pixels(rect);
    if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }
    if radius <= 0.0 {
        match brush {
            Brush::Solid(color) => draw_rect(rect, *color),
            Brush::LinearGradient { stops, .. } if stops.is_empty() => {}
            Brush::LinearGradient { .. } => draw_mesh(&fill_mesh(rect, brush, 0.0)),
        }
    } else {
        draw_mesh(&fill_mesh(rect, brush, radius));
    }
}

fn draw_stroke(rect: Rect, brush: &Brush, width: f32, radius: f32) {
    let rect = snap_rect_to_physical_pixels(rect);
    let vertices = stroke_vertices(rect, brush, width, radius);
    if vertices.is_empty() {
        return;
    }
    let vertex_count = vertices.len() as u16;
    draw_mesh(&Mesh {
        vertices,
        indices: (0..vertex_count).collect(),
        texture: None,
    });
}

fn fill_mesh(rect: Rect, brush: &Brush, radius: f32) -> Mesh {
    let radius = radius
        .max(0.0)
        .min(rect.size.width.min(rect.size.height) * 0.5);
    let points = rounded_points(rect, radius);
    let center = [
        rect.origin.x + rect.size.width * 0.5,
        rect.origin.y + rect.size.height * 0.5,
    ];
    let mut vertices = Vec::with_capacity(points.len() + 1);
    vertices.push(Vertex::new(
        center[0],
        center[1],
        0.0,
        0.0,
        0.0,
        brush_color(brush, center[0], center[1]),
    ));
    for point in &points {
        vertices.push(Vertex::new(
            point[0],
            point[1],
            0.0,
            0.0,
            0.0,
            brush_color(brush, point[0], point[1]),
        ));
    }
    let mut indices = Vec::with_capacity(points.len() * 3);
    for index in 0..points.len() {
        indices.extend([
            0,
            (index + 1) as u16,
            ((index + 1) % points.len() + 1) as u16,
        ]);
    }
    Mesh {
        vertices,
        indices,
        texture: None,
    }
}

fn stroke_vertices(rect: Rect, brush: &Brush, width: f32, radius: f32) -> Vec<Vertex> {
    let min_dimension = rect.size.width.min(rect.size.height);
    let width = width.max(0.0).min(min_dimension * 0.5);
    if width == 0.0 || min_dimension <= 0.0 {
        return Vec::new();
    }
    let radius = radius.max(0.0).min(min_dimension * 0.5);
    if radius == 0.0 {
        let x = rect.origin.x;
        let y = rect.origin.y;
        let right = x + rect.size.width;
        let bottom = y + rect.size.height;
        return vec![
            quad_vertex(brush, [x, y]),
            quad_vertex(brush, [right, y]),
            quad_vertex(brush, [right, y + width]),
            quad_vertex(brush, [x, y]),
            quad_vertex(brush, [right, y + width]),
            quad_vertex(brush, [x, y + width]),
            quad_vertex(brush, [x, bottom - width]),
            quad_vertex(brush, [right, bottom - width]),
            quad_vertex(brush, [right, bottom]),
            quad_vertex(brush, [x, bottom - width]),
            quad_vertex(brush, [right, bottom]),
            quad_vertex(brush, [x, bottom]),
            quad_vertex(brush, [x, y + width]),
            quad_vertex(brush, [x + width, y + width]),
            quad_vertex(brush, [x + width, bottom - width]),
            quad_vertex(brush, [x, y + width]),
            quad_vertex(brush, [x + width, bottom - width]),
            quad_vertex(brush, [x, bottom - width]),
            quad_vertex(brush, [right - width, y + width]),
            quad_vertex(brush, [right, y + width]),
            quad_vertex(brush, [right, bottom - width]),
            quad_vertex(brush, [right - width, y + width]),
            quad_vertex(brush, [right, bottom - width]),
            quad_vertex(brush, [right - width, bottom - width]),
        ];
    }
    let outer = rounded_points(rect, radius);
    let inner_rect = Rect::new(
        rect.origin.x + width,
        rect.origin.y + width,
        (rect.size.width - width * 2.0).max(0.0),
        (rect.size.height - width * 2.0).max(0.0),
    );
    let inner = rounded_points(inner_rect, (radius - width).max(0.0));
    let mut vertices = Vec::with_capacity(outer.len() * 6);
    for index in 0..outer.len() {
        let next = (index + 1) % outer.len();
        let outer0 = outer[index];
        let outer1 = outer[next];
        let inner0 = inner[index];
        let inner1 = inner[next];
        vertices.extend([
            quad_vertex(brush, outer0),
            quad_vertex(brush, outer1),
            quad_vertex(brush, inner1),
            quad_vertex(brush, outer0),
            quad_vertex(brush, inner1),
            quad_vertex(brush, inner0),
        ]);
    }
    vertices
}

fn rounded_points(rect: Rect, radius: f32) -> Vec<[f32; 2]> {
    const SEGMENTS: usize = 8;
    let radius = radius
        .max(0.0)
        .min(rect.size.width.min(rect.size.height) * 0.5);
    let x = rect.origin.x;
    let y = rect.origin.y;
    let right = x + rect.size.width;
    let bottom = y + rect.size.height;
    let corners = [
        (x + radius, y + radius, std::f32::consts::PI),
        (right - radius, y + radius, 1.5 * std::f32::consts::PI),
        (right - radius, bottom - radius, 0.0),
        (x + radius, bottom - radius, 0.5 * std::f32::consts::PI),
    ];
    let mut points = Vec::with_capacity(SEGMENTS * 4);
    for (cx, cy, start) in corners {
        for step in 0..SEGMENTS {
            let angle = start + step as f32 * std::f32::consts::FRAC_PI_2 / SEGMENTS as f32;
            points.push([cx + radius * angle.cos(), cy + radius * angle.sin()]);
        }
    }
    points
}

fn quad_vertex(brush: &Brush, point: [f32; 2]) -> Vertex {
    Vertex::new(
        point[0],
        point[1],
        0.0,
        0.0,
        0.0,
        brush_color(brush, point[0], point[1]),
    )
}

fn brush_color(brush: &Brush, x: f32, y: f32) -> QuadColor {
    to_quad_color(match brush {
        Brush::Solid(color) => *color,
        Brush::LinearGradient { start, end, stops } => {
            let dx = end.x - start.x;
            let dy = end.y - start.y;
            let denominator = dx * dx + dy * dy;
            let position = if denominator > 0.0 {
                ((x - start.x) * dx + (y - start.y) * dy) / denominator
            } else {
                0.0
            };
            gradient_color(stops, position)
        }
    })
}

fn gradient_color(stops: &[GradientStop], position: f32) -> Color {
    let Some(first) = stops.first() else {
        return Color::TRANSPARENT;
    };
    let position = position.clamp(0.0, 1.0);
    if position <= first.position {
        return first.color;
    }
    for pair in stops.windows(2) {
        let from = &pair[0];
        let to = &pair[1];
        if position <= to.position {
            let span = (to.position - from.position).max(f32::EPSILON);
            return from.color.lerp(to.color, (position - from.position) / span);
        }
    }
    stops.last().map_or(first.color, |stop| stop.color)
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
    fn on_demand_mode_uses_a_blocking_event_loop() {
        let window = window_config(&karu::AppConfig::default(), QuadFrameMode::OnDemand);

        assert!(window.miniquad_conf.platform.blocking_event_loop);
        let update_on = window.update_on.expect("on-demand mode has input triggers");
        assert!(update_on.key_down);
        assert!(update_on.mouse_motion);
        assert!(update_on.mouse_wheel);
        assert!(update_on.touch);
    }

    #[test]
    fn continuous_mode_keeps_the_event_loop_running() {
        let window = window_config(&karu::AppConfig::default(), QuadFrameMode::Continuous);

        assert!(!window.miniquad_conf.platform.blocking_event_loop);
        assert!(window.update_on.is_none());
    }

    #[test]
    fn quad_builders_select_the_requested_frame_mode() {
        assert_eq!(
            ConfiguredQuad::default().frame_mode,
            QuadFrameMode::OnDemand
        );
        assert_eq!(
            Quad::new().continuous().frame_mode,
            QuadFrameMode::Continuous
        );
        assert_eq!(Quad::new().on_demand().frame_mode, QuadFrameMode::OnDemand);
    }

    #[test]
    fn converts_karu_color_to_quad_color() {
        let color = to_quad_color(Color::rgba(0.1, 0.2, 0.3, 0.4));

        assert_eq!(color.r, 0.1);
        assert_eq!(color.g, 0.2);
        assert_eq!(color.b, 0.3);
        assert_eq!(color.a, 0.4);
    }

    #[test]
    fn rounded_fill_mesh_is_bounded_and_clamps_radius() {
        let mesh = fill_mesh(
            Rect::new(0.0, 0.0, 100.0, 40.0),
            &Brush::Solid(Color::WHITE),
            100.0,
        );

        assert_eq!(mesh.vertices.len(), 33);
        assert_eq!(mesh.indices.len(), 96);
        assert!(mesh.vertices.iter().all(|vertex| {
            vertex.position.x >= 0.0
                && vertex.position.x <= 100.0
                && vertex.position.y >= 0.0
                && vertex.position.y <= 40.0
        }));
    }

    #[test]
    fn rounded_quad_stroke_has_four_corner_arcs() {
        let vertices = stroke_vertices(
            Rect::new(0.0, 0.0, 100.0, 40.0),
            &Brush::Solid(Color::BLACK),
            2.0,
            8.0,
        );

        assert_eq!(vertices.len(), 192);
        assert!(vertices.iter().any(|vertex| {
            (vertex.position.x - 0.0).abs() < 0.001 && (vertex.position.y - 8.0).abs() < 0.001
        }));
        assert!(vertices.iter().any(|vertex| {
            (vertex.position.x - 100.0).abs() < 0.001 && (vertex.position.y - 32.0).abs() < 0.001
        }));
    }

    #[test]
    fn quad_gradient_stroke_uses_endpoint_colors() {
        let brush = Brush::LinearGradient {
            start: Offset::new(0.0, 0.0),
            end: Offset::new(100.0, 0.0),
            stops: vec![
                GradientStop {
                    position: 0.0,
                    color: Color::BLACK,
                },
                GradientStop {
                    position: 1.0,
                    color: Color::WHITE,
                },
            ],
        };
        let vertices = stroke_vertices(Rect::new(0.0, 0.0, 100.0, 40.0), &brush, 2.0, 8.0);

        assert!(vertices.iter().any(|vertex| vertex.color == [0, 0, 0, 255]));
        assert!(
            vertices
                .iter()
                .any(|vertex| vertex.color == [255, 255, 255, 255])
        );
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

    #[test]
    fn shortcut_mapping_keeps_textual_keys_out_of_key_mapping() {
        assert!(keyboard_keys().iter().all(|(key, _)| {
            !matches!(
                key,
                QuadKeyCode::A
                    | QuadKeyCode::C
                    | QuadKeyCode::V
                    | QuadKeyCode::X
                    | QuadKeyCode::Y
                    | QuadKeyCode::Z
            )
        }));
        assert_eq!(
            edit_command(
                QuadKeyCode::V,
                KeyModifiers {
                    ctrl: true,
                    ..Default::default()
                }
            ),
            Some(TextEditCommand::Paste)
        );
        assert_eq!(edit_command(QuadKeyCode::V, KeyModifiers::default()), None);
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
