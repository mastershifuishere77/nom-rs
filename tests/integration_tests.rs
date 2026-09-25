use nix_output_monitor::engine::monitor_stream;
use nix_output_monitor::render::Config;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

fn find_integration_file(rel_path: &str) -> Option<PathBuf> {
    let candidates = [
        format!(
            "nix-output-monitor/nix-output-monitor/test/integration/{}",
            rel_path
        ),
        format!("nix-output-monitor/test/integration/{}", rel_path),
        format!("test/integration/{}", rel_path),
    ];
    for c in &candidates {
        let p = Path::new(c);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }
    None
}

fn ensure_derivations_instantiated(rel_nix_path: &str, seed: &str) {
    if let Some(path) = find_integration_file(rel_nix_path) {
        if rel_nix_path.starts_with("standard") {
            let _ = std::process::Command::new("nix-build")
                .arg(&path)
                .arg("--argstr")
                .arg("seed")
                .arg(seed)
                .arg("--no-out-link")
                .output();
        } else {
            let _ = std::process::Command::new("nix-instantiate")
                .arg(&path)
                .arg("--argstr")
                .arg("seed")
                .arg(seed)
                .output();
        }
    }
}

#[test]
fn test_integration_standard_json() {
    ensure_derivations_instantiated("standard/default.nix", "json");
    let file_path =
        find_integration_file("standard/stderr.json").expect("Could not find standard/stderr.json");
    let content = fs::read(&file_path).expect("Failed to read standard/stderr.json");

    let cursor = Cursor::new(content);
    let config = Config {
        silent: true,
        piping: true,
    };

    let state = monitor_stream(cursor, true, config);
    let summary = &state.full_summary;

    assert!(
        summary.planned_builds.is_empty(),
        "Expected 0 planned builds, got {:?}",
        summary.planned_builds
    );
    assert!(
        summary.running_builds.is_empty(),
        "Expected 0 running builds, got {:?}",
        summary.running_builds
    );
    assert_eq!(
        summary.completed_builds.len(),
        4,
        "Expected 4 completed builds, got {}",
        summary.completed_builds.len()
    );
}

#[test]
fn test_integration_fail_json() {
    ensure_derivations_instantiated("fail/default.nix", "json");
    let file_path =
        find_integration_file("fail/stderr.json").expect("Could not find fail/stderr.json");
    let content = fs::read(&file_path).expect("Failed to read fail/stderr.json");

    let cursor = Cursor::new(content);
    let config = Config {
        silent: true,
        piping: true,
    };

    let state = monitor_stream(cursor, true, config);
    let summary = &state.full_summary;

    assert_eq!(
        summary.planned_builds.len(),
        1,
        "Expected 1 planned build, got {:?}",
        summary.planned_builds
    );
    assert_eq!(
        summary.failed_builds.len(),
        1,
        "Expected 1 failed build, got {:?}",
        summary.failed_builds
    );
    assert_eq!(
        summary.completed_builds.len(),
        0,
        "Expected 0 completed builds, got {}",
        summary.completed_builds.len()
    );
    assert_eq!(
        summary.running_builds.len(),
        1,
        "Expected 1 running build, got {}",
        summary.running_builds.len()
    );
}

#[test]
fn test_integration_fail_old_style() {
    ensure_derivations_instantiated("fail/default.nix", "old-style");
    let file_path = find_integration_file("fail/stderr").expect("Could not find fail/stderr");
    let content = fs::read(&file_path).expect("Failed to read fail/stderr");

    let cursor = Cursor::new(content);
    let config = Config {
        silent: true,
        piping: true,
    };

    let state = monitor_stream(cursor, false, config);
    let summary = &state.full_summary;

    assert_eq!(
        summary.planned_builds.len(),
        1,
        "Expected 1 planned build, got {:?}",
        summary.planned_builds
    );
    assert_eq!(
        summary.failed_builds.len(),
        1,
        "Expected 1 failed build, got {:?}",
        summary.failed_builds
    );
}
