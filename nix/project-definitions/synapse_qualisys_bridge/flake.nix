{
  description = "CogniPilot integration definition for synapse_qualisys_bridge";

  outputs = _: {
    flakeModules.default = import ./module.nix;
  };
}
