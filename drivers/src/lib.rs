//! Peripheral drivers for the nRF52840. See Cargo.toml for where this code
//! runs.
#![allow(future_incompatible)] // saturating_cast vs. a possible future std method
#![no_std]

pub mod fifo;
pub mod mmio;
pub mod ppi;
pub mod timer;
pub mod uarte;
