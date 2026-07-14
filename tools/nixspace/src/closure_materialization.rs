use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use clap::Args;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{CliError, Result};

const SUPPORTED_API_VERSION: &str = "nixspace/v1";
const SUPPORTED_KIND: &str = "ClosureMaterialization";
const SUPPORTED_INTERFACE_VERSION: u64 = 2;

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Args)]
pub(crate) struct MaterializeArgs {
    /// Read the versioned generic closure-materialization envelope from this file.
    #[arg(long, value_name = "PATH")]
    plan_file: PathBuf,

    /// Read Nix __structuredAttrs, including exportReferencesGraph data, here.
    #[arg(long, value_name = "PATH")]
    structured_attrs: PathBuf,

    /// Atomically write the materialized JSON document to this exact path.
    #[arg(long, value_name = "PATH")]
    output: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MaterializationPlan {
    api_version: String,
    kind: String,
    interface_version: u64,
    closure_attribute: String,
    closure_records_pointer: String,
    proof_bindings: Vec<ProofBinding>,
    document: Value,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProofBinding {
    source_output_pointer: String,
    destination_digest_pointer: String,
    transform: ProofTransform,
}

#[derive(Clone, Copy, Debug, Deserialize)]
enum ProofTransform {
    #[serde(rename = "nix-nar-sha256-to-hex")]
    NixNarSha256ToHex,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ClosureRecord {
    path: String,
    nar_hash: String,
    nar_size: u64,
    #[serde(rename = "closureSize")]
    _closure_size: u64,
    #[serde(rename = "valid")]
    _valid: bool,
    references: Vec<String>,
}

impl ClosureRecord {
    fn normalized(mut self) -> Result<Self> {
        if !self._valid {
            return Err(CliError(format!(
                "Nix closure record `{}` is not valid",
                self.path
            )));
        }
        if self.path.is_empty()
            || self.nar_hash.is_empty()
            || self.path.contains('\0')
            || self.nar_hash.contains('\0')
        {
            return Err(CliError(
                "Nix closure records require nonempty path and narHash values without NUL".into(),
            ));
        }
        self.references.sort();
        if self
            .references
            .iter()
            .any(|reference| reference.is_empty() || reference.contains('\0'))
        {
            return Err(CliError(
                "Nix closure record references must be nonempty and contain no NUL".into(),
            ));
        }
        if self.references.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(CliError(format!(
                "Nix closure record `{}` contains duplicate references",
                self.path
            )));
        }
        Ok(self)
    }

    fn document(&self) -> Value {
        serde_json::json!({
            "path": self.path,
            "narHash": self.nar_hash,
            "narSize": self.nar_size,
            "references": self.references,
        })
    }
}

pub(crate) fn materialize(arguments: MaterializeArgs) -> Result<()> {
    let plan_bytes = fs::read(&arguments.plan_file).map_err(|error| {
        CliError(format!(
            "cannot read closure-materialization plan at {}: {error}",
            arguments.plan_file.display()
        ))
    })?;
    let plan: MaterializationPlan = serde_json::from_slice(&plan_bytes).map_err(|error| {
        CliError(format!(
            "closure-materialization plan at {} is unreadable: {error}",
            arguments.plan_file.display()
        ))
    })?;
    let attrs_bytes = fs::read(&arguments.structured_attrs).map_err(|error| {
        CliError(format!(
            "cannot read Nix structured attributes at {}: {error}",
            arguments.structured_attrs.display()
        ))
    })?;
    let attrs: Value = serde_json::from_slice(&attrs_bytes).map_err(|error| {
        CliError(format!(
            "Nix structured attributes at {} are unreadable: {error}",
            arguments.structured_attrs.display()
        ))
    })?;

    let document = materialize_document(plan, &attrs)?;
    let mut encoded = serde_json::to_vec_pretty(&document)
        .map_err(|error| CliError(format!("cannot serialize materialized document: {error}")))?;
    encoded.push(b'\n');
    atomic_write(&arguments.output, &encoded)
}

fn materialize_document(mut plan: MaterializationPlan, attrs: &Value) -> Result<Value> {
    validate_plan(&plan)?;
    let raw_closure = attrs
        .as_object()
        .and_then(|attrs| attrs.get(&plan.closure_attribute))
        .ok_or_else(|| {
            CliError(format!(
                "Nix structured attributes do not contain declared closure attribute `{}`",
                plan.closure_attribute
            ))
        })?;
    let raw_records = raw_closure.as_array().ok_or_else(|| {
        CliError(format!(
            "Nix structured closure attribute `{}` must be an array",
            plan.closure_attribute
        ))
    })?;
    let mut records = raw_records
        .iter()
        .map(|record| {
            serde_json::from_value::<ClosureRecord>(record.clone())
                .map_err(|error| CliError(format!("invalid Nix closure record: {error}")))?
                .normalized()
        })
        .collect::<Result<Vec<_>>>()?;
    records.sort_by(|left, right| left.path.cmp(&right.path));
    if records.windows(2).any(|pair| pair[0].path == pair[1].path) {
        return Err(CliError(
            "Nix structured closure contains duplicate store paths".into(),
        ));
    }
    let proofs: BTreeMap<_, _> = records
        .iter()
        .map(|record| (record.path.as_str(), record))
        .collect();

    let proof_count = attach_output_proofs(&mut plan.document, &proofs)?;
    if proof_count == 0 {
        return Err(CliError(
            "closure-materialization document declares no output objects with drvPath and storePath"
                .into(),
        ));
    }

    apply_proof_bindings(&mut plan.document, &plan.proof_bindings)?;

    let closure_target = plan
        .document
        .pointer_mut(&plan.closure_records_pointer)
        .ok_or_else(|| {
            CliError(format!(
                "closureRecordsPointer `{}` does not resolve inside the materialization document",
                plan.closure_records_pointer
            ))
        })?;
    if !closure_target.is_null() {
        return Err(CliError(format!(
            "closureRecordsPointer `{}` must target a null placeholder",
            plan.closure_records_pointer
        )));
    }
    *closure_target = Value::Array(records.iter().map(ClosureRecord::document).collect());
    Ok(plan.document)
}

fn validate_plan(plan: &MaterializationPlan) -> Result<()> {
    if plan.api_version != SUPPORTED_API_VERSION {
        return Err(CliError(format!(
            "closure-materialization API `{}` is unsupported; expected `{SUPPORTED_API_VERSION}`",
            plan.api_version
        )));
    }
    if plan.kind != SUPPORTED_KIND {
        return Err(CliError(format!(
            "closure-materialization kind `{}` is unsupported; expected `{SUPPORTED_KIND}`",
            plan.kind
        )));
    }
    if plan.interface_version != SUPPORTED_INTERFACE_VERSION {
        return Err(CliError(format!(
            "closure-materialization interface version {} is unsupported; expected {SUPPORTED_INTERFACE_VERSION}",
            plan.interface_version
        )));
    }
    if plan.closure_attribute.is_empty()
        || plan.closure_attribute.contains('\0')
        || plan.closure_records_pointer.is_empty()
        || !plan.closure_records_pointer.starts_with('/')
    {
        return Err(CliError(
            "closureAttribute and closureRecordsPointer must be nonempty safe values".into(),
        ));
    }
    if !plan.document.is_object() {
        return Err(CliError(
            "closure-materialization document must be a JSON object".into(),
        ));
    }
    let mut source_pointers = BTreeSet::new();
    let mut destination_pointers = BTreeSet::new();
    for binding in &plan.proof_bindings {
        for (label, pointer) in [
            ("sourceOutputPointer", &binding.source_output_pointer),
            (
                "destinationDigestPointer",
                &binding.destination_digest_pointer,
            ),
        ] {
            if pointer.is_empty() || !pointer.starts_with('/') || pointer.contains('\0') {
                return Err(CliError(format!(
                    "proof binding {label} must be a nonempty JSON pointer without NUL"
                )));
            }
        }
        if !source_pointers.insert(&binding.source_output_pointer) {
            return Err(CliError(format!(
                "duplicate proof binding sourceOutputPointer `{}`",
                binding.source_output_pointer
            )));
        }
        if !destination_pointers.insert(&binding.destination_digest_pointer) {
            return Err(CliError(format!(
                "duplicate proof binding destinationDigestPointer `{}`",
                binding.destination_digest_pointer
            )));
        }
    }
    Ok(())
}

fn apply_proof_bindings(document: &mut Value, bindings: &[ProofBinding]) -> Result<()> {
    for binding in bindings {
        let nar_hash = document
            .pointer(&binding.source_output_pointer)
            .and_then(Value::as_object)
            .ok_or_else(|| {
                CliError(format!(
                    "proof binding sourceOutputPointer `{}` does not resolve to an output object",
                    binding.source_output_pointer
                ))
            })?
            .get("narHash")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                CliError(format!(
                    "proof binding sourceOutputPointer `{}` is not an output proved by the declared Nix closure",
                    binding.source_output_pointer
                ))
            })?
            .to_owned();
        let digest = match binding.transform {
            ProofTransform::NixNarSha256ToHex => nar_sha256_to_hex(&nar_hash)?,
        };
        let destination = document
            .pointer_mut(&binding.destination_digest_pointer)
            .ok_or_else(|| {
                CliError(format!(
                    "proof binding destinationDigestPointer `{}` does not resolve inside the materialization document",
                    binding.destination_digest_pointer
                ))
            })?;
        if !destination.is_null() {
            return Err(CliError(format!(
                "proof binding destinationDigestPointer `{}` must target a null placeholder",
                binding.destination_digest_pointer
            )));
        }
        *destination = Value::String(digest);
    }
    Ok(())
}

