// SPDX-License-Identifier: MIT

//! CLI errors
//!
//! [`CLIError`] is the container for CLI errors that tracks the location,
//! the remaining input at the point of the error, the context (which parsing "rules"
//! were active, e.g., parsing a quoted string) and the kind of error encountered.

use core::convert::From;
use std::fmt;

use nom::{
    error::{ContextError, ErrorKind, ParseError},
    Offset,
};
use thiserror::Error;

use crate::cli::{span::Span, InputRadix};
use crate::env::SimError;

/// CLI parsing error container
#[derive(Debug, Clone)]
pub struct CLIError {
    /// The remaining input at error point
    pub input: String,
    /// Line and column info
    pub location: Location,
    /// Stack of what we were parsing
    pub context: Vec<&'static str>,
    /// Error diagnosis
    pub kind: CLIErrorKind,
}

/// Error location: line, column, offset after the column
#[derive(Debug, Clone)]
pub struct Location {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
}

/// The kind of parsing error encountered.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum CLIErrorKind {
    /// Unimplemented command
    #[error("Unimplemented command")]
    UnimplementedCommand,

    /// Unknown/unsupported CLI command
    #[error("Unknown command: {0}")]
    UnknownCommand(String),

    /// Ambiguous CLI command ("R" could mean "RESET", "RESTORE", "RENAME", ...)
    #[error("Ambiguous command, possibly one of {0}")]
    AmbiguousCommand(String),

    /// [`nom`] error wrapper.
    #[error("Syntax error: {0:?}")]
    Nom(ErrorKind),

    /// Expected a specific token type
    #[error("Expected {0}")]
    ExpectedToken(String),

    /// Got EOF unexpectedly
    #[error("Unexpected end of input")]
    UnexpectedEof,

    /// Invalid number
    #[error("Invalid number: {0}")]
    InvalidNumber(String),

    /// Invalid device
    #[error("Invalid device {0}")]
    InvalidDevice(String),

    /// Extraneous, trailing input
    #[error("Extraneous input: '{0}'")]
    ExtraneousInput(String),

    /// Unknown resource
    #[error("Unknown or invalid resource{}: {}",
                .device.as_ref().map(|d| format!(" for device {}", d)).unwrap_or_default(),
                .resource)]
    UnknownResource {
        resource: String,
        device: Option<String>,
    },

    /// Expected a resource, got a device.
    #[error("Expected a resource, got {0}")]
    ExpectedResource(String),

    /// Ambiguous resource that occurs in multiple devices
    #[error("{res} is ambiguously published by multiple devices ({devices})")]
    AmbiguousResource { res: String, devices: String },

    /// Invalid scalar
    #[error("Invalid {radix} scalar")]
    InvalidScalar { radix: InputRadix },

    /// Invalid array range
    #[error("Invalid array range from {0} to {1}")]
    InvalidArrayRange(usize, usize),

    /// Invalid array resource
    #[error("Invalid array resource: {0}")]
    InvalidArrayResource(String),

    /// Invalid end of array range
    #[error("Invalid array end index: {0}")]
    InvalidRangeEnd(usize),

    /// Custom formatter with the same name exists
    #[error("Custom formatter {0} already exists")]
    DuplicateFormatter(String),

    /// Invalid flag
    #[error("Invalid flag: '{0}'")]
    InvalidFlag(char),

    /// Miscellaneous error/diagnostic message.
    #[error("{0}")]
    Message(String),

    /// Command I/O error
    #[error("{0}")]
    IOError(String),

    /// Simulator's channel is dead.
    #[error("Simulator disconnected from the CLI")]
    SimThreadDead,
}

impl CLIError {
    fn new(span: Span, ctx: &'static str, kind: CLIErrorKind) -> Self {
        Self {
            input: span.input.to_string(),
            location: span.into(),
            context: vec![ctx],
            kind,
        }
    }

    pub fn unimplemented_command(span: Span, ctx: &'static str) -> Self {
        Self::new(span, ctx, CLIErrorKind::UnimplementedCommand)
    }

    pub fn unknown_command(span: Span, ctx: &'static str, cmd: String) -> Self {
        Self::new(span, ctx, CLIErrorKind::UnknownCommand(cmd.to_string()))
    }

    pub fn ambiguous_command(span: Span, ctx: &'static str, cmds: String) -> Self {
        Self::new(span, ctx, CLIErrorKind::AmbiguousCommand(cmds.to_string()))
    }

