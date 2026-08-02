use arboard::Clipboard as SystemClipboard;
use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping, SwashCache, Wrap};
use karu::{
    AppBackend, AppConfig, AppRoot, Brush, CaretAffinity, CaretPosition, Clipboard, Color,
    Composition, Constraints, GradientStop, KeyCode, KeyEvent, KeyModifiers, Offset, PointerEvent,
    PointerKind, PointerPhase, Recomposer, Rect, RenderBackend, RenderCommand, Size,
    TextEditCommand, TextInputCommand, TextInputEvent, TextLayoutEngine, TextWrap,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::Infallible;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::num::NonZeroU64;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use unicode_segmentation::UnicodeSegmentation;
use wgpu::util::DeviceExt;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode as WinitKeyCode, ModifiersState, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

#[derive(Clone, Copy, Debug, Default)]
pub struct Wgpu;

impl Wgpu {
    pub fn new() -> Self {
        Self
    }

    pub fn default_system_font(self) -> ConfiguredWgpu {
        ConfiguredWgpu::default().default_system_font()
    }

    pub fn system_font(self, family: impl Into<String>) -> ConfiguredWgpu {
        ConfiguredWgpu::default().system_font(family)
    }

    pub fn enable_debug_info(self) -> ConfiguredWgpu {
        ConfiguredWgpu::default().enable_debug_info()
    }
}

#[derive(Clone, Debug, Default)]
pub struct ConfiguredWgpu {
    font: Option<FontConfig>,
    debug_info: bool,
}

impl ConfiguredWgpu {
    pub fn default_system_font(mut self) -> Self {
        self.font = Some(FontConfig::DefaultSystem);
        self
    }

    pub fn system_font(mut self, family: impl Into<String>) -> Self {
        self.font = Some(FontConfig::SystemFamily(family.into()));
        self
    }

    pub fn enable_debug_info(mut self) -> Self {
        self.debug_info = true;
        self
    }

    fn font_family(&self) -> Option<String> {
        match self.font.as_ref() {
            Some(FontConfig::SystemFamily(family)) => Some(family.clone()),
            Some(FontConfig::DefaultSystem) | None => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum FontConfig {
    DefaultSystem,
    SystemFamily(String),
}

impl AppBackend for Wgpu {
    fn run(self, root: AppRoot, config: AppConfig) {
        ConfiguredWgpu::default().run(root, config);
    }
}

impl AppBackend for ConfiguredWgpu {
    fn run(self, root: AppRoot, config: AppConfig) {
        let event_loop = EventLoop::new().expect("failed to create winit event loop");
        let mut app = WgpuApp::new(root, config, self.font_family(), self.debug_info);
        event_loop
            .run_app(&mut app)
            .expect("winit event loop failed");
    }
}

struct WgpuApp {
    root: Option<AppRoot>,
    config: AppConfig,
    family: Option<String>,
    debug_info: bool,
    window: Option<Arc<Window>>,
    runtime: Option<WgpuRuntime>,
    cursor: Offset,
    modifiers: ModifiersState,
    redraw_pending: bool,
}

impl WgpuApp {
    fn new(root: AppRoot, config: AppConfig, family: Option<String>, debug_info: bool) -> Self {
        Self {
            root: Some(root),
            config,
            family,
            debug_info,
            window: None,
            runtime: None,
            cursor: Offset::ZERO,
            modifiers: ModifiersState::empty(),
            redraw_pending: false,
        }
    }

    fn request_redraw(&mut self) {
        if self.redraw_pending {
            return;
        }
        self.redraw_pending = true;
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn request_redraw_if_needed(&mut self) {
        let dirty = self
            .runtime
            .as_ref()
            .is_some_and(|runtime| runtime.composition.is_dirty());
        if dirty {
            self.request_redraw();
        }
    }

    fn redraw(&mut self) {
        self.redraw_pending = false;
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };

        let scale = window.scale_factor() as f32;
        let physical = window.inner_size();
        if physical.width == 0 || physical.height == 0 {
            return;
        }
        let logical_width = physical.width as f32 / scale.max(0.001);
        let logical_height = physical.height as f32 / scale.max(0.001);
        runtime.renderer.resize_with_scale(physical, scale);
        runtime
            .composition
            .set_constraints(Constraints::loose(logical_width, logical_height));

        let recomposed_result = runtime
            .recomposer
            .recompose_with(&mut runtime.composition, &mut runtime.text_layout);
        let result = recomposed_result
            .or_else(|| runtime.composition.last_result().cloned())
            .expect("composition result exists");

        runtime
            .renderer
            .render(&result.render_tree, &result.commands)
            .expect("wgpu rendering failed");
        update_ime(window, &result.commands);
    }

    fn dispatch_pointer(&mut self, event: PointerEvent) {
        if let Some(runtime) = self.runtime.as_mut() {
            runtime
                .composition
                .dispatch_pointer_event_with(&mut runtime.text_layout, event);
        }
    }

    fn dispatch_scroll(&mut self, delta: Offset) -> bool {
        if let Some(runtime) = self.runtime.as_mut() {
            return runtime
                .composition
                .dispatch_scroll_event(karu::ScrollEvent {
                    position: self.cursor,
                    delta,
                });
        }
        false
    }

    fn dispatch_text(&mut self, event: TextInputEvent) -> bool {
        let result = if let Some(runtime) = self.runtime.as_mut() {
            runtime
                .composition
                .dispatch_text_input_event_with_result_with(&mut runtime.text_layout, event)
        } else {
            karu::TextInputResult::default()
        };
        let handled = result.handled;
        result
            .commands
            .into_iter()
            .for_each(|command| self.handle_text_command(command));
        handled
    }

    fn dispatch_key(&mut self, event: KeyEvent) -> bool {
        self.dispatch_text(TextInputEvent::Key {
            position: self.cursor,
            event,
        })
    }

    fn handle_text_command(&mut self, command: TextInputCommand) {
        match command {
            TextInputCommand::Copy(text) | TextInputCommand::Cut(text) => {
                if let Some(runtime) = self.runtime.as_mut() {
                    let _ = runtime.renderer.clipboard().set_text(&text);
                }
            }
            TextInputCommand::PasteRequest => {
                let text = self
                    .runtime
                    .as_mut()
                    .and_then(|runtime| runtime.renderer.clipboard().get_text().ok().flatten());
                if let Some(text) = text {
                    self.dispatch_text(TextInputEvent::Paste {
                        position: self.cursor,
                        text,
                    });
                }
            }
        }
    }
}

impl ApplicationHandler for WgpuApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.runtime.is_some() {
            return;
        }
        let attributes = WindowAttributes::default()
            .with_title(self.config.title.clone())
            .with_inner_size(LogicalSize::new(
                self.config.width as f64,
                self.config.height as f64,
            ))
            .with_resizable(self.config.resizable);
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("failed to create winit window"),
        );
        let renderer = pollster::block_on(WgpuBackend::new(
            window.clone(),
            self.config.background,
            self.family.clone(),
            self.debug_info,
        ));
        let context = renderer.text_context();
        let mut text_layout = CosmicTextLayout::with_context(context, self.family.clone());
        let root = self.root.take().expect("application root already consumed");
        let mut composition = Composition::new(root);
        composition.set_constraints(Constraints::loose(
            self.config.width as f32,
            self.config.height as f32,
        ));
        composition.compose_with(&mut text_layout);
        self.window = Some(window.clone());
        self.runtime = Some(WgpuRuntime {
            renderer,
            text_layout,
            composition,
            recomposer: Recomposer::new(),
        });
        self.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.window.as_ref().map(|window| window.id()) != Some(id) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(runtime) = self.runtime.as_mut() {
                    runtime.renderer.resize(size);
                }
                self.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(window) = self.window.clone() {
                    if let Some(runtime) = self.runtime.as_mut() {
                        runtime
                            .renderer
                            .resize_with_scale(window.inner_size(), window.scale_factor() as f32);
                    }
                }
                self.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = logical_position(position, self.window.as_ref());
                self.dispatch_pointer(PointerEvent {
                    kind: PointerKind::Mouse,
                    phase: PointerPhase::Move,
                    position: self.cursor,
                    primary: false,
                });
                self.request_redraw_if_needed();
            }
            WindowEvent::MouseInput { state, button, .. } if button == MouseButton::Left => {
                self.dispatch_pointer(PointerEvent {
                    kind: PointerKind::Mouse,
                    phase: if state == ElementState::Pressed {
                        PointerPhase::Down
                    } else {
                        PointerPhase::Up
                    },
                    position: self.cursor,
                    primary: true,
                });
                self.request_redraw_if_needed();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let delta = match delta {
                    MouseScrollDelta::LineDelta(x, y) => Offset::new(x * 24.0, -y * 24.0),
                    MouseScrollDelta::PixelDelta(position) => {
                        Offset::new(position.x as f32, -position.y as f32)
                    }
                };
                if self.dispatch_scroll(delta) {
                    self.request_redraw();
                }
            }
            WindowEvent::Touch(touch) => {
                self.cursor = logical_position(touch.location, self.window.as_ref());
                self.dispatch_pointer(PointerEvent {
                    kind: PointerKind::Touch { id: touch.id },
                    phase: match touch.phase {
                        TouchPhase::Started => PointerPhase::Down,
                        TouchPhase::Moved => PointerPhase::Move,
                        TouchPhase::Ended => PointerPhase::Up,
                        TouchPhase::Cancelled => PointerPhase::Cancel,
                    },
                    position: self.cursor,
                    primary: true,
                });
                self.request_redraw_if_needed();
            }
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let modifiers = to_modifiers(self.modifiers);
                let mut suppress_text = modifiers.command() || modifiers.alt;
                if let Some(command) = map_edit_command(event.physical_key, modifiers) {
                    suppress_text |= self.dispatch_text(TextInputEvent::Command {
                        position: self.cursor,
                        command,
                    });
                } else if let Some(code) = map_key(event.physical_key) {
                    suppress_text |= self.dispatch_key(KeyEvent {
                        code,
                        modifiers,
                        repeat: event.repeat,
                    });
                }
                if !suppress_text && let Some(text) = event.text {
                    if !text.chars().all(char::is_control) {
                        self.dispatch_text(TextInputEvent::Insert {
                            position: self.cursor,
                            text: text.to_string(),
                        });
                    }
                }
                self.request_redraw_if_needed();
            }
            WindowEvent::Ime(ime) => {
                match ime {
                    Ime::Enabled => {
                        self.dispatch_text(TextInputEvent::CompositionStart {
                            position: self.cursor,
                        });
                    }
                    Ime::Preedit(text, _) => {
                        self.dispatch_text(TextInputEvent::CompositionUpdate {
                            position: self.cursor,
                            text,
                        });
                    }
                    Ime::Commit(text) => {
                        self.dispatch_text(TextInputEvent::CompositionCommit {
                            position: self.cursor,
                            text,
                        });
                    }
                    Ime::Disabled => {
                        self.dispatch_text(TextInputEvent::CompositionEnd {
                            position: self.cursor,
                        });
                    }
                }
                self.request_redraw_if_needed();
            }
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.request_redraw_if_needed();
    }
}

