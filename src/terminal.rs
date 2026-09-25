use crate::render::table::display_width;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use terminal_size::{terminal_size, Width};

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
        }
    }

    pub fn draw(&mut self, nix_output_lines: &[String], nom_output: &str, pad: bool) {
        let mut stderr = io::stderr().lock();
        let nix_lines_count = nix_output_lines.len();
        let reflow_correction: usize = if nix_lines_count == 0 {
            0
        } else {
            let term_width = if let Some((Width(w), _)) = terminal_size() {
                if w > 0 {
                    w as usize
                } else {
                    80
                }
            } else {
                80
            };
            nix_output_lines
                .iter()
                .map(|l| display_width(l) / term_width)
                .sum()
        };

        let nom_lines: Vec<&str> = if nom_output.trim().is_empty() {
            Vec::new()
        } else {
            nom_output.lines().collect()
        };

        let nom_lines_count = nom_lines.len();

        let lines_to_pad = if pad && nom_lines_count > 0 {
            self.last_printed_line_count
                .saturating_sub(reflow_correction + nix_lines_count + nom_lines_count)
        } else {
            0
        };

        self.buffer.clear();
        let _ = self.buffer.write_all(START_ATOMIC_UPDATE.as_bytes());

        // Clear previous output from screen:
        if self.last_printed_line_count == 1 {
            let _ = self.buffer.write_all(CURSOR_TO_COL_0.as_bytes());
        }
        if self.last_printed_line_count > 0 {
            let _ = self.buffer.write_all(CLEAR_LINE.as_bytes());
        }

        for _ in 1..self.last_printed_line_count {
            let _ = self.buffer.write_all(CURSOR_PREV_LINE_1.as_bytes());
            let _ = self.buffer.write_all(CLEAR_LINE.as_bytes());
        }

        let all_lines = nix_output_lines
            .iter()
            .map(|l| l.trim_end_matches('\r'))
            .chain(std::iter::repeat_n("", lines_to_pad))
            .chain(nom_lines.iter().copied());

        for (idx, line) in all_lines.enumerate() {
            if idx == 0 {
                // Stay in line
            } else if idx + reflow_correction < self.last_printed_line_count {
                // Within previously printed area: move down to next line without scrolling!
                let _ = self.buffer.write_all(CURSOR_NEXT_LINE_1.as_bytes());
            } else {
                // Exceeded previously printed area: need a newline
                let _ = self.buffer.write_all(b"\n");
                let _ = self.buffer.write_all(CURSOR_TO_COL_0.as_bytes());
            }
            let _ = self.buffer.write_all(line.as_bytes());
            if idx < nix_lines_count && !nix_output_lines.is_empty() {
                let _ = self.buffer.write_all(b"\x1b[0m");
            }
        }

        if nom_lines_count == 0 && nix_lines_count > 0 {
            let _ = self.buffer.write_all(b"\n");
            let _ = self.buffer.write_all(CURSOR_TO_COL_0.as_bytes());
        }

        let _ = self.buffer.write_all(END_ATOMIC_UPDATE.as_bytes());

        let _ = stderr.write_all(&self.buffer);
        let _ = stderr.flush();

        self.last_printed_line_count = nom_lines_count + lines_to_pad;
    }

    pub fn finish(&mut self) {
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
        let mut stderr = io::stderr().lock();
        let _ = stderr.write_all(SHOW_CURSOR.as_bytes());
        let _ = stderr.write_all(b"\n");
        let _ = stderr.flush();
        int_clone.store(true, Ordering::SeqCst);
        std::process::exit(130);
    });

    interrupted
}
