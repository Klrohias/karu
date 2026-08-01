use crate::element::{ElementKind, NodeId};
use crate::layout::{LayoutNode, Offset, Rect};
use crate::modifier::{Brush, Color};
use crate::{BasicTextLayoutEngine, CaretPosition, TextLayout, TextLayoutEngine};
use std::convert::Infallible;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageId(pub u64);

#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub color: Color,
    pub font_size: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextInputResult {
    pub handled: bool,
    pub clipboard: Option<String>,
}

impl TextInputResult {
    pub fn handled() -> Self {
        Self {
            handled: true,
            clipboard: None,
        }
    }

    pub(crate) fn merge(&mut self, other: Self) {
        self.handled |= other.handled;
        if other.clipboard.is_some() {
            self.clipboard = other.clipboard;
        }
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            color: Color::BLACK,
            font_size: 14.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RenderCommand {
    PushClip(Rect),
    PopClip,
    FillRect {
        node: NodeId,
        rect: Rect,
        color: Color,
    },
    FillBrush {
        node: NodeId,
        rect: Rect,
        brush: Brush,
    },
    StrokeRect {
        node: NodeId,
        rect: Rect,
        brush: Brush,
        width: f32,
        radius: f32,
    },
    DrawText {
        node: NodeId,
        rect: Rect,
        text: String,
        style: TextStyle,
        layout: TextLayout,
        offset: Offset,
        selection: Option<std::ops::Range<usize>>,
        cursor: Option<CaretPosition>,
        composition: Option<std::ops::Range<usize>>,
    },
    DrawSelection {
        node: NodeId,
        rect: Rect,
        color: Color,
    },
    DrawCursor {
        node: NodeId,
        rect: Rect,
        color: Color,
    },
    DrawComposition {
        node: NodeId,
        rect: Rect,
        color: Color,
    },
    DrawImage {
        node: NodeId,
        rect: Rect,
        image: ImageId,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderTree {
    pub root: LayoutNode,
}

pub trait Renderer {
    type Output;
    type Error;

    fn render(
        &mut self,
        tree: &RenderTree,
        commands: &[RenderCommand],
    ) -> Result<Self::Output, Self::Error>;
}

#[derive(Clone, Debug, Default)]
pub struct HeadlessRenderer;

#[derive(Clone, Debug, PartialEq)]
pub struct HeadlessOutput {
    pub tree: RenderTree,
    pub commands: Vec<RenderCommand>,
}

impl Renderer for HeadlessRenderer {
    type Output = HeadlessOutput;
    type Error = Infallible;

    fn render(
        &mut self,
        tree: &RenderTree,
        commands: &[RenderCommand],
    ) -> Result<Self::Output, Self::Error> {
        Ok(HeadlessOutput {
            tree: tree.clone(),
            commands: commands.to_vec(),
        })
    }
}

pub fn commands_for_tree(root: &LayoutNode) -> Vec<RenderCommand> {
    commands_for_tree_with_engine(root, &BasicTextLayoutEngine)
}

pub fn commands_for_tree_with_engine(
    root: &LayoutNode,
    engine: &dyn TextLayoutEngine,
) -> Vec<RenderCommand> {
    let mut commands = Vec::new();
    push_commands(root, &mut commands, engine);
    commands
}

fn push_commands(
    node: &LayoutNode,
    commands: &mut Vec<RenderCommand>,
    engine: &dyn TextLayoutEngine,
) {
    if node.clip {
        commands.push(RenderCommand::PushClip(node.bounds));
    }

    if let Some(brush) = &node.background_brush {
        match brush {
            Brush::Solid(color) => commands.push(RenderCommand::FillRect {
                node: node.id,
                rect: node.bounds,
                color: *color,
            }),
            _ => commands.push(RenderCommand::FillBrush {
                node: node.id,
                rect: node.bounds,
                brush: brush.clone(),
            }),
        }
    } else if let Some(color) = node.background {
        commands.push(RenderCommand::FillRect {
            node: node.id,
            rect: node.bounds,
            color,
        });
    }

    if let Some(border) = &node.border {
        commands.push(RenderCommand::StrokeRect {
            node: node.id,
            rect: node.bounds,
            brush: border.brush.clone(),
            width: border.width,
            radius: node.border_radius,
        });
    }

    if let ElementKind::Text(text) = &node.kind {
        let layout = engine.layout(
            text,
            node.font_size.unwrap_or(14.0),
            node.bounds.size.width,
            node.text_wrap,
        );
        if let Some(value) = &node.text_field {
            for selection in layout.selection_rects(value.selection.clone()) {
                commands.push(RenderCommand::DrawSelection {
                    node: node.id,
                    rect: translate_text_rect(node, selection),
                    color: Color::rgba(0.25, 0.5, 0.95, 0.35),
                });
            }
            if node.text_focused {
                let caret = layout.caret(node.text_cursor);
                commands.push(RenderCommand::DrawCursor {
                    node: node.id,
                    rect: translate_text_rect(
                        node,
                        Rect::new(caret.position.x, caret.position.y, 1.0, caret.height),
                    ),
                    color: Color::BLACK,
                });
            }
            if let Some(composition) = &value.composition {
                for rect in layout.selection_rects(composition.clone()) {
                    commands.push(RenderCommand::DrawComposition {
                        node: node.id,
                        rect: translate_text_rect(
                            node,
                            Rect::new(
                                rect.origin.x,
                                rect.origin.y + rect.size.height - 1.0,
                                rect.size.width.max(1.0),
                                1.0,
                            ),
                        ),
                        color: Color::BLACK,
                    });
                }
            }
            let cursor = node.text_focused.then(|| layout.caret(node.text_cursor));
            commands.push(RenderCommand::DrawText {
                node: node.id,
                rect: node.bounds,
                text: text.clone(),
                style: TextStyle {
                    color: node.text_color.unwrap_or(Color::BLACK),
                    font_size: node.font_size.unwrap_or(14.0),
                },
                layout,
                offset: Offset::new(
                    node.bounds.origin.x - node.text_scroll.x,
                    node.bounds.origin.y - node.text_scroll.y,
                ),
                selection: Some(value.selection.clone()),
                cursor,
                composition: value.composition.clone(),
            });
        } else {
            commands.push(RenderCommand::DrawText {
                node: node.id,
                rect: node.bounds,
                text: text.clone(),
                style: TextStyle {
                    color: node.text_color.unwrap_or(Color::BLACK),
                    font_size: node.font_size.unwrap_or(14.0),
                },
                layout,
                offset: node.bounds.origin,
                selection: None,
                cursor: None,
                composition: None,
            });
        }
    }

    for child in &node.children {
        push_commands(child, commands, engine);
    }

    if node.clip {
        commands.push(RenderCommand::PopClip);
    }
}

fn translate_text_rect(node: &LayoutNode, rect: Rect) -> Rect {
    Rect::new(
        node.bounds.origin.x + rect.origin.x - node.text_scroll.x,
        node.bounds.origin.y + rect.origin.y - node.text_scroll.y,
        rect.size.width,
        rect.size.height,
    )
}
