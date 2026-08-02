use super::*;
use arboard::Clipboard as SystemClipboard;
use cosmic_text::FontSystem;
use karu::{Color, Offset, Rect, RenderBackend, RenderCommand, TextWrap};
use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::Infallible;
use std::hash::Hash;
use std::num::NonZeroU64;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;
use winit::window::Window;

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
pub(crate) struct TextResourceKey {
    text: String,
    font_size: u32,
    color: [u32; 4],
    rect: [u32; 4],
    offset: [u32; 2],
    wrap: u8,
    scale_factor: u32,
}

pub(crate) struct CachedTextResource {
    _texture: wgpu::Texture,
    _view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
}

#[derive(Default)]
pub(crate) struct TextResourceCache {
    entries: HashMap<TextResourceKey, CachedTextResource>,
}

pub(crate) struct CachedShapeResource {
    fingerprint: u64,
    buffer: wgpu::Buffer,
    vertex_count: u32,
}

#[derive(Default)]
pub(crate) struct ShapeResourceCache {
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

pub(crate) struct FrameStats {
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

    pub(crate) fn text_context(&self) -> Rc<RefCell<FontSystem>> {
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
    pub(crate) fn draw<'a>(
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
