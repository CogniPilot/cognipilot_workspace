{
  description = "CogniPilot integration definition for FastDyn";

  outputs = _: {
    flakeModules.default = import ./module.nix;
  };
}
