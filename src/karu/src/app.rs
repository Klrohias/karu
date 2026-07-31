use crate::composition::Composer;
use crate::modifier::Color;

pub type AppRoot = Box<dyn FnMut(&mut Composer)>;

#[derive(Clone, Debug, PartialEq)]
pub struct AppConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub background: Color,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            title: "Karu".to_string(),
            width: 800,
            height: 600,
            resizable: true,
            background: Color::WHITE,
        }
    }
}

pub trait AppBackend: Sized + 'static {
    fn run(self, root: AppRoot, config: AppConfig);
}

#[derive(Clone, Debug)]
pub struct App<B> {
    backend: B,
    config: AppConfig,
}

#[derive(Clone, Debug)]
pub struct AppBuilder<B = ()> {
    backend: B,
    config: AppConfig,
}

impl App<()> {
    pub fn builder() -> AppBuilder<()> {
        AppBuilder::default()
    }
}

impl Default for AppBuilder<()> {
    fn default() -> Self {
        Self {
            backend: (),
            config: AppConfig::default(),
        }
    }
}

impl<B> AppBuilder<B> {
    pub fn with_renderer<N>(self, backend: N) -> AppBuilder<N> {
        AppBuilder {
            backend,
            config: self.config,
        }
    }

    pub fn with_config(mut self, config: AppConfig) -> Self {
        self.config = config;
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.config.title = title.into();
        self
    }

    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.config.width = width;
        self.config.height = height;
        self
    }

    pub fn resizable(mut self, resizable: bool) -> Self {
        self.config.resizable = resizable;
        self
    }

    pub fn background(mut self, background: Color) -> Self {
        self.config.background = background;
        self
    }

    pub fn build(self) -> App<B> {
        App {
            backend: self.backend,
            config: self.config,
        }
    }
}

impl<B: AppBackend> App<B> {
    pub fn run(self, root: impl FnMut(&mut Composer) + 'static) {
        self.backend.run(Box::new(root), self.config);
    }
}
