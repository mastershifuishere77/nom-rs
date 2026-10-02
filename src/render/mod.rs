pub mod progress;
pub mod table;
pub mod tree;

use crate::render::progress::print_bytes;
use crate::render::table::{
    display_width, prepend_lines, print_aligned_table, truncate_display, Entry, BLUE, BOLD, GREEN,
    GREY, MAGENTA, RED, RESET, YELLOW,
};
use crate::render::tree::{show_forest, TreeNode};
use crate::sorting::{calculate_sort_key, SortKey};
use crate::state::{
    BuildStatus, DependencySummary, DerivationId, DerivationInfo, FailType, Host, NomState,
    ProgressState, TransferInfo,
};
use chrono::Local;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeSet;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HostSort {
    #[default]
    None,
    DownloadSize,
    Builds,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Config {
    pub silent: bool,
    pub piping: bool,
    pub host_sort: HostSort,
    pub host_cap: Option<usize>,
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
    } else if !state.full_summary.running_downloads.is_empty()
        || !state.full_summary.running_uploads.is_empty()
    {
        sections.push(render_active_transfers(
            state,
            term_width.saturating_sub(2),
            max_tree_height,
            now,
        ));
    }

    // 4. Summary table section
    if !state.full_summary.is_empty() || !sections.is_empty() {
        sections.push(render_summary_table(state, config, now));
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
    let derivations_to_show = select_derivations_to_show(state, max_height);
    let mut seen_build = FxHashSet::default();
    let drv_forest =
        go_build_forest(state, &state.forest_roots, &derivations_to_show, &mut seen_build);

    let forest: Vec<TreeNode<Option<f64>>> = drv_forest
        .iter()
        .map(|node| convert_tree(state, node, true, &host_abbrevs, now))
        .collect();

    let rows = show_forest(&forest);

    let num_raw_roots = state.forest_roots.len();
    let num_roots = forest.len();
    let graph_title = format!("{}Dependency Graph{}", BOLD, RESET);
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

fn render_active_transfers(
    state: &NomState,
    _max_width: usize,
    max_height: usize,
    now: f64,
) -> String {
    let host_abbrevs = compute_host_abbrevs(state);
    let mut rows: Vec<String> = Vec::new();

    // 1. Running downloads
    for (&path_id, dl) in &state.full_summary.running_downloads {
        let sp = state.get_store_path(path_id);
        let path_name = &sp.name.name;

        let mut parts = Vec::new();
        parts.push(format!(
            "{}{}{} {} {}{}",
            BOLD, YELLOW, DOWN, RUNNING, path_name, RESET
        ));

        let host_name = dl.host.hostname_only();
        let disambiguated = host_abbrevs
            .get(host_name)
            .map(|s| s.as_str())
            .unwrap_or(host_name);
        parts.push(format!("{}from {}{}", MAGENTA, disambiguated, RESET));

        if now - dl.start > 1.0 {
            parts.push(format!(
                "{} {}",
                CLOCK,
                format_duration(now - dl.start)
            ));
        }

        if let Some(act_id) = dl.activity_id {
            if let Some(act) = state.activities.get(&act_id) {
                if let Some(p) = act.file_transfer_progress.as_ref().or(act.progress.as_ref()) {
                    if p.expected > 0 {
                        parts.push(format!(
                            "{}{} {}/{}{}",
                            GREEN,
                            DOWN,
                            print_bytes(p.done),
                            print_bytes(p.expected),
                            RESET
                        ));
                    }
                }
            }
        }

        rows.push(parts.join(" "));
        if rows.len() >= max_height {
            break;
        }
    }

    // 2. Running uploads
    if rows.len() < max_height {
        for (&path_id, ul) in &state.full_summary.running_uploads {
            let sp = state.get_store_path(path_id);
            let path_name = &sp.name.name;

            let mut parts = Vec::new();
            parts.push(format!(
                "{}{}{} {} {}{}",
                BOLD, YELLOW, UP, RUNNING, path_name, RESET
            ));

            let host_name = ul.host.hostname_only();
            let disambiguated = host_abbrevs
                .get(host_name)
                .map(|s| s.as_str())
                .unwrap_or(host_name);
            parts.push(format!("{}to {}{}", MAGENTA, disambiguated, RESET));

            if now - ul.start > 1.0 {
                parts.push(format!(
                    "{} {}",
                    CLOCK,
                    format_duration(now - ul.start)
                ));
            }

            rows.push(parts.join(" "));
            if rows.len() >= max_height {
                break;
            }
        }
    }

    if rows.is_empty() {
        return String::new();
    }

    let header = format!(" {}Downloads{}:", BOLD, RESET);
    let mut lines = Vec::with_capacity(rows.len() + 1);
    lines.push(header);
    lines.extend(rows);

    prepend_lines(
        HORIZONTAL,
        &format!("{} ", VERTICAL),
        &format!("{} ", VERTICAL),
        &lines,
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TreeLocation {
    Root,
    Twig,
    Leaf,
}

struct DrvNode {
    drv_id: DerivationId,
    children: Vec<DrvNode>,
}

fn go_derivations_to_show(
    state: &NomState,
    this_drv: DerivationId,
    limits_height: usize,
    seen_ids: &mut FxHashSet<DerivationId>,
    sorted_set: &mut BTreeSet<(bool, SortKey, DerivationId)>,
) {
    if limits_height == 0 {
        return;
    }
    if state.is_summary_including_root_empty(this_drv) {
        return;
    }
    if seen_ids.contains(&this_drv) {
        return;
    }

    let sort_key = calculate_sort_key(state, this_drv);
    let drv = state.get_derivation(this_drv);

    let mut may_hide = true;

    match &drv.build_status {
        BuildStatus::Building(_) | BuildStatus::Failed(_)
            if !seen_ids.contains(&this_drv) => {
                may_hide = false;
            }
        _ => {}
    }

    if may_hide {
        for &failed_id in drv.dependency_summary.failed_builds.keys() {
            if !seen_ids.contains(&failed_id) {
                may_hide = false;
                break;
            }
        }
    }

    if may_hide {
        for &running_id in drv.dependency_summary.running_builds.keys() {
            if !seen_ids.contains(&running_id) {
                may_hide = false;
                break;
            }
        }
    }

    if may_hide {
        for &path_id in drv
            .dependency_summary
            .running_downloads
            .keys()
            .chain(drv.dependency_summary.running_uploads.keys())
        {
            if path_id.0 < state.store_path_infos.len() {
                let sp_info = &state.store_path_infos[path_id.0];
                if let Some(prod_id) = sp_info.producer {
                    if !seen_ids.contains(&prod_id) {
                        may_hide = false;
                        break;
                    }
                }
                for &input_id in &sp_info.input_for {
                    if !seen_ids.contains(&input_id) {
                        may_hide = false;
                        break;
                    }
                }
                if !may_hide {
                    break;
                }
            }
        }
    }

    let can_fit = !may_hide
        || sorted_set.len() < limits_height
        || sorted_set
            .iter()
            .nth(limits_height.saturating_sub(1))
            .is_some_and(|elem| sort_key < elem.1);

    if !can_fit {
        return;
    }

    seen_ids.insert(this_drv);
    sorted_set.insert((may_hide, sort_key, this_drv));

    for input in &drv.input_derivations {
        go_derivations_to_show(state, input.derivation, limits_height, seen_ids, sorted_set);
    }
}

pub fn select_derivations_to_show(state: &NomState, max_height: usize) -> FxHashSet<DerivationId> {
    if max_height == 0 {
        return FxHashSet::default();
    }

    let mut seen_ids = FxHashSet::default();
    let mut sorted_set = BTreeSet::new();

    for &root_id in &state.forest_roots {
        go_derivations_to_show(state, root_id, max_height, &mut seen_ids, &mut sorted_set);
    }

    sorted_set
        .iter()
        .enumerate()
        .take_while(|&(index, &(can_be_hidden, _, _))| !can_be_hidden || index < max_height)
        .map(|(_, &(_, _, drv_id))| drv_id)
        .collect()
}

fn go_build_forest(
    state: &NomState,
    drvs: &[DerivationId],
    derivations_to_show: &FxHashSet<DerivationId>,
    seen_ids: &mut FxHashSet<DerivationId>,
) -> Vec<DrvNode> {
    let mut forest = Vec::new();
    for &this_drv in drvs {
        if !seen_ids.contains(&this_drv) && derivations_to_show.contains(&this_drv) {
            seen_ids.insert(this_drv);
            let drv = state.get_derivation(this_drv);
            let child_ids: Vec<DerivationId> =
                drv.input_derivations.iter().map(|i| i.derivation).collect();
            let subforest = go_build_forest(state, &child_ids, derivations_to_show, seen_ids);
            forest.push(DrvNode {
                drv_id: this_drv,
                children: subforest,
            });
        }
    }
    forest
}

fn convert_tree(
    state: &NomState,
    node: &DrvNode,
    top: bool,
    host_abbrevs: &FxHashMap<String, String>,
    now: f64,
) -> TreeNode<Option<f64>> {
    let loc = if node.children.is_empty() {
        TreeLocation::Leaf
    } else if top {
        TreeLocation::Root
    } else {
        TreeLocation::Twig
    };

    let drv = state.get_derivation(node.drv_id);
    let (label, progress) = format_derivation_node(state, drv, loc, host_abbrevs, now);

    let children = node
        .children
        .iter()
        .map(|c| convert_tree(state, c, false, host_abbrevs, now))
        .collect();

    TreeNode {
        label,
        extra: progress,
        children,
    }
}

fn format_derivation_node(
    state: &NomState,
    drv: &DerivationInfo,
    loc: TreeLocation,
    host_abbrevs: &FxHashMap<String, String>,
    now: f64,
) -> (String, Option<f64>) {
    let (planned, row_str, progress_val) = format_derivation_row(state, drv, host_abbrevs, now);
    let summary = format_dependency_summary(&drv.dependency_summary);
    let display_summary = loc == TreeLocation::Leaf && planned && !summary.is_empty();
    let label = if display_summary {
        format!("{}{} waiting for {}{}", row_str, GREY, summary, RESET)
    } else {
        row_str
    };
    (label, progress_val)
}

fn format_derivation_row(
    state: &NomState,
    drv: &DerivationInfo,
    host_abbrevs: &FxHashMap<String, String>,
    now: f64,
) -> (bool, String, Option<f64>) {
    let drv_name = format_differing_platform(state, drv);
    let mut progress_val = None;

    // Check downloads / uploads on outputs
    let dep_sum = &drv.dependency_summary;
    let summary_has_transfers = !dep_sum.running_downloads.is_empty()
        || !dep_sum.running_uploads.is_empty()
        || !dep_sum.completed_downloads.is_empty()
        || !dep_sum.completed_uploads.is_empty()
        || !dep_sum.planned_downloads.is_empty();

    let (
        running_downloads,
        running_uploads,
        completed_downloads,
        completed_uploads,
        is_planned_download,
    ) = if summary_has_transfers {
        let mut rd = Vec::new();
        let mut ru = Vec::new();
        let mut cd = Vec::new();
        let mut cu = Vec::new();
        let mut is_pd = false;
        for &path_id in drv.outputs.values() {
            if let Some(dl) = dep_sum.running_downloads.get(&path_id) {
                rd.push(dl);
            }
            if let Some(ul) = dep_sum.running_uploads.get(&path_id) {
                ru.push(ul);
            }
            if let Some(dl) = dep_sum.completed_downloads.get(&path_id) {
                cd.push(dl);
            }
            if let Some(ul) = dep_sum.completed_uploads.get(&path_id) {
                cu.push(ul);
            }
            if dep_sum.planned_downloads.contains(path_id.0 as u32) {
                is_pd = true;
            }
        }
        (rd, ru, cd, cu, is_pd)
    } else {
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), false)
    };

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
            parts.push(format!("{}{} {}{}", GREEN, DOWN, prog_text.trim(), RESET));
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
            parts.push(format!("{}{} {}{}", GREEN, UP, prog_text.trim(), RESET));
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
                        .filter_map(|act| {
                            act.file_transfer_progress
                                .as_ref()
                                .or(act.progress.as_ref())
                        })
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
                        .filter_map(|act| {
                            act.file_transfer_progress
                                .as_ref()
                                .or(act.progress.as_ref())
                        })
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
                } else if !drv.dependency_summary.is_empty() {
                    format!("{}{} {}{}", BLUE, TODO, drv_name, RESET)
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

                let curl_progress = drv.curl_progress.as_ref().or_else(|| {
                    bi.activity_id
                        .and_then(|id| state.activities.get(&id))
                        .and_then(|act| act.curl_progress.as_ref())
                });

                if let Some(cp) = curl_progress {
                    if cp.host != Host::Localhost {
                        let h_str = cp.host.hostname_only();
                        let label = host_abbrevs.get(h_str).map(|s| s.as_str()).unwrap_or(h_str);
                        parts.push(format!("from {}{}{}", MAGENTA, label, RESET));
                    }
                } else {
                    let host_str = format_single_host(&bi.host, host_abbrevs, true);
                    if !host_str.is_empty() {
                        parts.push(host_str.trim().to_string());
                    }
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

                let file_transfer_progress = bi
                    .activity_id
                    .and_then(|id| state.activities.get(&id))
                    .and_then(|act| act.file_transfer_progress.as_ref().or(act.progress.as_ref()));

                if let Some(p) = file_transfer_progress {
                    if p.expected > 0 {
                        progress_val = Some(p.done as f64 / p.expected as f64);
                        parts.push(format!(
                            "{}{} {}/{}{}",
                            GREEN,
                            DOWN,
                            print_bytes(p.done),
                            print_bytes(p.expected),
                            RESET
                        ));
                    } else if p.done > 0 {
                        parts.push(format!(
                            "{}{} {}{}",
                            GREEN,
                            DOWN,
                            print_bytes(p.done),
                            RESET
                        ));
                    }
                } else if let Some(cp) = curl_progress {
                    if cp.total_bytes > 0 {
                        progress_val = Some(cp.done_bytes as f64 / cp.total_bytes as f64);
                        parts.push(format!(
                            "{}{} {}/{}{}",
                            GREEN,
                            DOWN,
                            print_bytes(cp.done_bytes),
                            print_bytes(cp.total_bytes),
                            RESET
                        ));
                    } else if cp.done_bytes > 0 {
                        parts.push(format!(
                            "{}{} {}{}",
                            GREEN,
                            DOWN,
                            print_bytes(cp.done_bytes),
                            RESET
                        ));
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

                let curl_progress = drv.curl_progress.as_ref().or_else(|| {
                    bi.activity_id
                        .and_then(|id| state.activities.get(&id))
                        .and_then(|act| act.curl_progress.as_ref())
                });

                if let Some(cp) = curl_progress {
                    if cp.total_bytes > 0 || cp.done_bytes > 0 {
                        let bytes = cp.total_bytes.max(cp.done_bytes);
                        extra_parts.push(print_bytes(bytes));
                    }
                    if cp.host != Host::Localhost {
                        let h_str = cp.host.hostname_only();
                        let label = host_abbrevs.get(h_str).map(|s| s.as_str()).unwrap_or(h_str);
                        extra_parts.push(format!("from {}", label));
                    }
                } else {
                    let host_str = format_single_host(&bi.host, host_abbrevs, false);
                    if !host_str.is_empty() {
                        extra_parts.push(host_str.trim().to_string());
                    }
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

    (is_planned, row_str, progress_val)
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
                if let Some(p) = act
                    .file_transfer_progress
                    .as_ref()
                    .or(act.progress.as_ref())
                {
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
    base_name.to_string()
}

fn format_single_host(host: &Host, host_abbrevs: &FxHashMap<String, String>, color: bool) -> String {
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
    host_abbrevs: &FxHashMap<String, String>,
    dir: &str,
) -> String {
    if host_abbrevs.len() <= 1 || transfers.is_empty() {
        return String::new();
    }
    let unique_hosts: FxHashSet<&Host> = transfers.iter().map(|t| &t.host).collect();
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

fn render_summary_table(state: &NomState, config: Config, now: f64) -> String {
    let s = &state.full_summary;
    let num_running_builds = s.running_builds.len();
    let num_completed_builds = s.completed_builds.len();
    let num_planned_builds = s.planned_builds.len() as usize;
    let total_builds = num_running_builds + num_completed_builds + num_planned_builds;

    let num_curl_running = s
        .running_builds
        .keys()
        .filter(|&&drv_id| state.get_derivation(drv_id).curl_progress.is_some())
        .count();
    let num_curl_completed = s
        .completed_builds
        .keys()
        .filter(|&&drv_id| state.get_derivation(drv_id).curl_progress.is_some())
        .count();

    let num_running_dl = s.running_downloads.len() + num_curl_running;
    let num_completed_dl = s.completed_downloads.len() + num_curl_completed;
    let num_planned_dl = s.planned_downloads.len() as usize;
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
        pb: usize,
        rd: usize,
        cd: usize,
        ru: usize,
        cu: usize,
        host_done: usize,
        host_expected: usize,
    }

    static LOCALHOST: Host = Host::Localhost;
    let mut host_stats: FxHashMap<&str, HostStats> = FxHashMap::default();
    host_stats.insert(
        LOCALHOST.hostname_only(),
        HostStats {
            host: &LOCALHOST,
            rb: 0,
            cb: 0,
            pb: num_planned_builds,
            rd: 0,
            cd: 0,
            ru: 0,
            cu: 0,
            host_done: 0,
            host_expected: 0,
        },
    );
    let mut total_done = 0;
    let mut total_expected = 0;

    for (drv_id, b) in &s.running_builds {
        let drv = state.get_derivation(*drv_id);
        let curl_progress = drv.curl_progress.as_ref().or_else(|| {
            b.activity_id
                .and_then(|id| state.activities.get(&id))
                .and_then(|act| act.curl_progress.as_ref())
        });

        if let Some(cp) = curl_progress {
            let stats = host_stats
                .entry(cp.host.hostname_only())
                .or_insert_with(|| HostStats {
                    host: &cp.host,
                    rb: 0,
                    cb: 0,
                    pb: 0,
                    rd: 0,
                    cd: 0,
                    ru: 0,
                    cu: 0,
                    host_done: 0,
                    host_expected: 0,
                });
            stats.rd += 1;
            stats.host_done += cp.done_bytes;
            stats.host_expected += cp.total_bytes;
            total_done += cp.done_bytes;
            total_expected += cp.total_bytes;

            host_stats
                .entry(b.host.hostname_only())
                .or_insert_with(|| HostStats {
                    host: &b.host,
                    rb: 0,
                    cb: 0,
                    pb: 0,
                    rd: 0,
                    cd: 0,
                    ru: 0,
                    cu: 0,
                    host_done: 0,
                    host_expected: 0,
                })
                .rb += 1;
        } else {
            host_stats
                .entry(b.host.hostname_only())
                .or_insert_with(|| HostStats {
                    host: &b.host,
                    rb: 0,
                    cb: 0,
                    pb: 0,
                    rd: 0,
                    cd: 0,
                    ru: 0,
                    cu: 0,
                    host_done: 0,
                    host_expected: 0,
                })
                .rb += 1;
        }
    }
    for (drv_id, b) in &s.completed_builds {
        let drv = state.get_derivation(*drv_id);
        let curl_progress = drv.curl_progress.as_ref().or_else(|| {
            b.activity_id
                .and_then(|id| state.activities.get(&id))
                .and_then(|act| act.curl_progress.as_ref())
        });

        if let Some(cp) = curl_progress {
            let stats = host_stats
                .entry(cp.host.hostname_only())
                .or_insert_with(|| HostStats {
                    host: &cp.host,
                    rb: 0,
                    cb: 0,
                    pb: 0,
                    rd: 0,
                    cd: 0,
                    ru: 0,
                    cu: 0,
                    host_done: 0,
                    host_expected: 0,
                });
            stats.cd += 1;
            let d_done = cp.total_bytes.max(cp.done_bytes);
            stats.host_done += d_done;
            stats.host_expected += cp.total_bytes;
            total_done += d_done;
            total_expected += cp.total_bytes;

            host_stats
                .entry(b.host.hostname_only())
                .or_insert_with(|| HostStats {
                    host: &b.host,
                    rb: 0,
                    cb: 0,
                    pb: 0,
                    rd: 0,
                    cd: 0,
                    ru: 0,
                    cu: 0,
                    host_done: 0,
                    host_expected: 0,
                })
                .cb += 1;
        } else {
            host_stats
                .entry(b.host.hostname_only())
                .or_insert_with(|| HostStats {
                    host: &b.host,
                    rb: 0,
                    cb: 0,
                    pb: 0,
                    rd: 0,
                    cd: 0,
                    ru: 0,
                    cu: 0,
                    host_done: 0,
                    host_expected: 0,
                })
                .cb += 1;
        }
    }
    for d in s.running_downloads.values() {
        let stats = host_stats
            .entry(d.host.hostname_only())
            .or_insert_with(|| HostStats {
                host: &d.host,
                rb: 0,
                cb: 0,
                pb: 0,
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
                if let Some(p) = act
                    .file_transfer_progress
                    .as_ref()
                    .or(act.progress.as_ref())
                {
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
                pb: 0,
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
                if let Some(p) = act
                    .file_transfer_progress
                    .as_ref()
                    .or(act.progress.as_ref())
                {
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
                pb: 0,
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
                pb: 0,
                rd: 0,
                cd: 0,
                ru: 0,
                cu: 0,
                host_done: 0,
                host_expected: 0,
            })
            .cu += 1;
    }

    let is_active_host = |s: &HostStats| {
        s.rb > 0
            || s.cb > 0
            || s.pb > 0
            || s.rd > 0
            || s.cd > 0
            || s.ru > 0
            || s.cu > 0
            || s.host_done > 0
            || s.host_expected > 0
    };

    let active_keys: Vec<&str> = host_stats
        .iter()
        .filter(|(_, s)| is_active_host(s))
        .map(|(k, _)| *k)
        .collect();

    let show_hosts = active_keys.len() > 1;

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
        let localhost_active = active_keys.contains(&"localhost");
        let mut other_keys: Vec<&str> = active_keys
            .into_iter()
            .filter(|&k| k != "localhost")
            .collect();

        match config.host_sort {
            HostSort::DownloadSize => {
                other_keys.sort_by(|&a, &b| {
                    let sa = &host_stats[a];
                    let sb = &host_stats[b];
                    let size_a = sa.host_expected.max(sa.host_done);
                    let size_b = sb.host_expected.max(sb.host_done);
                    let builds_a = sa.rb + sa.cb + sa.pb;
                    let builds_b = sb.rb + sb.cb + sb.pb;
                    size_a
                        .cmp(&size_b)
                        .then_with(|| builds_a.cmp(&builds_b))
                        .then_with(|| a.cmp(b))
                });
            }
            HostSort::Builds => {
                other_keys.sort_by(|&a, &b| {
                    let sa = &host_stats[a];
                    let sb = &host_stats[b];
                    let builds_a = sa.rb + sa.cb + sa.pb;
                    let builds_b = sb.rb + sb.cb + sb.pb;
                    let size_a = sa.host_expected.max(sa.host_done);
                    let size_b = sb.host_expected.max(sb.host_done);
                    builds_a
                        .cmp(&builds_b)
                        .then_with(|| size_a.cmp(&size_b))
                        .then_with(|| a.cmp(b))
                });
            }
            HostSort::None => {
                if config.host_cap.is_some() {
                    other_keys.sort_by(|&a, &b| {
                        let sa = &host_stats[a];
                        let sb = &host_stats[b];
                        let act_a = (sa.host_expected.max(sa.host_done), sa.rb + sa.cb + sa.pb);
                        let act_b = (sb.host_expected.max(sb.host_done), sb.rb + sb.cb + sb.pb);
                        act_a.cmp(&act_b).then_with(|| a.cmp(b))
                    });
                } else {
                    other_keys.sort_by(|a, b| a.split('.').rev().cmp(b.split('.').rev()));
                }
            }
        }

        struct DisplayHostStats {
            host_display: String,
            rb: usize,
            cb: usize,
            pb: usize,
            rd: usize,
            cd: usize,
            ru: usize,
            cu: usize,
            host_done: usize,
            host_expected: usize,
        }

        let total_other = other_keys.len();
        let mut final_host_stats = Vec::new();

        // 1. localhost is always pinned at the top (if active), non-sortable and never capped into "other"
        if localhost_active {
            let s = &host_stats["localhost"];
            final_host_stats.push(DisplayHostStats {
                host_display: s.host.format_with_proto_context(),
                rb: s.rb,
                cb: s.cb,
                pb: s.pb,
                rd: s.rd,
                cd: s.cd,
                ru: s.ru,
                cu: s.cu,
                host_done: s.host_done,
                host_expected: s.host_expected,
            });
        }

        // 2. Cap and sort other (remote/substituter) hosts
        if let Some(cap) = config.host_cap {
            if cap < total_other {
                let num_capped = total_other - cap;
                let capped_keys = &other_keys[..num_capped];
                let display_keys = &other_keys[num_capped..];

                let mut other = DisplayHostStats {
                    host_display: "other".to_string(),
                    rb: 0,
                    cb: 0,
                    pb: 0,
                    rd: 0,
                    cd: 0,
                    ru: 0,
                    cu: 0,
                    host_done: 0,
                    host_expected: 0,
                };

                for &k in capped_keys {
                    let s = &host_stats[k];
                    other.rb += s.rb;
                    other.cb += s.cb;
                    other.pb += s.pb;
                    other.rd += s.rd;
                    other.cd += s.cd;
                    other.ru += s.ru;
                    other.cu += s.cu;
                    other.host_done += s.host_done;
                    other.host_expected += s.host_expected;
                }

                // "other" appears below localhost
                final_host_stats.push(other);

                // Then the capped non-localhost hosts in ascending order (biggest at bottom)
                for &k in display_keys {
                    let s = &host_stats[k];
                    final_host_stats.push(DisplayHostStats {
                        host_display: s.host.format_with_proto_context(),
                        rb: s.rb,
                        cb: s.cb,
                        pb: s.pb,
                        rd: s.rd,
                        cd: s.cd,
                        ru: s.ru,
                        cu: s.cu,
                        host_done: s.host_done,
                        host_expected: s.host_expected,
                    });
                }
            } else {
                for &k in &other_keys {
                    let s = &host_stats[k];
                    final_host_stats.push(DisplayHostStats {
                        host_display: s.host.format_with_proto_context(),
                        rb: s.rb,
                        cb: s.cb,
                        pb: s.pb,
                        rd: s.rd,
                        cd: s.cd,
                        ru: s.ru,
                        cu: s.cu,
                        host_done: s.host_done,
                        host_expected: s.host_expected,
                    });
                }
            }
        } else {
            for &k in &other_keys {
                let s = &host_stats[k];
                final_host_stats.push(DisplayHostStats {
                    host_display: s.host.format_with_proto_context(),
                    rb: s.rb,
                    cb: s.cb,
                    pb: s.pb,
                    rd: s.rd,
                    cd: s.cd,
                    ru: s.ru,
                    cu: s.cu,
                    host_done: s.host_done,
                    host_expected: s.host_expected,
                });
            }
        }

        for stats in final_host_stats {
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
                if stats.pb > 0 {
                    host_row.push(non_zero_entry(TODO, stats.pb, |e| e.blue()));
                } else {
                    host_row.push(Entry::text(""));
                }
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
                if stats.host_expected > 0 || stats.host_done > 0 {
                    let expected = stats.host_expected.max(stats.host_done);
                    host_row.push(
                        Entry::text(format!(
                            "{} {}/{}",
                            DOWN,
                            print_bytes(stats.host_done),
                            print_bytes(expected)
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
            host_row.push(Entry::header(stats.host_display).magenta());
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
        } else if let Some(planned_bytes) = state.planned_download_bytes {
            total_row.push(
                Entry::text(format!(
                    "{} {}/{}",
                    DOWN,
                    print_bytes(total_done),
                    print_bytes(planned_bytes)
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
        } else if !state.nix_traces.is_empty() {
            format!(
                "{}{}{}{} Finished {}with {} traces reported by nix{} at {} after {}{}",
                BOLD,
                YELLOW,
                WARNING,
                GREEN,
                YELLOW,
                state.nix_traces.len(),
                GREEN,
                local_time,
                dur,
                RESET
            )
        } else {
            format!(
                "{}{}Finished at {} after {}{}",
                BOLD, GREEN, local_time, dur, RESET
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

fn compute_host_abbrevs(state: &NomState) -> FxHashMap<String, String> {
    let mut remote_hosts: FxHashSet<&str> = FxHashSet::default();
    for h in &state.remote_hosts {
        remote_hosts.insert(h.as_str());
    }
    for drv in state.full_summary.running_builds.values() {
        if let Host::Remote { ref host, .. } = drv.host {
            remote_hosts.insert(host.as_str());
        }
    }
    for tr in state.full_summary.running_downloads.values() {
        if let Host::Remote { ref host, .. } = tr.host {
            remote_hosts.insert(host.as_str());
        }
    }
    for tr in state.full_summary.running_uploads.values() {
        if let Host::Remote { ref host, .. } = tr.host {
            remote_hosts.insert(host.as_str());
        }
    }

    if remote_hosts.len() <= 1 {
        return FxHashMap::default();
    }

    let mut map = FxHashMap::default();
    for h in remote_hosts {
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
