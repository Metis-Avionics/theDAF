# Design annex: the native GraphQL seam

*Companion to `docs/proposals/daf-production-readiness.md` (W-P3/W-P4/W-P5).
Design-only; nothing here is implemented in this PR. Evidence citations point
into this repo (and the crates.io-pinned `themql-core` engine source) only;
the sanitization law holds — a consuming consumer appears as "a downstream
consumer".*

## A. Why a member, not a feature

The 0.3 band welded two laws into `daf-core`:

1. **Transport-blind**: "DAF still never imports a storage/transport crate"
   (the 0.3.1 release note, `CHANGELOG.md` [0.3.1]; enforced by the module
   docstrings of `generic/**`).
2. **Tenant-owned denial**: the domain function owns the B-281 pool denial
   behind its own scope rules (`generic/access.rs:257`, the 0.3.1 ordering
   comment).

A `graphql` feature on `daf-core` would put an async-graphql dependency one
flag away from the law's subject — the flag toggles the very surface the law
governs. A separate **`crates/daf-graphql` member** mirrors the precedent
that already exists: `daf-http`'s `DataAccessRouter`
(`daf-http/src/lib.rs:63`) is the Axum adapter for the 0.2 JSON facade;
`daf-graphql` is the same role for the 0.3 typed seam. The member:

- depends on `daf-core` + `async-graphql` (+ nothing else new);
- carries NO tenancy logic of its own — tenancy is the policy table's
  (`generic/policy.rs:9`, `:56`), the resolver's context construction is
  `Subject`-shaped (`generic/types.rs:13`);
- is feature-gated itself in the workspace's *consumption*, not inside
  `daf-core` (a consumer workspace that never needs GraphQL simply does not
  adopt the member — exactly the one-hop rule the utilization flag encodes).

## B. The typed-schema generation thesis

**"The macro authors the field + its wired types, so the consumer = 1 macro
line, not 152 hand shims."**

The consumer-side state today (utilization flag, `docs
/daf-utilization-flag.md` §Unused capability): all five declaration macros
sit at **zero invocations**; the access adapters, policy rows and error
mapping are hand-wired per resource. The 0.3 band already holds the
composition law: the macros wrap the single-authored adapters —
`daf_query!`/`daf_mutation!` "build their access rows by wrapping
`QueryAccess`/`MutationAccess` instead of re-authoring the JSON boundary"
(`generic/macros.rs` module comment, `CHANGELOG.md` [0.3.2]). The
GraphQL thesis is the SAME law pointed at the transport boundary:

```
// today: the consumer authors
//   - an access adapter per resource (hand implementation of DataAccess)
//   - a policy table row per resource
//   - a schema field per resource + its input/output types
//   - an error projection per resource
// with daf-graphql the consumer authors ONE declaration per resource:

daf_field! {
    resource = "backtests",
    domain   = MyQueryDomain,          // the QueryDomain impl, as today
    policy   = MyPolicyTable,          // the PolicyTable impl, as today
    args     = MyArguments,            // QueryDomain::parse_arguments output
    output   = MyOutput,               // QueryDomain::run output
    mutation = SomeMutationDomain,     // optional: joins the mutation half
}
```

The macro's one-time expansion (in `daf-graphql`, so `daf-core` stays blind):

1. **The field resolver** — an `async-graphql` field delegating to the SAME
   `QueryAccess<MutationAccess>` adapter the access half already wraps; the
   generated body is `access.query(&QueryInfo{..}, &subject_projection)`.
   No parallel path: if the macro stops compiling, the seam stops, not a
   schema that silently detaches from the access layer.
2. **The wired types** — `args` → an `InputObject` projection (the
   `QueryDomain::parse_arguments` contract already defines the decode
   direction, `generic/contracts.rs:16`); `output` → a `ComplexObject` (the
   `run` contract defines the encode direction).
3. **The policy directive map** — `ResourcePolicy`'s two rows
   (`query`/`mutation` admission modes, `generic/policy.rs:56`) → the
   resolver's guard: `AdmissionMode::Tenant` (`policy.rs:9`) = the
   subject's tenant scope must exist before the field executes —
   fail-closed, before pool availability (the 0.3.1 ordering preserved:
   `access.rs:257`).
