# Workspace performance baseline

Status: historical baseline from the pre-cutover task implementation. These
numbers are useful comparison data, not proof of the Nix-generated task and
Rust `nixspace` architecture.

The measurements were recorded on 2026-07-13 on host `xps15`, from workspace
revision `95dd34a66284f487c2e400fe59ad0f9ffecc5dbb` plus the then-uncommitted
changes described in `devenv-performance-roadmap.md`. Raw reports and phase
logs are under `dev/benchmarks/`.

## Measurement terms

- **Workspace-cold** meant fresh isolated legacy workspace build/artifact/task
  state while shared Nix, Cargo, npm, pip, Git/download, and system caches
  remained populated. It was not machine-cold or an empty Nix store.
- **Warm/hot** meant an immediate unchanged invocation using the same mutable
  native build state.
- Measurements were sequential wall-clock times. Cold numbers were sensitive
  to host load, network state, and shared cache contents.

The former Python/Bash benchmark implementation is gone. The current
`ws benchmark` command is the generic Rust client consuming a strict
Nix-emitted `BenchmarkPlan`; it times only the exact commands in that plan and
does not discover packages or restore workspace orchestration.

## Recorded baseline

| Target | Workspace-cold | Warm/hot | Historical warm budget |
| --- | ---: | ---: | ---: |
| `synapse_fbs` | 2.265s | 0.397s | 1.0s |
| `rumoca` | 9.430s | 1.045s | 2.0s |
| `modelica_models` | 1.535s | 0.473s | 2.0s |
| `csyn` | 35.525s | 0.539s | 2.0s |
| `synapse_ppm_bridge` | 9.148s | 0.382s | 2.0s |
| `cerebri_modules` | 36.101s | 0.724s | 2.0s |
| `electrode_web` | 138.886s | 2.070s | 4.0s |
| `cerebri_cubs2` | 51.457s | 3.881s | 6.0s |
| `FastDyn` | 389.255s | 4.762s | 8.0s |

The sequential totals were 673.602 seconds workspace-cold and 14.273 seconds
warm. FastDyn's one-time QEMU build dominated the cold total. Excluding
FastDyn, the totals were 284.347 seconds cold and 9.511 seconds warm.

The complete non-FastDyn run is
`dev/benchmarks/20260713T204902Z.json`; FastDyn's repaired run is
`dev/benchmarks/20260713T210125Z.json`.

## Current Nixspace lightweight matrix

The retained 2026-07-14 lightweight BenchmarkReport v1 is historical protocol
evidence generated from the then-current BenchmarkPlan v2 on host `storm`
(AMD Ryzen 9 5950X, 32 logical
CPUs, Linux `x86_64`) with measured Nix 2.34.8 and outer plan evaluator Nix
2.34.7. Each case has seven measured samples and, except for the explicitly
evaluator-cold case, one warmup. The report records the exact root lock SHA-256,
both Nix versions, system, cache-state definition, argv, p50/p95 gates, and
per-sample logs at `dev/benchmarks/nixspace/20260714-final-v3.json`.

| Nix-emitted case | p50 | p95 | Gate |
| --- | ---: | ---: | ---: |
| completion backend | 2.239ms | 2.261ms | 100ms |
| graph plan | 4.351ms | 4.364ms | 200ms |
| help | 2.231ms | 2.233ms | 100ms |
| launch list | 4.350ms | 4.364ms | 100ms |
| launch plan | 4.357ms | 4.389ms | 200ms |
| fully serialized 100-project index | 270.609ms | 271.581ms | 1s |
| package list | 4.360ms | 5.490ms | 100ms |
| selected-flake evaluation, evaluation cache disabled | 250.373ms | 251.715ms | 10s |
| editable default-shell derivation evaluation | 11,470.046ms | 11,581.823ms | 15s |
| setup-installed `./ws` build-plan dispatch | 7.533ms | 8.674ms | 500ms |

The scale fixture also fully serializes 1, 15, 50, 100, and 200 projects in a
focused evaluation test. These results cover cached native queries and the
pure module/index scale gate. The selected-flake case disables Nix's evaluation
cache but deliberately retains immutable input/store warmth; the shell case
evaluates the PWD-bound editable derivation without realizing it. Neither
result implies an editable build or a machine-cold Nix store.

