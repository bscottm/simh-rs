// SPDX-License-Identifier: MIT

use sim_core::cli::{cmd_repl::CmdREPL, InputRadix};
use sim_core::env::{run_simulator, SimEnvironment};

mod cpu;
mod kl8e;
mod pdp8_asm;
mod pdp8_defs;
mod rk05;
mod loaders;

#[cfg(test)]
mod cpu_tests;

use cpu::PDP8Processor;

/// PDP-8 simulator driver.
fn main() {
    // Construct the CPU:
    let pdp8_processor = PDP8Processor::new();

    // Initialize the simulation environment, SimEnvironment<PDP8Processor>. We'll need the CLI channels when
    // the CLI gets constructed.
    let mut pdp8_environment = SimEnvironment::new(pdp8_processor);

    // Add device metadata to the simulation environment. Device metadata is how the CLI and the underlying
    // simulator understand devices and units. Generate the device and unit manifest needed to construct the
    // CLI.
    pdp8_environment
        .add_standalone(Box::new(kl8e::TTI::new()))
        .add_standalone(Box::new(kl8e::TTO::new()))
        .add_controller(Box::new(rk05::RKController::new()));

    // Construct the CLI
    let mut cli = CmdREPL::new("PDP-8", pdp8_environment.resource_manifest());

    // For PDP-8, the default input radix is... octal!
    cli.set_input_radix(InputRadix::Oct)
        .set_address_format(cpu::pdp8_address_format);

    // Connect the PDP-8's environment to the CLI.
    cli.sim_connect(&mut pdp8_environment);

    // Fire up the simulator in its own thread! Note that run_simulator() consumes the simulator environment
    // by virtue of starting a new thread.
    let sim_thread = run_simulator(pdp8_environment);

    // Then start the CLI:
    cli.run().ok();

    // Wait for the simulator thread to exit (not strictly necessary, but hygenic.)
    sim_thread.join().unwrap();
}
