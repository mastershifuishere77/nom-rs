pub mod progress;
pub mod table;
pub mod tree;

use crate::render::progress::print_bytes;
use crate::render::table::{
    display_width, prepend_lines, print_aligned_table, truncate_display, Entry, BLUE, BOLD, GREEN,
    GREY, MAGENTA, RED, RESET, YELLOW,
};
use crate::render::tree::{show_forest, TreeNode};
use crate::sorting::calculate_sort_key;
use crate::state::{
    BuildStatus, DependencySummary, DerivationId, DerivationInfo, FailType, Host, NomState,
    ProgressState, TransferInfo,
};
use chrono::Local;
use std::collections::{HashMap, HashSet};
use terminal_size::{terminal_size, Height, Width};

pub const VERTICAL: &str = "┃";
pub const LOWERLEFT: &str = "┗";
pub const UPPERLEFT: &str = "┏";
pub const LEFT_T: &str = "┣";
pub const HORIZONTAL: &str = "━";
pub const DOWN: &str = "↓";
pub const UP: &str = "↑";
pub const CLOCK: &str = "⏱";
pub const RUNNING: &str = "⏵";
pub const DONE: &str = "✔";
pub const TODO: &str = "⏸";
pub const WARNING: &str = "⚠";
pub const AVERAGE: &str = "∅";
pub const BIGSUM: &str = "∑";

#[derive(Clone, Copy, Debug, Default)]
pub struct Config {
    pub silent: bool,
    pub piping: bool,
}

pub fn format_duration(diff: f64) -> String {
    let diff_secs = diff.max(0.0) as u64;
    let minute = 60;
    let hour = 60 * minute;
    let day = 24 * hour;

    if diff_secs < minute {
        format!("{:02}s", diff_secs)
    } else if diff_secs < hour {
        let mins = diff_secs / minute;
        let secs = diff_secs % minute;
        format!("{:02}m{:02}s", mins, secs)
    } else if diff_secs < day {
        let hours = diff_secs / hour;
        let mins = (diff_secs % hour) / minute;
        let secs = diff_secs % minute;
        format!("{:02}h{:02}m{:02}s", hours, mins, secs)
    } else {
        let days = diff_secs / day;
        let hours = (diff_secs % day) / hour;
        let mins = (diff_secs % hour) / minute;
        let secs = diff_secs % minute;
        format!("{}d{:02}h{:02}m{:02}s", days, hours, mins, secs)
    }
}

pub fn render_state_to_text(state: &NomState, config: Config, now: f64) -> String {
    let (term_width, term_height) = if let Some((Width(w), Height(h))) = terminal_size() {
        if w > 0 && h > 0 {
            (w as usize, h as usize)
        } else {
            (120, 24)
        }
    } else {
        (120, 24)
    };

    if state.progress_state == ProgressState::JustStarted && config.piping {
        let time_str = format!("{} {}", CLOCK, format_duration(now - state.start_time));
        if now - state.start_time > 15.0 {
            return format!(
                "{}{}{}{} nom hasn‘t detected any input. Have you redirected nix-build stderr into nom? (See -h and the README for details.){}",
                BOLD, time_str, RESET, GREY, RESET
            );
        } else {
            return format!("{}{}{}", BOLD, time_str, RESET);
        }
    }

    if state.progress_state == ProgressState::Finished && config.silent {
        return String::new();
    }

    let mut sections: Vec<String> = Vec::new();
    let max_tree_height = term_height / 3;

    // 1. Errors section
    if !state.nix_errors.is_empty() {
        sections.push(render_errors(&state.nix_errors, max_tree_height));
    }

    // 2. Traces section
    if !state.nix_traces.is_empty() {
        sections.push(render_traces(&state.nix_traces, max_tree_height));
    }

    // 3. Tree section
    if !state.forest_roots.is_empty() {
        sections.push(render_builds(
            state,
            term_width.saturating_sub(2),
            max_tree_height,
            now,
        ));
    }

    // 4. Summary table section
    if !state.full_summary.is_empty() || !sections.is_empty() {
        sections.push(render_summary_table(state, now));
    }

    if sections.is_empty() {
        if config.silent {
            String::new()
        } else {
            format!(
                "{}{} {}{}",
                BOLD,
                CLOCK,
                format_duration(now - state.start_time),
                RESET
            )
        }
    } else {
        let mut out = String::new();
        out.push_str(RESET);
        for (i, sec) in sections.iter().enumerate() {
            if i == 0 {
                out.push_str(UPPERLEFT);
            } else {
                out.push('\n');
                out.push_str(RESET);
                out.push_str(LEFT_T);
            }
            out.push_str(sec);
        }
        truncate_output_to_window(&out, term_width, term_height)
    }
}

