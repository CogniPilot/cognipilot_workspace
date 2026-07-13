let
  definitions = {
    default = {
      includes = [ "cubs2" ];
      components = [ ];
    };

    modelica = {
      includes = [ ];
      components = [
        "rumoca"
        "modelica_models"
      ];
    };

    cubs2 = {
      includes = [ "modelica" ];
      components = [
        "cerebri_cubs2"
        "cerebri_modules"
        "csyn"
        "electrode_web"
        "synapse_fbs"
      ];
    };

    rdd2 = {
      includes = [ "modelica" ];
      components = [
        "cerebri_modules"
        "cerebri_rdd2"
        "csyn"
        "electrode_web"
        "synapse_fbs"
      ];
    };

    zros = {
      includes = [ ];
      components = [
        "zros"
        "zros_drivers"
      ];
    };

    qualisys = {
      includes = [ ];
      components = [
        "qualisys_rust_sdk"
        "synapse_qualisys_bridge"
      ];
    };

    ros2 = {
      includes = [ ];
      components = [ "csyn_ros2_bridge" ];
    };

    fastdyn = {
      includes = [ ];
      components = [ "FastDyn" ];
    };
  };

  unique = builtins.foldl' (
    result: item: if builtins.elem item result then result else result ++ [ item ]
  ) [ ];

  resolve =
    stack: name:
    if builtins.elem name stack then
      throw "workspace profile include cycle: ${builtins.concatStringsSep " -> " (stack ++ [ name ])}"
    else if !(builtins.hasAttr name definitions) then
      throw "workspace profile includes unknown profile: ${name}"
    else
      let
        definition = definitions.${name};
        inherited = builtins.concatLists (
          map (included: resolve (stack ++ [ name ]) included) definition.includes
        );
      in
      unique (inherited ++ definition.components);
in
{
  inherit definitions;
  resolved = builtins.mapAttrs (name: _: resolve [ ] name) definitions;
}
