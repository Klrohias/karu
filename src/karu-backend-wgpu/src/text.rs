use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping, SwashCache, Wrap};
use karu::{CaretAffinity, CaretPosition, Color, Offset, Rect, Size, TextLayoutEngine, TextWrap};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Range;
use std::rc::Rc;
use unicode_segmentation::UnicodeSegmentation;

pub(crate) struct TextRasterizer {
    context: Rc<RefCell<FontSystem>>,
    swash_cache: SwashCache,
    family: Option<String>,
}

impl TextRasterizer {
    pub(crate) fn new(context: Rc<RefCell<FontSystem>>, family: Option<String>) -> Self {
        Self {
            context,
            swash_cache: SwashCache::new(),
            family,
        }
    }

    pub(crate) fn rasterize(
        &mut self,
        rect: &Rect,
        text: &str,
        font_size: f32,
        color: Color,
        wrap: TextWrap,
        offset: Offset,
        scale_factor: f32,
    ) -> Option<(u32, u32, Vec<u8>)> {
        let scale_factor = normalized_scale(scale_factor);
        let width = physical_text_extent(rect.size.width, scale_factor);
        let height = physical_text_extent(rect.size.height, scale_factor);
        let mut pixels = vec![0; width as usize * height as usize * 4];
        let mut context = self.context.borrow_mut();
        let mut buffer = make_buffer(
            &mut context,
            self.family.as_deref(),
            text,
            font_size * scale_factor,
            width as f32,
            height as f32,
            wrap,
        );
        let local_x = (offset.x - rect.origin.x) * scale_factor;
        let local_y = (offset.y - rect.origin.y) * scale_factor;
        buffer.draw(
            &mut context,
            &mut self.swash_cache,
            cosmic_color(color),
            |x, y, w, h, pixel| {
                let x0 = local_x.round() as i32 + x;
                let y0 = local_y.round() as i32 + y;
                for py in 0..h as i32 {
                    for px in 0..w as i32 {
                        let tx = x0 + px;
                        let ty = y0 + py;
                        if tx < 0 || ty < 0 || tx >= width as i32 || ty >= height as i32 {
                            continue;
                        }
                        let index = ((ty as u32 * width + tx as u32) * 4) as usize;
                        pixels[index] = pixel.r();
                        pixels[index + 1] = pixel.g();
                        pixels[index + 2] = pixel.b();
                        pixels[index + 3] = pixel.a();
                    }
                }
            },
        );
        Some((width, height, pixels))
    }
}

pub(crate) fn normalized_scale(scale_factor: f32) -> f32 {
    if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    }
}

pub(crate) fn aligned_uniform_stride(size: u64, alignment: u64) -> u64 {
    let alignment = alignment.max(1);
    size.div_ceil(alignment) * alignment
}

pub(crate) fn physical_text_extent(logical_size: f32, scale_factor: f32) -> u32 {
    (logical_size.max(1.0) * normalized_scale(scale_factor))
        .ceil()
        .max(1.0) as u32
}

pub struct CosmicTextLayout {
    context: Rc<RefCell<FontSystem>>,
    family: Option<String>,
    geometry_cache: HashMap<TextGeometryKey, CosmicGeometry>,
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub(crate) struct TextGeometryKey {
    text: String,
    font_size: u32,
    max_width: u32,
    wrap: u8,
}

impl CosmicTextLayout {
    pub fn new(family: Option<String>) -> Self {
        Self::with_context(Rc::new(RefCell::new(FontSystem::new())), family)
    }

    pub(crate) fn with_context(context: Rc<RefCell<FontSystem>>, family: Option<String>) -> Self {
        Self {
            context,
            family,
            geometry_cache: HashMap::new(),
        }
    }

    fn geometry(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
    ) -> CosmicGeometry {
        let key = TextGeometryKey {
            text: text.to_string(),
            font_size: font_size.to_bits(),
            max_width: max_width.to_bits(),
            wrap: text_wrap_key(wrap),
        };
        if let Some(geometry) = self.geometry_cache.get(&key) {
            return geometry.clone();
        }

        let geometry = self.build_geometry(text, font_size, max_width, wrap);
        if self.geometry_cache.len() >= 512 {
            self.geometry_cache.clear();
        }
        self.geometry_cache.insert(key, geometry.clone());
        geometry
    }

