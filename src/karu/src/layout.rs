use crate::element::{Element, ElementKind, NodeId};
use crate::modifier::{Color, ModifierData};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Dp(pub f32);

impl From<f32> for Dp {
    fn from(value: f32) -> Self {
        Self(value)
    }
}

impl From<u32> for Dp {
    fn from(value: u32) -> Self {
        Self(value as f32)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };

    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Offset {
    pub x: f32,
    pub y: f32,
}

impl Offset {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub origin: Offset,
    pub size: Size,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            origin: Offset::new(x, y),
            size: Size::new(width, height),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Constraints {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
}

impl Constraints {
    pub fn tight(size: Size) -> Self {
        Self {
            min_width: size.width,
            max_width: size.width,
            min_height: size.height,
            max_height: size.height,
        }
    }

    pub fn loose(max_width: f32, max_height: f32) -> Self {
        Self {
            min_width: 0.0,
            max_width,
            min_height: 0.0,
            max_height,
        }
    }

    pub fn constrain(&self, size: Size) -> Size {
        Size {
            width: size.width.clamp(self.min_width, self.max_width),
            height: size.height.clamp(self.min_height, self.max_height),
        }
    }
}

impl Default for Constraints {
    fn default() -> Self {
        Self::loose(f32::INFINITY, f32::INFINITY)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutNode {
    pub id: NodeId,
    pub kind: ElementKind,
    pub bounds: Rect,
    pub background: Option<Color>,
    pub clip: bool,
    pub clickable: bool,
    pub children: Vec<LayoutNode>,
}

impl LayoutNode {
    pub fn find(&self, id: NodeId) -> Option<&LayoutNode> {
        if self.id == id {
            return Some(self);
        }

        self.children.iter().find_map(|child| child.find(id))
    }
}

pub fn layout_tree(element: &Element, constraints: Constraints) -> LayoutNode {
    layout_element(element, Offset::ZERO, constraints)
}

fn layout_element(element: &Element, origin: Offset, constraints: Constraints) -> LayoutNode {
    let data = element.modifier.data();
    let child_constraints = Constraints {
        min_width: 0.0,
        max_width: constraints.max_width - data.padding.horizontal(),
        min_height: 0.0,
        max_height: constraints.max_height - data.padding.vertical(),
    };

    let child_origin = Offset::new(origin.x + data.padding.left, origin.y + data.padding.top);

    let mut children = match element.kind {
        ElementKind::Root | ElementKind::Column | ElementKind::Component(_) => {
            layout_vertical_children(&element.children, child_origin, child_constraints)
        }
        ElementKind::Text(_) | ElementKind::Custom(_) => Vec::new(),
    };

    let content_size = intrinsic_size(element, &children);
    let size = apply_modifier_size(data, content_size, constraints);

    if matches!(
        element.kind,
        ElementKind::Root | ElementKind::Column | ElementKind::Component(_)
    ) {
        if data.fill_max_width {
            let available = constraints.max_width;
            if available.is_finite() {
                children = layout_vertical_children(
                    &element.children,
                    child_origin,
                    Constraints {
                        max_width: available - data.padding.horizontal(),
                        ..child_constraints
                    },
                );
            }
        }
    }

    LayoutNode {
        id: element.id,
        kind: element.kind.clone(),
        bounds: Rect { origin, size },
        background: data.background,
        clip: data.clip,
        clickable: data.clickable,
        children,
    }
}

fn layout_vertical_children(
    children: &[Element],
    origin: Offset,
    constraints: Constraints,
) -> Vec<LayoutNode> {
    let mut cursor_y = origin.y;
    let mut nodes = Vec::with_capacity(children.len());

    for child in children {
        let node = layout_element(child, Offset::new(origin.x, cursor_y), constraints);
        cursor_y += node.bounds.size.height;
        nodes.push(node);
    }

    nodes
}

fn intrinsic_size(element: &Element, children: &[LayoutNode]) -> Size {
    match &element.kind {
        ElementKind::Text(text) => Size::new(text.len() as f32 * 8.0, 20.0),
        ElementKind::Custom(_) => Size::ZERO,
        ElementKind::Root | ElementKind::Column | ElementKind::Component(_) => {
            let width = children
                .iter()
                .map(|child| child.bounds.size.width)
                .fold(0.0, f32::max);
            let height = children.iter().map(|child| child.bounds.size.height).sum();
            Size::new(width, height)
        }
    }
}

fn apply_modifier_size(data: ModifierData, content_size: Size, constraints: Constraints) -> Size {
    let mut size = content_size;

    if data.fill_max_width && constraints.max_width.is_finite() {
        size.width = constraints.max_width;
    }

    if let Some(width) = data.width {
        size.width = width;
    }

    if let Some(height) = data.height {
        size.height = height;
    }

    size.width += data.padding.horizontal();
    size.height += data.padding.vertical();
    constraints.constrain(size)
}
