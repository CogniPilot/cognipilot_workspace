{
  description = "CogniPilot integration definition for synapse_fbs";

  outputs = _: {
    flakeModules.default = import ./module.nix;
  };
}
