use crate::modifier::Modifier;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(pub u64);

#[derive(Clone, Debug, PartialEq)]
pub struct CustomElement {
    pub name: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ElementKind {
    Root,
    Column,
    Row,
    Text(String),
    Component(&'static str),
    Custom(CustomElement),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub id: NodeId,
    pub kind: ElementKind,
    pub modifier: Modifier,
    pub children: Vec<Element>,
}

impl Element {
    pub fn new(id: NodeId, kind: ElementKind, modifier: Modifier) -> Self {
        Self {
            id,
            kind,
            modifier,
            children: Vec::new(),
        }
    }
}
