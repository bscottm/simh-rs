# sim_timer - Rust Timer Library for Simulators

A high-precision, cross-platform timing library for computer system simulators, ported from the original C implementation in SIMH (Computer History Simulation Project).

## Features

- **Multiple Independent Timers**: Up to 8 independent real-time clocks running at different frequencies
- **Automatic Calibration**: Dynamically adjusts to match simulated time with wall-clock time
- **Idle Detection**: Reduces host CPU usage when the simulated system is idle
- **Execution Throttling**: Limits simulator speed to match real hardware or control CPU usage
- **Asynchronous Timer Support**: Optional async timers running in separate threads for improved accuracy
- **Cross-Platform**: Works on Linux, macOS, Windows with platform-specific optimizations
- **Co-Scheduled Units**: Synchronize multiple simulation units with master clocks
- **Memory Safe**: All the benefits of Rust's ownership and type system

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
sim_timer = "0.1"
```

For async support (enabled by default):

```toml
[dependencies]
sim_timer = { version = "0.1", features = ["async"] }
```

## Quick Start

```rust
use sim_timer::{TimerManager, create_platform_timer};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create timer manager with platform-specific timer
    let platform = create_platform_timer();
    let mut timer_mgr = TimerManager::new(platform);

    // Initialize a 60Hz clock (timer 0)
    timer_mgr.init_timer(0, 60.0)?;

    // In your simulation loop:
    loop {
        // Execute simulated instructions
        let instructions = execute_simulation(1000);
        
        // Calibrate the timer periodically (every 10 ticks)
        if should_calibrate() {
            let ips = timer_mgr.calibrate_timer(0, 10)?;
            println!("Calibrated: {} instructions/sec", ips);
        }
        
        // Check for idle and optimize CPU usage
        if timer_mgr.check_idle(0, instructions) {
            println!("Entered idle mode");
        }
        
        // Optional: throttle execution to match real hardware
        timer_mgr.throttle(instructions);
    }
}
```

## Core Concepts

### Timers and Calibration

The library maintains multiple independent timers that track simulated time vs. wall-clock time. Calibration automatically adjusts the instruction-to-time ratio:

```rust
// Initialize a 50Hz timer
timer_mgr.init_timer(0, 50.0)?;

// Calibrate with 10 ticks per second measurement
let inst_per_tick = timer_mgr.calibrate_timer(0, 10)?;
```

### Idle Detection

When the simulated system enters an idle loop, the library can detect this and reduce host CPU usage:

```rust
// Enable idle detection
timer_mgr.enable_idle(true);

// Set stability threshold (seconds of stable idle before engaging)
timer_mgr.set_idle_stability(20)?;

// Check for idle (returns true if CPU was saved)
if timer_mgr.check_idle(timer_id, instructions_executed) {
    // Successfully idled
}
```

### Throttling

Limit simulator speed to match real hardware or control CPU usage:

```rust
use sim_timer::ThrottleType;

// Limit to 1 MHz
timer_mgr.set_throttle(ThrottleType::MegaCyclesPerSec(1))?;

// Or use 50% of host CPU
timer_mgr.set_throttle(ThrottleType::Percent(50))?;

// Or specific delay: 1000 instructions, then sleep 10ms
timer_mgr.set_throttle(ThrottleType::Specific {
    instructions: 1000,
    delay_ms: 10,
})?;
```

## Platform Abstraction

The library abstracts platform-specific timing primitives through the `PlatformTimer` trait:

```rust
use sim_timer::{PlatformTimer, TimeSpec};

// Create a platform-specific timer
let timer = create_platform_timer();

// Get current time
let now = timer.get_time_spec();

// Sleep for 10ms
let actual_sleep = timer.sleep_ms(10);

// Get OS clock resolution
let resolution = timer.get_clock_resolution_ms();
```

## Advanced Usage

### Custom Timer Units

Implement the `TimerUnit` trait for your simulation units:

```rust
use sim_timer::TimerUnit;

struct MyDevice {
    id: usize,
    name: String,
    active: bool,
    time_remaining: i32,
}