    pub fn incomplete_input(span: Span, ctx: &'static str) -> Self {
        Self::new(span, ctx, CLIErrorKind::UnexpectedEof)
    }

    pub fn expected_token(span: Span, ctx: &'static str, token: String) -> Self {
        Self::new(span, ctx, CLIErrorKind::ExpectedToken(token))
    }

    pub fn invalid_device(span: Span, ctx: &'static str, device: &str) -> Self {
        Self::new(span, ctx, CLIErrorKind::InvalidDevice(device.to_string()))
    }

    pub fn message(span: Span, ctx: &'static str, msg: String) -> Self {
        Self::new(span, ctx, CLIErrorKind::Message(msg))
    }

    pub fn extraneous_input(span: Span, ctx: &'static str) -> Self {
        Self::new(span, ctx, CLIErrorKind::ExtraneousInput(span.input.to_string()))
    }

    pub fn unknown_resource(span: Span, ctx: &'static str, reg_name: &str, device: Option<&str>) -> Self {
        Self::new(
            span,
            ctx,
            CLIErrorKind::UnknownResource {
                resource: reg_name.to_string(),
                device: device.map(|s| s.to_string()),
            },
        )
    }

    pub fn expected_resource(span: Span, ctx: &'static str, thing: &str) -> Self {
        Self::new(span, ctx, CLIErrorKind::ExpectedResource(thing.to_string()))
    }

    pub fn ambiguous_resource(span: Span, ctx: &'static str, reg_name: &str, devices: String) -> Self {
        Self::new(
            span,
            ctx,
            CLIErrorKind::AmbiguousResource {
                res: reg_name.to_string(),
                devices: devices,
            },
        )
    }

    pub fn invalid_scalar(span: Span, ctx: &'static str, radix: InputRadix) -> Self {
        Self::new(span, ctx, CLIErrorKind::InvalidScalar { radix })
    }

    pub fn invalid_array_range(span: Span, ctx: &'static str, start: usize, end: usize) -> Self {
        Self::new(span, ctx, CLIErrorKind::InvalidArrayRange(start, end))
    }

    pub fn invalid_array_resource(span: Span, ctx: &'static str, res_name: &str) -> Self {
        Self::new(
            span,
            ctx,
            CLIErrorKind::InvalidArrayResource(res_name.to_string()),
        )
    }

    pub fn duplicate_formatter(formatter: String) -> Self {
        // This is a generic error, not associated with parsing.
        CLIError {
            input: String::new(),
            location: Location {
                line: 0,
                column: 0,
                offset: 0,
            },
            context: vec!["custom formatters"],
            kind: CLIErrorKind::DuplicateFormatter(formatter.clone()),
        }
    }

    pub fn invalid_flag(span: Span, ctx: &'static str, flag: char) -> Self {
        Self::new(span, ctx, CLIErrorKind::InvalidFlag(flag))
    }

    pub fn io_error<'a>(span: Span<'a>, ctx: &'static str, err: std::io::Error) -> Self {
        Self::new(span, ctx, CLIErrorKind::IOError(err.to_string()))
    }

    pub fn simulator_disconnected<'a>(span: Span<'a>, ctx: &'static str) -> Self {
        Self::new(span, ctx, CLIErrorKind::SimThreadDead)
    }

    pub fn generic_message(msg: String) -> Self {
        CLIError {
            input: String::new(),
            location: Location {
                line: 0,
                column: 0,
                offset: 0,
            },
            context: vec![],
            kind: CLIErrorKind::Message(msg),
        }
    }
}

impl<'a> ParseError<&'a str> for CLIError {
    fn from_error_kind(input: &'a str, kind: ErrorKind) -> Self {
        CLIError {
            input: input.to_string(),
            location: Location::from_input(input),
            context: Vec::new(),
            kind: CLIErrorKind::Nom(kind),
        }
    }

    fn append(_input: &str, _kind: ErrorKind, other: Self) -> Self {
        // This is called when errors bubble up - we typically keep the deepest one
        other
    }

    fn from_char(input: &'a str, c: char) -> Self {
        CLIError {
            input: input.to_string(),
            location: Location::from_input(input),
            context: Vec::new(),
            kind: CLIErrorKind::ExpectedToken(c.to_string()),
        }
    }

