use crate::{
    gpio::{
        gpiob::{PTB2, PTB3},
        Alternate, ALT2,
    },
    mcg::MultipurposeClockGenerator,
    sim::SystemIntegrationModule,
};
use cmim::{Context, Move};
use core::{
    marker::PhantomData,
    sync::atomic::{AtomicBool, Ordering},
};
use hal::i2c::{
    blocking::{Read, Write},
    SevenBitAddress,
};
use mk20d7::interrupt;

pub trait Sda: private::Sealed {}
pub trait Scl: private::Sealed {}

impl Sda for PTB2<Alternate<ALT2>> {}
impl Scl for PTB3<Alternate<ALT2>> {}

mod private {
    pub trait Sealed {}
    use super::*;

    impl Sealed for PTB2<Alternate<ALT2>> {}
    impl Sealed for PTB3<Alternate<ALT2>> {}
}

pub struct I2C0<SDA, SCL> {
    sda: PhantomData<SDA>,
    scl: PhantomData<SCL>,
    i2c: mk20d7::I2C0,
}

impl<SDA, SCL> I2C0<SDA, SCL> {
    pub fn i2c0(
        i2c: mk20d7::I2C0,
        _pins: (SDA, SCL),
        baud: u32,
        mcg: &mut MultipurposeClockGenerator,
        sim: &mut SystemIntegrationModule,
    ) -> Self
    where
        SDA: Sda,
        SCL: Scl,
    {
        let (_, bus, _) = sim.get_frequencies(mcg.mcgoutclk());
        let (ul, icr) = find_freq(baud, bus);
        // enable i2c0 clock
        sim.enable_i2c0();
        // Set clock frequency
        i2c.f
            .write(|w| unsafe { w.icr().bits(icr).mult().bits(ul) });
        // enable
        i2c.c1.write(|w| w.iicen().set_bit().mst().set_bit());

        todo!()
    }

    fn run_interrupt(&self, mode: I2CMode, address: SevenBitAddress) {
        {
            let mut done = AtomicBool::new(false);
            let state = I2CState::new(mode, address, &self.i2c, &mut done);
            I2C0_STATE.try_move(state).ok();

            while !done.load(Ordering::Relaxed) {
                cortex_m::asm::wfi()
            }
        }
    }
}

impl<SDA, SCL> Write<SevenBitAddress> for I2C0<SDA, SCL>
where
    SDA: Sda,
    SCL: Scl,
{
    type Error = crate::Error;
    fn write(&mut self, address: SevenBitAddress, buffer: &[u8]) -> Result<(), Self::Error> {
        self.i2c.c1.write(|w| w.iicie().set_bit());

        self.run_interrupt(I2CMode::PrimaryTx(buffer), address);

        self.i2c.c1.write(|w| w.iicie().clear_bit());
        Ok(())
    }
}

impl<SDA, SCL> Read<SevenBitAddress> for I2C0<SDA, SCL>
where
    SDA: Sda,
    SCL: Scl,
{
    type Error = crate::Error;
    fn read(&mut self, address: SevenBitAddress, buffer: &mut [u8]) -> Result<(), Self::Error> {
        self.i2c.c1.write(|w| w.iicie().set_bit());

        self.run_interrupt(I2CMode::PrimaryRx(buffer), address);

        self.i2c.c1.write(|w| w.iicie().clear_bit());
        Ok(())
    }
}

const DIVISIONS: &[u32] = &[
    20, 22, 24, 26, 28, 30, 32, 34, 36, 40, 44, 48, 52, 56, 60, 64, 68, 72, 80, 88, 96, 104, 112,
    128, 136, 144, 160, 176, 192, 224, 240, 256, 288, 320, 352, 384, 448, 480, 512, 576, 640, 768,
    896, 960, 1024, 1152, 1280, 1536, 1920, 1792, 2048, 2304, 2560, 3072, 3840,
];
const MULT: &[u32] = &[1, 2, 4];

fn find_freq(target: u32, bus: u32) -> (u8, u8) {
    let mut distance = u32::MAX;

    let mut idx: usize = 0;
    let mut mul: usize = 0;

    for (i, d) in DIVISIONS.into_iter().enumerate() {
        for (j, m) in MULT.into_iter().enumerate() {
            let calc = bus / (d * m);
            // no abs_diff in stable
            let diff = if calc > target {
                calc - target
            } else {
                target - calc
            };
            if distance > diff {
                distance = diff;
                idx = i;
                mul = j;
            }
        }
    }

    (mul as u8, idx as u8)
}

