use crate::types::{Derivation, FailType, Host, StorePath};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq)]
pub enum NixOldStyleMessage {
    Uploading(StorePath, Host),
    Downloading(StorePath, Host),
    PlanCopies(usize),
    Build(Derivation, Host),
    PlanBuilds(BTreeSet<Derivation>, Derivation),
    PlanDownloads(f64, f64, BTreeSet<StorePath>),
    Checking(Derivation),
    Failed(Derivation, FailType),
}

use std::borrow::Cow;

pub fn strip_ansi_codes(s: &str) -> Cow<'_, str> {
    let bytes = s.as_bytes();
    if !bytes.contains(&b'\x1b') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if let Some(rel) = memchr::memchr(b'\x1b', &bytes[i..]) {
            let esc_start = i + rel;
            if let Ok(chunk) = std::str::from_utf8(&bytes[i..esc_start]) {
                out.push_str(chunk);
            }
            i = esc_start + 1;
            while i < bytes.len() {
                let b = bytes[i];
                i += 1;
                if b.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            if let Ok(chunk) = std::str::from_utf8(&bytes[i..]) {
                out.push_str(chunk);
            }
            break;
        }
    }
    Cow::Owned(out)
}

fn parse_size_bytes(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }
    let num: f64 = parts[0].parse().ok()?;
    let unit = parts.get(1).unwrap_or(&"");
    let mult = if unit.starts_with("KiB") || unit.starts_with("K") {
        1024.0
    } else if unit.starts_with("MiB") || unit.starts_with("M") {
        1024.0 * 1024.0
    } else if unit.starts_with("GiB") || unit.starts_with("G") {
        1024.0 * 1024.0 * 1024.0
    } else if unit.starts_with("TiB") || unit.starts_with("T") {
        1024.0 * 1024.0 * 1024.0 * 1024.0
    } else {
        1.0
    };
    Some(num * mult)
}

fn extract_tick_content(s: &str) -> Option<&str> {
    let start = s.find('\'')? + 1;
    let end = s[start..].find('\'')? + start;
    Some(&s[start..end])
}