4. **The error projection** — the generated resolver maps the engine's
   `DataAccessError` taxonomy (`lib.rs:94`) onto the response envelope the
   consumer already projects onto the MQL taxonomy (`themql-core`
   `Error`/`ErrorCode`, `lib.rs:1201`/`:1159`) — the same mapping, one
   author.

**Why the macro and not derive-less hand code:** the consumer's ~152 shims
are each a small bespoke version of the above four maps; the composition law
(upstream, #50) says one author per pattern. The macro IS that author. The
macro's generated code remains source at expansion — the Power-of-Ten rust
gate (`.github/workflows/ci.yml:50`) keeps covering it.

## C. SDL / DDL agreement hooks

- **SDL**: every `daf_field!` expansion emits the field's SDL fragment. The
  fragments are committed to a snapshot file; a CI leg diffs emitted-vs-
  snapshot (the same checked-artifact pattern the repo's parity harness
  already uses: `.github/workflows/ci.yml:59`, `parity-differential`).
  Schema drift = red CI, not a runtime warning.
- **DDL**: the resource row may optionally declare its persistence binding
  (table name + key columns). The upstream does NOT own migrations (the
  access law: storage-blind); instead the declared row is compiled into the
  SAME agreement artifact: a check-only lint that the declared table/keys
  exist in the consumer-side migration set referenced by the snapshot. Where
  the two disagree, the artifact fails — this replaces the wiring-scanner
  class of runtime warning (the approach a downstream consumer's served-vs-
  declared scanner implements today) with a generated invariant.
- **Catalog**: the emitted SDL fragments aggregate into a per-domain catalog
  artifact (one file per domain, checked), giving the consumer's catalog
  layer a source of truth that cannot drift from the compiled schema —
  because it is compiled from the same macro.

## D. The subscription story

Subscriptions are generation-driven: the CAS `Generation` hierarchy
(`lib.rs:248`, the thesix-wrapped present-only counter) is the
invalidation authority the whole access layer already routes through. The
design:

- `daf-field!(subscribe = true)` emits a subscription resolver whose trigger
  = the resource's generation key crossing a client-supplied watermark.
- The watch primitive lands in `daf-messaging` — the crate exists as a
  2-line shell (`daf-messaging/src/lib.rs:3`); giving it the subscription
  home is the re-homing decision the production proposal carries (the
  alternative in the proposal table = retire it at 0.5; this design makes
  the "give it a real role" leg concrete).
- Ordering: the watch's ordering guarantee = the generation order (a total
  order per resource key), not wall-clock — the CAS hierarchy IS the order.
- The latency/ordering semantics under real load are the honest
  downstream-integration-test row (proposal §4).

## E. The eval surface fit

- **Parity**: the `parity-differential` job gains the SDL-snapshot leg +
  a resolver-contract leg (the same query executed through the macro path
  and through the hand access row must produce byte-equal responses).
- **Power-of-Ten**: the macro author keeps every generated loop bounded and
  recursion-free — the generated source remains under `ci.yml:50`'s gate.
- **The consumer's eval loop** (the downstream ADR-057 system): the seam's
  catalog artifact becomes the eval loop's query surface definition input —
  one artifact, one definition of "what is served" — instead of the hand
  catalog that layer maintains per the owner's seam inventory.
- **The assertion-density rule**: the generated resolver bodies carry the
  invariant asserts (resource-named, subject-shaped — the same
  `debug_assert!` density the 0.3 macros already emit in the access half);
  the density gate's below-threshold residue stays per-function.

## F. What the seam deliberately does NOT do

- No auth *decisions* in the resolver body: the policy table's verdict is
  the only authority; the resolver is the guard, not the judge.
- No pool/connection management: the pool stays `Option<P>` user-shaped
  (`generic/access.rs:61`) — resilience composition (W-P2's hook trait and
  the consumer's own breakers) sits ABOVE the access row, not inside it.
- No migration authoring: DDL stays owned by the deploy side; the seam
  carries the agreement artifact only.
- No federation/stitching surface: single-service composition only. The
  catalog artifact is the future extension point, stated so it is not
  invented speculatively.