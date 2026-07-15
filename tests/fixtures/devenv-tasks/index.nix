{
  interfaceVersion = 1;
  projects = {
    alpha-definition = {
      packageId = "alpha";
      repositoryId = "alpha-repo";
      source = {
        input = "alpha-source";
        root = ".";
        visibility = "private";
      };
      targets.default = {
        variants = {
          dimensions.mode = {
            values = [
              "debug"
              "release"
            ];
            default = "debug";
          };
          allowedCombinations = [ ];
        };
        artifacts = {
          outputs.bundle = {
            producedBy = "build";
            kind = "directory";
            path = "dist";
            contract = {
              name = "alpha-api";
              version = 1;
            };
          };
          inputs = { };
        };
        actions = {
          build = {
            kind = "build";
            adapter = "cargo-v1";
            cacheExcludes = [ "target" ];
            cacheInputs = [
              "Cargo.toml"
              "**/*.rs"
            ];
            dependsOn = [ ];
            environment = { };
            argv = [
              "nix"
              "develop"
              "--no-pure-eval"
              "."
              "-c"
              "cargo"
              "build"
              "--workspace"
            ];
            requirements = {
              cpu = 2;
              memoryMiB = 1024;
              exclusiveLocks = [
                "shared-cache"
                "cargo-target"
              ];
            };
          };
          test = {
            kind = "test";
            adapter = "cargo-v1";
            cacheExcludes = [ "target" ];
            cacheInputs = [
              "Cargo.toml"
              "**/*.rs"
            ];
            environment = { };
            dependsOn = [ "build" ];
            argv = [
              "nix"
              "develop"
              "--no-pure-eval"
              "."
              "-c"
              "cargo"
              "test"
              "--workspace"
            ];
            requirements = {
              cpu = null;
              memoryMiB = null;
              exclusiveLocks = [ ];
            };
          };
          generate = {
            kind = "generate";
            adapter = "bespoke-v1";
            cacheExcludes = [ ];
            cacheInputs = [ "schema/*.fbs" ];
            environment.DANGEROUS_LITERAL = "$(not-expanded); still literal";
            argv = [
              "cargo"
              "xtask"
              "generate; touch /tmp/not-allowed"
              "$(not-allowed)"
            ];
            dependsOn = [ "build" ];
            requirements = {
              cpu = null;
              memoryMiB = null;
              exclusiveLocks = [ ];
            };
          };
          docs = {
            kind = "other";
            adapter = "bespoke-v1";
            cacheExcludes = [ ];
            cacheInputs = [ "**/*.md" ];
            dependsOn = [ ];
            environment = { };
            argv = [
              "cargo"
              "doc"
              "--no-deps"
            ];
            requirements = {
              cpu = null;
              memoryMiB = null;
              exclusiveLocks = [ ];
            };
          };
        };
      };
    };

    beta-definition = {
      packageId = "beta";
      repositoryId = "beta-repo";
      source = {
        input = "beta-source";
        root = "firmware";
        visibility = "private";
      };
      targets.default = {
        variants = {
          dimensions = { };
          allowedCombinations = [ ];
        };
        artifacts = {
          outputs = { };
          inputs.alpha = {
            consumedBy = [ "build" ];
            from = "alpha:default:bundle";
            environment = "ALPHA_BUNDLE";
            contract = {
              name = "alpha-api";
              version = 1;
            };
          };
        };
        actions = {
          build = {
            kind = "build";
            adapter = "cmake-v1";
            cacheExcludes = [ "build" ];
            cacheInputs = [
              "CMakeLists.txt"
              "**/*.c"
            ];
            dependsOn = [ ];
            environment = { };
            argv = [
              "nix"
              "develop"
              "--no-pure-eval"
              "."
              "-c"
              "cmake"
              "--build"
              "build"
              {
                artifactInput = "alpha";
                prefix = "-DALPHA_BUNDLE=";
                suffix = "";
              }
            ];
            requirements = {
              cpu = null;
              memoryMiB = null;
              exclusiveLocks = [ ];
            };
          };
          test = {
            kind = "test";
            adapter = "cmake-v1";
            cacheExcludes = [ "build" ];
            cacheInputs = [
              "CMakeLists.txt"
              "**/*.c"
            ];
            environment = { };
            dependsOn = [ "build" ];
            argv = [
              "nix"
              "develop"
              "--no-pure-eval"
              "."
              "-c"
              "ctest"
              "--test-dir"
              "build"
            ];
            requirements = {
              cpu = null;
              memoryMiB = null;
              exclusiveLocks = [ ];
            };
          };
        };
      };
    };

    npm-definition = {
      packageId = "web";
      repositoryId = "web-repo";
      source = {
        input = "web-source";
        root = ".";
        visibility = "private";
      };
      targets.default = {
        variants = {
          dimensions = { };
          allowedCombinations = [ ];
        };
        artifacts = {
          outputs = { };
          inputs = { };
        };
        actions = {
          build = {
            kind = "build";
            adapter = "npm-v1";
            cacheExcludes = [ ];
            cacheInputs = [
              "package.json"
              "**/*.js"
            ];
            dependsOn = [ ];
            environment = { };
            argv = [
              "nix"
              "develop"
              "--no-pure-eval"
              "."
              "-c"
              "npm"
              "run"
              "build"
            ];
            requirements = {
              cpu = null;
              memoryMiB = null;
              exclusiveLocks = [ ];
            };
          };
          test = {
            kind = "test";
            adapter = "npm-v1";
            cacheExcludes = [ ];
            cacheInputs = [
              "package.json"
              "**/*.js"
            ];
            environment = { };
            dependsOn = [ "build" ];
            argv = [
              "nix"
              "develop"
              "--no-pure-eval"
              "."
              "-c"
              "npm"
              "test"
            ];
            requirements = {
              cpu = null;
              memoryMiB = null;
              exclusiveLocks = [ ];
            };
          };
        };
      };
    };

    west-definition = {
      packageId = "firmware";
      repositoryId = "firmware-repo";
      source = {
        input = "firmware-source";
        root = ".";
        visibility = "private";
      };
      targets.default = {
        variants = {
          dimensions = { };
          allowedCombinations = [ ];
        };
        artifacts = {
          outputs = { };
          inputs = { };
        };
        actions.build = {
          kind = "build";
          adapter = "west-v1";
          cacheExcludes = [ ];
          cacheInputs = [
            "west.yml"
            "**/*.c"
          ];
          dependsOn = [ ];
          environment = { };
          argv = [
            "nix"
            "develop"
            "--no-pure-eval"
            "."
            "-c"
            "west"
            "build"
          ];
          requirements = {
            cpu = null;
            memoryMiB = null;
            exclusiveLocks = [ "west-workspace" ];
          };
        };
      };
    };

    twister-definition = {
      packageId = "qualification";
      repositoryId = "qualification-repo";
      source = {
        input = "qualification-source";
        root = ".";
        visibility = "private";
      };
      targets.default = {
        variants = {
          dimensions = { };
          allowedCombinations = [ ];
        };
        artifacts = {
          outputs = { };
          inputs = { };
        };
        actions.test = {
          kind = "test";
          adapter = "twister-v1";
          cacheExcludes = [ ];
          cacheInputs = [
            "west.yml"
            "**/*.c"
          ];
          dependsOn = [ ];
          environment = { };
          argv = [
            "nix"
            "develop"
            "--no-pure-eval"
            "."
            "-c"
            "west"
            "twister"
          ];
          requirements = {
            cpu = null;
            memoryMiB = null;
            exclusiveLocks = [ ];
          };
        };
      };
    };
  };
}
