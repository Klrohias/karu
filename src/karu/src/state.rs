use crate::composition::{
    InvalidationHandle, RecomposeScopeId, current_recompose_scope, remember_slot,
    with_current_composer,
};
use std::cell::{Ref, RefCell};
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Deref;
use std::rc::Rc;

/// Defines whether assigning a value should invalidate readers.
pub trait SnapshotMutationPolicy<T>: 'static {
    fn equivalent(&self, current: &T, next: &T) -> bool;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StructuralEquality;

impl<T: PartialEq> SnapshotMutationPolicy<T> for StructuralEquality {
    fn equivalent(&self, current: &T, next: &T) -> bool {
        current == next
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NeverEqual;

impl<T> SnapshotMutationPolicy<T> for NeverEqual {
    fn equivalent(&self, _: &T, _: &T) -> bool {
        false
    }
}

/// A read-only observable value.
pub struct State<T: 'static> {
    value: Rc<RefCell<T>>,
    subscribers: Rc<RefCell<std::collections::HashSet<RecomposeScopeId>>>,
}

impl<T: 'static> Clone for State<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            subscribers: self.subscribers.clone(),
        }
    }
}

impl<T: 'static> State<T> {
    pub fn read(&self) -> StateRead<'_, T> {
        register_read(&self.subscribers);
        StateRead {
            value: self.value.borrow(),
        }
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        register_read(&self.subscribers);
        self.value.borrow().clone()
    }
}

/// A mutable observable value. Writes are scheduled on the current composition.
pub struct MutableState<T: 'static> {
    value: Rc<RefCell<T>>,
    invalidation: InvalidationHandle,
    policy: Rc<dyn SnapshotMutationPolicy<T>>,
    subscribers: Rc<RefCell<std::collections::HashSet<RecomposeScopeId>>>,
}

impl<T: 'static> Clone for MutableState<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            invalidation: self.invalidation.clone(),
            policy: self.policy.clone(),
            subscribers: self.subscribers.clone(),
        }
    }
}

impl<T: 'static> MutableState<T> {
    pub fn as_state(&self) -> State<T> {
        State {
            value: self.value.clone(),
            subscribers: self.subscribers.clone(),
        }
    }

    pub fn read(&self) -> StateRead<'_, T> {
        register_read(&self.subscribers);
        StateRead {
            value: self.value.borrow(),
        }
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        register_read(&self.subscribers);
        self.value.borrow().clone()
    }

    pub fn set(&self, value: T) {
        let mut current = self.value.borrow_mut();
        if self.policy.equivalent(&current, &value) {
            return;
        }
        *current = value;
        drop(current);
        self.invalidate_readers();
    }

    pub fn update(&self, update: impl FnOnce(&mut T)) {
        update(&mut self.value.borrow_mut());
        self.invalidate_readers();
    }

    pub(crate) fn set_without_invalidation(&self, value: T) {
        *self.value.borrow_mut() = value;
    }

    fn invalidate_readers(&self) {
        let subscribers = self.subscribers.borrow();
        if subscribers.is_empty() {
            self.invalidation.invalidate();
            return;
        }
        for scope in subscribers.iter().cloned() {
            self.invalidation.invalidate_scope(scope);
        }
    }
}

pub struct StateRead<'a, T> {
    value: Ref<'a, T>,
}

impl<T> Deref for StateRead<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

fn register_read(subscribers: &Rc<RefCell<std::collections::HashSet<RecomposeScopeId>>>) {
    if let Some(scope) = current_recompose_scope() {
        subscribers.borrow_mut().insert(scope);
    }
}

pub fn mutable_state_of<T: PartialEq + 'static>(value: T) -> MutableState<T> {
    mutable_state_with_policy(value, StructuralEquality)
}

pub fn mutable_state_with_policy<T: 'static>(
    value: T,
    policy: impl SnapshotMutationPolicy<T>,
) -> MutableState<T> {
    MutableState {
        value: Rc::new(RefCell::new(value)),
        invalidation: InvalidationHandle::detached(),
        policy: Rc::new(policy),
        subscribers: Rc::new(RefCell::new(Default::default())),
    }
}

/// Remembers an arbitrary cloneable value in the current composition group.
pub fn remember<T: Clone + 'static>(initializer: impl FnOnce() -> T) -> T {
    with_current_composer(|composer| {
        let (value, _) = remember_slot(composer, initializer)
            .expect("remember slot type changed during Karu composition");
        value.borrow().clone()
    })
}

