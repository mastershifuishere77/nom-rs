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
    if chunk.len() < 8 {
        return false;
    }
    let v = u64::from_ne_bytes(chunk[..8].try_into().unwrap());
    let below = v.wrapping_sub(0x2020_2020_2020_2020);
    let above = 0x7e7e_7e7e_7e7e_7e7e_u64.wrapping_sub(v);
    ((below | above) & 0x8080_8080_8080_8080) == 0
}

#[inline(always)]
fn is_printable_ascii_16(chunk: &[u8]) -> bool {
    is_printable_ascii_8(chunk) && is_printable_ascii_8(&chunk[8..])
}

pub fn display_width(s: &str) -> usize {
    let bytes = s.as_bytes();
    let len = bytes.len();

    // Fast-path: if string has no escape codes and all characters are printable ASCII,
    // each byte has width 1 and the total display width is exactly len.
    if memchr::memchr(b'\x1b', bytes).is_none() {
        let mut i = 0;
        let mut all_printable = true;
        while i + 16 <= len {
            if !is_printable_ascii_16(&bytes[i..i + 16]) {
                all_printable = false;
                break;
            }
            i += 16;
        }
        if all_printable {
            while i + 8 <= len {
                if !is_printable_ascii_8(&bytes[i..i + 8]) {
                    all_printable = false;
                    break;
                }
                i += 8;
            }
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
    if s.len() <= max_width && memchr::memchr(b'\x1b', s.as_bytes()).is_none() && s.is_ascii() {
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EntryCodes {
    arr: [&'static str; 3],
    len: u8,
}

impl EntryCodes {
    pub const fn new() -> Self {
        Self {
            arr: ["", "", ""],
            len: 0,
        }
    }

    pub fn push(&mut self, s: &'static str) {
        if (self.len as usize) < self.arr.len() {
            self.arr[self.len as usize] = s;
            self.len += 1;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn contains(&self, s: &&str) -> bool {
        self.as_slice().contains(s)
    }

    pub fn as_slice(&self) -> &[&'static str] {
        &self.arr[..self.len as usize]
    }
}

impl<'a> IntoIterator for &'a EntryCodes {
    type Item = &'a &'static str;
    type IntoIter = std::slice::Iter<'a, &'static str>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub codes: EntryCodes,
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
            codes: EntryCodes::new(),
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
            codes: EntryCodes::new(),
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

#[inline]
pub fn render_entry_to(entry: &Entry, col_width: usize, out: &mut String) {
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
}

pub fn render_entry(entry: &Entry, col_width: usize) -> String {
    let mut out = String::with_capacity(col_width + 16);
    render_entry_to(entry, col_width, &mut out);
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

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let mut row_str = String::with_capacity(128);
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

            render_entry_to(entry, span_width, &mut row_str);
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

    let total_len: usize = rows.iter().map(|r| r.len() + mid.len() + 1).sum();
    let mut out = String::with_capacity(total_len);
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
