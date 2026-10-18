//! Chunked storage for the nodes and the attributes of a tree.
//!
//! An item is addressed by one number, its index. The items live in chunks of a fixed size, and the
//! chunks of a tree are shared with its clones until one of them changes an item: only the chunk
//! that holds it is copied. A tree of a million nodes is cloned by copying a few thousand pointers,
//! and an edit that touches fifty paragraphs copies the chunks those paragraphs are in.
//!
//! Chunks are also what stops an arena from over-allocating. A `Vec` that grows by doubling holds
//! up to twice what it needs when it stops growing, and a parsed part is never grown again, so an
//! arena of that kind holds about 70 per cent more than it uses (the arena of a dense contract is
//! 4.0 times the XML and a `Vec` of it is 6.9). A chunked arena holds at most one chunk more than
//! it uses, and never copies what it has already stored to make room.
//!
//! Two forms of one structure. A `Builder` owns its chunks outright, so the parser changes items
//! without the cost of finding out whether anyone shares them. An `Arena` holds its chunks behind
//! `Arc`, which is how a clone shares them, and copies a chunk when it changes one that is shared.
//! `Builder::freeze` makes the second from the first without moving any item.

use std::marker::PhantomData;
use std::sync::Arc;

/// How a chunk is held.
pub(super) trait Chunk<T>: Sized {
    /// An empty chunk with room for `capacity` items.
    fn empty(capacity: usize) -> Self;
    /// The items.
    fn items(&self) -> &Vec<T>;
    /// The items, for changing. A chunk that is shared is copied first.
    fn items_mut(&mut self) -> &mut Vec<T>;
    /// The item at `at` for changing, or `None` if the chunk holds no item there. A chunk that is
    /// shared is copied only when there is an item to change.
    fn item_mut(&mut self, at: usize) -> Option<&mut T>;
}

impl<T> Chunk<T> for Vec<T> {
    fn empty(capacity: usize) -> Self {
        Vec::with_capacity(capacity)
    }

    fn items(&self) -> &Vec<T> {
        self
    }

    fn items_mut(&mut self) -> &mut Vec<T> {
        self
    }

    #[inline]
    fn item_mut(&mut self, at: usize) -> Option<&mut T> {
        self.get_mut(at)
    }
}

impl<T: Clone> Chunk<T> for Arc<Vec<T>> {
    fn empty(capacity: usize) -> Self {
        Arc::new(Vec::with_capacity(capacity))
    }

    fn items(&self) -> &Vec<T> {
        self
    }

    fn items_mut(&mut self) -> &mut Vec<T> {
        Arc::make_mut(self)
    }

    #[inline]
    fn item_mut(&mut self, at: usize) -> Option<&mut T> {
        if at < self.len() {
            Arc::make_mut(self).get_mut(at)
        } else {
            None
        }
    }
}

/// Items in chunks of `1 << SHIFT`, held as `C`. See the module notes.
pub(super) struct Chunked<T, C, const SHIFT: u32> {
    chunks: Vec<C>,
    /// How many items are stored. Unused room at the end of a chunk is not counted.
    items: usize,
    /// How many items the last chunk holds, which is 0 when there is no chunk. It is the length of
    /// that chunk, kept here so that a push does not have to look at the chunk to find where it goes.
    last_len: usize,
    marker: PhantomData<T>,
}

/// Chunks that clones share.
pub(super) type Arena<T, const SHIFT: u32> = Chunked<T, Arc<Vec<T>>, SHIFT>;

/// Chunks that nobody shares, for building an arena.
pub(super) type Builder<T, const SHIFT: u32> = Chunked<T, Vec<T>, SHIFT>;

impl<T, C: Clone, const SHIFT: u32> Clone for Chunked<T, C, SHIFT> {
    fn clone(&self) -> Self {
        Chunked {
            chunks: self.chunks.clone(),
            items: self.items,
            last_len: self.last_len,
            marker: PhantomData,
        }
    }
}

impl<T: Clone, C: Chunk<T>, const SHIFT: u32> Chunked<T, C, SHIFT> {
    /// How many items a chunk holds.
    pub(super) const SIZE: usize = 1 << SHIFT;
    const MASK: usize = (1 << SHIFT) - 1;

    pub(super) fn new() -> Self {
        Chunked {
            chunks: Vec::new(),
            items: 0,
            last_len: 0,
            marker: PhantomData,
        }
    }

    /// How many items are stored.
    pub(super) fn len(&self) -> usize {
        self.items
    }

    /// How many items the chunks have room for in all, used or not.
    #[cfg(test)]
    pub(super) fn capacity(&self) -> usize {
        self.chunks.len() * Self::SIZE
    }

