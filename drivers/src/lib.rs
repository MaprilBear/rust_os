//! Peripheral drivers for the nRF52840. See Cargo.toml for where this code
//! runs and how it is tested.
#![no_std]

pub mod fifo;
pub mod mmio;
pub mod ppi;
pub mod timer;
// Its statics need `const` constructors, which loom's types lack.
#[cfg(not(loom))]
pub mod uarte;
