use crate::TextFieldValue;
use crate::element::{Element, ElementKind, NodeId};
use crate::event::{EventRegistry, InteractionState};
use crate::modifier::{
    Alignment, Arrangement, BorderData, Brush, Color, CrossAxisAlignment, ModifierData, ScrollAxis,
    Semantics,
};
use crate::renderer::TextLayoutEngine;
use crate::text_layout::TextWrap;

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

    pub fn contains(self, point: Offset) -> bool {
        point.x >= self.origin.x
            && point.y >= self.origin.y
            && point.x <= self.origin.x + self.size.width
            && point.y <= self.origin.y + self.size.height
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
    pub background_brush: Option<Brush>,
    pub text_color: Option<Color>,
    pub font_size: Option<f32>,
    pub border: Option<BorderData>,
    pub border_radius: f32,
    pub clip: bool,
    pub clickable: bool,
    pub interactive: bool,
    pub interaction: InteractionState,
    pub alignment: Option<Alignment>,
    pub semantics: Semantics,
    pub text_field: Option<TextFieldValue>,
    pub text_focused: bool,
    pub text_cursor: usize,
    pub text_origin: Offset,
    pub text_viewport: Rect,
    pub text_wrap: TextWrap,
    pub text_scroll: Offset,
    pub children: Vec<LayoutNode>,
}

impl LayoutNode {
    pub fn find(&self, id: NodeId) -> Option<&LayoutNode> {
        if self.id == id {
            return Some(self);
        }

        self.children.iter().find_map(|child| child.find(id))
    }

    pub fn find_by_test_tag(&self, tag: &str) -> Option<&LayoutNode> {
        if self.semantics.test_tag.as_deref() == Some(tag) {
            return Some(self);
        }
        self.children
            .iter()
            .find_map(|child| child.find_by_test_tag(tag))
    }

    pub fn find_by_content_description(&self, description: &str) -> Option<&LayoutNode> {
        if self.semantics.content_description.as_deref() == Some(description) {
            return Some(self);
        }
        self.children
            .iter()
            .find_map(|child| child.find_by_content_description(description))
    }
}

#[allow(dead_code)]
pub fn layout_tree(element: &Element, constraints: Constraints) -> LayoutNode {
    let mut layout = crate::HeadlessTextLayout;
    layout_element(
        element,
        Offset::ZERO,
        constraints,
        &|_| InteractionState::default(),
        &mut layout,
    )
}

pub(crate) fn layout_tree_with_events(
    element: &Element,
    constraints: Constraints,
    events: &EventRegistry,
    layout: &mut dyn TextLayoutEngine,
) -> LayoutNode {
    layout_element(
        element,
        Offset::ZERO,
        constraints,
        &|node| events.interaction(node),
        layout,
    )
}

