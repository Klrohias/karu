use crate::element::{Element, ElementKind, NodeId};
use crate::layout::{Constraints, layout_tree};
use crate::modifier::Modifier;
use crate::renderer::{HeadlessOutput, RenderTree, Renderer, commands_for_tree};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CompositionId(pub usize);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RecomposeScopeId(pub Vec<usize>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecomposeRequest {
    pub composition_id: CompositionId,
    pub scope: RecomposeScopeId,
}

pub type RecomposeCallback = Box<dyn Fn(RecomposeRequest)>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompositionError {
    StateTypeMismatch { path: Vec<usize> },
    UnbalancedNodeStack,
}

impl fmt::Display for CompositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StateTypeMismatch { path } => {
                write!(f, "remember_state type changed at slot path {path:?}")
            }
            Self::UnbalancedNodeStack => write!(f, "composition produced an unbalanced node stack"),
        }
    }
}

impl std::error::Error for CompositionError {}

#[derive(Clone)]
pub struct Composer {
    inner: Rc<RefCell<ComposerInner>>,
}

impl Composer {
    fn new() -> Self {
        let invalidation = Rc::new(InvalidationState::new(CompositionId(0)));
        let inner = Rc::new(RefCell::new(ComposerInner::new(invalidation.clone())));
        invalidation.set_composition_id(CompositionId(Rc::as_ptr(&inner) as usize));

        Self { inner }
    }

    pub fn is_dirty(&self) -> bool {
        self.inner.borrow().invalidation.is_dirty()
    }

    fn begin_composition(&mut self) {
        self.inner.borrow_mut().begin_composition();
    }

    fn finish_composition(&mut self) -> Result<Element, CompositionError> {
        self.inner.borrow_mut().finish_composition()
    }

    fn set_recompose_callback(&self, callback: impl Fn(RecomposeRequest) + 'static) {
        self.inner
            .borrow()
            .invalidation
            .set_callback(Box::new(callback));
    }

    fn take_recompose_requests(&self) -> Vec<RecomposeRequest> {
        self.inner.borrow().invalidation.take_requests()
    }
}

pub struct Composition {
    root: Box<dyn FnMut(&mut Composer)>,
    composer: Composer,
    constraints: Constraints,
    last_result: Option<CompositionResult>,
}

impl Composition {
    pub fn new(root: impl FnMut(&mut Composer) + 'static) -> Self {
        Self {
            root: Box::new(root),
            composer: Composer::new(),
            constraints: Constraints::default(),
            last_result: None,
        }
    }

    pub fn with_constraints(mut self, constraints: Constraints) -> Self {
        self.constraints = constraints;
        self
    }

    pub fn set_constraints(&mut self, constraints: Constraints) -> &mut Self {
        self.constraints = constraints;
        self
    }

    pub fn compose(&mut self) -> CompositionResult {
        self.run()
    }

    pub fn recompose(&mut self) -> CompositionResult {
        self.run()
    }

    pub fn render_with<R: Renderer>(&mut self, renderer: &mut R) -> Result<R::Output, R::Error> {
        let result = self.run();
        renderer.render(&result.render_tree, &result.commands)
    }

    pub fn set_recompose_callback(&mut self, callback: impl Fn(RecomposeRequest) + 'static) {
        self.composer.set_recompose_callback(callback);
    }

    pub fn take_recompose_requests(&mut self) -> Vec<RecomposeRequest> {
        self.composer.take_recompose_requests()
    }

    pub fn is_dirty(&self) -> bool {
        self.composer.is_dirty()
    }

    pub fn last_result(&self) -> Option<&CompositionResult> {
        self.last_result.as_ref()
    }

    fn run(&mut self) -> CompositionResult {
        self.composer.begin_composition();
        (self.root)(&mut self.composer);

        let root = self
            .composer
            .finish_composition()
            .expect("karu composition ended with an unbalanced node stack");
        let layout_root = layout_tree(&root, self.constraints);
        let render_tree = RenderTree { root: layout_root };
        let commands = commands_for_tree(&render_tree.root);
        let output = HeadlessOutput {
            tree: render_tree.clone(),
            commands: commands.clone(),
        };
        let result = CompositionResult {
            root,
            render_tree,
            commands,
            output,
        };
        self.last_result = Some(result.clone());
        result
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompositionResult {
    pub root: Element,
    pub render_tree: RenderTree,
    pub commands: Vec<crate::renderer::RenderCommand>,
    pub output: HeadlessOutput,
}

pub fn with_component_scope<R>(
    composer: &mut Composer,
    name: &'static str,
    body: impl FnOnce(&mut Composer) -> R,
) -> R {
    composer
        .inner
        .borrow_mut()
        .begin_node(ElementKind::Component(name), Modifier::empty());
    let result = body(composer);
    composer.inner.borrow_mut().end_node();
    result
}

pub(crate) fn emit_node(
    composer: &mut Composer,
    kind: ElementKind,
    modifier: Modifier,
    children: impl FnOnce(&mut Composer),
) {
    composer.inner.borrow_mut().begin_node(kind, modifier);
    children(composer);
    composer.inner.borrow_mut().end_node();
}

pub(crate) fn emit_leaf(composer: &mut Composer, kind: ElementKind, modifier: Modifier) {
    let mut inner = composer.inner.borrow_mut();
    inner.begin_node(kind, modifier);
    inner.end_node();
}

pub(crate) fn remember_slot<T: 'static>(
    composer: &mut Composer,
    initializer: impl FnOnce() -> T,
) -> Result<(Rc<RefCell<T>>, InvalidationHandle), CompositionError> {
    composer.inner.borrow_mut().remember_slot(initializer)
}

#[derive(Clone)]
pub(crate) struct InvalidationHandle {
    state: Rc<InvalidationState>,
    scope: RecomposeScopeId,
}

impl InvalidationHandle {
    fn new(state: Rc<InvalidationState>, scope: RecomposeScopeId) -> Self {
        Self { state, scope }
    }

