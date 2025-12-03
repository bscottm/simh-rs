//! Clock calibration implementation
//!
//! This module provides calibration algorithms to match simulated time
//! with real wall-clock time. The algorithm is self-regulating and adjusts
//! the number of instructions executed per clock tick to maintain synchronization.

use super::traits::{PlatformTimer, TimerResult};
use super::types::{CalibrationState, RealTimeClock};
use super::SIM_TMAX;

/// Milliseconds per second
const MSEC_PER_SEC_F64: f64 = 1000.0;

/// Advanced calibration with EWMA smoothing and virtual time tracking
///
/// This combines the virtual time tracking from C SIMH with EWMA smoothing
/// for more stable calibration that doesn't oscillate.
///
/// # Arguments
/// * `rtc` - The RTC to calibrate
/// * `calibration_state` - Global calibration state (for EWMA tracking)
/// * `platform` - Platform timer for measurements
/// * `total_instructions` - Total instructions executed
/// * `ticks_per_second` - Expected ticks per second
/// * `idle_pct` - Percentage of time spent idle (0-100)
///
/// # Returns
/// The calibrated instructions per tick value
pub fn calibrate_clock_ewma(
    rtc: &mut RealTimeClock,
    calibration_state: &mut CalibrationState,
    platform: &dyn PlatformTimer,
    total_instructions: i64,
    ticks_per_second: u32,
    idle_pct: u32,
) -> TimerResult<i32> {
    // Increment tick counter
    rtc.ticks += 1;

    // Not yet time to calibrate?
    if rtc.ticks < ticks_per_second {
        return Ok(rtc.current_delay);
    }

    // Reset tick counter for next second
    rtc.ticks = 0;

    // Get current wall time
    let new_rtime = platform.get_msec();

    // Check for time running backwards
    if new_rtime < rtc.rtime {
        rtc.calib_backwards += 1;
        rtc.vtime = new_rtime;
        rtc.rtime = new_rtime;
        rtc.next_calibration_interval = 1000;
        rtc.base_delay = rtc.current_delay;
        return Ok(rtc.current_delay);
    }

    // Calculate elapsed real time
    let delta_rtime = new_rtime - rtc.rtime;
    rtc.rtime = new_rtime;

    // Advance virtual time by 1 second
    rtc.vtime += 1000;

    // Check for gap too big
    if delta_rtime > 30000 {
        rtc.calib_gap_too_big += 1;
        rtc.vtime = rtc.rtime;
        rtc.next_calibration_interval = 1000;
        rtc.instructions_last = total_instructions;
        rtc.base_delay = rtc.current_delay;
        return Ok(rtc.current_delay);
    }

    // Skip calibration if idle percentage is too high
    if idle_pct > 80 {
        rtc.calib_skip_idle += 1;
        rtc.instructions_last = total_instructions;
        rtc.base_delay = rtc.current_delay;
        return Ok(rtc.current_delay);
    }

    // Calculate instructions executed
    let instructions_executed = total_instructions - rtc.instructions_last;
    rtc.instructions_last = total_instructions;

    if delta_rtime == 0 {
        return Ok(rtc.current_delay);
    }

    // Calculate actual IPS
    let actual_ips = (instructions_executed as f64 * MSEC_PER_SEC_F64) / delta_rtime as f64;

    // Update EWMA smoothing
    calibration_state.update_ewma(actual_ips);

    // Use predicted IPS (includes acceleration)
    let smoothed_ips = calibration_state.predicted_ips();

    // Calculate base rate using smoothed IPS
    rtc.base_delay = (smoothed_ips / ticks_per_second as f64) as i32;

    // Calculate virtual time gap
    let delta_vtime = (rtc.vtime as i32 - rtc.rtime as i32).clamp(-SIM_TMAX, SIM_TMAX);

    // Next interval adjusts for gap
    rtc.next_calibration_interval = (1000 + delta_vtime) as u32;

    // Calculate instructions per tick with gap adjustment
    rtc.current_delay =
        ((rtc.base_delay as f64 * rtc.next_calibration_interval as f64) / MSEC_PER_SEC_F64) as i32;

    // Ensure values never go negative or zero
    if rtc.base_delay <= 0 {
        rtc.base_delay = 1;
    }
    if rtc.current_delay <= 0 {
        rtc.current_delay = 1;
    }

    // Prevent excessive swings (less needed with EWMA, but keep for safety)
    let max_swing = rtc.initial_delay_estimate.saturating_mul(5); // Reduced from 10x due to EWMA
    let min_swing = rtc.initial_delay_estimate / 5;

    if rtc.current_delay > max_swing && max_swing > 0 {
        rtc.current_delay = max_swing;
    }
    if rtc.current_delay < min_swing && min_swing > 0 {
        rtc.current_delay = min_swing;
    }

    rtc.calib_samples += 1;

    Ok(rtc.current_delay)
}