fn layout_element(
    element: &Element,
    origin: Offset,
    constraints: Constraints,
    interactions: &dyn Fn(NodeId) -> InteractionState,
    layout: &mut dyn TextLayoutEngine,
) -> LayoutNode {
    let interaction = interactions(element.id);
    let data = element.modifier.data_for(interaction);
    let child_constraints = Constraints {
        min_width: 0.0,
        max_width: (constraints.max_width - data.padding.horizontal()).max(0.0),
        min_height: 0.0,
        max_height: (constraints.max_height - data.padding.vertical()).max(0.0),
    };

    let child_origin = Offset::new(origin.x + data.padding.left, origin.y + data.padding.top);

    let layout_constraints = if data.width.is_some() || data.height.is_some() {
        Constraints {
            max_width: data
                .width
                .map(|width| (width - data.padding.horizontal()).max(0.0))
                .unwrap_or(child_constraints.max_width),
            max_height: data
                .height
                .map(|height| (height - data.padding.vertical()).max(0.0))
                .unwrap_or(child_constraints.max_height),
            ..child_constraints
        }
    } else {
        child_constraints
    };

    let content_constraints = match data.scroll.as_ref().map(|scroll| scroll.axis) {
        Some(ScrollAxis::Horizontal) => Constraints {
            max_width: f32::INFINITY,
            ..layout_constraints
        },
        Some(ScrollAxis::Vertical) => Constraints {
            max_height: f32::INFINITY,
            ..layout_constraints
        },
        None => layout_constraints,
    };

    let mut children = match element.kind {
        ElementKind::Root | ElementKind::Column | ElementKind::Component(_) => {
            layout_vertical_children(
                &element.children,
                child_origin,
                content_constraints,
                interactions,
                data.vertical_arrangement,
                layout,
            )
        }
        ElementKind::Row => layout_horizontal_children(
            &element.children,
            child_origin,
            content_constraints,
            interactions,
            data.horizontal_arrangement,
            layout,
        ),
        ElementKind::Box => layout_stack_children(
            &element.children,
            child_origin,
            content_constraints,
            interactions,
            layout,
        ),
        ElementKind::Text(_) | ElementKind::Custom(_) => Vec::new(),
    };

    let content_size = intrinsic_size(element, &children, child_constraints, layout);
    let size = apply_modifier_size(&data, content_size, constraints);

    let text_size = match &element.kind {
        ElementKind::Text(text) => {
            let max_width = (size.width - data.padding.horizontal()).max(0.0);
            let mut text_size = layout.measure_text(
                text,
                data.font_size.unwrap_or(14.0),
                max_width,
                data.text_wrap,
            );
            if let Some(min_lines) = data.text_min_lines {
                let line_height = data.font_size.unwrap_or(14.0) * (20.0 / 14.0);
                text_size.height = text_size.height.max(min_lines as f32 * line_height);
            }
            if let Some(max_lines) = data.text_max_lines {
                let line_height = data.font_size.unwrap_or(14.0) * (20.0 / 14.0);
                text_size.height = text_size.height.min(max_lines as f32 * line_height);
            }
            Some(text_size)
        }
        _ => None,
    };
    let text_scroll =
        if let (Some(text_size), Some(state)) = (text_size, element.modifier.text_field_state()) {
            let text = match &element.kind {
                ElementKind::Text(text) => text,
                _ => unreachable!(),
            };
            state.ensure_cursor_visible(
                text,
                data.font_size.unwrap_or(14.0),
                (size.width - data.padding.horizontal()).max(0.0),
                data.text_wrap,
                text_size,
                Size::new(
                    (size.width - data.padding.horizontal()).max(0.0),
                    (size.height - data.padding.vertical()).max(0.0),
                ),
                layout,
            );
            state.scroll_offset()
        } else {
            Offset::ZERO
        };

    match element.kind {
        ElementKind::Column => align_column_children(
            &mut children,
            origin,
            size,
            data.padding,
            data.column_cross_alignment,
        ),
        ElementKind::Row => align_row_children(
            &mut children,
            origin,
            size,
            data.padding,
            data.row_cross_alignment,
        ),
        _ => {}
    }

    if let Some(scroll) = &data.scroll {
        let viewport = match scroll.axis {
            ScrollAxis::Horizontal => (size.width - data.padding.horizontal()).max(0.0),
            ScrollAxis::Vertical => (size.height - data.padding.vertical()).max(0.0),
        };
        let content = match scroll.axis {
            ScrollAxis::Horizontal => content_size.width,
            ScrollAxis::Vertical => content_size.height,
        };
        scroll.state.set_max_value(content - viewport);
        let offset = scroll.state.value();
        for child in &mut children {
            match scroll.axis {
                ScrollAxis::Horizontal => translate_node(child, -offset, 0.0),
                ScrollAxis::Vertical => translate_node(child, 0.0, -offset),
            }
        }
    }

    if matches!(
        element.kind,
        ElementKind::Root | ElementKind::Column | ElementKind::Row | ElementKind::Component(_)
    ) && data.fill_max_width
    {
        let available = constraints.max_width;
        if available.is_finite() {
            children = match element.kind {
                ElementKind::Row => layout_horizontal_children(
                    &element.children,
                    child_origin,
                    Constraints {
                        max_width: (available - data.padding.horizontal()).max(0.0),
                        ..child_constraints
                    },
                    interactions,
                    data.horizontal_arrangement,
                    layout,
                ),
                _ => layout_vertical_children(
                    &element.children,
                    child_origin,
                    Constraints {
                        max_width: (available - data.padding.horizontal()).max(0.0),
                        ..child_constraints
                    },
                    interactions,
                    data.vertical_arrangement,
                    layout,
                ),
            };
        }
    }

    if matches!(element.kind, ElementKind::Box) {
        for child in &mut children {
            let alignment = child_alignment(element, child, data.alignment.unwrap_or_default());
            let next = aligned_origin(child.bounds, origin, size, data.padding, alignment);
            let previous = child.bounds.origin;
            translate_node(child, next.x - previous.x, next.y - previous.y);
        }
    }

    let text_origin = Offset::new(origin.x + data.padding.left, origin.y + data.padding.top);
    let text_viewport = Rect::new(
        text_origin.x,
        text_origin.y,
        (size.width - data.padding.horizontal()).max(0.0),
        (size.height - data.padding.vertical()).max(0.0),
    );

    LayoutNode {
        id: element.id,
        kind: element.kind.clone(),
        bounds: Rect { origin, size },
        background: data.background,
        background_brush: data.background_brush,
        text_color: data.text_color,
        font_size: data.font_size,
        border: data.border,
        border_radius: data.border_radius,
        clip: data.clip,
        clickable: data.clickable,
        interactive: data.interactive,
        interaction,
        alignment: data.alignment,
        semantics: data.semantics,
        text_field: element.modifier.text_field_value(),
        text_focused: interaction.focused,
        text_cursor: element
            .modifier
            .text_field_state()
            .map(|state| state.active_endpoint())
            .unwrap_or(0),
        text_origin,
        text_viewport,
        text_wrap: data.text_wrap,
        text_scroll,
        children,
    }
}

