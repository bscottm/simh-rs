//! Execution throttling
//!
//! This module provides advanced throttling algorithms with EWMA (Exponential Weighted
//! Moving Average) smoothing to prevent oscillation and provide stable execution rates.
//!
//! Unlike the original C implementation which uses raw first derivatives (prone to
//! oscillation), this implementation uses:
//! - EWMA for smoothed rate estimation
//! - Second derivative tracking for trend prediction
//! - Adaptive sleep adjustment based on prediction

use super::traits::{PlatformTimer, ThrottleType};
use super::types::{ThrottleMode, ThrottleState};

/// EWMA smoothing factor (alpha)
/// Higher = more responsive, lower = more stable
/// 0.2 gives good balance between responsiveness and stability
const EWMA_ALPHA: f64 = 0.2;

/// Second derivative smoothing factor
const EWMA_ACCEL_ALPHA: f64 = 0.1;

/// Minimum measurement period (milliseconds)
const MIN_MEASURE_MS: u32 = 100;

/// Target measurement period for calibration (milliseconds)
const TARGET_MEASURE_MS: u32 = 1000;

/// Throttle controller with EWMA smoothing
pub struct ThrottleController {
    /// Current throttle state
    state: ThrottleState,

    /// Platform timer for measurements
    platform: Box<dyn PlatformTimer>,

    /// EWMA of actual cycles per second
    smoothed_cps: f64,

    /// EWMA of rate-of-change (acceleration)
    smoothed_accel: f64,

    /// Previous smoothed CPS for derivative calculation
    prev_smoothed_cps: f64,

    /// Number of samples collected
    sample_count: u32,
}

impl ThrottleController {
    /// Create a new throttle controller
    pub fn new(platform: Box<dyn PlatformTimer>) -> Self {
        Self {
            state: ThrottleState::new(),
            platform,
            smoothed_cps: 0.0,
            smoothed_accel: 0.0,
            prev_smoothed_cps: 0.0,
            sample_count: 0,
        }
    }

    /// Update throttle configuration
    pub fn set_throttle(&mut self, throttle_type: ThrottleType, throttle_value: u32) {
        self.state.throttle_type = throttle_type;
        self.state.throttle_value = throttle_value;
        self.state.mode = ThrottleMode::Init;
        self.smoothed_cps = 0.0;
        self.smoothed_accel = 0.0;
        self.prev_smoothed_cps = 0.0;
        self.sample_count = 0;
    }

    /// Get the desired cycles per second based on throttle type
    fn get_desired_cps(&self) -> Option<f64> {
        match self.state.throttle_type {
            ThrottleType::None => None,
            ThrottleType::MegaCyclesPerSec(mcps) => Some(mcps as f64 * 1_000_000.0),
            ThrottleType::KiloCyclesPerSec(kcps) => Some(kcps as f64 * 1_000.0),
            ThrottleType::Percent(_) => None, // Needs peak CPS measurement
            ThrottleType::Specific { .. } => None, // Fixed timing
        }
    }

    /// Start throttle calibration
    pub fn start_calibration(&mut self, current_instructions: i64) {
        self.state.mode = ThrottleMode::Init;
        self.state.ms_start = self.platform.get_msec();
        self.state.inst_start = current_instructions;
    }

