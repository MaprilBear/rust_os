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

    #[allow(unused)]
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
