// pdp-8/src/kl8e.rs
// SPDX-License-Identifier: MIT

//! KL8E Terminal I/O Devices

use sim_core::{
    env::{DeviceTraits, SimError, SystemBus},
    SimResources,
};

use crate::cpu::{default_format, format_bit, PDP8IoPayload, PDP8Processor};
use crate::pdp8_defs::InterruptFlags;

/// TTI - Teletype input device
#[derive(Debug, Clone, SimResources)]
pub struct TTI {
    #[resource(name = "FLAG", bits = 1, fmt = format_bit)]
    pub flag: bool,
    #[resource(name = "BUF", bits = 8, fmt = default_format)]
    pub buffer: u8,
}

impl TTI {
    pub fn new() -> Self {
        Self {
            flag: false,
            buffer: 0,
        }
    }
}

impl DeviceTraits<PDP8Processor> for TTI {
    fn device_name(&self) -> &'static str {
        "TTI"
    }
    fn description(&self) -> &'static str {
        "Teletype Input Device"
    }

    fn device_service(&mut self, cpu: &mut PDP8Processor, bus: &mut SystemBus) -> Result<(), SimError> {
        if !cpu.int_req.contains(InterruptFlags::ION) {
            return Ok(());
        }
        if self.flag {
            return Ok(());
        }
        if let Some(ch) = bus.console_read() {
            self.buffer = (ch as u8) & 0x7F;
            self.flag = true;
            cpu.int_req.insert(InterruptFlags::TTI);
        }
        Ok(())
    }

    fn device_reset(&mut self) {
        self.flag = false;
        self.buffer = 0;
    }
}

/// TTO device state
#[derive(Debug, Clone, SimResources)]
pub struct TTO {
    #[resource(name = "FLAG", bits = 1, fmt = format_bit)]
    pub flag: bool,
    #[resource(name = "BUF", bits = 8, fmt = default_format)]
    pub buffer: u8,
}

impl TTO {
    pub fn new() -> Self {
        Self {
            flag: true,
            buffer: 0,
        }
    }
}

impl DeviceTraits<PDP8Processor> for TTO {
    fn device_name(&self) -> &'static str {
        "TTO"
    }
    fn description(&self) -> &'static str {
        "Teletype Output Device"
    }

    fn device_service(&mut self, cpu: &mut PDP8Processor, bus: &mut SystemBus) -> Result<(), SimError> {
        if !self.flag && self.buffer != 0 {
            let ch = (self.buffer & 0x7F) as char;
            bus.console_write(ch);
            self.flag = true;
            self.buffer = 0;
            cpu.int_req.insert(InterruptFlags::TTO);
        }
        Ok(())
    }

    fn device_reset(&mut self) {
        self.flag = true;
        self.buffer = 0;
    }

    fn execute_io(
        &mut self,
        _payload: &PDP8IoPayload,
        _cpu: &mut PDP8Processor,
        _bus: &mut SystemBus,
    ) -> Result<(), SimError> {
        Ok(())
    }
}
