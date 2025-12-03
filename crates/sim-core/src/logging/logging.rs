// SPDX-License-Identifier: MIT

//! SIMH-RS logging infrastructure.
//!
//! # Overview
//!
//! Two distinct log sessions are supported:
//!
//! - **Debug log** (`SET DEBUG [-P] [-I] <dest>`) — structured output from the simulator
//!   and CLI, with optional program counter and instruction count. Managed via
//!   [`SharedDebugState`].
//!
//! - **Transcript log** (`SET LOG [-A] <dest>`) — a human-readable record of CLI
//!   interactions and simulator console output. Managed via [`SharedTranscriptSink`].
//!
//! # Shared-file mode
//!
//! Original SIMH allowed either log to be redirected to the other's file:
//!
//! - `SET DEBUG LOG` — debug output goes to the transcript file. Before each debug
//!   record, any partial (un-newline-terminated) transcript line is flushed with a
//!   timestamp marker, ensuring the debug record is not interleaved mid-line.
//! - `SET LOG DEBUG` — transcript output goes to the debug file. The transcript line
//!   buffer flushes complete lines to the debug file; no special handling is required
//!   since debug records are always line-complete.
//!
//! Shared-file mode is implemented by cloning the [`SharedSink`] `Arc`. Both sessions
//! write through the same `Mutex<Box<dyn Write>>`, serialising output without
//! interleaving.
//!
//! # Thread safety
//!
//! Both the simulator and CLI produce debug output, so all paths are thread-safe.
//! Lock ordering is strict: **DebugState → TranscriptInner → SharedSink**.
//! The CLI console path only ever acquires **TranscriptInner → SharedSink**.
//! No deadlock is possible.
//!
//! # flexi_logger integration
//!
//! [`LogDestination`] holds a [`LoggerHandle`] for each active session, enabling
//! future log rotation. [`DebugSinkWriter`] and [`TranscriptSinkWriter`] implement
//! [`LogWriter`] and are passed (boxed) to `log_to_writer()`, routing `log::` macro
//! calls through the same [`SharedSink`] as the `sim_debug!`/`cli_debug!` macros.

use std::fmt;
use std::fs::OpenOptions;
use std::io::{self, BufWriter, Write as IOWrite};
use std::sync::OnceLock;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use derive_more::Display;
use flexi_logger::{writers::LogWriter, DeferredNow, FlexiLoggerError, Logger, LoggerHandle, Record};

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Timestamp
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// Startup time reference for elapsed microsecond calculation.
static STARTUP_TIME: OnceLock<Instant> = OnceLock::new();

