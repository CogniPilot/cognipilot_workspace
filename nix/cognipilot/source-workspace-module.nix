{
  config,
  inputs ? { },
  lib,
  ...
}:

let
  cfg = config.cognipilot.sourceWorkspace;
  safeRuntimePath = value:
    let
      segments = builtins.filter (segment: segment != "") (lib.splitString "/" value);
    in
    value == "."
    || (
      value != ""
      && segments != [ ]
      && builtins.all (segment: segment != "." && segment != "..") segments
    );
  safeRuntimeFilePath =
    value:
    value != "."
    && safeRuntimePath value
    && !(lib.hasPrefix "/" value)
    && !(lib.hasInfix "\\" value)
    && builtins.match "^[A-Za-z]:.*" value == null;
  index = config.cognipilot.validatedIndex;
  projects = builtins.attrValues index.projects;
  packageToProject = builtins.listToAttrs (
    map (project: lib.nameValuePair project.packageId project) projects
  );
  repositoryIds = lib.unique (map (project: project.repositoryId) projects);
  defaultRepositoryIds = lib.unique (
    map (project: project.repositoryId) (
      builtins.filter (project: project.source.visibility == "public") projects
    )
  );
  projectsForRepository = repositoryId: builtins.filter (
    project: project.repositoryId == repositoryId
  ) projects;
  projectForRepository = repositoryId: builtins.head (projectsForRepository repositoryId);
  conflictingRepositories = builtins.filter (
    repositoryId:
    builtins.length (
      lib.unique (map (project: project.source.input) (projectsForRepository repositoryId))
    ) != 1
  ) repositoryIds;

  selfSource =
    if inputs ? self && inputs.self ? outPath then
      toString inputs.self.outPath
    else
      throw "cognipilot.sourceWorkspace requires the importing flake's `self` input";
  lockPath = "${selfSource}/flake.lock";
  lock =
    if builtins.pathExists lockPath then
      builtins.fromJSON (builtins.readFile lockPath)
    else
      throw "cognipilot.sourceWorkspace requires a committed flake.lock at ${lockPath}";
  rootNode = lock.nodes.${lock.root};

  nodeNameFor = inputName:
    let
      reference = rootNode.inputs.${inputName} or (
        throw "source input `${inputName}` is not a direct input in the importing flake lock"
      );
    in
    if builtins.isString reference then
      reference
    else
      throw "source input `${inputName}` must be a direct locked input, not a follows path";
  lockNodeFor = inputName: lock.nodes.${nodeNameFor inputName};

  urlFor = inputName:
    let
      node = lockNodeFor inputName;
      original = node.original or { };
    in
    if builtins.hasAttr inputName cfg.urls then
      cfg.urls.${inputName}
    else if (original.type or null) == "github" && original ? owner && original ? repo then
      "https://github.com/${original.owner}/${original.repo}.git"
    else if (original.type or null) == "git" && original ? url then
      original.url
    else
      throw ''
        source input `${inputName}` is not a GitHub/Git lock node; set
        cognipilot.sourceWorkspace.urls.${inputName} explicitly
      '';

  branchFor = inputName:
    let
      original = (lockNodeFor inputName).original or { };
    in
    cfg.branches.${inputName} or (original.ref or cfg.defaultBranch);

  pathFor = project:
    cfg.paths.${project.source.input} or "src/${project.repositoryId}";

  repositoryFor = repositoryId:
    let
      project = projectForRepository repositoryId;
      inputName = project.source.input;
      node = lockNodeFor inputName;
      locked = node.locked or { };
      path = pathFor project;
      url = urlFor inputName;
      branch = branchFor inputName;
      literalArgs = map (value: {
        kind = "literal";
        inherit value;
      });
      oldHead = { kind = "old-head"; };
      expectedCurrent = { kind = "expected-current"; };
    in
    {
      id = repositoryId;
      packages = map (candidate: candidate.packageId) (projectsForRepository repositoryId);
      inherit path;
      source = {
        input = inputName;
        roots = map (candidate: candidate.source.root) (projectsForRepository repositoryId);
        locked = {
          type = locked.type or null;
          rev = locked.rev or null;
          narHash = locked.narHash or null;
        };
      };
      git = {
        inherit branch url;
        clone.argv = [
          "git"
          "clone"
          "--origin"
          "origin"
          "--branch"
          branch
          "--"
          url
          path
        ];
        status.argv = [
          "git"
          "-C"
          path
          "status"
          "--short"
          "--branch"
        ];
        inspect = {
          worktree.argv = [
            "git"
            "-C"
            path
            "rev-parse"
            "--show-toplevel"
          ];
          origin.argv = [
            "git"
            "-C"
            path
            "remote"
            "get-url"
            "origin"
          ];
          branch.argv = [
            "git"
            "-C"
            path
            "symbolic-ref"
            "--quiet"
            "--short"
            "HEAD"
          ];
          clean.argv = [
            "git"
            "-C"
            path
            "status"
            "--porcelain=v1"
            "--untracked-files=normal"
          ];
          head.argv = [
            "git"
            "-C"
            path
            "rev-parse"
            "--verify"
            "HEAD"
          ];
          target.argv = [
            "git"
            "-C"
            path
            "rev-parse"
            "--verify"
            "origin/${branch}"
          ];
        };
        fetch.argv = [
          "git"
          "-C"
          path
          "fetch"
          "--prune"
          "origin"
          branch
        ];
        fastForwardCheck.argv = [
          "git"
          "-C"
          path
          "merge-base"
          "--is-ancestor"
          "HEAD"
          "origin/${branch}"
        ];
        fastForward.argv = [
          "git"
          "-C"
          path
          "merge"
          "--ff-only"
          "origin/${branch}"
        ];
        rollback = {
          # Move the branch only when it still names the transaction target.
          # The separate two-tree restore updates the index/worktree without
          # moving the ref, so a later concurrent commit is never overwritten.
          refUpdate.argv = literalArgs [
            "git"
            "-C"
            path
            "update-ref"
            "HEAD"
          ] ++ [
            oldHead
            expectedCurrent
          ];
          worktreeRestore.argv = literalArgs [
            "git"
            "-C"
            path
            "read-tree"
            "-m"
            "-u"
          ] ++ [
            expectedCurrent
            oldHead
          ];
          # If the worktree transition refuses a concurrent edit, restore the
          # original ref only when no other process has moved it in the interim.
          refRestore.argv = literalArgs [
            "git"
            "-C"
            path
            "update-ref"
            "HEAD"
          ] ++ [
            expectedCurrent
            oldHead
          ];
        };
      };
    };

  repositories = builtins.listToAttrs (
    map (repositoryId: lib.nameValuePair repositoryId (repositoryFor repositoryId)) repositoryIds
  );
  repositoryPaths = map (repository: repository.path) (builtins.attrValues repositories);
  duplicateRepositoryPaths = builtins.filter (
    path: lib.count (candidate: candidate == path) repositoryPaths > 1
  ) (lib.unique repositoryPaths);
  repositoriesForPackage = packageId:
    let
      graph = index.graph.packages.${packageId};
      dependencyPackages = lib.unique (map (node: node.package) graph.nodes);
    in
    lib.unique (
      map (dependencyPackage: packageToProject.${dependencyPackage}.repositoryId) (
        [ packageId ] ++ dependencyPackages
      )
    );
  packagePlans = builtins.listToAttrs (
    map (
      project:
      lib.nameValuePair project.packageId (repositoriesForPackage project.packageId)
    ) projects
  );

  plan =
    if conflictingRepositories != [ ] then
      throw ''
        repository IDs select multiple source inputs: ${lib.concatStringsSep ", " conflictingRepositories}
      ''
    else if duplicateRepositoryPaths != [ ] then
      throw ''
        source repositories select duplicate checkout paths: ${lib.concatStringsSep ", " duplicateRepositoryPaths}
      ''
    else if cfg.mutationLockPath == cfg.transactionJournalPath then
      throw "source workspace mutation lock and transaction journal paths must be distinct"
    else
      {
        apiVersion = "nixspace/v1";
        kind = "SourceWorkspace";
        interfaceVersion = 3;
        workspaceRoot = cfg.workspaceRoot;
        transaction = {
          mutationLock = cfg.mutationLockPath;
          journal = cfg.transactionJournalPath;
        };
        inherit repositories;
        plans = {
          default = defaultRepositoryIds;
          all = builtins.attrNames repositories;
          packages = packagePlans;
        };
      };
