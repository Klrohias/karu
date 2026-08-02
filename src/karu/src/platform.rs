use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardError {
    Unsupported,
    Platform(String),
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => f.write_str("clipboard is not supported by this platform"),
            Self::Platform(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ClipboardError {}

pub trait Clipboard {
    fn get_text(&mut self) -> Result<Option<String>, ClipboardError>;
    fn set_text(&mut self, text: &str) -> Result<(), ClipboardError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoopClipboard;

impl Clipboard for NoopClipboard {
    fn get_text(&mut self) -> Result<Option<String>, ClipboardError> {
        Ok(None)
    }

    fn set_text(&mut self, _text: &str) -> Result<(), ClipboardError> {
        Ok(())
    }
}