    pub(crate) fn detached() -> Self {
        Self::new(
            Rc::new(InvalidationState::new(CompositionId(0))),
            RecomposeScopeId(Vec::new()),
        )
    }

    pub(crate) fn invalidate(&self) {
        self.state.invalidate(self.scope.clone());
    }
}

struct InvalidationState {
    composition_id: Cell<CompositionId>,
    dirty: Cell<bool>,
    requests: RefCell<Vec<RecomposeRequest>>,
    callback: RefCell<Option<RecomposeCallback>>,
}

impl InvalidationState {
    fn new(composition_id: CompositionId) -> Self {
        Self {
            composition_id: Cell::new(composition_id),
            dirty: Cell::new(false),
            requests: RefCell::new(Vec::new()),
            callback: RefCell::new(None),
        }
    }

    fn set_composition_id(&self, composition_id: CompositionId) {
        self.composition_id.set(composition_id);
    }

    fn set_callback(&self, callback: RecomposeCallback) {
        *self.callback.borrow_mut() = Some(callback);
    }

    fn invalidate(&self, scope: RecomposeScopeId) {
        let request = RecomposeRequest {
            composition_id: self.composition_id.get(),
            scope,
        };

        self.dirty.set(true);
        self.requests.borrow_mut().push(request.clone());

        if let Some(callback) = self.callback.borrow().as_ref() {
            callback(request);
        }
    }

    fn is_dirty(&self) -> bool {
        self.dirty.get()
    }

    fn clear_dirty(&self) {
        self.dirty.set(false);
    }

    fn take_requests(&self) -> Vec<RecomposeRequest> {
        self.requests.borrow_mut().drain(..).collect()
    }
}

pub(crate) struct ComposerInner {
    next_node_id: u64,
    node_ids: HashMap<Vec<usize>, NodeId>,
    state_slots: HashMap<Vec<usize>, Rc<dyn Any>>,
    used_node_paths: HashSet<Vec<usize>>,
    used_state_paths: HashSet<Vec<usize>>,
    frames: Vec<Frame>,
    invalidation: Rc<InvalidationState>,
}

impl ComposerInner {
    fn new(invalidation: Rc<InvalidationState>) -> Self {
        Self {
            next_node_id: 0,
            node_ids: HashMap::new(),
            state_slots: HashMap::new(),
            used_node_paths: HashSet::new(),
            used_state_paths: HashSet::new(),
            frames: Vec::new(),
            invalidation,
        }
    }

    fn begin_composition(&mut self) {
        self.used_node_paths.clear();
        self.used_state_paths.clear();
        self.frames.clear();
        let root_id = self.node_id_for(&[]);
        self.frames.push(Frame {
            path: Vec::new(),
            next_slot: 0,
            element: Element::new(root_id, ElementKind::Root, Modifier::empty()),
        });
        self.invalidation.clear_dirty();
    }

    fn finish_composition(&mut self) -> Result<Element, CompositionError> {
        if self.frames.len() != 1 {
            return Err(CompositionError::UnbalancedNodeStack);
        }

        self.state_slots
            .retain(|path, _| self.used_state_paths.contains(path));
        self.node_ids
            .retain(|path, _| self.used_node_paths.contains(path));

        Ok(self
            .frames
            .pop()
            .expect("frame length checked before pop")
            .element)
    }

    fn begin_node(&mut self, kind: ElementKind, modifier: Modifier) {
        let path = self.next_slot_path();
        let id = self.node_id_for(&path);
        self.frames.push(Frame {
            path,
            next_slot: 0,
            element: Element::new(id, kind, modifier),
        });
    }

    fn end_node(&mut self) {
        let frame = self
            .frames
            .pop()
            .expect("node end called without an active node frame");
        self.frames
            .last_mut()
            .expect("node end called after root frame was popped")
            .element
            .children
            .push(frame.element);
    }

    fn remember_slot<T: 'static>(
        &mut self,
        initializer: impl FnOnce() -> T,
    ) -> Result<(Rc<RefCell<T>>, InvalidationHandle), CompositionError> {
        let scope = self.current_scope_id();
        let path = self.next_slot_path();
        self.used_state_paths.insert(path.clone());
        let handle = InvalidationHandle::new(self.invalidation.clone(), scope);

        if let Some(existing) = self.state_slots.get(&path) {
            let slot = existing
                .clone()
                .downcast::<RefCell<T>>()
                .map_err(|_| CompositionError::StateTypeMismatch { path })?;
            return Ok((slot, handle));
        }

        let slot = Rc::new(RefCell::new(initializer()));
        self.state_slots.insert(path, slot.clone() as Rc<dyn Any>);
        Ok((slot, handle))
    }

    fn current_scope_id(&self) -> RecomposeScopeId {
        RecomposeScopeId(
            self.frames
                .last()
                .map(|frame| frame.path.clone())
                .unwrap_or_default(),
        )
    }

    fn next_slot_path(&mut self) -> Vec<usize> {
        let frame = self
            .frames
            .last_mut()
            .expect("slot requested without active composition frame");
        let slot = frame.next_slot;
        frame.next_slot += 1;

        let mut path = frame.path.clone();
        path.push(slot);
        path
    }

    fn node_id_for(&mut self, path: &[usize]) -> NodeId {
        self.used_node_paths.insert(path.to_vec());
        if let Some(id) = self.node_ids.get(path) {
            return *id;
        }

        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        self.node_ids.insert(path.to_vec(), id);
        id
    }
}

struct Frame {
    path: Vec<usize>,
    next_slot: usize,
    element: Element,
}
