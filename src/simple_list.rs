use alloc::vec::Vec;
use core::ops::{Index, IndexMut};

/// 'SimpleList' is an indexable list that allows items to be added and removed without shifting
/// other elements.
///
/// h2. Important Notes
/// - Every index of the Vec may not have a value.
/// - New items are added to the first available index.
/// - The length refers to the number of Some values.
/// - The capacity refers to the current number of indicies allocated
pub struct SimpleList<T> {
    data: Vec<Option<T>>,
    length: usize,
}

impl<T> SimpleList<T> {
    pub const fn new() -> Self {
        Self {
            data: Vec::new(),
            length: 0,
        }
    }

    pub fn push(&mut self, item: T) -> usize {
        if let Some((index, slot)) = self
            .data
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.is_none())
        {
            *slot = Some(item);
            self.length += 1;
            index
        } else {
            self.data.push(Some(item));
            self.length += 1;
            self.data.len() - 1
        }
    }

    pub fn remove(&mut self, index: usize) {
        if let Some(slot) = self.data.get_mut(index) {
            *slot = None;
            self.length -= 1;
        }
    }

    pub fn len(&self) -> usize {
        self.length
    }

    pub fn capacity(&self) -> usize {
        self.data.capacity()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Option<T>> {
        self.data.iter()
    }

    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut T) -> bool,
    {
        for (_, slot) in self.data.iter_mut().enumerate() {
            if let Some(item) = slot {
                if !f(item) {
                    *slot = None;
                    self.length -= 1;
                }
            }
        }
    }
}

impl<T> Index<usize> for SimpleList<T> {
    type Output = Option<T>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.data[index]
    }
}

impl<T> IndexMut<usize> for SimpleList<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.data[index]
    }
}