fn nar_sha256_to_hex(value: &str) -> Result<String> {
    let bytes = if let Some(encoded) = value.strip_prefix("sha256-") {
        let bytes = BASE64_STANDARD.decode(encoded).map_err(|error| {
            CliError(format!(
                "Nix NAR proof `{value}` has invalid SRI base64 encoding: {error}"
            ))
        })?;
        if BASE64_STANDARD.encode(&bytes) != encoded {
            return Err(CliError(format!(
                "Nix NAR proof `{value}` is not canonical SRI base64"
            )));
        }
        bytes
    } else if let Some(encoded) = value.strip_prefix("sha256:") {
        let bytes = nix_base32::from_nix_base32(encoded).ok_or_else(|| {
            CliError(format!(
                "Nix NAR proof `{value}` has invalid Nix base32 encoding"
            ))
        })?;
        if nix_base32::to_nix_base32(&bytes) != encoded {
            return Err(CliError(format!(
                "Nix NAR proof `{value}` is not canonical Nix base32"
            )));
        }
        bytes
    } else {
        return Err(CliError(format!(
            "Nix NAR proof `{value}` is not sha256 SRI or Nix base32"
        )));
    };
    if bytes.len() != 32 {
        return Err(CliError(format!(
            "Nix NAR proof `{value}` decodes to {} bytes; sha256 requires 32",
            bytes.len()
        )));
    }
    let mut digest = String::with_capacity(64);
    for byte in bytes {
        write!(&mut digest, "{byte:02x}")
            .map_err(|error| CliError(format!("cannot encode NAR sha256 digest: {error}")))?;
    }
    debug_assert_eq!(digest.len(), 64);
    debug_assert!(digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
    Ok(digest)
}

fn attach_output_proofs(
    value: &mut Value,
    proofs: &BTreeMap<&str, &ClosureRecord>,
) -> Result<usize> {
    match value {
        Value::Array(values) => values.iter_mut().try_fold(0, |count, value| {
            attach_output_proofs(value, proofs).map(|added| count + added)
        }),
        Value::Object(object) => {
            let mut count = 0;
            if object.contains_key("drvPath") {
                count += attach_one_output_proof(object, proofs)?;
            }
            let nested = object.values_mut().try_fold(0, |count, value| {
                attach_output_proofs(value, proofs).map(|added| count + added)
            })?;
            Ok(count + nested)
        }
        _ => Ok(0),
    }
}

fn attach_one_output_proof(
    object: &mut Map<String, Value>,
    proofs: &BTreeMap<&str, &ClosureRecord>,
) -> Result<usize> {
    let drv_path = object
        .get("drvPath")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CliError("output object drvPath must be a nonempty string".into()))?;
    let store_path = object
        .get("storePath")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CliError(format!(
                "output object with drvPath `{drv_path}` requires a nonempty storePath"
            ))
        })?;
    if object.contains_key("narHash") || object.contains_key("narSize") {
        return Err(CliError(format!(
            "output object `{store_path}` must not predeclare narHash or narSize"
        )));
    }
    let proof = proofs.get(store_path).ok_or_else(|| {
        CliError(format!(
            "output object `{store_path}` is absent from the declared Nix closure"
        ))
    })?;
    object.insert("narHash".into(), Value::String(proof.nar_hash.clone()));
    object.insert("narSize".into(), Value::from(proof.nar_size));
    Ok(1)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        CliError(format!(
            "materialization output {} has no parent directory",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        CliError(format!(
            "cannot create materialization output directory {}: {error}",
            parent.display()
        ))
    })?;
    let name = path.file_name().ok_or_else(|| {
        CliError(format!(
            "materialization output {} has no file name",
            path.display()
        ))
    })?;
    let temporary = parent.join(format!(
        ".{}.tmp-{}-{}",
        name.to_string_lossy(),
        std::process::id(),
        NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| {
                CliError(format!(
                    "cannot create temporary materialization output {}: {error}",
                    temporary.display()
                ))
            })?;
        file.write_all(bytes).map_err(|error| {
            CliError(format!(
                "cannot write temporary materialization output {}: {error}",
                temporary.display()
            ))
        })?;
        file.sync_all().map_err(|error| {
            CliError(format!(
                "cannot sync temporary materialization output {}: {error}",
                temporary.display()
            ))
        })?;
        fs::rename(&temporary, path).map_err(|error| {
            CliError(format!(
                "cannot atomically publish materialization output {}: {error}",
                path.display()
            ))
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn plan(document: Value) -> MaterializationPlan {
        MaterializationPlan {
            api_version: SUPPORTED_API_VERSION.into(),
            kind: SUPPORTED_KIND.into(),
            interface_version: SUPPORTED_INTERFACE_VERSION,
            closure_attribute: "closureGraph".into(),
            closure_records_pointer: "/closure/records".into(),
            proof_bindings: Vec::new(),
            document,
        }
    }

    #[test]
    fn recursively_proves_outputs_without_treating_input_store_paths_as_outputs() {
        let attrs = json!({
            "closureGraph": [
                {"path": "/nix/store/b", "narHash": "sha256-b", "narSize": 2, "closureSize": 2, "valid": true, "references": []},
                {"path": "/nix/store/a", "narHash": "sha256-a", "narSize": 1, "closureSize": 3, "valid": true, "references": ["/nix/store/b"]}
            ]
        });
        let result = materialize_document(
            plan(json!({
                "output": {"drvPath": "/nix/store/a.drv", "storePath": "/nix/store/a"},
                "nested": [{"value": {"drvPath": "/nix/store/b.drv", "storePath": "/nix/store/b"}}],
                "inputIdentity": {"storePath": "/nix/store/not-in-closure"},
                "closure": {"records": null}
            })),
            &attrs,
        )
        .unwrap();

        assert_eq!(result["output"]["narHash"], "sha256-a");
        assert_eq!(result["nested"][0]["value"]["narSize"], 2);
        assert!(result["inputIdentity"].get("narHash").is_none());
        assert_eq!(result["closure"]["records"][0]["path"], "/nix/store/a");
    }

    #[test]
    fn rejects_missing_output_proofs_and_prepopulated_closure_slots() {
        let attrs = json!({"closureGraph": []});
        let missing = materialize_document(
            plan(json!({
                "output": {"drvPath": "/nix/store/missing.drv", "storePath": "/nix/store/missing"},
                "closure": {"records": null}
            })),
            &attrs,
        )
        .unwrap_err();
        assert!(missing.0.contains("absent from the declared Nix closure"));

        let attrs = json!({
            "closureGraph": [
                {"path": "/nix/store/a", "narHash": "sha256-a", "narSize": 1, "closureSize": 1, "valid": true, "references": []}
            ]
        });
        let populated = materialize_document(
            plan(json!({
                "output": {"drvPath": "/nix/store/a.drv", "storePath": "/nix/store/a"},
                "closure": {"records": []}
            })),
            &attrs,
        )
        .unwrap_err();
        assert!(populated.0.contains("must target a null placeholder"));
    }

    #[test]
    fn binds_a_proved_nar_sha256_to_a_declared_hex_digest_slot() {
        let attrs = json!({
            "closureGraph": [{
                "path": "/nix/store/a",
                "narHash": "sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=",
                "narSize": 1,
                "closureSize": 1,
                "valid": true,
                "references": []
            }]
        });
        let mut materialization = plan(json!({
            "subject": [{
                "digest": {"sha256": null},
                "annotations": {"output": {
                    "drvPath": "/nix/store/a.drv",
                    "storePath": "/nix/store/a"
                }}
            }],
            "closure": {"records": null}
        }));
        materialization.proof_bindings = vec![ProofBinding {
            source_output_pointer: "/subject/0/annotations/output".into(),
            destination_digest_pointer: "/subject/0/digest/sha256".into(),
            transform: ProofTransform::NixNarSha256ToHex,
        }];

        let result = materialize_document(materialization, &attrs).unwrap();

        assert_eq!(
            result["subject"][0]["digest"]["sha256"],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            result["subject"][0]["annotations"]["output"]["narHash"],
            "sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU="
        );
        assert_eq!(
            nar_sha256_to_hex("sha256:11gq51wnzwq1lz2x6kqpaxkw0qnadk7rcca9xk6nx29k48pzraac")
                .unwrap(),
            "4ca9fc2f2233896ecdec493196cf6cca62c06757174fd3c5a701f36f7928f885"
        );
    }

    #[test]
    fn proof_bindings_reject_ambiguous_unproved_or_invalid_destinations() {
        let attrs = json!({
            "closureGraph": [{
                "path": "/nix/store/a",
                "narHash": "sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=",
                "narSize": 1,
                "closureSize": 1,
                "valid": true,
                "references": []
            }]
        });
        let document = json!({
            "output": {"drvPath": "/nix/store/a.drv", "storePath": "/nix/store/a"},
            "identity": {"storePath": "/nix/store/a"},
            "digest": {"sha256": null, "occupied": "deadbeef"},
            "closure": {"records": null}
        });

        for (source, destination, diagnostic) in [
            (
                "/missing",
                "/digest/sha256",
                "does not resolve to an output object",
            ),
            (
                "/identity",
                "/digest/sha256",
                "is not an output proved by the declared Nix closure",
            ),
            (
                "/output",
                "/digest/missing",
                "does not resolve inside the materialization document",
            ),
            (
                "/output",
                "/digest/occupied",
                "must target a null placeholder",
            ),
        ] {
            let mut materialization = plan(document.clone());
            materialization.proof_bindings = vec![ProofBinding {
                source_output_pointer: source.into(),
                destination_digest_pointer: destination.into(),
                transform: ProofTransform::NixNarSha256ToHex,
            }];
            let error = materialize_document(materialization, &attrs).unwrap_err();
            assert!(error.0.contains(diagnostic), "{}", error.0);
        }
    }

    #[test]
    fn proof_bindings_reject_duplicate_pointers_and_invalid_sha256_encodings() {
        let mut duplicate = plan(json!({
            "output": {"drvPath": "/nix/store/a.drv", "storePath": "/nix/store/a"},
            "secondOutput": {"drvPath": "/nix/store/a.drv", "storePath": "/nix/store/a"},
            "digest": {"one": null, "two": null},
            "closure": {"records": null}
        }));
        duplicate.proof_bindings = vec![
            ProofBinding {
                source_output_pointer: "/output".into(),
                destination_digest_pointer: "/digest/one".into(),
                transform: ProofTransform::NixNarSha256ToHex,
            },
            ProofBinding {
                source_output_pointer: "/output".into(),
                destination_digest_pointer: "/digest/two".into(),
                transform: ProofTransform::NixNarSha256ToHex,
            },
        ];
        let error = validate_plan(&duplicate).unwrap_err();
        assert!(error
            .0
            .contains("duplicate proof binding sourceOutputPointer"));

        duplicate.proof_bindings[1].source_output_pointer = "/secondOutput".into();
        duplicate.proof_bindings[1].destination_digest_pointer = "/digest/one".into();
        let error = validate_plan(&duplicate).unwrap_err();
        assert!(error
            .0
            .contains("duplicate proof binding destinationDigestPointer"));

        for value in [
            "sha512-Zm9v",
            "sha256-Zm9v",
            "sha256-***",
            "sha256:not-nix-base32",
        ] {
            let error = nar_sha256_to_hex(value).unwrap_err();
            assert!(
                error.0.contains("sha256 SRI")
                    || error.0.contains("decodes to")
                    || error.0.contains("invalid")
            );
        }
    }
}
