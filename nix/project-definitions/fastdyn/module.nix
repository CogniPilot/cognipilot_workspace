{
  cognipilot.projects.fastdyn = {
    lifecycle = "experimental";
    deployability = "local-only";
    owner = "jgoppert";
    source.visibility = "private";
    definition.origin = "external";
    # FastDyn owns its QEMU/virtualenv provisioning workflow in setup.sh.  The
    # external definition supplies only the exact argv and a Nix-owned tool
    # profile; no workspace control-plane shell is introduced.
    preset = "resource-only-v1";

    customActions = {
      build = {
        kind = "build";
        toolProfile = "fastdyn-qemu-v1";
        argv = [
          "./setup.sh"
          "--python"
          "python3"
          "--venv"
          "build/venv"
          "--qemu-root"
          "build/qemu"
          "--build-qemu"
          "--skip-optifuzz"
          "--skip-qemu-workspace"
        ];
      };
      test = {
        kind = "test";
        dependsOn = [ "build" ];
        argv = [
          "build/venv/bin/python"
          "-m"
          "pytest"
          "tests/unit"
        ];
      };
    };

    targets.default.artifacts.outputs = {
      plugin = {
        producedBy = "build";
        kind = "file";
        path = "build/libfastdyn.so";
        contract = {
          name = "fastdyn-plugin";
          version = 1;
        };
      };
      python = {
        producedBy = "build";
        kind = "executable";
        path = "build/venv/bin/python";
        contract = {
          name = "fastdyn-python-environment";
          version = 1;
        };
      };
      qemu = {
        producedBy = "build";
        kind = "executable";
        path = "build/qemu/build/qemu-system-arm";
        contract = {
          name = "fastdyn-patched-qemu";
          version = 1;
        };
      };
    };

    resources.configurations = {
      kind = "configuration";
      path = "configs";
    };
  };
}
