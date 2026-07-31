mod app;
mod composition;
mod element;
mod layout;
mod modifier;
mod renderer;
mod state;
mod ui;

extern crate self as karu;

pub use karu_macros::{composable, mangled_composable};

pub use app::{App, AppBackend, AppConfig, AppRoot};
pub use composition::{
    Composer, Composition, CompositionError, CompositionId, CompositionResult, RecomposeCallback,
    RecomposeRequest, RecomposeScopeId, with_component_scope,
};
pub use element::{CustomElement, Element, ElementKind, NodeId};
pub use layout::{Constraints, Dp, LayoutNode, Offset, Rect, Size};
pub use modifier::{
    Background, Clickable, Clip, Color, FillMaxWidth, FixedHeight, FixedSize, FixedWidth,
    LayoutChain, LayoutInput, LayoutOutput, MeasureChain, MeasureInput, MeasureOutput, Modifier,
    ModifierElement, Padding, PaintChain, PaintInput,
};
pub use renderer::{
    HeadlessOutput, HeadlessRenderer, ImageId, RenderCommand, RenderTree, Renderer, TextStyle,
};
pub use state::{State, StateRead, remember_state};
pub use ui::{Column, ColumnWithModifier, Row, RowWithModifier, Text, TextWithModifier};

#[doc(hidden)]
pub mod __private {
    pub use crate::composition::with_component_scope;
    pub use crate::state::__karu_remember_state as remember_state;
    pub use crate::ui::{
        __karu_Column, __karu_Column as Column,
        __karu_ColumnWithModifier,
        __karu_Row, __karu_Row as Row,
        __karu_RowWithModifier,
        __karu_Text, __karu_Text as Text,
        __karu_TextWithModifier,
    };
}