/// Rust-friendly keyed equivalent of Compose's `remember(key) { ... }`.
pub fn remember_keyed<K: Hash, T: Clone + 'static>(key: K, initializer: impl FnOnce() -> T) -> T {
    crate::composition::key(key, || remember(initializer))
}

pub fn remember_mutable_state<T: PartialEq + 'static>(
    initializer: impl FnOnce() -> T,
) -> MutableState<T> {
    remember_mutable_state_with_policy(initializer, StructuralEquality)
}

struct RememberedState<T> {
    value: Rc<RefCell<T>>,
    subscribers: Rc<RefCell<std::collections::HashSet<RecomposeScopeId>>>,
}

pub fn remember_mutable_state_with_policy<T: 'static>(
    initializer: impl FnOnce() -> T,
    policy: impl SnapshotMutationPolicy<T>,
) -> MutableState<T> {
    with_current_composer(|composer| {
        let (slot, invalidation) = remember_slot(composer, || RememberedState {
            value: Rc::new(RefCell::new(initializer())),
            subscribers: Rc::new(RefCell::new(Default::default())),
        })
        .expect("mutable state slot type changed during Karu composition");
        let slot = slot.borrow();
        MutableState {
            value: slot.value.clone(),
            invalidation,
            policy: Rc::new(policy),
            subscribers: slot.subscribers.clone(),
        }
    })
}

/// Keeps the newest value available to a long-lived effect without restarting it.
#[derive(Clone)]
pub struct UpdatedState<T: 'static> {
    value: Rc<RefCell<T>>,
}

impl<T: Clone + 'static> UpdatedState<T> {
    pub fn get(&self) -> T {
        self.value.borrow().clone()
    }
}

pub fn remember_updated_state<T: Clone + 'static>(value: T) -> UpdatedState<T> {
    with_current_composer(|composer| {
        let (slot, _) = remember_slot(composer, || value.clone())
            .expect("updated state slot type changed during Karu composition");
        *slot.borrow_mut() = value;
        UpdatedState { value: slot }
    })
}

pub struct DerivedState<T> {
    compute: Rc<dyn Fn() -> T>,
}

impl<T> Clone for DerivedState<T> {
    fn clone(&self) -> Self {
        Self {
            compute: self.compute.clone(),
        }
    }
}

impl<T> DerivedState<T> {
    pub fn get(&self) -> T {
        (self.compute)()
    }
}

pub fn derived_state_of<T: 'static>(compute: impl Fn() -> T + 'static) -> DerivedState<T> {
    DerivedState {
        compute: Rc::new(compute),
    }
}

#[derive(Clone)]
pub struct SnapshotStateList<T: Clone + PartialEq + 'static> {
    state: MutableState<Vec<T>>,
}

impl<T: Clone + PartialEq + 'static> SnapshotStateList<T> {
    pub fn new(values: Vec<T>) -> Self {
        Self {
            state: mutable_state_of(values),
        }
    }
    pub fn get(&self) -> Vec<T> {
        self.state.get()
    }
    pub fn push(&self, value: T) {
        self.state.update(|values| values.push(value));
    }
    pub fn remove(&self, index: usize) -> T {
        let mut removed = None;
        self.state
            .update(|values| removed = Some(values.remove(index)));
        removed.expect("index checked by Vec::remove")
    }
    pub fn len(&self) -> usize {
        self.state.read().len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Clone)]
pub struct SnapshotStateMap<
    K: Eq + Hash + Clone + PartialEq + 'static,
    V: Clone + PartialEq + 'static,
> {
    state: MutableState<HashMap<K, V>>,
}

impl<K: Eq + Hash + Clone + PartialEq + 'static, V: Clone + PartialEq + 'static>
    SnapshotStateMap<K, V>
{
    pub fn new(values: HashMap<K, V>) -> Self {
        Self {
            state: mutable_state_of(values),
        }
    }
    pub fn get(&self) -> HashMap<K, V> {
        self.state.get()
    }
    pub fn insert(&self, key: K, value: V) {
        self.state.update(|values| {
            values.insert(key, value);
        });
    }
    pub fn remove(&self, key: &K) -> Option<V> {
        let mut removed = None;
        self.state.update(|values| removed = values.remove(key));
        removed
    }
}
