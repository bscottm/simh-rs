// SPDX-License-Identifier: MIT

//! Runtime-extensible debug category system and output macros.
//!
//! # Categories
//!
//! Devices and subsystems register named [`DebugCategory`] tokens with [`DebugRegistry`] at startup. The
//! global registry is accessed via [`debug_registry`]. Category IDs are `u32` for fast [`FxHashSet`] lookup
//! on the hot path.
//!
//! # Macros

//! Two macros provide category-gated debug output. Both take a [`crate::logging::SharedDebugState`] handle
//! which owns the sink, format flags and CPU snapshot.
//!
//! ## `sim_debug!`
//!
//! For simulator and device code. Format is controlled by [`crate::logging::DebugFormatFlags`]:
//!
//! ```text
//! [0001234567] DEBUG PC=017652 #42 ETH_RX: message   (-P -I)
//! [0001234567] DEBUG PC=017652 ETH_RX: message       (-P)
//! [#0000000042] DEBUG ETH_RX: message                (-I)
//! [0001234567] DEBUG ETH_RX: message                 (default)
//! ```
//!
//! ## `cli_debug!`
//!
//! For CLI code. Always uses wall-clock timestamp; never includes PC or instruction count (the CLI is idle
//! while the simulator runs).
//!
//! ```text
//! [0001234567] DEBUG SET: Opening debug log: foo.log
//! ```
//!
//! Both macros are zero-cost when the category is disabled: `is_enabled` is the only work performed before
//! the short-circuit.
//!
//! # Sink sharing
//!
//! The macros take `&Option<SharedDebugState>`. `None` exits after the category check.  When `SET DEBUG LOG`
//! is active, [`crate::logging::DebugState`] routes writes through
//! [`crate::logging::TranscriptSink::write_debug_with_flush`], flushing any partial transcript line before
//! the debug record.

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{OnceLock, RwLock};

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Global registry
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// Global debug registry. Lazily initialised on first access.
pub static DEBUG: OnceLock<DebugRegistry> = OnceLock::new();

/// Get the global debug registry, initialising it if necessary.
#[inline]
pub fn debug_registry() -> &'static DebugRegistry {
    DEBUG.get_or_init(DebugRegistry::new)
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// DebugCategory
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// Opaque debug category identifier.
///
/// IDs 0–99 are reserved for core timer system categories (see [`core`]).
/// Device-specific categories start at 100 and are allocated by [`DebugRegistry::register`].
/// Values are `u32` so [`FxHashSet`] can hash them in 1–2 cycles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DebugCategory(u32);

