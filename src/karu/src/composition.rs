use crate::element::{Element, ElementKind, NodeId};
use crate::event::{EventRegistry, KeyEvent, PointerEvent, ScrollEvent, TextInputEvent};
use crate::layout::{Constraints, layout_tree_with_events};
use crate::modifier::Modifier;
use crate::renderer::TextInputResult;
use crate::renderer::{HeadlessOutput, RenderTree, Renderer, commands_for_tree};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::future::Future;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::pin::Pin;
use std::ptr;
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

thread_local! {
    static CURRENT_COMPOSER: Cell<*mut Composer> = const { Cell::new(ptr::null_mut()) };
}

struct CurrentComposerGuard;

impl CurrentComposerGuard {
    fn install(composer: *mut Composer) -> Self {
        CURRENT_COMPOSER.with(|cell| cell.set(composer));
        Self
    }
}

impl Drop for CurrentComposerGuard {
    fn drop(&mut self) {
        CURRENT_COMPOSER.with(|cell| cell.set(ptr::null_mut()));
    }
}

struct NodeFrameGuard {
    inner: Rc<RefCell<ComposerInner>>,
}

impl Drop for NodeFrameGuard {
    fn drop(&mut self) {
        self.inner.borrow_mut().end_node();
    }
}

struct LocalProviderGuard {
    inner: Rc<RefCell<ComposerInner>>,
    local: LocalId,
}

impl Drop for LocalProviderGuard {
    fn drop(&mut self) {
        self.inner.borrow_mut().pop_local_id(self.local);
    }
}

pub fn with_current_composer<R>(f: impl FnOnce(&mut Composer) -> R) -> R {
    let ptr = CURRENT_COMPOSER.with(|cell| cell.get());
    assert!(
        !ptr.is_null(),
        "with_current_composer called outside of a Karu composition. \
         Composable functions must be called within Composition::new() or another composable function."
    );
    // SAFETY: The pointer is valid because Composition::compose() sets it
    // before calling the root function and clears it immediately after.
    unsafe { f(&mut *ptr) }
}

pub(crate) fn current_recompose_scope() -> Option<RecomposeScopeId> {
    let ptr = CURRENT_COMPOSER.with(|cell| cell.get());
    if ptr.is_null() {
        None
    } else {
        // SAFETY: identical lifetime guarantee to with_current_composer.
        Some(unsafe { (&*ptr).inner.borrow().current_scope_id() })
    }
}

/// Gives a subtree an identity independent from its sibling position.
///
/// Keys must be unique among siblings, just as they are in Compose.
pub fn key<K: Hash, R>(value: K, content: impl FnOnce() -> R) -> R {
    with_current_composer(|composer| composer.with_key(value, content))
}

/// Schedules work after the current composition has successfully completed.
pub fn side_effect(effect: impl FnOnce() + 'static) {
    with_current_composer(|composer| composer.record_side_effect(effect));
}

pub fn disposable_effect<K: Hash, F, D>(key: K, setup: F)
where
    F: FnOnce() -> D + 'static,
    D: FnOnce() + 'static,
{
    with_current_composer(|composer| composer.record_disposable_effect(key, setup));
}

pub trait TaskHandle: 'static {
    fn cancel(&self);
}

pub trait TaskRuntime: 'static {
    fn spawn(&self, task: Pin<Box<dyn Future<Output = ()> + 'static>>) -> Rc<dyn TaskHandle>;
}

pub fn launched_effect<K: Hash>(key: K, task: impl Future<Output = ()> + 'static) {
    with_current_composer(|composer| composer.launch_effect(key, task));
}

#[derive(Clone)]
pub struct CompositionLocal<T: Clone + 'static> {
    default: fn() -> T,
    identity: Rc<()>,
}

impl<T: Clone + 'static> CompositionLocal<T> {
    pub fn new(default: fn() -> T) -> Self {
        Self {
            default,
            identity: Rc::new(()),
        }
    }

    pub fn current(&self) -> T {
        with_current_composer(|composer| composer.current_local(self))
    }
}

pub fn composition_local_of<T: Clone + 'static>(default: fn() -> T) -> CompositionLocal<T> {
    CompositionLocal::new(default)
}

