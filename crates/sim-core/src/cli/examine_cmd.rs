// SPDX-License-Identifier: MIT

use std::cmp::min;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write as IOWrite};

use crate::env::{
    ExamineRequest, ExamineResult, SimError, SimRequest, SimResponse, CPU_DEVICE_NAME, MEM_RESOURCE_NAME,
};

use crate::cli::{
    cli_error::CLIError,
    cmd_repl::CmdContext,
    parsers::{
        parse_examine_command, ExaminedResource, Examinee, MaskOperation, SearchOperation, EXAMINE_CTX,
    },
    repl_state::REPLState,
    span::Span,
};

use crate::{sim_write, sim_writeln};

/// Fallback formatter used when a resource has no `formatter` set.
fn default_format(val: u64) -> String {
    format!("{}", val)
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

/// `EXAMINE` command action function.
pub fn examine_command(context: &mut CmdContext, args: Span<'_>) -> Result<(), CLIError> {
    let (remainder, examine) = parse_examine_command(context, args)?;

    let format = DisplayFormat::from_switches(&examine.switches);

    let device_name = examine
        .what
        .device
        .clone()
        .unwrap_or_else(|| CPU_DEVICE_NAME.to_string());

    let mut batch: Vec<ExamineRequest> = Vec::new();

    match examine.what.arg {
        Examinee::State => {
            let dev_meta = context
                .state
                .get_device_meta(&device_name)
                .ok_or_else(|| CLIError::invalid_device(remainder, EXAMINE_CTX, &device_name))?;

            for res_meta in &dev_meta.resources {
                if res_meta.name != MEM_RESOURCE_NAME {
                    batch.push(ExamineRequest {
                        device_name: device_name.clone(),
                        resource_name: res_meta.name.clone(),
                        start: 0,
                        count: min(res_meta.length, 8),
                        mnemonic: false,
                    });
                }
            }
        }

        Examinee::All => {
            let dev_meta = context
                .state
                .get_device_meta(&device_name)
                .ok_or_else(|| CLIError::invalid_device(remainder, EXAMINE_CTX, &device_name))?;

            for res_meta in &dev_meta.resources {
                if res_meta.name != MEM_RESOURCE_NAME {
                    batch.push(ExamineRequest {
                        device_name: device_name.clone(),
                        resource_name: res_meta.name.clone(),
                        start: 0,
                        count: res_meta.length,
                        mnemonic: false,
                    });
                }
            }
        }

        Examinee::ResourceList { resources } => {
            for res_spec in resources {
                let name = res_spec.resource_name();

                let target_device = if name == MEM_RESOURCE_NAME && examine.what.device.is_none() {
                    Some(CPU_DEVICE_NAME)
                } else {
                    examine.what.device.as_deref()
                };

                let mut resolved =
                    context
                        .state
                        .resolve_resource(remainder, EXAMINE_CTX, name, target_device)?;

                if let ExaminedResource::Slice {
                    start_offset,
                    end_offset,
                    ..
                } = res_spec
                {
                    if start_offset > end_offset {
                        return Err(CLIError::invalid_array_range(
                            remainder,
                            EXAMINE_CTX,
                            start_offset,
                            end_offset,
                        ));
                    }
                    resolved.start_offset = start_offset;
                    resolved.count = end_offset - start_offset + 1;
                }

                let is_mnemonic = matches!(format, DisplayFormat::Instruction)
                    && resolved.resource.name == MEM_RESOURCE_NAME;
                batch.push(ExamineRequest {
                    device_name: resolved.device.name.clone(),
                    resource_name: resolved.resource.name.clone(),
                    start: resolved.start_offset,
                    count: resolved.count,
                    mnemonic: is_mnemonic,
                });
            }
        }
    }

    if batch.is_empty() {
        return Ok(());
    }

    let results = context.state.transact_apply(
        remainder,
        EXAMINE_CTX,
        SimRequest::Examine(batch.clone()),
        |resp| match resp {
            SimResponse::ExamineData(data) => Some(data),
            _ => None,
        },
    )?;

    if let Some(ref filename) = examine.outfile {
        let file = OpenOptions::new().create(true).append(true).open(filename)?;
        let mut writer = BufWriter::new(file);
        render_examine_results(
            &mut writer,
            &batch,
            results,
            context.state,
            format,
            examine.mask_op.as_ref(),
            examine.search_op.as_ref(),
        )?;
        writer.flush()?;
    } else {
        let mut guard = context.state.output_sink.borrow_mut();
        render_examine_results(
            &mut *guard,
            &batch,
            results,
            context.state,
            format,
            examine.mask_op.as_ref(),
            examine.search_op.as_ref(),
        )?;
    }

    Ok(())
}

/// Emit formatted examine results.
fn render_examine_results(
    writer: &mut dyn IOWrite,
    requests: &[ExamineRequest],
    results: Vec<Result<ExamineResult, SimError>>,
    state: &REPLState,
    format: DisplayFormat,
    mask_op: Option<&MaskOperation>,
    search_op: Option<&SearchOperation>,
) -> Result<(), CLIError> {
    for (req, result) in requests.iter().zip(results) {
        match result {
            Ok(ExamineResult::Values(values)) => {
                let meta = state
                    .get_resource_meta(&req.device_name, &req.resource_name)
                    .unwrap();

                let processed: Vec<u64> = values
                    .iter()
                    .map(|&v| mask_op.map_or(v, |m| m.apply(v)))
                    .collect();

                let filtered: Vec<(usize, u64)> = processed
                    .iter()
                    .enumerate()
                    .filter(|(_, &v)| search_op.map_or(true, |s| s.matches(v)))
                    .map(|(i, &v)| (req.start + i, v))
                    .collect();

                if filtered.is_empty() && search_op.is_some() {
                    continue;
                }

                let display_list: Vec<(usize, u64)> = if search_op.is_some() {
                    filtered
                } else {
                    processed
                        .iter()
                        .enumerate()
                        .map(|(i, &v)| (req.start + i, v))
                        .collect()
                };

                let mut last_addr = display_list[0].0;
                let is_array = display_list.len() > 1;

                for (display_idx, (addr, val)) in display_list.iter().enumerate() {
                    if display_idx % 8 == 0 || last_addr + 1 != *addr {
                        if display_idx > 1 {
                            sim_write!(state, writer, "\n");
                        }

                        let _ = sim_write!(
                            state,
                            writer,
                            "{:<10}",
                            if req.resource_name != MEM_RESOURCE_NAME {
                                format!(
                                    "{}{}",
                                    req.resource_name,
                                    if is_array {
                                        format!("[{}]", *addr)
                                    } else {
                                        "".to_string()
                                    }
                                )
                            } else {
                                // Just print the memory address, even if it's just a single result
                                (state.address_format)(*addr)
                            }
                        );
                    }
                    sim_write!(
                        state,
                        writer,
                        " {}",
                        format.format_value(*val, meta.word_size, meta.formatter)
                    );
                    last_addr = *addr;
                }

                sim_writeln!(state, writer, "");
            }

            Ok(ExamineResult::Mnemonics(mnemonics)) => {
                // mask and search are not meaningful for disassembled text.
                for (addr, text) in &mnemonics {
                    sim_writeln!(state, writer, "{}  {}", (state.address_format)(*addr), text);
                }
            }

            Err(e) => sim_writeln!(state, writer, "{}: Error - {}", req.resource_name, e),
        }
    }
    Ok(())
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
// DisplayFormat
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

#[derive(Debug, Clone, Copy, PartialEq)]
enum DisplayFormat {
    Default,
    Ascii,
    CharString,
    Instruction,
    Octal,
    Decimal,
    Hex,
    Binary,
}

impl DisplayFormat {
    fn from_switches(switches: &[char]) -> Self {
        for &sw in switches.iter().rev() {
            match sw.to_ascii_lowercase() {
                'a' => return Self::Ascii,
                'c' => return Self::CharString,
                'm' => return Self::Instruction,
                'o' => return Self::Octal,
                'd' => return Self::Decimal,
                'h' => return Self::Hex,
                '2' => return Self::Binary,
                _ => {}
            }
        }
        Self::Default
    }

    fn format_value(&self, value: u64, bits: u32, formatter: Option<fn(u64) -> String>) -> String {
        match self {
            Self::Default => formatter.map_or_else(|| default_format(value), |f| f(value)),
            Self::Ascii => {
                if value <= 127 {
                    let ch = value as u8 as char;
                    if ch.is_ascii_graphic() || ch == ' ' {
                        format!("'{}'", ch)
                    } else {
                        format!("'\\{:03o}'", value)
                    }
                } else {
                    format!("\\{:03o}", value)
                }
            }
            Self::CharString => {
                let byte_count = (bits + 7) / 8;
                let bytes: Vec<u8> = (0..byte_count)
                    .rev()
                    .map(|i| ((value >> (i * 8)) & 0xFF) as u8)
                    .collect();
                bytes
                    .iter()
                    .map(|&b| {
                        let ch = b as char;
                        if ch.is_ascii_graphic() || ch == ' ' {
                            ch.to_string()
                        } else {
                            format!("\\{:03o}", b)
                        }
                    })
                    .collect()
            }
            Self::Instruction => {
                let width = ((bits + 3) / 4) as usize;
                format!("{:0width$X}", value, width = width)
            }
            Self::Octal => {
                let width = ((bits + 2) / 3) as usize;
                format!("{:0width$o}", value, width = width)
            }
            Self::Decimal => format!("{}", value),
            Self::Hex => {
                let width = ((bits + 3) / 4) as usize;
                format!("{:0width$X}", value, width = width)
            }
            Self::Binary => format!("{:0width$b}", value, width = bits as usize),
        }
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
// Tests (unchanged from original — just import paths adjusted)
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::parsers::{
        parse_array_range, parse_bare_address_range, parse_outfile, parse_resource_list, InputRadix,
    };

    #[test]
    fn test_atsign_redirect() {
        let (remainder, outfile) = parse_outfile(Span::new("@somefile.ext")).unwrap();
        assert_eq!(outfile, Some("somefile.ext".to_string()));
        assert!(remainder.input.is_empty());

        let (remainder, outfile) = parse_outfile(Span::new(" @foobar.txt")).unwrap();
        assert_eq!(outfile, Some("foobar.txt".to_string()));
        assert!(remainder.input.is_empty());
    }

    #[test]
    fn test_array_range() {
        let (remainder, result) = parse_array_range(InputRadix::Dec, Span::new("100:200")).unwrap();
        assert_eq!(result, (100, 200));
        assert!(remainder.input.is_empty());

        let (remainder, result) = parse_array_range(InputRadix::Oct, Span::new("100:200")).unwrap();
        assert_eq!(result, (0o100, 0o200));
        assert!(remainder.input.is_empty());
    }

    #[test]
    fn test_resource_list() {
        let (remainder, result) = parse_resource_list(InputRadix::Dec, Span::new("PC, AC,SP,FOO")).unwrap();
        assert_eq!(
            result,
            vec![
                ExaminedResource::Whole {
                    res_name: "PC".to_string()
                },
                ExaminedResource::Whole {
                    res_name: "AC".to_string()
                },
                ExaminedResource::Whole {
                    res_name: "SP".to_string()
                },
                ExaminedResource::Whole {
                    res_name: "FOO".to_string()
                },
            ]
        );
        assert!(remainder.input.is_empty());
    }

    #[test]
    fn test_bare_address_range() {
        let (remainder, res) = parse_bare_address_range(InputRadix::Oct, Span::new("100")).unwrap();
        assert_eq!(
            res,
            ExaminedResource::Slice {
                res_name: "MEM".to_string(),
                start_offset: 0o100,
                end_offset: 0o100,
            }
        );
        assert!(remainder.input.is_empty());

        let (_, res) = parse_bare_address_range(InputRadix::Oct, Span::new("0:7")).unwrap();
        assert_eq!(
            res,
            ExaminedResource::Slice {
                res_name: "MEM".to_string(),
                start_offset: 0,
                end_offset: 7,
            }
        );
    }

    #[test]
    fn test_display_format_from_switches() {
        assert_eq!(DisplayFormat::from_switches(&['a']), DisplayFormat::Ascii);
        assert_eq!(DisplayFormat::from_switches(&['o']), DisplayFormat::Octal);
        assert_eq!(DisplayFormat::from_switches(&['o', 'h']), DisplayFormat::Hex);
    }
}
