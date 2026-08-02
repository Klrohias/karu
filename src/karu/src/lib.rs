mod app;
mod composition;
mod element;
mod event;
mod foundation;
mod layout;
mod modifier;
mod platform;
mod renderer;
mod state;
mod text_layout;
mod ui;

extern crate self as karu;

pub use karu_macros::composable;

pub use app::{App, AppBackend, AppConfig, AppRoot};
pub use composition::{
    Applier, Composer, Composition, CompositionError, CompositionId, CompositionLocal,
    CompositionResult, ElementApplier, RecomposeCallback, RecomposeRequest, RecomposeScopeId,
    Recomposer, TaskHandle, TaskRuntime, composition_local_of, disposable_effect, key,
    launched_effect, provide, side_effect, with_component_scope, with_current_composer,
};
pub use element::{CustomElement, Element, ElementKind, NodeId};
pub use event::{
    EventRegistry, InteractionState, KeyCode, KeyEvent, KeyModifiers, PointerEvent, PointerKind,
    PointerPhase, ScrollEvent, TextEditCommand, TextInputContext, TextInputEvent, TextPointerEvent,
};
pub use foundation::{
    Animatable, FocusRequester, FocusState, ScrollState, TextFieldState, TextFieldValue, TweenSpec,
    animate_float_as_state, remember_scroll_state, remember_text_field_state,
};
pub use layout::{Constraints, Dp, LayoutNode, Offset, Rect, Size};
pub use modifier::{
    Alignment, Arrangement, Background, Border, BorderData, BorderRadius, Brush, Clickable, Clip,
    Color, ColumnCrossAlignment, CrossAxisAlignment, FillMaxHeight, FillMaxWidth, FixedHeight,
    FixedSize, FixedWidth, FocusTarget, FontSize, GradientStop, LayoutChain, LayoutInput,
    LayoutOutput, MeasureChain, MeasureInput, MeasureOutput, MinSize, Modifier, ModifierData,
    ModifierElement, Padding, PaintChain, PaintInput, Role, RowCrossAlignment, Scroll, ScrollAxis,
    ScrollData, SemanticRole, Semantics, TestTag, TextColor, TextInput, Weight, WeightData,
};
pub use platform::{Clipboard, ClipboardError, NoopClipboard};
pub use renderer::{
    HeadlessBackend, HeadlessOutput, HeadlessTextLayout, ImageId, RenderBackend, RenderCommand,
    RenderTree, TextInputCommand, TextInputResult, TextLayoutEngine, TextStyle,
    commands_for_tree_with_layout,
};
pub use state::{
    DerivedState, MutableState, NeverEqual, SnapshotMutationPolicy, SnapshotStateList,
    SnapshotStateMap, State, StateRead, StructuralEquality, UpdatedState, derived_state_of,
    mutable_state_of, mutable_state_with_policy, remember, remember_keyed, remember_mutable_state,
    remember_mutable_state_with_policy, remember_updated_state,
};
pub use text_layout::{CaretAffinity, CaretPosition, TextWrap};
pub use ui::{
    BasicTextField, Box, BoxOptions, Column, ColumnOptions, LazyColumn, LazyColumnOptions,
    LazyColumnScope, Row, RowOptions, Spacer, SpacerOptions, Text, TextFieldOptions, TextOptions,
};

#[doc(hidden)]
pub mod __private {
    pub use crate::composition::{
        disposable_effect, key, launched_effect, provide, side_effect, with_component_scope,
        with_component_scope_unit, with_current_composer,
    };
    pub use crate::state::{
        remember, remember_keyed, remember_mutable_state, remember_updated_state,
    };
    pub use crate::ui::{
        __karu_BasicTextField as BasicTextField, __karu_Box as Box, __karu_Column as Column,
        __karu_LazyColumn as LazyColumn, __karu_Row as Row, __karu_Spacer as Spacer,
        __karu_Text as Text,
    };
}
