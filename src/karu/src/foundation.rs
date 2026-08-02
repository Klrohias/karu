use crate::renderer::{TextInputCommand, TextLayoutEngine};
use crate::text_layout::{
    TextWrap, grapheme_boundaries, next_grapheme_boundary, previous_grapheme_boundary,
};
use crate::{
    KeyCode, KeyEvent, MutableState, Offset, Size, TextEditCommand, TextInputResult,
    mutable_state_of,
};
use std::cell::RefCell;
use std::fmt;
use std::ops::Range;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use unicode_segmentation::UnicodeSegmentation;

/// Renderer-independent scroll position used by scroll modifiers and lazy layouts.
#[derive(Clone)]
pub struct ScrollState {
    offset: MutableState<f32>,
    max_value: MutableState<f32>,
}

impl ScrollState {
    pub fn new(initial: f32) -> Self {
        Self {
            offset: mutable_state_of(initial.max(0.0)),
            max_value: mutable_state_of(0.0),
        }
    }

    pub fn value(&self) -> f32 {
        self.offset.get()
    }
    pub fn max_value(&self) -> f32 {
        self.max_value.get()
    }
    pub fn set_max_value(&self, value: f32) {
        let value = value.max(0.0);
        self.max_value.set_without_invalidation(value);
        let current = self.value();
        if current > value {
            self.offset.set(value);
        }
    }
    pub fn scroll_to(&self, value: f32) {
        self.offset.set(value.clamp(0.0, self.max_value()));
    }
    pub fn scroll_by(&self, delta: f32) -> f32 {
        let previous = self.value();
        self.scroll_to(previous + delta);
        self.value() - previous
    }
}

impl PartialEq for ScrollState {
    fn eq(&self, other: &Self) -> bool {
        self.value() == other.value() && self.max_value() == other.max_value()
    }
}

impl fmt::Debug for ScrollState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScrollState")
            .field("value", &self.value())
            .field("max_value", &self.max_value())
            .finish()
    }
}

impl Default for ScrollState {
    fn default() -> Self {
        Self::new(0.0)
    }
}

