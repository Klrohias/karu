use crate::composition::{Composer, InvalidationHandle, remember_slot};
use std::cell::{Ref, RefCell};
use std::ops::Deref;
use std::rc::Rc;

pub struct State<T: 'static> {
    value: Rc<RefCell<T>>,
    invalidation: InvalidationHandle,
}

impl<T: 'static> Clone for State<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            invalidation: self.invalidation.clone(),
        }
    }
}

impl<T: 'static> State<T> {
    pub fn read(&self) -> StateRead<'_, T> {
        StateRead {
            value: self.value.borrow(),
        }
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.value.borrow().clone()
    }

    pub fn set(&self, value: T) {
        *self.value.borrow_mut() = value;
        self.invalidation.invalidate();
    }

    pub fn update(&self, f: impl FnOnce(&mut T)) {
        f(&mut self.value.borrow_mut());
        self.invalidation.invalidate();
    }
}

impl<T: Clone + 'static> State<Vec<T>> {
    pub fn iter(&self) -> std::vec::IntoIter<T> {
        self.get().into_iter()
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

pub fn remember_state<T: 'static>(initializer: impl FnOnce() -> T) -> State<T> {
    State {
        value: Rc::new(RefCell::new(initializer())),
        invalidation: InvalidationHandle::detached(),
    }
}

#[doc(hidden)]
pub fn __karu_remember_state<T: 'static>(
    composer: &mut Composer,
    initializer: impl FnOnce() -> T,
) -> State<T> {
    let (value, invalidation) = remember_slot(composer, initializer)
        .expect("remember_state slot type changed during Karu composition");
    State {
        value,
        invalidation,
    }
}
