# Conventional Nix tool compatibility spike

Date: 2026-07-14

## Outcome

The build-free spike makes three decisions:

| Tool | Decision | Boundary |
| --- | --- | --- |
| `flake-parts` | **retain** | Root/project typed module composition and normal flake output discovery |
| Divnix Standard (`std`) | **reject for workspace discovery** | It can coexist through `std.flakeModule`, but Cells add a second discovery hierarchy and require an incompatible extra nixpkgs pin |
| `zephyr-nix` | **adopt as the Zephyr tool provider** | Selective SDK, Zephyr-derived Python environment, and supported host tools; keep product policy and extra CogniPilot Python packages outside it |
| `west2nix` | **pilot only for pure release builds** | Consume a West-resolved, hash-augmented frozen manifest; do not use it for the editable shared workspace |

No SDK, QEMU, firmware, or fixed-output source derivation was realized. The
complete proof evaluates in one command:

```console
nix eval path:./nix/spikes/conventional-tools#conventionalToolSpike --json
```

The evaluation instantiates metadata for the CUBS2 setup hook but does not run
the hook or fetch its projects. The snapshot intentionally uses zero hashes, so
it cannot accidentally become a successful build input.

## Reproducible inputs

The spike has its own [`flake.lock`](../nix/spikes/conventional-tools/flake.lock)
and does not change the production lock.

| Input | Pinned commit | Commit date |
| --- | --- | --- |
| `hercules-ci/flake-parts` | `17c9d6cdfc60c64f4ee8d306f9bc0b4ccb51481e` | 2026-07-01 |
| `divnix/std` | `4177882c378184b795fa97594b5effd062213891` | 2025-08-17 |
| `nix-community/zephyr-nix` | `6966fb1cbf2fdb494bea3062c5e8e7d44dd8ac9c` | 2026-04-21 |
| `adisbladis/west2nix` | `f84670d66f881d9340b7d7626fbfe499438c134b` | 2025-07-31 |

