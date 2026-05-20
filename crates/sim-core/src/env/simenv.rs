// SPDX-License-Identifier: MIT

//! Simulator environment container.
//!
//! [`SimEnvironment`] wraps a CPU, its devices, the timer subsystem, and the CLI message
//! channel into one owned value that is moved into the simulator thread at startup.
//!
//! # Resource index
//!
//! At device-registration time (after every `add_standalone`, `add_controller`, `add_unit`)
//! `SimEnvironment` builds a [`ResourceIndex`] — a `HashMap<(device_name, resource_name),
//! ResourceLocator>` — that lets `handle_request` resolve a name-pair into a
//! `(local_id, unit_index)` in O(1) without walking the device tree.
//!
//! This completely replaces the old `ResourceId` scheme: the CLI now sends
//! `(device_name, resource_name)` strings in every `Examine` / `Deposit` request; the
//! simulator resolves them here.

use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::sync::{
    mpsc::{Receiver, Sender},
    Arc,
};

use rustc_hash::FxHasher;

use crate::{
    env::{
        console::SimConsole,
        metadata::{DeviceMetadata, UnitMetadata},
        device_node::{Controller, DeviceLookup, DeviceLookupMut, DeviceNode, StandaloneDevice},
        machine::{
            AttachmentResource, CPUTraits, DeviceAccessor, DeviceHandle, DeviceRole, DeviceTraits,
            ResourceCLIMetadata, SimResourceProvider, CPU_DEVICE_ID, CPU_DEVICE_NAME,
        },
        messages::{ExamineResult, SimRequest, SimResponse},
        simerror::SimError,
        sysbus::SystemBus,
    },
    logging::{debug_registry, SharedDebugSnapshot, SharedDebugState},
    timers::{create_platform_timer, TimerError, TimerManager, TimerResult},
};

// ─── resource index ───────────────────────────────────────────────────────────

/// Where to find a resource in the device tree.
///
/// The key of [`ResourceIndex`] is `(device_or_unit_name_upper, resource_name_upper)`.
/// `top_level_name` is always the key into [`SimEnvironment::devices`]; for units this
/// differs from the device/unit name that appears in the key.
#[derive(Debug, Clone)]
pub struct ResourceLocator {
    /// Index into `SimEnvironment::devices` Vec
    pub device_index: usize,
    /// `None` for standalone devices and controllers; `Some(i)` for a unit
    pub unit_index: Option<usize>,
    /// Device-local resource ID
    pub local_id: u32,
}

/// `(device_name_upper, resource_name_upper)` → `ResourceLocator`
type ResourceIndex = HashMap<(String, String), ResourceLocator, BuildHasherDefault<FxHasher>>;

// ─── ActiveDevices wrapper ────────────────────────────────────────────────────

// A zero-cost wrapper to satisfy the DeviceAccessor trait
pub(crate) struct ActiveDevices<'a, CPU: CPUTraits>(pub(crate) &'a mut Vec<DeviceMetadata<CPU>>);

impl<'a, CPU: CPUTraits> DeviceAccessor<CPU> for ActiveDevices<'a, CPU> {
    fn resolve_handle(&self, name: &str) -> Option<DeviceHandle> {
        let upper = name.to_uppercase();
        self.0
            .iter()
            .position(|d| d.node().device().device_name() == upper)
            .map(DeviceHandle)
    }

    #[inline(always)] // Force the compiler to inline this array access
    fn get_device_mut(&mut self, handle: DeviceHandle) -> Option<&mut (dyn DeviceTraits<CPU> + Send)> {
        self.0.get_mut(handle.0).map(|d| d.device_mut())
    }
}

// ─── SimEnvironment ───────────────────────────────────────────────────────────

/// The simulator environment.
pub struct SimEnvironment<CPU>
where
    CPU: CPUTraits + Send + 'static,
{
    /// The CPU
    pub(crate) cpu: CPU,

    /// System bus — Runtime home of the timer and console.
    pub(crate) bus: SystemBus,

    /// Device namespace keyed by controller / standalone device name only.
    ///
    /// Units live inside `DeviceNode::Controller::units`; use [`Self::lookup`] / [`Self::lookup_mut`] to find
    /// them by name.
    pub(crate) devices: Vec<DeviceMetadata<CPU>>,

    /// Fast `(device_name, resource_name)` → locator index.
    ///
    /// Rebuilt by [`Self::rebuild_resource_index`] after every device or unit registration.
    pub(crate) resource_index: ResourceIndex,

    /// `true` while the CPU should be executing instructions.
    pub(crate) running: bool,

    /// Debug session state shared with the CLI.
    pub debug_state: Option<SharedDebugState>,

    /// CPU state snapshot written each execution cycle (PC, instruction count).
    pub debug_snapshot: Option<SharedDebugSnapshot>,

    /// Message channel to/from the CLI.
    pub cli_connection: Option<SimCLIConnection>,
}

