// SPDX-License-Identifier: MIT

//! Device namespace types
//!
//! Provides the internal representation of devices, controllers, and units
//! within the simulator's device registry.

use crate::env::machine::{CPUTraits, DeviceTraits};

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// UnitMetadata
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// The simulator's internal registry entry for a unit within a controller.
///
/// Mirrors [`crate::env::DeviceMetadata`] at the unit level. Bundles the unit's trait object, operational
/// state, event handlers, and debug category names together so that all per-unit data travels as a single
/// value inside [`crate::env::Controller::units`].
///
/// For the CLI-facing equivalent, see [`crate::env::ResourceCLIMetadata`].
pub struct UnitMetadata<CPU: CPUTraits> {
    /// The unit's trait object.
    pub unit: Box<dyn DeviceTraits<CPU> + Send>,
    /// Whether this unit is active.
    ///
    /// `false` corresponds to `SET <unit> DISABLED`. The unit remains registered and its state is preserved;
    /// its service handler is simply not called.
    pub enabled: bool,
    /// Names of debug categories registered by this unit.
    ///
    /// Captured at registration time via [`DeviceTraits::debug_categories`].  Enabled state is queried live
    /// from the global [`crate::logging::DebugRegistry`] at manifest-generation time.
    pub debug_category_names: Vec<String>,
}

impl<CPU: CPUTraits> UnitMetadata<CPU> {
    /// Convenience accessor — shared reference to the unit's trait object.
    pub fn device(&self) -> &dyn DeviceTraits<CPU> {
        self.unit.as_ref()
    }

    /// Convenience accessor — mutable reference to the unit's trait object.
    pub fn device_mut(&mut self) -> &mut (dyn DeviceTraits<CPU> + Send) {
        self.unit.as_mut()
    }

    /// Convenience method to invoke the reset() handler on this unit.
    pub fn device_reset(&mut self) {
        if self.enabled {
            self.unit.device_reset();
        }
    }
}
