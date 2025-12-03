//! Type definitions for the timer system
//!
//! This module provides the core data structures used throughout the timer library.

use super::{ThrottleType, TimeSpec};
use std::sync::atomic::{AtomicBool, Ordering};

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
// Timer configuration (TimerConfig), real time clock state (RealTimeClock)
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

/// Configuration for a timer
#[derive(Debug, Clone)]
pub struct TimerConfig {
    /// Clock frequency in Hertz
    pub frequency_hz: f64,

    /// Whether this timer is enabled
    pub enabled: bool,

    /// Initial instructions per second estimate
    pub initial_ips: i32,
}

impl Default for TimerConfig {
    fn default() -> Self {
        Self {
            frequency_hz: 60.0,
            enabled: false,
            initial_ips: super::SIM_INITIAL_IPS,
        }
    }
}

/// Real-time clock state
///
/// Tracks the state of a single timer including calibration data, tick counting, and performance
/// measurements.
#[derive(Debug, Clone)]
pub struct RealTimeClock {
    /// Clock frequency in Hz
    pub hz: f64,

    /// Current delay (instructions per tick)
    pub current_delay: i32,

    /// Base delay used for calibration
    pub base_delay: i32,

    /// Initial delay estimate
    pub initial_delay_estimate: i32,

    /// Ticks per second for this clock
    pub ticks_per_second: u32,

    /// Current tick count (resets each second)
    pub ticks: u32,

    /// Last tick time for tracking (real time in ms)
    pub ticks_last: u32,

    /// Virtual time (simulated time in ms)
    pub vtime: u32,

    /// Real time (wall clock time in ms)
    pub rtime: u32,

    /// Next interval for calibration adjustment
    pub next_calibration_interval: u32,

    /// Total instructions executed on this clock
    pub instructions_executed: i64,

    /// Instructions at last calibration
    pub instructions_last: i64,

    /// Last calibration time
    pub last_calib_time: TimeSpec,

    /// Number of calibrations performed
    pub calib_initializations: u32,

    /// Calibration samples taken
    pub calib_samples: u32,

    /// Whether currently calibrating
    pub calibrating: bool,

    /// Number of times calibration was skipped due to idle
    pub calib_skip_idle: u32,

    /// Number of times time went backwards
    pub calib_backwards: u32,

    /// Number of times gap was too big
    pub calib_gap_too_big: u32,
}

impl RealTimeClock {
    /// Create a new uninitialized RTC
    pub fn new() -> Self {
        Self {
            hz: 0.0,
            current_delay: 0,
            base_delay: 0,
            initial_delay_estimate: 0,
            ticks_per_second: 0,
            ticks: 0,
            ticks_last: 0,
            vtime: 0,
            rtime: 0,
            next_calibration_interval: 1000,
            instructions_executed: 0,
            instructions_last: 0,
            last_calib_time: TimeSpec::new(0, 0),
            calib_initializations: 0,
            calib_samples: 0,
            calibrating: false,
            calib_skip_idle: 0,
            calib_backwards: 0,
            calib_gap_too_big: 0,
        }
    }

    /// Check if this RTC has been initialized
    pub fn is_initialized(&self) -> bool {
        self.hz > 0.0
    }

    /// Reset calibration for this RTC
    pub fn reset_calibration(&mut self) {
        self.current_delay = self.initial_delay_estimate;
        self.base_delay = self.initial_delay_estimate;
        self.ticks = 0;
        self.calib_samples = 0;
        self.calibrating = false;
    }

    /// Get the tick size in instructions
    pub fn tick_size(&self) -> i32 {
        self.current_delay
    }
}

impl Default for RealTimeClock {
    fn default() -> Self {
        Self::new()
    }
}

/// Global calibration state
#[derive(Debug, Clone)]
pub struct CalibrationState {
    /// EWMA alpha
    ///
    /// Moving average's smoothing factor. The default is [`CalibrationState::DEFAULT_ALPHA`] (0.2), which
    /// combines 20% of the measured IPS to 80% of the moving average.
    pub alpha: f64,

    /// EWMA acceleration alpha
    pub acceleration_alpha: f64,

