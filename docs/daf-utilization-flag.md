# DAF 0.3 utilization flag — what a downstream consumer actually uses

*Status: evidence flag, not a ruling. Generated from the published-band sources
(`daf-*` = this repo at `main` after the #50/#51 band, which is the 0.3.2
candidate surface) and the exact crates.io-pinned engine sources the consumer
resolves (`themql-core 0.2.0`, `prv-core/filter/monte-carlo/optimizer 0.2.1`,
`prv-evaluation 0.2.2`). Consumer identity is pseudonymized per the
sanitization law: consumers appear only as `consumer-Axx` / `harness-Axx` with
a role label. The machine ledger beside this file
(`daf-utilization-flag.json`) carries the same data in structured form.*

## Method (so the numbers can be argued with)

This is a structural-resolution pass, not an import-grep:

1. **Inventory.** Every public symbol (`struct`/`enum`/`trait`/`fn`/`type`,
   `const`, `mod`, `#[macro_export]` macro) of each engine crate is collected
   from its source, with internal `pub use` chains resolved so crate-level
   re-exports count their target once.
2. **Use-symbol resolution.** For every source file in the consumer tree, each
   `use` statement is parsed into its imported symbol set (nested brace
   groups, aliases, globs). Imports through a downstream re-export barrel are
   credited **one hop** to the origin engine crate.
3. **Body verification.** A symbol counts as *used* only when a reference
   exists in the file body — string literals are blanked before tokenizing, so
   a resource name or doc-word cannot counterfeit a use. Definition-site
   vs use-site occurrences are counted separately (`def_sites` vs `use_refs`).
4. **Macro-awareness.** An engine declaration macro invoked in consumer code
   counts as consumption of (a) the macro itself, (b) every symbol named in
   the macro input, keyed `macro-input`, and (c) the trait the macro's
   generated impl binds (`DataAccess`), keyed `macro-impl`.
5. **Utilization %** = distinct public symbols with ≥1 verified consumer use
   ÷ total public symbols of the engine crate.

Result categories per engine: **used** (≥1 verified use), **imported but
unused** (a use statement names it, no body reference anywhere), and **never
imported** (entirely unconsumed).

## The utilization table

| engine | public symbols | used | utilization % | consumer pin |
|---|---:|---:|---:|---|
| `daf-core` | 82 | 16 | **19.5** | `=0.3.1` exact band |
| `daf-algorithms` | 2 | 0 | 0.0 | not pinned |
| `daf-application` | 9 | 0 | 0.0 | not pinned |
| `daf-cache` | 39 | 0 | 0.0 | not pinned |
| `daf-ffi` | 8 | 0 | 0.0 | not pinned |
| `daf-http` | 4 | 0 | 0.0 | not pinned |
| `daf-messaging` | 2 | 0 | 0.0 | not pinned |
| `daf-repository` | 3 | 0 | 0.0 | not pinned |
| `daf-runtime` | 7 | 0 | 0.0 | not pinned |
| `themql-core` | 87 | 15 | **17.2** | `=0.2.0` exact |
| `prv-core` | 32 | 2 | 6.2 | `0.2.1` caret |
| `prv-filter` | 22 | 6 | **27.3** | `0.2.1` caret |
| `prv-monte-carlo` | 12 | 1 | 8.3 | `0.2.1` caret |
| `prv-optimizer` | 37 | 9 | **24.3** | `0.2.1`, default-features off |
| `prv-evaluation` | 20 | 4 | 20.0 | `0.2.2` (0.2.1 predates the tail-risk module), default-features off |
| `prv-cache` | 33 | 0 | 0.0 | transitive only, not pinned |
| `prv-data` | 18 | 0 | 0.0 | transitive only, not pinned |
| `prv-policy` | 9 | 0 | 0.0 | transitive only, not pinned |

Roster: 29 crates in the consumer tree import engine symbols (20 domain
crates, 6 shared/infra planes, 3 harnesses); a further 18 discovered crates
carry zero engine imports.

## What is actually consumed

**`daf-core` (16 symbols, 19.5%).** The generic-access seam is the live
consumption: the `DataAccess` trait (`generic/access.rs:14`), the
`QueryAccess`/`MutationAccess` adapters (`access.rs:126`, `access.rs:191`),
`DomainDataAccess` (`access.rs:297`), the `QueryDomain`/`MutationDomain`
traits (`generic/contracts.rs:16`, `:55`), `Subject` (`generic/types.rs:13` —
the single heaviest dependency: ~880 body references; the consumer's typed
tenancy is built on it), `Generation` (`lib.rs:248`, the CAS
hierarchy), `QueryInfo` (`generic/types.rs:90`), `ResourceId` (`lib.rs:57`) + the error
taxonomy (`DataAccessError`, `NotFoundError`, `ValidationError`,
`AuthorizationError`, `ConflictError`, `InternalError`, `CacheError`,
`RepositoryError` — top-level `lib.rs:94..194`), and the `generic` module
root itself.

