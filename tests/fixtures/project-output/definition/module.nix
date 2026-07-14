{
  cognipilot.projects.fake-app = {
    lifecycle = "stable";
    deployability = "qualification";
    owner = "CogniPilot Test";
    license.spdx = "Apache-2.0";
    repositoryId = "fake-app";
    source.input = "fake_source";
    definition = {
      origin = "external";
      input = "fake_definition";
    };
    preset = "cargo-v1";
  };
}