impl<CPU> SimEnvironment<CPU>
where
    CPU: CPUTraits + Send + 'static,
{
    /// Create a new simulator environment, calibrate the timer, and return.
    pub fn new(cpu: CPU) -> Self {
        let mut timer_mgr = TimerManager::new(create_platform_timer());
        timer_mgr.init_timer(0, 60.0).ok();

        Self {
            cpu,
            bus: SystemBus::new(timer_mgr),
            devices: Vec::default(),
            resource_index: HashMap::default(),
            running: false,
            debug_state: None,
            debug_snapshot: None,
            cli_connection: None,
        }
    }

    // ── accessors ─────────────────────────────────────────────────────────────

    pub fn cpu(&self) -> &CPU {
        &self.cpu
    }

    pub fn cpu_mut(&mut self) -> &mut CPU {
        &mut self.cpu
    }

    pub fn sysbus(&self) -> &SystemBus {
        &self.bus
    }

    pub fn sysbus_mut(&mut self) -> &mut SystemBus {
        &mut self.bus
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn set_running(&mut self, running: bool) {
        self.running = running;
    }

    // ── Instructions-per-second benchmarking ───────────────────────────────────

    pub fn benchmark_ips(&mut self) {
        // Generate the initial instructions-per-second benchmark code.
        self.cpu.initial_ips_code();

        // Benchmark!
        let initial_ips = self.measure_initial_ips().unwrap_or(5_000_000);

        self.bus.timer.set_instructions_per_sec(initial_ips as f64);
    }

    //=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
    // Initial IPS measurement
    //=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

    /// Measure initial instructions per second
    ///
    /// Executes a CPU instruction function repeatedly for a calibrated time period
    /// and calculates the initial instructions per second rate.
    ///
    /// # Returns
    /// Estimated instructions per second, or error if measurement fails
    pub fn measure_initial_ips(&mut self) -> TimerResult<i32> {
        const MIN_MEASURE_TIME_MS: u32 = 100;
        const TARGET_MEASURE_TIME_MS: u32 = 500;
        const MAX_MEASURE_INSTRUCTIONS: u64 = 10_000_000;

        let cpu = &mut self.cpu;
        let mut sysbus = &mut self.bus;
        let mut devices = ActiveDevices(&mut self.devices);

        // Generate the code.
        cpu.initial_ips_code();

        // Warm-up: execute a few instructions to initialize caches
        for _ in 0..1000 {
            if cpu.simulate_instruction(&mut sysbus, &mut devices).is_err() {
                return Err(TimerError::CalibrationFailed(
                    "CPU instruction execution failed during warm-up".to_string(),
                ));
            }
        }

        // Start measurement
        let start_time = sysbus.timer.get_msec();
        let mut instructions_executed = 0u64;

        loop {
            // Execute a batch of instructions
            for _ in 0..100 {
                if cpu.simulate_instruction(&mut sysbus, &mut devices).is_err() {
                    return Err(TimerError::CalibrationFailed(
                        "CPU instruction execution failed during measurement".to_string(),
                    ));
                }
                instructions_executed += 1;
            }

            let elapsed = sysbus.timer.get_msec() - start_time;

            // Check if we've measured long enough
            if elapsed >= MIN_MEASURE_TIME_MS {
                // Calculate IPS
                let ips = (instructions_executed as f64 * 1000.0) / elapsed as f64;

                // Sanity check: IPS should be reasonable (1K to 10B per second)
                if ips < 1_000.0 || ips > 10_000_000_000.0 {
                    return Err(TimerError::CalibrationFailed(format!(
                        "Measured IPS ({:.0}) is outside reasonable range",
                        ips
                    )));
                }

                return Ok(ips as i32);
            }

            // Safety limit: don't measure forever
            if elapsed >= TARGET_MEASURE_TIME_MS || instructions_executed >= MAX_MEASURE_INSTRUCTIONS {
                let ips = (instructions_executed as f64 * 1000.0) / elapsed as f64;
                return Ok(ips as i32);
            }
        }
    }

    // ── CLI connection ─────────────────────────────────────────────────────────

    /// Wire up the CLI message channel and console queues.
    pub fn cli_connect(
        &mut self,
        cli_tx: Sender<SimResponse>,
        cli_rx: Receiver<SimRequest>,
        console: &SimConsole,
    ) {
        self.bus.attach_console(console);
        self.cli_connection = Some(SimCLIConnection::new(cli_tx, cli_rx, Arc::clone(console)));
    }

    // ── device namespace lookup ────────────────────────────────────────────────

    /// Resolve a name that may be a device/controller name **or** a unit name.
    pub fn lookup(&self, name: &str) -> Option<DeviceLookup<'_, CPU>> {
        let upper = name.to_ascii_uppercase();

        for dev_env in &self.devices {
            // 1. Check the top-level device or controller name
            if dev_env.dev_node.device().device_name().to_ascii_uppercase() == upper {
                return Some(DeviceLookup::Device(&dev_env.dev_node));
            }

            // 2. If it's a controller, check its nested units
            if let DeviceNode::Controller(c) = &dev_env.dev_node {
                for (index, unit_env) in c.units.iter().enumerate() {
                    if unit_env.unit.device_name().to_ascii_uppercase() == upper {
                        return Some(DeviceLookup::Unit {
                            node: &dev_env.dev_node,
                            index,
                        });
                    }
                }
            }
        }
        None
    }

    /// Mutable variant of [`Self::lookup`].
    pub fn lookup_mut(&mut self, name: &str) -> Option<DeviceLookupMut<'_, CPU>> {
        let upper = name.to_ascii_uppercase();

        for dev_env in &mut self.devices {
            // 1. Check the top-level device or controller
            if dev_env.dev_node.device().device_name().to_ascii_uppercase() == upper {
                return Some(DeviceLookupMut::Device(&mut dev_env.dev_node));
            }

            // 2. Check nested units
            if let DeviceNode::Controller(c) = &mut dev_env.dev_node {
                for (index, unit_env) in c.units.iter().enumerate() {
                    if unit_env.unit.device_name().to_ascii_uppercase() == upper {
                        // Re-borrow dev_node mutably to satisfy the return type
                        return Some(DeviceLookupMut::Unit {
                            node: &mut dev_env.dev_node,
                            index,
                        });
                    }
                }
            }
        }
        None
    }

    pub fn has_device(&self, name: &str) -> bool {
        self.lookup(name).is_some()
    }

    /// Get a mutable `DeviceTraits` reference by name (searches devices and units).
    pub fn get_device_mut(&mut self, name: &str) -> Result<&mut (dyn DeviceTraits<CPU> + Send), SimError>
    where
        CPU: DeviceTraits<CPU>,
    {
        let upper = name.to_ascii_uppercase();

        if upper == CPU_DEVICE_NAME {
            return Ok(&mut self.cpu);
        }

        match self.lookup_mut(name) {
            Some(DeviceLookupMut::Device(node)) => Ok(node.device_mut()),
            Some(DeviceLookupMut::Unit { node, index }) => node
                .unit_mut(index)
                .ok_or_else(|| SimError::DeviceNotFound(name.to_string())),
            None => Err(SimError::DeviceNotFound(name.to_string())),
        }
    }

    // ── device registration ────────────────────────────────────────────────────

    /// Register a standalone device.
    pub fn add_standalone(&mut self, device: Box<dyn DeviceTraits<CPU> + Send>) -> &mut Self {
        let name = device.device_name().to_ascii_uppercase();
        self.check_name(&name).unwrap();

        let debug_category_names = device.debug_categories().into_iter().map(|(n, _)| n).collect();

        self.devices.push(DeviceMetadata {
            dev_node: DeviceNode::Standalone(StandaloneDevice { device }),
            enabled: true,
            debug_category_names,
        });
        self.rebuild_resource_index();
        self
    }

    /// Register a controller (units added later via [`Self::add_unit`]).
    pub fn add_controller(&mut self, controller: Box<dyn DeviceTraits<CPU> + Send>) -> &mut Self {
        let name = controller.device_name().to_ascii_uppercase();
        self.check_name(&name).unwrap();

        let debug_category_names = controller
            .debug_categories()
            .into_iter()
            .map(|(n, _)| n)
            .collect();

        self.devices.push(DeviceMetadata {
            dev_node: DeviceNode::Controller(Controller {
                controller,
                units: Vec::new(),
            }),
            enabled: true,
            debug_category_names,
        });
        self.rebuild_resource_index();
        self
    }

    /// Add a unit to an existing controller.
    pub fn add_unit(&mut self, controller_name: &str, unit: Box<dyn DeviceTraits<CPU> + Send>) -> &mut Self {
        let ctrl_upper = controller_name.to_ascii_uppercase();
        let unit_upper = unit.device_name().to_ascii_uppercase();
        let debug_category_names = unit.debug_categories().into_iter().map(|(n, _)| n).collect();

        let dev_env = self
            .devices
            .iter_mut()
            .find(|d| d.dev_node.device().device_name().to_ascii_uppercase() == ctrl_upper)
            .unwrap_or_else(|| panic!("Controller '{}' not found", ctrl_upper));

        if let DeviceNode::Controller(c) = &mut dev_env.dev_node {
            if c.units
                .iter()
                .any(|u| u.unit.device_name().to_ascii_uppercase() == unit_upper)
            {
                panic!("Unit '{}' already registered", unit_upper);
            }
            c.units.push(UnitMetadata {
                unit,
                enabled: true,
                debug_category_names,
            });
        } else {
            panic!("'{}' is not a controller", ctrl_upper);
        }

        self.rebuild_resource_index();
        self
    }

    /// Validate that `name` is not reserved and does not already exist.
    pub fn check_name(&self, name: &str) -> Result<(), SimError> {
        let upper = name.to_ascii_uppercase();
        if upper == CPU_DEVICE_NAME {
            return Err(SimError::DuplicateDevice(format!("{} (reserved)", name)));
        }

        // Scan the Vec for a duplicate name
        if self
            .devices
            .iter()
            .any(|d| d.dev_node.device().device_name().to_ascii_uppercase() == upper)
        {
            return Err(SimError::DuplicateDevice(upper));
        }
        Ok(())
    }

    pub fn device_names(&self) -> Vec<String> {
        let mut names = vec![CPU_DEVICE_NAME.to_string()];
        names.extend(
            self.devices
                .iter()
                .map(|d| d.dev_node.device().device_name().to_ascii_uppercase()),
        );
        names.sort();
        names
    }

    // ── resource index ─────────────────────────────────────────────────────────

    /// (Re)build the `(device_name, resource_name)` → `ResourceLocator` index.
    ///
    /// Called automatically after every registration operation.  The CPU's resources are
    /// also indexed here (keyed under [`CPU_DEVICE_NAME`]).
    fn rebuild_resource_index(&mut self)
    where
        CPU: SimResourceProvider,
    {
        let mut idx: ResourceIndex = HashMap::default();

        // CPU resources.
        for meta in self.cpu.get_metadata() {
            idx.insert(
                (CPU_DEVICE_NAME.to_string(), meta.name.to_ascii_uppercase()),
                ResourceLocator {
                    device_index: CPU_DEVICE_ID,
                    unit_index: None,
                    local_id: meta.resource_id,
                },
            );
        }

        // Registered devices.
        for (dev_idx, dev_meta) in self.devices.iter().enumerate() {
            let top_name = dev_meta.dev_node.device().device_name().to_ascii_uppercase();

            match &dev_meta.dev_node {
                DeviceNode::Standalone(s) => {
                    for meta in s.device.get_metadata() {
                        idx.insert(
                            (top_name.clone(), meta.name.to_ascii_uppercase()),
                            ResourceLocator {
                                device_index: dev_idx,
                                unit_index: None,
                                local_id: meta.resource_id,
                            },
                        );
                    }
                }
                DeviceNode::Controller(c) => {
                    for meta in c.controller.get_metadata() {
                        idx.insert(
                            (top_name.clone(), meta.name.to_ascii_uppercase()),
                            ResourceLocator {
                                device_index: dev_idx,
                                unit_index: None,
                                local_id: meta.resource_id,
                            },
                        );
                    }
                    for (unit_idx, unit_env) in c.units.iter().enumerate() {
                        let unit_name = unit_env.unit.device_name().to_ascii_uppercase();
                        for meta in unit_env.unit.get_metadata() {
                            idx.insert(
                                (unit_name.clone(), meta.name.to_ascii_uppercase()),
                                ResourceLocator {
                                    device_index: dev_idx,
                                    unit_index: Some(unit_idx),
                                    local_id: meta.resource_id,
                                },
                            );
                        }
                    }
                }
            }
        }
        self.resource_index = idx;
    }

    /// Look up a `ResourceLocator` by `(device_name, resource_name)`.
    fn locate_resource(&self, device_name: &str, resource_name: &str) -> Option<&ResourceLocator> {
        self.resource_index.get(&(
            device_name.to_ascii_uppercase(),
            resource_name.to_ascii_uppercase(),
        ))
    }

    /// Read one or more elements of a named resource on a named device/unit.
    fn read_resource_by_name(
        &self,
        device_name: &str,
        resource_name: &str,
        start: usize,
        count: usize,
    ) -> Result<Vec<u64>, SimError>
    where
        CPU: DeviceTraits<CPU>,
    {
        let loc = self
            .locate_resource(device_name, resource_name)
            .ok_or_else(|| SimError::NoSuchResource(resource_name.to_string()))?
            .clone();

        let provider: &dyn SimResourceProvider = if loc.device_index == CPU_DEVICE_ID {
            &self.cpu
        } else {
            let dev_meta = self
                .devices
                .get(loc.device_index)
                .ok_or_else(|| SimError::DeviceNotFound(device_name.to_string()))?;

            match loc.unit_index {
                None => dev_meta.dev_node.device(),
                Some(ui) => dev_meta.dev_node.unit_env(ui).unwrap().unit.as_ref(),
            }
        };

        (0..count)
            .map(|i| provider.read_resource(loc.local_id, start + i))
            .collect()
    }

    /// Write one or more elements of a named resource on a named device/unit.
    fn write_resource_by_name(
        &mut self,
        device_name: &str,
        resource_name: &str,
        start: usize,
        values: Vec<u64>,
    ) -> Result<(), SimError>
    where
        CPU: DeviceTraits<CPU>,
    {
        let loc = self
            .locate_resource(device_name, resource_name)
            .ok_or_else(|| SimError::NoSuchResource(resource_name.to_string()))?
            .clone();

        let provider: &mut dyn SimResourceProvider = if loc.device_index == CPU_DEVICE_ID {
            &mut self.cpu
        } else {
            let dev_meta = self
                .devices
                .get_mut(loc.device_index)
                .ok_or_else(|| SimError::DeviceNotFound(device_name.to_string()))?;

            match loc.unit_index {
                None => dev_meta.dev_node.device_mut(),
                Some(ui) => {
                    if let DeviceNode::Controller(ref mut controller) = dev_meta.dev_node {
                        controller
                            .units
                            .get_mut(ui)
                            .ok_or_else(|| SimError::UnitNotFound(device_name.to_string(), ui))?
                            .unit
                            .as_mut()
                    } else {
                        // This case is logically unreachable if ResourceLocator is accurate,
                        // but the compiler requires it.
                        unreachable!("ResourceLocator with unit_index for non-controller device");
                    }
                }
            }
        };

        values
            .into_iter()
            .enumerate()
            .try_for_each(|(i, v)| provider.write_resource(loc.local_id, start + i, v))
    }

    // ── manifest generation ────────────────────────────────────────────────────

    /// Build the metadata bundle sent to the CLI at startup.
    pub fn resource_manifest(&self) -> Vec<ResourceCLIMetadata>
    where
        CPU: DeviceTraits<CPU>,
    {
        let reg = debug_registry();
        let mut manifest = Vec::new();

        manifest.push(ResourceCLIMetadata {
            name: CPU_DEVICE_NAME.to_string(),
            description: self.cpu.description().to_string(),
            role: DeviceRole::Normal,
            enabled: true,
            resources: self.cpu.get_metadata(),
            debug_categories: vec![],
            units: vec![],
            attached: false,
            attachment: None,
        });

        for dev_meta in &self.devices {
            let name = dev_meta.dev_node.device().device_name().to_string();
            match &dev_meta.dev_node {
                DeviceNode::Standalone(s) => {
                    let debug_categories = dev_meta
                        .debug_category_names
                        .iter()
                        .map(|n| (n.clone(), reg.is_enabled_by_name(n)))
                        .collect();

                    manifest.push(ResourceCLIMetadata {
                        name: name.clone(),
                        description: s.device.description().to_string(),
                        role: s.device.device_role(),
                        enabled: dev_meta.enabled,
                        resources: s.device.get_metadata(),
                        debug_categories,
                        units: vec![],
                        attached: s.device.attachment().map_or(false, |a| a.is_attached()),
                        attachment: s.device.attachment().and_then(|a| a.attachment_name()),
                    });
                }
                DeviceNode::Controller(c) => {
                    let debug_categories = dev_meta
                        .debug_category_names
                        .iter()
                        .map(|n| (n.clone(), reg.is_enabled_by_name(n)))
                        .collect();

                    let units = c
                        .units
                        .iter()
                        .map(|unit_meta| {
                            let unit_cats = unit_meta
                                .debug_category_names
                                .iter()
                                .map(|n| (n.clone(), reg.is_enabled_by_name(n)))
                                .collect();

                            ResourceCLIMetadata {
                                name: unit_meta.unit.device_name().to_string(),
                                description: unit_meta.unit.description().to_string(),
                                role: unit_meta.unit.device_role(),
                                enabled: unit_meta.enabled,
                                resources: unit_meta.unit.get_metadata(),
                                debug_categories: unit_cats,
                                units: vec![],
                                attached: unit_meta.unit.attachment().map_or(false, |a| a.is_attached()),
                                attachment: unit_meta.unit.attachment().and_then(|a| a.attachment_name()),
                            }
                        })
                        .collect();

                    manifest.push(ResourceCLIMetadata {
                        name: name.clone(),
                        description: c.controller.description().to_string(),
                        role: c.controller.device_role(),
                        enabled: dev_meta.enabled,
                        resources: c.controller.get_metadata(),
                        debug_categories,
                        units,
                        attached: false,
                        attachment: None,
                    });
                }
            }
        }

        manifest
    }

    //~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
    // Device servicing, reset:
    //~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

    /// Service a device
    pub fn service_device(&mut self, device_name: &str) -> Result<(), SimError> {
        let upper = device_name.to_ascii_uppercase();
        if let Some(dev_meta) = self
            .devices
            .iter_mut()
            .find(|d| d.dev_node.device().device_name().to_ascii_uppercase() == upper)
        {
            dev_meta.device_service(&mut self.cpu, &mut self.bus)
        } else {
            Err(SimError::DeviceNotFound(device_name.to_string()))
        }
    }

    /// Reset some or all devices.
    pub fn reset(&mut self, specific_device: Option<&str>) {
        match specific_device {
            Some(name) => {
                let upper = name.to_ascii_uppercase();
                if upper == CPU_DEVICE_NAME {
                    self.cpu.cpu_reset();
                    return;
                }
                if let Some(dev_meta) = self
                    .devices
                    .iter_mut()
                    .find(|d| d.dev_node.device().device_name().to_ascii_uppercase() == upper)
                {
                    dev_meta.device_reset();
                }
            }
            None => {
                self.cpu.cpu_reset();
                for dev_meta in &mut self.devices {
                    dev_meta.device_reset();
                }
            }
        }
    }

    pub fn reset_all(&mut self) {
        self.reset(None)
    }

    //=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
    // Attachment handling.
    //=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

    pub fn attach_device(&mut self, device_name: &str, resource: AttachmentResource) -> Result<(), SimError>
    where
        CPU: DeviceTraits<CPU>,
    {
        let device = self.get_device_mut(device_name)?;
        if let Some(attachment) = device.attachment_mut() {
            attachment.attach(resource)
        } else {
            Err(SimError::UnsupportedAttachment)
        }
    }

    pub fn detach_device(&mut self, device_name: &str) -> Result<(), SimError>
    where
        CPU: DeviceTraits<CPU>,
    {
        let device = self.get_device_mut(device_name)?;
        if let Some(attachment) = device.attachment_mut() {
            attachment.detach()
        } else {
            Err(SimError::UnsupportedAttachment)
        }
    }

    //=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
    // CPU device table/mapping initialization.
    //=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

    /// Execute the CPU's device mapping initialization
    ///
    /// Invoke the CPU's [`CPUTraits::wire_devices`] to finalize internal device mappings and [`DeviceHandle`]
    /// translations.
    pub fn finalize_hardware(&mut self) {
        let accessor = ActiveDevices(&mut self.devices);
        self.cpu.wire_devices(&accessor);
    }

    //=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
    // Messsage handling
    //=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

    /// Dispatch one [`SimRequest`] from the CLI.  Returns `true` to signal shutdown.
    pub fn handle_request(&mut self, req: SimRequest) -> bool
    where
        CPU: DeviceTraits<CPU>,
    {
        match req {
            SimRequest::Examine(batch) => {
                let results: Vec<Result<ExamineResult, SimError>> = batch
                    .into_iter()
                    .map(|r| {
                        if r.mnemonic {
                            // Disassemble directly from CPU memory via disassemble.
                            // No pre-fetch needed — the CPU reads its own memory.
                            // If the CPU has no disassembler, fall back to plain values.
                            let end = r.start + r.count;
                            let mut addr = r.start;

                            // Check whether this CPU supports disassembly.
                            match self.cpu.disassemble(addr) {
                                Some(_) => {
                                    let mut mnemonics: Vec<(usize, String)> = Vec::new();
                                    while addr < end {
                                        let (text, next_pc) = self.cpu.disassemble(addr).unwrap();
                                        mnemonics.push((addr, text));
                                        // Guard against a stuck disassembler.
                                        addr = if next_pc > addr { next_pc } else { addr + 1 };
                                    }
                                    Ok(ExamineResult::Mnemonics(mnemonics))
                                }
                                None => {
                                    // No disassembler — fall back to raw values.
                                    let values = self.read_resource_by_name(
                                        &r.device_name,
                                        &r.resource_name,
                                        r.start,
                                        r.count,
                                    )?;
                                    Ok(ExamineResult::Values(values))
                                }
                            }
                        } else {
                            let values = self.read_resource_by_name(
                                &r.device_name,
                                &r.resource_name,
                                r.start,
                                r.count,
                            )?;
                            Ok(ExamineResult::Values(values))
                        }
                    })
                    .collect();
                self.send(SimResponse::ExamineData(results));
            }

            SimRequest::Deposit {
                device_name,
                resource_name,
                start,
                values,
            } => {
                let result = self.write_resource_by_name(&device_name, &resource_name, start, values);
                self.send_result(result);
            }

            SimRequest::Attach {
                device_name,
                resource,
            } => {
                let result = self.attach_device(&device_name, resource);
                self.send_result(result);
            }

            SimRequest::Detach { device_name } => {
                let result = self.detach_device(&device_name);
                self.send_result(result);
            }

            SimRequest::LoadFile { flags, path } => {
                let result = self.cpu.load_file(flags, path);
                self.send_result(result);
            }

            SimRequest::Step(_n) => {
                self.set_running(true);
                self.send_success();
            }

            SimRequest::Stop => {
                self.running = false;
                self.send_success();
            }

            SimRequest::Reset(device) => {
                self.reset(device.as_deref());
                self.send_success();
            }

            SimRequest::ScheduleDevice { name, delay } => {
                self.bus.timer.schedule_device(&name, delay);
                self.send_success();
            }

            SimRequest::SetDebugState(state, snapshot) => {
                self.debug_state = Some(state);
                self.debug_snapshot = snapshot;
                self.send_success();
            }

            SimRequest::DebugDisable => {
                self.debug_state = None;
                self.debug_snapshot = None;
                self.send_success();
            }

            SimRequest::Quit => return true,
        }
        false
    }

    // ── internal send helpers ──────────────────────────────────────────────────

    fn send_result(&self, result: Result<(), SimError>) -> Option<()> {
        self.send(match result {
            Ok(()) => SimResponse::Ok,
            Err(e) => SimResponse::Error(e),
        })
    }

    fn send_success(&self) -> Option<()> {
        self.send(SimResponse::Ok)
    }

    fn send(&self, msg: SimResponse) -> Option<()> {
        self.cli_connection
            .as_ref()
            .unwrap_or_else(|| panic!("SimEnvironment::send: CLI not connected!"))
            .send(msg)
    }
}

// ─── SimCLIConnection ─────────────────────────────────────────────────────────

/// Simulator side of the CLI ↔ simulator message channel.
pub struct SimCLIConnection {
    pub cli_tx: Sender<SimResponse>,
    pub cli_rx: Receiver<SimRequest>,
    pub console: SimConsole,
}

impl SimCLIConnection {
    pub(crate) fn new(
        cli_tx: Sender<SimResponse>,
        cli_rx: Receiver<SimRequest>,
        console: SimConsole,
    ) -> Self {
        Self {
            cli_tx,
            cli_rx,
            console,
        }
    }

    pub(crate) fn send(&self, msg: SimResponse) -> Option<()> {
        self.cli_tx.send(msg).ok()
    }
}
