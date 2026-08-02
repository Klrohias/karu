use crate::element::NodeId;
use crate::layout::{LayoutNode, Offset};
use crate::renderer::{TextInputResult, TextLayoutEngine};
use crate::text_layout::TextWrap;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PointerKind {
    Mouse,
    Touch { id: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PointerPhase {
    Down,
    Move,
    Up,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerEvent {
    pub kind: PointerKind,
    pub phase: PointerPhase,
    pub position: Offset,
    pub primary: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollEvent {
    pub position: Offset,
    pub delta: Offset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Backspace,
    Delete,
    Enter,
    Tab,
    Escape,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct KeyModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub logo: bool,
}

impl KeyModifiers {
    pub fn command(self) -> bool {
        self.ctrl || self.logo
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextEditCommand {
    SelectAll,
    Copy,
    Cut,
    Paste,
    Undo,
    Redo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyEvent {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
    pub repeat: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TextInputEvent {
    Insert {
        position: Offset,
        text: String,
    },
    Backspace {
        position: Offset,
    },
    Key {
        position: Offset,
        event: KeyEvent,
    },
    Command {
        position: Offset,
        command: TextEditCommand,
    },
    Paste {
        position: Offset,
        text: String,
    },
    CompositionStart {
        position: Offset,
    },
    CompositionUpdate {
        position: Offset,
        text: String,
    },
    CompositionCommit {
        position: Offset,
        text: String,
    },
    CompositionEnd {
        position: Offset,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InteractionState {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PointerDispatchResult {
    pub handled: bool,
    pub interaction_changed: bool,
    pub state_changed: bool,
}

type PointerCallback = Rc<RefCell<dyn FnMut(PointerEvent)>>;
type TextPointerCallback =
    Rc<RefCell<dyn FnMut(TextPointerEvent, &mut dyn TextLayoutEngine) -> bool>>;
type ClickCallback = Rc<RefCell<dyn FnMut()>>;
type ScrollCallback = Rc<RefCell<dyn FnMut(ScrollEvent) -> bool>>;
type TextInputCallback = Rc<
    RefCell<
        dyn FnMut(TextInputEvent, TextInputContext, &mut dyn TextLayoutEngine) -> TextInputResult,
    >,
>;

#[derive(Clone, Debug, PartialEq)]
pub struct TextInputContext {
    pub text: String,
    pub font_size: f32,
    pub max_width: f32,
    pub wrap: TextWrap,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextPointerEvent {
    pub event: PointerEvent,
    pub local_position: Offset,
    pub bounds: crate::Rect,
    pub was_focused: bool,
    pub context: TextInputContext,
    pub scroll_offset: Offset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum PointerId {
    Mouse,
    Touch(u64),
}

impl From<PointerKind> for PointerId {
    fn from(kind: PointerKind) -> Self {
        match kind {
            PointerKind::Mouse => Self::Mouse,
            PointerKind::Touch { id } => Self::Touch(id),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PointerRecord {
    target: Option<NodeId>,
    hovered: Option<NodeId>,
}

#[derive(Default)]
pub struct EventRegistry {
    pointer_handlers: HashMap<NodeId, Vec<PointerCallback>>,
    text_pointer_handlers: HashMap<NodeId, Vec<TextPointerCallback>>,
    click_handlers: HashMap<NodeId, Vec<ClickCallback>>,
    scroll_handlers: HashMap<NodeId, Vec<ScrollCallback>>,
    text_input_handlers: HashMap<NodeId, Vec<TextInputCallback>>,
    active_text_input: Option<NodeId>,
    interactions: HashMap<NodeId, InteractionState>,
    pointers: HashMap<PointerId, PointerRecord>,
}

impl fmt::Debug for EventRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventRegistry")
            .field("pointer_handlers", &self.pointer_handlers.len())
            .field("click_handlers", &self.click_handlers.len())
            .field("interactions", &self.interactions)
            .finish()
    }
}

impl EventRegistry {
    pub(crate) fn begin_composition(&mut self) {
        self.pointer_handlers.clear();
        self.text_pointer_handlers.clear();
        self.click_handlers.clear();
        self.scroll_handlers.clear();
        self.text_input_handlers.clear();
    }

    pub(crate) fn register_pointer_handler(
        &mut self,
        node: NodeId,
        handler: impl FnMut(PointerEvent) + 'static,
    ) {
        self.pointer_handlers
            .entry(node)
            .or_default()
            .push(Rc::new(RefCell::new(handler)));
    }

    pub(crate) fn register_click_handler(&mut self, node: NodeId, handler: impl FnMut() + 'static) {
        self.click_handlers
            .entry(node)
            .or_default()
            .push(Rc::new(RefCell::new(handler)));
    }

    pub(crate) fn register_text_pointer_handler(
        &mut self,
        node: NodeId,
        handler: impl FnMut(TextPointerEvent, &mut dyn TextLayoutEngine) -> bool + 'static,
    ) {
        self.text_pointer_handlers
            .entry(node)
            .or_default()
            .push(Rc::new(RefCell::new(handler)));
    }

    pub(crate) fn register_scroll_handler(
        &mut self,
        node: NodeId,
        handler: impl FnMut(ScrollEvent) -> bool + 'static,
    ) {
        self.scroll_handlers
            .entry(node)
            .or_default()
            .push(Rc::new(RefCell::new(handler)));
    }

    pub(crate) fn register_text_input_handler(
        &mut self,
        node: NodeId,
        handler: impl FnMut(
            TextInputEvent,
            TextInputContext,
            &mut dyn TextLayoutEngine,
        ) -> TextInputResult
        + 'static,
    ) {
        self.text_input_handlers
            .entry(node)
            .or_default()
            .push(Rc::new(RefCell::new(handler)));
    }

    pub(crate) fn interaction(&self, node: NodeId) -> InteractionState {
        let mut interaction = self.interactions.get(&node).copied().unwrap_or_default();
        interaction.focused = self.active_text_input == Some(node);
        interaction
    }

    pub(crate) fn dispatch(
        &mut self,
        tree: &LayoutNode,
        event: PointerEvent,
        layout: &mut dyn TextLayoutEngine,
    ) -> PointerDispatchResult {
        let pointer = PointerId::from(event.kind);
        let hit = hit_test(tree, event.position);
        let previous = self.pointers.get(&pointer).copied();
        let mut handled = hit.is_some();
        let mut interaction_changed =
            previous.map_or(hit.is_some(), |record| record.hovered != hit);
        let mut state_changed = false;

        match event.phase {
            PointerPhase::Down => {
                if event.primary {
                    let was_focused = self.active_text_input == hit;
                    let next_active_text_input =
                        hit.filter(|node| self.text_input_handlers.contains_key(node));
                    interaction_changed |= self.active_text_input != next_active_text_input;
                    self.active_text_input = next_active_text_input;
                    self.pointers.insert(
                        pointer,
                        PointerRecord {
                            target: hit,
                            hovered: hit,
                        },
                    );
                    if let Some(node) = hit {
                        interaction_changed |= !self
                            .interactions
                            .get(&node)
                            .is_some_and(|state| state.pressed);
                        self.set_pressed(node, true);
                        state_changed |= self.invoke_text_pointer_handler(
                            tree,
                            node,
                            event,
                            was_focused,
                            layout,
                        );
                    }
                }
                handled |= self.invoke_pointer_handlers(hit, event);
            }
            PointerPhase::Move => {
                let record = self.pointers.entry(pointer).or_insert(PointerRecord {
                    target: None,
                    hovered: None,
                });
                record.hovered = hit;
                if let Some(node) = record.target {
                    let pressed = hit == Some(node);
                    interaction_changed |= self
                        .interactions
                        .get(&node)
                        .is_some_and(|state| state.pressed)
                        != pressed;
                    self.set_pressed(node, pressed);
                    state_changed |=
                        self.invoke_text_pointer_handler(tree, node, event, true, layout);
                    handled = true;
                }
                handled |= self.invoke_pointer_handlers(hit, event);
            }
            PointerPhase::Up => {
                let record = self.pointers.remove(&pointer).or(previous);
                if let Some(record) = record
                    && let Some(node) = record.target
                {
                    interaction_changed |= self
                        .interactions
                        .get(&node)
                        .is_some_and(|state| state.pressed);
                    self.set_pressed(node, false);
                    state_changed |=
                        self.invoke_text_pointer_handler(tree, node, event, true, layout);
                    handled = true;
                    if event.primary
                        && hit == Some(node)
                        && let Some(handlers) = self.click_handlers.get(&node)
                    {
                        for handler in handlers {
                            (handler.borrow_mut())();
                            handled = true;
                        }
                    }
                }
                handled |= self.invoke_pointer_handlers(hit, event);
            }
            PointerPhase::Cancel => {
                if let Some(record) = self.pointers.remove(&pointer).or(previous)
                    && let Some(node) = record.target
                {
                    interaction_changed |= self
                        .interactions
                        .get(&node)
                        .is_some_and(|state| state.pressed);
                    self.set_pressed(node, false);
                }
                handled |= self.invoke_pointer_handlers(hit, event);
            }
        }

        self.recompute_hover_states();
        PointerDispatchResult {
            handled,
            interaction_changed,
            state_changed,
        }
    }

    pub(crate) fn dispatch_scroll(&mut self, tree: &LayoutNode, event: ScrollEvent) -> bool {
        let mut path = Vec::new();
        if !hit_path(tree, event.position, &mut path) {
            return false;
        }
        for node in path.into_iter().rev() {
            let Some(handlers) = self.scroll_handlers.get(&node) else {
                continue;
            };
            for handler in handlers {
                if (handler.borrow_mut())(event) {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn dispatch_text_input(
        &mut self,
        tree: &LayoutNode,
        event: TextInputEvent,
        layout: &mut dyn TextLayoutEngine,
    ) -> TextInputResult {
        if let Some(node) = self.active_text_input
            && let Some(handlers) = self.text_input_handlers.get(&node)
        {
            let Some(context) = text_context(tree, node) else {
                return TextInputResult::default();
            };
            let mut result = TextInputResult::default();
            for handler in handlers {
                result.merge((handler.borrow_mut())(
                    event.clone(),
                    context.clone(),
                    layout,
                ));
            }
            result.handled = true;
            return result;
        }

        let position = match &event {
            TextInputEvent::Insert { position, .. } | TextInputEvent::Backspace { position } => {
                *position
            }
            TextInputEvent::Key { position, .. }
            | TextInputEvent::Command { position, .. }
            | TextInputEvent::Paste { position, .. }
            | TextInputEvent::CompositionStart { position }
            | TextInputEvent::CompositionUpdate { position, .. }
            | TextInputEvent::CompositionCommit { position, .. }
            | TextInputEvent::CompositionEnd { position } => *position,
        };
        let Some(node) = hit_test(tree, position) else {
            return TextInputResult::default();
        };
        let Some(handlers) = self.text_input_handlers.get(&node) else {
            return TextInputResult::default();
        };
        let mut result = TextInputResult::default();
        let Some(context) = text_context(tree, node) else {
            return TextInputResult::default();
        };
        for handler in handlers {
            result.merge((handler.borrow_mut())(
                event.clone(),
                context.clone(),
                layout,
            ));
        }
        result.handled = true;
        result
    }

    fn invoke_text_pointer_handler(
        &mut self,
        tree: &LayoutNode,
        node: NodeId,
        event: PointerEvent,
        was_focused: bool,
        layout: &mut dyn TextLayoutEngine,
    ) -> bool {
        let Some(handlers) = self.text_pointer_handlers.get(&node) else {
            return false;
        };
        let Some(node_layout) = tree.find(node) else {
            return false;
        };
        let Some(context) = text_context(tree, node) else {
            return false;
        };
        let text_event = TextPointerEvent {
            event,
            local_position: Offset::new(
                event.position.x - node_layout.text_origin.x + node_layout.text_scroll.x,
                event.position.y - node_layout.text_origin.y + node_layout.text_scroll.y,
            ),
            bounds: node_layout.bounds,
            was_focused,
            context,
            scroll_offset: node_layout.text_scroll,
        };
        handlers.iter().fold(false, |changed, handler| {
            changed | (handler.borrow_mut())(text_event.clone(), layout)
        })
    }

    fn invoke_pointer_handlers(&mut self, node: Option<NodeId>, event: PointerEvent) -> bool {
        let Some(node) = node else {
            return false;
        };

        let Some(handlers) = self.pointer_handlers.get(&node) else {
            return false;
        };

        for handler in handlers {
            (handler.borrow_mut())(event);
        }
        true
    }

    fn set_pressed(&mut self, node: NodeId, pressed: bool) {
        self.interactions.entry(node).or_default().pressed = pressed;
    }

    fn recompute_hover_states(&mut self) {
        for state in self.interactions.values_mut() {
            state.hovered = false;
        }
        for record in self.pointers.values() {
            if let Some(node) = record.hovered {
                self.interactions.entry(node).or_default().hovered = true;
            }
        }
    }
}

fn text_context(tree: &LayoutNode, node: NodeId) -> Option<TextInputContext> {
    let node_data = tree.find(node)?;
    let text = match &node_data.kind {
        crate::ElementKind::Text(text) => text.clone(),
        _ => return None,
    };
    Some(TextInputContext {
        text,
        font_size: node_data.font_size.unwrap_or(14.0),
        max_width: node_data.text_viewport.size.width,
        wrap: node_data.text_wrap,
    })
}

fn hit_test(node: &LayoutNode, position: Offset) -> Option<NodeId> {
    if !node.bounds.contains(position) {
        return None;
    }

    for child in node.children.iter().rev() {
        if let Some(id) = hit_test(child, position) {
            return Some(id);
        }
    }

    if node.interactive {
        Some(node.id)
    } else {
        None
    }
}

fn hit_path(node: &LayoutNode, position: Offset, path: &mut Vec<NodeId>) -> bool {
    if !node.bounds.contains(position) {
        return false;
    }
    if node.interactive {
        path.push(node.id);
    }
    for child in node.children.iter().rev() {
        if hit_path(child, position, path) {
            return true;
        }
    }
    true
}
