// SPDX-License-Identifier: MIT

//! # PDP-8 Simulated CPU
//!
//! This module defines the PDP-8 processor with its [`CPUTraits`] and [`DeviceTraits`].
//!
//! ## Register State
//!
//! The register state for the PDP-8 is:
//!
//! | Register          | Description                   |
//! | ----------------- | ----------------------------- |
//! | `AC<0:11>`        | Accumulator                   |
//! | `MQ<0:11>`        | Multiplier-quotient           |
//! | `L`               | Link flag                     |
//! | `PC<0:11>`        | Program counter               |
//! | `IF<0:2>`         | Instruction field             |
//! | `IB<0:2>`         | Instruction buffer            |
//! | `DF<0:2>`         | Data field                    |
//! | `UF`              | User flag                     |
//! | `UB`              | User buffer                   |
//! | `SF<0:6>`         | Interrupt save field          |

use sim_core::{
    env::{
        CPUTraits, DeviceAccessor, DeviceHandle, DeviceTraits, Disassembler, SimError, SystemBus,
        CPU_DEVICE_NAME, MEM_RESOURCE_NAME,
    },
    SimResources,
};

use crate::pdp8_defs::{InterruptFlags, PDP8Devices};
use std::{array, fs::File, io::BufReader};

/*=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
 * Constants
 *=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~*/

/// Maximum memory size for PDP-8: 32K words
pub const MAX_MEM_SIZE: usize = 32768;
/// Shift width for bits beyond the normal 12-bit PDP-8 word
pub const FIELD_SHIFT_BITS: usize = 12;
/// Register and memory mask for PDP-8: 12 bits
pub const VALUE_MASK: u16 = 0o7777;
/// Link bit mask (accumulator bit 12)
pub const LINK_MASK: u16 = 0o10000;
/// Memory/data field mask: 3 bits beyond the lower 12.
pub const FIELD_MASK: u16 = 0o70000;
/// Field + address mask
pub const FULL_ADDR_MASK: u16 = 0o77777;
/// Link'ACcumulator mask
pub const LINKACC_MASK: u16 = 0o17777;

/// Size of the PC queue array.
pub const PCQ_SIZE: usize = 64;

/*=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
 * Types
 *=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~*/

/// Extended Arithmetic Element mode
#[derive(Debug, Copy, Clone)]
pub enum EAEMode {
    ModeA,
    ModeB,
}
/// The PDP-8 specific I/O parameters passed to hardware on the Omnibus.
pub struct PDP8IoPayload {
    pub _device: u8, // 6-bit device number from bits 3-8 of the IOT word
    pub _pulse: u16, // 3-bit pulse code from bits 0-3
}

/*=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
 * PDP-8 CPU
 *=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~*/

#[derive(Debug, SimResources)]
pub struct PDP8Processor {
    /// Program Counter: PC (bits 0-14 = full address) and IF (bits 12-14 = instruction field).
    #[resource(name = "PC", bits = 15, fmt = format_with_field,
               descr = "Program Counter (PC) with Instruction Field (IF)",
        get = Ok(self.pc as u64),
        set = self.pc = (val as u16) & FULL_ADDR_MASK
    )]
    #[resource(name = "IF", bits = 3, shift = 12, fmt = field_format,
               descr = "Instruction field (upper 3 bits of PC)")]
    pub pc: u16,

    /// Accumulator: AC (bits 0-11) and L link flag (bit 12).
    ///
    /// # Note
    /// The print format for the PDP-8 accumulator is "L'ACC", combined link and accumulator.
    #[resource(name = "AC", bits = 12, fmt = format_acc,
        get = Ok((self.acc & VALUE_MASK) as u64),
        set = self.acc = (self.acc & LINK_MASK) | ((val as u16) & VALUE_MASK),
        descr = "Accumulator (AC) with Link flag (L) in bit 12"
    )]
    #[resource(name = "L", bits = 1, shift = 12, fmt = format_bit, descr = "Link flag (L)")]
    pub acc: u16,

    /// Multiplier-Quotient register
    #[resource(name = "MQ", bits = 12, fmt = default_format)]
    pub mq: u16,

    /// Front panel switch register
    #[resource(name = "SR", bits = 12, fmt = default_format)]
    pub sr: u16,

    /// Data field (bits 12-14 = DF)
    #[resource(name = "DF", bits = 3, shift = 12, fmt = field_format)]
    pub df: u16,

    /// Instruction buffer (bits 12-14 = IB)
    #[resource(name = "IB", bits = 3, shift = 12, fmt = field_format)]
    pub ib: u16,

    /// Save field: SF is a 7-bit composite of sf_uf (bit 6), sf_if (bits 3-5), sf_df (bits 0-2).
    /// Stored across three fields; the SF resource is anchored on sf_uf with explicit get/set.
    #[resource(name = "SF", bits = 7, fmt = default_format,
        get = Ok(((self.sf_uf as u64) << 6) | ((self.sf_if as u64) << 3) | (self.sf_df as u64)),
        set = {
            self.sf_uf = (val & 0o100) != 0;
            self.sf_if = ((val >> 3) & 0o7) as u16;
            self.sf_df = (val & 0o7) as u16
        }
    )]
    pub sf_uf: bool,
    /// Save field: instruction field (3 bits) — part of the SF composite resource above.
    pub sf_if: u16,
    /// Save field: data field (3 bits) — part of the SF composite resource above.
    pub sf_df: u16,
    /// User mode buffer
    #[resource(name = "UB", bits = 1, fmt = format_bit)]
    pub ub: bool,
    /// User mode flag
    #[resource(name = "UF", bits = 1, fmt = format_bit)]
    pub uf: bool,
    /// EAE shift count
    #[resource(name = "SC", bits = 5, fmt = default_format)]
    pub eae_sc: u16,
    /// EAE GTF flag
    #[resource(name = "GTF", bits = 1, fmt = format_bit)]
    pub eae_gtf: bool,
    /// EAE mode (Mode A / Mode B)
    #[resource(name = "EMODE", bits = 1, fmt = eaemode_format,
        get = Ok(self.emode as u64),
        set = self.emode = if val == 0 { EAEMode::ModeA } else { EAEMode::ModeB }
    )]
    pub emode: EAEMode,

    /// TSC8-75 IR (not CLI-visible)
    pub tsc_ir: u16,
    /// TSC8-75 PC (not CLI-visible)
    pub tsc_pc: u16,
    /// TSC8-75 CDF flag (not CLI-visible)
    pub tsc_cdf: u16,
    /// TSC8-75 enable flag (not CLI-visible)
    pub tsc_enab: bool,

    /// PC queue (used for back-trace)
    #[resource(name = "pcq", read_only, bits = 12, len = PCQ_SIZE, fmt = default_format)]
    pub pcq: [u16; PCQ_SIZE],
    /// PC queue index
    pub pcq_idx: usize,

    /// Currently permitted interrupts (not CLI-visible)
    pub int_enable: InterruptFlags,
    /// Interrupt request flags (not CLI-visible)
    pub int_req: InterruptFlags,

    /// PDP-8 main memory
    #[resource(name = MEM_RESOURCE_NAME, bits = 12, len = MAX_MEM_SIZE, fmt = default_format)]
    pub memory: Box<[u16; MAX_MEM_SIZE]>,
    /// Current memory size in words (not CLI-visible)
    pub mem_size: usize,

    /// PDP-8 Omnibus routing: Hardware ID (0-63) -> DeviceHandle
    pub iot_mapping: [Option<DeviceHandle>; 64],
}

