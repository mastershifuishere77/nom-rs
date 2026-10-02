use nix_output_monitor::parser::old_style::{parse_old_style_chunk, NixOldStyleMessage};
use nix_output_monitor::render::Config;
use nix_output_monitor::types::{Derivation, FailType, Host, StorePath};
use rustc_hash::FxHashMap;
use std::collections::BTreeSet;

#[test]
fn test_parse_plan() {
    let input = "these derivations will be built:\n  /nix/store/7n05q79qhrgvnfmvv2v3cnj3yqf4d1hf-haskell-language-server-0.4.0.0.drv\nthese paths will be fetched (134.19 MiB download, 1863.82 MiB unpacked):\n  /nix/store/60zb5dndaw1fzir3s69sy3xhy19gll1p-ghc-8.8.2\ngarbage";

    let (msg1, consumed1) = parse_old_style_chunk(input).expect("parsing plan builds succeeds");
    let drv = Derivation::parse(
        "/nix/store/7n05q79qhrgvnfmvv2v3cnj3yqf4d1hf-haskell-language-server-0.4.0.0.drv",
    )
    .unwrap();
    let mut expected_drvs = BTreeSet::new();
    expected_drvs.insert(drv.clone());

    assert_eq!(msg1, NixOldStyleMessage::PlanBuilds(expected_drvs, drv));

    let rest = &input[consumed1..];
    let (msg2, consumed2) = parse_old_style_chunk(rest).expect("parsing plan downloads succeeds");

    let sp = StorePath::parse("/nix/store/60zb5dndaw1fzir3s69sy3xhy19gll1p-ghc-8.8.2").unwrap();
    let mut expected_paths = BTreeSet::new();
    expected_paths.insert(sp);

    assert_eq!(
        msg2,
        NixOldStyleMessage::PlanDownloads(
            134.19 * 1024.0 * 1024.0,
            1863.82 * 1024.0 * 1024.0,
            expected_paths
        )
    );

    let rest2 = &rest[consumed2..];
    assert_eq!(rest2.trim(), "garbage");
}

#[test]
fn test_parse_downloading() {
    let input = "copying path '/nix/store/yk1164s4bkj6p3s4mzxm5fc4qn38cnmf-ghc-8.8.2-doc' from 'https://cache.nixos.org'...\n";
    let (msg, _) = parse_old_style_chunk(input).expect("parsing downloading succeeds");

    let sp = StorePath::parse("/nix/store/yk1164s4bkj6p3s4mzxm5fc4qn38cnmf-ghc-8.8.2-doc").unwrap();
    let host = Host::parse("https://cache.nixos.org");

    assert_eq!(msg, NixOldStyleMessage::Downloading(sp, host));
}

#[test]
fn test_parse_local_building() {
    let input = "building '/nix/store/dpqlnrbvzhjxp06d1mc3ksf2w8m2ldms-aeson-1.5.2.0.drv'...\n";
    let (msg, _) = parse_old_style_chunk(input).expect("parsing local building succeeds");

    let drv =
        Derivation::parse("/nix/store/dpqlnrbvzhjxp06d1mc3ksf2w8m2ldms-aeson-1.5.2.0.drv").unwrap();
    assert_eq!(msg, NixOldStyleMessage::Build(drv, Host::Localhost));
}

#[test]
fn test_parse_remote_building() {
    let input = "building '/nix/store/63jjdifv1x1nymjxdwla603xy1sggakk-hoogle-local-0.1.drv' on 'ssh://maralorn@example.com'...\n";
    let (msg, _) = parse_old_style_chunk(input).expect("parsing remote building succeeds");

    let drv = Derivation::parse("/nix/store/63jjdifv1x1nymjxdwla603xy1sggakk-hoogle-local-0.1.drv")
        .unwrap();
    let host = Host::parse("ssh://maralorn@example.com");
    assert_eq!(msg, NixOldStyleMessage::Build(drv, host));
}

#[test]
fn test_parse_failed_build() {
    let input = "builder for '/nix/store/fbpdwqrfwr18nn504kb5jqx7s06l1mar-regex-base-0.94.0.1.drv' failed with exit code 1\n";
    let (msg, _) = parse_old_style_chunk(input).expect("parsing failed build succeeds");

    let drv =
        Derivation::parse("/nix/store/fbpdwqrfwr18nn504kb5jqx7s06l1mar-regex-base-0.94.0.1.drv")
            .unwrap();
    assert_eq!(msg, NixOldStyleMessage::Failed(drv, FailType::ExitCode(1)));
}

#[test]
fn test_parse_failed_build_nix24() {
    let input = "error: builder for '/nix/store/dylih0mw8yisn6nrjc3qlf51knmdkrq1-local-build-3.drv' failed with exit code 1;\n";
    let (msg, _) = parse_old_style_chunk(input).expect("parsing failed build nix 2.4 succeeds");

    let drv =
        Derivation::parse("/nix/store/dylih0mw8yisn6nrjc3qlf51knmdkrq1-local-build-3.drv").unwrap();
    assert_eq!(msg, NixOldStyleMessage::Failed(drv, FailType::ExitCode(1)));
}

#[test]
fn test_parse_failed_build_nix229() {
    let input = "error: Cannot build '/nix/store/d055cqki6z1vll144kvj496cknwvwi44-build-fail.drv'.\n       Reason: builder failed with exit code 1.\n       Output paths:\n          /nix/store/pvf324ikpfb9nhyszmc4zz5g9y8by0f6-build-fail\n";
    let (msg, _) = parse_old_style_chunk(input).expect("parsing failed build nix 2.29 succeeds");

    let drv =
        Derivation::parse("/nix/store/d055cqki6z1vll144kvj496cknwvwi44-build-fail.drv").unwrap();
    assert_eq!(msg, NixOldStyleMessage::Failed(drv, FailType::ExitCode(1)));
}


#[test]
fn test_store_path_and_host_parsing() {
    let sp = StorePath::parse("/nix/store/7n05q79qhrgvnfmvv2v3cnj3yqf4d1hf-test-name").unwrap();
    assert_eq!(sp.hash, "7n05q79qhrgvnfmvv2v3cnj3yqf4d1hf");
    assert_eq!(sp.name, "test-name");

    let drv =
        Derivation::parse("/nix/store/7n05q79qhrgvnfmvv2v3cnj3yqf4d1hf-test-name.drv").unwrap();
    assert_eq!(drv.store_path.name, "test-name");

    let host1 = Host::parse("local");
    assert_eq!(host1, Host::Localhost);

    let host2 = Host::parse("ssh://user@builder.example.com");
    match host2 {
        Host::Remote { proto, user, host } => {
            assert_eq!(proto.as_deref(), Some("ssh"));
            assert_eq!(user.as_deref(), Some("user"));
            assert_eq!(host, "builder.example.com");
        }
        _ => panic!("Expected remote host"),
    }
}

#[test]
fn test_truncate_display_ansi_safety() {
    use nix_output_monitor::render::table::{truncate_display, RESET, YELLOW};

    let input = format!(
        "{}This is a very long string that will be truncated",
        YELLOW
    );
    let truncated = truncate_display(&input, 15);
    assert!(
        truncated.ends_with(RESET),
        "Truncated string must end with RESET"
    );
}

#[test]
fn test_non_zero_entry_colors() {
    use nix_output_monitor::render::non_zero_entry;
    use nix_output_monitor::render::table::YELLOW;

    let entry_zero = non_zero_entry("⏵", 0, |e| e.yellow());
    assert!(
        entry_zero.codes.contains(&YELLOW),
        "Symbol must retain color even with 0 count"
    );

    let entry_nonzero = non_zero_entry("⏵", 5, |e| e.yellow());
    assert!(
        entry_nonzero.codes.contains(&YELLOW),
        "Symbol must retain color with non-zero count"
    );
}

