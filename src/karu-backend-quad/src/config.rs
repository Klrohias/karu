use super::*;
use karu::{AppBackend, AppConfig, AppRoot};

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

    pub fn default_system_font(self) -> ConfiguredQuad {
        ConfiguredQuad::default().default_system_font()
    }

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
    pub(crate) frame_mode: QuadFrameMode,
    pub(crate) font: Option<FontConfig>,
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

    pub(crate) fn font_family(&self) -> Option<String> {
        match self.font.as_ref()? {
            FontConfig::DefaultSystem => None,
            FontConfig::SystemFamily(family) => Some(family.clone()),
        }
    }

    pub fn default_system_font(mut self) -> Self {
        self.font = Some(FontConfig::DefaultSystem);
        self
    }

    pub fn system_font(mut self, family: impl Into<String>) -> Self {
        self.font = Some(FontConfig::SystemFamily(family.into()));
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FontConfig {
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
