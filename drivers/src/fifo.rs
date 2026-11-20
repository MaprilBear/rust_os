// `Result<_, ()>`: full/empty carry no further information. Whether to give the
// FIFO real error types is to be decided when its tests are written.
#![allow(clippy::result_unit_err)]

// Under `--cfg loom` (`cargo xtask test-loom`) the atomics and the cell come
// from loom, so its model checker sees every access. Loom's types are not
// `const`, so the constructors have a non-const twin for that build.
#[cfg(not(loom))]
use core::sync::atomic::{AtomicUsize, Ordering};
#[cfg(loom)]
use loom::sync::atomic::{AtomicUsize, Ordering};

/// One buffer slot: an `UnsafeCell` (loom's under `cfg(loom)`).
pub struct Slot<T> {
    #[cfg(not(loom))]
    cell: core::cell::UnsafeCell<T>,
    #[cfg(loom)]
    cell: loom::cell::UnsafeCell<T>,
}

impl<T: Copy> Slot<T> {
    #[cfg(not(loom))]
    const fn new(value: T) -> Self {
        Self {
            cell: core::cell::UnsafeCell::new(value),
        }
    }

    #[cfg(loom)]
    fn new(value: T) -> Self {
        Self {
            cell: loom::cell::UnsafeCell::new(value),
        }
    }

    /// # Safety
    /// No write to this slot may run concurrently.
    unsafe fn read(&self) -> T {
        #[cfg(not(loom))]
        return *self.cell.get();
        #[cfg(loom)]
        return self.cell.with(|p| *p);
    }

    /// # Safety
    /// No other access to this slot may run concurrently.
    unsafe fn write(&self, value: T) {
        #[cfg(not(loom))]
        {
            *self.cell.get() = value;
        }
        #[cfg(loom)]
        self.cell.with_mut(|p| *p = value);
    }
}

pub struct Fifo<T: Copy, const S: usize, P: FifoPolicy<T>> {
    buffer: [Slot<T>; S],
    policy: P,
}

impl<T: Copy, const S: usize, P: FifoPolicy<T>> Fifo<T, S, P> {
    #[cfg(not(loom))]
    pub const fn new(policy: P) -> Self {
        Self {
            buffer: [const { Slot::new(unsafe { core::mem::zeroed() }) }; S],
            policy,
        }
    }

    #[cfg(loom)]
    pub fn new(policy: P) -> Self {
        Self {
            buffer: core::array::from_fn(|_| Slot::new(unsafe { core::mem::zeroed() })),
            policy,
        }
    }

    pub fn push(&self, item: T) -> Result<(), ()> {
        self.policy.push(&self.buffer, item)
    }

    pub fn pop(&self) -> Result<T, ()> {
        self.policy.pop(&self.buffer)
    }

    /// Drain into `out`, returning how many items were copied.
    pub fn pop_into(&self, out: &mut [T]) -> usize {
        let mut n = 0;
        for slot in out.iter_mut() {
            match self.pop() {
                Ok(v) => {
                    *slot = v;
                    n += 1;
                }
                Err(()) => break,
            }
        }
        n
    }
}

pub trait FifoPolicy<T> {
    fn push(&self, buffer: &[Slot<T>], item: T) -> Result<(), ()>;
    fn pop(&self, buffer: &[Slot<T>]) -> Result<T, ()>;
}