struct WgpuRuntime {
    renderer: WgpuBackend,
    text_layout: CosmicTextLayout,
    composition: Composition,
    recomposer: Recomposer,
}

fn logical_position(position: PhysicalPosition<f64>, window: Option<&Arc<Window>>) -> Offset {
    let scale = window.map_or(1.0, |window| window.scale_factor()) as f32;
    Offset::new(position.x as f32 / scale, position.y as f32 / scale)
}

fn to_modifiers(state: ModifiersState) -> KeyModifiers {
    KeyModifiers {
        shift: state.shift_key(),
        ctrl: state.control_key(),
        alt: state.alt_key(),
        logo: state.super_key(),
    }
}

fn map_key(key: PhysicalKey) -> Option<KeyCode> {
    Some(match key {
        PhysicalKey::Code(WinitKeyCode::ArrowLeft) => KeyCode::Left,
        PhysicalKey::Code(WinitKeyCode::ArrowRight) => KeyCode::Right,
        PhysicalKey::Code(WinitKeyCode::ArrowUp) => KeyCode::Up,
        PhysicalKey::Code(WinitKeyCode::ArrowDown) => KeyCode::Down,
        PhysicalKey::Code(WinitKeyCode::Home) => KeyCode::Home,
        PhysicalKey::Code(WinitKeyCode::End) => KeyCode::End,
        PhysicalKey::Code(WinitKeyCode::Backspace) => KeyCode::Backspace,
        PhysicalKey::Code(WinitKeyCode::Delete) => KeyCode::Delete,
        PhysicalKey::Code(WinitKeyCode::Enter) => KeyCode::Enter,
        PhysicalKey::Code(WinitKeyCode::Tab) => KeyCode::Tab,
        PhysicalKey::Code(WinitKeyCode::Escape) => KeyCode::Escape,
        _ => return None,
    })
}

fn map_edit_command(key: PhysicalKey, modifiers: KeyModifiers) -> Option<TextEditCommand> {
    if !modifiers.command() {
        return None;
    }
    let PhysicalKey::Code(key) = key else {
        return None;
    };
    Some(match key {
        WinitKeyCode::KeyA => TextEditCommand::SelectAll,
        WinitKeyCode::KeyC => TextEditCommand::Copy,
        WinitKeyCode::KeyV => TextEditCommand::Paste,
        WinitKeyCode::KeyX => TextEditCommand::Cut,
        WinitKeyCode::KeyZ if modifiers.shift => TextEditCommand::Redo,
        WinitKeyCode::KeyZ => TextEditCommand::Undo,
        WinitKeyCode::KeyY => TextEditCommand::Redo,
        _ => return None,
    })
}

fn update_ime(window: &Window, commands: &[RenderCommand]) {
    let cursor = commands.iter().find_map(|command| match command {
        RenderCommand::DrawCursor { rect, .. } => Some(*rect),
        _ => None,
    });
    window.set_ime_allowed(cursor.is_some());
    if let Some(rect) = cursor {
        let scale = window.scale_factor() as f32;
        let position = PhysicalPosition::new(
            (rect.origin.x * scale).round() as i32,
            ((rect.origin.y + rect.size.height) * scale).round() as i32,
        );
        window.set_ime_cursor_area(position, PhysicalSize::new(1, 1));
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ScreenUniform {
    size: [f32; 2],
    clip_count: u32,
    _padding: u32,
    clip_rects: [[f32; 4]; MAX_CLIPS],
    clip_radii: [[f32; 4]; 2],
}

const MAX_CLIPS: usize = 8;

#[derive(Clone, Copy)]
struct ClipRect {
    rect: Rect,
    radius: f32,
}

fn screen_uniform(size: [f32; 2], clips: &[ClipRect]) -> ScreenUniform {
    let mut clip_rects = [[0.0; 4]; MAX_CLIPS];
    let mut clip_radii = [[0.0; 4]; 2];
    for (index, clip) in clips.iter().take(MAX_CLIPS).enumerate() {
        clip_rects[index] = [
            clip.rect.origin.x,
            clip.rect.origin.y,
            clip.rect.size.width.max(0.0),
            clip.rect.size.height.max(0.0),
        ];
        clip_radii[index / 4][index % 4] = clip.radius.max(0.0);
    }
    ScreenUniform {
        size,
        clip_count: clips.len().min(MAX_CLIPS) as u32,
        _padding: 0,
        clip_rects,
        clip_radii,
    }
}

fn clip_uniforms_for_commands(
    commands: &[RenderCommand],
    logical_size: [f32; 2],
) -> Vec<ScreenUniform> {
    let mut clips = Vec::new();
    let mut uniforms = Vec::with_capacity(commands.len().max(1));
    for command in commands {
        uniforms.push(screen_uniform(logical_size, &clips));
        match command {
            RenderCommand::PushClip { rect, radius, .. } => {
                let rect = clips
                    .last()
                    .copied()
                    .map(|parent: ClipRect| intersect_rect(parent.rect, *rect))
                    .unwrap_or(*rect);
                clips.push(ClipRect {
                    rect,
                    radius: *radius,
                });
            }
            RenderCommand::PopClip => {
                clips.pop();
            }
            _ => {}
        }
    }
    if uniforms.is_empty() {
        uniforms.push(screen_uniform(logical_size, &clips));
    }
    uniforms
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Debug)]
struct ShapeVertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl ShapeVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

    fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TextVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

impl TextVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];

    fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

