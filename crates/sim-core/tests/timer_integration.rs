//! Integration tests for the timer subsystem
//!
//! These tests verify that the timer module works correctly when integrated
//! with the rest of the sim-core crate.

use sim_core::timers::{
    create_platform_timer, IdleManager, ThrottleManager, ThrottleType, TimerError, TimerManager,
};
use std::time::Duration;

#[test]
fn test_timer_basic_workflow() {
    // Create timer manager
    let platform = create_platform_timer();
    let mut timer_mgr = TimerManager::new(platform);

    // Initialize a 60Hz timer
    timer_mgr.init_timer(0, 60.0).unwrap();

    // Verify initialization
    let rtc = timer_mgr.get_rtc(0).unwrap();
    assert!(rtc.is_initialized());
    assert_eq!(rtc.hz, 60.0);
    assert!(rtc.current_delay > 0);
}

#[test]
fn test_multiple_timers() {
    let platform = create_platform_timer();
    let mut timer_mgr = TimerManager::new(platform);

    // Initialize multiple timers at different frequencies
    timer_mgr.init_timer(0, 60.0).unwrap();
    timer_mgr.init_timer(1, 50.0).unwrap();
    timer_mgr.init_timer(2, 120.0).unwrap();

    // Verify all are initialized correctly
    let rtc0 = timer_mgr.get_rtc(0).unwrap();
    let rtc1 = timer_mgr.get_rtc(1).unwrap();
    let rtc2 = timer_mgr.get_rtc(2).unwrap();

    assert_eq!(rtc0.hz, 60.0);
    assert_eq!(rtc1.hz, 50.0);
    assert_eq!(rtc2.hz, 120.0);

    // Higher frequency should have fewer instructions per tick
    assert!(rtc2.current_delay < rtc0.current_delay);
    assert!(rtc0.current_delay < rtc1.current_delay);
}

#[test]
fn test_invalid_timer_id() {
    let platform = create_platform_timer();
    let mut timer_mgr = TimerManager::new(platform);

    // Try to initialize an invalid timer
    let result = timer_mgr.init_timer(100, 60.0);
    assert!(result.is_err());

    match result {
        Err(TimerError::InvalidTimerId(id)) => assert_eq!(id, 100),
        _ => panic!("Expected InvalidTimerId error"),
    }
}

#[test]
fn test_instruction_tracking() {
    let platform = create_platform_timer();
    let mut timer_mgr = TimerManager::new(platform);

    assert_eq!(timer_mgr.total_instructions(), 0);

    timer_mgr.add_instructions(1000);
    assert_eq!(timer_mgr.total_instructions(), 1000);

    timer_mgr.add_instructions(500);
    assert_eq!(timer_mgr.total_instructions(), 1500);
}

#[test]
fn test_idle_enable_disable() {
    let platform = create_platform_timer();
    let mut timer_mgr = TimerManager::new(platform);

    // Initially disabled
    assert!(!timer_mgr.is_enabled());

    // Enable idle
    timer_mgr.set_enabled(true);
    assert!(timer_mgr.is_enabled());

    // Disable idle
    timer_mgr.set_enabled(false);
    assert!(!timer_mgr.is_enabled());
}

#[test]
fn test_idle_stability_threshold() {
    let platform = create_platform_timer();
    let mut timer_mgr = TimerManager::new(platform);

    // Set valid threshold
    timer_mgr.set_stability_threshold(30).unwrap();
    assert_eq!(timer_mgr.get_stability_threshold(), 30);

    // Try invalid thresholds
    assert!(timer_mgr.set_stability_threshold(1).is_err());
    assert!(timer_mgr.set_stability_threshold(1000).is_err());
}

#[test]
fn test_throttle_types() {
    let platform = create_platform_timer();
    let mut timer_mgr = TimerManager::new(platform);

    // Test None
    timer_mgr.set_throttle(ThrottleType::None).unwrap();
    assert!(matches!(timer_mgr.get_throttle_type(), ThrottleType::None));

    // Test MegaCycles
    timer_mgr.set_throttle(ThrottleType::MegaCyclesPerSec(1)).unwrap();
    assert!(matches!(
        timer_mgr.get_throttle_type(),
        ThrottleType::MegaCyclesPerSec(1)
    ));

    // Test KiloCycles
    timer_mgr
        .set_throttle(ThrottleType::KiloCyclesPerSec(500))
        .unwrap();
    assert!(matches!(
        timer_mgr.get_throttle_type(),
        ThrottleType::KiloCyclesPerSec(500)
    ));

    // Test Percent
    timer_mgr.set_throttle(ThrottleType::Percent(50)).unwrap();
    assert!(matches!(timer_mgr.get_throttle_type(), ThrottleType::Percent(50)));

    // Test Specific
    timer_mgr
        .set_throttle(ThrottleType::Specific {
            instructions: 1000,
            delay_ms: 10,
        })
        .unwrap();
    assert!(matches!(
        timer_mgr.get_throttle_type(),
        ThrottleType::Specific { .. }
    ));
}