fn truncate_output_to_window(output: &str, width: usize, height: usize) -> String {
    let lines: Vec<&str> = output.lines().collect();
    let mut truncated_lines = Vec::new();

    for line in lines {
        let is_table_line = line.contains(" │ ")
            || line.starts_with(LEFT_T)
            || line.starts_with(LOWERLEFT)
            || line.contains(BIGSUM);
        if !is_table_line && display_width(line) > width {
            let truncated = truncate_display(line, width.saturating_sub(1));
            truncated_lines.push(format!("{}…{}", truncated, RESET));
        } else {
            truncated_lines.push(line.to_string());
        }
    }

    if truncated_lines.len() >= height && height > 6 {
        let mut final_rows = Vec::new();
        final_rows.push(truncated_lines[0].clone());
        final_rows.push(" ⋮ ".to_string());
        let take_from_end = height.saturating_sub(4);
        let start_idx = truncated_lines.len().saturating_sub(take_from_end);
        final_rows.extend(truncated_lines[start_idx..].iter().cloned());
        final_rows.join("\n")
    } else {
        truncated_lines.join("\n")
    }
}

fn render_errors(errors: &[String], max_height: usize) -> String {
    let count = errors.len();
    let title = format!("{}{}{} Errors: {}", BOLD, RED, count, RESET);
    let mut lines = Vec::new();
    lines.push(format!("{}{}", HORIZONTAL, title));

    let compact = errors.iter().map(|e| e.lines().count()).sum::<usize>() > max_height;
    for err in errors.iter().rev() {
        let msg = if compact {
            err.split("\n       last 10 log lines:")
                .next()
                .unwrap_or(err)
        } else {
            err.as_str()
        };
        for l in msg.lines() {
            lines.push(l.to_string());
        }
    }

    prepend_lines(
        "",
        &format!("{} ", VERTICAL),
        &format!("{} ", VERTICAL),
        &lines,
    )
}

fn render_traces(traces: &[String], max_height: usize) -> String {
    let count = traces.len();
    let label = if count == 1 { " Trace" } else { " Traces" };
    let title = format!("{}{}{} {}: {}", BOLD, YELLOW, count, label, RESET);
    let mut lines = Vec::new();
    lines.push(format!("{}{}", HORIZONTAL, title));

    let compact = traces.iter().map(|t| t.lines().count()).sum::<usize>() > max_height;
    for tr in traces {
        let msg = if compact {
            tr.split("\n       last 10 log lines:").next().unwrap_or(tr)
        } else {
            tr.as_str()
        };
        for l in msg.lines() {
            lines.push(l.to_string());
        }
    }

    prepend_lines(
        "",
        &format!("{} ", VERTICAL),
        &format!("{} ", VERTICAL),
        &lines,
    )
}

fn render_builds(state: &NomState, _max_width: usize, max_height: usize, now: f64) -> String {
    let host_abbrevs = compute_host_abbrevs(state);
    let forest = build_display_forest(state, &host_abbrevs, max_height, now);
    let rows = show_forest(&forest);

    let num_raw_roots = state.forest_roots.len();
    let num_roots = forest.len();
    let graph_title = format!("{}{}{}Dependency Graph{}", RESET, BOLD, BLUE, RESET);
    let header_inner = if num_raw_roots <= 1 {
        graph_title
    } else if num_raw_roots == num_roots {
        format!("{} with {} roots", graph_title, num_roots)
    } else {
        format!(
            "{} showing {} of {} roots",
            graph_title, num_roots, num_raw_roots
        )
    };

    let mut lines = Vec::new();
    lines.push(format!(" {}:", header_inner));

    for (left, _progress_opt) in rows {
        lines.push(left);
    }

    prepend_lines(
        HORIZONTAL,
        &format!("{} ", VERTICAL),
        &format!("{} ", VERTICAL),
        &lines,
    )
}

fn build_display_forest(
    state: &NomState,
    host_abbrevs: &HashMap<String, String>,
    max_height: usize,
    now: f64,
) -> Vec<TreeNode<Option<f64>>> {
    let derivations_to_show = select_derivations_to_show(state, max_height);
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for &root_id in &state.forest_roots {
        if let Some(node) = build_tree_node(
            state,
            root_id,
            &derivations_to_show,
            &mut seen,
            host_abbrevs,
            true,
            now,
        ) {
            result.push(node);
        }
    }

    result
}