#[test]
fn test_tree_selection_with_many_dependencies() {
    use nix_output_monitor::render::select_derivations_to_show;
    use nix_output_monitor::state::{BuildStatus, NomState};
    use nix_output_monitor::types::{Derivation, Host, StorePath};

    let mut state = NomState::new(0.0, None, FxHashMap::default());
    let root_drv = Derivation {
        store_path: StorePath::new("00000000000000000000000000000000", "root-package"),
    };
    let root_id = state.get_derivation_id(&root_drv);
    state.forest_roots.push(root_id);

    let mut last_dep_id = None;
    for i in 0..30 {
        let dep_drv = Derivation {
            store_path: StorePath::new(format!("{:032}", i), format!("dep-{}", i)),
        };
        let dep_id = state.get_derivation_id(&dep_drv);
        state
            .get_derivation_mut(dep_id)
            .derivation_parents
            .insert(root_id);
        state.get_derivation_mut(root_id).input_derivations.push(
            nix_output_monitor::state::InputDerivation {
                derivation: dep_id,
                outputs: std::collections::BTreeSet::new(),
            },
        );
        last_dep_id = Some(dep_id);
    }

    let active_dep = last_dep_id.unwrap();
    state.get_derivation_mut(active_dep).build_status =
        BuildStatus::Building(nix_output_monitor::state::BuildInfo {
            start: 0.0,
            host: Host::Localhost,
            estimate: None,
            activity_id: None,
            end: (),
        });

    let dep_status = state.get_derivation(active_dep).build_status.clone();
    NomState::update_summary_for_derivation(
        &mut state.get_derivation_mut(root_id).dependency_summary,
        &BuildStatus::Unknown,
        &dep_status,
        active_dep,
    );

    let selected = select_derivations_to_show(&state, 5);
    assert!(
        selected.contains(&root_id),
        "Root must be retained in the tree"
    );
    assert!(
        selected.contains(&active_dep),
        "Active building dependency must be retained"
    );
}

#[test]
fn test_tree_selection_preserves_ancestor_chain() {
    use nix_output_monitor::render::select_derivations_to_show;
    use nix_output_monitor::state::{BuildInfo, BuildStatus, InputDerivation, NomState};
    use nix_output_monitor::types::{Derivation, Host, StorePath};
    use rustc_hash::FxHashMap;
    use std::collections::BTreeSet;

    let mut state = NomState::new(0.0, None, FxHashMap::default());

    let names = [
        "root-app",
        "layer-1",
        "layer-2",
        "layer-3",
        "layer-4",
        "deep-leaf",
    ];
    let mut ids = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let drv = Derivation {
            store_path: StorePath::new(format!("{:032x}", i + 1), *name),
        };
        ids.push(state.get_derivation_id(&drv));
    }

    state.forest_roots.push(ids[0]);

    for i in 0..ids.len() - 1 {
        let parent = ids[i];
        let child = ids[i + 1];
        state
            .get_derivation_mut(child)
            .derivation_parents
            .insert(parent);
        state
            .get_derivation_mut(parent)
            .input_derivations
            .push(InputDerivation {
                derivation: child,
                outputs: BTreeSet::new(),
            });
    }

    let leaf_id = *ids.last().unwrap();
    state.get_derivation_mut(leaf_id).build_status = BuildStatus::Building(BuildInfo {
        start: 1.0,
        host: Host::Localhost,
        estimate: None,
        activity_id: None,
        end: (),
    });

    let leaf_status = state.get_derivation(leaf_id).build_status.clone();
    for &id in &ids[..ids.len() - 1] {
        NomState::update_summary_for_derivation(
            &mut state.get_derivation_mut(id).dependency_summary,
            &BuildStatus::Unknown,
            &leaf_status,
            leaf_id,
        );
    }

    let selected = select_derivations_to_show(&state, 2);
    for &id in &ids {
        assert!(
            selected.contains(&id),
            "Derivation {:?} in chain must be preserved so tree remains connected",
            state.get_derivation(id).name.store_path.name
        );
    }
}

#[test]
fn test_large_dependency_tree_render_end_to_end() {
    use nix_output_monitor::render::{render_state_to_text, Config};
    use nix_output_monitor::state::{BuildInfo, BuildStatus, InputDerivation, NomState};
    use nix_output_monitor::types::{Derivation, Host, StorePath};
    use rustc_hash::FxHashMap;
    use std::collections::BTreeSet;

    let mut state = NomState::new(0.0, None, FxHashMap::default());
    let root_drv = Derivation {
        store_path: StorePath::new("10000000000000000000000000000000", "massive-project"),
    };
    let root_id = state.get_derivation_id(&root_drv);
    state.forest_roots.push(root_id);

    let mut leaf_ids = Vec::new();
    for i in 0..100 {
        let dep_drv = Derivation {
            store_path: StorePath::new(format!("{:032x}", i + 100), format!("dep-pkg-{:03}", i)),
        };
        let dep_id = state.get_derivation_id(&dep_drv);
        state
            .get_derivation_mut(dep_id)
            .derivation_parents
            .insert(root_id);
        state
            .get_derivation_mut(root_id)
            .input_derivations
            .push(InputDerivation {
                derivation: dep_id,
                outputs: BTreeSet::new(),
            });
        leaf_ids.push(dep_id);
    }

    let active1 = leaf_ids[10];
    let active2 = leaf_ids[80];
    for &dep_id in &[active1, active2] {
        state.get_derivation_mut(dep_id).build_status = BuildStatus::Building(BuildInfo {
            start: 5.0,
            host: Host::Localhost,
            estimate: None,
            activity_id: None,
            end: (),
        });
        let status = state.get_derivation(dep_id).build_status.clone();
        NomState::update_summary_for_derivation(
            &mut state.get_derivation_mut(root_id).dependency_summary,
            &BuildStatus::Unknown,
            &status,
            dep_id,
        );
        NomState::update_summary_for_derivation(
            &mut state.full_summary,
            &BuildStatus::Unknown,
            &status,
            dep_id,
        );
    }

    let rendered = render_state_to_text(&state, Config::default(), 10.0);
    assert!(
        rendered.contains("Dependency Graph"),
        "Must contain Dependency Graph header"
    );
    assert!(
        rendered.contains("massive-project"),
        "Must contain root derivation"
    );
    assert!(
        rendered.contains("dep-pkg-010"),
        "Must contain active building dependency 10"
    );
    assert!(
        rendered.contains("dep-pkg-080"),
        "Must contain active building dependency 80"
    );
    assert!(rendered.contains("Builds"), "Must contain summary table");
}

#[test]
fn test_stream_with_many_planned_derivations() {
    use nix_output_monitor::engine::monitor_stream;
    use nix_output_monitor::render::{render_state_to_text, Config};
    use std::io::Cursor;

    let mut log = String::new();
    log.push_str("these 60 derivations will be built:\n");
    for i in 0..60 {
        log.push_str(&format!(
            "  /nix/store/{:032x}-package-{:03}.drv\n",
            i + 1,
            i
        ));
    }
    log.push_str("building '/nix/store/00000000000000000000000000000014-package-019.drv'...\n");

    let cursor = Cursor::new(log.into_bytes());
    let config = Config {
        silent: false,
        piping: false,
        ..Default::default()
    };
    let state = monitor_stream(cursor, false, config);

    assert_eq!(
        state.full_summary.running_builds.len(),
        1,
        "Expected 1 running build"
    );
    assert_eq!(
        state.full_summary.planned_builds.len(),
        59,
        "Expected 59 planned builds remaining"
    );

    let rendered = render_state_to_text(&state, config, 1.0);
    assert!(
        rendered.contains("Dependency Graph"),
        "Must render tree for planned/running stream"
    );
    assert!(
        rendered.contains("package-019"),
        "Must show running package"
    );
}

#[test]
fn test_summary_table_colors_always_present_when_zero() {
    use nix_output_monitor::render::table::{GREEN, RESET, YELLOW};
    use nix_output_monitor::render::{render_state_to_text, Config};
    use nix_output_monitor::state::{BuildInfo, BuildStatus, NomState};
    use nix_output_monitor::types::{Derivation, Host, StorePath};
    use rustc_hash::FxHashMap;

    let mut state = NomState::new(0.0, None, FxHashMap::default());
    let drv = Derivation {
        store_path: StorePath::new("12345678901234567890123456789012", "test-drv"),
    };
    let drv_id = state.get_derivation_id(&drv);
    let status = BuildStatus::Built(BuildInfo {
        start: 0.0,
        host: Host::Localhost,
        estimate: None,
        activity_id: None,
        end: 1.0,
    });
    NomState::update_summary_for_derivation(
        &mut state.full_summary,
        &BuildStatus::Unknown,
        &status,
        drv_id,
    );

    let rendered = render_state_to_text(&state, Config::default(), 2.0);

    assert!(
        rendered.contains(YELLOW),
        "Running icon must be colored YELLOW even when count is 0"
    );
    assert!(
        rendered.contains(GREEN),
        "Completed icon must be colored GREEN"
    );
    assert!(
        rendered.contains(RESET),
        "Must contain RESET to prevent color leakage"
    );
}