    /// The index the next item gets. It is `len` unless a list that did not fit in the room left
    /// in a chunk went to the next one (`push_slice`), which leaves that room unused.
    pub(super) fn end(&self) -> usize {
        match self.chunks.len().checked_sub(1) {
            None => 0,
            Some(last) => (last << SHIFT) + self.last_len,
        }
    }

    #[inline]
    pub(super) fn get(&self, index: usize) -> Option<&T> {
        self.chunks
            .get(index >> SHIFT)?
            .items()
            .get(index & Self::MASK)
    }

    /// The item for changing, or `None` if there is none at `index`. A chunk that is shared is
    /// copied only when there is an item in it to change.
    #[inline]
    pub(super) fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.chunks
            .get_mut(index >> SHIFT)?
            .item_mut(index & Self::MASK)
    }

    /// `len` items from `start`, which were stored together by `push_slice`.
    pub(super) fn slice(&self, start: usize, len: usize) -> Option<&[T]> {
        let at = start & Self::MASK;
        self.chunks
            .get(start >> SHIFT)?
            .items()
            .get(at..at.checked_add(len)?)
    }

    /// Room for `n` more items in the last chunk, or a new chunk if there is not.
    #[inline]
    fn make_room(&mut self, n: usize) {
        if self.chunks.is_empty() || self.last_len + n > Self::SIZE {
            self.chunks.push(C::empty(Self::SIZE));
            self.last_len = 0;
        }
    }

    /// Stores an item and returns its index.
    #[inline]
    pub(super) fn push(&mut self, item: T) -> usize {
        self.make_room(1);
        let index = self.end();
        if let Some(chunk) = self.chunks.last_mut() {
            chunk.items_mut().push(item);
            self.items += 1;
            self.last_len += 1;
        }
        index
    }

    /// Adds an item to the list that starts at `*first` and ends where the arena does, which is how
    /// a parser stores the attributes of a start tag as it reads them. It says whether the item
    /// fit. When the last chunk is full, the list so far moves to a new chunk and `*first` is where
    /// it starts now, because a list is in one chunk. A list has at most a chunk of items.
    #[inline]
    pub(super) fn push_listed(&mut self, first: &mut usize, item: T) -> bool {
        if self.chunks.is_empty() || self.last_len >= Self::SIZE {
            let so_far = self.end().saturating_sub(*first);
            if so_far >= Self::SIZE {
                return false;
            }
            self.move_list_to_new_chunk(first, so_far);
        }
        match self.chunks.last_mut() {
            Some(chunk) => {
                chunk.items_mut().push(item);
                self.items += 1;
                self.last_len += 1;
                true
            }
            None => false,
        }
    }

    /// Starts a new chunk, and takes the last `so_far` items of the last one with it.
    #[cold]
    fn move_list_to_new_chunk(&mut self, first: &mut usize, so_far: usize) {
        let moved: Vec<T> = match self.chunks.last_mut() {
            Some(chunk) if so_far > 0 => {
                let items = chunk.items_mut();
                let at = items.len().saturating_sub(so_far);
                items.split_off(at)
            }
            _ => Vec::new(),
        };
        let mut fresh = C::empty(Self::SIZE);
        let count = moved.len();
        fresh.items_mut().extend(moved);
        self.chunks.push(fresh);
        self.last_len = count;
        *first = (self.chunks.len() - 1) << SHIFT;
    }

    /// Stores items next to each other, all in one chunk, and returns the index of the first. A
    /// list that does not fit in the room left in the last chunk goes to a new one. `None` if the
    /// list is longer than a chunk.
    pub(super) fn push_slice(&mut self, list: &[T]) -> Option<usize> {
        if list.len() > Self::SIZE {
            return None;
        }
        if list.is_empty() {
            return Some(self.end());
        }
        self.make_room(list.len());
        let index = self.end();
        let chunk = self.chunks.last_mut()?;
        // One at a time: a list is a few items, and the room is there, so this costs less than the
        // call that copies a slice.
        let items = chunk.items_mut();
        for item in list {
            items.push(item.clone());
        }
        self.items += list.len();
        self.last_len += list.len();
        Some(index)
    }

    /// Drops every item from `index` on, when `index` is in the last chunk. Used to take back the
    /// list that was stored last.
    pub(super) fn truncate_last(&mut self, index: usize) {
        let in_last = self.chunks.len().checked_sub(1) == Some(index >> SHIFT);
        if !in_last {
            return;
        }
        if let Some(chunk) = self.chunks.last_mut() {
            let keep = index & Self::MASK;
            let had = chunk.items().len();
            if keep < had {
                chunk.items_mut().truncate(keep);
                self.items -= had - keep;
                self.last_len = keep;
            }
        }
    }

    /// Every item, for changing all of them. Every chunk that is shared is copied.
    #[cfg(any(test, feature = "fuzzing"))]
    pub(super) fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.chunks
            .iter_mut()
            .flat_map(|c| c.items_mut().iter_mut())
    }
}

