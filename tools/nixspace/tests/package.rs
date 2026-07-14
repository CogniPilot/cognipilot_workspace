use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

fn cargo(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO"))
        .args(arguments)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Cargo starts")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cargo_metadata_proves_the_package_is_standalone_and_installable() {
    let output = cargo(&[
        "metadata",
        "--format-version",
        "1",
        "--no-deps",
        "--locked",
        "--offline",
    ]);
    assert_success(&output);
    let metadata: Value = serde_json::from_slice(&output.stdout).expect("metadata is JSON");
    let packages = metadata["packages"]
        .as_array()
        .expect("packages is an array");
    assert_eq!(packages.len(), 1);
    let package = &packages[0];
    assert_eq!(package["name"], "nixspace");
    assert_eq!(
        package["description"],
        "Nix-first workspace meta-client for querying generated plans and delegating to native tools"
    );
    assert_eq!(package["documentation"], "https://docs.rs/nixspace");
    assert_eq!(package["license"], "Apache-2.0");
    assert_eq!(package["rust_version"], "1.82");
    assert_eq!(
        Path::new(package["readme"].as_str().expect("readme is a path"))
            .file_name()
            .expect("readme has a file name"),
        "README.md"
    );
    assert_eq!(metadata["workspace_members"].as_array().unwrap().len(), 1);
    assert_eq!(
        metadata["workspace_default_members"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let targets = package["targets"].as_array().expect("targets is an array");
    assert!(targets.iter().any(|target| {
        target["name"] == "nixspace"
            && target["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"))
    }));
    let dependencies = package["dependencies"]
        .as_array()
        .expect("dependencies is an array");
    let mut dependency_names: Vec<_> = dependencies
        .iter()
        .map(|dependency| dependency["name"].as_str().unwrap())
        .collect();
    dependency_names.sort_unstable();
    assert_eq!(
        dependency_names,
        [
            "base64",
            "clap",
            "clap_complete",
            "fs4",
            "miette",
            "nix-base32",
            "serde",
            "serde_json"
        ]
    );
    for (name, version) in [("base64", "=0.22.1"), ("nix-base32", "=0.2.0")] {
        let dependency = dependencies
            .iter()
            .find(|dependency| dependency["name"] == name)
            .unwrap_or_else(|| panic!("{name} is a direct dependency"));
        assert_eq!(dependency["req"], version);
    }
    let fs4 = dependencies
        .iter()
        .find(|dependency| dependency["name"] == "fs4")
        .expect("fs4 is a direct dependency");
    assert_eq!(fs4["req"], "=1.1.0");
    assert_eq!(fs4["uses_default_features"], false);
    assert_eq!(fs4["features"], serde_json::json!(["sync"]));
    for dependency in dependencies {
        assert!(
            dependency["path"].is_null(),
            "path dependency: {dependency}"
        );
        assert!(
            dependency["source"]
                .as_str()
                .is_some_and(|source| source.starts_with("registry+")),
            "non-registry dependency: {dependency}"
        );
    }
}

#[test]
fn cargo_package_list_contains_only_publishable_package_sources() {
    let output = cargo(&[
        "package",
        "--list",
        "--allow-dirty",
        "--locked",
        "--offline",
    ]);
    assert_success(&output);
    let listing = String::from_utf8(output.stdout).expect("package list is UTF-8");
    let files: Vec<_> = listing.lines().collect();
    for required in [
        "Cargo.lock",
        "Cargo.toml",
        "LICENSE",
        "README.md",
        "src/main.rs",
        "src/model.rs",
        "src/action.rs",
        "src/host.rs",
        "src/launch.rs",
        "src/launch_exec.rs",
        "src/source.rs",
        "tests/action.rs",
        "tests/host.rs",
        "tests/launch_exec.rs",
        "tests/source.rs",
        "tests/index.rs",
        "tests/query.rs",
    ] {
        assert!(
            files.contains(&required),
            "package omits {required}: {files:?}"
        );
    }
    assert!(files.iter().all(|path| !path.starts_with("target/")));
    assert!(files.iter().all(|path| !path.contains("../src/")));

    let forbidden_project_name = ["cogni", "pilot"].concat();
    for relative in files
        .iter()
        .filter(|path| path.ends_with(".rs") || path.ends_with(".toml") || path.ends_with(".md"))
    {
        let contents = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(relative))
            .expect("publishable text source is readable")
            .to_ascii_lowercase();
        assert!(
            !contents.contains(&forbidden_project_name),
            "publishable source {relative} contains project-specific coupling"
        );
    }
}
