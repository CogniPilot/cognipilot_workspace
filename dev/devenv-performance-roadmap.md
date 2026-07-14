# Devenv performance roadmap

> **Superseded pre-cutover record.** This document preserves measurements and
> design history from the deleted script-based workspace. Commands, paths, and
> interfaces below such as `ws ci`, `ws verify`, `devel/`, and
> `release-results/` are not supported by the current workspace. The active,
> Nix-first contract and remaining acceptance gates live in
> [`devenv-implementation-roadmap.md`](devenv-implementation-roadmap.md) and
> [`workspace-performance-summary.md`](workspace-performance-summary.md).

Historical working document from the effort to turn the CogniPilot workspace
into a fast, predictable developer environment. It is retained only as local
measurement provenance; `/dev/` is ignored by the workspace `.gitignore`.

## Baseline

- Warm shell entry: 0.27-0.29 seconds.
- Cold shell evaluation: 4.19-4.28 seconds.
- Unchanged `ws build cerebri_cubs2`: 527 seconds.
- Synapse task within that build: 466 seconds.
- Unchanged CUBS2 `west build -p auto`: 14.3 seconds.
- Direct incremental `cmake --build`: 2.38 seconds.

## Command contract

- `ws build`: incremental artifact production only. No tests, lint, docs,
  release packaging, exhaustive smoke tests, or forced clean builds.
- `ws test`: focused component tests that consume incremental build artifacts.
- `ws ci` or `ws verify`: clean, exhaustive validation and packaging.
- Clean rebuilds must be explicit (`--clean`) or triggered by a proven
  configuration incompatibility.

## Artifact and deployment contract

- `build/` contains disposable compiler intermediates and task fingerprints.
- `devel/` is the automatically activated local overlay. It may reflect
  uncommitted source and is never eligible for deployment.
- There is deliberately no ROS-style workspace `install/` prefix. Nix store
  outputs are the immutable installed artifacts.
- `release-results/` contains optional convenience links to release-mode Nix
  outputs. It is not on `PATH`, is not an overlay, and is not a deployment
  source of truth.
- Deployment promotes one explicit, versioned Nix output built from checked-in
  locks/pins and records its revision and content digest. Release evaluation
  never reads from `devel/`.

## Success metrics

- Warm shell entry remains below 0.5 seconds.
- Second unchanged CUBS2 build finishes below 5 seconds, with 10 seconds as the
  initial regression-test ceiling.
- A small CUBS2 source-only change builds below 10 seconds.
- An unchanged build does not execute CI, tests, documentation, packaging, or
  network fetches.
- Missing or stale artifacts are detected instead of being silently accepted by
  a task cache.

## Measured results

- Final unchanged `ws build cerebri_cubs2`: 4.16-4.85 seconds wall time, down
  from 527 seconds (more than 100x faster).
- Cached Synapse validation: 77 ms; cached Rumoca validation: 733 ms.
- Unchanged CUBS2 CMake/Ninja task: 3.31 seconds.
- Migration refresh after changing build recipes: 30.87 seconds. The same run
  before caching the pinned Synapse generators took about 4 minutes.
- The local Synapse build now generates only workspace-consumed development
  artifacts. CI remains responsible for language smoke tests, archives,
  documentation, and release validation.
- Workspace-wide isolated cold/warm results (seconds):

  | Target | Cold | Warm before caching | Warm optimized |
  | --- | ---: | ---: | ---: |
  | `synapse_fbs` | 2.265 | 0.391 | 0.397 |
  | `rumoca` | 9.430 | 1.045 | 1.045 |
  | `modelica_models` | 1.535 | 1.146 | 0.473 |
  | `csyn` | 35.525 | 1.752 | 0.539 |
  | `synapse_ppm_bridge` | 9.148 | 0.746 | 0.382 |
  | `cerebri_modules` | 36.101 | 22.681 | 0.724 |
  | `electrode_web` | 138.886 | 62.309 | 2.070 |
  | `cerebri_cubs2` | 51.457 | 3.967 | 3.881 |
  | `FastDyn` | 389.255 | failed before build | 4.762 |

  Cold uses fresh isolated workspace build/devel/task state while preserving
  shared package/download caches. Warm is an immediate unchanged repeat.

## Roadmap

### Phase 1: remove CI and clean builds from the hot path

- [x] Confirm `/dev/` is ignored.
- [x] Split repository/source dependencies from artifact build dependencies.
- [x] Replace the local Synapse `xtask ci` invocation with a development
  artifact command; retain the full command for release/CI.
- [x] Stop forcing pristine CUBS2 and RDD2 builds.
- [x] Stop running Cerebri Modules Twister validation as a CUBS2/RDD2 build
  prerequisite.
