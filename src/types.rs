use std::fmt;

pub const STORE_PREFIX: &str = "/nix/store/";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DerivationId(pub usize);

impl fmt::Display for DerivationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DrvId({})", self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StorePathId(pub usize);

impl fmt::Display for StorePathId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PathId({})", self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StorePath {
    pub hash: String,
    pub name: String,
}

impl StorePath {
    pub fn new(hash: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            hash: hash.into(),
            name: name.into(),
        }
    }

    pub fn to_store_path_string(&self) -> String {
        let mut s = String::with_capacity(STORE_PREFIX.len() + 33 + self.name.len());
        s.push_str(STORE_PREFIX);
        s.push_str(&self.hash);
        s.push('-');
        s.push_str(&self.name);
        s
    }

    pub fn parse(s: &str) -> Option<Self> {
        let path = s.strip_prefix(STORE_PREFIX).unwrap_or(s);
        if path.len() < 33 {
            return None;
        }
        let (hash, rest) = path.split_at(32);
        if !rest.starts_with('-') {
            return None;
        }
        let name = &rest[1..];
        // Hash must be valid nixbase32/alphanumeric (32 bytes checked in 8-byte chunks)
        if !is_valid_nix_hash_32(hash.as_bytes()) {
            return None;
        }
        Some(Self {
            hash: hash.to_string(),
            name: name.to_string(),
        })
    }
}

#[inline(always)]
fn is_ascii_alphanumeric_8(chunk: &[u8]) -> bool {
    let mut ok = true;
    for &b in chunk {
        ok &= b.is_ascii_alphanumeric();
    }
    ok
}

#[inline(always)]
fn is_valid_nix_hash_32(bytes: &[u8]) -> bool {
    if bytes.len() != 32 {
        return false;
    }
    is_ascii_alphanumeric_8(&bytes[0..8])
        && is_ascii_alphanumeric_8(&bytes[8..16])
        && is_ascii_alphanumeric_8(&bytes[16..24])
        && is_ascii_alphanumeric_8(&bytes[24..32])
}

impl fmt::Display for StorePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}-{}", STORE_PREFIX, self.hash, self.name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Derivation {
    pub store_path: StorePath,
}

impl Derivation {
    pub fn parse(s: &str) -> Option<Self> {
        let store_path = StorePath::parse(s)?;
        if let Some(real_name) = store_path.name.strip_suffix(".drv") {
            Some(Derivation {
                store_path: StorePath {
                    hash: store_path.hash,
                    name: real_name.to_string(),
                },
            })
        } else {
            None
        }
    }

    pub fn to_drv_string(&self) -> String {
        let mut s = String::with_capacity(STORE_PREFIX.len() + 33 + self.store_path.name.len() + 4);
        s.push_str(STORE_PREFIX);
        s.push_str(&self.store_path.hash);
        s.push('-');
        s.push_str(&self.store_path.name);
        s.push_str(".drv");
        s
    }
}

impl fmt::Display for Derivation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.drv", self.store_path)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HostWithoutContext {
    Localhost,
    Hostname(String),
}

impl HostWithoutContext {
    pub fn as_str(&self) -> &str {
        match self {
            HostWithoutContext::Localhost => "",
            HostWithoutContext::Hostname(h) => h.as_str(),
        }
    }
}

impl fmt::Display for HostWithoutContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostWithoutContext::Localhost => write!(f, "localhost"),
            HostWithoutContext::Hostname(h) => write!(f, "{}", h),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Host {
    Localhost,
    Remote {
        proto: Option<String>,
        user: Option<String>,
        host: String,
    },
}

impl Host {
    pub fn parse(hostname: &str) -> Self {
        let trimmed = hostname.trim();
        if trimmed.is_empty()
            || trimmed == "local"
            || trimmed == "local://"
            || trimmed == "unix"
            || trimmed == "unix://"
            || trimmed == "localhost"
        {
            return Host::Localhost;
        }

        let (proto, rest) = if let Some(idx) = trimmed.find("://") {
            (Some(trimmed[..idx].to_string()), &trimmed[idx + 3..])
        } else {
            (None, trimmed)
        };

        let (user, host) = if let Some(idx) = rest.find('@') {
            (Some(rest[..idx].to_string()), rest[idx + 1..].to_string())
        } else {
            (None, rest.to_string())
        };

        Host::Remote { proto, user, host }
    }

    pub fn hostname_only(&self) -> &str {
        match self {
            Host::Localhost => "localhost",
            Host::Remote { host, .. } => host,
        }
    }

    pub fn without_context(&self) -> HostWithoutContext {
        match self {
            Host::Localhost => HostWithoutContext::Localhost,
            Host::Remote { host, .. } => HostWithoutContext::Hostname(host.clone()),
        }
    }

    pub fn forget_proto(&self) -> Host {
        match self {
            Host::Localhost => Host::Localhost,
            Host::Remote { host, .. } => Host::Remote {
                proto: None,
                user: None,
                host: host.clone(),
            },
        }
    }

    pub fn format_with_proto_context(&self) -> String {
        match self {
            Host::Localhost => "localhost".to_string(),
            Host::Remote { proto, host, .. } => {
                if let Some(p) = proto {
                    format!("{} ({})", host, p)
                } else {
                    host.clone()
                }
            }
        }
    }
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Host::Localhost => write!(f, "localhost"),
            Host::Remote { proto, user, host } => {
                if let Some(p) = proto {
                    write!(f, "{}://", p)?;
                }
                if let Some(u) = user {
                    write!(f, "{}@", u)?;
                }
                write!(f, "{}", host)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FailType {
    ExitCode(i32),
    HashMismatch,
}

impl fmt::Display for FailType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FailType::ExitCode(code) => write!(f, "exit code {}", code),
            FailType::HashMismatch => write!(f, "hash mismatch"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OutputName {
    Out,
    Doc,
    Dev,
    Bin,
    Info,
    Lib,
    Man,
    Dist,
    Other(String),
}

impl OutputName {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "out" => OutputName::Out,
            "doc" => OutputName::Doc,
            "dev" => OutputName::Dev,
            "bin" => OutputName::Bin,
            "info" => OutputName::Info,
            "lib" => OutputName::Lib,
            "man" => OutputName::Man,
            "dist" => OutputName::Dist,
            _ => OutputName::Other(s.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            OutputName::Out => "out",
            OutputName::Doc => "doc",
            OutputName::Dev => "dev",
            OutputName::Bin => "bin",
            OutputName::Info => "info",
            OutputName::Lib => "lib",
            OutputName::Man => "man",
            OutputName::Dist => "dist",
            OutputName::Other(s) => s.as_str(),
        }
    }
}

impl fmt::Display for OutputName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
