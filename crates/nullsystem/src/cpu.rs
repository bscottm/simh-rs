// SPDX-License-Identifier: MIT

use sim_core::{
    cli::formats::hex32_format as default_hex_format,
    env::{CPUTraits, DeviceAccessor, DeviceTraits, SimError, SystemBus, CPU_DEVICE_NAME, MEM_RESOURCE_NAME},
    SimResources,
};

const MAX_NULLCPUMEM: usize = 8 * 1024 * 1024;

#[derive(Debug, SimResources)]
pub struct NullProcessor {
    /// Program counter
    #[resource(name = "PC", bits = 32, fmt = default_hex_format)]
    pc: u32,
    /// Hypothetical vector register
    #[resource(name = "VEC00", bits = 32, len = 8, fmt = default_hex_format)]
    vec00: [u32; 8],
    /// Memory
    #[resource(name = MEM_RESOURCE_NAME, bits = 32, len = MAX_NULLCPUMEM, fmt = default_hex_format)]
    mem: Box<[u32]>,
}

impl NullProcessor {
    pub fn new() -> Self {
        // This is how to allocate a large chunk of heap memory that's been zeroed out
        // without incurring a large copy from a stack temporary. I don't like the
        // "unsafe" block around it, but it's required because new_zeroed() is technically
        // unsafe.
        let mem: Box<[u32; MAX_NULLCPUMEM]> = unsafe { Box::new_zeroed().assume_init() };

        Self {
            pc: 0,
            vec00: [0; 8],
            mem: mem,
        }
    }

    fn reset(&mut self) -> () {
        self.pc = 0;
    }
}

impl CPUTraits for NullProcessor {
    // No, we don't use execute_io(). Use the unit type as the IoPayload type.
    type IoPayload = ();

    fn simulate_instruction(
        &mut self,
        _bus: &mut SystemBus,
        _devices: &mut dyn DeviceAccessor<Self>,
    ) -> Result<(), SimError> {
        // Nothing for now...
        Ok(())
    }

    fn initial_ips_code(&mut self) {
        // No actual instructions, but we'll set the initial PC to something non-zero for testing.
        self.pc = 0x1000;
    }

    fn current_pc(&self) -> Option<String> {
        Some(format!("{:#08x}", self.pc))
    }

    fn cpu_reset(&mut self) -> () {
        self.reset();
    }

    fn load_file(&mut self, flags: Vec<char>, filename: String) -> Result<(), SimError> {
        // For testing, we'll just print out the filename and flags, and not actually load anything.
        println!("Requested to load file: {}, with flags: {:?}", filename, flags);
        Ok(())
    }
}

impl DeviceTraits<NullProcessor> for NullProcessor {
    fn device_name(&self) -> &str {
        CPU_DEVICE_NAME
    }

    fn description(&self) -> &str {
        "Null processor system (pricipally for testing)"
    }

    fn device_service(&mut self, _cpu: &mut NullProcessor, _bus: &mut SystemBus) -> Result<(), SimError> {
        // No service routine, so just return Ok.
        Ok(())
    }

    fn device_reset(&mut self) -> () {
        self.cpu_reset()
    }
}