#[allow(dead_code)]
#[derive(Debug, PartialEq, Copy, Clone)]
enum I2CMode {
    PrimaryTx(*const [u8]),
    PrimaryRx(*mut [u8]),
    SecondaryTx,
    SecondaryRx,
}

#[derive(Debug, PartialEq, Copy, Clone)]
enum I2CStatus {
    // AddressSendTen(TenBitAddress), // TODO(ekf): how to handle 10-bit addresses?
    AddressSend(SevenBitAddress),
    AddressSent,
    Run(usize),
}

struct I2CState {
    mode: I2CMode,
    status: I2CStatus,
    i2c: *const mk20d7::I2C0,
    done: *mut AtomicBool,
}

unsafe impl Send for I2CState {}

impl I2CState {
    fn new(
        mode: I2CMode,
        address: SevenBitAddress,
        i2c: &mk20d7::I2C0,
        done: &mut AtomicBool,
    ) -> Self {
        I2CState {
            mode,
            i2c,
            done,
            status: I2CStatus::AddressSend(address),
        }
    }

    fn i2c(&self) -> &mk20d7::I2C0 {
        unsafe { &*self.i2c }
    }

    fn rx_ok(&self) -> bool {
        self.i2c().s.read().rxak().bit_is_clear()
    }

    fn send_byte(&mut self, byte: u8) {
        if self.rx_ok() {
            self.set_byte(byte)
        } else {
            self.i2c().c1.write(|w| w.mst()._0());
            self.mark_done();
        }
    }

    fn get_byte(&self) -> u8 {
        self.i2c().d.read().bits()
    }

    fn set_byte(&mut self, byte: u8) {
        self.i2c().d.write(|w| unsafe { w.bits(byte) });
    }

    fn next_byte(&mut self, buffer: *const [u8], loc: usize) -> Option<u8> {
        if let Some(byte) = unsafe { &*buffer }.get(loc) {
            self.status = I2CStatus::Run(loc + 1);
            Some(*byte)
        } else {
            None
        }
    }

    fn maybe_receive(&mut self, buffer: *mut [u8], loc: &mut usize) {
        let buf: &mut [u8] = unsafe { &mut *buffer };

        if *loc == buf.len() - 1 {
            self.stop_signal()
        } else if *loc == buf.len() - 2 {
            self.i2c().c1.write(|w| w.txak().set_bit());
        }

        buf[*loc] = self.get_byte();
        *loc += 1;
    }

    fn maybe_transmit(&mut self, buffer: *const [u8], loc: &mut usize) {
        match self.next_byte(buffer, *loc) {
            Some(b) => {
                self.send_byte(b);
                *loc += 1;
            }
            None => {
                self.stop_signal();
                self.mark_done();
            }
        }
    }

    fn stop_signal(&self) {
        self.i2c().c1.write(|w| w.iicen().set_bit().mst().set_bit());
    }

    fn mark_done(&self) {
        unsafe { &*self.done }.store(true, Ordering::Relaxed)
    }
}

static I2C0_STATE: Move<I2CState, mk20d7::Interrupt> =
    Move::new_uninitialized(Context::Interrupt(mk20d7::Interrupt::I2C0));
const I2C0_S: *mut u8 = 0x4006_6003 as *mut u8;

fn i2c0() {
    // Clear flag no matter what, or we're deadlocked
    unsafe { *I2C0_S |= 0b0000_0010 }

    I2C0_STATE
        .try_lock(|state| match (state.mode, state.status) {
            (I2CMode::SecondaryRx | I2CMode::SecondaryTx, _) => todo!(),
            (_, I2CStatus::AddressSend(addr)) => {
                state.set_byte(addr);
                state.status = I2CStatus::AddressSent;
            }
            (_, I2CStatus::AddressSent) => {
                if state.i2c().s.read().rxak().bit_is_clear() {
                    let mut loc = 0;
                    match state.mode {
                        I2CMode::PrimaryTx(buf) => state.maybe_transmit(buf, &mut loc),
                        I2CMode::PrimaryRx(_) => {
                            state.i2c().c1.write(|w| w.tx().clear_bit());
                            let _ = state.get_byte();
                        }
                        _ => unreachable!(), // Secondary covered by first branch
                    }
                    state.status = I2CStatus::Run(loc);
                }
            }
            (I2CMode::PrimaryTx(buf), I2CStatus::Run(mut loc)) => {
                state.maybe_transmit(buf, &mut loc)
            }
            (I2CMode::PrimaryRx(buf), I2CStatus::Run(mut loc)) => {
                state.maybe_receive(buf, &mut loc)
            }
        })
        .ok();
}

interrupt!(I2C0, i2c0);