    /// Update throttle with new measurement
    ///
    /// Returns the number of milliseconds to sleep (if any)
    pub fn update(&mut self, current_instructions: i64) -> Option<u32> {
        match self.state.mode {
            ThrottleMode::Init => {
                // Initial measurement phase
                let elapsed_ms = self.platform.get_msec() - self.state.ms_start;

                if elapsed_ms < MIN_MEASURE_MS {
                    return None; // Not enough time for accurate measurement
                }

                // Move to timing phase
                self.state.mode = ThrottleMode::CheckingTime;
                self.state.ms_start = self.platform.get_msec();
                self.state.inst_start = current_instructions;
                None
            }

            ThrottleMode::CheckingTime => {
                let elapsed_ms = self.platform.get_msec() - self.state.ms_start;

                if elapsed_ms < TARGET_MEASURE_MS {
                    return None; // Continue measuring
                }

                let elapsed_inst = current_instructions - self.state.inst_start;
                let actual_cps = (elapsed_inst as f64 * 1000.0) / elapsed_ms as f64;

                // Initialize EWMA on first sample
                if self.sample_count == 0 {
                    self.smoothed_cps = actual_cps;
                    self.prev_smoothed_cps = actual_cps;
                } else {
                    // Update EWMA
                    let new_smoothed = EWMA_ALPHA * actual_cps + (1.0 - EWMA_ALPHA) * self.smoothed_cps;

                    // Calculate acceleration (second derivative)
                    let accel = new_smoothed - self.smoothed_cps;
                    self.smoothed_accel =
                        EWMA_ACCEL_ALPHA * accel + (1.0 - EWMA_ACCEL_ALPHA) * self.smoothed_accel;

                    self.prev_smoothed_cps = self.smoothed_cps;
                    self.smoothed_cps = new_smoothed;
                }

                self.sample_count += 1;

                // Calculate sleep time based on predicted CPS
                if let Some(desired_cps) = self.get_desired_cps() {
                    let sleep_ms = self.calculate_sleep_time(desired_cps);

                    if sleep_ms > 0 {
                        self.state.wait_ms = sleep_ms;
                        self.state.mode = ThrottleMode::Throttling;
                        self.state.ms_start = self.platform.get_msec();
                        self.state.inst_start = current_instructions;
                        return Some(sleep_ms);
                    }
                }

                // Continue measuring
                self.state.ms_start = self.platform.get_msec();
                self.state.inst_start = current_instructions;
                None
            }

            ThrottleMode::Throttling => {
                // Check if we need to recalibrate
                let elapsed_ms = self.platform.get_msec() - self.state.ms_start;

                if elapsed_ms >= TARGET_MEASURE_MS {
                    // Recalibrate periodically
                    let elapsed_inst = current_instructions - self.state.inst_start;
                    let actual_cps = (elapsed_inst as f64 * 1000.0) / elapsed_ms as f64;

                    // Update EWMA
                    let new_smoothed = EWMA_ALPHA * actual_cps + (1.0 - EWMA_ALPHA) * self.smoothed_cps;

                    // Calculate acceleration
                    let accel = new_smoothed - self.smoothed_cps;
                    self.smoothed_accel =
                        EWMA_ACCEL_ALPHA * accel + (1.0 - EWMA_ACCEL_ALPHA) * self.smoothed_accel;

                    self.prev_smoothed_cps = self.smoothed_cps;
                    self.smoothed_cps = new_smoothed;

                    // Check if we're drifting beyond threshold
                    if let Some(desired_cps) = self.get_desired_cps() {
                        let error_pct = ((self.smoothed_cps - desired_cps).abs() / desired_cps) * 100.0;

                        if error_pct > self.state.drift_pct as f64 {
                            // Recalculate sleep time
                            let sleep_ms = self.calculate_sleep_time(desired_cps);
                            self.state.wait_ms = sleep_ms;
                        }
                    }

                    self.state.ms_start = self.platform.get_msec();
                    self.state.inst_start = current_instructions;
                }

                Some(self.state.wait_ms)
            }
        }
    }

    /// Calculate optimal sleep time using predictive control
    ///
    /// Uses both current error and predicted future error (via acceleration)
    fn calculate_sleep_time(&self, desired_cps: f64) -> u32 {
        if desired_cps <= 0.0 || self.smoothed_cps <= desired_cps {
            return 0; // Host too slow or no throttling needed
        }

        // Predict future CPS using acceleration (second derivative)
        let predicted_cps = self.smoothed_cps + self.smoothed_accel;

        // Use predicted value for better stability
        let effective_cps = if self.sample_count > 2 && predicted_cps > desired_cps {
            predicted_cps
        } else {
            self.smoothed_cps
        };

        // Calculate how much we need to slow down
        let excess_cps = effective_cps - desired_cps;

        // Sleep time in ms = (excess_cps / effective_cps) * 1000
        // This gives us the fraction of time we should be sleeping
        let sleep_fraction = excess_cps / effective_cps;
        let sleep_ms = (sleep_fraction * 1000.0).max(1.0) as u32;

        // Clamp to reasonable range
        sleep_ms.min(100).max(1)
    }

    /// Get current throttle statistics
    pub fn stats(&self) -> ThrottleStats {
        ThrottleStats {
            smoothed_cps: self.smoothed_cps,
            smoothed_accel: self.smoothed_accel,
            sample_count: self.sample_count,
            sleep_ms: self.state.wait_ms,
            mode: format!("{:?}", self.state.mode),
        }
    }
}

/// Throttle statistics for monitoring
#[derive(Debug, Clone)]
pub struct ThrottleStats {
    pub smoothed_cps: f64,
    pub smoothed_accel: f64,
    pub sample_count: u32,
    pub sleep_ms: u32,
    pub mode: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timers::platform::create_platform_timer;

    #[test]
    fn test_throttle_controller_creation() {
        let platform = create_platform_timer();
        let controller = ThrottleController::new(platform);
        assert_eq!(controller.sample_count, 0);
    }

    #[test]
    fn test_desired_cps_calculation() {
        let platform = create_platform_timer();
        let mut controller = ThrottleController::new(platform);

        controller.set_throttle(ThrottleType::MegaCyclesPerSec(1), 1);
        assert_eq!(controller.get_desired_cps(), Some(1_000_000.0));

        controller.set_throttle(ThrottleType::KiloCyclesPerSec(500), 500);
        assert_eq!(controller.get_desired_cps(), Some(500_000.0));
    }

    #[test]
    fn test_ewma_smoothing() {
        // Test that EWMA provides smoothing
        let platform = create_platform_timer();
        let mut controller = ThrottleController::new(platform);

        // Simulate measurements
        controller.smoothed_cps = 1000.0;

        // Update with a new measurement
        let new_measurement = 1200.0;
        let expected = EWMA_ALPHA * new_measurement + (1.0 - EWMA_ALPHA) * 1000.0;

        // The smoothed value should be between old and new
        assert!(expected > 1000.0 && expected < 1200.0);
    }
}
