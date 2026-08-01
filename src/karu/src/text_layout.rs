use crate::layout::{Offset, Rect, Size};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextWrap {
    #[default]
    NoWrap,
    Word,
    Character,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextLine {
    pub range: Range<usize>,
    pub origin: Offset,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretPosition {
    pub offset: usize,
    pub position: Offset,
    pub line: usize,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    pub text_len: usize,
    pub size: Size,
    pub line_height: f32,
    pub lines: Vec<TextLine>,
    carets: Vec<CaretPosition>,
}

impl TextLayout {
    pub fn caret(&self, offset: usize) -> CaretPosition {
        let offset = offset.min(self.text_len);
        self.carets
            .iter()
            .min_by_key(|caret| caret.offset.abs_diff(offset))
            .copied()
            .unwrap_or(CaretPosition {
                offset: 0,
                position: Offset::ZERO,
                line: 0,
                height: self.line_height,
            })
    }

    pub fn hit_test(&self, position: Offset) -> usize {
        let line = self
            .lines
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| {
                let left_distance = (position.y - left.origin.y).abs();
                let right_distance = (position.y - right.origin.y).abs();
                left_distance
                    .partial_cmp(&right_distance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(index, _)| index)
            .unwrap_or(0);
        let line_index = line;
        let line = &self.lines[line_index];
        self.carets
            .iter()
            .filter(|caret| caret.line == line_index)
            .min_by(|left, right| {
                (left.position.x - position.x)
                    .abs()
                    .partial_cmp(&(right.position.x - position.x).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|caret| caret.offset)
            .unwrap_or(line.range.start)
    }

    pub fn selection_rects(&self, range: Range<usize>) -> Vec<Rect> {
        if range.start == range.end {
            return Vec::new();
        }
        let start = self.caret(range.start);
        let end = self.caret(range.end);
        let mut rects = Vec::new();
        for (index, line) in self.lines.iter().enumerate() {
            let line_start = if index == start.line {
                start.position.x
            } else if index > start.line && index <= end.line {
                line.origin.x
            } else {
                continue;
            };
            let line_end = if index == end.line {
                end.position.x
            } else {
                line.origin.x + line.width
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

    pub fn caret_positions(&self) -> impl Iterator<Item = CaretPosition> + '_ {
        self.carets.iter().copied()
    }

    pub fn scale_carets_on_line(&mut self, line: usize, scale: f32) {
        if !scale.is_finite() || scale <= 0.0 {
            return;
        }
        for caret in &mut self.carets {
            if caret.line == line {
                caret.position.x *= scale;
            }
        }
    }
}

pub trait TextLayoutEngine {
    fn layout(&self, text: &str, font_size: f32, max_width: f32, wrap: TextWrap) -> TextLayout;

    fn measure(&self, text: &str, font_size: f32, max_width: f32, wrap: TextWrap) -> Size {
        self.layout(text, font_size, max_width, wrap).size
    }

    fn caret_position(&self, layout: &TextLayout, offset: usize) -> CaretPosition {
        layout.caret(offset)
    }

    fn hit_test(&self, layout: &TextLayout, position: Offset) -> usize {
        layout.hit_test(position)
    }

    fn selection_rects(&self, layout: &TextLayout, range: Range<usize>) -> Vec<Rect> {
        layout.selection_rects(range)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BasicTextLayoutEngine;

impl BasicTextLayoutEngine {
    fn grapheme_width(grapheme: &str, font_size: f32) -> f32 {
        let units = if grapheme.is_ascii() { 8.0 / 14.0 } else { 1.0 };
        (font_size.max(1.0) * units).max(1.0)
    }
}

impl TextLayoutEngine for BasicTextLayoutEngine {
    fn layout(&self, text: &str, font_size: f32, max_width: f32, wrap: TextWrap) -> TextLayout {
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
            let width = Self::grapheme_width(grapheme, font_size);
            if matches!(wrap, TextWrap::Character | TextWrap::Word)
                && line_width > 0.0
                && line_width + width > limit
            {
                let break_line = matches!(wrap, TextWrap::Word)
                    .then(|| last_word_break)
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
        if line_ranges.is_empty() {
            line_ranges.push((0, 0, 0.0));
        }

        let mut lines = Vec::with_capacity(line_ranges.len());
        let mut carets = Vec::new();
        for (line_index, (start, end, width)) in line_ranges.iter().copied().enumerate() {
            let line_y = line_index as f32 * line_height;
            lines.push(TextLine {
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
            });
            let mut x = 0.0;
            for (offset, grapheme) in text[start..end].grapheme_indices(true) {
                let absolute = start + offset;
                x += Self::grapheme_width(grapheme, font_size);
                carets.push(CaretPosition {
                    offset: absolute + grapheme.len(),
                    position: Offset::new(x, line_y),
                    line: line_index,
                    height: line_height,
                });
            }
        }
        if lines.is_empty() {
            lines.push(TextLine {
                range: 0..0,
                origin: Offset::ZERO,
                width: 0.0,
                height: line_height,
            });
        }
        let width = lines.iter().map(|line| line.width).fold(0.0, f32::max);
        TextLayout {
            text_len: text.len(),
            size: Size::new(width, lines.len() as f32 * line_height),
            line_height,
            lines,
            carets,
        }
    }
}

pub(crate) fn grapheme_boundaries(text: &str) -> impl Iterator<Item = usize> + '_ {
    text.grapheme_indices(true)
        .map(|(offset, _)| offset)
        .chain(std::iter::once(text.len()))
}

pub(crate) fn previous_grapheme_boundary(text: &str, index: usize) -> usize {
    grapheme_boundaries(text)
        .filter(|boundary| *boundary < index.min(text.len()))
        .last()
        .unwrap_or(0)
}

pub(crate) fn next_grapheme_boundary(text: &str, index: usize) -> usize {
    grapheme_boundaries(text)
        .find(|boundary| *boundary > index.min(text.len()))
        .unwrap_or(text.len())
}
