// SPDX-License-Identifier: MIT

use thiserror::Error;

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// General-purpose simulator error enumeration.
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
/// Simulator errors, patterned off SIMH's "SCPE_" error codes.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum SimError {
    /// Simulator halted
    #[error("Simulator halted")]
    SimulatorHalt,

    /// Was NXM
    #[error("Address space exceeded or out of bounds")]
    AddressBounds,

    /// Unit not attached to Device
    #[error("Unit not attached to a device")]
    UnitUnattached,

    /// I/O error wrapper
    #[error("I/O error: {0}")]
    IOError(String),

    /// File creation error.
    #[error("Cannot create file: {0}")]
    CannotCreateFile(String),

    /// Duplicate device name
    #[error("Attempted to add a duplicate device named {0}")]
    DuplicateDevice(String),

    /// Resource not found
    #[error("No such resource named {0}")]
    NoSuchResource(String),

    /// Ambiguous resource -- unique resource expected.
    #[error("Resource {0} has multiple device associations")]
    AmbiguousResource(String),

    /// Read-only resource
    #[error("{0} is a read-only resource")]
    ReadOnlyResource(String),

    /// Value range error.
    #[error("Value out of range for {resource}: {value} (restricted to {bits} bits)")]
    ValueOutOfRange {
        value: u64,
        bits: usize,
        resource: String,
    },

    /// Non-existent device
    #[error("No such device {0}")]
    DeviceNotFound(String),

    /// Non-existent unit
    #[error("No such unit {1} on device {0}")]
    UnitNotFound(String, usize),

    /// Device doesn't support attachments
    #[error("Device does not support attachments.")]
    UnsupportedAttachment,

    /// Device has no units.
    #[error("Device {0} has no units")]
    NoDeviceUnits(String),

    /// Format error (e.g. invalid file format)
    #[error("Format error: {0}")]
    FormatError(String),
}

// Convert [`std::io::Error`] to [`SimError`]
impl From<std::io::Error> for SimError {
    fn from(err: std::io::Error) -> Self {
        SimError::IOError(err.to_string())
    }
}