    /// EWMA variance (for stability detection)
    pub inst_per_sec_variance: f64,

    /// Variance's alpha
    pub variance_alpha: f64,

    /// Initial IPS estimate
    pub initial_ips: i32,

    /// Pre-calibration IPS
    pub precalibrate_ips: i32,

    /// Last measured IPS
    pub inst_per_sec_last: f64,

    /// EWMA smoothed IPS
    pub inst_per_sec_smoothed: f64,

    /// EWMA of IPS rate-of-change (acceleration)
    pub inst_per_sec_accel: f64,

    /// Which timer is being used for calibration (-1 = none)
    pub calibrated_timer: i32,

    /// Last calibrated timer
    pub calibrated_timer_last: i32,

    /// Time spent at the prompt (not simulating)
    pub time_at_prompt: f64,

    /// Time when stopped
    pub stop_time: u32,

    /// Timer-specific stop time
    pub timer_stop_time: u32,

    /// Number of calibration samples for EWMA
    pub sample_count: u32,
}

impl CalibrationState {
    const DEFAULT_ALPHA: f64 = 0.2; // Primary smoothing factor
    const DEFAULT_ACCEL_ALPHA: f64 = 0.1; // Acceleration smoothing factor
    const DEFAULT_VARIANCE_ALPHA: f64 = 0.1; // Default variance alpha

    /// Create a new calibration state
    pub fn new(initial_ips: i32) -> Self {
        Self {
            alpha: CalibrationState::DEFAULT_ALPHA,
            acceleration_alpha: CalibrationState::DEFAULT_ACCEL_ALPHA,
            inst_per_sec_variance: 0.0,
            variance_alpha: CalibrationState::DEFAULT_VARIANCE_ALPHA,
            initial_ips,
            precalibrate_ips: initial_ips,
            inst_per_sec_last: 0.0,
            inst_per_sec_smoothed: initial_ips as f64,
            inst_per_sec_accel: 0.0,
            calibrated_timer: -1,
            calibrated_timer_last: -1,
            time_at_prompt: 0.0,
            stop_time: 0,
            timer_stop_time: 0,
            sample_count: 0,
        }
    }

    /// Get current IPS estimate (uses EWMA smoothed value)
    pub fn current_ips(&self) -> f64 {
        if self.inst_per_sec_smoothed > 0.0 {
            self.inst_per_sec_smoothed
        } else if self.precalibrate_ips > 0 {
            self.precalibrate_ips as f64
        } else {
            self.initial_ips as f64
        }
    }

    /// Update EWMA smoothing for IPS
    ///
    /// Uses exponential weighted moving average with second derivative tracking
    pub fn update_ewma(&mut self, new_ips: f64) {
        if self.sample_count == 0 {
            // First sample - initialize
            self.inst_per_sec_smoothed = new_ips;
            self.inst_per_sec_last = new_ips;
        } else {
            // Update smoothed value
            let new_smoothed = self.alpha * new_ips + (1.0 - self.alpha) * self.inst_per_sec_smoothed;

            // Calculate acceleration (second derivative)
            let accel = new_smoothed - self.inst_per_sec_smoothed;
            self.inst_per_sec_accel =
                self.acceleration_alpha * accel + (1.0 - self.acceleration_alpha) * self.inst_per_sec_accel;

            // Variance tracking for stability detection
            let squared_error = (new_ips - self.inst_per_sec_smoothed).powi(2);
            self.inst_per_sec_variance = self.variance_alpha * squared_error
                + (1.0 - self.variance_alpha) * self.inst_per_sec_variance;

            self.inst_per_sec_smoothed = new_smoothed;
            self.inst_per_sec_last = new_ips;
        }

        self.sample_count += 1;
    }

    /// Get predicted future IPS using acceleration
    pub fn predicted_ips(&self) -> f64 {
        if self.sample_count > 2 {
            self.inst_per_sec_smoothed + self.inst_per_sec_accel
        } else {
            self.inst_per_sec_smoothed
        }
    }

