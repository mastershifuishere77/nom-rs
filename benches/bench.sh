#!/usr/bin/env bash
set -euo pipefail

# ANSI color codes
BOLD="\033[1m"
GREEN="\033[32m"
BLUE="\033[34m"
YELLOW="\033[33m"
CYAN="\033[36m"
RESET="\033[0m"

# Find root of repository
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ -f "$SCRIPT_DIR/Cargo.toml" ]; then
    REPO_DIR="$SCRIPT_DIR"
elif [ -f "$SCRIPT_DIR/../Cargo.toml" ]; then
    REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
else
    REPO_DIR="${REPO_ROOT:-$PWD}"
fi

if [ -n "${NOM_TEST_DIR:-}" ] && [ -d "$NOM_TEST_DIR" ]; then
    TEST_DIR="$NOM_TEST_DIR"
else
    TEST_DIR="$REPO_DIR/test"
fi

echo -e "${BOLD}${CYAN}=== nom-rs vs Haskell nom Benchmark ===${RESET}\n"

# 1. Resolve Rust binary
if [ -n "${NOM_RUST_BIN:-}" ] && [ -x "$NOM_RUST_BIN" ]; then
    NOM_RS="$NOM_RUST_BIN"
elif [ -f "$REPO_DIR/target/release/nom" ]; then
    NOM_RS="$REPO_DIR/target/release/nom"
elif command -v nom-rs >/dev/null 2>&1; then
    NOM_RS="$(command -v nom-rs)"
else
    echo -e "${YELLOW}Building release binary target/release/nom...${RESET}"
    cargo build --release --manifest-path "$REPO_DIR/Cargo.toml"
    NOM_RS="$REPO_DIR/target/release/nom"
fi

# 2. Resolve Haskell binary
if [ -n "${NOM_HASKELL_BIN:-}" ] && [ -x "$NOM_HASKELL_BIN" ]; then
    HASKELL_NOM="$NOM_HASKELL_BIN"
elif command -v nom >/dev/null 2>&1; then
    HASKELL_NOM="$(command -v nom)"
else
    echo -e "${YELLOW}Haskell 'nom' not found in PATH or NOM_HASKELL_BIN. Please run via 'nix run .#bench'.${RESET}"
    exit 1
fi

# 3. Resolve synthetic generator binary
if [ -n "${NOM_GEN_BIN:-}" ] && [ -x "$NOM_GEN_BIN" ]; then
    GEN_STREAM="$NOM_GEN_BIN"
elif [ -f "$REPO_DIR/target/release/bench-stream-gen" ]; then
    GEN_STREAM="$REPO_DIR/target/release/bench-stream-gen"
elif command -v rustc >/dev/null 2>&1; then
    mkdir -p "$REPO_DIR/target/bench-bin"
    rustc -O "$REPO_DIR/benches/generate_stream.rs" -o "$REPO_DIR/target/bench-bin/bench-stream-gen" 2>/dev/null || true
    if [ -x "$REPO_DIR/target/bench-bin/bench-stream-gen" ]; then
        GEN_STREAM="$REPO_DIR/target/bench-bin/bench-stream-gen"
    else
        GEN_STREAM=""
    fi
else
    GEN_STREAM=""
fi

echo -e "Using nom-rs:      ${GREEN}$NOM_RS${RESET}"
echo -e "Using Haskell nom: ${YELLOW}$HASKELL_NOM${RESET}"
if [ -n "$GEN_STREAM" ]; then
    echo -e "Using generator:   ${BLUE}$GEN_STREAM${RESET}\n"
else
    echo -e "Using generator:   ${BLUE}embedded fallback generator${RESET}\n"
fi

# Helper: measure peak memory (RSS in KB)
measure_rss() {
    local bin="$1"
    local input_file="$2"
    command time -v bash -c "'$bin' --json < '$input_file' >/dev/null 2>&1 || true" 2>&1 | \
        grep "Maximum resident set size" | awk '{print $NF}' || echo "0"
}

run_suite() {
    local name="$1"
    local log_file="$2"

    if [ ! -f "$log_file" ]; then
        echo -e "${YELLOW}File $log_file not found, skipping $name.${RESET}"
        return
    fi

    local lines_count
    lines_count="$(wc -l < "$log_file")"
    local size_human
    size_human="$(du -h "$log_file" | cut -f1)"

    echo -e "${BOLD}${BLUE}--- Benchmark: $name ($lines_count lines, $size_human) ---${RESET}"

    # Measure memory
    echo -e "${CYAN}Measuring Peak RAM (Max RSS)...${RESET}"
    local rss_rust rss_haskell
    rss_rust="$(measure_rss "$NOM_RS" "$log_file")"
    rss_haskell="$(measure_rss "$HASKELL_NOM" "$log_file")"

    if [ -n "$rss_rust" ] && [ -n "$rss_haskell" ] && [ "$rss_rust" -gt 0 ] 2>/dev/null && [ "$rss_haskell" -gt 0 ] 2>/dev/null; then
        local mb_rust mb_haskell
        mb_rust="$(awk "BEGIN {printf \"%.1f\", $rss_rust / 1024}")"
        mb_haskell="$(awk "BEGIN {printf \"%.1f\", $rss_haskell / 1024}")"
        local ratio
        ratio="$(awk "BEGIN {printf \"%.1fx\", $rss_haskell / $rss_rust}")"
        echo -e "  nom-rs (Rust):    ${GREEN}${mb_rust} MB${RESET} (${rss_rust} KB)"
        echo -e "  nom (Haskell):    ${YELLOW}${mb_haskell} MB${RESET} (${rss_haskell} KB)"
        echo -e "  Memory reduction: ${BOLD}${GREEN}${ratio} less RAM${RESET}\n"
    fi

    local warmup_count="${3:-3}"
    local max_runs="${4:-}"
    local extra_hyperfine=()
    if [ -n "$max_runs" ]; then
        extra_hyperfine+=(--max-runs "$max_runs")
    fi

    # Measure execution time via hyperfine
    echo -e "${CYAN}Running execution speed benchmark (hyperfine)...${RESET}"
    hyperfine -i --warmup "$warmup_count" "${extra_hyperfine[@]}" \
        -n "nom-rs (Rust)" "'$NOM_RS' --json < '$log_file'" \
        -n "nom (Haskell)" "'$HASKELL_NOM' --json < '$log_file'"

    echo ""
}

