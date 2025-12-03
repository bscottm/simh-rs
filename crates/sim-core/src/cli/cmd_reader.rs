// SPDX-License-Identifier: MIT

//! Command reader container
//!
//! The command reader container, [`CmdReader`] manages a stack of inputs sources, [`CmdInputSource`].
//! Sources include [`std::io::stdin`], a vector of `String`-s and [`std::fs::File`].

use crate::logging::SharedTranscriptSink;
use std::fs::File;
use std::io::{stdout, BufRead, BufReader, Error, Read, Write};

/// Command interpreter's input reader
#[derive(Debug)]
pub struct CmdReader {
    /// Input source stack
    sources: Vec<CmdInputSource>,
}

#[derive(Debug)]
pub struct CmdInputSource {
    stream: CmdInputStream,
    lineno: usize,
}

/// Command interpreter's input sources.
#[derive(Debug)]
pub enum CmdInputStream {
    Stdin(BufReader<std::io::Stdin>),
    File(BufReader<File>),
    StringVec(StringVecReader),
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
/// Wrapper for reading from a vector of strings
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// Wrapper for reading from a vector of strings
#[derive(Debug)]
pub struct StringVecReader {
    buffer: String,      // Concatenated input string.
    position: usize,     // Current read position in the buffer.
    current_line: usize, // Current line number, starting at 1.
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// CmdReader implementation:
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

impl CmdReader {
    /// Create a new [`CmdReader`] instance.
    pub fn new() -> Self {
        Self { sources: Vec::new() }
    }

    /// Push a standard input reader on the [`CmdReader::sources`] stack.
    pub fn from_stdin(&mut self) -> std::io::Result<&mut Self> {
        self.sources.push(CmdInputSource {
            stream: CmdInputStream::Stdin(BufReader::new(std::io::stdin())),
            lineno: 0,
        });
        Ok(self)
    }

    /// Push a file reader on the [`CmdReader::sources`] stack.
    pub fn from_file(&mut self, path: &str) -> std::io::Result<&mut Self> {
        let file = File::open(path)?;

        self.sources.push(CmdInputSource {
            stream: CmdInputStream::File(BufReader::new(file)),
            lineno: 0,
        });
        Ok(self)
    }

    /// Push a string vector reader on the [`CmdReader::sources`] stack.
    pub fn from_string_vec(&mut self, lines: Vec<String>) -> std::io::Result<&mut Self> {
        self.sources.push(CmdInputSource {
            stream: CmdInputStream::StringVec(StringVecReader::new(lines)),
            lineno: 0,
        });
        Ok(self)
    }

    /// Read a line of in put from the top [`CmdInputSource`] input source.
    pub fn read_logical_line(
        &mut self,
        prompt: &str,
        transcript_log: Option<&SharedTranscriptSink>,
    ) -> Result<Option<String>, Error> {
        let mut line = String::new();
        let mut result = String::new();
        let current_source: &mut CmdInputSource = match self.sources.last_mut() {
            Some(source) => source,
            None => return Ok(None),
        };

        loop {
            let bytes_read = match &mut current_source.stream {
                CmdInputStream::Stdin(reader) => {
                    let mut stdout = stdout();
                    stdout.write_all(prompt.as_bytes())?;
                    stdout.flush()?;
                    reader.read_line(&mut line)?
                }
                CmdInputStream::File(reader) => reader.read_line(&mut line)?,
                CmdInputStream::StringVec(reader) => reader.read_line(&mut line)?,
            };

            // Output the prompt to the transcript log
            if let Some(ref ts) = transcript_log {
                ts.write(format!("{} ", prompt));
            }

            if bytes_read == 0 {
                return if result.is_empty() {
                    // Done with the source, discard it.
                    self.sources.pop();
                    Ok(None)
                } else {
                    current_source.lineno += 1;
                    // Partially collected line.
                    if let Some(ref ts) = transcript_log {
                        ts.write(result.clone());
                    }
                    Ok(Some(result))
                };
            }

            // Emit to the transcript
            if let Some(ref ts) = transcript_log {
                ts.write(line.clone());
            }

            let trimmed_end = line.trim_end();
            if trimmed_end.ends_with('\\') && !trimmed_end.ends_with("\\\\") {
                result.push_str(&line[..line.len() - (line.len() - trimmed_end.len()) - 1]);
                line.clear();
                continue;
            } else {
                result.push_str(&line);
                break;
            }
        }

        // Signal success
        current_source.lineno += 1;
        // Emit to the transcript
        if let Some(ref ts) = transcript_log {
            ts.flush();
        }
        Ok(Some(result))
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    pub fn clear(&mut self) -> () {
        self.sources.clear();
    }

    pub fn pop(&mut self) -> () {
        self.sources.pop();
    }

    pub fn get_lineno(&self) -> usize {
        self.sources.last().map_or(0, |source| source.lineno)
    }
}

impl StringVecReader {
    /// Construct a new [`StringVecReader`].
    ///
    /// The reader concatenates the strings in the input vector, ensuring that
    /// each string (line) is newline-terminated.
    pub fn new(lines: Vec<String>) -> Self {
        let mut buffer = String::new();

        for mut line in lines {
            if !line.ends_with('\n') {
                line.push('\n'); // Ensure every line ends with a newline.
            }
            buffer.push_str(&line);
        }

        Self {
            buffer,
            position: 0,
            current_line: 1, // Start at the first line.
        }
    }
}

impl BufRead for StringVecReader {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.position >= self.buffer.len() {
            Ok(&[]) // EOF
        } else {
            Ok(&self.buffer.as_bytes()[self.position..])
        }
    }

    fn consume(&mut self, amt: usize) {
        let end_position = std::cmp::min(self.position + amt, self.buffer.len());

        // Update line number for any newlines encountered
        for byte in &self.buffer.as_bytes()[self.position..end_position] {
            if *byte == b'\n' {
                self.current_line += 1;
            }
        }

        self.position = end_position;
    }

    fn read_line(&mut self, buf: &mut String) -> std::io::Result<usize> {
        if self.position >= self.buffer.len() {
            return Ok(0); // EOF
        }

        // Find the end of the current line
        if let Some(line_end) = self.buffer[self.position..].find('\n') {
            let line_end = self.position + line_end + 1; // Include the newline
            buf.push_str(&self.buffer[self.position..line_end]);
            let bytes_read = line_end - self.position;
            self.position = line_end;
            Ok(bytes_read)
        } else {
            // No newline, read until the end of the buffer
            buf.push_str(&self.buffer[self.position..]);
            let bytes_read = self.buffer.len() - self.position;
            self.position = self.buffer.len();
            Ok(bytes_read)
        }
    }
}

impl Read for StringVecReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.position >= self.buffer.len() {
            return Ok(0); // EOF
        }

        // Determine how many bytes to read
        let available_bytes = &self.buffer.as_bytes()[self.position..];
        let bytes_to_copy = std::cmp::min(buf.len(), available_bytes.len());
        buf[..bytes_to_copy].copy_from_slice(&available_bytes[..bytes_to_copy]);

        // Update position and line number
        for byte in &available_bytes[..bytes_to_copy] {
            if *byte == b'\n' {
                self.current_line += 1;
            }
        }
        self.position += bytes_to_copy;

        Ok(bytes_to_copy)
    }
}
