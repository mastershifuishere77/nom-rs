use crate::cache_reports::{
    calculate_median, load_build_reports_from_dir, update_build_reports_in_memory,
    BuildReportsWriter,
};
use crate::parser::derivation::{parse_derivation_content, ParsedDerivation};
use crate::parser::json::{parse_json_line, Activity, NixJsonMessage};
use crate::parser::old_style::{parse_old_style_chunk, NixOldStyleMessage};
use crate::render::table::{BLUE, RESET};
use crate::render::{render_state_to_text, Config};
use crate::sorting::maintain_nom_state;
use crate::state::{
    ActivityStatus, BuildFail, BuildInfo, BuildStatus, CompletedEnd, DerivationId, FailType, Host,
    InputDerivation, NomState, ProgressState, StorePathId, StorePathState, TransferInfo,
};
use crate::store_watcher::StoreWatcher;
use crate::terminal::TerminalRenderer;
use crate::types::{Derivation, StorePath};
use compact_str::CompactString;
use crossbeam_channel::{select, tick, unbounded};
use rustc_hash::FxHashMap;
use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::time::{Duration, Instant};

pub fn monitor_stream<R: Read + Send + 'static>(
    reader: R,
    is_json: bool,
    config: Config,
) -> NomState {
    let start_instant = Instant::now();
    let start_time = 0.0;
    let build_platform = detect_current_system();

    let reports_dir = crate::cache_reports::get_build_reports_dir();
    let build_reports = load_build_reports_from_dir(&reports_dir);

    let mut state = NomState::new(start_time, build_platform, build_reports);
    let watcher = StoreWatcher::new();
    let mut terminal = TerminalRenderer::new();
    let reports_writer = BuildReportsWriter::new();

    let (input_tx, input_rx) = unbounded();

    // Spawn reader thread
    std::thread::spawn(move || {
        let mut buf_reader = BufReader::with_capacity(65536, reader);
        if is_json {
            let mut line = String::new();
            while let Ok(n) = buf_reader.read_line(&mut line) {
                if n == 0 {
                    break;
                }
                let msg = parse_json_line(&line);
                if input_tx.send(InputEvent::Json(msg)).is_err() {
                    break;
                }
                line.clear();
            }
        } else {
            let mut buf = [0u8; 16384];
            let mut accumulated = String::new();
            let mut start = 0;
            while let Ok(n) = buf_reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let chunk = String::from_utf8_lossy(&buf[..n]);
                accumulated.push_str(&chunk);

                while start < accumulated.len() {
                    let rest = &accumulated[start..];
                    if let Some((msg, consumed)) = parse_old_style_chunk(rest) {
                        let line = rest[..consumed].to_string();
                        start += consumed;
                        let _ = input_tx.send(InputEvent::OldStyle(Some(msg), line));
                    } else if let Some(idx) = rest.find('\n') {
                        let line = rest[..=idx].to_string();
                        start += idx + 1;
                        let _ = input_tx.send(InputEvent::OldStyle(None, line));
                    } else {
                        break;
                    }
                }

                if start > 0 {
                    accumulated.drain(..start);
                    start = 0;
                }
            }
            if start < accumulated.len() {
                let _ = input_tx.send(InputEvent::OldStyle(None, accumulated[start..].to_string()));
            }
        }
    });

    let frame_ticker = tick(Duration::from_millis(50));
    let mut dirty = true;
    let mut pending_log_lines: Vec<String> = Vec::new();
    let mut last_draw = Instant::now();
    let mut last_rendered_sec: u64 = 0;
    let mut last_event_now = 0.0f64;
    let mut next_now = || -> f64 {
        let raw = start_instant.elapsed().as_secs_f64();
        if raw <= last_event_now {
            last_event_now += 1e-9;
        } else {
            last_event_now = raw;
        }
        last_event_now
    };

    'main_loop: loop {
        select! {
            recv(input_rx) -> event => {
                let mut current_event = event;
                let batch_start = Instant::now();
                let mut batch_count = 0;
                loop {
                    let now = next_now();
                    match current_event {
                        Ok(InputEvent::Json(json_msg)) => {
                            let changed = process_json_message(&mut state, json_msg, &watcher, &mut pending_log_lines, now, &reports_writer);
                            if changed {
                                dirty = true;
                            }
                        }
                        Ok(InputEvent::OldStyle(msg_opt, raw_line)) => {
                            let changed = process_old_style_message(&mut state, msg_opt, raw_line, &watcher, &mut pending_log_lines, now, &reports_writer);
                            if changed {
                                dirty = true;
                            }
                        }
                        Err(_) => {
                            // EOF on input
                            break 'main_loop;
                        }
                    }

                    batch_count += 1;
                    if batch_count >= 50 || batch_start.elapsed() >= Duration::from_millis(16) {
                        break;
                    }

                    match input_rx.try_recv() {
                        Ok(next_event) => current_event = Ok(next_event),
                        Err(_) => break,
                    }
                }

                // If dirty and at least 50ms elapsed since last draw (or logs need to be printed), redraw now!
                let now_instant = Instant::now();
                if (dirty || !pending_log_lines.is_empty())
                    && now_instant.duration_since(last_draw) >= Duration::from_millis(50)
                {
                    let current_now = start_instant.elapsed().as_secs_f64();
                    maintain_nom_state(&mut state, current_now);
                    let rendered = render_state_to_text(&state, config, current_now);
                    terminal.draw(&pending_log_lines, &rendered, !config.silent);
                    pending_log_lines.clear();
                    dirty = false;
                    last_draw = now_instant;
                    last_rendered_sec = current_now.floor() as u64;
                }
            }
            recv(watcher.event_receiver) -> finished_build => {
                let now = next_now();
                if let Ok((host, drv_id)) = finished_build {
                    finish_build_by_drv_id(&mut state, &host, drv_id, now, &reports_writer);
                    dirty = true;
                }
                while let Ok((host, drv_id)) = watcher.event_receiver.try_recv() {
                    let now = next_now();
                    finish_build_by_drv_id(&mut state, &host, drv_id, now, &reports_writer);
                    dirty = true;
                }
            }
            recv(frame_ticker) -> _ => {
                let now_instant = Instant::now();
                let current_now = start_instant.elapsed().as_secs_f64();
                let current_sec = current_now.floor() as u64;
                let second_advanced = current_sec > last_rendered_sec;

                if dirty || !pending_log_lines.is_empty() || second_advanced {
                    maintain_nom_state(&mut state, current_now);
                    let rendered = render_state_to_text(&state, config, current_now);
                    terminal.draw(&pending_log_lines, &rendered, !config.silent);
                    pending_log_lines.clear();
                    dirty = false;
                    last_draw = now_instant;
                    last_rendered_sec = current_sec;
                }
            }
        }
    }

    // Finalizer
    watcher.flush_existing();
    let now = start_instant.elapsed().as_secs_f64();
    while let Ok((host, drv_id)) = watcher.event_receiver.try_recv() {
        finish_build_by_drv_id(&mut state, &host, drv_id, now, &reports_writer);
    }
    state.progress_state = ProgressState::Finished;
    maintain_nom_state(&mut state, now);
    let final_render = render_state_to_text(&state, config, now);
    terminal.draw(&pending_log_lines, &final_render, false);
    terminal.finish();

    // Flush and wait for background reports writer to finish
    reports_writer.finish();

    state
}