The `./ws` row is retained with its own exact logs and argv in
`dev/benchmarks/nixspace/20260714-ws-dispatch.json`; all other rows come from
the v3 report above. The ordered plan defaults include this dispatch case and
exclude the two evaluator-heavy cases, so ordinary `./ws benchmark` remains
quick. The current BenchmarkPlan v4 also contains opt-in `native-warm-*` cases
for every declared non-QEMU warm budget. Run those from the generated devenv
shell so dispatch stays on the direct task path. Explicit case IDs select only
the requested rows; `./ws benchmark --all` includes both evaluator-heavy and
native cases. FastDyn/QEMU is deliberately not in that plan. Version 4 also
carries the Nix-owned maximum sample count consumed by the Rust runner.

## Current strict lifecycle matrix

BenchmarkPlan v3 adds one-time setup/teardown plus per-sample before/measure/
after phases. The retained BenchmarkReport v2 is historical protocol evidence;
it records every lifecycle command and
infrastructure error while including only the aggregate measure phase in the
percentiles. All four focused cases and all eight p50/p95 gates passed on
`storm` with seven measured samples after one warmup; exact commands and logs
are retained at `dev/benchmarks/nixspace/20260714T195127Z-lifecycle-v3.json`.

| Nix-emitted lifecycle case | p50 | p95 | Gate |
| --- | ---: | ---: | ---: |
| dependency-free Cargo implementation edit | 225.007ms | 226.218ms | 1s |
| artifact interface-major edit | 329.831ms | 337.393ms | 2s |
| Zephyr board-variant plan switch | 329.782ms | 333.887ms | 2s |
| generated process-compose start/readiness | 172.664ms | 175.682ms | 2s |

The portable lifecycle cases are emitted for `x86_64-linux`, `aarch64-linux`,
and `aarch64-darwin`; process-compose start/readiness remains Linux-only. The
retained timings above are from `x86_64-linux` only and do not claim a completed
remote cross-platform performance run. The deferred full matrix means real
native-task performance, not the three-system public-root correctness matrix.

The Cargo case invokes a typed ActionTask v3 twice around a real source edit
and validates its declared executable artifact and NAR proof. The interface
and variant cases re-evaluate the actual CogniPilot module authority. The
launch case uses the generated launch execution plan, the pinned devenv
process-compose manager, native manager readiness, and the ordinary nixspace
launch session boundary. These are small control-plane fixtures; they do not
claim the five-pilot warm native-build matrix or a real product deployment.

## Current native task and compiler-cache evidence

After replacing the oversized inline Devenv task environment with its
Nix-generated task file, `synapse_fbs:default:build` completed once in 76.61s
(91.01s including shell evaluation) and was then served by Devenv's declared
input cache in 0.62s, 0.61s, and 0.61s on `storm`. The unchanged samples give
0.61s p50 and 0.62s p95 against the 1s historical gate. This is one real pilot
result, not the complete native matrix.

The shared Cargo cache was separately replayed after cleaning the fixture
target between builds. Nix-selected sccache 0.16.0 recorded one Rust miss, one
Rust hit, and one write in `.nixspace/state/sccache`. CI declares the same
clean miss-to-hit check over the conventional GitHub Actions backend and
retains native sccache JSON; that remote backend is not claimed until the
workflow runs.

The first `synapse_ppm_bridge` mutable qualification completed both the legacy
debug and release actions before that duplicate release path was deleted in
favor of the pure Nix release. The final single editable action was cached in
1.35s, 1.35s, and 1.34s (1.35s p50/p95). Its gate is now 2s because the
correctness-preserving Devenv input scan and typed result path replace the old
single-script 1s assumption; deployable release timing is measured separately
as a Nix derivation.

## What still needs proof

Do not run cold QEMU or the full matrix during ordinary implementation checks.
The roadmap deliberately leaves these expensive gates open:

- an unchanged default-product graph under the generated devenv task runner;
- real-product launch multi-instance isolation and cleanup;
- protected-main Cachix publication and empty-host substitution; and
- the complete warm matrix after atomic legacy task deletion.

The Nix plan now owns the complete non-QEMU budget map and exact commands; the
remaining gap is executing and retaining that opt-in matrix, not rebuilding a
second benchmark orchestrator.

New results must name the host, exact flake lock, Nix version, cache state,
task/launch coordinate, sample count, and p50/p95. Mutable native build results
and immutable Nix substitution must be reported separately. A public Cachix hit
does not imply a native Cargo/CMake/west hot-path hit, and the reverse is also
true.
