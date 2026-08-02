use crate::ScrollState;
use crate::TextFieldState;
use crate::TextWrap;
use crate::element::NodeId;
use crate::event::{EventRegistry, InteractionState, PointerEvent, ScrollEvent, TextInputEvent};
use crate::layout::{Constraints, Dp, Rect, Size};
use crate::renderer::RenderCommand;
use crate::{FocusRequester, FocusState, TextInputResult};
use std::any::Any;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
}

impl Color {
    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);
    pub const TRANSPARENT: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);

    pub const fn rgb(red: f32, green: f32, blue: f32) -> Self {
        Self::rgba(red, green, blue, 1.0)
    }

    pub const fn rgba(red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self::rgba(
            self.red + (other.red - self.red) * t,
            self.green + (other.green - self.green) * t,
            self.blue + (other.blue - self.blue) * t,
            self.alpha + (other.alpha - self.alpha) * t,
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GradientStop {
    pub position: f32,
    pub color: Color,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Brush {
    Solid(Color),
    LinearGradient {
        start: crate::layout::Offset,
        end: crate::layout::Offset,
        stops: Vec<GradientStop>,
    },
}

impl From<Color> for Brush {
    fn from(value: Color) -> Self {
        Self::Solid(value)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EdgeInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl EdgeInsets {
    pub fn all(value: f32) -> Self {
        Self {
            left: value,
            top: value,
            right: value,
            bottom: value,
        }
    }

    pub fn horizontal(self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(self) -> f32 {
        self.top + self.bottom
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModifierData {
    pub padding: EdgeInsets,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub min_width: Option<f32>,
    pub min_height: Option<f32>,
    pub fill_max_width: bool,
    pub fill_max_height: bool,
    pub background: Option<Color>,
    pub background_brush: Option<Brush>,
    pub text_color: Option<Color>,
    pub font_size: Option<f32>,
    pub border: Option<BorderData>,
    pub border_radius: f32,
    pub clip: bool,
    pub clickable: bool,
    pub interactive: bool,
    pub alignment: Option<Alignment>,
    pub weight: Option<WeightData>,
    pub horizontal_arrangement: Arrangement,
    pub vertical_arrangement: Arrangement,
    pub column_cross_alignment: CrossAxisAlignment,
    pub row_cross_alignment: CrossAxisAlignment,
    pub semantics: Semantics,
    pub scroll: Option<ScrollData>,
    pub text_wrap: TextWrap,
    pub text_min_lines: Option<usize>,
    pub text_max_lines: Option<usize>,
}

impl Default for ModifierData {
    fn default() -> Self {
        Self {
            padding: EdgeInsets::default(),
            width: None,
            height: None,
            min_width: None,
            min_height: None,
            fill_max_width: false,
            fill_max_height: false,
            background: None,
            background_brush: None,
            text_color: None,
            font_size: None,
            border: None,
            border_radius: 0.0,
            clip: false,
            clickable: false,
            interactive: false,
            alignment: None,
            weight: None,
            horizontal_arrangement: Arrangement::Start,
            vertical_arrangement: Arrangement::Start,
            column_cross_alignment: CrossAxisAlignment::Start,
            row_cross_alignment: CrossAxisAlignment::Start,
            semantics: Semantics::default(),
            scroll: None,
            text_wrap: TextWrap::NoWrap,
            text_min_lines: None,
            text_max_lines: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BorderData {
    pub width: f32,
    pub brush: Brush,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Alignment {
    #[default]
    TopStart,
    TopCenter,
    TopEnd,
    CenterStart,
    Center,
    CenterEnd,
    BottomStart,
    BottomCenter,
    BottomEnd,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Arrangement {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CrossAxisAlignment {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeightData {
    pub value: f32,
    pub fill: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Semantics {
    pub content_description: Option<String>,
    pub test_tag: Option<String>,
    pub role: Option<Role>,
    pub disabled: bool,
    pub focused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Button,
    Checkbox,
    Image,
    TextField,
    Switch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScrollData {
    pub axis: ScrollAxis,
    pub state: ScrollState,
}

pub trait ModifierElement: fmt::Debug {
    fn apply(&self, data: &mut ModifierData);

    fn resolve(&self, data: &mut ModifierData, _interaction: InteractionState) {
        self.apply(data);
    }

    fn install(&self, _node: NodeId, _events: &mut EventRegistry) {}

    fn measure(&self, input: MeasureInput, next: &mut dyn MeasureChain) -> MeasureOutput {
        next.measure(input)
    }

    fn layout(&self, input: LayoutInput, next: &mut dyn LayoutChain) -> LayoutOutput {
        next.layout(input)
    }

    fn paint(&self, input: PaintInput<'_>, next: &mut dyn PaintChain) {
        next.paint(input);
    }

    fn as_any(&self) -> &dyn Any;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasureInput {
    pub constraints: Constraints,
    pub content_size: Size,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasureOutput {
    pub size: Size,
}

pub trait MeasureChain {
    fn measure(&mut self, input: MeasureInput) -> MeasureOutput;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutInput {
    pub node: NodeId,
    pub bounds: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutOutput {
    pub bounds: Rect,
}

pub trait LayoutChain {
    fn layout(&mut self, input: LayoutInput) -> LayoutOutput;
}

#[derive(Debug)]
pub struct PaintInput<'a> {
    pub node: NodeId,
    pub bounds: Rect,
    pub commands: &'a mut Vec<RenderCommand>,
}

pub trait PaintChain {
    fn paint(&mut self, input: PaintInput<'_>);
}

#[derive(Clone, Default)]
pub struct Modifier {
    elements: Rc<[Rc<dyn ModifierElement>]>,
}

impl Modifier {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn then(mut self, next: impl ModifierElement + 'static) -> Self {
        let mut elements = self.elements.iter().cloned().collect::<Vec<_>>();
        elements.push(Rc::new(next));
        self.elements = elements.into();
        self
    }

    pub fn then_modifier(mut self, next: Modifier) -> Self {
        let mut elements = self.elements.iter().cloned().collect::<Vec<_>>();
        elements.extend(next.elements.iter().cloned());
        self.elements = elements.into();
        self
    }

    pub fn padding(self, value: impl Into<Dp>) -> Self {
        self.then(Padding::all(value))
    }

    pub fn size(self, width: impl Into<Dp>, height: impl Into<Dp>) -> Self {
        self.then(FixedSize {
            width: width.into().0,
            height: height.into().0,
        })
    }

    pub fn width(self, width: impl Into<Dp>) -> Self {
        self.then(FixedWidth(width.into().0))
    }

    pub fn height(self, height: impl Into<Dp>) -> Self {
        self.then(FixedHeight(height.into().0))
    }

    pub fn min_size(self, width: impl Into<Dp>, height: impl Into<Dp>) -> Self {
        self.then(MinSize {
            width: width.into().0,
            height: height.into().0,
        })
    }

    pub fn fill_max_width(self) -> Self {
        self.then(FillMaxWidth)
    }

    pub fn fill_max_height(self) -> Self {
        self.then(FillMaxHeight)
    }

    pub fn fill_max_size(self) -> Self {
        self.fill_max_width().fill_max_height()
    }

    pub fn align(self, alignment: Alignment) -> Self {
        self.then(Align(alignment))
    }

    pub fn weight(self, value: f32) -> Self {
        self.weight_with_fill(value, true)
    }

    pub fn weight_with_fill(self, value: f32, fill: bool) -> Self {
        assert!(
            value.is_finite() && value > 0.0,
            "weight must be finite and positive"
        );
        self.then(Weight { value, fill })
    }

    pub fn horizontal_arrangement(self, arrangement: Arrangement) -> Self {
        self.then(HorizontalArrangement(arrangement))
    }

    pub fn vertical_arrangement(self, arrangement: Arrangement) -> Self {
        self.then(VerticalArrangement(arrangement))
    }

    pub fn column_cross_alignment(self, alignment: CrossAxisAlignment) -> Self {
        self.then(ColumnCrossAlignment(alignment))
    }

    pub fn row_cross_alignment(self, alignment: CrossAxisAlignment) -> Self {
        self.then(RowCrossAlignment(alignment))
    }

    pub fn content_description(self, value: impl Into<String>) -> Self {
        self.then(ContentDescription(value.into()))
    }
    pub fn test_tag(self, value: impl Into<String>) -> Self {
        self.then(TestTag(value.into()))
    }
    pub fn role(self, role: Role) -> Self {
        self.then(SemanticRole(role))
    }
    pub fn disabled(self) -> Self {
        self.then(Disabled)
    }

    pub fn vertical_scroll(self, state: ScrollState) -> Self {
        self.then(Scroll {
            axis: ScrollAxis::Vertical,
            state,
        })
    }
    pub fn horizontal_scroll(self, state: ScrollState) -> Self {
        self.then(Scroll {
            axis: ScrollAxis::Horizontal,
            state,
        })
    }

    pub fn text_input(self, state: TextFieldState) -> Self {
        self.then(TextInput::new(state))
    }

    pub fn focusable(self, requester: FocusRequester, state: FocusState) -> Self {
        self.then(FocusTarget { requester, state })
    }

    pub fn background(self, brush: impl Into<Brush>) -> Self {
        self.then(Background(brush.into()))
    }

    pub fn text_color(self, color: Color) -> Self {
        self.then(TextColor(color))
    }

    pub fn font_size(self, size: f32) -> Self {
        assert!(
            size.is_finite() && size > 0.0,
            "font size must be finite and positive"
        );
        self.then(FontSize(size))
    }

    pub fn border(self, width: impl Into<Dp>, brush: impl Into<Brush>) -> Self {
        self.then(Border {
            width: width.into().0,
            brush: brush.into(),
        })
    }

    pub fn border_radius(self, radius: impl Into<Dp>) -> Self {
        self.then(BorderRadius(radius.into().0))
    }

    pub fn clip(self) -> Self {
        self.then(Clip)
    }

    pub(crate) fn text_field_config(
        self,
        wrap: TextWrap,
        min_lines: Option<usize>,
        max_lines: Option<usize>,
    ) -> Self {
        self.then(TextFieldConfig {
            wrap,
            min_lines,
            max_lines,
        })
    }

    pub fn clickable(self, on_click: impl FnMut() + 'static) -> Self {
        self.then(Clickable::new(on_click))
    }

    pub fn on_pointer_event(self, handler: impl FnMut(PointerEvent) + 'static) -> Self {
        self.then(PointerInput::new(handler))
    }

    pub fn data(&self) -> ModifierData {
        self.data_for(InteractionState::default())
    }

    pub(crate) fn data_for(&self, interaction: InteractionState) -> ModifierData {
        let mut data = ModifierData::default();
        for element in self.elements.iter() {
            element.resolve(&mut data, interaction);
        }
        data
    }

    pub(crate) fn install_events(&self, node: NodeId, events: &mut EventRegistry) {
        for element in self.elements.iter() {
            element.install(node, events);
        }
    }

    pub(crate) fn text_field_value(&self) -> Option<crate::TextFieldValue> {
        self.elements
            .iter()
            .rev()
            .find_map(|element| element.as_any().downcast_ref::<TextInput>())
            .map(|input| input.state.value())
    }

    pub(crate) fn text_field_state(&self) -> Option<TextFieldState> {
        self.elements
            .iter()
            .rev()
            .find_map(|element| element.as_any().downcast_ref::<TextInput>())
            .map(|input| input.state.clone())
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }
}

impl fmt::Debug for Modifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.elements.iter()).finish()
    }
}

impl PartialEq for Modifier {
    fn eq(&self, other: &Self) -> bool {
        self.data() == other.data()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Padding {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Padding {
    pub fn all(value: impl Into<Dp>) -> Self {
        let value = value.into().0;
        Self {
            left: value,
            top: value,
            right: value,
            bottom: value,
        }
    }
}

impl ModifierElement for Padding {
    fn apply(&self, data: &mut ModifierData) {
        data.padding.left += self.left;
        data.padding.top += self.top;
        data.padding.right += self.right;
        data.padding.bottom += self.bottom;
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FixedSize {
    pub width: f32,
    pub height: f32,
}
impl ModifierElement for FixedSize {
    fn apply(&self, data: &mut ModifierData) {
        data.width = Some(self.width);
        data.height = Some(self.height);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FixedWidth(pub f32);
impl ModifierElement for FixedWidth {
    fn apply(&self, data: &mut ModifierData) {
        data.width = Some(self.0);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FixedHeight(pub f32);
impl ModifierElement for FixedHeight {
    fn apply(&self, data: &mut ModifierData) {
        data.height = Some(self.0);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinSize {
    pub width: f32,
    pub height: f32,
}
impl ModifierElement for MinSize {
    fn apply(&self, data: &mut ModifierData) {
        data.min_width = Some(self.width);
        data.min_height = Some(self.height);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillMaxWidth;
impl ModifierElement for FillMaxWidth {
    fn apply(&self, data: &mut ModifierData) {
        data.fill_max_width = true;
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillMaxHeight;
impl ModifierElement for FillMaxHeight {
    fn apply(&self, data: &mut ModifierData) {
        data.fill_max_height = true;
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Align(pub Alignment);
impl ModifierElement for Align {
    fn apply(&self, data: &mut ModifierData) {
        data.alignment = Some(self.0);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weight {
    pub value: f32,
    pub fill: bool,
}
impl ModifierElement for Weight {
    fn apply(&self, data: &mut ModifierData) {
        data.weight = Some(WeightData {
            value: self.value,
            fill: self.fill,
        });
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HorizontalArrangement(pub Arrangement);
impl ModifierElement for HorizontalArrangement {
    fn apply(&self, data: &mut ModifierData) {
        data.horizontal_arrangement = self.0;
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VerticalArrangement(pub Arrangement);
impl ModifierElement for VerticalArrangement {
    fn apply(&self, data: &mut ModifierData) {
        data.vertical_arrangement = self.0;
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColumnCrossAlignment(pub CrossAxisAlignment);
impl ModifierElement for ColumnCrossAlignment {
    fn apply(&self, data: &mut ModifierData) {
        data.column_cross_alignment = self.0;
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowCrossAlignment(pub CrossAxisAlignment);
impl ModifierElement for RowCrossAlignment {
    fn apply(&self, data: &mut ModifierData) {
        data.row_cross_alignment = self.0;
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContentDescription(pub String);
impl ModifierElement for ContentDescription {
    fn apply(&self, data: &mut ModifierData) {
        data.semantics.content_description = Some(self.0.clone());
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct TestTag(pub String);
impl ModifierElement for TestTag {
    fn apply(&self, data: &mut ModifierData) {
        data.semantics.test_tag = Some(self.0.clone());
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticRole(pub Role);
impl ModifierElement for SemanticRole {
    fn apply(&self, data: &mut ModifierData) {
        data.semantics.role = Some(self.0);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Disabled;
impl ModifierElement for Disabled {
    fn apply(&self, data: &mut ModifierData) {
        data.semantics.disabled = true;
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scroll {
    pub axis: ScrollAxis,
    pub state: ScrollState,
}
impl ModifierElement for Scroll {
    fn apply(&self, data: &mut ModifierData) {
        data.scroll = Some(ScrollData {
            axis: self.axis,
            state: self.state.clone(),
        });
        data.interactive = true;
    }
    fn install(&self, node: NodeId, events: &mut EventRegistry) {
        let state = self.state.clone();
        let axis = self.axis;
        events.register_scroll_handler(node, move |event: ScrollEvent| {
            let delta = match axis {
                ScrollAxis::Horizontal => event.delta.x,
                ScrollAxis::Vertical => event.delta.y,
            };
            state.scroll_by_without_invalidation(delta) != 0.0
        });
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub struct TextInput {
    state: TextFieldState,
}
impl TextInput {
    pub fn new(state: TextFieldState) -> Self {
        Self { state }
    }
}
impl fmt::Debug for TextInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TextInput")
    }
}
impl ModifierElement for TextInput {
    fn apply(&self, data: &mut ModifierData) {
        data.interactive = true;
    }
    fn install(&self, node: NodeId, events: &mut EventRegistry) {
        let state = self.state.clone();
        events.register_text_pointer_handler(node, {
            let state = state.clone();
            move |event, renderer| {
                let previous_value = state.value();
                let previous_active = state.active_endpoint();
                if matches!(
                    event.event.phase,
                    crate::PointerPhase::Down | crate::PointerPhase::Move
                ) {
                    if event.event.phase == crate::PointerPhase::Move {
                        state.extend_selection_from_position(
                            event.local_position,
                            &event.context.text,
                            event.context.font_size,
                            event.context.max_width,
                            event.context.wrap,
                            renderer,
                        );
                    } else {
                        state.set_cursor_from_position(
                            event.local_position,
                            &event.context.text,
                            event.context.font_size,
                            event.context.max_width,
                            event.context.wrap,
                            renderer,
                        );
                        state.begin_selection_from_position(
                            event.local_position,
                            &event.context.text,
                            event.context.font_size,
                            event.context.max_width,
                            event.context.wrap,
                            renderer,
                        );
                    }
                } else if event.event.phase == crate::PointerPhase::Up {
                    state.clear_selection_anchor();
                }
                state.value() != previous_value || state.active_endpoint() != previous_active
            }
        });
        events.register_text_input_handler(node, move |event, context, renderer| match event {
            TextInputEvent::Insert { text, .. } | TextInputEvent::Paste { text, .. } => {
                state.replace_selection(text);
                TextInputResult::handled()
            }
            TextInputEvent::Command { command, .. } => state.handle_command(command),
            TextInputEvent::Backspace { .. } => {
                state.backspace();
                TextInputResult::handled()
            }
            TextInputEvent::Key { event, .. } => state.handle_key_with_layout(
                event,
                &context.text,
                context.font_size,
                context.max_width,
                context.wrap,
                renderer,
            ),
            TextInputEvent::CompositionStart { .. } => {
                state.start_composition();
                TextInputResult::handled()
            }
            TextInputEvent::CompositionUpdate { text, .. } => {
                state.update_composition(text);
                TextInputResult::handled()
            }
            TextInputEvent::CompositionCommit { text, .. } => {
                state.commit_composition(text);
                TextInputResult::handled()
            }
            TextInputEvent::CompositionEnd { .. } => {
                state.end_composition();
                TextInputResult::handled()
            }
        });
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct TextFieldConfig {
    wrap: TextWrap,
    min_lines: Option<usize>,
    max_lines: Option<usize>,
}

impl fmt::Debug for TextFieldConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextFieldConfig")
            .field("wrap", &self.wrap)
            .field("min_lines", &self.min_lines)
            .field("max_lines", &self.max_lines)
            .finish()
    }
}

impl ModifierElement for TextFieldConfig {
    fn apply(&self, data: &mut ModifierData) {
        data.text_wrap = self.wrap;
        data.text_min_lines = self.min_lines;
        data.text_max_lines = self.max_lines;
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub struct FocusTarget {
    requester: FocusRequester,
    state: FocusState,
}
impl fmt::Debug for FocusTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FocusTarget")
    }
}
impl ModifierElement for FocusTarget {
    fn apply(&self, data: &mut ModifierData) {
        data.interactive = true;
    }
    fn resolve(&self, data: &mut ModifierData, _interaction: InteractionState) {
        self.apply(data);
        data.semantics.focused = self.state.is_focused(self.requester);
    }
    fn install(&self, node: NodeId, events: &mut EventRegistry) {
        let state = self.state.clone();
        let requester = self.requester;
        events.register_pointer_handler(node, move |event| {
            if event.phase == crate::PointerPhase::Down && event.primary {
                state.request_focus(requester);
            }
        });
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Background(pub Brush);
impl ModifierElement for Background {
    fn apply(&self, data: &mut ModifierData) {
        data.background = match self.0 {
            Brush::Solid(color) => Some(color),
            _ => None,
        };
        data.background_brush = Some(self.0.clone());
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextColor(pub Color);
impl ModifierElement for TextColor {
    fn apply(&self, data: &mut ModifierData) {
        data.text_color = Some(self.0);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontSize(pub f32);
impl ModifierElement for FontSize {
    fn apply(&self, data: &mut ModifierData) {
        data.font_size = Some(self.0);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Border {
    pub width: f32,
    pub brush: Brush,
}
impl ModifierElement for Border {
    fn apply(&self, data: &mut ModifierData) {
        data.border = Some(BorderData {
            width: self.width,
            brush: self.brush.clone(),
        });
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BorderRadius(pub f32);
impl ModifierElement for BorderRadius {
    fn apply(&self, data: &mut ModifierData) {
        data.border_radius = self.0;
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clip;
impl ModifierElement for Clip {
    fn apply(&self, data: &mut ModifierData) {
        data.clip = true;
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub struct Clickable {
    handler: Rc<RefCell<dyn FnMut()>>,
}
impl Clickable {
    pub fn new(handler: impl FnMut() + 'static) -> Self {
        Self {
            handler: Rc::new(RefCell::new(handler)),
        }
    }
}
impl fmt::Debug for Clickable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Clickable")
    }
}
impl ModifierElement for Clickable {
    fn apply(&self, data: &mut ModifierData) {
        data.clickable = true;
        data.interactive = true;
    }
    fn install(&self, node: NodeId, events: &mut EventRegistry) {
        let handler = self.handler.clone();
        events.register_click_handler(node, move || (handler.borrow_mut())());
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub struct PointerInput {
    handler: Rc<RefCell<dyn FnMut(PointerEvent)>>,
}
impl PointerInput {
    pub fn new(handler: impl FnMut(PointerEvent) + 'static) -> Self {
        Self {
            handler: Rc::new(RefCell::new(handler)),
        }
    }
}
impl fmt::Debug for PointerInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PointerInput")
    }
}
impl ModifierElement for PointerInput {
    fn apply(&self, data: &mut ModifierData) {
        data.interactive = true;
    }
    fn install(&self, node: NodeId, events: &mut EventRegistry) {
        let handler = self.handler.clone();
        events.register_pointer_handler(node, move |event| (handler.borrow_mut())(event));
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}
