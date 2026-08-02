use super::*;
use karu::{
    AppConfig, AppRoot, Composition, Constraints, KeyEvent, KeyModifiers, PointerEvent,
    PointerPhase, Recomposer, RenderBackend, TextInputEvent,
};
use macroquad::conf::{Conf, UpdateTrigger};
use macroquad::input::{
    KeyCode as QuadKeyCode, MouseButton, TouchPhase, get_char_pressed, is_key_down, is_key_pressed,
    is_mouse_button_pressed, is_mouse_button_released, mouse_position, mouse_wheel, touches,
};
use macroquad::prelude::{clear_background, next_frame, screen_height, screen_width};

pub(crate) fn window_config(config: &AppConfig, frame_mode: QuadFrameMode) -> Conf {
    let mut window = Conf::default();
    window.miniquad_conf.window_title = config.title.clone();
    window.miniquad_conf.window_width = config.width as i32;
    window.miniquad_conf.window_height = config.height as i32;
    window.miniquad_conf.window_resizable = config.resizable;

    if frame_mode == QuadFrameMode::OnDemand {
        window.miniquad_conf.platform.blocking_event_loop = true;
        window.update_on = Some(UpdateTrigger {
            key_down: true,
            mouse_down: true,
            mouse_up: true,
            mouse_motion: true,
            mouse_wheel: true,
            touch: true,
            ..Default::default()
        });
    } else {
        window.update_on = None;
    }

    window
}

pub(crate) async fn run_quad(root: AppRoot, config: AppConfig, quad: ConfiguredQuad) {
    let family = quad.font_family();
    let mut text_layout = CosmicTextLayout::new(family.clone());
    let mut backend = QuadBackend::new(family);
    let mut composition = Composition::new(root);
    let mut recomposer = Recomposer::new();
    let mut stats = QuadFrameStats::new(std::env::var_os("KARU_QUAD_DEBUG").is_some());
    macroquad::input::simulate_mouse_with_touch(false);
    let mut previous_mouse_position = None;

    loop {
        clear_background(to_quad_color(config.background));

        composition.set_constraints(Constraints::loose(screen_width(), screen_height()));
        let mouse = mouse_position();
        let mouse_position = karu::Offset::new(mouse.0, mouse.1);
        let wheel = mouse_wheel();
        if wheel.0 != 0.0 || wheel.1 != 0.0 {
            composition.dispatch_scroll_event(karu::ScrollEvent {
                position: mouse_position,
                delta: karu::Offset::new(wheel.0, -wheel.1 * 24.0),
            });
        }
        let modifiers = KeyModifiers {
            shift: is_key_down(QuadKeyCode::LeftShift) || is_key_down(QuadKeyCode::RightShift),
            ctrl: is_key_down(QuadKeyCode::LeftControl) || is_key_down(QuadKeyCode::RightControl),
            alt: is_key_down(QuadKeyCode::LeftAlt) || is_key_down(QuadKeyCode::RightAlt),
            logo: is_key_down(QuadKeyCode::LeftSuper) || is_key_down(QuadKeyCode::RightSuper),
        };
        let mut suppress_character_input = modifiers.command() || modifiers.alt;
        for (quad_key, key) in keyboard_keys() {
            if !is_key_pressed(quad_key) {
                continue;
            }
            let event = KeyEvent {
                code: key,
                modifiers,
                repeat: false,
            };
            let result = composition.dispatch_key_event_with_result_with(
                &mut text_layout,
                mouse_position,
                event,
            );
            suppress_character_input |= handle_text_result(
                result,
                &mut composition,
                &mut text_layout,
                &mut backend,
                mouse_position,
            );
        }
        for quad_key in shortcut_keys() {
            if !is_key_pressed(quad_key) {
                continue;
            }
            let Some(command) = edit_command(quad_key, modifiers) else {
                continue;
            };
            let result = composition.dispatch_text_input_event_with_result_with(
                &mut text_layout,
                TextInputEvent::Command {
                    position: mouse_position,
                    command,
                },
            );
            suppress_character_input |= handle_text_result(
                result,
                &mut composition,
                &mut text_layout,
                &mut backend,
                mouse_position,
            );
        }
        if !suppress_character_input {
            while let Some(character) = get_char_pressed() {
                if !character.is_control() {
                    composition.dispatch_text_input_event_with(
                        &mut text_layout,
                        TextInputEvent::Insert {
                            position: mouse_position,
                            text: character.to_string(),
                        },
                    );
                }
            }
        } else {
            while get_char_pressed().is_some() {}
        }
        if is_mouse_button_pressed(MouseButton::Left) {
            composition.dispatch_pointer_event_with(
                &mut text_layout,
                PointerEvent {
                    kind: karu::PointerKind::Mouse,
                    phase: PointerPhase::Down,
                    position: mouse_position,
                    primary: true,
                },
            );
        } else if is_mouse_button_released(MouseButton::Left) {
            composition.dispatch_pointer_event_with(
                &mut text_layout,
                PointerEvent {
                    kind: karu::PointerKind::Mouse,
                    phase: PointerPhase::Up,
                    position: mouse_position,
                    primary: true,
                },
            );
        } else if previous_mouse_position != Some(mouse_position) {
            composition.dispatch_pointer_event_with(
                &mut text_layout,
                PointerEvent {
                    kind: karu::PointerKind::Mouse,
                    phase: PointerPhase::Move,
                    position: mouse_position,
                    primary: false,
                },
            );
        }
        previous_mouse_position = Some(mouse_position);

        for touch in touches() {
            let phase = match touch.phase {
                TouchPhase::Started => PointerPhase::Down,
                TouchPhase::Stationary | TouchPhase::Moved => PointerPhase::Move,
                TouchPhase::Ended => PointerPhase::Up,
                TouchPhase::Cancelled => PointerPhase::Cancel,
            };
            composition.dispatch_pointer_event_with(
                &mut text_layout,
                PointerEvent {
                    kind: karu::PointerKind::Touch { id: touch.id },
                    phase,
                    position: karu::Offset::new(touch.position.x, touch.position.y),
                    primary: true,
                },
            );
        }

        let recomposed =
            if let Some(result) = recomposer.recompose_with(&mut composition, &mut text_layout) {
                render_result(&mut backend, &result);
                true
            } else {
                let result = composition
                    .last_result()
                    .expect("composition result exists after the first frame");
                render_result(&mut backend, result);
                false
            };

        stats.record(recomposed, composition.last_result());

        next_frame().await;
    }
}

pub(crate) fn render_result(backend: &mut QuadBackend, result: &karu::CompositionResult) {
    backend
        .render(&result.render_tree, &result.commands)
        .expect("quad rendering succeeds");
    update_ime(&result.commands);
}