pub fn remember_scroll_state(initial: f32) -> ScrollState {
    crate::remember(|| ScrollState::new(initial))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FocusRequester(u64);

impl FocusRequester {
    pub fn new() -> Self {
        static NEXT_FOCUS_REQUESTER_ID: AtomicU64 = AtomicU64::new(1);
        Self(NEXT_FOCUS_REQUESTER_ID.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for FocusRequester {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct FocusState {
    focused: MutableState<Option<FocusRequester>>,
}

impl Default for FocusState {
    fn default() -> Self {
        Self {
            focused: mutable_state_of(None),
        }
    }
}

impl FocusState {
    pub fn request_focus(&self, requester: FocusRequester) {
        self.focused.set(Some(requester));
    }
    pub fn clear_focus(&self) {
        self.focused.set(None);
    }
    pub fn focused(&self) -> Option<FocusRequester> {
        self.focused.get()
    }
    pub fn is_focused(&self, requester: FocusRequester) -> bool {
        self.focused.get() == Some(requester)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextFieldValue {
    pub text: String,
    pub selection: Range<usize>,
    pub composition: Option<Range<usize>>,
}

impl TextFieldValue {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let end = text.len();
        Self {
            text,
            selection: end..end,
            composition: None,
        }
    }

    fn normalize_selection(&mut self) {
        let start = character_boundary_at_or_before(&self.text, self.selection.start);
        let end = character_boundary_at_or_before(&self.text, self.selection.end);
        self.selection = start.min(end)..start.max(end);
        self.composition = self.composition.clone().map(|range| {
            let start = character_boundary_at_or_before(&self.text, range.start);
            let end = character_boundary_at_or_before(&self.text, range.end);
            start.min(end)..start.max(end)
        });
    }
}

#[derive(Clone)]
pub struct TextFieldState {
    value: MutableState<TextFieldValue>,
    anchor: Rc<RefCell<Option<usize>>>,
    active: Rc<RefCell<usize>>,
    history: Rc<RefCell<TextHistory>>,
    composition_before: Rc<RefCell<Option<TextFieldValue>>>,
    scroll_offset: Rc<RefCell<crate::Offset>>,
    desired_x: Rc<RefCell<Option<f32>>>,
}

struct TextNavigation<'a> {
    text: &'a str,
    font_size: f32,
    max_width: f32,
    wrap: TextWrap,
    layout: &'a mut dyn TextLayoutEngine,
}

#[derive(Default)]
struct TextHistory {
    undo: Vec<TextFieldValue>,
    redo: Vec<TextFieldValue>,
}

impl TextFieldState {
    pub fn new(initial: impl Into<String>) -> Self {
        let initial = initial.into();
        let active = initial.len();
        Self {
            value: mutable_state_of(TextFieldValue::new(initial)),
            anchor: Rc::new(RefCell::new(None)),
            active: Rc::new(RefCell::new(active)),
            history: Rc::new(RefCell::new(TextHistory::default())),
            composition_before: Rc::new(RefCell::new(None)),
            scroll_offset: Rc::new(RefCell::new(crate::Offset::ZERO)),
            desired_x: Rc::new(RefCell::new(None)),
        }
    }
    pub fn value(&self) -> TextFieldValue {
        self.value.get()
    }
    pub fn set_value(&self, value: TextFieldValue) {
        let mut value = value;
        value.normalize_selection();
        *self.anchor.borrow_mut() = None;
        *self.active.borrow_mut() = value.selection.end;
        *self.composition_before.borrow_mut() = None;
        *self.scroll_offset.borrow_mut() = crate::Offset::ZERO;
        *self.desired_x.borrow_mut() = None;
        self.value.set(value);
    }
    pub fn edit(&self, edit: impl FnOnce(&mut TextFieldValue)) {
        self.mutate(|value| {
            edit(value);
            value.normalize_selection();
        });
    }
    pub fn text(&self) -> String {
        self.value.get().text
    }

    pub fn active_endpoint(&self) -> usize {
        *self.active.borrow()
    }

    pub fn replace_selection(&self, replacement: impl AsRef<str>) {
        let replacement = replacement.as_ref().to_string();
        self.mutate(|value| {
            value.normalize_selection();
            let start = value.selection.start;
            let end = value.selection.end;
            value.text.replace_range(start..end, &replacement);
            let cursor = start + replacement.len();
            value.selection = cursor..cursor;
            value.composition = None;
        });
    }

    pub fn backspace(&self) {
        self.mutate(|value| {
            value.normalize_selection();
            let start = value.selection.start;
            let end = value.selection.end;
            if start != end {
                value.text.replace_range(start..end, "");
                value.selection = start..start;
            } else if start > 0 {
                let previous = previous_boundary(&value.text, start);
                value.text.replace_range(previous..start, "");
                value.selection = previous..previous;
            }
            value.composition = None;
        });
    }

    pub fn delete(&self) {
        self.mutate(|value| {
            value.normalize_selection();
            let start = value.selection.start;
            let end = if start != value.selection.end {
                value.selection.end
            } else {
                next_boundary(&value.text, start)
            };
            if start < end {
                value.text.replace_range(start..end, "");
                value.selection = start..start;
            }
            value.composition = None;
        });
    }

    pub fn select_all(&self) {
        let end = self.value.get().text.len();
        self.select_range(0..end, None);
    }

    pub fn selected_text(&self) -> Option<String> {
        let value = self.value.get();
        if value.selection.start == value.selection.end {
            None
        } else {
            Some(value.text[value.selection.clone()].to_string())
        }
    }

    pub fn select_range(&self, range: Range<usize>, anchor: Option<usize>) {
        let mut value = self.value.get();
        let raw_start = range.start;
        value.selection = range;
        value.normalize_selection();
        let active = anchor
            .map(|anchor| {
                if raw_start == anchor {
                    value.selection.end
                } else {
                    value.selection.start
                }
            })
            .unwrap_or(value.selection.end);
        *self.anchor.borrow_mut() = anchor;
        *self.active.borrow_mut() = active;
        self.value.set(value);
    }

    pub fn set_cursor(&self, cursor: usize) {
        let cursor = character_boundary_at_or_before(&self.text(), cursor);
        *self.desired_x.borrow_mut() = None;
        self.select_range(cursor..cursor, None);
    }

    pub fn set_cursor_from_x(&self, x: f32, font_size: f32) {
        let cursor = self.cursor_from_x(x, font_size);
        self.set_cursor(cursor);
    }

    pub fn set_cursor_from_position(
        &self,
        position: crate::Offset,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        layout: &mut dyn TextLayoutEngine,
    ) {
        self.set_cursor(layout.hit_test_text(text, font_size, max_width, wrap, position));
    }

    pub(crate) fn begin_selection_from_position(
        &self,
        position: crate::Offset,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        layout: &mut dyn TextLayoutEngine,
    ) {
        *self.anchor.borrow_mut() =
            Some(layout.hit_test_text(text, font_size, max_width, wrap, position));
    }

    pub(crate) fn extend_selection_from_position(
        &self,
        position: crate::Offset,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        layout: &mut dyn TextLayoutEngine,
    ) {
        let cursor = layout.hit_test_text(text, font_size, max_width, wrap, position);
        let anchor = self
            .anchor
            .borrow()
            .unwrap_or_else(|| self.value.get().selection.start);
        self.select_range(anchor.min(cursor)..anchor.max(cursor), Some(anchor));
        *self.active.borrow_mut() = cursor;
    }

    pub fn scroll_offset(&self) -> crate::Offset {
        *self.scroll_offset.borrow()
    }

    pub fn ensure_cursor_visible(
        &self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        text_size: Size,
        viewport: Size,
        layout: &mut dyn TextLayoutEngine,
    ) {
        let caret = layout.caret_position(text, font_size, max_width, wrap, self.active_endpoint());
        let mut scroll = self.scroll_offset.borrow_mut();
        if text_size.height > caret.height {
            scroll.x = 0.0;
        } else {
            let right = scroll.x + viewport.width;
            if caret.position.x < scroll.x {
                scroll.x = caret.position.x.max(0.0);
            } else if caret.position.x > right {
                scroll.x = (caret.position.x - viewport.width).max(0.0);
            }
        }
        if text_size.height > caret.height {
            let bottom = scroll.y + viewport.height;
            if caret.position.y < scroll.y {
                scroll.y = caret.position.y.max(0.0);
            } else if caret.position.y + caret.height > bottom {
                scroll.y = (caret.position.y + caret.height - viewport.height).max(0.0);
            }
        } else {
            scroll.y = 0.0;
        }
        let max_x = (text_size.width - viewport.width).max(0.0);
        let max_y = (text_size.height - viewport.height).max(0.0);
        scroll.x = scroll.x.min(max_x);
        scroll.y = scroll.y.min(max_y);
    }

    pub(crate) fn clear_selection_anchor(&self) {
        *self.anchor.borrow_mut() = None;
    }

    fn cursor_from_x(&self, x: f32, font_size: f32) -> usize {
        let text = self.text();
        let char_width = (font_size * (8.0 / 14.0)).max(1.0);
        let target = (x.max(0.0) / char_width).round() as usize;
        text.char_indices()
            .nth(target)
            .map(|(index, _)| index)
            .unwrap_or(text.len())
    }

    pub fn handle_key(&self, event: KeyEvent) -> TextInputResult {
        self.handle_key_internal(event, None)
    }

    pub fn handle_command(&self, command: TextEditCommand) -> TextInputResult {
        match command {
            TextEditCommand::SelectAll => {
                self.select_all();
                TextInputResult::handled()
            }
            TextEditCommand::Copy => self
                .selected_text()
                .map_or_else(TextInputResult::handled, |text| {
                    TextInputResult::command(TextInputCommand::Copy(text))
                }),
            TextEditCommand::Cut => {
                self.selected_text()
                    .map_or_else(TextInputResult::handled, |text| {
                        self.replace_selection("");
                        TextInputResult::command(TextInputCommand::Cut(text))
                    })
            }
            TextEditCommand::Paste => TextInputResult::command(TextInputCommand::PasteRequest),
            TextEditCommand::Undo => {
                self.undo();
                TextInputResult::handled()
            }
            TextEditCommand::Redo => {
                self.redo();
                TextInputResult::handled()
            }
        }
    }

    pub(crate) fn handle_key_with_layout(
        &self,
        event: KeyEvent,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        layout: &mut dyn TextLayoutEngine,
    ) -> TextInputResult {
        self.handle_key_internal(
            event,
            Some(TextNavigation {
                text,
                font_size,
                max_width,
                wrap,
                layout,
            }),
        )
    }

    fn handle_key_internal(
        &self,
        event: KeyEvent,
        mut navigation: Option<TextNavigation<'_>>,
    ) -> TextInputResult {
        let command = event.modifiers.command();
        match (event.code, command, event.modifiers.shift) {
            (KeyCode::Backspace, _, _) => {
                if command {
                    self.delete_word_backward();
                } else {
                    self.backspace();
                }
                TextInputResult::handled()
            }
            (KeyCode::Delete, _, _) => {
                if command {
                    self.delete_word_forward();
                } else {
                    self.delete();
                }
                TextInputResult::handled()
            }
            (KeyCode::Left, _, shift) => {
                self.move_horizontal(-1, command, shift);
                TextInputResult::handled()
            }
            (KeyCode::Right, _, shift) => {
                self.move_horizontal(1, command, shift);
                TextInputResult::handled()
            }
            (KeyCode::Up, _, shift) => {
                self.move_vertical(-1, shift, navigation.as_mut());
                TextInputResult::handled()
            }
            (KeyCode::Down, _, shift) => {
                self.move_vertical(1, shift, navigation.as_mut());
                TextInputResult::handled()
            }
            (KeyCode::Home, _, shift) => {
                let value = self.value.get();
                let cursor = if command {
                    0
                } else if let Some(navigation) = navigation.as_mut() {
                    navigation
                        .layout
                        .text_line_range(
                            navigation.text,
                            navigation.font_size,
                            navigation.max_width,
                            navigation.wrap,
                            self.active_endpoint(),
                        )
                        .start
                } else {
                    line_start(&value.text, value.selection.end)
                };
                self.move_to(cursor, shift);
                TextInputResult::handled()
            }
            (KeyCode::End, _, shift) => {
                let value = self.value.get();
                let cursor = if command {
                    value.text.len()
                } else if let Some(navigation) = navigation.as_mut() {
                    navigation
                        .layout
                        .text_line_range(
                            navigation.text,
                            navigation.font_size,
                            navigation.max_width,
                            navigation.wrap,
                            self.active_endpoint(),
                        )
                        .end
                } else {
                    line_end(&value.text, value.selection.end)
                };
                self.move_to(cursor, shift);
                TextInputResult::handled()
            }
            (KeyCode::Enter, false, _) => {
                self.replace_selection("\n");
                TextInputResult::handled()
            }
            (KeyCode::Tab, false, _) => {
                self.replace_selection("\t");
                TextInputResult::handled()
            }
            (KeyCode::Escape, _, _) => {
                self.end_composition();
                TextInputResult::handled()
            }
            _ => TextInputResult::default(),
        }
    }

    pub fn start_composition(&self) {
        let mut value = self.value.get();
        if value.composition.is_some() {
            return;
        }
        *self.composition_before.borrow_mut() = Some(value.clone());
        value.normalize_selection();
        let start = value.selection.start;
        let end = value.selection.end;
        if start != end {
            value.text.replace_range(start..end, "");
            value.selection = start..start;
        }
        value.composition = Some(start..start);
        *self.desired_x.borrow_mut() = None;
        self.value.set(value);
    }

    pub fn update_composition(&self, text: impl AsRef<str>) {
        let text = text.as_ref().to_string();
        if self.value.get().composition.is_none() {
            self.start_composition();
        }
        let mut value = self.value.get();
        let range = value.composition.clone().unwrap_or(value.selection.clone());
        let start = range.start;
        value.text.replace_range(range, &text);
        let end = start + text.len();
        value.selection = end..end;
        value.composition = Some(start..end);
        *self.active.borrow_mut() = end;
        *self.desired_x.borrow_mut() = None;
        self.value.set(value);
    }

    pub fn commit_composition(&self, text: impl AsRef<str>) {
        if self.value.get().composition.is_some() {
            self.update_composition(text);
            self.finish_composition();
        } else {
            self.replace_selection(text);
        }
    }

    pub fn end_composition(&self) {
        let before = self.composition_before.borrow_mut().take();
        if let Some(before) = before {
            self.value.set(before);
        } else {
            self.value.update(|value| value.composition = None);
        }
        *self.active.borrow_mut() = self.value.get().selection.end;
    }

    pub fn cancel_composition(&self) {
        self.end_composition();
    }

    pub fn undo(&self) {
        let Some(previous) = self.history.borrow_mut().undo.pop() else {
            return;
        };
        let current = self.value.get();
        self.history.borrow_mut().redo.push(current);
        let active = previous.selection.end;
        self.value.set(previous);
        *self.anchor.borrow_mut() = None;
        *self.active.borrow_mut() = active;
    }

    pub fn move_left(&self, extend: bool) {
        self.move_horizontal(-1, false, extend);
    }

    pub fn move_right(&self, extend: bool) {
        self.move_horizontal(1, false, extend);
    }

    pub fn move_word_left(&self, extend: bool) {
        self.move_horizontal(-1, true, extend);
    }

    pub fn move_word_right(&self, extend: bool) {
        self.move_horizontal(1, true, extend);
    }

    pub fn delete_word_backward(&self) {
        self.delete_word_backward_impl();
    }

    pub fn delete_word_forward(&self) {
        self.delete_word_forward_impl();
    }

    pub fn redo(&self) {
        let Some(next) = self.history.borrow_mut().redo.pop() else {
            return;
        };
        let current = self.value.get();
        self.history.borrow_mut().undo.push(current);
        let active = next.selection.end;
        self.value.set(next);
        *self.anchor.borrow_mut() = None;
        *self.active.borrow_mut() = active;
    }

    fn mutate(&self, edit: impl FnOnce(&mut TextFieldValue)) {
        let before = self.value.get();
        self.value.update(edit);
        let after = self.value.get();
        if before.text != after.text {
            let mut history = self.history.borrow_mut();
            history.undo.push(before);
            history.redo.clear();
        }
        *self.composition_before.borrow_mut() = None;
        *self.anchor.borrow_mut() = None;
        *self.active.borrow_mut() = after.selection.end;
        *self.desired_x.borrow_mut() = None;
    }

    fn finish_composition(&self) {
        let before = self.composition_before.borrow_mut().take();
        self.value.update(|value| value.composition = None);
        if let Some(before) = before {
            let after = self.value.get();
            if before.text != after.text {
                let mut history = self.history.borrow_mut();
                history.undo.push(before);
                history.redo.clear();
            }
        }
        *self.active.borrow_mut() = self.value.get().selection.end;
    }

    fn move_horizontal(&self, direction: isize, by_word: bool, extend: bool) {
        *self.desired_x.borrow_mut() = None;
        let value = self.value.get();
        let cursor = if !extend && value.selection.start != value.selection.end {
            if direction < 0 {
                value.selection.start
            } else {
                value.selection.end
            }
        } else {
            let cursor = if extend {
                self.active_endpoint()
            } else if direction < 0 {
                value.selection.start
            } else {
                value.selection.end
            };
            if by_word {
                word_boundary(&value.text, cursor, direction < 0)
            } else if direction < 0 {
                previous_boundary(&value.text, cursor)
            } else {
                next_boundary(&value.text, cursor)
            }
        };
        self.move_to(cursor, extend);
    }

    fn move_to(&self, cursor: usize, extend: bool) {
        if extend {
            let anchor = self
                .anchor
                .borrow_mut()
                .get_or_insert(*self.active.borrow())
                .to_owned();
            self.select_range(anchor.min(cursor)..anchor.max(cursor), Some(anchor));
            *self.active.borrow_mut() = cursor;
        } else {
            self.set_cursor(cursor);
        }
    }

    fn move_vertical(
        &self,
        direction: isize,
        extend: bool,
        navigation: Option<&mut TextNavigation<'_>>,
    ) {
        if let Some(navigation) = navigation {
            let current = navigation.layout.caret_position(
                navigation.text,
                navigation.font_size,
                navigation.max_width,
                navigation.wrap,
                self.active_endpoint(),
            );
            let desired_x = self
                .desired_x
                .borrow_mut()
                .get_or_insert(current.position.x)
                .to_owned();
            let target = navigation.layout.hit_test_text(
                navigation.text,
                navigation.font_size,
                navigation.max_width,
                navigation.wrap,
                Offset::new(
                    desired_x,
                    current.position.y + direction as f32 * current.height,
                ),
            );
            self.move_to(target, extend);
            *self.desired_x.borrow_mut() = Some(desired_x);
            return;
        }
        let value = self.value.get();
        let cursor = self.active_endpoint();
        let line_start = value.text[..cursor]
            .rfind('\n')
            .map(|index| index + 1)
            .unwrap_or(0);
        let column = value.text[line_start..cursor].graphemes(true).count();
        let target = if direction < 0 {
            if line_start == 0 {
                0
            } else {
                let previous_end = line_start - 1;
                let previous_start = value.text[..previous_end]
                    .rfind('\n')
                    .map(|index| index + 1)
                    .unwrap_or(0);
                char_offset(&value.text, previous_start, column)
            }
        } else {
            let line_end = value.text[cursor..]
                .find('\n')
                .map(|index| cursor + index)
                .unwrap_or(value.text.len());
            if line_end == value.text.len() {
                value.text.len()
            } else {
                let next_start = line_end + 1;
                char_offset(&value.text, next_start, column)
            }
        };
        self.move_to(target, extend);
    }

    fn delete_word_backward_impl(&self) {
        let value = self.value.get();
        if value.selection.start != value.selection.end {
            self.replace_selection("");
            return;
        }
        let start = word_boundary(&value.text, value.selection.start, true);
        self.select_range(start..value.selection.start, None);
        self.replace_selection("");
    }

    fn delete_word_forward_impl(&self) {
        let value = self.value.get();
        if value.selection.start != value.selection.end {
            self.replace_selection("");
            return;
        }
        let end = word_boundary(&value.text, value.selection.end, false);
        self.select_range(value.selection.end..end, None);
        self.replace_selection("");
    }
}

fn character_boundary_at_or_before(text: &str, index: usize) -> usize {
    let index = index.min(text.len());
    grapheme_boundaries(text)
        .take_while(|boundary| *boundary <= index)
        .last()
        .unwrap_or(0)
}

fn previous_boundary(text: &str, index: usize) -> usize {
    previous_grapheme_boundary(text, index)
}

fn next_boundary(text: &str, index: usize) -> usize {
    next_grapheme_boundary(text, index)
}

fn char_offset(text: &str, start: usize, count: usize) -> usize {
    text[start..]
        .grapheme_indices(true)
        .nth(count)
        .map(|(offset, _)| start + offset)
        .unwrap_or(text.len())
}

fn line_start(text: &str, cursor: usize) -> usize {
    text[..cursor.min(text.len())]
        .rfind('\n')
        .map(|offset| offset + 1)
        .unwrap_or(0)
}

fn line_end(text: &str, cursor: usize) -> usize {
    let cursor = cursor.min(text.len());
    text[cursor..]
        .find('\n')
        .map(|offset| cursor + offset)
        .unwrap_or(text.len())
}

fn word_boundary(text: &str, index: usize, backward: bool) -> usize {
    if backward {
        let mut cursor = index;
        while cursor > 0 {
            let next = previous_boundary(text, cursor);
            if !text[next..cursor]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace())
            {
                break;
            }
            cursor = next;
        }
        while cursor > 0 {
            let next = previous_boundary(text, cursor);
            if text[next..cursor]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace())
            {
                break;
            }
            cursor = next;
        }
        cursor
    } else {
        let mut cursor = index;
        while cursor < text.len() {
            let next = next_boundary(text, cursor);
            if !text[cursor..next]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace())
            {
                break;
            }
            cursor = next;
        }
        while cursor < text.len() {
            let next = next_boundary(text, cursor);
            if text[cursor..next]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace())
            {
                break;
            }
            cursor = next;
        }
        cursor
    }
}

pub fn remember_text_field_state(initial: impl Into<String>) -> TextFieldState {
    let initial = initial.into();
    crate::remember(|| TextFieldState::new(initial))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TweenSpec {
    pub duration_ms: f32,
}

impl TweenSpec {
    pub fn new(duration_ms: f32) -> Self {
        assert!(
            duration_ms.is_finite() && duration_ms >= 0.0,
            "duration must be finite and non-negative"
        );
        Self { duration_ms }
    }
}

impl Default for TweenSpec {
    fn default() -> Self {
        Self::new(300.0)
    }
}

#[derive(Clone)]
pub struct Animatable {
    value: MutableState<f32>,
    inner: Rc<RefCell<AnimationData>>,
}

#[derive(Clone, Copy, Debug)]
struct AnimationData {
    start: f32,
    target: f32,
    elapsed_ms: f32,
    spec: TweenSpec,
    running: bool,
}

impl Animatable {
    pub fn new(initial: f32) -> Self {
        Self {
            value: mutable_state_of(initial),
            inner: Rc::new(RefCell::new(AnimationData {
                start: initial,
                target: initial,
                elapsed_ms: 0.0,
                spec: TweenSpec::default(),
                running: false,
            })),
        }
    }

    pub fn value(&self) -> f32 {
        self.value.get()
    }
    pub fn target(&self) -> f32 {
        self.inner.borrow().target
    }
    pub fn is_running(&self) -> bool {
        self.inner.borrow().running
    }

    pub fn snap_to(&self, value: f32) {
        self.value.set(value);
        let mut inner = self.inner.borrow_mut();
        inner.start = value;
        inner.target = value;
        inner.elapsed_ms = 0.0;
        inner.running = false;
    }

    pub fn animate_to(&self, target: f32, spec: TweenSpec) {
        let current = self.value();
        let mut inner = self.inner.borrow_mut();
        inner.start = current;
        inner.target = target;
        inner.elapsed_ms = 0.0;
        inner.spec = spec;
        inner.running = spec.duration_ms > 0.0 && current != target;
        if !inner.running {
            drop(inner);
            self.value.set(target);
        }
    }

    /// Advances the animation by a host-provided frame delta. Returns whether it remains active.
    pub fn advance_by(&self, delta_ms: f32) -> bool {
        let mut inner = self.inner.borrow_mut();
        if !inner.running {
            return false;
        }
        inner.elapsed_ms = (inner.elapsed_ms + delta_ms.max(0.0)).min(inner.spec.duration_ms);
        let progress = (inner.elapsed_ms / inner.spec.duration_ms).clamp(0.0, 1.0);
        let value = inner.start + (inner.target - inner.start) * progress;
        let done = progress >= 1.0;
        if done {
            inner.running = false;
        }
        drop(inner);
        self.value.set(value);
        !done
    }
}

pub fn animate_float_as_state(target: f32, spec: TweenSpec) -> Animatable {
    let animation = crate::remember(|| Animatable::new(target));
    if animation.target() != target {
        animation.animate_to(target, spec);
    }
    animation
}