pub struct WgpuBackend {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    scale_factor: f32,
    background: Color,
    shape_pipeline: wgpu::RenderPipeline,
    text_pipeline: wgpu::RenderPipeline,
    screen_buffer: wgpu::Buffer,
    screen_bind_group_layout: wgpu::BindGroupLayout,
    screen_uniform_stride: u64,
    text_bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    text_context: Rc<RefCell<FontSystem>>,
    text_rasterizer: TextRasterizer,
    text_cache: TextResourceCache,
    shape_cache: ShapeResourceCache,
    clipboard: WgpuClipboard,
    debug_info: bool,
    frame_stats: FrameStats,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct TextResourceKey {
    text: String,
    font_size: u32,
    color: [u32; 4],
    rect: [u32; 4],
    offset: [u32; 2],
    wrap: u8,
    scale_factor: u32,
}

struct CachedTextResource {
    _texture: wgpu::Texture,
    _view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
}

#[derive(Default)]
struct TextResourceCache {
    entries: HashMap<TextResourceKey, CachedTextResource>,
}

struct CachedShapeResource {
    fingerprint: u64,
    buffer: wgpu::Buffer,
    vertex_count: u32,
}

#[derive(Default)]
struct ShapeResourceCache {
    entries: Vec<Option<CachedShapeResource>>,
}

impl ShapeResourceCache {
    fn prepare<'a>(
        &'a mut self,
        commands: &[RenderCommand],
        device: &wgpu::Device,
    ) -> Vec<Option<(&'a wgpu::Buffer, u32)>> {
        if self.entries.len() < commands.len() {
            self.entries.resize_with(commands.len(), || None);
        }

        for (index, command) in commands.iter().enumerate() {
            let Some(vertices) = shape_vertices(command) else {
                self.entries[index] = None;
                continue;
            };
            let fingerprint = shape_fingerprint(&vertices);
            let cached = self.entries[index]
                .as_ref()
                .is_some_and(|resource| resource.fingerprint == fingerprint);
            if cached {
                continue;
            }

            let vertex_count = vertices.len() as u32;
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("karu-wgpu-shape-vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            self.entries[index] = Some(CachedShapeResource {
                fingerprint,
                buffer,
                vertex_count,
            });
        }

        commands
            .iter()
            .enumerate()
            .map(|(index, command)| {
                shape_vertices(command)?;
                self.entries[index]
                    .as_ref()
                    .map(|resource| (&resource.buffer, resource.vertex_count))
            })
            .collect()
    }
}

impl TextResourceCache {
    const CAPACITY: usize = 512;
}

struct FrameStats {
    sample_started: Instant,
    frames: u32,
    fps: f32,
}

impl FrameStats {
    fn new() -> Self {
        Self {
            sample_started: Instant::now(),
            frames: 0,
            fps: 0.0,
        }
    }

