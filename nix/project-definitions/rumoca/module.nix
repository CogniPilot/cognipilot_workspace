{
  cognipilot.projects.rumoca = {
    lifecycle = "stable";
    deployability = "qualification";
    owner = "CogniPilot";
    license.spdx = "Apache-2.0";
    source.visibility = "public";
    definition.origin = "external";
    preset = "rumoca-v1";
    targets.default.artifacts.outputs = {
      compiler = {
        producedBy = "compiler-build";
        kind = "executable";
        path = "result-rumoca/bin/rumoca";
        contract = {
          name = "rumoca-compiler";
          version = 1;
        };
      };
      python = {
        producedBy = "python-build";
        kind = "executable";
        path = "result-rumoca-python/bin/python";
        contract = {
          name = "rumoca-python-environment";
          version = 1;
        };
      };
      javascript = {
        producedBy = "javascript-build";
        kind = "directory";
        path = "packages/rumoca/dist/dev-core";
        contract = {
          name = "rumoca-javascript-package";
          version = 1;
        };
      };
    };
  };
}
