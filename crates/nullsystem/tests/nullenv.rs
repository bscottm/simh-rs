// SPDX-License-Identifier: MIT

use std::cell::RefCell;
use std::io::Cursor;
use std::rc::Rc;

use nullsystem::cpu::NullProcessor;
use nullsystem::devices::{DevOne, DevZero, NullInput};
use sim_core::env::{SimEnvironment, SimError};

pub struct NullTestingEnvironment {
    // If we need to inspect the CPU in tests, add a getter to SimEnvironment: env.get_cpu()
    pub env: SimEnvironment<NullProcessor>,
}

impl NullTestingEnvironment {
    pub fn new() -> Result<Self, SimError> {
        let nullproc = NullProcessor::new();
        let mut nullenv = SimEnvironment::new(nullproc);

        // Assuming add_device now takes Box<dyn DeviceTraits + Send>
        nullenv
            .add_standalone(Box::new(NullInput::new()))
            .add_standalone(Box::new(DevZero::new()))
            .add_standalone(Box::new(DevOne::new()));

        Ok(Self { env: nullenv })
    }
}

#[cfg(test)]
pub fn cmd_output_to_string(cmd_buffer: Rc<RefCell<Cursor<Vec<u8>>>>) -> String {
    String::from_utf8_lossy(cmd_buffer.borrow().get_ref().as_slice()).to_string()
}