    fn tick(&mut self) {
        self.frames = self.frames.saturating_add(1);
        let elapsed = self.sample_started.elapsed();
        if elapsed >= Duration::from_millis(250) {
            self.fps = self.frames as f32 / elapsed.as_secs_f32().max(f32::EPSILON);
            self.frames = 0;
            self.sample_started = Instant::now();
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WgpuClipboard;

impl karu::Clipboard for WgpuClipboard {
    fn get_text(&mut self) -> Result<Option<String>, karu::ClipboardError> {
        let mut clipboard = SystemClipboard::new()
            .map_err(|error| karu::ClipboardError::Platform(error.to_string()))?;
        clipboard
            .get_text()
            .map(Some)
            .map_err(|error| karu::ClipboardError::Platform(error.to_string()))
    }

    fn set_text(&mut self, text: &str) -> Result<(), karu::ClipboardError> {
        let mut clipboard = SystemClipboard::new()
            .map_err(|error| karu::ClipboardError::Platform(error.to_string()))?;
        clipboard
            .set_text(text.to_string())
            .map_err(|error| karu::ClipboardError::Platform(error.to_string()))
    }
}

impl WgpuBackend {
    pub async fn new(
        window: Arc<Window>,
        background: Color,
        family: Option<String>,
        debug_info: bool,
    ) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(window.clone())
            .expect("failed to create wgpu surface");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("no compatible wgpu adapter found");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("karu-wgpu-device"),
                ..Default::default()
            })
            .await
            .expect("failed to create wgpu device");
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("adapter cannot configure the wgpu surface");
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("karu-wgpu-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let scale_factor = window.scale_factor() as f32;
        let screen_uniform_stride = aligned_uniform_stride(
            std::mem::size_of::<ScreenUniform>() as u64,
            device.limits().min_uniform_buffer_offset_alignment as u64,
        );
        let screen_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("karu-wgpu-screen"),
            contents: bytemuck::bytes_of(&screen_uniform(
                logical_surface_size(size, scale_factor),
                &[],
            )),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let screen_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("karu-wgpu-screen-layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let text_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("karu-wgpu-text-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let shape_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("karu-wgpu-shape-pipeline-layout"),
            bind_group_layouts: &[Some(&screen_bind_group_layout)],
            immediate_size: 0,
        });
        let text_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("karu-wgpu-text-pipeline-layout"),
            bind_group_layouts: &[
                Some(&screen_bind_group_layout),
                Some(&text_bind_group_layout),
            ],
            immediate_size: 0,
        });
        let blend = Some(wgpu::BlendState::ALPHA_BLENDING);
        let shape_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("karu-wgpu-shape-pipeline"),
            layout: Some(&shape_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("shape_vs"),
                compilation_options: Default::default(),
                buffers: &[ShapeVertex::layout()],
            },
            primitive: primitive_state(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("shape_fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let text_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("karu-wgpu-text-pipeline"),
            layout: Some(&text_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("text_vs"),
                compilation_options: Default::default(),
                buffers: &[TextVertex::layout()],
            },
            primitive: primitive_state(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("text_fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("karu-wgpu-text-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let text_context = Rc::new(RefCell::new(FontSystem::new()));
        let text_rasterizer = TextRasterizer::new(text_context.clone(), family);
        Self {
            surface,
            device,
            queue,
            config,
            scale_factor,
            background,
            shape_pipeline,
            text_pipeline,
            screen_buffer,
            screen_bind_group_layout,
            screen_uniform_stride,
            text_bind_group_layout,
            sampler,
            text_context,
            text_rasterizer,
            text_cache: TextResourceCache::default(),
            shape_cache: ShapeResourceCache::default(),
            clipboard: WgpuClipboard,
            debug_info,
            frame_stats: FrameStats::new(),
        }
    }

    fn text_context(&self) -> Rc<RefCell<FontSystem>> {
        self.text_context.clone()
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        self.resize_with_scale(size, self.scale_factor);
    }

    pub fn resize_with_scale(&mut self, size: PhysicalSize<u32>, scale_factor: f32) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        let scale_factor = scale_factor.max(0.001);
        let surface_changed = self.config.width != size.width
            || self.config.height != size.height
            || self.scale_factor != scale_factor;
        self.scale_factor = scale_factor;
        self.config.width = size.width;
        self.config.height = size.height;
        self.queue.write_buffer(
            &self.screen_buffer,
            0,
            bytemuck::bytes_of(&screen_uniform(
                logical_surface_size(size, self.scale_factor),
                &[],
            )),
        );
        if surface_changed {
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn render_commands(&mut self, commands: &[RenderCommand]) {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Lost
            | wgpu::CurrentSurfaceTexture::Validation => return,
        };
        if self.debug_info {
            self.frame_stats.tick();
        }
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("karu-wgpu-frame-encoder"),
            });
        let scale = self.scale_factor.max(0.001);
        let surface_width = self.config.width;
        let surface_height = self.config.height;
        let device = &self.device;
        let queue = &self.queue;
        let logical_size =
            logical_surface_size(PhysicalSize::new(surface_width, surface_height), scale);
        let clip_uniforms = clip_uniforms_for_commands(commands, logical_size);
        let uniform_count = clip_uniforms.len().max(1) as u64;
        let frame_screen_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("karu-wgpu-frame-screen"),
            size: self.screen_uniform_stride * uniform_count,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        for (index, uniform) in clip_uniforms.iter().enumerate() {
            queue.write_buffer(
                &frame_screen_buffer,
                index as u64 * self.screen_uniform_stride,
                bytemuck::bytes_of(uniform),
            );
        }
        let frame_screen_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("karu-wgpu-frame-screen-bind-group"),
            layout: &self.screen_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &frame_screen_buffer,
                    offset: 0,
                    size: NonZeroU64::new(std::mem::size_of::<ScreenUniform>() as u64),
                }),
            }],
        });
        let shape_buffers = self.shape_cache.prepare(commands, device);
        let debug_info_buffer = self.debug_info.then(|| {
            let vertices = rect_vertices(
                Rect::new(8.0, 8.0, 86.0, 28.0),
                Color::rgba(0.04, 0.06, 0.10, 0.88),
            );
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("karu-wgpu-debug-info-background"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let shape_pipeline = &self.shape_pipeline;
        let screen_bind_group = &frame_screen_bind_group;
        let text_pipeline = &self.text_pipeline;
        let text_bind_group_layout = &self.text_bind_group_layout;
        let sampler = &self.sampler;
        let text_rasterizer = &mut self.text_rasterizer;
        let text_cache = &mut self.text_cache;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("karu-wgpu-render-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(to_wgpu_color(self.background)),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let mut clips = Vec::<ClipRect>::new();
        for (command_index, command) in commands.iter().enumerate() {
            let screen_offset = (command_index as u64 * self.screen_uniform_stride) as u32;
            match command {
                RenderCommand::FillRect { .. }
                | RenderCommand::FillBrush { .. }
                | RenderCommand::StrokeRect { .. }
                | RenderCommand::DrawSelection { .. }
                | RenderCommand::DrawCursor { .. }
                | RenderCommand::DrawComposition { .. } => {
                    if let Some((buffer, vertex_count)) = shape_buffers[command_index] {
                        draw_shape(
                            &mut pass,
                            buffer,
                            vertex_count,
                            shape_pipeline,
                            screen_bind_group,
                            screen_offset,
                        );
                    }
                }
                RenderCommand::DrawText {
                    rect,
                    text,
                    style,
                    wrap,
                    offset,
                    ..
                } => {
                    text_cache.draw(
                        &mut pass,
                        device,
                        queue,
                        text_pipeline,
                        screen_bind_group,
                        screen_offset,
                        text_bind_group_layout,
                        sampler,
                        text_rasterizer,
                        *rect,
                        text,
                        style.font_size,
                        style.color,
                        *wrap,
                        *offset,
                        scale,
                    );
                }
                RenderCommand::PushClip { rect, radius, .. } => {
                    let logical = clips
                        .last()
                        .copied()
                        .map(|parent| intersect_rect(parent.rect, *rect))
                        .unwrap_or(*rect);
                    clips.push(ClipRect {
                        rect: logical,
                        radius: *radius,
                    });
                    let (x, y, width, height) =
                        scissor_rect(logical, scale, surface_width, surface_height);
                    pass.set_scissor_rect(x, y, width, height);
                }
                RenderCommand::PopClip => {
                    clips.pop();
                    if let Some(rect) = clips.last() {
                        let (x, y, width, height) =
                            scissor_rect(rect.rect, scale, surface_width, surface_height);
                        pass.set_scissor_rect(x, y, width, height);
                    } else {
                        pass.set_scissor_rect(0, 0, surface_width, surface_height);
                    }
                }
                RenderCommand::DrawImage { .. } => {}
            }
        }
        if self.debug_info {
            pass.set_scissor_rect(0, 0, surface_width, surface_height);
            draw_debug_info(
                &mut pass,
                device,
                queue,
                shape_pipeline,
                text_pipeline,
                screen_bind_group,
                0,
                text_bind_group_layout,
                sampler,
                text_cache,
                text_rasterizer,
                debug_info_buffer
                    .as_ref()
                    .expect("debug info buffer exists"),
                self.frame_stats.fps,
                scale,
            );
        }
        drop(pass);
        queue.submit(Some(encoder.finish()));
        frame.present();
    }
}

impl TextResourceCache {
    fn draw<'a>(
        &mut self,
        pass: &mut wgpu::RenderPass<'a>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        text_pipeline: &wgpu::RenderPipeline,
        screen_bind_group: &wgpu::BindGroup,
        screen_offset: u32,
        text_bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        text_rasterizer: &mut TextRasterizer,
        rect: Rect,
        text: &str,
        font_size: f32,
        color: Color,
        wrap: TextWrap,
        offset: Offset,
        scale_factor: f32,
    ) {
        let scale_factor = normalized_scale(scale_factor);
        let key = TextResourceKey {
            text: text.to_string(),
            font_size: font_size.to_bits(),
            color: [color.red, color.green, color.blue, color.alpha].map(f32::to_bits),
            rect: [
                rect.size.width,
                rect.size.height,
                rect.origin.x,
                rect.origin.y,
            ]
            .map(f32::to_bits),
            offset: [offset.x, offset.y].map(f32::to_bits),
            wrap: match wrap {
                TextWrap::NoWrap => 0,
                TextWrap::Word => 1,
                TextWrap::Character => 2,
            },
            scale_factor: scale_factor.to_bits(),
        };

        if !self.entries.contains_key(&key) {
            if self.entries.len() >= Self::CAPACITY {
                self.entries.clear();
            }
            let Some((width, height, pixels)) = text_rasterizer.rasterize(
                &rect,
                text,
                font_size,
                color,
                wrap,
                offset,
                scale_factor,
            ) else {
                return;
            };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("karu-wgpu-texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("karu-wgpu-text-bind-group"),
                layout: text_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            });
            let x = rect.origin.x;
            let y = rect.origin.y;
            let vertices = [
                TextVertex {
                    position: [x, y],
                    uv: [0.0, 0.0],
                },
                TextVertex {
                    position: [x + rect.size.width, y],
                    uv: [1.0, 0.0],
                },
                TextVertex {
                    position: [x + rect.size.width, y + rect.size.height],
                    uv: [1.0, 1.0],
                },
                TextVertex {
                    position: [x, y],
                    uv: [0.0, 0.0],
                },
                TextVertex {
                    position: [x + rect.size.width, y + rect.size.height],
                    uv: [1.0, 1.0],
                },
                TextVertex {
                    position: [x, y + rect.size.height],
                    uv: [0.0, 1.0],
                },
            ];
            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("karu-wgpu-text-vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            self.entries.insert(
                key.clone(),
                CachedTextResource {
                    _texture: texture,
                    _view: view,
                    bind_group,
                    vertex_buffer,
                },
            );
        }

        let Some(resource) = self.entries.get(&key) else {
            return;
        };
        pass.set_pipeline(text_pipeline);
        pass.set_bind_group(0, screen_bind_group, &[screen_offset]);
        pass.set_bind_group(1, &resource.bind_group, &[]);
        pass.set_vertex_buffer(0, resource.vertex_buffer.slice(..));
        pass.draw(0..6, 0..1);
    }
}

impl RenderBackend for WgpuBackend {
    type Output = ();
    type Error = Infallible;
    type Clipboard = WgpuClipboard;

