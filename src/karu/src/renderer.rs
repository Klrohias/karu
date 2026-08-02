use crate::element::{ElementKind, NodeId};
use crate::layout::{LayoutNode, Offset, Rect};
use crate::modifier::{Brush, Color};
use crate::{CaretAffinity, CaretPosition, Clipboard, NoopClipboard, Size, TextWrap};
use std::convert::Infallible;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

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
    pub commands: Vec<TextInputCommand>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextInputCommand {
    Copy(String),
    Cut(String),
    PasteRequest,
}

impl TextInputResult {
    pub fn handled() -> Self {
        Self {
            handled: true,
            commands: Vec::new(),
        }
    }

    pub fn command(command: TextInputCommand) -> Self {
        Self {
            handled: true,
            commands: vec![command],
        }
    }

    pub(crate) fn merge(&mut self, other: Self) {
        self.handled |= other.handled;
        self.commands.extend(other.commands);
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
        wrap: TextWrap,
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

/// Measures text and answers text-editing geometry queries.
///
/// This is deliberately independent from drawing. A platform can use one
/// shaping engine for layout while sending the resulting command stream to a
/// different graphics backend.
pub trait TextLayoutEngine {
    fn measure_text(&mut self, text: &str, font_size: f32, max_width: f32, wrap: TextWrap) -> Size;

    fn caret_position(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        offset: usize,
    ) -> CaretPosition;

    fn hit_test_text(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        position: Offset,
    ) -> usize;

    fn text_line_range(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        offset: usize,
    ) -> Range<usize>;

    fn selection_rects(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        range: Range<usize>,
    ) -> Vec<Rect>;
}

/// Consumes a platform-independent render list.
pub trait RenderBackend {
    type Output;
    type Error;
    type Clipboard: Clipboard;

    fn render(
        &mut self,
        tree: &RenderTree,
        commands: &[RenderCommand],
    ) -> Result<Self::Output, Self::Error>;

    fn clipboard(&mut self) -> &mut Self::Clipboard;
}

#[derive(Clone, Debug, Default)]
pub struct HeadlessTextLayout;

#[derive(Clone, Debug, PartialEq)]
pub struct HeadlessOutput {
    pub tree: RenderTree,
    pub commands: Vec<RenderCommand>,
}

#[derive(Clone, Debug, Default)]
pub struct HeadlessBackend {
    clipboard: NoopClipboard,
}

impl RenderBackend for HeadlessBackend {
    type Output = HeadlessOutput;
    type Error = Infallible;
    type Clipboard = NoopClipboard;

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

    fn clipboard(&mut self) -> &mut Self::Clipboard {
        &mut self.clipboard
    }
}

impl TextLayoutEngine for HeadlessTextLayout {
    fn measure_text(&mut self, text: &str, font_size: f32, max_width: f32, wrap: TextWrap) -> Size {
        basic_geometry(text, font_size, max_width, wrap).size
    }

    fn caret_position(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        offset: usize,
    ) -> CaretPosition {
        basic_geometry(text, font_size, max_width, wrap).caret(offset)
    }

    fn hit_test_text(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        position: Offset,
    ) -> usize {
        basic_geometry(text, font_size, max_width, wrap).hit_test(position)
    }

    fn text_line_range(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        offset: usize,
    ) -> Range<usize> {
        basic_geometry(text, font_size, max_width, wrap).line_range(offset)
    }

    fn selection_rects(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        range: Range<usize>,
    ) -> Vec<Rect> {
        basic_geometry(text, font_size, max_width, wrap).selection_rects(range)
    }
}

#[allow(dead_code)]
pub fn commands_for_tree(root: &LayoutNode) -> Vec<RenderCommand> {
    let mut layout = HeadlessTextLayout;
    commands_for_tree_with_layout(root, &mut layout)
}

pub fn commands_for_tree_with_layout(
    root: &LayoutNode,
    layout: &mut dyn TextLayoutEngine,
) -> Vec<RenderCommand> {
    let mut commands = Vec::new();
    push_commands(root, &mut commands, layout);
    commands
}

fn push_commands(
    node: &LayoutNode,
    commands: &mut Vec<RenderCommand>,
    layout: &mut dyn TextLayoutEngine,
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
        let font_size = node.font_size.unwrap_or(14.0);
        let max_width = node.text_viewport.size.width;
        if let Some(value) = &node.text_field {
            for selection in layout.selection_rects(
                text,
                font_size,
                max_width,
                node.text_wrap,
                value.selection.clone(),
            ) {
                commands.push(RenderCommand::DrawSelection {
                    node: node.id,
                    rect: translate_text_rect(node, selection),
                    color: Color::rgba(0.25, 0.5, 0.95, 0.35),
                });
            }
            if node.text_focused {
                let caret = layout.caret_position(
                    text,
                    font_size,
                    max_width,
                    node.text_wrap,
                    node.text_cursor,
                );
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
                for rect in layout.selection_rects(
                    text,
                    font_size,
                    max_width,
                    node.text_wrap,
                    composition.clone(),
                ) {
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
            commands.push(RenderCommand::DrawText {
                node: node.id,
                rect: node.text_viewport,
                text: text.clone(),
                style: TextStyle {
                    color: node.text_color.unwrap_or(Color::BLACK),
                    font_size: node.font_size.unwrap_or(14.0),
                },
                wrap: node.text_wrap,
                offset: Offset::new(
                    node.text_origin.x - node.text_scroll.x,
                    node.text_origin.y - node.text_scroll.y,
                ),
                selection: Some(value.selection.clone()),
                cursor: node.text_focused.then(|| {
                    layout.caret_position(
                        text,
                        font_size,
                        max_width,
                        node.text_wrap,
                        node.text_cursor,
                    )
                }),
                composition: value.composition.clone(),
            });
        } else {
            commands.push(RenderCommand::DrawText {
                node: node.id,
                rect: node.text_viewport,
                text: text.clone(),
                style: TextStyle {
                    color: node.text_color.unwrap_or(Color::BLACK),
                    font_size: node.font_size.unwrap_or(14.0),
                },
                wrap: node.text_wrap,
                offset: node.text_origin,
                selection: None,
                cursor: None,
                composition: None,
            });
        }
    }

    for child in &node.children {
        push_commands(child, commands, layout);
    }

    if node.clip {
        commands.push(RenderCommand::PopClip);
    }
}

fn translate_text_rect(node: &LayoutNode, rect: Rect) -> Rect {
    Rect::new(
        node.text_origin.x + rect.origin.x - node.text_scroll.x,
        node.text_origin.y + rect.origin.y - node.text_scroll.y,
        rect.size.width,
        rect.size.height,
    )
}

#[derive(Clone, Debug)]
struct BasicTextGeometry {
    size: Size,
    line_height: f32,
    lines: Vec<BasicTextLine>,
    carets: Vec<CaretPosition>,
}

#[derive(Clone, Debug)]
struct BasicTextLine {
    range: Range<usize>,
    origin: Offset,
    width: f32,
    height: f32,
}

impl BasicTextGeometry {
    fn caret(&self, offset: usize) -> CaretPosition {
        let offset = offset.min(self.carets.last().map_or(0, |caret| caret.offset));
        self.carets
            .iter()
            .find(|caret| caret.offset == offset && caret.affinity == CaretAffinity::After)
            .or_else(|| {
                self.carets
                    .iter()
                    .min_by_key(|caret| caret.offset.abs_diff(offset))
            })
            .copied()
            .unwrap_or(CaretPosition {
                offset: 0,
                position: Offset::ZERO,
                line: 0,
                height: self.line_height,
                affinity: CaretAffinity::After,
            })
    }

    fn hit_test(&self, position: Offset) -> usize {
        let line_index = self
            .lines
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| {
                distance_to_line(left, position.y)
                    .partial_cmp(&distance_to_line(right, position.y))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(index, _)| index)
            .unwrap_or(0);
        let mut carets = self
            .carets
            .iter()
            .filter(|caret| caret.line == line_index)
            .copied()
            .collect::<Vec<_>>();
        carets.sort_by(|left, right| {
            left.position
                .x
                .partial_cmp(&right.position.x)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.offset.cmp(&right.offset))
        });
        let Some(first) = carets.first() else {
            return self
                .lines
                .get(line_index)
                .map_or(0, |line| line.range.start);
        };
        if position.x <= first.position.x {
            return first.offset;
        }
        for pair in carets.windows(2) {
            if position.x < (pair[0].position.x + pair[1].position.x) * 0.5 {
                return pair[0].offset;
            }
        }
        carets.last().map_or(first.offset, |caret| caret.offset)
    }

    fn line_range(&self, offset: usize) -> Range<usize> {
        self.lines
            .iter()
            .find(|line| offset >= line.range.start && offset <= line.range.end)
            .map(|line| line.range.clone())
            .unwrap_or_else(|| self.lines.last().map_or(0..0, |line| line.range.clone()))
    }

    fn selection_rects(&self, range: Range<usize>) -> Vec<Rect> {
        if range.start == range.end {
            return Vec::new();
        }
        let start = range.start.min(range.end);
        let end = range.start.max(range.end);
        let mut rects = Vec::new();
        for (index, line) in self.lines.iter().enumerate() {
            let overlaps = if line.range.start == line.range.end {
                start <= line.range.start && end > line.range.start
            } else {
                start < line.range.end && end > line.range.start
            };
            if !overlaps {
                continue;
            }
            let line_start = if start <= line.range.start {
                line.origin.x
            } else {
                self.caret_on_line(index, start, true).position.x
            };
            let line_end = if end >= line.range.end {
                line.origin.x + line.width
            } else {
                self.caret_on_line(index, end, false).position.x
            };
            if line_end > line_start {
                rects.push(Rect::new(
                    line_start,
                    line.origin.y,
                    line_end - line_start,
                    line.height,
                ));
            }
        }
        rects
    }

    fn caret_on_line(&self, line: usize, offset: usize, prefer_after: bool) -> CaretPosition {
        self.carets
            .iter()
            .filter(|caret| caret.line == line && caret.offset == offset)
            .find(|caret| {
                (prefer_after && caret.affinity == CaretAffinity::After)
                    || (!prefer_after && caret.affinity == CaretAffinity::Before)
            })
            .or_else(|| {
                self.carets
                    .iter()
                    .filter(|caret| caret.line == line)
                    .min_by_key(|caret| caret.offset.abs_diff(offset))
            })
            .copied()
            .unwrap_or(CaretPosition {
                offset,
                position: self
                    .lines
                    .get(line)
                    .map_or(Offset::ZERO, |line| line.origin),
                line,
                height: self.line_height,
                affinity: CaretAffinity::After,
            })
    }
}

fn basic_geometry(text: &str, font_size: f32, max_width: f32, wrap: TextWrap) -> BasicTextGeometry {
    let font_size = font_size.max(1.0);
    let line_height = font_size * (20.0 / 14.0);
    let limit = if max_width.is_finite() {
        max_width.max(1.0)
    } else {
        f32::INFINITY
    };
    let mut line_ranges = Vec::new();
    let mut line_start = 0;
    let mut line_width = 0.0;
    let mut last_word_break = None;
    for (offset, grapheme) in text.grapheme_indices(true) {
        if grapheme == "\n" {
            line_ranges.push((line_start, offset, line_width));
            line_start = offset + grapheme.len();
            line_width = 0.0;
            last_word_break = None;
            continue;
        }
        let width = basic_grapheme_width(grapheme, font_size);
        if matches!(wrap, TextWrap::Character | TextWrap::Word)
            && line_width > 0.0
            && line_width + width > limit
        {
            let break_line = matches!(wrap, TextWrap::Word)
                .then_some(last_word_break)
                .flatten()
                .filter(|(break_offset, _)| *break_offset > line_start);
            if let Some((break_offset, break_width)) = break_line {
                line_ranges.push((line_start, break_offset, break_width));
                line_start = break_offset;
                line_width -= break_width;
            } else {
                line_ranges.push((line_start, offset, line_width));
                line_start = offset;
                line_width = 0.0;
            }
            last_word_break = None;
        }
        line_width += width;
        if grapheme.chars().all(char::is_whitespace) {
            last_word_break = Some((offset + grapheme.len(), line_width));
        }
    }
    line_ranges.push((line_start, text.len(), line_width));

    let mut lines = Vec::with_capacity(line_ranges.len());
    let mut carets = Vec::new();
    for (line_index, (start, end, width)) in line_ranges.iter().copied().enumerate() {
        let line_y = line_index as f32 * line_height;
        lines.push(BasicTextLine {
            range: start..end,
            origin: Offset::new(0.0, line_y),
            width,
            height: line_height,
        });
        carets.push(CaretPosition {
            offset: start,
            position: Offset::new(0.0, line_y),
            line: line_index,
            height: line_height,
            affinity: CaretAffinity::After,
        });
        let mut x = 0.0;
        for (offset, grapheme) in text[start..end].grapheme_indices(true) {
            let absolute = start + offset;
            x += basic_grapheme_width(grapheme, font_size);
            carets.push(CaretPosition {
                offset: absolute + grapheme.len(),
                position: Offset::new(x, line_y),
                line: line_index,
                height: line_height,
                affinity: CaretAffinity::Before,
            });
        }
    }
    let width = lines.iter().map(|line| line.width).fold(0.0, f32::max);
    BasicTextGeometry {
        size: Size::new(width, lines.len() as f32 * line_height),
        line_height,
        lines,
        carets,
    }
}

fn basic_grapheme_width(grapheme: &str, font_size: f32) -> f32 {
    let units = if grapheme.is_ascii() { 8.0 / 14.0 } else { 1.0 };
    (font_size * units).max(1.0)
}

fn distance_to_line(line: &BasicTextLine, y: f32) -> f32 {
    if y < line.origin.y {
        line.origin.y - y
    } else if y > line.origin.y + line.height {
        y - line.origin.y - line.height
    } else {
        0.0
    }
}
