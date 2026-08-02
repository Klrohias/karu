use crate::app::*;
use crate::config::*;
use crate::input::*;
use crate::paint::*;
use crate::renderer::*;
use crate::text::*;
use karu::{
    Brush, Color, GradientStop, KeyModifiers, Offset, Rect, TextEditCommand, TextLayoutEngine,
    TextWrap,
};
use macroquad::input::KeyCode as QuadKeyCode;

fn cosmic_text_content() {
    karu::__private::Text("Karu Foundation Playground", karu::TextOptions::default());
}

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
    let caret = renderer.caret_position(text, 14.0, f32::INFINITY, TextWrap::NoWrap, text.len());
    let size = renderer.measure_text("Karu Foundation Playground", 24.0, 600.0, TextWrap::NoWrap);

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

#[test]
fn configures_default_system_font() {
    let quad = Quad::new().default_system_font();
    assert_eq!(quad.font, Some(FontConfig::DefaultSystem));

    let quad = Quad.default_system_font();
    assert_eq!(quad.font, Some(FontConfig::DefaultSystem));
}
