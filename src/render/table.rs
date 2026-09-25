use unicode_width::UnicodeWidthChar;

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const RED: &str = "\x1b[31m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const BLUE: &str = "\x1b[34m";
pub const MAGENTA: &str = "\x1b[35m";
pub const GREY: &str = "\x1b[90m";

#[inline(always)]
fn is_printable_ascii_8(chunk: &[u8]) -> bool {
    let mut ok = true;
    for &b in chunk {
        ok &= (0x20..=0x7e).contains(&b);
    }
    ok
}

pub fn display_width(s: &str) -> usize {
    let bytes = s.as_bytes();
    let len = bytes.len();

    // Fast-path: if string has no escape codes and all characters are printable ASCII,
    // each byte has width 1 and the total display width is exactly len.
    if !bytes.contains(&b'\x1b') {
        let mut i = 0;
        let mut all_printable = true;
        while i + 8 <= len {
            if !is_printable_ascii_8(&bytes[i..i + 8]) {
                all_printable = false;
                break;
            }
            i += 8;
        }
        if all_printable {
            while i < len {
                if !(0x20..=0x7e).contains(&bytes[i]) {
                    all_printable = false;
                    break;
                }
                i += 1;
            }
            if all_printable {
                return len;
            }
        }
    }

    let mut i = 0;
    let mut width = 0;

    while i < len {
        let b = bytes[i];
        if b == b'\x1b' {
            i += 1;
            while i < len {
                let c = bytes[i];
                i += 1;
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else if b < 0x80 {
            if (0x20..0x7f).contains(&b) {
                width += 1;
                i += 1;
                while i + 8 <= len && is_printable_ascii_8(&bytes[i..i + 8]) {
                    width += 8;
                    i += 8;
                }
            } else {
                i += 1;
            }
        } else if let Some(c) = s[i..].chars().next() {
            width += UnicodeWidthChar::width(c).unwrap_or(0);
            i += c.len_utf8();
        } else {
            i += 1;
        }
    }

    width
}

pub fn truncate_display(s: &str, max_width: usize) -> String {
    // Fast path: if string contains no ANSI escapes and is pure ASCII fitting within max_width,
    // its display width is simply its length and no truncation or formatting is needed.
    if !s.as_bytes().contains(&b'\x1b') && s.len() <= max_width && s.is_ascii() {
        return s.to_string();
    }

    let mut current_width = 0;
    let mut in_escape = false;
    let mut out = String::new();
    let mut had_escape = false;

    for c in s.chars() {
        if in_escape {
            out.push(c);
            if c.is_ascii_alphabetic() {
                in_escape = false;
            }
        } else if c == '\x1b' {
            in_escape = true;
            had_escape = true;
            out.push(c);
        } else {
            let char_width = UnicodeWidthChar::width(c).unwrap_or(0);
            if current_width + char_width > max_width {
                break;
            }
            out.push(c);
            current_width += char_width;
        }
    }

    if in_escape {
        if let Some(pos) = out.rfind('\x1b') {
            out.truncate(pos);
        }
    }

    if had_escape && !out.ends_with(RESET) {
        out.push_str(RESET);
    }

    out
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub codes: Vec<&'static str>,
    pub lcontent: String,
    pub rcontent: String,
    pub width: usize,
    pub lw: usize,
    pub rw: usize,
}

impl Entry {
    pub fn text(t: impl Into<String>) -> Self {
        let rcontent = t.into();
        let rw = display_width(&rcontent);
        Self {
            codes: Vec::new(),
            lcontent: String::new(),
            rcontent,
            width: 1,
            lw: 0,
            rw,
        }
    }

    pub fn header(t: impl Into<String>) -> Self {
        let lcontent = t.into();
        let lw = display_width(&lcontent);
        Self {
            codes: Vec::new(),
            lcontent,
            rcontent: String::new(),
            width: 1,
            lw,
            rw: 0,
        }
    }

    pub fn label(mut self, t: impl Into<String>) -> Self {
        self.lcontent = t.into();
        self.lw = display_width(&self.lcontent);
        self
    }

    pub fn cells(mut self, width: usize) -> Self {
        self.width = width;
        self
    }

    pub fn bold(mut self) -> Self {
        self.codes.push(BOLD);
        self
    }

    pub fn red(mut self) -> Self {
        self.codes.push(RED);
        self
    }

    pub fn green(mut self) -> Self {
        self.codes.push(GREEN);
        self
    }

    pub fn yellow(mut self) -> Self {
        self.codes.push(YELLOW);
        self
    }

    pub fn blue(mut self) -> Self {
        self.codes.push(BLUE);
        self
    }

    pub fn magenta(mut self) -> Self {
        self.codes.push(MAGENTA);
        self
    }

    pub fn grey(mut self) -> Self {
        self.codes.push(GREY);
        self
    }

    #[inline]
    pub fn entry_width(&self) -> usize {
        self.lw + self.rw + if self.lw > 0 && self.rw > 0 { 1 } else { 0 }
    }
}

pub fn markup(style_fn: impl Fn(Entry) -> Entry, text: &str) -> String {
    let entry = style_fn(Entry::text(text));
    render_entry(&entry, entry.entry_width())
}

fn render_entry(entry: &Entry, col_width: usize) -> String {
    let mut out = String::new();
    for code in &entry.codes {
        out.push_str(code);
    }
    out.push_str(&entry.lcontent);

    let needed_spaces = col_width.saturating_sub(entry.lw + entry.rw);
    out.extend(std::iter::repeat_n(' ', needed_spaces));

    out.push_str(&entry.rcontent);
    if !entry.codes.is_empty() {
        out.push_str(RESET);
    }
    out
}

pub fn print_aligned_table(rows: &[Vec<Entry>], sep: &str) -> Vec<String> {
    if rows.is_empty() {
        return Vec::new();
    }

    let num_cols = rows
        .iter()
        .map(|r| r.iter().map(|e| e.width).sum())
        .max()
        .unwrap_or(0);
    let mut col_widths = vec![0; num_cols];

    for row in rows {
        let mut col_idx = 0;
        for entry in row {
            if entry.width == 1 {
                let w = entry.entry_width();
                if col_idx < col_widths.len() && w > col_widths[col_idx] {
                    col_widths[col_idx] = w;
                }
            }
            col_idx += entry.width;
        }
    }

    let sep_w = display_width(sep);
    for row in rows {
        let mut col_idx = 0;
        for entry in row {
            if entry.width > 1 {
                let current_span: usize = (0..entry.width)
                    .map(|offset| {
                        let idx = col_idx + offset;
                        if idx < col_widths.len() {
                            col_widths[idx]
                        } else {
                            0
                        }
                    })
                    .sum::<usize>()
                    + sep_w * (entry.width.saturating_sub(1));
                let needed = entry.entry_width();
                if needed > current_span {
                    let diff = needed - current_span;
                    let extra_per_col = diff / entry.width;
                    let remainder = diff % entry.width;
                    for offset in 0..entry.width {
                        let idx = col_idx + offset;
                        if idx < col_widths.len() {
                            col_widths[idx] +=
                                extra_per_col + if offset < remainder { 1 } else { 0 };
                        }
                    }
                }
            }
            col_idx += entry.width;
        }
    }

    let mut result = Vec::new();
    for row in rows {
        let mut row_str = String::new();
        let mut col_idx = 0;

        for (i, entry) in row.iter().enumerate() {
            if i > 0 {
                row_str.push_str(sep);
            }

            let span_width: usize = (0..entry.width)
                .map(|offset| {
                    let idx = col_idx + offset;
                    if idx < col_widths.len() {
                        col_widths[idx]
                    } else {
                        0
                    }
                })
                .sum::<usize>()
                + sep_w * (entry.width.saturating_sub(1));

            row_str.push_str(&render_entry(entry, span_width));
            col_idx += entry.width;
        }

        result.push(row_str);
    }

    result
}

pub fn prepend_lines(top: &str, mid: &str, bot: &str, rows: &[String]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    if rows.len() == 1 {
        return format!("{}{}", top, rows[0]);
    }

    let mut out = String::new();
    for (i, row) in rows.iter().enumerate() {
        if i == 0 {
            out.push_str(top);
        } else if i == rows.len() - 1 {
            out.push('\n');
            out.push_str(bot);
        } else {
            out.push('\n');
            out.push_str(mid);
        }
        out.push_str(row);
    }

    out
}
