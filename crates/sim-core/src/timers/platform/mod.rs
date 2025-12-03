//! Platform-specific timer implementations
//!
//! This module provides concrete implementations of the PlatformTimer trait
//! for different operating systems (Unix, Windows, macOS).

use super::traits::PlatformTimer;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(all(unix, not(target_os = "macos")))]
mod unix;
#[cfg(windows)]
mod windows;

/// Create a platform-specific timer implementation
///
/// This function returns the appropriate timer implementation for the current platform.
pub fn create_platform_timer() -> Box<dyn PlatformTimer> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacOSTimer::new())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Box::new(unix::UnixTimer::new())
    }
    #[cfg(windows)]
    {
        Box::new(windows::WindowsTimer::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_platform_timer() {
        let timer = create_platform_timer();

        // Basic functionality test
        let t1 = timer.get_msec();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let t2 = timer.get_msec();

        assert!(t2 >= t1);
    }

    #[test]
    fn test_timespec_operations() {
        let timer = create_platform_timer();
        let ts = timer.get_time_spec();

        assert!(ts.tv_sec > 0 || ts.tv_nsec > 0);
        assert!(ts.as_millis() > 0);
    }

    #[test]
    fn test_sleep_accuracy() {
        let timer = create_platform_timer();
        let min_sleep = timer.get_sleep_min_ms();

        let start = timer.get_msec();
        let actual = timer.sleep_ms(min_sleep);
        let end = timer.get_msec();

        let elapsed = end - start;

        // Sleep should be at least the minimum
        assert!(actual >= min_sleep);
        assert!(elapsed >= min_sleep);
    }
}
