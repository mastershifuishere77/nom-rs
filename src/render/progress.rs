
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
