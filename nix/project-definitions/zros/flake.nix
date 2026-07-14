{
  description = "CogniPilot integration definition for zros";

  outputs = _: {
    flakeModules.default = import ./module.nix;
  };
}