# Synthetic generator for stress testing
generate_synthetic_log() {
    local out="$1"
    local drvs="$2"
    local logs="$3"
    local downloads="${4:-50}"

    if [ -n "$GEN_STREAM" ] && [ -x "$GEN_STREAM" ]; then
        "$GEN_STREAM" --drvs "$drvs" --logs "$logs" --downloads "$downloads" -o "$out"
    else
        awk -v num_drvs="$drvs" -v logs_per_drv="$logs" 'BEGIN {
            print "@nix {\"action\":\"msg\",\"level\":3,\"msg\":\"these " num_drvs " derivations will be built:\"}";
            for (i=0; i<num_drvs; i++) {
                printf "@nix {\"action\":\"msg\",\"level\":3,\"msg\":\"  /nix/store/%032d-package-%04d.drv\"}\n", i, i;
            }
            for (i=0; i<num_drvs; i++) {
                id = 100000 + i;
                printf "@nix {\"action\":\"start\",\"id\":%d,\"level\":0,\"parent\":0,\"text\":\"building /nix/store/%032d-package-%04d.drv\",\"type\":105}\n", id, i, i;
                for (j=0; j<logs_per_drv; j++) {
                    printf "@nix {\"action\":\"result\",\"fields\":[\"gcc -O2 -Wall -c src/module_%03d.c\"],\"id\":%d,\"type\":101}\n", j, id;
                }
                printf "@nix {\"action\":\"stop\",\"id\":%d}\n", id;
            }
            print "@nix {\"action\":\"msg\",\"level\":3,\"msg\":\"build complete\"}";
        }' > "$out"
    fi
}

# Run benchmark suites
run_suite "Standard Build (Small)" "$TEST_DIR/integration/standard/stderr.json"
run_suite "Multi-Drv Build with Failures (Medium)" "$TEST_DIR/integration/fail/stderr.json"

LARGE_LOG="$(mktemp --suffix=_nom_bench_large.json)"
ULTRA_LOG="$(mktemp --suffix=_nom_bench_ultra.json)"
MEGA_LOG="$(mktemp --suffix=_nom_bench_mega.json)"
WIDE_LOG="$(mktemp --suffix=_nom_bench_wide.json)"
GIGA_LOG="$(mktemp --suffix=_nom_bench_giga_1m.json)"
trap 'rm -f "$LARGE_LOG" "$ULTRA_LOG" "$MEGA_LOG" "$WIDE_LOG" "$GIGA_LOG"' EXIT

generate_synthetic_log "$LARGE_LOG" 100 50 50
run_suite "Massive Build Stress Test (Large: 100 pkgs, ~5,500 events)" "$LARGE_LOG"
rm -f "$LARGE_LOG"

generate_synthetic_log "$ULTRA_LOG" 500 60 100
run_suite "Ultra Load Stress Test (Ultra: 500 pkgs, ~31,000 events)" "$ULTRA_LOG"
rm -f "$ULTRA_LOG"

echo -e "${BOLD}${YELLOW}Generating Mega Load (5,000 derivations, ~105,000 events)...${RESET}"
generate_synthetic_log "$MEGA_LOG" 5000 18 200
run_suite "Mega Extreme Stress Test (Mega: 5,000 pkgs, ~105,000 events)" "$MEGA_LOG" 1 5
rm -f "$MEGA_LOG"

echo -e "${BOLD}${YELLOW}Generating 10k Wide Tree Load (10,000 derivations, ~100,000 events)...${RESET}"
generate_synthetic_log "$WIDE_LOG" 10000 7 100
run_suite "10k Wide Tree Stress Test (10,000 pkgs, ~100,000 events)" "$WIDE_LOG" 1 5
rm -f "$WIDE_LOG"

echo -e "${BOLD}${YELLOW}Generating Giga Load (10,000 derivations, 1,000,000 events, ~100MB)...${RESET}"
generate_synthetic_log "$GIGA_LOG" 10000 98 500
run_suite "Giga Extreme Stress Test (1,000,000 events, 10,000 pkgs, ~100MB)" "$GIGA_LOG" 1 3
rm -f "$GIGA_LOG"
trap - EXIT

echo -e "${BOLD}${GREEN}=== Benchmark Completed Successfully ===${RESET}"