in
{
  options.cognipilot.sourceWorkspace = {
    enable = lib.mkEnableOption "the Nix-locked editable source workspace plan";

    workspaceRoot = lib.mkOption {
      type = lib.types.addCheck lib.types.str safeRuntimePath;
      default = ".";
      description = "Runtime root against which repository checkout paths are resolved.";
    };

    mutationLockPath = lib.mkOption {
      type = lib.types.addCheck (lib.types.strMatching "[^[:space:]]+") safeRuntimeFilePath;
      default = ".nixspace/source-mutation.lock";
      description = "Workspace-relative exclusive mutation lock path consumed by nixspace.";
    };

    transactionJournalPath = lib.mkOption {
      type = lib.types.addCheck (lib.types.strMatching "[^[:space:]]+") safeRuntimeFilePath;
      default = ".nixspace/source-update-transaction.json";
      description = "Workspace-relative durable update journal path consumed by nixspace.";
    };

    defaultBranch = lib.mkOption {
      type = lib.types.strMatching "[^[:space:]]+";
      default = "main";
      description = "Development branch used when the locked input has no explicit ref.";
    };

    branches = lib.mkOption {
      type = lib.types.attrsOf (lib.types.strMatching "[^[:space:]]+");
      default = { };
      description = "Root-owned development branch overrides keyed by source input.";
    };

    urls = lib.mkOption {
      type = lib.types.attrsOf (lib.types.strMatching "[^[:space:]]+");
      default = { };
      description = "Explicit clone URL overrides for lock input types without a Git URL.";
    };

    paths = lib.mkOption {
      type = lib.types.attrsOf (
        lib.types.addCheck (lib.types.strMatching "[^[:space:]]+") safeRuntimePath
      );
      default = { };
      description = "Workspace-root-relative checkout path overrides keyed by source input.";
    };
  };

  config = lib.mkIf cfg.enable {
    flake.nixspaceSourcePlan = plan;

    perSystem =
      { pkgs, ... }:
      {
        packages.nixspace-source-plan = pkgs.writeTextDir "share/nixspace/source-plan.json" (
          builtins.toJSON plan
        );
      };
  };
}
