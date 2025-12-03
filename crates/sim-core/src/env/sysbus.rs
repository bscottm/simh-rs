// SPDX-License-Identifier: MIT

use std::sync::Arc;

use crate::{env::console::SimConsole, timers::TimerManager};

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// System Bus - The runtime execution environment
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// System bus - the runtime execution environment where CPU and devices interact
pub struct SystemBus {
    /// Timer manager for scheduling
    pub timer: TimerManager,

    /// Console I/O (if connected)
    pub console: Option<SimConsole>,
    // Possible future additions to a system bus:
    // pub dma: Option<DMAController>,
    // pub io_space: IOSpace,
    // pub interrupt_controller: InterruptController,
}

impl SystemBus {
    /// Create a new system bus
    pub fn new(timer: TimerManager) -> Self {
        Self { timer, console: None }
    }

    /// Attach console to the bus
    pub fn attach_console(&mut self, console: &SimConsole) {
        self.console = Some(Arc::clone(console));
    }

    /// Schedule a device service event
    pub fn schedule_device(&mut self, name: &str, delay: i64) {
        self.timer.schedule_device(name, delay);
    }

    /// Try to read a character from console
    pub fn console_read(&self) -> Option<char> {
        self.console.as_ref()?.pop_input()
    }

    /// Write a character to console
    pub fn console_write(&self, ch: char) {
        if let Some(console) = &self.console {
            console.push_output(ch);
        }
    }

    /// Check if console has pending input
    pub fn console_has_input(&self) -> bool {
        self.console
            .as_ref()
            .map(|c| !c.input.lock().unwrap().is_empty())
            .unwrap_or(false)
    }
}