    fn render(
        &mut self,
        _tree: &karu::RenderTree,
        commands: &[RenderCommand],
    ) -> Result<Self::Output, Self::Error> {
        self.render_commands(commands);
        Ok(())
    }

    fn clipboard(&mut self) -> &mut Self::Clipboard {
        &mut self.clipboard
    }
}

fn primitive_state() -> wgpu::PrimitiveState {
    wgpu::PrimitiveState {
        topology: wgpu::PrimitiveTopology::TriangleList,
        strip_index_format: None,
        front_face: wgpu::FrontFace::Ccw,
        cull_mode: None,
        unclipped_depth: false,
        polygon_mode: wgpu::PolygonMode::Fill,
        conservative: false,
    }
}

fn draw_shape<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    shape_buffer: &'a wgpu::Buffer,
    vertex_count: u32,
    shape_pipeline: &wgpu::RenderPipeline,
    screen_bind_group: &wgpu::BindGroup,
    screen_offset: u32,
) {
    pass.set_pipeline(shape_pipeline);
    pass.set_bind_group(0, screen_bind_group, &[screen_offset]);
    pass.set_vertex_buffer(0, shape_buffer.slice(..));
    pass.draw(0..vertex_count, 0..1);
}

fn draw_debug_info<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    shape_pipeline: &wgpu::RenderPipeline,
    text_pipeline: &wgpu::RenderPipeline,
    screen_bind_group: &wgpu::BindGroup,
    screen_offset: u32,
    text_bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    text_cache: &mut TextResourceCache,
    text_rasterizer: &mut TextRasterizer,
    background_buffer: &'a wgpu::Buffer,
    fps: f32,
    scale_factor: f32,
) {
    let rect = Rect::new(8.0, 8.0, 86.0, 28.0);
    draw_shape(
        pass,
        background_buffer,
        6,
        shape_pipeline,
        screen_bind_group,
        screen_offset,
    );

    let label = format!("FPS {:.0}", fps);
    text_cache.draw(
        pass,
        device,
        queue,
        text_pipeline,
        screen_bind_group,
        screen_offset,
        text_bind_group_layout,
        sampler,
        text_rasterizer,
        rect,
        &label,
        13.0,
        Color::WHITE,
        TextWrap::NoWrap,
        Offset::new(16.0, 14.0),
        scale_factor,
    );
}

fn shape_vertices(command: &RenderCommand) -> Option<Vec<ShapeVertex>> {
    let vertices = match command {
        RenderCommand::FillRect {
            rect,
            color,
            radius,
            ..
        } => rounded_rect_vertices(*rect, &Brush::Solid(*color), *radius),
        RenderCommand::FillBrush {
            rect,
            brush,
            radius,
            ..
        } => rounded_rect_vertices(*rect, brush, *radius),
        RenderCommand::StrokeRect {
            rect,
            brush,
            width,
            radius,
            ..
        } => stroke_vertices(*rect, brush, *width, *radius),
        RenderCommand::DrawSelection { rect, color, .. }
        | RenderCommand::DrawCursor { rect, color, .. }
        | RenderCommand::DrawComposition { rect, color, .. } => rect_vertices(*rect, *color),
        _ => return None,
    };
    (!vertices.is_empty()).then_some(vertices)
}