pub fn provide<T: Clone + 'static, R>(
    local: &CompositionLocal<T>,
    value: T,
    content: impl FnOnce() -> R,
) -> R {
    with_current_composer(|composer| composer.provide_local(local, value, content))
}

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

    pub fn with_key<K: Hash, R>(&mut self, value: K, content: impl FnOnce() -> R) -> R {
        self.inner.borrow_mut().begin_key_scope(hash_key(value));
        let _frame_guard = NodeFrameGuard {
            inner: self.inner.clone(),
        };
        content()
    }

    pub fn record_side_effect(&mut self, effect: impl FnOnce() + 'static) {
        self.inner.borrow_mut().side_effects.push(Box::new(effect));
    }

    pub fn record_disposable_effect<K: Hash, F, D>(&mut self, key: K, setup: F)
    where
        F: FnOnce() -> D + 'static,
        D: FnOnce() + 'static,
    {
        let hash = hash_key(key);
        self.inner
            .borrow_mut()
            .record_disposable_effect(hash, setup);
    }

    pub fn launch_effect<K: Hash>(&mut self, key: K, task: impl Future<Output = ()> + 'static) {
        self.inner
            .borrow_mut()
            .launch_effect(hash_key(key), Box::pin(task));
    }

    fn set_task_runtime(&mut self, runtime: Rc<dyn TaskRuntime>) {
        self.inner.borrow_mut().task_runtime = Some(runtime);
    }

    fn take_side_effects(&mut self) -> Vec<Box<dyn FnOnce()>> {
        std::mem::take(&mut self.inner.borrow_mut().side_effects)
    }

    fn provide_local<T: Clone + 'static, R>(
        &mut self,
        local: &CompositionLocal<T>,
        value: T,
        content: impl FnOnce() -> R,
    ) -> R {
        self.inner.borrow_mut().push_local(local, value);
        let _provider_guard = LocalProviderGuard {
            inner: self.inner.clone(),
            local: local_id(local),
        };
        content()
    }

    fn current_local<T: Clone + 'static>(&mut self, local: &CompositionLocal<T>) -> T {
        self.inner.borrow().current_local(local)
    }

    pub(crate) fn dispatch_pointer_event(
        &mut self,
        tree: &crate::layout::LayoutNode,
        event: PointerEvent,
    ) -> bool {
        let handled = self.inner.borrow_mut().events.dispatch(tree, event);
        if handled {
            self.inner.borrow().invalidation.mark_dirty();
        }
        handled
    }

    pub(crate) fn dispatch_scroll_event(
        &mut self,
        tree: &crate::layout::LayoutNode,
        event: ScrollEvent,
    ) -> bool {
        let handled = self.inner.borrow_mut().events.dispatch_scroll(tree, event);
        if handled {
            self.inner.borrow().invalidation.mark_dirty();
        }
        handled
    }

    pub(crate) fn dispatch_text_input_event(
        &mut self,
        tree: &crate::layout::LayoutNode,
        event: TextInputEvent,
    ) -> TextInputResult {
        let result = self
            .inner
            .borrow_mut()
            .events
            .dispatch_text_input(tree, event);
        if result.handled {
            self.inner.borrow().invalidation.mark_dirty();
        }
        result
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
    root: Box<dyn FnMut()>,
    composer: Composer,
    constraints: Constraints,
    last_result: Option<CompositionResult>,
}

