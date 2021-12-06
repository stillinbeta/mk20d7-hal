#![no_std]

extern crate bit_field;
extern crate bitrate;
extern crate cast;
extern crate cortex_m;
extern crate embedded_hal as hal;
pub extern crate mk20d7;
extern crate nb;
extern crate void;

pub mod delay;
pub mod gpio;
pub mod i2c;
pub mod mcg;
pub mod osc;
pub mod prelude;
pub mod serial;
pub mod sim;
pub mod wdog;

#[derive(Debug)]
pub enum Error {
    /// Delay must be between 1 and 0x00ffffff (1 << 24).
    InvalidDelay,
}