pub fn parse_old_style_chunk(chunk: &str) -> Option<(NixOldStyleMessage, usize)> {
    let mut line_iter = chunk.split_inclusive('\n');
    let first_raw = line_iter.next()?;
    let first_stripped = strip_ansi_codes(first_raw);
    let first = first_stripped.trim();
    if first.is_empty() {
        return None;
    }

    // 1. Plan Builds
    if first.ends_with("will be built:") {
        let mut drvs = BTreeSet::new();
        let mut last_drv = None;
        let mut consumed_bytes = first_raw.len();

        for raw_line in chunk[first_raw.len()..].split_inclusive('\n') {
            let stripped_line = strip_ansi_codes(raw_line);
            let trimmed = stripped_line.trim();
            if trimmed.starts_with("/nix/store/") && trimmed.ends_with(".drv") {
                if let Some(drv) = Derivation::parse(trimmed) {
                    last_drv = Some(drv.clone());
                    drvs.insert(drv);
                    consumed_bytes += raw_line.len();
                    continue;
                }
            }
            break;
        }

        if let Some(last) = last_drv {
            return Some((NixOldStyleMessage::PlanBuilds(drvs, last), consumed_bytes));
        }
    }

    // 2. Plan Downloads
    if (first.starts_with("these paths will be fetched")
        || first.starts_with("this path will be fetched")
        || (first.starts_with("these ") && first.contains("paths will be fetched")))
        && first.contains("download,")
    {
        // e.g. "these paths will be fetched (134.19 MiB download, 1863.82 MiB unpacked):"
        let paren_start = first.find('(')?;
        let paren_end = first.find(')')?;
        let inner = &first[paren_start + 1..paren_end];
        let parts: Vec<&str> = inner.split(',').collect();
        let download_size = parts
            .first()
            .and_then(|p| parse_size_bytes(p.trim().strip_suffix("download")?.trim()))
            .unwrap_or(0.0);
        let unpacked_size = parts
            .get(1)
            .and_then(|p| parse_size_bytes(p.trim().strip_suffix("unpacked")?.trim()))
            .unwrap_or(0.0);

        let mut paths = BTreeSet::new();
        let mut consumed_bytes = first_raw.len();
        for raw_line in chunk[first_raw.len()..].split_inclusive('\n') {
            let stripped_line = strip_ansi_codes(raw_line);
            let trimmed = stripped_line.trim();
            if trimmed.starts_with("/nix/store/") {
                if let Some(sp) = StorePath::parse(trimmed) {
                    paths.insert(sp);
                    consumed_bytes += raw_line.len();
                    continue;
                }
            }
            break;
        }

        return Some((
            NixOldStyleMessage::PlanDownloads(download_size, unpacked_size, paths),
            consumed_bytes,
        ));
    }

    // Single line messages
    let line = first_stripped.trim_end_matches(['\r', '\n']);
    let line_len = first_raw.len();

    // 3. building '...' on '...'... or building '...'...
    if line.starts_with("building '") {
        let drv_str = extract_tick_content(line)?;
        let drv = Derivation::parse(drv_str)?;
        let host = if let Some(on_idx) = line.find("' on '") {
            let host_part = &line[on_idx + 6..];
            let host_str = host_part.split('\'').next().unwrap_or_default();
            Host::parse(host_str)
        } else {
            Host::Localhost
        };
        return Some((NixOldStyleMessage::Build(drv, host), line_len));
    }

    // 4. copying path '...' from '...'... or copying path '...' to '...'...
    if line.starts_with("copying path '") {
        let path_str = extract_tick_content(line)?;
        let sp = StorePath::parse(path_str)?;
        if let Some(from_idx) = line.find("' from '") {
            let host_part = &line[from_idx + 8..];
            let host_str = host_part.split('\'').next().unwrap_or_default();
            return Some((
                NixOldStyleMessage::Downloading(sp, Host::parse(host_str)),
                line_len,
            ));
        } else if let Some(to_idx) = line.find("' to '") {
            let host_part = &line[to_idx + 6..];
            let host_str = host_part.split('\'').next().unwrap_or_default();
            return Some((
                NixOldStyleMessage::Uploading(sp, Host::parse(host_str)),
                line_len,
            ));
        }
    }

    // 5. copying N paths...
    if line.starts_with("copying ") && line.ends_with(" paths...") {
        let num_str = line
            .strip_prefix("copying ")?
            .strip_suffix(" paths...")?
            .trim();
        if let Ok(n) = num_str.parse::<usize>() {
            return Some((NixOldStyleMessage::PlanCopies(n), line_len));
        }
    }

    // 6. checking outputs of '...'...
    if line.starts_with("checking outputs of '") {
        let drv_str = extract_tick_content(line)?;
        let drv = Derivation::parse(drv_str)?;
        return Some((NixOldStyleMessage::Checking(drv), line_len));
    }

    // 7. failed: builder for '...' failed with exit code N
    // or error: builder for '...' failed with exit code N;
    if line.contains("builder for '") && line.contains("failed with exit code") {
        let drv_str = extract_tick_content(line)?;
        let drv = Derivation::parse(drv_str)?;
        let idx = line.find("failed with exit code")? + "failed with exit code".len();
        let code_part = line[idx..].trim_matches(|c: char| !c.is_ascii_digit());
        let code: i32 = code_part.parse().unwrap_or(1);
        return Some((
            NixOldStyleMessage::Failed(drv, FailType::ExitCode(code)),
            line_len,
        ));
    }

    // 8. Cannot build '...'. Reason: builder failed with exit code N.
    if line.contains("Cannot build '") {
        let mut second_iter = chunk.split_inclusive('\n').skip(1);
        if let Some(second_raw) = second_iter.next() {
            let second_stripped = strip_ansi_codes(second_raw);
            let second_line = second_stripped.trim();
            if second_line.contains("failed with exit code") {
                let drv_str = extract_tick_content(line)?;
                let drv = Derivation::parse(drv_str)?;
                let idx = second_line.find("failed with exit code")? + "failed with exit code".len();
                let code_part = second_line[idx..].trim_matches(|c: char| !c.is_ascii_digit());
                let code: i32 = code_part.parse().unwrap_or(1);
                let bytes_consumed = first_raw.len() + second_raw.len();
                return Some((
                    NixOldStyleMessage::Failed(drv, FailType::ExitCode(code)),
                    bytes_consumed,
                ));
            }
        }
    }

    // 9. hash mismatch in fixed-output derivation '...':
    if line.contains("hash mismatch in fixed-output derivation '") {
        let drv_str = extract_tick_content(line)?;
        let drv = Derivation::parse(drv_str)?;
        return Some((
            NixOldStyleMessage::Failed(drv, FailType::HashMismatch),
            line_len,
        ));
    }

    None
}
