# Supervised development processes

Devenv processes are for commands that remain running during a development
session. Devenv starts them in dependency order, checks readiness, aggregates
their logs, restarts them according to policy, and stops the whole session
together. Builds, tests, generators, West operations, and flashing remain
finite Devenv tasks.

## Processes available today

Processes are profile-scoped. Selecting a profile exposes only the processes
supported by that work area.

| Profile | Process | Purpose | Default |
| --- | --- | --- | --- |
| `qualisys` | `synapse-qualisys-bridge` | Publish QTM motion-capture data to Synapse and serve its dashboard | Started; waits for its dashboard HTTP endpoint |
| `ppm` | `synapse-ppm-bridge` | Forward Synapse manual-control data to a standalone PPM serial encoder | Started; defaults to `/dev/ttyACM0` and `udp/127.0.0.1:7447` |
| `cubs2` | `electrode-ground-station` | Serve the Electrode UI and own the local Zenoh router | Started; waits for `/gcs/health` |
| `cubs2` | `electrode-ppm-bridge` | Arbitrate manual/autopilot output and drive the PPM encoder | Started after the ground station |
| `cubs2` | `electrode-fake-vehicle` | Publish fake mocap and autopilot telemetry for UI development | Stopped; explicitly selected for UI simulation |
| `cubs2` | `synapse-qualisys-bridge` | Feed Qualisys data into the ground-station Zenoh endpoint | Stopped; explicitly selected for mocap deployments |

The CUBS2 profile uses Electrode's integrated PPM bridge because it understands
both manual input and CUBS2 autopilot PWM. The focused `ppm` profile retains the
separate, simpler `synapse_ppm_bridge` development environment. The Qualisys
bridge requires a reachable QTM endpoint, either physical hardware or a
compatible simulator.

## Start and control a session

Start the enabled CUBS2 deployment processes in the foreground:

```sh
devenv --profile cubs2 up
```

This starts `electrode-ground-station` and `electrode-ppm-bridge`. The
foreground TUI combines status and logs; exiting it stops the session. Name
processes explicitly to select an optional stack:

```sh
PPM_NO_SERIAL=true devenv --profile cubs2 up \
  electrode-ground-station electrode-ppm-bridge electrode-fake-vehicle
QUALISYS_HOST=192.168.1.10 devenv --profile cubs2 up \
  electrode-ground-station electrode-ppm-bridge synapse-qualisys-bridge
devenv --profile qualisys up synapse-qualisys-bridge
devenv --profile ppm up synapse-ppm-bridge
```

Dependencies still apply when processes are selected explicitly. The fake
vehicle waits for the ground station and PPM bridge. The PPM and Qualisys
bridges wait for the ground station's ready state.

Use detached mode when the processes should remain running after the start
command returns. Every management command must use the same profile:

```sh
devenv --profile cubs2 up -d
devenv --profile cubs2 processes wait --timeout 300
devenv --profile cubs2 processes list
devenv --profile cubs2 processes logs electrode-ground-station
devenv --profile cubs2 processes restart electrode-ppm-bridge
devenv --profile cubs2 processes restart electrode-fake-vehicle
devenv --profile cubs2 processes stop synapse-qualisys-bridge
devenv --profile cubs2 processes start synapse-qualisys-bridge
devenv --profile cubs2 processes down
```

Named ports are stable when their preferred port is free. By default Devenv
selects the next available port after a conflict. Add `--strict-ports` to `up`
when a conflict should fail instead:

```sh
devenv --profile cubs2 up --strict-ports
```

## Configure a session

Unlike a ROS launch file, `devenv up` does not forward arbitrary trailing
arguments to a process. Its positional arguments select process names. Use one
of the following native configuration mechanisms instead.

For values that change on each run, set environment variables supported by the
project executable:

```sh
PPM_SERIAL_DEVICE=/dev/ttyUSB0 \
  devenv --profile cubs2 up

PPM_NO_SERIAL=true devenv --profile cubs2 up \
  electrode-ground-station electrode-ppm-bridge electrode-fake-vehicle

QUALISYS_HOST=127.0.0.1 QUALISYS_PORT=22223 \
  devenv --profile qualisys up synapse-qualisys-bridge

PPM_SERIAL_DEVICE=/dev/ttyUSB0 PPM_BAUD_RATE=115200 \
ZENOH_CONNECT=udp/127.0.0.1:7447 \
  devenv --profile ppm up synapse-ppm-bridge
```

The same environment can be expressed as typed Devenv configuration overrides,
which is useful in scripts because the type and configuration path are
explicit:

