//! Timer Manager - Central coordinator for all timer operations
//!
//! The TimerManager is the main entry point for using the timer library.
//! It manages multiple independent real-time clocks, handles calibration,
//! idle detection, and throttling.

use super::calibration;
use super::throttle::ThrottleController;
use super::traits::{
    IdleManager, PlatformTimer, ThrottleManager, ThrottleType, TimeSpec, TimerError, TimerResult,
};
use super::types::{CalibrationState, IdleState, RealTimeClock, ThrottleState, TimerEvent};
use super::{SIM_IDLE_STMAX, SIM_IDLE_STMIN, SIM_NTIMERS};
use std::collections::BinaryHeap;

/// Main timer manager
///
/// This is the central coordinator for all timer operations. It manages:
/// - Multiple independent real-time clocks
/// - Calibration of simulated vs real time
/// - Idle detection and CPU optimization
/// - Execution throttling
/// - Event scheduling for device service routines
pub struct TimerManager {
    /// Platform-specific timer implementation
    platform: Box<dyn PlatformTimer>,

    /// Array of real-time clocks
    rtcs: [RealTimeClock; SIM_NTIMERS + 1],

    /// Global calibration state
    calibration: CalibrationState,

    /// Idle detection state
    idle: IdleState,

    /// Throttling state (legacy, for compatibility)
    throttle: ThrottleState,

    /// Advanced throttle controller with EWMA smoothing
    throttle_controller: Option<ThrottleController>,

    /// Total instructions executed (for calibration)
    total_instructions: i64,

    /// Whether timer services are currently running
    services_running: bool,

    /// Priority queue of scheduled events (min-heap by fire_time)
    event_queue: BinaryHeap<TimerEvent>,
}

impl TimerManager {
    /// Create a new timer manager with the given platform timer
    ///
    /// # Arguments
    /// * `platform` - Platform-specific timer implementation
    ///
    /// # Example
    /// ```
    /// use sim_core::timers::{TimerManager, create_platform_timer};
    ///
    /// let platform = create_platform_timer();
    /// let manager = TimerManager::new(platform);
    /// ```
    pub fn new(platform: Box<dyn PlatformTimer>) -> Self {
        // Initialize the platform timer
        platform.init_ms_sleep();

        Self {
            platform,
            rtcs: std::array::from_fn(|_| RealTimeClock::new()),
            calibration: CalibrationState::default(),
            idle: IdleState::new(),
            throttle: ThrottleState::new(),
            throttle_controller: None,
            total_instructions: 0,
            services_running: false,
            event_queue: BinaryHeap::new(),
        }
    }

    /// Initialize a timer with the given frequency
    ///
    /// # Arguments
    /// * `timer_id` - Timer ID (0 to SIM_NTIMERS-1)
    /// * `frequency_hz` - Clock frequency in Hertz
    ///
    /// # Returns
    /// Ok(()) on success, or an error if the timer ID is invalid
    pub fn init_timer(&mut self, timer_id: usize, frequency_hz: f64) -> TimerResult<()> {
        if timer_id > SIM_NTIMERS {
            return Err(TimerError::InvalidTimerId(timer_id));
        }

        let rtc = &mut self.rtcs[timer_id];
        rtc.hz = frequency_hz;
        rtc.initial_delay_estimate = (self.calibration.initial_ips as f64 / frequency_hz) as i32;
        rtc.current_delay = rtc.initial_delay_estimate;
        rtc.base_delay = rtc.initial_delay_estimate;
        rtc.ticks_per_second = frequency_hz as u32;
        rtc.last_calib_time = self.platform.get_time_spec();
        rtc.calib_initializations += 1;

        // Initialize virtual and real time for advanced calibration
        let current_time = self.platform.get_msec();
        rtc.rtime = current_time;
        rtc.vtime = current_time;
        rtc.next_calibration_interval = 1000;

        Ok(())
    }

    /// Get the current instructions per second estimate
    pub fn instructions_per_sec(&self) -> f64 {
        self.calibration.current_ips()
    }

    /// Set the instructions per second estimate
    pub fn set_instructions_per_sec(&mut self, ips: f64) {
        self.calibration.inst_per_sec_last = ips;
    }

    /// Add instructions to the total count
    pub fn add_instructions(&mut self, count: i64) {
        self.total_instructions += count;
    }