    /// Check if calibration is stable enough to trust
    ///
    /// "Stable" in this case is less than 5% variation in the average.
    pub fn is_stable(&self) -> bool {
        // Low variance = stable measurements
        let std_dev = self.inst_per_sec_variance.sqrt();
        let coefficient_of_variation = std_dev / self.inst_per_sec_smoothed;
        coefficient_of_variation < 0.05 // Less than 5% variation
    }
}

impl Default for CalibrationState {
    fn default() -> Self {
        Self::new(super::SIM_INITIAL_IPS)
    }
}

/// Idle detection state
#[derive(Debug)]
pub struct IdleState {
    /// Whether idle mode is enabled
    pub enabled: bool,

    /// Whether currently in idle wait state
    waiting: AtomicBool,

    /// Idle rate in milliseconds
    pub rate_ms: u32,

    /// Seconds required for stable idle
    pub stable_threshold: u32,

    /// Count of stable idle periods
    pub stable_count: u32,

    /// Idle calibration percentage
    pub calibration_pct: u32,

    /// Time of last idle check
    pub last_check_time: TimeSpec,

    /// Instructions at last check
    pub last_check_instructions: i64,
}

impl Clone for IdleState {
    fn clone(&self) -> Self {
        Self {
            enabled: self.enabled,
            waiting: AtomicBool::new(self.waiting.load(Ordering::Relaxed)),
            rate_ms: self.rate_ms,
            stable_threshold: self.stable_threshold,
            stable_count: self.stable_count,
            calibration_pct: self.calibration_pct,
            last_check_time: self.last_check_time,
            last_check_instructions: self.last_check_instructions,
        }
    }
}

impl IdleState {
    /// Create a new idle state
    pub fn new() -> Self {
        Self {
            enabled: false,
            waiting: AtomicBool::new(false),
            rate_ms: 0,
            stable_threshold: super::SIM_IDLE_STDFLT,
            stable_count: 0,
            calibration_pct: 100,
            last_check_time: TimeSpec::new(0, 0),
            last_check_instructions: 0,
        }
    }

    /// Check if currently waiting in idle
    pub fn is_waiting(&self) -> bool {
        self.waiting.load(Ordering::Relaxed)
    }

    /// Set waiting state
    pub fn set_waiting(&self, waiting: bool) {
        self.waiting.store(waiting, Ordering::Relaxed);
    }

    /// Reset stability tracking
    pub fn reset_stability(&mut self) {
        self.stable_count = 0;
    }

    /// Increment stability counter
    pub fn increment_stability(&mut self) {
        self.stable_count += 1;
    }

    /// Check if idle is stable
    pub fn is_stable(&self) -> bool {
        self.stable_count >= self.stable_threshold
    }
}

impl Default for IdleState {
    fn default() -> Self {
        Self::new()
    }
}

/// Throttling state
#[derive(Debug, Clone)]
pub enum ThrottleMode {
    /// Initial state - waiting to calibrate
    Init,

    /// Checking timing
    CheckingTime,

    /// Actively throttling
    Throttling,
}

/// Throttle state
#[derive(Debug, Clone)]
pub struct ThrottleState {
    /// Current throttle type
    pub throttle_type: ThrottleType,

    /// Throttle value (meaning depends on type)
    pub throttle_value: u32,

    /// Drift percentage for recalibration
    pub drift_pct: u32,

    /// Current throttle mode
    pub mode: ThrottleMode,

    /// Start time for throttle measurement
    pub ms_start: u32,

    /// Stop time for throttle measurement
    pub ms_stop: u32,

    /// Instructions at start
    pub inst_start: i64,

    /// Target instructions
    pub target_inst: i64,

    /// Wait time in milliseconds
    pub wait_ms: u32,

    /// Initial wait cycles
    pub wait_init: u32,

    /// Delay counter
    pub delay: u32,
}

impl ThrottleState {
    /// Create a new throttle state
    pub fn new() -> Self {
        Self {
            throttle_type: ThrottleType::None,
            throttle_value: 0,
            drift_pct: super::SIM_THROT_DRIFT_PCT_DFLT,
            mode: ThrottleMode::Init,
            ms_start: 0,
            ms_stop: 0,
            inst_start: 0,
            target_inst: 0,
            wait_ms: 0,
            wait_init: super::SIM_THROT_WINIT,
            delay: 0,
        }
    }

