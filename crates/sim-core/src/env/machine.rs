// SPDX-License-Identifier: MIT

//! CPU, Device and Unit abstractions
//!
//! [`SimResourceProvider`] is the fundamental resource-access interface (get/set registers, memory).
//! It is a supertrait of [`DeviceTraits`], so every device automatically exposes it through the
//! same `dyn DeviceTraits<CPU>` pointer — no casting required.
//!
//! The `#[derive(SimResources)]` proc-macro (crate `resource-macro`) generates a
//! `SimResourceProvider` implementation from annotated struct fields.
//!
//! # Controller and Unit Model
//!
//! SIMH-RS supports both simple devices and controller/unit architectures:
//! - **Simple devices**: Implement only [`DeviceTraits`] (e.g., CPU, simple I/O devices)
//! - **Controllers**: Implement [`DeviceTraits`] with controller-level resources and logic
//! - **Units**: Implement [`DeviceTraits`] as individual units (e.g., disk drives, tape drives)

use std::fmt::Debug;

use crate::env::{simerror::SimError, sysbus::SystemBus};

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
// SimResourceProvider — the resource-access supertrait
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

/// Resource inspection and modification interface, shared by CPUs and all devices.
///
/// This trait is implemented automatically by `#[derive(SimResources)]`, or manually via
/// `#[derive(SimResources)]`.  It is a supertrait of [`DeviceTraits`], so a
/// `dyn DeviceTraits<CPU>` pointer already carries this vtable — no downcasting is needed.
///
/// # Object Safety
/// All three methods are fully object-safe.  The trait can be used as `dyn SimResourceProvider`.
pub trait SimResourceProvider {
    /// Return the metadata for every resource this device or unit exposes.
    ///
    /// Resource IDs in [`ResourceMetadata::resource_id`] are stable for the lifetime of the process — they
    /// are assigned at compile time by the [`crate::SimResources`] derive macro (declaration order) or
    /// [`crate::env::SimEnvironment`] indexes them into its `resource_index` map once during device
    /// registration.
    fn get_metadata(&self) -> Vec<ResourceMetadata> {
        Vec::new()
    }

    /// Read one element of a resource.
    ///
    /// `res_id` is the device-local identifier from [`ResourceMetadata::resource_id`].
    /// `index` is zero for scalar resources; for array resources it selects the element.
    fn read_resource(&self, res_id: u32, index: usize) -> Result<u64, SimError> {
        // No resources by default, but this is how devices will implement it eventually.
        Err(SimError::NoSuchResource(format!(
            "Resource ID {} index {}",
            res_id, index
        )))
    }

