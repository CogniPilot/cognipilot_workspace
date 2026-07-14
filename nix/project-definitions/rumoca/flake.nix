{
  description = "CogniPilot integration definition for rumoca";

  outputs = _: {
    flakeModules.default = import ./module.nix;
  };
}
