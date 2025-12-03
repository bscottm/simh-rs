//! Core traits for the timer library
//!
//! This module defines the fundamental traits that provide platform abstraction
//! for the timer system. The timer system integrates with SIMH-RS's device
//! architecture using device names rather than direct trait object references.

use std::fmt;
use std::time::Duration;

/// Result type for timer operations
pub type TimerResult<T> = Result<T, TimerError>;

/// High-resolution timestamp structure
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimeSpec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

impl TimeSpec {
    /// Create a new TimeSpec
    pub fn new(sec: i64, nsec: i64) -> Self {
        Self {
            tv_sec: sec,
            tv_nsec: nsec,
        }
    }

    /// Convert to Duration (for positive values)
    pub fn as_duration(&self) -> Option<Duration> {
        if self.tv_sec >= 0 && self.tv_nsec >= 0 {
            Some(Duration::new(self.tv_sec as u64, self.tv_nsec as u32))
        } else {
            None
        }
    }

    /// Convert to total milliseconds
    pub fn as_millis(&self) -> i64 {
        self.tv_sec * 1000 + self.tv_nsec / 1_000_000
    }

    /// Convert to total microseconds
    pub fn as_micros(&self) -> i64 {
        self.tv_sec * 1_000_000 + self.tv_nsec / 1_000
    }

    /// Convert to total nanoseconds (may overflow for large values)
    pub fn as_nanos(&self) -> i64 {
        self.tv_sec
            .saturating_mul(1_000_000_000)
            .saturating_add(self.tv_nsec)
    }

    /// Subtract two TimeSpecs
    pub fn diff(&self, other: &TimeSpec) -> TimeSpec {
        let mut sec = self.tv_sec - other.tv_sec;
        let mut nsec = self.tv_nsec - other.tv_nsec;

        if nsec < 0 {
            sec -= 1;
            nsec += 1_000_000_000;
        }

        TimeSpec {
            tv_sec: sec,
            tv_nsec: nsec,
        }
    }

    /// Add a duration to this TimeSpec
    pub fn add_duration(&self, duration: Duration) -> TimeSpec {
        let mut sec = self.tv_sec + duration.as_secs() as i64;
        let mut nsec = self.tv_nsec + duration.subsec_nanos() as i64;

        if nsec >= 1_000_000_000 {
            sec += nsec / 1_000_000_000;
            nsec %= 1_000_000_000;
        }

        TimeSpec {
            tv_sec: sec,
            tv_nsec: nsec,
        }
    }
}

impl std::ops::Sub for TimeSpec {
    type Output = TimeSpec;

    fn sub(self, rhs: TimeSpec) -> TimeSpec {
        self.diff(&rhs)
    }
}

/// Thread priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadPriority {
    BelowNormal,
    Normal,
    AboveNormal,
}

/// Errors that can occur in timer operations
#[derive(Debug, Clone)]
pub enum TimerError {
    /// Invalid timer ID (out of range)
    InvalidTimerId(usize),

    /// Platform-specific error
    PlatformError(String),

    /// Calibration failed
    CalibrationFailed(String),

    /// Throttle configuration error
    ThrottleConfigError(String),

    /// Device not found
    DeviceNotFound(String),

    /// Invalid parameter
    InvalidParameter(String),

    /// Operation not supported on this platform
    NotSupported(String),
}

impl fmt::Display for TimerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimerError::InvalidTimerId(id) => write!(f, "Invalid timer ID: {}", id),
            TimerError::PlatformError(msg) => write!(f, "Platform error: {}", msg),
            TimerError::CalibrationFailed(msg) => write!(f, "Calibration failed: {}", msg),
            TimerError::ThrottleConfigError(msg) => write!(f, "Throttle config error: {}", msg),
            TimerError::DeviceNotFound(msg) => write!(f, "Device not found: {}", msg),
            TimerError::InvalidParameter(msg) => write!(f, "Invalid parameter: {}", msg),
            TimerError::NotSupported(msg) => write!(f, "Not supported: {}", msg),
        }
    }
}

impl std::error::Error for TimerError {}

/// Platform-specific timer operations
///
/// This trait abstracts OS-level timing primitives to enable cross-platform
/// support and testing with mock implementations.
pub trait PlatformTimer: Send + Sync {
    /// Get current time in milliseconds since an arbitrary epoch
    fn get_msec(&self) -> u32;

    /// Get high-resolution current time
    fn get_time_spec(&self) -> TimeSpec;

