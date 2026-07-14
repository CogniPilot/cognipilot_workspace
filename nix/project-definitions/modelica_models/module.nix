{
  cognipilot.projects.modelica_models = {
    lifecycle = "stable";
    deployability = "qualification";
    owner = "CogniPilot";
    license.spdx = "Apache-2.0";
    source.visibility = "public";
    definition.origin = "external";
    preset = "nix-flake-app-v1";

    targets.default.artifacts = {
      inputs.rumoca-compiler = {
        from = "rumoca:default:compiler";
        consumedBy = [ "test" ];
        environment = "MODELICA_MODELS_RUMOCA";
        contract = {
          name = "rumoca-compiler";
          version = 1;
        };
      };
      outputs = {
        ci-runner = {
          producedBy = "build";
          kind = "executable";
          path = "result-default/bin/modelica-models-ci";
          contract = {
            name = "modelica-models-ci";
            version = 1;
          };
        };
        planning-results = {
          producedBy = "test";
          kind = "directory";
          path = "artifacts/planning";
          contract = {
            name = "modelica-planning-results";
            version = 1;
          };
        };
      };
    };

    resources.models = {
      kind = "model";
      path = ".";
    };
    executables.ci.from = "modelica_models:default:ci-runner";
  };
}
