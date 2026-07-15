use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    plan: PathBuf,
    executable: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "nixspace-benchmark-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let plan = root.join("benchmark-plan.json");
        let executable = std::env::current_exe().expect("integration test executable path");
        let fixture = Self {
            root,
            plan,
            executable,
        };
        fixture.write_plan(&fixture.plan_value(60_000, 0));
        fixture
    }

    fn plan_value(&self, p95_milliseconds: u64, exit_code: i32) -> Value {
        json!({
            "apiVersion": "nixspace/v1",
            "kind": "BenchmarkPlan",
            "interfaceVersion": 4,
            "limits": {"maxSamplesPerPhase": 1000},
            "id": "fixture",
            "reference": {"name": "test-host", "class": "test"},
            "context": {"cacheState": "fixture-warm", "flakeLockSha256": "test-lock"},
            "stateRoot": "state/benchmarks",
            "defaultCases": ["smoke"],
            "cases": {
                "smoke": {
                    "description": "Exercise an exact command.",
                    "context": {"coordinate": "fixture/smoke"},
                    "setup": [],
                    "beforeEach": [],
                    "measure": [{
                        "argv": [
                            self.executable,
                            "--exact",
                            "benchmark_helper_process",
                            "--nocapture"
                        ],
                        "cwd": ".",
                        "environment": {
                            "BENCH_VALUE": "from-plan",
                            "EXIT_CODE": exit_code.to_string(),
                            "HELPER_ARGUMENTS": "exact value",
                            "HELPER_MODE": "exit",
                            "NIXSPACE_BENCHMARK_HELPER": "1"
                        },
                        "inheritEnvironment": false,
                        "timeoutMilliseconds": 10_000,
                        "expectedExitCodes": [0]
                    }],
                    "afterEach": [],
                    "teardown": [],
                    "warmupSamples": 1,
                    "measuredSamples": 3,
                    "gates": {
                        "p50Milliseconds": 60_000,
                        "p95Milliseconds": p95_milliseconds
                    }
                }
            }
        })
    }

    fn write_plan(&self, value: &Value) {
        fs::write(&self.plan, serde_json::to_vec(value).unwrap()).unwrap();
    }

    fn with_secondary_case(&self) -> Value {
        let mut plan = self.plan_value(60_000, 0);
        let mut secondary = plan["cases"]["smoke"].clone();
        secondary["description"] = json!("Exercise a second exact command.");
        secondary["context"]["coordinate"] = json!("fixture/secondary");
        secondary["measure"][0]["environment"]["BENCH_VALUE"] = json!("secondary-plan");
        plan["cases"]
            .as_object_mut()
            .unwrap()
            .insert("secondary".into(), secondary);
        plan
    }

    fn run(&self, output: &Path, extra: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_nixspace"));
        command
            .arg("--workspace-root")
            .arg(&self.root)
            .arg("--benchmark-plan")
            .arg(&self.plan)
            .arg("benchmark")
            .arg("--output")
            .arg(output)
            .args(extra);
        command.output().expect("nixspace starts")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn report(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
#[allow(clippy::zombie_processes)] // The parent is intentionally SIGKILLed with this child as a process-tree regression.
fn benchmark_helper_process() {
    if std::env::var_os("NIXSPACE_BENCHMARK_HELPER").as_deref() != Some("1".as_ref()) {
        return;
    }
    if std::env::var("HELPER_MODE").as_deref() == Ok("hang") {
        loop {
            std::hint::spin_loop();
        }
    }
    if std::env::var("HELPER_MODE").as_deref() == Ok("spawn-child") {
        let _child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "benchmark_helper_process", "--nocapture"])
            .env("HELPER_MODE", "delayed-marker")
            .spawn()
            .unwrap();
        loop {
            std::hint::spin_loop();
        }
    }
    if std::env::var("HELPER_MODE").as_deref() == Ok("delayed-marker") {
        thread::sleep(Duration::from_millis(250));
        fs::write(std::env::var_os("CHILD_MARKER").unwrap(), "survived\n").unwrap();
        return;
    }
    let value = std::env::var("BENCH_VALUE").expect("helper benchmark value");
    if let Some(path) = std::env::var_os("TRACE_PATH") {
        let mut trace = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        writeln!(trace, "{value}").unwrap();
    }
    if let Ok(milliseconds) = std::env::var("SLEEP_MILLISECONDS") {
        thread::sleep(Duration::from_millis(milliseconds.parse().unwrap()));
    }
    let arguments = std::env::var("HELPER_ARGUMENTS").expect("helper arguments");
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{value}|{arguments}").unwrap();
    stdout.flush().unwrap();
    let code = std::env::var("EXIT_CODE")
        .expect("helper exit code")
        .parse()
        .expect("numeric helper exit code");
    std::process::exit(code);
}

