use karu::{AppBackend, AppConfig, AppRoot, Color, Composition, Constraints, Rect, RenderCommand};
#[cfg(feature = "font-kit")]
use macroquad::prelude::load_ttf_font_from_bytes;
use macroquad::prelude::{
    Color as QuadColor, Conf, Font as QuadFont, TextParams, clear_background, draw_rectangle,
    draw_text_ex, next_frame, screen_height, screen_width,
};
#[cfg(feature = "font-kit")]
use std::path::Path;

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

            draw_text_ex(
                text,
                snap_to_physical_pixel(rect.origin.x),
                snap_to_physical_pixel(rect.origin.y + style.font_size),
                params,
            );
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
    let font = quad.font.as_ref()?;

    match load_system_font(font) {
        Ok(font) => Some(font),
        Err(error) => {
            eprintln!("karu-backend-quad: failed to load configured system font: {error}");
            None
        }
    }
}

#[cfg(feature = "font-kit")]
fn load_system_font(font: &FontConfig) -> Result<QuadFont, String> {
    use font_kit::family_name::FamilyName;
    use font_kit::properties::Properties;
    use font_kit::source::SystemSource;

    let source = SystemSource::new();
    let families = match font {
        FontConfig::DefaultSystem => default_system_font_families(),
        FontConfig::SystemFamily(family) => vec![FamilyName::Title(family.clone())],
    };

    let mut errors = Vec::new();

    for family in families {
        match source.select_best_match(&[family.clone()], &Properties::new()) {
            Ok(handle) => return load_font_handle(handle),
            Err(error) => errors.push(format!("font match failed for {family:?}: {error:?}")),
        }
    }

    if matches!(font, FontConfig::DefaultSystem) {
        for path in default_system_font_paths() {
            match load_font_path(path) {
                Ok(font) => return Ok(font),
                Err(error) => errors.push(error),
            }
        }
    }

    Err(errors.join("; "))
}

#[cfg(feature = "font-kit")]
fn load_font_handle(handle: font_kit::handle::Handle) -> Result<QuadFont, String> {
    match handle {
        font_kit::handle::Handle::Path { path, .. } => load_font_path(path),
        font_kit::handle::Handle::Memory { bytes, .. } => {
            load_ttf_font_from_bytes(bytes.as_ref()).map_err(|error| format!("{error:?}"))
        }
    }
}

#[cfg(feature = "font-kit")]
fn load_font_path(path: impl AsRef<Path>) -> Result<QuadFont, String> {
    let path = path.as_ref();
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    load_ttf_font_from_bytes(&bytes).map_err(|error| format!("{error:?}"))
}

#[cfg(all(feature = "font-kit", target_os = "macos"))]
fn default_system_font_families() -> Vec<font_kit::family_name::FamilyName> {
    use font_kit::family_name::FamilyName;

    vec![
        FamilyName::Title("PingFang SC".to_string()),
        FamilyName::Title("Hiragino Sans GB".to_string()),
        FamilyName::Title("STHeiti".to_string()),
        FamilyName::SansSerif,
    ]
}

#[cfg(all(feature = "font-kit", not(target_os = "macos")))]
fn default_system_font_families() -> Vec<font_kit::family_name::FamilyName> {
    vec![font_kit::family_name::FamilyName::SansSerif]
}

#[cfg(all(feature = "font-kit", target_os = "macos"))]
fn default_system_font_paths() -> &'static [&'static str] {
    &[
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/LanguageSupport/PingFang.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
    ]
}

#[cfg(all(feature = "font-kit", not(target_os = "macos")))]
fn default_system_font_paths() -> &'static [&'static str] {
    &[]
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

    #[cfg(all(feature = "font-kit", target_os = "macos"))]
    #[test]
    fn macos_default_system_font_prefers_cjk_families() {
        use font_kit::family_name::FamilyName;

        let families = default_system_font_families();
        assert_eq!(families[0], FamilyName::Title("PingFang SC".to_string()));
        assert!(families.contains(&FamilyName::Title("Hiragino Sans GB".to_string())));
    }
}
