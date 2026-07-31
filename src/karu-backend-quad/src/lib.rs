use karu::{AppBackend, AppConfig, AppRoot, Color, Composition, Constraints, Rect, RenderCommand};
#[cfg(feature = "font-kit")]
use macroquad::prelude::load_ttf_font_from_bytes;
use macroquad::prelude::{
    Color as QuadColor, Conf, Font as QuadFont, TextParams, clear_background, draw_rectangle,
    draw_text_ex, next_frame, screen_height, screen_width,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct Quad;

impl Quad {
    pub fn new() -> Self {
        Self
    }

    #[cfg(feature = "font-kit")]
    pub fn system_font(self, family: impl Into<String>) -> ConfiguredQuad {
        ConfiguredQuad::default().system_font(family)
    }
}

#[derive(Clone, Debug, Default)]
pub struct ConfiguredQuad {
    #[cfg(feature = "font-kit")]
    system_font_family: Option<String>,
}

impl ConfiguredQuad {
    #[cfg(feature = "font-kit")]
    pub fn system_font(mut self, family: impl Into<String>) -> Self {
        self.system_font_family = Some(family.into());
        self
    }
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

async fn run_quad(mut root: AppRoot, config: AppConfig, quad: ConfiguredQuad) {
    let mut composition = Composition::new(move |composer| root(composer));
    let font = load_configured_font(&quad).await;

    loop {
        clear_background(to_quad_color(config.background));

        composition.set_constraints(Constraints::loose(screen_width(), screen_height()));
        let result = composition.compose();

        for command in &result.commands {
            draw_command(command, font.as_ref());
        }

        next_frame().await;
    }
}

fn draw_command(command: &RenderCommand, font: Option<&QuadFont>) {
    match command {
        RenderCommand::FillRect { rect, color, .. } => draw_rect(*rect, *color),
        RenderCommand::DrawText {
            rect, text, style, ..
        } => {
            let params = TextParams {
                font,
                font_size: font_size(style.font_size),
                color: to_quad_color(style.color),
                ..Default::default()
            };

            draw_text_ex(text, rect.origin.x, rect.origin.y + style.font_size, params);
        }
        RenderCommand::PushClip(_) | RenderCommand::PopClip | RenderCommand::DrawImage { .. } => {}
    }
}

#[cfg(not(feature = "font-kit"))]
async fn load_configured_font(_: &ConfiguredQuad) -> Option<QuadFont> {
    None
}

#[cfg(feature = "font-kit")]
async fn load_configured_font(quad: &ConfiguredQuad) -> Option<QuadFont> {
    let family = quad.system_font_family.as_deref()?;

    match load_system_font(family) {
        Ok(font) => Some(font),
        Err(error) => {
            eprintln!("karu-backend-quad: failed to load system font {family:?}: {error}");
            None
        }
    }
}

#[cfg(feature = "font-kit")]
fn load_system_font(family: &str) -> Result<QuadFont, String> {
    use font_kit::family_name::FamilyName;
    use font_kit::handle::Handle;
    use font_kit::properties::Properties;
    use font_kit::source::SystemSource;

    let source = SystemSource::new();
    let handle = source
        .select_best_match(&[FamilyName::Title(family.to_string())], &Properties::new())
        .map_err(|error| format!("font match failed: {error:?}"))?;

    match handle {
        Handle::Path { path, .. } => {
            let bytes = std::fs::read(&path)
                .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
            load_ttf_font_from_bytes(&bytes).map_err(|error| format!("{error:?}"))
        }
        Handle::Memory { bytes, .. } => {
            load_ttf_font_from_bytes(bytes.as_ref()).map_err(|error| format!("{error:?}"))
        }
    }
}

fn draw_rect(rect: Rect, color: Color) {
    draw_rectangle(
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
        to_quad_color(color),
    );
}

fn to_quad_color(color: Color) -> QuadColor {
    QuadColor::new(color.red, color.green, color.blue, color.alpha)
}

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

    #[cfg(feature = "font-kit")]
    #[test]
    fn configures_system_font_family() {
        let quad = Quad::new().system_font("Arial");
        assert_eq!(quad.system_font_family.as_deref(), Some("Arial"));

        let quad = Quad.system_font("PingFang SC");
        assert_eq!(quad.system_font_family.as_deref(), Some("PingFang SC"));
    }
}
