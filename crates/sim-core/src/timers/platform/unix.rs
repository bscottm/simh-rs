//! Unix/Linux platform timer implementation

use super::super::traits::{PlatformTimer, ThreadPriority, TimeSpec, TimerError, TimerResult};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(target_os = "linux")]
use nix::sys::sysinfo::sysinfo;
#[cfg(target_os = "linux")]
use nix::time::{clock_gettime, ClockId};
#[cfg(target_os = "linux")]
use nix::unistd::sysconf;

/// Unix/Linux timer implementation
#[derive(Clone)]
pub struct UnixTimer {
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

impl UnixTimer {
    /// Create a new Unix timer
    pub fn new() -> Self {
        let (clock_resolution_ms, sleep_min_ms, sleep_inc_ms, tick_hz) =
            Self::detect_timing_characteristics();

        Self {
            clock_resolution_ms,
            sleep_min_ms,
            sleep_inc_ms,
            tick_hz,
            start_time: SystemTime::now(),
        }
    }

    /// Detect OS timing characteristics
    fn detect_timing_characteristics() -> (u32, u32, u32, u32) {
        #[cfg(target_os = "linux")]
        {
            // Get clock tick rate
            let tick_hz = match sysconf(nix::unistd::SysconfVar::CLK_TCK) {
                Ok(Some(hz)) => hz as u32,
                _ => 100, // Default to 100Hz if detection fails
            };

            // Calculate timing parameters based on tick rate
            let clock_resolution_ms = if tick_hz > 0 {
                1000 / tick_hz
            } else {
                10 // Default to 10ms
            };

            let sleep_min_ms = clock_resolution_ms;
            let sleep_inc_ms = clock_resolution_ms;

            (clock_resolution_ms, sleep_min_ms, sleep_inc_ms, tick_hz)
        }

        #[cfg(not(target_os = "linux"))]
        {
            // Default values for other Unix systems
            (10, 10, 10, 100)
        }
    }

    /// Get high-resolution time using clock_gettime
    #[cfg(target_os = "linux")]
    fn get_clock_time() -> TimeSpec {
        match clock_gettime(ClockId::CLOCK_MONOTONIC) {
            Ok(ts) => TimeSpec::new(ts.tv_sec(), ts.tv_nsec()),
            Err(_) => {
                // Fallback to system time
                let duration = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or(Duration::from_secs(0));
                TimeSpec::new(duration.as_secs() as i64, duration.subsec_nanos() as i64)
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn get_clock_time() -> TimeSpec {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0));
        TimeSpec::new(duration.as_secs() as i64, duration.subsec_nanos() as i64)
    }
}

impl Default for UnixTimer {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformTimer for UnixTimer {
    fn get_msec(&self) -> u32 {
        let duration = SystemTime::now()
            .duration_since(self.start_time)
            .unwrap_or(Duration::from_secs(0));
        duration.as_millis() as u32
    }

    fn get_time_spec(&self) -> TimeSpec {
        Self::get_clock_time()
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
        #[cfg(target_os = "linux")]
        unsafe {
            use nix::libc::{sched_param, sched_setscheduler, SCHED_BATCH, SCHED_IDLE, SCHED_OTHER};

            let policy = match priority {
                ThreadPriority::BelowNormal => SCHED_IDLE,
                ThreadPriority::Normal => SCHED_OTHER,
                ThreadPriority::AboveNormal => SCHED_BATCH,
            };

            let param = sched_param { sched_priority: 0 };

            // pid 0 refers to the calling thread
            if sched_setscheduler(0, policy, &param) == 0 {
                Ok(())
            } else {
                let err = std::io::Error::last_os_error();
                Err(TimerError::PlatformError(format!(
                    "sched_setscheduler failed: {}",
                    err
                )))
            }
        }

        #[cfg(not(target_os = "linux"))]
        { /* fallback */ }
    }

    fn idle_capable(&self) -> Option<(u32, u32)> {
        // Unix systems generally support idle well
        Some((self.sleep_min_ms, self.clock_resolution_ms))
    }

    fn init_ms_sleep(&self) -> u32 {
        // No special initialization needed on Unix
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
    fn test_unix_timer_creation() {
        let timer = UnixTimer::new();
        assert!(timer.tick_hz > 0);
        assert!(timer.clock_resolution_ms > 0);
    }

    #[test]
    fn test_unix_timer_monotonic() {
        let timer = UnixTimer::new();
        let t1 = timer.get_msec();
        std::thread::sleep(Duration::from_millis(5));
        let t2 = timer.get_msec();
        assert!(t2 >= t1);
    }

    #[test]
    fn test_unix_timespec() {
        let timer = UnixTimer::new();
        let ts1 = timer.get_time_spec();
        std::thread::sleep(Duration::from_millis(5));
        let ts2 = timer.get_time_spec();

        assert!(ts2.as_millis() > ts1.as_millis());
    }

    #[test]
    fn test_unix_sleep() {
        let timer = UnixTimer::new();
        let start = timer.get_msec();
        let actual = timer.sleep_ms(10);
        let end = timer.get_msec();

        assert!(actual >= 10);
        assert!(end >= start + 10);
    }
}
