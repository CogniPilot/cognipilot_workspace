{
  description = "External CogniPilot project authority fixture";

  outputs = _: {
    flakeModules.default = import ./module.nix;
  };
}