    fn build_geometry(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
    ) -> CosmicGeometry {
        let mut context = self.context.borrow_mut();
        let width = max_width.max(1.0);
        let buffer = make_buffer(
            &mut context,
            self.family.as_deref(),
            text,
            font_size,
            width,
            f32::INFINITY,
            wrap,
        );
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
                position: line.origin,
                line: line_index,
                height: line.height,
                affinity: CaretAffinity::After,
            });
            for glyph in run.glyphs {
                let cluster = &run.text[glyph.start..glyph.end];
                let graphemes = cluster.grapheme_indices(true).collect::<Vec<_>>();
                let glyph_width = glyph.w / graphemes.len().max(1) as f32;
                for (index, (byte_index, grapheme)) in graphemes.iter().enumerate() {
                    let x = glyph.x + index as f32 * glyph_width;
                    let start_x = if glyph.level.is_rtl() {
                        x + glyph_width
                    } else {
                        x
                    };
                    let end_x = if glyph.level.is_rtl() {
                        x
                    } else {
                        x + glyph_width
                    };
                    let absolute = base + glyph.start + byte_index + grapheme.len();
                    carets.push(CaretPosition {
                        offset: absolute - grapheme.len(),
                        position: Offset::new(start_x, run.line_top),
                        line: line_index,
                        height: run.line_height,
                        affinity: CaretAffinity::After,
                    });
                    carets.push(CaretPosition {
                        offset: absolute,
                        position: Offset::new(end_x, run.line_top),
                        line: line_index,
                        height: run.line_height,
                        affinity: CaretAffinity::Before,
                    });
                }
            }
        }
        if lines.is_empty() {
            let line_height = font_size.max(1.0) * (20.0 / 14.0);
            lines.push(CosmicLine {
                range: 0..text.len(),
                origin: Offset::ZERO,
                width: 0.0,
                height: line_height,
            });
            carets.push(CaretPosition {
                offset: 0,
                position: Offset::ZERO,
                line: 0,
                height: line_height,
                affinity: CaretAffinity::After,
            });
        }
        let size = Size::new(
            lines.iter().map(|line| line.width).fold(0.0, f32::max),
            lines
                .iter()
                .map(|line| line.origin.y + line.height)
                .fold(0.0, f32::max),
        );
        CosmicGeometry {
            size,
            line_height: font_size.max(1.0) * (20.0 / 14.0),
            lines,
            carets,
        }
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

pub(crate) fn make_buffer(
    font_system: &mut FontSystem,
    family: Option<&str>,
    text: &str,
    font_size: f32,
    width: f32,
    height: f32,
    wrap: TextWrap,
) -> Buffer {
    let mut buffer = Buffer::new(
        font_system,
        Metrics::new(font_size.max(1.0), font_size.max(1.0) * (20.0 / 14.0)),
    );
    buffer.set_size(
        Some(width.max(1.0)),
        height.is_finite().then_some(height.max(1.0)),
    );
    buffer.set_wrap(match wrap {
        TextWrap::NoWrap => Wrap::None,
        TextWrap::Word => Wrap::WordOrGlyph,
        TextWrap::Character => Wrap::Glyph,
    });
    let attrs = family
        .map(|family| Attrs::new().family(cosmic_text::Family::Name(family)))
        .unwrap_or_else(Attrs::new);
    buffer.set_text(text, &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);
    buffer
}

pub(crate) fn cosmic_color(color: Color) -> cosmic_text::Color {
    cosmic_text::Color::rgba(
        (color.red.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.green.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.blue.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

#[derive(Clone)]
pub(crate) struct CosmicGeometry {
    size: Size,
    line_height: f32,
    lines: Vec<CosmicLine>,
    carets: Vec<CaretPosition>,
}

pub(crate) fn text_wrap_key(wrap: TextWrap) -> u8 {
    match wrap {
        TextWrap::NoWrap => 0,
        TextWrap::Word => 1,
        TextWrap::Character => 2,
    }
}
#[derive(Clone)]
pub(crate) struct CosmicLine {
    range: Range<usize>,
    origin: Offset,
    width: f32,
    height: f32,
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
        let line = self
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
            .filter(|caret| caret.line == line)
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
            return self.lines.get(line).map_or(0, |line| line.range.start);
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
        let start = range.start.min(range.end);
        let end = range.start.max(range.end);
        if start == end {
            return Vec::new();
        }
        self.lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| {
                if !(start < line.range.end && end > line.range.start) {
                    return None;
                }
                let line_start = if start <= line.range.start {
                    0.0
                } else {
                    self.caret_on_line(index, start).position.x
                };
                let line_end = if end >= line.range.end {
                    line.width
                } else {
                    self.caret_on_line(index, end).position.x
                };
                (line_end > line_start).then(|| {
                    Rect::new(
                        line_start,
                        line.origin.y,
                        line_end - line_start,
                        line.height,
                    )
                })
            })
            .collect()
    }

    fn caret_on_line(&self, line: usize, offset: usize) -> CaretPosition {
        self.carets
            .iter()
            .filter(|caret| caret.line == line && caret.offset == offset)
            .find(|caret| caret.affinity == CaretAffinity::After)
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

pub(crate) fn distance_to_line(line: &CosmicLine, y: f32) -> f32 {
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
