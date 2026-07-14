{
  description = "CogniPilot integration definition for cerebri_cubs2";

  outputs = _: {
    flakeModules.default = import ./module.nix;
  };
}