fn shape_fingerprint(vertices: &[ShapeVertex]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for vertex in vertices {
        for value in vertex.position {
            value.to_bits().hash(&mut hasher);
        }
        for value in vertex.color {
            value.to_bits().hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn rect_vertices(rect: Rect, color: Color) -> Vec<ShapeVertex> {
    let c = rgba(color);
    let x = rect.origin.x;
    let y = rect.origin.y;
    let r = x + rect.size.width;
    let b = y + rect.size.height;
    vec![
        vertex(x, y, c),
        vertex(r, y, c),
        vertex(r, b, c),
        vertex(x, y, c),
        vertex(r, b, c),
        vertex(x, b, c),
    ]
}

fn brush_vertices(rect: Rect, brush: &Brush) -> Vec<ShapeVertex> {
    match brush {
        Brush::Solid(color) => rect_vertices(rect, *color),
        Brush::LinearGradient { start, end, stops } => {
            let color = |x: f32, y: f32| {
                let dx = end.x - start.x;
                let dy = end.y - start.y;
                let denominator = dx * dx + dy * dy;
                let t = if denominator > 0.0 {
                    ((x - start.x) * dx + (y - start.y) * dy) / denominator
                } else {
                    0.0
                };
                gradient_color(stops, t)
            };
            let x = rect.origin.x;
            let y = rect.origin.y;
            let r = x + rect.size.width;
            let b = y + rect.size.height;
            vec![
                vertex(x, y, rgba(color(x, y))),
                vertex(r, y, rgba(color(r, y))),
                vertex(r, b, rgba(color(r, b))),
                vertex(x, y, rgba(color(x, y))),
                vertex(r, b, rgba(color(r, b))),
                vertex(x, b, rgba(color(x, b))),
            ]
        }
    }
}

fn rounded_rect_vertices(rect: Rect, brush: &Brush, radius: f32) -> Vec<ShapeVertex> {
    if radius <= 0.0 {
        return brush_vertices(rect, brush);
    }
    let points = rounded_points(rect, radius);
    if points.is_empty() {
        return Vec::new();
    }
    let center = [
        rect.origin.x + rect.size.width * 0.5,
        rect.origin.y + rect.size.height * 0.5,
    ];
    let mut vertices = Vec::with_capacity(points.len() * 3);
    for index in 0..points.len() {
        let next = (index + 1) % points.len();
        vertices.extend([
            brush_vertex(brush, center[0], center[1]),
            brush_vertex(brush, points[index][0], points[index][1]),
            brush_vertex(brush, points[next][0], points[next][1]),
        ]);
    }
    vertices
}

fn rounded_points(rect: Rect, radius: f32) -> Vec<[f32; 2]> {
    const SEGMENTS: usize = 8;
    let radius = radius
        .max(0.0)
        .min(rect.size.width.min(rect.size.height) * 0.5);
    if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return Vec::new();
    }
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

fn brush_vertex(brush: &Brush, x: f32, y: f32) -> ShapeVertex {
    let color = match brush {
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
    };
    vertex(x, y, rgba(color))
}

fn stroke_vertices(rect: Rect, brush: &Brush, width: f32, radius: f32) -> Vec<ShapeVertex> {
    let x = rect.origin.x;
    let y = rect.origin.y;
    let r = x + rect.size.width;
    let b = y + rect.size.height;
    let min_dimension = rect.size.width.min(rect.size.height);
    let width = width.max(0.0).min(min_dimension * 0.5);
    if width == 0.0 || min_dimension <= 0.0 {
        return Vec::new();
    }
    let radius = radius.max(0.0).min(min_dimension * 0.5);
    if radius == 0.0 {
        return vec![
            brush_vertex(brush, x, y),
            brush_vertex(brush, r, y),
            brush_vertex(brush, r, y + width),
            brush_vertex(brush, x, y),
            brush_vertex(brush, r, y + width),
            brush_vertex(brush, x, y + width),
            brush_vertex(brush, x, b - width),
            brush_vertex(brush, r, b - width),
            brush_vertex(brush, r, b),
            brush_vertex(brush, x, b - width),
            brush_vertex(brush, r, b),
            brush_vertex(brush, x, b),
            brush_vertex(brush, x, y + width),
            brush_vertex(brush, x + width, y + width),
            brush_vertex(brush, x + width, b - width),
            brush_vertex(brush, x, y + width),
            brush_vertex(brush, x + width, b - width),
            brush_vertex(brush, x, b - width),
            brush_vertex(brush, r - width, y + width),
            brush_vertex(brush, r, y + width),
            brush_vertex(brush, r, b - width),
            brush_vertex(brush, r - width, y + width),
            brush_vertex(brush, r, b - width),
            brush_vertex(brush, r - width, b - width),
        ];
    }
    let mut vertices = Vec::new();
    let inner = (radius - width).max(0.0);

    extend_quad(
        &mut vertices,
        [x + radius, y],
        [r - radius, y],
        [r - inner, y + width],
        [x + inner, y + width],
        brush,
    );
    extend_quad(
        &mut vertices,
        [r, y + radius],
        [r, b - radius],
        [r - width, b - inner],
        [r - width, y + inner],
        brush,
    );
    extend_quad(
        &mut vertices,
        [x + radius, b],
        [r - radius, b],
        [r - inner, b - width],
        [x + inner, b - width],
        brush,
    );
    extend_quad(
        &mut vertices,
        [x, b - radius],
        [x, y + radius],
        [x + width, y + inner],
        [x + width, b - inner],
        brush,
    );

    for (cx, cy, start) in [
        (x + radius, y + radius, std::f32::consts::PI),
        (r - radius, y + radius, 1.5 * std::f32::consts::PI),
        (r - radius, b - radius, 0.0),
        (x + radius, b - radius, 0.5 * std::f32::consts::PI),
    ] {
        for step in 0..8 {
            let a0 = start + step as f32 * std::f32::consts::FRAC_PI_2 / 8.0;
            let a1 = start + (step + 1) as f32 * std::f32::consts::FRAC_PI_2 / 8.0;
            let outer0 = [cx + radius * a0.cos(), cy + radius * a0.sin()];
            let outer1 = [cx + radius * a1.cos(), cy + radius * a1.sin()];
            let inner0 = [cx + inner * a0.cos(), cy + inner * a0.sin()];
            let inner1 = [cx + inner * a1.cos(), cy + inner * a1.sin()];
            vertices.extend([
                brush_vertex(brush, outer0[0], outer0[1]),
                brush_vertex(brush, outer1[0], outer1[1]),
                brush_vertex(brush, inner1[0], inner1[1]),
                brush_vertex(brush, outer0[0], outer0[1]),
                brush_vertex(brush, inner1[0], inner1[1]),
                brush_vertex(brush, inner0[0], inner0[1]),
            ]);
        }
    }
    vertices
}

fn extend_quad(
    vertices: &mut Vec<ShapeVertex>,
    outer0: [f32; 2],
    outer1: [f32; 2],
    inner1: [f32; 2],
    inner0: [f32; 2],
    brush: &Brush,
) {
    vertices.extend([
        brush_vertex(brush, outer0[0], outer0[1]),
        brush_vertex(brush, outer1[0], outer1[1]),
        brush_vertex(brush, inner1[0], inner1[1]),
        brush_vertex(brush, outer0[0], outer0[1]),
        brush_vertex(brush, inner1[0], inner1[1]),
        brush_vertex(brush, inner0[0], inner0[1]),
    ]);
}

fn vertex(x: f32, y: f32, color: [f32; 4]) -> ShapeVertex {
    ShapeVertex {
        position: [x, y],
        color,
    }
}

fn rgba(color: Color) -> [f32; 4] {
    [
        color.red.clamp(0.0, 1.0),
        color.green.clamp(0.0, 1.0),
        color.blue.clamp(0.0, 1.0),
        color.alpha.clamp(0.0, 1.0),
    ]
}

fn to_wgpu_color(color: Color) -> wgpu::Color {
    wgpu::Color {
        r: color.red.clamp(0.0, 1.0) as f64,
        g: color.green.clamp(0.0, 1.0) as f64,
        b: color.blue.clamp(0.0, 1.0) as f64,
        a: color.alpha.clamp(0.0, 1.0) as f64,
    }
}

fn logical_surface_size(size: PhysicalSize<u32>, scale_factor: f32) -> [f32; 2] {
    let scale_factor = normalized_scale(scale_factor);
    [
        size.width as f32 / scale_factor,
        size.height as f32 / scale_factor,
    ]
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
        if position <= pair[1].position {
            let span = (pair[1].position - pair[0].position).max(f32::EPSILON);
            return pair[0]
                .color
                .lerp(pair[1].color, (position - pair[0].position) / span);
        }
    }
    stops.last().map_or(first.color, |stop| stop.color)
}

fn intersect_rect(left: Rect, right: Rect) -> Rect {
    let x = left.origin.x.max(right.origin.x);
    let y = left.origin.y.max(right.origin.y);
    let right_edge = (left.origin.x + left.size.width).min(right.origin.x + right.size.width);
    let bottom = (left.origin.y + left.size.height).min(right.origin.y + right.size.height);
    Rect::new(x, y, (right_edge - x).max(0.0), (bottom - y).max(0.0))
}

fn scissor_rect(
    rect: Rect,
    scale: f32,
    surface_width: u32,
    surface_height: u32,
) -> (u32, u32, u32, u32) {
    let scale = normalized_scale(scale);
    let viewport = Rect::new(
        0.0,
        0.0,
        surface_width as f32 / scale,
        surface_height as f32 / scale,
    );
    let rect = intersect_rect(viewport, rect);
    let x = (rect.origin.x * scale)
        .floor()
        .clamp(0.0, surface_width as f32) as u32;
    let y = (rect.origin.y * scale)
        .floor()
        .clamp(0.0, surface_height as f32) as u32;
    let right = ((rect.origin.x + rect.size.width) * scale)
        .ceil()
        .clamp(x as f32, surface_width as f32) as u32;
    let bottom = ((rect.origin.y + rect.size.height) * scale)
        .ceil()
        .clamp(y as f32, surface_height as f32) as u32;
    (x, y, right.saturating_sub(x), bottom.saturating_sub(y))
}

struct TextRasterizer {
    context: Rc<RefCell<FontSystem>>,
    swash_cache: SwashCache,
    family: Option<String>,
}

impl TextRasterizer {
    fn new(context: Rc<RefCell<FontSystem>>, family: Option<String>) -> Self {
        Self {
            context,
            swash_cache: SwashCache::new(),
            family,
        }
    }

    fn rasterize(
        &mut self,
        rect: &Rect,
        text: &str,
        font_size: f32,
        color: Color,
        wrap: TextWrap,
        offset: Offset,
        scale_factor: f32,
    ) -> Option<(u32, u32, Vec<u8>)> {
        let scale_factor = normalized_scale(scale_factor);
        let width = physical_text_extent(rect.size.width, scale_factor);
        let height = physical_text_extent(rect.size.height, scale_factor);
        let mut pixels = vec![0; width as usize * height as usize * 4];
        let mut context = self.context.borrow_mut();
        let mut buffer = make_buffer(
            &mut context,
            self.family.as_deref(),
            text,
            font_size * scale_factor,
            width as f32,
            height as f32,
            wrap,
        );
        let local_x = (offset.x - rect.origin.x) * scale_factor;
        let local_y = (offset.y - rect.origin.y) * scale_factor;
        buffer.draw(
            &mut context,
            &mut self.swash_cache,
            cosmic_color(color),
            |x, y, w, h, pixel| {
                let x0 = local_x.round() as i32 + x;
                let y0 = local_y.round() as i32 + y;
                for py in 0..h as i32 {
                    for px in 0..w as i32 {
                        let tx = x0 + px;
                        let ty = y0 + py;
                        if tx < 0 || ty < 0 || tx >= width as i32 || ty >= height as i32 {
                            continue;
                        }
                        let index = ((ty as u32 * width + tx as u32) * 4) as usize;
                        pixels[index] = pixel.r();
                        pixels[index + 1] = pixel.g();
                        pixels[index + 2] = pixel.b();
                        pixels[index + 3] = pixel.a();
                    }
                }
            },
        );
        Some((width, height, pixels))
    }
}

fn normalized_scale(scale_factor: f32) -> f32 {
    if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    }
}

fn aligned_uniform_stride(size: u64, alignment: u64) -> u64 {
    let alignment = alignment.max(1);
    size.div_ceil(alignment) * alignment
}

fn physical_text_extent(logical_size: f32, scale_factor: f32) -> u32 {
    (logical_size.max(1.0) * normalized_scale(scale_factor))
        .ceil()
        .max(1.0) as u32
}

pub struct CosmicTextLayout {
    context: Rc<RefCell<FontSystem>>,
    family: Option<String>,
    geometry_cache: HashMap<TextGeometryKey, CosmicGeometry>,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct TextGeometryKey {
    text: String,
    font_size: u32,
    max_width: u32,
    wrap: u8,
}

impl CosmicTextLayout {
    pub fn new(family: Option<String>) -> Self {
        Self::with_context(Rc::new(RefCell::new(FontSystem::new())), family)
    }

    fn with_context(context: Rc<RefCell<FontSystem>>, family: Option<String>) -> Self {
        Self {
            context,
            family,
            geometry_cache: HashMap::new(),
        }
    }

    fn geometry(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
    ) -> CosmicGeometry {
        let key = TextGeometryKey {
            text: text.to_string(),
            font_size: font_size.to_bits(),
            max_width: max_width.to_bits(),
            wrap: text_wrap_key(wrap),
        };
        if let Some(geometry) = self.geometry_cache.get(&key) {
            return geometry.clone();
        }

        let geometry = self.build_geometry(text, font_size, max_width, wrap);
        if self.geometry_cache.len() >= 512 {
            self.geometry_cache.clear();
        }
        self.geometry_cache.insert(key, geometry.clone());
        geometry
    }

    fn build_geometry(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
    ) -> CosmicGeometry {
        let mut context = self.context.borrow_mut();
        let width = max_width.max(1.0);
        let buffer = make_buffer(
            &mut context,
            self.family.as_deref(),
            text,
            font_size,
            width,
            f32::INFINITY,
            wrap,
        );
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
                position: line.origin,
                line: line_index,
                height: line.height,
                affinity: CaretAffinity::After,
            });
            for glyph in run.glyphs {
                let cluster = &run.text[glyph.start..glyph.end];
                let graphemes = cluster.grapheme_indices(true).collect::<Vec<_>>();
                let glyph_width = glyph.w / graphemes.len().max(1) as f32;
                for (index, (byte_index, grapheme)) in graphemes.iter().enumerate() {
                    let x = glyph.x + index as f32 * glyph_width;
                    let start_x = if glyph.level.is_rtl() {
                        x + glyph_width
                    } else {
                        x
                    };
                    let end_x = if glyph.level.is_rtl() {
                        x
                    } else {
                        x + glyph_width
                    };
                    let absolute = base + glyph.start + byte_index + grapheme.len();
                    carets.push(CaretPosition {
                        offset: absolute - grapheme.len(),
                        position: Offset::new(start_x, run.line_top),
                        line: line_index,
                        height: run.line_height,
                        affinity: CaretAffinity::After,
                    });
                    carets.push(CaretPosition {
                        offset: absolute,
                        position: Offset::new(end_x, run.line_top),
                        line: line_index,
                        height: run.line_height,
                        affinity: CaretAffinity::Before,
                    });
                }
            }
        }
        if lines.is_empty() {
            let line_height = font_size.max(1.0) * (20.0 / 14.0);
            lines.push(CosmicLine {
                range: 0..text.len(),
                origin: Offset::ZERO,
                width: 0.0,
                height: line_height,
            });
            carets.push(CaretPosition {
                offset: 0,
                position: Offset::ZERO,
                line: 0,
                height: line_height,
                affinity: CaretAffinity::After,
            });
        }
        let size = Size::new(
            lines.iter().map(|line| line.width).fold(0.0, f32::max),
            lines
                .iter()
                .map(|line| line.origin.y + line.height)
                .fold(0.0, f32::max),
        );
        CosmicGeometry {
            size,
            line_height: font_size.max(1.0) * (20.0 / 14.0),
            lines,
            carets,
        }
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

fn make_buffer(
    font_system: &mut FontSystem,
    family: Option<&str>,
    text: &str,
    font_size: f32,
    width: f32,
    height: f32,
    wrap: TextWrap,
) -> Buffer {
    let mut buffer = Buffer::new(
        font_system,
        Metrics::new(font_size.max(1.0), font_size.max(1.0) * (20.0 / 14.0)),
    );
    buffer.set_size(
        Some(width.max(1.0)),
        height.is_finite().then_some(height.max(1.0)),
    );
    buffer.set_wrap(match wrap {
        TextWrap::NoWrap => Wrap::None,
        TextWrap::Word => Wrap::WordOrGlyph,
        TextWrap::Character => Wrap::Glyph,
    });
    let attrs = family
        .map(|family| Attrs::new().family(cosmic_text::Family::Name(family)))
        .unwrap_or_else(Attrs::new);
    buffer.set_text(text, &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);
    buffer
}

fn cosmic_color(color: Color) -> cosmic_text::Color {
    cosmic_text::Color::rgba(
        (color.red.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.green.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.blue.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

#[derive(Clone)]
struct CosmicGeometry {
    size: Size,
    line_height: f32,
    lines: Vec<CosmicLine>,
    carets: Vec<CaretPosition>,
}

fn text_wrap_key(wrap: TextWrap) -> u8 {
    match wrap {
        TextWrap::NoWrap => 0,
        TextWrap::Word => 1,
        TextWrap::Character => 2,
    }
}
#[derive(Clone)]
struct CosmicLine {
    range: Range<usize>,
    origin: Offset,
    width: f32,
    height: f32,
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
        let line = self
            .lines
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| {
                distance_to_line(left, position.y)
                    .partial_cmp(&distance_to_line(right, position.y))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(index, _)| index)
            .unwrap_or(0);
        let mut carets = self
            .carets
            .iter()
            .filter(|caret| caret.line == line)
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
            return self.lines.get(line).map_or(0, |line| line.range.start);
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
        let start = range.start.min(range.end);
        let end = range.start.max(range.end);
        if start == end {
            return Vec::new();
        }
        self.lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| {
                if !(start < line.range.end && end > line.range.start) {
                    return None;
                }
                let line_start = if start <= line.range.start {
                    0.0
                } else {
                    self.caret_on_line(index, start).position.x
                };
                let line_end = if end >= line.range.end {
                    line.width
                } else {
                    self.caret_on_line(index, end).position.x
                };
                (line_end > line_start).then(|| {
                    Rect::new(
                        line_start,
                        line.origin.y,
                        line_end - line_start,
                        line.height,
                    )
                })
            })
            .collect()
    }

    fn caret_on_line(&self, line: usize, offset: usize) -> CaretPosition {
        self.carets
            .iter()
            .filter(|caret| caret.line == line && caret.offset == offset)
            .find(|caret| caret.affinity == CaretAffinity::After)
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

fn distance_to_line(line: &CosmicLine, y: f32) -> f32 {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app(debug_info: bool) -> WgpuApp {
        WgpuApp::new(Box::new(|| {}), AppConfig::default(), None, debug_info)
    }

    #[test]
    fn debug_info_is_opt_in() {
        assert!(!ConfiguredWgpu::default().debug_info);
        assert!(Wgpu.default_system_font().enable_debug_info().debug_info);
        assert!(Wgpu.enable_debug_info().debug_info);
    }

    #[test]
    fn redraw_requests_are_coalesced() {
        let mut app = test_app(false);

        app.request_redraw();
        app.request_redraw();

        assert!(app.redraw_pending);
    }

    #[test]
    fn debug_info_does_not_request_an_idle_redraw() {
        let mut app = test_app(true);

        app.request_redraw_if_needed();

        assert!(!app.redraw_pending);
    }

    #[test]
    fn gradient_clamps_and_interpolates() {
        let stops = vec![
            GradientStop {
                position: 0.0,
                color: Color::BLACK,
            },
            GradientStop {
                position: 1.0,
                color: Color::WHITE,
            },
        ];
        assert_eq!(gradient_color(&stops, -1.0), Color::BLACK);
        assert_eq!(gradient_color(&stops, 2.0), Color::WHITE);
        assert_eq!(gradient_color(&stops, 0.5), Color::rgba(0.5, 0.5, 0.5, 1.0));
    }

    #[test]
    fn nested_rectangles_intersect() {
        let result = intersect_rect(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Rect::new(20.0, 30.0, 100.0, 100.0),
        );
        assert_eq!(result, Rect::new(20.0, 30.0, 80.0, 70.0));
    }

    #[test]
    fn scissor_rect_is_contained_in_physical_surface() {
        assert_eq!(
            scissor_rect(Rect::new(800.0, 0.0, 160.0, 72.0), 1.75, 800, 600),
            (800, 0, 0, 126),
        );
        assert_eq!(
            scissor_rect(Rect::new(-20.0, -10.0, 100.0, 100.0), 2.0, 800, 600),
            (0, 0, 160, 180),
        );
        let (x, y, width, height) =
            scissor_rect(Rect::new(450.0, 300.0, 100.0, 100.0), 2.0, 800, 600);
        assert!(x + width <= 800);
        assert!(y + height <= 600);
    }

    #[test]
    fn logical_surface_size_uses_logical_coordinates() {
        assert_eq!(
            logical_surface_size(PhysicalSize::new(1600, 1200), 2.0),
            [800.0, 600.0]
        );
        assert_eq!(
            logical_surface_size(PhysicalSize::new(800, 600), 0.0),
            [800.0, 600.0]
        );
    }

    #[test]
    fn text_rasterization_uses_physical_pixel_dimensions() {
        assert_eq!(physical_text_extent(100.0, 1.0), 100);
        assert_eq!(physical_text_extent(100.0, 2.0), 200);
        assert_eq!(physical_text_extent(100.0, 1.25), 125);
        assert_eq!(physical_text_extent(10.1, 1.5), 16);
    }

    #[test]
    fn text_rasterization_normalizes_invalid_scale_factors() {
        assert_eq!(normalized_scale(0.0), 1.0);
        assert_eq!(normalized_scale(-1.0), 1.0);
        assert_eq!(normalized_scale(f32::INFINITY), 1.0);
        assert_eq!(normalized_scale(f32::NAN), 1.0);
        assert_eq!(physical_text_extent(0.0, 0.0), 1);
    }

    #[test]
    fn scissor_rect_expands_to_cover_fractional_physical_pixels() {
        assert_eq!(
            scissor_rect(Rect::new(10.25, 20.25, 10.25, 10.25), 2.0, 100, 100),
            (20, 40, 21, 21),
        );
    }

    #[test]
    fn solid_background_generates_a_full_rectangle() {
        let vertices = rect_vertices(Rect::new(10.0, 20.0, 30.0, 40.0), Color::WHITE);
        assert_eq!(vertices.len(), 6);
        assert_eq!(vertices[0].position, [10.0, 20.0]);
        assert_eq!(vertices[2].position, [40.0, 60.0]);
        assert!(vertices.iter().all(|vertex| vertex.color == [1.0; 4]));
    }

    #[test]
    fn rounded_background_generates_corner_geometry() {
        let vertices = rounded_rect_vertices(
            Rect::new(0.0, 0.0, 100.0, 40.0),
            &Brush::Solid(Color::WHITE),
            8.0,
        );

        assert_eq!(vertices.len(), 96);
        assert!(vertices.iter().all(|vertex| {
            vertex.position[0] >= 0.0
                && vertex.position[0] <= 100.0
                && vertex.position[1] >= 0.0
                && vertex.position[1] <= 40.0
        }));
    }

    #[test]
    fn square_stroke_contains_all_four_edges() {
        let brush = Brush::Solid(Color::BLACK);
        let vertices = stroke_vertices(Rect::new(0.0, 0.0, 100.0, 40.0), &brush, 1.0, 0.0);

        assert_eq!(vertices.len(), 24);
        assert!(vertices.iter().any(|vertex| vertex.position == [0.0, 0.0]));
        assert!(
            vertices
                .iter()
                .any(|vertex| vertex.position == [100.0, 0.0])
        );
        assert!(vertices.iter().any(|vertex| vertex.position == [0.0, 40.0]));
        assert!(
            vertices
                .iter()
                .any(|vertex| vertex.position == [100.0, 40.0])
        );
    }

    #[test]
    fn rounded_stroke_contains_straight_edges_and_corner_arcs() {
        let brush = Brush::Solid(Color::BLACK);
        let vertices = stroke_vertices(Rect::new(0.0, 0.0, 100.0, 40.0), &brush, 1.0, 4.0);

        assert_eq!(vertices.len(), 216);
        assert!(vertices.iter().any(|vertex| vertex.position == [4.0, 0.0]));
        assert!(vertices.iter().any(|vertex| vertex.position == [96.0, 0.0]));
        assert!(vertices.iter().any(|vertex| vertex.position == [0.0, 4.0]));
        assert!(vertices.iter().any(|vertex| vertex.position == [0.0, 36.0]));
        assert!(vertices.iter().all(|vertex| vertex.position[0] >= 0.0
            && vertex.position[0] <= 100.0
            && vertex.position[1] >= 0.0
            && vertex.position[1] <= 40.0));
    }

    #[test]
    fn gradient_stroke_interpolates_at_each_vertex() {
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
        let vertices = stroke_vertices(Rect::new(0.0, 0.0, 100.0, 40.0), &brush, 1.0, 4.0);

        assert!(
            vertices
                .iter()
                .any(|vertex| vertex.color == [0.0, 0.0, 0.0, 1.0])
        );
        assert!(
            vertices
                .iter()
                .any(|vertex| vertex.color == [1.0, 1.0, 1.0, 1.0])
        );
    }

    #[test]
    fn uniform_stride_respects_device_alignment() {
        assert_eq!(aligned_uniform_stride(176, 256), 256);
        assert_eq!(aligned_uniform_stride(176, 0), 176);
    }

    #[test]
    fn zero_width_or_empty_rect_produces_no_stroke_geometry() {
        let brush = Brush::Solid(Color::BLACK);
        assert!(stroke_vertices(Rect::new(0.0, 0.0, 20.0, 20.0), &brush, 0.0, 4.0).is_empty());
        assert!(stroke_vertices(Rect::new(0.0, 0.0, 0.0, 20.0), &brush, 1.0, 4.0).is_empty());
    }

    #[test]
    fn shape_commands_keep_independent_geometry_and_colors() {
        let first = RenderCommand::FillRect {
            node: karu::NodeId(1),
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            color: Color::rgb(1.0, 0.0, 0.0),
            radius: 0.0,
        };
        let second = RenderCommand::FillRect {
            node: karu::NodeId(2),
            rect: Rect::new(20.0, 30.0, 40.0, 50.0),
            color: Color::rgb(0.0, 1.0, 0.0),
            radius: 0.0,
        };
        let first_vertices = shape_vertices(&first).expect("first shape has vertices");
        let second_vertices = shape_vertices(&second).expect("second shape has vertices");

        assert_eq!(first_vertices[0].position, [0.0, 0.0]);
        assert_eq!(first_vertices[0].color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(second_vertices[0].position, [20.0, 30.0]);
        assert_eq!(second_vertices[0].color, [0.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn gradient_background_generates_interpolated_corner_colors() {
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
        let vertices = brush_vertices(Rect::new(0.0, 0.0, 100.0, 20.0), &brush);
        assert_eq!(vertices.len(), 6);
        assert_eq!(vertices[0].color, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(vertices[1].color, [1.0; 4]);
        assert_eq!(vertices[5].color, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn key_mapping_separates_text_from_editor_commands() {
        assert_eq!(
            map_key(PhysicalKey::Code(WinitKeyCode::ArrowLeft)),
            Some(KeyCode::Left)
        );
        assert_eq!(map_key(PhysicalKey::Code(WinitKeyCode::KeyV)), None);
        assert_eq!(
            map_edit_command(
                PhysicalKey::Code(WinitKeyCode::KeyV),
                KeyModifiers {
                    ctrl: true,
                    ..Default::default()
                }
            ),
            Some(TextEditCommand::Paste)
        );
        assert_eq!(map_key(PhysicalKey::Code(WinitKeyCode::F1)), None);
    }
}
