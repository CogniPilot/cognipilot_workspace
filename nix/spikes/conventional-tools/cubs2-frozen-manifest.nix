let
  # Evaluation-only placeholder. west2nix requires a real fixed-output hash for
  # every project before any derivation may be built.
  hash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
  project =
    name: url: revision: path:
    {
      inherit name url revision;
      nix = { inherit hash; };
    }
    // (if path == name then { } else { inherit path; });
in
{
  manifest = {
    projects = [
      (project "zephyr" "https://github.com/CogniPilot/zephyr" "c908d12ebc2baa679d0f5b8e31eeb459ea93caff" "zephyr")
      (project "cmsis" "https://github.com/zephyrproject-rtos/cmsis" "512cc7e895e8491696b61f7ba8066b4a182569b8" "modules/hal/cmsis")
      (project "cmsis_6" "https://github.com/zephyrproject-rtos/cmsis_6" "30a859f44ef8ab4dc8f84b03ed586fd16ccf9d74" "modules/hal/cmsis_6")
      (project "hal_nxp" "https://github.com/CogniPilot/hal_nxp" "cf8f1631e862528ee044850bcc70273975b90771" "modules/hal/nxp")
      (project "zros" "https://github.com/CogniPilot/zros" "959bce412d6e12b3a49fb8b55c219affcf7f6d0e" "modules/lib/zros")
      (project "csyn" "https://github.com/CogniPilot/csyn" "c1fdc903b5eb48e9a4362c716f50e658bed15961" "modules/lib/csyn")
      (project "cerebri_modules" "https://github.com/CogniPilot/cerebri_modules" "ef73a4f8adeb4385c34af4344c9af27e43e02033" "modules/lib/cerebri_lockstep")
      (project "modelica_models" "https://github.com/CogniPilot/modelica_models" "62ea9f97cec28e092c8e67c9f4a0dbb842f6233b" "models/vendor/CMM-v0.0.2")
      (project "zenoh-pico" "https://github.com/cognipilot/zenoh-pico.git" "cb9d8391c8e3c394fd288f2299c2b6758523b1c6" "modules/lib/zenoh-pico")
      (project "zephyr_boards" "https://github.com/CogniPilot/zephyr_boards" "d006ec84e621ee33f43c233a96380c8c3f6470eb" "modules/lib/zephyr_boards")
    ];
    self = {
      path = "cerebri_cubs2";
      "west-commands" = "scripts/west-commands.yml";
    };
  };
}
