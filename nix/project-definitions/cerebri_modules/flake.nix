{
  description = "CogniPilot integration definition for cerebri_modules";

  outputs = _: {
    flakeModules.default = import ./module.nix;
  };
}
