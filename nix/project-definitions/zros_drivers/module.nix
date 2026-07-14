{
  cognipilot.projects.zros_drivers = {
    lifecycle = "experimental";
    deployability = "qualification";
    owner = "CogniPilot";
    license.spdx = "Apache-2.0";
    source.visibility = "public";
    source.dependencies = [
      "synapse_fbs"
      "zros"
    ];
    definition.origin = "external";
    preset = "resource-only-v1";

    resources.zephyr-module = {
      kind = "configuration";
      path = "zephyr/module.yml";
    };
  };
}
