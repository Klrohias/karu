use super::*;
use karu::{
    Clipboard, Composition, KeyCode, KeyModifiers, Offset, RenderBackend, RenderCommand,
    TextEditCommand, TextInputCommand, TextInputEvent, TextInputResult,
};
use macroquad::input::KeyCode as QuadKeyCode;

pub(crate) fn handle_text_result(
    result: TextInputResult,
    composition: &mut Composition,
    text_layout: &mut CosmicTextLayout,
    backend: &mut QuadBackend,
    position: Offset,
) -> bool {
    for command in result.commands {
        match command {
            TextInputCommand::Copy(text) | TextInputCommand::Cut(text) => {
                let _ = backend.clipboard().set_text(&text);
            }
            TextInputCommand::PasteRequest => {
                if let Ok(Some(text)) = backend.clipboard().get_text() {
                    composition.dispatch_text_input_event_with(
                        text_layout,
                        TextInputEvent::Paste { position, text },
                    );
                }
            }
        }
    }
    result.handled
}

pub(crate) fn keyboard_keys() -> [(QuadKeyCode, KeyCode); 11] {
    [
        (QuadKeyCode::Left, KeyCode::Left),
        (QuadKeyCode::Right, KeyCode::Right),
        (QuadKeyCode::Up, KeyCode::Up),
        (QuadKeyCode::Down, KeyCode::Down),
        (QuadKeyCode::Home, KeyCode::Home),
        (QuadKeyCode::End, KeyCode::End),
        (QuadKeyCode::Backspace, KeyCode::Backspace),
        (QuadKeyCode::Delete, KeyCode::Delete),
        (QuadKeyCode::Enter, KeyCode::Enter),
        (QuadKeyCode::Tab, KeyCode::Tab),
        (QuadKeyCode::Escape, KeyCode::Escape),
    ]
}

pub(crate) fn shortcut_keys() -> [QuadKeyCode; 6] {
    [
        QuadKeyCode::A,
        QuadKeyCode::C,
        QuadKeyCode::V,
        QuadKeyCode::X,
        QuadKeyCode::Y,
        QuadKeyCode::Z,
    ]
}

pub(crate) fn edit_command(key: QuadKeyCode, modifiers: KeyModifiers) -> Option<TextEditCommand> {
    if !modifiers.command() {
        return None;
    }
    Some(match key {
        QuadKeyCode::A => TextEditCommand::SelectAll,
        QuadKeyCode::C => TextEditCommand::Copy,
        QuadKeyCode::V => TextEditCommand::Paste,
        QuadKeyCode::X => TextEditCommand::Cut,
        QuadKeyCode::Z if modifiers.shift => TextEditCommand::Redo,
        QuadKeyCode::Z => TextEditCommand::Undo,
        QuadKeyCode::Y => TextEditCommand::Redo,
        _ => return None,
    })
}

pub(crate) fn update_ime(commands: &[RenderCommand]) {
    let cursor = commands.iter().find_map(|command| {
        if let RenderCommand::DrawCursor { rect, .. } = command {
            Some(*rect)
        } else {
            None
        }
    });
    macroquad::miniquad::window::set_ime_enabled(cursor.is_some());
    if let Some(rect) = cursor {
        let scale = macroquad::miniquad::window::dpi_scale();
        macroquad::miniquad::window::set_ime_position(
            (rect.origin.x * scale) as i32,
            ((rect.origin.y + rect.size.height) * scale) as i32,
        );
    }
}
