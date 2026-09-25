use crate::state::{
    BuildStatus, DependencySummary, DerivationId, DerivationInfo, InputDerivation, NomState,
    StorePathState,
};
use std::cmp::Reverse;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SortOrder {
    Failed(f64),
    Building(f64),
    Downloading(f64),
    Uploading(f64),
    Waiting,
    DownloadWaiting,
    Done(f64),
    Downloaded(f64),
    Uploaded(f64),
    Unknown,
}

impl Eq for SortOrder {}

impl Ord for SortOrder {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let tag = |s: &SortOrder| match s {
            SortOrder::Failed(_) => 0,
            SortOrder::Building(_) => 1,
            SortOrder::Downloading(_) => 2,
            SortOrder::Uploading(_) => 3,
            SortOrder::Waiting => 4,
            SortOrder::DownloadWaiting => 5,
            SortOrder::Done(_) => 6,
            SortOrder::Downloaded(_) => 7,
            SortOrder::Uploaded(_) => 8,
            SortOrder::Unknown => 9,
        };
        match (self, other) {
            (SortOrder::Failed(a), SortOrder::Failed(b))
            | (SortOrder::Building(a), SortOrder::Building(b))
            | (SortOrder::Downloading(a), SortOrder::Downloading(b))
            | (SortOrder::Uploading(a), SortOrder::Uploading(b)) => {
                a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
            }
            (SortOrder::Done(a), SortOrder::Done(b))
            | (SortOrder::Downloaded(a), SortOrder::Downloaded(b))
            | (SortOrder::Uploaded(a), SortOrder::Uploaded(b)) => {
                // Down Double: larger double comes first (smaller in Ord)
                b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)
            }
            _ => tag(self).cmp(&tag(other)),
        }
    }
}

