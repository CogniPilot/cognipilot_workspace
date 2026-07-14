{
  config,
  lib,
  ...
}:

let
  inherit (builtins)
    attrNames
    length
    mapAttrs
    toJSON
    ;
  inherit (lib)
    all
    concatLists
    concatStringsSep
    filter
    mapAttrsToList
    mkOption
    optional
    splitString
    types
    unique
    ;

  rootConfig = config;
  policy = rootConfig.cognipilot.compliancePolicy;
  normalizedIndex = rootConfig.cognipilot.validatedIndex;
  inherit (normalizedIndex) projects;

  idPattern = "^[a-z][a-z0-9]*([._-][a-z0-9]+)*$";
  validId = value: builtins.match idPattern value != null;
  validBespokeCoordinate =
    coordinate:
    let
      parts = splitString ":" coordinate;
    in
    length parts == 3 && all validId parts;

  packageLabel =
    projectKey: project:
    if projectKey == project.packageId then
      "package `${project.packageId}`"
    else
      "package `${project.packageId}` (project `${projectKey}`)";

  enforcedProject = project: builtins.elem project.source.visibility policy.enforcedVisibilities;

  allowedValues = values: if values == [ ] then "<none>" else concatStringsSep ", " values;

  errorsForProject =
    projectKey: project:
    let
      label = packageLabel projectKey project;
    in
    if !(enforcedProject project) then
      [ ]
    else
      optional (
        policy.requireOwner && project.owner == null
      ) "${label} is missing the owner required by `cognipilot.compliancePolicy.requireOwner`"
    ++
      optional (policy.requireSpdxLicense && project.license.spdx == null)
        "${label} is missing the SPDX license required by `cognipilot.compliancePolicy.requireSpdxLicense`"
    ++
      optional (!(builtins.elem project.lifecycle policy.allowedLifecycles))
        "${label} lifecycle `${project.lifecycle}` is not allowed; allowed lifecycles: ${allowedValues policy.allowedLifecycles}"
    ++
      optional (!(builtins.elem project.deployability policy.allowedDeployability))
        "${label} deployability `${project.deployability}` is not allowed; allowed deployability: ${allowedValues policy.allowedDeployability}";

  policyErrors =
    optional (
      policy.allowedLifecycles == [ ]
    ) "root compliance policy must allow at least one lifecycle"
    ++ optional (
      length policy.allowedLifecycles != length (unique policy.allowedLifecycles)
    ) "root compliance policy declares duplicate allowed lifecycles"
    ++ optional (
      policy.allowedDeployability == [ ]
    ) "root compliance policy must allow at least one deployability value"
    ++ optional (
      length policy.allowedDeployability != length (unique policy.allowedDeployability)
    ) "root compliance policy declares duplicate allowed deployability values"
    ++ optional (
      policy.enforcedVisibilities == [ ]
    ) "root compliance policy must enforce at least one source visibility"
    ++ optional (
      length policy.enforcedVisibilities != length (unique policy.enforcedVisibilities)
    ) "root compliance policy declares duplicate enforced source visibilities"
    ++ optional (
      length policy.approvedBespokeActions != length (unique policy.approvedBespokeActions)
    ) "root compliance policy declares duplicate bespoke action approvals";

  bespokeActions = concatLists (
    mapAttrsToList (
      projectKey: project:
      concatLists (
        mapAttrsToList (
          targetId: target:
          concatLists (
            mapAttrsToList (
              actionId: action:
              optional (enforcedProject project && action.adapter == "bespoke-v1") {
                coordinate = "${project.packageId}:${targetId}:${actionId}";
                inherit actionId projectKey targetId;
                inherit (project) packageId;
              }
            ) target.actions
          )
        ) project.targets
      )
    ) projects
  );
  bespokeCoordinates = map (finding: finding.coordinate) bespokeActions;
  enforcedBespokeAdapterCount = length bespokeActions;

  invalidApprovals = filter (
    approval: !validBespokeCoordinate approval
  ) policy.approvedBespokeActions;
  staleApprovals = filter (
    approval: validBespokeCoordinate approval && !(builtins.elem approval bespokeCoordinates)
  ) policy.approvedBespokeActions;
  unapprovedBespokeActions = filter (
    finding: !(builtins.elem finding.coordinate policy.approvedBespokeActions)
  ) bespokeActions;

  approvalErrors =
    map (
      approval: "bespoke action approval `${approval}` is invalid; expected `package:target:action` IDs"
    ) invalidApprovals
    ++ map (
      approval:
      "bespoke action approval `${approval}` is stale; it does not reference a selected bespoke action"
    ) staleApprovals
    ++ map (
      finding:
      "bespoke action `${finding.coordinate}` is not approved; add its exact coordinate to `cognipilot.compliancePolicy.approvedBespokeActions`"
    ) unapprovedBespokeActions;

  bespokeAdapterError =
    optional (enforcedBespokeAdapterCount > policy.maximumBespokeAdapters)
      ''
        policy-enforced packages declare ${toString enforcedBespokeAdapterCount} bespoke adapters, exceeding `cognipilot.compliancePolicy.maximumBespokeAdapters` (${toString policy.maximumBespokeAdapters}); actions: ${concatStringsSep ", " bespokeCoordinates}
      '';

  validationErrors =
    policyErrors
    ++ approvalErrors
    ++ concatLists (mapAttrsToList errorsForProject projects)
    ++ bespokeAdapterError;

  complianceReport = {
    schemaVersion = 1;
    compliant = true;
    contractInterfaceVersion = normalizedIndex.interfaceVersion;
    policy = {
      inherit (policy)
        allowedDeployability
        allowedLifecycles
        approvedBespokeActions
        enforcedVisibilities
        maximumBespokeAdapters
        requireOwner
        requireSpdxLicense
        ;
    };
    summary = {
      selectedPackageCount = length (attrNames projects);
      bespokeAdapterCount = enforcedBespokeAdapterCount;
      inherit (normalizedIndex.compliance) warningCount;
    };
    bespoke = {
      approvals = policy.approvedBespokeActions;
      findings = map (finding: finding // { approved = true; }) bespokeActions;
    };
    packages = mapAttrs (projectKey: project: {
      projectId = projectKey;
      inherit (project)
        deployability
        lifecycle
        owner
        packageId
        ;
      enforced = enforcedProject project;
      licenseSpdx = project.license.spdx;
      bespokeAdapterCount = project.compliance.bespokeAdapterCount;
      compliant = true;
    }) projects;
  };

  validatedReport =
    if validationErrors == [ ] then
      complianceReport
    else
      throw ''
        CogniPilot product compliance violations:
        - ${concatStringsSep "\n- " validationErrors}
      '';
in
{
  options.cognipilot = {
    compliancePolicy = {
      requireOwner = mkOption {
        type = types.bool;
        default = true;
        description = "Require every selected package to declare an owner.";
      };

      requireSpdxLicense = mkOption {
        type = types.bool;
        default = true;
        description = "Require every selected package to declare an SPDX license expression.";
      };

      allowedLifecycles = mkOption {
        type = types.listOf (
          types.enum [
            "experimental"
            "stable"
            "deprecated"
            "retired"
          ]
        );
        default = [ "stable" ];
        description = "Package lifecycles accepted by this product root.";
      };

      allowedDeployability = mkOption {
        type = types.listOf (
          types.enum [
            "local-only"
            "qualification"
            "deployable"
          ]
        );
        default = [
          "qualification"
          "deployable"
        ];
        description = "Package deployability levels accepted by this product root.";
      };

      enforcedVisibilities = mkOption {
        type = types.listOf (
          types.enum [
            "private"
            "public"
          ]
        );
        default = [
          "private"
          "public"
        ];
        description = ''
          Source visibility classes governed by this product policy. A public
          development product can enforce public packages while retaining an
          explicitly private, non-promotable integration without weakening
          the public package rules.
        '';
      };

      maximumBespokeAdapters = mkOption {
        type = types.ints.unsigned;
        default = 0;
        description = "Maximum total bespoke adapters allowed across selected packages.";
      };

      approvedBespokeActions = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = ''
          Exact root-approved bespoke action coordinates in
          `package:target:action` form. Invalid, stale, duplicate, and missing
          approvals fail product compliance.
        '';
      };
    };

    complianceReport = mkOption {
      type = types.attrs;
      readOnly = true;
      description = "Validated, JSON-safe product compliance report.";
    };
  };

  config = {
    cognipilot.complianceReport = validatedReport;

    perSystem =
      { pkgs, ... }:
      let
        reportJson = pkgs.writeText "cognipilot-compliance-report.json" (
          toJSON rootConfig.cognipilot.complianceReport
        );
      in
      {
        checks.cognipilot-compliance = pkgs.runCommand "cognipilot-compliance" { } ''
          mkdir -p "$out"
          cp ${reportJson} "$out/report.json"
        '';
      };
  };
}
