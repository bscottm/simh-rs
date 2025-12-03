// SPDX-License-Identifier: MIT

#![allow(rustdoc::private_intra_doc_links)]

mod console;
mod devenv;
mod device_node;
mod disassembler;
mod machine;
mod messages;
mod simenv;
mod simerror;
mod simloop;
mod sysbus;
mod unitenv;

// Re-export commonly used types
pub use console::{new_console, SimConsole};
pub use devenv::DeviceMetadata;
pub use device_node::{Controller, DeviceNode, StandaloneDevice};
pub use disassembler::Disassembler;
pub use machine::{
    AttachmentInfo, AttachmentResource, AttachmentType, CPUTraits, DeviceAccessor, DeviceAttachmentInterface,
    DeviceHandle, DeviceRole, DeviceTraits, NetworkConfig, NetworkInterfaceType, ResourceCLIMetadata,
    ResourceMetadata, SimResourceProvider, CPU_DEVICE_NAME, MEM_RESOURCE_NAME,
};
pub use messages::{ExamineRequest, ExamineResult, SimRequest, SimResponse};
pub use simenv::SimEnvironment;
pub use simerror::SimError;
pub use simloop::run_simulator;
pub use sysbus::SystemBus;
pub use unitenv::UnitMetadata;
