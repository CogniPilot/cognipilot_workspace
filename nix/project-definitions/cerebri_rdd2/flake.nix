{
  description = "CogniPilot integration definition for cerebri_rdd2";

  outputs = _: {
    flakeModules.default = import ./module.nix;
  };
}