#[test]
fn test_throttle_validation() {
    let platform = create_platform_timer();
    let mut timer_mgr = TimerManager::new(platform);

    // Invalid MCPS (0)
    assert!(timer_mgr.set_throttle(ThrottleType::MegaCyclesPerSec(0)).is_err());

    // Invalid KCPS (0)
    assert!(timer_mgr.set_throttle(ThrottleType::KiloCyclesPerSec(0)).is_err());

    // Invalid Percent (0 and >100)
    assert!(timer_mgr.set_throttle(ThrottleType::Percent(0)).is_err());
    assert!(timer_mgr.set_throttle(ThrottleType::Percent(150)).is_err());

    // Invalid Specific (0 instructions or delay)
    assert!(timer_mgr
        .set_throttle(ThrottleType::Specific {
            instructions: 0,
            delay_ms: 10,
        })
        .is_err());

    assert!(timer_mgr
        .set_throttle(ThrottleType::Specific {
            instructions: 1000,
            delay_ms: 0,
        })
        .is_err());
}

#[test]
fn test_services_lifecycle() {
    let platform = create_platform_timer();
    let mut timer_mgr = TimerManager::new(platform);

    timer_mgr.start_services();
    // Services should be running

    timer_mgr.stop_services();
    // Services should be stopped

    // Can start again
    timer_mgr.start_services();
}

#[test]
fn test_platform_timer_monotonic() {
    let platform = create_platform_timer();

    let t1 = platform.get_msec();
    std::thread::sleep(Duration::from_millis(10));
    let t2 = platform.get_msec();

    // Time should advance
    assert!(t2 >= t1);
    assert!(t2 - t1 >= 10);
}

#[test]
fn test_platform_sleep() {
    let platform = create_platform_timer();

    let start = platform.get_msec();
    let actual_sleep = platform.sleep_ms(20);
    let end = platform.get_msec();

    let elapsed = end - start;

    // Should have slept at least the minimum time
    assert!(actual_sleep >= 20 || elapsed >= 20);
}

#[test]
fn test_clock_characteristics() {
    let platform = create_platform_timer();
    let timer_mgr = TimerManager::new(platform);

    let (min_ms, inc_ms, res_ms, hz) = timer_mgr.clock_characteristics();

    // All values should be positive
    assert!(min_ms > 0);
    assert!(inc_ms > 0);
    assert!(res_ms > 0);
    assert!(hz > 0);

    // Resolution should be reasonable (< 100ms for modern systems)
    assert!(res_ms < 100);
}

#[test]
fn test_timespec_operations() {
    let platform = create_platform_timer();
    let ts1 = platform.get_time_spec();
    std::thread::sleep(Duration::from_millis(10));
    let ts2 = platform.get_time_spec();

    // ts2 should be after ts1
    assert!(ts2.as_millis() > ts1.as_millis());

    // Difference should be reasonable
    let diff = ts2 - ts1;
    assert!(diff.as_millis() >= 10);
}

#[test]
fn test_re_exports() {
    // Verify that the main re-exports work
    let platform = sim_core::timers::create_platform_timer();
    let _timer_mgr = sim_core::timers::TimerManager::new(platform);

    // This tests that the re-exports in lib.rs work correctly
}

#[test]
fn test_host_speed_factor() {
    let platform = create_platform_timer();
    let timer_mgr = TimerManager::new(platform);

    let factor = timer_mgr.host_speed_factor();

    // Should be a reasonable positive number
    assert!(factor > 0.0);
    assert!(factor < 100.0); // Sanity check
}

#[test]
fn test_timer_count() {
    let platform = create_platform_timer();
    let mut timer_mgr = TimerManager::new(platform);

    assert_eq!(timer_mgr.initialized_timer_count(), 0);

    timer_mgr.init_timer(0, 60.0).unwrap();
    assert_eq!(timer_mgr.initialized_timer_count(), 1);

    timer_mgr.init_timer(2, 50.0).unwrap();
    assert_eq!(timer_mgr.initialized_timer_count(), 2);
}

#[test]
fn test_drift_percent() {
    let platform = create_platform_timer();
    let mut timer_mgr = TimerManager::new(platform);

    // Default value
    assert_eq!(timer_mgr.get_drift_percent(), 5);

    // Set valid value
    timer_mgr.set_drift_percent(10).unwrap();
    assert_eq!(timer_mgr.get_drift_percent(), 10);

    // Invalid values
    assert!(timer_mgr.set_drift_percent(0).is_err());
    assert!(timer_mgr.set_drift_percent(101).is_err());
}

#[test]
#[cfg(unix)]
fn test_platform_specific_unix() {
    // Unix-specific tests
    let platform = create_platform_timer();

    // Should have reasonable tick rate
    let hz = platform.get_tick_hz();
    assert!(hz >= 100); // Most systems are at least 100Hz
    assert!(hz <= 1000); // Usually not more than 1000Hz
}

#[test]
#[cfg(windows)]
fn test_platform_specific_windows() {
    // Windows-specific tests
    let platform = create_platform_timer();

    // Windows typically has 64Hz or 1000Hz
    let hz = platform.get_tick_hz();
    assert!(hz > 0);
}
