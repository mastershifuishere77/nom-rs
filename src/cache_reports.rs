use crate::types::HostWithoutContext;
use chrono::{DateTime, NaiveDateTime, Utc};
use fd_lock::RwLock;
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

pub const TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
pub const HISTORY_LIMIT: usize = 10;

pub type BuildReportMap = FxHashMap<(HostWithoutContext, String), BTreeMap<DateTime<Utc>, i64>>;

pub fn get_build_reports_dir() -> PathBuf {
    if let Ok(state_home) = std::env::var("XDG_STATE_HOME") {
        if !state_home.trim().is_empty() {
            return PathBuf::from(state_home).join("nix-output-monitor");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("nix-output-monitor")
    } else {
        PathBuf::from("/tmp/nix-output-monitor")
    }
}

pub fn insert_history_with_limit(
    history: &mut BTreeMap<DateTime<Utc>, i64>,
    time: DateTime<Utc>,
    build_secs: i64,
) {
    history.insert(time, build_secs);
    while history.len() > HISTORY_LIMIT {
        history.pop_first();
    }
}

pub fn calculate_median(history: &BTreeMap<DateTime<Utc>, i64>) -> Option<i64> {
    let len = history.len();
    if len == 0 {
        return None;
    }
    let mut values = [0i64; 16];
    let count = len.min(16);
    for (i, &v) in history.values().take(count).enumerate() {
        values[i] = v;
    }
    let slice = &mut values[..count];
    slice.sort_unstable();
    if count % 2 == 1 {
        Some(slice[count / 2])
    } else {
        let low = slice[(count / 2) - 1];
        let high = slice[count / 2];
        Some((low + high) / 2)
    }
}

pub fn load_build_reports_from_dir(dir: &Path) -> BuildReportMap {
    let csv_path = dir.join("build-reports.csv");
    if !csv_path.exists() {
        return FxHashMap::default();
    }

    let file = match File::open(&csv_path) {
        Ok(f) => f,
        Err(_) => return FxHashMap::default(),
    };

    let reader = BufReader::new(file);
    let mut map: BuildReportMap = FxHashMap::default();

    let mut lines = reader.lines();
    // Skip header line
    let _header = lines.next();

    for line in lines.map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split(',');
        let host_str = match parts.next() {
            Some(s) => s.trim(),
            None => continue,
        };
        let drv_name = match parts.next() {
            Some(s) => s.trim(),
            None => continue,
        };
        let time_str = match parts.next() {
            Some(s) => s.trim(),
            None => continue,
        };
        let secs_str = match parts.next() {
            Some(s) => s.trim(),
            None => continue,
        };

        let host = if host_str.is_empty() || host_str == "localhost" {
            HostWithoutContext::Localhost
        } else {
            HostWithoutContext::Hostname(host_str.into())
        };

        let end_time = match NaiveDateTime::parse_from_str(time_str, TIME_FORMAT) {
            Ok(naive) => DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc),
            Err(_) => continue,
        };

        let build_secs: i64 = match secs_str.parse() {
            Ok(s) => s,
            Err(_) => continue,
        };

        let entry = map.entry((host, drv_name.to_string())).or_default();
        insert_history_with_limit(entry, end_time, build_secs);
    }

    map
}

pub fn save_build_reports_to_dir(dir: &Path, reports: &BuildReportMap) {
    let _ = fs::create_dir_all(dir);
    let csv_path = dir.join("build-reports.csv");
    let mut file = match File::create(csv_path) {
        Ok(f) => f,
        Err(_) => return,
    };

    let _ = writeln!(file, "hostname,derivation name,utc time,build seconds");

    for ((host, drv_name), history) in reports {
        let host_str = match host {
            HostWithoutContext::Localhost => "",
            HostWithoutContext::Hostname(h) => h.as_str(),
        };
        for (end_time, build_secs) in history {
            let formatted_time = end_time.format(TIME_FORMAT);
            let _ = writeln!(
                file,
                "{},{},{},{}",
                host_str, drv_name, formatted_time, build_secs
            );
        }
    }
}

pub fn append_build_reports_locked(items: &[(HostWithoutContext, String, i64)]) {
    if items.is_empty() {
        return;
    }
    let dir = get_build_reports_dir();
    let _ = fs::create_dir_all(&dir);
    let lock_path = dir.join("build-reports.csv.lock");
    let lock_file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
    {
        Ok(f) => f,
        Err(_) => return,
    };

    let mut lock = RwLock::new(lock_file);
    let _guard = lock.write();

    let csv_path = dir.join("build-reports.csv");
    let needs_header = !csv_path.exists();
    let mut file = match OpenOptions::new()
        .create(true)
        .append(true)
        .open(&csv_path)
    {
        Ok(f) => f,
        Err(_) => return,
    };

    if needs_header {
        let _ = writeln!(file, "hostname,derivation name,utc time,build seconds");
    }

    let now_str = Utc::now().format(TIME_FORMAT).to_string();
    for (host, drv_name, build_secs) in items {
        let host_str = match host {
            HostWithoutContext::Localhost => "",
            HostWithoutContext::Hostname(h) => h.as_str(),
        };
        let _ = writeln!(file, "{},{},{},{}", host_str, drv_name, now_str, build_secs);
    }
}

pub fn update_build_reports_locked<F>(update_fn: F) -> BuildReportMap
where
    F: FnOnce(&mut BuildReportMap),
{
    let dir = get_build_reports_dir();
    let _ = fs::create_dir_all(&dir);
    let lock_path = dir.join("build-reports.csv.lock");

    let lock_file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
    {
        Ok(f) => f,
        Err(_) => {
            let mut reports = load_build_reports_from_dir(&dir);
            update_fn(&mut reports);
            save_build_reports_to_dir(&dir, &reports);
            return reports;
        }
    };

    let mut lock = RwLock::new(lock_file);
    let _guard = lock.write();

    let mut reports = load_build_reports_from_dir(&dir);
    update_fn(&mut reports);
    save_build_reports_to_dir(&dir, &reports);

    reports
}

pub fn update_build_reports_in_memory(
    reports: &mut BuildReportMap,
    host: HostWithoutContext,
    drv_name: String,
    build_secs: i64,
) {
    let now = Utc::now();
    let entry = reports.entry((host, drv_name)).or_default();
    insert_history_with_limit(entry, now, build_secs);
}

pub struct BuildReportsWriter {
    sender: crossbeam_channel::Sender<(HostWithoutContext, String, i64)>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Default for BuildReportsWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildReportsWriter {
    pub fn new() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded::<(HostWithoutContext, String, i64)>();
        let handle = std::thread::spawn(move || {
            let mut pending: Vec<(HostWithoutContext, String, i64)> = Vec::new();
            while let Ok(item) = rx.recv() {
                pending.push(item);
                while let Ok(next) = rx.try_recv() {
                    pending.push(next);
                }
                if !pending.is_empty() {
                    append_build_reports_locked(&pending);
                    pending.clear();
                }
            }
        });

        Self {
            sender: tx,
            handle: Some(handle),
        }
    }

    pub fn send(&self, host: HostWithoutContext, drv_name: String, build_secs: i64) {
        let _ = self.sender.send((host, drv_name, build_secs));
    }

    pub fn finish(mut self) {
        drop(self.sender);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
