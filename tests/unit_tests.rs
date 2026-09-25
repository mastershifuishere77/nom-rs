use nix_output_monitor::parser::old_style::{parse_old_style_chunk, NixOldStyleMessage};
use nix_output_monitor::render::progress::{
    clamp_to_byte, lookup_progress_char, print_progress_bar, word5_to_word8,
};
use nix_output_monitor::types::{Derivation, FailType, Host, StorePath};
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
fn test_braille_progress_bar() {
    assert_eq!(clamp_to_byte(-23.0), 0);
    assert_eq!(clamp_to_byte(17.3), 18);
    assert_eq!(clamp_to_byte(300.0), 31);
    assert_eq!(word5_to_word8(31), 255);

    assert_eq!(lookup_progress_char(31), '\u{28FF}');
    assert_eq!(lookup_progress_char(0), '\u{2800}');

    let bar = print_progress_bar(3, 0.477);
    assert_eq!(bar, "\u{28FF}\u{2807}\u{2800}");
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
            assert_eq!(proto, Some("ssh".to_string()));
            assert_eq!(user, Some("user".to_string()));
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

    let mut state = NomState::new(0.0, None, std::collections::HashMap::new());
    let root_drv = Derivation {
        store_path: StorePath {
            hash: "00000000000000000000000000000000".to_string(),
            name: "root-package".to_string(),
        },
    };
    let root_id = state.get_derivation_id(&root_drv);
    state.forest_roots.push(root_id);

    let mut last_dep_id = None;
    for i in 0..30 {
        let dep_drv = Derivation {
            store_path: StorePath {
                hash: format!("{:032}", i),
                name: format!("dep-{}", i),
            },
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
    use std::collections::{BTreeSet, HashMap};

    let mut state = NomState::new(0.0, None, HashMap::new());

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
            store_path: StorePath {
                hash: format!("{:032x}", i + 1),
                name: name.to_string(),
            },
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
    use std::collections::{BTreeSet, HashMap};

    let mut state = NomState::new(0.0, None, HashMap::new());
    let root_drv = Derivation {
        store_path: StorePath {
            hash: "10000000000000000000000000000000".to_string(),
            name: "massive-project".to_string(),
        },
    };
    let root_id = state.get_derivation_id(&root_drv);
    state.forest_roots.push(root_id);

    let mut leaf_ids = Vec::new();
    for i in 0..100 {
        let dep_drv = Derivation {
            store_path: StorePath {
                hash: format!("{:032x}", i + 100),
                name: format!("dep-pkg-{:03}", i),
            },
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
    use std::collections::HashMap;

    let mut state = NomState::new(0.0, None, HashMap::new());
    let drv = Derivation {
        store_path: StorePath {
            hash: "12345678901234567890123456789012".to_string(),
            name: "test-drv".to_string(),
        },
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
    use std::collections::HashMap;

    let mut state = NomState::new(0.0, None, HashMap::new());

    // Register an activity with progress: 71.4 MiB / 3.0 GiB
    let act_id = 42;
    state.activities.insert(
        act_id,
        ActivityStatus {
            activity: Activity::CopyPath {
                path: StorePath {
                    hash: "hash1234567890123456789012345678".to_string(),
                    name: "pkg1".to_string(),
                },
                from: Host::Remote {
                    proto: Some("https".to_string()),
                    user: None,
                    host: "cache.nixos.org".to_string(),
                },
                to: Host::Localhost,
            },
            phase: None,
            progress: Some(ActivityProgress {
                done: 74868326,       // ~71.4 MiB
                expected: 3221225472, // 3.0 GiB
                running: 1,
                failed: 0,
            }),
        },
    );

    let sp_id = state.get_store_path_id(&StorePath {
        hash: "hash1234567890123456789012345678".to_string(),
        name: "pkg1".to_string(),
    });

    state.full_summary.running_downloads.insert(
        sp_id,
        TransferInfo {
            host: Host::Remote {
                proto: Some("https".to_string()),
                user: None,
                host: "cache.nixos.org".to_string(),
            },
            start: 0.0,
            activity_id: Some(act_id),
            end: (),
        },
    );

    // Also add a completed download
    let sp_done_id = state.get_store_path_id(&StorePath {
        hash: "done1234567890123456789012345678".to_string(),
        name: "pkg-done".to_string(),
    });
    state.full_summary.completed_downloads.insert(
        sp_done_id,
        TransferInfo {
            host: Host::Remote {
                proto: Some("https".to_string()),
                user: None,
                host: "cache.nixos.org".to_string(),
            },
            start: 0.0,
            activity_id: None,
            end: CompletedEnd(Some(1.0)),
        },
    );

    // Also add a build on Localhost so show_hosts is true (multiple hosts)
    let drv_id = state.get_derivation_id(&nix_output_monitor::types::Derivation {
        store_path: StorePath {
            hash: "local123456789012345678901234567".to_string(),
            name: "local-build".to_string(),
        },
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
    use nix_output_monitor::types::{Derivation, Host, StorePath};
    use std::collections::HashMap;

    let mut state = NomState::new(0.0, None, HashMap::new());

    // Create root
    let root = Derivation {
        store_path: StorePath {
            hash: "root0000000000000000000000000000".to_string(),
            name: "system-root".to_string(),
        },
    };
    let root_id = state.get_derivation_id(&root);
    state.forest_roots.push(root_id);

    // Active branch: root -> active_parent -> active_leaf
    let active_parent = Derivation {
        store_path: StorePath {
            hash: "actp0000000000000000000000000000".to_string(),
            name: "active-parent".to_string(),
        },
    };
    let active_parent_id = state.get_derivation_id(&active_parent);

    let active_leaf = Derivation {
        store_path: StorePath {
            hash: "actl0000000000000000000000000000".to_string(),
            name: "active-leaf".to_string(),
        },
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
            store_path: StorePath {
                hash: format!("inact{:028}", i),
                name: format!("inactive-depth-{}", i),
            },
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

    let selected = select_derivations_to_show(&state, 20);

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
    use std::collections::HashMap;

    let mut state = NomState::new(0.0, None, HashMap::new());

    // Root: nixos-system
    let root = Derivation {
        store_path: StorePath {
            hash: "sys0000000000000000000000000000".to_string(),
            name: "nixos-system".to_string(),
        },
    };
    let root_id = state.get_derivation_id(&root);
    state.forest_roots.push(root_id);

    // Completed build 1: etc
    let etc = Derivation {
        store_path: StorePath {
            hash: "etc0000000000000000000000000000".to_string(),
            name: "etc".to_string(),
        },
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
        store_path: StorePath {
            hash: "act0000000000000000000000000000".to_string(),
            name: "activate".to_string(),
        },
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
        store_path: StorePath {
            hash: "stg0000000000000000000000000000".to_string(),
            name: "stage-2-init.sh".to_string(),
        },
    };
    let stage2_id = state.get_derivation_id(&stage2);

    // Inert child of root: pre-switch-checks
    let preswitch = Derivation {
        store_path: StorePath {
            hash: "pre0000000000000000000000000000".to_string(),
            name: "pre-switch-checks".to_string(),
        },
    };
    let preswitch_id = state.get_derivation_id(&preswitch);

    // Inert child of etc: system-path
    let syspath = Derivation {
        store_path: StorePath {
            hash: "path000000000000000000000000000".to_string(),
            name: "system-path".to_string(),
        },
    };
    let syspath_id = state.get_derivation_id(&syspath);

    // Inert child of system-path: gnused
    let gnused = Derivation {
        store_path: StorePath {
            hash: "sed0000000000000000000000000000".to_string(),
            name: "gnused-4.10".to_string(),
        },
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
