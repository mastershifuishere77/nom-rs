use compact_str::CompactString;
use crate::parser::old_style::strip_ansi_codes;
use crate::types::Host;

#[derive(Clone, Debug, PartialEq)]
pub struct CurlProgress {
    pub host: Host,
    pub done_bytes: usize,
    pub total_bytes: usize,
}

impl CurlProgress {
    pub fn new(host: Host) -> Self {
        Self {
            host,
            done_bytes: 0,
            total_bytes: 0,
        }
    }

    pub fn new_fallback() -> Self {
        Self::new(Host::fallback_curl())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurlProgressSnapshot {
    pub done_bytes: usize,
    pub total_bytes: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CurlLogEvent {
    /// URL target detected (e.g., "trying https://cdn.geekbench.com/...")
    TargetUrl { host: Host },
    /// Parsed progress snapshot
    Progress(CurlProgressSnapshot),
    /// Table header row or noise separator
    HeaderNoise,
    /// Regular log message or error/warning
    RegularLog,
}

pub fn parse_trying_url(line: &str) -> Option<Host> {
    let stripped = strip_ansi_codes(line);
    let trimmed = stripped.trim();
    let url_str = trimmed.strip_prefix("trying ")?.trim();
    if url_str.is_empty() {
        return None;
    }

    let (proto, rest) = if let Some(idx) = url_str.find("://") {
        (Some(CompactString::new(&url_str[..idx])), &url_str[idx + 3..])
    } else {
        (None, url_str)
    };

    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host_part = if let Some(at) = authority.rfind('@') {
        &authority[at + 1..]
    } else {
        authority
    };

    if host_part.is_empty() {
        return Some(Host::fallback_curl());
    }

    Some(Host::Remote {
        proto,
        user: None,
        host: CompactString::new(host_part),
    })
}

pub fn is_curl_header_noise(line: &str) -> bool {
    let stripped = strip_ansi_codes(line);
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.contains("% Total") && trimmed.contains("% Received") {
        return true;
    }
    if trimmed.contains("Dload") && trimmed.contains("Upload") && trimmed.contains("Speed") {
        return true;
    }
    false
}

pub fn parse_curl_size(s: &str) -> Option<usize> {
    let s = s.trim();
    if s == "--" {
        return Some(0);
    }
    if s.is_empty() {
        return None;
    }
    let last = s.chars().last()?;
    if last.is_ascii_alphabetic() {
        let (num_part, _) = s.split_at(s.len() - last.len_utf8());
        let n: f64 = num_part.parse().ok()?;
        let multiplier = match last {
            'k' | 'K' => 1024.0,
            'M' | 'm' => 1024.0 * 1024.0,
            'G' | 'g' => 1024.0 * 1024.0 * 1024.0,
            'T' | 't' => 1024.0 * 1024.0 * 1024.0 * 1024.0,
            'P' | 'p' => 1024.0 * 1024.0 * 1024.0 * 1024.0 * 1024.0,
            _ => return None,
        };
        Some((n * multiplier).round() as usize)
    } else if let Ok(b) = s.parse::<usize>() {
        Some(b)
    } else if let Ok(f) = s.parse::<f64>() {
        Some(f.round() as usize)
    } else {
        None
    }
}

pub fn parse_curl_progress_row(line: &str) -> Option<CurlProgressSnapshot> {
    let stripped = strip_ansi_codes(line);
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        return None;
    }

    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    if tokens.len() < 9 || tokens.len() > 13 {
        return None;
    }

    // Token 0: % Total (integer 0..=100 or --)
    let is_valid_pct0 = tokens[0] == "--"
        || tokens[0]
            .parse::<u8>()
            .map(|p| p <= 100)
            .unwrap_or(false);
    if !is_valid_pct0 {
        return None;
    }

    // Token 1: Total size
    let total_bytes = parse_curl_size(tokens[1])?;

    // Token 2: % Received (integer 0..=100 or --)
    let is_valid_pct2 = tokens[2] == "--"
        || tokens[2]
            .parse::<u8>()
            .map(|p| p <= 100)
            .unwrap_or(false);
    if !is_valid_pct2 {
        return None;
    }

    // Token 3: Received size
    let done_bytes = parse_curl_size(tokens[3])?;

    Some(CurlProgressSnapshot {
        done_bytes,
        total_bytes,
    })
}

pub fn classify_curl_line(line: &str) -> CurlLogEvent {
    let stripped = strip_ansi_codes(line);
    let trimmed = stripped.trim();

    if trimmed.starts_with("trying ") {
        if let Some(host) = parse_trying_url(trimmed) {
            return CurlLogEvent::TargetUrl { host };
        }
    }

    if is_curl_header_noise(trimmed) {
        return CurlLogEvent::HeaderNoise;
    }

    if let Some(snap) = parse_curl_progress_row(trimmed) {
        return CurlLogEvent::Progress(snap);
    }

    CurlLogEvent::RegularLog
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_trying_url_https() {
        let line = "trying https://cdn.geekbench.com/Geekbench-6.7.1-Linux.tar.gz";
        let host = parse_trying_url(line).expect("must parse url");
        assert_eq!(host.hostname_only(), "cdn.geekbench.com");
        assert_eq!(host.format_with_proto_context(), "cdn.geekbench.com (https)");
    }

    #[test]
    fn test_parse_trying_url_http_port() {
        let line = "trying http://mirror.example.com:8080/path/to/archive.tar.xz?v=1#hash";
        let host = parse_trying_url(line).expect("must parse url");
        assert_eq!(host.hostname_only(), "mirror.example.com:8080");
        assert_eq!(host.format_with_proto_context(), "mirror.example.com:8080 (http)");
    }

    #[test]
    fn test_parse_trying_url_empty_fallback() {
        let line = "trying ";
        assert!(parse_trying_url(line).is_none());
    }

    #[test]
    fn test_curl_header_noise() {
        let h1 = "  % Total    % Received % Xferd  Average Speed  Time    Time    Time   Current";
        let h2 = "                                 Dload  Upload  Total   Spent   Left   Speed";
        assert!(is_curl_header_noise(h1));
        assert!(is_curl_header_noise(h2));
        assert_eq!(classify_curl_line(h1), CurlLogEvent::HeaderNoise);
        assert_eq!(classify_curl_line(h2), CurlLogEvent::HeaderNoise);
    }

    #[test]
    fn test_parse_curl_size_units() {
        assert_eq!(parse_curl_size("0"), Some(0));
        assert_eq!(parse_curl_size("57"), Some(57));
        assert_eq!(parse_curl_size("1024k"), Some(1024 * 1024));
        assert_eq!(parse_curl_size("217.5M"), Some((217.5 * 1024.0 * 1024.0) as usize));
        assert_eq!(parse_curl_size("3.85M"), Some((3.85f64 * 1024.0 * 1024.0).round() as usize));
        assert_eq!(parse_curl_size("1.5G"), Some((1.5f64 * 1024.0 * 1024.0 * 1024.0).round() as usize));
        assert_eq!(parse_curl_size("--"), Some(0));
        assert_eq!(parse_curl_size("not-a-number"), None);
    }

    #[test]
    fn test_parse_curl_progress_row() {
        let row_initial = "  0      0   0      0   0      0      0      0                              0";
        let snap1 = parse_curl_progress_row(row_initial).expect("must parse initial zero row");
        assert_eq!(snap1.done_bytes, 0);
        assert_eq!(snap1.total_bytes, 0);

        let row_active = "  1 217.5M   1  3.85M   0      0  2.49M      0   01:27   00:01   01:26  3.85M";
        let snap2 = parse_curl_progress_row(row_active).expect("must parse active row");
        assert_eq!(snap2.done_bytes, (3.85f64 * 1024.0 * 1024.0).round() as usize);
        assert_eq!(snap2.total_bytes, (217.5f64 * 1024.0 * 1024.0).round() as usize);

        let row_22 = " 22 203.0M  22 45.82M   0      0  8.28M      0   00:24   00:05   00:19  9.09M";
        let snap3 = parse_curl_progress_row(row_22).expect("must parse row 22");
        assert_eq!(snap3.done_bytes, (45.82 * 1024.0 * 1024.0) as usize);
        assert_eq!(snap3.total_bytes, (203.0 * 1024.0 * 1024.0) as usize);
    }

    #[test]
    fn test_errors_and_warnings_are_regular_logs() {
        let err = "curl: (56) OpenSSL SSL_read: OpenSSL/3.6.4: error:0A000126:SSL routines::unexpected eof while reading, errno 0";
        let warn = "Warning: Problem (retrying all errors). Retrying in 1 second. 3 retries left.";
        assert_eq!(classify_curl_line(err), CurlLogEvent::RegularLog);
        assert_eq!(classify_curl_line(warn), CurlLogEvent::RegularLog);
    }

    #[test]
    fn test_retry_and_reversion_sequence() {
        // 1. Initial download reaches 14.46M of 217.5M
        let row1 = "  6 217.5M   6 14.46M   0      0  5.63M      0   00:38   00:02   00:36  7.15M";
        let snap1 = parse_curl_progress_row(row1).unwrap();
        assert_eq!(snap1.done_bytes, (14.46f64 * 1024.0 * 1024.0).round() as usize);
        assert_eq!(snap1.total_bytes, (217.5f64 * 1024.0 * 1024.0).round() as usize);

        // 2. Retry resets to 0
        let row2 = "  0      0   0      0   0      0      0      0                              0";
        let snap2 = parse_curl_progress_row(row2).unwrap();
        assert_eq!(snap2.done_bytes, 0);
        assert_eq!(snap2.total_bytes, 0);

        // 3. Resumes with different mirror / total size (203.0M)
        let row3 = "  1 203.0M   1  2.93M   0      0  1.94M      0   01:44   00:01   01:43  2.89M";
        let snap3 = parse_curl_progress_row(row3).unwrap();
        assert_eq!(snap3.done_bytes, (2.93f64 * 1024.0 * 1024.0).round() as usize);
        assert_eq!(snap3.total_bytes, (203.0f64 * 1024.0 * 1024.0).round() as usize);
    }

    #[test]
    fn test_fallback_host() {
        let cp = CurlProgress::new_fallback();
        assert_eq!(cp.host.hostname_only(), "curl");
    }
}