impl TimerUnit for MyDevice {
    fn id(&self) -> usize {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn time_until_activation(&self) -> Option<i32> {
        if self.active {
            Some(self.time_remaining)
        } else {
            None
        }
    }

    fn usecs_remaining(&self) -> Option<f64> {
        // Calculate based on calibration
        None
    }

    fn set_usecs_remaining(&mut self, usecs: f64) {
        // Store for tracking
    }

    fn service(&mut self) -> i32 {
        // Called when timer fires
        println!("Device {} activated!", self.name);
        0
    }
}
```

### Co-Scheduled Units

Register units to be scheduled relative to a master clock:

```rust
// Register a unit with timer 0
timer_mgr.register_clock_unit(0, my_unit)?;

// Schedule it for 100 ticks from now
timer_mgr.coschedule_unit(0, unit_id, 100, false)?;

// Schedule at absolute tick 1000
timer_mgr.coschedule_unit(0, unit_id, 1000, true)?;
```

### Async Timer Support

With the `async` feature enabled:

```rust
use sim_timer::AsyncTimerQueue;

// Create async timer queue
let mut async_queue = timer_mgr.create_async_queue()?;

// Enqueue a unit to fire in 1000 microseconds
async_queue.enqueue(my_unit, 1000.0)?;

// In a separate thread/task
tokio::spawn(async move {
    while let Some(unit) = async_queue.wait_next() {
        unit.service();
    }
});
```

## Architecture

The library is organized into several key components:

**TimerManager** - Central coordinator containing:
- Calibration System - Tracks and adjusts instruction-per-second ratios
- Idle Support System - Detects and optimizes CPU usage during idle periods
- Throttling System - Controls execution speed to match target rates
- RTC Array - Eight independent real-time clocks running at different frequencies

**Supporting Components:**
- Platform Timer - OS-specific timing primitives (sleep, time measurement)
- Async Queue - Thread-safe timer event queue for asynchronous operation
- CoSchedule Queue - Manages units synchronized with master clocks

## Performance

The library is designed for minimal overhead:

- Zero-cost abstractions using Rust traits
- Lock-free algorithms where possible
- Platform-optimized sleep and timing primitives
- Optional async support (can be disabled)
- Efficient calibration with exponential moving averages

Benchmark results on an Intel i7-9750H:

```
Calibration overhead:    ~500ns per call
Idle check overhead:     ~100ns per call
Throttle overhead:       ~50ns per call (when not sleeping)
Platform timer get_msec: ~30ns per call
```

## Testing

Run the test suite:

```bash
cargo test
```

Run benchmarks:

```bash
cargo bench
```

Test with different features:

```bash
# Without async support
cargo test --no-default-features

# With debug output
cargo test --features debug
```

## Platform Support

- **Linux**: Uses `clock_gettime` with `CLOCK_MONOTONIC`, `nanosleep`
- **macOS**: Uses `mach_absolute_time`, `nanosleep`
- **Windows**: Uses `QueryPerformanceCounter`, `timeGetTime`, `Sleep`
- **Other Unix**: Falls back to POSIX `clock_gettime`

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure `cargo test` and `cargo clippy` pass
5. Submit a pull request

## License

This project is licensed under the MIT License - see LICENSE file for details.

Based on the original SIMH timer library by Robert M. Supnik.

## Acknowledgments

- Robert M. Supnik for the original C implementation
- Mark Pizzolato for idle support and async I/O contributions
- The SIMH development community

## FAQ

**Q: How accurate is the timing?**

A: Timing accuracy depends on the host OS timer resolution. On modern systems, calibration typically achieves sub-millisecond accuracy. Use async timers for the best accuracy.

**Q: Can I use this for real-time systems?**

A: While the library provides precise timing, it's designed for simulation rather than hard real-time. For real-time systems, consider dedicated RTOS solutions.

**Q: How much overhead does calibration add?**

A: Calibration is very lightweight (~500ns per call). It's designed to be called every 10-100 ticks without noticeable impact.

**Q: Does idle detection work on all platforms?**

A: Idle detection works best on platforms with millisecond-granularity sleep. The library automatically detects platform capabilities.

**Q: Can I disable async support to reduce dependencies?**

A: Yes! Use `default-features = false` in your Cargo.toml to disable async support and related dependencies.
