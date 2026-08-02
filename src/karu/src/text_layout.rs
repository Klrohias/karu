use crate::layout::Offset;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextWrap {
    #[default]
    NoWrap,
    Word,
    Character,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CaretAffinity {
    Before,
    #[default]
    After,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretPosition {
    pub offset: usize,
    pub position: Offset,
    pub line: usize,
    pub height: f32,
    pub affinity: CaretAffinity,
}

pub(crate) fn grapheme_boundaries(text: &str) -> impl Iterator<Item = usize> + '_ {
    std::iter::once(0).chain(
        unicode_segmentation::UnicodeSegmentation::grapheme_indices(text, true)
            .map(|(offset, grapheme)| offset + grapheme.len()),
    )
}

pub(crate) fn previous_grapheme_boundary(text: &str, index: usize) -> usize {
    grapheme_boundaries(text)
        .take_while(|boundary| *boundary < index)
        .last()
        .unwrap_or(0)
}

pub(crate) fn next_grapheme_boundary(text: &str, index: usize) -> usize {
    grapheme_boundaries(text)
        .find(|boundary| *boundary > index)
        .unwrap_or(text.len())
}
