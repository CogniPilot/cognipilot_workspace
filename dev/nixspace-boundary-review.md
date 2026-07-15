# nixspace boundary review

Status: multi-agent audit completed and surviving findings resolved, 2026-07-14.

Method: 16 independent auditors (11 per-module + 5 cross-cutting lenses) reviewed
the `nixspace` Rust crate and the Nix modules against the boundary contract in
[AGENTS.md](../AGENTS.md) and [nix-rust-workflow-boundary.md](nix-rust-workflow-boundary.md).
Every raised finding was then adversarially verified (two skeptics for
high/critical, one otherwise) instructed to refute it. 35 findings were raised;
**2 survived** verification, 33 were refuted, and 80 compliant notes were
recorded. The audit snapshot was ~16,931 lines across 12 `src/*.rs` modules;
the strict deletion leaves 16,423 lines.

## Resolution

Both surviving findings were removed in the same strict cutover. The local
snapshot branch, Git inspection, `flake.lock` parsing, flake-reference
classification, percent encoding, and `git+file` coordinate synthesis were
deleted from `index.rs`; there is no compatibility path.

`nixspace index refresh` now accepts one complete opaque `--installable` (or
`NIXSPACE_INDEX_INSTALLABLE`) and passes it unchanged as one process argument:

```text
nix build --no-link --print-out-paths -- <installable>
```

Nix therefore owns flake syntax, Git/path filtering, locking, subdirectory
selection, and source identity. Rust retains only the permitted transport
boundary: run one build, require one output, validate the versioned generated
index, and atomically cache its exact bytes. A local installable intentionally
uses Nix's editable flake semantics; immutable promotion evidence must provide
an immutable Nix reference.

Rust integration tests poison `git` and prove refresh never invokes it, record
every Nix argument separately, prove opaque references containing `?rev=`,
`&dir=`, and `#` remain one unchanged argument, and reject the removed
`--flake`/`--index-installable` interface. A Nix-evaluated source-boundary guard
also rejects reintroduction of the deleted synthesis tokens and entry points.
Workspace tests remain exclusively Nix and Rust.

## Executive verdict

**Grade after remediation: thin frontend, with no surviving boundary
violation.**
The crate is overwhelmingly a faithful presentation/coordination frontend over
Nix-generated data: across every audited module the dependency graph, closures,
orderings, command scopes, and candidate selections arrive from Nix pre-computed
as data and are merely selected, validated, rendered, or executed — never
computed. Measured on graph/closure/build-engine computation the crate is at
**0% forbidden after the cutover** — `resolution.rs` refusing all fallback
(`candidate_for`, "fallback is forbidden") is the sharpest proof that Rust
cannot decide anything Nix did not pre-decide. The audit found one genuinely forbidden behavior:
source-coordinate synthesis in `index.rs`. That implementation and its tests
have now been deleted, and the replacement transports one opaque installable
to Nix. Product-agnosticism remains intact (zero CogniPilot identifiers in
`src/`) and Nix is the sole semantic authority. Residual maintainability work
is the intra-Nix devenv tool-pin trio and broader negative guards against every
possible future form of second-engine code; neither is an active boundary
violation.

## Survived findings at audit time (resolved)

| Severity | Module | Finding | Contract clause | Location |
|---|---|---|---|---|
| High | index | Synthesizes `git+file` flake URLs by inspecting a Git worktree — the explicitly-deleted `workspace-flake-ref` behavior | Clause 5 (source-URL reconstruction) + boundary ledger line 84 | `tools/nixspace/src/index.rs:248` |
| Low | index | Classifies flake-reference syntax (`path:` / `./` / `../` / `/abs`) to branch local-vs-remote, feeding the forbidden snapshot path | Clause 3 (interpreting Nix flake-reference grammar) | `tools/nixspace/src/index.rs:663` |

Only these two findings survived adversarial verification. Every other flagged
concern was refuted (see "What was checked and cleared").

## Per-module assessment

### index — clean after strict cutover

The index-materialization core is an exemplary thin frontend: `refresh` shells to
`nix build --print-out-paths`, runs the bytes through the shared `decode_index`
version gate, and atomically writes the Nix-built bytes verbatim — the textbook
clause-6 "materialize/cache the exact output, do not recompute" pattern:

