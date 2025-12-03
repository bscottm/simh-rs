// SPDX-License-Identifier: MIT

//! PDP-8 assembler and disassembler.
//!
//! Implements the [`Disassembler`] trait for [`PDP8Processor`], matching SIMH's
//! `fprint_sym` / `parse_sym` output format from `pdp8_sys.c`.
//!
//! # Instruction classes
//!
//! The PDP-8 instruction set partitions naturally into classes driven by bits
//! 9-11 (the major opcode):
//!
//! | Opcode | Class        | Notes                                      |
//! |--------|--------------|--------------------------------------------|
//! | 0-5    | Memory ref   | AND TAD ISZ DCA JMS JMP                    |
//! | 6      | IOT          | device + pulse bits                        |
//! | 7      | Operate      | groups 1/2/3 selected by bits 8 and 0      |
//!
//! IOT instructions with well-known device numbers are printed by mnemonic;
//! unknown IOTs fall back to `IOT ddd` (octal device+pulse).
//!
//! Operate instructions are additive — multiple micro-operations are ORed
//! together and printed as a space-separated list (e.g. `CLA CLL CMA RAL`).
//!
//! # Addressing
//!
//! Memory-reference instructions encode addressing in bits 7-8:
//! - Bit 8 = 0: page zero  (`0000`-`0177`)
//! - Bit 8 = 1: current page (7-bit displacement ORed with `addr & 07600`)
//! - Bit 9 = indirect flag → `I` printed between opcode and address
//!
//! The disassembler always prints absolute addresses.
//! The assembler accepts absolute addresses and resolves them to page-zero or
//! current-page automatically; it returns an error if the address is on neither.

use crate::cpu::PDP8Processor;
use sim_core::env::Disassembler;

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
// Instruction class flags (packed into bits above the 12-bit opcode word)
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

#[derive(Debug, Clone, Copy, PartialEq)]
enum Class {
    /// No operand (NPN): exact-match IOTs, standalone pseudo-ops
    Npn,
    /// Field-change instructions: CDF, CIF, CIF CDF
    Fld,
    /// Memory-reference: AND TAD ISZ DCA JMS JMP
    Mrf,
    /// Generic IOT fallback (printed as `IOT ddd`)
    Iot,
    /// Operate group 1 (bit 11 = 1, bit 8 = 0, bit 0 = 0)
    Op1,
    /// Operate group 2 (bit 11 = 1, bit 8 = 1, bit 0 = 0)
    Op2,
    /// Operate group 3 (bit 11 = 1, bit 0 = 1)  — EAE
    Op3,
}

// Masks applied to the raw word before comparing against an opcode entry's
// value.  Order matches the Class enum variants above.
const MASKS: [u16; 7] = [
    0o7777, // Npn — exact match
    0o7707, // Fld — mask out field bits
    0o7000, // Mrf — opcode only
    0o7000, // Iot — opcode only
    0o7400, // Op1
    0o7411, // Op2
    0o7401, // Op3 (17-bit with EAE mode in bit 12)
];

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
// Opcode table
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

struct OpcodeEntry {
    mnemonic: &'static str,
    /// 12-bit (or 13-bit for EAE) opcode value
    value: u16,
    class: Class,
    /// If true, only used for encoding (assemble), not decoding (disassemble)
    encode_only: bool,
}

macro_rules! op {
    ($m:expr, $v:expr, $c:ident) => {
        OpcodeEntry {
            mnemonic: $m,
            value: $v,
            class: Class::$c,
            encode_only: false,
        }
    };
    ($m:expr, $v:expr, $c:ident, enc) => {
        OpcodeEntry {
            mnemonic: $m,
            value: $v,
            class: Class::$c,
            encode_only: true,
        }
    };
}