    /// Write one element of a resource.
    ///
    /// `res_id` is the device-local identifier.
    /// `index` is zero for scalar resources.
    /// `val` is already range-checked by the CLI; the implementation may re-check if it wishes.
    fn write_resource(&mut self, res_id: u32, index: usize, _val: u64) -> Result<(), SimError> {
        // No resources by default, but this is how devices will implement it eventually.
        Err(SimError::NoSuchResource(format!(
            "Resource ID {} index {}",
            res_id, index
        )))
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
// CPUTraits
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

/// The simulator's CPU function interface.
///
/// # Notes
/// - A CPU **must** also expose a memory resource named [`MEM_RESOURCE_NAME`] (`"MEM"`).
///   [`crate::env::SimEnvironment::new`] panics if no such resource is found.
/// - A simulated CPU must also implement [`DeviceTraits`] (for CLI resource access) and
///   [`SimResourceProvider`] (via `#[derive(SimResources)]`).
pub trait CPUTraits: Debug + SimResourceProvider + Sized {
    /// Internal I/O payload passed down to [`DeviceTraits::execute_io`]
    ///
    /// CPU-specific data passed to [`DeviceTraits::execute_io`] method. For those that do require
    /// `execute_io()` dispatching:
    ///
    /// ```ignore
    /// impl CPUTraits for SomeProcessor {
    ///     // Define the SomeProcessorIoPayload structure to pass I/O context-specific data
    ///     // to `execute_io()`, set it here
    ///     type IoPayload = SomeProcessorIoPayload;
    ///
    ///     // ...
    ///
    ///     fn execute_io(&mut self, payload: &SomeProcesorPayload, cpu: &mut CPU) -> Result<(), SimError> {
    ///         // Process the I/O here.
    ///     }
    /// }
    /// ```
    ///
    /// For simulators that don't require `execute_io()` or use `execute_io()` but don't need the extra
    /// payload data, simply define `IoPayload` as the unit type. The Rust compiler understands this to be a
    /// zero-sized entity and can optimize it away.
    ///
    /// ```ignore
    /// impl CPUTraits for SomeProcessor {
    ///     // Define the SomeProcessorIoPayload structure to pass I/O context-specific data
    ///     // to `execute_io()`, set it here
    ///     type IoPayload = ();
    ///
    ///     // ...
    /// }
    /// ```
    type IoPayload;

    /// Simulate one instruction.
    ///
    /// The CPU receives the system bus (timers/console) and generic access to peripherals.
    fn simulate_instruction(
        &mut self,
        sysbus: &mut SystemBus,
        devices: &mut dyn DeviceAccessor<Self>,
    ) -> Result<(), SimError>;

    /// Populate memory with the instruction sequence used by `measure_initial_ips`.
    fn initial_ips_code(&mut self);

    /// Return a debug snapshot of the current program counter, or `None`.
    fn current_pc(&self) -> Option<String> {
        None
    }

    /// Reset the CPU to its power-on state.
    fn cpu_reset(&mut self);

    /// Disassemble the instruction at `address`, reading from the CPU's own memory.
    ///
    /// Returns `Some((text, next_pc))` where `next_pc` is the address of the following
    /// instruction, or `None` if this CPU does not implement a disassembler.
    ///
    /// The returned `next_pc` is `address + 1` for fixed-width ISAs; it may be larger
    /// for variable-length ISAs such as the PDP-11.
    ///
    /// The default implementation returns `None`.  Override in any CPU struct that also
    /// implements [`crate::env::Disassembler`].
    fn disassemble(&self, _address: usize) -> Option<(String, usize)> {
        None
    }

    /// Load a file into memory
    ///
    /// CPU-side of the CLI's "LOAD [flags] filename" command.
    fn load_file(&mut self, flags: Vec<char>, path: String) -> Result<(), SimError>;

    /// Optional lifecycle hook called by the environment before execution begins.
    /// Allows the CPU to resolve device names into fast O(1) handles.
    fn wire_devices(&mut self, _devices: &dyn DeviceAccessor<Self>) {
        // Default implementation does nothing.
        // CPUs with external I/O buses can override this (e.g., PDP-8)
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// DeviceAccessor
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// A fast, O(1) identifier for a registered device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceHandle(pub usize);

/// A generic interface for the CPU to request access to sibling devices during execution.
pub trait DeviceAccessor<CPU: CPUTraits> {
    /// Used ONCE during initialization to resolve a name to a fast handle.
    fn resolve_handle(&self, name: &str) -> Option<DeviceHandle>;
    /// Retrieve mutable access to a registered device by its CLI name.
    fn get_device_mut(&mut self, handle: DeviceHandle) -> Option<&mut (dyn DeviceTraits<CPU> + Send)>;
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
// DeviceTraits
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

/// Function interface for all devices (CPUs, simple devices, controllers, units).
///
/// `SimResourceProvider` is a **supertrait**: every `dyn DeviceTraits<CPU>` already provides
/// `get_metadata`, `read_resource`, and `write_resource` without any downcasting.
pub trait DeviceTraits<CPU: CPUTraits>: Debug + SimResourceProvider {
    /// Short device identifier, e.g. `"TTI"`, `"RK"`, `"RK0"`.
    ///
    /// This is the short name of the device, e.g., "RK" for a RK05 disk controller or "RK0" for a specific
    /// disk unit. Device resources are associated with this name.
    ///
    /// # Note
    /// For units, this should return the unit's unique name (e.g., "RK0", not the controller name "RK").
    fn device_name(&self) -> &str;

    /// The role this device plays in the system.
    ///
    /// Override for devices that have a special role (console, system clock, etc.).
    /// The default returns [`DeviceRole::Normal`], which is correct for the vast
    /// majority of devices.
    fn device_role(&self) -> DeviceRole {
        DeviceRole::Normal
    }

    /// Return the debug categories this device has registered, as `(name, is_enabled)` pairs.
    ///
    /// Called by [`crate::env::SimEnvironment::resource_manifest`] to populate
    /// [`crate::env::ResourceCLIMetadata::debug_categories`] for `SHOW DEBUG` / `SHOW DEVICES` display.  The
    /// enabled state is queried live from the global [`crate::logging::DebugRegistry`] at call time.
    ///
    /// The default returns an empty vec — devices that register no debug categories
    /// need not override this.
    fn debug_categories(&self) -> Vec<(String, bool)> {
        vec![]
    }

    /// Human-readable description.
    ///
    /// This is the generic description of the device, e.g., "RK05 disk system" for a controller, or
    /// "RK05 disk unit 0" for a specific unit.
    fn description(&self) -> &str;

    /// Device service function — receives mutable access to the system bus.
    ///
    /// Invoked by the execution loop when the device has pending events.
    fn device_service(&mut self, _cpu: &mut CPU, _bus: &mut SystemBus) -> Result<(), SimError> {
        // No service handler by default, but this is how devices will implement it eventually.
        Ok(())
    }

    /// Reset handler signature — called to reset the device or unit to power-on state.
    fn device_reset(&mut self) -> ();

    /// Executes a hardware I/O operation using the CPU's specific payload format.
    fn execute_io(
        &mut self,
        _payload: &CPU::IoPayload,
        _cpu: &mut CPU,
        _bus: &mut SystemBus,
    ) -> Result<(), SimError> {
        // Override as necessary.
        Ok(())
    }

    //~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
    // Attachment interface accessors
    //~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

    /// Get the attachment interface if this device supports attachments
    ///
    /// Typically:
    /// - **Controllers**: Return `None` (they don't attach to files)
    /// - **Units**: Return `Some(&self)` if they implement [`DeviceAttachmentInterface`]
    /// - **CPUs and simple devices**: Return `None`
    fn attachment(&self) -> Option<&dyn DeviceAttachmentInterface> {
        None
    }

    /// Get mutable access to the attachment interface
    fn attachment_mut(&mut self) -> Option<&mut dyn DeviceAttachmentInterface> {
        None
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Metadata and Access trait: Actor model halves that talk to each other. The Metadata is sent or resides over
// on the CLI side (generally) and the Access trait lives on the simulator.
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// The role a device plays in the simulator.
///
/// Used by the CLI to identify special-purpose devices without hardcoding names.
/// Reported via [`DeviceTraits::device_role`]; the default is [`DeviceRole::Normal`].
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceRole {
    /// Ordinary device — no special CLI or system role.
    Normal,
    /// The system console — the CLI routes raw terminal I/O through this device
    /// when the simulator is running.
    Console,
}

/// Device and unit metadata exported to the CLI.
///
/// `ResourceCLIMetadata` is the read-only data transfer object sent from the simulator
/// to the CLI at startup (via `resource_manifest()`) and queried for `SHOW DEVICES`,
/// `SHOW DEBUG`, EXAMINE, and DEPOSIT operations.
///
/// The struct is self-referential: a controller entry carries its units as a
/// `Vec<ResourceCLIMetadata>` in `units`. Standalone devices and units have an
/// empty `units` vec. The CLI traverses both levels uniformly — no separate
/// `UnitCLIMetadata` type is needed.
///
/// # Relationship to internal types
///
/// This is distinct from [`crate::env::DeviceMetadata`], which is the simulator's internal registry
/// entry. `ResourceCLIMetadata` is a snapshot derived from [`crate::env::DeviceMetadata`] at
/// manifest-generation time and sent over the channel to the CLI.
#[derive(Debug, Clone)]
pub struct ResourceCLIMetadata {
    /// Device or unit name, e.g. `"TTI"`, `"RK"`, `"RK0"`.
    pub name: String,
    /// Human-readable description, e.g. `"RK05 disk controller"`.
    pub description: String,
    /// The role this device plays in the system.
    pub role: DeviceRole,
    /// Whether this device or unit is currently enabled.
    pub enabled: bool,
    /// Resources exposed by this device or unit.
    pub resources: Vec<ResourceMetadata>,
    /// Debug categories registered by this device, as `(name, is_enabled)` pairs.
    /// Queried live from the registry at manifest-generation time.
    pub debug_categories: Vec<(String, bool)>,
    /// Subordinate units. Empty for standalone devices and units.
    pub units: Vec<ResourceCLIMetadata>,
    /// Whether this device or unit currently has something attached.
    /// Meaningful for units; typically `false` for controllers and standalone devices.
    pub attached: bool,
    /// Human-readable description of what is attached (e.g. a file path), if anything.
    pub attachment: Option<String>,
}

impl ResourceCLIMetadata {
    /// `true` if this entry has subordinate units (i.e. is a controller).
    pub fn is_controller(&self) -> bool {
        !self.units.is_empty()
    }

    /// `true` if this is a standalone device or unit with no subordinates.
    pub fn is_standalone(&self) -> bool {
        self.units.is_empty()
    }

    /// Find a unit by name (case-insensitive).
    pub fn get_unit(&self, name: &str) -> Option<&ResourceCLIMetadata> {
        let name_upper = name.to_uppercase();
        self.units.iter().find(|u| u.name == name_upper)
    }

    /// Find a unit by zero-based index.
    pub fn get_unit_by_index(&self, index: usize) -> Option<&ResourceCLIMetadata> {
        self.units.get(index)
    }
}

/// Per-resource metadata, included in every [`ResourceCLIMetadata`].
///
/// `formatter` is a bare function pointer (`Copy + Clone + Send + Sync`) so `ResourceMetadata`
/// itself remains `Copy`-friendly and crosses thread boundaries without restriction.
#[derive(Clone)]
pub struct ResourceMetadata {
    /// Canonical uppercase name, e.g. `"PC"`, `"MEM"`.
    pub name: String,
    /// Resource description
    ///
    /// Long form description of the resource. Defaults to the `name` if not specified. Used for help text and
    /// detailed resource listings in the CLI.
    pub description: String,
    /// Device-local identifier used by `read_resource` / `write_resource`.
    ///
    /// Assigned at compile time by `#[derive(SimResources)]`.
    /// Stable for the process lifetime; indexed by [`crate::env::SimEnvironment`] during
    /// device registration.
    pub resource_id: u32,
    /// Bit-width of one element.
    pub word_size: u32,
    /// 1 for scalars; > 1 for arrays (e.g. `MEM`).
    pub length: usize,
    /// Optional display formatter.  `None` means use the simulator's default numeric format.
    ///
    /// Stored as a bare function pointer rather than a name string so the formatter is
    /// checked at compile time and called with zero lookup overhead.
    pub formatter: Option<fn(u64) -> String>,
    /// `true` if the CLI may not deposit into this resource.
    pub read_only: bool,
}

impl std::fmt::Debug for ResourceMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceMetadata")
            .field("name", &self.name)
            .field("resource_id", &self.resource_id)
            .field("word_size", &self.word_size)
            .field("length", &self.length)
            .field("formatter", &self.formatter.map(|f| f as usize))
            .field("read_only", &self.read_only)
            .finish()
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
// Attachment Interface
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

/// External resource attachment interface (typically implemented by units).
///
/// This trait is typically implemented by units that can attach to external resources
/// (files, network interfaces, etc.). Controllers generally do not implement this trait.
pub trait DeviceAttachmentInterface {
    fn attach(&mut self, resource: AttachmentResource) -> Result<(), SimError>;
    fn detach(&mut self) -> Result<(), SimError>;
    fn is_attached(&self) -> bool;
    fn attachment_info(&self) -> Option<AttachmentInfo>;
    fn supported_attachments(&self) -> &[AttachmentType];
    fn attachment_name(&self) -> Option<String> {
        self.attachment_info().map(|info| info.description.clone())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttachmentType {
    File,
    Network,
    CharDevice,
    BlockDevice,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttachmentResource {
    File {
        path: String,
        read_only: bool,
        create_if_missing: bool,
    },
    Network {
        interface_type: NetworkInterfaceType,
        config: NetworkConfig,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum NetworkInterfaceType {
    TunTap(String),
    WinTun(String),
    Slirp,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NetworkConfig {
    pub mac_address: Option<[u8; 6]>,
    pub ip_address: Option<String>,
}

#[derive(Debug)]
pub struct AttachmentInfo {
    pub attachment_type: AttachmentType,
    pub description: String,
    pub read_only: bool,
    pub size: Option<u64>,
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
// Well-known names
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

/// Reserved name for the CPU device.
pub const CPU_DEVICE_NAME: &str = "CPU";
/// Reserved device identifier for the CPU.
pub const CPU_DEVICE_ID: usize = usize::MAX;
/// Canonical name of the memory resource.
pub const MEM_RESOURCE_NAME: &str = "MEM";
