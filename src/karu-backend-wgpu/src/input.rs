use karu::{KeyCode, KeyModifiers, Offset, RenderCommand, TextEditCommand};
use std::sync::Arc;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::keyboard::{KeyCode as WinitKeyCode, ModifiersState, PhysicalKey};
use winit::window::Window;

pub(crate) fn logical_position(
    position: PhysicalPosition<f64>,
    window: Option<&Arc<Window>>,
) -> Offset {
    let scale = window.map_or(1.0, |window| window.scale_factor()) as f32;
    Offset::new(position.x as f32 / scale, position.y as f32 / scale)
}

pub(crate) fn to_modifiers(state: ModifiersState) -> KeyModifiers {
    KeyModifiers {
        shift: state.shift_key(),
        ctrl: state.control_key(),
        alt: state.alt_key(),
        logo: state.super_key(),
    }
}

pub(crate) fn map_key(key: PhysicalKey) -> Option<KeyCode> {
    Some(match key {
        PhysicalKey::Code(WinitKeyCode::ArrowLeft) => KeyCode::Left,
        PhysicalKey::Code(WinitKeyCode::ArrowRight) => KeyCode::Right,
        PhysicalKey::Code(WinitKeyCode::ArrowUp) => KeyCode::Up,
        PhysicalKey::Code(WinitKeyCode::ArrowDown) => KeyCode::Down,
        PhysicalKey::Code(WinitKeyCode::Home) => KeyCode::Home,
        PhysicalKey::Code(WinitKeyCode::End) => KeyCode::End,
        PhysicalKey::Code(WinitKeyCode::Backspace) => KeyCode::Backspace,
        PhysicalKey::Code(WinitKeyCode::Delete) => KeyCode::Delete,
        PhysicalKey::Code(WinitKeyCode::Enter) => KeyCode::Enter,
        PhysicalKey::Code(WinitKeyCode::Tab) => KeyCode::Tab,
        PhysicalKey::Code(WinitKeyCode::Escape) => KeyCode::Escape,
        _ => return None,
    })
}

pub(crate) fn map_edit_command(
    key: PhysicalKey,
    modifiers: KeyModifiers,
) -> Option<TextEditCommand> {
    if !modifiers.command() {
        return None;
    }
    let PhysicalKey::Code(key) = key else {
        return None;
    };
    Some(match key {
        WinitKeyCode::KeyA => TextEditCommand::SelectAll,
        WinitKeyCode::KeyC => TextEditCommand::Copy,
        WinitKeyCode::KeyV => TextEditCommand::Paste,
        WinitKeyCode::KeyX => TextEditCommand::Cut,
        WinitKeyCode::KeyZ if modifiers.shift => TextEditCommand::Redo,
        WinitKeyCode::KeyZ => TextEditCommand::Undo,
        WinitKeyCode::KeyY => TextEditCommand::Redo,
        _ => return None,
    })
}

pub(crate) fn update_ime(window: &Window, commands: &[RenderCommand]) {
    let cursor = commands.iter().find_map(|command| match command {
        RenderCommand::DrawCursor { rect, .. } => Some(*rect),
        _ => None,
    });
    window.set_ime_allowed(cursor.is_some());
    if let Some(rect) = cursor {
        let scale = window.scale_factor() as f32;
        let position = PhysicalPosition::new(
            (rect.origin.x * scale).round() as i32,
            ((rect.origin.y + rect.size.height) * scale).round() as i32,
        );
        window.set_ime_cursor_area(position, PhysicalSize::new(1, 1));
    }
}
