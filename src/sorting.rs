use crate::state::{
    BuildStatus, DependencySummary, DerivationId, DerivationInfo, NomState, StorePathState,
};
use roaring::RoaringBitmap;
use std::cmp::Reverse;

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
    let min_failed = if summary.failed_builds.is_empty() {
        status_failed
    } else {
        summary
            .failed_builds
            .values()
            .map(|f| f.end.at)
            .chain(status_failed)
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    };
    if let Some(first_failed) = min_failed {
        return SortOrder::Failed(first_failed);
    }

    let status_building = match build_status {
        BuildStatus::Building(bi) => Some(bi.start),
        _ => None,
    };
    let min_building = if summary.running_builds.is_empty() {
        status_building
    } else {
        summary
            .running_builds
            .values()
            .map(|b| b.start)
            .chain(status_building)
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    };
    if let Some(first_building) = min_building {
        return SortOrder::Building(first_building);
    }

    if !summary.running_downloads.is_empty() {
        if let Some(first_dl) = summary
            .running_downloads
            .values()
            .map(|d| d.start)
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        {
            return SortOrder::Downloading(first_dl);
        }
    }

    if !summary.running_uploads.is_empty() {
        if let Some(first_ul) = summary
            .running_uploads
            .values()
            .map(|u| u.start)
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        {
            return SortOrder::Uploading(first_ul);
        }
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

    if !summary.completed_uploads.is_empty() {
        if let Some(latest_ul) = summary
            .completed_uploads
            .values()
            .map(|u| u.start)
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        {
            return SortOrder::Uploaded(latest_ul);
        }
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
        waiting_count: drv.dependency_summary.planned_builds.len() as usize
            + is_planned
            + drv.dependency_summary.planned_downloads.len() as usize,
    }
}

pub fn sort_deps_of_set(state: &mut NomState, touched: &RoaringBitmap) {
    for raw_id in touched.iter() {
        let drv_id = DerivationId(raw_id as usize);
        if drv_id.0 >= state.derivation_infos.len() {
            continue;
        }
        let num_inputs = state.derivation_infos[drv_id.0].input_derivations.len();
        if num_inputs <= 1 {
            continue;
        }

        let mut deps = std::mem::take(&mut state.derivation_infos[drv_id.0].input_derivations);
        deps.sort_by(|a, b| {
            calculate_sort_key(state, a.derivation)
                .cmp(&calculate_sort_key(state, b.derivation))
                .then_with(|| {
                    state.get_derivation(a.derivation).name.cmp(&state.get_derivation(b.derivation).name)
                })
        });
        state.derivation_infos[drv_id.0].input_derivations = deps;
    }
}

pub fn maintain_nom_state(state: &mut NomState, now: f64) {
    if !state.touched_ids.is_empty() {
        let touched = std::mem::take(&mut state.touched_ids);
        sort_deps_of_set(state, &touched);

        let mut roots = std::mem::take(&mut state.forest_roots);
        roots.sort_by(|a, b| {
            calculate_sort_key(state, *a)
                .cmp(&calculate_sort_key(state, *b))
                .then_with(|| {
                    state.get_derivation(*a).name.cmp(&state.get_derivation(*b).name)
                })
        });
        state.forest_roots = roots;
    }

    if state.evaluation_state.last_file_name.is_some()
        && state.evaluation_state.at <= now - 5.0
        && !state.full_summary.is_empty()
    {
        state.evaluation_state.last_file_name = None;
    }
}