```sh
devenv --profile qualisys \
  --option env.QUALISYS_HOST:string 127.0.0.1 \
  --option env.QUALISYS_PORT:string 22223 \
  up synapse-qualisys-bridge
```

Override the preferred base of a Devenv-managed server port through its process
option. All Nix expressions that reference the resolved port receive the same
value:

```sh
devenv --profile qualisys \
  --option processes.synapse-qualisys-bridge.ports.dashboard.allocate:int 8899 \
  up --strict-ports synapse-qualisys-bridge
```

Without `--strict-ports`, 8899 is a preferred base and Devenv may select the
next free port. With it, an occupied requested port is an error.

Use a profile for a developer work area with a distinct tool environment, not
for every runtime combination. A CUBS2 developer keeps the `cubs2` profile for
firmware, simulation, and deployment, then selects process names and per-run
environment values for the desired runtime topology.

If an application exposes only command-line flags and the invocation is truly
one-off, enter its profile shell and invoke the project command directly:

```sh
devenv --profile qualisys shell
cd src/synapse_qualisys_bridge
cargo run --locked --bin synapse-qualisys-bridge -- --help
```

That command is not supervised. For a recurring server mode, prefer adding
environment support to the project executable or defining a clearly named
process/profile preset instead of constructing a generic argument-forwarding
wrapper.

## What happens during `up`

For the default CUBS2 deployment session, Devenv performs this sequence:

1. Resolve named ports and the environment values that consume them.
2. Run finite prerequisite tasks, including generated dependency preparation,
   the locked npm installation, the Electrode build, and runtime-state setup.
3. Start `electrode-ground-station` and wait for its health endpoint.
4. Start `electrode-ppm-bridge` against the allocated ground-station Zenoh
   endpoint.
5. Monitor the processes, combine their logs, and apply restart policies.
6. Stop the process group together when the foreground session exits or
   `processes down` stops a detached session.

The Rust processes watch their repository's `rs` and `toml` files. A matching
change stops and launches the affected `cargo run` command again; Cargo retains
its normal incremental target directory. Frontend files are not currently part
of this restart behavior.

## Process or task?

Use a process when the command is expected to keep serving, consuming, or
streaming until the developer stops it. Use a task when success means the
command exits.

| Process | Task |
| --- | --- |
| Ground station or web development server | Production web build |
| Telemetry, motion-capture, or hardware bridge | Bridge unit or end-to-end test |
| Persistent local simulator | Finite SIL scenario that writes reports |
| Documentation preview server | Documentation build or link check |
| Background router or daemon | West update, firmware build, or flash |

Interactive terminal programs such as a serial console are normally better run
directly from a selected profile. Supervision adds little when the program owns
the terminal and expects direct keyboard input.

## Adding a process

Define the process in the profile that owns the complete runtime environment.
Keep its command declarative and delegate behavior to the project-owned
executable:

```nix
processes.example = {
  cwd = source "project";
  exec = "exec project-native-command";
  after = [
    "project:build"
    "devenv:processes:dependency@ready"
  ];
  ports.http.allocate = 8080;
  ready.http.get = {
    port = config.processes.example.ports.http.value;
    path = "/health";
  };
  restart = {
    on = "on_failure";
    max = 3;
  };
};
```

Use `exec` so signals reach the real process. Express setup as task edges and
process ordering with `@ready`; do not reproduce project build logic in the
process command. A hardware-dependent or occasional process may set
`start.enable = false`, which leaves it visible but stopped in the process TUI
during a normal `up` session.

Only add a readiness probe when it represents usable service state. HTTP health
checks are appropriate for the ground station and dashboards. A serial bridge
without a stable probe can run without one and report connection failures in
its managed logs.

## Planned process improvements

The following names and commands are design targets, not currently available
workspace interfaces:

- Add an `electrode-web-dev` Vite process to the `electrode` and `cubs2`
  profiles. It should wait for the ground station, use Vite's own hot module
  replacement, and expose an HTTP readiness check. The Vite ground-station
  proxy must consume the Devenv-allocated ground-station port rather than its
  current fixed port before this is reliable.
- Add an opt-in `qualisys-simulator` process and make the bridge consume its
  allocated RT port. This provides a hardware-free Qualisys development stack.
- Consider an opt-in `electrode-manual-control-bridge` process for joystick or
  hardware sessions and a `rumoca-docs` preview process. Both are useful
  long-running commands but should not expand the default CUBS2 session.

The CUBS2 native SIL runner remains a task. Even with real-time pacing, it runs
a finite scenario, evaluates results, and produces artifacts before exiting.
