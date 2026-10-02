use crate::cache_reports::BuildReportMap;
use crate::parser::json::{Activity, ActivityProgress};
pub use crate::types::{
    Derivation, DerivationId, FailType, Host, OutputName, StorePath, StorePathId,
};
use compact_str::CompactString;
use roaring::RoaringBitmap;
use rustc_hash::FxHashMap;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProgressState {
    JustStarted,
    InputReceived,
    Finished,
}

#[derive(Clone, Debug)]
pub struct TransferInfo<T> {
    pub host: Host,
    pub start: f64,
    pub activity_id: Option<u64>,
    pub end: T,
}

impl<T: PartialEq> PartialEq for TransferInfo<T> {
    fn eq(&self, other: &Self) -> bool {
        self.host == other.host
            && self.start.total_cmp(&other.start) == std::cmp::Ordering::Equal
            && self.activity_id == other.activity_id
            && self.end == other.end
    }
}

impl<T: Eq> Eq for TransferInfo<T> {}

impl<T: Ord> Ord for TransferInfo<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.host
            .cmp(&other.host)
            .then_with(|| self.start.total_cmp(&other.start))
            .then_with(|| self.activity_id.cmp(&other.activity_id))
            .then_with(|| self.end.cmp(&other.end))
    }
}

impl<T: Ord> PartialOrd for TransferInfo<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub type RunningTransferInfo = TransferInfo<()>;

#[derive(Clone, Debug, PartialEq)]
pub struct CompletedEnd(pub Option<f64>);

impl Eq for CompletedEnd {}

impl Ord for CompletedEnd {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self.0, other.0) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (Some(a), Some(b)) => a.total_cmp(&b),
        }
    }
}

