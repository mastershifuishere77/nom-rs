# nom-rs

Rust rewrite of [nix-output-monitor (`nom`)](https://github.com/maralorn/nix-output-monitor) with custom UI improvements and performance optimizations.

Pipe nix commands through `nom-rs` to get real-time visualization of build progress: live dependency tree, download metrics, build duration estimates and an activity status bar.

While fully drop-in compatible with the original Haskell `nom` and tools like `nh`, `nom-rs` includes several custom features and engine improvements.

## What's different from original nom

- Cleaner dependency tree: noisy braille progress bars have been removed from tree nodes; active transfer sizes are highlighted in green, with total transferred data shown in the summary table (`↓ 35.5 MiB/590.9 MiB`).
- Multithreaded derivation prefetching: dependency graphs are inspected across background worker threads, preventing UI stutter on large closures with thousands of derivations.
- Flicker-free terminal rendering: uses atomic terminal synchronization (`\x1b[?2026h`) and strict cursor resets to prevent line shearing under rapid log output.
- Minimal overhead: instant startup with negligible CPU and memory usage, without Haskell/GHC runtime overhead.

## Core Features

- Drop-in replacement for `nom` (`nom build`, `nom shell`, `nom develop`, `nom copy`, `nom flake`, and `nom-build` / `nom-shell` symlinks)
- Parses both Nix internal JSON (`--json`) and human-readable output
- Real-time dependency tree showing active, pending and finished derivations
- Build duration estimation based on historical data (`$XDG_STATE_HOME/nix-output-monitor/build-reports.csv`)
- Store monitoring via inotify
- Works out of the box with `nh`

## Visual Overview

```text
┏━ Dependency Graph:
┃       ┌─ ✔ user-environment 
┃       │                 ┌─ ✔ home-manager-path 
┃       │              ┌─ ✔ hm_hmfontconfigfonts.xml 
┃       │              │  ┌─ ✔ man-cache  ⏱ 01s
┃       │              ├─ ✔ hm_.manpath 
┃       │           ┌─ ✔ home-manager-files 
┃       │        ┌─ ✔ hm-putter.json 
┃       │     ┌─ ✔ home-manager-generation 
┃       │  ┌─ ✔ unit-home-manager-user.service 
┃       ├─ ✔ system-units  ⏱ 01s
┃    ┌─ ✔ etc 
┃ ┌─ ✔ activate 
┃ ✔ nixos-system-nixos-24.11.20241101.1234567 
┣━━━ Builds            │ Downloads                            │ Host                               
┃    ⏵ 0 │ ✔  0 │      │ ↓ 0 │ ✔ 4 │ ↓ 35.5 MiB/590.9 MiB     │ cache.nixos.org                    
┃    ⏵ 0 │ ✔ 21 │      │ ↓ 0 │ ✔ 0 │                          │ localhost                          
┗━ ∑ ⏵ 0 │ ✔ 21 │ ⏸  0 │ ↓ 0 │ ✔ 4 │ ⏸ 0 │ ↓ 35.5 MiB/590.9 MiB │ ✔ Finished at 10:46:49 after 01m01s
```

## Benchmarks & Performance

`nom-rs` is significantly faster than the original Haskell implementation across all workload sizes, delivering up to **84x speedup** on stream processing and up to **7.4x lower peak memory usage**:

| Workload | Scale / Lines | Metric | `nom-rs` (Rust) | `nom` (Haskell) | Winner / Advantage |
|:---|:---|:---|:---|:---|:---|
| **Standard Build** (Small) | 77 lines (8 KB) | Execution time<br>Peak RAM (RSS) | **5.5 ms**<br>**4.2 MB** | 459.3 ms<br>30.6 MB | 🏆 **`nom-rs`** (84.1x faster)<br>🏆 **`nom-rs`** (7.4x less RAM) |
| **Multi-Drv Failures** (Medium) | 608 lines (80 KB) | Execution time<br>Peak RAM (RSS) | **18.1 ms**<br>**6.7 MB** | 232.0 ms<br>30.8 MB | 🏆 **`nom-rs`** (12.8x faster)<br>🏆 **`nom-rs`** (4.6x less RAM) |
| **Massive Build** (Large) | 100 pkgs, ~5,500 events (612 KB) | Execution time<br>Peak RAM (RSS) | **8.0 ms**<br>**4.5 MB** | 145.8 ms<br>30.9 MB | 🏆 **`nom-rs`** (18.2x faster)<br>🏆 **`nom-rs`** (6.9x less RAM) |
| **Ultra Load** | 500 pkgs, ~31,000 events (3.5 MB) | Execution time<br>Peak RAM (RSS) | **21.6 ms**<br>**8.6 MB** | 405.8 ms<br>31.7 MB | 🏆 **`nom-rs`** (18.8x faster)<br>🏆 **`nom-rs`** (3.7x less RAM) |
| **Mega Extreme** | 5,000 pkgs, ~105,000 events (12 MB) | Execution time<br>Peak RAM (RSS) | **61.5 ms**<br>**23.4 MB** | 1.282 s<br>36.0 MB | 🏆 **`nom-rs`** (20.8x faster)<br>🏆 **`nom-rs`** (1.5x less RAM) |
| **Wide Dependency Tree** | 10,000 pkgs, ~100,000 events (11 MB) | Execution time<br>Peak RAM (RSS) | **66.8 ms**<br>**35.3 MB** | 1.448 s<br>42.3 MB | 🏆 **`nom-rs`** (21.7x faster)<br>🏆 **`nom-rs`** (1.2x less RAM) |
| **Giga Extreme** | 10,000 pkgs, **1,000,000 events** (111 MB) | Execution time<br>Peak RAM (RSS) | **392.6 ms**<br>51.8 MB | 9.748 s<br>**43.7 MB** | 🏆 **`nom-rs`** (**24.8x faster**)<br>🏆 **`nom`** (1.2x less RAM) |

> **Fair Breakdown**:
> - **Execution Speed**: `nom-rs` wins **7 out of 7** tests (12.8x to 84.1x faster across all benchmarks).
> - **Memory Usage (Max RSS)**: `nom-rs` wins **6 out of 7** tests (consuming up to 7.4x less RAM on typical builds). On the synthetic 1,000,000-event Giga stress test, Haskell's compact runtime holds a slight edge (43.7 MB vs 51.8 MB), while `nom-rs` finishes in **0.39s** vs Haskell's **9.75s** (24.8x faster).
> - Measured on Linux using `hyperfine` (mean execution time) and GNU `time -v` (peak resident set size / Max RSS).

### Reproducing Benchmarks

The benchmark suite is 100% reproducible via Nix Flakes:

```bash
nix run .#bench
```

Or run directly from the repository:

```bash
./benches/bench.sh
```

## Installation

### NixOS Flake (with `nh`)

If you use [nh](https://github.com/nix-community/nh), you can point it to `nom-rs`:

#### `flake.nix`
```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    nom-rs = {
      url = "github:mastershifuishere77/nom-rs";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, nom-rs, ... }@inputs: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      specialArgs = { inherit inputs; };
      modules = [ ./configuration.nix ];
    };
  };
}
```

#### `configuration.nix`
```nix
{ pkgs, inputs, ... }:

{
  programs.nh = {
    enable = true;
    flake = "/path/to/your/nixos-config";

    package = pkgs.nh.override {
      nix-output-monitor = inputs.nom-rs.packages.${pkgs.stdenv.hostPlatform.system}.default;
    };
  };

  environment.systemPackages = [
    inputs.nom-rs.packages.${pkgs.stdenv.hostPlatform.system}.default
  ];
}
```

## Usage

### Direct wrappers

```bash
nom build .#myPackage
nom develop
nom shell nixpkgs#htop
nom copy --to ssh://builder .#myPackage
nom-build default.nix
```

### Piping from nix commands

```bash
# nixos-rebuild
sudo nixos-rebuild switch --flake .#myhost --log-format internal-json -v 2>&1 | nom --json

# nix build
nix build --log-format internal-json -v .#myPackage 2>&1 | nom --json
```

### With `nh`

```bash
nh os switch
```

## Building from Source

```bash
git clone https://github.com/mastershifuishere77/nom-rs.git
cd nom-rs

# Via Cargo
cargo build --release

# Or via Nix
nix build
```

Binary outputs will be in `./target/release/nom-rs` (and `./target/release/nom`) or `./result/bin/nom-rs`.

## License

MIT License. See [LICENSE](LICENSE) for details.

## Author

[mastershifuishere77](https://github.com/mastershifuishere77)

Original Haskell implementation: [maralorn/nix-output-monitor](https://github.com/maralorn/nix-output-monitor).