impl PDP8Processor {
    /// Create a new PDP-8 processor instance
    pub fn new() -> Self {
        PDP8Processor {
            pc: 0,
            acc: 0,
            mq: 0,
            df: 0,
            ib: 0,
            sf_if: 0,
            sf_df: 0,
            sf_uf: false,
            emode: EAEMode::ModeA,
            eae_gtf: false,
            eae_sc: 0,
            ub: false,
            uf: false,
            sr: 0,
            tsc_ir: 0,
            tsc_pc: 0,
            tsc_cdf: 0,
            tsc_enab: false,
            pcq: [0; PCQ_SIZE],
            pcq_idx: 0,
            int_enable: InterruptFlags::INIT_ENABLE,
            int_req: InterruptFlags::NO_CIF_PENDING | InterruptFlags::NO_ION_PENDING,
            memory: unsafe { Box::new_zeroed().assume_init() },
            mem_size: MAX_MEM_SIZE,
            iot_mapping: [const { None }; 64],
        }
    }

    /// Mock device setup during testing.
    #[cfg(test)]
    pub fn mock_iot_device(&mut self, hw_id: u16, name: &str, devices: &dyn DeviceAccessor<PDP8Processor>) {
        self.iot_mapping = [const { None }; 64];
        self.iot_mapping[hw_id as usize] = devices.resolve_handle(name);
    }

    /// Increment PC within current field
    #[inline]
    pub fn bump_pc(&mut self) {
        self.pc = (self.pc & FIELD_MASK) | ((self.pc + 1) & VALUE_MASK);
    }

    /// Read from memory (15-bit address)
    #[inline]
    pub fn read(&self, addr: u16) -> u16 {
        let addr = (addr & FULL_ADDR_MASK) as usize;
        if addr < self.mem_size {
            self.memory[addr]
        } else {
            0
        }
    }

    /// Write to memory (15-bit address)
    #[inline]
    pub fn write(&mut self, addr: u16, val: u16) {
        let addr = (addr & FULL_ADDR_MASK) as usize;
        if addr < self.mem_size {
            self.memory[addr] = val & VALUE_MASK;
        }
    }

    /// Save PC to the PC queue
    #[inline]
    fn pcq_entry(&mut self, pc: u16) {
        self.pcq_idx = (self.pcq_idx.wrapping_sub(1)) & (PCQ_SIZE - 1);
        self.pcq[self.pcq_idx] = pc;
    }

    /// Calculate effective address for memory reference instructions (AND, TAD, ISZ, DCA)
    /// Returns full 15-bit address (field + offset)
    #[inline]
    fn calc_ea_memref(&mut self, ir: u16) -> u16 {
        let page_offset = ir & 0o177;
        let is_current_page = (ir & 0o200) != 0;
        let is_indirect = (ir & 0o400) != 0;

        // Calculate direct address (branchless)
        // If current page: use PC<0:4>, else use 0
        let page_base = (self.pc & 0o7600) & (0o7777 * is_current_page as u16);
        let mut ma = (self.pc & FIELD_MASK) | page_base | page_offset;

        // Handle indirect addressing
        if is_indirect {
            // Auto-increment for addresses 0010-0017
            let is_autoindex = (ma & 0o7770) == 0o0010;
            let mut indirect_addr = self.read(ma);

            if is_autoindex {
                indirect_addr = (indirect_addr + 1) & VALUE_MASK;
                self.write(ma, indirect_addr);
            }

            // Final address uses DF
            ma = self.df | indirect_addr;
        }

        ma
    }

    /// Calculate effective address for JMS/JMP instructions
    /// Returns (read_address, 12bit_target_address)
    #[inline]
    fn calc_ea_jmpjms(&mut self, ir: u16) -> (u16, u16) {
        let page_offset = ir & 0o177;
        let is_current_page = (ir & 0o200) != 0;
        let is_indirect = (ir & 0o400) != 0;

        // Calculate full 12-bit direct address (branchless)
        let page_base = (self.pc & 0o7600) & (0o7777 * is_current_page as u16);
        let direct_addr_12bit = page_base | page_offset;

        // Full 15-bit address for reading pointer (includes current IF)
        let ma = (self.pc & FIELD_MASK) | direct_addr_12bit;

        let addr_12bit = if is_indirect {
            let is_autoindex = (ma & 0o7770) == 0o0010;
            let mut indirect_addr = self.read(ma);

            if is_autoindex {
                indirect_addr = (indirect_addr + 1) & VALUE_MASK;
                self.write(ma, indirect_addr);
            }

            indirect_addr
        } else {
            direct_addr_12bit
        };

        (ma, addr_12bit)
    }

    /// Check if an interrupt should occur
    #[inline]
    pub fn should_interrupt(&self) -> bool {
        let flags_ready = self
            .int_req
            .contains(InterruptFlags::ION | InterruptFlags::NO_CIF_PENDING | InterruptFlags::NO_ION_PENDING);

        flags_ready && self.int_req.intersects(self.int_enable)
    }