pub fn select_derivations_to_show(state: &NomState, max_height: usize) -> HashSet<DerivationId> {
    if max_height == 0 {
        return HashSet::new();
    }

    // 1. Identify all truly active nodes:
    // - Derivations currently building (BuildStatus::Building)
    // - Derivations that failed (BuildStatus::Failed)
    // - Derivations with running downloads or uploads on their outputs
    // - Derivations associated with running downloads/uploads from state.full_summary
    let mut active_nodes: HashSet<DerivationId> = HashSet::new();

    for &drv_id in state.full_summary.failed_builds.keys() {
        active_nodes.insert(drv_id);
    }
    for &drv_id in state.full_summary.running_builds.keys() {
        active_nodes.insert(drv_id);
    }
    for &path_id in state
        .full_summary
        .running_downloads
        .keys()
        .chain(state.full_summary.running_uploads.keys())
    {
        if path_id.0 < state.store_path_infos.len() {
            let sp_info = &state.store_path_infos[path_id.0];
            if let Some(drv_id) = sp_info.producer {
                active_nodes.insert(drv_id);
            }
        }
    }

    if active_nodes.is_empty() {
        for (idx, drv) in state.derivation_infos.iter().enumerate() {
            if matches!(
                drv.build_status,
                BuildStatus::Building(_) | BuildStatus::Failed(_)
            ) {
                active_nodes.insert(DerivationId(idx));
            }
        }
    }

    // 2. Identify all nodes that have active descendants (including the active nodes themselves)
    // and their entire ancestor paths up to roots using a unified multi-source BFS.
    let mut active_path_nodes: HashSet<DerivationId> = active_nodes.clone();
    let mut has_active_descendants: HashSet<DerivationId> = HashSet::new();
    let mut parent_queue: Vec<DerivationId> = active_nodes.iter().copied().collect();

    while let Some(p) = parent_queue.pop() {
        let drv = state.get_derivation(p);
        for &parent_id in &drv.derivation_parents {
            let parent_drv = state.get_derivation(parent_id);
            if !matches!(parent_drv.build_status, BuildStatus::Unknown)
                || !state.is_summary_including_root_empty(parent_id)
            {
                has_active_descendants.insert(parent_id);
                if active_path_nodes.insert(parent_id) {
                    parent_queue.push(parent_id);
                }
            }
        }
    }

    // Always include forest roots with non-empty summaries
    let mut result = HashSet::new();
    for &root_id in &state.forest_roots {
        if !state.is_summary_including_root_empty(root_id) {
            result.insert(root_id);
        }
    }

    if !active_nodes.is_empty() {
        // Active builds / transfers are happening!
        // All active path nodes (ancestors + active nodes) MUST be shown.
        result.extend(active_path_nodes.iter().copied());

        let mut budget = max_height.saturating_sub(result.len());
        if budget > 0 {
            // For nodes that have active descendants, allow their immediate children
            // as collapsed leaves (sorted by sort_key) up to budget.
            // But NEVER recurse into children of an inactive sibling!
            let mut seen_candidates = HashSet::new();
            let mut immediate_candidates: Vec<DerivationId> = Vec::new();

            for &parent_id in has_active_descendants.iter().chain(&state.forest_roots) {
                let drv = state.get_derivation(parent_id);
                for input in &drv.input_derivations {
                    if !result.contains(&input.derivation)
                        && seen_candidates.insert(input.derivation)
                    {
                        immediate_candidates.push(input.derivation);
                    }
                }
            }

            // Sort unique immediate children by calculate_sort_key:
            // Planned downloads (↓ ⏸) and planned builds (⏸) have higher priority than completed/unknown!
            immediate_candidates
                .sort_by_cached_key(|&child_id| calculate_sort_key(state, child_id));

            for child_id in immediate_candidates {
                if budget == 0 {
                    break;
                }
                if !state.is_summary_including_root_empty(child_id) {
                    result.insert(child_id);
                    budget -= 1;
                }
            }
        }
    } else {
        // No active nodes anywhere (planning / evaluation / idle / finished).
        // Traverse level-by-level (BFS) from roots up to max_height, so we keep
        // the tree shallow and connected, without expanding deep 7-level chains.
        let mut queue: std::collections::VecDeque<DerivationId> =
            state.forest_roots.iter().copied().collect();
        let mut visited = result.clone();

        while let Some(parent_id) = queue.pop_front() {
            if result.len() >= max_height {
                break;
            }
            let drv = state.get_derivation(parent_id);
            let mut children: Vec<DerivationId> = drv
                .input_derivations
                .iter()
                .map(|i| i.derivation)
                .filter(|c| !visited.contains(c))
                .collect();
            children.sort_by_cached_key(|&c| calculate_sort_key(state, c));
            children.dedup();

            for child_id in children {
                if result.len() >= max_height {
                    break;
                }
                visited.insert(child_id);
                if !state.is_summary_including_root_empty(child_id) {
                    result.insert(child_id);
                    queue.push_back(child_id);
                }
            }
        }
    }

    result
}

