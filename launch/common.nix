{ config, ... }:

{
  process.manager.implementation = "process-compose";
  process.managers.process-compose.tui.enable = true;

  env.COGNIPILOT_LAUNCH_STATE = "${config.devenv.state}/launch";
}