    /// Handle interrupt
    pub fn handle_interrupt(&mut self) {
        // Disable interrupts
        self.int_req.remove(InterruptFlags::ION);

        // Save state to save field
        self.sf_if = (self.pc >> FIELD_SHIFT_BITS) & 0o7;
        self.sf_df = (self.df >> FIELD_SHIFT_BITS) & 0o7;
        self.sf_uf = self.uf;

        // Save PC to queue and memory location 0
        self.pcq_entry(self.pc);
        self.write(0, self.pc & VALUE_MASK);

        // Jump to location 1 in field 0
        self.pc = 1;

        // Clear all field registers and user mode
        self.ib = 0;
        self.df = 0;
        self.uf = false;
        self.ub = false;

        // Clear interrupt delays
        self.int_req
            .insert(InterruptFlags::NO_CIF_PENDING | InterruptFlags::NO_ION_PENDING);
    }

    /// Execute one instruction
    pub fn execute_instruction(
        &mut self,
        ir: u16,
        sysbus: &mut SystemBus,
        devices: &mut dyn DeviceAccessor<Self>,
    ) -> Result<(), SimError> {
        match (ir >> 9) & 0o7 {
            0o0 => self.op_and(ir),
            0o1 => self.op_tad(ir),
            0o2 => self.op_isz(ir),
            0o3 => self.op_dca(ir),
            0o4 => self.op_jms(ir),
            0o5 => self.op_jmp(ir),
            0o6 => self.op_iot(ir, sysbus, devices),
            0o7 => self.op_opr(ir),
            _ => unreachable!(),
        }
    }

    /*=========================================================================
     * Memory Reference Instructions
     *=======================================================================*/

    /// AND - Logical AND with memory
    #[inline]
    fn op_and(&mut self, ir: u16) -> Result<(), SimError> {
        let ea = self.calc_ea_memref(ir);
        self.acc &= self.read(ea) | LINK_MASK;
        Ok(())
    }

    /// TAD - Two's complement add
    #[inline]
    fn op_tad(&mut self, ir: u16) -> Result<(), SimError> {
        let ea = self.calc_ea_memref(ir);
        self.acc = (self.acc + self.read(ea)) & LINKACC_MASK;
        Ok(())
    }

    /// ISZ - Increment and skip if zero
    #[inline]
    fn op_isz(&mut self, ir: u16) -> Result<(), SimError> {
        let ea = self.calc_ea_memref(ir);
        let val = (self.read(ea) + 1) & VALUE_MASK;
        self.write(ea, val);

        if val == 0 {
            self.bump_pc();
        }

        Ok(())
    }

    /// DCA - Deposit and clear accumulator
    #[inline]
    fn op_dca(&mut self, ir: u16) -> Result<(), SimError> {
        let ea = self.calc_ea_memref(ir);
        self.write(ea, self.acc & VALUE_MASK);
        self.acc &= LINK_MASK;
        Ok(())
    }

    /// JMS - Jump to subroutine
    fn op_jms(&mut self, ir: u16) -> Result<(), SimError> {
        self.pcq_entry(self.pc);

        let (_, addr_12bit) = self.calc_ea_jmpjms(ir);

        // TSC8-75: Always save IR and clear CDF flag in user mode
        if self.uf {
            self.tsc_ir = ir;
            self.tsc_cdf = 0;
        }

        // TSC8-75: If enabled, trap instead of normal JMS
        if self.uf && self.tsc_enab {
            self.tsc_pc = ((self.pc.wrapping_sub(1)) & VALUE_MASK) as u16;
            self.int_req.insert(InterruptFlags::TSC);
        } else {
            // Normal JMS: write return address to target field
            let write_addr = self.ib | addr_12bit;
            self.write(write_addr, self.pc & VALUE_MASK);

            // Update user mode
            self.uf = self.ub;

            // Clear CIF delay
            self.int_req.insert(InterruptFlags::NO_CIF_PENDING);
        }

        // Always update PC (even when trapping)
        self.pc = self.ib | ((addr_12bit + 1) & VALUE_MASK);

        Ok(())
    }

    /// JMP - Jump
    fn op_jmp(&mut self, ir: u16) -> Result<(), SimError> {
        self.pcq_entry(self.pc);

        let (_, addr_12bit) = self.calc_ea_jmpjms(ir);

        // TSC8-75: Always save IR and clear CDF flag in user mode
        if self.uf {
            self.tsc_ir = ir;
            self.tsc_cdf = 0;

            if self.tsc_enab {
                self.tsc_pc = ((self.pc.wrapping_sub(1)) & VALUE_MASK) as u16;
                self.int_req.insert(InterruptFlags::TSC);
            }
        }

        // Update user mode and clear CIF delay
        self.uf = self.ub;
        self.int_req.insert(InterruptFlags::NO_CIF_PENDING);

        // Update PC
        self.pc = self.ib | addr_12bit;

        Ok(())
    }

    /*=========================================================================
     * IOT Instructions
     *=======================================================================*/

    /// IOT - Input/Output Transfer
    fn op_iot(
        &mut self,
        ir: u16,
        sysbus: &mut SystemBus,
        devices: &mut dyn DeviceAccessor<Self>,
    ) -> Result<(), SimError> {
        let dev_id = ((ir >> 3) & 0o77) as usize;
        let pulse = ir & 0o7;

        // Check for user mode violations
        if self.uf {
            self.int_req.insert(InterruptFlags::UF);
            self.tsc_ir = ir;

            // Device 62 (memory extension) sets/clears CDF flag
            if dev_id == 0o62 && pulse == 0o1 {
                self.tsc_cdf = 1;
            } else {
                self.tsc_cdf = 0;
            }

            return Ok(());
        }

        match dev_id {
            0o00 => self.iot_cpu_control(pulse),
            0o10 => self.iot_power_fail(pulse),
            0o20..=0o27 => self.iot_memory_extension(pulse, ir),
            _ => {
                // External IOT routing:
                if let Some(handle) = self.iot_mapping[dev_id] {
                    // 2. O(1) array read from sim_core environment
                    if let Some(dev) = devices.get_device_mut(handle) {
                        dev.execute_io(
                            &PDP8IoPayload {
                                _device: 0,
                                _pulse: pulse,
                            },
                            self,
                            sysbus,
                        )?;
                    }
                }
                Ok(())
            }
        }
    }

