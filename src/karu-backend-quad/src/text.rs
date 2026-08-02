use super::*;
use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping, SwashCache, Wrap};
use karu::{CaretAffinity, CaretPosition, Color, Offset, Rect, Size, TextLayoutEngine, TextWrap};
use macroquad::prelude::{Color as QuadColor, draw_rectangle};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use unicode_segmentation::UnicodeSegmentation;

pub struct CosmicTextLayout {
    context: Rc<RefCell<CosmicTextContext>>,
    swash_cache: SwashCache,
    family: Option<String>,
}

pub(crate) struct CosmicTextContext {
    font_system: FontSystem,
}

impl CosmicTextLayout {
    pub fn new(family: Option<String>) -> Self {
        let context = Rc::new(RefCell::new(CosmicTextContext {
            font_system: FontSystem::new(),
        }));
        Self::with_context(context, family)
    }

    fn with_context(context: Rc<RefCell<CosmicTextContext>>, family: Option<String>) -> Self {
        Self {
            context,
            swash_cache: SwashCache::new(),
            family,
        }
    }

    pub(crate) fn draw_text(
        &mut self,
        rect: &Rect,
        text: &str,
        font_size: f32,
        color: Color,
        wrap: TextWrap,
        offset: karu::Offset,
    ) {
        let line_height = font_size.max(1.0) * (20.0 / 14.0);
        let mut context = self.context.borrow_mut();
        let mut buffer = Buffer::new(
            &mut context.font_system,
            Metrics::new(font_size.max(1.0), line_height),
        );
        buffer.set_size(
            Some(rect.size.width.max(1.0)),
            Some(rect.size.height.max(1.0)),
        );
        buffer.set_wrap(match wrap {
            TextWrap::NoWrap => Wrap::None,
            TextWrap::Word => Wrap::WordOrGlyph,
            TextWrap::Character => Wrap::Glyph,
        });
        let attrs = self
            .family
            .as_deref()
            .map(|family| Attrs::new().family(cosmic_text::Family::Name(family)))
            .unwrap_or_else(Attrs::new);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut context.font_system, false);
        let base_x = snap_to_physical_pixel(offset.x);
        let base_y = snap_to_physical_pixel(offset.y);
        let quad_color = cosmic_color(color);
        buffer.draw(
            &mut context.font_system,
            &mut self.swash_cache,
            quad_color,
            |x, y, width, height, pixel: cosmic_text::Color| {
                let pixel = QuadColor::new(
                    f32::from(pixel.r()) / 255.0,
                    f32::from(pixel.g()) / 255.0,
                    f32::from(pixel.b()) / 255.0,
                    f32::from(pixel.a()) / 255.0,
                );
                draw_rectangle(
                    base_x + x as f32,
                    base_y + y as f32,
                    width as f32,
                    height as f32,
                    pixel,
                );
            },
        );
    }
}

impl TextLayoutEngine for CosmicTextLayout {
    fn measure_text(&mut self, text: &str, font_size: f32, max_width: f32, wrap: TextWrap) -> Size {
        self.geometry(text, font_size, max_width, wrap).size
    }

    fn caret_position(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        offset: usize,
    ) -> CaretPosition {
        self.geometry(text, font_size, max_width, wrap)
            .caret(offset)
    }

    fn hit_test_text(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        position: Offset,
    ) -> usize {
        self.geometry(text, font_size, max_width, wrap)
            .hit_test(position)
    }

    fn text_line_range(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        offset: usize,
    ) -> Range<usize> {
        self.geometry(text, font_size, max_width, wrap)
            .line_range(offset)
    }

    fn selection_rects(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        range: Range<usize>,
    ) -> Vec<Rect> {
        self.geometry(text, font_size, max_width, wrap)
            .selection_rects(range)
    }
}

impl std::fmt::Debug for CosmicTextLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CosmicTextLayout")
    }
}

impl Default for CosmicTextLayout {
    fn default() -> Self {
        Self::new(None)
    }
}

