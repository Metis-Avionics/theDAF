# Proposal: theDAF production-readiness program

*Status: draft proposal — DESIGN-FIRST LANE. No implementation is landed in
this PR; every wave below is a future reviewed PR. Evidence citations point
into this repo only. A consuming consumer is referenced exclusively as "a
downstream consumer" (the sanitization law); the owner-side mapping of
consumer surfaces is retained off-repo.*

## Why

The 0.3 generic seam (`crates/daf-core/src/generic/**`) established the
access composition law — one author per pattern, policy-driven admission,
typed subjects and CAS generation. The publish band 0.3.2 (`main` after
#50/#51) carries it. Production readiness, however, is not one crate: the
0.3 seam's own gates prove a *contract* base, while the *operational* legs a
production serving layer needs (resilience, observability, config,
supply-chain gates) are either partial or absent. A downstream consumer
today re-implements field-shim/schema/catalog wiring by hand precisely
because the upstream ships no transport seam beyond the 0.2-era Axum router.

**Problem statement, evidence-first.** Per the utilization flag
(`docs/daf-utilization-flag.md`): the consumer's daf-core consumption is the
generic seam only (19.5% of the published surface) and *every* declaration
macro sits at zero invocations; the transport/runtime/FFI/store legs are the
consumer's own. The upstream earns adoption by closing its production gaps
and making the consumer's hand-wiring deletable.

**Approach.** Four workstreams, sequenced as waves in the last section:
(1) close the operational legs (resilience/observability/config/
supply-chain); (2) a native GraphQL seam as a NEW adapter member with
macro-authored fields; (3) the 0.2 facade retirement arc; (4) the
eval/parity wiring so upstream CI continues to own the verdicts.

**Alternatives.**

- *Ship the GraphQL seam as a feature of `daf-core`* — rejected: violates
  the transport-blind law the 0.3 band's own comments encode (`generic
  /access.rs:257` — "DAF exposes the optional pool, but never preempts the
  tenant rule"; the module docstring law of the 0.3.1 release: "DAF still
  never imports a storage/transport crate"). A separate member keeps the
  core's dependency set identical to the 0.3.2 band.
- *Freeze the 0.2 facade as-is forever* — rejected: the facade is already
  `#[deprecated]` with adopt-notes pointing at the generic seam
  (`lib.rs:51..53`, `:403..405`, `:416..418`); a permanent deprecation that
  never retires is a lie in both directions.
- *Make resilience (breakers/pools) part of the access trait* — rejected
  for 0.3.x: the access layer is object-safe over user-provided pools
  (`access.rs:61` `required_pool`); resilience is the *composition*
  wrapper's job, not the domain contract's.

**Trade-off.** The strongest position: harden operations first (small, no
design risk), then the GraphQL member (the design risk is bounded by the
existing composition law: generated resolvers wrap the SAME access rows —
no parallel path), then retire the facade behind a feature gate. The cost:
one extra member crate and a checked-artifact chore (SDL snapshots) — both
explicitly cheap against the consumer-side surface they delete.

## 1. The production-gap audit (per crate, evidence-cited)

Legend: ✅ = exists and proved by CI/source; ⚠️ = partial; ❌ = absent.

| gap axis | `daf-core` (generic seam) | `daf-cache` | `daf-application` | `daf-runtime` / `daf-http` / `daf-ffi` / `daf-messaging` / `daf-repository` / `daf-algorithms` |
|---|---|---|---|---|
| **Tenancy** | ✅ `Subject` segments + tenant scopes (`generic/types.rs:13`); `AdmissionMode::Tenant`/`GlobalAdmin` (`generic/policy.rs:9`); the 0.3.1 tenant-before-pool ordering law (`generic/access.rs:257`) | ❌ no tenant row concept | ⚠️ routes subject through `DataAccess` but the factory carries no tenancy policy | ❌ (the 0.2 `UserId` facade is tenant-blind — deprecated `lib.rs:51`) |
| **Resilience (breakers/pools/timeout/retry)** | ⚠️ `required_pool` models unconfigured-pool denial (`access.rs:61`); no breaker/circuit/timeout/retry types | ❌ (L2 moka real; L3/L4 are compile-time stubs returning `CacheError` — `postgres.rs:20..44`) | ❌ | ❌ (`Runtime` = tokio pass-through, `lib.rs:31`; `DataAccessRouter` maps errors, no timeouts — `daf-http/src/lib.rs:63`) |
| **Persistence posture** | ❌ none (correct — transport/storage-blind is the law) | ⚠️ L2 `MokaCache` (`moka.rs:9`), `HierarchicalCache` ladder (`hierarchical.rs:9`); remote tiers = stubs | ⚠️ in-process coordination only | ❌ `MemoryRepository` (`daf-repository/src/memory.rs:8`) = in-memory CAS; no durable store |
| **Observability (tracing/metrics/hooks)** | ⚠️ `debug_assert!` invariant instrumentation (Power-of-Ten Rule 5 generation, `scripts/power_of_ten_rust.py`); NO tracing spans, NO metrics, NO hook trait on `DataAccess` | ⚠️ `tracing = "0.1"` in the manifest, hit/miss semantics internal | ⚠️ `tracing = "0.1"` in the manifest | ❌ no observability surface; `DafErrorCode` FFI contract only (`daf-ffi/src/lib.rs:53`) |
| **Config / feature flags** | ❌ no config surface (the 0.3 band reads nothing from env) | ❌ no env/config reader — a downstream consumer's local mirror re-implements `CacheConfig::from_env()` + `is_enabled` semantics for exactly this shape | ❌ (`DataAccessFactory` is a fixed constructor pair, `lib.rs:611`) | ❌ |
| **Hot-path latency discipline** | ✅ monomorphic domain bodies via the declaration macros; but the registry row = `Arc<dyn DataAccess>` (`access.rs:297`, `:329`) → dynamic dispatch per call; async-trait boxing per call | ✅ tier ladder is a fixed bound (L1→L4) | ⚠️ the `put` path decomposed to bounded helpers (`_build_put_merger`, `lib.rs:310`) | ❌ unmeasured |
| **Assertion / assertion-density gates** | ✅ `scripts/power_of_ten_rust.py` + CI job `power-of-ten-rust` (`.github/workflows/ci.yml:50`) | ✅ same | ✅ same | ✅ same |
| **Supply-chain hygiene** | ⚠️ exact pin of the cache-generation authority `thesix =0.5.0` (`Cargo.toml:35..39`), edition 2024 pin (`Cargo.toml:17`) | ⚠️ moka/redis/postgres/sled feature-gated | ⚠️ | ❌ **no `deny.toml`, no `cargo-deny`/`cargo-machete`/`cargo audit` job in CI** (verified absent from `.github/workflows/ci.yml`) |

Notes that gate the waves:

- The **GraphQL-relevant gap** is the config/flag + policy composition: the
  access row + `ResourcePolicy` (`generic/policy.rs:56`) already carry 80%
  of a typed field's contract; the missing 20% is transport-layer plumbing
  (the annex).
- The **stub honesty** must be preserved in any publish: the L3 `redis` /
  L4 `postgres` tiers return `CacheError` until implemented — the utilization
  flag shows a downstream consumer already mirrors the *config/metrics*
  shape locally while waiting for the tiers ("returns when `daf_cache`'s
  tiers land" — the mirror's own recorded decision). The wave plan makes
  good on exactly that seam.
- `daf-messaging` is a 2-line shell (`pub struct Messaging;`). Either give
  it a real role (the subscription home, annex) or retire it in the 0.4
  arc; an empty published crate is supply-chain noise.

## 2. The GraphQL-seam design

Full design in the annex: `docs/daf-graphql-seam-design.md`. The proposal's
positions:

- **Where it lives:** a NEW workspace member `crates/daf-graphql`, mirroring
  `daf-http`'s adapter role (`daf-http/src/lib.rs:63`) — `daf-core` stays
  transport-blind; `daf-graphql` depends on `daf-core` + `async-graphql`
  only. An optional `graphql` feature of `daf-core` is REJECTED (breaking
  the 0.3 dependency law for the transport layer).
- **The thesis:** the declaration macros author the field AND its wired
  types, so a consumer's resource becomes **one macro line**;
  the consumer-side alternative — ~152 hand-wired per-resource adaptations
  (the seam count a downstream consumer maintains today per the owner's own
  seam inventory) — is exactly what this deletes.
- **The eval surface fit:** the `parity-differential` job
  (`.github/workflows/ci.yml:59`) gains the SDL-snapshot parity leg; the
  Power-of-Ten rust gate keeps covering generated-macro source (the macro
  output is source at expansion — the recursion/loop-bound rules apply
  before codegen, in the macro AUTHOR).

## 3. The migration / compat path (the 0.2 facade retirement arc)

| band | change | adopt recipe (for a downstream consumer) |
|---|---|---|
| **0.3.x** (current) | facade present, `#[deprecated]` on `UserId`/`Algorithm`/`Authorizer` (`lib.rs:51`, `:403`, `:416`), `subject_from_legacy_user` bridge (`generic/types.rs:70`) | pin `daf-core =0.3.x` exact; keep the bridge; begin moving the compat plane's identity to `generic::Subject` |
| **0.4.0** | the facade moves behind the `facade-0-2` cargo feature, **default-on**; the deprecation window becomes opt-out | build with default features (no change), then `- facade-0-2` once the compat plane has collapsed; the `DataAccess` JSON contract stays (it is the 0.3 surface's own) |
| **0.5.0** | facade removed; `daf-messaging` re-homed (the subscription watch, annex) or retired; the 0.2 `QueryInfo`/`PostInfo`/`PutInfo`/`DeleteInfo` family deleted (`lib.rs:178..231`) | the consumer's error/wire mapping projects on the MQL taxonomy (`themql-core`'s `Error`/`ErrorCode`, `lib.rs:1201`/`:1159`) already — this band only drops the dead 0.2 shapes |

The arc's invariant: **every band is re-pointable by an exact-pin row plus a
feature flag — no API migration for consumers who adopted the generic seam.**

## 4. The wave plan

| wave | lands | why first | risk |
|---|---|---|---|
| **W-P1** (this PR) | the gap audit + the GraphQL annex, as docs | design-first; the ruling point for everything below | none (docs) |
| **W-P2** | supply-chain gate (`deny.toml` + `cargo-deny`/`cargo-machete`/`cargo audit` in CI) + the `daf-cache` config/env surface (the shape the consumer's mirror needs) + the tracing hooks on `DataAccess` (a non-object-safe hook trait defaulting no-op, so the object-safe contract is untouched) | cheapest, no design risk, closes the ❌/⚠️ rows the audit names | low; the hook trait must stay default-no-op to avoid breaking existing implementors |
| **W-P3** | the `daf-graphql` member skeleton: resolver generation from access rows + policy tables, SDL emission, the snapshot-parity CI leg | the core thesis; bounded by the composition law (wraps the SAME access rows) | medium: the macro-expansion surface grows; PoT gates cover generated source |
| **W-P4** | the macro field clause + typed schema (InputObject/ComplexObject) + the SDL/DDL agreement artifact (checked, lint-only) | makes the 1-line consumer true end-to-end | medium; the DDL agreement artifact is check-only (the DDL stays owned downstream-side, upstream carries the declared row) |
| **W-P5** | subscriptions over generation-watch (home: `daf-messaging`) + the `facade-0-2` feature gate (the 0.4 arc begins) | the invalidation signal already exists (`Generation`, `lib.rs:248` + the thesix authority) | high: ordering/latency semantics; deferred by design |
| **W-P6** | facade removal + `daf-messaging` disposition (0.5 arc) | only after W-P5's adoption settles | none given the arc |

### The honest row: what only a downstream integration test proves

The upstream CI can prove the CONTRACT surface (generated resolvers compile,
policy tables gate, SDL snapshots agree, PoT clean). It can NOT prove:

1. **The 1-line claim end-to-end** — that a real serving layer's ~152
   hand-wired shims actually collapse to the macro line, including the
   schema/catalog wiring that layer alone owns.
2. **Tenancy under load** — the tenant-before-pool ordering
   (`generic/access.rs:257`) and subject-shape denial behavior under
   concurrent tenants on a real pool; upstream has no pool.
3. **Subscription latency vs generation invalidation at real scale** —
   the watch is generation-driven in the design, but the latency/ordering
   semantics are only observable on a real deployment.
4. **The eval verdict's effect** — the consuming eval loop gates on the real
   query surface; a synthetic parity harness cannot reproduce the gate.

Each of these belongs on the consumer's integration plane as the wave's
acceptance record — the upstream PR carries the contract verdict only.