// SPDX-License-Identifier: MIT

//! Directory and file operations
//!
//! DIR, LS: Directory contents. Supports globbing.
//! CD: Change directory.
//! PWD: Print Working Directory.
//! COPY, CP: Copy files.

use chrono::{DateTime, Local};
use std::env::set_current_dir;
use std::path::PathBuf;
use walkdir::WalkDir;
// num_format provides the comma-separated number formatting.
use globset::Glob;
use nom::Parser;
use num_format::{Locale, ToFormattedString};

use crate::{
    cli::{
        cli_error::CLIError,
        cmd_repl::CmdContext,
        parsers::{consume_eol, filename_noglob, filename_token},
        span::Span,
    },
    sim_eprintln, sim_println,
};

/// Minimum depth for [`WalkDir`] directory traversals.
const MIN_DEPTH: usize = 1;
/// Maximum depth for [`WalkDir`] recursive directory traversals.
const MAX_DEPTH: usize = 8;
/// Characters that indicate globbing in file patterns.
const GLOB_CHARS: &[char] = &['*', '?', '[', '{'];

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
/// "DIR" and "LS"command action function.
///
/// Lists the contents of the current directory (without an argument) or the requested
/// directory (with an argument). Globbing is supported:
///
/// - `*` wildcards
/// - `**` recursive descent into subdirectories, e.g. `dir **/*.txt`
/// - `?` single-character wildcards
///
/// The globbing support comes from the [`globset` ] crate. The one unsupported globbing
/// feature is the braced alternation, e.g. `dir *.{txt,md}` (TODO feature.)
///
/// Differences from SIMH:
/// - "." and ".." are not included in the listing. [`WalkDir` ] explicitly does not include
///   these in the iterator.
/// - Lower-cased ante- and post-meridian (vs. upper-cased.)
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
pub fn dir_command(context: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    // The default directory is always "." (current directory).
    let default_dir = PathBuf::from(".");
    // The default directory portion is the current directory.
    let mut dir_parts = default_dir.clone();
    // The file pattern matches everything.
    let mut file_pattern = "*".to_string();
    // Default depth is minimal (no recursive descent).
    let mut depth = MIN_DEPTH;

    if !args.input.is_empty() {
        let (remainder, arg) = filename_token(args.clone()).map_err(|_e| {
            CLIError::message(args, "DIR filename", format!("Invalid filename: {}", args.input))
        })?;

        if !remainder.input.trim().is_empty() {
            return Err(CLIError::extraneous_input(remainder, "DIR"));
        }

        let path_buf = PathBuf::from(arg.clone());
        let path_vec = path_buf.into_iter().collect::<Vec<_>>();

        if arg.contains("**") {
            // Recursive descent requested.
            depth = MAX_DEPTH;
        }

        // Separate the directory portion from the file pattern/globbing portion.
        dir_parts = PathBuf::new();
        let mut glob_parts = PathBuf::new();

        for (i, c) in path_vec.iter().enumerate() {
            if c.to_str().unwrap().contains(&GLOB_CHARS[..]) {
                path_vec[i..].iter().for_each(|p| glob_parts.push(p));
                break;
            } else {
                dir_parts.push(c);
            }
        }

        if !glob_parts.as_os_str().is_empty() {
            file_pattern = glob_parts.to_str().unwrap().to_string();
        } else {
            // glob_parts is empty, which means that there's no file pattern specified.
            // If glob_parts is a file name without globbing characters, use that as the
            // file pattern.
            if dir_parts.is_file() {
                // file_name() -> Option<&OsStr>; unwrap() -> &OsStr; to_str() -> Option<&str>;
                // unwrap() -> &str; to_string() -> String
                file_pattern = dir_parts.file_name().unwrap().to_str().unwrap().to_string();
                // parent() -> Option<Path>; unwrap_or -> PathBuf, defaulting to "."; to_path_buf()
                // converts to desired PathBuf.
                dir_parts = dir_parts.parent().unwrap_or(&default_dir.as_path()).to_path_buf();
            }
        }

        if dir_parts.as_os_str().is_empty() {
            dir_parts = default_dir.clone();
        }
    }

    let glob =
        Glob::new(&file_pattern).map_err(|err| CLIError::message(args, "file pattern", err.to_string()))?;
    let pattern = glob.compile_matcher();

    let mut total_bytes = 0;
    let mut total_files = 0;
    let mut total_dirs = 0;

    let canonical_dir = dir_parts.canonicalize()?;

    sim_println!(context.state, " Directory of {}", canonical_dir.display());
    sim_println!(context.state, "");

    let dir_prefix = dir_parts.to_str().unwrap_or(".");

    // min_depth(1) ensures that the current directory's name isn't included
    // in the iteration. ok() converts the iterator's value to Ok<T>.
    for entry in WalkDir::new(dir_parts.clone())
        .min_depth(MIN_DEPTH)
        .max_depth(depth)
        .into_iter()
        .filter_map(|p| p.ok())
    {
        if pattern.is_match(entry.file_name()) {
            let entry_path = entry.path();
            let suffix_path = entry_path.strip_prefix(dir_prefix).unwrap_or(entry_path);

            match entry.metadata() {
                Ok(metadata) => {
                    let mod_systime = metadata.modified()?;
                    let mod_datetime: DateTime<Local> = mod_systime.into();
                    let ftype = if metadata.is_file() {
                        total_files += 1;
                        total_bytes += metadata.len();
                        format!("{:>17}", metadata.len().to_formatted_string(&Locale::en))
                    } else if metadata.is_dir() {
                        total_dirs += 1;
                        "   <DIR>         ".to_string()
                    } else if metadata.is_symlink() {
                        "   <SYMLINK>     ".to_string()
                    } else {
                        " ".repeat(17).to_string()
                    };

                    sim_println!(
                        context.state,
                        "{:16} {} {}",
                        mod_datetime.format("%m/%d/%Y  %I:%M %P"),
                        ftype,
                        suffix_path.display()
                    );
                }
                Err(_err) => {
                    sim_eprintln!(
                        context.state,
                        "{:^16} {} {}",
                        "(no metadata)",
                        " ".repeat(17),
                        suffix_path.display()
                    );
                }
            }
        }
    }

    sim_println!(
        context.state,
        "{:>16} File(s) {} bytes",
        total_files.to_formatted_string(&Locale::en),
        total_bytes.to_formatted_string(&Locale::en)
    );
    sim_println!(
        context.state,
        "{:>16} Dir(s)",
        total_dirs.to_formatted_string(&Locale::en)
    );

    Ok(())
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
/// "CD" command action function.
///
/// Changes the current working directory to the specified directory.
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
pub fn cd_command(_context: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    let dir_path = if args.input.trim().is_empty() {
        // TODO: Should this take the user to their home directory?
        return Err(CLIError::expected_token(
            args,
            "cd command",
            "directory name".to_string(),
        ));
    } else {
        let (remainder, arg) = filename_token(args.clone())?;

        if !remainder.input.trim().is_empty() {
            return Err(CLIError::extraneous_input(remainder, "cd command"));
        }

        PathBuf::from(arg)
    };

    let canonical_dir = dir_path.canonicalize()?;

    set_current_dir(&canonical_dir)?;
    Ok(())
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
/// "PWD" command action function.
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
pub fn pwd_command(context: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    // Should no arguments.
    consume_eol().parse(args)?;

    let current_dir = std::env::current_dir()?;
    sim_println!(context.state, "{}", current_dir.display());
    Ok(())
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
/// "COPY" and "CP" command action function.
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
pub fn file_copy_command(_context: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    let (span, source_file) = filename_noglob(args)?;
    let (span, dest_file) = filename_noglob(span)?;
    consume_eol().parse(span)?;

    std::fs::copy(source_file, dest_file)?;
    Ok(())
}
