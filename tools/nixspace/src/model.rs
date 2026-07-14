use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Index {
    pub api_version: String,
    pub kind: String,
    pub interface_version: u64,
    pub catalog: Catalog,
    pub graph: StaticGraph,
    pub launch_plans: BTreeMap<String, LaunchPlanTemplate>,
    pub action_plans: ActionPlans,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionPlans {
    pub schema_version: u64,
    pub runner: ActionRunner,
    pub actions: BTreeMap<String, ActionSelection>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRunner {
    pub kind: String,
    pub direct: DirectActionRunner,
    pub bootstrap: BootstrapActionRunner,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DirectActionRunner {
    pub argv: Vec<String>,
    pub required_environment: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapActionRunner {
    pub argv: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ActionSelection {
    pub all: Vec<String>,
    pub packages: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct Catalog {
    pub packages: Vec<PackageDocument>,
    pub targets: Vec<TargetDocument>,
    pub artifacts: Vec<ArtifactDocument>,
    pub resources: Vec<ResourceDocument>,
    pub executables: Vec<ExecutableDocument>,
    pub launches: Vec<LaunchDocument>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageDocument {
    pub id: String,
    pub package_id: String,
    pub aliases: Vec<String>,
    pub software_version: SoftwareVersion,
    pub lifecycle: String,
    pub deployability: String,
    pub owner: Option<String>,
    pub license: License,
    pub repository_id: String,
    pub preset: String,
    pub source: Source,
    pub targets: BTreeMap<String, Value>,
    pub resources: BTreeMap<String, Value>,
    pub executables: BTreeMap<String, Value>,
    pub launches: BTreeMap<String, Value>,
    pub compliance: Compliance,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareVersion {
    pub source: String,
    pub value: Option<String>,
    pub file: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct License {
    pub spdx: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Source {
    pub input: String,
    pub root: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Compliance {
    pub bespoke_adapter_count: u64,
    pub warnings: Vec<String>,
    pub warning_count: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetDocument {
    pub coordinate: String,
    pub package_id: String,
    pub target: Target,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Target {
    pub id: String,
    pub actions: BTreeMap<String, Value>,
    pub artifacts: TargetArtifacts,
    pub variants: Value,
    pub release: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TargetArtifacts {
    pub outputs: BTreeMap<String, Value>,
    pub inputs: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactDocument {
    pub coordinate: String,
    pub package_id: String,
    pub target_id: String,
    pub artifact_id: String,
    pub kind: String,
    pub path: String,
    pub contract: ArtifactContract,
    pub produced_by: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ArtifactContract {
    pub name: String,
    pub version: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDocument {
    pub coordinate: String,
    pub package_id: String,
    pub resource_id: String,
    pub kind: String,
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableDocument {
    pub coordinate: String,
    pub package_id: String,
    pub executable_id: String,
    pub from: String,
    pub argv: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchDocument {
    pub coordinate: String,
    pub package_id: String,
    pub launch_id: String,
    pub launch: LaunchSummary,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSummary {
    pub description: String,
    pub parameters: BTreeMap<String, ParameterDefinition>,
    pub required_artifacts: Vec<String>,
    pub required_resources: Vec<String>,
    pub processes: BTreeMap<String, Value>,
    pub includes: BTreeMap<String, Value>,
    pub capabilities: Value,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaticGraph {
    pub schema_version: u64,
    pub all: GraphDocument,
    pub packages: BTreeMap<String, GraphDocument>,
    pub reverse: BTreeMap<String, GraphDocument>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GraphDocument {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GraphNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub package: String,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub producer_artifact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumer_input: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract: Option<ArtifactContract>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchPlanTemplate {
    pub launch: String,
    pub parameters: BTreeMap<String, ParameterDefinition>,
    pub required_artifacts: Vec<String>,
    pub required_resources: Vec<String>,
    pub capabilities: Capabilities,
    pub instances: Vec<LaunchInstanceTemplate>,
    pub processes: Vec<LaunchProcessTemplate>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Capabilities {
    pub provides: Vec<String>,
    pub requires: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchInstanceTemplate {
    pub instance_id: String,
    pub launch: String,
    pub included_by: Option<String>,
    pub parameter_bindings: BTreeMap<String, Option<String>>,
    pub parameters: BTreeMap<String, ParameterDefinition>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchProcessTemplate {
    pub id: String,
    pub instance: String,
    pub launch: String,
    pub process_id: String,
    pub executable: String,
    pub artifact: String,
    pub executable_argv: Vec<String>,
    pub argv: Vec<LaunchValue>,
    pub environment: BTreeMap<String, LaunchValue>,
    pub working_directory: Option<String>,
    pub dependencies: BTreeMap<String, String>,
    pub endpoints: BTreeMap<String, EndpointTemplate>,
    pub readiness: Value,
    pub restart: Value,
    pub shutdown: Value,
    pub required: bool,
    pub on_exit: String,
    pub on_readiness_loss: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LaunchValue {
    pub literal: Option<String>,
    pub parameter: Option<String>,
    pub prefix: String,
    pub suffix: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointTemplate {
    pub protocol: String,
    pub host_parameter: Option<String>,
    pub port_parameter: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterDefinition {
    #[serde(rename = "type")]
    pub parameter_type: ParameterType,
    pub description: String,
    pub required: bool,
    pub default: Option<Value>,
    pub enum_values: Vec<String>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub allowed_roots: Vec<String>,
    pub must_exist: bool,
    pub access: String,
    pub allocation: PortAllocation,
    pub allocation_host: String,
    pub allocation_transport: PortTransport,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PortAllocation {
    #[default]
    Fixed,
    Automatic,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PortTransport {
    #[default]
    Tcp,
    Udp,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ParameterType {
    String,
    Integer,
    Float,
    Boolean,
    Enum,
    Host,
    Url,
    Port,
    Duration,
    Path,
    Secret,
}

impl ParameterType {
    pub fn name(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Integer => "integer",
            Self::Float => "float",
            Self::Boolean => "boolean",
            Self::Enum => "enum",
            Self::Host => "host",
            Self::Url => "url",
            Self::Port => "port",
            Self::Duration => "duration",
            Self::Path => "path",
            Self::Secret => "secret",
        }
    }
}