fn build_tree_node(
    state: &NomState,
    drv_id: DerivationId,
    derivations_to_show: &HashSet<DerivationId>,
    seen: &mut HashSet<DerivationId>,
    host_abbrevs: &HashMap<String, String>,
    is_root: bool,
    now: f64,
) -> Option<TreeNode<Option<f64>>> {
    if seen.contains(&drv_id) || !derivations_to_show.contains(&drv_id) {
        return None;
    }
    seen.insert(drv_id);

    let drv = state.get_derivation(drv_id);
    let mut children = Vec::new();
    for input in &drv.input_derivations {
        if let Some(child_node) = build_tree_node(
            state,
            input.derivation,
            derivations_to_show,
            seen,
            host_abbrevs,
            false,
            now,
        ) {
            children.push(child_node);
        }
    }

    let is_leaf = children.is_empty();

    // Check if this node is an inert pre-cached node:
    // If it's a leaf, has BuildStatus::Unknown, and has NO running/completed/planned transfers on its outputs,
    // it was never built/downloaded in this session and has no active descendants.
    let dep_sum = &drv.dependency_summary;
    let summary_has_transfers = !dep_sum.running_downloads.is_empty()
        || !dep_sum.running_uploads.is_empty()
        || !dep_sum.completed_downloads.is_empty()
        || !dep_sum.completed_uploads.is_empty()
        || !dep_sum.planned_downloads.is_empty();

    let has_transfers = summary_has_transfers
        && drv.outputs.values().any(|&path_id| {
            dep_sum.running_downloads.contains_key(&path_id)
                || dep_sum.running_uploads.contains_key(&path_id)
                || dep_sum.completed_downloads.contains_key(&path_id)
                || dep_sum.completed_uploads.contains_key(&path_id)
                || dep_sum.planned_downloads.contains(&path_id)
        });
    if matches!(drv.build_status, BuildStatus::Unknown)
        && !has_transfers
        && (is_leaf || state.is_summary_including_root_empty(drv_id))
    {
        return None;
    }

    let (label, progress) = format_derivation_row(state, drv, is_root, is_leaf, host_abbrevs, now);

    Some(TreeNode {
        label,
        extra: progress,
        children,
    })
}

