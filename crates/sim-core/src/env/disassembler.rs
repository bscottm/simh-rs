// SPDX-License-Identifier: MIT

//! Assembler / disassembler interface.
//!
//! [`Disassembler`] is an optional trait implemented by a CPU that knows how to
//! render its instruction set in symbolic form and parse it back.  It is separate
//! from [`crate::env::DeviceTraits`] and [`crate::env::CPUTraits`] because not
//! every CPU implementation needs it, and it is purely a display/input concern
//! with no effect on simulation correctness.
//!
//! # Interface design
//!
//! `disassemble` takes a program counter value and reads words directly from
//! the CPU's own memory.  It returns the disassembled text **and the next PC**
//! (i.e. the address of the following instruction).  Returning the next PC rather
//! than "extra words consumed" is the right abstraction for variable-length ISAs
//! such as the PDP-11 (where instructions may carry one or two extension words) or
//! the VAX (where instruction lengths depend on operand specifiers).  For fixed-
//! width ISAs like the PDP-8 the next PC is simply `address + 1`.
//!
//! `assemble` is the inverse: given a symbolic instruction string and the current
//! PC it produces one or more machine words.  The associated `type Word` is the
//! native machine word size (`u16` for PDP-8/11, `u32` for VAX, etc.) and is
//! used only by `assemble` — `disassemble` reads from the CPU's own memory
//! and needs no external slice.
//!
//! # Integration with EXAMINE / DEPOSIT
//!
//! The examine command uses `disassemble` when the `-m` (mnemonic) switch is
//! active.  The simulator calls `CPUTraits::disassemble` (the object-safe
//! bridge) in a loop, advancing the address by the returned next-PC each
//! iteration until the requested range is covered.
//!
//! The deposit command, when mnemonic input is detected, calls `assemble` and
//! converts the returned `Vec<Self::Word>` to `u64` for `write_resource`.

/// Assembler / disassembler interface for a simulated CPU.
///
/// `type Word` is the native machine word — `u16` for the PDP-8 and PDP-11,
/// `u32` for the VAX, etc.  Using a concrete associated type instead of always
/// passing `u64` catches mismatches at compile time and lets the implementation
/// work with typed slices without intermediate conversions.
pub trait Disassembler {
    /// Native machine word type used by [`Self::assemble`].
    ///
    /// `u16` for the PDP-8 and PDP-11, `u32` for the VAX, etc.
    type Word: Copy;

    /// Disassemble the instruction at `address`, reading from the CPU's own memory.
    ///
    /// # Returns
    /// `(text, next_pc)` where `text` is the formatted instruction and `next_pc`
    /// is the address of the next instruction.  For single-word ISAs `next_pc` is
    /// always `address + 1`; for variable-length ISAs it may be larger.
    fn disassemble(&self, address: usize) -> (String, usize);

    /// Assemble one instruction string at `address`.
    ///
    /// `address` is needed for memory-reference instructions that use
    /// page-relative or PC-relative addressing  (e.g. PDP-8 current-page vs. page-zero)..
    ///
    /// # Returns
    /// `Ok(words)` with one or more machine words, or `Err(message)` describing
    /// the parse failure.
    fn assemble(&self, address: usize, source: &str) -> Result<Vec<Self::Word>, String>;
}
