# nixspace

`nixspace` is a small, installable meta-client for Nix-first development
workspaces. Nix flakes remain the sole authority for package metadata,
dependency graphs, actions, launch plans, and tool inputs. `nixspace` reads the
versioned data emitted by a flake, presents it quickly, and delegates mutable
work to established tools such as West.

It deliberately does not discover project dependencies, reproduce native build
systems, or plan builds in Rust.

## Install

```console
cargo install nixspace
```

## Provider contract

A workspace wrapper supplies the exact generated index and auxiliary plan
paths through CLI options or environment variables. `nixspace` assigns no
flake output names or workspace state paths itself. The index top-level
`interfaceVersion` is `1`.

```console
NIXSPACE_INDEX=/nix/store/.../share/nixspace/index.json nixspace package list
nixspace graph --dot
nixspace build app
nixspace test app --plan --json
nixspace run app/tool -- --help
```

`build` and `test` do not calculate a dependency closure. They select exact
task roots from the generated `actionPlans` document and pass them to its
declared generated-task runner (the Devenv flake task helper in this
workspace), which owns task dependencies.
Their output streams unchanged and their exit status is the runner status.
`--plan` prints that generated selection without running it.

When an editable cache is desired, refresh performs one exact build using the
explicit flake output and generated-file location:

```console
nixspace --index .state/index.json index refresh \
  --flake . \
  --index-installable workspace-index \
  --index-file share/example/index.json
```

It validates the generated interface before atomically replacing the explicit
`--index` path. An existing cache is preserved on every failure.

The same inputs can be supplied for a remote workspace:

```console
nixspace --workspace-root ../product \
  --index /tmp/product-index.json \
  index refresh \
  --flake github:example/product \
  --index-installable workspace-index \
  --index-file share/example/index.json
```

Equivalent environment variables are `NIXSPACE_WORKSPACE_ROOT`,
`NIXSPACE_INDEX`, `NIXSPACE_FLAKE`, `NIXSPACE_INDEX_INSTALLABLE`, and
`NIXSPACE_INDEX_FILE`. West plans use `NIXSPACE_WEST_PLAN`,
`NIXSPACE_WEST_CACHE`, and `NIXSPACE_WEST_VIEW_ROOT`.

Editable source operations are also plan-driven:

```console
nixspace --source-plan /nix/store/.../share/nixspace/source-plan.json sync
nixspace status application
nixspace update application
```

Nix precomputes package-to-repository selections and every Git argv. `sync`
preflights every path before cloning; `update` verifies every worktree before
fetching and proves the entire selection fast-forwardable before the first
merge. Git retains all repository semantics.

Host setup is also data-driven. A flake may emit a versioned `Host` document
with its minimum Nix version, expected Nix settings, and pinned external tool
commands:

```console
nixspace --host-plan /nix/store/.../share/nixspace/host-plan.json doctor
nixspace --host-plan /nix/store/.../share/nixspace/host-plan.json cache coverage --json
nixspace --host-plan /nix/store/.../share/nixspace/host-plan.json setup --check
nixspace --host-plan /nix/store/.../share/nixspace/host-plan.json setup
```

`doctor` is read-only. `setup` replaces only the marked nixspace block in
`nix.conf`, preserves a one-time `.pre-nixspace` backup, restarts a daemon only
after a change, and invokes only install commands declared in the Host plan.
It refuses NixOS and symlink-managed configuration. Declaring
`trusted-users = *` is reported explicitly as root-equivalent access for every
local user. `NIXSPACE_HOST_PLAN`, `NIXSPACE_NIX_CONFIG`, and
`NIXSPACE_NIX_MODE` provide equivalent configuration overrides; the CLI flags
select the exact generated inputs. Host interface v4 may also
name already-realized store-root out-links and credential-free public cache
URIs, with an explicit union coverage policy. Doctor skips absent roots without
evaluating Nix or contacting a cache. For present roots, cache report v2 retains
each store's path/NAR/download diagnostics and adds per-root union hit/miss
metrics. A root is complete when every local closure path is available from at
least one observed declared store. Union `cacheDownloadBytes` sums the smallest
advertised compressed size for each covered path and is `null` if no observed
hit advertises a size for one or more covered paths. Actual transferred bytes
remain unknown because stable Nix inventory JSON does not observe an earlier
transfer.

