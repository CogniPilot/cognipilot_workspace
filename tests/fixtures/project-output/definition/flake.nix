{
  description = "External definition for the fake unmodified source";

  outputs = _: {
    flakeModules.default = import ./module.nix;
  };
}