/// Opcode table — order matters for disassembly (first match wins for NPN/MRF/FLD;
/// all matching entries are accumulated for OP1/OP2/OP3).
///
/// Ambiguous IOT devices (RL, CT, TD) are omitted here; they would require
/// runtime device-enable state.  They fall through to the generic `IOT ddd` path.
static OPCODES: &[OpcodeEntry] = &[
    //── Standard IOTs (NPN — exact match) ──────────────────────────────────
    op!("SKON", 0o6000, Npn),
    op!("ION", 0o6001, Npn),
    op!("IOF", 0o6002, Npn),
    op!("SRQ", 0o6003, Npn),
    op!("GTF", 0o6004, Npn),
    op!("RTF", 0o6005, Npn),
    op!("SGT", 0o6006, Npn),
    op!("CAF", 0o6007, Npn),
    //── Paper tape reader/punch ─────────────────────────────────────────────
    op!("RPE", 0o6010, Npn),
    op!("RSF", 0o6011, Npn),
    op!("RRB", 0o6012, Npn),
    op!("RFC", 0o6014, Npn),
    op!("RFC RRB", 0o6016, Npn),
    op!("PCE", 0o6020, Npn),
    op!("PSF", 0o6021, Npn),
    op!("PCF", 0o6022, Npn),
    op!("PPC", 0o6024, Npn),
    op!("PLS", 0o6026, Npn),
    //── Console TTY ─────────────────────────────────────────────────────────
    op!("KCF", 0o6030, Npn),
    op!("KSF", 0o6031, Npn),
    op!("KCC", 0o6032, Npn),
    op!("KRS", 0o6034, Npn),
    op!("KIE", 0o6035, Npn),
    op!("KRB", 0o6036, Npn),
    op!("TLF", 0o6040, Npn),
    op!("TSF", 0o6041, Npn),
    op!("TCF", 0o6042, Npn),
    op!("TPC", 0o6044, Npn),
    op!("SPI", 0o6045, Npn),
    op!("TLS", 0o6046, Npn),
    //── Memory management ───────────────────────────────────────────────────
    op!("CINT", 0o6204, Npn),
    op!("RDF", 0o6214, Npn),
    op!("RIF", 0o6224, Npn),
    op!("RIB", 0o6234, Npn),
    op!("RMF", 0o6244, Npn),
    op!("SINT", 0o6254, Npn),
    op!("CUF", 0o6264, Npn),
    op!("SUF", 0o6274, Npn),
    //── RK8E disk ───────────────────────────────────────────────────────────
    op!("DSKP", 0o6741, Npn),
    op!("DCLR", 0o6742, Npn),
    op!("DLAG", 0o6743, Npn),
    op!("DLCA", 0o6744, Npn),
    op!("DRST", 0o6745, Npn),
    op!("DLDC", 0o6746, Npn),
    op!("DMAN", 0o6747, Npn),
    //── RX8E/RX28 floppy ────────────────────────────────────────────────────
    op!("LCD", 0o6751, Npn),
    op!("XDR", 0o6752, Npn),
    op!("STR", 0o6753, Npn),
    op!("SER", 0o6754, Npn),
    op!("SDN", 0o6755, Npn),
    op!("INTR", 0o6756, Npn),
    op!("INIT", 0o6757, Npn),
    //── Line printer ────────────────────────────────────────────────────────
    op!("PSKF", 0o6661, Npn),
    op!("PCLF", 0o6662, Npn),
    op!("PSKE", 0o6663, Npn),
    op!("PSTB", 0o6664, Npn),
    op!("PSIE", 0o6665, Npn),
    op!("PCLF PSTB", 0o6666, Npn),
    op!("PCIE", 0o6667, Npn),
    //── TSC8-75 ─────────────────────────────────────────────────────────────
    op!("ETDS", 0o6360, Npn),
    op!("ESKP", 0o6361, Npn),
    op!("ECTF", 0o6362, Npn),
    op!("ECDF", 0o6363, Npn),
    op!("ERTB", 0o6364, Npn),
    op!("ESME", 0o6365, Npn),
    op!("ERIOT", 0o6366, Npn),
    op!("ETEN", 0o6367, Npn),
    //── Field change ────────────────────────────────────────────────────────
    op!("CDF", 0o6201, Fld),
    op!("CIF", 0o6202, Fld),
    op!("CIF CDF", 0o6203, Fld),
    //── Memory reference ────────────────────────────────────────────────────
    op!("AND", 0o0000, Mrf),
    op!("TAD", 0o1000, Mrf),
    op!("ISZ", 0o2000, Mrf),
    op!("DCA", 0o3000, Mrf),
    op!("JMS", 0o4000, Mrf),
    op!("JMP", 0o5000, Mrf),
    //── Generic IOT fallback ─────────────────────────────────────────────────
    op!("IOT", 0o6000, Iot),
    //── NOP variants ────────────────────────────────────────────────────────
    op!("NOP", 0o7000, Op1),
    op!("NOP2", 0o7400, Op2),
    op!("NOP3", 0o7401, Op3),
    //── Composite operate pseudo-ops (encode only) ───────────────────────────
    op!("STL", 0o7120, Npn, enc), // CLL IAC
    op!("GLK", 0o7204, Npn, enc), // CLA RAL
    op!("STA", 0o7240, Npn, enc), // CLA CMA
    op!("LAS", 0o7604, Npn, enc), // CLA OAS
    op!("CIA", 0o7041, Npn, enc), // CMA IAC
    //── Operate group 1 micro-ops ────────────────────────────────────────────
    op!("CLA", 0o7200, Op1),
    op!("CLL", 0o7100, Op1),
    op!("CMA", 0o7040, Op1),
    op!("CML", 0o7020, Op1),
    op!("RTR", 0o7012, Op1), // BSW + RAR -> Rotate right twice
    op!("RAR", 0o7010, Op1),
    op!("RTL", 0o7006, Op1), // BSW + RAL -> Rotate left twice
    op!("IAC", 0o7001, Op1),
    op!("BSW", 0o7002, Op1),
    op!("RAL", 0o7004, Op1),
    //── Operate group 2 ──────────────────────────────────────────────────────
    // skip conditions (mutually exclusive groups printed by first match)
    op!("CLA", 0o7600, Op2),
    op!("SNA", 0o7450, Op2),
    op!("SMA", 0o7500, Op2),
    op!("SZA", 0o7440, Op2),
    op!("SNL", 0o7420, Op2),
    op!("SKP", 0o7410, Op2),
    op!("SZL", 0o7430, Op2),
    op!("SPA", 0o7510, Op2),
    op!("OAS", 0o7404, Op2),
    op!("HLT", 0o7402, Op2),
    //── Operate group 3 / EAE ────────────────────────────────────────────────
    op!("CLA", 0o7601, Op3),
    op!("MQA", 0o7501, Op3),
    op!("MQL", 0o7421, Op3),
    op!("SCA", 0o7441, Op3),
    op!("SCL", 0o7403, Op3),
    op!("MUY", 0o7405, Op3),
    op!("DVI", 0o7407, Op3),
    op!("ASR", 0o7415, Op3), // More specific than NMI
    op!("LSR", 0o7417, Op3), // More specific than NMI
    op!("SHL", 0o7413, Op3), // More specific than NMI
    op!("NMI", 0o7411, Op3),
    op!("DAD", 0o7443, Op3),
    op!("DST", 0o7445, Op3),
    op!("DPSZ", 0o7451, Op3),
    op!("DPIC", 0o7453, Op3),
    op!("DCM", 0o7455, Op3),
    op!("SAM", 0o7457, Op3),
    // Mode B variants (bit 12 set, requires emode=1 in CPU state)
    op!("SCA SCL", 0o17403, Op3),
    op!("SCA MUY", 0o17405, Op3),
    op!("SCA DVI", 0o17407, Op3),
    op!("SCA NMI", 0o17411, Op3),
    op!("SCA SHL", 0o17413, Op3),
    op!("SCA ASR", 0o17415, Op3),
    op!("SCA LSR", 0o17417, Op3),
];

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
// Disassembler implementation
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