    /// Sleep for the specified number of seconds
    fn sleep_sec(&self, seconds: u32);

    /// Sleep for the specified number of milliseconds
    fn sleep_ms(&self, milliseconds: u32) -> u32;

    /// Get the minimum OS sleep granularity in milliseconds
    fn get_sleep_min_ms(&self) -> u32;

    /// Get the OS sleep increment in milliseconds
    fn get_sleep_inc_ms(&self) -> u32;

    /// Get the OS clock resolution in milliseconds
    fn get_clock_resolution_ms(&self) -> u32;

    /// Get the OS tick rate in Hertz
    fn get_tick_hz(&self) -> u32;

    /// Set the priority of the current thread
    fn set_thread_priority(&self, priority: ThreadPriority) -> TimerResult<()>;

    /// Check if the platform supports idle capability
    fn idle_capable(&self) -> Option<(u32, u32)>;

    /// Initialize the OS millisecond sleep subsystem
    fn init_ms_sleep(&self) -> u32;

    /// Clone the platform timer into a boxed trait object
    fn box_clone(&self) -> Box<dyn PlatformTimer>;
}

/// Enable cloning of boxed PlatformTimer trait objects
impl Clone for Box<dyn PlatformTimer> {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}

/// Clock calibration operations
pub trait ClockCalibration {
    /// Initialize calibration for this clock
    fn init_calibration(&mut self, frequency_hz: f64, initial_ips: i32) -> i32;

    /// Perform a calibration tick
    fn calibrate(&mut self, ticks_per_second: u32) -> TimerResult<i32>;

    /// Acknowledge a clock tick
    fn tick_acknowledge(&mut self, elapsed_time: u32) -> TimerResult<()>;

    /// Get the current instructions per tick value
    fn instructions_per_tick(&self) -> i32;

    /// Get the calibrated tick size in instructions
    fn tick_size(&self) -> i32;

    /// Check if this clock is calibrated and stable
    fn is_calibrated(&self) -> bool;

    /// Reset calibration state
    fn reset_calibration(&mut self);
}

/// Idle detection and management
pub trait IdleManager {
    /// Check if idle mode is enabled
    fn is_enabled(&self) -> bool;

    /// Enable or disable idle mode
    fn set_enabled(&mut self, enabled: bool);

    /// Check if the simulator is currently waiting in idle mode
    fn is_waiting(&self) -> bool;

    /// Attempt to enter idle mode
    fn try_idle(&mut self, timer_id: u32, instructions_since_check: u32) -> bool;

    /// Get the idle calibration percentage (accuracy)
    fn get_calibration_percent(&self) -> u32;

    /// Get the stability threshold in seconds
    fn get_stability_threshold(&self) -> u32;

    /// Set the stability threshold in seconds
    fn set_stability_threshold(&mut self, seconds: u32) -> TimerResult<()>;
}

/// Throttling management
pub trait ThrottleManager {
    /// Get the current throttle type
    fn get_throttle_type(&self) -> ThrottleType;

    /// Set throttling parameters
    fn set_throttle(&mut self, throttle_type: ThrottleType) -> TimerResult<()>;

    /// Schedule a throttle delay if needed
    fn schedule_throttle(&mut self, instructions_executed: u32);

    /// Cancel any pending throttle delays
    fn cancel_throttle(&mut self);

    /// Get the drift percentage threshold for recalibration
    fn get_drift_percent(&self) -> u32;

    /// Set the drift percentage threshold
    fn set_drift_percent(&mut self, percent: u32) -> TimerResult<()>;
}

/// Throttle type and configuration
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThrottleType {
    /// No throttling
    None,

    /// Megacycles per second
    MegaCyclesPerSec(u32),

    /// Kilocycles per second
    KiloCyclesPerSec(u32),

    /// Percentage of host CPU (0-100)
    Percent(u32),

    /// Specific delay: execute `instructions` then sleep `delay_ms` milliseconds
    Specific { instructions: u32, delay_ms: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timespec_diff() {
        let t1 = TimeSpec::new(100, 500_000_000);
        let t2 = TimeSpec::new(99, 700_000_000);
        let diff = t1.diff(&t2);

        assert_eq!(diff.tv_sec, 0);
        assert_eq!(diff.tv_nsec, 800_000_000);
    }

    #[test]
    fn test_timespec_as_millis() {
        let t = TimeSpec::new(5, 500_000_000);
        assert_eq!(t.as_millis(), 5500);
    }
}