impl<T: Clone, const SHIFT: u32> Builder<T, SHIFT> {
    /// The same items, in an arena. No item moves: each chunk is put behind an `Arc` as it is. The
    /// last chunk gives back the room it did not fill, which is most of a chunk for a small part.
    pub(super) fn freeze(mut self) -> Arena<T, SHIFT> {
        if let Some(last) = self.chunks.last_mut() {
            last.shrink_to_fit();
        }
        Chunked {
            chunks: self.chunks.into_iter().map(Arc::new).collect(),
            items: self.items,
            last_len: self.last_len,
            marker: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Small = Arena<u32, 3>;
    type SmallBuilder = Builder<u32, 3>;

    fn built(n: u32) -> Small {
        let mut b = SmallBuilder::new();
        for i in 0..n {
            assert_eq!(b.push(i), i as usize);
        }
        b.freeze()
    }

    /// How many chunks the two arenas hold in common: the same memory, not equal items.
    fn shared(a: &Small, b: &Small) -> usize {
        a.chunks
            .iter()
            .zip(&b.chunks)
            .filter(|(x, y)| Arc::ptr_eq(x, y))
            .count()
    }

    fn all(a: &Small) -> Vec<u32> {
        (0..a.end()).filter_map(|i| a.get(i).copied()).collect()
    }

    #[test]
    fn items_are_addressed_by_one_number_across_chunks() {
        let a = built(20);
        assert_eq!((a.len(), a.end()), (20, 20));
        assert_eq!(all(&a), (0..20).collect::<Vec<_>>());
        assert_eq!(a.get(19), Some(&19));
        assert_eq!(a.get(20), None);
        assert_eq!(a.get(1_000_000), None);
    }

    #[test]
    fn a_clone_shares_every_chunk_and_copies_only_the_chunk_it_changes() {
        let a = built(20);
        let mut b = a.clone();
        assert_eq!(shared(&b, &a), 3);
        *b.get_mut(9).unwrap() = 900;
        // One chunk was copied. The other two are still the same memory.
        assert_eq!(shared(&b, &a), 2);
        assert_eq!(a.get(9), Some(&9));
        assert_eq!(b.get(9), Some(&900));
        // A second change in the same chunk copies nothing more.
        *b.get_mut(10).unwrap() = 1000;
        assert_eq!(shared(&b, &a), 2);
        // Pushing changes the last chunk only.
        let mut c = a.clone();
        assert_eq!(c.push(77), 20);
        assert_eq!(shared(&c, &a), 2);
        assert_eq!((a.len(), c.len()), (20, 21));
        assert_eq!(all(&a), (0..20).collect::<Vec<_>>());
        // Looking for an item that is not there copies nothing.
        let mut d = a.clone();
        assert!(d.get_mut(20).is_none());
        assert!(d.get_mut(99).is_none());
        assert_eq!(shared(&d, &a), 3);
    }

    #[test]
    fn a_list_is_stored_in_one_chunk_and_what_is_left_of_the_last_one_is_not_used() {
        let mut b = SmallBuilder::new();
        for i in 0..6 {
            b.push(i);
        }
        // Two items are left in the first chunk, so a list of three goes to the second.
        assert_eq!(b.push_slice(&[100, 101, 102]), Some(8));
        assert_eq!((b.len(), b.end()), (9, 11));
        assert_eq!(b.slice(8, 3), Some(&[100, 101, 102][..]));
        // A list that fits goes after it, and an empty list takes no room.
        assert_eq!(b.push_slice(&[103, 104]), Some(11));
        assert_eq!(b.push_slice(&[]), Some(13));
        assert_eq!(b.slice(13, 0), Some(&[][..]));
        // A list of a whole chunk is fine, and one of more is not.
        assert_eq!(b.push_slice(&[0; 8]), Some(16));
        assert_eq!(b.push_slice(&[0; 9]), None);
        // The unused room holds nothing.
        assert_eq!(b.get(5), Some(&5));
        assert_eq!(b.get(6), None);
        assert_eq!(b.get(7), None);
        assert_eq!(b.slice(6, 3), None);
        assert_eq!(b.len(), 6 + 3 + 2 + 8);
    }

    #[test]
    fn a_list_that_grows_an_item_at_a_time_stays_in_one_chunk() {
        let mut b = SmallBuilder::new();
        // In the middle of a chunk: the list stays where it started.
        for i in 0..3 {
            b.push(i);
        }
        let mut first = b.end();
        for i in 0..4 {
            assert!(b.push_listed(&mut first, 100 + i));
        }
        assert_eq!(first, 3);
        assert_eq!(b.slice(first, 4), Some(&[100, 101, 102, 103][..]));
        // Past the end of the chunk: the list moves, with what it had, to the next one.
        let mut first = b.end();
        assert_eq!(first, 7);
        for i in 0..3 {
            assert!(b.push_listed(&mut first, 200 + i));
        }
        assert_eq!(first, 8);
        assert_eq!(b.slice(first, 3), Some(&[200, 201, 202][..]));
        // What was left of the first chunk holds nothing now, and the others are as they were.
        assert_eq!(b.get(7), None);
        assert_eq!(b.slice(3, 4), Some(&[100, 101, 102, 103][..]));
        assert_eq!(b.len(), 3 + 4 + 3);
        assert_eq!(b.end(), 11);
        // A list that starts at the beginning of a full chunk's successor, and one of a whole chunk.
        let mut first = b.end();
        for i in 0..5 {
            assert!(b.push_listed(&mut first, 300 + i));
        }
        assert_eq!(first, 11);
        assert_eq!(b.end(), 16);
        let mut first = b.end();
        for i in 0..8 {
            assert!(b.push_listed(&mut first, 400 + i));
        }
        assert_eq!(first, 16);
        // One more than a chunk does not fit.
        assert!(!b.push_listed(&mut first, 999));
        assert_eq!(b.slice(16, 8).map(<[u32]>::len), Some(8));
    }

    #[test]
    fn a_list_in_an_arena_that_is_shared_is_copied_with_its_chunk() {
        let mut a = built(5);
        let before = a.clone();
        let mut first = a.end();
        for i in 0..6 {
            assert!(a.push_listed(&mut first, 50 + i));
        }
        assert_eq!(first, 8);
        assert_eq!(a.slice(first, 6).map(<[u32]>::len), Some(6));
        assert_eq!((before.len(), a.len()), (5, 11));
        assert_eq!(all(&before), (0..5).collect::<Vec<_>>());
    }

    #[test]
    fn the_list_that_was_stored_last_can_be_taken_back() {
        let mut b = SmallBuilder::new();
        b.push_slice(&[1, 2, 3]);
        let at = b.push_slice(&[4, 5]).unwrap();
        assert_eq!(b.end(), 5);
        b.truncate_last(at);
        assert_eq!((b.len(), b.end()), (3, 3));
        assert_eq!(b.push_slice(&[6, 7]), Some(3));
        // An index in an earlier chunk is left alone.
        b.push_slice(&[0; 8]);
        b.truncate_last(0);
        assert_eq!(b.len(), 3 + 2 + 8);
    }

    #[test]
    fn an_arena_changes_a_shared_chunk_without_the_others_seeing_it() {
        let mut a = built(10);
        let b = a.clone();
        a.truncate_last(8);
        assert_eq!((a.len(), b.len()), (8, 10));
        assert_eq!(b.get(9), Some(&9));
        assert_eq!(a.get(9), None);
        let at = a.push_slice(&[5, 6]).unwrap();
        assert_eq!(at, 8);
        assert_eq!(all(&b), (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn a_chunk_is_allocated_whole_and_nothing_is_copied_to_grow() {
        let mut b = SmallBuilder::new();
        for i in 0..100 {
            b.push(i);
        }
        assert_eq!(b.chunks.len(), 13);
        assert!(b.chunks.iter().all(|c| c.capacity() == 8));
        // Every chunk but the last is full, and the last holds no more than it uses.
        let a = b.freeze();
        let capacities: Vec<usize> = a.chunks.iter().map(|c| c.capacity()).collect();
        assert_eq!(capacities.len(), 13);
        assert!(capacities[..12].iter().all(|&c| c == 8));
        assert_eq!(capacities[12], 4);
    }

    #[test]
    fn a_small_arena_is_no_larger_than_what_it_holds() {
        let mut b = SmallBuilder::new();
        b.push(1);
        b.push(2);
        let a = b.freeze();
        assert_eq!(a.chunks[0].capacity(), 2);
        // It can still grow.
        let mut a = a;
        for i in 0..20 {
            a.push(i);
        }
        assert_eq!(a.len(), 22);
        assert_eq!(a.get(21), Some(&19));
    }
}