enum InputEvent {
    Json(NixJsonMessage),
    OldStyle(Option<NixOldStyleMessage>, String),
}

fn detect_current_system() -> Option<String> {
    if let Ok(sys) = std::env::var("NIX_SYSTEM") {
        let trimmed = sys.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Some("x86_64-linux".to_string()),
        ("aarch64", "linux") => Some("aarch64-linux".to_string()),
        ("i686", "linux") => Some("i686-linux".to_string()),
        ("riscv64", "linux") => Some("riscv64-linux".to_string()),
        ("x86_64", "macos") => Some("x86_64-darwin".to_string()),
        ("aarch64", "macos") => Some("aarch64-darwin".to_string()),
        (arch, os) => Some(format!("{}-{}", arch, os)),
    }
}

fn read_and_parse_derivation(drv: &Derivation) -> Option<(Derivation, ParsedDerivation)> {
    let drv_path_str = drv.to_drv_string();
    let drv_path = Path::new(&drv_path_str);
    let content = fs::read_to_string(drv_path).ok()?;
    let parsed = parse_derivation_content(&content).ok()?;
    Some((drv.clone(), parsed))
}

pub fn parallel_prefetch_derivations(drvs: &[Derivation]) -> Vec<(Derivation, ParsedDerivation)> {
    if drvs.len() < 16 {
        return drvs.iter().filter_map(read_and_parse_derivation).collect();
    }

    use rayon::prelude::*;
    drvs.par_iter()
        .filter_map(read_and_parse_derivation)
        .collect()
}