/// Return microseconds elapsed since program startup.
///
/// All timestamps across both debug and transcript output share this origin, making
/// cross-stream correlation straightforward.
#[inline]
pub fn micros_since_startup() -> u64 {
    STARTUP_TIME.get_or_init(Instant::now).elapsed().as_micros() as u64
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// SharedSink — raw file handle
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// Reference-counted, mutex-protected output writer.
///
/// Clone the `Arc` to share the same underlying file between the debug and transcript
/// sessions (`SET DEBUG LOG` / `SET LOG DEBUG`). The `Mutex` serialises all writes.
///
/// Construction: [`open_sink`], [`stdout_sink`], [`stderr_sink`].
pub type SharedSink = Arc<Mutex<Box<dyn IOWrite + Send>>>;

/// Open a file and wrap it in a [`SharedSink`].
///
/// - `path`:   file path; created if absent
/// - `append`: `true` to append; `false` truncates
pub fn open_sink(path: &str, append: bool) -> Result<SharedSink, LogError> {
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(!append)
        .append(append)
        .open(path)?;

    Ok(Arc::new(Mutex::new(
        Box::new(BufWriter::new(file)) as Box<dyn IOWrite + Send>
    )))
}

/// Wrap `stdout` in a [`SharedSink`].
pub fn stdout_sink() -> SharedSink {
    Arc::new(Mutex::new(Box::new(io::stdout()) as Box<dyn IOWrite + Send>))
}

/// Wrap `stderr` in a [`SharedSink`].
pub fn stderr_sink() -> SharedSink {
    Arc::new(Mutex::new(Box::new(io::stderr()) as Box<dyn IOWrite + Send>))
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Debug snapshot — CPU state written by the execution loop
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// CPU state written by [`crate::env::SimEnvironment`]'s execution loop, read by [`crate::sim_debug!`].
///
/// `pc` is pre-formatted by the CPU in its native radix (e.g. `"017652"` for PDP-11, `"0x0001A3C0"` for VAX)
/// via [`crate::env::CPUTraits::current_pc`]. The execution loop owns `instruction_count`, incrementing it
/// each cycle and resetting to 0 on `RUN`/`BOOT`.
///
/// `pc` may be `None` if the CPU does not implement `debug_context`.
#[derive(Debug, Clone, Default)]
pub struct DebugSnapshot {
    pub pc: Option<String>,
    pub instruction_count: u64,
}

/// Shared between [`crate::env::SimEnvironment`] (writer) and [`DebugState`] (reader).
pub type SharedDebugSnapshot = Arc<RwLock<DebugSnapshot>>;

/// Construct a fresh, zeroed [`SharedDebugSnapshot`].
pub fn new_debug_snapshot() -> SharedDebugSnapshot {
    Arc::new(RwLock::new(DebugSnapshot::default()))
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// DebugFormatFlags — per-session format configuration
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// Controls what is included in each `sim_debug!` record.
///
/// Set by flags on `SET DEBUG`:
///
/// | Flag | Output prefix |
/// |------|--------------|
/// | (default) | `[0001234567] DEBUG target: message` |
/// | `-P` | `[0001234567] DEBUG PC=017652 target: message` |
/// | `-I` | `[#0000000042] DEBUG target: message` |
/// | `-P -I` | `[0001234567] DEBUG PC=017652 #42 target: message` |
///
/// `cli_debug!` always uses the wall-clock timestamp and never includes PC or
/// instruction count — the CLI is idle while the simulator runs.
#[derive(Debug, Clone, Default)]
pub struct DebugFormatFlags {
    /// Include `PC=<value>` in each simulator debug record. (`-P`)
    pub show_pc: bool,
    /// Prefix with instruction count `#N` instead of microsecond timestamp. (`-I`)
    pub show_instruction_count: bool,
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// TranscriptSink — line-buffered transcript writer
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// Inner state of [`TranscriptSink`], held behind a [`Mutex`].
///
/// The `Mutex` protects both `writer` and `line_buffer` together, ensuring that
/// the partial-line-flush in [`TranscriptSink::write_debug_with_flush`] is atomic
/// with respect to concurrent console character writes from the CLI thread.
pub struct TranscriptInner {
    /// Underlying output sink. May be shared with the debug session.
    pub writer: SharedSink,
    /// Accumulates simulator console characters until a newline is received.
    pub line_buffer: String,
}

impl TranscriptInner {
    pub fn flush(&mut self) {
        if !self.line_buffer.is_empty() {
            let line = std::mem::take(&mut self.line_buffer);
            let _ = self.writer.lock().unwrap().write_all(line.as_bytes());
        }
    }
}

/// Line-buffered transcript writer — target for `SET LOG`.
///
/// Console output from the simulator arrives character-at-a-time. Characters are accumulated in `line_buffer`
/// and flushed atomically to `writer` on `'\n'`, preventing partial-line interleaving with debug records when
/// the sink is shared.
///
/// When `SET DEBUG LOG` is active, [`DebugState`] holds a clone of this `Arc` and calls
/// [`TranscriptSink::write_debug_with_flush`] before each debug record, which flushes any partial line and
/// writes a timestamp marker. The line buffer is preserved (not cleared), so console output continues
/// naturally after the marker.
pub struct TranscriptSink {
    pub inner: Mutex<TranscriptInner>,
}

/// Reference-counted handle to a [`TranscriptSink`].
pub type SharedTranscriptSink = Arc<TranscriptSink>;

impl TranscriptSink {
    /// Construct a new transcript sink over an existing [`SharedSink`].
    ///
    /// Returns `Arc<Self>` so the caller can retain a clone alongside the
    /// [`LogDestination`] handle.
    pub fn new(writer: SharedSink) -> SharedTranscriptSink {
        Arc::new(Self {
            inner: Mutex::new(TranscriptInner {
                writer,
                line_buffer: String::new(),
            }),
        })
    }

    /// Clone the underlying [`SharedSink`] for use by a debug session sharing this file.
    ///
    /// Used by `SET DEBUG LOG` in `set_cmd.rs` to obtain the sink without exposing
    /// `TranscriptInner` directly.
    pub fn shared_sink(&self) -> SharedSink {
        Arc::clone(&self.inner.lock().unwrap().writer)
    }

    /// Append a character from the simulator console to the transcript.
    ///
    /// On `'\n'`, flushes the completed line atomically to the sink. On all other
    /// characters, only the line buffer is modified.
    pub fn push_char(&self, c: char) {
        let mut inner = self.inner.lock().unwrap();
        inner.line_buffer.push(c);

        if c == '\n' {
            inner.flush();
        }
    }

    /// Write a complete CLI line to the transcript (e.g. a command echo or response).
    pub fn write_line(&self, line: &str) {
        let inner = self.inner.lock().unwrap();
        let _ = writeln!(inner.writer.lock().unwrap(), "{}", line);
    }

    /// Write partial CLI output to the transcript
    pub fn write(&self, s: String) {
        self.write_str(s.as_str());
    }

    /// Write partial CLI `str` output to the transcript
    pub fn write_str(&self, s: &str) {
        let inner = self.inner.lock().unwrap();
        let _ = write!(inner.writer.lock().unwrap(), "{}", s);
    }

    /// Flush accumulated output
    pub fn flush(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.flush();
    }

    /// Flush the partial line buffer and write a debug timestamp cross-reference marker, then write
    /// `debug_line` — all under the same `TranscriptInner` lock.
    ///
    /// Called by [`DebugState::write_sim`] and [`DebugState::write_cli`] when `SET DEBUG LOG` is active. The
    /// line buffer is **not cleared** — console output resumes from the buffered partial line on the next
    /// [`TranscriptSink::push_char`] call.
    ///
    /// Resulting transcript:
    /// ```text
    /// CPU>                          ← partial line flushed (not cleared)
    /// [0001234567] -- debug --      ← cross-reference marker
    /// [0001234567] DEBUG ETH_RX: …  ← the actual debug record
    /// CPU>  go 0173000              ← partial line continues normally
    /// ```
    pub fn write_debug_with_flush(&self, debug_line: &str) {
        let inner = self.inner.lock().unwrap();
        let micros = micros_since_startup();

        // Acquire the sink lock once for the entire sequence so nothing can be
        // interleaved between the marker and the debug record.
        let mut sink = inner.writer.lock().unwrap();

        if !inner.line_buffer.is_empty() {
            let _ = sink.write_all(inner.line_buffer.as_bytes());
            let _ = writeln!(sink);
            let _ = writeln!(sink, "[{:010}] -- debug --", micros);
        }

        let _ = sink.write_all(debug_line.as_bytes());
    }
}

/// Thin [`LogWriter`] wrapper around [`SharedTranscriptSink`].
///
/// Kept separate from [`TranscriptSink`] because `log_to_writer()` requires
/// `Box<dyn LogWriter>`. Constructing a `TranscriptSinkWriter` and boxing it
/// leaves the `Arc<TranscriptSink>` available for direct `push_char` /
/// `write_line` / `write_debug_with_flush` calls without routing through
/// flexi_logger.
pub struct TranscriptSinkWriter {
    inner: SharedTranscriptSink,
}

impl LogWriter for TranscriptSinkWriter {
    /// Routes `log::info!` etc. to the transcript.
    ///
    /// Format: `%SIM-LEVEL: message` — no timestamp; transcript is human-readable.
    fn write(&self, _now: &mut DeferredNow, record: &Record) -> io::Result<()> {
        let line = format!("%SIM-{}: {}", record.level(), record.args());
        self.inner.write_line(&line);
        Ok(())
    }

    fn flush(&self) -> io::Result<()> {
        self.inner.inner.lock().unwrap().writer.lock().unwrap().flush()
    }

    fn max_log_level(&self) -> log::LevelFilter {
        log::LevelFilter::Info
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// DebugState — debug session configuration and write logic
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// All state required to produce a debug record.
///
/// Shared between the CLI ([`crate::cli::repl_state::REPLState`]) and the simulator
/// ([`crate::env::SimEnvironment`]) as a `Arc<Mutex<DebugState>>`. The CLI creates and configures it on `SET
/// DEBUG`; the simulator stores a clone and uses it in `sim_debug!`. The CLI updates format flags or clears
/// it on `SET DEBUG` / `SET NODEBUG`.
///
/// # Lock ordering
///
/// When [`DebugState::transcript`] is `Some` (i.e. `SET DEBUG LOG` is active), the call chain is:
///
///   **`DebugState` mutex -> `TranscriptInner` mutex -> `SharedSink` mutex**.
///
/// CLI console output only ever acquires **`TranscriptInner` -> `SharedSink`**.  This consistent ordering
/// prevents deadlock.
pub struct DebugState {
    /// Output sink. Direct file, stdout, stderr, or a clone of the transcript sink.
    pub sink: SharedSink,
    /// Present when `SET DEBUG LOG` is active. Debug writes are routed through
    /// [`TranscriptSink::write_debug_with_flush`] to handle partial-line flushing.
    pub transcript: Option<SharedTranscriptSink>,
    /// Format configuration set by `SET DEBUG` flags.
    pub flags: DebugFormatFlags,
    /// CPU snapshot; `Some` when `-P` or `-I` is requested.
    ///
    /// Written by [`crate::env::SimEnvironment`]'s execution loop before each instruction; read by
    /// [`DebugState::write_sim`] during formatting.
    pub snapshot: Option<SharedDebugSnapshot>,
}

impl std::fmt::Debug for DebugState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DebugState {{{} {:?}{} }}",
            if self.transcript.is_some() {
                " transcript: assigned,"
            } else {
                ""
            },
            self.flags,
            if self.snapshot.is_some() {
                ", snapshot: assigned"
            } else {
                ""
            }
        )
    }
}

/// Reference-counted, mutex-protected debug session state.
///
/// Held in `REPLState::debug_state` (CLI side) and `SimEnvironment::debug_state`
/// (simulator side). Both hold a clone of the same `Arc`.
pub type SharedDebugState = Arc<Mutex<DebugState>>;

impl DebugState {
    /// Format and emit a simulator debug record.
    ///
    /// Called by `crate::sim_debug!`. Formats the record according to [`DebugState::flags`], then:
    ///
    /// - If [`DebugState::transcript`] is `Some`: calls [`TranscriptSink::write_debug_with_flush`] to flush
    ///   any partial transcript line before the record.
    ///
    /// - Otherwise: writes directly to [`DebugState::sink`].
    pub fn write_sim(&self, target: &str, message: &str) {
        let line = self.format_sim_line(target, message);

        match &self.transcript {
            Some(ts) => ts.write_debug_with_flush(&line),
            None => {
                let _ = self.sink.lock().unwrap().write_all(line.as_bytes());
            }
        }
    }

    /// Format and emit a CLI debug record.
    ///
    /// Called by `cli_debug!`. Always uses wall-clock timestamp; never includes
    /// PC or instruction count — the CLI is idle while the simulator runs.
    pub fn write_cli(&self, target: &str, message: &str) {
        let micros = micros_since_startup();
        let line = format!("[{:010}] DEBUG {}: {}\n", micros, target, message);

        match &self.transcript {
            Some(ts) => ts.write_debug_with_flush(&line),
            None => {
                let _ = self.sink.lock().unwrap().write_all(line.as_bytes());
            }
        }
    }

    /// Format a simulator debug record according to [`DebugState::flags`].
    ///
    /// Output variants:
    /// ```text
    /// [0001234567] DEBUG PC=017652 #42 ETH_RX: msg   (-P -I)
    /// [0001234567] DEBUG PC=017652 ETH_RX: msg       (-P only)
    /// [#0000000042] DEBUG ETH_RX: msg                (-I only)
    /// [0001234567] DEBUG ETH_RX: msg                 (default)
    /// ```
    fn format_sim_line(&self, target: &str, message: &str) -> String {
        // Read snapshot once under a short-lived read lock, then release before
        // acquiring any sink or transcript lock.
        let snap = self.snapshot.as_ref().map(|s| s.read().unwrap());

        let pc_str = if self.flags.show_pc {
            snap.as_ref()
                .and_then(|s| s.pc.as_deref())
                .map(|pc| format!("PC={} ", pc))
                .unwrap_or_else(|| "PC=? ".to_string())
        } else {
            String::new()
        };

        let count_str = if self.flags.show_instruction_count {
            snap.as_ref()
                .map(|s| format!("#{} ", s.instruction_count))
                .unwrap_or_default()
        } else {
            String::new()
        };

        // When -I only: use instruction count as bracket prefix instead of timestamp.
        let prefix = if self.flags.show_instruction_count && !self.flags.show_pc {
            let count = snap.as_ref().map(|s| s.instruction_count).unwrap_or(0);
            format!("[#{:010}]", count)
        } else {
            format!("[{:010}]", micros_since_startup())
        };

        drop(snap); // release read lock before any write operations

        format!(
            "{} DEBUG {}{}{}: {}\n",
            prefix, pc_str, count_str, target, message
        )
    }
}

/// [`LogWriter`] impl for the debug session.
///
/// Holds a [`SharedDebugState`] so `log::debug!` etc. use the same format flags
/// and sink as `sim_debug!`. Boxed and passed to `log_to_writer()`.
pub struct DebugSinkWriter {
    pub state: SharedDebugState,
}

impl LogWriter for DebugSinkWriter {
    fn write(&self, _now: &mut DeferredNow, record: &Record) -> io::Result<()> {
        let state = self.state.lock().unwrap();
        state.write_sim(record.target(), &record.args().to_string());
        Ok(())
    }

    fn flush(&self) -> io::Result<()> {
        self.state.lock().unwrap().sink.lock().unwrap().flush()
    }

    fn max_log_level(&self) -> log::LevelFilter {
        log::LevelFilter::Trace
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// LogDestination — flexi_logger lifecycle handle
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// Display metadata for `SHOW DEBUG` / `SHOW LOG`.
#[derive(Debug, Display, Clone)]
pub enum LogType {
    Stdout,
    Stderr,
    #[display("file:{_0}")]
    File(String),
}

/// Lifecycle handle for a flexi_logger session.
///
/// Dropping this struct flushes and shuts down the session. The underlying
/// [`SharedSink`] is released when both this handle and any retained
/// `SharedDebugState` / `SharedTranscriptSink` clones are dropped, at which
/// point the file is closed by the OS.
///
/// # Future rotation
///
/// [`LoggerHandle`] provides `set_new_spec` and other methods for reconfiguring
/// the active session. When rotation is implemented, the file swap will update
/// the `Box<dyn IOWrite>` inside the [`SharedSink`]'s `Mutex` — both the
/// `LogWriter` path and the direct macro path will automatically use the new file.
pub struct LogDestination {
    /// flexi_logger session handle; dropping this flushes and shuts down the logger.
    pub handle: LoggerHandle,
    /// Display metadata for `SHOW DEBUG` / `SHOW LOG`.
    pub log_type: LogType,
}

impl LogDestination {
    /// Start a debug log session.
    ///
    /// Returns `(LogDestination, SharedDebugState)`. Store:
    ///
    /// - `LogDestination` in `REPLState::debug_log` for lifecycle management.
    ///
    /// - `SharedDebugState` in `REPLState::debug_state` for `cli_debug!` and to send to the simulator via
    ///   [`crate::env::SimRequest::SetDebugState`].
    ///
    /// # Arguments
    /// - `sink`: output sink — from [`open_sink`], [`stdout_sink`], or a clone of
    ///   the transcript's sink for shared-file mode
    /// - `transcript`: `Some(...)` when `SET DEBUG LOG` is active
    /// - `flags`: format configuration from `SET DEBUG` switches
    /// - `snapshot`: CPU state snapshot; `Some` when `-P` or `-I` is requested
    /// - `log_type`: metadata for `SHOW DEBUG`
    pub fn new_debug(
        sink: SharedSink,
        transcript: Option<SharedTranscriptSink>,
        flags: DebugFormatFlags,
        snapshot: Option<SharedDebugSnapshot>,
        log_type: LogType,
    ) -> Result<(Self, SharedDebugState), LogError> {
        let state = Arc::new(Mutex::new(DebugState {
            sink,
            transcript,
            flags,
            snapshot,
        }));

        let (_, handle) = Logger::try_with_env_or_str("debug")?
            .log_to_writer(Box::new(DebugSinkWriter {
                state: Arc::clone(&state),
            }))
            .build()?;

        Ok((Self { handle, log_type }, state))
    }

    /// Start a transcript log session.
    ///
    /// Returns `(LogDestination, SharedTranscriptSink)`. Store:
    /// - `LogDestination` in `REPLState::transcript_log` for lifecycle management.
    /// - `SharedTranscriptSink` in `REPLState::transcript_sink` for direct
    ///   [`TranscriptSink::push_char`] and [`TranscriptSink::write_line`] calls.
    ///
    /// # Arguments
    /// - `sink`: output sink — from [`open_sink`] or a clone of the debug sink for
    ///   shared-file mode
    /// - `log_type`: metadata for `SHOW LOG`
    pub fn new_transcript(
        sink: SharedSink,
        log_type: LogType,
    ) -> Result<(Self, SharedTranscriptSink), LogError> {
        let transcript = TranscriptSink::new(sink);
        let transcript_clone = Arc::clone(&transcript);

        let (_, handle) = Logger::try_with_env_or_str("info")?
            .log_to_writer(Box::new(TranscriptSinkWriter { inner: transcript }))
            .build()?;

        Ok((Self { handle, log_type }, transcript_clone))
    }
}

impl fmt::Debug for LogDestination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LogDestination")
            .field("log_type", &self.log_type)
            .finish_non_exhaustive()
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Error types
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

#[derive(Debug, Display)]
pub enum LogError {
    #[display("I/O error: {_0}")]
    IoError(String),
    #[display("flexi_logger error: {_0}")]
    FlexiLoggerError(String),
}

impl From<std::io::Error> for LogError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e.to_string())
    }
}

impl From<FlexiLoggerError> for LogError {
    fn from(e: FlexiLoggerError) -> Self {
        Self::FlexiLoggerError(e.to_string())
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Transcript logging:
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// Write a formatted message to stdout and, if active, to the transcript log.
///
/// Usage mirrors `println!` — accepts a format string and arguments. The message is written to stdout
/// unconditionally; if a transcript sink is active it is also written there as a complete line.
///
/// # Example
/// ```ignore
/// sim_println!(context.state, "EXAMINE: {} = {:06o}", resource_name, value);
/// sim_println!(context.state, "Reset complete.");
/// ```
#[macro_export]
macro_rules! sim_println {
    ($state:expr, $($arg:tt)*) => {
        {
            let line = format!($($arg)*);
            println!("{}", line);
            if let Some(ref ts) = $state.transcript_sink {
                ts.write_line(&line);
            }
        }
    };
}

/// Write a formatted message to stderr and, if active, to the transcript log.
///
/// Usage mirrors `eprintln!` — accepts a format string and arguments. The message is written to stderr
/// unconditionally; if a transcript sink is active it is also written there as a complete line.
#[macro_export]
macro_rules! sim_eprintln {
    ($state:expr, $($arg:tt)*) => {
        {
            let line = format!($($arg)*);
            eprintln!("{}", line);
            if let Some(ref ts) = $state.transcript_sink {
                ts.write_line(&line);
            }
        }
    };
}

/// Write output to a writer and transcript log, when active.
///
/// This sends output to a writer via `write!` (not `writeln!`) and also to the transcript log when active.
#[macro_export]
macro_rules! sim_write {
    ($state:expr, $writer:expr, $($arg:tt)*) => {
        {
            let line = format!($($arg)*);
            write!($writer, "{}", line)?;
            if let Some(ref ts) = $state.transcript_sink {
                ts.write(line);
            }
        }
    };
}

/// Write a line of output to a writer and transcript log, when active
///
/// The same
#[macro_export]
macro_rules! sim_writeln {
    ($state:expr, $writer:expr, $($arg:tt)*) => {
        {
            let line = format!($($arg)*);
            writeln!($writer, "{}", line)?;
            if let Some(ref ts) = $state.transcript_sink {
                ts.write_line(&line);
            }
        }
    };
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Tests
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_sink(name: &str) -> (SharedSink, std::path::PathBuf) {
        let path = std::env::temp_dir().join(name);
        let sink = open_sink(path.to_str().unwrap(), false).unwrap();
        (sink, path)
    }

    fn read_and_remove(path: &std::path::PathBuf) -> String {
        let contents = std::fs::read_to_string(path).unwrap_or_default();
        std::fs::remove_file(path).ok();
        contents
    }

    #[test]
    fn test_micros_since_startup_monotonic() {
        let t1 = micros_since_startup();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let t2 = micros_since_startup();
        assert!(t2 > t1);
        assert!(t2 - t1 >= 10_000);
    }

    #[test]
    fn test_transcript_push_char_and_flush() {
        let (sink, path) = temp_sink("simh_transcript_test.log");
        let ts = TranscriptSink::new(sink.clone());

        for c in "sim> go\n".chars() {
            ts.push_char(c);
        }
        sink.lock().unwrap().flush().unwrap();

        let contents = read_and_remove(&path);
        assert!(contents.contains("sim> go\n"));
    }

    #[test]
    fn test_transcript_partial_line_preserved_after_debug_flush() {
        let (sink, path) = temp_sink("simh_partial_test.log");
        let ts = TranscriptSink::new(sink.clone());

        // Partial line — no newline yet
        for c in "sim> ".chars() {
            ts.push_char(c);
        }

        // Debug fires: flushes partial, writes marker, writes debug record
        ts.write_debug_with_flush("[0001234567] DEBUG ETH_RX: pkt\n");

        // Console line completes
        for c in "go 0173000\n".chars() {
            ts.push_char(c);
        }
        sink.lock().unwrap().flush().unwrap();

        let contents = read_and_remove(&path);
        assert!(contents.contains("sim> "), "partial line flushed");
        assert!(contents.contains("-- debug --"), "marker written");
        assert!(contents.contains("DEBUG ETH_RX: pkt"), "debug record written");
        assert!(contents.contains("go 0173000"), "line completed");
    }

    #[test]
    fn test_debug_state_default_format() {
        let (sink, path) = temp_sink("simh_debug_default.log");
        let state = DebugState {
            sink: Arc::clone(&sink),
            transcript: None,
            flags: DebugFormatFlags::default(),
            snapshot: None,
        };

        state.write_sim("ETH_RX", "Received 64 bytes");
        sink.lock().unwrap().flush().unwrap();

        let contents = read_and_remove(&path);
        assert!(contents.contains("] DEBUG ETH_RX: Received 64 bytes"));
    }

    #[test]
    fn test_debug_state_with_pc_and_count() {
        let (sink, path) = temp_sink("simh_debug_pc.log");
        let snapshot = new_debug_snapshot();
        {
            let mut snap = snapshot.write().unwrap();
            snap.pc = Some("017652".to_string());
            snap.instruction_count = 42;
        }

        let state = DebugState {
            sink: Arc::clone(&sink),
            transcript: None,
            flags: DebugFormatFlags {
                show_pc: true,
                show_instruction_count: true,
            },
            snapshot: Some(snapshot),
        };

        state.write_sim("DISK", "Seek complete");
        sink.lock().unwrap().flush().unwrap();

        let contents = read_and_remove(&path);
        assert!(contents.contains("PC=017652"), "PC present");
        assert!(contents.contains("#42"), "instruction count present");
        assert!(contents.contains("DISK: Seek complete"));
    }

    #[test]
    fn test_shared_sink_serialises_two_writers() {
        let (sink, path) = temp_sink("simh_shared_test.log");
        let sink2 = Arc::clone(&sink);

        let _ = sink.lock().unwrap().write_all(b"[1] line one\n");
        let _ = sink2.lock().unwrap().write_all(b"[2] line two\n");
        sink.lock().unwrap().flush().unwrap();

        let contents = read_and_remove(&path);
        assert!(contents.contains("[1] line one"));
        assert!(contents.contains("[2] line two"));
    }

    #[test]
    fn test_shared_sink_accessor() {
        let (sink, _path) = temp_sink("simh_accessor_test.log");
        let ts = TranscriptSink::new(Arc::clone(&sink));
        let extracted = ts.shared_sink();
        // Both Arcs point to the same allocation
        assert!(Arc::ptr_eq(&sink, &extracted));
        std::fs::remove_file(_path).ok();
    }
}
