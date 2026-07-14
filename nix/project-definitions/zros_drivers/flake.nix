{
  description = "CogniPilot integration definition for zros_drivers";

  outputs = _: {
    flakeModules.default = import ./module.nix;
  };
}
