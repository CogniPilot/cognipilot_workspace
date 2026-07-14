{
  description = "CogniPilot integration definition for qualisys_rust_sdk";

  outputs = _: {
    flakeModules.default = import ./module.nix;
  };
}
