/* Interrupt flags

   The interrupt flags consist of three groups:

   1.   Devices with individual interrupt enables.  These record
        their interrupt requests in device_done and their enables
        in device_enable, and must occupy the low bit positions.

   2.   Devices without interrupt enables.  These record their
        interrupt requests directly in int_req, and must occupy
        the middle bit positions.

   3.   Overhead.  These exist only in int_req and must occupy the
        high bit positions.

   Because the PDP-8 does not have priority interrupts, the order
   of devices within groups does not matter.

   Note: all extra KL input and output interrupts must be assigned
   to contiguous bits.
*/

use bitflags::bitflags;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PDP8Devices {
    PTR = 0o001, // paper tape reader
    PTP = 0o002, // paper tape punch
    TTI = 0o003, // console input
    TTO = 0o004, // console output
    DPY = 0o005, // Type 34 display
    CLK = 0o013, // clock
    TSC = 0o036,
    KJ8 = 0o040,  // extra terminals
    FPP = 0o055,  // floating point
    DF32 = 0o060, // DF32
    // Rf = 0o060,       // RF08 - Note: Rust enums cannot have duplicate discriminant values
    // Rl = 0o060,       // RL8A - Note: See "Handling Duplicates" below
    LPT = 0o066, // line printer
    MT = 0o070,  // TM8E
    // Ct = 0o070,       // TA8E - Note: Duplicate value
    RK = 0o074,   // RK8E
    RX = 0o075,   // RX8E/RX28
    DTA = 0o076,  // TC08
    TD8E = 0o077, // TD8E
}

//  Keep the offsets as internal constants for maintainability
const V_START: u32 = 0;
const V_DIRECT: u32 = V_START + 14;
const V_OVHD: u32 = V_DIRECT + 12;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct InterruptFlags: u32 {
        // --- Device Offsets ---
        const LPT   = 1 << (V_START + 0);
        const PTP   = 1 << (V_START + 1);
        const PTR   = 1 << (V_START + 2);
        const TTO   = 1 << (V_START + 3);
        const TTI   = 1 << (V_START + 4);
        const CLK   = 1 << (V_START + 5);
        const TTO1  = 1 << (V_START + 6);
        const TTI1  = 1 << (V_START + 10);

        // --- Direct Start ---
        const RX    = 1 << (V_DIRECT + 0);
        const RK    = 1 << (V_DIRECT + 1);
        const RF    = 1 << (V_DIRECT + 2);
        const DF    = 1 << (V_DIRECT + 3);
        const MT    = 1 << (V_DIRECT + 4);
        const DTA   = 1 << (V_DIRECT + 5);
        const RL    = 1 << (V_DIRECT + 6);
        const CT    = 1 << (V_DIRECT + 7);
        const PWR   = 1 << (V_DIRECT + 8);
        const UF    = 1 << (V_DIRECT + 9);
        const TSC   = 1 << (V_DIRECT + 10);
        const FPP   = 1 << (V_DIRECT + 11);

        // --- Overhead ---
        const NO_ION_PENDING = 1 << (V_OVHD + 0);
        const NO_CIF_PENDING = 1 << (V_OVHD + 1);
        const ION            = 1 << (V_OVHD + 2);

        // --- Composite Masks ---
        const DEV_ENABLE = (1 << V_DIRECT) - 1;
        const ALL        = (1 << V_OVHD) - 1;

        const INIT_ENABLE = Self::TTI.bits()
                          | Self::TTO.bits()
                          | Self::PTR.bits()
                          | Self::PTP.bits()
                          | Self::LPT.bits()
                          | Self::TTI1.bits()
                          | Self::TTO1.bits();

        const PENDING = Self::ION.bits()
                      | Self::NO_CIF_PENDING.bits()
                      | Self::NO_ION_PENDING.bits();

        // Test device for unit testing (a device that only exists during testing, to avoid conflicts with
        // real devices and to allow testing of interrupt handling logic without needing to trigger real
        // device interrupts).
        #[cfg(test)]
        const TEST_DEVICE = 0o0100;
    }
}

impl InterruptFlags {
    // TODO: DELETE if not ever used.

    // / Returns the bit position (0-31) of a single interrupt flag.
    // / If multiple flags are set, it returns the index of the lowest bit.
    // / Returns None if no bits are set.
    #[allow(dead_code)]
    pub fn vector_index(&self) -> Option<u32> {
        if self.is_empty() {
            None
        } else {
            // .bits() gets the raw u32, trailing_zeros() is a CPU-level instruction
            Some(self.bits().trailing_zeros())
        }
    }
}
