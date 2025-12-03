// SPDX-License-Identifier: MIT

//! Example: RK05 Disk Controller and Units

use std::path::PathBuf;

use sim_core::{
    env::{
        AttachmentInfo, AttachmentResource, AttachmentType, DeviceAttachmentInterface, DeviceTraits,
        SimError, SystemBus,
    },
    SimResources,
};

use crate::cpu::{default_format, PDP8Processor};

//=================================================================================================
// CONTROLLER
//=================================================================================================

#[derive(Debug, Clone, SimResources)]
pub struct RKController {
    #[resource(name = "CSR",  bits = 16, fmt = default_format)]
    pub csr: u16,
    #[resource(name = "DATA", bits = 16, fmt = default_format)]
    pub data: u16,
    #[resource(name = "CMD",  bits = 16, fmt = default_format)]
    pub command: u16,
}

impl Default for RKController {
    fn default() -> Self {
        Self {
            csr: 0,
            data: 0,
            command: 0,
        }
    }
}

impl RKController {
    pub fn new() -> Self {
        Self::default()
    }
}

impl DeviceTraits<PDP8Processor> for RKController {
    fn device_name(&self) -> &str {
        "RK"
    }
    fn description(&self) -> &str {
        "RK05 disk controller"
    }

    fn device_service(&mut self, _cpu: &mut PDP8Processor, _bus: &mut SystemBus) -> Result<(), SimError> {
        // FIXME. This is where we dispatch or service a particular unit.
        Ok(())
    }

    fn device_reset(&mut self) -> () {
        self.csr = 0;
        self.data = 0;
        self.command = 0;
    }
}

//=================================================================================================
// UNIT
//=================================================================================================

#[derive(Debug, Clone, SimResources)]
pub struct RKUnit {
    #[resource(name = "CYL",    bits = 16, fmt = default_format)]
    pub cylinder: u16,
    #[resource(name = "HEAD", bits = 8)]
    pub head: u8,
    #[resource(name = "SECTOR", bits = 8)]
    pub sector: u8,

    unit_name: String,
    unit_num: u8,
    attached_file: Option<PathBuf>,
    read_only: bool,
}

impl RKUnit {
    pub fn new(unit_name: String, unit_num: u8) -> Self {
        Self {
            cylinder: 0,
            head: 0,
            sector: 0,
            unit_name,
            unit_num,
            attached_file: None,
            read_only: false,
        }
    }
}

impl DeviceTraits<PDP8Processor> for RKUnit {
    fn device_name(&self) -> &str {
        &self.unit_name
    }
    fn description(&self) -> &str {
        "RK05 disk unit"
    }

    fn device_service(&mut self, _cpu: &mut PDP8Processor, _bus: &mut SystemBus) -> Result<(), SimError> {
        // FIXME. This is where we dispatch or service a particular unit.
        Ok(())
    }

    fn device_reset(&mut self) -> () {
        self.cylinder = 0;
        self.head = 0;
        self.sector = 0;
    }

    fn attachment(&self) -> Option<&dyn DeviceAttachmentInterface> {
        Some(self)
    }
    fn attachment_mut(&mut self) -> Option<&mut dyn DeviceAttachmentInterface> {
        Some(self)
    }
}

impl DeviceAttachmentInterface for RKUnit {
    fn attach(&mut self, resource: AttachmentResource) -> Result<(), SimError> {
        match resource {
            AttachmentResource::File { path, read_only, .. } => {
                self.attached_file = Some(PathBuf::from(path));
                self.read_only = read_only;
                Ok(())
            }
            _ => Err(SimError::UnsupportedAttachment),
        }
    }

    fn detach(&mut self) -> Result<(), SimError> {
        self.attached_file = None;
        Ok(())
    }

    fn is_attached(&self) -> bool {
        self.attached_file.is_some()
    }

    fn attachment_info(&self) -> Option<AttachmentInfo> {
        self.attached_file.as_ref().map(|path| AttachmentInfo {
            attachment_type: AttachmentType::File,
            description: format!("Attached to: {}", path.display()),
            read_only: self.read_only,
            size: Some(1_228_800),
        })
    }

    fn supported_attachments(&self) -> &[AttachmentType] {
        &[AttachmentType::File]
    }
}