An optional versioned `SourceWorkspace` plan supplies editable checkout paths,
package selections, repository identity expectations, and every Git argv:

```console
nixspace sync                 # clone only missing repositories after preflight
nixspace status application  # stream the emitted status commands
nixspace update application  # preflight, fetch all, prove ff, then ff-only merge
```

`nixspace` does not calculate a repository graph. It selects the exact emitted
public `default`, `all`, or package list. Omission selects `default`; explicit
`all` may include private sources and must be treated as an opt-in. Update
validates every selected worktree, origin, branch, and clean tree before
networking; it fetches the full selection and proves every fast-forward before
changing any checkout. Configure the plan with `--source-plan` or
`NIXSPACE_SOURCE_PLAN`.

Launch execution is delegated to the exact process-compose commands emitted by
Nix:

```console
nixspace --launch-plan /nix/store/.../share/nixspace/launch-plan.json \
  launch up app/stack --name demo --detach --set port=9000
nixspace launch sessions
nixspace launch status demo
nixspace launch down demo
```

The generated plan selects the session state root, socket/log placeholders,
parameter environments, Nix-declared fixed/automatic port policy, and
session-relative state bindings. Automatic allocation is serialized and the
chosen port remains reserved through manager startup. Session metadata
contains the redacted resolved plan and exact manager commands, never secret
values. Successful shutdown must release recorded ports before owned state is
removed. Process-compose owns supervision; `nixspace` only starts it and
routes session operations to its recorded socket. Configure the plan with
`--launch-plan` or `NIXSPACE_LAUNCH_PLAN`.

Performance gates are likewise Nix-owned. `BenchmarkPlan` v3 declares exact
one-time `setup`, per-sample `beforeEach`, `measure`, and `afterEach`, and
one-time `teardown` command arrays. Each command contains only argv, working
directory, environment policy, timeout, and accepted exits; the case adds
bounded sample counts, reference-host context, and p50/p95 limits:

```console
nixspace --benchmark-plan /nix/store/.../share/nixspace/benchmark-plan.json \
  benchmark package-list --json
```

With no case arguments, the client runs the ordered `defaultCases` selected by
Nix. Pass exact case IDs for a focused run, or `--all` to run every case in the
plan. `--host-name NAME` records the actual reference host in the report
without changing the Nix-owned command or gate definitions.

Only the aggregate `measure` phase contributes to p50/p95. Setup and
preparation stop on failure; every `afterEach` and `teardown` command is still
attempted. A failed setup, preparation, or cleanup aborts later samples, while
a failed measurement may continue to the next sample after successful cleanup.
Every invocation writes an atomic JSON report plus separate stdout/stderr logs
and status/timing evidence for every lifecycle command, including recoverable
log-I/O infrastructure errors. Unexpected exits, timeouts, cleanup failures,
and failed gates remain in the report and produce a nonzero status. The
equivalent plan environment variable is `NIXSPACE_BENCHMARK_PLAN`.

`_run-task` validates every Nix-declared artifact after a successful native
task, runs only its emitted `nix hash path` proof argv, and publishes all proofs
atomically while declared locks remain held. It does not use those proofs as a
custom build cache or skip native tasks.

`nixspace west exec -- WEST_ARGS...` invokes the Nix-selected West executable;
`nixspace west run [--cwd RELATIVE] -- ARGV...` invokes another exact external
command in the materialized local West view. The optional working directory is
validated as a local-view-relative path.

Local path flakes must have tracked `flake.nix`, `flake.lock`, and cleanly
Git-filtered local path inputs before refresh. Remote flake references are
consumed directly by Nix.
