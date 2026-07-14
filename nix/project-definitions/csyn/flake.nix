{
  description = "CogniPilot integration definition for csyn";

  outputs = _: {
    flakeModules.default = import ./module.nix;
  };
}
