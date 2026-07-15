# CogniPilot promotion record

The selected product flake exports three immutable, independently inspectable
promotion artifacts:

- `promotion-record` / `cognipilot-promotion-record`;
- `promotion-sbom` / `cognipilot-promotion-sbom`; and
- `promotion-attestation` / `cognipilot-promotion-attestation`.

Each package/check pair is the same derivation. None is `packages.default`;
that name remains reserved for an explicitly selected natural deployable
target.

The check is the normative validation path. It writes
`share/cognipilot/promotion-record.json` with:

- the promotion-record schema version, product ID, project interface version,
  and target system;
- package IDs, declared software-version identities, source visibility,
  deployability, and exact adapter IDs;
- selected source and integration-definition input names plus their committed
  lock nodes, canonical lock coordinates, exact Git-commit or NAR-tree
  revisions, NAR hashes, and store paths;
- every selected target, variant declaration, release provider/output name,
  provider identity, and exact output derivation/store path;
- the workspace and named product output identities;
- the complete workspace store closure, sorted by store path, including each
  path's NAR hash, NAR size, and direct store references; and
- an exact SLSA builder identity whose URI contains the Nix derivation path
  identity, plus the nixspace version, output path, target system, and locked
  nixpkgs identity;
- the SHA-256 digest of the normalized selected product projection and the
  exact SPDX SBOM output identity; and
- the currently empty, explicit external-source-exception list.

This data is generated from `cognipilot.validatedIndex`, the selected module
configuration, the root flake inputs, and the committed root `flake.lock`.
There is no editable promotion manifest or second dependency graph. Promotion
record schema v2, the SPDX document, and the provenance statement are three
projections of the same normalized projects, resolved release outputs, and
committed lock identities.

## SPDX and provenance outputs

`promotion-sbom` writes `share/cognipilot/sbom.spdx.json` as SPDX JSON 2.3.
It identifies the product and every selected package, relates the product to
those packages, records declared versions/licenses/owners, and gives exact Nix
input and release-output references. `filesAnalyzed = false` is deliberate:
the inventory describes selected immutable packages and does not pretend that
a source-file scanner ran.

`promotion-attestation` writes
`share/cognipilot/provenance.intoto.json` as an in-toto Statement v1 with the
SLSA provenance v1 predicate. Subjects are the workspace/product/target
outputs; the promotion record and SBOM are byproducts. Resolved dependencies
are the locked source, definition, and release-provider identities.

Every subject and byproduct uses the standard in-toto `digest.sha256` field
with the lowercase hexadecimal SHA-256 of its NAR serialization. The generic
ClosureMaterialization v2 plan binds each declared output JSON pointer to its
digest JSON pointer and converts Nix's canonical base32 or SRI SHA-256 encoding
to lowercase hex only after `exportReferencesGraph` proves the output. The free-form
`annotations.nixOutput` retains the derivation/store identity, NAR SRI digest,
and NAR size. Resolved input descriptors use a lowercase-hex
`nixNarSha256` digest and retain the original flake-lock SRI and revision in
their typed Nix annotation; Git inputs additionally expose `gitCommit`.

The statement is a reproducible attestation payload, not a locally invented
signature envelope. Standard Nix/Cachix signatures authenticate substituted
outputs. If a separately signed in-toto envelope is required for a deployment
boundary, it must wrap these immutable bytes rather than regenerate
provenance.

`builder.id` names the Nix build boundary and target system; the corresponding
ResourceDescriptor identifies the exact nixspace output and locked nixpkgs.
This truthfully identifies the local attestation builder software/configuration,
not a separately secured hosted execution platform or an evaluation-time
derivation path. The statement makes no SLSA level claim. It also omits
`invocationId`: a reproducible Nix output cannot truthfully invent a globally
unique invocation identity. The realized closure is retained under the
vendor-prefixed SLSA metadata extension `cognipilot_nixClosure`.

## Strict boundary

A deployable package fails promotion evaluation when its source, definition,
or release-provider input is missing, dirty, unlocked, an absolute path input,
or not represented by an immutable Nix store path and NAR identity. A release
also fails when its selected package is not a store-backed derivation. A dirty
product flake cannot promote deployable entries.

Qualification entries remain in the record for auditability with
`promotionStatus = "qualification-only"`. They have no release reference and
do not become deployable merely because the record itself is a Nix output.
Missing source provenance on a qualification-only entry is recorded as
`status = "missing"` instead of being silently invented.

The record exposes `passthru.normalizedRecord`, the SBOM exposes
`passthru.document`, and the attestation exposes `passthru.statement` for pure
evaluation and inspection. Their closure placeholders are null until
realization because NAR hashes for unbuilt transitive closures do not exist.
The normative checks use Nix's `exportReferencesGraph` and the generic typed
nixspace materializer to fill them without import-from-derivation, repository
scanning, jq, or a second semantic planner.

## Current schema limits

- `native` and `file` software-version declarations remain exact source
  declarations. A promoted release additionally records and validates the
  selected provider output version; non-release qualification entries have no
  fabricated resolved version. Their locked source identity remains exact.
- The normalized project index intentionally omits definition location. The
  product module can identify it because the selected typed module
  configuration remains available during composition; cached index consumers
  cannot reconstruct it and must not guess.
- Interface v1 has no typed external-source-exception option, so the record
  emits an empty list. An exception policy must be added to the typed root
  schema before any exception can be promoted.
- Complete transitive store paths and NAR hashes are materialization facts.
  They are produced by the Nix check, not guessed during pure evaluation.

## Public cache boundary

`packages.<system>.public-cache-root` is the explicit closure root intended for
the public `cognipilot` Cachix cache. It links the public product definition,
source and definition inputs whose project source is explicitly marked
`public`, public release roots, and shared workspace tooling. The tooling
members include the exact
`nixspace-host` and `ws` wrappers realized by `./setup`, all generated runtime
plans (including the benchmark plan), and completions. Visibility defaults
fail-safe to `private` in the project schema.

FastDyn's separately locked private source store path and any private release
output are excluded from this root. Its integration definition is committed
inside the public product source and is therefore intentionally reachable.
Promotion JSON may record a private identity for local audit, but its JSON seed
has Nix string context discarded so the textual private source path cannot
accidentally add that source to the derivation closure.

Visibility governs Nix source/output closure publication. It does not redact
repository IDs, checkout paths, Git endpoints, branches, or revisions already
declared by this public product flake and needed by the generated editable
source plan. A genuinely secret repository locator belongs in a private product
definition and private cache rather than this public registry.

This is a disclosure boundary, not a promise that every public output has
already been uploaded. Protected-main publication and empty-host substitution
remain separate release gates. A future private cache needs separate
credentials and retention policy; caches are divided by source/output
visibility rather than one cache per repository.
