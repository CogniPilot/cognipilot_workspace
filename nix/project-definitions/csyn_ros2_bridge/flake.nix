{
  description = "CogniPilot integration definition for csyn_ros2_bridge";

  outputs = _: {
    flakeModules.default = import ./module.nix;
  };
}