    /// Get the total instructions executed
    pub fn total_instructions(&self) -> i64 {
        self.total_instructions
    }

    /// Get a reference to a specific RTC
    pub fn get_rtc(&self, timer_id: usize) -> TimerResult<&RealTimeClock> {
        if timer_id > SIM_NTIMERS {
            return Err(TimerError::InvalidTimerId(timer_id));
        }
        Ok(&self.rtcs[timer_id])
    }

    /// Get a mutable reference to a specific RTC
    pub fn get_rtc_mut(&mut self, timer_id: usize) -> TimerResult<&mut RealTimeClock> {
        if timer_id > SIM_NTIMERS {
            return Err(TimerError::InvalidTimerId(timer_id));
        }
        Ok(&mut self.rtcs[timer_id])
    }

    /// Start timer services
    ///
    /// This should be called when the simulation starts running.
    pub fn start_services(&mut self) {
        if !self.services_running {
            self.calibration.stop_time = self.platform.get_msec();
            self.services_running = true;
        }
    }

    /// Stop timer services
    ///
    /// This should be called when the simulation stops.
    pub fn stop_services(&mut self) {
        if self.services_running {
            let stop_time = self.platform.get_msec();
            let elapsed = stop_time - self.calibration.stop_time;
            self.calibration.time_at_prompt += elapsed as f64 / 1000.0;
            self.services_running = false;
        }
    }

    /// Get the platform timer
    pub fn platform(&self) -> &dyn PlatformTimer {
        self.platform.as_ref()
    }

    /// Get current time in milliseconds
    pub fn get_msec(&self) -> u32 {
        self.platform.get_msec()
    }

    /// Get high-resolution current time
    pub fn get_time_spec(&self) -> TimeSpec {
        self.platform.get_time_spec()
    }

    /// Sleep for the specified number of milliseconds
    pub fn sleep_ms(&self, milliseconds: u32) -> u32 {
        self.platform.sleep_ms(milliseconds)
    }

    /// Get calibrated timer ID
    ///
    /// Returns the ID of the timer currently being used for calibration,
    /// or -1 if no timer is calibrated.
    pub fn calibrated_timer(&self) -> i32 {
        self.calibration.calibrated_timer
    }

    /// Check if any timer is initialized
    pub fn has_initialized_timers(&self) -> bool {
        self.rtcs.iter().any(|rtc| rtc.is_initialized())
    }

    /// Get the number of initialized timers
    pub fn initialized_timer_count(&self) -> usize {
        self.rtcs.iter().filter(|rtc| rtc.is_initialized()).count()
    }

    /// Calculate host speed factor
    ///
    /// Returns the ratio of the initial IPS estimate to the pre-calibrated IPS.
    /// Values > 1.0 indicate the host is slower than expected.
    /// Values < 1.0 indicate the host is faster than expected.
    pub fn host_speed_factor(&self) -> f64 {
        if self.calibration.precalibrate_ips > self.calibration.initial_ips {
            1.0
        } else {
            self.calibration.initial_ips as f64 / self.calibration.precalibrate_ips as f64
        }
    }

    /// Reset all calibration data
    pub fn reset_calibration(&mut self) {
        for rtc in &mut self.rtcs {
            if rtc.is_initialized() {
                rtc.reset_calibration();
            }
        }
        self.calibration.inst_per_sec_last = 0.0;
        self.calibration.calibrated_timer = -1;
    }

    /// Get OS clock characteristics
    pub fn clock_characteristics(&self) -> (u32, u32, u32, u32) {
        (
            self.platform.get_sleep_min_ms(),
            self.platform.get_sleep_inc_ms(),
            self.platform.get_clock_resolution_ms(),
            self.platform.get_tick_hz(),
        )
    }

