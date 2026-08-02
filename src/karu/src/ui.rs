use crate::composition::{emit_leaf, emit_node, with_current_composer};
use crate::element::ElementKind;
use crate::modifier::{Alignment, Arrangement, CrossAxisAlignment, Modifier};
use crate::{Role, ScrollState, TextFieldState, TextWrap};
use std::hash::Hash;

#[derive(Clone, Default)]
pub struct ColumnOptions {
    pub modifier: Modifier,
}
impl ColumnOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = modifier;
        self
    }
    pub fn vertical_arrangement(mut self, arrangement: Arrangement) -> Self {
        self.modifier = self.modifier.vertical_arrangement(arrangement);
        self
    }
    pub fn horizontal_alignment(mut self, alignment: CrossAxisAlignment) -> Self {
        self.modifier = self.modifier.column_cross_alignment(alignment);
        self
    }
}

#[derive(Clone, Default)]
pub struct RowOptions {
    pub modifier: Modifier,
}
impl RowOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = modifier;
        self
    }
    pub fn horizontal_arrangement(mut self, arrangement: Arrangement) -> Self {
        self.modifier = self.modifier.horizontal_arrangement(arrangement);
        self
    }
    pub fn vertical_alignment(mut self, alignment: CrossAxisAlignment) -> Self {
        self.modifier = self.modifier.row_cross_alignment(alignment);
        self
    }
}

#[derive(Clone, Default)]
pub struct BoxOptions {
    pub modifier: Modifier,
}

#[derive(Clone, Default)]
pub struct TextOptions {
    pub modifier: Modifier,
}
impl TextOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = modifier;
        self
    }
    pub fn color(mut self, color: crate::Color) -> Self {
        self.modifier = self.modifier.text_color(color);
        self
    }
    pub fn font_size(mut self, size: f32) -> Self {
        self.modifier = self.modifier.font_size(size);
        self
    }
}

#[derive(Clone)]
pub struct TextFieldOptions {
    pub modifier: Modifier,
    single_line: bool,
    wrap: TextWrap,
    min_lines: Option<usize>,
    max_lines: Option<usize>,
}

impl Default for TextFieldOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl TextFieldOptions {
    pub fn new() -> Self {
        Self {
            modifier: Modifier::empty(),
            single_line: true,
            wrap: TextWrap::NoWrap,
            min_lines: None,
            max_lines: None,
        }
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = modifier;
        self
    }

    pub fn single_line(mut self, single_line: bool) -> Self {
        self.single_line = single_line;
        if single_line {
            self.wrap = TextWrap::NoWrap;
        } else if self.wrap == TextWrap::NoWrap {
            self.wrap = TextWrap::Word;
        }
        self
    }

    pub fn multiline(mut self) -> Self {
        self.single_line = false;
        if self.wrap == TextWrap::NoWrap {
            self.wrap = TextWrap::Word;
        }
        self
    }

    pub fn wrap(mut self, wrap: TextWrap) -> Self {
        self.single_line = false;
        self.wrap = wrap;
        self
    }

    pub fn min_lines(mut self, lines: usize) -> Self {
        self.min_lines = Some(lines.max(1));
        self
    }

    pub fn max_lines(mut self, lines: usize) -> Self {
        self.max_lines = Some(lines.max(1));
        self
    }
}

#[derive(Clone)]
pub struct LazyColumnOptions {
    pub modifier: Modifier,
    state: Option<ScrollState>,
    item_extent: Option<f32>,
    viewport_height: Option<f32>,
    overscan: usize,
}

impl Default for LazyColumnOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl LazyColumnOptions {
    pub fn new() -> Self {
        Self {
            modifier: Modifier::empty(),
            state: None,
            item_extent: None,
            viewport_height: None,
            overscan: 2,
        }
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = modifier;
        self
    }

    pub fn state(mut self, state: ScrollState) -> Self {
        self.state = Some(state);
        self
    }

    pub fn item_extent(mut self, extent: f32) -> Self {
        assert!(
            extent.is_finite() && extent > 0.0,
            "item extent must be positive"
        );
        self.item_extent = Some(extent);
        self
    }

    pub fn viewport_height(mut self, height: f32) -> Self {
        assert!(
            height.is_finite() && height > 0.0,
            "viewport height must be positive"
        );
        self.viewport_height = Some(height);
        self
    }

    pub fn overscan(mut self, count: usize) -> Self {
        self.overscan = count;
        self
    }
}

#[derive(Clone, Default)]
pub struct SpacerOptions {
    pub modifier: Modifier,
}

impl SpacerOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = modifier;
        self
    }
}
impl BoxOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = modifier;
        self
    }
    pub fn content_alignment(mut self, alignment: Alignment) -> Self {
        self.modifier = self.modifier.align(alignment);
        self
    }
}

#[allow(non_snake_case)]
pub fn Column(options: ColumnOptions, children: impl FnOnce()) {
    __karu_Column(options, children);
}

#[allow(non_snake_case)]
pub fn Row(options: RowOptions, children: impl FnOnce()) {
    __karu_Row(options, children);
}