pub(crate) fn cosmic_color(color: Color) -> cosmic_text::Color {
    cosmic_text::Color::rgba(
        (color.red.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.green.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.blue.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

#[derive(Clone, Debug)]
pub(crate) struct CosmicGeometry {
    size: Size,
    line_height: f32,
    lines: Vec<CosmicLine>,
    carets: Vec<CaretPosition>,
}

#[derive(Clone, Debug)]
pub(crate) struct CosmicLine {
    range: Range<usize>,
    origin: Offset,
    width: f32,
    height: f32,
}

impl CosmicTextLayout {
    fn geometry(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
    ) -> CosmicGeometry {
        let mut context = self.context.borrow_mut();
        let mut buffer = Buffer::new(
            &mut context.font_system,
            Metrics::new(font_size.max(1.0), font_size.max(1.0) * (20.0 / 14.0)),
        );
        buffer.set_size(max_width.is_finite().then_some(max_width.max(1.0)), None);
        buffer.set_wrap(match wrap {
            TextWrap::NoWrap => Wrap::None,
            TextWrap::Word => Wrap::WordOrGlyph,
            TextWrap::Character => Wrap::Glyph,
        });
        let attrs = self
            .family
            .as_deref()
            .map(|family| Attrs::new().family(cosmic_text::Family::Name(family)))
            .unwrap_or_else(Attrs::new);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut context.font_system, false);
        let starts = paragraph_starts(text);
        let mut lines = Vec::new();
        let mut carets = Vec::new();
        for run in buffer.layout_runs() {
            let base = starts.get(run.line_i).copied().unwrap_or(0);
            let start = run
                .glyphs
                .iter()
                .map(|glyph| glyph.start)
                .min()
                .unwrap_or(0);
            let end = run
                .glyphs
                .iter()
                .map(|glyph| glyph.end)
                .max()
                .unwrap_or(start);
            let line_index = lines.len();
            let line = CosmicLine {
                range: base + start..base + end,
                origin: Offset::new(0.0, run.line_top),
                width: run.line_w,
                height: run.line_height,
            };
            lines.push(line.clone());
            carets.push(CaretPosition {
                offset: line.range.start,
                position: karu::Offset::new(0.0, run.line_top),
                line: line_index,
                height: run.line_height,
                affinity: karu::CaretAffinity::After,
            });
            for glyph in run.glyphs {
                let cluster = &run.text[glyph.start..glyph.end];
                let graphemes = cluster.grapheme_indices(true).collect::<Vec<_>>();
                let width = glyph.w / graphemes.len().max(1) as f32;
                for (index, grapheme) in graphemes {
                    let x = glyph.x + index as f32 * width;
                    let start_x = if glyph.level.is_rtl() { x + width } else { x };
                    let end_x = if glyph.level.is_rtl() { x } else { x + width };
                    let absolute = base + glyph.start + index;
                    carets.push(CaretPosition {
                        offset: absolute,
                        position: karu::Offset::new(start_x, run.line_top),
                        line: line_index,
                        height: run.line_height,
                        affinity: karu::CaretAffinity::After,
                    });
                    carets.push(CaretPosition {
                        offset: absolute + grapheme.len(),
                        position: karu::Offset::new(end_x, run.line_top),
                        line: line_index,
                        height: run.line_height,
                        affinity: karu::CaretAffinity::Before,
                    });
                }
            }
        }
        if lines.is_empty() {
            let line_height = font_size.max(1.0) * (20.0 / 14.0);
            lines.push(CosmicLine {
                range: 0..text.len(),
                origin: karu::Offset::ZERO,
                width: 0.0,
                height: line_height,
            });
            carets.push(CaretPosition {
                offset: 0,
                position: karu::Offset::ZERO,
                line: 0,
                height: line_height,
                affinity: karu::CaretAffinity::After,
            });
        }
        let width = lines.iter().map(|line| line.width).fold(0.0, f32::max);
        let height = lines
            .iter()
            .map(|line| line.origin.y + line.height)
            .fold(0.0, f32::max);
        CosmicGeometry {
            size: Size::new(width, height),
            line_height: font_size.max(1.0) * (20.0 / 14.0),
            lines,
            carets,
        }
    }
}

impl CosmicGeometry {
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
                distance_to_cosmic_line(left, position.y)
                    .partial_cmp(&distance_to_cosmic_line(right, position.y))
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

pub(crate) fn distance_to_cosmic_line(line: &CosmicLine, y: f32) -> f32 {
    if y < line.origin.y {
        line.origin.y - y
    } else if y > line.origin.y + line.height {
        y - line.origin.y - line.height
    } else {
        0.0
    }
}

pub(crate) fn paragraph_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (offset, _) in text.match_indices('\n') {
        starts.push(offset + 1);
    }
    starts
}
