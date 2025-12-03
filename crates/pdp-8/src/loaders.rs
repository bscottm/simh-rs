// SPDX-License-Identifier: MIT

use std::io::{self, Read};

use crate::cpu::{pdp8_address_format, PDP8Processor};
use sim_core::env::SimError;

impl PDP8Processor {
    /// RIM Loader: Alternating (Address, Content) pairs. No checksum.
    ///
    /// RIM format consists of alternating high and low bytes. If the word (hi << 6 | lo) is > 07777, it sets
    /// the origin. Otherwise, it deposits the word and increments the origin.
    pub fn load_rim<R: Read>(&mut self, reader: &mut R) -> Result<(), SimError> {
        let mut origin = 0u16;
        let mut buf = [0u8; 1];

        // Skip leader: skip 0 and bytes with the 8th bit set (>= 0200)
        let mut hi = loop {
            reader.read_exact(&mut buf)?;
            let b = buf[0];
            if b != 0 && b < 0200 {
                break b;
            }
        };

        loop {
            // Trailer check: bit 7 set (>= 0200) signals the end of the RIM tape
            if hi >= 0200 {
                break;
            }

            reader.read_exact(&mut buf)?;
            let lo = buf[0];

            let wd = ((hi as u16) << 6) | (lo as u16);

            if wd > 0o7777 {
                origin = wd & 0o7777;
            } else {
                // Deposit in field 0 (RIM only targets the first 4K)
                self.memory[origin as usize] = wd;
                origin = (origin + 1) & 0o7777;
            }

            // Fetch next hi byte
            if reader.read_exact(&mut buf).is_err() {
                break;
            }
            hi = buf[0];
        }

        Ok(())
    }

    /// Loads a file in Binary (BIN) format.
    ///
    /// # Parameters
    /// - `reader`: The data source.
    /// - `load_all`: If true (the "-a" switch), loads all sections in the file.
    ///               If false, stops after the first valid section.
    pub fn load_bin<R: Read>(&mut self, reader: &mut R, load_all: bool) -> Result<(), SimError> {
        let mut sections_loaded = 1u32;

        'sections: loop {
            // Every section starts fresh with zeroed tracking states
            let mut csum = 0u16;
            let mut origin = 0u16;
            let mut section_origin = 0u16;
            let mut field = 0u16;
            let mut next_field = 0u16;
            let mut words = 0u32;

            // 1. Skip leader bytes (0 or >= 0200) until a valid data-word byte arrives
            let mut hi = 0u8;
            loop {
                match sim_bin_getc(reader, &mut next_field)? {
                    Some(c) => {
                        if c != 0 && c < 0o200 {
                            hi = c;
                            break;
                        }
                    }
                    None => {
                        if sections_loaded > 0 {
                            // Orderly termination after cleanly consuming available section(s)
                            return Ok(());
                        } else {
                            return Err(SimError::FormatError(
                                "Unexpected binary loader data format (EOF during leader)".to_string(),
                            ));
                        }
                    }
                }
            }

            // 2. Process the main data-word block for this section
            loop {
                // Fetch the low byte of the 12-bit word
                let lo = match sim_bin_getc(reader, &mut next_field)? {
                    Some(c) => c,
                    None => {
                        return Err(SimError::FormatError(
                            "Truncated BIN: missing low byte".to_string(),
                        ));
                    }
                };

                let wd = ((hi as u16) << 6) | (lo as u16);
                let t = hi as u16; // Save high byte state for the checksum computation

                // Fetch the next high byte (or the terminal 0200 trailer flag)
                match sim_bin_getc(reader, &mut next_field)? {
                    Some(c) => {
                        hi = c;
                    }
                    None => {
                        return Err(SimError::FormatError(
                            "Truncated BIN: missing next high byte or trailer".to_string(),
                        ));
                    }
                }

                if hi == 0o200 {
                    // Trailer reached. The word `wd` we just assembled is the tape's checksum word.
                    if ((csum.wrapping_sub(wd)) & 0o7777) != 0 {
                        return Err(SimError::FormatError(
                            "Unexpected binary loader checksum mismatch".to_string(),
                        ));
                    }

                    println!(
                        "Section {}: origin={}, length={} words",
                        sections_loaded,
                        pdp8_address_format(section_origin as usize),
                        words
                    );
                    sections_loaded += 1;
                    words = 0;

                    if !load_all {
                        // Stop immediately if we are not instructed to find/load subsequent segments
                        return Ok(());
                    }

                    // Loop back out to re-initialize states and check for an additional section leader
                    continue 'sections;
                }

                // Update the running 12-bit checksum
                csum = (csum + t + lo as u16) & 0o7777;

                if wd > 0o7777 {
                    // Channel 7 is set -> this word modifies the loading origin pointer
                    origin = wd & 0o7777;
                    section_origin = origin;
                } else {
                    // Normal data word -> write to target location under the current active field
                    let target_address = (field | origin) as usize;
                    self.memory[target_address] = wd;
                    origin = (origin + 1) & 0o7777;
                }

                words += 1;

                // Crucial timing: Latched field changes take effect strictly AFTER
                // the data word or origin setting they preceded has been processed.
                field = next_field;
            }
        }
    }
}

/// Helper function to read a character from the tape, filtering out rubouts
/// and intercepting channel-8 field settings.
fn sim_bin_getc<R: Read>(reader: &mut R, next_field: &mut u16) -> io::Result<Option<u8>> {
    let mut rubout = false;
    let mut buf = [0u8; 1];

    loop {
        match reader.read_exact(&mut buf) {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e),
        }
        let c = buf[0];

        if rubout {
            if c == 0o377 {
                rubout = false;
            }
            continue;
        }

        if c == 0o377 {
            rubout = true;
        } else if c > 0o200 {
            // Channel 8 is set: update the latched next field value.
            // Octal 070 corresponds to bits 3, 4, and 5. Shifting left by 9
            // positions them into bits 12, 13, and 14 for extended memory addressing.
            *next_field = ((c as u16) & 0o070) << 9;
        } else {
            return Ok(Some(c));
        }
    }
}