    /// Calibrate a timer
    ///
    /// This should be called periodically (typically once per simulated second)
    /// to adjust the timer to match wall-clock time.
    ///
    /// Uses EWMA (Exponential Weighted Moving Average) smoothing with second
    /// derivative tracking for stable, non-oscillating calibration.
    ///
    /// # Arguments
    /// * `timer_id` - Timer ID to calibrate
    /// * `ticks_per_second` - How many ticks per second to calibrate for
    ///
    /// # Returns
    /// The calibrated instructions per tick value
    pub fn calibrate_timer(&mut self, timer_id: usize, ticks_per_second: u32) -> TimerResult<i32> {
        if timer_id > SIM_NTIMERS {
            return Err(TimerError::InvalidTimerId(timer_id));
        }

        // Get idle percentage for this calibration
        let idle_pct = 0; // TODO: Calculate from idle state

        // Use EWMA-smoothed advanced calibration
        let result = calibration::calibrate_clock_ewma(
            &mut self.rtcs[timer_id],
            &mut self.calibration,
            self.platform.as_ref(),
            self.total_instructions,
            ticks_per_second,
            idle_pct,
        )?;

        // Update the calibrated timer
        if self.calibration.calibrated_timer == -1 {
            self.calibration.calibrated_timer = timer_id as i32;
        }

        // Store last IPS (already updated via EWMA in calibrate_clock_ewma)
        if self.rtcs[timer_id].hz > 0.0 {
            self.calibration.inst_per_sec_last = (result as f64) * self.rtcs[timer_id].hz;
        }

        // Adjust dependent timers
        self.adjust_dependent_timers(timer_id, ticks_per_second, result)?;

        Ok(result)
    }

    /// Adjust dependent timers based on master timer calibration
    ///
    /// When one timer is calibrated, other timers are adjusted proportionally
    /// to maintain their relative frequencies.
    fn adjust_dependent_timers(
        &mut self,
        master_timer_id: usize,
        master_ticks_per_second: u32,
        master_currd: i32,
    ) -> TimerResult<()> {
        for i in 0..=SIM_NTIMERS {
            if i != master_timer_id && self.rtcs[i].hz > 0.0 {
                // Calculate proportional delay for this timer
                // currd = (master_currd * master_hz) / this_hz
                let this_hz = self.rtcs[i].hz as u32;
                if this_hz > 0 {
                    self.rtcs[i].current_delay =
                        (master_currd * master_ticks_per_second as i32) / this_hz as i32;

                    // Ensure minimum value
                    if self.rtcs[i].current_delay <= 0 {
                        self.rtcs[i].current_delay = 1;
                    }
                }
            }
        }
        Ok(())
    }

    /// Acknowledge a timer tick for calibration
    pub fn tick_acknowledge(&mut self, timer_id: usize, elapsed_instructions: u32) -> TimerResult<()> {
        if timer_id > SIM_NTIMERS {
            return Err(TimerError::InvalidTimerId(timer_id));
        }

        let rtc = &mut self.rtcs[timer_id];
        calibration::tick_acknowledge(rtc, elapsed_instructions)
    }

    //=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
    // Event scheduling support
    //=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

    /// Schedule a device to be serviced after a delay
    ///
    /// # Arguments
    /// * `device_name` - Name of the device to service (e.g., "RKA0", "TTI")
    /// * `delay_instructions` - Number of instructions to wait before servicing
    ///
    /// # Example
    /// ```ignore
    /// // Schedule TTI to be serviced in 10,000 instructions
    /// timer_mgr.schedule_device("TTI", 10_000);
    /// ```
    pub fn schedule_device(&mut self, device_name: &str, delay_instructions: i64) {
        let fire_time = self.total_instructions + delay_instructions;
        let event = TimerEvent::new(device_name.to_uppercase(), fire_time);
        self.event_queue.push(event);
    }

    /// Schedule a device with additional event data
    pub fn schedule_device_with_data(&mut self, device_name: &str, delay_instructions: i64, data: u32) {
        let fire_time = self.total_instructions + delay_instructions;
        let event = TimerEvent::with_data(device_name.to_uppercase(), fire_time, data);
        self.event_queue.push(event);
    }

    /// Cancel all scheduled events for a specific device
    pub fn cancel_device(&mut self, device_name: &str) {
        let name_upper = device_name.to_ascii_uppercase();
        self.event_queue.retain(|event| event.device_name != name_upper);
    }

    /// Get devices that should be serviced now
    ///
    /// Returns a vector of device names whose scheduled time has arrived.
    /// This should be called each instruction batch in the simulator loop.
    ///
    /// # Returns
    /// Vector of `(device_name, event_data)` tuples for devices to service
    ///
    /// # Example
    /// ```ignore
    /// // In the simulator loop
    /// for (device_name, _data) in timer_mgr.get_ready_devices() {
    ///     env.service_device(&device_name)?;
    /// }
    /// ```
    pub fn get_ready_devices(&mut self) -> Vec<(String, u32)> {
        let mut ready = Vec::new();

        while let Some(event) = self.event_queue.peek() {
            if event.fire_time <= self.total_instructions {
                let event = self.event_queue.pop().unwrap();
                ready.push((event.device_name, event.data));
            } else {
                break;
            }
        }

        ready
    }

