use core::{cell::UnsafeCell, sync::atomic::AtomicUsize};

pub struct Fifo<T: Copy, const S: usize, P: FifoPolicy<T>> {
    buffer: [UnsafeCell<T>; S],
    policy: P,
}

impl<T: Copy, const S: usize, P: FifoPolicy<T>> Fifo<T, S, P> {
    pub const fn new(policy: P) -> Self {
        Self {
            buffer: [const { UnsafeCell::new(unsafe { core::mem::zeroed() }) }; S],
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
    fn push(&self, buffer: &[UnsafeCell<T>], item: T) -> Result<(), ()>;
    fn pop(&self, buffer: &[UnsafeCell<T>]) -> Result<T, ()>;
}

/// This policy is a SPSC (single producer, single consumer) lock-free FIFO queue. Only one thread
/// shall call `push` and only one thread shall call `pop`. Failure to follow this rule will result
/// in undefined behavior.
pub struct LockFreePolicy {
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl LockFreePolicy {
    pub const fn new() -> Self {
        Self {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }
}

impl<T: Copy> FifoPolicy<T> for LockFreePolicy {
    fn push(&self, buffer: &[UnsafeCell<T>], item: T) -> Result<(), ()> {
        let head = self.head.load(core::sync::atomic::Ordering::Relaxed);
        let next = (head + 1) % buffer.len();
        if next == self.tail.load(core::sync::atomic::Ordering::Acquire) {
            return Err(());
        }
        unsafe { *buffer[head].get() = item };
        self.head.store(next, core::sync::atomic::Ordering::Release);
        Ok(())
    }

    fn pop(&self, buffer: &[UnsafeCell<T>]) -> Result<T, ()> {
        let tail = self.tail.load(core::sync::atomic::Ordering::Relaxed);
        let head = self.head.load(core::sync::atomic::Ordering::Acquire);
        if head == tail {
            return Err(());
        }
        let item = unsafe { *buffer[tail].get() };
        self.tail.store(
            (tail + 1) % buffer.len(),
            core::sync::atomic::Ordering::Release,
        );
        Ok(item)
    }
}
