# FIRE Day 1 change and recovery notes

Date: 2026-07-14

This document records the coordinated `FIRE-day1` changes across the
CogniPilot workspace and the external Purdue parameter website. It is intended
to make tomorrow's diagnosis and rollback straightforward if the integrated
system behaves differently than expected.

## Repository branches

The changed repositories use the same branch name: `FIRE-day1`.

- `cognipilot_workspace`
- `cerebri_cubs2`
- `csyn`
- `electrode_web`
- `Purdue-Hackathon` (external website, outside the workspace manifest)

`autopilot-tools` has unrelated local work and is intentionally excluded.

## CUBS2 route and controller changes

`cerebri_cubs2` now uses four unique active waypoints:

1. `(-5, -5, 3)`
2. `(-5, 2, 3)`
3. `(17, 2, 3)`
4. `(16, -5, 3)`

The final route segment wraps directly from waypoint 4 to waypoint 1. The
telemetry publisher advertises four waypoints, so the visualizer no longer
shows the overlapping closing waypoint. The generated Modelica access path is
written with explicit fixed-index branches for the Rumoca production backend.

The default roll limit is 45 degrees (`0.7853981633974483` radians).

The realtime executable was rebuilt at:

`src/cerebri_cubs2/build-native_sim_realtime/zephyr/zephyr.exe`

## CSyn query fixes

Two defects in the Zephyr Zenoh query reply path were fixed:

1. The reply encoding object previously had block scope even though the reply
   options retained a reference to it. Parameter refresh bursts could therefore
   crash the autopilot in `z_query_reply`.
2. The copied request and reply arrays did not declare 8-byte alignment.
   A stack-layout change placed Rust-generated `ParamSetRequest` buffers on a
   4-byte boundary, causing FlatCC to reject otherwise valid requests containing
   a `double`. Parameter reads worked while writes returned `parameter set
   rejected`. Both buffers are now explicitly 8-byte aligned.

The realtime CUBS2 binary was rebuilt after these changes. Ground Station
parameter set and read-back both returned HTTP 200 in the end-to-end check.

## Electrode command boundary

The public LAN listener remains separate from the localhost vehicle connection:

- Private vehicle router: `udp/127.0.0.1:7447`
- Trusted browser router: `ws/127.0.0.1:7447`
- Checked public LAN router: `ws/0.0.0.0:7448`

Only these seven floating-point parameters are writable through the public LAN
router:

| Parameter | Minimum | Maximum |
| --- | ---: | ---: |
| `velocity.setpoint` | 0 | 5 |
| `route.crossTrackSteeringDistance` | 0 | 50 |
| `route.waypointSwitchingDistance` | 0.2 | 50 |
| `attitude.rollLimit` | 0.4363323129985824 | 3.141592653589793 |
| `attitude.headingPid.kp` | 0.2 | 5 |
| `attitude.headingPid.ki` | 0 | 10 |
| `attitude.headingPid.kd` | 0 | 3 |

The limits are stored in
`crates/electrode-command-authority/config/public-lan-parameter-limits.json`.
The trusted Ground Station parameter path remains outside these public value
limits. It still rejects non-finite values.

Public LAN manual-control and raw command topics remain rejected. Existing
Ground Station parameter changes are not routed through the public-LAN policy.

## Parameter audit records in MCAP

Electrode recordings now include a structured MCAP channel:

`gcs/v1/audit/parameter`

Each parameter attempt records:

- `timestampUnixMs`
- `source` (`ground_station` or `public_lan`)
- `name`
- `requestedValue`
- `effectiveValue` returned by the autopilot, when present
- `status` (`accepted`, `in_progress`, or `rejected`)
- `message`

This covers both the Ground Station HTTP parameter panel and the external LAN
website. Rejected public-LAN requests are recorded as well. Audit samples are
written only while Electrode recording is active.

The Ground Station backend and static web application were rebuilt. A Ground
Station process that was already running before this change must be restarted
before it emits audit records. Restarting Ground Station also stops its child
autopilot process.

## External Purdue website

The external site in `/home/micah/autopilot/Purdue-Hackathon` was reduced to the
parameter-control use case. Firmware update, packet flooding, grading-service,
and related tooling were removed. It serves on `0.0.0.0:8787` and derives the
Electrode endpoint from the page host, connecting to
`ws://<page-host>:7448`.

On the current LAN the page was reachable at:

`http://192.168.10.191:8787`

## Known operational observation

One long-running Ground Station process lost its TCP listener on port 7448
while its 7447 and 8790 listeners remained alive. The external website then
reported that it could not connect. An isolated test showed that normal
autopilot stop/start and concurrent Zenoh clients did not reproduce the loss,
so the autopilot restart was not established as the cause. Restart Ground
Station if 7448 is absent, then verify with:

```sh
ss -ltnup | rg '7447|7448|8790'
```

## Validation completed

- CUBS2 runtime-control C tests passed.
- Ninety consecutive parameter reads completed without an autopilot crash.
- Ground Station parameter set and read-back returned HTTP 200 after the CSyn
  alignment fix.
- Public-LAN parameter reads returned accepted values.
- `electrode-command-authority`: 19 unit tests and 8 policy tests passed.
- Electrode transport and MCAP recorder tests passed, including preservation of
  structured parameter audit records.
- `npm run lint` passed.
- `npm run check` completed with zero Svelte errors or warnings.
- The Electrode production web build and Ground Station Rust build completed.
- `git diff --check` passed in the changed repositories.

## Tomorrow's recovery workflow

1. Confirm every relevant checkout is on `FIRE-day1` with `ws status`.
2. Confirm only one `zephyr.exe` process is connected to the private router.
3. Confirm listeners 7447, 7448, 8787, and 8790 before flight.
4. Restart Ground Station and then the autopilot so the rebuilt backend and
   realtime executable are loaded.
5. Start an Electrode recording, make one harmless parameter change, stop the
   recording, and confirm `gcs/v1/audit/parameter` exists in the MCAP.
6. If behavior regresses, compare or revert the signed commits on `FIRE-day1`
   repository by repository rather than deleting build directories.
