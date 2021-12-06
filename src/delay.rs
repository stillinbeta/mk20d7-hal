use cast::u32;
use cortex_m::peripheral::syst::SystClkSource;
use cortex_m::peripheral::SYST;

use crate::mcg::MultipurposeClockGenerator;
use crate::sim::SystemIntegrationModule;
use hal::delay::blocking::{DelayMs, DelayUs};

pub struct Delay<'a> {
    sim: &'a SystemIntegrationModule<'a>,
    mcg: &'a MultipurposeClockGenerator<'a>,
    syst: SYST,
}

impl<'a> Delay<'a> {
    pub fn new(
        mut syst: SYST,
        sim: &'a SystemIntegrationModule<'a>,
        mcg: &'a MultipurposeClockGenerator,
    ) -> Self {
        syst.set_clock_source(SystClkSource::Core);

        Delay { syst, mcg, sim }
    }

    pub fn free(self) -> SYST {
        self.syst
    }
}

impl<'a> DelayMs<u32> for Delay<'a> {
    type Error = crate::Error;
    fn delay_ms(&mut self, ms: u32) -> Result<(), Self::Error> {
        self.delay_us(ms * 1_000)
    }
}

impl<'a> DelayMs<u16> for Delay<'a> {
    type Error = crate::Error;
    fn delay_ms(&mut self, ms: u16) -> Result<(), Self::Error> {
        self.delay_ms(u32(ms))
    }
}

impl<'a> DelayMs<u8> for Delay<'a> {
    type Error = crate::Error;
    fn delay_ms(&mut self, ms: u8) -> Result<(), Self::Error> {
        self.delay_ms(u32(ms))
    }
}

impl<'a> DelayUs<u32> for Delay<'a> {
    type Error = crate::Error;
    fn delay_us(&mut self, us: u32) -> Result<(), Self::Error> {
        let mcgoutclk = self.mcg.mcgoutclk();
        let (core, _, _) = self.sim.get_frequencies(mcgoutclk);
        let rvr = us * u32::from(core);

        if rvr > (1 << 24) {
            return Err(crate::Error::InvalidDelay);
        }

        self.syst.set_reload(rvr);
        self.syst.clear_current();
        self.syst.enable_counter();

        while !self.syst.has_wrapped() {}

        self.syst.disable_counter();
        Ok(())
    }
}

impl<'a> DelayUs<u16> for Delay<'a> {
    type Error = crate::Error;
    fn delay_us(&mut self, us: u16) -> Result<(), Self::Error> {
        self.delay_us(u32(us))
    }
}

impl<'a> DelayUs<u8> for Delay<'a> {
    type Error = crate::Error;
    fn delay_us(&mut self, us: u8) -> Result<(), Self::Error> {
        self.delay_us(u32(us))
    }
}
