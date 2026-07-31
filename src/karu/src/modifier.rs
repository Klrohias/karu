use crate::element::NodeId;
use crate::layout::{Constraints, Dp, Rect, Size};
use crate::renderer::RenderCommand;
use std::any::Any;
use std::fmt;
use std::sync::Arc;

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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ModifierData {
    pub padding: EdgeInsets,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub fill_max_width: bool,
    pub background: Option<Color>,
    pub clip: bool,
    pub clickable: bool,
}

pub trait ModifierElement: fmt::Debug + Send + Sync {
    fn apply(&self, data: &mut ModifierData);

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
    elements: Arc<[Arc<dyn ModifierElement>]>,
}

impl Modifier {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn then(mut self, next: impl ModifierElement + 'static) -> Self {
        let mut elements = self.elements.iter().cloned().collect::<Vec<_>>();
        elements.push(Arc::new(next));
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

    pub fn fill_max_width(self) -> Self {
        self.then(FillMaxWidth)
    }

    pub fn background(self, color: Color) -> Self {
        self.then(Background(color))
    }

    pub fn clip(self) -> Self {
        self.then(Clip)
    }

    pub fn clickable(self) -> Self {
        self.then(Clickable)
    }

    pub fn data(&self) -> ModifierData {
        let mut data = ModifierData::default();
        for element in self.elements.iter() {
            element.apply(&mut data);
        }
        data
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
pub struct Background(pub Color);

impl ModifierElement for Background {
    fn apply(&self, data: &mut ModifierData) {
        data.background = Some(self.0);
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clickable;

impl ModifierElement for Clickable {
    fn apply(&self, data: &mut ModifierData) {
        data.clickable = true;
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
