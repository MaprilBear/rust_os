//! Helpers shared by the suite images in `src/bin`.
#![no_std]

use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering::SeqCst};

/// An append-only log of one-byte events, for asserting the order in which
/// threads ran. Lives in user RAM; any thread may append.
pub struct EventLog<const N: usize> {
    events: [AtomicU8; N],
    len: AtomicUsize,
}

impl<const N: usize> EventLog<N> {
    pub const fn new() -> Self {
        Self {
            events: [const { AtomicU8::new(0) }; N],
            len: AtomicUsize::new(0),
        }
    }

    pub fn clear(&self) {
        self.len.store(0, SeqCst);
    }

    pub fn push(&self, event: u8) {
        let i = self.len.fetch_add(1, SeqCst);
        assert!(i < N, "event log full");
        self.events[i].store(event, SeqCst);
    }

    /// The events so far, as a string (events are ASCII letters/digits).
    pub fn get(&self) -> Snapshot<N> {
        let len = self.len.load(SeqCst).min(N);
        let mut bytes = [0; N];
        for (b, e) in bytes.iter_mut().zip(&self.events[..len]) {
            *b = e.load(SeqCst);
        }
        Snapshot { bytes, len }
    }
}

impl<const N: usize> Default for EventLog<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// A copy of an [`EventLog`], comparable with a `&str`.
pub struct Snapshot<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> Snapshot<N> {
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("<non-ASCII>")
    }
}

impl<const N: usize> core::fmt::Debug for Snapshot<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

impl<const N: usize> PartialEq<&str> for Snapshot<N> {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}
