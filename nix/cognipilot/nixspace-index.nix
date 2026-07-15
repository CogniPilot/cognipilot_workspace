{ index }:

let
  packageDocument = project: {
    id = project.packageId;
    inherit (project) aliases;
    extensions."org.cognipilot/package-v1" = {
      projectId = project.id;
      inherit (project)
        compliance
        deployability
        lifecycle
        license
        owner
        preset
        repositoryId
        softwareVersion
        source
        ;
    };
  };
in
{
  apiVersion = "nixspace/v1";
  kind = "Workspace";
  interfaceVersion = 2;
  catalog = index.catalog // {
    packages = map packageDocument index.catalog.packages;
  };
  inherit (index)
    actionPlans
    graph
    launchPlans
    ;
}