```rust
decode_index(&bytes, &generated.display().to_string())...?;
atomic_replace(destination, &bytes)?;
```

At audit time, `prepared_flake_reference` / `snapshot_repository` /
`local_path_inputs` / `file_url` / `local_flake_root` formed the sole real
drift. That entire branch is deleted. The current boundary is exactly:

```text
nix build --no-link --print-out-paths -- <opaque-installable>
```

The client performs no source inspection or syntax classification. Nix owns all
local, remote, Git, path, lock, and subdirectory semantics.

### resolution — strongly thin (~85% legit)

The local-vs-locked decision, command scopes, dependency/reverse closures, and
override/refusal policy are **all** computed by `resolution-template.nix` and
merely read here. The candidate selection is a bare map lookup that refuses
fallback — the strongest single piece of thin-frontend evidence in the crate:

```rust
self.scope.selected_candidates.get(package).copied().ok_or_else(|| CliError(format!(
    "package `{}` command scope does not explicitly select candidate `{package}`; fallback is forbidden", ...)))
```

Scope itself is chosen by the Nix `selectedScope` field, stale local generations
are a **hard error** (not a re-route to locked), and `run` refuses a mixed
local/locked selection rather than reconciling one. `validate()` cross-checks
closure invariants (the selected scope must **exactly cover** the
`dependencyClosure`, i.e. `closure == scope_selections`; `compiledReverseClosure`
⊆ `reverseClosure`) but only to reject inconsistent Nix output — it builds
transient `BTreeSet`s and asserts equality/subset, never traversing adjacency.
No clause-2/clause-6 selection or closure computation.

### host — strong (~90% legit)

`ws doctor`/`setup`/`cache-coverage` read expected cache URLs,
signing-key-bearing substituters, store names, thresholds, and tool versions
entirely from the deserialized `HostPlan` (owned by `host-module.nix` +
`cache-policy.nix`); no CogniPilot identity is literal in Rust. The cache closure
comes from the native tool, not Rust:

```rust
let local_arguments = vec!["nix","path-info","--json","--recursive","--size", path_text];
// "must never evaluate or realize a flake output just to obtain cache statistics"
```

`render_configuration` is a thin `key = value` serializer of
`loaded.plan.nix.settings`. Plan validation cross-checks store URIs against the
plan's own substituters rather than inventing policy. The residual ~10%
(store-path hash format, daemon socket path, default `nix.conf` locations,
daemon-restart service names) re-encodes generic Nix infrastructure knowledge —
a mild coupling risk, but not CogniPilot-specific and not a semantic-authority
leak (all refuted as violations).

### source — strongly thin (~90-95% legit)

Every git action (clone/fetch/status/merge-base/merge --ff-only/rev-parse/
update-ref) is a Nix-declared argv the module simply spawns. The two forbidden
temptations are correctly delegated: fast-forward eligibility runs the
Nix-generated `fast_forward_check` argv (`merge-base --is-ancestor`), and origin
identity is compared against the Nix-declared URL rather than reconstructed:

```rust
if origin != repository.git.url {
    return Err(CliError(format!("repository `{}` origin mismatch: expected `{}`, got `{origin}`", ...)));
}
```

The module's value-add is a robust local transaction (file-locked guard, fsynced
journal, RAII, conditional two-tree rollback) that the contract explicitly
permits. The one genuinely semantic item — parsing git porcelain unmerged status
codes (`source.rs:234`) to gate readiness — was **refuted, but not cleanly**:
one verifier held only medium confidence, since Rust does encode a fixed set of
git conflict codes rather than reading them from the plan. The refutation stands
(reading git's own frozen `--porcelain=v1` report is allowed native-tool
coordination, not merge-algorithm reimplementation), but treat it as a residual
medium design tension, not a settled non-issue.

### action — clean core (~85% legit)

`run_action` purely **selects** among Nix-emitted ordered task lists and runner
argvs — no `add_target`, no closure accumulation, no topological sort, no
build-system emulation:

```rust
let tasks = if let Some(package) = canonical { selection.packages.get(package)...? } else { &selection.all };
```