impl PartialOrd for SortOrder {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SortKey {
    pub order_this: SortOrder,
    pub order_summary: SortOrder,
    pub running_builds_neg: Reverse<usize>,
    pub running_downloads_neg: Reverse<usize>,
    pub waiting_count: usize,
}

pub fn sort_order_from_summary(summary: &DependencySummary) -> SortOrder {
    if let Some(first_failed) = summary
        .failed_builds
        .values()
        .map(|f| f.end.at)
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    {
        return SortOrder::Failed(first_failed);
    }

    if let Some(first_building) = summary
        .running_builds
        .values()
        .map(|b| b.start)
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    {
        return SortOrder::Building(first_building);
    }

    if let Some(first_dl) = summary
        .running_downloads
        .values()
        .map(|d| d.start)
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    {
        return SortOrder::Downloading(first_dl);
    }

    if let Some(first_ul) = summary
        .running_uploads
        .values()
        .map(|u| u.start)
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    {
        return SortOrder::Uploading(first_ul);
    }

    if !summary.planned_builds.is_empty() {
        return SortOrder::Waiting;
    }

    if !summary.planned_downloads.is_empty() {
        return SortOrder::DownloadWaiting;
    }

    if let Some(latest_done) = summary.latest_completed_build_end {
        return SortOrder::Done(latest_done);
    }

    if let Some(latest_dl) = summary.latest_completed_download_start {
        return SortOrder::Downloaded(latest_dl);
    }

    if let Some(latest_ul) = summary
        .completed_uploads
        .values()
        .map(|u| u.start)
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    {
        return SortOrder::Uploaded(latest_ul);
    }

    SortOrder::Unknown
}

pub fn summary_including_root(state: &NomState, drv_id: DerivationId) -> DependencySummary {
    let drv = state.get_derivation(drv_id);
    let mut sum = drv.dependency_summary.clone();
    NomState::update_summary_for_derivation(
        &mut sum,
        &BuildStatus::Unknown,
        &drv.build_status,
        drv_id,
    );
    sum
}

pub fn summary_only_this_node(state: &NomState, drv_id: DerivationId) -> DependencySummary {
    let drv = state.get_derivation(drv_id);
    let mut sum = DependencySummary::default();
    NomState::update_summary_for_derivation(
        &mut sum,
        &BuildStatus::Unknown,
        &drv.build_status,
        drv_id,
    );

    let empty_states = BTreeSet::new();
    for &out_path_id in drv.outputs.values() {
        let out_info = state.get_store_path(out_path_id);
        NomState::update_summary_for_store_path(
            &mut sum,
            &empty_states,
            &out_info.states,
            out_path_id,
        );
    }

    sum
}

pub fn sort_order_for_this_node(state: &NomState, drv: &DerivationInfo) -> SortOrder {
    match &drv.build_status {
        BuildStatus::Failed(bi) => return SortOrder::Failed(bi.end.at),
        BuildStatus::Building(bi) => return SortOrder::Building(bi.start),
        _ => {}
    }

    let mut min_dl_start: Option<f64> = None;
    let mut min_ul_start: Option<f64> = None;
    let mut has_dl_planned = false;
    let mut max_dl_start: Option<f64> = None;
    let mut max_ul_start: Option<f64> = None;

    for &out_path_id in drv.outputs.values() {
        let out_info = state.get_store_path(out_path_id);
        for state in &out_info.states {
            match state {
                StorePathState::Downloading(ti) => {
                    min_dl_start = Some(min_dl_start.map_or(ti.start, |m| m.min(ti.start)));
                }
                StorePathState::Uploading(ti) => {
                    min_ul_start = Some(min_ul_start.map_or(ti.start, |m| m.min(ti.start)));
                }
                StorePathState::DownloadPlanned => {
                    has_dl_planned = true;
                }
                StorePathState::Downloaded(ti) => {
                    max_dl_start = Some(max_dl_start.map_or(ti.start, |m| m.max(ti.start)));
                }
                StorePathState::Uploaded(ti) => {
                    max_ul_start = Some(max_ul_start.map_or(ti.start, |m| m.max(ti.start)));
                }
            }
        }
    }

    if let Some(dl) = min_dl_start {
        return SortOrder::Downloading(dl);
    }
    if let Some(ul) = min_ul_start {
        return SortOrder::Uploading(ul);
    }
    if matches!(drv.build_status, BuildStatus::Planned) {
        return SortOrder::Waiting;
    }
    if has_dl_planned {
        return SortOrder::DownloadWaiting;
    }
    if let BuildStatus::Built(bi) = &drv.build_status {
        return SortOrder::Done(bi.end);
    }
    if let Some(dl) = max_dl_start {
        return SortOrder::Downloaded(dl);
    }
    if let Some(ul) = max_ul_start {
        return SortOrder::Uploaded(ul);
    }

    SortOrder::Unknown
}

pub fn sort_order_for_summary_including_root(
    summary: &DependencySummary,
    build_status: &BuildStatus,
) -> SortOrder {
    let status_failed = match build_status {
        BuildStatus::Failed(bi) => Some(bi.end.at),
        _ => None,
    };
    let min_failed = summary
        .failed_builds
        .values()
        .map(|f| f.end.at)
        .chain(status_failed)
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if let Some(first_failed) = min_failed {
        return SortOrder::Failed(first_failed);
    }

    let status_building = match build_status {
        BuildStatus::Building(bi) => Some(bi.start),
        _ => None,
    };
    let min_building = summary
        .running_builds
        .values()
        .map(|b| b.start)
        .chain(status_building)
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if let Some(first_building) = min_building {
        return SortOrder::Building(first_building);
    }

    if let Some(first_dl) = summary
        .running_downloads
        .values()
        .map(|d| d.start)
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    {
        return SortOrder::Downloading(first_dl);
    }

    if let Some(first_ul) = summary
        .running_uploads
        .values()
        .map(|u| u.start)
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    {
        return SortOrder::Uploading(first_ul);
    }

    if !summary.planned_builds.is_empty() || matches!(build_status, BuildStatus::Planned) {
        return SortOrder::Waiting;
    }

    if !summary.planned_downloads.is_empty() {
        return SortOrder::DownloadWaiting;
    }

    let status_built = match build_status {
        BuildStatus::Built(bi) => Some(bi.end),
        _ => None,
    };
    let max_done = match (summary.latest_completed_build_end, status_built) {
        (Some(sum_end), Some(st_end)) => Some(sum_end.max(st_end)),
        (Some(sum_end), None) => Some(sum_end),
        (None, Some(st_end)) => Some(st_end),
        (None, None) => None,
    };
    if let Some(latest_done) = max_done {
        return SortOrder::Done(latest_done);
    }

    if let Some(latest_dl) = summary.latest_completed_download_start {
        return SortOrder::Downloaded(latest_dl);
    }

    if let Some(latest_ul) = summary
        .completed_uploads
        .values()
        .map(|u| u.start)
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    {
        return SortOrder::Uploaded(latest_ul);
    }

    SortOrder::Unknown
}

pub fn calculate_sort_key(state: &NomState, drv_id: DerivationId) -> SortKey {
    let drv = state.get_derivation(drv_id);
    let order_this = sort_order_for_this_node(state, drv);
    let order_summary =
        sort_order_for_summary_including_root(&drv.dependency_summary, &drv.build_status);

    let is_building = matches!(drv.build_status, BuildStatus::Building(_)) as usize;
    let is_planned = matches!(drv.build_status, BuildStatus::Planned) as usize;

    SortKey {
        order_this,
        order_summary,
        running_builds_neg: Reverse(drv.dependency_summary.running_builds.len() + is_building),
        running_downloads_neg: Reverse(drv.dependency_summary.running_downloads.len()),
        waiting_count: drv.dependency_summary.planned_builds.len()
            + is_planned
            + drv.dependency_summary.planned_downloads.len(),
    }
}

pub fn sort_deps_of_set(state: &mut NomState, touched: &BTreeSet<DerivationId>) {
    for &drv_id in touched {
        let num_inputs = state.derivation_infos[drv_id.0].input_derivations.len();
        if num_inputs <= 1 {
            continue;
        }

        let mut indexed_keys: Vec<(usize, SortKey)> = state.derivation_infos[drv_id.0]
            .input_derivations
            .iter()
            .enumerate()
            .map(|(orig_idx, input)| (orig_idx, calculate_sort_key(state, input.derivation)))
            .collect();
        indexed_keys.sort_by(|a, b| a.1.cmp(&b.1));

        let is_already_sorted = indexed_keys
            .iter()
            .enumerate()
            .all(|(new_idx, (orig_idx, _))| new_idx == *orig_idx);
        if is_already_sorted {
            continue;
        }

        let mut old_inputs =
            std::mem::take(&mut state.derivation_infos[drv_id.0].input_derivations);
        let mut new_inputs = Vec::with_capacity(num_inputs);
        for (orig_idx, _) in indexed_keys {
            new_inputs.push(std::mem::replace(
                &mut old_inputs[orig_idx],
                InputDerivation {
                    derivation: DerivationId(0),
                    outputs: BTreeSet::new(),
                },
            ));
        }
        state.derivation_infos[drv_id.0].input_derivations = new_inputs;
    }
}

pub fn maintain_nom_state(state: &mut NomState, now: f64) {
    if !state.touched_ids.is_empty() {
        let touched = std::mem::take(&mut state.touched_ids);
        sort_deps_of_set(state, &touched);

        let mut roots = std::mem::take(&mut state.forest_roots);
        roots.sort_by_cached_key(|&r| calculate_sort_key(state, r));
        state.forest_roots = roots;
    }

    if state.evaluation_state.last_file_name.is_some()
        && state.evaluation_state.at <= now - 5.0
        && !state.full_summary.is_empty()
    {
        state.evaluation_state.last_file_name = None;
    }
}
