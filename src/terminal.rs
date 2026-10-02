use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub const START_ATOMIC_UPDATE: &str = "\x1b[?2026h";
pub const END_ATOMIC_UPDATE: &str = "\x1b[?2026l";
pub const HIDE_CURSOR: &str = "\x1b[?25l";
pub const SHOW_CURSOR: &str = "\x1b[?25h";
pub const CLEAR_LINE: &str = "\x1b[2K";
pub const CURSOR_TO_COL_0: &str = "\r";
pub const CURSOR_UP_1: &str = "\x1b[1A";

pub const CURSOR_PREV_LINE_1: &str = "\x1b[1F";
pub const CURSOR_NEXT_LINE_1: &str = "\x1b[1E";

pub struct TerminalRenderer {
    last_printed_line_count: usize,
    buffer: Vec<u8>,
    finished: bool,
}

impl Default for TerminalRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalRenderer {
    pub fn new() -> Self {
        let mut stderr = io::stderr().lock();
        let _ = stderr.write_all(HIDE_CURSOR.as_bytes());
        let _ = stderr.flush();

        TerminalRenderer {
            last_printed_line_count: 0,
            buffer: Vec::with_capacity(4096),
            finished: false,
        }
    }

    pub fn draw(&mut self, nix_output_lines: &[String], nom_output: &str, _pad: bool) {
        self.finished = false;
        let mut stderr = io::stderr().lock();

        self.buffer.clear();
        let _ = self.buffer.write_all(START_ATOMIC_UPDATE.as_bytes());

        if !nix_output_lines.is_empty() {
            // New log lines arrived!
            // First, erase the previous widget from screen:
            if self.last_printed_line_count > 1 {
                for _ in 1..self.last_printed_line_count {
                    let _ = self.buffer.write_all(CURSOR_PREV_LINE_1.as_bytes());
                }
            }
            if self.last_printed_line_count > 0 {
                let _ = self.buffer.write_all(CURSOR_TO_COL_0.as_bytes());
                let _ = self.buffer.write_all(b"\x1b[J"); // clear from cursor to bottom of screen
            }

            // Print the new log lines directly:
            for line in nix_output_lines {
                let trimmed = line.trim_end_matches('\r');
                let _ = self.buffer.write_all(trimmed.as_bytes());
                let _ = self.buffer.write_all(b"\x1b[0m\n");
            }

            // Now draw the new nom_output below the log lines:
            let lines: Vec<&str> = if nom_output.trim().is_empty() {
                Vec::new()
            } else {
                nom_output.lines().collect()
            };
            for (idx, line) in lines.iter().enumerate() {
                if idx > 0 {
                    let _ = self.buffer.write_all(b"\n");
                    let _ = self.buffer.write_all(CURSOR_TO_COL_0.as_bytes());
                }
                let _ = self.buffer.write_all(line.as_bytes());
                let _ = self.buffer.write_all(b"\x1b[K");
            }
            self.last_printed_line_count = lines.len();
        } else {
            // Normal frame: redraw widget in place without blanking!
            let lines: Vec<&str> = if nom_output.trim().is_empty() {
                Vec::new()
            } else {
                nom_output.lines().collect()
            };
            let new_count = lines.len();

            if self.last_printed_line_count > 1 {
                for _ in 1..self.last_printed_line_count {
                    let _ = self.buffer.write_all(CURSOR_PREV_LINE_1.as_bytes());
                }
            }
            if self.last_printed_line_count > 0 {
                let _ = self.buffer.write_all(CURSOR_TO_COL_0.as_bytes());
            }

            for (idx, line) in lines.iter().enumerate() {
                if idx > 0 {
                    let _ = self.buffer.write_all(CURSOR_NEXT_LINE_1.as_bytes());
                }
                let _ = self.buffer.write_all(line.as_bytes());
                let _ = self.buffer.write_all(b"\x1b[K"); // clear to end of line
            }

            // If new output has fewer lines, clear the remaining old lines below:
            if self.last_printed_line_count > new_count {
                for _ in new_count..self.last_printed_line_count {
                    let _ = self.buffer.write_all(CURSOR_NEXT_LINE_1.as_bytes());
                    let _ = self.buffer.write_all(CLEAR_LINE.as_bytes());
                }
                for _ in new_count..self.last_printed_line_count {
                    let _ = self.buffer.write_all(CURSOR_PREV_LINE_1.as_bytes());
                }
            }

            self.last_printed_line_count = new_count;
        }

        let _ = self.buffer.write_all(END_ATOMIC_UPDATE.as_bytes());

        let _ = stderr.write_all(&self.buffer);
        let _ = stderr.flush();
    }

    pub fn finish(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        let mut stderr = io::stderr().lock();
        let _ = stderr.write_all(SHOW_CURSOR.as_bytes());
        let _ = stderr.write_all(b"\n");
        let _ = stderr.flush();
        self.last_printed_line_count = 0;
    }
}

impl Drop for TerminalRenderer {
    fn drop(&mut self) {
        self.finish();
    }
}

pub fn install_signal_handlers() -> Arc<AtomicBool> {
    let interrupted = Arc::new(AtomicBool::new(false));
    let int_clone = Arc::clone(&interrupted);

    // Using ctrlc or simple custom signal handling
    // When SIGINT occurs:
    let _ = ctrlc::set_handler(move || {
        let _ = io::stderr().write_all(SHOW_CURSOR.as_bytes());
        let _ = io::stderr().write_all(b"\n");
        let _ = io::stderr().flush();
        int_clone.store(true, Ordering::SeqCst);
        std::process::exit(130);
    });

    interrupted
}
