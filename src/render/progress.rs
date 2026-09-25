pub const fn word5_to_word8(n: u8) -> u8 {
    if n < 16 {
        n * 16
    } else {
        (n - 16) + 240
    }
}

pub const fn wire_bits(bit_to_read: u8, bit_to_set: u8, x: u8) -> u8 {
    if (x & (1 << bit_to_read)) != 0 {
        1 << bit_to_set
    } else {
        0
    }
}

pub const fn permute_progress_byte_bits_to_braille_offset(x: u8) -> u8 {
    wire_bits(0, 7, x)
        | wire_bits(1, 5, x)
        | wire_bits(2, 4, x)
        | wire_bits(3, 3, x)
        | wire_bits(4, 6, x)
        | wire_bits(5, 2, x)
        | wire_bits(6, 1, x)
        | wire_bits(7, 0, x)
}

pub const fn progress_byte_to_braille_char(num: u8) -> char {
    let offset = permute_progress_byte_bits_to_braille_offset(num);
    match std::char::from_u32(0x2800 + offset as u32) {
        Some(c) => c,
        None => ' ',
    }
}

const PROGRESS_CHARS: [char; 32] = {
    let mut table = [' '; 32];
    let mut i = 0;
    while i < 32 {
        table[i] = progress_byte_to_braille_char(word5_to_word8(i as u8));
        i += 1;
    }
    table
};

pub fn clamp_to_byte(val: f64) -> u8 {
    if val <= 0.0 {
        0
    } else if val >= 31.0 {
        31
    } else {
        val.ceil() as u8
    }
}

pub fn lookup_progress_char(n: u8) -> char {
    PROGRESS_CHARS[(n.min(31)) as usize]
}

pub fn print_progress_bar(len: usize, progress: f64) -> String {
    if len == 0 {
        return String::new();
    }
    let progress_clamped = progress.clamp(0.0, 1.0);
    let total_progress = progress_clamped * (len as f64);
    let mut out = String::with_capacity(len * 3);

    for pos in 0..len {
        let word5 = clamp_to_byte(32.0 * (total_progress - (pos as f64)));
        out.push(lookup_progress_char(word5));
    }
    out
}

pub fn print_percent(p: f64) -> String {
    format!("{:5.1}%", p * 100.0)
}

pub fn print_bytes(bytes: usize) -> String {
    let sizes = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut val = bytes as f64;
    let mut unit_idx = 0;
    while val >= 1000.0 && unit_idx + 1 < sizes.len() {
        val /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{:.0} {}", val, sizes[unit_idx])
    } else {
        format!("{:.1} {}", val, sizes[unit_idx])
    }
}
