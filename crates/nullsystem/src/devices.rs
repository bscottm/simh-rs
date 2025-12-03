// SPDX-License-Identifier: MIT

use sim_core::{
    cli::formats::hex32_format as default_hex_format,
    env::{DeviceTraits, ResourceMetadata, SimError, SimResourceProvider, SystemBus},
    SimResources,
};

use crate::cpu::NullProcessor;

#[derive(Debug)]
pub struct NullInput {}

impl NullInput {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceTraits<NullProcessor> for NullInput {
    fn device_name(&self) -> &str {
        "NULL_INPUT"
    }

    fn description(&self) -> &str {
        "Null input device (pricipally for testing)"
    }

    fn device_service(&mut self, _cpu: &mut NullProcessor, _bus: &mut SystemBus) -> Result<(), SimError> {
        // No service routine, so just return Ok.
        Ok(())
    }

    fn device_reset(&mut self) -> () {
        // NOP.
    }
}

// Have to implement SimResourceProvider for devices that have no resources, even when there are default
// implementations for the resource methods.
impl SimResourceProvider for NullInput {
    fn get_metadata(&self) -> Vec<ResourceMetadata> {
        // No metadata exported.
        Vec::new()
    }

    fn read_resource(&self, res_id: u32, index: usize) -> Result<u64, SimError> {
        // And no resources to read.
        Err(SimError::NoSuchResource(format!(
            "Resource ID {} index {}",
            res_id, index
        )))
    }

    fn write_resource(&mut self, res_id: u32, index: usize, _val: u64) -> Result<(), SimError> {
        // And can't deposit anything into it, either.
        Err(SimError::NoSuchResource(format!(
            "Resource ID {} index {}",
            res_id, index
        )))
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// DevZero: A device with resources in the NullSystem
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

#[derive(Debug, SimResources)]
pub struct DevZero {
    #[resource(name = "reg1", bits = 32, fmt = default_hex_format)]
    reg1: u32,
    #[resource(name = "dup2", bits = 32, fmt = default_hex_format)]
    dup2: u32,
}

impl DevZero {
    pub fn new() -> Self {
        Self {
            reg1: 0xff00,
            dup2: 0x0000,
        }
    }
}

impl DeviceTraits<NullProcessor> for DevZero {
    fn device_name(&self) -> &str {
        "DevZero"
    }

    fn description(&self) -> &str {
        "Dummy device 0 (pricipally for testing)"
    }

    fn device_service(&mut self, _cpu: &mut NullProcessor, _bus: &mut SystemBus) -> Result<(), SimError> {
        // No service routine, so just return Ok.
        Ok(())
    }

    fn device_reset(&mut self) -> () {
        self.reg1 = 0xff00;
        self.dup2 = 0
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// DevOne: Another device in the NullSystem.
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

#[derive(Debug, SimResources)]
pub struct DevOne {
    internal_reg1: u32,
    #[resource(name = "dup2", bits = 32, fmt = default_hex_format)]
    dup2: u32,
    xyzzy: u32,
}

impl DevOne {
    pub fn new() -> Self {
        Self {
            internal_reg1: 0x00ff,
            dup2: 0,
            xyzzy: 0xea00,
        }
    }
}

impl DeviceTraits<NullProcessor> for DevOne {
    fn device_name(&self) -> &str {
        "DevOne"
    }

    fn description(&self) -> &str {
        "Dummy device 1 (pricipally for testing)"
    }

    fn device_service(&mut self, _cpu: &mut NullProcessor, _bus: &mut SystemBus) -> Result<(), SimError> {
        // No service routine, so just return Ok.
        Ok(())
    }

    fn device_reset(&mut self) -> () {
        self.internal_reg1 = 0x00ff;
        self.dup2 = 0;
        self.xyzzy = 0xefba;
    }
}