impl Disassembler for PDP8Processor {
    type Word = u16;

    fn disassemble(&self, address: usize) -> (String, usize) {
        let raw = self.memory.get(address).copied().unwrap_or(0) & 0o7777;
        // For operate instructions, include EAE mode bit (bit 12) so that
        // Mode B EAE mnemonics are selected correctly.
        let inst_with_mode = raw | (((self.emode as u16) & 1) << 12);
        let major = (raw >> 9) & 0o7;
        let next_pc = address + 1;

        match major {
            // ── Memory reference (opcodes 0-5) ──────────────────────────────
            0o0..=0o5 => {
                let indirect = (raw & 0o0400) != 0;
                let cur_page = (raw & 0o0200) != 0;
                let disp = raw & 0o0177;
                let eff_addr = if cur_page {
                    (address & 0o7600) | disp as usize
                } else {
                    disp as usize
                };
                let opcode_base = raw & 0o7000;
                let mnemonic = OPCODES
                    .iter()
                    .find(|e| e.class == Class::Mrf && e.value == opcode_base)
                    .map(|e| e.mnemonic)
                    .unwrap_or("???");
                let indirect_str = if indirect { " I" } else { "" };
                (format!("{}{} {:04o}", mnemonic, indirect_str, eff_addr), next_pc)
            }

            // ── IOT (opcode 6) ───────────────────────────────────────────────
            0o6 => {
                // Try exact NPN match first
                if let Some(e) = OPCODES
                    .iter()
                    .find(|e| e.class == Class::Npn && e.value == raw && !e.encode_only)
                {
                    return (e.mnemonic.to_string(), next_pc);
                }
                // Field-change instructions
                if let Some(e) = OPCODES
                    .iter()
                    .find(|e| e.class == Class::Fld && e.value == (raw & 0o7707))
                {
                    let field = (raw >> 3) & 0o7;
                    return (format!("{} {}", e.mnemonic, field), next_pc);
                }
                // Generic IOT fallback: show device (bits 3-8) and pulse (bits 0-2)
                (format!("IOT {:03o}", raw & 0o0777), next_pc)
            }

            // ── Operate (opcode 7) ───────────────────────────────────────────
            _ => (disassemble_operate(inst_with_mode), next_pc),
        }
    }

