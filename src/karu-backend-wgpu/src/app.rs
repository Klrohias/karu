use super::*;
use karu::{
    AppConfig, AppRoot, Clipboard, Composition, Constraints, KeyEvent, Offset, PointerEvent,
    PointerKind, PointerPhase, Recomposer, RenderBackend, TextInputCommand, TextInputEvent,
};
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowAttributes, WindowId};

pub(crate) struct WgpuApp {
    root: Option<AppRoot>,
    config: AppConfig,
    family: Option<String>,
    pub(crate) debug_info: bool,
    window: Option<Arc<Window>>,
    runtime: Option<WgpuRuntime>,
    cursor: Offset,
    modifiers: ModifiersState,
    pub(crate) redraw_pending: bool,
}

impl WgpuApp {
    pub(crate) fn new(
        root: AppRoot,
        config: AppConfig,
        family: Option<String>,
        debug_info: bool,
    ) -> Self {
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

    pub(crate) fn request_redraw(&mut self) {
        if self.redraw_pending {
            return;
        }
        self.redraw_pending = true;
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub(crate) fn request_redraw_if_needed(&mut self) {
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

pub(crate) struct WgpuRuntime {
    renderer: WgpuBackend,
    text_layout: CosmicTextLayout,
    composition: Composition,
    recomposer: Recomposer,
}
