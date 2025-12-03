//! macOS platform timer implementation
//!
//! Uses mach_absolute_time for high-resolution timing on macOS.

use super::super::traits::{PlatformTimer, ThreadPriority, TimeSpec, TimerError, TimerResult};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(target_os = "macos")]
use mach2::mach_time::{mach_absolute_time, mach_timebase_info, mach_timebase_info_data_t};

/// macOS timer implementation
#[derive(Clone)]
pub struct MacOSTimer {
    /// Mach timebase for converting ticks to nanoseconds
    #[cfg(target_os = "macos")]
    timebase: mach_timebase_info_data_t,

    /// OS clock resolution in milliseconds
    clock_resolution_ms: u32,

    /// Minimum sleep granularity in milliseconds
    sleep_min_ms: u32,

    /// Sleep increment in milliseconds
    sleep_inc_ms: u32,

    /// System tick rate in Hz
    tick_hz: u32,

    /// Start time for relative measurements
    start_time: SystemTime,
}

impl MacOSTimer {
    /// Create a new macOS timer
    pub fn new() -> Self {
        #[cfg(target_os = "macos")]
        {
            let timebase = Self::get_timebase();
            let (clock_resolution_ms, tick_hz) = Self::detect_timing_characteristics();

            Self {
                timebase,
                clock_resolution_ms,
                sleep_min_ms: clock_resolution_ms,
                sleep_inc_ms: clock_resolution_ms,
                tick_hz,
                start_time: SystemTime::now(),
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            Self {
                clock_resolution_ms: 10,
                sleep_min_ms: 10,
                sleep_inc_ms: 10,
                tick_hz: 100,
                start_time: SystemTime::now(),
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn get_timebase() -> mach_timebase_info_data_t {
        unsafe {
            let mut info = mach_timebase_info_data_t { numer: 0, denom: 0 };
            mach_timebase_info(&mut info as *mut _);
            info
        }
    }

    #[cfg(target_os = "macos")]
    fn detect_timing_characteristics() -> (u32, u32) {
        // macOS typically has good timer resolution
        // Most modern Macs have ~1ms or better
        (1, 1000)
    }

    #[cfg(not(target_os = "macos"))]
    fn detect_timing_characteristics() -> (u32, u32) {
        (10, 100)
    }

    #[cfg(target_os = "macos")]
    fn mach_time_to_nanos(&self, ticks: u64) -> u64 {
        (ticks * self.timebase.numer as u64) / self.timebase.denom as u64
    }
}

impl Default for MacOSTimer {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformTimer for MacOSTimer {
    fn get_msec(&self) -> u32 {
        #[cfg(target_os = "macos")]
        {
            let ticks = unsafe { mach_absolute_time() };
            let nanos = self.mach_time_to_nanos(ticks);
            (nanos / 1_000_000) as u32
        }

        #[cfg(not(target_os = "macos"))]
        {
            let duration = SystemTime::now()
                .duration_since(self.start_time)
                .unwrap_or(Duration::from_secs(0));
            duration.as_millis() as u32
        }
    }

    fn get_time_spec(&self) -> TimeSpec {
        #[cfg(target_os = "macos")]
        {
            let ticks = unsafe { mach_absolute_time() };
            let nanos = self.mach_time_to_nanos(ticks);
            let secs = nanos / 1_000_000_000;
            let nsecs = nanos % 1_000_000_000;
            TimeSpec::new(secs as i64, nsecs as i64)
        }

        #[cfg(not(target_os = "macos"))]
        {
            let duration = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0));
            TimeSpec::new(duration.as_secs() as i64, duration.subsec_nanos() as i64)
        }
    }

    fn sleep_sec(&self, seconds: u32) {
        std::thread::sleep(Duration::from_secs(seconds as u64));
    }

    fn sleep_ms(&self, milliseconds: u32) -> u32 {
        let start = self.get_msec();
        std::thread::sleep(Duration::from_millis(milliseconds as u64));
        let end = self.get_msec();
        end.saturating_sub(start)
    }

    fn get_sleep_min_ms(&self) -> u32 {
        self.sleep_min_ms
    }

    fn get_sleep_inc_ms(&self) -> u32 {
        self.sleep_inc_ms
    }

    fn get_clock_resolution_ms(&self) -> u32 {
        self.clock_resolution_ms
    }

    fn get_tick_hz(&self) -> u32 {
        self.tick_hz
    }

    fn set_thread_priority(&self, _priority: ThreadPriority) -> TimerResult<()> {
        // Thread priority on macOS requires different APIs
        // Not implemented for now
        Err(TimerError::NotSupported(
            "Thread priority not supported on macOS yet".to_string(),
        ))
    }

    fn idle_capable(&self) -> Option<(u32, u32)> {
        Some((self.sleep_min_ms, self.clock_resolution_ms))
    }

    fn init_ms_sleep(&self) -> u32 {
        self.sleep_min_ms
    }

    fn box_clone(&self) -> Box<dyn PlatformTimer> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_macos_timer_creation() {
        let timer = MacOSTimer::new();
        assert!(timer.tick_hz > 0);
        assert!(timer.clock_resolution_ms > 0);
    }

    #[test]
    fn test_macos_timer_monotonic() {
        let timer = MacOSTimer::new();
        let t1 = timer.get_msec();
        std::thread::sleep(Duration::from_millis(5));
        let t2 = timer.get_msec();
        assert!(t2 >= t1);
    }
}