/// Acknowledge a clock tick for calibration tracking
pub fn tick_acknowledge(rtc: &mut RealTimeClock, elapsed_instructions: u32) -> TimerResult<()> {
    rtc.ticks += 1;
    rtc.instructions_executed += elapsed_instructions as i64;
    Ok(())
}

/// Check if calibration has drifted beyond threshold
pub fn check_drift(rtc: &RealTimeClock, threshold_percent: u32) -> bool {
    if rtc.initial_delay_estimate == 0 {
        return false;
    }

    let drift_pct =
        ((rtc.current_delay - rtc.initial_delay_estimate).abs() * 100) / rtc.initial_delay_estimate;
    drift_pct as u32 > threshold_percent
}

/// Reset calibration state
pub fn reset_calibration(rtc: &mut RealTimeClock) {
    rtc.current_delay = rtc.initial_delay_estimate;
    rtc.base_delay = rtc.initial_delay_estimate;
    rtc.ticks = 0;
    rtc.calib_samples = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tick_acknowledge() {
        let mut rtc = RealTimeClock::new();
        rtc.hz = 60.0;
        rtc.initial_delay_estimate = 10000;

        assert_eq!(rtc.ticks, 0);

        tick_acknowledge(&mut rtc, 1000).unwrap();
        assert_eq!(rtc.ticks, 1);
        assert_eq!(rtc.instructions_executed, 1000);

        tick_acknowledge(&mut rtc, 1000).unwrap();
        assert_eq!(rtc.ticks, 2);
        assert_eq!(rtc.instructions_executed, 2000);
    }

    #[test]
    fn test_check_drift() {
        let mut rtc = RealTimeClock::new();
        rtc.initial_delay_estimate = 1000;
        rtc.current_delay = 1000;

        // No drift
        assert!(!check_drift(&rtc, 10));

        // 5% drift
        rtc.current_delay = 1050;
        assert!(!check_drift(&rtc, 10));

        // 15% drift
        rtc.current_delay = 1150;
        assert!(check_drift(&rtc, 10));
    }

    #[test]
    fn test_reset_calibration() {
        let mut rtc = RealTimeClock::new();
        rtc.initial_delay_estimate = 1000;
        rtc.current_delay = 1500;
        rtc.base_delay = 1200;
        rtc.ticks = 10;
        rtc.calib_samples = 5;

        reset_calibration(&mut rtc);

        assert_eq!(rtc.current_delay, 1000);
        assert_eq!(rtc.base_delay, 1000);
        assert_eq!(rtc.ticks, 0);
        assert_eq!(rtc.calib_samples, 0);
    }

    // These two tests need some rethinking on how to simulate time running backward and large gaps.
    //
    //     #[test]
    //     fn test_advanced_calibration_time_backwards() {
    //         let platform = create_platform_timer();
    //         let mut rtc = RealTimeClock::new();
    //         rtc.hz = 60.0;
    //         rtc.initial_delay_estimate = 10000;
    //         rtc.current_delay = 10000;
    //         rtc.rtime = 1000000; // Large time
    //         rtc.vtime = 1000000;
    //         rtc.next_calibration_interval = 1000;
    //         rtc.ticks = 60; // Trigger calibration

    //         // Set current time to smaller value (time went backwards)
    //         let mut calib_state = CalibrationState::new(5_000_000);
    //         let result = calibrate_clock_ewma(&mut rtc, &mut calib_state, platform.as_ref(), 600000, 60, 0);
    //         assert!(result.is_ok());

    //         // Should have detected backwards time
    //         assert_eq!(rtc.calib_backwards, 1);
    //     }

    //     #[test]
    //     fn test_advanced_calibration_gap_too_big() {
    //         let platform = create_platform_timer();
    //         let mut rtc = RealTimeClock::new();
    //         rtc.hz = 60.0;
    //         rtc.initial_delay_estimate = 10000;
    //         rtc.current_delay = 10000;
    //         rtc.rtime = 0; // Start at 0
    //         rtc.vtime = 0;
    //         rtc.next_calibration_interval = 1000;
    //         rtc.ticks = 60; // Trigger calibration

    //         // Simulate 35 second gap (> 30 second threshold)
    //         std::thread::sleep(std::time::Duration::from_millis(100));
    //         let future_time = 35000; // Simulate large gap
    //         rtc.rtime = 0; // Force calculation with artificial gap

    //         // Manually trigger with simulated large delta
    //         // (In real code, platform.get_msec() would return the large value)

    //         // This test is simplified - in practice the platform timer would show the gap
    //         assert!(rtc.calib_gap_too_big >= 0);
    //     }
}
