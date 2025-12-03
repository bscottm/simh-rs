// SPDX-License-Identifier: MIT

/*!
   # Simulator Template
*/

use sim_core::cli::CmdREPL;
use sim_core::env::{run_simulator, SimEnvironment};

use nullsystem::cpu::NullProcessor;
use nullsystem::devices::{DevOne, DevZero, NullInput};

/// Example simulator [`main()`]
///
/// This is an example SIMH-RS simulator `main` function that can be adapted as a template.
fn main() {
    let nullproc = NullProcessor::new();
    let mut nullenv = SimEnvironment::new(nullproc);

    nullenv
        .add_standalone(Box::new(NullInput::new()))
        .add_standalone(Box::new(DevZero::new()))
        .add_standalone(Box::new(DevOne::new()));

    let mut cli = CmdREPL::new("NullSystem", nullenv.resource_manifest());

    cli.sim_connect(&mut nullenv);

    let sim_thread = run_simulator(nullenv);
    cli.run().ok();
    sim_thread.join().unwrap();
}