The primary sources inspected were those exact commits, the installed West
1.5.0 implementation, and the checked-out CUBS2 manifest/CMake files. Commit
links: [flake-parts](https://github.com/hercules-ci/flake-parts/commit/17c9d6cdfc60c64f4ee8d306f9bc0b4ccb51481e),
[std](https://github.com/divnix/std/commit/4177882c378184b795fa97594b5effd062213891),
[zephyr-nix](https://github.com/nix-community/zephyr-nix/commit/6966fb1cbf2fdb494bea3062c5e8e7d44dd8ac9c),
and [west2nix](https://github.com/adisbladis/west2nix/commit/f84670d66f881d9340b7d7626fbfe499438c134b).

## Flake-parts versus Standard Cells/Blocks

### Executable comparison

Both adapters consume the same two-project fixture:

```text
producer.outputs.generated
            |
            v
consumer.inputs.generated -> consumer.outputs.firmware
```

The flake-parts adapter declares typed module options. The Standard adapter
uses a `projects` data Block inside a `workspace` Cell and validates the same
records with Standard's Yants input. `std.flakeModule` is imported into the
same current flake-parts evaluation. The evaluated outputs compare equal.

This proves technical coexistence, including coexistence with devenv's own
flake-parts module. It does not make Cells the same abstraction as devenv
tasks: Standard's `devshells` Block integrates Numtide devshell, while the
workspace action DAG is already provided by devenv tasks.

### Code and dependency comparison

Nonblank, noncomment adapter lines in this deliberately small fixture are:

| Adapter | Lines | What those lines provide |
| --- | ---: | --- |
| flake-parts module | 37 | typed options, module merging, and a normal flake output |
| Standard Cell | 14 | Yants validation and a data Block target |

Standard appears to remove 23 adapter lines, then adds seven root declarations
for its input/import/grow/pick integration: a net toy-fixture reduction of 16
lines. That number is not a semantic reduction. The removed flake-parts lines
are the public option contract that lets independently supplied project
definitions merge, report provenance, and produce option documentation. A
Standard data Block only validates the final leaf value; reproducing the
distributed module contract brings those lines back.

The dependency cost is also concrete. flake-parts adds one locked node while
sharing the root nixpkgs library. Standard's reachable closure adds 12 concrete
lock nodes, including its own full nixpkgs. Following the workspace's current
nixpkgs into Standard was evaluated and failed in Paisano's `System` type with
`x86_64-darwin is not a member of enum System`. Retaining Standard's
release-23.11-era nixpkgs pin makes the fixture evaluate, but creates the exact
second package universe the workspace is trying to avoid.

### Decision: REJECT Standard Cells as workspace discovery

Do not add per-project Cells or Blocks. Keep one flake-parts module contract,
one normal flake output tree, and devenv tasks as the mutable DAG. Standard's
concepts remain useful prior art, but adopting the framework would trade away
module composition and add a parallel CLI/directory convention plus a second
nixpkgs pin. No production compatibility layer is warranted.

## `zephyr-nix` against CUBS2

The current flake interface evaluates with the workspace nixpkgs pin and
exports these relevant package families:

- `sdk`, `sdkFull`, and versioned SDKs for 0.16.9, 0.17.4, and 1.0.1;
- `hosttools` and versioned host-tool bundles;
- `hosttools-nix`;
- `pythonEnv`; and
- `openocd-zephyr`.

The exact evaluated names are recorded under
`conventionalToolSpike.zephyrNix.packageNames`.

### Supported boundary

- Override `zephyr-nix.inputs.zephyr` to follow the selected product's
  `flake = false` Zephyr fork. `pythonEnv` then reads that exact fork's
  `scripts/requirements.txt`, so West and Python requirements stay aligned
  with CUBS2 instead of zephyr-nix's default Zephyr 4.3.0 source.
- Use `sdk.override { targets = [ "arm-zephyr-eabi" ]; }` for the CUBS2 NXP
  firmware path. Do not put `sdkFull` on developer or CI hot paths.
- Compose CUBS2's Rumoca, Synapse, plotting, and SIL Python dependencies onto
  the provided Python environment; they are product requirements, not Zephyr
  requirements.
- `hosttools-nix` supplies nixpkgs `dtc`, OpenOCD, QEMU, `gcc_multi` on
  x86_64-linux, and related tools. That matches the 32-bit `native_sim` need,
  but also gives QEMU a potentially expensive cold closure. Keep it lazy and
  cache it; do not realize it during metadata/query checks.
- The upstream binary `hosttools` package explicitly warns that some tools can
  fail because of libc compatibility. Prefer `hosttools-nix` when its complete
  closure is wanted, or retain a narrow host-tool set for latency-sensitive
  `native_sim` shells.

This is an interface/evaluation compatibility result. A later cache-backed
targeted firmware build must still prove the CUBS Zephyr fork and board against
SDK 1.0.1 before replacing the currently working compiler selection.

### `native_sim`

`zephyr-nix` does not impose board names. CUBS2 can continue to pass
`native_sim` and `native_sim/native/64` to West with
`ZEPHYR_TOOLCHAIN_VARIANT=host`. The actual CUBS2 CMake already accounts for
Nix's multilib GCC behavior on the 32-bit board. `gcc_multi` and
`glibc_multi.dev` remain required on x86_64-linux; no SDK cross target is
needed for the hosted simulator itself.

## `west2nix` against the actual CUBS2 workspace

### Evaluated interface

The pinned flake exports `lib.mkWest2nix = { pkgs }: ...`. The resulting scope
contains `west2nix` and `mkWest2nixHook`. The CUBS2 snapshot instantiates that
hook for all ten projects and the `cerebri_cubs2` self path without realizing
it.

`mkWest2nixHook` accepts either:

- an attribute set with a frozen manifest, or
- a path to TOML, loaded with `lib.importTOML`.

It does **not** load `west.yml` directly. Its CLI performs the conventional
translation:

1. run `west manifest --freeze` in a materialized West workspace;
2. resolve remote names, imports, filters, paths, and revisions using West;
3. run `nix-prefetch-git` for each resolved checkout; and
4. write `west2nix.toml` with a Nix hash per project.

The build hook creates fixed-output `fetchgit` sources, copies each project to
its resolved path, creates a fake Git commit required by West, runs
`west init -l <self.path>`, and invokes `west build $westBuildFlags`. It does
not call `west update` during the build.

### Imports and fork precedence

The current CUBS2 `west.yml` contains no manifest imports. Its build-free
`west manifest --freeze` result has ten projects and resolves the CogniPilot
forks for Zephyr, `hal_nxp`, ZROS, CSyn, cerebri_modules, and zephyr_boards.

For a future imported manifest, West 1.5.0 remains the authority. Its actual
resolution order is:

1. projects from `self: import`;
2. projects declared by the current manifest; and
3. projects from each project's `import`.

Earlier definitions win by project name. Therefore an explicit top-level
CogniPilot fork wins over a same-named project imported later from Zephyr.
Import allow/block lists and path prefixes are also resolved before freezing.
`west2nix` preserves the flattened result, but it cannot reconstruct or audit
where that precedence came from. Keep a root policy test that compares the
frozen name/URL/revision/path set with the selected product policy.

### Modules

`west2nix` materializes paths; it does not discover Zephyr modules. CUBS2 is
compatible because its CMake explicitly supplies these eight paths in
`ZEPHYR_MODULES`, and every path in the inspected shared West closure has a
`zephyr/module.yml`:

- `modules/hal/cmsis`
- `modules/hal/cmsis_6`
- `modules/hal/nxp`
- `modules/lib/zenoh-pico`
- `modules/lib/cerebri_lockstep`
- `modules/lib/zros`
- `modules/lib/csyn`
- `modules/lib/zephyr_boards`

The `modelica_models` checkout is materialized at
`models/vendor/CMM-v0.0.2` but is not a Zephyr module. Editable local module
precedence via `EXTRA_ZEPHYR_MODULES` remains a mutable shared-workspace
feature; it is intentionally absent from a pure release derivation.

### Native simulation and offline materialization

The hook forwards arbitrary `westBuildFlags`, so both CUBS2 native simulator
board identifiers work without west2nix changes. It has no board- or runner-
specific discovery of its own.

Once every fixed-output Git source, Zephyr tool derivation, and Nix dependency
is in the local store or an approved substituter, the hook is offline: it
copies store paths and never runs `west update`. `--offline` cannot create a
missing fixed-output source, so CI must build and push the full release closure
to the `cognipilot` Cachix cache. Developers can then substitute it instead of
rebuilding QEMU, SDKs, or firmware.

### Decision: CONDITIONAL ADOPTION for pure releases

Use `west2nix` only behind a generated, reviewed `west2nix.toml` for immutable
release/CI builds. Keep the shared West repository for editable development,
fork testing, and source overrides. Promotion requires:

1. generate real hashes with the pinned CLI from the selected product's
   synchronized West workspace;
2. assert the frozen project set against product policy;
3. perform one cache-backed `native_sim/native/64` build and one selective
   `arm-zephyr-eabi` firmware build; and
4. push both complete closures to Cachix.

Do not add a CogniPilot-specific manifest resolver. West already owns import
and precedence semantics, and west2nix already owns the freeze-to-fixed-source
translation.
