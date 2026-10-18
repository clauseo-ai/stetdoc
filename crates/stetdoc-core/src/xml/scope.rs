//! A stack of namespace bindings that answers "what does this prefix mean here" in one lookup.
//!
//! Each prefix has its own stack of values, and one undo list records the order in which prefixes
//! were bound. Closing a scope pops exactly the bindings made inside it. So opening a scope,
//! looking a prefix up and closing the scope each cost time in proportion to the bindings they
//! touch, and never to the bindings that are merely in scope. A part with thousands of
//! declarations nested thousands deep cannot make resolution quadratic.

use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;

pub(super) struct ScopeStack<K, V> {
    stacks: HashMap<K, Vec<V>>,
    undo: Vec<K>,
    /// Changes whenever an answer could have changed: on every binding, and on every close that
    /// drops one. Callers use it to keep a small cache of recent answers.
    epoch: u64,
}

impl<K: Hash + Eq + Clone, V: Copy> ScopeStack<K, V> {
    pub(super) fn new() -> Self {
        ScopeStack {
            stacks: HashMap::new(),
            undo: Vec::new(),
            epoch: 0,
        }
    }

    /// A number that is the same for two moments only if no binding changed between them.
    pub(super) fn epoch(&self) -> u64 {
        self.epoch
    }

    /// A marker for the current scope depth. Pass it to `close` to drop what was bound since.
    pub(super) fn mark(&self) -> usize {
        self.undo.len()
    }

    /// Binds `key` to `value` until the scope that contains this call closes.
    pub(super) fn bind(&mut self, key: K, value: V) {
        self.stacks.entry(key.clone()).or_default().push(value);
        self.undo.push(key);
        self.epoch += 1;
    }

    /// The innermost value bound to `key`.
    pub(super) fn get<Q>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.stacks.get(key).and_then(|s| s.last().copied())
    }

    /// Drops every binding made since `mark`.
    pub(super) fn close(&mut self, mark: usize) {
        if self.undo.len() > mark {
            self.epoch += 1;
        }
        while self.undo.len() > mark {
            let Some(key) = self.undo.pop() else {
                return;
            };
            if let Some(stack) = self.stacks.get_mut(&key) {
                stack.pop();
                if stack.is_empty() {
                    self.stacks.remove(&key);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inner_bindings_shadow_and_then_restore() {
        let mut s: ScopeStack<&str, u16> = ScopeStack::new();
        s.bind("a", 1);
        let m = s.mark();
        s.bind("a", 2);
        s.bind("b", 3);
        assert_eq!((s.get("a"), s.get("b")), (Some(2), Some(3)));
        s.close(m);
        assert_eq!((s.get("a"), s.get("b")), (Some(1), None));
        s.close(0);
        assert_eq!(s.get("a"), None);
    }
}