fn layout_vertical_children(
    children: &[Element],
    origin: Offset,
    constraints: Constraints,
    interactions: &dyn Fn(NodeId) -> InteractionState,
    arrangement: Arrangement,
    layout: &mut dyn TextLayoutEngine,
) -> Vec<LayoutNode> {
    layout_weighted_children(
        children,
        origin,
        constraints,
        interactions,
        arrangement,
        Axis::Vertical,
        layout,
    )
}

fn layout_horizontal_children(
    children: &[Element],
    origin: Offset,
    constraints: Constraints,
    interactions: &dyn Fn(NodeId) -> InteractionState,
    arrangement: Arrangement,
    layout: &mut dyn TextLayoutEngine,
) -> Vec<LayoutNode> {
    layout_weighted_children(
        children,
        origin,
        constraints,
        interactions,
        arrangement,
        Axis::Horizontal,
        layout,
    )
}

#[derive(Clone, Copy)]
enum Axis {
    Horizontal,
    Vertical,
}

fn layout_weighted_children(
    children: &[Element],
    origin: Offset,
    constraints: Constraints,
    interactions: &dyn Fn(NodeId) -> InteractionState,
    arrangement: Arrangement,
    axis: Axis,
    layout: &mut dyn TextLayoutEngine,
) -> Vec<LayoutNode> {
    let weights = children
        .iter()
        .map(|child| child.modifier.data().weight)
        .collect::<Vec<_>>();
    let total_weight = weights
        .iter()
        .flatten()
        .map(|weight| weight.value)
        .sum::<f32>();
    let mut nodes = children
        .iter()
        .zip(&weights)
        .map(|(child, weight)| {
            weight
                .is_none()
                .then(|| layout_element(child, origin, constraints, interactions, layout))
        })
        .collect::<Vec<_>>();
    let occupied = nodes
        .iter()
        .flatten()
        .map(|node| match axis {
            Axis::Horizontal => node.bounds.size.width,
            Axis::Vertical => node.bounds.size.height,
        })
        .sum::<f32>();
    let available = match axis {
        Axis::Horizontal => constraints.max_width,
        Axis::Vertical => constraints.max_height,
    };
    let remaining = if available.is_finite() {
        (available - occupied).max(0.0)
    } else {
        0.0
    };

    for ((child, weight), node) in children.iter().zip(&weights).zip(&mut nodes) {
        let Some(weight) = weight else { continue };
        let share = remaining * weight.value / total_weight;
        let child_constraints = match axis {
            Axis::Horizontal => Constraints {
                min_width: if weight.fill { share } else { 0.0 },
                max_width: share,
                ..constraints
            },
            Axis::Vertical => Constraints {
                min_height: if weight.fill { share } else { 0.0 },
                max_height: share,
                ..constraints
            },
        };
        *node = Some(layout_element(
            child,
            origin,
            child_constraints,
            interactions,
            layout,
        ));
    }
    let mut nodes = nodes.into_iter().map(Option::unwrap).collect::<Vec<_>>();
    place_arranged_children(&mut nodes, origin, available, arrangement, axis);
    nodes
}