#[test]
fn records_exact_command_status_timings_logs_and_nearest_rank_percentiles() {
    let fixture = Fixture::new();
    let output_path = fixture.root.join("reports/successful.json");
    let output = fixture.run(&output_path, &["--json"]);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value = report(&output_path);
    assert_eq!(value["apiVersion"], "nixspace/v1");
    assert_eq!(value["kind"], "BenchmarkReport");
    assert_eq!(value["interfaceVersion"], 3);
    assert_eq!(value["pathBase"], "workspace");
    assert_eq!(value["workspace"], ".");
    assert_eq!(value["planPath"], "benchmark-plan.json");
    assert_eq!(value["reportPath"], "reports/successful.json");
    assert_eq!(value["planId"], "fixture");
    assert_eq!(value["context"]["flakeLockSha256"], "test-lock");
    assert_eq!(
        value["reference"],
        json!({"name": "test-host", "class": "test"})
    );
    assert_eq!(value["passed"], true);

    let case = &value["cases"][0];
    assert_eq!(case["id"], "smoke");
    assert_eq!(case["context"]["coordinate"], "fixture/smoke");
    assert_eq!(case["commands"]["setup"], json!([]));
    assert_eq!(case["setup"], json!([]));
    assert_eq!(
        case["commands"]["measure"][0]["argv"],
        json!([
            fixture.executable,
            "--exact",
            "benchmark_helper_process",
            "--nocapture"
        ])
    );
    assert_eq!(case["commands"]["measure"][0]["cwd"].as_str(), Some("."));
    assert_eq!(case["commands"]["measure"][0]["inheritEnvironment"], false);
    assert_eq!(
        case["commands"]["measure"][0]["expectedExitCodes"],
        json!([0])
    );
    assert_eq!(case["warmups"].as_array().unwrap().len(), 1);
    assert_eq!(case["samples"].as_array().unwrap().len(), 3);
    assert_eq!(case["commands"]["teardown"], json!([]));
    assert_eq!(case["teardown"], json!([]));

    let mut durations: Vec<u64> = case["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|sample| {
            assert_eq!(
                sample["measure"][0]["status"],
                json!({"kind": "exited", "success": true, "code": 0, "expected": true})
            );
            let stdout = fixture
                .root
                .join(sample["measure"][0]["stdoutLog"].as_str().unwrap());
            let stderr = fixture
                .root
                .join(sample["measure"][0]["stderrLog"].as_str().unwrap());
            assert!(fs::read_to_string(stdout)
                .unwrap()
                .contains("from-plan|exact value\n"));
            assert_eq!(fs::read(stderr).unwrap(), Vec::<u8>::new());
            sample["durationNanoseconds"].as_u64().unwrap()
        })
        .collect();
    durations.sort_unstable();
    assert_eq!(
        case["statistics"]["p50Nanoseconds"].as_u64().unwrap(),
        durations[1]
    );
    assert_eq!(
        case["statistics"]["p95Nanoseconds"].as_u64().unwrap(),
        durations[2]
    );
    let stdout_report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        stdout_report["reportPath"].as_str(),
        Some("reports/successful.json")
    );
}