Runner argv and task IDs execute in the exact Nix-emitted order; the output
digest is delegated to the Nix-declared proof command
(`nix hash path --type sha256 --sri`), not reimplemented. The ~15% that drifts —
`run_task`'s output-proof + generation-store machinery — was scrutinized against
the deleted `workspace-cached-task` scripts and **refuted**: the layout and proof
command are Nix-declared, `command.status()` runs the task unconditionally (no
cache-hit short-circuit), and the store is a write-only post-hoc provenance
record. Incremental/skip validity stays in devenv's native content-hash cache.

### launch / launch_exec — strongly thin (~90% / ~85% legit)

`launch.rs` turns one already-selected Nix `LaunchPlanTemplate` into a resolved
plan for presentation: it parses `--set NAME=VALUE`, enforces Nix-declared
per-parameter constraints (type, enum, min/max, port range, path allow-roots),
and renders — no process-set or graph computation. `launch_exec.rs`'s `up` execs
the exact Nix-generated manager argv with no supervision loop of its own
(`command.exec()`, unix execvp, no retry loop). The process-set merge stays in
Nix (`devenv-launch-renderer.nix mergeUnique`); Rust reads the flattened resolved
process/endpoint set. The `probe` subcommand is a single-shot TCP/HTTP hook Nix
wires into process-compose, not a duplicate supervisor. Automatic ports are
chosen via OS ephemeral bind (`bind_port(transport, host, 0, …)`), not selected
from a Nix list. The borderline items — a path-root containment check, an
in-memory cross-session port-claim map, and the `127.0.0.1` default host — were
all refuted; the host default deterministically reproduces the identical
`127.0.0.1` literal Nix itself emits for non-host-parameterized endpoints
(`devenv-launch-renderer.nix:424-425` explicitly sanctions the client testing
"these exact defaults").

### closure_materialization — compliant (~90% legit)

The hidden `_materialize-closure` subcommand materializes closures, it does not
compute them: Nix's `exportReferencesGraph` produces the graph, and Rust
validates, normalizes (sort/dedup for stable serialization), joins by store path,
re-encodes a Nix-provided hash, and atomically serializes. References are copied
through verbatim:

```rust
fn document(&self) -> Value { serde_json::json!({ "path": self.path, "narHash": ..., "references": self.references }) }
```

No reachability walk, no topological sort, no completeness computation. The only
note — auto-discovering proof targets by scanning the document for `drvPath`
rather than an explicit declared list — was refuted as low-severity structural
coupling: Nix authors the `drvPath` keys, so Rust invents nothing.

### west — strong thin frontend (~80% legit)

Manifest/project resolution is delegated to native `west list -f {name}|{path}`
and `west manifest --freeze`; module policy comes from Nix `zephyrModule` flags;
`contentKey`/`policyId` are read from Nix, not recomputed. Store ingestion runs a
fully Nix-declared typed argv. The suspected drift — hand-writing `.west/config`
with a hardcoded `[zephyr] base = zephyr` semantic (`west.rs:1716`) — was flagged
then **refuted**: writing a minimal bootstrap config so native west can operate
on a Nix-store-backed immutable checkout is materialization glue (the dedicated
design doc assigns this step to Rust), `zephyr` is a generic Zephyr constant (not
a CogniPilot token), and native west retains all resolution. Worth noting the
verifiers were split — one pass would have kept a medium-severity finding on the
`[zephyr] base` literal not flowing through the versioned plan. It is a
legitimate design nit, not a boundary violation.

## Product-agnosticism (generic crate)

**Essentially fully compliant.** A full grep of `src/` for every CogniPilot
marker (`cognipilot`, `COGNIPILOT_`,
`cerebri`/`synapse`/`zros`/`csyn`/`rumoca`/`electrode`/`fastdyn`/`modelica`/`qualisys`,
`cachix`, GitHub org, cache URLs/keys) returns **zero** hits (the only near-match
is "recognized" in `source.rs:750`). Every product coordinate — complete index
installable, index file, and all plans — is a required CLI flag or
`NIXSPACE_*` env var that hard-errors when absent rather than defaulting to a
CogniPilot value. Cache/substituter/key expectations are read from
`loaded.plan.nix.settings`. On-disk state uses the crate's own generic
`.nixspace-*` prefix. `Cargo.toml` is generic and independently publishable
(`name = "nixspace"`, `documentation = "https://docs.rs/nixspace"`, no path/git
deps into `src/`, no CogniPilot org). The only product-adjacent strings are test
fixtures (a `CACHIX_AUTH_TOKEN` leak-guard sentinel `"must-never-appear"`, and
placeholder `cache.nixos.org` / `cache.example.test` URIs). The lone genericity
note is `DEVENV_TASK_OUTPUT_FILE`, which couples the crate to devenv — an
expected first-class owner in the boundary — not to CogniPilot.

