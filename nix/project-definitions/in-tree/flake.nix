{
  description = "In-tree CogniPilot project authority fixture";

  outputs = _: {
    flakeModules.default = import ./module.nix;
  };
}
