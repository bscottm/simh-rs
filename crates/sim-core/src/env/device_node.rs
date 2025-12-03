// SPDX-License-Identifier: MIT

//! Device namespace types
//!
//! Provides the internal representation of devices, controllers, and units
//! within the simulator's device registry.

use crate::env::{
    machine::{CPUTraits, DeviceTraits},
    simerror::SimError,
    sysbus::SystemBus,
    unitenv::UnitMetadata,
};

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// StandaloneDevice, Controller and DeviceNode:
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// A standalone device with no subordinate units.
pub struct StandaloneDevice<CPU: CPUTraits> {
    pub device: Box<dyn DeviceTraits<CPU> + Send>,
}

/// A controller with one or more subordinate units.
///
/// Each unit is stored as a [`UnitMetadata`] rather than a bare trait object, so enabled state and handlers
/// travel with the unit rather than living in separate maps on [`crate::env::SimEnvironment`].
pub struct Controller<CPU: CPUTraits> {
    pub controller: Box<dyn DeviceTraits<CPU> + Send>,
    pub units: Vec<UnitMetadata<CPU>>,
}

/// A node in the device namespace — either a standalone device or a controller with units.
pub enum DeviceNode<CPU: CPUTraits> {
    Standalone(StandaloneDevice<CPU>),
    Controller(Controller<CPU>),
}

impl<CPU: CPUTraits> DeviceNode<CPU> {
    /// Shared reference to the device or controller's trait object.
    pub fn device(&self) -> &dyn DeviceTraits<CPU> {
        match self {
            DeviceNode::Standalone(s) => s.device.as_ref(),
            DeviceNode::Controller(c) => c.controller.as_ref(),
        }
    }

    /// Mutable reference to the device or controller's trait object.
    pub fn device_mut(&mut self) -> &mut (dyn DeviceTraits<CPU> + Send) {
        match self {
            DeviceNode::Standalone(s) => s.device.as_mut(),
            DeviceNode::Controller(c) => c.controller.as_mut(),
        }
    }

    /// Invoke the device's `service()` handler.
    pub fn device_service(&mut self, cpu: &mut CPU, sysbus: &mut SystemBus) -> Result<(), SimError> {
        self.device_mut().device_service(cpu, sysbus)
    }

    /// Invoke the reset() handler on this device or controller.
    pub fn device_reset(&mut self) -> () {
        self.device_mut().device_reset()
    }

    /// Shared reference to a unit's trait object by zero-based index (controllers only).
    pub fn unit(&self, index: usize) -> Option<&(dyn DeviceTraits<CPU> + Send)> {
        match self {
            DeviceNode::Controller(c) => c.units.get(index).map(|u| u.unit.as_ref()),
            DeviceNode::Standalone(_) => None,
        }
    }

    /// Mutable reference to a unit's trait object by zero-based index (controllers only).
    pub fn unit_mut(&mut self, index: usize) -> Option<&mut (dyn DeviceTraits<CPU> + Send)> {
        match self {
            DeviceNode::Controller(c) => Some(c.units.get_mut(index)?.unit.as_mut()),
            DeviceNode::Standalone(_) => None,
        }
    }

    /// Shared reference to the full [`UnitMetadata`] by zero-based index.
    ///
    /// Use this when you need access to `enabled`, `service`, or `reset` alongside
    /// the unit trait object.
    pub fn unit_env(&self, index: usize) -> Option<&UnitMetadata<CPU>> {
        match self {
            DeviceNode::Controller(c) => c.units.get(index),
            DeviceNode::Standalone(_) => None,
        }
    }

    /// Mutable reference to the full [`UnitMetadata`] by zero-based index.
    pub fn unit_env_mut(&mut self, index: usize) -> Option<&mut UnitMetadata<CPU>> {
        match self {
            DeviceNode::Controller(c) => c.units.get_mut(index),
            DeviceNode::Standalone(_) => None,
        }
    }

    /// Append a [`UnitMetadata`] to this controller.
    /// Panics if called on a standalone device.
    pub fn add_unit(&mut self, unit_env: UnitMetadata<CPU>) {
        match self {
            DeviceNode::Controller(c) => c.units.push(unit_env),
            DeviceNode::Standalone(_) => panic!("Cannot add a unit to a standalone device"),
        }
    }

    /// `true` if this node is a controller with subordinate units.
    pub fn is_controller(&self) -> bool {
        matches!(self, DeviceNode::Controller(_))
    }
}

/// Result of a read-only device namespace lookup.
pub enum DeviceLookup<'a, CPU: CPUTraits> {
    /// A standalone device or controller, referenced by its top-level name.
    Device(&'a DeviceNode<CPU>),
    /// A unit inside a controller — carries both the parent node and the unit index.
    Unit { node: &'a DeviceNode<CPU>, index: usize },
}

/// Result of a mutable device namespace lookup.
pub enum DeviceLookupMut<'a, CPU: CPUTraits> {
    /// A standalone device or controller.
    Device(&'a mut DeviceNode<CPU>),
    /// A unit inside a controller.
    Unit {
        node: &'a mut DeviceNode<CPU>,
        index: usize,
    },
}
