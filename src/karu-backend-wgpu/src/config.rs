use super::*;
use karu::{AppBackend, AppConfig, AppRoot};
use winit::event_loop::EventLoop;

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
    pub(crate) debug_info: bool,
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
pub(crate) enum FontConfig {
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