fn process_json_message(
    state: &mut NomState,
    msg: NixJsonMessage,
    watcher: &StoreWatcher,
    logs: &mut Vec<String>,
    now: f64,
    reports_writer: &BuildReportsWriter,
) -> bool {
    state.set_input_received();

    match msg {
        NixJsonMessage::Message(action) => {
            let stripped = crate::parser::old_style::strip_ansi_codes(&action.message);
            let trimmed = stripped.trim();
            if trimmed.starts_with("error:") {
                let err_core = trimmed;
                if !state.nix_errors.iter().any(|e| {
                    crate::parser::old_style::strip_ansi_codes(e).trim() == err_core
                }) {
                    state.nix_errors.push(action.message.clone());
                }
                logs.push(action.message.clone());
                if let Some((NixOldStyleMessage::Failed(drv, fail_type), _)) =
                    parse_old_style_chunk(&format!("{}\n", stripped))
                {
                    let drv_id = lookup_derivation(state, &drv);
                    mark_failed_build(state, drv_id, fail_type, now);
                }
                return true;
            } else if trimmed.starts_with("trace:") {
                if !state.nix_traces.iter().any(|t| {
                    crate::parser::old_style::strip_ansi_codes(t).trim() == trimmed
                }) {
                    state.nix_traces.push(action.message.clone());
                }
                logs.push(action.message.clone());
                return true;
            } else if action.message.starts_with("evaluating file '") {
                if let Some(suffix) = action.message.strip_prefix("evaluating file '") {
                    let file_name = suffix.trim_end_matches('\'').to_string();
                    state.evaluation_state.count += 1;
                    state.evaluation_state.last_file_name = Some(file_name);
                    state.evaluation_state.at = now;
                    return true;
                }
            } else if (action.level as u64) <= 3 && (action.level as u64) > 0 {
                logs.push(action.message.clone());

                if trimmed.starts_with("/nix/store/") {
                    if let Some(drv) = Derivation::parse(trimmed) {
                        let drv_id = lookup_derivation(state, &drv);
                        let old_status = state.get_derivation(drv_id).build_status.clone();
                        update_derivation_state(state, drv_id, old_status, BuildStatus::Planned);
                        return true;
                    } else if let Some(sp) = StorePath::parse(trimmed) {
                        let path_id = state.get_store_path_id(&sp);
                        let old_states = state.get_store_path(path_id).states.clone();
                        let mut new_states = old_states.clone();
                        new_states.insert(StorePathState::DownloadPlanned);
                        update_store_path_states(state, path_id, old_states, new_states);
                        return true;
                    }
                }
            }
            false
        }
        NixJsonMessage::Result(action) => match action.result {
            crate::parser::json::ActivityResult::BuildLogLine(line) => {
                use crate::parser::curl::{classify_curl_line, CurlLogEvent, CurlProgress};

                let event = classify_curl_line(&line);
                let mut changed = false;

                let (drv_id_opt, prefix) = if let Some(act) = state.activities.get(&action.id) {
                    let drv_id = match &act.activity {
                        Activity::Build { drv, .. } => state.derivation_ids.get(drv).copied(),
                        _ => None,
                    };
                    (drv_id, act.prefix.clone())
                } else {
                    (None, CompactString::default())
                };

                match event {
                    CurlLogEvent::TargetUrl { host } => {
                        if let Some(act) = state.activities.get_mut(&action.id) {
                            let cp = act
                                .curl_progress
                                .get_or_insert_with(|| CurlProgress::new(host.clone()));
                            cp.host = host.clone();
                        }
                        if let Some(drv_id) = drv_id_opt {
                            let d_cp = state
                                .get_derivation_mut(drv_id)
                                .curl_progress
                                .get_or_insert_with(|| CurlProgress::new(host.clone()));
                            d_cp.host = host;
                        }
                        changed = true;
                    }
                    CurlLogEvent::Progress(snap) => {
                        if let Some(act) = state.activities.get_mut(&action.id) {
                            let cp = act
                                .curl_progress
                                .get_or_insert_with(CurlProgress::new_fallback);
                            cp.done_bytes = snap.done_bytes;
                            cp.total_bytes = snap.total_bytes;
                        }
                        if let Some(drv_id) = drv_id_opt {
                            let d_cp = state
                                .get_derivation_mut(drv_id)
                                .curl_progress
                                .get_or_insert_with(CurlProgress::new_fallback);
                            d_cp.done_bytes = snap.done_bytes;
                            d_cp.total_bytes = snap.total_bytes;
                        }
                        changed = true;
                    }
                    CurlLogEvent::HeaderNoise | CurlLogEvent::RegularLog => {}
                }

                if prefix.is_empty() {
                    logs.push(line);
                } else {
                    let mut full = String::with_capacity(prefix.len() + line.len());
                    full.push_str(&prefix);
                    full.push_str(&line);
                    logs.push(full);
                }

                changed
            }
            crate::parser::json::ActivityResult::SetPhase(phase) => {
                if let Some(act) = state.activities.get_mut(&action.id) {
                    act.phase = Some(phase);
                    true
                } else {
                    false
                }
            }
            crate::parser::json::ActivityResult::Progress(progress) => {
                let mut changed = false;

                // Find store path associated with this download activity to maintain its known total size
                let copy_path_opt = state
                    .activity_parents
                    .get(&action.id)
                    .and_then(|p_id| state.activities.get(p_id))
                    .or_else(|| state.activities.get(&action.id))
                    .and_then(|act| match &act.activity {
                        Activity::CopyPath { path, .. } => Some(path.clone()),
                        _ => None,
                    });

                let known_max = if let Some(path) = copy_path_opt {
                    let path_id = state.get_store_path_id(&path);
                    let entry = state
                        .max_download_sizes
                        .entry(path_id)
                        .or_insert(progress.expected);
                    *entry = (*entry).max(progress.expected);
                    *entry
                } else {
                    progress.expected
                };

                if let Some(act) = state.activities.get_mut(&action.id) {
                    let prev_expected = act.progress.as_ref().map_or(0, |p| p.expected);
                    let mut p = progress.clone();
                    p.expected = p.expected.max(prev_expected).max(known_max);
                    act.progress = Some(p);
                    changed = true;
                }
                if let Some(&parent_id) = state.activity_parents.get(&action.id) {
                    if let Some(parent_act) = state.activities.get_mut(&parent_id) {
                        let prev_expected = parent_act
                            .file_transfer_progress
                            .as_ref()
                            .map_or(0, |p| p.expected);
                        let mut p = progress;
                        p.expected = p.expected.max(prev_expected).max(known_max);
                        parent_act.file_transfer_progress = Some(p);
                        changed = true;
                    }
                }
                changed
            }
            _ => false,
        },
        NixJsonMessage::Start(action) => {
            let prefix = get_activity_prefix_for_activity(state, &action.activity);
            if !action.text.is_empty() && (action.level as u64) <= 3 {
                logs.push(format!("{}{}", prefix, action.text));
            }

            let changed = match &action.activity {
                Activity::Build { drv, host } => {
                    mark_building(state, drv, host, now, Some(action.id));
                    true
                }
                Activity::CopyPath { path, from, to } => {
                    let path_id = state.get_store_path_id(path);
                    if *to == Host::Localhost {
                        start_downloading(state, path_id, from.clone(), now, Some(action.id));
                        true
                    } else if *from == Host::Localhost {
                        start_uploading(state, path_id, to.clone(), now, Some(action.id));
                        true
                    } else {
                        false
                    }
                }
                Activity::FileTransfer(_) => true,
                _ => false,
            };

            if let Some(parent_id) = action.parent {
                state.activity_parents.insert(action.id, parent_id);
            }

            state.activities.insert(
                action.id,
                ActivityStatus {
                    activity: action.activity,
                    phase: None,
                    progress: None,
                    file_transfer_progress: None,
                    curl_progress: None,
                    prefix: CompactString::new(&prefix),
                },
            );

            changed
        }
        NixJsonMessage::Stop(action) => {
            state.activity_parents.remove(&action.id);
            if let Some(act_status) = state.activities.get(&action.id).cloned() {
                match act_status.activity {
                    Activity::CopyPath { path, from, to } => {
                        let path_id = state.get_store_path_id(&path);
                        if to == Host::Localhost {
                            finish_downloading(state, path_id, from, now);
                            return true;
                        } else if from == Host::Localhost {
                            finish_uploading(state, path_id, to, now);
                            return true;
                        }
                    }
                    Activity::Build { drv, host } => {
                        let drv_id = lookup_derivation(state, &drv);
                        if let Some(out_path) = state.derivation_to_any_out_path(drv_id) {
                            watcher.subscribe(out_path, (host, drv_id));
                        } else {
                            // No out output, finish immediately
                            finish_build_by_drv_id(state, &host, drv_id, now, reports_writer);
                            return true;
                        }
                    }
                    _ => {
                        state.activities.remove(&action.id);
                    }
                }
            } else {
                state.activities.remove(&action.id);
            }
            false
        }
        NixJsonMessage::Plain(raw) => {
            let stripped = crate::parser::old_style::strip_ansi_codes(&raw);
            let trimmed = stripped.trim();
            if trimmed.starts_with("error:") {
                let err_core = trimmed;
                if !state.nix_errors.iter().any(|e| {
                    crate::parser::old_style::strip_ansi_codes(e).trim() == err_core
                }) {
                    state.nix_errors.push(raw.clone());
                }
                if let Some((NixOldStyleMessage::Failed(drv, fail_type), _)) =
                    parse_old_style_chunk(&format!("{}\n", stripped))
                {
                    let drv_id = lookup_derivation(state, &drv);
                    mark_failed_build(state, drv_id, fail_type, now);
                }
            } else if trimmed.starts_with("trace:")
                && !state.nix_traces.iter().any(|t| {
                    crate::parser::old_style::strip_ansi_codes(t).trim() == trimmed
                })
            {
                state.nix_traces.push(raw.clone());
            }
            if !raw.starts_with("debug: nixos_rebuild.") {
                logs.push(raw);
            }
            true
        }
        NixJsonMessage::ParseError(err) => {
            logs.push(format!("nom-rs parse error: {}", err));
            false
        }
    }
}