    /// IOT Device 00 - CPU Control
    fn iot_cpu_control(&mut self, pulse: u16) -> Result<(), SimError> {
        match pulse {
            0o0 => {
                // SKON - Skip if interrupt on
                if self.int_req.contains(InterruptFlags::ION) {
                    self.bump_pc();
                }
                self.int_req.remove(InterruptFlags::ION);
                Ok(())
            }
            0o1 => {
                // ION - Interrupts on
                self.int_req.insert(InterruptFlags::ION);
                self.int_req.remove(InterruptFlags::NO_ION_PENDING);
                Ok(())
            }
            0o2 => {
                // IOF - Interrupts off
                self.int_req.remove(InterruptFlags::ION);
                self.int_req.insert(InterruptFlags::NO_ION_PENDING);
                Ok(())
            }
            0o3 => {
                // SRQ - Skip on interrupt request
                if self.int_req.intersects(self.int_enable) {
                    self.bump_pc();
                }
                Ok(())
            }
            0o4 => {
                // GTF - Get flags
                self.acc = (self.acc & LINK_MASK)
                    | ((self.acc & LINK_MASK) >> 1)
                    | ((self.eae_gtf as u16) << 10)
                    | ((self.int_req.intersects(self.int_enable) as u16) << 9)
                    | ((self.int_req.contains(InterruptFlags::ION) as u16) << 7)
                    | (self.sf_uf as u16) << 6
                    | (self.sf_if << 3)
                    | self.sf_df;
                Ok(())
            }
            0o5 => {
                // RTF - Restore flags
                self.eae_gtf = (self.acc & 0o2000) != 0;
                self.ub = (self.acc & 0o0100) != 0;
                self.ib = (self.acc & 0o0070) << 9;
                self.df = (self.acc & 0o0007) << 12;
                self.acc = ((self.acc & 0o4000) << 1) | (self.acc & VALUE_MASK);
                self.int_req.insert(InterruptFlags::ION);
                self.int_req.remove(InterruptFlags::NO_CIF_PENDING);
                Ok(())
            }
            0o6 => {
                // SGT - Skip on greater than flag
                if self.eae_gtf {
                    self.bump_pc();
                }
                Ok(())
            }
            0o7 => {
                // CAF - Clear all flags
                self.eae_gtf = false;
                self.emode = EAEMode::ModeA;
                self.int_req = InterruptFlags::NO_CIF_PENDING;
                self.acc = 0;
                // TODO: Reset all devices via bus
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// IOT Device 10 - Power Fail
    fn iot_power_fail(&mut self, pulse: u16) -> Result<(), SimError> {
        match pulse {
            0o1 => Ok(()), // SBE - Skip on battery enable (not implemented)
            0o2 => {
                // SPL - Skip on power low
                if self.int_req.contains(InterruptFlags::PWR) {
                    self.bump_pc();
                }
                Ok(())
            }
            0o3 => {
                // CAL - Clear power low flag
                self.int_req.remove(InterruptFlags::PWR);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// IOT Devices 20-27 - Memory Extension Control
    fn iot_memory_extension(&mut self, pulse: u16, ir: u16) -> Result<(), SimError> {
        let field = ((ir >> 3) & 0o7) << FIELD_SHIFT_BITS;

        match pulse {
            0o1 => {
                // CDF - Change data field
                self.df = field;
                Ok(())
            }
            0o2 => {
                // CIF - Change instruction field
                self.ib = field;
                self.int_req.remove(InterruptFlags::NO_CIF_PENDING);
                Ok(())
            }
            0o3 => {
                // CDF CIF - Change both fields
                self.df = field;
                self.ib = field;
                self.int_req.remove(InterruptFlags::NO_CIF_PENDING);
                Ok(())
            }
            0o4 => {
                // Extended functions
                let subfunc = (ir >> 3) & 0o7;
                match subfunc {
                    0o0 => {
                        // CINT - Clear user mode interrupt
                        self.int_req.remove(InterruptFlags::UF);
                        Ok(())
                    }
                    0o1 => {
                        // RDF - Read data field
                        self.acc |= (self.df >> FIELD_SHIFT_BITS) << 6;
                        Ok(())
                    }
                    0o2 => {
                        // RIF - Read instruction field
                        let if_field = (self.pc >> FIELD_SHIFT_BITS) & 0o7;
                        self.acc |= if_field << 6;
                        Ok(())
                    }
                    0o3 => {
                        // RIB - Read interrupt buffer
                        self.acc |= (self.sf_uf as u16) << 6 | (self.sf_if << 3) | self.sf_df;
                        Ok(())
                    }
                    0o4 => {
                        // RMF - Restore memory fields
                        self.ub = self.sf_uf;
                        self.ib = self.sf_if << FIELD_SHIFT_BITS;
                        self.df = self.sf_df << FIELD_SHIFT_BITS;
                        self.int_req.remove(InterruptFlags::NO_CIF_PENDING);
                        Ok(())
                    }
                    0o5 => {
                        // SINT - Skip on user mode interrupt
                        if self.int_req.contains(InterruptFlags::UF) {
                            self.bump_pc();
                        }
                        Ok(())
                    }
                    0o6 => {
                        // CUF - Clear user flag
                        self.ub = false;
                        self.int_req.remove(InterruptFlags::NO_CIF_PENDING);
                        Ok(())
                    }
                    0o7 => {
                        // SUF - Set user flag
                        self.ub = true;
                        self.int_req.remove(InterruptFlags::NO_CIF_PENDING);
                        Ok(())
                    }
                    _ => Ok(()),
                }
            }
            _ => Ok(()),
        }
    }

    /*=========================================================================
     * OPR Instructions
     *=======================================================================*/

    /// OPR - Operate instruction
    fn op_opr(&mut self, ir: u16) -> Result<(), SimError> {
        if (ir & 0o400) == 0 {
            self.opr_group1(ir)
        } else if (ir & 0o001) == 0 {
            self.opr_group2(ir)
        } else {
            self.opr_group3(ir)
        }
    }

    /// OPR Group 1 - Microcoded register operations
    fn opr_group1(&mut self, ir: u16) -> Result<(), SimError> {
        // Time pulse 1 & 2: Clear and complement
        match (ir >> 4) & 0o17 {
            0o00 => {}                                                             // NOP
            0o01 => self.acc ^= LINK_MASK,                                         // CML
            0o02 => self.acc ^= VALUE_MASK,                                        // CMA
            0o03 => self.acc ^= LINKACC_MASK,                                      // CMA CML
            0o04 => self.acc &= VALUE_MASK,                                        // CLL
            0o05 => self.acc = (self.acc & VALUE_MASK) | LINK_MASK,                // CLL CML (STL)
            0o06 => self.acc = (self.acc ^ VALUE_MASK) & VALUE_MASK,               // CLL CMA
            0o07 => self.acc = ((self.acc ^ VALUE_MASK) & VALUE_MASK) | LINK_MASK, // CLL CMA CML
            0o10 => self.acc &= LINK_MASK,                                         // CLA
            0o11 => self.acc = (self.acc & LINK_MASK) ^ LINK_MASK,                 // CLA CML
            0o12 => self.acc |= VALUE_MASK,                                        // CLA CMA (STA)
            0o13 => self.acc = (self.acc | VALUE_MASK) ^ LINK_MASK,                // CLA CMA CML
            0o14 => self.acc = 0,                                                  // CLA CLL
            0o15 => self.acc = LINK_MASK,                                          // CLA CLL CML
            0o16 => self.acc = VALUE_MASK,                                         // CLA CLL CMA
            0o17 => self.acc = LINKACC_MASK,                                       // CLA CLL CMA CML
            _ => {}
        }

        // Time pulse 3: Increment
        if (ir & 0o001) != 0 {
            self.acc = (self.acc + 1) & LINKACC_MASK;
        }

        // Time pulse 4: Rotate
        match (ir >> 1) & 0o7 {
            0o0 => {} // No rotate
            0o1 => {
                // BSW - Byte swap
                let temp = self.acc & VALUE_MASK;
                self.acc = (self.acc & LINK_MASK) | ((temp >> 6) & 0o77) | ((temp & 0o77) << 6);
            }
            0o2 => {
                // RAL - Rotate accumulator left
                self.acc = ((self.acc << 1) | (self.acc >> 12)) & LINKACC_MASK;
            }
            0o3 => {
                // RTL - Rotate two left
                self.acc = ((self.acc << 2) | (self.acc >> 11)) & LINKACC_MASK;
            }
            0o4 => {
                // RAR - Rotate accumulator right
                self.acc = ((self.acc >> 1) | (self.acc << 12)) & LINKACC_MASK;
            }
            0o5 => {
                // RTR - Rotate two right
                self.acc = ((self.acc >> 2) | (self.acc << 11)) & LINKACC_MASK;
            }
            0o6 => {
                // RAL RAR - Undefined, uses AND path
                self.acc &= (ir | LINK_MASK) & LINKACC_MASK;
            }
            0o7 => {
                // RTL RTR - Undefined, uses address path
                let ma = (self.pc & 0o77600) | (ir & 0o177);
                self.acc = (self.acc & LINK_MASK) | (ma & VALUE_MASK);
            }
            _ => {}
        }

        Ok(())
    }

    /// OPR Group 2 - Skip group
    fn opr_group2(&mut self, ir: u16) -> Result<(), SimError> {
        let reverse_sense = (ir & 0o010) != 0;
        let mut skip = false;

        if !reverse_sense {
            // OR group: skip if ANY condition is true
            if (ir & 0o100) != 0 && (self.acc & 0o4000) != 0 {
                // SMA
                skip = true;
            }
            if (ir & 0o040) != 0 && (self.acc & VALUE_MASK) == 0 {
                // SZA
                skip = true;
            }
            if (ir & 0o020) != 0 && (self.acc & LINK_MASK) != 0 {
                // SNL
                skip = true;
            }
        } else {
            // AND group: skip if ALL conditions are true
            skip = true;
            if (ir & 0o100) != 0 && (self.acc & 0o4000) == 0 {
                // SPA
                skip = false;
            }
            if (ir & 0o040) != 0 && (self.acc & VALUE_MASK) != 0 {
                // SNA
                skip = false;
            }
            if (ir & 0o020) != 0 && (self.acc & LINK_MASK) == 0 {
                // SZL
                skip = false;
            }

            // Unconditional skip if no conditions selected
            if (ir & 0o160) == 0 {
                skip = true;
            }
        }

        if skip {
            self.bump_pc();
        }

        // CLA
        if (ir & 0o200) != 0 {
            self.acc &= LINK_MASK;
        }

        // OSR and HLT
        if (ir & 0o006) != 0 && self.uf {
            // User mode violation
            self.int_req.insert(InterruptFlags::UF);
            self.tsc_ir = ir;
            self.tsc_cdf = 0;
        } else {
            if (ir & 0o004) != 0 {
                // OSR
                self.acc |= self.sr & VALUE_MASK;
            }
            if (ir & 0o002) != 0 {
                // HLT
                return Err(SimError::SimulatorHalt);
            }
        }

        Ok(())
    }

    /// OPR Group 3 - EAE group
    fn opr_group3(&mut self, ir: u16) -> Result<(), SimError> {
        // Check for mode switching instructions (must be decoded first)
        if ir == 0o7431 {
            // SWAB - Switch to Mode B
            self.emode = EAEMode::ModeB;
            // SWAB also performs MQL as part of group 3 standard decoding
            self.mq = self.acc & VALUE_MASK;
            self.acc &= LINK_MASK;
            return Ok(());
        }

        if ir == 0o7447 {
            // SWBA - Switch to Mode A
            self.emode = EAEMode::ModeA;
            self.eae_gtf = false;
            return Ok(());
        }

        // Save original AC before any modifications (needed for SAM, DPIC, DCM)
        let original_ac = self.acc;

        // Standard Group 3 operations

        // CLA (bit 7)
        if (ir & 0o200) != 0 {
            self.acc &= LINK_MASK;
        }

        // MQ operations (bits 6 and 5)
        // Save MQ for potential swap
        let temp_mq = self.mq;

        if (ir & 0o020) != 0 {
            // MQL - AC to MQ, clear AC
            self.mq = self.acc & VALUE_MASK;
            self.acc &= LINK_MASK;
        }

        if (ir & 0o100) != 0 {
            // MQA - OR MQ into AC
            self.acc |= temp_mq;
        }

        // EAE operations (bit 0 set indicates EAE)
        if (ir & 0o001) != 0 {
            self.execute_eae(ir, original_ac)?;
        }

        Ok(())
    }

    /// Execute EAE-specific operations
    fn execute_eae(&mut self, ir: u16, original_ac: u16) -> Result<(), SimError> {
        // Clear GTF in Mode A
        if matches!(self.emode, EAEMode::ModeA) {
            self.eae_gtf = false;
        }

        // Decode EAE operation: ((IR >> 1) & 027)
        let eae_op = (ir >> 1) & 0o27;

        match eae_op {
            0o00 => Ok(()), // NOP

            0o01 => {
                // SCL (Mode A) / ACS (Mode B)
                if matches!(self.emode, EAEMode::ModeB) {
                    self.eae_acs()
                } else {
                    self.eae_scl()
                }
            }

            0o02 => self.eae_muy(), // MUY
            0o03 => self.eae_dvi(), // DVI
            0o04 => self.eae_nmi(), // NMI
            0o05 => self.eae_shl(), // SHL
            0o06 => self.eae_asr(), // ASR
            0o07 => self.eae_lsr(), // LSR

            0o20 => self.eae_sca(), // SCA

            0o21 => {
                // SCA + SCL (Mode A) / DAD (Mode B)
                self.eae_sca()?;
                if matches!(self.emode, EAEMode::ModeB) {
                    self.eae_dad()
                } else {
                    self.eae_scl()
                }
            }

            0o22 => {
                // SCA + MUY (Mode A) / DST (Mode B)
                self.eae_sca()?;
                if matches!(self.emode, EAEMode::ModeB) {
                    self.eae_dst()
                } else {
                    self.eae_muy()
                }
            }

            0o23 => {
                // SCA + DVI (Mode A) / SWBA (Mode B, handled earlier)
                self.eae_sca()?;
                if !matches!(self.emode, EAEMode::ModeB) {
                    self.eae_dvi()
                } else {
                    Ok(())
                }
            }

            0o24 => {
                // SCA + NMI (Mode A) / DPSZ (Mode B)
                self.eae_sca()?;
                if matches!(self.emode, EAEMode::ModeB) {
                    self.eae_dpsz()
                } else {
                    self.eae_nmi()
                }
            }

            0o25 => {
                // SCA + SHL (Mode A) / DPIC (Mode B)
                self.eae_sca()?;
                if matches!(self.emode, EAEMode::ModeB) {
                    self.eae_dpic(original_ac)
                } else {
                    self.eae_shl()
                }
            }

            0o26 => {
                // SCA + ASR (Mode A) / DCM (Mode B)
                self.eae_sca()?;
                if matches!(self.emode, EAEMode::ModeB) {
                    self.eae_dcm(original_ac)
                } else {
                    self.eae_asr()
                }
            }

            0o27 => {
                // SCA + LSR (Mode A) / SAM (Mode B)
                self.eae_sca()?;
                if matches!(self.emode, EAEMode::ModeB) {
                    self.eae_sam(original_ac)
                } else {
                    self.eae_lsr()
                }
            }

            _ => Ok(()), // Undefined operations
        }
    }

    /*=========================================================================
     * EAE Instructions
     *=======================================================================*/

    /// SCA - Shift Counter to AC
    fn eae_sca(&mut self) -> Result<(), SimError> {
        self.acc |= self.eae_sc & 0o37;
        Ok(())
    }

    /// SCL - Step Counter Load (Mode A)
    fn eae_scl(&mut self) -> Result<(), SimError> {
        if matches!(self.emode, EAEMode::ModeA) {
            let ma = self.pc;
            self.eae_sc = (!self.read(ma)) & 0o37;
            self.bump_pc();
        }
        Ok(())
    }

    /// ACS - AC to Shift Counter (Mode B)
    fn eae_acs(&mut self) -> Result<(), SimError> {
        if matches!(self.emode, EAEMode::ModeB) {
            self.eae_sc = self.acc & 0o37;
            self.acc &= LINK_MASK;
        }
        Ok(())
    }

    /// MUY - Multiply
    fn eae_muy(&mut self) -> Result<(), SimError> {
        let ma = self.pc;
        let multiplier = if matches!(self.emode, EAEMode::ModeB) {
            let ptr = self.read(ma);
            let addr = self.df | ptr;
            if (addr & 0o7770) == 0o0010 {
                let val = (self.read(addr) + 1) & VALUE_MASK;
                self.write(addr, val);
                val
            } else {
                self.read(addr)
            }
        } else {
            self.read(ma)
        };

        let product = (self.mq as u32 * multiplier as u32) + (self.acc & VALUE_MASK) as u32;
        self.acc = ((product >> 12) & VALUE_MASK as u32) as u16;
        self.mq = (product & VALUE_MASK as u32) as u16;
        self.eae_sc = 0o014;

        self.bump_pc();
        Ok(())
    }

    /// DVI - Divide
    fn eae_dvi(&mut self) -> Result<(), SimError> {
        let ma = self.pc;
        let divisor = if matches!(self.emode, EAEMode::ModeB) {
            let ptr = self.read(ma);
            let addr = self.df | ptr;
            if (addr & 0o7770) == 0o0010 {
                let val = (self.read(addr) + 1) & VALUE_MASK;
                self.write(addr, val);
                val
            } else {
                self.read(addr)
            }
        } else {
            self.read(ma)
        };

        let dividend_high = (self.acc & VALUE_MASK) as u32;
        let dividend_low = self.mq as u32;

        if dividend_high >= divisor as u32 {
            self.acc |= LINK_MASK;
            self.mq = ((self.mq << 1) | 1) & VALUE_MASK;
            self.eae_sc = 0;
        } else {
            let dividend = (dividend_high << 12) | dividend_low;
            if divisor != 0 {
                self.mq = (dividend / divisor as u32) as u16;
                self.acc = (dividend % divisor as u32) as u16;
            } else {
                self.mq = 0o7777;
                self.acc = 0o7777;
            }
            self.eae_sc = 0o015;
        }

        self.bump_pc();
        Ok(())
    }

    /// NMI - Normalize
    fn eae_nmi(&mut self) -> Result<(), SimError> {
        let mut temp = ((self.acc as u32 & LINKACC_MASK as u32) << 12) | (self.mq as u32);

        self.eae_sc = 0;

        while (temp & 0o077777777) != 0
            && (temp & 0o100000000) == ((temp << 1) & 0o100000000)
            && self.eae_sc < 0o37
        {
            temp <<= 1;
            self.eae_sc += 1;
        }

        self.acc = ((temp >> 12) & LINKACC_MASK as u32) as u16;
        self.mq = (temp & VALUE_MASK as u32) as u16;

        if matches!(self.emode, EAEMode::ModeB) {
            if (self.acc & VALUE_MASK) == 0o4000 && self.mq == 0 {
                self.acc &= LINK_MASK;
            }
        }

        Ok(())
    }

    /// SHL - Shift Left
    fn eae_shl(&mut self) -> Result<(), SimError> {
        let ma = self.pc;
        let shift_count = if matches!(self.emode, EAEMode::ModeA) {
            (self.read(ma) & 0o37) + 1
        } else {
            self.eae_sc & 0o37
        };

        let mut temp = ((self.acc as u32 & VALUE_MASK as u32) << 12) | self.mq as u32;

        if shift_count > 25 {
            temp = 0;
        } else {
            temp <<= shift_count;
        }

        self.acc = (self.acc & LINK_MASK) | ((temp >> 12) & VALUE_MASK as u32) as u16;
        self.mq = (temp & VALUE_MASK as u32) as u16;

        if matches!(self.emode, EAEMode::ModeA) {
            self.eae_sc = 0;
            self.bump_pc();
        } else {
            self.eae_sc = 0o037;
        }

        Ok(())
    }

    /// ASR - Arithmetic Shift Right
    fn eae_asr(&mut self) -> Result<(), SimError> {
        let ma = self.pc;
        let shift_count = if matches!(self.emode, EAEMode::ModeA) {
            (self.read(ma) & 0o37) + 1
        } else {
            self.eae_sc & 0o37
        };

        let ac_val = (self.acc & VALUE_MASK) as u32;
        let mq_val = self.mq as u32;
        let mut temp = (ac_val << 12) | mq_val;

        let is_negative = (temp & 0x00800000) != 0;

        if is_negative {
            temp |= 0xFF000000;
        }

        if matches!(self.emode, EAEMode::ModeB) && shift_count > 0 && shift_count <= 24 {
            self.eae_gtf = ((temp >> (shift_count - 1)) & 1) != 0;
        }

        if shift_count > 25 {
            temp = if is_negative { 0xFFFFFFFF } else { 0 };
        } else {
            temp = ((temp as i32) >> shift_count) as u32;
        }

        self.acc = (self.acc & LINK_MASK) | ((temp >> 12) & VALUE_MASK as u32) as u16;
        self.mq = (temp & VALUE_MASK as u32) as u16;

        if matches!(self.emode, EAEMode::ModeA) {
            self.eae_sc = 0;
            self.bump_pc();
        } else {
            self.eae_sc = 0o037;
        }

        Ok(())
    }

    /// LSR - Logical Shift Right
    fn eae_lsr(&mut self) -> Result<(), SimError> {
        let ma = self.pc;
        let shift_count = if matches!(self.emode, EAEMode::ModeA) {
            (self.read(ma) & 0o37) + 1
        } else {
            self.eae_sc & 0o37
        };

        let mut temp = ((self.acc as u32 & VALUE_MASK as u32) << 12) | self.mq as u32;

        if matches!(self.emode, EAEMode::ModeB) && shift_count > 0 {
            self.eae_gtf = ((temp >> (shift_count - 1)) & 1) != 0;
        }

        if shift_count > 24 {
            temp = 0;
        } else {
            temp >>= shift_count;
        }

        self.acc = (self.acc & LINK_MASK) | ((temp >> 12) & VALUE_MASK as u32) as u16;
        self.mq = (temp & VALUE_MASK as u32) as u16;

        if matches!(self.emode, EAEMode::ModeA) {
            self.eae_sc = 0;
            self.bump_pc();
        } else {
            self.eae_sc = 0o037;
        }

        Ok(())
    }

    /// DAD - Double Precision Add (Mode B only)
    fn eae_dad(&mut self) -> Result<(), SimError> {
        if !matches!(self.emode, EAEMode::ModeB) {
            return Ok(());
        }

        let ma = self.pc;
        let ptr = self.read(ma);
        let mut addr = self.df | ptr;

        if (addr & 0o7770) == 0o0010 {
            let val = (self.read(addr) + 1) & VALUE_MASK;
            self.write(addr, val);
            addr = self.df | val;
        }

        let low_sum = self.mq as u32 + self.read(addr) as u32;
        self.mq = (low_sum & VALUE_MASK as u32) as u16;

        let carry = low_sum >> 12;
        addr = self.df | ((addr + 1) & VALUE_MASK);
        let high_sum = (self.acc & VALUE_MASK) as u32 + self.read(addr) as u32 + carry;
        self.acc = (self.acc & LINK_MASK) | ((high_sum & VALUE_MASK as u32) as u16);

        self.bump_pc();
        Ok(())
    }

    /// DST - Double Precision Store (Mode B only)
    fn eae_dst(&mut self) -> Result<(), SimError> {
        if !matches!(self.emode, EAEMode::ModeB) {
            return Ok(());
        }

        let ma = self.pc;
        let ptr = self.read(ma);
        let mut addr = self.df | ptr;

        if (addr & 0o7770) == 0o0010 {
            let val = (self.read(addr) + 1) & VALUE_MASK;
            self.write(addr, val);
            addr = self.df | val;
        }

        self.write(addr, self.mq);

        addr = self.df | ((addr + 1) & VALUE_MASK);
        self.write(addr, self.acc & VALUE_MASK);

        self.bump_pc();
        Ok(())
    }

    /// DPSZ - Double Precision Skip if Zero (Mode B only)
    fn eae_dpsz(&mut self) -> Result<(), SimError> {
        if !matches!(self.emode, EAEMode::ModeB) {
            return Ok(());
        }

        if ((self.acc | self.mq) & VALUE_MASK) == 0 {
            self.bump_pc();
        }

        Ok(())
    }

    /// DPIC - Double Precision Increment (Mode B only)
    fn eae_dpic(&mut self, original_ac: u16) -> Result<(), SimError> {
        if !matches!(self.emode, EAEMode::ModeB) {
            return Ok(());
        }

        let temp = (original_ac & VALUE_MASK) + 1;
        let carry = if (temp & 0o10000) != 0 { 1 } else { 0 };

        self.mq = temp & VALUE_MASK;
        self.acc = (self.acc & VALUE_MASK) + carry;
        self.acc &= LINKACC_MASK;

        Ok(())
    }

    /// DCM - Double Precision Complement (Mode B only)
    fn eae_dcm(&mut self, original_ac: u16) -> Result<(), SimError> {
        if !matches!(self.emode, EAEMode::ModeB) {
            return Ok(());
        }

        let original_ac_val = original_ac & VALUE_MASK;
        let original_mq_val = self.mq;

        let temp = (-(original_ac_val as i32)) as u16 & VALUE_MASK;
        let carry = if temp == 0 { 1 } else { 0 };

        self.mq = temp;
        self.acc = ((original_mq_val ^ VALUE_MASK) as u32 + carry) as u16;
        self.acc &= LINKACC_MASK;

        Ok(())
    }

    /// SAM - Subtract AC from MQ (Mode B only)
    fn eae_sam(&mut self, original_ac: u16) -> Result<(), SimError> {
        if !matches!(self.emode, EAEMode::ModeB) {
            return Ok(());
        }

        let temp = original_ac & VALUE_MASK;

        self.acc = (self.mq as u32 + (temp ^ VALUE_MASK) as u32 + 1) as u16;
        self.acc &= LINKACC_MASK;

        self.eae_gtf = (temp <= self.mq) ^ (((temp ^ self.mq) >> 11) != 0);

        Ok(())
    }
}

/*=============================================================================
 * Device Traits Implementation
 *===========================================================================*/

impl DeviceTraits<PDP8Processor> for PDP8Processor {
    fn device_name(&self) -> &'static str {
        CPU_DEVICE_NAME
    }

    fn description(&self) -> &'static str {
        "PDP-8 CPU"
    }

    fn device_service(
        &mut self,
        _cpu: &mut PDP8Processor,
        _bus: &mut sim_core::env::SystemBus,
    ) -> Result<(), SimError> {
        // Not strictly needed.
        Ok(())
    }

    fn device_reset(&mut self) -> () {
        self.cpu_reset();
    }
}

/*=============================================================================
 * CPU Traits Implementation
 *===========================================================================*/

impl CPUTraits for PDP8Processor {
    type IoPayload = PDP8IoPayload;

    fn simulate_instruction(
        &mut self,
        sysbus: &mut SystemBus,
        devices: &mut dyn DeviceAccessor<Self>,
    ) -> Result<(), SimError> {
        // Check for interrupts
        if self.should_interrupt() {
            self.handle_interrupt();
        }

        // Fetch instruction
        let ir = self.read(self.pc);
        self.bump_pc();

        // Clear ION delay
        self.int_req.insert(InterruptFlags::NO_ION_PENDING);

        // Execute instruction
        self.execute_instruction(ir, sysbus, devices)
    }

    fn initial_ips_code(&mut self) {
        let mut pc = 0o0100;
        for insn in ["MQA MQL", "ISZ 112", "JMP I 106", "JMP I 106"] {
            match self.assemble(pc, insn) {
                Ok(words) => {
                    self.memory[pc] = words[0];
                    pc += 1
                }
                Err(_) => {
                    // This should never happen since the instructions are hardcoded and known to be valid,
                    // but if it does, panic with a clear message.
                    panic!(
                        "PDP-8: Failed to assemble initial IPS code -- offending instruction: {}",
                        insn
                    );
                }
            }
        }

        // Set up indirect jump target for JMP I 106 (back to 0o0100, the MQA MQL instruction)
        self.memory[0o106] = 0o0100;
        self.pc = 0o0100;
    }

    fn current_pc(&self) -> Option<String> {
        Some(format!("{:04o}", self.pc))
    }

    fn disassemble(&self, address: usize) -> Option<(String, usize)> {
        Some(<PDP8Processor as Disassembler>::disassemble(self, address))
    }

    fn cpu_reset(&mut self) {
        self.pc = 0;
        self.acc = 0;
        self.mq = 0;
        self.sr = 0;
        self.df = 0;
        self.ib = 0;
        self.sf_uf = false;
        self.sf_if = 0;
        self.sf_df = 0;
        self.ub = false;
        self.uf = false;
        self.eae_sc = 0;
        self.eae_gtf = false;
        self.emode = EAEMode::ModeA;
    }

    // Load a RIM or BIN format paper tape file into memory directly.
    fn load_file(&mut self, switches: Vec<char>, path: String) -> Result<(), SimError> {
        let file = File::open(&path).map_err(|e| SimError::IOError(e.to_string()))?;
        let mut reader = BufReader::new(file);

        // Determine format: switch -r or .RIM extension (unless -b is forced)
        let is_rim =
            switches.contains(&'r') || (path.to_uppercase().ends_with(".RIM") && !switches.contains(&'b'));

        if is_rim {
            self.load_rim(&mut reader)?
        } else {
            self.load_bin(&mut reader, switches.contains(&'a'))?
        }

        Ok(())
    }

    /// Wire the devices to [`DeviceHandle`]-s for future [`CPUTraits::execute_io`]
    ///
    /// This is the non-`#[cfg(test)]` version, which wires up the standard Omnibus.
    #[cfg(not(test))]
    fn wire_devices(&mut self, devices: &dyn DeviceAccessor<Self>) {
        self.iot_mapping[PDP8Devices::TTI as usize] = devices.resolve_handle("TTI");
        self.iot_mapping[PDP8Devices::TTO as usize] = devices.resolve_handle("TTO");
        self.iot_mapping[PDP8Devices::RK as usize] = devices.resolve_handle("RK");
    }

    #[cfg(test)]
    fn wire_devices(&mut self, _devices: &dyn DeviceAccessor<Self>) {
        // During testing, the Omnibus mapping is not configured.
    }
}

/*=============================================================================
 * Type Conversions
 *===========================================================================*/

impl From<u64> for EAEMode {
    fn from(mode: u64) -> Self {
        match mode {
            0 => EAEMode::ModeA,
            _ => EAEMode::ModeB,
        }
    }
}

/*=============================================================================
 * Utility Functions
 *===========================================================================*/

pub fn format_bit(val: u64) -> String {
    format!("{}", val & 1)
}

pub fn format_acc(acc: u64) -> String {
    let link = (acc >> FIELD_SHIFT_BITS) & 1;
    let ac = acc & VALUE_MASK as u64;
    format!("{}'{:04o}", link, ac)
}

pub fn format_with_field(val: u64) -> String {
    format!("{:o}'{:04o}", val >> FIELD_SHIFT_BITS, val & VALUE_MASK as u64)
}

pub fn default_format(val: u64) -> String {
    format!("{:04o}", val & VALUE_MASK as u64)
}

pub fn field_format(val: u64) -> String {
    format!("{:o}", val & 0o7)
}

pub fn eaemode_format(mode: u64) -> String {
    match EAEMode::from(mode) {
        EAEMode::ModeA => "Mode A",
        EAEMode::ModeB => "Mode B",
    }
    .to_string()
}

pub fn pdp8_address_format(addr: usize) -> String {
    format!(
        "{:1o}'{:04o}",
        (addr & FIELD_MASK as usize) >> FIELD_SHIFT_BITS,
        addr & VALUE_MASK as usize
    )
}