- [x] Add focused task-graph and command regression tests.

### Phase 2: reliable task caching

- [x] Add precise input fingerprints and required-output checks for generated
  Synapse and Rumoca artifacts.
- [x] Batch repository content hashing so large packages can validate cached
  outputs without spawning one checksum process per source file.
- [x] Keep firmware tasks runnable on every invocation so native CMake/Ninja can
  perform source-level incremental compilation.
- [x] Use the existing CMake/Ninja graph directly after configuration and
  return to west automatically when configuration inputs change.
- [x] Cache unchanged outputs for every currently available non-firmware build
  while including the complete transitive local-source dependency closure in
  each fingerprint.
- [ ] Add explicit force/refresh and clean escape hatches.
- [x] Put workspace-global artifacts and selections in a state root independent
  of profile-specific `DEVENV_STATE`.

### Phase 3: dependency and tool caches

- [ ] Cache pinned `flatc`, `flatcc`, FlatBuffers, and MCAP tool sources by
  version/commit instead of deleting and fetching them on each generation.
- [ ] Separate npm dependency installation (lockfile keyed) from application
  builds.
- [x] Fetch only the required QEMU branch tip for FastDyn instead of its full
  multi-decade Git history.
- [ ] Add a bounded Nix jobs recommendation and warning to `ws doctor`.
- [x] Select a standard organization binary-cache architecture: Cachix first,
  Attic if self-hosting is required, and protected CI as the only stable-cache
  writer. Retain release pins rather than overriding nixpkgs for cache hits.
- [x] Wire the public `cognipilot` Cachix cache for workspace/devenv and
  read-only pull-request CI use.
- [x] Add the cache-scoped `CACHIX_AUTH_TOKEN` organization secret.
- [x] Authorize all organization repositories as trusted public-cache
  publishers.
- [ ] Verify protected `main` CI can push.
- [ ] Publish the product flake roots once those pure outputs exist.
- [ ] Pilot shared `sccache` for mutable C/C++/Rust task builds.

### Phase 4: developer operations and guardrails

- [x] Isolate release tasks from `devel/` search paths and require clean,
  committed component inputs.
- [ ] Add `ws doctor`, `ws disk-usage`, and scoped cleanup commands.
- [ ] Make README installation instructions use the pinned devenv version.
- [ ] Ensure container test layers consume build artifacts without rebuilding.
- [x] Add a repeated-build benchmark with isolated state, retained logs,
  unavailable-target reporting, and checked warm-build budgets.
- [ ] Fix existing Nix formatting failures.

## Implementation log

- 2026-07-13: Completed the baseline review. Identified CI-in-build, forced
  pristine firmware builds, conflated dependency types, unused task caching,
  profile-fragmented state, and serialized nested Nix jobs as the primary
  constraints.
- 2026-07-13: Began Phase 1. The existing `/dev/` ignore rule satisfies the
  requested local-roadmap convention.
- 2026-07-13: Established a strict local/release boundary: `devel/` is an
  automatic, mutable local overlay; release evaluation ignores it; deployment
  promotes explicit locked Nix outputs. Avoided the ambiguous `install/` name
  and reserved `release-results/` for non-activated convenience links only.
- 2026-07-13: The first migrated CUBS2 build completed the development-only
  Synapse generator without running CI, then exposed a task-fingerprint bug on
  a symlink to a directory. Added a regression test and fixed symlink hashing.
- 2026-07-13: Added pinned generator reuse, batched task-cache hashing, and a
  guarded CMake/Ninja fast path. The second unchanged end-to-end build measured
  4.85 seconds versus the 527-second baseline.
- 2026-07-13: Added `ws benchmark`, recorded all declared local targets, and
  established checked warm-build budgets. Output-aware caching reduced the two
  largest unchanged hot paths: Electrode from 62.3 to 2.1 seconds and Cerebri
  Modules from 22.7 to 0.7 seconds.
- 2026-07-13: Repaired FastDyn's renamed Nix dependencies and changed its QEMU
  checkout to depth-1/blob-filtered fetch. Its first valid measurement is
  389.3 seconds cold and 4.8 seconds warm.
- 2026-07-13: Selected Cachix-compatible Nix closure caching for pure product
  outputs and `sccache` for mutable compiler work. This deliberately avoids a
  custom workspace cache and does not weaken per-project release pins.
- 2026-07-13: Disabled unused QEMU docs and SDL auto-detection in FastDyn's
  headless setup. The isolated result is 167.427 seconds cold and 0.270 seconds
  warm; Synapse is 73.898 seconds cold and 0.302 seconds warm.
