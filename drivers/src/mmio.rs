use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::ops::Deref;
use core::ptr;

use bitfields::bitfield;
pub struct Ro;
pub struct Wo;
pub struct Rw;
pub trait Readable {}
pub trait Writable {}
impl Readable for Ro {}
impl Readable for Rw {}
impl Writable for Wo {}
impl Writable for Rw {}

#[repr(transparent)]
pub struct Reg<T: Copy, A>(UnsafeCell<T>, PhantomData<A>);
impl<T: Copy, A: Readable> Reg<T, A> {
    pub fn read(&self) -> T {
        unsafe { ptr::read_volatile(self.0.get()) }
    }
}
impl<T: Copy, A: Writable> Reg<T, A> {
    pub fn write(&self, v: T) {
        unsafe { ptr::write_volatile(self.0.get(), v) }
    }
}

pub struct Task(Reg<u32, Wo>);
impl Task {
    pub fn trigger(&self) {
        self.0.write(1)
    }
}

pub struct Event(Reg<u32, Rw>);
impl Event {
    pub fn generated(&self) -> bool {
        self.0.read() & 1 != 0
    }
    pub fn clear(&self) {
        self.0.write(0)
    }
}

#[bitfield(u32)]
pub struct Count16 {
    #[bits(16)]
    count: u16,
    #[bits(16)]
    __: u32,
}

#[bitfield(u32)]
pub struct Count4 {
    #[bits(4)]
    count: u8,
    #[bits(28)]
    __: u32,
}

pub struct Mmio<T>(*const T);

impl<T> Mmio<T> {
    /// # Safety
    /// `addr` must be the address of a valid, live `T` (the peripheral's
    /// register block), and only one owner may exist per peripheral.
    pub const unsafe fn new(addr: usize) -> Self {
        Self(addr as *const T) // int->ptr cast is fine in const; deref is not
    }

    /// Like `new`, from a pointer, which keeps its provenance (host tests
    /// over register blocks in RAM run under Miri's strict provenance).
    ///
    /// # Safety
    /// As for `new`.
    #[cfg(test)]
    pub(crate) const unsafe fn from_ptr(ptr: *const T) -> Self {
        Self(ptr)
    }

    pub const fn as_ptr(&self) -> *const T {
        self.0
    }
}

impl<T> Deref for Mmio<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // Sound by the contract on `new`; evaluated at runtime, not in const.
        unsafe { &*self.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

    fn reg<A>(v: u32) -> Reg<u32, A> {
        Reg(UnsafeCell::new(v), PhantomData)
    }
    fn raw<A>(r: &Reg<u32, A>) -> u32 {
        unsafe { *r.0.get() }
    }

    #[test]
    fn registers_are_one_word() {
        assert_eq!(size_of::<Reg<u32, Rw>>(), 4);
        assert_eq!(size_of::<Task>(), 4);
        assert_eq!(size_of::<Event>(), 4);
    }

    #[test]
    fn reg_reads_and_writes_the_word() {
        let r = reg::<Rw>(0x1234);
        assert_eq!(r.read(), 0x1234);
        r.write(0xDEAD_BEEF);
        assert_eq!(r.read(), 0xDEAD_BEEF);
    }

    #[test]
    fn task_trigger_writes_one() {
        let t = Task(reg(0));
        t.trigger();
        assert_eq!(raw(&t.0), 1);
    }

    #[test]
    fn event_looks_only_at_bit_0_and_clear_writes_zero() {
        let e = Event(reg(0));
        assert!(!e.generated());
        e.0.write(1);
        assert!(e.generated());
        e.0.write(0xFFFF_FFFE);
        assert!(!e.generated(), "only bit 0 is the event");
        e.0.write(1);
        e.clear();
        assert_eq!(raw(&e.0), 0);
    }

    #[test]
    fn count_fields_keep_their_width() {
        assert_eq!(
            Count16Builder::new().with_count(0xFFFF).build().into_bits(),
            0xFFFF
        );
        assert_eq!(Count16::from_bits(0xABCD_1234).count(), 0x1234);
        assert_eq!(
            Count4Builder::new().with_count(0xF).build().into_bits(),
            0xF
        );
        assert_eq!(Count4::from_bits(0xFFFF_FFF5).count(), 5);
    }

    #[test]
    fn mmio_derefs_to_the_block_at_its_address() {
        let block = reg::<Rw>(7);
        let m: Mmio<Reg<u32, Rw>> = unsafe { Mmio::from_ptr(&block) };
        assert_eq!(m.as_ptr(), &block as *const _);
        assert_eq!(m.read(), 7);
        m.write(9);
        assert_eq!(block.read(), 9);
    }
}
