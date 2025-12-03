// SPDX-License-Identifier: MIT

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Shared console I/O queues between the CLI thread and the simulator thread.
///
/// The CLI thread owns the terminal (via crossterm in console mode) and is the
/// sole producer of input and sole consumer of output. The simulator's console
/// device is the sole consumer of input and sole producer of output.
///
/// No condvar is needed — the simulator polls at batch boundaries and the CLI
/// spins on crossterm events.
pub struct ConsoleQueues {
    /// CLI → simulator: raw keystrokes queued by the CLI console mode loop.
    /// Type-ahead, SEND command pre-load, and interactive input all feed here.
    pub input: Mutex<VecDeque<char>>,
    /// Simulator → CLI: character output from the console device.
    pub output: Mutex<VecDeque<char>>,
}

impl ConsoleQueues {
    pub fn new() -> Self {
        Self {
            input: Mutex::new(VecDeque::new()),
            output: Mutex::new(VecDeque::new()),
        }
    }

    /// Push a character into the input queue.
    /// Returns `true` if the queue was previously empty — the caller
    /// should schedule the console device if so.
    pub fn push_input(&self, ch: char) -> bool {
        let mut q = self.input.lock().unwrap();
        let was_empty = q.is_empty();
        q.push_back(ch);
        was_empty
    }

    /// Pop a character from the input queue. Returns `None` if empty.
    pub fn pop_input(&self) -> Option<char> {
        self.input.lock().unwrap().pop_front()
    }

    /// Push a character into the output queue.
    pub fn push_output(&self, ch: char) {
        self.output.lock().unwrap().push_back(ch);
    }

    /// Drain all pending output characters.
    pub fn drain_output(&self) -> Vec<char> {
        self.output.lock().unwrap().drain(..).collect()
    }
}

/// Simulator Console I/O
///
/// Shared simulator console I/O container
pub type SimConsole = Arc<ConsoleQueues>;

/// Construct a new simulator console I/O container.
pub fn new_console() -> SimConsole {
    Arc::new(ConsoleQueues::new())
}