## Two-authority drift

On the Nix side the single-authority discipline is mostly good and, importantly,
**the Rust client is not a second author** of these facts — it materializes the
Nix-generated index/plans, so there is no Nix↔Rust drift. The
substituter/public-key lists are triple-authored (flake `nixConfig`,
`cache-policy.nix`, host plan) but every mirror is pinned by
`testCachePolicyIsSingleAuthority`, which reads the real values and compares to
golden literals (including the `builtins.tail` relationship for the flake copy).
Cache stores/roots, trusted-users, host `interfaceVersion`, and benchmark
`defaultCases` are similarly golden-guarded.

The **one genuine weakness** is the devenv tool-pin trio: the exact devenv commit
is authored twice (flake input `url` and `cache-policy.nix` `installArgv`), the
version string once more (`expectedVersion = "2.1.2"`), and **none of the three
are tied together by a check**. This is intra-Nix duplication (all loci are
NIX-owned, so it is not a cross-owner boundary violation, and the finding was
refuted *as a boundary-contract finding*), but it is a real drift risk requiring
manual sync. Note also `nativeWarmBudgets`/per-case budget numbers are
single-authored but their values are unguarded — the test only checks
`any (hasPrefix "native-warm-")`, not the numbers.

## Nix-side integrity / hidden shell

Nix genuinely is the sole authority and it is not inverted. Roughly 95% of the
Nix code is legitimate declarative data/plan generation: `flake-module.nix`
computes the package/action/artifact graph and transitive closures in pure Nix
(`builtins.foldl'` fixpoints over `dependsOn`/artifact edges, cycle detection),
and the normalized index is `builtins.toJSON` of an attrset. Every shell string
found is a tiny store-materialization or single-command exec adapter (`_run-task`,
`_probe`/`run`, `_materialize-closure`, `cat`, `cp`/`mkdir`, `jq -e` shape
checks). There is **no** multi-step bash program, **no** shell-level
graph/cache/closure computation, **no** runtime preflight, **no** shell loop, and
**no** nested `bash -c` in production Nix. The only Nix→Rust invocations at
evaluation time are validating contract checks that consume already-Nix-generated
data, so the Rust client never computes the index/graph that Nix then trusts.
Densest adapters are the index-interface check and the west-plan `jq` predicate —
both single-command assertions within the documented "tiny adapter" allowance.

## Contract enforcement gaps

Four of six clauses are strongly enforced by named automated checks:

- **No CogniPilot leak in the generic crate** — `testReusableNixspaceContainsNoProductPolicy`
  scans `nix/nixspace`, `tools/nixspace/src`, `tools/nixspace/tests` for product names.
- **Host-plan / flake `nixConfig` parity** — `testCachePolicyIsSingleAuthority`.
- **Index/plan parity, no second workspace model** — the `index.rs` exact-bytes
  test asserts cached bytes equal the Nix-generated index verbatim, run under
  `nix flake check` (`tool-module.nix` `doCheck = true`), plus the
  `nixspace-interface` check.
- **Only bootstrap shell checked in** — `cognipilot-workspace-policy` over the
  real git-filtered source (allowlist `[".envrc" "setup" "ws"]`, test-pinned).

**Two clauses retain partial anti-regression coverage, not active violations:**

1. **No second-engine code in Rust** has positive behavioral coverage plus a
   Nix-evaluated negative guard for the source-coordinate synthesis vocabulary
   and deleted index entry points. A future graph planner expressed with
   unrelated vocabulary still relies on architectural review and behavioral
   contract tests.
2. **No hidden multi-step shell in Nix** is enforced for generated devenv tasks
   (single-line data-only exec, newline rejection) but hand-written
   `runCommand`/builder strings in the workspace's own modules are never scanned —
   governed only by a manual disposition ledger.