fn format_derivation_row(
    state: &NomState,
    drv: &DerivationInfo,
    _is_root: bool,
    is_leaf: bool,
    host_abbrevs: &HashMap<String, String>,
    now: f64,
) -> (String, Option<f64>) {
    let drv_name = format_differing_platform(state, drv);
    let mut progress_val = None;

    // Check downloads / uploads on outputs
    let dep_sum = &drv.dependency_summary;
    let summary_has_transfers = !dep_sum.running_downloads.is_empty()
        || !dep_sum.running_uploads.is_empty()
        || !dep_sum.completed_downloads.is_empty()
        || !dep_sum.completed_uploads.is_empty()
        || !dep_sum.planned_downloads.is_empty();

    let mut running_downloads = Vec::new();
    let mut running_uploads = Vec::new();
    let mut completed_downloads = Vec::new();
    let mut completed_uploads = Vec::new();

    if summary_has_transfers {
        for &path_id in drv.outputs.values() {
            if let Some(dl) = dep_sum.running_downloads.get(&path_id) {
                running_downloads.push(dl);
            }
            if let Some(ul) = dep_sum.running_uploads.get(&path_id) {
                running_uploads.push(ul);
            }
            if let Some(dl) = dep_sum.completed_downloads.get(&path_id) {
                completed_downloads.push(dl);
            }
            if let Some(ul) = dep_sum.completed_uploads.get(&path_id) {
                completed_uploads.push(ul);
            }
        }
    }

    let is_planned_download = summary_has_transfers
        && drv
            .outputs
            .values()
            .any(|p| dep_sum.planned_downloads.contains(p));

    let row_str = if !running_downloads.is_empty() {
        let (prct, prog_text) = compute_transfer_progress(state, &running_downloads);
        progress_val = prct;
        let earliest_start = running_downloads
            .iter()
            .map(|d| d.start)
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(now);
        let mut parts = Vec::new();
        parts.push(format!(
            "{}{}{} {} {}{}",
            BOLD, YELLOW, DOWN, RUNNING, drv_name, RESET
        ));
        let hosts = format_hosts(&running_downloads, host_abbrevs, "from");
        if !hosts.is_empty() {
            parts.push(hosts.trim().to_string());
        }
        if now - earliest_start > 1.0 {
            parts.push(format!(
                "{} {}",
                CLOCK,
                format_duration(now - earliest_start)
            ));
        }
        if !prog_text.is_empty() {
            parts.push(format!("{}{}{}", GREEN, prog_text.trim(), RESET));
        }
        parts.join(" ")
    } else if !running_uploads.is_empty() {
        let (prct, prog_text) = compute_transfer_progress(state, &running_uploads);
        progress_val = prct;
        let earliest_start = running_uploads
            .iter()
            .map(|u| u.start)
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(now);
        let mut parts = Vec::new();
        parts.push(format!(
            "{}{}{} {} {}{}",
            BOLD, YELLOW, UP, RUNNING, drv_name, RESET
        ));
        let hosts = format_hosts(&running_uploads, host_abbrevs, "to");
        if !hosts.is_empty() {
            parts.push(hosts.trim().to_string());
        }
        if now - earliest_start > 1.0 {
            parts.push(format!(
                "{} {}",
                CLOCK,
                format_duration(now - earliest_start)
            ));
        }
        if !prog_text.is_empty() {
            parts.push(format!("{}{}{}", GREEN, prog_text.trim(), RESET));
        }
        parts.join(" ")
    } else {
        match &drv.build_status {
            BuildStatus::Unknown => {
                if is_planned_download {
                    format!("{}{} {} {}{}", BLUE, DOWN, TODO, drv_name, RESET)
                } else if !completed_downloads.is_empty() {
                    let total_bytes: usize = completed_downloads
                        .iter()
                        .filter_map(|d| d.activity_id)
                        .filter_map(|act_id| state.activities.get(&act_id))
                        .filter_map(|act| act.progress.as_ref())
                        .map(|p| p.expected)
                        .sum();
                    let mut parts = Vec::new();
                    parts.push(format!("{}{} {} {}{}", GREEN, DOWN, DONE, drv_name, RESET));
                    let mut grey_parts = Vec::new();
                    if total_bytes > 0 {
                        grey_parts.push(print_bytes(total_bytes));
                    }
                    let hosts = format_hosts(&completed_downloads, host_abbrevs, "from");
                    if !hosts.is_empty() {
                        grey_parts.push(hosts.trim().to_string());
                    }
                    if !grey_parts.is_empty() {
                        parts.push(format!("{}{}{}", GREY, grey_parts.join(" "), RESET));
                    }
                    parts.join(" ")
                } else if !completed_uploads.is_empty() {
                    let total_bytes: usize = completed_uploads
                        .iter()
                        .filter_map(|u| u.activity_id)
                        .filter_map(|act_id| state.activities.get(&act_id))
                        .filter_map(|act| act.progress.as_ref())
                        .map(|p| p.expected)
                        .sum();
                    let mut parts = Vec::new();
                    parts.push(format!("{}{} {} {}{}", GREEN, UP, DONE, drv_name, RESET));
                    let mut grey_parts = Vec::new();
                    if total_bytes > 0 {
                        grey_parts.push(print_bytes(total_bytes));
                    }
                    let hosts = format_hosts(&completed_uploads, host_abbrevs, "to");
                    if !hosts.is_empty() {
                        grey_parts.push(hosts.trim().to_string());
                    }
                    if !grey_parts.is_empty() {
                        parts.push(format!("{}{}{}", GREY, grey_parts.join(" "), RESET));
                    }
                    parts.join(" ")
                } else {
                    drv_name.clone()
                }
            }
            BuildStatus::Planned => {
                format!("{}{} {}{}", BLUE, TODO, drv_name, RESET)
            }
            BuildStatus::Building(bi) => {
                let mut parts = Vec::new();
                parts.push(format!(
                    "{}{}{} {}{}",
                    BOLD, YELLOW, RUNNING, drv_name, RESET
                ));
                let host_str = format_single_host(&bi.host, host_abbrevs, true);
                if !host_str.is_empty() {
                    parts.push(host_str.trim().to_string());
                }
                if let Some(act) = bi.activity_id.and_then(|id| state.activities.get(&id)) {
                    if let Some(phase) = &act.phase {
                        parts.push(format!("{}({}){}", BOLD, phase, RESET));
                    }
                }
                if now - bi.start > 1.0 {
                    let dur = format_duration(now - bi.start);
                    if let Some(est) = bi.estimate {
                        parts.push(format!(
                            "{} {} ({} {})",
                            CLOCK,
                            dur,
                            AVERAGE,
                            format_duration(est as f64)
                        ));
                    } else {
                        parts.push(format!("{} {}", CLOCK, dur));
                    }
                }
                parts.join(" ")
            }
            BuildStatus::Failed(bi) => {
                let mut parts = Vec::new();
                parts.push(WARNING.to_string());
                parts.push(drv_name);
                let host_str = format_single_host(&bi.host, host_abbrevs, false);
                if !host_str.is_empty() {
                    parts.push(host_str.trim().to_string());
                }
                let fail_desc = match bi.end.fail_type {
                    FailType::ExitCode(c) => format!("exit code {}", c),
                    FailType::HashMismatch => "hash mismatch".to_string(),
                };
                parts.push("failed with".to_string());
                parts.push(fail_desc);
                parts.push("after".to_string());
                parts.push(CLOCK.to_string());
                parts.push(format_duration(bi.end.at - bi.start));
                if let Some(act) = bi.activity_id.and_then(|id| state.activities.get(&id)) {
                    if let Some(phase) = &act.phase {
                        parts.push(format!("in {}", phase));
                    }
                }
                format!("{}{}{}{}", BOLD, RED, parts.join(" "), RESET)
            }
            BuildStatus::Built(bi) => {
                let main_str = format!("{}{} {}{}", GREEN, DONE, drv_name, RESET);
                let mut extra_parts = Vec::new();
                let host_str = format_single_host(&bi.host, host_abbrevs, false);
                if !host_str.is_empty() {
                    extra_parts.push(host_str.trim().to_string());
                }
                if bi.end - bi.start > 1.0 {
                    extra_parts.push(format!("{} {}", CLOCK, format_duration(bi.end - bi.start)));
                }
                if !extra_parts.is_empty() {
                    format!("{} {}{}{}", main_str, GREY, extra_parts.join(" "), RESET)
                } else {
                    main_str
                }
            }
        }
    };

    let is_planned = matches!(drv.build_status, BuildStatus::Planned)
        || (matches!(drv.build_status, BuildStatus::Unknown) && is_planned_download);

    let summary_str = if is_leaf && is_planned {
        let s = format_dependency_summary(&drv.dependency_summary);
        if !s.is_empty() {
            format!(" {} waiting for {}{}", GREY, s, RESET)
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    (format!("{}{}", row_str, summary_str), progress_val)
}

fn compute_transfer_progress<T>(
    state: &NomState,
    transfers: &[&TransferInfo<T>],
) -> (Option<f64>, String) {
    let mut total_done = 0;
    let mut total_expected = 0;

    for t in transfers {
        if let Some(act_id) = t.activity_id {
            if let Some(act) = state.activities.get(&act_id) {
                if let Some(ref p) = act.progress {
                    total_done += p.done;
                    total_expected += p.expected;
                }
            }
        }
    }

    if total_expected > 0 {
        let fraction = (total_done as f64) / (total_expected as f64);
        let text = format!(
            " {}/{}",
            print_bytes(total_done),
            print_bytes(total_expected)
        );
        (Some(fraction), text)
    } else {
        (None, String::new())
    }
}

fn format_differing_platform(state: &NomState, drv: &DerivationInfo) -> String {
    let base_name = drv.name.store_path.name.clone();
    if let (Some(ref p1), Some(ref p2)) = (&state.build_platform, &drv.platform) {
        if p1 != p2 {
            return format!("{}-{}", base_name, p2);
        }
    }
    base_name
}

fn format_single_host(host: &Host, host_abbrevs: &HashMap<String, String>, color: bool) -> String {
    match host {
        Host::Localhost => String::new(),
        _ => {
            let h_str = host.hostname_only();
            let label = host_abbrevs.get(h_str).map(|s| s.as_str()).unwrap_or(h_str);
            if color {
                format!(" on {}{}{}{}", MAGENTA, label, RESET, "")
            } else {
                format!(" on {}", label)
            }
        }
    }
}

fn format_hosts<T>(
    transfers: &[&TransferInfo<T>],
    host_abbrevs: &HashMap<String, String>,
    dir: &str,
) -> String {
    if host_abbrevs.len() <= 1 || transfers.is_empty() {
        return String::new();
    }
    let unique_hosts: HashSet<&Host> = transfers.iter().map(|t| &t.host).collect();
    let mut names = Vec::new();
    for h in unique_hosts {
        let h_str = h.hostname_only();
        let label = host_abbrevs.get(h_str).map(|s| s.as_str()).unwrap_or(h_str);
        names.push(label);
    }
    format!(" {} {}", dir, names.join(", "))
}

fn format_dependency_summary(s: &DependencySummary) -> String {
    let mut parts = Vec::new();
    if !s.failed_builds.is_empty() {
        parts.push(format!(
            "{}{} {}{}",
            RED,
            s.failed_builds.len(),
            WARNING,
            RESET
        ));
    }
    if !s.running_builds.is_empty() {
        parts.push(format!(
            "{}{} {}{}",
            YELLOW,
            s.running_builds.len(),
            RUNNING,
            RESET
        ));
    }
    if !s.planned_builds.is_empty() {
        parts.push(format!(
            "{}{} {}{}",
            BLUE,
            s.planned_builds.len(),
            TODO,
            RESET
        ));
    }
    if !s.running_uploads.is_empty() {
        parts.push(format!(
            "{}{} {}{}",
            YELLOW,
            s.running_uploads.len(),
            UP,
            RESET
        ));
    }
    if !s.running_downloads.is_empty() {
        parts.push(format!(
            "{}{} {}{}",
            YELLOW,
            s.running_downloads.len(),
            DOWN,
            RESET
        ));
    }
    if !s.planned_downloads.is_empty() {
        parts.push(format!(
            "{}{} {} {}{}",
            BLUE,
            s.planned_downloads.len(),
            DOWN,
            TODO,
            RESET
        ));
    }
    parts.join(" ")
}

fn render_summary_table(state: &NomState, now: f64) -> String {
    let s = &state.full_summary;
    let num_running_builds = s.running_builds.len();
    let num_completed_builds = s.completed_builds.len();
    let num_planned_builds = s.planned_builds.len();
    let total_builds = num_running_builds + num_completed_builds + num_planned_builds;

    let num_running_dl = s.running_downloads.len();
    let num_completed_dl = s.completed_downloads.len();
    let num_planned_dl = s.planned_downloads.len();
    let total_dl = num_running_dl + num_completed_dl + num_planned_dl;

    let num_running_ul = s.running_uploads.len();
    let num_completed_ul = s.completed_uploads.len();
    let total_ul = num_running_ul + num_completed_ul;

    let show_builds = total_builds > 0;
    let show_dl = total_dl > 0;
    let show_ul = total_ul > 0;

    struct HostStats<'a> {
        host: &'a Host,
        rb: usize,
        cb: usize,
        rd: usize,
        cd: usize,
        ru: usize,
        cu: usize,
        host_done: usize,
        host_expected: usize,
    }

    let mut host_stats: HashMap<&str, HostStats> = HashMap::new();
    let mut total_done = 0;
    let mut total_expected = 0;

    for b in s.running_builds.values() {
        host_stats
            .entry(b.host.hostname_only())
            .or_insert_with(|| HostStats {
                host: &b.host,
                rb: 0,
                cb: 0,
                rd: 0,
                cd: 0,
                ru: 0,
                cu: 0,
                host_done: 0,
                host_expected: 0,
            })
            .rb += 1;
    }
    for b in s.completed_builds.values() {
        host_stats
            .entry(b.host.hostname_only())
            .or_insert_with(|| HostStats {
                host: &b.host,
                rb: 0,
                cb: 0,
                rd: 0,
                cd: 0,
                ru: 0,
                cu: 0,
                host_done: 0,
                host_expected: 0,
            })
            .cb += 1;
    }
    for d in s.running_downloads.values() {
        let stats = host_stats
            .entry(d.host.hostname_only())
            .or_insert_with(|| HostStats {
                host: &d.host,
                rb: 0,
                cb: 0,
                rd: 0,
                cd: 0,
                ru: 0,
                cu: 0,
                host_done: 0,
                host_expected: 0,
            });
        stats.rd += 1;
        if let Some(act_id) = d.activity_id {
            if let Some(act) = state.activities.get(&act_id) {
                if let Some(ref p) = act.progress {
                    stats.host_done += p.done;
                    stats.host_expected += p.expected;
                    total_done += p.done;
                    total_expected += p.expected;
                }
            }
        }
    }
    for d in s.completed_downloads.values() {
        let stats = host_stats
            .entry(d.host.hostname_only())
            .or_insert_with(|| HostStats {
                host: &d.host,
                rb: 0,
                cb: 0,
                rd: 0,
                cd: 0,
                ru: 0,
                cu: 0,
                host_done: 0,
                host_expected: 0,
            });
        stats.cd += 1;
        if let Some(act_id) = d.activity_id {
            if let Some(act) = state.activities.get(&act_id) {
                if let Some(ref p) = act.progress {
                    let d_done = p.expected.max(p.done);
                    stats.host_done += d_done;
                    stats.host_expected += p.expected;
                    total_done += d_done;
                    total_expected += p.expected;
                }
            }
        }
    }
    for u in s.running_uploads.values() {
        host_stats
            .entry(u.host.hostname_only())
            .or_insert_with(|| HostStats {
                host: &u.host,
                rb: 0,
                cb: 0,
                rd: 0,
                cd: 0,
                ru: 0,
                cu: 0,
                host_done: 0,
                host_expected: 0,
            })
            .ru += 1;
    }
    for u in s.completed_uploads.values() {
        host_stats
            .entry(u.host.hostname_only())
            .or_insert_with(|| HostStats {
                host: &u.host,
                rb: 0,
                cb: 0,
                rd: 0,
                cd: 0,
                ru: 0,
                cu: 0,
                host_done: 0,
                host_expected: 0,
            })
            .cu += 1;
    }

    let show_hosts = host_stats.len() > 1;

    let mut header_row = Vec::new();
    if show_builds {
        header_row.push(Entry::header("Builds").bold().cells(3));
    }
    if show_dl {
        header_row.push(Entry::header("Downloads").bold().cells(4));
    }
    if show_ul {
        header_row.push(Entry::header("Uploads").bold().cells(2));
    }
    if show_hosts {
        header_row.push(Entry::header("Host").bold());
    }

    let mut rows: Vec<Vec<Entry>> = Vec::new();
    if !header_row.is_empty() {
        rows.push(header_row);
    }

    // Host rows if show_hosts
    if show_hosts {
        let mut sorted_keys: Vec<&str> = host_stats.keys().copied().collect();
        sorted_keys.sort_unstable();
        for h in sorted_keys {
            let stats = &host_stats[h];
            let host_display = stats.host.format_with_proto_context();
            let mut host_row = Vec::new();
            if show_builds {
                if stats.rb > 0 {
                    host_row.push(non_zero_entry(RUNNING, stats.rb, |e| e.yellow()));
                } else {
                    host_row.push(Entry::text(""));
                }
                if stats.cb > 0 {
                    host_row.push(non_zero_entry(DONE, stats.cb, |e| e.green()));
                } else {
                    host_row.push(Entry::text(""));
                }
                host_row.push(Entry::text(""));
            }
            if show_dl {
                if stats.rd > 0 {
                    host_row.push(non_zero_entry(DOWN, stats.rd, |e| e.yellow()));
                } else {
                    host_row.push(Entry::text(""));
                }
                if stats.cd > 0 {
                    host_row.push(non_zero_entry(DOWN, stats.cd, |e| e.green()));
                } else {
                    host_row.push(Entry::text(""));
                }
                host_row.push(Entry::text(""));
                if stats.host_expected > 0 {
                    host_row.push(
                        Entry::text(format!(
                            "{} {}/{}",
                            DOWN,
                            print_bytes(stats.host_done),
                            print_bytes(stats.host_expected)
                        ))
                        .green(),
                    );
                } else {
                    host_row.push(Entry::text(""));
                }
            }
            if show_ul {
                if stats.ru > 0 {
                    host_row.push(non_zero_entry(UP, stats.ru, |e| e.yellow()));
                } else {
                    host_row.push(Entry::text(""));
                }
                if stats.cu > 0 {
                    host_row.push(non_zero_entry(UP, stats.cu, |e| e.green()));
                } else {
                    host_row.push(Entry::text(""));
                }
            }
            host_row.push(Entry::header(host_display).magenta());
            rows.push(host_row);
        }
    }

    // Last row: totals
    let mut total_row = Vec::new();
    if show_builds {
        total_row.push(non_zero_entry(RUNNING, num_running_builds, |e| e.yellow()));
        total_row.push(non_zero_entry(DONE, num_completed_builds, |e| e.green()));
        total_row.push(non_zero_entry(TODO, num_planned_builds, |e| e.blue()));
    }
    if show_dl {
        total_row.push(non_zero_entry(DOWN, num_running_dl, |e| e.yellow()));
        total_row.push(non_zero_entry(DOWN, num_completed_dl, |e| e.green()));
        total_row.push(non_zero_entry(TODO, num_planned_dl, |e| e.blue()));

        if total_expected > 0 {
            total_row.push(
                Entry::text(format!(
                    "{} {}/{}",
                    DOWN,
                    print_bytes(total_done),
                    print_bytes(total_expected)
                ))
                .green(),
            );
        } else {
            total_row.push(Entry::text(""));
        }
    }
    if show_ul {
        total_row.push(non_zero_entry(UP, num_running_ul, |e| e.yellow()));
        total_row.push(non_zero_entry(UP, num_completed_ul, |e| e.green()));
    }

    let time_str = if state.progress_state == ProgressState::Finished {
        let local_time = Local::now().format("%H:%M:%S").to_string();
        let dur = format_duration(now - state.start_time);
        if !s.failed_builds.is_empty() {
            format!(
                "{}{}{} Exited after {} build failures at {} after {}{}",
                BOLD,
                RED,
                WARNING,
                s.failed_builds.len(),
                local_time,
                dur,
                RESET
            )
        } else if !state.nix_errors.is_empty() {
            format!(
                "{}{}{} Exited with {} errors reported by nix at {} after {}{}",
                BOLD,
                RED,
                WARNING,
                state.nix_errors.len(),
                local_time,
                dur,
                RESET
            )
        } else {
            format!(
                "{}{}{} Finished at {} after {}{}",
                BOLD, GREEN, DONE, local_time, dur, RESET
            )
        }
    } else {
        format!("{} {}", CLOCK, format_duration(now - state.start_time))
    };

    total_row.push(Entry::header(time_str).bold());
    rows.push(total_row);

    let sep = " │ ";
    let formatted_rows = print_aligned_table(&rows, sep);
    prepend_lines(
        "━━━ ",
        &format!("{}    ", VERTICAL),
        &format!("{}{} {} ", LOWERLEFT, HORIZONTAL, BIGSUM),
        &formatted_rows,
    )
}

pub fn non_zero_entry(symbol: &str, count: usize, color_fn: impl Fn(Entry) -> Entry) -> Entry {
    if count > 0 {
        color_fn(Entry::text(count.to_string()).label(symbol).bold())
    } else {
        color_fn(Entry::text("0").label(symbol))
    }
}

fn compute_host_abbrevs(state: &NomState) -> HashMap<String, String> {
    let mut hosts: HashSet<&str> = HashSet::new();
    for drv in state.full_summary.running_builds.values() {
        hosts.insert(drv.host.hostname_only());
    }
    for tr in state.full_summary.running_downloads.values() {
        hosts.insert(tr.host.hostname_only());
    }

    if hosts.len() <= 1 {
        return HashMap::new();
    }

    let mut map = HashMap::with_capacity(hosts.len());
    for h in hosts {
        let parts: Vec<&str> = h.split('.').collect();
        let abbrev = if parts.len() >= 2 {
            format!(
                "{}{}",
                parts[0].chars().next().unwrap_or('?'),
                parts[1].chars().next().unwrap_or('?')
            )
        } else {
            h.chars().take(2).collect()
        };
        map.insert(h.to_string(), abbrev);
    }
    map
}