    fn assemble(&self, address: usize, source: &str) -> Result<Vec<u16>, String> {
        let source = source.trim().to_ascii_uppercase();
        if source.is_empty() {
            return Err("Empty input".to_string());
        }

        let mut tokens = source.split_whitespace().peekable();
        let first = tokens.next().ok_or("Empty input")?;

        // ── Exact NPN match ─────────────────────────────────────────────────
        if let Some(e) = OPCODES
            .iter()
            .find(|e| e.class == Class::Npn && e.mnemonic == first)
        {
            expect_end(&mut tokens.collect::<Vec<_>>().join(" "))?;
            return Ok(vec![e.value]);
        }

        // ── Field change ────────────────────────────────────────────────────
        if let Some(e) = OPCODES.iter().find(|e| {
            e.class == Class::Fld && {
                // mnemonic is multi-word e.g. "CIF CDF"; check only first token
                e.mnemonic.split_whitespace().next() == Some(first)
            }
        }) {
            let field = parse_octal_field(&mut tokens, 7)?;
            return Ok(vec![e.value | (field << 3)]);
        }

        // ── Memory reference ────────────────────────────────────────────────
        if let Some(e) = OPCODES
            .iter()
            .find(|e| e.class == Class::Mrf && e.mnemonic == first)
        {
            let rest: Vec<&str> = tokens.collect();
            let (indirect, addr_str) = if rest.first().map(|&s| s) == Some("I") {
                (true, &rest[1..])
            } else {
                (false, &rest[..])
            };

            if addr_str.is_empty() {
                return Err("Missing address".to_string());
            }
            let addr = parse_octal(addr_str[0]).ok_or_else(|| format!("Invalid address: {}", addr_str[0]))?;

            let word = encode_mrf(e.value, address, addr as usize, indirect)?;
            return Ok(vec![word]);
        }

        // ── Generic IOT ─────────────────────────────────────────────────────
        if first == "IOT" {
            let dev_pulse: Vec<&str> = tokens.collect();
            if dev_pulse.is_empty() {
                return Err("IOT requires device+pulse operand".to_string());
            }
            let operand =
                parse_octal(dev_pulse[0]).ok_or_else(|| format!("Invalid IOT operand: {}", dev_pulse[0]))?;
            if operand > 0o777 {
                return Err(format!("IOT operand out of range: {:o}", operand));
            }
            return Ok(vec![0o6000 | operand]);
        }

        // ── Operate ─────────────────────────────────────────────────────────
        // Accumulate micro-ops; all must belong to the same group.
        assemble_operate(first, &mut source[first.len()..].trim())
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
// Operate disassembly
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

fn disassemble_operate(inst: u16) -> String {
    let group = if inst & 0o0001 != 0 {
        Class::Op3
    } else if inst & 0o0400 != 0 {
        Class::Op2
    } else {
        Class::Op1
    };

    let mask = MASKS[group as usize];
    let prefix = inst & mask;
    let mut suffix = inst & !mask;
    let mut parts: Vec<&'static str> = Vec::new();

    for e in OPCODES.iter().filter(|e| e.class == group && !e.encode_only) {
        // There are composite entries in the table that have to match EXACTLY (e.g., RTL, which is BSW + RAL).
        // Ensure that the suffix bits are non-zero and match the entry exactly.
        if (e.value & mask) == prefix && (e.value & suffix) != 0 && (e.value & suffix) == (e.value & !mask) {
            parts.push(e.mnemonic);
            suffix &= !e.value; // clear matched bits
        }
    }

    if parts.is_empty() {
        // NOP / NOP2 / NOP3
        let nop = match group {
            Class::Op1 => "NOP",
            Class::Op2 => "NOP2",
            Class::Op3 => "NOP3",
            _ => unreachable!(),
        };
        nop.to_string()
    } else {
        parts.join(" ")
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
// Operate assembly
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

fn assemble_operate(first: &str, rest: &str) -> Result<Vec<u16>, String> {
    // Collect all tokens
    let mut all_tokens: Vec<&str> = vec![first];
    all_tokens.extend(rest.split_whitespace());

    // Determine the group from non-CLA operations
    // (CLA is ambiguous across all three groups)
    let mut determined_group: Option<Class> = None;

    for token in &all_tokens {
        if *token != "CLA" {
            let entry = OPCODES
                .iter()
                .find(|e| matches!(e.class, Class::Op1 | Class::Op2 | Class::Op3) && e.mnemonic == *token)
                .ok_or_else(|| format!("Unknown opcode: {}", token))?;

            match determined_group {
                Some(prev_group) if prev_group != entry.class => {
                    return Err(format!("'{}' is in a different operate group", token));
                }
                _ => determined_group = Some(entry.class),
            }
        }
    }

    // Default to group 1 if only CLA or no operations
    let group = determined_group.unwrap_or(Class::Op1);

    // Accumulate all operations using the determined group
    let mut word = 0u16;
    for token in &all_tokens {
        let entry = OPCODES
            .iter()
            .find(|e| e.class == group && e.mnemonic == *token)
            .ok_or_else(|| format!("'{}' is not a valid {} micro-op", token, group_name(group)))?;

        if (word & entry.value) == entry.value {
            return Err(format!("Duplicate micro-op: {}", token));
        }
        word |= entry.value;
    }

    Ok(vec![word])
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
// Memory-reference encoding
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

/// Encode a memory-reference instruction.
///
/// Resolves `target` to page-zero (0-177 octal) or current-page
/// (`addr & 07600 | disp`) addressing, returning an error if neither applies.
fn encode_mrf(opcode: u16, pc: usize, target: usize, indirect: bool) -> Result<u16, String> {
    let indirect_bit: u16 = if indirect { 0o0400 } else { 0 };

    // Page zero: target in 0-0177
    if target <= 0o0177 {
        return Ok(opcode | indirect_bit | target as u16);
    }

    // Current page: target on same page as pc
    let page_base = pc & 0o7600;
    if (target & 0o7600) == page_base {
        let disp = (target & 0o0177) as u16;
        return Ok(opcode | indirect_bit | 0o0200 | disp);
    }

    Err(format!(
        "Address {:04o} is not on page zero or current page ({:04o}-{:04o})",
        target,
        page_base,
        page_base + 0o177
    ))
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
// Parsing helpers
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

fn parse_octal(s: &str) -> Option<u16> {
    u16::from_str_radix(s, 8).ok()
}

fn parse_octal_field<'a, I: Iterator<Item = &'a str>>(tokens: &mut I, max: u16) -> Result<u16, String> {
    let tok = tokens.next().ok_or("Missing operand")?;
    let val = parse_octal(tok).ok_or_else(|| format!("Expected octal, got '{}'", tok))?;
    if val > max {
        return Err(format!("Value {:o} exceeds maximum {:o}", val, max));
    }
    Ok(val)
}

fn expect_end(rest: &str) -> Result<(), String> {
    if rest.trim().is_empty() {
        Ok(())
    } else {
        Err(format!("Unexpected trailing input: '{}'", rest.trim()))
    }
}

fn group_name(class: Class) -> &'static str {
    match class {
        Class::Op1 => "group 1",
        Class::Op2 => "group 2",
        Class::Op3 => "group 3 / EAE",
        _ => "operate",
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
// Tests
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

#[cfg(test)]
mod tests {
    use crate::cpu::PDP8Processor;
    use sim_core::env::Disassembler;

    /// Create a CPU with one word loaded at the given address.
    fn cpu_with(address: usize, word: u16) -> PDP8Processor {
        let mut cpu = PDP8Processor::new();
        cpu.memory[address] = word;
        cpu
    }

    // ── Disassembly ────────────────────────────────────────────────────────

    #[test]
    fn disasm_mrf_page_zero() {
        // TAD 0042 — page zero address
        let cpu = cpu_with(0o1000, 0o1042);
        let (text, next_pc) = cpu.disassemble(0o1000);
        assert_eq!(text, "TAD 0042");
        assert_eq!(next_pc, 0o1001);
    }

    #[test]
    fn disasm_mrf_current_page() {
        // JMP current page — addr = 1200, inst = 5200 | 0042 = current-page JMP to 1242
        let cpu = cpu_with(0o1200, 0o5242);
        let (text, next_pc) = cpu.disassemble(0o1200);
        assert_eq!(text, "JMP 1242");
        assert_eq!(next_pc, 0o1201);
        // And with indirect bit
        let cpu = cpu_with(0o1200, 0o5642);
        let (text, _) = cpu.disassemble(0o1200);
        assert_eq!(text, "JMP I 1242");
    }

    #[test]
    fn disasm_mrf_indirect() {
        // JMS I 0123
        let cpu = cpu_with(0o0000, 0o4523);
        let (text, next_pc) = cpu.disassemble(0o0000);
        assert_eq!(text, "JMS I 0123");
        assert_eq!(next_pc, 0o0001);
    }

    #[test]
    fn disasm_iot_known() {
        let cpu = cpu_with(0, 0o6031);
        let (text, _) = cpu.disassemble(0);
        assert_eq!(text, "KSF");
    }

    #[test]
    fn disasm_iot_unknown() {
        let cpu = cpu_with(0, 0o6177);
        let (text, _) = cpu.disassemble(0);
        assert_eq!(text, "IOT 177");
    }

    #[test]
    fn disasm_cdf() {
        // CIF CDF 3 = 6233
        let cpu = cpu_with(0, 0o6233);
        let (text, _) = cpu.disassemble(0);
        assert_eq!(text, "CIF CDF 3");
        // CIF 3 = 6232 (bits 3-5 = 011 = field 3)
        let cpu = cpu_with(0, 0o6232);
        let (text, _) = cpu.disassemble(0);
        assert_eq!(text, "CIF 3");
        // CDF 1 = 6211
        let cpu = cpu_with(0, 0o6211);
        let (text, _) = cpu.disassemble(0);
        assert_eq!(text, "CDF 1");
    }

    #[test]
    fn disasm_operate_group1() {
        // CLA CLL = 7300
        let cpu = cpu_with(0, 0o7300);
        let (text, _) = cpu.disassemble(0);
        assert_eq!(text, "CLA CLL");
    }

    #[test]
    fn disasm_operate_cla_cll_cma_ral() {
        // CLA CLL CMA RAL = 7344
        let cpu = cpu_with(0, 0o7344);
        let (text, _) = cpu.disassemble(0);
        assert_eq!(text, "CLA CLL CMA RAL");
    }

    #[test]
    fn disasm_nop() {
        let cpu = cpu_with(0, 0o7000);
        let (text, _) = cpu.disassemble(0);
        assert_eq!(text, "NOP");
    }

    #[test]
    fn disasm_hlt() {
        let cpu = cpu_with(0, 0o7402);
        let (text, _) = cpu.disassemble(0);
        assert_eq!(text, "HLT");
    }

    // ── Assembly ───────────────────────────────────────────────────────────

    #[test]
    fn asm_tad_page_zero() {
        let cpu = PDP8Processor::new();
        let words = cpu.assemble(0o1000, "TAD 0042").unwrap();
        assert_eq!(words, vec![0o1042]);
    }

    #[test]
    fn asm_tad_current_page() {
        let cpu = PDP8Processor::new();
        // Address 1166 is on the same page as PC 1000
        let words = cpu.assemble(0o1000, "TAD 1166").unwrap();
        assert_eq!(words, vec![0o1366]);
        let words = cpu.assemble(0o1000, "TAD I 1042").unwrap();
        assert_eq!(words, vec![0o1642]);
    }

    #[test]
    fn asm_not_same_page() {
        let cpu = PDP8Processor::new();
        assert!(cpu.assemble(0o1000, "TAD 1367").is_err());
        assert!(cpu.assemble(0o1000, "TAD 1167").is_ok());
        assert!(cpu.assemble(0o1000, "TAD I 1367").is_err());
        assert!(cpu.assemble(0o1000, "TAD I 1167").is_ok());
    }

    #[test]
    fn asm_jmp_indirect() {
        let cpu = PDP8Processor::new();
        let words = cpu.assemble(0o0000, "JMP I 0123").unwrap();
        assert_eq!(words, vec![0o5523]);
    }

    #[test]
    fn asm_cdf() {
        let cpu = PDP8Processor::new();
        let words = cpu.assemble(0, "CDF 3").unwrap();
        assert_eq!(words, vec![0o6231]);
    }

    #[test]
    fn asm_operate() {
        let cpu = PDP8Processor::new();
        let words = cpu.assemble(0, "CLA CLL CMA RAL").unwrap();
        assert_eq!(words, vec![0o7344]);
    }

    #[test]
    fn asm_cla_group2() {
        let cpu = PDP8Processor::new();
        let words = cpu.assemble(0, "CLA SZA").unwrap();
        assert_eq!(words, vec![0o7640]);
        let words = cpu.assemble(0, "CLA MQA").unwrap();
        assert_eq!(words, vec![0o7701]);
        let words = cpu.assemble(0, "CLA").unwrap();
        assert_eq!(words, vec![0o7200]);
    }

    #[test]
    fn asm_ksf() {
        let cpu = PDP8Processor::new();
        let words = cpu.assemble(0, "KSF").unwrap();
        assert_eq!(words, vec![0o6031]);
    }

    #[test]
    fn asm_iot_generic() {
        let cpu = PDP8Processor::new();
        let words = cpu.assemble(0, "IOT 177").unwrap();
        assert_eq!(words, vec![0o6177]);
    }

    #[test]
    fn asm_address_error() {
        let cpu = PDP8Processor::new();
        assert!(cpu.assemble(0o0000, "TAD 2000").is_err());
    }

    // ── Round-trip ─────────────────────────────────────────────────────────

    #[test]
    fn roundtrip_mrf() {
        for word in [0o1042u16, 0o1642, 0o5523, 0o4400, 0o3200] {
            let cpu = cpu_with(0o1000, word);
            let (text, _) = cpu.disassemble(0o1000);
            let assembled = cpu.assemble(0o1000, &text).unwrap();
            assert_eq!(
                assembled[0], word,
                "roundtrip failed for {:04o}: '{}'",
                word, text
            );
        }
    }

    #[test]
    fn roundtrip_operate() {
        for word in [
            0o7300u16, 0o7344, 0o7402, 0o7000, 0o7501, 0o7006, 0o7012, 0o7460, 0o7470, 0o7560, 0o7570,
        ] {
            let cpu = cpu_with(0, word);
            let (text, _) = cpu.disassemble(0);
            let assembled = cpu.assemble(0, &text).unwrap();
            assert_eq!(
                assembled[0], word,
                "roundtrip failed for {:04o}: '{}'",
                word, text
            );
        }
    }
}
