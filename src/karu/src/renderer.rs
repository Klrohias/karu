use crate::element::{ElementKind, NodeId};
use crate::layout::{LayoutNode, Rect};
use crate::modifier::Color;
use std::convert::Infallible;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageId(pub u64);

#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub color: Color,
    pub font_size: f32,
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
    DrawText {
        node: NodeId,
        rect: Rect,
        text: String,
        style: TextStyle,
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
    let mut commands = Vec::new();
    push_commands(root, &mut commands);
    commands
}

fn push_commands(node: &LayoutNode, commands: &mut Vec<RenderCommand>) {
    if node.clip {
        commands.push(RenderCommand::PushClip(node.bounds));
    }

    if let Some(color) = node.background {
        commands.push(RenderCommand::FillRect {
            node: node.id,
            rect: node.bounds,
            color,
        });
    }

    if let ElementKind::Text(text) = &node.kind {
        commands.push(RenderCommand::DrawText {
            node: node.id,
            rect: node.bounds,
            text: text.clone(),
            style: TextStyle::default(),
        });
    }

    for child in &node.children {
        push_commands(child, commands);
    }

    if node.clip {
        commands.push(RenderCommand::PopClip);
    }
}