impl PartialOrd for CompletedEnd {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub type CompletedTransferInfo = TransferInfo<CompletedEnd>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StorePathState {
    DownloadPlanned,
    Downloading(RunningTransferInfo),
    Uploading(RunningTransferInfo),
    Downloaded(CompletedTransferInfo),
    Uploaded(CompletedTransferInfo),
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildFail {
    pub at: f64,
    pub fail_type: FailType,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildInfo<T> {
    pub start: f64,
    pub host: Host,
    pub estimate: Option<i64>,
    pub activity_id: Option<u64>,
    pub end: T,
}

pub type RunningBuildInfo = BuildInfo<()>;
pub type CompletedBuildInfo = BuildInfo<f64>;
pub type FailedBuildInfo = BuildInfo<BuildFail>;

#[derive(Clone, Debug, PartialEq)]
pub enum BuildStatus {
    Unknown,
    Planned,
    Building(RunningBuildInfo),
    Failed(FailedBuildInfo),
    Built(CompletedBuildInfo),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DependencySummary {
    pub planned_builds: RoaringBitmap,
    pub running_builds: BTreeMap<DerivationId, RunningBuildInfo>,
    pub completed_builds: BTreeMap<DerivationId, CompletedBuildInfo>,
    pub failed_builds: BTreeMap<DerivationId, FailedBuildInfo>,
    pub planned_downloads: RoaringBitmap,
    pub running_downloads: BTreeMap<StorePathId, RunningTransferInfo>,
    pub completed_downloads: BTreeMap<StorePathId, CompletedTransferInfo>,
    pub running_uploads: BTreeMap<StorePathId, RunningTransferInfo>,
    pub completed_uploads: BTreeMap<StorePathId, CompletedTransferInfo>,
    pub latest_completed_build_end: Option<f64>,
    pub latest_completed_download_start: Option<f64>,
}

impl DependencySummary {
    pub fn is_empty(&self) -> bool {
        self.planned_builds.is_empty()
            && self.running_builds.is_empty()
            && self.completed_builds.is_empty()
            && self.failed_builds.is_empty()
            && self.planned_downloads.is_empty()
            && self.running_downloads.is_empty()
            && self.completed_downloads.is_empty()
            && self.running_uploads.is_empty()
            && self.completed_uploads.is_empty()
    }

    pub fn merge(&mut self, other: &DependencySummary) {
        self.planned_builds |= &other.planned_builds;
        for (k, v) in &other.running_builds {
            self.running_builds.insert(*k, v.clone());
        }
        for (k, v) in &other.completed_builds {
            self.completed_builds.insert(*k, v.clone());
        }
        for (k, v) in &other.failed_builds {
            self.failed_builds.insert(*k, v.clone());
        }
        self.planned_downloads |= &other.planned_downloads;
        for (k, v) in &other.running_downloads {
            self.running_downloads.insert(*k, v.clone());
        }
        for (k, v) in &other.completed_downloads {
            self.completed_downloads.insert(*k, v.clone());
        }
        for (k, v) in &other.running_uploads {
            self.running_uploads.insert(*k, v.clone());
        }
        for (k, v) in &other.completed_uploads {
            self.completed_uploads.insert(*k, v.clone());
        }
        if let Some(other_end) = other.latest_completed_build_end {
            self.latest_completed_build_end = Some(
                self.latest_completed_build_end
                    .map_or(other_end, |cur| cur.max(other_end)),
            );
        }
        if let Some(other_dl) = other.latest_completed_download_start {
            self.latest_completed_download_start = Some(
                self.latest_completed_download_start
                    .map_or(other_dl, |cur| cur.max(other_dl)),
            );
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct InputDerivation {
    pub derivation: DerivationId,
    pub outputs: BTreeSet<OutputName>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DerivationInfo {
    pub name: Derivation,
    pub outputs: FxHashMap<OutputName, StorePathId>,
    pub input_derivations: Vec<InputDerivation>,
    pub input_sources: BTreeSet<StorePathId>,
    pub build_status: BuildStatus,
    pub dependency_summary: DependencySummary,
    pub cached: bool,
    pub derivation_parents: BTreeSet<DerivationId>,
    pub pname: Option<String>,
    pub platform: Option<String>,
    pub is_root: bool,
    pub curl_progress: Option<crate::parser::curl::CurlProgress>,
}

impl DerivationInfo {
    pub fn new(name: Derivation) -> Self {
        Self {
            name,
            outputs: FxHashMap::default(),
            input_derivations: Vec::new(),
            input_sources: BTreeSet::new(),
            build_status: BuildStatus::Unknown,
            dependency_summary: DependencySummary::default(),
            cached: false,
            derivation_parents: BTreeSet::new(),
            pname: None,
            platform: None,
            is_root: false,
            curl_progress: None,
        }
    }

    pub fn get_report_name(&self) -> &str {
        if let Some(ref pname) = self.pname {
            pname.as_str()
        } else {
            let full_name = &self.name.store_path.name;
            let trimmed =
                full_name.trim_end_matches(|c: char| c == '.' || c == '-' || c.is_ascii_digit());
            if trimmed.is_empty() {
                full_name
            } else {
                trimmed
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StorePathInfo {
    pub name: StorePath,
    pub states: BTreeSet<StorePathState>,
    pub producer: Option<DerivationId>,
    pub input_for: BTreeSet<DerivationId>,
}

impl StorePathInfo {
    pub fn new(name: StorePath) -> Self {
        Self {
            name,
            states: BTreeSet::new(),
            producer: None,
            input_for: BTreeSet::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActivityStatus {
    pub activity: Activity,
    pub phase: Option<String>,
    pub progress: Option<ActivityProgress>,
    pub file_transfer_progress: Option<ActivityProgress>,
    pub curl_progress: Option<crate::parser::curl::CurlProgress>,
    pub prefix: CompactString,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvalInfo {
    pub last_file_name: Option<String>,
    pub count: usize,
    pub at: f64,
}

#[derive(Clone, Debug)]
pub struct NomState {
    pub derivation_infos: Vec<DerivationInfo>,
    pub store_path_infos: Vec<StorePathInfo>,
    pub full_summary: DependencySummary,
    pub forest_roots: Vec<DerivationId>,
    pub build_reports: BuildReportMap,
    pub start_time: f64,
    pub progress_state: ProgressState,
    pub store_path_ids: FxHashMap<StorePath, StorePathId>,
    pub derivation_ids: FxHashMap<Derivation, DerivationId>,
    pub touched_ids: RoaringBitmap,
    pub activities: FxHashMap<u64, ActivityStatus>,
    pub activity_parents: FxHashMap<u64, u64>,
    pub planned_download_bytes: Option<usize>,
    pub max_download_sizes: FxHashMap<StorePathId, usize>,
    pub nix_errors: Vec<String>,
    pub nix_traces: Vec<String>,
    pub build_platform: Option<String>,
    pub evaluation_state: EvalInfo,
    pub parsed_drv_cache: FxHashMap<Derivation, crate::parser::derivation::ParsedDerivation>,
    pub visited_epoch: Vec<u32>,
    pub current_epoch: u32,
    pub remote_hosts: BTreeSet<CompactString>,
}

impl NomState {
    pub fn new(start_time: f64, build_platform: Option<String>, reports: BuildReportMap) -> Self {
        Self {
            derivation_infos: Vec::new(),
            store_path_infos: Vec::new(),
            full_summary: DependencySummary::default(),
            forest_roots: Vec::new(),
            build_reports: reports,
            start_time,
            progress_state: ProgressState::JustStarted,
            store_path_ids: FxHashMap::default(),
            derivation_ids: FxHashMap::default(),
            touched_ids: RoaringBitmap::new(),
            activities: FxHashMap::default(),
            activity_parents: FxHashMap::default(),
            planned_download_bytes: None,
            max_download_sizes: FxHashMap::default(),
            nix_errors: Vec::new(),
            nix_traces: Vec::new(),
            build_platform,
            evaluation_state: EvalInfo::default(),
            parsed_drv_cache: FxHashMap::default(),
            visited_epoch: Vec::new(),
            current_epoch: 0,
            remote_hosts: BTreeSet::new(),
        }
    }

    pub fn register_host(&mut self, host: &Host) {
        if let Host::Remote { ref host, .. } = host {
            if !self.remote_hosts.contains(host) {
                self.remote_hosts.insert(host.clone());
            }
        }
    }

    pub fn get_store_path_id(&mut self, path: &StorePath) -> StorePathId {
        if let Some(&id) = self.store_path_ids.get(path) {
            id
        } else {
            let id = StorePathId(self.store_path_infos.len());
            self.store_path_infos.push(StorePathInfo::new(path.clone()));
            self.store_path_ids.insert(path.clone(), id);
            id
        }
    }

    pub fn get_derivation_id(&mut self, drv: &Derivation) -> DerivationId {
        if let Some(&id) = self.derivation_ids.get(drv) {
            id
        } else {
            let id = DerivationId(self.derivation_infos.len());
            self.derivation_infos.push(DerivationInfo::new(drv.clone()));
            self.derivation_ids.insert(drv.clone(), id);
            id
        }
    }

    pub fn get_derivation(&self, id: DerivationId) -> &DerivationInfo {
        &self.derivation_infos[id.0]
    }

    pub fn get_derivation_mut(&mut self, id: DerivationId) -> &mut DerivationInfo {
        &mut self.derivation_infos[id.0]
    }

    pub fn get_store_path(&self, id: StorePathId) -> &StorePathInfo {
        &self.store_path_infos[id.0]
    }

    pub fn get_store_path_mut(&mut self, id: StorePathId) -> &mut StorePathInfo {
        &mut self.store_path_infos[id.0]
    }

    pub fn derivation_to_any_out_path(&self, drv_id: DerivationId) -> Option<StorePath> {
        let drv = self.get_derivation(drv_id);
        if let Some(&path_id) = drv.outputs.get(&OutputName::Out) {
            Some(self.get_store_path(path_id).name.clone())
        } else if let Some(&path_id) = drv.outputs.values().next() {
            Some(self.get_store_path(path_id).name.clone())
        } else {
            None
        }
    }

    pub fn out_path_to_derivation(&self, path_id: StorePathId) -> Option<DerivationId> {
        self.get_store_path(path_id).producer
    }

    #[inline]
    pub fn is_summary_including_root_empty(&self, drv_id: DerivationId) -> bool {
        let drv = self.get_derivation(drv_id);
        drv.dependency_summary.is_empty() && matches!(drv.build_status, BuildStatus::Unknown)
    }

    pub fn set_input_received(&mut self) -> bool {
        if self.progress_state == ProgressState::JustStarted {
            self.progress_state = ProgressState::InputReceived;
            true
        } else {
            false
        }
    }

    pub fn update_summary_for_derivation(
        summary: &mut DependencySummary,
        old_status: &BuildStatus,
        new_status: &BuildStatus,
        drv_id: DerivationId,
    ) {
        Self::update_summary_for_derivation_opt(summary, old_status, new_status, drv_id, true);
    }

    pub fn update_summary_for_derivation_opt(
        summary: &mut DependencySummary,
        old_status: &BuildStatus,
        new_status: &BuildStatus,
        drv_id: DerivationId,
        is_full_summary: bool,
    ) {
        Self::clear_derivation_id_from_summary(summary, old_status, drv_id);
        match new_status {
            BuildStatus::Unknown => {}
            BuildStatus::Planned => {
                summary.planned_builds.insert(drv_id.0 as u32);
            }
            BuildStatus::Building(bi) => {
                summary.running_builds.insert(drv_id, bi.clone());
            }
            BuildStatus::Failed(bi) => {
                summary.failed_builds.insert(drv_id, bi.clone());
            }
            BuildStatus::Built(bi) => {
                summary.latest_completed_build_end = Some(
                    summary
                        .latest_completed_build_end
                        .map_or(bi.end, |cur| cur.max(bi.end)),
                );
                if is_full_summary {
                    summary.completed_builds.insert(drv_id, bi.clone());
                }
            }
        }
    }

    pub fn clear_derivation_id_from_summary(
        summary: &mut DependencySummary,
        old_status: &BuildStatus,
        drv_id: DerivationId,
    ) {
        match old_status {
            BuildStatus::Unknown => {}
            BuildStatus::Planned => {
                summary.planned_builds.remove(drv_id.0 as u32);
            }
            BuildStatus::Building(_) => {
                summary.running_builds.remove(&drv_id);
            }
            BuildStatus::Failed(_) => {
                summary.failed_builds.remove(&drv_id);
            }
            BuildStatus::Built(_) => {
                summary.completed_builds.remove(&drv_id);
                summary.latest_completed_build_end = summary
                    .completed_builds
                    .values()
                    .map(|b| b.end)
                    .max_by(|a, b| a.total_cmp(b));
            }
        }
    }

    pub fn update_summary_for_store_path(
        summary: &mut DependencySummary,
        old_states: &BTreeSet<StorePathState>,
        new_states: &BTreeSet<StorePathState>,
        path_id: StorePathId,
    ) {
        Self::update_summary_for_store_path_opt(summary, old_states, new_states, path_id, true);
    }

    pub fn update_summary_for_store_path_opt(
        summary: &mut DependencySummary,
        old_states: &BTreeSet<StorePathState>,
        new_states: &BTreeSet<StorePathState>,
        path_id: StorePathId,
        is_full_summary: bool,
    ) {
        let deleted = old_states.difference(new_states);
        for state in deleted {
            Self::remove_store_path_state_from_summary(summary, state, path_id);
        }
        let added = new_states.difference(old_states);
        for state in added {
            Self::insert_store_path_state_into_summary(summary, state, path_id, is_full_summary);
        }
    }

    pub fn clear_store_paths_from_summary(
        summary: &mut DependencySummary,
        states: &BTreeSet<StorePathState>,
        path_id: StorePathId,
    ) {
        for state in states {
            Self::remove_store_path_state_from_summary(summary, state, path_id);
        }
    }

    fn insert_store_path_state_into_summary(
        summary: &mut DependencySummary,
        state: &StorePathState,
        path_id: StorePathId,
        is_full_summary: bool,
    ) {
        match state {
            StorePathState::DownloadPlanned => {
                summary.planned_downloads.insert(path_id.0 as u32);
            }
            StorePathState::Downloading(info) => {
                summary.running_downloads.insert(path_id, info.clone());
            }
            StorePathState::Uploading(info) => {
                summary.running_uploads.insert(path_id, info.clone());
            }
            StorePathState::Downloaded(info) => {
                summary.latest_completed_download_start = Some(
                    summary
                        .latest_completed_download_start
                        .map_or(info.start, |cur| cur.max(info.start)),
                );
                if is_full_summary {
                    summary.completed_downloads.insert(path_id, info.clone());
                }
            }
            StorePathState::Uploaded(info) => {
                summary.completed_uploads.insert(path_id, info.clone());
            }
        }
    }

    fn remove_store_path_state_from_summary(
        summary: &mut DependencySummary,
        state: &StorePathState,
        path_id: StorePathId,
    ) {
        match state {
            StorePathState::DownloadPlanned => {
                summary.planned_downloads.remove(path_id.0 as u32);
            }
            StorePathState::Downloading(_) => {
                summary.running_downloads.remove(&path_id);
            }
            StorePathState::Uploading(_) => {
                summary.running_uploads.remove(&path_id);
            }
            StorePathState::Downloaded(_) => {
                summary.completed_downloads.remove(&path_id);
                summary.latest_completed_download_start = summary
                    .completed_downloads
                    .values()
                    .map(|d| d.start)
                    .max_by(|a, b| a.total_cmp(b));
            }
            StorePathState::Uploaded(_) => {
                summary.completed_uploads.remove(&path_id);
            }
        }
    }

    pub fn update_parents<FU, FC>(
        &mut self,
        force_direct: bool,
        mut update_func: FU,
        mut clear_func: FC,
        direct_parents: &[DerivationId],
    ) where
        FU: FnMut(&mut DependencySummary),
        FC: FnMut(&mut DependencySummary),
    {
        if direct_parents.is_empty() {
            return;
        }

        let num_drvs = self.derivation_infos.len();
        if self.visited_epoch.len() < num_drvs {
            self.visited_epoch.resize(num_drvs, 0);
        }

        if self.current_epoch >= u32::MAX - 10 {
            self.visited_epoch.fill(0);
            self.current_epoch = 1;
        } else {
            self.current_epoch += 1;
        }
        let rel_epoch = self.current_epoch;
        self.current_epoch += 1;
        let all_epoch = self.current_epoch;

        let mut rel_parents = Vec::new();
        self.collect_parents_fast_epoch(true, direct_parents, rel_epoch, &mut rel_parents);

        if force_direct {
            for &dp in direct_parents {
                if dp.0 < num_drvs && self.visited_epoch[dp.0] != rel_epoch {
                    self.visited_epoch[dp.0] = rel_epoch;
                    rel_parents.push(dp);
                }
            }
        }

        let mut all_parents = Vec::new();
        self.collect_parents_fast_epoch(false, direct_parents, all_epoch, &mut all_parents);

        for &parent in &rel_parents {
            update_func(&mut self.derivation_infos[parent.0].dependency_summary);
        }
        for &parent in &all_parents {
            if parent.0 < num_drvs && self.visited_epoch[parent.0] != rel_epoch {
                clear_func(&mut self.derivation_infos[parent.0].dependency_summary);
            }
        }
        for parent in all_parents {
            self.touched_ids.insert(parent.0 as u32);
        }
    }

    fn collect_parents_fast_epoch(
        &mut self,
        no_irrelevant: bool,
        parents_to_scan: &[DerivationId],
        epoch: u32,
        collected: &mut Vec<DerivationId>,
    ) {
        let mut queue: Vec<DerivationId> = parents_to_scan.to_vec();

        while let Some(current) = queue.pop() {
            if current.0 >= self.visited_epoch.len() || self.visited_epoch[current.0] == epoch {
                continue;
            }

            let drv = &self.derivation_infos[current.0];
            let all_transfers_completed = drv
                .outputs
                .get(&OutputName::Out)
                .map(|&out_id| {
                    let out_info = &self.store_path_infos[out_id.0];
                    out_info.states.iter().all(|s| {
                        matches!(
                            s,
                            StorePathState::Downloaded(_) | StorePathState::Uploaded(_)
                        )
                    })
                })
                .unwrap_or(true);

            let is_irrelevant = (all_transfers_completed
                && matches!(drv.build_status, BuildStatus::Unknown))
                || matches!(drv.build_status, BuildStatus::Built(_));

            if !(is_irrelevant && no_irrelevant) {
                self.visited_epoch[current.0] = epoch;
                collected.push(current);
                for &p in &drv.derivation_parents {
                    if p.0 < self.visited_epoch.len() && self.visited_epoch[p.0] != epoch {
                        queue.push(p);
                    }
                }
            }
        }
    }
}