fn process_old_style_message(
    state: &mut NomState,
    msg_opt: Option<NixOldStyleMessage>,
    raw_line: String,
    watcher: &StoreWatcher,
    logs: &mut Vec<String>,
    now: f64,
    reports_writer: &BuildReportsWriter,
) -> bool {
    state.set_input_received();

    if let Some(msg) = msg_opt {
        match msg {
            NixOldStyleMessage::Uploading(path, host) => {
                let path_id = state.get_store_path_id(&path);
                finish_uploading(state, path_id, host, now);
                true
            }
            NixOldStyleMessage::Downloading(path, host) => {
                let path_id = state.get_store_path_id(&path);
                finish_downloading(state, path_id, host.clone(), now);
                if let Some(prod_drv) = state.out_path_to_derivation(path_id) {
                    finish_build_by_drv_id(state, &host, prod_drv, now, reports_writer);
                }
                true
            }
            NixOldStyleMessage::PlanCopies(_) => false,
            NixOldStyleMessage::Build(drv, host) => {
                mark_building(state, &drv, &host, now, None);
                let drv_id = lookup_derivation(state, &drv);
                if let Some(out_path) = state.derivation_to_any_out_path(drv_id) {
                    watcher.subscribe(out_path, (host, drv_id));
                }
                true
            }
            NixOldStyleMessage::PlanBuilds(drvs, _) => {
                let uncached: Vec<Derivation> = drvs
                    .iter()
                    .filter(|drv| {
                        if state.parsed_drv_cache.contains_key(*drv) {
                            return false;
                        }
                        if let Some(&id) = state.derivation_ids.get(*drv) {
                            !state.get_derivation(id).cached
                        } else {
                            true
                        }
                    })
                    .cloned()
                    .collect();
                if uncached.len() >= 16 {
                    let prefetched = parallel_prefetch_derivations(&uncached);
                    for (d, p) in prefetched {
                        state.parsed_drv_cache.insert(d, p);
                    }
                }
                for drv in drvs {
                    let drv_id = lookup_derivation(state, &drv);
                    let old_status = state.get_derivation(drv_id).build_status.clone();
                    update_derivation_state(state, drv_id, old_status, BuildStatus::Planned);
                }
                true
            }
            NixOldStyleMessage::PlanDownloads(download_size, _, paths) => {
                state.planned_download_bytes = Some(download_size as usize);
                for p in paths {
                    let path_id = state.get_store_path_id(&p);
                    let old_states = state.get_store_path(path_id).states.clone();
                    let mut new_states = old_states.clone();
                    new_states.insert(StorePathState::DownloadPlanned);
                    update_store_path_states(state, path_id, old_states, new_states);
                }
                true
            }
            NixOldStyleMessage::Checking(drv) => {
                mark_building(state, &drv, &Host::Localhost, now, None);
                true
            }
            NixOldStyleMessage::Failed(drv, fail_type) => {
                let drv_id = lookup_derivation(state, &drv);
                mark_failed_build(state, drv_id, fail_type, now);
                logs.push(raw_line);
                true
            }
        }
    } else {
        let stripped = crate::parser::old_style::strip_ansi_codes(&raw_line);
        let trimmed = stripped.trim();

        if trimmed.starts_with("error:") {
            let err_core = trimmed;
            if !state.nix_errors.iter().any(|e| {
                crate::parser::old_style::strip_ansi_codes(e).trim() == err_core
            }) {
                state.nix_errors.push(raw_line.clone());
            }
        } else if trimmed.starts_with("trace:")
            && !state.nix_traces.iter().any(|t| {
                crate::parser::old_style::strip_ansi_codes(t).trim() == trimmed
            })
        {
            state.nix_traces.push(raw_line.clone());
        }

        if let Some(idx) = trimmed.find("> ") {
            let prefix = &trimmed[..idx];
            let body = &trimmed[idx + 2..];
            use crate::parser::curl::{classify_curl_line, CurlLogEvent, CurlProgress};
            let event = classify_curl_line(body);

            let drv_id_opt = state.derivation_infos.iter().enumerate().find_map(|(idx, d)| {
                if matches!(d.build_status, BuildStatus::Building(_)) {
                    if d.get_report_name() == prefix || d.name.store_path.name.starts_with(prefix) {
                        Some(DerivationId(idx))
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

            match event {
                CurlLogEvent::TargetUrl { host } => {
                    if let Some(drv_id) = drv_id_opt {
                        let d_cp = state
                            .get_derivation_mut(drv_id)
                            .curl_progress
                            .get_or_insert_with(|| CurlProgress::new(host.clone()));
                        d_cp.host = host;
                    }
                    logs.push(raw_line);
                    return true;
                }
                CurlLogEvent::Progress(snap) => {
                    if let Some(drv_id) = drv_id_opt {
                        let d_cp = state
                            .get_derivation_mut(drv_id)
                            .curl_progress
                            .get_or_insert_with(CurlProgress::new_fallback);
                        d_cp.done_bytes = snap.done_bytes;
                        d_cp.total_bytes = d_cp.total_bytes.max(snap.total_bytes);
                    }
                    logs.push(raw_line);
                    return true;
                }
                CurlLogEvent::HeaderNoise => {
                    logs.push(raw_line);
                    return false;
                }
                CurlLogEvent::RegularLog => {}
            }
        }

        if trimmed.starts_with("/nix/store/") {
            if trimmed.ends_with(".drv") {
                if let Some(drv) = Derivation::parse(trimmed) {
                    let drv_id = lookup_derivation(state, &drv);
                    let old_status = state.get_derivation(drv_id).build_status.clone();
                    if matches!(old_status, BuildStatus::Unknown) {
                        update_derivation_state(state, drv_id, old_status, BuildStatus::Planned);
                        return true;
                    }
                }
            } else if let Some(sp) = StorePath::parse(trimmed) {
                let path_id = state.get_store_path_id(&sp);
                let old_states = state.get_store_path(path_id).states.clone();
                if old_states.is_empty() {
                    let mut new_states = old_states.clone();
                    new_states.insert(StorePathState::DownloadPlanned);
                    update_store_path_states(state, path_id, old_states, new_states);
                    return true;
                }
            }
        }
        logs.push(raw_line);
        false
    }
}

pub fn lookup_derivation(state: &mut NomState, drv: &Derivation) -> DerivationId {
    let drv_id = state.get_derivation_id(drv);
    if state.get_derivation(drv_id).cached {
        return drv_id;
    }

    let parsed_opt = if let Some(p) = state.parsed_drv_cache.remove(drv) {
        Some(p)
    } else {
        let drv_path_str = drv.to_drv_string();
        let drv_path = Path::new(&drv_path_str);
        if let Ok(content) = fs::read_to_string(drv_path) {
            parse_derivation_content(&content).ok()
        } else {
            None
        }
    };

    if let Some(parsed) = parsed_opt {
        // Prefetch any uncached dependencies in parallel across CPU threads!
        let uncached_deps: Vec<Derivation> = parsed
            .input_drvs
            .keys()
            .filter(|dep| {
                if state.parsed_drv_cache.contains_key(*dep) {
                    return false;
                }
                if let Some(&dep_id) = state.derivation_ids.get(*dep) {
                    !state.get_derivation(dep_id).cached
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        if uncached_deps.len() >= 16 {
            let prefetched = parallel_prefetch_derivations(&uncached_deps);
            for (d, p) in prefetched {
                state.parsed_drv_cache.insert(d, p);
            }
        }

        let mut output_map = FxHashMap::default();
        for (name, sp) in parsed.outputs {
            let sp_id = state.get_store_path_id(&sp);
            state.get_store_path_mut(sp_id).producer = Some(drv_id);
            output_map.insert(name, sp_id);
        }

        let mut input_sources = BTreeSet::new();
        for src in parsed.input_srcs {
            let sp_id = state.get_store_path_id(&src);
            state.get_store_path_mut(sp_id).input_for.insert(drv_id);
            input_sources.insert(sp_id);
        }

        let mut input_derivations = Vec::with_capacity(parsed.input_drvs.len());
        let mut children_needing_propagation = Vec::new();
        for (dep_drv, dep_outputs) in parsed.input_drvs {
            let dep_id = lookup_derivation(state, &dep_drv);
            let dep_mut = state.get_derivation_mut(dep_id);
            let was_first_parent = dep_mut.derivation_parents.is_empty();
            let is_new_link = dep_mut.derivation_parents.insert(drv_id);
            if was_first_parent && dep_mut.is_root {
                dep_mut.is_root = false;
                if let Some(pos) = state.forest_roots.iter().position(|&r| r == dep_id) {
                    state.forest_roots.swap_remove(pos);
                }
            }
            // If this is a newly created parent link and the child has a
            // non-empty summary (status changes already happened), we need
            // to propagate the child's summary upward into the new parent
            // chain so that sort keys reflect the full subtree.
            if is_new_link {
                let dep = state.get_derivation(dep_id);
                if !dep.dependency_summary.is_empty()
                    || !matches!(dep.build_status, BuildStatus::Unknown)
                {
                    children_needing_propagation.push(dep_id);
                }
            }
            input_derivations.push(InputDerivation {
                derivation: dep_id,
                outputs: dep_outputs.into_iter().collect(),
            });
        }
        // Propagate pre-existing child summaries up the new parent chain.
        for child_id in children_needing_propagation {
            let child_summary = state.get_derivation(child_id).dependency_summary.clone();
            let child_status = state.get_derivation(child_id).build_status.clone();
            // Merge child's own build status into drv_id's summary
            if !matches!(child_status, BuildStatus::Unknown) {
                NomState::update_summary_for_derivation_opt(
                    &mut state.get_derivation_mut(drv_id).dependency_summary,
                    &BuildStatus::Unknown,
                    &child_status,
                    child_id,
                    false,
                );
            }
            // Merge child's dependency_summary into drv_id's summary
            state.get_derivation_mut(drv_id).dependency_summary.merge(&child_summary);
        }
        input_derivations.sort_by(|a, b| {
            state.get_derivation(a.derivation).name.cmp(&state.get_derivation(b.derivation).name)
        });

        let is_root_needed = {
            let drv_mut = state.get_derivation_mut(drv_id);
            drv_mut.outputs = output_map;
            drv_mut.input_sources = input_sources;
            drv_mut.input_derivations = input_derivations;
            drv_mut.cached = true;
            drv_mut.platform = Some(parsed.platform);
            drv_mut.pname = parsed.pname;

            if drv_mut.derivation_parents.is_empty() && !drv_mut.is_root {
                drv_mut.is_root = true;
                true
            } else {
                false
            }
        };
        state.touched_ids.insert(drv_id.0 as u32);

        if is_root_needed {
            state.forest_roots.push(drv_id);
        }
        return drv_id;
    }

    let drv_mut = state.get_derivation_mut(drv_id);
    drv_mut.cached = true;
    if drv_mut.derivation_parents.is_empty() && !drv_mut.is_root {
        drv_mut.is_root = true;
        state.forest_roots.push(drv_id);
    }

    drv_id
}

fn mark_building(
    state: &mut NomState,
    drv: &Derivation,
    host: &Host,
    now: f64,
    activity_id: Option<u64>,
) {
    let drv_id = lookup_derivation(state, drv);
    state.register_host(host);
    let drv_info = state.get_derivation(drv_id);
    let report_name = drv_info.get_report_name().to_string();
    let host_wc = host.without_context();

    let estimate = state
        .build_reports
        .get(&(host_wc, report_name))
        .and_then(calculate_median);

    let old_status = state.get_derivation(drv_id).build_status.clone();
    let new_status = match &old_status {
        BuildStatus::Building(bi) => {
            // SSH-ng double start handling: update activity_id without resetting start
            let mut updated = bi.clone();
            updated.activity_id = activity_id;
            BuildStatus::Building(updated)
        }
        _ => BuildStatus::Building(BuildInfo {
            start: now,
            host: host.clone(),
            estimate,
            activity_id,
            end: (),
        }),
    };

    update_derivation_state(state, drv_id, old_status, new_status);
}

fn mark_failed_build(state: &mut NomState, drv_id: DerivationId, fail_type: FailType, now: f64) {
    let old_status = state.get_derivation(drv_id).build_status.clone();
    let new_status = match &old_status {
        BuildStatus::Building(bi) => BuildStatus::Failed(BuildInfo {
            start: bi.start,
            host: bi.host.clone(),
            estimate: bi.estimate,
            activity_id: bi.activity_id,
            end: BuildFail { at: now, fail_type },
        }),
        BuildStatus::Built(bi) => BuildStatus::Failed(BuildInfo {
            start: bi.start,
            host: bi.host.clone(),
            estimate: bi.estimate,
            activity_id: bi.activity_id,
            end: BuildFail { at: now, fail_type },
        }),
        _ => return,
    };

    update_derivation_state(state, drv_id, old_status, new_status);
}

fn finish_build_by_drv_id(
    state: &mut NomState,
    host: &Host,
    drv_id: DerivationId,
    now: f64,
    reports_writer: &BuildReportsWriter,
) {
    let old_status = state.get_derivation(drv_id).build_status.clone();
    if let BuildStatus::Building(ref bi) = old_status {
        let build_secs = (now - bi.start).max(0.0).round() as i64;
        let report_name = state.get_derivation(drv_id).get_report_name().to_string();
        let host_wc = host.without_context();

        update_build_reports_in_memory(
            &mut state.build_reports,
            host_wc.clone(),
            report_name.clone(),
            build_secs,
        );
        reports_writer.send(host_wc, report_name, build_secs);

        let new_status = BuildStatus::Built(BuildInfo {
            start: bi.start,
            host: bi.host.clone(),
            estimate: bi.estimate,
            activity_id: bi.activity_id,
            end: now,
        });

        if let Some(act_id) = bi.activity_id {
            state.activities.remove(&act_id);
        }

        update_derivation_state(state, drv_id, old_status, new_status);
    }
}

fn update_derivation_state(
    state: &mut NomState,
    drv_id: DerivationId,
    old_status: BuildStatus,
    new_status: BuildStatus,
) {
    if old_status == new_status {
        return;
    }

    state.get_derivation_mut(drv_id).build_status = new_status.clone();
    NomState::update_summary_for_derivation(
        &mut state.full_summary,
        &old_status,
        &new_status,
        drv_id,
    );

    let parents = &state.get_derivation(drv_id).derivation_parents;
    if parents.is_empty() {
        state.touched_ids.insert(drv_id.0 as u32);
        return;
    }
    let parents_vec: Vec<DerivationId> = parents.iter().copied().collect();
    state.update_parents(
        false,
        |sum| {
            NomState::update_summary_for_derivation_opt(
                sum,
                &old_status,
                &new_status,
                drv_id,
                false,
            )
        },
        |sum| NomState::clear_derivation_id_from_summary(sum, &old_status, drv_id),
        &parents_vec,
    );
    state.touched_ids.insert(drv_id.0 as u32);
}

fn start_downloading(
    state: &mut NomState,
    path_id: StorePathId,
    from: Host,
    start: f64,
    activity_id: Option<u64>,
) {
    state.register_host(&from);
    let old_states = state.get_store_path(path_id).states.clone();
    let mut new_states = old_states.clone();
    new_states.remove(&StorePathState::DownloadPlanned);
    new_states.insert(StorePathState::Downloading(TransferInfo {
        host: from,
        start,
        activity_id,
        end: (),
    }));
    update_store_path_states(state, path_id, old_states, new_states);
}

fn finish_downloading(state: &mut NomState, path_id: StorePathId, from: Host, end: f64) {
    state.register_host(&from);
    let old_states = state.get_store_path(path_id).states.clone();
    let mut new_states = old_states.clone();

    let mut start_time = end;
    let mut act_id = None;
    new_states.retain(|s| {
        if let StorePathState::Downloading(info) = s {
            if info.host == from {
                start_time = info.start;
                act_id = info.activity_id;
                return false;
            }
        }
        true
    });

    new_states.insert(StorePathState::Downloaded(TransferInfo {
        host: from,
        start: start_time,
        activity_id: act_id,
        end: CompletedEnd(Some(end)),
    }));
    update_store_path_states(state, path_id, old_states, new_states);
}

fn start_uploading(
    state: &mut NomState,
    path_id: StorePathId,
    to: Host,
    start: f64,
    activity_id: Option<u64>,
) {
    state.register_host(&to);
    let old_states = state.get_store_path(path_id).states.clone();
    let mut new_states = old_states.clone();
    new_states.insert(StorePathState::Uploading(TransferInfo {
        host: to,
        start,
        activity_id,
        end: (),
    }));
    update_store_path_states(state, path_id, old_states, new_states);
}

fn finish_uploading(state: &mut NomState, path_id: StorePathId, to: Host, end: f64) {
    state.register_host(&to);
    let old_states = state.get_store_path(path_id).states.clone();
    let mut new_states = old_states.clone();

    let mut start_time = end;
    let mut act_id = None;
    new_states.retain(|s| {
        if let StorePathState::Uploading(info) = s {
            if info.host == to {
                start_time = info.start;
                act_id = info.activity_id;
                return false;
            }
        }
        true
    });

    new_states.insert(StorePathState::Uploaded(TransferInfo {
        host: to,
        start: start_time,
        activity_id: act_id,
        end: CompletedEnd(Some(end)),
    }));
    update_store_path_states(state, path_id, old_states, new_states);
}

fn update_store_path_states(
    state: &mut NomState,
    path_id: StorePathId,
    old_states: BTreeSet<StorePathState>,
    new_states: BTreeSet<StorePathState>,
) {
    NomState::update_summary_for_store_path(
        &mut state.full_summary,
        &old_states,
        &new_states,
        path_id,
    );

    let sp_info = state.get_store_path(path_id);
    if sp_info.input_for.is_empty() && sp_info.producer.is_none() {
        state.get_store_path_mut(path_id).states = new_states;
        return;
    }

    let mut direct_parents = Vec::with_capacity(sp_info.input_for.len() + 1);
    direct_parents.extend(sp_info.input_for.iter().copied());
    if let Some(prod) = sp_info.producer {
        direct_parents.push(prod);
    }

    state.update_parents(
        true,
        |sum| {
            NomState::update_summary_for_store_path_opt(
                sum,
                &old_states,
                &new_states,
                path_id,
                false,
            )
        },
        |sum| NomState::clear_store_paths_from_summary(sum, &old_states, path_id),
        &direct_parents,
    );

    state.get_store_path_mut(path_id).states = new_states;
}


fn get_activity_prefix_for_activity(state: &NomState, act: &Activity) -> String {
    if let Activity::Build { drv, .. } = act {
        if let Some(&drv_id) = state.derivation_ids.get(drv) {
            let drv_info = state.get_derivation(drv_id);
            let name = drv_info.get_report_name();
            format!("{}{}{}{}> ", RESET, BLUE, name, RESET)
        } else {
            String::new()
        }
    } else {
        String::new()
    }
}