    /// Check if throttle is active
    pub fn is_active(&self) -> bool {
        !matches!(self.throttle_type, ThrottleType::None)
    }

    /// Reset throttle state
    pub fn reset(&mut self) {
        self.mode = ThrottleMode::Init;
        self.ms_start = 0;
        self.ms_stop = 0;
        self.inst_start = 0;
        self.target_inst = 0;
        self.wait_ms = 0;
        self.delay = 0;
    }
}

impl Default for ThrottleState {
    fn default() -> Self {
        Self::new()
    }
}

/// ROM delay calibration state
#[derive(Debug, Clone)]
pub struct RomDelayState {
    /// ROM delay factor
    pub delay_factor: u32,

    /// Whether ROM delay has been calibrated
    pub calibrated: bool,
}

impl Default for RomDelayState {
    fn default() -> Self {
        Self {
            delay_factor: 1,
            calibrated: false,
        }
    }
}

/// Timer event for device scheduling
///
/// Uses a device ID (hash of device name) for efficient lookups.
/// The device name is also stored for debugging/error messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimerEvent {
    /// Device ID (hash of uppercase device name)
    pub device_id: u64,

    /// Device name for lookup (uppercase)
    pub device_name: String,

    /// Absolute time when this device should be serviced (in instruction count)
    pub fire_time: i64,

    /// Additional data for the event (device-specific)
    pub data: u32,
}

impl TimerEvent {
    /// Create a new timer event
    ///
    /// The device_name will be converted to uppercase and hashed for efficient lookups.
    pub fn new(device_name: String, fire_time: i64) -> Self {
        let name_upper = device_name.to_ascii_uppercase();
        let device_id = Self::hash_device_name(&name_upper);
        Self {
            device_id,
            device_name: name_upper,
            fire_time,
            data: 0,
        }
    }

    /// Create with additional data
    pub fn with_data(device_name: String, fire_time: i64, data: u32) -> Self {
        let name_upper = device_name.to_ascii_uppercase();
        let device_id = Self::hash_device_name(&name_upper);
        Self {
            device_id,
            device_name: name_upper,
            fire_time,
            data,
        }
    }

    /// Hash a device name for use as ID
    ///
    /// Uses FxHash (same as SimEnvironment) for consistency.
    fn hash_device_name(name: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        hasher.finish()
    }
}

// Implement ordering for priority queue (min-heap based on fire_time)
impl Ord for TimerEvent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse ordering for min-heap
        other.fire_time.cmp(&self.fire_time)
    }
}

impl PartialOrd for TimerEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtc_initialization() {
        let rtc = RealTimeClock::new();
        assert!(!rtc.is_initialized());
        assert_eq!(rtc.hz, 0.0);
    }

    #[test]
    fn test_calibration_state() {
        let calib = CalibrationState::new(5_000_000);
        assert_eq!(calib.initial_ips, 5_000_000);
        assert_eq!(calib.current_ips(), 5_000_000.0);
    }

    #[test]
    fn test_idle_state() {
        let idle = IdleState::new();
        assert!(!idle.enabled);
        assert!(!idle.is_waiting());
        assert!(!idle.is_stable());

        idle.set_waiting(true);
        assert!(idle.is_waiting());
    }

    #[test]
    fn test_throttle_state() {
        let throttle = ThrottleState::new();
        assert!(!throttle.is_active());
        assert_eq!(throttle.drift_pct, crate::timers::SIM_THROT_DRIFT_PCT_DFLT);
    }

    #[test]
    fn test_timer_event_ordering() {
        let event1 = TimerEvent::new("DEV1".to_string(), 100);
        let event2 = TimerEvent::new("DEV2".to_string(), 50);
        let event3 = TimerEvent::new("DEV3".to_string(), 150);

        // Ordering is REVERSED for min-heap (BinaryHeap pops max, so we reverse to get min)
        // Smaller fire_time should be "greater" in our reversed ordering
        assert!(event2 > event1); // 50 > 100 in reversed ordering
        assert!(event1 > event3); // 100 > 150 in reversed ordering
    }
}