    /// Check how many instructions until the next scheduled event
    ///
    /// Returns `None` if no events are scheduled, otherwise returns the
    /// number of instructions until the next event.
    pub fn instructions_until_next_event(&self) -> Option<i64> {
        self.event_queue
            .peek()
            .map(|event| (event.fire_time - self.total_instructions).max(0))
    }

    /// Clear all scheduled events
    pub fn clear_events(&mut self) {
        self.event_queue.clear();
    }

    /// Get the number of scheduled events
    pub fn event_count(&self) -> usize {
        self.event_queue.len()
    }
}

impl IdleManager for TimerManager {
    fn is_enabled(&self) -> bool {
        self.idle.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.idle.enabled = enabled;
    }

    fn is_waiting(&self) -> bool {
        self.idle.is_waiting()
    }

    fn try_idle(&mut self, timer_id: u32, instructions_since_check: u32) -> bool {
        if !self.idle.enabled {
            return false;
        }

        // Check if we're in a stable idle pattern
        if !self.idle.is_stable() {
            self.idle.increment_stability();
            return false;
        }

        // Calculate how long to sleep
        let sleep_ms = self.idle.rate_ms.max(self.platform.get_sleep_min_ms());

        // Enter idle mode
        self.idle.set_waiting(true);
        let actual_sleep = self.platform.sleep_ms(sleep_ms);
        self.idle.set_waiting(false);

        // Return true if we actually saved CPU time
        actual_sleep >= sleep_ms / 2
    }

    fn get_calibration_percent(&self) -> u32 {
        self.idle.calibration_pct
    }

    fn get_stability_threshold(&self) -> u32 {
        self.idle.stable_threshold
    }

    fn set_stability_threshold(&mut self, seconds: u32) -> TimerResult<()> {
        if seconds < SIM_IDLE_STMIN {
            return Err(TimerError::InvalidParameter(format!(
                "Stability threshold must be at least {} seconds",
                SIM_IDLE_STMIN
            )));
        }
        if seconds > SIM_IDLE_STMAX {
            return Err(TimerError::InvalidParameter(format!(
                "Stability threshold must be at most {} seconds",
                SIM_IDLE_STMAX
            )));
        }
        self.idle.stable_threshold = seconds;
        Ok(())
    }
}

impl ThrottleManager for TimerManager {
    fn get_throttle_type(&self) -> ThrottleType {
        self.throttle.throttle_type
    }

    fn set_throttle(&mut self, throttle_type: ThrottleType) -> TimerResult<()> {
        // Validate throttle parameters
        match throttle_type {
            ThrottleType::None => {
                self.throttle_controller = None;
                self.throttle.throttle_type = ThrottleType::None;
            }
            ThrottleType::MegaCyclesPerSec(mcps) => {
                if mcps == 0 {
                    return Err(TimerError::ThrottleConfigError(
                        "MCPS must be greater than 0".to_string(),
                    ));
                }
                // Create advanced throttle controller
                let platform = self.platform.box_clone();
                let mut controller = ThrottleController::new(platform);
                controller.set_throttle(throttle_type, mcps);
                controller.start_calibration(self.total_instructions);
                self.throttle_controller = Some(controller);
                self.throttle.throttle_type = throttle_type;
            }
            ThrottleType::KiloCyclesPerSec(kcps) => {
                if kcps == 0 {
                    return Err(TimerError::ThrottleConfigError(
                        "KCPS must be greater than 0".to_string(),
                    ));
                }
                let platform = self.platform.box_clone();
                let mut controller = ThrottleController::new(platform);
                controller.set_throttle(throttle_type, kcps);
                controller.start_calibration(self.total_instructions);
                self.throttle_controller = Some(controller);
                self.throttle.throttle_type = throttle_type;
            }
            ThrottleType::Percent(pct) => {
                if pct == 0 || pct > 100 {
                    return Err(TimerError::ThrottleConfigError(
                        "Percent must be 1-100".to_string(),
                    ));
                }
                // TODO: Percent throttling needs peak CPS measurement
                self.throttle.throttle_type = throttle_type;
                self.throttle.throttle_value = pct;
            }
            ThrottleType::Specific {
                instructions,
                delay_ms,
            } => {
                if instructions == 0 {
                    return Err(TimerError::ThrottleConfigError(
                        "Instructions must be greater than 0".to_string(),
                    ));
                }
                if delay_ms == 0 {
                    return Err(TimerError::ThrottleConfigError(
                        "Delay must be greater than 0".to_string(),
                    ));
                }
                // Simple fixed throttling
                self.throttle.throttle_type = throttle_type;
                self.throttle.throttle_value = instructions;
                self.throttle_controller = None;
            }
        }

        self.throttle.reset();
        Ok(())
    }

