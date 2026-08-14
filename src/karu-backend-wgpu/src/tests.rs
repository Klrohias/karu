use crate::app::*;
use crate::config::*;
use crate::geometry::*;
use crate::input::*;
use crate::renderer::*;
use crate::text::*;
use karu::{
    AppConfig, Brush, Color, GradientStop, KeyCode, KeyModifiers, Offset, Rect, RenderCommand,
    TextEditCommand,
};
use winit::dpi::PhysicalSize;
use winit::keyboard::{KeyCode as WinitKeyCode, PhysicalKey};

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
    let (x, y, width, height) = scissor_rect(Rect::new(450.0, 300.0, 100.0, 100.0), 2.0, 800, 600);
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
fn rounded_corner_segments_scale_with_radius() {
    assert_eq!(rounded_corner_segments(4.0), 8);
    assert!(rounded_corner_segments(100.0) > rounded_corner_segments(8.0));
    assert_eq!(rounded_corner_segments(10_000.0), 24);
}

#[test]
fn msaa_prefers_four_samples_and_falls_back_to_one() {
    assert_eq!(select_msaa_sample_count(&[1, 2, 4, 8], true), 4);
    assert_eq!(select_msaa_sample_count(&[1, 2, 4, 8], false), 1);
    assert_eq!(select_msaa_sample_count(&[1, 2], true), 1);
    assert_eq!(select_msaa_sample_count(&[], true), 1);
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