#[test]
fn default_explicit_and_all_case_selections_are_distinct_and_exact() {
    let fixture = Fixture::new();
    fixture.write_plan(&fixture.with_secondary_case());

    let default_path = fixture.root.join("default.json");
    let default = fixture.run(&default_path, &[]);
    assert!(default.status.success(), "{:?}", default);
    assert_eq!(
        report(&default_path)["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| case["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["smoke"]
    );

    let explicit_path = fixture.root.join("explicit.json");
    let explicit = fixture.run(&explicit_path, &["secondary", "smoke"]);
    assert!(explicit.status.success(), "{:?}", explicit);
    assert_eq!(
        report(&explicit_path)["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| case["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["secondary", "smoke"]
    );

    let all_path = fixture.root.join("all.json");
    let all = fixture.run(&all_path, &["--all"]);
    assert!(all.status.success(), "{:?}", all);
    assert_eq!(
        report(&all_path)["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| case["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["secondary", "smoke"]
    );

    let conflict_path = fixture.root.join("conflict.json");
    let conflict = fixture.run(&conflict_path, &["--all", "smoke"]);
    assert_eq!(conflict.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("cannot be used with"));
    assert!(!conflict_path.exists());
}

#[test]
fn host_name_override_changes_only_the_report_reference_name() {
    let fixture = Fixture::new();
    let output_path = fixture.root.join("host-name.json");
    let output = fixture.run(&output_path, &["--host-name", "ci-builder-17"]);
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        report(&output_path)["reference"],
        json!({"name": "ci-builder-17", "class": "test"})
    );

    let invalid_path = fixture.root.join("invalid-host-name.json");
    let invalid = fixture.run(&invalid_path, &["--host-name", ""]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("host name must be nonempty"));
    assert!(!invalid_path.exists());
}

#[test]
fn a_failed_gate_writes_the_complete_report_and_returns_failure() {
    let fixture = Fixture::new();
    fixture.write_plan(&fixture.plan_value(0, 0));
    let output_path = fixture.root.join("gate-failure.json");
    let output = fixture.run(&output_path, &[]);
    assert_eq!(output.status.code(), Some(1));
    let value = report(&output_path);
    assert_eq!(value["passed"], false);
    let p95 = &value["cases"][0]["gates"][1];
    assert_eq!(p95["statistic"], "p95");
    assert_eq!(p95["limitMilliseconds"], 0);
    assert_eq!(p95["passed"], false);
    assert_eq!(value["cases"][0]["samples"].as_array().unwrap().len(), 3);
}

#[test]
fn unexpected_exit_codes_are_evidence_not_lost_errors() {
    let fixture = Fixture::new();
    fixture.write_plan(&fixture.plan_value(60_000, 23));
    let output_path = fixture.root.join("command-failure.json");
    let output = fixture.run(&output_path, &[]);
    assert_eq!(output.status.code(), Some(1));
    let value = report(&output_path);
    assert_eq!(value["passed"], false);
    assert_eq!(
        value["cases"][0]["samples"][0]["measure"][0]["status"],
        json!({"kind": "exited", "success": false, "code": 23, "expected": false})
    );
}

#[test]
fn strict_versioned_plan_is_rejected_before_any_sample_runs() {
    let fixture = Fixture::new();
    let mut invalid = fixture.plan_value(60_000, 0);
    invalid["unexpected"] = json!(true);
    fixture.write_plan(&invalid);
    let output_path = fixture.root.join("invalid.json");
    let output = fixture.run(&output_path, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field `unexpected`"));
    assert!(!output_path.exists());
}

#[test]
fn empty_measure_and_invalid_lifecycle_commands_are_rejected_before_execution() {
    let fixture = Fixture::new();
    let invalid_plans = [
        (
            "empty-measure",
            {
                let mut plan = fixture.plan_value(60_000, 0);
                plan["cases"]["smoke"]["measure"] = json!([]);
                plan
            },
            "measure must declare at least one command",
        ),
        (
            "invalid-hook",
            {
                let mut plan = fixture.plan_value(60_000, 0);
                let mut command = plan["cases"]["smoke"]["measure"][0].clone();
                command["argv"] = json!([]);
                plan["cases"]["smoke"]["beforeEach"] = json!([command]);
                plan
            },
            "beforeEach command 1 argv must start with a non-empty executable",
        ),
    ];
    for (name, plan, diagnostic) in invalid_plans {
        fixture.write_plan(&plan);
        let output_path = fixture.root.join(format!("invalid-{name}.json"));
        let output = fixture.run(&output_path, &[]);
        assert_eq!(output.status.code(), Some(1), "{name}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(diagnostic),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!output_path.exists());
    }
}

#[test]
fn expected_exit_codes_and_sample_counts_are_strictly_bounded() {
    let fixture = Fixture::new();
    let invalid_plans = [
        (
            "duplicate-exit-code",
            {
                let mut plan = fixture.plan_value(60_000, 0);
                plan["cases"]["smoke"]["measure"][0]["expectedExitCodes"] = json!([0, 0]);
                plan
            },
            "expectedExitCodes must contain unique signed 32-bit values",
        ),
        (
            "too-many-warmups",
            {
                let mut plan = fixture.plan_value(60_000, 0);
                plan["limits"]["maxSamplesPerPhase"] = json!(2);
                plan["cases"]["smoke"]["warmupSamples"] = json!(3);
                plan
            },
            "warmupSamples must not exceed 2",
        ),
        (
            "too-many-samples",
            {
                let mut plan = fixture.plan_value(60_000, 0);
                plan["limits"]["maxSamplesPerPhase"] = json!(2);
                plan["cases"]["smoke"]["measuredSamples"] = json!(3);
                plan
            },
            "measuredSamples must be between 1 and 2",
        ),
        (
            "zero-plan-limit",
            {
                let mut plan = fixture.plan_value(60_000, 0);
                plan["limits"]["maxSamplesPerPhase"] = json!(0);
                plan
            },
            "limits.maxSamplesPerPhase must be positive",
        ),
    ];
    for (name, plan, diagnostic) in invalid_plans {
        fixture.write_plan(&plan);
        let output_path = fixture.root.join(format!("invalid-{name}.json"));
        let output = fixture.run(&output_path, &[]);
        assert_eq!(output.status.code(), Some(1), "{name}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(diagnostic),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!output_path.exists());
    }
}

#[test]
fn old_interfaces_and_invalid_default_selections_are_rejected_without_fallback() {
    let fixture = Fixture::new();
    let cases = [
        (
            "v3",
            json!(3),
            None,
            "benchmark interface version 3 is unsupported; expected 4",
        ),
        (
            "empty",
            json!(4),
            Some(json!([])),
            "defaultCases must declare at least one case ID",
        ),
        (
            "duplicate",
            json!(4),
            Some(json!(["smoke", "smoke"])),
            "defaultCases contains duplicate case `smoke`",
        ),
        (
            "unknown",
            json!(4),
            Some(json!(["missing"])),
            "defaultCases references unknown case `missing`",
        ),
    ];
    for (name, version, defaults, diagnostic) in cases {
        let mut plan = fixture.plan_value(60_000, 0);
        plan["interfaceVersion"] = version;
        if let Some(defaults) = defaults {
            plan["defaultCases"] = defaults;
        }
        fixture.write_plan(&plan);
        let output_path = fixture.root.join(format!("invalid-{name}.json"));
        let output = fixture.run(&output_path, &[]);
        assert_eq!(output.status.code(), Some(1), "{name}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(diagnostic),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!output_path.exists());
    }

    let mut missing = fixture.plan_value(60_000, 0);
    missing
        .as_object_mut()
        .unwrap()
        .remove("defaultCases")
        .unwrap();
    fixture.write_plan(&missing);
    let output_path = fixture.root.join("invalid-missing-defaults.json");
    let output = fixture.run(&output_path, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing field `defaultCases`"));
    assert!(!output_path.exists());

    let mut old_v2 = fixture.plan_value(60_000, 0);
    old_v2["interfaceVersion"] = json!(2);
    let case = old_v2["cases"]["smoke"].as_object_mut().unwrap();
    let command = case["measure"][0].clone();
    case.remove("setup");
    case.remove("beforeEach");
    case.remove("measure");
    case.remove("afterEach");
    case.remove("teardown");
    for field in [
        "argv",
        "cwd",
        "environment",
        "inheritEnvironment",
        "timeoutMilliseconds",
        "expectedExitCodes",
    ] {
        case.insert(field.into(), command[field].clone());
    }
    fixture.write_plan(&old_v2);
    let output_path = fixture.root.join("actual-v2-shape.json");
    let output = fixture.run(&output_path, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field `argv`"));
    assert!(!output_path.exists());
}

#[test]
fn emitted_timeout_terminates_the_sample_and_remains_in_the_report() {
    let fixture = Fixture::new();
    let mut plan = fixture.plan_value(60_000, 0);
    plan["cases"]["smoke"]["measure"][0]["environment"]["HELPER_MODE"] = json!("hang");
    plan["cases"]["smoke"]["warmupSamples"] = json!(0);
    plan["cases"]["smoke"]["measuredSamples"] = json!(1);
    plan["cases"]["smoke"]["measure"][0]["timeoutMilliseconds"] = json!(10);
    let trace = fixture.root.join("timeout-cleanup.trace");
    let mut cleanup = plan["cases"]["smoke"]["measure"][0].clone();
    cleanup["environment"]["BENCH_VALUE"] = json!("cleanup-after-timeout-1");
    cleanup["environment"]["HELPER_MODE"] = json!("exit");
    cleanup["environment"]["TRACE_PATH"] = json!(trace);
    let mut second_cleanup = cleanup.clone();
    second_cleanup["environment"]["BENCH_VALUE"] = json!("cleanup-after-timeout-2");
    plan["cases"]["smoke"]["afterEach"] = json!([cleanup, second_cleanup]);
    fixture.write_plan(&plan);
    let output_path = fixture.root.join("timeout.json");
    let output = fixture.run(&output_path, &[]);
    assert_eq!(output.status.code(), Some(1));
    let value = report(&output_path);
    assert_eq!(
        value["cases"][0]["samples"][0]["measure"][0]["status"],
        json!({"kind": "timed-out", "timeoutMilliseconds": 10})
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("timeout-cleanup.trace")).unwrap(),
        "cleanup-after-timeout-1\ncleanup-after-timeout-2\n"
    );
}

#[test]
fn timeout_terminates_descendants_before_returning() {
    let fixture = Fixture::new();
    let marker = fixture.root.join("descendant-survived");
    let mut plan = fixture.plan_value(60_000, 0);
    plan["cases"]["smoke"]["measure"][0]["environment"]["HELPER_MODE"] = json!("spawn-child");
    plan["cases"]["smoke"]["measure"][0]["environment"]["CHILD_MARKER"] = json!(marker);
    plan["cases"]["smoke"]["warmupSamples"] = json!(0);
    plan["cases"]["smoke"]["measuredSamples"] = json!(1);
    plan["cases"]["smoke"]["measure"][0]["timeoutMilliseconds"] = json!(25);
    fixture.write_plan(&plan);

    let output_path = fixture.root.join("process-tree-timeout.json");
    let output = fixture.run(&output_path, &[]);
    assert_eq!(output.status.code(), Some(1));
    thread::sleep(Duration::from_millis(350));
    assert!(
        !marker.exists(),
        "timed-out benchmark descendant survived process-tree termination"
    );
}

#[test]
fn lifecycle_commands_are_ordered_recorded_and_excluded_from_measured_duration() {
    let fixture = Fixture::new();
    let mut plan = fixture.plan_value(60_000, 0);
    let trace = fixture.root.join("lifecycle.trace");
    let command = plan["cases"]["smoke"]["measure"][0].clone();
    let phase = |name: &str, sleep_milliseconds: u64| {
        let mut value = command.clone();
        value["environment"]["BENCH_VALUE"] = json!(name);
        value["environment"]["TRACE_PATH"] = json!(trace);
        value["environment"]["SLEEP_MILLISECONDS"] = json!(sleep_milliseconds.to_string());
        value
    };
    plan["cases"]["smoke"]["setup"] = json!([phase("setup", 30)]);
    plan["cases"]["smoke"]["beforeEach"] = json!([phase("before-1", 30), phase("before-2", 30)]);
    plan["cases"]["smoke"]["measure"] = json!([phase("measure-1", 5), phase("measure-2", 5)]);
    plan["cases"]["smoke"]["afterEach"] = json!([phase("after-1", 30), phase("after-2", 30)]);
    plan["cases"]["smoke"]["teardown"] = json!([phase("teardown", 30)]);
    plan["cases"]["smoke"]["warmupSamples"] = json!(1);
    plan["cases"]["smoke"]["measuredSamples"] = json!(1);
    fixture.write_plan(&plan);

    let output_path = fixture.root.join("lifecycle.json");
    let output = fixture.run(&output_path, &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(&trace).unwrap(),
        concat!(
            "setup\n",
            "before-1\nbefore-2\nmeasure-1\nmeasure-2\nafter-1\nafter-2\n",
            "before-1\nbefore-2\nmeasure-1\nmeasure-2\nafter-1\nafter-2\n",
            "teardown\n"
        )
    );
    let value = report(&output_path);
    let case = &value["cases"][0];
    assert_eq!(case["setup"].as_array().unwrap().len(), 1);
    assert_eq!(case["warmups"].as_array().unwrap().len(), 1);
    assert_eq!(case["teardown"].as_array().unwrap().len(), 1);
    let sample = &case["samples"][0];
    assert_eq!(sample["beforeEach"].as_array().unwrap().len(), 2);
    assert_eq!(sample["measure"].as_array().unwrap().len(), 2);
    assert_eq!(sample["afterEach"].as_array().unwrap().len(), 2);
    assert_eq!(sample["passed"], true);
    let measured = sample["durationNanoseconds"].as_u64().unwrap();
    let phase_total: u64 = sample["beforeEach"]
        .as_array()
        .unwrap()
        .iter()
        .chain(sample["measure"].as_array().unwrap())
        .chain(sample["afterEach"].as_array().unwrap())
        .map(|command| command["durationNanoseconds"].as_u64().unwrap())
        .sum();
    assert!(
        measured < phase_total,
        "measured={measured} total={phase_total}"
    );
}

#[test]
fn failed_setup_skips_measurement_attempts_every_cleanup_and_aborts_later_samples() {
    let fixture = Fixture::new();
    let mut plan = fixture.plan_value(60_000, 0);
    let trace = fixture.root.join("setup-failure.trace");
    let command = plan["cases"]["smoke"]["measure"][0].clone();
    let phase = |name: &str, exit_code: i32| {
        let mut value = command.clone();
        value["environment"]["BENCH_VALUE"] = json!(name);
        value["environment"]["EXIT_CODE"] = json!(exit_code.to_string());
        value["environment"]["TRACE_PATH"] = json!(trace);
        value
    };
    plan["cases"]["smoke"]["setup"] =
        json!([phase("setup-fail", 23), phase("setup-must-not-run", 0)]);
    plan["cases"]["smoke"]["measure"] = json!([phase("must-not-run", 0)]);
    plan["cases"]["smoke"]["teardown"] = json!([phase("teardown-1", 17), phase("teardown-2", 0)]);
    plan["cases"]["smoke"]["warmupSamples"] = json!(0);
    plan["cases"]["smoke"]["measuredSamples"] = json!(3);
    fixture.write_plan(&plan);

    let output_path = fixture.root.join("setup-failure.json");
    let output = fixture.run(&output_path, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        fs::read_to_string(&trace).unwrap(),
        "setup-fail\nteardown-1\nteardown-2\n"
    );
    let value = report(&output_path);
    let case = &value["cases"][0];
    assert_eq!(case["setup"].as_array().unwrap().len(), 1);
    assert_eq!(case["warmups"], json!([]));
    assert_eq!(case["samples"], json!([]));
    assert_eq!(case["teardown"].as_array().unwrap().len(), 2);
    assert_eq!(case["passed"], false);
    assert!(case["statistics"]["p50Nanoseconds"].is_null());
}

#[test]
fn failed_measure_command_stops_its_sequence_runs_all_cleanup_and_allows_next_sample() {
    let fixture = Fixture::new();
    let mut plan = fixture.plan_value(60_000, 0);
    let trace = fixture.root.join("measure-failure.trace");
    let command = plan["cases"]["smoke"]["measure"][0].clone();
    let phase = |name: &str, exit_code: i32| {
        let mut value = command.clone();
        value["environment"]["BENCH_VALUE"] = json!(name);
        value["environment"]["EXIT_CODE"] = json!(exit_code.to_string());
        value["environment"]["TRACE_PATH"] = json!(trace);
        value
    };
    plan["cases"]["smoke"]["measure"] =
        json!([phase("measure-fail", 23), phase("measure-must-not-run", 0)]);
    plan["cases"]["smoke"]["afterEach"] = json!([phase("cleanup-1", 0), phase("cleanup-2", 0)]);
    plan["cases"]["smoke"]["warmupSamples"] = json!(0);
    plan["cases"]["smoke"]["measuredSamples"] = json!(2);
    fixture.write_plan(&plan);

    let output_path = fixture.root.join("measure-failure.json");
    let output = fixture.run(&output_path, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        fs::read_to_string(&trace).unwrap(),
        concat!(
            "measure-fail\ncleanup-1\ncleanup-2\n",
            "measure-fail\ncleanup-1\ncleanup-2\n"
        )
    );
    let value = report(&output_path);
    let samples = value["cases"][0]["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 2);
    for sample in samples {
        assert_eq!(sample["measure"].as_array().unwrap().len(), 1);
        assert_eq!(sample["afterEach"].as_array().unwrap().len(), 2);
        assert_eq!(sample["passed"], false);
    }
}

#[test]
fn spawn_failure_is_reported_and_still_runs_every_cleanup_command() {
    let fixture = Fixture::new();
    let mut plan = fixture.plan_value(60_000, 0);
    let trace = fixture.root.join("spawn-failure.trace");
    let command = plan["cases"]["smoke"]["measure"][0].clone();
    let mut missing = command.clone();
    missing["argv"][0] = json!(fixture.root.join("missing-executable"));
    let cleanup = |name: &str| {
        let mut value = command.clone();
        value["environment"]["BENCH_VALUE"] = json!(name);
        value["environment"]["TRACE_PATH"] = json!(trace);
        value
    };
    plan["cases"]["smoke"]["measure"] = json!([missing]);
    plan["cases"]["smoke"]["afterEach"] = json!([cleanup("cleanup-1"), cleanup("cleanup-2")]);
    plan["cases"]["smoke"]["warmupSamples"] = json!(0);
    plan["cases"]["smoke"]["measuredSamples"] = json!(1);
    fixture.write_plan(&plan);

    let output_path = fixture.root.join("spawn-failure.json");
    let output = fixture.run(&output_path, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        fs::read_to_string(&trace).unwrap(),
        "cleanup-1\ncleanup-2\n"
    );
    let value = report(&output_path);
    let sample = &value["cases"][0]["samples"][0];
    assert_eq!(sample["measure"][0]["status"]["kind"], "spawn-error");
    assert_eq!(sample["afterEach"].as_array().unwrap().len(), 2);
}

#[test]
fn log_io_failures_are_reported_without_suppressing_commands_or_cleanup() {
    let fixture = Fixture::new();
    let mut plan = fixture.plan_value(60_000, 0);
    let trace = fixture.root.join("log-io.trace");
    let command = plan["cases"]["smoke"]["measure"][0].clone();
    let phase = |name: &str| {
        let mut value = command.clone();
        value["environment"]["BENCH_VALUE"] = json!(name);
        value["environment"]["TRACE_PATH"] = json!(trace);
        value
    };
    plan["cases"]["smoke"]["measure"] = json!([phase("measure")]);
    plan["cases"]["smoke"]["afterEach"] = json!([phase("cleanup-1"), phase("cleanup-2")]);
    plan["cases"]["smoke"]["warmupSamples"] = json!(0);
    plan["cases"]["smoke"]["measuredSamples"] = json!(1);
    fixture.write_plan(&plan);

    let output_path = fixture.root.join("log-io.json");
    let case_logs = fixture.root.join("log-io.logs/smoke");
    fs::create_dir_all(case_logs.join("sample-001-measure-001.stdout.log")).unwrap();
    fs::create_dir_all(case_logs.join("sample-001-after-001.stdout.log")).unwrap();
    let output = fixture.run(&output_path, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        fs::read_to_string(&trace).unwrap(),
        "measure\ncleanup-1\ncleanup-2\n"
    );
    let value = report(&output_path);
    let sample = &value["cases"][0]["samples"][0];
    assert!(!sample["measure"][0]["infrastructureErrors"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(!sample["afterEach"][0]["infrastructureErrors"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(sample["afterEach"][1]["infrastructureErrors"], json!([]));
}

#[test]
fn relative_executable_is_resolved_against_its_declared_cwd() {
    let fixture = Fixture::new();
    let mut plan = fixture.plan_value(60_000, 0);
    let executable_parent = fixture.executable.parent().unwrap();
    let executable_name = fixture.executable.file_name().unwrap().to_string_lossy();
    plan["cases"]["smoke"]["measure"][0]["argv"][0] = json!(format!("./{executable_name}"));
    plan["cases"]["smoke"]["measure"][0]["cwd"] = json!(executable_parent);
    plan["cases"]["smoke"]["warmupSamples"] = json!(0);
    plan["cases"]["smoke"]["measuredSamples"] = json!(1);
    fixture.write_plan(&plan);

    let output_path = fixture.root.join("relative-executable.json");
    let output = fixture.run(&output_path, &[]);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value = report(&output_path);
    assert_eq!(
        value["cases"][0]["commands"]["measure"][0]["argv"][0],
        format!("./{executable_name}")
    );
}
