{
  description = "CogniPilot integration definition for electrode_web";

  outputs = _: {
    flakeModules.default = import ./module.nix;
  };
}