    fn schedule_throttle(&mut self, instructions_executed: u32) {
        // Use advanced controller if available
        if let Some(ref mut controller) = self.throttle_controller {
            if let Some(sleep_ms) = controller.update(self.total_instructions) {
                self.platform.sleep_ms(sleep_ms);
            }
            return;
        }

        // Fallback: simple throttling for Specific type
        if let ThrottleType::Specific {
            instructions,
            delay_ms,
        } = self.throttle.throttle_type
        {
            if instructions_executed >= instructions {
                self.platform.sleep_ms(delay_ms);
            }
        }
    }

    fn cancel_throttle(&mut self) {
        self.throttle.throttle_type = ThrottleType::None;
        self.throttle.reset();
    }

    fn get_drift_percent(&self) -> u32 {
        self.throttle.drift_pct
    }

    fn set_drift_percent(&mut self, percent: u32) -> TimerResult<()> {
        if percent == 0 || percent > 100 {
            return Err(TimerError::InvalidParameter(
                "Drift percent must be 1-100".to_string(),
            ));
        }
        self.throttle.drift_pct = percent;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::platform::create_platform_timer;
    use super::*;

    #[test]
    fn test_manager_creation() {
        let platform = create_platform_timer();
        let manager = TimerManager::new(platform);

        assert!(!manager.services_running);
        assert_eq!(manager.total_instructions(), 0);
        assert!(!manager.has_initialized_timers());
    }

    #[test]
    fn test_timer_initialization() {
        let platform = create_platform_timer();
        let mut manager = TimerManager::new(platform);

        manager.init_timer(0, 60.0).unwrap();

        let rtc = manager.get_rtc(0).unwrap();
        assert!(rtc.is_initialized());
        assert_eq!(rtc.hz, 60.0);
    }

    #[test]
    fn test_invalid_timer_id() {
        let platform = create_platform_timer();
        let mut manager = TimerManager::new(platform);

        let result = manager.init_timer(999, 60.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_instructions_tracking() {
        let platform = create_platform_timer();
        let mut manager = TimerManager::new(platform);

        manager.add_instructions(1000);
        assert_eq!(manager.total_instructions(), 1000);

        manager.add_instructions(500);
        assert_eq!(manager.total_instructions(), 1500);
    }

    #[test]
    fn test_idle_manager() {
        let platform = create_platform_timer();
        let mut manager = TimerManager::new(platform);

        assert!(!manager.is_enabled());
        manager.set_enabled(true);
        assert!(manager.is_enabled());
    }

    #[test]
    fn test_throttle_validation() {
        let platform = create_platform_timer();
        let mut manager = TimerManager::new(platform);

        // Valid throttle
        assert!(manager.set_throttle(ThrottleType::MegaCyclesPerSec(1)).is_ok());

        // Invalid throttle (0 MCPS)
        assert!(manager.set_throttle(ThrottleType::MegaCyclesPerSec(0)).is_err());

        // Invalid percent (> 100)
        assert!(manager.set_throttle(ThrottleType::Percent(150)).is_err());
    }

    #[test]
    fn test_services_lifecycle() {
        let platform = create_platform_timer();
        let mut manager = TimerManager::new(platform);

        assert!(!manager.services_running);

        manager.start_services();
        assert!(manager.services_running);

        manager.stop_services();
        assert!(!manager.services_running);
    }

    #[test]
    fn test_clock_characteristics() {
        let platform = create_platform_timer();
        let manager = TimerManager::new(platform);

        let (min_ms, inc_ms, res_ms, hz) = manager.clock_characteristics();

        assert!(min_ms > 0);
        assert!(inc_ms > 0);
        assert!(res_ms > 0);
        assert!(hz > 0);
    }
}