    fn or(self, other: Self) -> Self {
        // Choose the error that progressed furthest
        if self.location.offset >= other.location.offset {
            self
        } else {
            other
        }
    }
}

impl<'a> ParseError<Span<'a>> for CLIError {
    fn from_error_kind(span: Span<'a>, kind: ErrorKind) -> Self {
        CLIError {
            input: span.input.to_string(),
            location: span.into(),
            context: Vec::new(),
            kind: CLIErrorKind::Nom(kind),
        }
    }

    fn append(_span: Span<'a>, _kind: ErrorKind, other: Self) -> Self {
        // This is called when errors bubble up - we typically keep the deepest one
        other
    }

    fn from_char(span: Span<'a>, c: char) -> Self {
        CLIError {
            input: span.input.to_string(),
            location: span.into(),
            context: Vec::new(),
            kind: CLIErrorKind::ExpectedToken(c.to_string()),
        }
    }

    fn or(self, other: Self) -> Self {
        // Choose the error that progressed furthest
        if self.location.offset >= other.location.offset {
            self
        } else {
            other
        }
    }
}

impl Location {
    pub fn from_input(_input: &str) -> Self {
        // You'd need to track this - see below
        Location {
            line: 0,
            column: 0,
            offset: 0,
        }
    }

    pub fn from_offset(original: &str, remaining: &str) -> Self {
        let offset = original.offset(remaining);
        let prefix = &original[..offset];

        let line = prefix.lines().count();
        let column = prefix.lines().last().map(|l| l.len()).unwrap_or(0);

        Location { line, column, offset }
    }
}

/// Type conversion from [`Span`] to an error [`Location`].
impl<'a> From<Span<'a>> for Location {
    fn from(src: Span<'a>) -> Self {
        Self {
            line: src.line,
            column: src.column,
            offset: src.offset,
        }
    }
}

/// Implementation for [`nom::error::context`] to add context to errors
impl ContextError<&str> for CLIError {
    fn add_context(_input: &str, ctx: &'static str, mut other: Self) -> Self {
        other.context.push(ctx);
        other
    }
}

impl ContextError<Span<'_>> for CLIError {
    fn add_context(_input: Span<'_>, ctx: &'static str, mut other: Self) -> Self {
        other.context.push(ctx);
        other
    }
}

/// Implementation for [`std::fmt`] to output a [`CLIError`]
impl fmt::Display for CLIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // If the line number is greater than 0, it's a real CLI error. Otherwise,
        // it's a more generic error that percolated up as the result of a type
        // conversion (see the From<> instances below.)
        if self.location.line > 0 {
            write!(f, "Line {}, column {} ", self.location.line, self.location.column)?;
        }
        if !self.context.is_empty() {
            write!(f, "(")?;
            for (i, ctx) in self.context.iter().rev().enumerate() {
                if i > 0 {
                    write!(f, " -> ")?;
                }
                write!(f, "{}", ctx)?;
            }
            write!(f, ") ")?;
        }

        // For errors that include self.input, we need to add it manually:
        match &self.kind {
            CLIErrorKind::InvalidScalar { radix } => {
                write!(f, "Invalid {} scalar: {}", radix, self.input)
            }
            // For all other cases, just use the derived Display
            _ => write!(f, "{}", self.kind),
        }?;

        Ok(())
    }
}

impl From<nom::Err<CLIError>> for CLIError {
    fn from(value: nom::Err<CLIError>) -> Self {
        match value {
            nom::Err::Error(e) | nom::Err::Failure(e) => e,
            nom::Err::Incomplete(_) => CLIError {
                input: String::new(),
                location: Location {
                    line: 0,
                    column: 0,
                    offset: 0,
                },
                context: vec!["incomplete input"],
                kind: CLIErrorKind::UnexpectedEof,
            },
        }
    }
}

impl From<std::io::Error> for CLIError {
    fn from(value: std::io::Error) -> Self {
        CLIError {
            input: String::new(),
            location: Location {
                line: 0,
                column: 0,
                offset: 0,
            },
            context: vec!["I/O operation"],
            kind: CLIErrorKind::IOError(value.to_string()),
        }
    }
}

impl From<SimError> for CLIError {
    fn from(value: SimError) -> Self {
        CLIError {
            input: String::new(),
            location: Location {
                line: 0,
                column: 0,
                offset: 0,
            },
            context: vec!["Simulator environment"],
            kind: CLIErrorKind::Message(value.to_string()),
        }
    }
}
