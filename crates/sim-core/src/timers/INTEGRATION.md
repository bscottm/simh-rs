# Timer Module Integration Guide

The `timers` module integrates with SIMH-RS's device architecture using device names and MPSC channels.

## Architecture Overview

The timer system works with the existing simulator loop:

```rust
// In run_simulator() loop
loop {
    // Handle messages...
    
    if running {
        for _ in 0..INSTRUCTION_BATCH {
            // Execute instruction
            env.cpu().simulate_instruction()?;
            timer_mgr.add_instructions(1);
            
            // Service any devices whose timers have expired
            for (device_name, _data) in timer_mgr.get_ready_devices() {
                env.service_device(&device_name)?;
            }
        }
    }
}
```

## Key Features

1. **No tokio required** - Uses synchronous `std::sync::mpsc` channels
2. **Device name-based** - Integrates with `SimEnvironment`'s device lookup
3. **Priority queue** - Efficient min-heap for event scheduling
4. **Calibration** - Automatic adjustment to match wall-clock time

## Basic Usage

### 1. Create Timer Manager

```rust
use sim_core::timers::{TimerManager, create_platform_timer};

let platform = create_platform_timer();
let mut timer_mgr = TimerManager::new(platform);
```

### 2. Initialize a Timer

```rust
// Initialize a 60Hz system clock
timer_mgr.init_timer(0, 60.0)?;
```

### 3. Schedule Device Events

```rust
// Schedule a device to be serviced in 50,000 instructions
timer_mgr.schedule_device("RKA0", 50_000);
```

### 4. Service Ready Devices

In the simulator loop:

```rust
// Track instructions
timer_mgr.add_instructions(1);

// Check for ready devices
for (device_name, event_data) in timer_mgr.get_ready_devices() {
    env.service_device(&device_name)?;
}
```

### 5. Calibrate Periodically

```rust
// Call once per simulated second
timer_mgr.calibrate_timer(0, 60)?;
```

## Complete Integration Example

```rust
use std::sync::mpsc;
use sim_core::env::{SimEnvironment, messages::*};
use sim_core::timers::{TimerManager, create_platform_timer};

pub fn run_simulator<P>(mut env: SimEnvironment<P>, rx: Receiver<SimRequest>, tx: Sender<SimResponse>)
where P: DeviceTraits + CPUTraits + Send + 'static
{
    // Create timer manager
    let platform = create_platform_timer();
    let mut timer_mgr = TimerManager::new(platform);
    
    // Initialize system clock (60Hz)
    timer_mgr.init_timer(0, 60.0).unwrap();
    
    let mut running = false;

    loop {
        let msg = if running {
            rx.try_recv().ok()
        } else {
            rx.recv().ok()
        };

        if let Some(request) = msg {
            if handle_request(&mut env, request, &tx, &mut running).is_some() {
                break;
            }
        }

        if running {
            for _ in 0..173 {
                // Execute instruction
                if let Err(e) = env.cpu().simulate_instruction() {
                    running = false;
                    break;
                }
                
                // Track execution
                timer_mgr.add_instructions(1);
                
                // Service devices whose timers expired
                for (device_name, _data) in timer_mgr.get_ready_devices() {
                    let _ = env.service_device(&device_name);
                }
            }
        }
    }
}
```

## No Async Required

The timer system is entirely synchronous:
- No `tokio` dependency
- No `async`/`await`
- Uses standard `std::sync::mpsc` channels
- Compatible with regular `main()` functions

## License

MIT License - Same as SIMH