fn place_arranged_children(
    nodes: &mut [LayoutNode],
    origin: Offset,
    available: f32,
    arrangement: Arrangement,
    axis: Axis,
) {
    let occupied = nodes
        .iter()
        .map(|node| match axis {
            Axis::Horizontal => node.bounds.size.width,
            Axis::Vertical => node.bounds.size.height,
        })
        .sum::<f32>();
    let free = if available.is_finite() {
        (available - occupied).max(0.0)
    } else {
        0.0
    };
    let count = nodes.len();
    let (mut cursor, gap) = match arrangement {
        Arrangement::Start => (0.0, 0.0),
        Arrangement::Center => (free / 2.0, 0.0),
        Arrangement::End => (free, 0.0),
        Arrangement::SpaceBetween if count > 1 => (0.0, free / (count - 1) as f32),
        Arrangement::SpaceAround if count > 0 => (free / (count * 2) as f32, free / count as f32),
        Arrangement::SpaceEvenly if count > 0 => {
            (free / (count + 1) as f32, free / (count + 1) as f32)
        }
        _ => (0.0, 0.0),
    };
    for node in nodes {
        let previous = node.bounds.origin;
        let next = match axis {
            Axis::Horizontal => Offset::new(origin.x + cursor, origin.y),
            Axis::Vertical => Offset::new(origin.x, origin.y + cursor),
        };
        translate_node(node, next.x - previous.x, next.y - previous.y);
        cursor += match axis {
            Axis::Horizontal => node.bounds.size.width,
            Axis::Vertical => node.bounds.size.height,
        } + gap;
    }
}

fn translate_node(node: &mut LayoutNode, dx: f32, dy: f32) {
    node.bounds.origin.x += dx;
    node.bounds.origin.y += dy;
    node.text_origin.x += dx;
    node.text_origin.y += dy;
    node.text_viewport.origin.x += dx;
    node.text_viewport.origin.y += dy;
    for child in &mut node.children {
        translate_node(child, dx, dy);
    }
}

fn align_column_children(
    children: &mut [LayoutNode],
    origin: Offset,
    size: Size,
    padding: crate::modifier::EdgeInsets,
    alignment: CrossAxisAlignment,
) {
    let left = origin.x + padding.left;
    let width = (size.width - padding.horizontal()).max(0.0);
    for child in children {
        let x = match alignment {
            CrossAxisAlignment::Start => left,
            CrossAxisAlignment::Center => left + (width - child.bounds.size.width) / 2.0,
            CrossAxisAlignment::End => left + width - child.bounds.size.width,
        };
        translate_node(child, x - child.bounds.origin.x, 0.0);
    }
}

fn align_row_children(
    children: &mut [LayoutNode],
    origin: Offset,
    size: Size,
    padding: crate::modifier::EdgeInsets,
    alignment: CrossAxisAlignment,
) {
    let top = origin.y + padding.top;
    let height = (size.height - padding.vertical()).max(0.0);
    for child in children {
        let y = match alignment {
            CrossAxisAlignment::Start => top,
            CrossAxisAlignment::Center => top + (height - child.bounds.size.height) / 2.0,
            CrossAxisAlignment::End => top + height - child.bounds.size.height,
        };
        translate_node(child, 0.0, y - child.bounds.origin.y);
    }
}

fn layout_stack_children(
    children: &[Element],
    origin: Offset,
    constraints: Constraints,
    interactions: &dyn Fn(NodeId) -> InteractionState,
    layout: &mut dyn TextLayoutEngine,
) -> Vec<LayoutNode> {
    children
        .iter()
        .map(|child| layout_element(child, origin, constraints, interactions, layout))
        .collect()
}

