// SPDX-License-Identifier: MIT

use std::thread::JoinHandle;

use crate::{
    env::{
        machine::{CPUTraits, DeviceTraits},
        simenv::{ActiveDevices, SimEnvironment},
        simerror::SimError,
    },
    logging::core,
    sim_debug,
};

/// The number of instructions to execute in a single burst before
/// checking for new incoming messages.
///
/// 173 is a prime number for no other reason than that it's prime.
const INSTRUCTION_BATCH: i32 = 173;

/// Spawn the simulator instruction loop thread.
///
/// # Returns
/// Returns the `JoinHandle` of the simulator instruction loop thread.
pub fn run_simulator<CPU>(env: SimEnvironment<CPU>) -> JoinHandle<()>
where
    CPU: CPUTraits + DeviceTraits<CPU> + Send + 'static,
{
    std::thread::spawn(move || actual_sim_loop(env))
}

/// Starts the main execution loop for the simulator.
///
/// This function manages the lifecycle of the simulation, switching between a "Running" state (executing CPU
/// cycles) and a "Paused" state (waiting for commands).
///
/// # Arguments
/// * `env` - The simulation environment containing the CPU and peripherals.
///
/// # State Behavior
/// * **Running**: Uses non-blocking checks for messages to prioritize instruction throughput.
/// * **Paused**: Uses blocking checks for messages to minimize CPU usage while idle.
fn actual_sim_loop<CPU>(mut env: SimEnvironment<CPU>)
where
    CPU: CPUTraits + DeviceTraits<CPU> + Send + 'static,
{
    let mut instruction_count: u64 = 0;
    let mut instructions_since_tick: i32 = 0;

    // Make sure we're connected to the CLI's message queues:
    if env.cli_connection.is_none() {
        panic!("actual_sim_loop: No CLI message queues?")
    }

    // Do the initial instruction-per-second benchmarking.
    if let Err(ips_err) = env.measure_initial_ips() {
        println!("Error benchmarking IPS: {}", ips_err);
        return;
    }

    // Wire up the devices, if the processor needs to fix devices in place.
    env.finalize_hardware();

    // Ensure that we're not running until the CLI sends us a STEP command.
    env.set_running(false);

    loop {
        let sim_rx = &env.cli_connection.as_ref().unwrap().cli_rx;

        let msg = if env.running() {
            sim_rx.try_recv().ok()
        } else {
            sim_rx.recv().ok()
        };

        if let Some(request) = msg {
            if env.handle_request(request) {
                println!("Simulator exits. Bye!");
                break;
            }
        }

        if env.running() {
            'batch: {
                // Destructure env to get mutable references to the parts we need for instruction execution
                // and avoid borrowing the whole env for the entire batch.
                let SimEnvironment {
                    ref mut cpu,
                    ref mut bus,
                    ref mut devices,
                    ref debug_snapshot,
                    ..
                } = env;

                let mut accessor = ActiveDevices(devices);

                for _ in 0..INSTRUCTION_BATCH {
                    // Physically separate the borrows so Rust knows they don't overlap!

                    // Update snapshot before each instruction — only when debug is active
                    // and snapshot was requested (-P or -I flags).
                    if let Some(snap) = &debug_snapshot {
                        if let Ok(mut s) = snap.write() {
                            s.pc = cpu.current_pc();
                            s.instruction_count = instruction_count;
                        }
                    }

                    if let Err(sim_err) = cpu.simulate_instruction(&mut *bus, &mut accessor) {
                        if sim_err == SimError::SimulatorHalt {
                            println!("Simulator halted.");
                            // TODO: Tell the CLI that the simulator halted.
                        } else {
                            println!("%SIM-ERROR: {}", sim_err);
                        }

                        env.set_running(false);
                        break 'batch;
                    }

                    instruction_count += 1;
                    instructions_since_tick += 1;
                }

                env.bus.timer.add_instructions(INSTRUCTION_BATCH as i64);

                // Service any devices whose timers have expired
                for (device_name, _data) in env.bus.timer.get_ready_devices() {
                    let device_result = env.service_device(&device_name);
                    if let Err(device_err) = device_result {
                        println!("Device servicing error: {}", device_err.to_string());
                        env.set_running(false);
                        // TODO: Tell the CLI that the simulator halted.
                        break 'batch;
                    }
                }

                // Check if a simulated clock tick has elapsed on timer 0.
                // current_delay is instructions per tick, set by calibration.
                let current_delay = env.bus.timer.get_rtc(0).map(|rtc| rtc.current_delay).unwrap_or(1);

                if instructions_since_tick >= current_delay {
                    // Saturating subtraction will not allow instructions_since_tick to go negative.
                    instructions_since_tick = instructions_since_tick.saturating_sub(current_delay);

                    // calibrate_clock_ewma counts ticks internally and only does
                    // real work once per second (when ticks >= ticks_per_second).
                    // This call is cheap on every tick.
                    if let Ok(new_delay) = env.bus.timer.calibrate_timer(0, 60) {
                        sim_debug!(
                            core::CALIBRATION,
                            &env.debug_state,
                            "TIMER",
                            "calibrated timer 0: delay={} ips={:.0}",
                            new_delay,
                            env.bus.timer.instructions_per_sec()
                        );
                    }
                }
            }
        }
    }
}
