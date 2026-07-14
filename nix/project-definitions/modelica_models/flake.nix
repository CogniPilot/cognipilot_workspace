{
  description = "CogniPilot integration definition for modelica_models";

  outputs = _: {
    flakeModules.default = import ./module.nix;
  };
}
