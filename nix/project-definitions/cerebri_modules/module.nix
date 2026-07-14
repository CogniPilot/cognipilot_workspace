{
  cognipilot.projects.cerebri_modules = {
    lifecycle = "stable";
    deployability = "qualification";
    owner = "CogniPilot";
    license.spdx = "Apache-2.0";
    source.visibility = "public";
    definition.origin = "external";
    preset = "twister-v1";

    targets.default = {
      artifacts.outputs = {
        build-report = {
          producedBy = "build";
          kind = "directory";
          path = "build/twister/build";
          contract = {
            name = "twister-build-report";
            version = 1;
          };
        };
        test-report = {
          producedBy = "test";
          kind = "directory";
          path = "build/twister/test";
          contract = {
            name = "twister-test-report";
            version = 1;
          };
        };
      };
      actionRequirements = {
        build.exclusiveLocks = [ "west-workspace" ];
        test.exclusiveLocks = [ "west-workspace" ];
      };
    };

    resources.zephyr-module = {
      kind = "configuration";
      path = "zephyr/module.yml";
    };
  };
}
