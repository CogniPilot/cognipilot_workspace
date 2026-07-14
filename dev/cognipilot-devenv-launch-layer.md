# CogniPilot devenv launch layer

The launch layer renders normalized launch IR directly into the pinned devenv
process options. Devenv remains the integration surface and process-compose
remains the supervisor. CogniPilot does not generate a second supervisor
configuration, process loop, or secret store. `nixspace` may keep redacted,
runtime-only session records so a new terminal can address the exact
process-compose Unix socket; it never supervises a process.

## Flake integration

Import the exact pinned devenv flake-parts module, the CogniPilot contract
module, and `nix/cognipilot/devenv-launch-module.nix`. A product root normally
imports `nix/cognipilot/devenv-workspace-module.nix`, which enables all launches
and installs the standalone client into every generated shell without project
boilerplate. A lower-level composition can instead enable one option directly:

```nix
cognipilot.devenvLaunches.enable = true;
```

That exports every normalized launch. A composition root may select a smaller
set without changing any project definition:

```nix
cognipilot.devenvLaunches.launches = [ "app:router" ];
```

For `app:router`, the conventional outputs are:

- dev shell and app: `launch-app--router`;
- generated upstream YAML: `launch-app--router-config`;
- evaluation check: `launch-app--router-config`.

The root also emits one `nixspace-launch-plan` package. It contains the exact
Nix-built process-compose executable, generated config, foreground/detached,
attach/status/log/down argv, parameter contracts, and a root-selected session
state path for every launch. Dynamic socket/log paths are typed placeholders
filled by the client; no Nix evaluation is needed when runtime values change.

For example, `nix run .#launch-app--router` runs devenv's own
`procfileScript`; it does not run a CogniPilot wrapper. Empty projects add no
outputs. Output derivations are independent, so selecting one config does not
realize unrelated configs.

## Executables and parameters

The default process command is one safely escaped invocation:

```text
nixspace run package/executable -- ARG...
```

The standalone `nixspace` runtime client resolves the exact executable artifact
path from Nix-generated data. It does not invent local-versus-locked provenance
and must not link workspace project implementations. A composition root can
optionally set `executableBindings."package:executable"` to an
immutable `/nix/store` path; mutable-path overrides are rejected and package
modules never repeat this wiring.

Every runtime parameter becomes a stable environment reference such as
`$COGNIPILOT_PARAM_APP_ROUTER_PORT`. The generated Nix and process-compose
configuration contain parameter names and non-secret declarations, never
runtime values. Secret parameters remain environment-only and their metadata
always has a null default.

Parameter defaults and validation, runtime secret lookup, automatic port
allocation, and session record lifecycle are runtime responsibilities declared
by the version-3 generated execution plan. Automatic ports have a Nix-selected
host, transport, and preferred value; the client serializes and reserves them
until process-compose starts. The plan also records each process's
`required`, `onExit`, and `onReadinessLoss` policy; the client delegates start,
attach, status, logs, and shutdown to its exact process-compose commands.

## Pinned devenv 2.1.2 mapping

| Launch IR | Pinned devenv/process-compose field |
| --- | --- |
| executable and argv | `processes.<name>.exec` |
| environment | `processes.<name>.env` |
| working directory | `processes.<name>.cwd` |
| started/ready/completed dependency | `processes.<name>.after` |
| successful-completion dependency | `process-compose.depends_on` escape hatch |
| endpoint readiness | `processes.<name>.ready.exec` calling a typed one-shot `nixspace _probe tcp` or `_probe http` |
| restart policy and maximum | `processes.<name>.restart` |
| restart backoff | `process-compose.availability.backoff_seconds` escape hatch |
| shutdown signal and timeout | `process-compose.shutdown` escape hatch |

Backoff and timeout milliseconds round up to process-compose's integer seconds.
TCP readiness means exact one-shot reachability. HTTP readiness performs an
exact HTTP/1.1 GET against the declared path and requires the declared response
status. HTTPS, Zenoh, UDP, and `other` readiness are rejected until the IR and
runtime have a concrete protocol contract; they do not receive guessed TCP
semantics.

Process-compose 1.116 supports the initial SIGINT/SIGTERM and timeout but fixes
the final escalation to SIGKILL. A requested final SIGTERM is therefore rejected.
Pinned devenv's typed HTTP readiness port is an integer, so it cannot carry a
runtime environment reference; the typed exec probe delegates that resolution
to the runtime client.

Pinned devenv translates its typed overall `ready.timeout` into
process-compose's per-probe `timeout_seconds`. Consequently, the generated
process-compose backend has no distinct overall readiness deadline; runtime
preflight/session policy must enforce that deadline.

Pinned devenv 2.1.2 also asserts that SecretSpec loading is unavailable through
flake integration. SecretSpec may wrap the runtime/CLI boundary, but secret
values must never enter Nix evaluation, argv, the Nix store, or generated YAML.

## Verification

The Nix-only fixtures are evaluation checks and do not realize QEMU or package
builds:

```sh
nix eval --offline --impure --json --file tests/fixtures/devenv-launch/tests.nix
nix eval --offline --impure --json --file tests/fixtures/devenv-launch/pinned-devenv.nix
```

The second check imports the locally pinned devenv process and process-compose
modules and verifies their generated settings, including dependency, readiness,
restart, environment, and shutdown translation.
