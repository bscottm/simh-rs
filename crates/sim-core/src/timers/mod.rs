//! Timer subsystem for SIMH simulators
//!
//! A high-precision, cross-platform timing library for computer system simulators.
//! Provides calibrated real-time clocks, idle detection, execution throttling,
//! and asynchronous timer support.
//!
//! # Features
//!
//! - Multiple independent real-time clocks (up to 8)
//! - Automatic calibration to match simulated vs real time
//! - CPU idle detection and optimization
//! - Execution throttling to match real hardware speeds
//! - Asynchronous timer support for improved accuracy
//! - Cross-platform support (Unix, Windows, macOS)
//! - Co-scheduled unit management
//!
//! # Example
//!
//! ```rust,no_run
//! use sim_core::timers::{TimerManager, create_platform_timer};
//!
//! // Create a timer manager with platform-specific timer
//! let platform = create_platform_timer();
//! let mut timer_mgr = TimerManager::new(platform);
//!
//! // Initialize a 60Hz clock
//! timer_mgr.init_timer(0, 60.0).unwrap();
//! ```

mod calibration;
mod idle;
mod manager;
mod platform;
mod throttle;
mod traits;
mod types;

// Re-export main types
pub use traits::{
    ClockCalibration, IdleManager, PlatformTimer, ThreadPriority, ThrottleManager, ThrottleType, TimeSpec,
    TimerError, TimerResult,
};

pub use types::{CalibrationState, IdleState, RealTimeClock, ThrottleState, TimerConfig};

pub use manager::TimerManager;

// Re-export platform timer creation
pub use platform::create_platform_timer;

/// Number of independent timers supported
pub const SIM_NTIMERS: usize = 8;

/// Maximum timer makeup (accumulation limit in milliseconds)
pub const SIM_TMAX: i32 = 500;

/// Initial uncalibrated assumption about instructions per second
pub const SIM_INITIAL_IPS: i32 = 5_000_000;

/// Minimum time to run precalibration activities (milliseconds)
pub const SIM_PRE_CALIBRATE_MIN_MS: u32 = 100;

/// Idle calibration time (milliseconds)
pub const SIM_IDLE_CAL: u32 = 10;

/// Minimum seconds for idle stability
pub const SIM_IDLE_STMIN: u32 = 2;

/// Default seconds for idle stability
pub const SIM_IDLE_STDFLT: u32 = 20;

/// Maximum seconds for idle stability
pub const SIM_IDLE_STMAX: u32 = 600;

/// Throttle initial wait cycles to skip
pub const SIM_THROT_WINIT: u32 = 1000;

/// Throttle initial wait time
pub const SIM_THROT_WST: u32 = 10000;

/// Throttle wait multiplier
pub const SIM_THROT_WMUL: u32 = 4;

/// Throttle minimum wait
pub const SIM_THROT_WMIN: u32 = 50;

/// Default drift percentage for recalibration
pub const SIM_THROT_DRIFT_PCT_DFLT: u32 = 5;

/// Throttle minimum measurement time (milliseconds)
pub const SIM_THROT_MSMIN: u32 = 10;

// Debug flags
/// Debug flag for idle debugging
pub const TIMER_DBG_IDLE: u32 = 0x001;

/// Debug flag for async queue debugging
pub const TIMER_DBG_QUEUE: u32 = 0x002;

/// Debug flag for mux debugging
pub const TIMER_DBG_MUX: u32 = 0x004;