#[allow(non_snake_case)]
pub fn Box(options: BoxOptions, children: impl FnOnce()) {
    __karu_Box(options, children);
}

#[allow(non_snake_case)]
pub fn Text(text: impl Into<String>, options: TextOptions) {
    __karu_Text(text, options);
}

#[allow(non_snake_case)]
pub fn BasicTextField(state: &TextFieldState, options: TextFieldOptions) {
    __karu_BasicTextField(state, options);
}

pub struct LazyColumnScope {
    state: ScrollState,
    item_extent: Option<f32>,
    viewport_height: Option<f32>,
    overscan: usize,
}

impl LazyColumnScope {
    pub fn item<K: Hash>(&mut self, key: K, content: impl FnOnce()) {
        crate::key(key, content);
    }

    pub fn items<T, K: Hash>(
        &mut self,
        items: impl IntoIterator<Item = T>,
        key: impl Fn(&T) -> K,
        mut content: impl FnMut(T),
    ) {
        let items = items.into_iter().collect::<Vec<_>>();
        let Some(extent) = self.item_extent else {
            for item in items {
                crate::key(key(&item), || content(item));
            }
            return;
        };
        let Some(viewport_height) = self.viewport_height else {
            for item in items {
                crate::key(key(&item), || content(item));
            }
            return;
        };

        let total = items.len();
        let offset = self.state.value();
        let start = ((offset / extent).floor() as usize).saturating_sub(self.overscan);
        let end =
            (((offset + viewport_height) / extent).ceil() as usize + self.overscan).min(total);
        Spacer(SpacerOptions::new().modifier(Modifier::empty().height(start as f32 * extent)));
        for (index, item) in items.into_iter().enumerate() {
            if index < start || index >= end {
                continue;
            }
            let item_key = key(&item);
            crate::key(item_key, || {
                __karu_Box(
                    BoxOptions::new().modifier(Modifier::empty().height(extent)),
                    || content(item),
                );
            });
        }
        Spacer(
            SpacerOptions::new()
                .modifier(Modifier::empty().height(total.saturating_sub(end) as f32 * extent)),
        );
    }
}

#[allow(non_snake_case)]
pub fn LazyColumn(options: LazyColumnOptions, content: impl FnOnce(&mut LazyColumnScope)) {
    __karu_LazyColumn(options, content);
}

#[allow(non_snake_case)]
pub fn Spacer(options: SpacerOptions) {
    __karu_Spacer(options);
}

#[doc(hidden)]
#[allow(non_snake_case)]
pub fn __karu_Column(options: ColumnOptions, children: impl FnOnce()) {
    with_current_composer(|composer| {
        emit_node(composer, ElementKind::Column, options.modifier, |_| {
            children()
        });
    });
}

#[doc(hidden)]
#[allow(non_snake_case)]
pub fn __karu_Row(options: RowOptions, children: impl FnOnce()) {
    with_current_composer(|composer| {
        emit_node(composer, ElementKind::Row, options.modifier, |_| children());
    });
}

#[doc(hidden)]
#[allow(non_snake_case)]
pub fn __karu_Box(options: BoxOptions, children: impl FnOnce()) {
    with_current_composer(|composer| {
        emit_node(composer, ElementKind::Box, options.modifier, |_| children());
    });
}

#[doc(hidden)]
#[allow(non_snake_case)]
pub fn __karu_Text(text: impl Into<String>, options: TextOptions) {
    with_current_composer(|composer| {
        emit_leaf(composer, ElementKind::Text(text.into()), options.modifier);
    });
}

#[doc(hidden)]
#[allow(non_snake_case)]
pub fn __karu_BasicTextField(state: &TextFieldState, options: TextFieldOptions) {
    let modifier = options.modifier.text_field_config(
        if options.single_line {
            TextWrap::NoWrap
        } else {
            options.wrap
        },
        options.min_lines,
        options.max_lines,
    );
    __karu_Text(
        state.text(),
        TextOptions::new().modifier(modifier.role(Role::TextField).text_input(state.clone())),
    );
}

#[doc(hidden)]
#[allow(non_snake_case)]
pub fn __karu_LazyColumn(options: LazyColumnOptions, content: impl FnOnce(&mut LazyColumnScope)) {
    let state = options
        .state
        .unwrap_or_else(|| crate::remember_scroll_state(0.0));
    let viewport_height = options
        .viewport_height
        .or_else(|| options.modifier.data().height);
    let virtualized = options.item_extent.is_some() && viewport_height.is_some();
    state.set_recompose_on_scroll(virtualized);
    let scope_state = state.clone();
    __karu_Column(
        ColumnOptions::new().modifier(options.modifier.vertical_scroll(state)),
        || {
            let mut scope = LazyColumnScope {
                state: scope_state,
                item_extent: options.item_extent,
                viewport_height,
                overscan: options.overscan,
            };
            content(&mut scope);
        },
    );
}

#[doc(hidden)]
#[allow(non_snake_case)]
pub fn __karu_Spacer(options: SpacerOptions) {
    __karu_Box(BoxOptions::new().modifier(options.modifier), || {});
}