fn intrinsic_size(
    element: &Element,
    children: &[LayoutNode],
    constraints: Constraints,
    layout: &mut dyn TextLayoutEngine,
) -> Size {
    match &element.kind {
        ElementKind::Text(text) => {
            let data = element.modifier.data();
            let mut size = layout.measure_text(
                text,
                data.font_size.unwrap_or(14.0),
                constraints.max_width,
                data.text_wrap,
            );
            if let Some(min_lines) = data.text_min_lines {
                size.height = size
                    .height
                    .max(min_lines.max(1) as f32 * data.font_size.unwrap_or(14.0) * (20.0 / 14.0));
            }
            if let Some(max_lines) = data.text_max_lines {
                size.height = size
                    .height
                    .min(max_lines.max(1) as f32 * data.font_size.unwrap_or(14.0) * (20.0 / 14.0));
            }
            size
        }
        ElementKind::Custom(_) => Size::ZERO,
        ElementKind::Root | ElementKind::Column | ElementKind::Component(_) => {
            let width = children
                .iter()
                .map(|child| child.bounds.size.width)
                .fold(0.0, f32::max);
            let height = children.iter().map(|child| child.bounds.size.height).sum();
            Size::new(width, height)
        }
        ElementKind::Box => {
            let width = children
                .iter()
                .map(|child| child.bounds.size.width)
                .fold(0.0, f32::max);
            let height = children
                .iter()
                .map(|child| child.bounds.size.height)
                .fold(0.0, f32::max);
            Size::new(width, height)
        }
        ElementKind::Row => {
            let width = children.iter().map(|child| child.bounds.size.width).sum();
            let height = children
                .iter()
                .map(|child| child.bounds.size.height)
                .fold(0.0, f32::max);
            Size::new(width, height)
        }
    }
}

fn apply_modifier_size(data: &ModifierData, content_size: Size, constraints: Constraints) -> Size {
    let mut size = content_size;

    if data.fill_max_width && constraints.max_width.is_finite() {
        size.width = constraints.max_width;
    }

    if data.fill_max_height && constraints.max_height.is_finite() {
        size.height = constraints.max_height;
    }

    if let Some(width) = data.width {
        size.width = width;
    }

    if let Some(height) = data.height {
        size.height = height;
    }

    if let Some(width) = data.min_width {
        size.width = size.width.max(width);
    }

    if let Some(height) = data.min_height {
        size.height = size.height.max(height);
    }

    size.width += data.padding.horizontal();
    size.height += data.padding.vertical();
    constraints.constrain(size)
}

fn child_alignment(element: &Element, child: &LayoutNode, default: Alignment) -> Alignment {
    let _ = element;
    child.alignment.unwrap_or(default)
}

fn aligned_origin(
    child: Rect,
    origin: Offset,
    size: Size,
    padding: crate::modifier::EdgeInsets,
    alignment: Alignment,
) -> Offset {
    let content_origin = Offset::new(origin.x + padding.left, origin.y + padding.top);
    let content_size = Size::new(
        (size.width - padding.horizontal()).max(0.0),
        (size.height - padding.vertical()).max(0.0),
    );
    let x = match alignment {
        Alignment::TopStart | Alignment::CenterStart | Alignment::BottomStart => content_origin.x,
        Alignment::TopCenter | Alignment::Center | Alignment::BottomCenter => {
            content_origin.x + (content_size.width - child.size.width) / 2.0
        }
        Alignment::TopEnd | Alignment::CenterEnd | Alignment::BottomEnd => {
            content_origin.x + content_size.width - child.size.width
        }
    };
    let y = match alignment {
        Alignment::TopStart | Alignment::TopCenter | Alignment::TopEnd => content_origin.y,
        Alignment::CenterStart | Alignment::Center | Alignment::CenterEnd => {
            content_origin.y + (content_size.height - child.size.height) / 2.0
        }
        Alignment::BottomStart | Alignment::BottomCenter | Alignment::BottomEnd => {
            content_origin.y + content_size.height - child.size.height
        }
    };
    Offset::new(x, y)
}