/// This policy is a SPSC (single producer, single consumer) lock-free FIFO queue. Only one thread
/// shall call `push` and only one thread shall call `pop`. Failure to follow this rule will result
/// in undefined behavior.
pub struct LockFreePolicy {
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl LockFreePolicy {
    #[cfg(not(loom))]
    pub const fn new() -> Self {
        Self {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    #[cfg(loom)]
    pub fn new() -> Self {
        Self {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }
}

impl Default for LockFreePolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy> FifoPolicy<T> for LockFreePolicy {
    fn push(&self, buffer: &[Slot<T>], item: T) -> Result<(), ()> {
        let head = self.head.load(Ordering::Relaxed);
        let next = (head + 1) % buffer.len();
        if next == self.tail.load(Ordering::Acquire) {
            return Err(());
        }
        unsafe { buffer[head].write(item) };
        self.head.store(next, Ordering::Release);
        Ok(())
    }

    fn pop(&self, buffer: &[Slot<T>]) -> Result<T, ()> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if head == tail {
            return Err(());
        }
        let item = unsafe { buffer[tail].read() };
        self.tail
            .store((tail + 1) % buffer.len(), Ordering::Release);
        Ok(item)
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    extern crate std;

    use super::*;
    use std::vec::Vec;

    type Fifo8<const S: usize> = Fifo<u8, S, LockFreePolicy>;

    fn drain<const S: usize>(f: &Fifo8<S>) -> Vec<u8> {
        core::iter::from_fn(|| f.pop().ok()).collect()
    }

    #[test]
    fn new_fifo_is_empty() {
        let f = Fifo8::<4>::new(LockFreePolicy::default());
        assert_eq!(f.pop(), Err(()));
    }

    #[test]
    fn items_come_out_in_order() {
        let f = Fifo8::<8>::new(LockFreePolicy::new());
        for b in [3, 1, 4, 1, 5] {
            f.push(b).unwrap();
        }
        assert_eq!(drain(&f), [3, 1, 4, 1, 5]);
        assert_eq!(f.pop(), Err(()));
    }

    #[test]
    fn capacity_is_one_less_than_the_buffer() {
        let f = Fifo8::<4>::new(LockFreePolicy::new());
        for b in 0..3 {
            assert_eq!(f.push(b), Ok(()));
        }
        assert_eq!(f.push(99), Err(()), "full");
        assert_eq!(drain(&f), [0, 1, 2], "a rejected push overwrites nothing");
    }

    #[test]
    fn a_one_slot_buffer_holds_nothing() {
        let f = Fifo8::<1>::new(LockFreePolicy::new());
        assert_eq!(f.push(1), Err(()));
        assert_eq!(f.pop(), Err(()));
    }

    #[test]
    fn indices_wrap_around_many_times() {
        let f = Fifo8::<5>::new(LockFreePolicy::new());
        let mut next_in = 0u8;
        let mut next_out = 0u8;
        for round in 0..200 {
            // Vary the fill level so head and tail wrap at different points.
            for _ in 0..(round % 4 + 1) {
                if f.push(next_in).is_ok() {
                    next_in = next_in.wrapping_add(1);
                }
            }
            for _ in 0..(round % 3 + 1) {
                if let Ok(b) = f.pop() {
                    assert_eq!(b, next_out);
                    next_out = next_out.wrapping_add(1);
                }
            }
        }
        for b in drain(&f) {
            assert_eq!(b, next_out);
            next_out = next_out.wrapping_add(1);
        }
        assert_eq!(next_in, next_out, "nothing lost or duplicated");
    }

    #[test]
    fn pop_into_copies_up_to_the_smaller_of_out_and_contents() {
        let f = Fifo8::<8>::new(LockFreePolicy::new());
        for b in 1..=5 {
            f.push(b).unwrap();
        }
        let mut out = [0u8; 3];
        assert_eq!(f.pop_into(&mut out), 3);
        assert_eq!(out, [1, 2, 3]);
        let mut out = [0u8; 8];
        assert_eq!(f.pop_into(&mut out), 2);
        assert_eq!(out[..2], [4, 5]);
        assert_eq!(out[2..], [0; 6], "the rest of out is untouched");
        assert_eq!(f.pop_into(&mut out), 0);
    }

    #[test]
    fn pop_into_an_empty_slice_takes_nothing() {
        let f = Fifo8::<4>::new(LockFreePolicy::new());
        f.push(7).unwrap();
        assert_eq!(f.pop_into(&mut []), 0);
        assert_eq!(f.pop(), Ok(7));
    }

    #[test]
    fn works_for_wider_copy_types() {
        let f = Fifo::<(u32, u16), 3, LockFreePolicy>::new(LockFreePolicy::new());
        f.push((0xDEAD_BEEF, 7)).unwrap();
        assert_eq!(f.pop(), Ok((0xDEAD_BEEF, 7)));
    }

    /// `Fifo` is not `Sync` on its own (users such as `Uarte` assert it and
    /// keep to one producer and one consumer); this test does the same.
    struct Spsc(Fifo<u32, 16, LockFreePolicy>);
    unsafe impl Sync for Spsc {}

    #[test]
    fn producer_and_consumer_threads_transfer_everything_in_order() {
        static F: Spsc = Spsc(Fifo::new(LockFreePolicy::new()));
        const N: u32 = 100_000;
        let producer = std::thread::spawn(|| {
            for i in 0..N {
                while F.0.push(i).is_err() {
                    std::hint::spin_loop();
                }
            }
        });
        let mut expected = 0;
        while expected < N {
            if let Ok(v) = F.0.pop() {
                assert_eq!(v, expected);
                expected += 1;
            }
        }
        producer.join().unwrap();
        assert_eq!(F.0.pop(), Err(()));
    }
}

/// Loom explores every interleaving (and allowed memory reordering) of these
/// small models: `cargo xtask test-loom`.
#[cfg(all(test, loom))]
mod loom_tests {
    extern crate std;

    use super::*;
    use loom::sync::Arc;
    use loom::thread;
    use std::vec::Vec;

    /// Shared between loom threads; the models keep to the access pattern
    /// they describe (see `Uarte`'s `Sync` impl for the real one).
    struct Shared(Fifo<u32, 3, LockFreePolicy>);
    unsafe impl Send for Shared {}
    unsafe impl Sync for Shared {}

    fn shared() -> Arc<Shared> {
        Arc::new(Shared(Fifo::new(LockFreePolicy::new())))
    }

    /// SPSC: one producer, one consumer, four items through a buffer that
    /// holds two, so it fills up and a consumed slot is written again (that
    /// reuse is what the Acquire/Release pair on `tail` protects). Every item arrives
    /// once, in order, and loom checks the slot accesses for data races
    /// (they rely on the Acquire/Release pairs on head and tail).
    #[test]
    fn spsc_transfers_every_item_in_order() {
        // Unbounded takes minutes; every ordering bug in push/pop shows up
        // within two preemptions (checked by weakening each ordering).
        let mut model = loom::model::Builder::new();
        model.preemption_bound = Some(2);
        model.check(|| {
            let f = shared();
            let producer = {
                let f = f.clone();
                thread::spawn(move || {
                    for i in 1..=4 {
                        while f.0.push(i).is_err() {
                            thread::yield_now();
                        }
                    }
                })
            };
            let consumer = thread::spawn(move || {
                let mut got = Vec::new();
                while got.len() < 4 {
                    match f.0.pop() {
                        Ok(v) => got.push(v),
                        Err(()) => thread::yield_now(),
                    }
                }
                got
            });
            producer.join().unwrap();
            assert_eq!(consumer.join().unwrap(), [1, 2, 3, 4]);
        });
    }

    /// The FIFO is single-consumer by contract: with two consumers, both can
    /// read the same slot and get one item twice. This is why callers must
    /// serialise `pop` (known issue 4: `Uarte::receive` does not; that is
    /// tested on the target).
    #[test]
    #[should_panic(expected = "42 received twice")]
    fn two_consumers_can_get_the_same_item() {
        loom::model(|| {
            let f = shared();
            f.0.push(42).unwrap();
            let other = {
                let f = f.clone();
                thread::spawn(move || f.0.pop().is_ok() as u32)
            };
            let mine = f.0.pop().is_ok() as u32;
            assert_eq!(mine + other.join().unwrap(), 1, "42 received twice");
        });
    }
}