## What was checked and cleared

The audit was adversarial: **many** plausible concerns were raised and then
refuted on the evidence. The reader should trust the two survivors precisely
because so much was cleared:

- **west** — three separate findings refuted: hand-writing `.west/config` +
  `[zephyr] base` (materialization glue, generic Zephyr constant), "reimplements
  `west init`" (native west retains all resolution), and hardcoded west
  subcommand argv (native-tool coordination is explicitly allowed).
- **resolution** — proof re-derivation (delegated to Nix-declared
  `nix hash path`), the 60s timing tolerance (validates the crate's *own*
  wall-vs-monotonic clock manifest, not a Nix record), and coordinate-grammar
  validation (cross-checks redundant Nix fields, never constructs lookup keys).
- **action** — content-hashing outputs / generation store (Nix-declared layout +
  proof, no cache-hit short-circuit) and hardcoded devenv/nix protocol constants
  (pinning your own protocol's version/kind identifiers, none CogniPilot-specific).
- **host** — store-path hash format, daemon socket path, `nix.conf` locations,
  daemon-restart service names (all generic Nix infrastructure, not CogniPilot
  leaks).
- **source** — git porcelain conflict codes (reading git's frozen scripting
  interface — refuted at medium confidence; see the source section) and
  40/64-hex object-ID validation (explicitly-permitted defensive value validation).
- **launch/launch_exec** — path-root containment (validates a user CLI value
  against a Nix list), the port-claim map, and the `127.0.0.1` default
  (reproduces the identical Nix-emitted default).
- **index** — the git-plumbing snapshot itself (atomic materialization around
  native git) and flake.lock path-input reading (a snapshot-completeness safety
  guard over Nix-resolved data) were refuted; only the URL *synthesis* and its
  gating classifier survived.
- **benchmark** — the duplicated `MAX_SAMPLES_PER_PHASE = 1000` (a
  self-protection executor ceiling, though a real maintainability nit), the
  p50/p95 gate vocabulary, and the percentile estimator (measurement methodology
  is the runner's own domain).
- **main-model** — `validate_index_relationships`, `coordinate_matches`, and
  `_complete` vocabularies (integrity validation / CLI presentation over
  Nix-generated data, not graph computation).
- **two-authority** — the devenv commit/version duplication and cachix pull-name
  were refuted *as boundary-contract violations* (all loci NIX-owned) while
  remaining flagged as intra-Nix DRY nits.

## Recommendations (prioritized)

1. **[Resolved] Delete local-flake source-coordinate synthesis from
   `index.rs`.** The whole implementation and its compatibility surface are
   gone; refresh now passes one complete installable unchanged to Nix.

2. **[Partially resolved] Add anti-regression static guards.** The Nix module
   now rejects source-coordinate synthesis and deleted index entry points in
   shipped Rust. Broader graph/closure vocabulary and hand-written Nix builder
   auditing remain maintainability improvements rather than current defects.

3. **[Medium — real drift risk] Tie the devenv tool-pin trio to a single
   source.** Derive `cache-policy.nix` `installArgv` and `expectedVersion` from
   the flake input `rev`/version (or add a golden contract test asserting all
   three agree), so the pin cannot silently diverge. This is the one place today
   where a fact is authored multiple times with no guard.

4. **[Low — reduce coupling] Push residual Nix-infrastructure constants into the
   plan where cheap.** The store-path hash format, daemon socket path, `nix.conf`
   locations, and daemon-restart service names in `host.rs`, the git conflict-code
   set in `source.rs`, and the west `[zephyr] base` literal are all generic (not
   violations) but re-encode semantics the plan could carry. Where a versioned
   interface field is inexpensive, prefer sourcing them from the plan over
   embedding them in the generic crate.

5. **[Low — maintainability] Extend golden-tests to the unguarded benchmark
   budgets and de-duplicate the `1000` sample bound.** The `nativeWarmBudgets`
   package set and numeric budgets, and the twice-authored
   `MAX_SAMPLES_PER_PHASE`/`maxSamples = 1000`, are correct today but require
   manual sync; a golden test (budgets) and a single shared constant (sample
   bound) remove the drift risk.