#[test]
fn test_print_bytes_spacing() {
    use nix_output_monitor::render::progress::print_bytes;

    assert_eq!(print_bytes(500), "500 B");
    assert_eq!(print_bytes(1024), "1.0 KiB");
    assert_eq!(print_bytes(21 * 1024 * 1024 + 1024 * 100), "21.1 MiB");
    assert_eq!(print_bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
}

#[test]
fn test_downloads_table_four_columns_and_arrow_symbols() {
    use nix_output_monitor::parser::json::{Activity, ActivityProgress};
    use nix_output_monitor::render::table::{GREEN, YELLOW};
    use nix_output_monitor::render::{render_state_to_text, Config, DOWN};
    use nix_output_monitor::state::{ActivityStatus, CompletedEnd, NomState, TransferInfo};
    use nix_output_monitor::types::{Host, StorePath};
    use rustc_hash::FxHashMap;

    let mut state = NomState::new(0.0, None, FxHashMap::default());

    // Register an activity with progress: 71.4 MiB / 3.0 GiB
    let act_id = 42;
    state.activities.insert(
        act_id,
        ActivityStatus {
            activity: Activity::CopyPath {
                path: StorePath::new("hash1234567890123456789012345678", "pkg1"),
                from: Host::parse("https://cache.nixos.org"),
                to: Host::Localhost,
            },
            phase: None,
            progress: Some(ActivityProgress {
                done: 74868326,       // ~71.4 MiB
                expected: 3221225472, // 3.0 GiB
                running: 1,
                failed: 0,
            }),
            file_transfer_progress: None,
            curl_progress: None,
            prefix: "".into(),
        },
    );

    let sp_id = state.get_store_path_id(&StorePath::new("hash1234567890123456789012345678", "pkg1"));

    state.full_summary.running_downloads.insert(
        sp_id,
        TransferInfo {
            host: Host::parse("https://cache.nixos.org"),
            start: 0.0,
            activity_id: Some(act_id),
            end: (),
        },
    );

    // Also add a completed download
    let sp_done_id = state.get_store_path_id(&StorePath::new("done1234567890123456789012345678", "pkg-done"));
    state.full_summary.completed_downloads.insert(
        sp_done_id,
        TransferInfo {
            host: Host::parse("https://cache.nixos.org"),
            start: 0.0,
            activity_id: None,
            end: CompletedEnd(Some(1.0)),
        },
    );

    // Also add a build on Localhost so show_hosts is true (multiple hosts)
    let drv_id = state.get_derivation_id(&nix_output_monitor::types::Derivation {
        store_path: StorePath::new("local123456789012345678901234567", "local-build"),
    });
    state.full_summary.running_builds.insert(
        drv_id,
        nix_output_monitor::state::BuildInfo {
            start: 0.0,
            host: Host::Localhost,
            estimate: None,
            activity_id: None,
            end: (),
        },
    );

    let rendered = render_state_to_text(&state, Config::default(), 2.0);

    assert!(
        rendered.contains("Downloads"),
        "Must contain Downloads section"
    );
    assert!(
        rendered.contains("71.4 MiB/3.0 GiB"),
        "Must render 4th column with transferred size progress: {}",
        rendered
    );
    assert!(
        rendered.contains(&format!("{}{}", YELLOW, DOWN)),
        "Running downloads must use DOWN arrow symbol with YELLOW"
    );
    assert!(
        rendered.contains(&format!("{}{}", GREEN, DOWN)),
        "Completed downloads must use DOWN arrow symbol with GREEN (not checkmark ✔)"
    );
    assert!(
        rendered.contains("cache.nixos.org (https)"),
        "Host must include protocol context (https)"
    );
}

#[test]
fn test_inactive_deep_subtrees_pruning() {
    use nix_output_monitor::render::select_derivations_to_show;
    use nix_output_monitor::state::{BuildInfo, BuildStatus, InputDerivation, NomState};
    use rustc_hash::FxHashMap;

    let mut state = NomState::new(0.0, None, FxHashMap::default());

    // Create root
    let root = Derivation {
        store_path: StorePath::new("root0000000000000000000000000000", "system-root"),
    };
    let root_id = state.get_derivation_id(&root);
    state.forest_roots.push(root_id);

    // Active branch: root -> active_parent -> active_leaf
    let active_parent = Derivation {
        store_path: StorePath::new("actp0000000000000000000000000000", "active-parent"),
    };
    let active_parent_id = state.get_derivation_id(&active_parent);

    let active_leaf = Derivation {
        store_path: StorePath::new("actl0000000000000000000000000000", "active-leaf"),
    };
    let active_leaf_id = state.get_derivation_id(&active_leaf);

    // Link active branch
    state
        .get_derivation_mut(root_id)
        .input_derivations
        .push(InputDerivation {
            derivation: active_parent_id,
            outputs: std::collections::BTreeSet::new(),
        });
    state
        .get_derivation_mut(active_parent_id)
        .derivation_parents
        .insert(root_id);
    state
        .get_derivation_mut(active_parent_id)
        .input_derivations
        .push(InputDerivation {
            derivation: active_leaf_id,
            outputs: std::collections::BTreeSet::new(),
        });
    state
        .get_derivation_mut(active_leaf_id)
        .derivation_parents
        .insert(active_parent_id);

    // Make active_leaf building and update summaries of its parents
    let building_status = BuildStatus::Building(BuildInfo {
        start: 0.0,
        host: Host::Localhost,
        estimate: None,
        activity_id: None,
        end: (),
    });
    state.get_derivation_mut(active_leaf_id).build_status = building_status.clone();
    NomState::update_summary_for_derivation(
        &mut state
            .get_derivation_mut(active_parent_id)
            .dependency_summary,
        &BuildStatus::Unknown,
        &building_status,
        active_leaf_id,
    );
    NomState::update_summary_for_derivation(
        &mut state.get_derivation_mut(root_id).dependency_summary,
        &BuildStatus::Unknown,
        &building_status,
        active_leaf_id,
    );

    // Deep inactive branch: root -> inact1 -> inact2 -> inact3 -> inact4 -> inact5
    let mut prev_id = root_id;
    let mut deep_inactive_ids = Vec::new();
    for i in 1..=5 {
        let drv = Derivation {
            store_path: StorePath::new(format!("inact{:028}", i), format!("inactive-depth-{}", i)),
        };
        let id = state.get_derivation_id(&drv);
        state.get_derivation_mut(id).build_status = BuildStatus::Planned;
        state
            .get_derivation_mut(prev_id)
            .input_derivations
            .push(InputDerivation {
                derivation: id,
                outputs: std::collections::BTreeSet::new(),
            });
        state
            .get_derivation_mut(id)
            .derivation_parents
            .insert(prev_id);
        deep_inactive_ids.push(id);
        prev_id = id;
    }

    let selected = select_derivations_to_show(&state, 4);

    // Must contain active path
    assert!(selected.contains(&root_id), "Must contain root");
    assert!(
        selected.contains(&active_parent_id),
        "Must contain active_parent"
    );
    assert!(
        selected.contains(&active_leaf_id),
        "Must contain active_leaf"
    );

    // Deep inactive children (depth 3, 4, 5) must NOT be selected!
    assert!(
        !selected.contains(&deep_inactive_ids[2]),
        "Must NOT contain deep inactive node at depth 3"
    );
    assert!(
        !selected.contains(&deep_inactive_ids[3]),
        "Must NOT contain deep inactive node at depth 4"
    );
    assert!(
        !selected.contains(&deep_inactive_ids[4]),
        "Must NOT contain deep inactive node at depth 5"
    );
}

#[test]
fn test_inert_unknown_derivations_pruned() {
    use nix_output_monitor::render::{render_state_to_text, select_derivations_to_show, Config};
    use nix_output_monitor::state::{BuildStatus, CompletedBuildInfo, InputDerivation, NomState};
    use nix_output_monitor::types::{Derivation, Host, StorePath};
    use rustc_hash::FxHashMap;

    let mut state = NomState::new(0.0, None, FxHashMap::default());

    // Root: nixos-system
    let root = Derivation {
        store_path: StorePath::new("sys0000000000000000000000000000", "nixos-system"),
    };
    let root_id = state.get_derivation_id(&root);
    state.forest_roots.push(root_id);

    // Completed build 1: etc
    let etc = Derivation {
        store_path: StorePath::new("etc0000000000000000000000000000", "etc"),
    };
    let etc_id = state.get_derivation_id(&etc);
    state.get_derivation_mut(etc_id).build_status = BuildStatus::Built(CompletedBuildInfo {
        start: 0.0,
        host: Host::Localhost,
        estimate: None,
        activity_id: None,
        end: 10.0,
    });
    NomState::update_summary_for_derivation(
        &mut state.get_derivation_mut(root_id).dependency_summary,
        &BuildStatus::Unknown,
        &BuildStatus::Built(CompletedBuildInfo {
            start: 0.0,
            host: Host::Localhost,
            estimate: None,
            activity_id: None,
            end: 10.0,
        }),
        etc_id,
    );

    // Completed build 2: activate
    let act = Derivation {
        store_path: StorePath::new("act0000000000000000000000000000", "activate"),
    };
    let act_id = state.get_derivation_id(&act);
    state.get_derivation_mut(act_id).build_status = BuildStatus::Built(CompletedBuildInfo {
        start: 0.0,
        host: Host::Localhost,
        estimate: None,
        activity_id: None,
        end: 10.0,
    });
    NomState::update_summary_for_derivation(
        &mut state.get_derivation_mut(root_id).dependency_summary,
        &BuildStatus::Unknown,
        &BuildStatus::Built(CompletedBuildInfo {
            start: 0.0,
            host: Host::Localhost,
            estimate: None,
            activity_id: None,
            end: 10.0,
        }),
        act_id,
    );

    // Root also built
    state.get_derivation_mut(root_id).build_status = BuildStatus::Built(CompletedBuildInfo {
        start: 0.0,
        host: Host::Localhost,
        estimate: None,
        activity_id: None,
        end: 10.0,
    });

    // Inert direct child of root: stage-2-init.sh (Unknown, no activity)
    let stage2 = Derivation {
        store_path: StorePath::new("stg0000000000000000000000000000", "stage-2-init.sh"),
    };
    let stage2_id = state.get_derivation_id(&stage2);

    // Inert child of root: pre-switch-checks
    let preswitch = Derivation {
        store_path: StorePath::new("pre0000000000000000000000000000", "pre-switch-checks"),
    };
    let preswitch_id = state.get_derivation_id(&preswitch);

    // Inert child of etc: system-path
    let syspath = Derivation {
        store_path: StorePath::new("path000000000000000000000000000", "system-path"),
    };
    let syspath_id = state.get_derivation_id(&syspath);

    // Inert child of system-path: gnused
    let gnused = Derivation {
        store_path: StorePath::new("sed0000000000000000000000000000", "gnused-4.10"),
    };
    let gnused_id = state.get_derivation_id(&gnused);

    // Wire up input derivations and parents
    state.get_derivation_mut(root_id).input_derivations = vec![
        InputDerivation {
            derivation: stage2_id,
            outputs: std::collections::BTreeSet::new(),
        },
        InputDerivation {
            derivation: preswitch_id,
            outputs: std::collections::BTreeSet::new(),
        },
        InputDerivation {
            derivation: etc_id,
            outputs: std::collections::BTreeSet::new(),
        },
        InputDerivation {
            derivation: act_id,
            outputs: std::collections::BTreeSet::new(),
        },
    ];
    state
        .get_derivation_mut(stage2_id)
        .derivation_parents
        .insert(root_id);
    state
        .get_derivation_mut(preswitch_id)
        .derivation_parents
        .insert(root_id);
    state
        .get_derivation_mut(etc_id)
        .derivation_parents
        .insert(root_id);
    state
        .get_derivation_mut(act_id)
        .derivation_parents
        .insert(root_id);

    state.get_derivation_mut(etc_id).input_derivations = vec![InputDerivation {
        derivation: syspath_id,
        outputs: std::collections::BTreeSet::new(),
    }];
    state
        .get_derivation_mut(syspath_id)
        .derivation_parents
        .insert(etc_id);

    state.get_derivation_mut(syspath_id).input_derivations = vec![InputDerivation {
        derivation: gnused_id,
        outputs: std::collections::BTreeSet::new(),
    }];
    state
        .get_derivation_mut(gnused_id)
        .derivation_parents
        .insert(syspath_id);

    // Test select_derivations_to_show
    let selected = select_derivations_to_show(&state, 20);
    assert!(selected.contains(&root_id), "Must contain root");
    assert!(selected.contains(&etc_id), "Must contain built etc");
    assert!(selected.contains(&act_id), "Must contain built activate");
    assert!(
        !selected.contains(&stage2_id),
        "Must NOT contain inert stage-2-init.sh"
    );
    assert!(
        !selected.contains(&preswitch_id),
        "Must NOT contain inert pre-switch-checks"
    );
    assert!(
        !selected.contains(&syspath_id),
        "Must NOT contain inert system-path"
    );
    assert!(
        !selected.contains(&gnused_id),
        "Must NOT contain inert gnused-4.10"
    );

    // Test rendered tree
    let config = Config {
        silent: false,
        piping: false,
        ..Default::default()
    };
    let rendered = render_state_to_text(&state, config, 15.0);
    assert!(rendered.contains("etc"), "Rendered tree must contain etc");
    assert!(
        rendered.contains("activate"),
        "Rendered tree must contain activate"
    );
    assert!(
        rendered.contains("nixos-system"),
        "Rendered tree must contain nixos-system"
    );
    assert!(
        !rendered.contains("stage-2-init.sh"),
        "Rendered tree must NOT contain stage-2-init.sh"
    );
    assert!(
        !rendered.contains("pre-switch-checks"),
        "Rendered tree must NOT contain pre-switch-checks"
    );
    assert!(
        !rendered.contains("system-path"),
        "Rendered tree must NOT contain system-path"
    );
    assert!(
        !rendered.contains("gnused"),
        "Rendered tree must NOT contain gnused"
    );
}

#[test]
fn test_localhost_presence_and_order_before_builds() {
    use nix_output_monitor::render::{render_state_to_text, Config};
    use nix_output_monitor::state::{NomState, TransferInfo};
    use nix_output_monitor::types::{Derivation, Host, StorePath};
    use rustc_hash::FxHashMap;

    let mut state = NomState::new(0.0, None, FxHashMap::default());

    // 1 planned build on localhost, but NOT yet building
    let drv = Derivation::parse("/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-foo-1.0.drv").unwrap();
    let drv_id = state.get_derivation_id(&drv);
    state.get_derivation_mut(drv_id).build_status = nix_output_monitor::state::BuildStatus::Planned;
    state.full_summary.planned_builds.insert(drv_id.0 as u32);

    // 1 running download from cache.nixos.org
    let sp = StorePath::parse("/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-dep-1.0").unwrap();
    let sp_id = state.get_store_path_id(&sp);
    state.full_summary.running_downloads.insert(
        sp_id,
        TransferInfo {
            host: Host::parse("https://cache.nixos.org"),
            start: 0.0,
            activity_id: None,
            end: (),
        },
    );

    let config = Config {
        silent: false,
        piping: false,
        ..Default::default()
    };
    let rendered = render_state_to_text(&state, config, 1.0);

    // Both localhost and cache.nixos.org must be present
    assert!(
        rendered.contains("localhost"),
        "Table must display localhost even before builds start"
    );
    assert!(
        rendered.contains("cache.nixos.org"),
        "Table must display cache.nixos.org"
    );

    // localhost must appear BEFORE cache.nixos.org in the host table
    let pos_localhost = rendered.find("localhost").unwrap();
    let pos_cache = rendered.rfind("cache.nixos.org").unwrap();
    assert!(
        pos_localhost < pos_cache,
        "localhost (pos {}) must appear before cache.nixos.org (pos {}) in table",
        pos_localhost,
        pos_cache
    );
}

#[test]
fn test_compressed_download_size_preferred_over_unpacked() {
    use nix_output_monitor::parser::json::{Activity, ActivityProgress};
    use nix_output_monitor::render::{render_state_to_text, Config};
    use nix_output_monitor::state::{ActivityStatus, NomState, TransferInfo};
    use nix_output_monitor::types::{Host, StorePath};
    use rustc_hash::FxHashMap;

    let mut state = NomState::new(0.0, None, FxHashMap::default());
    let act_id = 100;
    let sp = StorePath::parse("/nix/store/cccccccccccccccccccccccccccccccc-source").unwrap();
    let sp_id = state.get_store_path_id(&sp);

    // CopyPath progress indicates unpacked NAR size: 200 MiB (209715200 bytes)
    // Child FileTransfer progress indicates compressed download size: 48 MiB (50331648 bytes)
    state.activities.insert(
        act_id,
        ActivityStatus {
            activity: Activity::CopyPath {
                path: sp.clone(),
                from: Host::parse("https://cache.nixos.org"),
                to: Host::Localhost,
            },
            phase: None,
            progress: Some(ActivityProgress {
                done: 209715200,
                expected: 209715200,
                running: 1,
                failed: 0,
            }),
            file_transfer_progress: Some(ActivityProgress {
                done: 50331648,
                expected: 50331648,
                running: 1,
                failed: 0,
            }),
            curl_progress: None,
            prefix: "".into(),
        },
    );

    state.full_summary.running_downloads.insert(
        sp_id,
        TransferInfo {
            host: Host::parse("https://cache.nixos.org"),
            start: 0.0,
            activity_id: Some(act_id),
            end: (),
        },
    );

    let config = Config {
        silent: false,
        piping: false,
        ..Default::default()
    };
    let rendered = render_state_to_text(&state, config, 1.0);

    // It should display 48.0 MiB, NOT 200.0 MiB
    assert!(
        rendered.contains("48.0 MiB"),
        "Rendered output must show compressed download size (48.0 MiB), got: \n{}",
        rendered
    );
    assert!(
        !rendered.contains("200.0 MiB"),
        "Rendered output must NOT show unpacked NAR size (200.0 MiB)"
    );
}

#[test]
fn test_single_substituter_does_not_display_from_abbrev() {
    use nix_output_monitor::render::{render_state_to_text, Config};
    use nix_output_monitor::state::{BuildInfo, NomState, TransferInfo};
    use nix_output_monitor::types::{Derivation, Host, OutputName, StorePath};
    use rustc_hash::FxHashMap;

    let mut state = NomState::new(0.0, None, FxHashMap::default());

    // 1 local build on localhost
    let drv_build =
        Derivation::parse("/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-foo-1.0.drv").unwrap();
    let drv_build_id = state.get_derivation_id(&drv_build);
    state.full_summary.running_builds.insert(
        drv_build_id,
        BuildInfo {
            start: 0.0,
            host: Host::Localhost,
            estimate: None,
            activity_id: None,
            end: (),
        },
    );
    state.forest_roots.push(drv_build_id);

    // 1 download from the only substituter: cache.nixos.org
    let sp = StorePath::parse("/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-dep-1.0").unwrap();
    let sp_id = state.get_store_path_id(&sp);
    let dl_info = TransferInfo {
        host: Host::parse("https://cache.nixos.org"),
        start: 0.0,
        activity_id: None,
        end: (),
    };
    state
        .full_summary
        .running_downloads
        .insert(sp_id, dl_info.clone());

    let drv_dl =
        Derivation::parse("/nix/store/11111111111111111111111111111111-dep-1.0.drv").unwrap();
    let drv_dl_id = state.get_derivation_id(&drv_dl);
    state
        .get_derivation_mut(drv_dl_id)
        .outputs
        .insert(OutputName::Out, sp_id);
    state
        .get_derivation_mut(drv_dl_id)
        .dependency_summary
        .running_downloads
        .insert(sp_id, dl_info);
    state.forest_roots.push(drv_dl_id);

    let config = Config {
        silent: false,
        piping: false,
        ..Default::default()
    };
    let rendered = render_state_to_text(&state, config, 1.0);

    // Tree must contain dep-1.0
    assert!(
        rendered.contains("dep-1.0"),
        "Rendered tree must contain dep-1.0: \n{}",
        rendered
    );
    // Should NOT contain "from cn" or "from cache.nixos.org"
    assert!(
        !rendered.contains("from cn"),
        "Rendered tree must NOT contain 'from cn' when only 1 substituter exists: \n{}",
        rendered
    );
    assert!(
        !rendered.contains("from cache"),
        "Rendered tree must NOT contain 'from cache': \n{}",
        rendered
    );
}

#[test]
fn test_multiple_substituters_display_from_abbrev() {
    use nix_output_monitor::render::{render_state_to_text, Config};
    use nix_output_monitor::state::{NomState, TransferInfo};
    use nix_output_monitor::types::{Derivation, Host, OutputName, StorePath};
    use rustc_hash::FxHashMap;

    let mut state = NomState::new(0.0, None, FxHashMap::default());

    // 2 downloads from 2 different substituters
    let sp1 = StorePath::parse("/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-dep-1.0").unwrap();
    let sp1_id = state.get_store_path_id(&sp1);
    let dl1_info = TransferInfo {
        host: Host::parse("https://cache.nixos.org"),
        start: 0.0,
        activity_id: None,
        end: (),
    };
    state
        .full_summary
        .running_downloads
        .insert(sp1_id, dl1_info.clone());

    let drv1 =
        Derivation::parse("/nix/store/11111111111111111111111111111111-dep-1.0.drv").unwrap();
    let drv1_id = state.get_derivation_id(&drv1);
    state
        .get_derivation_mut(drv1_id)
        .outputs
        .insert(OutputName::Out, sp1_id);
    state
        .get_derivation_mut(drv1_id)
        .dependency_summary
        .running_downloads
        .insert(sp1_id, dl1_info);
    state.forest_roots.push(drv1_id);

    let sp2 = StorePath::parse("/nix/store/cccccccccccccccccccccccccccccccc-dep-2.0").unwrap();
    let sp2_id = state.get_store_path_id(&sp2);
    let dl2_info = TransferInfo {
        host: Host::parse("https://cuda-maintainers.cachix.org"),
        start: 0.0,
        activity_id: None,
        end: (),
    };
    state
        .full_summary
        .running_downloads
        .insert(sp2_id, dl2_info.clone());

    let drv2 =
        Derivation::parse("/nix/store/22222222222222222222222222222222-dep-2.0.drv").unwrap();
    let drv2_id = state.get_derivation_id(&drv2);
    state
        .get_derivation_mut(drv2_id)
        .outputs
        .insert(OutputName::Out, sp2_id);
    state
        .get_derivation_mut(drv2_id)
        .dependency_summary
        .running_downloads
        .insert(sp2_id, dl2_info);
    state.forest_roots.push(drv2_id);

    let config = Config {
        silent: false,
        piping: false,
        ..Default::default()
    };
    let rendered = render_state_to_text(&state, config, 1.0);

    // Should contain "from cn" and "from cc"
    assert!(
        rendered.contains("from cn"),
        "Rendered tree MUST contain 'from cn' when multiple substituters exist: \n{}",
        rendered
    );
    assert!(
        rendered.contains("from cc"),
        "Rendered tree MUST contain 'from cc' when multiple substituters exist: \n{}",
        rendered
    );
}

#[test]
fn test_waiting_unknown_derivation_renders_with_todo_symbol() {
    use nix_output_monitor::render::{render_state_to_text, Config};
    use nix_output_monitor::state::{BuildInfo, BuildStatus, InputDerivation, NomState};
    use nix_output_monitor::types::{Derivation, Host};
    use rustc_hash::FxHashMap;

    let mut state = NomState::new(0.0, None, FxHashMap::default());

    // Parent has BuildStatus::Unknown, but active child is building
    let parent = Derivation::parse("/nix/store/11111111111111111111111111111111-source-pkg.drv").unwrap();
    let parent_id = state.get_derivation_id(&parent);
    state.forest_roots.push(parent_id);

    let child = Derivation::parse("/nix/store/22222222222222222222222222222222-child-build.drv").unwrap();
    let child_id = state.get_derivation_id(&child);
    let building_status = BuildStatus::Building(BuildInfo {
        start: 0.0,
        host: Host::Localhost,
        estimate: None,
        activity_id: None,
        end: (),
    });
    state.get_derivation_mut(child_id).build_status = building_status.clone();

    // Link parent -> child
    state.get_derivation_mut(parent_id).input_derivations = vec![InputDerivation {
        derivation: child_id,
        outputs: std::collections::BTreeSet::new(),
    }];
    state.get_derivation_mut(child_id).derivation_parents.insert(parent_id);

    NomState::update_summary_for_derivation(
        &mut state.get_derivation_mut(parent_id).dependency_summary,
        &BuildStatus::Unknown,
        &building_status,
        child_id,
    );

    let config = Config {
        silent: false,
        piping: false,
        ..Default::default()
    };
    let rendered = render_state_to_text(&state, config, 1.0);

    // parent MUST be rendered with ⏸ (TODO), never as bare uncolored text
    assert!(
        rendered.contains("⏸ source-pkg"),
        "Parent with active dependencies MUST render with ⏸ TODO icon, got:\n{}",
        rendered
    );
}

#[test]
fn test_curl_progress_json_stream_end_to_end() {
    use std::io::Cursor;
    use nix_output_monitor::engine::monitor_stream;
    use nix_output_monitor::render::render_state_to_text;

    let json_input = concat!(
        "@nix {\"action\":\"start\",\"fields\":[\"/nix/store/vh8zlb3v42hw7k19r193sn1l2idnan51-Geekbench-6.7.1-Linux.tar.gz.drv\",\"local\"],\"id\":105,\"level\":3,\"text\":\"building '/nix/store/vh8zlb3v42hw7k19r193sn1l2idnan51-Geekbench-6.7.1-Linux.tar.gz.drv'\",\"type\":105}\n",
        "@nix {\"action\":\"result\",\"fields\":[\"trying https://cdn.geekbench.com/Geekbench-6.7.1-Linux.tar.gz\"],\"id\":105,\"type\":101}\n",
        "@nix {\"action\":\"result\",\"fields\":[\"  % Total    % Received % Xferd  Average Speed  Time    Time    Time   Current\"],\"id\":105,\"type\":101}\n",
        "@nix {\"action\":\"result\",\"fields\":[\"                                 Dload  Upload  Total   Spent   Left   Speed\"],\"id\":105,\"type\":101}\n",
        "@nix {\"action\":\"result\",\"fields\":[\"  1 203.0M   1  2.93M   0      0  1.94M      0   01:44   00:01   01:43  2.89M\"],\"id\":105,\"type\":101}\n",
        "@nix {\"action\":\"result\",\"fields\":[\" 22 203.0M  22 45.82M   0      0  8.28M      0   00:24   00:05   00:19  9.09M\"],\"id\":105,\"type\":101}\n"
    );

    let config = Config {
        silent: false,
        piping: false,
        ..Default::default()
    };
    let state = monitor_stream(Cursor::new(json_input), true, config);

    let drv = Derivation::parse("/nix/store/vh8zlb3v42hw7k19r193sn1l2idnan51-Geekbench-6.7.1-Linux.tar.gz.drv").unwrap();
    let drv_id = state.derivation_ids.get(&drv).expect("derivation must be registered");
    let drv_info = state.get_derivation(*drv_id);

    let cp = drv_info.curl_progress.as_ref().expect("curl_progress must be present on derivation");
    assert_eq!(cp.host.hostname_only(), "cdn.geekbench.com");
    assert_eq!(cp.host.format_with_proto_context(), "cdn.geekbench.com (https)");
    assert_eq!(cp.done_bytes, (45.82f64 * 1024.0 * 1024.0).round() as usize);
    assert_eq!(cp.total_bytes, (203.0f64 * 1024.0 * 1024.0).round() as usize);

    let rendered = render_state_to_text(&state, config, 2.0);
    assert!(rendered.contains("cdn.geekbench.com"), "Rendered view must contain detected hostname:\n{}", rendered);
    assert!(rendered.contains("45.8 MiB/203.0 MiB"), "Rendered view must contain transfer progress:\n{}", rendered);
    assert!(!rendered.contains("9.09M/s"), "Rendered view must NOT contain speed:\n{}", rendered);
}

#[test]
fn test_curl_progress_retry_resets_size_and_updates_mirror() {
    use std::io::Cursor;
    use nix_output_monitor::engine::monitor_stream;

    let json_input = concat!(
        "@nix {\"action\":\"start\",\"fields\":[\"/nix/store/vh8zlb3v42hw7k19r193sn1l2idnan51-Geekbench-6.7.1-Linux.tar.gz.drv\",\"local\"],\"id\":105,\"level\":3,\"text\":\"building '/nix/store/vh8zlb3v42hw7k19r193sn1l2idnan51-Geekbench-6.7.1-Linux.tar.gz.drv'\",\"type\":105}\n",
        "@nix {\"action\":\"result\",\"fields\":[\"trying https://cdn.geekbench.com/Geekbench-6.7.1-Linux.tar.gz\"],\"id\":105,\"type\":101}\n",
        "@nix {\"action\":\"result\",\"fields\":[\"  6 217.5M   6 14.46M   0      0  5.63M      0   00:38   00:02   00:36  7.15M\"],\"id\":105,\"type\":101}\n",
        "@nix {\"action\":\"result\",\"fields\":[\"curl: (56) OpenSSL SSL_read: error\"],\"id\":105,\"type\":101}\n",
        "@nix {\"action\":\"result\",\"fields\":[\"Warning: Problem (retrying all errors). Retrying in 1 second. 3 retries left.\"],\"id\":105,\"type\":101}\n",
        "@nix {\"action\":\"result\",\"fields\":[\"trying https://backup.mirror.org/Geekbench-6.7.1-Linux.tar.gz\"],\"id\":105,\"type\":101}\n",
        "@nix {\"action\":\"result\",\"fields\":[\"  0      0   0      0   0      0      0      0                              0\"],\"id\":105,\"type\":101}\n",
        "@nix {\"action\":\"result\",\"fields\":[\" 12 203.0M  12 24.79M   0      0  7.02M      0   00:28   00:03   00:25  8.17M\"],\"id\":105,\"type\":101}\n"
    );

    let config = Config {
        silent: true,
        piping: false,
        ..Default::default()
    };
    let state = monitor_stream(Cursor::new(json_input), true, config);

    let drv = Derivation::parse("/nix/store/vh8zlb3v42hw7k19r193sn1l2idnan51-Geekbench-6.7.1-Linux.tar.gz.drv").unwrap();
    let drv_id = state.derivation_ids.get(&drv).expect("derivation must be registered");
    let drv_info = state.get_derivation(*drv_id);

    let cp = drv_info.curl_progress.as_ref().expect("curl_progress must be present on derivation");
    assert_eq!(cp.host.hostname_only(), "backup.mirror.org");
    assert_eq!(cp.done_bytes, (24.79f64 * 1024.0 * 1024.0).round() as usize);
    assert_eq!(cp.total_bytes, (203.0f64 * 1024.0 * 1024.0).round() as usize);
}

#[test]
fn test_curl_progress_fallback_host_when_no_trying_line() {
    use std::io::Cursor;
    use nix_output_monitor::engine::monitor_stream;

    let json_input = concat!(
        "@nix {\"action\":\"start\",\"fields\":[\"/nix/store/vh8zlb3v42hw7k19r193sn1l2idnan51-pkg.tar.gz.drv\",\"local\"],\"id\":105,\"level\":3,\"text\":\"building '/nix/store/vh8zlb3v42hw7k19r193sn1l2idnan51-pkg.tar.gz.drv'\",\"type\":105}\n",
        "@nix {\"action\":\"result\",\"fields\":[\"  5 100.0M   5  5.00M   0      0  1.00M      0   01:40   00:05   01:35  1.00M\"],\"id\":105,\"type\":101}\n"
    );

    let config = Config {
        silent: true,
        piping: false,
        ..Default::default()
    };
    let state = monitor_stream(Cursor::new(json_input), true, config);

    let drv = Derivation::parse("/nix/store/vh8zlb3v42hw7k19r193sn1l2idnan51-pkg.tar.gz.drv").unwrap();
    let drv_id = state.derivation_ids.get(&drv).expect("derivation must be registered");
    let drv_info = state.get_derivation(*drv_id);

    let cp = drv_info.curl_progress.as_ref().expect("curl_progress must be present on derivation");
    assert_eq!(cp.host.hostname_only(), "curl");
}

#[test]
fn test_host_sorting_by_download_size() {
    use nix_output_monitor::parser::json::{Activity, ActivityProgress};
    use nix_output_monitor::render::{render_state_to_text, Config, HostSort};
    use nix_output_monitor::state::{ActivityStatus, NomState, TransferInfo};
    use nix_output_monitor::types::{Host, StorePath};
    use rustc_hash::FxHashMap;

    let mut state = NomState::new(0.0, None, FxHashMap::default());

    let make_dl = |state: &mut NomState, act_id: u64, host_url: &str, path_str: &str, size_bytes: usize| {
        let sp = StorePath::parse(path_str).unwrap();
        let sp_id = state.get_store_path_id(&sp);
        let host = Host::parse(host_url);
        state.activities.insert(
            act_id,
            ActivityStatus {
                activity: Activity::CopyPath {
                    path: sp.clone(),
                    from: host.clone(),
                    to: Host::Localhost,
                },
                phase: None,
                progress: Some(ActivityProgress {
                    done: size_bytes,
                    expected: size_bytes,
                    running: 0,
                    failed: 0,
                }),
                file_transfer_progress: None,
                curl_progress: None,
                prefix: "".into(),
            },
        );
        state.full_summary.completed_downloads.insert(
            sp_id,
            TransferInfo {
                host,
                start: 0.0,
                activity_id: Some(act_id),
                end: nix_output_monitor::state::CompletedEnd(Some(1.0)),
            },
        );
    };

    make_dl(&mut state, 1, "https://small.cache.org", "/nix/store/11111111111111111111111111111111-p1", 10 * 1024 * 1024);
    make_dl(&mut state, 2, "https://large.cache.org", "/nix/store/22222222222222222222222222222222-p2", 500 * 1024 * 1024);
    make_dl(&mut state, 3, "https://medium.cache.org", "/nix/store/33333333333333333333333333333333-p3", 50 * 1024 * 1024);

    let config = Config {
        silent: false,
        piping: false,
        host_sort: HostSort::DownloadSize,
        host_cap: None,
    };

    let rendered = render_state_to_text(&state, config, 2.0);

    let pos_small = rendered.find("small.cache.org").expect("small.cache.org must be present");
    let pos_med = rendered.find("medium.cache.org").expect("medium.cache.org must be present");
    let pos_large = rendered.find("large.cache.org").expect("large.cache.org must be present");

    // Biggest ones go to the bottom: small < medium < large
    assert!(pos_small < pos_med, "small must appear before medium");
    assert!(pos_med < pos_large, "medium must appear before large (biggest at bottom)");
}

#[test]
fn test_host_sorting_by_builds() {
    use nix_output_monitor::render::{render_state_to_text, Config, HostSort};
    use nix_output_monitor::state::{BuildInfo, NomState};
    use nix_output_monitor::types::{Derivation, Host};
    use rustc_hash::FxHashMap;

    let mut state = NomState::new(0.0, None, FxHashMap::default());

    let add_build = |state: &mut NomState, host_name: &str, drv_str: &str| {
        let drv = Derivation::parse(drv_str).unwrap();
        let drv_id = state.get_derivation_id(&drv);
        let host = Host::parse(host_name);
        state.full_summary.completed_builds.insert(
            drv_id,
            BuildInfo {
                start: 0.0,
                host,
                estimate: None,
                activity_id: None,
                end: 1.0,
            },
        );
    };

    // builder-small: 1 build
    add_build(&mut state, "builder-small.org", "/nix/store/11111111111111111111111111111111-p1.drv");

    // builder-medium: 3 builds
    add_build(&mut state, "builder-medium.org", "/nix/store/22222222222222222222222222222222-p2.drv");
    add_build(&mut state, "builder-medium.org", "/nix/store/33333333333333333333333333333333-p3.drv");
    add_build(&mut state, "builder-medium.org", "/nix/store/44444444444444444444444444444444-p4.drv");

    // builder-large: 5 builds
    for i in 5..=9 {
        add_build(&mut state, "builder-large.org", &format!("/nix/store/{:032x}-p{}.drv", i, i));
    }

    let config = Config {
        silent: false,
        piping: false,
        host_sort: HostSort::Builds,
        host_cap: None,
    };

    let rendered = render_state_to_text(&state, config, 2.0);

    let pos_small = rendered.find("builder-small.org").expect("builder-small.org must be present");
    let pos_med = rendered.find("builder-medium.org").expect("builder-medium.org must be present");
    let pos_large = rendered.find("builder-large.org").expect("builder-large.org must be present");

    // Biggest ones go to the bottom: small (1) < medium (3) < large (5)
    assert!(pos_small < pos_med, "small must appear before medium");
    assert!(pos_med < pos_large, "medium must appear before large (biggest at bottom)");
}

#[test]
fn test_host_capping_with_other() {
    use nix_output_monitor::render::{render_state_to_text, Config, HostSort};
    use nix_output_monitor::state::{BuildInfo, NomState};
    use nix_output_monitor::types::{Derivation, Host};
    use rustc_hash::FxHashMap;

    let mut state = NomState::new(0.0, None, FxHashMap::default());

    let add_build = |state: &mut NomState, host_name: &str, drv_str: &str| {
        let drv = Derivation::parse(drv_str).unwrap();
        let drv_id = state.get_derivation_id(&drv);
        let host = Host::parse(host_name);
        state.full_summary.completed_builds.insert(
            drv_id,
            BuildInfo {
                start: 0.0,
                host,
                estimate: None,
                activity_id: None,
                end: 1.0,
            },
        );
    };

    add_build(&mut state, "node-1.org", "/nix/store/11111111111111111111111111111111-p1.drv");
    add_build(&mut state, "node-2.org", "/nix/store/22222222222222222222222222222222-p2.drv");
    add_build(&mut state, "node-3.org", "/nix/store/33333333333333333333333333333333-p3.drv");
    for i in 10..15 {
        add_build(&mut state, "node-big-1.org", &format!("/nix/store/{:032x}-p{}.drv", i, i));
    }
    for i in 20..30 {
        add_build(&mut state, "node-big-2.org", &format!("/nix/store/{:032x}-p{}.drv", i, i));
    }

    // Cap to 2 hosts.
    // Out of 6 total hosts (localhost, node-1, node-2, node-3, node-big-1, node-big-2),
    // 2 largest must be shown (node-big-1 and node-big-2), and all others combined into "other".
    let config = Config {
        silent: false,
        piping: false,
        host_sort: HostSort::Builds,
        host_cap: Some(2),
    };

    let rendered = render_state_to_text(&state, config, 2.0);

    // "other" must appear because total hosts (6) > cap (2)
    assert!(rendered.contains("other"), "Must contain 'other' host row:\n{}", rendered);

    // node-big-1 and node-big-2 must appear
    assert!(rendered.contains("node-big-1.org"), "Must contain top host node-big-1.org");
    assert!(rendered.contains("node-big-2.org"), "Must contain top host node-big-2.org");

    // node-1, node-2, node-3 must NOT appear individually
    assert!(!rendered.contains("node-1.org"), "Must NOT contain capped-out node-1.org");
    assert!(!rendered.contains("node-2.org"), "Must NOT contain capped-out node-2.org");
    assert!(!rendered.contains("node-3.org"), "Must NOT contain capped-out node-3.org");

    // Order: "other" (top) -> node-big-1 -> node-big-2 (bottom)
    let pos_other = rendered.find("other").unwrap();
    let pos_b1 = rendered.find("node-big-1.org").unwrap();
    let pos_b2 = rendered.find("node-big-2.org").unwrap();
    assert!(pos_other < pos_b1, "other must appear before top hosts");
    assert!(pos_b1 < pos_b2, "node-big-1 must appear before node-big-2 (biggest at bottom)");
}

#[test]
fn test_host_capping_no_other_when_within_cap() {
    use nix_output_monitor::render::{render_state_to_text, Config, HostSort};
    use nix_output_monitor::state::{BuildInfo, NomState};
    use nix_output_monitor::types::{Derivation, Host};
    use rustc_hash::FxHashMap;

    let mut state = NomState::new(0.0, None, FxHashMap::default());

    let drv = Derivation::parse("/nix/store/11111111111111111111111111111111-p1.drv").unwrap();
    let drv_id = state.get_derivation_id(&drv);
    state.full_summary.completed_builds.insert(
        drv_id,
        BuildInfo {
            start: 0.0,
            host: Host::parse("builder.org"),
            estimate: None,
            activity_id: None,
            end: 1.0,
        },
    );

    let drv_local = Derivation::parse("/nix/store/00000000000000000000000000000000-local.drv").unwrap();
    let drv_local_id = state.get_derivation_id(&drv_local);
    state.full_summary.completed_builds.insert(
        drv_local_id,
        BuildInfo {
            start: 0.0,
            host: Host::Localhost,
            estimate: None,
            activity_id: None,
            end: 1.0,
        },
    );

    // Total hosts = 2 (localhost, builder.org). Cap = 5.
    let config = Config {
        silent: false,
        piping: false,
        host_sort: HostSort::Builds,
        host_cap: Some(5),
    };

    let rendered = render_state_to_text(&state, config, 2.0);

    // "other" must NOT appear because 2 <= 5
    assert!(!rendered.contains("other"), "Must NOT contain 'other' when within cap:\n{}", rendered);
    assert!(rendered.contains("builder.org"), "Must contain builder.org");
    assert!(rendered.contains("localhost"), "Must contain localhost");
}

#[test]
fn test_localhost_never_capped_into_other() {
    use nix_output_monitor::render::{render_state_to_text, Config, HostSort};
    use nix_output_monitor::state::{BuildInfo, NomState};
    use nix_output_monitor::types::{Derivation, Host};
    use rustc_hash::FxHashMap;

    let mut state = NomState::new(0.0, None, FxHashMap::default());

    let add_build = |state: &mut NomState, host: Host, drv_str: &str| {
        let drv = Derivation::parse(drv_str).unwrap();
        let drv_id = state.get_derivation_id(&drv);
        state.full_summary.completed_builds.insert(
            drv_id,
            BuildInfo {
                start: 0.0,
                host,
                estimate: None,
                activity_id: None,
                end: 1.0,
            },
        );
    };

    // localhost gets 1 build (fewest)
    add_build(&mut state, Host::Localhost, "/nix/store/00000000000000000000000000000000-local.drv");
    // remote hosts get more builds
    for i in 1..=5u32 {
        add_build(
            &mut state,
            Host::parse(&format!("node-{}.org", i)),
            &format!("/nix/store/{:032x}-p{}.drv", i, i),
        );
    }

    // Cap to 2 — only 2 remote hosts shown, rest folded into "other".
    // localhost must ALWAYS appear (never capped), even though it has the fewest builds.
    let config = Config {
        silent: false,
        piping: false,
        host_sort: HostSort::Builds,
        host_cap: Some(2),
    };

    let rendered = render_state_to_text(&state, config, 2.0);

    assert!(rendered.contains("localhost"), "localhost must always appear, never capped into other:\n{}", rendered);
    assert!(rendered.contains("other"), "other must appear when remote hosts exceed cap:\n{}", rendered);

    // localhost must appear BEFORE "other"
    let pos_localhost = rendered.find("localhost").unwrap();
    let pos_other = rendered.find("other").unwrap();
    assert!(pos_localhost < pos_other, "localhost must appear before other");
}

#[test]
fn test_cli_parse_nom_options() {
    use nix_output_monitor::cli::parse_nom_options;
    use nix_output_monitor::render::HostSort;

    let args = vec![
        "--json".to_string(),
        "--sort-hosts-by-size".to_string(),
        "--cap-hosts".to_string(),
        "5".to_string(),
        "foo".to_string(),
    ];
    let (clean, sort, cap) = parse_nom_options(&args);
    assert_eq!(clean, vec!["--json", "foo"]);
    assert_eq!(sort, HostSort::DownloadSize);
    assert_eq!(cap, Some(5));

    let args2 = vec![
        "--sort-hosts-by-builds".to_string(),
        "--cap-hosts=3".to_string(),
    ];
    let (clean2, sort2, cap2) = parse_nom_options(&args2);
    assert!(clean2.is_empty());
    assert_eq!(sort2, HostSort::Builds);
    assert_eq!(cap2, Some(3));
}

#[test]
fn test_env_parse_nom_options() {
    use nix_output_monitor::cli::parse_nom_options;
    use nix_output_monitor::render::HostSort;

    unsafe {
        std::env::set_var("NOM_SORT_BY_BUILDS", "true");
        std::env::set_var("NOM_HOST_CAP", "7");
    }
    let (clean, sort, cap) = parse_nom_options(&[]);
    assert!(clean.is_empty());
    assert_eq!(sort, HostSort::Builds);
    assert_eq!(cap, Some(7));

    unsafe {
        std::env::remove_var("NOM_SORT_BY_BUILDS");
        std::env::remove_var("NOM_HOST_CAP");
        std::env::set_var("NOM_SORT_BY_SIZE", "1");
        std::env::set_var("NOM_CAP_HOSTS", "4");
    }
    let (clean2, sort2, cap2) = parse_nom_options(&[]);
    assert!(clean2.is_empty());
    assert_eq!(sort2, HostSort::DownloadSize);
    assert_eq!(cap2, Some(4));

    unsafe {
        std::env::remove_var("NOM_SORT_BY_SIZE");
        std::env::remove_var("NOM_CAP_HOSTS");
        std::env::remove_var("NOM_HOST_CAP");
    }
}

#[test]
fn test_download_expected_size_retained_on_retry() {
    use nix_output_monitor::engine::monitor_stream;
    use nix_output_monitor::render::Config;
    use std::io::Cursor;

    let json_stream = r#"@nix {"action":"start","id":10,"level":4,"parent":0,"text":"copying path '/nix/store/894zv7pm5ggbdysrfgl6mj91a8x8qzc4-linux-firmware' from 'https://cache.nixos.org'","type":100,"fields":["/nix/store/894zv7pm5ggbdysrfgl6mj91a8x8qzc4-linux-firmware","https://cache.nixos.org",""]}
@nix {"action":"start","id":11,"level":4,"parent":10,"text":"fetching nar","type":101,"fields":["https://cache.nixos.org/nar/123.nar.zst"]}
@nix {"action":"result","id":11,"type":105,"fields":[186000000,804000000,0,0]}
@nix {"action":"msg","level":1,"msg":"warning: unable to download 'https://cache.nixos.org/nar/123.nar.zst': HTTP error 200; retrying from offset 186000000"}
@nix {"action":"result","id":11,"type":105,"fields":[0,618000000,0,0]}
"#;

    let config = Config::default();
    let state = monitor_stream(Cursor::new(json_stream.as_bytes()), true, config);

    let parent_act = state.activities.get(&10).expect("parent activity must exist");
    let prog = parent_act
        .file_transfer_progress
        .as_ref()
        .expect("must have file_transfer_progress");

    assert_eq!(
        prog.expected, 804000000,
        "Expected download size must NOT drop to remaining chunk (618MB) on retry"
    );
}

#[test]
fn test_errors_captured_into_nix_errors_from_plain_and_json() {
    use nix_output_monitor::engine::monitor_stream;
    use nix_output_monitor::render::{render_state_to_text, Config};
    use std::io::Cursor;

    let input_stream = "error: unable to download 'https://cache.nixos.org/nar/1.nar.zst': HTTP error 206 (curl error: Failed sending data to the peer)\n@nix {\"action\":\"msg\",\"level\":0,\"msg\":\"error: unable to download 'https://cache.nixos.org/nar/2.nar.zst': HTTP error 206\"}\n";

    let config = Config::default();
    let state = monitor_stream(Cursor::new(input_stream.as_bytes()), true, config);

    assert_eq!(state.nix_errors.len(), 2, "Both plain and JSON errors must be captured in nix_errors");
    let rendered = render_state_to_text(&state, config, 1.0);
    assert!(rendered.contains("2 Errors:"), "Rendered output must display '2 Errors:':\n{}", rendered);
}

#[test]
fn test_parse_builtin_fetcher_derivation() {
    use nix_output_monitor::parser::derivation::parse_derivation_content;
    use nix_output_monitor::types::OutputName;

    let drv_content = r#"Derive([("out","/nix/store/anjj2gxcivlzcpz0idsvz202i2vnxyn9-libxcrypt-4.5.2.tar.xz","sha256","71513a31c01a428bccd5367a32fd95f115d6dac50fb5b60c779d5c7942aec071")],[],[],"builtin","builtin:fetchurl",[],[("builder","builtin:fetchurl"),("executable",""),("impureEnvVars","http_proxy https_proxy ftp_proxy all_proxy no_proxy"),("name","libxcrypt-4.5.2.tar.xz"),("out","/nix/store/anjj2gxcivlzcpz0idsvz202i2vnxyn9-libxcrypt-4.5.2.tar.xz"),("outputHash","sha256-cVE6McAaQovM1TZ6Mv2V8RXW2sUPtbYMd51ceUKuwHE="),("outputHashAlgo",""),("outputHashMode","flat"),("preferLocalBuild","1"),("system","builtin"),("unpack",""),("url","https://github.com/besser82/libxcrypt/releases/download/v4.5.2/libxcrypt-4.5.2.tar.xz"),("urls","https://github.com/besser82/libxcrypt/releases/download/v4.5.2/libxcrypt-4.5.2.tar.xz")])"#;

    let parsed = parse_derivation_content(drv_content).expect("Must parse builtin:fetchurl derivation");
    assert_eq!(parsed.platform, "builtin");
    assert_eq!(parsed.pname, Some("libxcrypt-4.5.2.tar.xz".to_string()));
    assert!(parsed.outputs.contains_key(&OutputName::parse("out")));

    // Test CA derivation with empty string in outputs table recovered from env
    let ca_content = r#"Derive([("out","","","")],[],[],"builtin","builtin:fetchurl",[],[("builder","builtin:fetchurl"),("name","test-ca"),("out","/nix/store/iz25ayazkdr0gsy0qqfd923vbpam90kd-test-ca"),("outputHashAlgo","sha256"),("outputHashMode","flat"),("system","builtin")])"#;
    let parsed_ca = parse_derivation_content(ca_content).expect("Must parse CA derivation with empty output");
    assert_eq!(parsed_ca.pname, Some("test-ca".to_string()));
    assert!(parsed_ca.outputs.contains_key(&OutputName::parse("out")), "Output must be recovered from env when empty in outputs table");
}

#[test]
fn test_builtin_fetcher_build_displays_transfer_progress() {
    use nix_output_monitor::engine::monitor_stream;
    use nix_output_monitor::render::{render_state_to_text, Config};
    use std::io::Cursor;

    let stream = r#"@nix {"action":"start","id":100,"level":0,"parent":0,"text":"building '/nix/store/55d91flpk4776srkw66wdx69wll7b5zr-libxcrypt-4.5.2.tar.xz.drv'","type":105,"fields":["/nix/store/55d91flpk4776srkw66wdx69wll7b5zr-libxcrypt-4.5.2.tar.xz.drv","localhost"]}
@nix {"action":"start","id":101,"level":4,"parent":100,"text":"fetching 'https://github.com/besser82/libxcrypt/releases/download/v4.5.2/libxcrypt-4.5.2.tar.xz'","type":101,"fields":["https://github.com/besser82/libxcrypt/releases/download/v4.5.2/libxcrypt-4.5.2.tar.xz"]}
@nix {"action":"result","id":101,"type":105,"fields":[524288,1048576,0,0]}
"#;

    let config = Config::default();
    let state = monitor_stream(Cursor::new(stream.as_bytes()), true, config);
    let rendered = render_state_to_text(&state, config, 2.0);
    assert!(
        rendered.contains("512.0 KiB/1.0 MiB"),
        "Rendered output must show file transfer progress on builtin fetcher build: {}",
        rendered
    );
}