impl Composition {
    pub fn new(root: impl FnMut() + 'static) -> Self {
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

    pub fn with_task_runtime(mut self, runtime: Rc<dyn TaskRuntime>) -> Self {
        self.composer.set_task_runtime(runtime);
        self
    }

    pub fn set_task_runtime(&mut self, runtime: Rc<dyn TaskRuntime>) -> &mut Self {
        self.composer.set_task_runtime(runtime);
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

    pub fn dispatch_pointer_event(&mut self, event: PointerEvent) -> bool {
        let Some(result) = self.last_result.as_ref() else {
            return false;
        };
        let tree = result.render_tree.root.clone();
        self.composer.dispatch_pointer_event(&tree, event)
    }

    pub fn dispatch_scroll_event(&mut self, event: ScrollEvent) -> bool {
        let Some(result) = self.last_result.as_ref() else {
            return false;
        };
        self.composer
            .dispatch_scroll_event(&result.render_tree.root.clone(), event)
    }

    pub fn dispatch_text_input_event(&mut self, event: TextInputEvent) -> bool {
        self.dispatch_text_input_event_with_result(event).handled
    }

    pub fn dispatch_text_input_event_with_result(
        &mut self,
        event: TextInputEvent,
    ) -> TextInputResult {
        let Some(result) = self.last_result.as_ref() else {
            return TextInputResult::default();
        };
        self.composer
            .dispatch_text_input_event(&result.render_tree.root.clone(), event)
    }

    pub fn dispatch_key_event(&mut self, position: crate::Offset, event: KeyEvent) -> bool {
        self.dispatch_text_input_event(TextInputEvent::Key { position, event })
    }

    pub fn dispatch_key_event_with_result(
        &mut self,
        position: crate::Offset,
        event: KeyEvent,
    ) -> TextInputResult {
        self.dispatch_text_input_event_with_result(TextInputEvent::Key { position, event })
    }

    pub fn last_result(&self) -> Option<&CompositionResult> {
        self.last_result.as_ref()
    }

    fn run(&mut self) -> CompositionResult {
        self.composer.begin_composition();

        // Set thread-local composer pointer for the duration of root execution
        let composer_ptr: *mut Composer = &mut self.composer;
        let _composer_guard = CurrentComposerGuard::install(composer_ptr);
        (self.root)();
        drop(_composer_guard);

        let root = self
            .composer
            .finish_composition()
            .expect("karu composition ended with an unbalanced node stack");
        for effect in self.composer.take_side_effects() {
            effect();
        }
        let layout_root = {
            let inner = self.composer.inner.borrow();
            layout_tree_with_events(&root, self.constraints, &inner.events)
        };
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

impl Drop for Composition {
    fn drop(&mut self) {
        self.composer.inner.borrow_mut().dispose();
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
    let _frame_guard = NodeFrameGuard {
        inner: composer.inner.clone(),
    };
    body(composer)
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

    pub(crate) fn invalidate_scope(&self, scope: RecomposeScopeId) {
        self.state.invalidate(scope);
    }
}

struct InvalidationState {
    composition_id: Cell<CompositionId>,
    dirty: Cell<bool>,
    active_scopes: RefCell<HashSet<RecomposeScopeId>>,
    requests: RefCell<Vec<RecomposeRequest>>,
    callback: RefCell<Option<RecomposeCallback>>,
}

impl InvalidationState {
    fn new(composition_id: CompositionId) -> Self {
        Self {
            composition_id: Cell::new(composition_id),
            dirty: Cell::new(false),
            active_scopes: RefCell::new(HashSet::new()),
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

    fn set_active_scopes(&self, scopes: HashSet<RecomposeScopeId>) {
        *self.active_scopes.borrow_mut() = scopes;
    }

    fn invalidate(&self, scope: RecomposeScopeId) {
        let active_scopes = self.active_scopes.borrow();
        if !active_scopes.is_empty() && !active_scopes.contains(&scope) {
            return;
        }
        drop(active_scopes);
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

    fn mark_dirty(&self) {
        self.dirty.set(true);
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
    locals: HashMap<LocalId, Vec<Rc<dyn Any>>>,
    side_effects: Vec<Box<dyn FnOnce()>>,
    disposable_effects: HashMap<Vec<usize>, DisposableEffectSlot>,
    used_disposable_effects: HashSet<Vec<usize>>,
    pending_disposable_effects: Vec<PendingDisposableEffect>,
    launched_effects: HashMap<Vec<usize>, LaunchedEffectSlot>,
    used_launched_effects: HashSet<Vec<usize>>,
    pending_launched_effects: Vec<PendingLaunchedEffect>,
    task_runtime: Option<Rc<dyn TaskRuntime>>,
    invalidation: Rc<InvalidationState>,
    events: EventRegistry,
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
            locals: HashMap::new(),
            side_effects: Vec::new(),
            disposable_effects: HashMap::new(),
            used_disposable_effects: HashSet::new(),
            pending_disposable_effects: Vec::new(),
            launched_effects: HashMap::new(),
            used_launched_effects: HashSet::new(),
            pending_launched_effects: Vec::new(),
            task_runtime: None,
            invalidation,
            events: EventRegistry::default(),
        }
    }

    fn begin_composition(&mut self) {
        self.used_node_paths.clear();
        self.used_state_paths.clear();
        self.frames.clear();
        self.side_effects.clear();
        self.used_disposable_effects.clear();
        self.pending_disposable_effects.clear();
        self.used_launched_effects.clear();
        self.pending_launched_effects.clear();
        self.events.begin_composition();
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
        self.invalidation.set_active_scopes(
            self.used_node_paths
                .iter()
                .cloned()
                .map(RecomposeScopeId)
                .collect(),
        );
        let stale_disposables = self
            .disposable_effects
            .keys()
            .filter(|path| !self.used_disposable_effects.contains(*path))
            .cloned()
            .collect::<Vec<_>>();
        for path in stale_disposables {
            if let Some(effect) = self.disposable_effects.remove(&path) {
                (effect.dispose)();
            }
        }
        for pending in std::mem::take(&mut self.pending_disposable_effects) {
            if let Some(effect) = self.disposable_effects.remove(&pending.path) {
                (effect.dispose)();
            }
            self.disposable_effects.insert(
                pending.path,
                DisposableEffectSlot {
                    key: pending.key,
                    dispose: (pending.setup)(),
                },
            );
        }
        let stale_launched = self
            .launched_effects
            .keys()
            .filter(|path| !self.used_launched_effects.contains(*path))
            .cloned()
            .collect::<Vec<_>>();
        for path in stale_launched {
            if let Some(effect) = self.launched_effects.remove(&path) {
                effect.handle.cancel();
            }
        }
        for pending in std::mem::take(&mut self.pending_launched_effects) {
            if let Some(effect) = self.launched_effects.remove(&pending.path) {
                effect.handle.cancel();
            }
            self.launched_effects.insert(
                pending.path,
                LaunchedEffectSlot {
                    key: pending.key,
                    handle: pending.runtime.spawn(pending.task),
                },
            );
        }

        Ok(self
            .frames
            .pop()
            .expect("frame length checked before pop")
            .element)
    }

    fn dispose(&mut self) {
        for (_, effect) in self.disposable_effects.drain() {
            (effect.dispose)();
        }
        for (_, effect) in self.launched_effects.drain() {
            effect.handle.cancel();
        }
    }

    fn begin_node(&mut self, kind: ElementKind, modifier: Modifier) {
        let path = self.next_slot_path();
        let id = self.node_id_for(&path);
        modifier.install_events(id, &mut self.events);
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

    fn record_disposable_effect<F, D>(&mut self, key: u64, setup: F)
    where
        F: FnOnce() -> D + 'static,
        D: FnOnce() + 'static,
    {
        let path = self.next_slot_path();
        self.used_disposable_effects.insert(path.clone());
        if self
            .disposable_effects
            .get(&path)
            .is_some_and(|effect| effect.key == key)
        {
            return;
        }
        self.pending_disposable_effects
            .push(PendingDisposableEffect {
                path,
                key,
                setup: Box::new(move || Box::new(setup())),
            });
    }

    fn launch_effect(&mut self, key: u64, task: Pin<Box<dyn Future<Output = ()> + 'static>>) {
        let path = self.next_slot_path();
        self.used_launched_effects.insert(path.clone());
        if self
            .launched_effects
            .get(&path)
            .is_some_and(|effect| effect.key == key)
        {
            return;
        }
        let runtime = self
            .task_runtime
            .as_ref()
            .expect("LaunchedEffect requires a TaskRuntime on Composition")
            .clone();
        self.pending_launched_effects.push(PendingLaunchedEffect {
            path,
            key,
            task,
            runtime,
        });
    }

    fn begin_key_scope(&mut self, hash: u64) {
        let frame = self.frames.last_mut().expect("key outside composition");
        frame.next_slot += 1;
        let mut path = frame.path.clone();
        path.push(usize::MAX);
        path.push(hash as usize);
        let id = self.node_id_for(&path);
        self.frames.push(Frame {
            path,
            next_slot: 0,
            element: Element::new(id, ElementKind::Component("key"), Modifier::empty()),
        });
    }

    fn push_local<T: Clone + 'static>(&mut self, local: &CompositionLocal<T>, value: T) {
        self.locals
            .entry(local_id(local))
            .or_default()
            .push(Rc::new(value));
    }

    fn pop_local_id(&mut self, local: LocalId) {
        let locals = self
            .locals
            .get_mut(&local)
            .expect("CompositionLocal provider stack underflow");
        locals.pop();
        if locals.is_empty() {
            self.locals.remove(&local);
        }
    }

    fn current_local<T: Clone + 'static>(&self, local: &CompositionLocal<T>) -> T {
        self.locals
            .get(&local_id(local))
            .and_then(|values| values.last())
            .and_then(|value| value.downcast_ref::<T>())
            .cloned()
            .unwrap_or_else(local.default)
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

struct DisposableEffectSlot {
    key: u64,
    dispose: Box<dyn FnOnce()>,
}

struct PendingDisposableEffect {
    path: Vec<usize>,
    key: u64,
    setup: Box<dyn FnOnce() -> Box<dyn FnOnce()>>,
}

struct LaunchedEffectSlot {
    key: u64,
    handle: Rc<dyn TaskHandle>,
}

struct PendingLaunchedEffect {
    path: Vec<usize>,
    key: u64,
    task: Pin<Box<dyn Future<Output = ()> + 'static>>,
    runtime: Rc<dyn TaskRuntime>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct LocalId {
    value_type: std::any::TypeId,
    identity: usize,
}

fn local_id<T: Clone + 'static>(local: &CompositionLocal<T>) -> LocalId {
    LocalId {
        value_type: std::any::TypeId::of::<T>(),
        identity: Rc::as_ptr(&local.identity) as usize,
    }
}

fn hash_key<K: Hash>(value: K) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}
