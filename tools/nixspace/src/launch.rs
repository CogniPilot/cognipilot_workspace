use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{json, Number, Value};

use crate::model::{
    Capabilities, EndpointTemplate, LaunchPlanTemplate, LaunchValue, ParameterDefinition,
    ParameterType, PortAllocation,
};

const REDACTED_SECRET: &str = "<redacted>";

#[derive(Clone, Debug)]
struct ResolvedParameter {
    definition: ParameterDefinition,
    value: Option<Value>,
    source: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedLaunchPlan {
    pub interface_version: u64,
    pub launch: String,
    pub parameters: BTreeMap<String, ParameterDocument>,
    pub required_artifacts: Vec<String>,
    pub required_resources: Vec<String>,
    pub capabilities: Capabilities,
    pub instances: Vec<InstanceDocument>,
    pub processes: Vec<ProcessDocument>,
}

#[derive(Debug, Serialize)]
pub struct ParameterDocument {
    #[serde(rename = "type")]
    pub(crate) parameter_type: ParameterType,
    pub(crate) source: &'static str,
    pub(crate) value: Value,
    #[serde(skip_serializing_if = "fixed_allocation")]
    pub(crate) allocation: PortAllocation,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy: Option<PathPolicy>,
}

fn fixed_allocation(allocation: &PortAllocation) -> bool {
    *allocation == PortAllocation::Fixed
}

impl ResolvedLaunchPlan {
    pub(crate) fn parameter(
        &self,
        workspace_coordinate: &str,
        parameter_id: &str,
    ) -> Result<&ParameterDocument, String> {
        let mut matches = self.instances.iter().filter_map(|instance| {
            (instance.launch == workspace_coordinate)
                .then(|| instance.parameters.get(parameter_id))
                .flatten()
        });
        let first = matches.next().ok_or_else(|| {
            format!(
                "resolved launch `{}` has no parameter `{}:{}` required by its execution plan",
                self.launch, workspace_coordinate, parameter_id
            )
        })?;
        if matches
            .any(|other| other.parameter_type != first.parameter_type || other.value != first.value)
        {
            return Err(format!(
                "resolved launch `{}` has ambiguous values for parameter `{}:{}`",
                self.launch, workspace_coordinate, parameter_id
            ));
        }
        Ok(first)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PathPolicy {
    allowed_roots: Vec<String>,
    must_exist: bool,
    access: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceDocument {
    instance_id: String,
    launch: String,
    included_by: Option<String>,
    parameters: BTreeMap<String, ParameterDocument>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessDocument {
    id: String,
    instance: String,
    launch: String,
    process_id: String,
    executable: String,
    artifact: String,
    argv: Vec<String>,
    environment: BTreeMap<String, Value>,
    working_directory: Option<String>,
    dependencies: BTreeMap<String, String>,
    endpoints: BTreeMap<String, EndpointDocument>,
    readiness: Value,
    restart: Value,
    shutdown: Value,
    required: bool,
    on_exit: String,
    on_readiness_loss: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EndpointDocument {
    protocol: String,
    host: Option<Value>,
    port: Option<u64>,
    path: Option<String>,
    expected_status: Option<u16>,
}

pub fn build_plan(
    interface_version: u64,
    template: &LaunchPlanTemplate,
    assignments: &[String],
) -> Result<ResolvedLaunchPlan, String> {
    let provided = parse_assignments(assignments)?;
    let root = resolve_root_parameters(template, &provided)?;
    let mut resolved_instances = BTreeMap::new();
    let mut instances = Vec::with_capacity(template.instances.len());

    for instance in &template.instances {
        let resolved = if instance.instance_id == template.launch {
            root.clone()
        } else {
            resolve_instance_parameters(template, instance, &root)?
        };
        instances.push(InstanceDocument {
            instance_id: instance.instance_id.clone(),
            launch: instance.launch.clone(),
            included_by: instance.included_by.clone(),
            parameters: parameter_documents(&resolved),
        });
        resolved_instances.insert(instance.instance_id.clone(), resolved);
    }

    let mut processes = Vec::with_capacity(template.processes.len());
    for process in &template.processes {
        let parameters = resolved_instances.get(&process.instance).ok_or_else(|| {
            format!(
                "normalized launch template `{}` has no instance `{}`",
                template.launch, process.instance
            )
        })?;
        let mut argv = process.executable_argv.clone();
        for token in &process.argv {
            let value = resolve_launch_value(
                token,
                parameters,
                &format!("launch process `{}` argv", process.id),
            )?;
            let value = value.as_str().ok_or_else(|| {
                format!(
                    "normalized launch process `{}` contains a non-scalar argv value",
                    process.id
                )
            })?;
            argv.push(value.to_owned());
        }
        let environment = process
            .environment
            .iter()
            .map(|(name, token)| {
                resolve_launch_value(
                    token,
                    parameters,
                    &format!("launch process `{}` environment `{name}`", process.id),
                )
                .map(|value| (name.clone(), value))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let endpoints = process
            .endpoints
            .iter()
            .map(|(endpoint_id, endpoint)| {
                resolve_endpoint(endpoint, parameters, &process.id, endpoint_id)
                    .map(|document| (endpoint_id.clone(), document))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        processes.push(ProcessDocument {
            id: process.id.clone(),
            instance: process.instance.clone(),
            launch: process.launch.clone(),
            process_id: process.process_id.clone(),
            executable: process.executable.clone(),
            artifact: process.artifact.clone(),
            argv,
            environment,
            working_directory: process.working_directory.clone(),
            dependencies: process.dependencies.clone(),
            endpoints,
            readiness: process.readiness.clone(),
            restart: process.restart.clone(),
            shutdown: process.shutdown.clone(),
            required: process.required,
            on_exit: process.on_exit.clone(),
            on_readiness_loss: process.on_readiness_loss.clone(),
        });
    }

    Ok(ResolvedLaunchPlan {
        interface_version,
        launch: template.launch.clone(),
        parameters: parameter_documents(&root),
        required_artifacts: template.required_artifacts.clone(),
        required_resources: template.required_resources.clone(),
        capabilities: template.capabilities.clone(),
        instances,
        processes,
    })
}

pub fn emit_human(plan: &ResolvedLaunchPlan) {
    println!("Launch plan: {}", plan.launch);
    println!("Parameters:");
    for (name, parameter) in &plan.parameters {
        println!(
            "  {name}: {} ({})",
            serde_json::to_string(&parameter.value).expect("runtime value is serializable"),
            parameter.source
        );
    }
    println!(
        "Required artifacts: {}",
        display_values(&plan.required_artifacts)
    );
    println!(
        "Required resources: {}",
        display_values(&plan.required_resources)
    );
    println!("Processes:");
    for process in &plan.processes {
        println!("  {}:", process.id);
        println!("    executable: {}", process.executable);
        println!(
            "    argv: {}",
            serde_json::to_string(&process.argv).expect("argv is serializable")
        );
        println!(
            "    environment: {}",
            serde_json::to_string(&process.environment).expect("environment is serializable")
        );
    }
}

fn display_values(values: &[String]) -> String {
    if values.is_empty() {
        "none".into()
    } else {
        values.join(", ")
    }
}

fn parse_assignments(assignments: &[String]) -> Result<BTreeMap<String, String>, String> {
    let mut provided = BTreeMap::new();
    for assignment in assignments {
        let Some((name, value)) = assignment.split_once('=') else {
            return Err("launch parameter assignment is invalid; use `--set NAME=VALUE`".into());
        };
        if !valid_id(name) {
            return Err(
                "launch parameter name is invalid; use `--set NAME=VALUE` with a declared parameter ID"
                    .into(),
            );
        }
        if provided.insert(name.to_owned(), value.to_owned()).is_some() {
            return Err(format!(
                "launch parameter `{name}` was set more than once; pass exactly one `--set {name}=VALUE`"
            ));
        }
    }
    Ok(provided)
}

fn valid_id(value: &str) -> bool {
    let mut segment_start = true;
    for (index, character) in value.chars().enumerate() {
        if index == 0 && !character.is_ascii_lowercase() {
            return false;
        }
        if matches!(character, '.' | '_' | '-') {
            if segment_start {
                return false;
            }
            segment_start = true;
        } else if character.is_ascii_lowercase() || character.is_ascii_digit() {
            segment_start = false;
        } else {
            return false;
        }
    }
    !value.is_empty() && !segment_start
}

fn resolve_root_parameters(
    template: &LaunchPlanTemplate,
    provided: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, ResolvedParameter>, String> {
    if let Some(unknown) = provided
        .keys()
        .find(|name| !template.parameters.contains_key(*name))
    {
        let available = if template.parameters.is_empty() {
            "none".into()
        } else {
            template
                .parameters
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        };
        return Err(format!(
            "launch `{}` has no parameter `{unknown}`; choose one of: {available}",
            template.launch
        ));
    }

    template
        .parameters
        .iter()
        .map(|(name, definition)| {
            let resolved = if definition.parameter_type == ParameterType::Secret {
                if provided.contains_key(name) {
                    return Err(format!(
                        "launch `{}` secret parameter `{name}` cannot be passed with `--set`; use a runtime secret provider",
                        template.launch
                    ));
                }
                resolve_secret(definition)
            } else if let Some(raw) = provided.get(name) {
                ResolvedParameter {
                    definition: definition.clone(),
                    value: Some(parse_runtime_value(
                        name,
                        definition,
                        raw,
                        &template.launch,
                    )?),
                    source: "set",
                }
            } else {
                resolve_default(name, definition, &template.launch)?
            };
            Ok((name.clone(), resolved))
        })
        .collect()
}

fn resolve_instance_parameters(
    template: &LaunchPlanTemplate,
    instance: &crate::model::LaunchInstanceTemplate,
    root: &BTreeMap<String, ResolvedParameter>,
) -> Result<BTreeMap<String, ResolvedParameter>, String> {
    instance
        .parameters
        .iter()
        .map(|(name, definition)| {
            let resolved = match instance.parameter_bindings.get(name).and_then(|value| value.as_ref()) {
                Some(root_name) => {
                    let forwarded = root.get(root_name).ok_or_else(|| {
                        format!(
                            "normalized launch template `{}` binds `{name}` to missing root parameter `{root_name}`",
                            template.launch
                        )
                    })?;
                    if forwarded.value.is_none() && definition.required {
                        return Err(format!(
                            "launch `{}` include `{}` cannot resolve required parameter `{name}` from unset `{root_name}`; pass `--set {root_name}=VALUE` or declare a default",
                            template.launch, instance.instance_id
                        ));
                    }
                    ResolvedParameter {
                        definition: definition.clone(),
                        value: forwarded.value.clone(),
                        source: if definition.parameter_type == ParameterType::Secret
                            && forwarded.value.is_some()
                        {
                            "runtime-secret"
                        } else if forwarded.value.is_some() {
                            "forwarded"
                        } else {
                            "unset"
                        },
                    }
                }
                None if definition.parameter_type == ParameterType::Secret => resolve_secret(definition),
                None => resolve_default(name, definition, &instance.launch)?,
            };
            Ok((name.clone(), resolved))
        })
        .collect()
}

fn resolve_secret(definition: &ParameterDefinition) -> ResolvedParameter {
    ResolvedParameter {
        definition: definition.clone(),
        value: definition
            .required
            .then(|| Value::String(REDACTED_SECRET.into())),
        source: if definition.required {
            "runtime-secret"
        } else {
            "unset"
        },
    }
}

fn resolve_default(
    name: &str,
    definition: &ParameterDefinition,
    launch: &str,
) -> Result<ResolvedParameter, String> {
    if let Some(value) = &definition.default {
        Ok(ResolvedParameter {
            definition: definition.clone(),
            value: Some(value.clone()),
            source: "default",
        })
    } else if definition.required {
        Err(format!(
            "launch `{launch}` requires parameter `{name}`; pass `--set {name}=VALUE`"
        ))
    } else {
        Ok(ResolvedParameter {
            definition: definition.clone(),
            value: None,
            source: "unset",
        })
    }
}

fn parse_runtime_value(
    name: &str,
    definition: &ParameterDefinition,
    raw: &str,
    launch: &str,
) -> Result<Value, String> {
    let invalid = || {
        format!(
            "launch `{launch}` parameter `{name}` has an invalid `{}` value; pass a valid value with `--set {name}=VALUE`",
            definition.parameter_type.name()
        )
    };
    let value = match definition.parameter_type {
        ParameterType::String => Value::String(raw.into()),
        ParameterType::Integer => raw
            .parse::<i64>()
            .map(Number::from)
            .map(Value::Number)
            .map_err(|_| invalid())?,
        ParameterType::Float => {
            let parsed = raw.parse::<f64>().map_err(|_| invalid())?;
            if !parsed.is_finite() {
                return Err(invalid());
            }
            Value::Number(Number::from_f64(parsed).ok_or_else(invalid)?)
        }
        ParameterType::Boolean => match raw {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => return Err(invalid()),
        },
        ParameterType::Enum => {
            if !definition.enum_values.iter().any(|value| value == raw) {
                return Err(invalid());
            }
            Value::String(raw.into())
        }
        ParameterType::Host | ParameterType::Url => {
            if raw.trim().is_empty() {
                return Err(invalid());
            }
            Value::String(raw.into())
        }
        ParameterType::Port => {
            let parsed = raw.parse::<u64>().map_err(|_| invalid())?;
            if !(1..=65535).contains(&parsed) {
                return Err(invalid());
            }
            Value::Number(Number::from(parsed))
        }
        ParameterType::Duration => {
            let parsed = raw.parse::<u64>().map_err(|_| invalid())?;
            Value::Number(Number::from(parsed))
        }
        ParameterType::Path => {
            if !valid_runtime_path(raw, &definition.allowed_roots) {
                return Err(invalid());
            }
            Value::String(raw.into())
        }
        ParameterType::Secret => return Err(invalid()),
    };

    if let Some(number) = value.as_f64() {
        if definition.minimum.is_some_and(|minimum| number < minimum) {
            return Err(format!(
                "launch `{launch}` parameter `{name}` is below its minimum {}; pass `--set {name}=VALUE` within the declared range",
                definition.minimum.expect("checked")
            ));
        }
        if definition.maximum.is_some_and(|maximum| number > maximum) {
            return Err(format!(
                "launch `{launch}` parameter `{name}` is above its maximum {}; pass `--set {name}=VALUE` within the declared range",
                definition.maximum.expect("checked")
            ));
        }
    }
    Ok(value)
}

fn valid_runtime_path(value: &str, allowed_roots: &[String]) -> bool {
    if value.is_empty() || value.as_bytes().contains(&0) {
        return false;
    }
    let normalized = normalize_runtime_path(value);
    if normalized.is_none() {
        return false;
    }
    let normalized = normalized.expect("checked");
    allowed_roots.iter().any(|root| {
        let Some(root) = normalize_runtime_path(root) else {
            return false;
        };
        if root == "/" {
            normalized.starts_with('/')
        } else if root == "." {
            !normalized.starts_with('/')
        } else {
            normalized == root || normalized.starts_with(&format!("{root}/"))
        }
    })
}

fn normalize_runtime_path(value: &str) -> Option<String> {
    let portable = value.replace('\\', "/");
    let absolute = portable.starts_with('/');
    let mut parts = Vec::new();
    for part in portable.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            value => parts.push(value),
        }
    }
    let joined = parts.join("/");
    Some(if absolute {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".into()
    } else {
        joined
    })
}

fn parameter_documents(
    parameters: &BTreeMap<String, ResolvedParameter>,
) -> BTreeMap<String, ParameterDocument> {
    parameters
        .iter()
        .map(|(name, parameter)| {
            let policy =
                (parameter.definition.parameter_type == ParameterType::Path).then(|| PathPolicy {
                    allowed_roots: parameter.definition.allowed_roots.clone(),
                    must_exist: parameter.definition.must_exist,
                    access: parameter.definition.access.clone(),
                });
            (
                name.clone(),
                ParameterDocument {
                    parameter_type: parameter.definition.parameter_type,
                    source: parameter.source,
                    value: parameter.value.clone().unwrap_or(Value::Null),
                    allocation: parameter.definition.allocation,
                    policy,
                },
            )
        })
        .collect()
}

fn resolve_launch_value(
    token: &LaunchValue,
    parameters: &BTreeMap<String, ResolvedParameter>,
    context: &str,
) -> Result<Value, String> {
    if let Some(literal) = &token.literal {
        return Ok(Value::String(literal.clone()));
    }
    let name = token.parameter.as_ref().ok_or_else(|| {
        format!("normalized {context} has neither a literal nor a parameter reference")
    })?;
    let parameter = parameters
        .get(name)
        .ok_or_else(|| format!("normalized {context} references unknown parameter `{name}`"))?;
    let value = parameter.value.as_ref().ok_or_else(|| {
        format!(
            "{context} references unset parameter `{name}`; pass `--set {name}=VALUE` or declare a default"
        )
    })?;
    if parameter.definition.parameter_type == ParameterType::Secret {
        Ok(json!({
            "secretParameter": name,
            "resolved": false,
            "value": REDACTED_SECRET,
        }))
    } else {
        Ok(Value::String(format!(
            "{}{}{}",
            token.prefix,
            runtime_scalar_string(value)?,
            token.suffix
        )))
    }
}

fn runtime_scalar_string(value: &Value) -> Result<String, String> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Number(value) => Ok(value.to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        _ => Err("normalized runtime parameter is not a JSON scalar".into()),
    }
}

fn resolve_endpoint(
    endpoint: &EndpointTemplate,
    parameters: &BTreeMap<String, ResolvedParameter>,
    process: &str,
    endpoint_id: &str,
) -> Result<EndpointDocument, String> {
    let context = format!("launch process `{process}` endpoint `{endpoint_id}`");
    let host = endpoint
        .host_parameter
        .as_ref()
        .map(|parameter| {
            resolve_launch_value(
                &LaunchValue {
                    literal: None,
                    parameter: Some(parameter.clone()),
                    prefix: String::new(),
                    suffix: String::new(),
                },
                parameters,
                &context,
            )
        })
        .transpose()?;
    let port = endpoint
        .port_parameter
        .as_ref()
        .map(|parameter| {
            parameters
                .get(parameter)
                .and_then(|resolved| resolved.value.as_ref())
                .and_then(Value::as_u64)
                .ok_or_else(|| format!("{context} has an unresolved port parameter `{parameter}`"))
        })
        .transpose()?;
    Ok(EndpointDocument {
        protocol: endpoint.protocol.clone(),
        host,
        port,
        path: endpoint.path.clone(),
        expected_status: endpoint.expected_status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition(parameter_type: ParameterType) -> ParameterDefinition {
        ParameterDefinition {
            parameter_type,
            description: String::new(),
            required: false,
            default: None,
            enum_values: Vec::new(),
            minimum: None,
            maximum: None,
            allowed_roots: Vec::new(),
            must_exist: false,
            access: "read".into(),
            allocation: PortAllocation::Fixed,
            allocation_host: "127.0.0.1".into(),
            allocation_transport: crate::model::PortTransport::Tcp,
        }
    }

    fn accepts(parameter_type: ParameterType, raw: &str) -> bool {
        parse_runtime_value("value", &definition(parameter_type), raw, "app/test").is_ok()
    }

    #[test]
    fn every_runtime_scalar_type_is_parsed_strictly() {
        assert!(accepts(ParameterType::String, ""));
        assert!(accepts(ParameterType::Integer, "-4"));
        assert!(!accepts(ParameterType::Integer, "4.0"));
        assert!(accepts(ParameterType::Float, "1.25"));
        assert!(!accepts(ParameterType::Float, "NaN"));
        assert!(!accepts(ParameterType::Float, "inf"));
        assert!(accepts(ParameterType::Boolean, "true"));
        assert!(!accepts(ParameterType::Boolean, "yes"));

        let mut enumeration = definition(ParameterType::Enum);
        enumeration.enum_values = vec!["safe".into(), "fast".into()];
        assert!(parse_runtime_value("mode", &enumeration, "safe", "app/test").is_ok());
        assert!(parse_runtime_value("mode", &enumeration, "unsafe", "app/test").is_err());

        assert!(accepts(ParameterType::Host, "localhost"));
        assert!(!accepts(ParameterType::Host, "  "));
        assert!(accepts(ParameterType::Url, "https://example.test"));
        assert!(!accepts(ParameterType::Url, ""));
        assert!(accepts(ParameterType::Port, "1"));
        assert!(accepts(ParameterType::Port, "65535"));
        assert!(!accepts(ParameterType::Port, "0"));
        assert!(!accepts(ParameterType::Port, "65536"));
        assert!(accepts(ParameterType::Duration, "0"));
        assert!(!accepts(ParameterType::Duration, "-1"));
        assert!(!accepts(ParameterType::Secret, "plaintext"));
    }

    #[test]
    fn numeric_bounds_and_portable_path_roots_are_enforced() {
        let mut bounded = definition(ParameterType::Float);
        bounded.minimum = Some(0.5);
        bounded.maximum = Some(2.0);
        assert!(parse_runtime_value("ratio", &bounded, "0.5", "app/test").is_ok());
        assert!(parse_runtime_value("ratio", &bounded, "2", "app/test").is_ok());
        assert!(parse_runtime_value("ratio", &bounded, "0.49", "app/test").is_err());
        assert!(parse_runtime_value("ratio", &bounded, "2.01", "app/test").is_err());

        let mut path = definition(ParameterType::Path);
        path.allowed_roots = vec!["config".into()];
        assert!(parse_runtime_value("path", &path, "config/app.json", "app/test").is_ok());
        assert!(parse_runtime_value("path", &path, "config\\app.json", "app/test").is_ok());
        assert!(parse_runtime_value("path", &path, "../config/app.json", "app/test").is_err());
        assert!(parse_runtime_value("path", &path, "/etc/passwd", "app/test").is_err());
    }
}