**`themql-core` (15 symbols, 17.2%).** The error + wire envelope half: `Error`
and `ErrorCode` (`themql-core 0.2.0/src/lib.rs:1201`, `:1159`), the MQL query
morphology (`Query`, `Selection`, `Projection`, `Predicate`, `FieldPath`,
`Comparison`, `Resource`), the response half (`Response`, `ResponseValue`,
`Context`, `Principal`), and two correlation primitives (`MessageId`,
`CorrelationId`).

**`prv-*` engine cores.** A narrow surface per engine:
`prv-filter`: `Ekf`, `EkfResult`, the `ObservationModel`/`TransitionModel`
traits, `Observation`, `State` (`prv-filter 0.2.1/src/ekf.rs:16`,
`observation.rs:4`, `transition.rs:3`).
`prv-optimizer`: `minimize`, the `Objective` trait, `Trajectory`,
`LinearConstraint`, `OptimizerError` (`prv-optimizer 0.2.1/src/solve.rs:145`,
`objective.rs:10`, `:68`).
`prv-evaluation`: exactly the four point metrics `value_at_risk`,
`expected_shortfall`, `rmse`, `mae` (`prv-evaluation 0.2.2/src/tail.rs:147`,
`:163`).
`prv-monte-carlo`: only `Simulator` (`simulator.rs:32`).
`prv-core`: `State` + `capacity` + `PrvError` (`src/state.rs:5`, `:59`).

## Unused capability, grouped by feature

**Types (unconsumed).** `daf-core`: the entire 0.2 cache/facade family (`Tier`,
`CacheEntry`, `PostInfo`/`PutInfo`/`DeleteInfo`/`QueryResult`/
`MutationResult`), the 0.2 identity (`UserId`, deprecated), `LockRegistry`/
`LockGuard` (`lock_registry.rs:11`), and the generic `MutationContext`/
`MutationInfo` pair; `daf-cache`: every cache class (`MemoryCache`, `MokaCache`
(`moka.rs:9`), `HierarchicalCache` (`hierarchical.rs:9`), `RedisCache`,
`PostgresCache` — the two remote tiers are feature-gated *stubs* that return
`CacheError::new("… feature not enabled")` (`postgres.rs:20`), `SledCache`,
the prefix-trie traversal family); `themql-core`: the message-identity
quartet's remainder (`CausationId`, `TraceId`), `Metadata`, `Timestamp`,
`Operation`, `FormatTag`, `Payload`, and — most notably — the tenant-pattern
matcher `Subject`/`SubjectPattern`/`SubjectSegment` (`lib.rs:206`, `:300`);
`prv-core`: `TimeSeries` (`src/time.rs:6`), `Observation`, `Regime`,
`PressureReleaseValve`, the pressure accessor family; `prv-monte-carlo`:
`ShockSpec`/`ShockType`/`MonteCarloProbabilities` (`shocks.rs:19`); `prv-data`
/`prv-cache`/`prv-policy` (transitive): the whole `DataLoader`/`NsCache`/
`PolicyEngine` families.

**Traits (unconsumed).** `daf-core`: `PolicyTable` (`generic/authorizer.rs:8`)
and `Idempotency` (`generic/access.rs:40`); `prv-evaluation`: `Model`
(the backtest model family).

**Macros (unconsumed — 0 invocations).** All five declaration macros:
`daf_query!` (`generic/macros.rs:13`), `daf_mutation!` (`:114`),
`daf_policy!` (`:346`), `daf_rows!` (`:391`), `daf_row!` (`:408`) — the
consumer builds adapters by hand instead of by declaration.

**Adapters / policy (unconsumed).** `PolicyAccess` (`generic/authorized.rs:13`),
`AdmissionMode`/`ResourcePolicy` (`generic/policy.rs:9`, `:56`),
`StaticPolicyAuthorizer` (re-exported by the consumer's seam crate but with no
body use); `typed_access_row`/`required_pool`/`subject_json_key`
(`access.rs:287`, `:61`, `:97`); `subject_from_legacy_user`; the 0.2
application/router legs (`DataAccessFactory`,
`DataAccessRouter` (`daf-http/src/lib.rs:63`), `Runtime` (`daf-runtime
/src/lib.rs:31`), `MemoryRepository` (`daf-repository/src/memory.rs:8`),
`FibonacciDP` (`daf-algorithms/src/lib.rs:18`), `Messaging`,
`DafErrorCode` + the whole FFI boundary (`daf-ffi/src/lib.rs:53`)).

