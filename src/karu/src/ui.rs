use crate::composition::{Composer, emit_leaf, emit_node};
use crate::element::ElementKind;
use crate::modifier::Modifier;

#[allow(non_snake_case)]
pub fn Column(children: impl FnOnce()) {
    children();
}

#[allow(non_snake_case)]
pub fn ColumnWithModifier(modifier: Modifier, children: impl FnOnce()) {
    let _ = modifier;
    children();
}

#[allow(non_snake_case)]
pub fn Row(children: impl FnOnce()) {
    children();
}

#[allow(non_snake_case)]
pub fn RowWithModifier(modifier: Modifier, children: impl FnOnce()) {
    let _ = modifier;
    children();
}

#[allow(non_snake_case)]
pub fn Text(text: impl Into<String>) {
    let _ = text.into();
}

#[allow(non_snake_case)]
pub fn TextWithModifier(text: impl Into<String>, modifier: Modifier) {
    let _ = text.into();
    let _ = modifier;
}

#[doc(hidden)]
#[allow(non_snake_case)]
pub fn __karu_Column(composer: &mut Composer, children: impl FnOnce(&mut Composer)) {
    __karu_ColumnWithModifier(composer, Modifier::empty(), children);
}

#[doc(hidden)]
#[allow(non_snake_case)]
pub fn __karu_ColumnWithModifier(
    composer: &mut Composer,
    modifier: Modifier,
    children: impl FnOnce(&mut Composer),
) {
    emit_node(composer, ElementKind::Column, modifier, children);
}

#[doc(hidden)]
#[allow(non_snake_case)]
pub fn __karu_Row(composer: &mut Composer, children: impl FnOnce(&mut Composer)) {
    __karu_RowWithModifier(composer, Modifier::empty(), children);
}

#[doc(hidden)]
#[allow(non_snake_case)]
pub fn __karu_RowWithModifier(
    composer: &mut Composer,
    modifier: Modifier,
    children: impl FnOnce(&mut Composer),
) {
    emit_node(composer, ElementKind::Row, modifier, children);
}

#[doc(hidden)]
#[allow(non_snake_case)]
pub fn __karu_Text(composer: &mut Composer, text: impl Into<String>) {
    __karu_TextWithModifier(composer, text, Modifier::empty());
}

#[doc(hidden)]
#[allow(non_snake_case)]
pub fn __karu_TextWithModifier(
    composer: &mut Composer,
    text: impl Into<String>,
    modifier: Modifier,
) {
    emit_leaf(composer, ElementKind::Text(text.into()), modifier);
}
