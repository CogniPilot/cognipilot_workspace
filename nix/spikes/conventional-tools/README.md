# Conventional-tool compatibility spike

This flake is an evaluation-only proof for the roadmap's Nix Standard,
`zephyr-nix`, and `west2nix` decisions. Its lock is deliberately independent of
the production workspace lock.

Evaluate the complete proof without building anything:

```console
nix eval path:./nix/spikes/conventional-tools#conventionalToolSpike --json
```

Do not build `cubs2WestHook`: the checked-in CUBS2 manifest snapshot uses
`lib.fakeHash`-equivalent placeholders because fetching ten repositories just
to calculate hashes is outside this build-free spike. Generate real hashes with
the pinned `west2nix` CLI before promoting that hook to a release definition.

The findings and adoption boundaries are in
[`dev/conventional-tools-compatibility-spike.md`](../../../dev/conventional-tools-compatibility-spike.md).
