// SPDX-License-Identifier: MIT

//! Formatting utilities for the SIMH CLI, principally default formatters for resources.

//! 32-bit hexadecimal format, with leading "0x" and zero-padded to 8 digits.
pub fn hex32_format(val: u64) -> String {
    format!("{:#08x}", val)
}
