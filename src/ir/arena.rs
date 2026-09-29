//! Typed ID newtypes and the tombstoning [`Arena`] every IR entity lives in.
//!
//! IDs are indices into their arena. Removing an entry leaves a tombstone
//! instead of shifting later entries, so an ID stays valid (or reports
//! `None`) for the whole life of the arena and is never reused -- passes can
//! delete nodes without renumbering anything that still points at others.

use std::fmt;
use std::marker::PhantomData;

/// An index-like ID usable as an [`Arena`] key.
pub trait ArenaId: Copy + Eq + fmt::Debug {
    fn from_index(index: usize) -> Self;
    fn index(self) -> usize;
}

macro_rules! arena_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub u32);

        impl ArenaId for $name {
            fn from_index(index: usize) -> Self {
                $name(u32::try_from(index).expect("IR arena exceeded u32::MAX entries"))
            }
            fn index(self) -> usize {
                self.0 as usize
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }
    };
}

arena_id!(
    /// A module definition in a [`crate::ir::Design`].
    ModuleId
);
arena_id!(
    /// A cell (gate or module instance) -- a hypergraph node.
    CellId
);
arena_id!(
    /// A net -- a hypergraph hyperedge joining any number of pins.
    NetId
);
arena_id!(
    /// A connection point on a cell or port.
    PinId
);
arena_id!(
    /// A module boundary port.
    PortId
);

/// `Vec<Option<T>>` keyed by a typed ID. See the module docs.
#[derive(Clone)]
pub struct Arena<I, T> {
    slots: Vec<Option<T>>,
    live: usize,
    _id: PhantomData<I>,
}

impl<I: ArenaId, T> Default for Arena<I, T> {
    fn default() -> Self {
        Arena { slots: Vec::new(), live: 0, _id: PhantomData }
    }
}

impl<I: ArenaId, T> Arena<I, T> {
    pub fn alloc(&mut self, value: T) -> I {
        let id = I::from_index(self.slots.len());
        self.slots.push(Some(value));
        self.live += 1;
        id
    }

    pub fn get(&self, id: I) -> Option<&T> {
        self.slots.get(id.index()).and_then(Option::as_ref)
    }

    pub fn get_mut(&mut self, id: I) -> Option<&mut T> {
        self.slots.get_mut(id.index()).and_then(Option::as_mut)
    }

    pub fn contains(&self, id: I) -> bool {
        self.get(id).is_some()
    }

    /// Tombstones `id` and returns what was there. The ID is never reused.
    pub fn remove(&mut self, id: I) -> Option<T> {
        let taken = self.slots.get_mut(id.index()).and_then(Option::take);
        if taken.is_some() {
            self.live -= 1;
        }
        taken
    }

    /// Number of live (non-removed) entries.
    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// Live entries in ID order.
    pub fn iter(&self) -> impl Iterator<Item = (I, &T)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| slot.as_ref().map(|v| (I::from_index(i), v)))
    }

    /// Live IDs in order. Collected, so the arena can be mutated while
    /// walking them.
    pub fn ids(&self) -> Vec<I> {
        self.iter().map(|(id, _)| id).collect()
    }
}

impl<I: ArenaId, T> std::ops::Index<I> for Arena<I, T> {
    type Output = T;
    fn index(&self, id: I) -> &T {
        self.get(id).unwrap_or_else(|| panic!("{id:?} is not a live entry"))
    }
}

impl<I: ArenaId, T> std::ops::IndexMut<I> for Arena<I, T> {
    fn index_mut(&mut self, id: I) -> &mut T {
        self.get_mut(id).unwrap_or_else(|| panic!("{id:?} is not a live entry"))
    }
}

/// Prints only live entries, as `{ Id(n): value, ... }`.
impl<I: ArenaId, T: fmt::Debug> fmt::Debug for Arena<I, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removed_ids_are_tombstoned_not_reused() {
        let mut a: Arena<NetId, &str> = Arena::default();
        let x = a.alloc("x");
        let y = a.alloc("y");
        assert_eq!(a.remove(x), Some("x"));
        assert!(!a.contains(x));
        assert_eq!(a[y], "y");
        let z = a.alloc("z");
        assert_ne!(z, x);
        assert_eq!(a.len(), 2);
        assert_eq!(a.ids(), vec![y, z]);
    }
}