impl DebugCategory {
    /// Create a category with a fixed reserved ID (core module use only).
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Get the raw category ID.
    pub const fn id(&self) -> u32 {
        self.0
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// DebugRegistry
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// Runtime-extensible registry of named debug categories.
///
/// `is_enabled` is the hot path and uses `RwLock::read()` for concurrent access
/// across multiple device threads. Write operations (enable/disable) only occur
/// on CLI commands, which are rare and serialised.
pub struct DebugRegistry {
    /// Currently enabled categories.
    active: RwLock<FxHashSet<u32>>,
    /// ID → display name (for `SHOW DEBUG`).
    names: RwLock<FxHashMap<u32, &'static str>>,
    /// Uppercase name → ID (for CLI `enable_by_name`).
    name_to_id: RwLock<FxHashMap<String, u32>>,
    /// Auto-allocation counter; starts at 100.
    next_id: AtomicU32,
}

impl DebugRegistry {
    /// Create a new, empty registry.
    pub fn new() -> Self {
        Self {
            active: RwLock::new(FxHashSet::default()),
            names: RwLock::new(FxHashMap::default()),
            name_to_id: RwLock::new(FxHashMap::default()),
            next_id: AtomicU32::new(100),
        }
    }

    /// Register a device debug category and return its [`DebugCategory`] token.
    ///
    /// Names should be unique and descriptive (e.g. `"ETH_RX"`, `"DISK_SEEK"`).
    /// Lookup by name is case-insensitive.
    pub fn register(&self, name: &'static str) -> DebugCategory {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.names.write().unwrap().insert(id, name);
        self.name_to_id.write().unwrap().insert(name.to_uppercase(), id);
        DebugCategory(id)
    }

    /// Register a core timer category with a reserved ID (must be < 100).
    pub fn register_core(&self, id: u32, name: &'static str) -> DebugCategory {
        assert!(id < 100, "Core debug IDs must be < 100");
        self.names.write().unwrap().insert(id, name);
        self.name_to_id.write().unwrap().insert(name.to_uppercase(), id);
        DebugCategory(id)
    }

    /// Check if a category is enabled.
    ///
    /// Hot path. Concurrent reads via `RwLock::read()`. Typical uncontended cost:
    /// ~12–15 cycles (FxHash on u32 + lock acquire + HashSet lookup).
    #[inline]
    pub fn is_enabled(&self, category: DebugCategory) -> bool {
        self.active.read().unwrap().contains(&category.0)
    }

    /// Enable a category by token.
    pub fn enable(&self, category: DebugCategory) {
        self.active.write().unwrap().insert(category.0);
    }

    /// Disable a category by token.
    pub fn disable(&self, category: DebugCategory) {
        self.active.write().unwrap().remove(&category.0);
    }

    /// Check if a category is enabled by name (case-insensitive).
    ///
    /// Returns `false` if the name is not registered rather than an error.  Used by
    /// [`crate::env::SimEnvironment::resource_manifest`] to populate live enabled state in
    /// [`crate::env::ResourceCLIMetadata::debug_categories`].
    pub fn is_enabled_by_name(&self, name: &str) -> bool {
        match self.id_by_name(name) {
            Ok(id) => self.active.read().unwrap().contains(&id),
            Err(_) => false,
        }
    }

    /// Enable a category by name (case-insensitive). Used by CLI commands.
    pub fn enable_by_name(&self, name: &str) -> Result<(), String> {
        let id = self.id_by_name(name)?;
        self.active.write().unwrap().insert(id);
        Ok(())
    }

    /// Disable a category by name (case-insensitive). Used by CLI commands.
    pub fn disable_by_name(&self, name: &str) -> Result<(), String> {
        let id = self.id_by_name(name)?;
        self.active.write().unwrap().remove(&id);
        Ok(())
    }

    /// Enable all registered categories.
    pub fn enable_all(&self) {
        let ids: Vec<u32> = self.names.read().unwrap().keys().copied().collect();
        self.active.write().unwrap().extend(ids);
    }

    /// Disable all categories.
    pub fn disable_all(&self) {
        self.active.write().unwrap().clear();
    }

    /// Get the display name for a category (for `SHOW DEBUG`).
    pub fn get_name(&self, category: DebugCategory) -> Option<&'static str> {
        self.names.read().unwrap().get(&category.0).copied()
    }

    /// List all registered categories as `(id, name, is_enabled)`, sorted by ID.
    pub fn list_categories(&self) -> Vec<(u32, &'static str, bool)> {
        let names = self.names.read().unwrap();
        let active = self.active.read().unwrap();
        let mut list: Vec<_> = names
            .iter()
            .map(|(id, name)| (*id, *name, active.contains(id)))
            .collect();
        list.sort_by_key(|(id, _, _)| *id);
        list
    }

    fn id_by_name(&self, name: &str) -> Result<u32, String> {
        self.name_to_id
            .read()
            .unwrap()
            .get(&name.to_uppercase())
            .copied()
            .ok_or_else(|| format!("Unknown debug category: {}", name))
    }
}

impl Default for DebugRegistry {
    fn default() -> Self {
        Self::new()
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// sim_debug! — simulator / device debug macro
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// Emit a debug record from simulator or device code.
///
/// Zero-cost if `$category` is disabled or `$debug_state` is `None`.
///
/// Format is controlled by [`crate::logging::DebugFormatFlags`] in the active debug session:
/// - Default: `[0001234567] DEBUG target: message`
/// - `-P`: `[0001234567] DEBUG PC=017652 target: message`
/// - `-I`: `[#0000000042] DEBUG target: message`
/// - `-P -I`: `[0001234567] DEBUG PC=017652 #42 target: message`
///
/// When `SET DEBUG LOG` is active, the partial transcript line is flushed
/// and a cross-reference marker is written before the debug record.
///
/// # Arguments
/// - `$category`    — [`DebugCategory`] token from `debug_registry().register()`
/// - `$debug_state` — `&Option<SharedDebugState>` from `SimEnvironment`
/// - `$target`      — subsystem label, e.g. `"ETH_RX"`, `"DISK"`
/// - `$($arg)*`     — `format!`-style message
///
/// # Example
/// ```ignore
/// sim_debug!(self.dbg_rx, &env.debug_state, "ETH_RX",
///            "Received {} bytes from {}", len, src);
/// ```
#[macro_export]
macro_rules! sim_debug {
    ($category:expr, $debug_state:expr, $target:expr, $($arg:tt)*) => {
        if $crate::logging::debug_registry().is_enabled($category) {
            if let Some(state_arc) = ($debug_state).as_ref() {
                let state = state_arc.lock().unwrap();
                state.write_sim($target, &format!($($arg)*));
            }
        }
    };
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// cli_debug! — CLI debug macro
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// Emit a debug record from CLI code.
///
/// Identical call signature to [`sim_debug!`] but delegates to [`crate::logging::DebugState::write_cli`],
/// which always uses a wall-clock timestamp and never includes PC or instruction count.
///
/// # Arguments
/// - `$category`    — [`DebugCategory`] token from `debug_registry().register()`
/// - `$debug_state` — `&Option<SharedDebugState>` from `REPLState`
/// - `$target`      — CLI subsystem label, e.g. `"SET"`, `"EXAMINE"`
/// - `$($arg)*`     — `format!`-style message
///
/// # Example
/// ```ignore
/// cli_debug!(SET_CAT, &context.state.debug_state, "SET",
///            "Opening debug log: {}", filename);
/// ```
#[macro_export]
macro_rules! cli_debug {
    ($category:expr, $debug_state:expr, $target:expr, $($arg:tt)*) => {
        if $crate::logging::debug_registry().is_enabled($category) {
            if let Some(state_arc) = ($debug_state).as_ref() {
                let state = state_arc.lock().unwrap();
                state.write_cli($target, &format!($($arg)*));
            }
        }
    };
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Core timer debug categories (IDs 0–99)
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// Reserved debug categories for the core timer subsystem.
pub mod core {
    use super::DebugCategory;
    pub const CALIBRATION: DebugCategory = DebugCategory::new(1);
    pub const THROTTLE: DebugCategory = DebugCategory::new(2);
    pub const IDLE: DebugCategory = DebugCategory::new(3);
    pub const EVENTS: DebugCategory = DebugCategory::new(4);
    pub const PLATFORM: DebugCategory = DebugCategory::new(5);
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Tests
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_enable_disable() {
        let reg = DebugRegistry::new();
        let cat = reg.register("TEST_CAT");
        assert!(!reg.is_enabled(cat));
        reg.enable(cat);
        assert!(reg.is_enabled(cat));
        reg.disable(cat);
        assert!(!reg.is_enabled(cat));
    }

    #[test]
    fn test_enable_by_name_case_insensitive() {
        let reg = DebugRegistry::new();
        let cat = reg.register("ETH_RX");
        reg.enable_by_name("eth_rx").unwrap();
        assert!(reg.is_enabled(cat));
        reg.disable_by_name("ETH_RX").unwrap();
        assert!(!reg.is_enabled(cat));
    }

    #[test]
    fn test_list_categories_sorted() {
        let reg = DebugRegistry::new();
        let a = reg.register("CAT_A");
        let _b = reg.register("CAT_B");
        reg.enable(a);
        let list = reg.list_categories();
        assert_eq!(list.len(), 2);
        assert!(list.iter().find(|(_, n, _)| *n == "CAT_A").unwrap().2);
        assert!(!list.iter().find(|(_, n, _)| *n == "CAT_B").unwrap().2);
        // Sorted by ID
        assert!(list[0].0 < list[1].0);
    }

    #[test]
    fn test_core_registration() {
        let reg = DebugRegistry::new();
        reg.register_core(core::CALIBRATION.id(), "CALIBRATION");
        reg.enable(core::CALIBRATION);
        assert!(reg.is_enabled(core::CALIBRATION));
    }

    #[test]
    fn test_unknown_category_error() {
        let reg = DebugRegistry::new();
        assert!(reg.enable_by_name("DOES_NOT_EXIST").is_err());
    }

    #[test]
    fn test_enable_all_disable_all() {
        let reg = DebugRegistry::new();
        let a = reg.register("A");
        let b = reg.register("B");
        reg.enable_all();
        assert!(reg.is_enabled(a));
        assert!(reg.is_enabled(b));
        reg.disable_all();
        assert!(!reg.is_enabled(a));
        assert!(!reg.is_enabled(b));
    }
}