## Value cases — what the unused capability would delete

Each case is backed by a verified consumer-side re-implementation (the
re-implementations live in the consumer's tree; identities sanitized).

1. **The policy-table kit (highest value).** The consumer hand-rolls tenant
   admission: a locally defined `Authorizer` trait plus admit/deny pipelines
   across its tenant/compat plane, re-implemented per domain entry point.
   `AdmissionMode` + `ResourcePolicy` + `PolicyTable` + `PolicyAccess` +
   `daf_policy!` are exactly that contract, fail-closed, statically
   declarable, and policy-table-driven. Adopting it replaces hand-rolled
   admission with the table (`policy.rs` modes: `Public`, `Authenticated`,
   `Tenant`, `GlobalAdmin`) — the same gate the consumer's architecture tests
   already assert on the behavior level.
2. **The declaration macros.** Zero invocations. The consumer wires each
   resource's access adapter by hand (route refusal, replay ordering, output
   encoding are exactly what #50 single-authored upstream). The macros exist
   to delete that duplication; the one-author law at birth means every macro
   line now consumes the maintained implementation.
3. **The `daf-cache` tier engine + `LockRegistry`.** The consumer maintains a
   local legacy-surface mirror of cache configuration/flag/metrics behavior —
   mirror header explicitly records "returns when `daf_cache`'s tiers land in
   the consumer workspace". The mirror is waiting for this engine; `MokaCache`
   + `HierarchicalCache` + `LockRegistry`/`LockGuard` are the surface the
   mirror predates. (Caveat, stated for the ruling: the remote tiers remain
   feature-gated stubs upstream — adoption buys the config/metrics/ladder
   shape, not a redis/postgres implementation, and the consumer already
   carries its own tiered cache plane over the six-tier authority.)
4. **The themql tenant-pattern matcher.** `SubjectPattern::matches`
   (`themql-core 0.2.0/src/lib.rs:300..349`) is verified-string pattern
   matching over typed subject segments. The consumer's tenant scope checks
   hand-roll their segment comparisons; the published matcher is a drop-in
   canonical form with the error taxonomy attached (`SubjectError`).
5. **The message-envelope identity family.** `MessageId`/`CorrelationId` are
   consumed (2 of the envelope's identity types); `CausationId`, `TraceId`,
   `Metadata`, `Timestamp`, `Operation`, `FormatTag`, `Payload` are not — the
   consumer's own event envelope re-invents the segment/timestamp shapes. The
   published taxonomy is the correlated-trace contract for the
   query/mutation envelope.
6. **`prv` evaluation/backtest + scoring breadth.** The consumer consumes the
   four tail/point metrics; the published `Evaluator`/`backtest`/`Baseline`
   machinery + `brier_score`/`log_loss` (`metrics.rs:45`) unadopted — the
   parity-band harness the consumer hand-builds for its simulation domains
   re-implements a scorer subset of it.
7. **`prv-core` shape library.** `TimeSeries`/`Observation`/`Regime`/the
   pressure accessor family (30 unused symbols) — the typed base the
   consumer's scenario machinery shapes by hand (its risk-simulation domain
   implements series + pressure shapes at a local structural equivalent).
8. **`prv-monte-carlo` shock machinery.** `ShockSpec`/`ShockType`/
   `MonteCarloProbabilities` + `apply`/`sample_state`/`run_path` — beyond the
   one consumed `Simulator`, the correlated-shock vocabulary is unused; the
   consumer encodes its own shock/correlation-group model.

## What this flag does NOT prove

- Utilization is *surface consumption*, not value judgment: 0% on
  `daf-ffi`/`daf-http`/`daf-runtime`/`daf-messaging`/`daf-algorithms`/
  `daf-repository` says only that the consumer's transport/runtime/FFI/store
  legs are its own today — not that the published legs are waste.
- The counts inherit the one-hop barrel rule: consumption through a
  re-export is credited upstream, but a consumer crate that *copies* a shape
  without importing it (a semantic twin) is invisible to the pass — the
  value cases above were each verified by reading the consumer-side twin.
- The macro count is invocation-based; a consumer *planning* to adopt the
  declaration macros is invisible by design.

## Regeneration

The machine ledger is `daf-utilization-flag.json` in this directory. The
resolution pass is stdlib-only (no cargo, no rustc): inventory from engine
sources, use-tree parsing + body-token verification + macro keying +
one-hop barrel crediting, as described in *Method*. `--offline`-reproducible
against any pinned consumer snapshot.