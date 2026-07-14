{
  cognipilot.projects.csyn_ros2_bridge = {
    lifecycle = "experimental";
    deployability = "qualification";
    owner = "CogniPilot";
    license.spdx = "Apache-2.0";
    source.visibility = "public";
    # Synapse is pinned and materialized by the source input's recursive Git
    # submodule. CSyn remains a sibling editable source dependency.
    source.dependencies = [ "csyn" ];
    definition.origin = "external";
    preset = "colcon-v1";

    targets.default.artifacts.outputs.ci-runner = {
      producedBy = "build";
      kind = "executable";
      path = "result-ci/bin/csyn-ros2-ci";
      contract = {
        name = "csyn-ros2-ci";
        version = 1;
      };
    };
    executables.ci.from = "csyn_ros2_bridge:default:ci-runner";
  };
}
