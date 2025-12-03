// SPDX-License-Identifier: MIT

//! Windows platform timer implementation

use super::super::traits::{PlatformTimer, ThreadPriority, TimeSpec, TimerError, TimerResult};
use std::time::{Duration, SystemTime};

use winapi::shared::minwindef::FILETIME;
use winapi::um::processthreadsapi::{GetCurrentThread, SetThreadPriority};
use winapi::um::sysinfoapi::GetSystemTimePreciseAsFileTime;
use winapi::um::timeapi::{timeBeginPeriod, timeEndPeriod};
use winapi::um::winbase::{
    THREAD_PRIORITY_ABOVE_NORMAL, THREAD_PRIORITY_BELOW_NORMAL, THREAD_PRIORITY_NORMAL,
};

/// Windows timer implementation
#[derive(Clone)]
pub struct WindowsTimer {
    /// OS clock resolution in milliseconds
    clock_resolution_ms: u32,

    /// Minimum sleep granularity in milliseconds
    sleep_min_ms: u32,

    /// Sleep increment in milliseconds
    sleep_inc_ms: u32,

    /// System tick rate in Hz
    tick_hz: u32,

    /// Whether high-resolution timer was initialized
    high_res_initialized: bool,

    /// Start time for relative measurements
    start_time: SystemTime,
}

impl WindowsTimer {
    /// Create a new Windows timer
    pub fn new() -> Self {
        let (clock_resolution_ms, tick_hz) = Self::detect_timing_characteristics();

        Self {
            clock_resolution_ms,
            sleep_min_ms: clock_resolution_ms,
            sleep_inc_ms: clock_resolution_ms,
            tick_hz,
            high_res_initialized: false,
            start_time: SystemTime::now(),
        }
    }

    fn detect_timing_characteristics() -> (u32, u32) {
        // Windows typically has 15.6ms (64Hz) or better resolution
        // Modern Windows (10+) often has 1ms resolution

        // Try to detect the actual resolution by measuring GetTickCount precision
        let resolution_ms = 15; // Conservative default
        let tick_hz = 1000 / resolution_ms;

        (resolution_ms, tick_hz)
    }

    fn filetime_to_timespec(ft: &FILETIME) -> TimeSpec {
        // Combine the two 32-bit parts into a 64-bit value
        let ticks = ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64);

        // Windows epoch (1601-01-01) to Unix epoch (1970-01-01) is 11644473600 seconds
        const WINDOWS_TO_UNIX_EPOCH_SECS: u64 = 11644473600;
        const TICKS_PER_SECOND: u64 = 10_000_000; // 100ns ticks

        // Convert ticks to seconds and nanoseconds
        let total_secs = ticks / TICKS_PER_SECOND;
        let remaining_ticks = ticks % TICKS_PER_SECOND;

        // Adjust from Windows epoch to Unix epoch
        let unix_secs = total_secs.saturating_sub(WINDOWS_TO_UNIX_EPOCH_SECS);
        let nsecs = remaining_ticks * 100; // Convert 100ns ticks to nanoseconds

        TimeSpec::new(unix_secs as i64, nsecs as i64)
    }
}

impl Default for WindowsTimer {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformTimer for WindowsTimer {
    fn get_msec(&self) -> u32 {
        let ts = self.get_time_spec();
        let total_ms = (ts.tv_sec as u64 * 1000) + (ts.tv_nsec as u64 / 1_000_000);
        total_ms as u32
    }

    fn get_time_spec(&self) -> TimeSpec {
        unsafe {
            let mut ft: FILETIME = std::mem::zeroed();
            GetSystemTimePreciseAsFileTime(&mut ft);
            Self::filetime_to_timespec(&ft)
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

    fn set_thread_priority(&self, priority: ThreadPriority) -> TimerResult<()> {
        unsafe {
            let priority_value: i32 = match priority {
                ThreadPriority::BelowNormal => THREAD_PRIORITY_BELOW_NORMAL as i32,
                ThreadPriority::Normal => THREAD_PRIORITY_NORMAL as i32,
                ThreadPriority::AboveNormal => THREAD_PRIORITY_ABOVE_NORMAL as i32,
            };

            let handle = GetCurrentThread();
            if SetThreadPriority(handle, priority_value) != 0 {
                Ok(())
            } else {
                Err(TimerError::PlatformError(
                    "Failed to set thread priority".to_string(),
                ))
            }
        }
    }

    fn idle_capable(&self) -> Option<(u32, u32)> {
        // Windows supports idle well
        Some((self.sleep_min_ms, self.clock_resolution_ms))
    }

    fn init_ms_sleep(&self) -> u32 {
        // Request 1ms timer resolution on Windows
        // This improves Sleep() accuracy significantly
        unsafe {
            if timeBeginPeriod(1) == 0 {
                // Successfully set to 1ms
                return 1;
            }
        }

        self.sleep_min_ms
    }

    fn box_clone(&self) -> Box<dyn PlatformTimer> {
        Box::new(self.clone())
    }
}

impl Drop for WindowsTimer {
    fn drop(&mut self) {
        if self.high_res_initialized {
            unsafe {
                timeEndPeriod(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows_timer_creation() {
        let timer = WindowsTimer::new();
        assert!(timer.tick_hz > 0);
        assert!(timer.clock_resolution_ms > 0);
    }

    #[test]
    fn test_windows_timer_monotonic() {
        let timer = WindowsTimer::new();
        let t1 = timer.get_msec();
        std::thread::sleep(Duration::from_millis(5));
        let t2 = timer.get_msec();
        assert!(t2 >= t1);
    }

    #[test]
    fn test_windows_sleep() {
        let timer = WindowsTimer::new();
        let start = timer.get_msec();
        let actual = timer.sleep_ms(10);
        let end = timer.get_msec();

        assert!(actual >= 10);
        assert!(end >= start + 10);
    }
}
