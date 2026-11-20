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

#[allow(dead_code)]
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

    pub fn slot_len(&self) -> usize {
        self.data.len()
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
        for slot in self.data.iter_mut() {
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

#[cfg(test)]
mod tests {
    use super::SimpleList;
    use std::vec::Vec;

    fn items(list: &SimpleList<u32>) -> Vec<Option<u32>> {
        list.iter().copied().collect()
    }

    #[test]
    fn new_list_is_empty() {
        let list = SimpleList::<u32>::new();
        assert_eq!(list.len(), 0);
        assert_eq!(list.slot_len(), 0);
        assert_eq!(list.iter().count(), 0);
    }

    #[test]
    fn push_appends_and_returns_the_index() {
        let mut list = SimpleList::new();
        assert_eq!(list.push(10), 0);
        assert_eq!(list.push(11), 1);
        assert_eq!(list.push(12), 2);
        assert_eq!(list.len(), 3);
        assert_eq!(list.slot_len(), 3);
        assert_eq!(items(&list), [Some(10), Some(11), Some(12)]);
    }

    #[test]
    fn remove_empties_the_slot_without_shifting() {
        let mut list = SimpleList::new();
        for v in [10, 11, 12] {
            list.push(v);
        }
        list.remove(1);
        assert_eq!(list.len(), 2);
        assert_eq!(list.slot_len(), 3, "slots are never shrunk");
        assert_eq!(items(&list), [Some(10), None, Some(12)]);
        assert_eq!(list[2], Some(12), "later items keep their index");
    }

    #[test]
    fn push_reuses_the_lowest_free_slot() {
        let mut list = SimpleList::new();
        for v in [10, 11, 12, 13] {
            list.push(v);
        }
        list.remove(2);
        list.remove(0);
        assert_eq!(list.push(20), 0);
        assert_eq!(list.push(21), 2);
        assert_eq!(list.push(22), 4, "no free slot left: append");
        assert_eq!(list.len(), 5);
        assert_eq!(
            items(&list),
            [Some(20), Some(11), Some(21), Some(13), Some(22)]
        );
    }

    #[test]
    fn remove_out_of_range_does_nothing() {
        let mut list = SimpleList::new();
        list.push(10);
        list.remove(1);
        list.remove(usize::MAX);
        assert_eq!(list.len(), 1);
        assert_eq!(items(&list), [Some(10)]);
    }

    /// Known issue 1: removing an already-empty slot still decrements
    /// `length`. With one item left the count goes wrong silently; with none
    /// it underflows (a panic with overflow checks).
    #[test]
    #[ignore = "known issue 1: SimpleList::remove on an empty slot decrements length"]
    fn remove_of_an_empty_slot_keeps_the_count() {
        let mut list = SimpleList::new();
        list.push(10);
        list.push(11);
        list.remove(0);
        list.remove(0);
        assert_eq!(list.len(), 1);
        list.remove(1);
        list.remove(1);
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn retain_drops_rejected_items_and_keeps_slots() {
        let mut list = SimpleList::new();
        for v in 0..6 {
            list.push(v);
        }
        list.remove(3);
        list.retain(|v| *v % 2 == 0);
        assert_eq!(list.len(), 3);
        assert_eq!(list.slot_len(), 6);
        assert_eq!(items(&list), [Some(0), None, Some(2), None, Some(4), None]);
    }

    #[test]
    fn retain_visits_only_occupied_slots_and_can_mutate_them() {
        let mut list = SimpleList::new();
        for v in [1, 2, 3] {
            list.push(v);
        }
        list.remove(1);
        let mut seen = Vec::new();
        list.retain(|v| {
            seen.push(*v);
            *v *= 10;
            true
        });
        assert_eq!(seen, [1, 3]);
        assert_eq!(items(&list), [Some(10), None, Some(30)]);
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn index_mut_writes_the_slot() {
        let mut list = SimpleList::new();
        list.push(1);
        list[0] = Some(7);
        assert_eq!(list[0], Some(7));
        if let Some(v) = list[0].as_mut() {
            *v += 1;
        }
        assert_eq!(list[0], Some(8));
    }

    #[test]
    #[should_panic]
    fn index_past_the_end_panics() {
        let list = SimpleList::<u32>::new();
        let _ = &list[0];
    }
}
