use crate::composition::{Composer, emit_leaf, emit_node};
use crate::element::ElementKind;
use crate::modifier::Modifier;

#[allow(non_snake_case)]
pub fn Column(composer: &mut Composer, children: impl FnOnce(&mut Composer)) {
    Column_with_modifier(composer, Modifier::empty(), children);
}

#[allow(non_snake_case)]
pub fn Column_with_modifier(
    composer: &mut Composer,
    modifier: Modifier,
    children: impl FnOnce(&mut Composer),
) {
    emit_node(composer, ElementKind::Column, modifier, children);
}

#[allow(non_snake_case)]
pub fn Text(composer: &mut Composer, text: impl Into<String>) {
    Text_with_modifier(composer, text, Modifier::empty());
}

#[allow(non_snake_case)]
pub fn Text_with_modifier(composer: &mut Composer, text: impl Into<String>, modifier: Modifier) {
    emit_leaf(composer, ElementKind::Text(text.into()), modifier);
}
