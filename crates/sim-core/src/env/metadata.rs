// SPDX-License-Identifier: MIT

//! Device metadata — the simulator's internal registry entry for a device.
//!
//! # Overview
//!
//! [`DeviceMetadata`] is the value type stored in [`crate::env::SimEnvironment::devices`]. Each entry in the
//! device registry is a `DeviceMetadata`, consolidating everything the simulator needs to know about a
//! registered device:
//!
//! - **Structure**: a [`DeviceNode`] describing whether the entry is a standalone device or a controller with
//!     subordinate units (each held in a [`crate::env::UnitMetadata`]).
//!
//! - **State**: an `enabled` flag controlling whether the device participates actively in the simulation.
//!
//! - **Handlers**: `service` and `reset` function pointers extracted from the device at registration
//!   time. Keeping these here ensures all per-device data travels together.
//!
//! - **Debug categories**: names of debug categories registered by this device, captured at registration
//!   time. Enabled state is queried live from the registry when building the CLI manifest.
//!
//! # Naming
//!
//! `DeviceMetadata` is the simulator's *internal* record. The *CLI-facing* DTO is
//! [`crate::env::ResourceCLIMetadata`], which is a snapshot derived from `DeviceMetadata` at
//! manifest-generation time and sent over the channel to the CLI.
//!
//! Units inside a controller follow the same pattern via [`crate::env::UnitMetadata`], which carries its own
//! `enabled`, `service`, `reset`, and `debug_category_names` fields.
//!
//! # Enabled vs. Disabled
//!
//! Disabling a device (via `SET <device> DISABLED`) sets `enabled` to `false`. The device
//! remains fully registered — its name is visible in `SHOW DEVICES`, its resources can
//! still be examined and deposited, and its configuration is preserved. Its service
//! handler is not called and it does not participate in I/O or interrupt processing.
//! Re-enabling restores full participation without re-registration or loss of state.
//!
//! This mirrors C SIMH's `SET <device> DISABLED` / `SET <device> ENABLED` behaviour.

use crate::env::{
    device_node::DeviceNode,
    machine::{CPUTraits, DeviceTraits},
    simerror::SimError,
    sysbus::SystemBus,
};

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// DeviceMetadata
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// The simulator's internal registry entry for a device or controller.
///
/// Stored in [crate::env::`SimEnvironment::devices`] keyed by device name. Bundles the device's structural
/// node, operational state, event handlers, and debug category names into a single value so all per-device
/// data travels together.
///
/// For the CLI-facing equivalent, see [`crate::env::ResourceCLIMetadata`].
pub struct DeviceMetadata<CPU: CPUTraits> {
    /// The device or controller node, including any subordinate [`crate::env::UnitMetadata`]-s.
    pub dev_node: DeviceNode<CPU>,
    /// Whether this device is active in the simulation.
    ///
    /// `false` corresponds to `SET <device> DISABLED`. Service handlers are skipped
    /// for disabled devices; all other namespace operations still work normally.
    pub enabled: bool,
    /// Names of debug categories registered by this device.
    ///
    /// Captured at registration time via [`crate::env::DeviceTraits::debug_categories`]. Enabled state is
    /// not stored here — it is queried live from the global [`crate::logging::DebugRegistry`] when
    /// [`crate::env::SimEnvironment::resource_manifest`] builds the [`crate::env::ResourceCLIMetadata`]
    /// snapshot for the CLI.
    pub debug_category_names: Vec<String>,
}

impl<CPU: CPUTraits> DeviceMetadata<CPU> {
    /// Convenience accessor — shared reference to the device or controller's trait object.
    pub fn device(&self) -> &dyn DeviceTraits<CPU> {
        self.dev_node.device()
    }

    /// Convenience accessor — mutable reference to the device or controller's trait object.
    pub fn device_mut(&mut self) -> &mut (dyn DeviceTraits<CPU> + Send) {
        self.dev_node.device_mut()
    }

    /// Convenience accessor — shared reference to the device or controller's structural node.
    pub fn node(&self) -> &DeviceNode<CPU> {
        &self.dev_node
    }

    /// Convenience accessor — mutable reference to the device or controller's structural node.
    pub fn node_mut(&mut self) -> &mut DeviceNode<CPU> {
        &mut self.dev_node
    }

    /// Convenience method to invoke the reset() handler on this device or controller.
    pub fn device_reset(&mut self) {
        if self.enabled {
            self.dev_node.device_reset();

            if let DeviceNode::Controller(ctlr) = &mut self.dev_node {
                for unit_meta in &mut ctlr.units {
                    unit_meta.device_reset();
                }
            }
        }
    }

    /// Convenience method to invoke the service() handler on this device or controller.
    pub fn device_service(&mut self, cpu: &mut CPU, sysbus: &mut SystemBus) -> Result<(), SimError> {
        if self.enabled {
            self.dev_node.device_service(cpu, sysbus)?
        }

        Ok(())
    }
}

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
