let
  pkgs = import <nixpkgs> { };
  evaluate = modules: (pkgs.lib.evalModules { inherit modules; }).config.cognipilot.validatedIndex;
  graphIndex = evaluate [ ./fixtures/project-flakes/golden/semantic-dag.nix ];
  launchIndex = evaluate [ ./fixtures/project-flakes/golden/launch-ir.nix ];
  actionIndex = evaluate [
    ../nix/cognipilot/flake-module.nix
    {
      cognipilot.projects.app = {
        repositoryId = "app";
        source.input = "app-source";
        preset = "cargo-v1";
        customActions.package = {
          kind = "other";
          argv = [ "package-app" ];
          dependsOn = [ "build" ];
        };
      };
    }
  ];
  hasNode = id: nodes: builtins.any (node: node.id == id) nodes;
  hasEdge = from: to: kind: edges: builtins.any (
    edge: edge.from == from && edge.to == to && edge.kind == kind
  ) edges;
  stack = launchIndex.launchPlans."app/stack";
in
assert graphIndex.graph.schemaVersion == 1;
assert hasNode "codegen/schema/build" graphIndex.graph.all.nodes;
assert hasNode "codegen/schema" graphIndex.graph.packages.flight.nodes;
assert hasNode "flight/firmware/build" graphIndex.graph.reverse.codegen.nodes;
assert hasEdge "codegen/schema" "flight/firmware" "artifact" graphIndex.graph.all.edges;
assert hasEdge "app/default/build" "app/default/package" "action" actionIndex.graph.all.edges;
assert map (launch: launch.coordinate) launchIndex.catalog.launches == [
  "app/router"
  "app/stack"
];
assert map (instance: instance.instanceId) stack.instances == [
  "app/stack"
  "app/stack/base"
];
assert (builtins.elemAt stack.instances 1).parameterBindings.host == "router-host";
assert (builtins.elemAt stack.processes 0).dependencies == {
  "app/stack/base/router" = "ready";
};
assert stack.requiredArtifacts == [ "app:default:router-bin" ];
assert stack.requiredResources == [ "app/router-config" ];
{
  success = true;
  interfaceVersion = launchIndex.interfaceVersion;
}
